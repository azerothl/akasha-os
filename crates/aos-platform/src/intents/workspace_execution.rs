//! Capability and confirmation boundary for Dev Assistant host commands.
use crate::{subsystem::PlatformSubsystem, workspace_process};
use aos_ipc::{msg::Status, BusService};
use aos_proto::{
    workspace::{GitCommitRequest, WorkspaceTaskRequest},
    AuditAppendRequest,
};
use serde_json::{json, Value};
use std::sync::Arc;

pub fn register(svc: &mut BusService, sub: Arc<PlatformSubsystem>) {
    for intent in [
        "process.run",
        "git.commit",
        "studio.project",
        "studio.lsp",
        "studio.reindex",
    ] {
        let sub = sub.clone();
        svc.on(intent, move |ctx| {
            let sub = sub.clone();
            async move {
                let args = match ctx.payload::<Value>() {
                    Ok(value) => value,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(Status::BadRequest, "invalid workspace request")
                            .await;
                        return;
                    }
                };
                let actor = ctx.intent.from.clone();
                let caps = sub
                    .granted_caps
                    .lock()
                    .unwrap()
                    .get(&actor)
                    .cloned()
                    .unwrap_or_default();
                // Transport capabilities are the same explicit grants used by other
                // bus clients; caller-supplied actor/caps in the payload are ignored.
                let caps = if ctx.intent.caps.is_empty() {
                    caps
                } else {
                    ctx.intent.caps.clone()
                };
                let trace = format!(
                    "workspace-{}-{}",
                    std::process::id(),
                    ctx.intent.correlation_id
                );
                match dispatch(&sub, intent, args, &actor, &caps, &trace).await {
                    Ok(value) => {
                        let _ = ctx.respond(Status::Ok, &value).await;
                    }
                    Err(error) => {
                        let _ = ctx.respond_error(Status::PermissionDenied, &error).await;
                    }
                }
            }
        });
    }
}

pub async fn dispatch(
    sub: &PlatformSubsystem,
    intent: &str,
    mut args: Value,
    actor: &str,
    caps: &[String],
    trace: &str,
) -> Result<Value, String> {
    if intent.starts_with("studio.") {
        return dispatch_studio(sub, intent, args, actor, caps, trace).await;
    }
    if !args.is_object() {
        return Err("arguments must be an object".into());
    }
    args["actor"] = json!(actor);
    args["caps"] = json!(caps);
    args["trace_id"] = json!(trace);
    let root = args["root"].as_str().unwrap_or("").to_owned();
    let prepared = {
        let mgr = sub.workspaces.lock().unwrap();
        match intent {
            "process.run" => serde_json::from_value::<WorkspaceTaskRequest>(args)
                .map_err(|e| e.to_string())
                .and_then(|req| {
                    workspace_process::prepare_task(&mgr, &req, &sub.workspace_process_policy)
                }),
            "git.commit" => serde_json::from_value::<GitCommitRequest>(args)
                .map_err(|e| e.to_string())
                .and_then(|req| workspace_process::prepare_commit(&mgr, &req)),
            _ => Err("unsupported workspace execution intent".into()),
        }
    };
    let command = match prepared {
        Ok(command) => command,
        Err(error) => {
            sub.audit(AuditAppendRequest {
                trace_id: trace.into(),
                actor: actor.into(),
                action: intent.into(),
                target: root,
                detail: json!({"ok":false,"error":error}),
            });
            return Err(error);
        }
    };
    let reason=format!("Autoriser {} {:?} dans /host/{} ? Cette commande utilise les droits de votre compte sur l'hôte.",
        command.program.file_name().unwrap_or_default().to_string_lossy(),command.args,command.workspace_id);
    let (_, answer) = sub
        .confirm
        .ask(actor.into(), intent.into(), root.clone(), reason, Some(120))
        .await;
    if !answer.await.map(|result| result.approved).unwrap_or(false) {
        sub.audit(AuditAppendRequest {
            trace_id: trace.into(),
            actor: actor.into(),
            action: intent.into(),
            target: root,
            detail: json!({"ok":false,"error":"confirmation denied or expired"}),
        });
        return Err("confirmation denied or expired".into());
    }
    let started = std::time::Instant::now();
    let result = workspace_process::execute(command).await;
    sub.audit(AuditAppendRequest {trace_id:trace.into(),actor:actor.into(),action:intent.into(),target:root,
        detail:json!({"ok":result.as_ref().map(|r|r.ok).unwrap_or(false),
            "exit_code":result.as_ref().ok().and_then(|r|r.exit_code),"duration_ms":started.elapsed().as_millis(),
            "error":result.as_ref().err()})});
    serde_json::to_value(result?).map_err(|e| e.to_string())
}

async fn dispatch_studio(
    sub: &PlatformSubsystem,
    intent: &str,
    args: Value,
    actor: &str,
    caps: &[String],
    trace: &str,
) -> Result<Value, String> {
    let root = args["root"].as_str().unwrap_or("").to_owned();
    let prepared = serde_json::from_value::<crate::studio_adapter::Request>(args)
        .map_err(|e| e.to_string())
        .and_then(|request| {
            let mgr = sub.workspaces.lock().unwrap();
            crate::studio_adapter::prepare(
                &mgr,
                &sub.workspace_process_policy
                    .with_file_name("studio-adapter.json"),
                intent,
                request,
                caps,
            )
        });
    let result = match prepared {
        Ok(command) => {
            if intent != "studio.project" {
                let (_, answer) = sub.confirm.ask(actor.into(), intent.into(), root.clone(),
                    format!("Autoriser {intent} pour /host/{} via le daemon Code Studio local ? Le serveur de langage utilise les droits de votre compte.",command.workspace_id), Some(120)).await;
                if !answer.await.map(|r| r.approved).unwrap_or(false) {
                    Err("confirmation denied or expired".into())
                } else {
                    command.execute().await
                }
            } else {
                command.execute().await
            }
        }
        Err(error) => Err(error),
    };
    sub.audit(AuditAppendRequest {
        trace_id: trace.into(),
        actor: actor.into(),
        action: intent.into(),
        target: root,
        detail: json!({"ok":result.is_ok(),"error":result.as_ref().err()}),
    });
    result
}
