//! Mutable state owned by the audit and capabilities panels.

use aos_proto::{AuditEvent, CapInfo};

#[derive(Default)]
pub(crate) struct SecurityUiState {
    pub(crate) audit: Vec<AuditEvent>,
    pub(crate) caps: Vec<CapInfo>,
    pub(crate) caps_holder: String,
    pub(crate) device_permissions: Vec<aos_proto::DevicePermissionInfo>,
    pub(crate) device_active: Vec<aos_proto::DeviceActiveCapture>,
}

impl SecurityUiState {
    pub(crate) fn set_audit(&mut self, audit: Vec<AuditEvent>) {
        self.audit = audit;
    }

    pub(crate) fn set_caps(&mut self, holder: String, caps: Vec<CapInfo>) {
        self.caps_holder = holder;
        self.caps = caps;
    }

    pub(crate) fn select_holder(&mut self, holder: String) {
        self.caps_holder = holder;
    }

    pub(crate) fn set_device_permissions(&mut self, permissions: Vec<aos_proto::DevicePermissionInfo>) {
        self.device_permissions = permissions;
    }

    pub(crate) fn set_device_active(&mut self, active: Vec<aos_proto::DeviceActiveCapture>) {
        self.device_active = active;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_caps_keeps_holder_and_payload_together() {
        let mut state = SecurityUiState::default();
        state.set_caps(
            "agent:a1".into(),
            vec![CapInfo {
                cap_id: 7,
                holder: "agent:a1".into(),
                object: "notes".into(),
                rights: vec!["read".into()],
            }],
        );
        assert_eq!(state.caps_holder, "agent:a1");
        assert_eq!(state.caps.len(), 1);
        assert_eq!(state.caps[0].cap_id, 7);
    }
}
