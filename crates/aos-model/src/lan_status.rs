//! LAN cluster readiness surfaced to the UI (`model.cluster.nodes`).

/// Runtime counters and bind outcomes collected by `aos-modeld`.
#[derive(Debug, Clone, Default)]
pub struct LanRuntimeSnapshot {
    pub worker_ready: bool,
    pub worker_detail: String,
    pub discovery_active: bool,
    pub discovery_detail: String,
    pub discovery_tx: u64,
    pub discovery_rx: u64,
}

/// Stable worker states returned to the chrome (mapped to i18n in the UI).
pub const WORKER_STATE_DISABLED: &str = "disabled";
pub const WORKER_STATE_READY: &str = "ready";
pub const WORKER_STATE_SECRET_MISSING: &str = "secret_missing";
pub const WORKER_STATE_BIND_FAILED: &str = "bind_failed";
pub const WORKER_STATE_RESTART_REQUIRED: &str = "restart_required";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanClusterStatusFields {
    pub worker_state: String,
    pub worker_detail: String,
    pub discovery_active: bool,
    pub discovery_detail: String,
    pub discovery_tx: u64,
    pub discovery_rx: u64,
}

pub fn compute_lan_cluster_status(
    enabled: bool,
    session_key_available: bool,
    session_key_error: &str,
    snapshot: &LanRuntimeSnapshot,
) -> LanClusterStatusFields {
    if !enabled {
        return LanClusterStatusFields {
            worker_state: WORKER_STATE_DISABLED.into(),
            worker_detail: String::new(),
            discovery_active: false,
            discovery_detail: String::new(),
            discovery_tx: 0,
            discovery_rx: 0,
        };
    }

    let (worker_state, worker_detail) = if session_key_available {
        if snapshot.worker_ready {
            (
                WORKER_STATE_READY.into(),
                snapshot.worker_detail.clone(),
            )
        } else if snapshot.worker_detail.is_empty()
            || worker_detail_is_secret_race(&snapshot.worker_detail)
        {
            // Key is readable now but the listener never started (startup race
            // with secrets.get) — ask for a Preview relaunch / modeld restart.
            (WORKER_STATE_RESTART_REQUIRED.into(), String::new())
        } else {
            (
                WORKER_STATE_BIND_FAILED.into(),
                snapshot.worker_detail.clone(),
            )
        }
    } else {
        (
            WORKER_STATE_SECRET_MISSING.into(),
            session_key_error.to_string(),
        )
    };

    LanClusterStatusFields {
        worker_state,
        worker_detail,
        discovery_active: snapshot.discovery_active,
        discovery_detail: snapshot.discovery_detail.clone(),
        discovery_tx: snapshot.discovery_tx,
        discovery_rx: snapshot.discovery_rx,
    }
}

fn worker_detail_is_secret_race(detail: &str) -> bool {
    let lower = detail.to_ascii_lowercase();
    lower.contains("secrets.get")
        || lower.contains("notfound")
        || lower.contains("aucun service")
        || lower.contains("clé lan indisponible")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_cluster_reports_disabled_worker_state() {
        let status = compute_lan_cluster_status(
            false,
            false,
            "missing",
            &LanRuntimeSnapshot {
                discovery_active: true,
                discovery_tx: 3,
                ..LanRuntimeSnapshot::default()
            },
        );
        assert_eq!(status.worker_state, WORKER_STATE_DISABLED);
        assert!(!status.discovery_active);
    }

    #[test]
    fn missing_secret_surfaces_secret_missing_even_if_discovery_runs() {
        let status = compute_lan_cluster_status(
            true,
            false,
            "unknown secret: lan_cluster_session_key",
            &LanRuntimeSnapshot {
                discovery_active: true,
                discovery_tx: 12,
                discovery_rx: 1,
                ..LanRuntimeSnapshot::default()
            },
        );
        assert_eq!(status.worker_state, WORKER_STATE_SECRET_MISSING);
        assert!(status.discovery_active);
        assert_eq!(status.discovery_tx, 12);
        assert_eq!(status.discovery_rx, 1);
    }

    #[test]
    fn ready_worker_reports_ready_state() {
        let status = compute_lan_cluster_status(
            true,
            true,
            "",
            &LanRuntimeSnapshot {
                worker_ready: true,
                worker_detail: "0.0.0.0:9001".into(),
                discovery_active: true,
                discovery_tx: 4,
                discovery_rx: 2,
                ..LanRuntimeSnapshot::default()
            },
        );
        assert_eq!(status.worker_state, WORKER_STATE_READY);
        assert_eq!(status.worker_detail, "0.0.0.0:9001");
    }

    #[test]
    fn secret_present_without_listener_requires_restart() {
        let status = compute_lan_cluster_status(
            true,
            true,
            "",
            &LanRuntimeSnapshot::default(),
        );
        assert_eq!(status.worker_state, WORKER_STATE_RESTART_REQUIRED);
    }

    #[test]
    fn bind_failure_surfaces_detail() {
        let status = compute_lan_cluster_status(
            true,
            true,
            "",
            &LanRuntimeSnapshot {
                worker_detail: "address already in use".into(),
                ..LanRuntimeSnapshot::default()
            },
        );
        assert_eq!(status.worker_state, WORKER_STATE_BIND_FAILED);
        assert_eq!(status.worker_detail, "address already in use");
    }

    #[test]
    fn stale_secrets_get_race_becomes_restart_when_key_readable() {
        let status = compute_lan_cluster_status(
            true,
            true,
            "",
            &LanRuntimeSnapshot {
                worker_detail:
                    "clé LAN indisponible: statut NotFound: aucun service pour secrets.get"
                        .into(),
                ..LanRuntimeSnapshot::default()
            },
        );
        assert_eq!(status.worker_state, WORKER_STATE_RESTART_REQUIRED);
        assert!(status.worker_detail.is_empty());
    }
}
