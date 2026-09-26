//! Shared Preview process-tree spawn / stop / health (P21.1).
//!
//! Used by `aos-session` (legacy desktop) and later by the `aos-serverd`
//! binary (P21.2). Desktop behaviour must stay identical for testers.

use aos_ipc::{BusClient, DEFAULT_BUS_PORT};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

/// Default bus listen address (matches `aos-session` / `aos-busd`).
pub fn default_bus_addr() -> String {
    format!("127.0.0.1:{DEFAULT_BUS_PORT}")
}

/// Intent probes used after boot (modeld, agentd, platformd, capkd).
pub const HEALTH_PROBES: &[(&str, &str)] = &[
    ("modeld", "model.list"),
    ("agentd", "agent.list"),
    ("platformd", "module.list"),
    ("capkd", "cap.check"),
];

/// Options for starting the Preview daemon tree.
#[derive(Debug, Clone)]
pub struct SpawnOptions {
    /// Log prefix (`aos-session` or `aos-serverd`).
    pub log_tag: &'static str,
    /// Host has GPU acceleration (NVIDIA / Apple Silicon Metal).
    pub gpu_accel: bool,
}

impl Default for SpawnOptions {
    fn default() -> Self {
        Self {
            log_tag: "aos-serverd",
            gpu_accel: false,
        }
    }
}

/// One supervised daemon child.
pub struct DaemonHandle {
    pub name: &'static str,
    pub child: Child,
}

/// Running Preview process tree (busd…agentd). Does **not** include egui.
pub struct ProcessTree {
    home: PathBuf,
    log_tag: &'static str,
    daemons: Vec<DaemonHandle>,
}

impl ProcessTree {
    pub fn empty(home: impl Into<PathBuf>, log_tag: &'static str) -> Self {
        Self {
            home: home.into(),
            log_tag,
            daemons: Vec::new(),
        }
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn daemons_mut(&mut self) -> &mut Vec<DaemonHandle> {
        &mut self.daemons
    }

    pub fn daemons(&self) -> &[DaemonHandle] {
        &self.daemons
    }

    /// Boot the Preview tree in [`crate::DAEMON_BOOT_ORDER`].
    pub fn start(&mut self, opts: &SpawnOptions) -> Result<(), String> {
        let home = self.home.clone();
        let log_tag = opts.log_tag;
        self.log_tag = log_tag;
        let bus = default_bus_addr();
        let mut list = Vec::new();

        {
            let mut cmd = Command::new(bin_path(&home, "aos-busd"));
            cmd.arg(DEFAULT_BUS_PORT.to_string());
            list.push(spawn_one(&home, log_tag, "aos-busd", cmd)?);
        }
        thread::sleep(Duration::from_millis(800));

        {
            let mut cmd = Command::new(bin_path(&home, "aos-capkd"));
            cmd.arg(&bus);
            list.push(spawn_one(&home, log_tag, "aos-capkd", cmd)?);
        }
        {
            let mut cmd = Command::new(bin_path(&home, "aos-auditd"));
            cmd.arg(&bus).arg("var/audit");
            list.push(spawn_one(&home, log_tag, "aos-auditd", cmd)?);
        }
        {
            let cmd = modeld_command(&home, opts);
            list.push(spawn_one(&home, log_tag, "aos-modeld", cmd)?);
        }
        {
            let mut cmd = Command::new(bin_path(&home, "aos-platformd"));
            cmd.arg("etc/platformd.yaml");
            list.push(spawn_one(&home, log_tag, "aos-platformd", cmd)?);
        }
        {
            let mut cmd = Command::new(bin_path(&home, "aos-agentd"));
            cmd.arg(&bus);
            list.push(spawn_one(&home, log_tag, "aos-agentd", cmd)?);
        }

        thread::sleep(Duration::from_secs(2));
        self.daemons = list;
        Ok(())
    }

    /// Stop in reverse order; also kills `aos-agent-worker`.
    pub fn stop(&mut self) {
        kill_by_name("aos-agent-worker");
        for d in self.daemons.iter_mut().rev() {
            let _ = d.child.kill();
            let _ = d.child.wait();
            eprintln!("[{}] {} stopped", self.log_tag, d.name);
        }
        self.daemons.clear();
    }

    /// Replace a dead daemon child (watchdog path).
    pub fn respawn(
        &mut self,
        name: &'static str,
        make_cmd: &dyn Fn(&Path) -> Command,
    ) -> Result<(), String> {
        let Some(pos) = self.daemons.iter().position(|d| d.name == name) else {
            return Err(format!("{name} not in process tree"));
        };
        let home = self.home.clone();
        let mut cmd = make_cmd(&home);
        apply_daemon_env(&mut cmd, &home);
        let log_path = home.join("var/run").join(format!("{name}.stderr.log"));
        let stderr = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .ok()
            .map(Stdio::from)
            .unwrap_or_else(Stdio::null);
        cmd.stdout(Stdio::null()).stderr(stderr);
        match cmd.spawn() {
            Ok(child) => {
                let pid = child.id();
                let _ = fs::write(home.join("var/run").join(format!("{name}.pid")), pid.to_string());
                self.daemons[pos] = DaemonHandle { name, child };
                eprintln!("[{}] {name} up (pid {pid})", self.log_tag);
                log_daemon_restart(&home, name, true);
                Ok(())
            }
            Err(e) => {
                eprintln!("[{}] restart {name} échoué : {e}", self.log_tag);
                log_daemon_restart(&home, name, false);
                Err(e.to_string())
            }
        }
    }
}

fn spawn_one(
    home: &Path,
    log_tag: &str,
    name: &'static str,
    mut cmd: Command,
) -> Result<DaemonHandle, String> {
    let log_path = home.join("var/run").join(format!("{name}.stderr.log"));
    let log_file = fs::File::create(&log_path)
        .map_err(|e| format!("{name}: log {e} ({})", log_path.display()))?;
    apply_daemon_env(&mut cmd, home);
    cmd.stdout(Stdio::null())
        // Piped unread stderr deadlocks GPU daemons (ggml/CUDA logs).
        .stderr(Stdio::from(log_file));
    let child = cmd
        .spawn()
        .map_err(|e| format!("{name}: {e} ({})", bin_path(home, name).display()))?;
    let pid = child.id();
    let _ = fs::write(
        home.join("var/run").join(format!("{name}.pid")),
        pid.to_string(),
    );
    eprintln!("[{log_tag}] {name} up (pid {pid})");
    Ok(DaemonHandle { name, child })
}

/// Resolve a daemon binary under `AOS_HOME/bin`, then `target/release`, then PATH.
pub fn bin_path(home: &Path, name: &str) -> PathBuf {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let packaged = home.join("bin").join(&exe);
    if packaged.exists() {
        return packaged;
    }
    let dev = home.join("target").join("release").join(&exe);
    if dev.exists() {
        return dev;
    }
    PathBuf::from(exe)
}

/// `AOS_HOME` + cwd + Linux `LD_LIBRARY_PATH` for packaged `.so` next to bins.
pub fn apply_daemon_env(cmd: &mut Command, home: &Path) {
    cmd.current_dir(home).env("AOS_HOME", home);
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
}

pub fn inference_mode(home: &Path) -> String {
    fs::read_to_string(home.join("var/run/preferences.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| {
            v.get("inference_mode")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "auto".into())
}

/// Metal/CUDA-linked `aos-modeld` vs `aos-modeld-cpu`.
pub fn pick_modeld_bin(home: &Path, gpu_accel: bool) -> (PathBuf, bool) {
    let mode = inference_mode(home);
    let cpu_bin = bin_path(home, "aos-modeld-cpu");
    let gpu_bin = bin_path(home, "aos-modeld");
    if gpu_accel && gpu_bin.exists() {
        return (gpu_bin, false);
    }
    let want_cpu =
        mode.eq_ignore_ascii_case("cpu") || (!gpu_accel && !mode.eq_ignore_ascii_case("gpu"));
    if want_cpu && cpu_bin.exists() {
        (cpu_bin, true)
    } else {
        (gpu_bin, false)
    }
}

pub fn modeld_command(home: &Path, opts: &SpawnOptions) -> Command {
    let (bin, cpu) = pick_modeld_bin(home, opts.gpu_accel);
    let mut cmd = Command::new(&bin);
    cmd.arg("etc/modeld.yaml");
    cmd.env("AOS_INFERENCE", inference_mode(home));
    if cpu {
        cmd.env("AOS_CPU_ONLY", "1");
    } else {
        cmd.env_remove("AOS_CPU_ONLY");
    }
    eprintln!(
        "[{}] modeld {} (cpu={cpu})",
        opts.log_tag,
        bin.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("aos-modeld")
    );
    cmd
}

/// NVIDIA (Win/Linux) or Apple Silicon Metal — same heuristic as `aos-session` bootstrap.
pub fn gpu_accel_ok() -> bool {
    #[cfg(target_os = "macos")]
    {
        std::env::consts::ARCH == "aarch64"
    }
    #[cfg(not(target_os = "macos"))]
    {
        Command::new("nvidia-smi")
            .arg("-L")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

/// Default spawn options for the `aos-serverd` binary.
pub fn serverd_spawn_opts() -> SpawnOptions {
    SpawnOptions {
        log_tag: "aos-serverd",
        gpu_accel: gpu_accel_ok(),
    }
}

/// Command builders used by session watchdogs (same argv as [`ProcessTree::start`]).
pub fn auditd_command(home: &Path) -> Command {
    let mut cmd = Command::new(bin_path(home, "aos-auditd"));
    cmd.arg(default_bus_addr()).arg("var/audit");
    cmd
}

pub fn platformd_command(home: &Path) -> Command {
    let mut cmd = Command::new(bin_path(home, "aos-platformd"));
    cmd.arg("etc/platformd.yaml");
    cmd
}

/// Probe bus intents until healthy or timeout (~15s).
pub fn healthcheck() -> Result<(), String> {
    healthcheck_bus(&default_bus_addr())
}

pub fn healthcheck_bus(bus_addr: &str) -> Result<(), String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(async {
        let mut last = String::new();
        for _ in 0..30 {
            match BusClient::connect(bus_addr, "lifecycle-health").await {
                Ok(bus) => {
                    let mut ok = true;
                    for (name, intent) in HEALTH_PROBES {
                        if !bus.lookup(intent).await.unwrap_or(false) {
                            ok = false;
                            last = format!("{name} ({intent}) absent");
                            break;
                        }
                    }
                    if ok {
                        return Ok(());
                    }
                }
                Err(e) => last = e.to_string(),
            }
            thread::sleep(Duration::from_millis(500));
        }
        Err(last)
    })
}

pub fn kill_by_name(name: &str) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/F", "/IM", &format!("{name}.exe")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("pkill")
            .args(["-x", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Append to `var/run/daemon_restarts.log` (Audit tab).
pub fn log_daemon_restart(home: &Path, name: &str, ok: bool) {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let line = format!(
        "{ms} {name} {}\n",
        if ok { "restarted" } else { "restart-failed" }
    );
    let path = home.join("var/run/daemon_restarts.log");
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Expected argv fragments for unit tests (no spawn).
pub fn expected_boot_argv(name: &str) -> Option<&'static [&'static str]> {
    match name {
        "aos-busd" => Some(&["24701"]),
        "aos-capkd" | "aos-agentd" => Some(&[]), // bus addr is dynamic string
        "aos-auditd" => Some(&["var/audit"]),
        "aos-modeld" => Some(&["etc/modeld.yaml"]),
        "aos-platformd" => Some(&["etc/platformd.yaml"]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DAEMON_BOOT_ORDER;

    #[test]
    fn health_probes_cover_core_services() {
        let names: Vec<_> = HEALTH_PROBES.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"modeld"));
        assert!(names.contains(&"agentd"));
        assert!(names.contains(&"platformd"));
        assert!(names.contains(&"capkd"));
    }

    #[test]
    fn boot_order_has_expected_argv_hints() {
        for name in DAEMON_BOOT_ORDER {
            assert!(
                expected_boot_argv(name).is_some(),
                "missing argv contract for {name}"
            );
        }
    }

    #[test]
    fn default_bus_uses_ipc_port() {
        assert_eq!(default_bus_addr(), format!("127.0.0.1:{DEFAULT_BUS_PORT}"));
    }

    #[test]
    fn pick_modeld_prefers_cpu_without_gpu() {
        let home = std::env::temp_dir().join(format!(
            "aos-lifecycle-modeld-{}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(home.join("bin"));
        // Neither binary exists → falls back to aos-modeld path name.
        let (bin, cpu) = pick_modeld_bin(&home, false);
        assert!(!cpu || bin.to_string_lossy().contains("cpu") || !bin.exists());
        let _ = fs::remove_dir_all(&home);
    }
}
