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
    /// Prefer real Blender when found; otherwise mock (CI / offline default).
    Auto,
    /// Always produce mock golden PNG — never spawn Blender.
    Mock,
    /// Require a real Blender binary; error if missing.
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

/// Resolve Blender binary: `AOS_BLENDER_BIN` → pack `bin/blender` → PATH `blender`.
pub fn resolve_blender_bin(pack_root: Option<&Path>) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AOS_BLENDER_BIN") {
        let pb = PathBuf::from(p.trim());
        if !p.trim().is_empty() && pb.is_file() {
            return Some(pb);
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
            let cand = root.join("blender.exe");
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    which_on_path("blender")
}

/// Resolve Renderer Pack root: `AOS_ILLUSTRATION_RENDERER_PACK` → relative defaults.
pub fn resolve_pack_root() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AOS_ILLUSTRATION_RENDERER_PACK") {
        let pb = PathBuf::from(p.trim());
        if !p.trim().is_empty() && pb.is_dir() {
            return Some(pb);
        }
    }
    // Dev checkout defaults (repo-relative from CWD or crate).
    let candidates = [
        PathBuf::from("share/illustration-renderer-pack"),
        PathBuf::from("../share/illustration-renderer-pack"),
        PathBuf::from("../../share/illustration-renderer-pack"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../share/illustration-renderer-pack"),
    ];
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
    path_dirs.push("/usr/bin".into());
    path_dirs.push("/bin".into());
    env.insert("PATH".into(), path_dirs.join(":"));
    env.insert("HOME".into(), String::new());
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
        notes: "Default Auto falls back to mock without Blender binary",
    },
    IsolationRow {
        platform: "Any",
        mode: "mock",
        fs_deny: "n/a",
        net_deny: "n/a",
        status: "GO",
        notes: "Deterministic golden PNG from SceneGraph export digest",
    },
];

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
        assert!(rows.iter().any(|r| r.mode.contains("mock")));
    }
}
