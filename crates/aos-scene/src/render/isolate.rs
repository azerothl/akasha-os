//! Isolated Blender child process spawn (harness-like, no free shell).
//!
//! Model:
//! - Fixed argv only — never `shell.exec` / never libre argv from modules
//! - Work directory contains only the export JSON + expected output path
//! - Optional Linux `bwrap` FS+net deny when available
//! - Clear env of ambient secrets; keep a minimal PATH for the binary
//!
//! Gaps vs P0-B spike (documented, fail-soft): Windows AppContainer and
//! macOS sandbox-exec are **not** wired yet. Without `bwrap`, Linux still
//! uses a dedicated workdir + fixed argv but inherits ambient FS/net.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// Default wall-clock timeout for a beauty render child.
pub const DEFAULT_BLENDER_TIMEOUT_SECS: u64 = 120;

/// How the host resolves the Blender executable / mock path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlenderRunMode {
    /// Prefer real Blender when pack + binary found; mock when pack present
    /// without binary; **fail-closed** when the opt-in Renderer Pack is absent.
    Auto,
    /// Always produce mock golden PNG — never spawn Blender (CI path).
    Mock,
    /// Require pack + real Blender binary; error if missing.
    Require,
}

impl BlenderRunMode {
    pub fn from_env() -> Self {
        match std::env::var("AOS_BLENDER_MODE")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "mock" | "stub" => Self::Mock,
            "require" | "real" => Self::Require,
            _ => Self::Auto,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Mock => "mock",
            Self::Require => "require",
        }
    }
}

/// Opt-in Illustration Renderer Pack probe (DeclUI `render.pack.status`).
#[derive(Debug, Clone)]
pub struct BlenderPackStatus {
    pub pack_root: Option<PathBuf>,
    pub blender_bin: Option<PathBuf>,
    pub adapter_py: Option<PathBuf>,
    pub mode: BlenderRunMode,
    /// Pack root + `adapters/akasha_beauty.py` present (mock path viable in Auto).
    pub ready_for_mock: bool,
    /// Pack + adapter + Blender binary present (real spawn viable).
    pub ready_for_spawn: bool,
}

impl BlenderPackStatus {
    pub fn summary_en(&self) -> String {
        match (
            &self.pack_root,
            self.ready_for_mock,
            self.ready_for_spawn,
        ) {
            (None, _, _) => {
                "Blender pack: missing (opt-in Renderer Pack; beauty fail-closed — install pack or AOS_BLENDER_MODE=mock)".into()
            }
            (Some(_), true, true) => format!(
                "Blender pack: ready (mode={}, binary+adapter)",
                self.mode.as_str()
            ),
            (Some(_), true, false) => format!(
                "Blender pack: mock-ready (mode={}, adapter; no Blender binary)",
                self.mode.as_str()
            ),
            (Some(_), false, true) => format!(
                "Blender pack: binary ready (mode={}; adapter missing)",
                self.mode.as_str()
            ),
            (Some(_), false, false) => format!(
                "Blender pack: present but incomplete (mode={})",
                self.mode.as_str()
            ),
        }
    }

    pub fn summary_fr(&self) -> String {
        match (
            &self.pack_root,
            self.ready_for_mock,
            self.ready_for_spawn,
        ) {
            (None, _, _) => {
                "Pack Blender : absent (Renderer Pack opt-in ; beauté refusée — installer le pack ou AOS_BLENDER_MODE=mock)".into()
            }
            (Some(_), true, true) => format!(
                "Pack Blender : prêt (mode={}, binaire+adaptateur)",
                self.mode.as_str()
            ),
            (Some(_), true, false) => format!(
                "Pack Blender : mock prêt (mode={}, adaptateur ; pas de binaire Blender)",
                self.mode.as_str()
            ),
            (Some(_), false, true) => format!(
                "Pack Blender : binaire prêt (mode={} ; adaptateur manquant)",
                self.mode.as_str()
            ),
            (Some(_), false, false) => format!(
                "Pack Blender : présent mais incomplet (mode={})",
                self.mode.as_str()
            ),
        }
    }
}

/// Probe Renderer Pack install / enable status for DeclUI and Auto fail-closed.
pub fn probe_pack_status() -> BlenderPackStatus {
    let mode = BlenderRunMode::from_env();
    let pack_root = resolve_pack_root();
    let adapter_py = pack_root.as_ref().and_then(|p| resolve_adapter_py(p));
    let blender_bin = resolve_blender_bin(pack_root.as_deref());
    let ready_for_mock = pack_root.is_some() && adapter_py.is_some();
    let ready_for_spawn = ready_for_mock && blender_bin.is_some();
    BlenderPackStatus {
        pack_root,
        blender_bin,
        adapter_py,
        mode,
        ready_for_mock,
        ready_for_spawn,
    }
}

#[derive(Debug, Clone)]
pub struct BlenderSpawnPlan {
    pub blender_bin: PathBuf,
    pub adapter_py: PathBuf,
    pub work_dir: PathBuf,
    /// Expected scene JSON path inside work_dir (documentation / callers).
    #[allow(dead_code)]
    pub scene_json: PathBuf,
    /// Expected beauty PNG path inside work_dir (documentation / callers).
    #[allow(dead_code)]
    pub output_png: PathBuf,
    pub use_bwrap: bool,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct BlenderSpawnResult {
    pub exit_code: i32,
    #[allow(dead_code)]
    pub stdout_tail: String,
    pub stderr_tail: String,
    #[allow(dead_code)]
    pub argv: Vec<String>,
    #[allow(dead_code)]
    pub isolated_with_bwrap: bool,
}

/// Resolve Blender binary: `AOS_BLENDER_BIN` → pack `bin/` → PATH → Windows installs.
pub fn resolve_blender_bin(pack_root: Option<&Path>) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AOS_BLENDER_BIN") {
        let pb = PathBuf::from(p.trim());
        if !p.trim().is_empty() && pb.is_file() {
            return Some(pb);
        }
    }
    {
        let home = std::env::var("AOS_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let managed = home
            .join("var/illustration-studio/integrations/blender/4.5.14");
        let name = if cfg!(windows) {
            "blender.exe"
        } else if cfg!(target_os = "macos") {
            "Blender"
        } else {
            "blender"
        };
        if let Some(bin) = find_managed_binary(&managed, name, 6) {
            return Some(bin);
        }
    }
    if let Some(root) = pack_root {
        for rel in ["bin/blender", "blender", "Blender.app/Contents/MacOS/Blender"] {
            let cand = root.join(rel);
            if cand.is_file() {
                return Some(cand);
            }
        }
        #[cfg(windows)]
        {
            for rel in ["bin/blender.exe", "blender.exe"] {
                let cand = root.join(rel);
                if cand.is_file() {
                    return Some(cand);
                }
            }
        }
    }
    if let Some(p) = which_on_path("blender") {
        return Some(p);
    }
    #[cfg(windows)]
    {
        if let Some(p) = windows_installed_blender() {
            return Some(p);
        }
    }
    None
}

fn find_managed_binary(root: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    if depth == 0 || !root.is_dir() {
        return None;
    }
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if path.is_file() && path.file_name().is_some_and(|file| file == name) {
            return Some(path);
        }
        if path.is_dir() {
            if let Some(found) = find_managed_binary(&path, name, depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

/// Official Windows installs under `Program Files\Blender Foundation\Blender X.Y\`
/// often omit `blender` from PATH.
#[cfg(windows)]
fn windows_installed_blender() -> Option<PathBuf> {
    let mut roots = Vec::new();
    for key in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Ok(base) = std::env::var(key) {
            let base = PathBuf::from(base);
            if key == "LOCALAPPDATA" {
                roots.push(base.join("Programs"));
            } else {
                roots.push(base.join("Blender Foundation"));
            }
        }
    }
    let mut found = Vec::new();
    for root in roots {
        let Ok(rd) = std::fs::read_dir(&root) else {
            continue;
        };
        for ent in rd.flatten() {
            let path = ent.path();
            let direct = path.join("blender.exe");
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case("blender.exe"))
                && path.is_file()
            {
                found.push(path);
                continue;
            }
            if direct.is_file() {
                found.push(direct);
            }
        }
    }
    found.sort();
    found.pop()
}

/// Resolve Renderer Pack root: `AOS_ILLUSTRATION_RENDERER_PACK` → AOS_HOME → CWD.
/// When the env var is set to a non-empty path that is not a directory, returns
/// `None` (fail-closed) — do not fall through to checkout defaults.
pub fn resolve_pack_root() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AOS_ILLUSTRATION_RENDERER_PACK") {
        let trimmed = p.trim();
        if !trimmed.is_empty() {
            let pb = PathBuf::from(trimmed);
            return pb.is_dir().then_some(pb);
        }
    }
    let mut candidates = Vec::new();
    if let Ok(home) = std::env::var("AOS_HOME") {
        let home = PathBuf::from(home.trim());
        if !home.as_os_str().is_empty() {
            candidates.push(home.join("share/illustration-renderer-pack"));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(bin_dir) = exe.parent() {
            candidates.push(bin_dir.join("../share/illustration-renderer-pack"));
        }
    }
    candidates.extend([
        PathBuf::from("share/illustration-renderer-pack"),
        PathBuf::from("../share/illustration-renderer-pack"),
        PathBuf::from("../../share/illustration-renderer-pack"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../share/illustration-renderer-pack"),
    ]);
    candidates.into_iter().find(|p| p.is_dir())
}

pub fn resolve_adapter_py(pack_root: &Path) -> Option<PathBuf> {
    let cand = pack_root.join("adapters/akasha_beauty.py");
    cand.is_file().then_some(cand)
}

fn which_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(name);
        if cand.is_file() {
            return Some(cand);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{name}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

pub fn bwrap_available() -> bool {
    which_on_path("bwrap").is_some()
}

/// Build fixed argv for headless beauty render. No shell.
pub fn build_blender_argv(plan: &BlenderSpawnPlan) -> Vec<String> {
    // blender -b --factory-startup --python <adapter> -- <work_dir>
    // Adapter reads scene.json + writes beauty.png inside work_dir only.
    vec![
        plan.blender_bin.to_string_lossy().into_owned(),
        "-b".into(),
        "--factory-startup".into(),
        "--python".into(),
        plan.adapter_py.to_string_lossy().into_owned(),
        "--".into(),
        plan.work_dir.to_string_lossy().into_owned(),
    ]
}

/// Spawn Blender (optionally under bubblewrap). Stdin closed; stdout/stderr capped.
pub fn spawn_isolated(plan: &BlenderSpawnPlan) -> Result<BlenderSpawnResult, String> {
    let argv = build_blender_argv(plan);
    let (program, args, isolated): (PathBuf, Vec<String>, bool) = if plan.use_bwrap && bwrap_available()
    {
        let mut bw: Vec<String> = vec![
            "bwrap".into(),
            "--die-with-parent".into(),
            "--unshare-net".into(),
            "--ro-bind".into(),
            plan.blender_bin.to_string_lossy().into_owned(),
            plan.blender_bin.to_string_lossy().into_owned(),
            "--ro-bind".into(),
            plan.adapter_py.to_string_lossy().into_owned(),
            plan.adapter_py.to_string_lossy().into_owned(),
            "--bind".into(),
            plan.work_dir.to_string_lossy().into_owned(),
            plan.work_dir.to_string_lossy().into_owned(),
            "--ro-bind".into(),
            "/usr".into(),
            "/usr".into(),
            "--ro-bind".into(),
            "/lib".into(),
            "/lib".into(),
            "--ro-bind".into(),
            "/lib64".into(),
            "/lib64".into(),
            "--proc".into(),
            "/proc".into(),
            "--dev".into(),
            "/dev".into(),
            "--chdir".into(),
            plan.work_dir.to_string_lossy().into_owned(),
        ];
        if let Some(parent) = plan.blender_bin.parent() {
            bw.push("--ro-bind".into());
            bw.push(parent.to_string_lossy().into_owned());
            bw.push(parent.to_string_lossy().into_owned());
        }
        if let Some(parent) = plan.adapter_py.parent() {
            bw.push("--ro-bind".into());
            bw.push(parent.to_string_lossy().into_owned());
            bw.push(parent.to_string_lossy().into_owned());
        }
        bw.extend(argv.clone());
        (PathBuf::from("bwrap"), bw[1..].to_vec(), true)
    } else {
        (plan.blender_bin.clone(), argv[1..].to_vec(), false)
    };

    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .current_dir(&plan.work_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .envs(minimal_child_env(&plan.blender_bin));

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn blender failed: {e}"))?;

    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    let stdout_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut out) = stdout_pipe {
            let _ = std::io::Read::read_to_end(&mut out, &mut buf);
        }
        buf
    });
    let stderr_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut err) = stderr_pipe {
            let _ = std::io::Read::read_to_end(&mut err, &mut buf);
        }
        buf
    });

    let deadline = std::time::Instant::now() + plan.timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stdout_handle.join();
                    let _ = stderr_handle.join();
                    return Err(format!(
                        "blender timed out after {}s",
                        plan.timeout.as_secs()
                    ));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("wait blender: {e}")),
        }
    };

    let stdout_buf = stdout_handle.join().unwrap_or_default();
    let stderr_buf = stderr_handle.join().unwrap_or_default();

    Ok(BlenderSpawnResult {
        exit_code: status.code().unwrap_or(-1),
        stdout_tail: tail_utf8(&stdout_buf, 4_000),
        stderr_tail: tail_utf8(&stderr_buf, 4_000),
        argv,
        isolated_with_bwrap: isolated,
    })
}

fn minimal_child_env(blender_bin: &Path) -> HashMap<String, String> {
    let mut env = HashMap::new();
    // Keep a narrow PATH so dynamic loaders / blender helpers can resolve.
    let mut path_dirs = Vec::new();
    if let Some(parent) = blender_bin.parent() {
        path_dirs.push(parent.to_string_lossy().into_owned());
    }
    #[cfg(windows)]
    {
        // Windows CreateProcess + Blender need SystemRoot / Win32 PATH.
        // Using `:` (Unix) here breaks DLL lookup and often makes spawn fail
        // while DeclUI pack status still reports "ready" (path probe only).
        let system_root = std::env::var("SystemRoot")
            .or_else(|_| std::env::var("WINDIR"))
            .unwrap_or_else(|_| r"C:\Windows".into());
        path_dirs.push(format!(r"{system_root}\System32"));
        path_dirs.push(system_root.clone());
        env.insert("PATH".into(), path_dirs.join(";"));
        env.insert("SystemRoot".into(), system_root.clone());
        env.insert("WINDIR".into(), system_root);
        for key in ["TEMP", "TMP", "USERPROFILE", "APPDATA", "LOCALAPPDATA"] {
            if let Ok(v) = std::env::var(key) {
                if !v.is_empty() {
                    env.insert(key.into(), v);
                }
            }
        }
        if let Ok(profile) = std::env::var("USERPROFILE") {
            env.insert("HOME".into(), profile);
        } else {
            env.insert("HOME".into(), String::new());
        }
    }
    #[cfg(not(windows))]
    {
        path_dirs.push("/usr/bin".into());
        path_dirs.push("/bin".into());
        env.insert("PATH".into(), path_dirs.join(":"));
        env.insert("HOME".into(), String::new());
    }
    env.insert("LANG".into(), "C".into());
    // Discourage network-touching Python in adapter (honor if possible).
    env.insert("PYTHONNOUSERSITE".into(), "1".into());
    env.insert("AOS_BLENDER_ISOLATED".into(), "1".into());
    if let Ok(disp) = std::env::var("DISPLAY") {
        // Eevee may need display on some Linux hosts; Cycles CPU does not.
        // Pass through only when already set (fail-soft for GPU paths).
        env.insert("DISPLAY".into(), disp);
    }
    env
}

fn tail_utf8(bytes: &[u8], max: usize) -> String {
    let s = String::from_utf8_lossy(bytes);
    if s.len() <= max {
        s.into_owned()
    } else {
        s[s.len() - max..].to_string()
    }
}

/// Platform isolation matrix (static documentation helper for tests/docs).
pub fn isolation_matrix() -> &'static [IsolationRow] {
    &ISOLATION_MATRIX
}

#[derive(Debug, Clone, Copy)]
pub struct IsolationRow {
    pub platform: &'static str,
    pub mode: &'static str,
    pub fs_deny: &'static str,
    pub net_deny: &'static str,
    pub status: &'static str,
    pub notes: &'static str,
}

static ISOLATION_MATRIX: [IsolationRow; 6] = [
    IsolationRow {
        platform: "Linux",
        mode: "bwrap present",
        fs_deny: "bind work+bin+adapter only",
        net_deny: "--unshare-net",
        status: "GO (best-effort)",
        notes: "Dynamic linker / GPU ICD paths may need extra ro-binds",
    },
    IsolationRow {
        platform: "Linux",
        mode: "no bwrap",
        fs_deny: "workdir only (convention)",
        net_deny: "not enforced",
        status: "GO gap",
        notes: "Fixed argv + env_clear; ambient FS/net remain — P0-B fail-closed gap",
    },
    IsolationRow {
        platform: "Windows",
        mode: "CREATE_NO_WINDOW",
        fs_deny: "not enforced",
        net_deny: "not enforced",
        status: "GO gap",
        notes: "AppContainer / job object FS+net jail not wired (P0-B)",
    },
    IsolationRow {
        platform: "macOS",
        mode: "spawn",
        fs_deny: "not enforced",
        net_deny: "not enforced",
        status: "GO gap / high risk",
        notes: "sandbox-exec + Metal headless unproven (P0-B)",
    },
    IsolationRow {
        platform: "CI",
        mode: "AOS_BLENDER_MODE=mock",
        fs_deny: "n/a (no child)",
        net_deny: "n/a",
        status: "GO",
        notes: "Explicit mock never needs Blender; Auto mocks only when opt-in pack+adapter present",
    },
    IsolationRow {
        platform: "Any",
        mode: "pack absent (Auto/Require)",
        fs_deny: "n/a",
        net_deny: "n/a",
        status: "fail-closed",
        notes: "BackendUnavailable until Renderer Pack install / AOS_ILLUSTRATION_RENDERER_PACK",
    },
];

/// Serialize env mutations that touch pack resolution (parallel test safety).
#[cfg(test)]
pub(crate) static BLENDER_PACK_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
struct BlenderPackEnvRestore {
    pack: Option<String>,
    mode: Option<String>,
    bin: Option<String>,
}

#[cfg(test)]
impl BlenderPackEnvRestore {
    fn capture() -> Self {
        Self {
            pack: std::env::var("AOS_ILLUSTRATION_RENDERER_PACK").ok(),
            mode: std::env::var("AOS_BLENDER_MODE").ok(),
            bin: std::env::var("AOS_BLENDER_BIN").ok(),
        }
    }
}

#[cfg(test)]
impl Drop for BlenderPackEnvRestore {
    fn drop(&mut self) {
        match &self.pack {
            Some(v) => std::env::set_var("AOS_ILLUSTRATION_RENDERER_PACK", v),
            None => std::env::remove_var("AOS_ILLUSTRATION_RENDERER_PACK"),
        }
        match &self.mode {
            Some(v) => std::env::set_var("AOS_BLENDER_MODE", v),
            None => std::env::remove_var("AOS_BLENDER_MODE"),
        }
        match &self.bin {
            Some(v) => std::env::set_var("AOS_BLENDER_BIN", v),
            None => std::env::remove_var("AOS_BLENDER_BIN"),
        }
    }
}

/// Holds pack-env lock + restores `AOS_*` vars on drop (parallel test safety).
#[cfg(test)]
pub(crate) struct BlenderPackTestEnv {
    _lock: std::sync::MutexGuard<'static, ()>,
    _restore: BlenderPackEnvRestore,
}

#[cfg(test)]
impl BlenderPackTestEnv {
    /// Checkout Renderer Pack + default Auto mode, no explicit `AOS_BLENDER_BIN`.
    pub(crate) fn checkout_auto_mock() -> Self {
        use std::path::PathBuf;
        let lock = BLENDER_PACK_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let restore = BlenderPackEnvRestore::capture();
        let pack = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../share/illustration-renderer-pack");
        if pack.is_dir() {
            std::env::set_var("AOS_ILLUSTRATION_RENDERER_PACK", &pack);
        } else {
            std::env::remove_var("AOS_ILLUSTRATION_RENDERER_PACK");
        }
        std::env::remove_var("AOS_BLENDER_BIN");
        std::env::remove_var("AOS_BLENDER_MODE");
        Self {
            _lock: lock,
            _restore: restore,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn argv_is_fixed_shape() {
        let plan = BlenderSpawnPlan {
            blender_bin: PathBuf::from("/opt/blender/blender"),
            adapter_py: PathBuf::from("/pack/adapters/akasha_beauty.py"),
            work_dir: PathBuf::from("/tmp/job"),
            scene_json: PathBuf::from("/tmp/job/scene.json"),
            output_png: PathBuf::from("/tmp/job/beauty.png"),
            use_bwrap: false,
            timeout: Duration::from_secs(30),
        };
        let argv = build_blender_argv(&plan);
        assert_eq!(argv[1], "-b");
        assert!(argv.contains(&"--factory-startup".to_string()));
        assert!(argv.contains(&"--python".to_string()));
        assert_eq!(argv.last().unwrap(), "/tmp/job");
    }

    #[test]
    fn matrix_mentions_gaps() {
        let rows = isolation_matrix();
        assert!(rows.iter().any(|r| r.status.contains("gap")));
        assert!(rows.iter().any(|r| r.status.contains("fail-closed")));
    }

    #[cfg(windows)]
    #[test]
    fn windows_child_env_uses_semicolon_path_and_systemroot() {
        let env = minimal_child_env(Path::new(
            r"C:\Program Files\Blender Foundation\Blender 5.2\blender.exe",
        ));
        let path = env.get("PATH").expect("PATH");
        assert!(
            path.contains(';'),
            "Windows PATH must use ';', got {path:?}"
        );
        assert!(
            !path.contains("/usr/bin"),
            "Unix PATH fragments must not appear on Windows: {path:?}"
        );
        assert!(env.contains_key("SystemRoot"));
        assert!(env.contains_key("WINDIR"));
    }

    #[test]
    fn pack_env_invalid_path_fail_closed() {
        let _lock = BLENDER_PACK_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _restore = BlenderPackEnvRestore::capture();
        std::env::set_var(
            "AOS_ILLUSTRATION_RENDERER_PACK",
            "/tmp/aos-missing-blender-renderer-pack",
        );
        let status = probe_pack_status();
        assert!(status.pack_root.is_none());
        assert!(!status.ready_for_mock);
        assert!(!status.ready_for_spawn);
        assert!(status.summary_en().contains("missing"));
        assert!(status.summary_fr().contains("absent"));
    }

    #[test]
    fn checkout_pack_is_mock_ready() {
        let _lock = BLENDER_PACK_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _restore = BlenderPackEnvRestore::capture();
        let pack = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../share/illustration-renderer-pack");
        assert!(pack.is_dir());
        std::env::set_var("AOS_ILLUSTRATION_RENDERER_PACK", &pack);
        std::env::remove_var("AOS_BLENDER_BIN");
        std::env::set_var("AOS_BLENDER_MODE", "auto");
        let status = probe_pack_status();
        assert!(status.ready_for_mock);
        assert!(status.adapter_py.is_some());
    }
}
