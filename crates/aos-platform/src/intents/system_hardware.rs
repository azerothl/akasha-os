//! Intent `system.hardware` — probe machine frais pour agents / salon.

use crate::subsystem::PlatformSubsystem;
use aos_ipc::BusService;
use aos_placement::probe_host_hardware;
use aos_proto::system_hardware::intents;
use aos_proto::{
    AuditAppendRequest, SystemHardwareRequest, SystemHardwareResponse,
};
use std::path::PathBuf;
use std::sync::Arc;

fn aos_home() -> PathBuf {
    std::env::var("AOS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

pub fn register(svc: &mut BusService, sub: Arc<PlatformSubsystem>) {
    let s = sub;
    svc.on(intents::HARDWARE, move |ctx| {
        let s = s.clone();
        async move {
            let req = match ctx.payload::<SystemHardwareRequest>() {
                Ok(r) => r,
                Err(_) => SystemHardwareRequest::default(),
            };
            let _ = req.refresh; // always fresh; reserved for future cache TTL
            let home = aos_home();
            let info = probe_host_hardware(&home);
            s.audit(AuditAppendRequest {
                trace_id: String::new(),
                actor: ctx.intent.from.clone(),
                action: intents::HARDWARE.into(),
                target: info.gpu_name.clone(),
                detail: serde_json::json!({
                    "tier": info.tier.as_str(),
                    "vram_mib": info.vram_mib,
                    "vram_free_mib": info.vram_free_mib,
                    "ram_mib": info.ram_mib,
                    "probed_at_unix_ms": info.probed_at_unix_ms,
                }),
            });
            let _ = ctx
                .respond(
                    aos_ipc::msg::Status::Ok,
                    &SystemHardwareResponse {
                        summary: info.agent_summary(),
                    },
                )
                .await;
        }
    });
}
