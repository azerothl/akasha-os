//! Opt-in, bounded OS tools. Configuration is read from the server's environment,
//! never from MCP arguments. Module discovery uses installed manifests; destructive
//! module tools require a separate confirmation contract and are not admitted here.

use super::{call_error_text, tool_json_result, BusState};
use aos_ipc::client::BusClient;
use aos_proto::{
    AuditAppendRequest, DownloadKind, MediaImageGenerateRequest, MediaImageOptions,
    MediaImageUpscaleRequest, ModuleIdRequest, ModuleInvokeRequest,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const READ_ONLY_MODULE_TOOLS: &[&str] = &[
    "notes.list",
    "notes.read",
    "notes.search",
    "notes.links",
    "notes.related",
    "tasks.list",
];
const MEDIA_TOOLS: &[&str] = &[
    "akasha_image_generate",
    "akasha_video_generate",
    "akasha_image_upscale",
];
static TRACE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Default)]
pub struct OsToolPolicy {
    tools: BTreeSet<String>,
    caps: Vec<String>,
}

impl OsToolPolicy {
    /// `tools`: exact MCP media names or exact OS module tool names (e.g. notes.list).
    /// Unknown entries, including destructive tools, grant nothing.
    pub fn new(tools: Vec<String>, caps: Vec<String>) -> Self {
        Self {
            tools: tools.into_iter().collect(),
            caps,
        }
    }

    pub fn from_env() -> Self {
        let split = |key| {
            std::env::var(key)
                .unwrap_or_default()
                .split(';')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        };
        Self::new(split("AOS_MCP_TOOLS"), split("AOS_MCP_CAPS"))
    }

    fn media_allowed(&self, name: &str) -> bool {
        MEDIA_TOOLS.contains(&name)
            && self.tools.contains(name)
            && self.caps.iter().any(|c| c == "media.generate")
            && self
                .caps
                .iter()
                .any(|c| c.starts_with("fs.write:/downloads/"))
    }

    fn module_allowed(&self, tool: &str) -> bool {
        let module = tool.split('.').next().unwrap_or("");
        READ_ONLY_MODULE_TOOLS.contains(&tool)
            && self.tools.contains(tool)
            && self.caps.contains(&format!("tool.invoke:{module}"))
    }

    pub(super) fn has_module_tools(&self) -> bool {
        self.tools.iter().any(|tool| self.module_allowed(tool))
    }

    pub(super) fn media_catalog(&self) -> Vec<Value> {
        MEDIA_TOOLS.iter().filter(|name| self.media_allowed(name)).map(|name| {
            let (description, properties, required) = if *name == "akasha_image_upscale" {
                ("Upscale an image through Akasha Create. Returns a VFS path and metadata.",
                 json!({"source_path":{"type":"string"},"output_path":{"type":"string"},
                    "upscale_model":{"type":"string"},"upscale_repeats":{"type":"integer","minimum":1,"maximum":4},
                    "upscale_tile_size":{"type":"integer","minimum":32,"maximum":2048}}),
                 json!(["source_path","upscale_model"]))
            } else {
                (if *name == "akasha_video_generate" {
                    "Generate a short video through Akasha Create (sd_mode=vid_gen). Returns a WebM VFS path and metadata."
                } else { "Generate an image through Akasha Create. Returns a VFS path and metadata." },
                 json!({"prompt":{"type":"string","minLength":1},"path":{"type":"string"},
                    "model_id":{"type":"string"},"options":{"type":"object",
                    "description":"Akasha MediaImageOptions; unknown keys are refused. Video frames 1..129 and FPS 1..120."}}),
                 json!(["prompt"]))
            };
            json!({"name":name,"description":description,"inputSchema":{
                "type":"object","properties":properties,"required":required,"additionalProperties":false}})
        }).collect()
    }

    pub(super) async fn module_catalog(&self, client: &BusClient) -> Vec<Value> {
        let mut catalog = Vec::new();
        for module in ["notes", "tasks"] {
            if !self
                .tools
                .iter()
                .any(|tool| tool.starts_with(&format!("{module}.")) && self.module_allowed(tool))
            {
                continue;
            }
            let module_id = ModuleIdRequest {
                module: module.into(),
            };
            let request = client.call::<_, Value>("module.describe", &module_id, vec![]);
            let Ok(Ok(description)) = tokio::time::timeout(Duration::from_secs(3), request).await
            else {
                continue;
            };
            let Some(tools) = description
                .pointer("/manifest/tools")
                .and_then(Value::as_array)
            else {
                continue;
            };
            for tool in tools {
                let Some(name) = tool["name"]
                    .as_str()
                    .filter(|name| self.module_allowed(name))
                else {
                    continue;
                };
                catalog.push(json!({"name": module_mcp_name(name),
                    "description":tool["description"],"inputSchema":tool["input_schema"],
                    "annotations":{"readOnlyHint":true,"destructiveHint":false}}));
            }
        }
        catalog
    }

    fn require_path(&self, kind: &str, path: &str) -> Result<(), String> {
        if !path.starts_with('/')
            || path.contains(['\\', '\0', ':'])
            || path
                .split('/')
                .skip(1)
                .any(|p| p.is_empty() || p == "." || p == "..")
        {
            return Err("invalid logical VFS path".into());
        }
        let allowed = self
            .caps
            .iter()
            .filter_map(|cap| cap.strip_prefix(&format!("fs.{kind}:")))
            .any(|scope| {
                scope == path
                    || scope
                        .strip_suffix("/**")
                        .is_some_and(|prefix| path.starts_with(&format!("{prefix}/")))
            });
        if allowed {
            Ok(())
        } else {
            Err(format!("cap fs.{kind} required for {path}"))
        }
    }

    fn prepare(
        &self,
        name: &str,
        args: Value,
        actor: &str,
        trace: &str,
    ) -> Result<(&'static str, Value), String> {
        if let Some(tool) = READ_ONLY_MODULE_TOOLS
            .iter()
            .find(|tool| module_mcp_name(tool) == name && self.module_allowed(tool))
        {
            if !args.is_object() {
                return Err("arguments must be an object".into());
            }
            if args
                .as_object()
                .unwrap()
                .keys()
                .any(|key| ["actor", "actor_caps", "caps", "trace_id"].contains(&key.as_str()))
            {
                return Err("identity and capabilities are server-owned".into());
            }
            let module = tool.split('.').next().unwrap();
            return Ok((
                "module.invoke",
                serde_json::to_value(ModuleInvokeRequest {
                    module: module.into(),
                    tool: (*tool).into(),
                    args,
                    actor: actor.into(),
                    actor_caps: self.caps.clone(),
                    trace_id: trace.into(),
                })
                .map_err(|e| e.to_string())?,
            ));
        }
        if !self.media_allowed(name) {
            return Err(format!("tool not allowed or capabilities missing: {name}"));
        }
        let object = args.as_object().ok_or("arguments must be an object")?;
        let permitted: &[&str] = if name == "akasha_image_upscale" {
            &[
                "source_path",
                "output_path",
                "upscale_model",
                "upscale_repeats",
                "upscale_tile_size",
            ]
        } else {
            &["prompt", "path", "model_id", "options"]
        };
        if object.keys().any(|key| !permitted.contains(&key.as_str())) {
            return Err("unknown argument; identity and capabilities are server-owned".into());
        }
        let mut payload = args;
        payload["actor"] = json!(actor);
        payload["caps"] = json!(self.caps);
        payload["trace_id"] = json!(trace);
        if name == "akasha_image_upscale" {
            let mut req: MediaImageUpscaleRequest =
                serde_json::from_value(payload).map_err(|e| e.to_string())?;
            self.require_path("read", &req.source_path)?;
            if req.upscale_model.contains(['/', '\\', ':']) || req.upscale_model.starts_with('.') {
                return Err("upscale_model must be a catalogue filename".into());
            }
            if !(1..=4).contains(&req.upscale_repeats.unwrap_or(1))
                || !(32..=2048).contains(&req.upscale_tile_size.unwrap_or(128))
            {
                return Err("upscale controls are outside supported limits".into());
            }
            let dest = req
                .output_path
                .unwrap_or_else(|| format!("/downloads/images/{trace}-upscaled.png"));
            self.require_path("write", &dest)?;
            if !dest.starts_with("/downloads/") {
                return Err("output must be under /downloads".into());
            }
            req.output_path = Some(dest);
            return Ok((
                "media.image.upscale",
                serde_json::to_value(req).map_err(|e| e.to_string())?,
            ));
        }
        let mut req: MediaImageGenerateRequest =
            serde_json::from_value(payload).map_err(|e| e.to_string())?;
        if req.prompt.trim().is_empty() || req.prompt.len() > 32_768 {
            return Err("prompt must contain 1..32768 bytes".into());
        }
        let video = name == "akasha_video_generate";
        if !video
            && req
                .options
                .sd_mode
                .as_deref()
                .is_some_and(|mode| mode != "img_gen")
        {
            return Err("use akasha_video_generate for video".into());
        }
        req.options.sd_mode = Some(if video { "vid_gen" } else { "img_gen" }.into());
        if video {
            req.options.video_frames = Some(req.options.video_frames.unwrap_or(17));
            req.options.fps = Some(req.options.fps.unwrap_or(8));
        } else if req.options.video_frames.is_some() || req.options.fps.is_some() {
            return Err("video controls require akasha_video_generate".into());
        }
        validate_media_options(&req.options)?;
        for path in [
            &req.options.init_image,
            &req.options.end_image,
            &req.options.mask_image,
        ]
        .into_iter()
        .flatten()
        {
            self.require_path("read", path)?;
        }
        let raw = req.path.unwrap_or_else(|| {
            if video {
                format!("/downloads/video/{trace}.webm")
            } else {
                format!("/downloads/images/{trace}.png")
            }
        });
        self.require_path("write", &raw)?;
        let dest = aos_proto::normalize_download_path(
            &raw,
            if video {
                DownloadKind::Video
            } else {
                DownloadKind::Images
            },
        );
        if !dest.starts_with("/downloads/") {
            return Err("output must be under /downloads".into());
        }
        self.require_path("write", &dest)?;
        req.path = Some(dest);
        Ok((
            "media.image.generate",
            serde_json::to_value(req).map_err(|e| e.to_string())?,
        ))
    }
}

fn module_mcp_name(tool: &str) -> String {
    format!("akasha_module__{}", tool.replace('.', "__"))
}

fn validate_media_options(options: &MediaImageOptions) -> Result<(), String> {
    for asset in options
        .loras
        .iter()
        .chain(options.vae.iter())
        .chain(options.upscale_model.iter())
    {
        if asset.is_empty() || asset.contains(['/', '\\', ':', '\0']) || asset.starts_with('.') {
            return Err("media assets must be catalogue filenames, not host paths".into());
        }
    }
    if !(64..=2048).contains(&options.width.unwrap_or(512))
        || !(64..=2048).contains(&options.height.unwrap_or(512))
        || !(1..=100).contains(&options.steps.unwrap_or(20))
        || !(1..=129).contains(&options.video_frames.unwrap_or(1))
        || !(1..=120).contains(&options.fps.unwrap_or(8))
    {
        return Err("media controls are outside supported limits".into());
    }
    Ok(())
}

impl BusState {
    pub(super) async fn call_os_tool(&self, name: &str, args: Value) -> Result<Value, String> {
        let trace = format!(
            "mcp-{}-{}",
            std::process::id(),
            TRACE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        // Never inherit the service/human bypass of downstream handlers.
        let actor = format!("mcp:{}", self.from());
        let prepared = self.os_policy.prepare(name, args, &actor, &trace);
        if let Err(error) = &prepared {
            tracing::warn!(tool=name, trace_id=trace, %error, "MCP tool denied");
        }
        let client = match self.client().await {
            Ok(client) => client,
            Err(error) => {
                return Err(prepared
                    .err()
                    .unwrap_or_else(|| format!("bus connect: {error}")))
            }
        };
        let result = match prepared {
            Err(error) => Err(error),
            Ok((intent, payload)) => {
                if intent == "module.invoke" {
                    let listed = self.os_policy.module_catalog(&client).await;
                    if !listed.iter().any(|tool| tool["name"] == name) {
                        let error = "module tool unavailable or not installed";
                        tracing::warn!(tool = name, trace_id = trace, error, "MCP tool denied");
                        let audit = AuditAppendRequest {
                            trace_id: trace,
                            actor,
                            action: "mcp.tool.call".into(),
                            target: name.into(),
                            detail: json!({"ok":false,"error":error}),
                        };
                        let _ = tokio::time::timeout(
                            Duration::from_secs(2),
                            client.call::<_, bool>("audit.append", &audit, vec![]),
                        )
                        .await;
                        return Err(error.into());
                    }
                }
                tokio::time::timeout(
                    Duration::from_secs(600),
                    client.call::<_, Value>(intent, &payload, self.os_policy.caps.clone()),
                )
                .await
                .map_err(|_| "OS tool timed out".to_string())
                .and_then(|result| result.map_err(call_error_text))
            }
        };
        let audit = AuditAppendRequest {
            trace_id: trace,
            actor,
            action: "mcp.tool.call".into(),
            target: name.into(),
            detail: json!({"ok":result.is_ok(),"error":result.as_ref().err()}),
        };
        if !matches!(
            tokio::time::timeout(
                Duration::from_secs(2),
                client.call::<_, bool>("audit.append", &audit, vec![])
            )
            .await,
            Ok(Ok(_))
        ) {
            tracing::warn!(tool = name, "MCP audit service unavailable");
        }
        result.map(|value| tool_json_result(&value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_options_cannot_supply_host_asset_paths() {
        for args in [
            json!({"prompt":"test","options":{"vae":"C:\\secret"}}),
            json!({"prompt":"test","options":{"loras":["../outside"]}}),
            json!({"prompt":"test","options":{"upscale_model":"/tmp/model.pth"}}),
        ] {
            assert!(policy()
                .prepare("akasha_image_generate", args, "mcp:test", "test")
                .is_err());
        }
    }

    fn policy() -> OsToolPolicy {
        OsToolPolicy::new(
            MEDIA_TOOLS
                .iter()
                .map(|s| s.to_string())
                .chain(["notes.list".into(), "notes.delete".into()])
                .collect(),
            vec![
                "media.generate".into(),
                "fs.write:/downloads/**".into(),
                "fs.read:/downloads/**".into(),
                "tool.invoke:notes".into(),
            ],
        )
    }

    #[test]
    fn deny_by_default_and_require_both_explicit_grants() {
        assert!(OsToolPolicy::default()
            .prepare(
                "akasha_image_generate",
                json!({"prompt":"test"}),
                "mcp:test",
                "test"
            )
            .is_err());
        assert!(
            OsToolPolicy::new(vec!["akasha_image_generate".into()], vec![])
                .media_catalog()
                .is_empty()
        );
        assert!(!policy().module_allowed("notes.delete"));
        assert_eq!(policy().media_catalog().len(), 3);
    }

    #[test]
    fn spoofing_and_paths_are_refused() {
        for args in [
            json!({"prompt":"test","actor":"human:ui"}),
            json!({"prompt":"test","caps":["*"]}),
            json!({"prompt":"test","path":"/downloads/../secrets.png"}),
            json!({"prompt":"test","options":{"init_image":"/documents/private.png"}}),
            json!({"prompt":"test","options":{"argv":"--shell"}}),
        ] {
            assert!(policy()
                .prepare("akasha_image_generate", args, "mcp:test", "trace")
                .is_err());
        }
    }

    #[test]
    fn video_keeps_typed_options_and_non_privileged_identity() {
        let (intent, payload) = policy()
            .prepare(
                "akasha_video_generate",
                json!({"prompt":"clouds","options":{"video_frames":17,"fps":12}}),
                "mcp:service:mcp",
                "trace",
            )
            .unwrap();
        assert_eq!(intent, "media.image.generate");
        assert_eq!(payload["actor"], "mcp:service:mcp");
        assert_eq!(payload["options"]["sd_mode"], "vid_gen");
        assert_eq!(payload["options"]["fps"], 12);
        assert!(payload["path"].as_str().unwrap().ends_with(".webm"));
    }
}
