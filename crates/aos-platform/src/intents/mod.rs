//! Intent registration split out of `aos-platformd` (P09.7).

pub mod audit;
pub mod device;
pub mod device_usb;
pub mod fs;
pub mod helpers;

use aos_ipc::BusService;
use crate::subsystem::PlatformSubsystem;
use std::sync::Arc;

pub fn register_audit_and_fs(svc: &mut BusService, sub: &Arc<PlatformSubsystem>) {
    audit::register(svc, sub.clone());
    device::register(svc, sub.clone());
    device_usb::register(svc, sub.clone());
    fs::register(svc, sub.clone());
}
