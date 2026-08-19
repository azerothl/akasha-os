//! `audit.query` / `audit.verify` (local journal).

use aos_ipc::BusService;
use aos_proto::*;
use crate::subsystem::PlatformSubsystem;
use std::sync::Arc;

pub fn register(svc: &mut BusService, sub: Arc<PlatformSubsystem>) {
    {
        let s = sub.clone();
        svc.on("audit.query", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<AuditQueryRequest>() {
                    Ok(req) => {
                        let events = s.audit.lock().unwrap().query(&req);
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &events).await;
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
        svc.on("audit.verify", move |ctx| {
            let s = s.clone();
            async move {
                let ok = s.audit.lock().unwrap().verify().is_ok();
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &ok).await;
            }
        });
    }
}
