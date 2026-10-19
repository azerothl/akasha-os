//! Actor/capability/confirmation boundary shared by bus clients and modules.
use crate::{
    subsystem::PlatformSubsystem,
    workspace_debug::{self, DebugRequest},
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
    let target = args["root"].as_str().unwrap_or("").to_owned();
    let result = execute(sub, intent, args, actor, caps).await;
    sub.audit(AuditAppendRequest {
        trace_id: trace.into(),
        actor: actor.into(),
        action: intent.into(),
        target,
        detail: json!({"ok":result.is_ok(),"error":result.as_ref().err()}),
    });
    result
}
async fn execute(
    sub: &PlatformSubsystem,
    intent: &str,
    args: Value,
    actor: &str,
    caps: &[String],
) -> Result<Value, String> {
    let req: DebugRequest = serde_json::from_value(args).map_err(|e| e.to_string())?;
    let (id, root) = {
        let mgr = sub.workspaces.lock().unwrap();
        workspace_debug::root(&mgr, &req.root)?
    };
    let session_id = req.session_id.as_deref().unwrap_or("");
    if intent == "debug.stop" {
        // Owners can always stop their own session, including after cap revocation.
        return sub.debug.lock().await.stop(session_id, actor, &id).await;
    }
    workspace_debug::require_caps(&id, caps)?;
    if sub.net.lock().unwrap().mode() == crate::net::NetMode::OfflineStrict {
        if !session_id.is_empty() {
            let _ = sub.debug.lock().await.stop(session_id, actor, &id).await;
        }
        return Err(
            "debugging is disabled in offline_strict: host adapters are not network-sandboxed"
                .into(),
        );
    }
    if intent == "debug.start" {
        let prepared = {
            let mgr = sub.workspaces.lock().unwrap();
            workspace_debug::prepare(
                &mgr,
                &sub.workspace_process_policy
                    .with_file_name("workspace-debug-policy.json"),
                &req,
                caps,
            )?
        };
        let (_,reply)=sub.confirm.ask(actor.into(),intent.into(),req.root.clone(),
            format!("Autoriser {} dans {} ? L'adaptateur et le programme utilisent les droits de votre compte.",prepared.description(),root.display()),Some(120)).await;
        if !reply.await.map(|r| r.approved).unwrap_or(false) {
            return Err("confirmation denied or expired".into());
        }
        let refreshed = {
            let mgr = sub.workspaces.lock().unwrap();
            workspace_debug::prepare(
                &mgr,
                &sub.workspace_process_policy
                    .with_file_name("workspace-debug-policy.json"),
                &req,
                caps,
            )?
        };
        if refreshed.root != prepared.root || refreshed.description() != prepared.description() {
            return Err(
                "debug policy or workspace changed during confirmation; review again".into(),
            );
        }
        let result = {
            let mut debug = sub.debug.lock().await;
            if sub.net.lock().unwrap().mode() == crate::net::NetMode::OfflineStrict {
                return Err("debugging disabled in offline_strict".into());
            }
            debug.start(refreshed, actor).await?
        };
        let expires = result["session_id"]
            .as_str()
            .ok_or("debug session id missing")?
            .to_owned();
        let manager = sub.debug.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(
                workspace_debug::SESSION_SECONDS,
            ))
            .await;
            manager.lock().await.expire(&expires);
        });
        Ok(result)
    } else if intent == "debug.request" {
        sub.debug
            .lock()
            .await
            .request(session_id, actor, &id, &req)
            .await
    } else {
        Err("unsupported debug intent".into())
    }
}
