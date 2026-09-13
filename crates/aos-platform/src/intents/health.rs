//! `health.snapshot` / `health.canary` (E23 runtime health plane).

use crate::subsystem::PlatformSubsystem;
use aos_ipc::BusService;
use aos_proto::HealthSnapshot;
use std::sync::Arc;

pub fn register(svc: &mut BusService, sub: Arc<PlatformSubsystem>) {
    {
        let s = sub.clone();
        svc.on("health.snapshot", move |ctx| {
            let s = s.clone();
            async move {
                let snap = s
                    .health()
                    .map(|h| h.snapshot())
                    .unwrap_or_default();
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &snap).await;
            }
        });
    }
    {
        let s = sub.clone();
        svc.on("health.canary", move |ctx| {
            let s = s.clone();
            async move {
                let Some(rt) = s.health() else {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::NotFound,
                            "health runtime not started",
                        )
                        .await;
                    return;
                };
                match crate::health::run_canary(&s, &rt).await {
                    Ok(snap) => {
                        rt.publish(snap.clone());
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &snap).await;
                    }
                    Err(e) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::InternalError, &e)
                            .await;
                    }
                }
            }
        });
    }
}

#[allow(dead_code)]
fn _type_check(snap: HealthSnapshot) -> HealthSnapshot {
    snap
}
