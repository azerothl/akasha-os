//! `aos-serverd` — Preview server lifecycle owner (P21 / ADR 0012).
//!
//! **P21.1:** shared spawn / stop / health in [`lifecycle`].
//! **P21.2:** headless `serve` owns the process tree.
//! **P21.3:** extended watchdogs + local `status` / `restart` / `stop` API.
//! **P21.4:** agent job intake → `aos-agentd` (`enqueue-agent` / `server.job.*`).

mod control;
mod intake;
mod lifecycle;

pub use control::{
    control_endpoint_present, send_control, serve_control, ControlRequest, ControlResponse,
    ControlState, DaemonStatus, TreeStatus,
};
pub use intake::{
    enqueue, get_job, jobs_store_relpath, list_jobs, validate_enqueue, EnqueueParams, IntakeMode,
    JobRecord,
};
pub use lifecycle::{
    agentd_command, apply_daemon_env, auditd_command, bin_path, busd_command, capkd_command,
    default_bus_addr, expected_boot_argv, gpu_accel_ok, healthcheck, healthcheck_bus,
    inference_mode, kill_by_name, log_control_audit, log_daemon_restart, modeld_command,
    pick_modeld_bin, platformd_command, serverd_spawn_opts, DaemonHandle, ProcessTree, SpawnOptions,
    HEALTH_PROBES,
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

/// Soft watchdogs: individual respawn (agentd + baseline).
pub const WATCHDOG_SOFT: &[&str] = &[
    "aos-auditd",
    "aos-platformd",
    "aos-modeld",
    "aos-agentd",
];

/// Hard watchdogs: death triggers ordered tree restart with backoff (P21.3).
pub const WATCHDOG_HARD: &[&str] = &["aos-busd", "aos-capkd"];

/// Additional watchdogs required for P21 MVP (at least agentd).
pub const WATCHDOG_P21_EXTRA: &[&str] = &["aos-agentd", "aos-busd", "aos-capkd"];

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

/// Local control-plane commands (ADR 0012).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlCommand {
    Status,
    Restart,
    Stop,
    /// Intake → existing `aos-agentd` paths (P21.4).
    EnqueueAgent,
    JobList,
    JobStatus,
}

impl ControlCommand {
    pub fn as_str(self) -> &'static str {
        match self {
            ControlCommand::Status => "status",
            ControlCommand::Restart => "restart",
            ControlCommand::Stop => "stop",
            ControlCommand::EnqueueAgent => "enqueue-agent",
            ControlCommand::JobList => "job-list",
            ControlCommand::JobStatus => "job-status",
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

/// Readiness for the `aos-serverd` binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScaffoldStatus {
    pub lot: &'static str,
    pub lib_spawns_daemons: bool,
    /// Binary can own the headless tree via `serve` / `--headless` (P21.2).
    pub binary_owns_tree: bool,
    pub control_plane_live: bool,
    /// Agent intake façade live (P21.4).
    pub agent_intake_live: bool,
}

pub fn scaffold_status() -> ScaffoldStatus {
    ScaffoldStatus {
        lot: "P21.4",
        lib_spawns_daemons: true,
        binary_owns_tree: true,
        control_plane_live: true,
        agent_intake_live: true,
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
    fn p21_4_intake_ready() {
        let st = scaffold_status();
        assert_eq!(st.lot, "P21.4");
        assert!(st.lib_spawns_daemons);
        assert!(st.binary_owns_tree);
        assert!(st.control_plane_live);
        assert!(st.agent_intake_live);
    }

    #[test]
    fn hard_watchdogs_cover_bus_and_cap() {
        assert!(WATCHDOG_HARD.contains(&"aos-busd"));
        assert!(WATCHDOG_HARD.contains(&"aos-capkd"));
        assert!(WATCHDOG_SOFT.contains(&"aos-agentd"));
    }

    #[test]
    fn control_command_names_are_stable() {
        assert_eq!(ControlCommand::Status.as_str(), "status");
        assert_eq!(ControlCommand::EnqueueAgent.as_str(), "enqueue-agent");
        assert_eq!(ControlCommand::JobList.as_str(), "job-list");
        assert_eq!(ControlCommand::JobStatus.as_str(), "job-status");
    }
}
