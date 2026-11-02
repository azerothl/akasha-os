//! Mutable state owned by the audit and capabilities panels.

use crate::ui_audit::DaemonRestart;
use aos_proto::{AuditEvent, CapInfo};

#[derive(Debug, Clone)]
pub(crate) struct ChatErrorRecord {
    pub(crate) at_ms: u64,
    pub(crate) session_id: Option<String>,
    pub(crate) code: String,
    pub(crate) cause: String,
    /// Bounded diagnostic only; local paths and suspected secrets are omitted.
    pub(crate) technical_detail: Option<String>,
}

pub(crate) fn diagnostic_error_detail(raw: &str) -> Option<String> {
    if raw.len() > 4096
        || raw.contains(['/', '\\'])
        || aos_platform::extract::looks_like_secret(raw)
    {
        return None;
    }
    let detail = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    (!detail.is_empty()).then(|| detail.chars().take(384).collect())
}

#[derive(Default)]
pub(crate) struct SecurityUiState {
    pub(crate) audit: Vec<AuditEvent>,
    pub(crate) caps: Vec<CapInfo>,
    pub(crate) caps_holder: String,
    pub(crate) device_permissions: Vec<aos_proto::DevicePermissionInfo>,
    pub(crate) device_active: Vec<aos_proto::DeviceActiveCapture>,
    /// S7.6 : filtres d'audit.
    pub(crate) audit_search: String,
    pub(crate) audit_problems_only: bool,
    /// Session chat ciblée (None = toutes).
    pub(crate) audit_session: Option<String>,
    /// Nombre d'événements demandés (0 = défaut 200).
    pub(crate) audit_last: usize,
    /// Ligne sélectionnée (seq) pour le détail.
    pub(crate) audit_selected: Option<u64>,
    /// Trace ciblée via "chaîne" (None = toutes).
    pub(crate) audit_trace: Option<String>,
    /// Résultat `audit.verify` (None = non vérifié cette session).
    pub(crate) audit_verified: Option<bool>,
    /// Redémarrages watchdog (daemon_restarts.log).
    pub(crate) audit_restarts: Vec<DaemonRestart>,
    /// Queues stderr des daemons (daemon.stderr.log).
    pub(crate) audit_logs: Vec<(String, Vec<String>)>,
    /// E23 runtime health snapshot (`health.snapshot`).
    pub(crate) health: Option<aos_proto::HealthSnapshot>,
    /// Previous canary_ok — detect ok→fail for status bar.
    pub(crate) health_canary_was_ok: Option<bool>,
    /// Recent chat errors (classified chrome) for Audit Health / Troubleshoot.
    pub(crate) recent_chat_errors: Vec<ChatErrorRecord>,
}

const MAX_RECENT_CHAT_ERRORS: usize = 20;

impl SecurityUiState {
    pub(crate) fn set_audit(&mut self, audit: Vec<AuditEvent>) {
        self.audit = audit;
    }

    /// Nombre d'événements demandés au journal (défaut 200).
    pub(crate) fn audit_last_or_default(&self) -> usize {
        if self.audit_last == 0 {
            200
        } else {
            self.audit_last
        }
    }

    pub(crate) fn set_caps(&mut self, holder: String, caps: Vec<CapInfo>) {
        self.caps_holder = holder;
        self.caps = caps;
    }

    pub(crate) fn select_holder(&mut self, holder: String) {
        self.caps_holder = holder;
    }

    pub(crate) fn set_device_permissions(
        &mut self,
        permissions: Vec<aos_proto::DevicePermissionInfo>,
    ) {
        self.device_permissions = permissions;
    }

    pub(crate) fn set_device_active(&mut self, active: Vec<aos_proto::DeviceActiveCapture>) {
        self.device_active = active;
    }

    pub(crate) fn record_chat_error(
        &mut self,
        session_id: Option<String>,
        code: &str,
        cause: String,
        technical_detail: Option<String>,
        at_ms: u64,
    ) {
        self.recent_chat_errors.push(ChatErrorRecord {
            at_ms,
            session_id,
            code: code.to_string(),
            cause,
            technical_detail,
        });
        while self.recent_chat_errors.len() > MAX_RECENT_CHAT_ERRORS {
            self.recent_chat_errors.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_detail_retains_bounded_technical_cause_without_secrets_or_paths() {
        assert_eq!(
            diagnostic_error_detail("provider returned HTTP 429").as_deref(),
            Some("provider returned HTTP 429")
        );
        assert!(diagnostic_error_detail("failed to open C:\\Users\\private\\file.gguf").is_none());
        assert!(diagnostic_error_detail("error /home/private/file").is_none());
        assert!(diagnostic_error_detail("Authorization: Bearer sensitive-token").is_none());
        assert_eq!(
            diagnostic_error_detail(&"failure ".repeat(100))
                .unwrap()
                .chars()
                .count(),
            384
        );
    }

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
