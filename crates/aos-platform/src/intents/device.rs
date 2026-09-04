//! Intents `device.*` (issue #137).

use crate::device_capture::DeviceCaptureError;
use crate::subsystem::PlatformSubsystem;
use aos_ipc::BusService;
use aos_proto::device_capture::{self, DeviceKind};
use aos_proto::{
    AuditAppendRequest, DeviceCaptureRequest, DeviceCaptureStopRequest,
    DevicePermissionRevokeRequest,
};
use std::collections::HashMap;
use std::sync::Arc;

pub fn register(svc: &mut BusService, sub: Arc<PlatformSubsystem>) {
    {
        let s = sub.clone();
        svc.on(device_capture::intents::ENUMERATE, move |ctx| {
            let s = s.clone();
            async move {
                let result = s.devices.lock().unwrap().enumerate();
                match result {
                    Ok(devices) => {
                        s.audit(AuditAppendRequest {
                            trace_id: String::new(),
                            actor: ctx.intent.from.clone(),
                            action: device_capture::intents::ENUMERATE.into(),
                            target: "device".into(),
                            detail: serde_json::json!({ "count": devices.len() }),
                        });
                        let _ = ctx
                            .respond(
                                aos_ipc::msg::Status::Ok,
                                &aos_proto::DeviceEnumerateResponse { devices },
                            )
                            .await;
                    }
                    Err(e) => respond_error(ctx, e).await,
                }
            }
        });
    }
    register_capture(
        svc,
        sub.clone(),
        DeviceKind::Camera,
        device_capture::intents::CAMERA_CAPTURE,
    );
    register_capture(
        svc,
        sub.clone(),
        DeviceKind::Microphone,
        device_capture::intents::MIC_CAPTURE,
    );
    {
        let s = sub.clone();
        svc.on(device_capture::intents::CAPTURE_ACTIVE, move |ctx| {
            let s = s.clone();
            async move {
                let active = s.devices.lock().unwrap().active_captures();
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &active).await;
            }
        });
    }
    {
        let s = sub.clone();
        svc.on(device_capture::intents::CAPTURE_STOP, move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<DeviceCaptureStopRequest>() {
                    Ok(req) => {
                        let result = {
                            let mut devices = s.devices.lock().unwrap();
                            devices.stop_for_agent(&req.capture_id, &req.agent_id)
                        };
                        match result {
                            Ok((duration_ms, _size)) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: String::new(), actor: req.agent_id.clone(),
                                    action: device_capture::intents::CAPTURE_STOP.into(),
                                    target: req.capture_id.clone(),
                                    detail: serde_json::json!({ "duration_ms": duration_ms, "result": "stopped" }),
                                });
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &aos_proto::DeviceCaptureStopResponse {
                                    capture_id: req.capture_id, stopped: true, duration_ms,
                                }).await;
                            }
                            Err(e) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor: req.agent_id.clone(),
                                    action: "device.capture.error".into(),
                                    target: req.capture_id.clone(),
                                    detail: serde_json::json!({ "error": e.to_string(), "operation": "stop" }),
                                });
                                respond_error(ctx, e).await
                            }
                        }
                    }
                    Err(_) => { let _ = ctx.respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide").await; }
                }
            }
        });
    }
    {
        let s = sub.clone();
        svc.on("device.permission.list", move |ctx| {
            let s = s.clone();
            async move {
                let agent = ctx.intent.from.strip_prefix("agent:");
                let permissions = s.devices.lock().unwrap().persistent_permissions(agent);
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &permissions).await;
            }
        });
    }
    {
        let s = sub.clone();
        svc.on("device.permission.revoke", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<DevicePermissionRevokeRequest>() {
                    Ok(req) => {
                        let result = {
                            let mut devices = s.devices.lock().unwrap();
                            devices.revoke(&req.agent_id, &req.device_id, req.kind, req.mode)
                        };
                        match result {
                            Ok(stopped) => {
                                s.audit(AuditAppendRequest { trace_id: String::new(), actor: ctx.intent.from.clone(), action: "device.permission.revoked".into(), target: req.device_id.clone(), detail: serde_json::json!({ "agent_id": req.agent_id, "kind": req.kind, "mode": req.mode, "stopped": stopped }) });
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &stopped).await;
                            }
                            Err(e) => respond_error(ctx, e).await,
                        }
                    }
                    Err(_) => { let _ = ctx.respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide").await; }
                }
            }
        });
    }
}

fn register_capture(
    svc: &mut BusService,
    sub: Arc<PlatformSubsystem>,
    kind: DeviceKind,
    intent: &'static str,
) {
    svc.on(intent, move |ctx| {
        let s = sub.clone();
        async move {
            let req = match ctx.payload::<DeviceCaptureRequest>() {
                Ok(mut req) => {
                    // L'identité de l'enveloppe prévaut pour les appels d'agents.
                    if ctx.intent.from.starts_with("agent:") {
                        req.agent_id = ctx.intent.from.clone();
                    }
                    if req.kind != kind {
                        let _ = ctx.respond_error(aos_ipc::msg::Status::BadRequest, "type de périphérique incompatible avec l'intent").await;
                        return;
                    }
                    req
                }
                Err(_) => { let _ = ctx.respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide").await; return; }
            };
            let cap = device_capture::capability_for(req.kind, req.mode);
            let actor = req.agent_id.clone();
            s.audit(AuditAppendRequest {
                trace_id: String::new(), actor: actor.clone(), action: "device.capture.request".into(), target: req.device_id.clone(),
                detail: serde_json::json!({ "kind": req.kind, "mode": req.mode, "capability": cap }),
            });
            let persistent = s.devices.lock().unwrap().has_persistent_cap(&req.agent_id, &req.device_id, req.kind, req.mode);
            let tier = s.trust.lock().unwrap().tier(&req.agent_id);
            if !persistent && tier == crate::trust::Tier::Low {
                s.audit(AuditAppendRequest { trace_id: String::new(), actor: actor.clone(), action: "device.capture.denied".into(), target: req.device_id.clone(), detail: serde_json::json!({ "reason": "trust_low" }) });
                let _ = ctx.respond_error(aos_ipc::msg::Status::PermissionDenied, "confiance faible: capture refusée").await;
                return;
            }
            let allowed = if persistent {
                true
            } else {
                let mut context = HashMap::new();
                context.insert("action.kind".into(), cap.clone());
                context.insert("device.id".into(), req.device_id.clone());
                context.insert("device.kind".into(), format!("{:?}", req.kind).to_lowercase());
                context.insert("capture.mode".into(), format!("{:?}", req.mode).to_lowercase());
                s.policy_gate(context, &actor, &cap, &req.device_id, "device-capture").await
            };
            if !allowed {
                let _ = ctx.respond_error(aos_ipc::msg::Status::PermissionDenied, "capture refusée ou confirmation expirée").await;
                return;
            }
            let result = s.devices.lock().unwrap().capture(&req, true);
            match result {
                Ok(response) => {
                    s.audit(AuditAppendRequest {
                        trace_id: response.capture_id.clone(), actor, action: "device.capture.opened".into(), target: req.device_id,
                        // Aucun chemin arbitraire ni octet média dans l'audit.
                        detail: serde_json::json!({ "capture_id": response.capture_id, "kind": req.kind, "mode": req.mode, "duration_ms": response.metadata.duration_ms, "size_bytes": response.metadata.size_bytes, "state": response.metadata.state }),
                    });
                    let _ = ctx.respond(aos_ipc::msg::Status::Ok, &response).await;
                }
                Err(e) => {
                    s.audit(AuditAppendRequest { trace_id: String::new(), actor, action: "device.capture.error".into(), target: req.device_id, detail: serde_json::json!({ "error": e.to_string() }) });
                    respond_error(ctx, e).await;
                }
            }
        }
    });
}

async fn respond_error(ctx: aos_ipc::IntentCtx, error: DeviceCaptureError) {
    let status = match error {
        DeviceCaptureError::UnsupportedPlatform => aos_ipc::msg::Status::NotFound,
        DeviceCaptureError::DeviceAbsent(_) => aos_ipc::msg::Status::NotFound,
        DeviceCaptureError::OsPermissionDenied => aos_ipc::msg::Status::PermissionDenied,
        DeviceCaptureError::DeviceBusy | DeviceCaptureError::QuotaExceeded(_) => {
            aos_ipc::msg::Status::PermissionDenied
        }
        DeviceCaptureError::CaptureNotFound(_)
        | DeviceCaptureError::InvalidRequest(_)
        | DeviceCaptureError::Backend(_)
        | DeviceCaptureError::Artifact(_) => aos_ipc::msg::Status::InternalError,
    };
    let message = error.to_string();
    let _ = ctx.respond_error(status, &message).await;
}
