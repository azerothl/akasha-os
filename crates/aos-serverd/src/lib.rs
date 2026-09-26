//! `aos-serverd` — Preview server lifecycle owner (P21 / ADR 0012).
//!
//! **P21.1:** shared spawn / stop / health in [`lifecycle`].
//! **P21.2:** headless `serve` owns the process tree.
//! **P21.3:** extended watchdogs + local `status` / `restart` / `stop` API.
//! **P21.4:** agent job intake → `aos-agentd` (`enqueue-agent` / `server.job.*`).
//! **P21.6:** opt-in `aos-mcpd` / `aos-bridged` + session attach handoff.

mod config;
mod control;
mod intake;
mod lifecycle;

pub use config::{load_serverd_config, serverd_config_example, ServerdConfig};
pub use control::{
    control_endpoint_present, send_control, serve_control, ControlRequest, ControlResponse,
    ControlState, DaemonStatus, TreeStatus,
};
pub use intake::{
    enqueue, get_job, jobs_store_relpath, list_jobs, validate_enqueue, EnqueueParams, IntakeMode,
    JobRecord,
};
pub use lifecycle::{
    agentd_command, apply_daemon_env, auditd_command, bin_path, bridged_command, busd_command,
    capkd_command, default_bus_addr, expected_boot_argv, gpu_accel_ok, healthcheck, healthcheck_bus,
    inference_mode, kill_by_name, log_control_audit, log_daemon_restart, mcpd_command,
    modeld_command, pick_modeld_bin, platformd_command, serverd_spawn_opts, serverd_spawn_opts_for,
    DaemonHandle, ProcessTree, SpawnOptions, HEALTH_PROBES,
};

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

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

/// Optional daemons started only when `etc/serverd.yaml` opts in (P21.6).
pub const OPTIONAL_DAEMONS: &[&str] = &["aos-bridged", "aos-mcpd"];

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

impl SessionHandoff {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionHandoff::LegacySpawn => "legacy-spawn",
            SessionHandoff::BootstrapThenAttach => "bootstrap-then-attach",
            SessionHandoff::AttachUiOnly => "attach-ui-only",
        }
    }
}

/// Resolve session↔serverd relationship (P21.6).
///
/// Override with `AOS_SESSION_HANDOFF=legacy|attach|bootstrap`.
/// Otherwise: live control endpoint → attach; config/env bootstrap →
/// bootstrap-then-attach; else legacy spawn.
pub fn resolve_handoff(home: &Path) -> SessionHandoff {
    if let Ok(raw) = std::env::var("AOS_SESSION_HANDOFF") {
        match raw.trim().to_ascii_lowercase().as_str() {
            "legacy" | "legacy-spawn" => return SessionHandoff::LegacySpawn,
            "attach" | "attach-ui-only" => return SessionHandoff::AttachUiOnly,
            "bootstrap" | "bootstrap-then-attach" => {
                return SessionHandoff::BootstrapThenAttach
            }
            _ => {}
        }
    }
    if control_endpoint_present(home) {
        // Prefer attach when serverd already owns the tree.
        return SessionHandoff::AttachUiOnly;
    }
    let cfg = load_serverd_config(home);
    if cfg.session_bootstrap_serverd {
        return SessionHandoff::BootstrapThenAttach;
    }
    SessionHandoff::LegacySpawn
}

/// Start `aos-serverd serve` in the background and wait until the control
/// endpoint (and bus health) are ready. Used by BootstrapThenAttach.
pub fn ensure_serverd_running(home: &Path) -> Result<(), String> {
    if control_endpoint_present(home) {
        return Ok(());
    }
    let bin = bin_path(home, "aos-serverd");
    if !bin.exists() {
        return Err(format!(
            "aos-serverd missing at {} — cannot bootstrap",
            bin.display()
        ));
    }
    let run_dir = home.join("var/run");
    fs_create_run(home)?;
    let log_path = run_dir.join("aos-serverd.bootstrap.stderr.log");
    let log_file = std::fs::File::create(&log_path)
        .map_err(|e| format!("bootstrap log: {e}"))?;
    let mut cmd = Command::new(&bin);
    cmd.arg("serve")
        .current_dir(home)
        .env("AOS_HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log_file));
    #[cfg(target_os = "linux")]
    {
        let bin_dir = home.join("bin");
        let mut ld = bin_dir.to_string_lossy().to_string();
        if let Ok(prev) = std::env::var("LD_LIBRARY_PATH") {
            if !prev.is_empty() {
                ld = format!("{ld}:{prev}");
            }
        }
        cmd.env("LD_LIBRARY_PATH", ld);
    }
    let child = cmd
        .spawn()
        .map_err(|e| format!("spawn aos-serverd serve: {e}"))?;
    eprintln!(
        "[aos-serverd] bootstrap serve started (pid {})",
        child.id()
    );
    // Detach: leak Child so we don't kill on drop. Serverd owns its lifetime.
    std::mem::forget(child);

    for i in 0..60 {
        if control_endpoint_present(home) && healthcheck().is_ok() {
            eprintln!("[aos-serverd] bootstrap ready ({i} polls)");
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }
    Err(format!(
        "aos-serverd did not become ready (see {})",
        log_path.display()
    ))
}

fn fs_create_run(home: &Path) -> Result<(), String> {
    std::fs::create_dir_all(home.join("var/run")).map_err(|e| format!("var/run: {e}"))
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
    /// Opt-in mcpd/bridged + session attach (P21.6).
    pub optional_daemons_and_attach: bool,
}

pub fn scaffold_status() -> ScaffoldStatus {
    ScaffoldStatus {
        lot: "P21.6",
        lib_spawns_daemons: true,
        binary_owns_tree: true,
        control_plane_live: true,
        agent_intake_live: true,
        optional_daemons_and_attach: true,
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
        assert!(!DAEMON_BOOT_ORDER.contains(&"aos-mcpd"));
        assert!(!DAEMON_BOOT_ORDER.contains(&"aos-bridged"));
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
    fn p21_6_ready() {
        let st = scaffold_status();
        assert_eq!(st.lot, "P21.6");
        assert!(st.lib_spawns_daemons);
        assert!(st.binary_owns_tree);
        assert!(st.control_plane_live);
        assert!(st.agent_intake_live);
        assert!(st.optional_daemons_and_attach);
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

    #[test]
    fn handoff_defaults_to_legacy_without_endpoint() {
        let home = PathBuf::from("/tmp/aos-handoff-no-serverd-xyz");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("var/run")).unwrap();
        // Clear override if present in test process.
        std::env::remove_var("AOS_SESSION_HANDOFF");
        std::env::remove_var("AOS_SESSION_BOOTSTRAP_SERVERD");
        assert_eq!(resolve_handoff(&home), SessionHandoff::LegacySpawn);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn handoff_env_override_attach() {
        std::env::set_var("AOS_SESSION_HANDOFF", "attach");
        let home = PathBuf::from("/tmp/aos-handoff-env-xyz");
        assert_eq!(resolve_handoff(&home), SessionHandoff::AttachUiOnly);
        std::env::remove_var("AOS_SESSION_HANDOFF");
    }

    #[test]
    fn optional_daemon_names_documented() {
        assert!(OPTIONAL_DAEMONS.contains(&"aos-bridged"));
        assert!(OPTIONAL_DAEMONS.contains(&"aos-mcpd"));
    }
}
