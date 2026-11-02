//! Real stdio MCP -> broker -> platform/model/audit daemons -> installed sd.cpp.
//! Uses existing weights via hard links in a fresh home; never launches Preview.
//! Usage: mcp_media_probe <bin-dir> <sd-executable> <models-dir> <repo-root>
use aos_ipc::BusClient;
use aos_proto::{AuditQueryRequest, ModuleInstallRequest, ModuleInvokeRequest, ModuleManifest};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn hash(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn binary(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
}

fn command(exe: &Path, home: &Path) -> Command {
    let mut c = Command::new(exe);
    c.current_dir(home)
        .env("AOS_HOME", home)
        .env_remove("AOS_MEDIA_STUB")
        .kill_on_drop(true);
    #[cfg(windows)]
    c.creation_flags(0x08000000);
    c
}

fn daemon(exe: &Path, args: &[String], home: &Path, sd: &Path) -> Result<Child> {
    let name = exe.file_stem().unwrap().to_string_lossy();
    let log = std::fs::File::create(home.join(format!("{name}.log")))?;
    Ok(command(exe, home)
        .args(args)
        .env("AOS_SD_BIN", sd)
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()?)
}

struct Mcp {
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    id: u64,
}
impl Mcp {
    fn start(
        bin: &Path,
        home: &Path,
        addr: &str,
        granted: bool,
        children: &mut Vec<Child>,
    ) -> Result<Self> {
        let label = if granted { "allow" } else { "deny" };
        let log = std::fs::File::create(home.join(format!("mcp-{label}.log")))?;
        let mut child = command(bin, home)
            .env("AOS_BUS_ADDR", addr)
            .env("AOS_MCP_FROM", format!("service:mcp-proof-{label}"))
            .env(
                "AOS_MCP_TOOLS",
                "akasha_image_generate;akasha_video_generate;notes.list;notes.read;notes.create",
            )
            .env(
                "AOS_MCP_CAPS",
                if granted {
                    "media.generate;fs.write:/downloads/**;tool.invoke:notes"
                } else {
                    ""
                },
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(log)
            .spawn()?;
        let client = Self {
            input: child.stdin.take().unwrap(),
            output: BufReader::new(child.stdout.take().unwrap()),
            id: 0,
        };
        children.push(child);
        Ok(client)
    }

    async fn rpc(&mut self, method: &str, params: Value) -> Result<Value> {
        self.id += 1;
        let request = json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params});
        self.input
            .write_all(format!("{request}\n").as_bytes())
            .await?;
        self.input.flush().await?;
        let mut line = String::new();
        let n = tokio::time::timeout(Duration::from_secs(660), self.output.read_line(&mut line))
            .await??;
        if n == 0 {
            return Err("MCP stdout closed".into());
        }
        let response: Value = serde_json::from_str(&line)?;
        if response["jsonrpc"] != "2.0" || response["id"] != self.id {
            return Err("unexpected/non-protocol MCP stdout".into());
        }
        if let Some(error) = response.get("error") {
            return Err(format!("MCP protocol error: {error}").into());
        }
        Ok(response["result"].clone())
    }

    async fn call(&mut self, name: &str, arguments: Value) -> Result<Value> {
        self.rpc("tools/call", json!({"name":name,"arguments":arguments}))
            .await
    }
}

fn tool_value(result: &Value) -> Result<Value> {
    if result["isError"] == true {
        return Err(format!("MCP tool error: {result}").into());
    }
    Ok(serde_json::from_str(
        result["content"][0]["text"]
            .as_str()
            .ok_or("missing tool text")?,
    )?)
}

async fn wait_intents(bus: &BusClient, intents: &[&str]) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            let mut ready = true;
            for intent in intents {
                ready &= bus.lookup(intent).await?;
            }
            if ready {
                return Ok::<_, aos_ipc::client::CallError>(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await??;
    Ok(())
}

async fn exercise(
    args: &[PathBuf],
    home: &Path,
    children: &mut Vec<Child>,
    report: &mut Value,
    control: &mut Option<std::sync::Arc<BusClient>>,
) -> Result<()> {
    let [bins, sd, weights, repo] = args else {
        return Err("expected four paths".into());
    };
    let model_dir = home.join("share/models");
    std::fs::create_dir_all(&model_dir)?;
    std::fs::create_dir_all(home.join("data/models"))?;
    let offerings: Value = serde_json::from_slice(&std::fs::read(
        repo.join("share/models/catalog-offerings.json"),
    )?)?;
    let selected: Vec<_> = offerings["models"]
        .as_array()
        .ok_or("invalid offerings")?
        .iter()
        .filter(|m| matches!(m["id"].as_str(), Some("local:sd-v1-5" | "local:ltx2.3-dev")))
        .cloned()
        .collect();
    if selected.len() != 2 {
        return Err("expected image and video offerings".into());
    }
    let mut assets = std::collections::BTreeSet::new();
    let mut catalog = Vec::new();
    for m in &selected {
        let file = m["filename"].as_str().ok_or("missing filename")?;
        assets.insert(file.to_owned());
        for extra in m["extra_files"].as_array().into_iter().flatten() {
            assets.insert(
                extra["filename"]
                    .as_str()
                    .ok_or("missing sidecar filename")?
                    .to_owned(),
            );
        }
        catalog.push(json!({"id":m["id"],"name":m["name"],"modality":m["modality"],"format":m["format"],
            "source":{"type":"local_file","path":model_dir.join(file),"sha256":"0".repeat(64)},
            "architecture":{"n_layers":0,"n_params":m["n_params"],"context_length":8192},
            "resource_hints":{"weights_bytes":weights.join(file).metadata()?.len(),"embed_bytes":0,"kv_bytes_per_token":0,"supports_layer_offload":false},
            "capabilities":m["profiles"],"backends_compatible":["sdcpp"],"privacy_class":"local"}));
    }
    // Hard links avoid copying tens of GB or modifying the installed store.
    // Offerings must only contain plain filenames, never arbitrary destinations.
    for file in &assets {
        if file.contains(['/', '\\', ':']) || file.starts_with('.') {
            return Err("unsafe asset filename".into());
        }
        std::fs::hard_link(weights.join(file), model_dir.join(file))?;
    }
    std::fs::write(
        model_dir.join("catalog-offerings.json"),
        serde_json::to_vec_pretty(&json!({"models":selected}))?,
    )?;
    std::fs::write(
        home.join("data/models/catalog.yaml"),
        serde_json::to_vec_pretty(&json!({"models":catalog}))?,
    )?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?.to_string();
    let port = listener.local_addr()?.port().to_string();
    drop(listener);
    children.push(daemon(&binary(bins, "aos-busd"), &[port], home, sd)?);
    let bus = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(c) = BusClient::connect(&addr, "test:mcp-media-proof").await {
                break c;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await?;
    *control = Some(bus.clone());
    children.push(daemon(
        &binary(bins, "aos-auditd"),
        &[
            addr.clone(),
            home.join("audit-canonical").to_string_lossy().into_owned(),
        ],
        home,
        sd,
    )?);
    let path = |p: &str| home.join(p).to_string_lossy().into_owned();
    let platform = json!({"bus":addr,"audit_dir":path("audit-local"),"storage_dir":path("var/storage"),
        "memory_dir":path("var/memory"),"modules_dir":path("var/modules"),"skills_dir":path("var/skills"),
        "sessions_dir":path("var/sessions"),"catalogue_file":path("no-catalogue.yaml"),
        "community_catalogue_dir":path("var/community"),"secrets_file":path("var/secrets/keys.yaml"),"net_mode":"offline_strict"});
    std::fs::write(
        home.join("platform.yaml"),
        serde_json::to_vec_pretty(&platform)?,
    )?;
    let model_paths: serde_json::Map<String, Value> = catalog.iter().map(|entry| (
        entry["id"].as_str().unwrap().to_owned(),
        json!({"path":entry["source"]["path"],"n_layers":0,"weights_bytes":entry["resource_hints"]["weights_bytes"]})
    )).collect();
    let model = json!({"bus":addr,"gpu":false,"default_model":"local:sd-v1-5","n_threads":8,
        "os_reserve_ram_bytes":1073741824u64,"os_reserve_vram_bytes":1073741824u64,
        "adaptive_planner":false,"routing":"local_only","models":model_paths});
    std::fs::write(home.join("model.yaml"), serde_json::to_vec_pretty(&model)?)?;
    children.push(daemon(
        &binary(bins, "aos-platformd"),
        &[path("platform.yaml")],
        home,
        sd,
    )?);
    children.push(daemon(
        &binary(bins, "aos-modeld"),
        &[path("model.yaml")],
        home,
        sd,
    )?);
    wait_intents(
        &bus,
        &[
            "module.install",
            "module.invoke",
            "media.image.generate",
            "auditd.query",
        ],
    )
    .await?;
    let notes = repo.join("share/modules/notes.aospkg");
    let manifest: ModuleManifest =
        serde_yaml::from_str(&std::fs::read_to_string(notes.join("manifest.yaml"))?)?;
    let installed: Value = bus
        .call(
            "module.install",
            &ModuleInstallRequest {
                source_dir: notes.to_string_lossy().into_owned(),
                approved_caps: Some(manifest.permissions.required_caps),
                actor: "human:mcp-proof-setup".into(),
                actor_caps: vec![],
            },
            vec![],
        )
        .await?;
    if installed["quarantined"] == true {
        return Err("fixture module quarantined".into());
    }
    let seeded: Value = bus.call("module.invoke", &ModuleInvokeRequest {module:"notes".into(),tool:"notes.create".into(),
        args:json!({"title":"MCP isolated fixture","content":"This synthetic note belongs only to the MCP probe."}),
        actor:"human:mcp-proof-setup".into(),actor_caps:vec![],trace_id:"mcp-proof-setup".into()}, vec![]).await?;
    if seeded["ok"] != true {
        return Err(format!("could not seed fixture: {seeded}").into());
    }
    let mcp_bin = binary(bins, "aos-mcpd");
    let mut allow = Mcp::start(&mcp_bin, home, &addr, true, children)?;
    let mut deny = Mcp::start(&mcp_bin, home, &addr, false, children)?;
    for client in [&mut allow, &mut deny] {
        let init = client.rpc("initialize",json!({"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"akasha-real-media-probe","version":"1"}})).await?;
        if init["protocolVersion"] != "2024-11-05" {
            return Err("unexpected MCP version".into());
        }
        client
            .input
            .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
            .await?;
        client.input.flush().await?;
    }
    let listed = allow.rpc("tools/list", json!({})).await?;
    let names: Vec<_> = listed["tools"]
        .as_array()
        .ok_or("missing tools")?
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    for expected in [
        "akasha_image_generate",
        "akasha_video_generate",
        "akasha_module__notes__list",
    ] {
        if !names.contains(&expected) {
            return Err(format!("missing {expected}").into());
        }
    }
    if names.contains(&"akasha_module__notes__create") {
        return Err("destructive tool exposed".into());
    }
    let note = tool_value(&allow.call("akasha_module__notes__list", json!({})).await?)?;
    if note["ok"] != true || !note.to_string().contains("MCP isolated fixture") {
        return Err(format!("real note not returned: {note}").into());
    }
    report["module_read"] = json!({"ok":true,"module":"notes","fixture_found":true});
    for name in [
        "akasha_image_generate",
        "akasha_video_generate",
        "akasha_module__notes__list",
    ] {
        let result = deny
            .call(name, json!({"prompt":"synthetic request"}))
            .await?;
        if result["isError"] != true {
            return Err(format!("missing-cap call accepted: {name}").into());
        }
    }
    for arguments in [
        json!({"prompt":"test","actor":"human:ui"}),
        json!({"prompt":"test","path":"/downloads/../outside.png"}),
        json!({"prompt":"test","options":{"argv":"--shell"}}),
    ] {
        if allow.call("akasha_image_generate", arguments).await?["isError"] != true {
            return Err("unsafe arguments accepted".into());
        }
    }
    report["denials"] = json!({"missing_caps":3,"spoofing_path_unknown_option":3,"ok":true});
    std::fs::write(
        home.join("report.partial.json"),
        serde_json::to_vec_pretty(report)?,
    )?;
    for (tool, id, options, dest) in [
        (
            "akasha_image_generate",
            "local:sd-v1-5",
            json!({"width":128,"height":128,"steps":1,"seed":42}),
            "/downloads/images/mcp-proof.png",
        ),
        (
            "akasha_video_generate",
            "local:ltx2.3-dev",
            json!({"width":128,"height":128,"steps":1,"seed":42,"video_frames":9,"fps":8,"cfg_scale":1.0,
            "backend":"te=cpu,llm=cpu,diffusion=gpu,vae=cpu","params_backend":"cpu","offload_to_cpu":true,"stream_layers":true}),
            "/downloads/video/mcp-proof.webm",
        ),
    ] {
        eprintln!("running {tool}; artifacts: {}", home.display());
        let value = tool_value(&allow.call(tool,json!({"prompt":"a small red cube on a white background","model_id":id,"path":dest,"options":options})).await?)?;
        if value["engine"] != "sdcpp" || value["path"] != dest {
            return Err(format!("non-real or wrong-path output: {value}").into());
        }
        let file = home
            .join("var/storage/data")
            .join(dest.trim_start_matches('/'));
        let bytes = file.metadata()?.len();
        if bytes == 0 || value["bytes"].as_u64() != Some(bytes) {
            return Err("wrong output byte count".into());
        }
        let verification = if tool == "akasha_image_generate" {
            let image = image::open(&file)?;
            if image.width() != 128 || image.height() != 128 {
                return Err("wrong image dimensions".into());
            }
            json!({"width":image.width(),"height":image.height()})
        } else {
            let output = tokio::time::timeout(
                Duration::from_secs(20),
                Command::new("ffprobe")
                    .args([
                        "-v",
                        "error",
                        "-count_frames",
                        "-select_streams",
                        "v:0",
                        "-show_entries",
                        "stream=codec_name,width,height,nb_read_frames,r_frame_rate",
                        "-of",
                        "json",
                    ])
                    .arg(&file)
                    .output(),
            )
            .await??;
            if !output.status.success() {
                return Err("ffprobe failed".into());
            }
            let decoded: Value = serde_json::from_slice(&output.stdout)?;
            let stream = &decoded["streams"][0];
            if stream["width"] != 128
                || stream["height"] != 128
                || stream["nb_read_frames"] != "9"
                || stream["r_frame_rate"] != "8/1"
            {
                return Err(format!("wrong video geometry/frames: {decoded}").into());
            }
            decoded
        };
        report[tool] =
            json!({"ok":true,"response":value,"sha256":hash(&file)?,"verified":verification});
        std::fs::write(
            home.join("report.partial.json"),
            serde_json::to_vec_pretty(report)?,
        )?;
    }
    let events: Vec<Value> = bus
        .call(
            "auditd.query",
            &AuditQueryRequest {
                action: Some("mcp.tool.call".into()),
                trace_id: None,
                actor: None,
                last: 100,
            },
            vec![],
        )
        .await?;
    if events.len() != 9
        || events.iter().any(|ev| {
            !ev["actor"]
                .as_str()
                .unwrap_or("")
                .starts_with("mcp:service:mcp-proof-")
        })
    {
        return Err(format!("unexpected audit records: {events:?}").into());
    }
    let refused = events
        .iter()
        .filter(|ev| ev["detail"]["ok"] == false)
        .count();
    if refused != 6 {
        return Err("wrong audit refusal count".into());
    }
    let verified: bool = bus.call("auditd.verify", &(), vec![]).await?;
    if !verified {
        return Err("canonical audit verification failed".into());
    }
    for label in ["allow", "deny"] {
        if std::fs::read_to_string(home.join(format!("mcp-{label}.log")))?
            .contains("MCP audit service unavailable")
        {
            return Err("MCP falsely reports canonical audit unavailable".into());
        }
    }
    report["audit"] = json!({"records":events.len(),"refused":refused,"chain_verified":verified,"actor_preserved":true});
    let mut identities = Vec::new();
    for file in assets {
        let path = weights.join(&file);
        identities
            .push(json!({"filename":file,"bytes":path.metadata()?.len(),"sha256":hash(&path)?}));
    }
    report["assets"] = json!(identities);
    report["sd_sha256"] = json!(hash(sd)?);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len() != 4 || args.iter().any(|p| !p.is_absolute() || !p.exists()) {
        return Err("expected absolute bin-dir, sd executable, model-dir and repo-root".into());
    }
    let home = std::env::temp_dir().join(format!(
        "akasha-mcp-media-proof-{:032x}",
        rand::random::<u128>()
    ));
    std::fs::create_dir(&home)?;
    eprintln!("isolated probe home: {}", home.display());
    let mut children = Vec::new();
    let mut report = json!({"issue":439,"scope":"real stdio MCP and native daemons with existing sd.cpp/model assets; synthetic requests; no installed UI validation"});
    let mut control = None;
    let result = exercise(&args, &home, &mut children, &mut report, &mut control).await;
    if result.is_err() {
        // Stop the engine owned by this isolated daemon before terminating it.
        if let Some(bus) = control {
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                bus.call::<_, bool>("media.image.cancel", &(), vec![]),
            )
            .await;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
    let mut stopped = true;
    for child in children.iter_mut().rev() {
        if child.try_wait()?.is_none() {
            let _ = child.kill().await;
        }
        stopped &= child.wait().await.is_ok();
    }
    report["owned_processes_stopped"] = json!(stopped);
    // Release only the links created in this probe's model directory. Keeping
    // them would retain large weights after the original store removes them.
    let mut released = 0;
    if let Ok(entries) = std::fs::read_dir(home.join("share/models")) {
        for entry in entries {
            let path = entry?.path();
            if matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("gguf" | "safetensors")
            ) {
                std::fs::remove_file(path)?;
                released += 1;
            }
        }
    }
    report["owned_weight_links_released"] = json!(released);
    report["ok"] = json!(result.is_ok() && stopped);
    if let Err(error) = &result {
        report["error"] = json!(error.to_string());
    }
    std::fs::write(
        home.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    result
}
