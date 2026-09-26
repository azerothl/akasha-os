//! Intents `workspace.*` + `fs.search` / `fs.apply_patch` (#247 P0).

use crate::subsystem::PlatformSubsystem;
use crate::workspace::WorkspaceError;
use aos_ipc::BusService;
use aos_proto::host_folder::looks_like_host_path;
use aos_proto::workspace::intents;
use aos_proto::{
    AuditAppendRequest, FsApplyPatchRequest, FsSearchRequest, FsUndoPatchRequest,
    WorkspaceBindRequest, WorkspaceBindResponse, WorkspaceListRequest, WorkspaceListResponse,
    WorkspaceUnbindRequest, WorkspaceUnbindResponse,
};
use std::sync::Arc;

pub fn register(svc: &mut BusService, sub: Arc<PlatformSubsystem>) {
    {
        let s = sub.clone();
        svc.on(intents::BIND, move |ctx| {
            let s = s.clone();
            async move {
                let mut req = match ctx.payload::<WorkspaceBindRequest>() {
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
                if req.agent_id.is_empty() {
                    req.agent_id = ctx.intent.from.clone();
                }
                if !looks_like_host_path(&req.host_path) {
                    let _ = ctx
                        .respond(
                            aos_ipc::msg::Status::Ok,
                            &WorkspaceBindResponse {
                                ok: false,
                                workspace_id: None,
                                vfs_root: None,
                                host_path: None,
                                caps: vec![],
                                message: Some(
                                    "chemin hôte invalide — dossier disque absolu requis".into(),
                                ),
                            },
                        )
                        .await;
                    return;
                }

                let result = {
                    let mut mgr = s.workspaces.lock().unwrap();
                    mgr.bind(
                        &req.host_path,
                        req.workspace_id.as_deref(),
                        &req.agent_id,
                    )
                };

                match result {
                    Ok(info) => {
                        if req.grant_persistent {
                            let folder_key = aos_proto::normalize_folder_key(&info.host_path);
                            let _ = s
                                .host_folders
                                .lock()
                                .unwrap()
                                .grant_persistent(&req.agent_id, &folder_key);
                        }
                        s.audit(AuditAppendRequest {
                            trace_id: req.trace_id.clone(),
                            actor: req.agent_id.clone(),
                            action: "workspace.bind".into(),
                            target: info.display_name.clone(),
                            detail: serde_json::json!({
                                "workspace_id": info.workspace_id,
                                "vfs_root": info.vfs_root,
                            }),
                        });
                        let _ = ctx
                            .respond(
                                aos_ipc::msg::Status::Ok,
                                &WorkspaceBindResponse {
                                    ok: true,
                                    workspace_id: Some(info.workspace_id),
                                    vfs_root: Some(info.vfs_root),
                                    host_path: Some(info.host_path),
                                    caps: info.caps,
                                    message: None,
                                },
                            )
                            .await;
                    }
                    Err(e) => {
                        let _ = ctx
                            .respond(
                                aos_ipc::msg::Status::Ok,
                                &WorkspaceBindResponse {
                                    ok: false,
                                    workspace_id: None,
                                    vfs_root: None,
                                    host_path: None,
                                    caps: vec![],
                                    message: Some(e.to_string()),
                                },
                            )
                            .await;
                    }
                }
            }
        });
    }

    {
        let s = sub.clone();
        svc.on(intents::UNBIND, move |ctx| {
            let s = s.clone();
            async move {
                let mut req = match ctx.payload::<WorkspaceUnbindRequest>() {
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
                if req.agent_id.is_empty() {
                    req.agent_id = ctx.intent.from.clone();
                }
                let result = {
                    let mut mgr = s.workspaces.lock().unwrap();
                    mgr.unbind(&req.workspace_id, &req.agent_id)
                };
                match result {
                    Ok(()) => {
                        s.audit(AuditAppendRequest {
                            trace_id: req.trace_id.clone(),
                            actor: req.agent_id,
                            action: "workspace.unbind".into(),
                            target: req.workspace_id,
                            detail: serde_json::json!({}),
                        });
                        let _ = ctx
                            .respond(
                                aos_ipc::msg::Status::Ok,
                                &WorkspaceUnbindResponse {
                                    ok: true,
                                    message: None,
                                },
                            )
                            .await;
                    }
                    Err(WorkspaceError::NotFound(id)) => {
                        let _ = ctx
                            .respond(
                                aos_ipc::msg::Status::Ok,
                                &WorkspaceUnbindResponse {
                                    ok: false,
                                    message: Some(format!("workspace inconnu: {id}")),
                                },
                            )
                            .await;
                    }
                    Err(e) => {
                        let _ = ctx
                            .respond(
                                aos_ipc::msg::Status::Ok,
                                &WorkspaceUnbindResponse {
                                    ok: false,
                                    message: Some(e.to_string()),
                                },
                            )
                            .await;
                    }
                }
            }
        });
    }

    {
        let s = sub.clone();
        svc.on(intents::LIST, move |ctx| {
            let s = s.clone();
            async move {
                let req = match ctx.payload::<WorkspaceListRequest>() {
                    Ok(mut req) => {
                        if req.agent_id.is_empty() && ctx.intent.from.starts_with("agent:") {
                            req.agent_id = ctx.intent.from.clone();
                        }
                        req
                    }
                    Err(_) => WorkspaceListRequest {
                        agent_id: ctx.intent.from.clone(),
                    },
                };
                let bindings = s.workspaces.lock().unwrap().list(&req.agent_id);
                let _ = ctx
                    .respond(aos_ipc::msg::Status::Ok, &WorkspaceListResponse { bindings })
                    .await;
            }
        });
    }

    // DA.2 search + DA.3 patch; 
    register_search(svc, sub.clone(), intents::FS_SEARCH);
    register_search(svc, sub.clone(), intents::CODE_SEARCH);
    {
        let s = sub.clone();
        svc.on(intents::APPLY_PATCH, move |ctx| {
            let s = s.clone();
            async move {
                let mut req = match ctx.payload::<FsApplyPatchRequest>() {
                    Ok(req) => req,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                if req.actor.is_empty() && ctx.intent.from.starts_with("agent:") {
                    req.actor = ctx.intent.from.clone();
                }
                if req.caps.is_empty() && !req.actor.is_empty() {
                    if let Some(granted) = s.granted_caps.lock().unwrap().get(&req.actor) {
                        req.caps = granted.clone();
                    }
                }
                let resp = {
                    let binds = s.workspaces.lock().unwrap();
                    let mut patches = s.workspace_patches.lock().unwrap();
                    patches.apply(&binds, &req)
                };
                if resp.ok {
                    s.audit(AuditAppendRequest {
                        trace_id: req.trace_id.clone(),
                        actor: req.actor.clone(),
                        action: "fs.apply_patch".into(),
                        target: resp.undo_group_id.clone().unwrap_or_default(),
                        detail: serde_json::json!({
                            "applied": resp.applied.len(),
                            "paths": resp.applied,
                        }),
                    });
                }
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
            }
        });
    }
    {
        let s = sub.clone();
        svc.on(intents::UNDO_PATCH, move |ctx| {
            let s = s.clone();
            async move {
                let mut req = match ctx.payload::<FsUndoPatchRequest>() {
                    Ok(req) => req,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                if req.actor.is_empty() && ctx.intent.from.starts_with("agent:") {
                    req.actor = ctx.intent.from.clone();
                }
                if req.caps.is_empty() && !req.actor.is_empty() {
                    if let Some(granted) = s.granted_caps.lock().unwrap().get(&req.actor) {
                        req.caps = granted.clone();
                    }
                }
                let resp = {
                    let mut patches = s.workspace_patches.lock().unwrap();
                    patches.undo(&req)
                };
                if resp.ok {
                    s.audit(AuditAppendRequest {
                        trace_id: req.trace_id.clone(),
                        actor: req.actor.clone(),
                        action: "fs.undo_patch".into(),
                        target: req.undo_group_id.clone(),
                        detail: serde_json::json!({ "restored": resp.restored.len() }),
                    });
                }
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
            }
        });
    }
}

fn register_search(svc: &mut BusService, sub: Arc<PlatformSubsystem>, intent: &'static str) {
    svc.on(intent, move |ctx| {
        let s = sub.clone();
        async move {
            let mut req = match ctx.payload::<FsSearchRequest>() {
                Ok(req) => req,
                Err(_) => {
                    let _ = ctx
                        .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                        .await;
                    return;
                }
            };
            if req.actor.is_empty() && ctx.intent.from.starts_with("agent:") {
                req.actor = ctx.intent.from.clone();
            }
            // Merge session-granted string caps when payload caps empty.
            if req.caps.is_empty() && !req.actor.is_empty() {
                if let Some(granted) = s.granted_caps.lock().unwrap().get(&req.actor) {
                    req.caps = granted.clone();
                }
            }
            let resp = {
                let mgr = s.workspaces.lock().unwrap();
                crate::workspace_search::search_workspace(&mgr, &req)
            };
            if resp.ok {
                s.audit(AuditAppendRequest {
                    trace_id: req.trace_id.clone(),
                    actor: req.actor.clone(),
                    action: intent.into(),
                    target: req.root.clone(),
                    detail: serde_json::json!({
                        "query": req.query,
                        "hits": resp.hits.len(),
                        "truncated": resp.truncated,
                    }),
                });
            }
            let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
        }
    });
}
