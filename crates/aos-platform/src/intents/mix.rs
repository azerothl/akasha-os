//! Mix Path A bus intents (#432).

use crate::subsystem::PlatformSubsystem;
use aos_ipc::BusService;
use aos_proto::mix::{
    intents as mix_intents, MixApplyRequest, MixIngestRequest, MixProposeRequest,
};
use aos_proto::AuditAppendRequest;
use std::sync::Arc;

pub fn register(svc: &mut BusService, sub: Arc<PlatformSubsystem>) {
    {
        let s = sub.clone();
        svc.on(mix_intents::CATALOG, move |ctx| {
            let s = s.clone();
            async move {
                let resp = s.mix.lock().unwrap().catalog().unwrap_or_else(|e| {
                    aos_proto::mix::MixCatalogResponse {
                        ok: false,
                        catalog_version: 0,
                        presets: vec![],
                        message: Some(e),
                    }
                });
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
            }
        });
    }
    {
        let s = sub.clone();
        svc.on(mix_intents::INGEST_STATE, move |ctx| {
            let s = s.clone();
            async move {
                let mut req = match ctx.payload::<MixIngestRequest>() {
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
                let resp = s.mix.lock().unwrap().ingest(&req);
                if resp.ok {
                    s.audit(AuditAppendRequest {
                        trace_id: req.trace_id.clone(),
                        actor: req.actor.clone(),
                        action: mix_intents::INGEST_STATE.into(),
                        target: resp.session_id.clone(),
                        detail: serde_json::json!({ "ok": true }),
                    });
                }
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
            }
        });
    }
    {
        let s = sub.clone();
        svc.on(mix_intents::PROPOSE, move |ctx| {
            let s = s.clone();
            async move {
                let mut req = match ctx.payload::<MixProposeRequest>() {
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
                let resp = s.mix.lock().unwrap().propose(&req);
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
            }
        });
    }
    {
        let s = sub.clone();
        svc.on(mix_intents::APPLY, move |ctx| {
            let s = s.clone();
            async move {
                let mut req = match ctx.payload::<MixApplyRequest>() {
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
                let has_cap = req.caps.iter().any(|c| {
                    c == aos_proto::mix::MIX_APPLY_CAP
                        || c == "mix.apply:*"
                        || c == "*"
                        || c.starts_with("mix.apply:")
                });
                let resp = s.mix.lock().unwrap().apply(&req, has_cap);
                if resp.ok {
                    s.audit(AuditAppendRequest {
                        trace_id: req.trace_id.clone(),
                        actor: req.actor.clone(),
                        action: mix_intents::APPLY.into(),
                        target: resp.session_id.clone(),
                        detail: serde_json::json!({
                            "applied": resp.applied.len(),
                            "daw": "outside_sandbox",
                        }),
                    });
                }
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
            }
        });
    }
    {
        let s = sub.clone();
        svc.on(mix_intents::UNDO, move |ctx| {
            let s = s.clone();
            async move {
                let mut req = match ctx.payload::<MixApplyRequest>() {
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
                let has_cap = req.caps.iter().any(|c| {
                    c == aos_proto::mix::MIX_APPLY_CAP || c == "*" || c.starts_with("mix.apply:")
                });
                let resp = s.mix.lock().unwrap().undo(&req, has_cap);
                if resp.ok {
                    s.audit(AuditAppendRequest {
                        trace_id: req.trace_id.clone(),
                        actor: req.actor.clone(),
                        action: mix_intents::UNDO.into(),
                        target: resp.session_id.clone(),
                        detail: serde_json::json!({ "undone": resp.applied.len() }),
                    });
                }
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
            }
        });
    }
}
