//! Intents `device.usb.*` (issue #137, slice 3).

use crate::device_usb::UsbIoError;
use crate::subsystem::PlatformSubsystem;
use aos_ipc::BusService;
use aos_proto::device_usb::{self, usb_io_capability};
use aos_proto::{
    AuditAppendRequest, UsbCloseRequest, UsbOpenRequest, UsbPermissionRevokeRequest,
    UsbReadRequest, UsbWriteRequest,
};
use std::collections::HashMap;
use std::sync::Arc;

pub fn register(svc: &mut BusService, sub: Arc<PlatformSubsystem>) {
    {
        let s = sub.clone();
        svc.on(device_usb::intents::ENUMERATE, move |ctx| {
            let s = s.clone();
            async move {
                let result = s.usb.lock().unwrap().enumerate();
                match result {
                    Ok(devices) => {
                        s.audit(AuditAppendRequest {
                            trace_id: String::new(),
                            actor: ctx.intent.from.clone(),
                            action: device_usb::intents::ENUMERATE.into(),
                            target: "usb".into(),
                            detail: serde_json::json!({ "count": devices.len() }),
                        });
                        let _ = ctx
                            .respond(
                                aos_ipc::msg::Status::Ok,
                                &aos_proto::UsbEnumerateResponse { devices },
                            )
                            .await;
                    }
                    Err(e) => respond_error(ctx, e).await,
                }
            }
        });
    }
    register_open(svc, sub.clone());
    register_io(svc, sub.clone(), device_usb::intents::READ, IoOp::Read);
    register_io(svc, sub.clone(), device_usb::intents::WRITE, IoOp::Write);
    {
        let s = sub.clone();
        svc.on(device_usb::intents::CLOSE, move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<UsbCloseRequest>() {
                    Ok(req) => {
                        let result = s.usb.lock().unwrap().close_handle(&req);
                        match result {
                            Ok(resp) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor: req.agent_id.clone(),
                                    action: device_usb::intents::CLOSE.into(),
                                    target: req.handle_id.clone(),
                                    detail: serde_json::json!({ "closed": resp.closed }),
                                });
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
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
    {
        let s = sub.clone();
        svc.on(device_usb::intents::HANDLE_ACTIVE, move |ctx| {
            let s = s.clone();
            async move {
                let active = s.usb.lock().unwrap().active_handles();
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &active).await;
            }
        });
    }
    {
        let s = sub.clone();
        svc.on("device.usb.permission.list", move |ctx| {
            let s = s.clone();
            async move {
                let agent = ctx.intent.from.strip_prefix("agent:");
                let permissions = s.usb.lock().unwrap().persistent_permissions(agent);
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &permissions).await;
            }
        });
    }
    {
        let s = sub.clone();
        svc.on("device.usb.permission.revoke", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<UsbPermissionRevokeRequest>() {
                    Ok(req) => {
                        let result = s
                            .usb
                            .lock()
                            .unwrap()
                            .revoke(&req.agent_id, &req.device_id);
                        match result {
                            Ok(stopped) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor: ctx.intent.from.clone(),
                                    action: "device.usb.permission.revoked".into(),
                                    target: req.device_id.clone(),
                                    detail: serde_json::json!({
                                        "agent_id": req.agent_id,
                                        "handles_closed": stopped.len()
                                    }),
                                });
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &stopped).await;
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

#[derive(Copy, Clone)]
enum IoOp {
    Read,
    Write,
}

fn register_open(svc: &mut BusService, sub: Arc<PlatformSubsystem>) {
    svc.on(device_usb::intents::OPEN, move |ctx| {
        let s = sub.clone();
        async move {
            let req = match ctx.payload::<UsbOpenRequest>() {
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
            let cap = usb_io_capability();
            let actor = req.agent_id.clone();
            s.audit(AuditAppendRequest {
                trace_id: String::new(),
                actor: actor.clone(),
                action: "device.usb.open.request".into(),
                target: req.device_id.clone(),
                detail: serde_json::json!({ "capability": cap }),
            });
            let persistent = s
                .usb
                .lock()
                .unwrap()
                .has_persistent_cap(&req.agent_id, &req.device_id);
            let tier = s.trust.lock().unwrap().tier(&req.agent_id);
            if !persistent && tier == crate::trust::Tier::Low {
                s.audit(AuditAppendRequest {
                    trace_id: String::new(),
                    actor: actor.clone(),
                    action: "device.usb.open.denied".into(),
                    target: req.device_id.clone(),
                    detail: serde_json::json!({ "reason": "trust_low" }),
                });
                let _ = ctx
                    .respond_error(
                        aos_ipc::msg::Status::PermissionDenied,
                        "confiance faible: accès USB refusé",
                    )
                    .await;
                return;
            }
            let allowed = if persistent {
                true
            } else {
                let mut context = HashMap::new();
                context.insert("action.kind".into(), cap.clone());
                context.insert("device.id".into(), req.device_id.clone());
                context.insert("device.kind".into(), "usb".into());
                s.policy_gate(context, &actor, &cap, &req.device_id, "device-usb")
                    .await
            };
            if !allowed {
                let _ = ctx
                    .respond_error(
                        aos_ipc::msg::Status::PermissionDenied,
                        "accès USB refusé ou confirmation expirée",
                    )
                    .await;
                return;
            }
            let result = s.usb.lock().unwrap().open_device(&req, true);
            match result {
                Ok(response) => {
                    s.audit(AuditAppendRequest {
                        trace_id: response.handle_id.clone(),
                        actor,
                        action: "device.usb.opened".into(),
                        target: req.device_id,
                        detail: serde_json::json!({
                            "handle_id": response.handle_id,
                            "class": response.class
                        }),
                    });
                    let _ = ctx.respond(aos_ipc::msg::Status::Ok, &response).await;
                }
                Err(e) => {
                    s.audit(AuditAppendRequest {
                        trace_id: String::new(),
                        actor,
                        action: "device.usb.error".into(),
                        target: req.device_id,
                        detail: serde_json::json!({ "error": e.to_string(), "operation": "open" }),
                    });
                    respond_error(ctx, e).await;
                }
            }
        }
    });
}

fn register_io(
    svc: &mut BusService,
    sub: Arc<PlatformSubsystem>,
    intent: &'static str,
    op: IoOp,
) {
    svc.on(intent, move |ctx| {
        let s = sub.clone();
        async move {
            let actor = ctx.intent.from.clone();
            match op {
                IoOp::Read => match ctx.payload::<UsbReadRequest>() {
                    Ok(mut req) => {
                        if ctx.intent.from.starts_with("agent:") {
                            req.agent_id = ctx.intent.from.clone();
                        }
                        let result = s.usb.lock().unwrap().read(&req);
                        match result {
                            Ok(resp) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor,
                                    action: device_usb::intents::READ.into(),
                                    target: req.handle_id.clone(),
                                    detail: serde_json::json!({
                                        "bytes_read": resp.bytes_read
                                    }),
                                });
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
                            }
                            Err(e) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor,
                                    action: "device.usb.error".into(),
                                    target: req.handle_id,
                                    detail: serde_json::json!({
                                        "error": e.to_string(),
                                        "operation": "read"
                                    }),
                                });
                                respond_error(ctx, e).await;
                            }
                        }
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                },
                IoOp::Write => match ctx.payload::<UsbWriteRequest>() {
                    Ok(mut req) => {
                        if ctx.intent.from.starts_with("agent:") {
                            req.agent_id = ctx.intent.from.clone();
                        }
                        let payload_len = req.data_base64.len();
                        let result = s.usb.lock().unwrap().write(&req);
                        match result {
                            Ok(resp) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor,
                                    action: device_usb::intents::WRITE.into(),
                                    target: req.handle_id.clone(),
                                    detail: serde_json::json!({
                                        "bytes_written": resp.bytes_written,
                                        "payload_chars": payload_len
                                    }),
                                });
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
                            }
                            Err(e) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor,
                                    action: "device.usb.error".into(),
                                    target: req.handle_id,
                                    detail: serde_json::json!({
                                        "error": e.to_string(),
                                        "operation": "write"
                                    }),
                                });
                                respond_error(ctx, e).await;
                            }
                        }
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                },
            }
        }
    });
}

async fn respond_error(ctx: aos_ipc::IntentCtx, error: UsbIoError) {
    let status = match error {
        UsbIoError::UnsupportedPlatform => aos_ipc::msg::Status::NotFound,
        UsbIoError::DeviceAbsent(_) => aos_ipc::msg::Status::NotFound,
        UsbIoError::OsPermissionDenied => aos_ipc::msg::Status::PermissionDenied,
        UsbIoError::DeviceBusy | UsbIoError::QuotaExceeded(_) | UsbIoError::IoTimeout => {
            aos_ipc::msg::Status::PermissionDenied
        }
        UsbIoError::HandleNotFound(_)
        | UsbIoError::InvalidRequest(_)
        | UsbIoError::Backend(_) => aos_ipc::msg::Status::InternalError,
    };
    let message = error.to_string();
    let _ = ctx.respond_error(status, &message).await;
}
