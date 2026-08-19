//! `fs.*` intents.

use aos_ipc::BusService;
use aos_proto::*;
use crate::intents::helpers::resolve_fs_caps;
use crate::subsystem::PlatformSubsystem;
use std::sync::Arc;

pub fn register(svc: &mut BusService, sub: Arc<PlatformSubsystem>) {
    {
        let s = sub.clone();
        svc.on("fs.read", move |ctx| {
            let s = s.clone();
            async move {
                let envelope = ctx.intent.caps.clone();
                let from = ctx.intent.from.clone();
                match ctx.payload::<FsReadRequest>() {
                    Ok(req) => {
                        let holder = if req.actor.is_empty() {
                            from
                        } else {
                            req.actor.clone()
                        };
                        let caps = match resolve_fs_caps(
                            &s,
                            &envelope,
                            &holder,
                            &req.path,
                            "read",
                            "fs.read",
                            req.caps.clone(),
                        )
                        .await
                        {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::PermissionDenied, &e)
                                    .await;
                                return;
                            }
                        };
                        let r = s.fs.lock().unwrap().read(&req.path, &caps);
                        match r {
                            Ok((content, class, version)) => {
                                let _ = ctx
                                    .respond(
                                        aos_ipc::msg::Status::Ok,
                                        &FsReadResponse {
                                            path: req.path,
                                            content,
                                            class,
                                            version,
                                        },
                                    )
                                    .await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::PermissionDenied,
                                        &e.to_string(),
                                    )
                                    .await;
                            }
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
    {
        let s = sub.clone();
        svc.on("fs.write", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<FsWriteRequest>() {
                    Ok(req) => {
                        let envelope = ctx.intent.caps.clone();
                        let from = ctx.intent.from.clone();
                        let trace = req.trace_id.clone();
                        let actor = if req.actor.is_empty() {
                            "human:ui".to_string()
                        } else {
                            req.actor.clone()
                        };
                        let holder = if req.actor.is_empty() {
                            from
                        } else {
                            req.actor.clone()
                        };
                        let caps = match resolve_fs_caps(
                            &s,
                            &envelope,
                            &holder,
                            &req.path,
                            "write",
                            "fs.write",
                            req.caps.clone(),
                        )
                        .await
                        {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::PermissionDenied, &e)
                                    .await;
                                return;
                            }
                        };
                        let r =
                            s.fs.lock()
                                .unwrap()
                                .write(&req.path, &req.content, &actor, &caps);
                        match r {
                            Ok(version) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: trace,
                                    actor,
                                    action: "fs.write".into(),
                                    target: req.path.clone(),
                                    detail: serde_json::json!({"version": version}),
                                });
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &version).await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::PermissionDenied,
                                        &e.to_string(),
                                    )
                                    .await;
                            }
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
    {
        let s = sub.clone();
        svc.on("fs.list", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<FsListRequest>() {
                    Ok(req) => {
                        let entries = s.fs.lock().unwrap().list(&req.prefix, &req.caps);
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &entries).await;
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
    {
        let s = sub.clone();
        svc.on("fs.begin_tx", move |ctx| {
            let s = s.clone();
            async move {
                let actor = ctx
                    .payload::<FsTxRequest>()
                    .map(|r| r.actor)
                    .unwrap_or_else(|_| "human:ui".into());
                let tx = s.fs.lock().unwrap().begin_tx(&actor);
                let _ = ctx
                    .respond(
                        aos_ipc::msg::Status::Ok,
                        &FsTxResponse {
                            tx_id: tx,
                            committed_ops: 0,
                        },
                    )
                    .await;
            }
        });
    }
    {
        let s = sub.clone();
        svc.on("fs.commit", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<FsTxRequest>() {
                    Ok(req) => {
                        let id = req.tx_id.unwrap_or_default();
                        let r = s.fs.lock().unwrap().commit(&id);
                        match r {
                            Ok(n) => {
                                let _ = ctx
                                    .respond(
                                        aos_ipc::msg::Status::Ok,
                                        &FsTxResponse {
                                            tx_id: id,
                                            committed_ops: n as u32,
                                        },
                                    )
                                    .await;
                            }
                            Err(e) => {
                                if matches!(e, crate::storage::FsError::Conflict(_)) {
                                    s.audit(AuditAppendRequest {
                                        trace_id: String::new(),
                                        actor: "service:platformd".into(),
                                        action: "fs.conflict".into(),
                                        target: id.clone(),
                                        detail: serde_json::json!({"error": e.to_string()}),
                                    });
                                }
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::BadRequest, &e.to_string())
                                    .await;
                            }
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
    {
        let s = sub.clone();
        svc.on("fs.rollback", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<FsTxRequest>() {
                    Ok(req) => {
                        let id = req.tx_id.unwrap_or_default();
                        let r = s.fs.lock().unwrap().rollback(&id);
                        let _ = ctx
                            .respond(aos_ipc::msg::Status::Ok, &r.map_err(|e| e.to_string()))
                            .await;
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
    {
        let s = sub.clone();
        svc.on("fs.delete", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<FsDeleteRequest>() {
                    Ok(req) => {
                        let envelope = ctx.intent.caps.clone();
                        let from = ctx.intent.from.clone();
                        let trace = req.trace_id.clone();
                        let actor = if req.actor.is_empty() {
                            "human:ui".to_string()
                        } else {
                            req.actor.clone()
                        };
                        let holder = if req.actor.is_empty() {
                            from
                        } else {
                            req.actor.clone()
                        };
                        let allowed = s
                            .policy_gate(
                                std::collections::HashMap::from([(
                                    "actor".to_string(),
                                    actor.clone(),
                                )]),
                                &actor,
                                "fs.delete",
                                &req.path,
                                &trace,
                            )
                            .await;
                        if !allowed {
                            let _ = ctx
                                .respond_error(
                                    aos_ipc::msg::Status::PermissionDenied,
                                    "action refusée (politique ou timeout de confirmation)",
                                )
                                .await;
                            return;
                        }
                        let caps = match resolve_fs_caps(
                            &s,
                            &envelope,
                            &holder,
                            &req.path,
                            "write",
                            "fs.write",
                            req.caps.clone(),
                        )
                        .await
                        {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::PermissionDenied, &e)
                                    .await;
                                return;
                            }
                        };
                        let r = s.fs.lock().unwrap().delete(&req.path, &actor, &caps);
                        match r {
                            Ok(v) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: trace,
                                    actor,
                                    action: "fs.delete".into(),
                                    target: req.path.clone(),
                                    detail: serde_json::json!({}),
                                });
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &v).await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::PermissionDenied,
                                        &e.to_string(),
                                    )
                                    .await;
                            }
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
    {
        let s = sub.clone();
        svc.on("fs.set_class", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<FsSetClassRequest>() {
                    Ok(req) => {
                        let r =
                            s.fs.lock()
                                .unwrap()
                                .set_class(&req.path, req.class, &req.caps);
                        let _ = ctx
                            .respond(aos_ipc::msg::Status::Ok, &r.map_err(|e| e.to_string()))
                            .await;
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
