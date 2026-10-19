//! Shared bus/WASM boundary for policy-filtered project context and source maps.
use crate::{
    subsystem::PlatformSubsystem,
    workspace_debug,
    workspace_inventory::{self, Request},
};
use aos_proto::AuditAppendRequest;
use serde_json::{json, Value};
pub async fn dispatch(
    sub: &PlatformSubsystem,
    intent: &str,
    args: Value,
    actor: &str,
    caps: &[String],
    trace: &str,
) -> Result<Value, String> {
    let mut target = "unresolved_workspace".to_owned();
    let result = async {
        let request: Request = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let (id, root) = {
            let mgr = sub.workspaces.lock().unwrap();
            workspace_debug::root(&mgr, &request.root)?
        };
        target = format!("/host/{id}");
        let cache = sub
            .workspace_process_policy
            .parent()
            .ok_or("workspace cache unavailable")?
            .to_path_buf();
        let intent = intent.to_owned();
        let caps = caps.to_vec();
        tokio::task::spawn_blocking(move || {
            workspace_inventory::execute(&root, &id, &cache, &intent, &request, &caps)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    .await;
    sub.audit(AuditAppendRequest{trace_id:trace.into(),actor:actor.into(),action:intent.into(),target,
        detail:json!({"ok":result.is_ok(),"error":result.as_ref().err().map(|_|"project_context_refused"),"eligible_files":result.as_ref().ok().and_then(|v|v["scan"]["eligible_files"].as_u64()),"complete":result.as_ref().ok().and_then(|v|v["scan"]["complete"].as_bool())})});
    result
}
