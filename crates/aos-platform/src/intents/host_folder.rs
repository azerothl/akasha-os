//! Intents `fs.host.*` (issue #157).

use crate::host_folder::{folder_key_for_request, HostFolderError};
use crate::subsystem::PlatformSubsystem;
use aos_ipc::BusService;
use aos_proto::folder_display_name;
use aos_proto::host_folder::intents;
use aos_proto::{
    AuditAppendRequest, HostFolderAccessRequest, HostFolderAccessResponse, HostFolderOperation,
    HostFolderPermissionRevokeRequest, HOST_FOLDER_ACCESS_ACTION,
};
use std::collections::HashMap;
use std::sync::Arc;

pub fn register(svc: &mut BusService, sub: Arc<PlatformSubsystem>) {
    {
        let s = sub.clone();
        svc.on(intents::ACCESS, move |ctx| {
            let s = s.clone();
            async move {
                let req = match ctx.payload::<HostFolderAccessRequest>() {
                    Ok(mut req) => {
                        if ctx.intent.from.starts_with("agent:") {
                            req.agent_id = ctx.intent.from.clone();
                        }
                        req
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                let agent_id = req.agent_id.clone();
                let folder_key = folder_key_for_request(&req.path);
                if folder_key.is_empty() {
                    let _ = ctx
                        .respond_error(aos_ipc::msg::Status::BadRequest, "chemin invalide")
                        .await;
                    return;
                }
                let display = folder_display_name(&folder_key);
                let actor = agent_id.clone();
                s.audit(AuditAppendRequest {
                    trace_id: req.trace_id.clone(),
                    actor: actor.clone(),
                    action: "fs.host.access.request".into(),
                    target: display.clone(),
                    detail: serde_json::json!({
                        "operation": req.operation,
                        "folder": display,
                    }),
                });

                let persistent = {
                    let mgr = s.host_folders.lock().unwrap();
                    mgr.has_persistent_grant(&agent_id, &folder_key)
                };
                let gate = if !persistent {
                    let mut context = HashMap::new();
                    context.insert("action.kind".into(), HOST_FOLDER_ACCESS_ACTION.into());
                    context.insert("folder.name".into(), display.clone());
                    Some(
                        s.policy_gate(
                            context,
                            &actor,
                            HOST_FOLDER_ACCESS_ACTION,
                            &display,
                            &req.trace_id,
                        )
                        .await,
                    )
                } else {
                    None
                };
                if let Some(result) = gate {
                    if !result.approved {
                        s.audit(AuditAppendRequest {
                            trace_id: req.trace_id.clone(),
                            actor,
                            action: "fs.host.access.denied".into(),
                            target: display,
                            detail: serde_json::json!({ "reason": "confirmation" }),
                        });
                        let _ = ctx
                            .respond(
                                aos_ipc::msg::Status::PermissionDenied,
                                &HostFolderAccessResponse {
                                    ok: false,
                                    content: None,
                                    entries: None,
                                    message: Some("room_host_path_disallowed".into()),
                                },
                            )
                            .await;
                        return;
                    }
                    let mut mgr = s.host_folders.lock().unwrap();
                    if result.persistent {
                        let _ = mgr.grant_persistent(&agent_id, &folder_key);
                    } else {
                        mgr.grant_once(&agent_id, &folder_key);
                    }
                }

                let content = req.content.as_deref();
                let format = req.format.as_deref();
                let exec = {
                    let mut mgr = s.host_folders.lock().unwrap();
                    mgr.execute(
                        &agent_id,
                        &folder_key,
                        &req.path,
                        req.operation,
                        content,
                        format,
                    )
                };
                match exec {
                    Ok(out) => {
                        let (content, entries) = match req.operation {
                            HostFolderOperation::List => (None, serde_json::from_str(&out).ok()),
                            _ => (Some(out), None),
                        };
                        s.audit(AuditAppendRequest {
                            trace_id: req.trace_id.clone(),
                            actor,
                            action: "fs.host.access.opened".into(),
                            target: display,
                            detail: serde_json::json!({ "operation": req.operation }),
                        });
                        let _ = ctx
                            .respond(
                                aos_ipc::msg::Status::Ok,
                                &HostFolderAccessResponse {
                                    ok: true,
                                    content,
                                    entries,
                                    message: None,
                                },
                            )
                            .await;
                    }
                    Err(e) => respond_error(ctx, e).await,
                }
            }
        });
    }
    {
        let s = sub.clone();
        svc.on("fs.host.permission.list", move |ctx| {
            let s = s.clone();
            async move {
                let agent = ctx.intent.from.strip_prefix("agent:");
                let permissions = s.host_folders.lock().unwrap().persistent_permissions(agent);
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &permissions).await;
            }
        });
    }
    {
        let s = sub.clone();
        svc.on("fs.host.permission.revoke", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<HostFolderPermissionRevokeRequest>() {
                    Ok(req) => {
                        let result = s
                            .host_folders
                            .lock()
                            .unwrap()
                            .revoke(&req.agent_id, &req.folder_key);
                        match result {
                            Ok(()) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor: ctx.intent.from.clone(),
                                    action: "fs.host.permission.revoked".into(),
                                    target: req.folder_key.clone(),
                                    detail: serde_json::json!({ "agent_id": req.agent_id }),
                                });
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &true).await;
                            }
                            Err(e) => respond_error(ctx, e).await,
                        }
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }
}

async fn respond_error(ctx: aos_ipc::IntentCtx, error: HostFolderError) {
    let status = match error {
        HostFolderError::PermissionDenied => aos_ipc::msg::Status::PermissionDenied,
        HostFolderError::NotFound(_) => aos_ipc::msg::Status::NotFound,
        HostFolderError::InvalidPath(_) | HostFolderError::Io(_) => {
            aos_ipc::msg::Status::InternalError
        }
    };
    let _ = ctx
        .respond(
            status,
            &HostFolderAccessResponse {
                ok: false,
                content: None,
                entries: None,
                message: Some(error.to_string()),
            },
        )
        .await;
}
