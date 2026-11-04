//! Explicit project edits and administrator-configured tasks; no ambient host access.
use crate::tools::{ToolBackend, ToolDesc};
use aos_ipc::BusClient;
use aos_proto::workspace::WorkspaceTaskRequest;
use aos_proto::{FsApplyPatchRequest, FsPatchHunk, FsUndoPatchRequest};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const PROMPT_GUIDE: &str = "## Modifications du projet\n\
    Ces outils exigent une demande explicite de modification et les grants exacts du projet. \
    Lis les sources utiles avec workspace.read avant de proposer un patch. \
    fs.apply_patch accepte des hunks sur /host/<id>/fichier, diff unifié ou replace_all=true. \
    Conserve undo_group_id pour fs.undo_patch. Vérifie le résultat avant de conclure. \
    process.run exécute uniquement une tâche nommée configurée par l'administrateur ; \
    il exige une confirmation humaine et un mode réseau autorisant les processus non isolés. \
    N'invente ni programme, ni arguments, ni confirmation. Ne contourne aucun refus. \
    Une erreur de transport ne garantit pas l'annulation : vérifie l'état avant de réessayer.";

pub fn is_operation_tool(name: &str) -> bool {
    matches!(name, "fs.apply_patch" | "fs.undo_patch" | "process.run")
}

pub fn advisory_refusal(goal: &str, name: &str) -> Option<&'static str> {
    (is_operation_tool(name) && aos_proto::chat_user_wants_advisory(goal)).then_some(
        "action refusée : le goal est une évaluation/conseil. Aucune modification ou commande de projet sans demande explicite.",
    )
}

pub fn catalog() -> Vec<ToolDesc> {
    [
        ("fs.apply_patch", "Appliquer un patch transactionnel à un projet déjà lié. Demande explicite, grants fs.write sur les chemins /host/<id>/... ciblés requis. Renvoie undo_group_id. Aucun grant implicite.", json!({"type":"object","properties":{"hunks":{"type":"array","minItems":1,"maxItems":32,"items":{"type":"object","properties":{"path":{"type":"string"},"diff":{"type":"string"},"replace_all":{"type":"boolean"}},"required":["path","diff"],"additionalProperties":false}}},"required":["hunks"],"additionalProperties":false})),
        ("fs.undo_patch", "Restaurer un groupe de patch connu, sous les permissions de l'appelant. Ne pas inventer l'identifiant.", json!({"type":"object","properties":{"undo_group_id":{"type":"string"}},"required":["undo_group_id"],"additionalProperties":false})),
        ("process.run", "Exécuter une tâche nommée du projet, préconfigurée par l'administrateur. Grants process.run et fs.read/write exacts, réseau online et confirmation humaine obligatoires. Ni shell ni programme arbitraire.", json!({"type":"object","properties":{"root":{"type":"string"},"task":{"type":"string"}},"required":["root","task"],"additionalProperties":false})),
    ].into_iter().map(|(name, description, input_schema)| ToolDesc {
        name:name.into(), description:description.into(), input_schema,
        backend:ToolBackend::Native, required_caps:vec![],
    }).collect()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Hunk {
    path: String,
    diff: String,
    #[serde(default)]
    replace_all: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Patch {
    hunks: Vec<Hunk>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Undo {
    undo_group_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Task {
    root: String,
    task: String,
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

fn project_path(path: &str) -> Option<&str> {
    let tail = path.strip_prefix("/host/")?;
    let (root, relative) = tail.split_once('/')?;
    if !identifier(root)
        || relative.is_empty()
        || path.len() > 4096
        || path.contains(['\\', ':', '\0'])
        || relative
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return None;
    }
    Some(root)
}

fn prepare(
    name: &str,
    args: &Value,
    actor: &str,
    caps: &[String],
    trace: &str,
) -> Result<Value, &'static str> {
    let invalid = "invalid_project_operation_arguments";
    if args.to_string().len() > 512 * 1024 {
        return Err(invalid);
    }
    match name {
        "fs.apply_patch" => {
            let request: Patch = serde_json::from_value(args.clone()).map_err(|_| invalid)?;
            if request.hunks.is_empty() || request.hunks.len() > aos_proto::APPLY_PATCH_MAX_FILES {
                return Err(invalid);
            }
            let path_error = "project_patch_requires_host_paths: use /host/<id>/relative/file for every hunk; no traversal or mixed workspaces";
            let root = project_path(&request.hunks[0].path).ok_or(path_error)?;
            if request
                .hunks
                .iter()
                .any(|h| project_path(&h.path) != Some(root))
            {
                return Err(path_error);
            }
            let hunks = request
                .hunks
                .into_iter()
                .map(|h| FsPatchHunk {
                    path: h.path,
                    diff: h.diff,
                    replace_all: h.replace_all,
                })
                .collect();
            serde_json::to_value(FsApplyPatchRequest {
                hunks,
                actor: actor.into(),
                caps: caps.to_vec(),
                trace_id: trace.into(),
                undo_group_id: None,
            })
            .map_err(|_| invalid)
        }
        "fs.undo_patch" => {
            let request: Undo = serde_json::from_value(args.clone()).map_err(|_| invalid)?;
            if !identifier(&request.undo_group_id) {
                return Err(invalid);
            }
            serde_json::to_value(FsUndoPatchRequest {
                undo_group_id: request.undo_group_id,
                actor: actor.into(),
                caps: caps.to_vec(),
                trace_id: trace.into(),
            })
            .map_err(|_| invalid)
        }
        "process.run" => {
            let request: Task = serde_json::from_value(args.clone()).map_err(|_| invalid)?;
            let root = request.root.strip_prefix("/host/").unwrap_or(&request.root);
            if !identifier(root) || !identifier(&request.task) {
                return Err(invalid);
            }
            serde_json::to_value(WorkspaceTaskRequest {
                root: root.into(),
                task: request.task,
                actor: actor.into(),
                caps: caps.to_vec(),
                trace_id: trace.into(),
            })
            .map_err(|_| invalid)
        }
        _ => Err("unsupported_project_operation_tool"),
    }
}

pub async fn invoke(
    bus: &BusClient,
    agent_id: &str,
    caps: &[String],
    name: &str,
    args: &Value,
) -> String {
    let actor = format!("agent:{agent_id}");
    // Match the existing read-only context boundary: shared daemon clients
    // cannot impersonate a room member or fall back to daemon privileges.
    if bus.from != actor {
        return json!({"ok":false,"error":"project_operation_requires_agent_bound_bus"})
            .to_string();
    }
    let trace = format!(
        "agent-project-{agent_id}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let payload = match prepare(name, args, &actor, caps, &trace) {
        Ok(payload) => payload,
        Err(error) => return json!({"ok":false,"error":error}).to_string(),
    };
    if name == "process.run" {
        let online = tokio::time::timeout(
            Duration::from_secs(5),
            bus.call_from::<_, Value>(&actor, "net.status", &(), vec![]),
        )
        .await;
        if !matches!(online, Ok(Ok(ref status)) if status["mode"] == "online") {
            return json!({"ok":false,"error":"project_command_requires_online_policy: host processes are not network-sandboxed"}).to_string();
        }
    }
    match tokio::time::timeout(
        // Host confirmation can take 120s, followed by a task capped at 300s.
        Duration::from_secs(430),
        bus.call_from::<_, Value>(&actor, name, &payload, caps.to_vec()),
    )
    .await
    {
        Ok(Ok(response)) if response.to_string().len() <= 512 * 1024 => response.to_string(),
        Ok(Ok(_)) => json!({"ok":false,"error":"project_operation_response_too_large","execution_state":"unknown","retry_safe":false}).to_string(),
        Ok(Err(error)) => json!({"ok":false,"error":error.to_string(),"execution_state":"unknown","retry_safe":false}).to_string(),
        Err(_) => json!({"ok":false,"error":"project_operation_timeout","execution_state":"unknown","retry_safe":false}).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_ipc::{msg::Status, BusService};
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn transport_preserves_runtime_identity_grants_and_host_refusals() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let broker = tokio::spawn(aos_ipc::broker::serve(listener));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let online = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut svc = BusService::new("project-operation-test");
        let state = online.clone();
        svc.on("net.status", move |ctx| {
            let state = state.clone();
            async move {
                let mode = if state.load(std::sync::atomic::Ordering::SeqCst) {
                    "online"
                } else {
                    "offline_strict"
                };
                ctx.respond(Status::Ok, &json!({"mode":mode}))
                    .await
                    .unwrap();
            }
        });
        for name in ["fs.apply_patch", "fs.undo_patch", "process.run"] {
            let sink = calls.clone();
            svc.on(name, move |ctx| {
                let sink = sink.clone();
                async move {
                    sink.lock().unwrap().push((
                        name,
                        ctx.intent.from.clone(),
                        ctx.intent.caps.clone(),
                        ctx.payload::<Value>().unwrap(),
                    ));
                    ctx.respond(Status::Ok, &json!({"ok":false,"message":"host denied"}))
                        .await
                        .unwrap();
                }
            });
        }
        let service_addr = addr.clone();
        let service = tokio::spawn(async move { svc.serve(&service_addr).await });
        let bus = BusClient::connect(&addr, "agent:one").await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !bus.lookup("process.run").await.unwrap() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let caps = vec!["fs.write:/host/demo/**".to_owned()];
        let task = json!({"root":"demo","task":"check"});
        let shared = BusClient::connect(&addr, "shared-daemon").await.unwrap();
        assert!(invoke(&shared, "one", &caps, "process.run", &task)
            .await
            .contains("requires_agent_bound_bus"));
        assert!(invoke(&bus, "another-agent", &caps, "process.run", &task)
            .await
            .contains("requires_agent_bound_bus"));
        let result = invoke(&bus, "one", &caps, "process.run", &task).await;
        assert!(result.contains("requires_online_policy"));
        assert!(calls.lock().unwrap().is_empty());
        online.store(true, std::sync::atomic::Ordering::SeqCst);
        for (name, args) in [
            (
                "fs.apply_patch",
                json!({"hunks":[{"path":"/host/demo/a","diff":"new","replace_all":true}]}),
            ),
            ("fs.undo_patch", json!({"undo_group_id":"undo-1-2"})),
            ("process.run", task),
        ] {
            let result: Value =
                serde_json::from_str(&invoke(&bus, "one", &caps, name, &args).await).unwrap();
            assert_eq!(result, json!({"ok":false,"message":"host denied"}));
        }
        let recorded = calls.lock().unwrap();
        assert_eq!(recorded.len(), 3);
        for (_, from, envelope_caps, payload) in recorded.iter() {
            assert_eq!(from, "agent:one");
            assert_eq!(envelope_caps, &caps);
            assert_eq!(payload["actor"], "agent:one");
            assert_eq!(payload["caps"], json!(caps));
            assert!(payload["trace_id"]
                .as_str()
                .unwrap()
                .starts_with("agent-project-one-"));
        }
        service.abort();
        broker.abort();
    }
    #[test]
    fn actor_caps_and_trace_come_from_the_runtime() {
        let caps = vec!["fs.write:/host/demo/**".into()];
        let args = json!({"hunks":[{"path":"/host/demo/src/lib.rs","diff":"replacement","replace_all":true}]});
        let payload = prepare("fs.apply_patch", &args, "agent:one", &caps, "trace-one").unwrap();
        assert_eq!(payload["actor"], "agent:one");
        assert_eq!(payload["caps"], json!(caps));
        assert_eq!(payload["trace_id"], "trace-one");
        for key in ["actor", "caps", "trace_id", "undo_group_id"] {
            let mut forged = args.clone();
            forged[key] = json!("forged");
            assert!(prepare("fs.apply_patch", &forged, "agent:one", &caps, "t").is_err());
        }
    }
    #[test]
    fn patch_is_bounded_and_stays_inside_one_logical_project() {
        for path in [
            "/documents/a",
            "C:/repo/a",
            "/host/demo/../a",
            "/host/demo/a/./b",
            "/host/demo//b",
            "/host/demo/a\\b",
            "/host/demo/a:stream",
        ] {
            assert!(
                prepare(
                    "fs.apply_patch",
                    &json!({"hunks":[{"path":path,"diff":"x"}]}),
                    "agent:a",
                    &[],
                    "t"
                )
                .is_err(),
                "{path}"
            );
        }
        assert!(prepare("fs.apply_patch", &json!({"hunks":[]}), "agent:a", &[], "t").is_err());
        let hunks = vec![json!({"path":"/host/demo/a","diff":"x"}); 33];
        assert!(prepare(
            "fs.apply_patch",
            &json!({"hunks":hunks}),
            "agent:a",
            &[],
            "t"
        )
        .is_err());
        assert!(prepare("fs.apply_patch", &json!({"hunks":[{"path":"/host/demo/a","diff":"x"},{"path":"/host/other/b","diff":"x"}]}), "agent:a", &[], "t").is_err());
    }
    #[test]
    fn task_cannot_supply_a_program_or_identity() {
        let args = json!({"root":"/host/demo","task":"check"});
        assert_eq!(
            prepare("process.run", &args, "agent:a", &[], "t").unwrap()["root"],
            "demo"
        );
        for key in ["program", "args", "env", "cwd", "actor", "caps"] {
            let mut forged = args.clone();
            forged[key] = json!("injected");
            assert!(prepare("process.run", &forged, "agent:a", &[], "t").is_err());
        }
        assert!(prepare(
            "process.run",
            &json!({"root":"../demo","task":"check"}),
            "agent:a",
            &[],
            "t"
        )
        .is_err());
    }
    #[test]
    fn selection_never_mints_capabilities_and_advice_never_mutates() {
        for tool in catalog() {
            assert!(tool.required_caps.is_empty());
            assert!(advisory_refusal(
                "Quels seraient les besoins pour un assistant de développement ?",
                &tool.name
            )
            .is_some());
            assert!(advisory_refusal("Corrige les deux fichiers du projet", &tool.name).is_none());
        }
    }
}
