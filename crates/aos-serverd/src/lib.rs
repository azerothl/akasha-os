//! `aos-serverd` — Preview server lifecycle owner (P21 / ADR 0012).
//!
//! **P21.1:** shared spawn / stop / health live in [`lifecycle`] and are used by
//! `aos-session` (desktop unchanged). The `aos-serverd` binary still does not
//! own a live tree by itself (that is P21.2).

mod lifecycle;

pub use lifecycle::{
    apply_daemon_env, auditd_command, bin_path, default_bus_addr, expected_boot_argv, healthcheck,
    healthcheck_bus, inference_mode, kill_by_name, log_daemon_restart, modeld_command,
    pick_modeld_bin, platformd_command, DaemonHandle, ProcessTree, SpawnOptions, HEALTH_PROBES,
};

use std::path::{Path, PathBuf};

/// Ordered Preview daemon names owned by `aos-serverd` (same order as
/// `aos-session` today). UI (`aos-ui-egui`) is **not** in this list.
pub const DAEMON_BOOT_ORDER: &[&str] = &[
    "aos-busd",
    "aos-capkd",
    "aos-auditd",
    "aos-modeld",
    "aos-platformd",
    "aos-agentd",
];

/// Stop order is the reverse of boot (workers killed first by the supervisor).
pub fn daemon_stop_order() -> impl Iterator<Item = &'static str> {
    DAEMON_BOOT_ORDER.iter().copied().rev()
}

/// Daemons that already have session watchdogs in 0.18 (baseline).
pub const WATCHDOG_BASELINE: &[&str] = &["aos-auditd", "aos-platformd", "aos-modeld"];

/// Additional watchdogs required for P21 MVP (at least agentd).
pub const WATCHDOG_P21_EXTRA: &[&str] = &["aos-agentd"];

/// How `aos-session` relates to a live `aos-serverd`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionHandoff {
    /// 0.18 behaviour: session spawns the tree itself (no serverd).
    LegacySpawn,
    /// Session starts serverd if needed, then attaches UI to the bus.
    BootstrapThenAttach,
    /// Serverd already owns the tree; session only launches egui.
    AttachUiOnly,
}

/// Local control-plane commands (ADR 0012). Framing lands in P21.3/P21.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlCommand {
    Status,
    Restart,
    Stop,
    /// Intake → existing `aos-agentd` paths (not implemented in scaffold).
    EnqueueAgent,
}

impl ControlCommand {
    pub fn as_str(self) -> &'static str {
        match self {
            ControlCommand::Status => "status",
            ControlCommand::Restart => "restart",
            ControlCommand::Stop => "stop",
            ControlCommand::EnqueueAgent => "enqueue-agent",
        }
    }
}

/// Control socket / pipe path under `AOS_HOME` (never a network bind in 0.19).
pub fn control_socket_path(aos_home: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        aos_home.join("var/run/aos-serverd.pipe")
    }
    #[cfg(not(windows))]
    {
        aos_home.join("var/run/aos-serverd.sock")
    }
}

/// Relative path written next to other Preview pid files.
pub fn serverd_pid_relpath() -> &'static str {
    "var/run/aos-serverd.pid"
}

/// Readiness for the `aos-serverd` binary (not the shared lib).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScaffoldStatus {
    pub lot: &'static str,
    /// Shared lib can spawn (session uses it); binary still status-only until P21.2.
    pub lib_spawns_daemons: bool,
    pub binary_owns_tree: bool,
    pub control_plane_live: bool,
}

pub fn scaffold_status() -> ScaffoldStatus {
    ScaffoldStatus {
        lot: "P21.1",
        lib_spawns_daemons: true,
        binary_owns_tree: false,
        control_plane_live: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn boot_order_matches_session_contract() {
        assert_eq!(
            DAEMON_BOOT_ORDER,
            &[
                "aos-busd",
                "aos-capkd",
                "aos-auditd",
                "aos-modeld",
                "aos-platformd",
                "aos-agentd",
            ]
        );
        assert!(!DAEMON_BOOT_ORDER.contains(&"aos-ui-egui"));
    }

    #[test]
    fn stop_order_is_reverse_boot() {
        let stop: Vec<_> = daemon_stop_order().collect();
        assert_eq!(stop.first().copied(), Some("aos-agentd"));
        assert_eq!(stop.last().copied(), Some("aos-busd"));
    }

    #[test]
    fn control_socket_under_aos_home_run() {
        let home = PathBuf::from("/tmp/aos-home-test");
        let p = control_socket_path(&home);
        let s = p.to_string_lossy();
        assert!(s.contains("var/run"));
        assert!(s.contains("aos-serverd"));
    }

    #[test]
    fn p21_1_lib_spawns_binary_does_not() {
        let st = scaffold_status();
        assert_eq!(st.lot, "P21.1");
        assert!(st.lib_spawns_daemons);
        assert!(!st.binary_owns_tree);
        assert!(!st.control_plane_live);
    }

    #[test]
    fn control_command_names_are_stable() {
        assert_eq!(ControlCommand::Status.as_str(), "status");
        assert_eq!(ControlCommand::EnqueueAgent.as_str(), "enqueue-agent");
    }
}
