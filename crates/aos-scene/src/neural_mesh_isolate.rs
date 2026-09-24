//! Isolated neural-mesh Model Pack spawn (Blender-pack pattern).
//!
//! Model:
//! - Fixed argv only — never libre shell from modules
//! - Work directory quarantines image in + GLB out
//! - Optional Linux `bwrap --unshare-net` when available
//! - Clear env of ambient secrets; keep a minimal platform PATH (+ Vulkan ICD discovery on Windows)
//!
//! Real runner shape matches **trellis.cpp** `trellis-cli`:
//! `trellis-cli <input.png> <output.glb> --models <GGUF_DIR> [--res 512|1024|…]`
//!
//! Weights (TRELLIS.2 GGUF multi-file sets) and the trellis.cpp / LocalAI
//! runner are **never** vendored into the Akasha git tree. Point
//! `AOS_NEURAL_MESH_PACK` / `AOS_NEURAL_MESH_BIN` / `AOS_NEURAL_MESH_WEIGHTS`
//! at an offline install. See `share/illustration-neural-mesh-pack/README.md`.
//!
//! Gaps (same class as Blender P0-B): Windows AppContainer and macOS
//! sandbox-exec are not wired yet.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Default wall-clock timeout for a neural mesh child (minutes-scale jobs).
pub const DEFAULT_NEURAL_MESH_TIMEOUT_SECS: u64 = 600;

/// Default geometry resolution for trellis-cli (`--res`).
pub const DEFAULT_NEURAL_MESH_RES: u32 = 512;

/// How the host resolves the neural mesh runner / mock path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeuralMeshRunMode {
    /// Prefer real runner when found; otherwise fixture mock when available.
    Auto,
    /// Always use fixture GLB — never spawn trellis / LocalAI.
    Mock,
    /// Require a real runner binary + weights; error if missing.
    Require,
}

impl NeuralMeshRunMode {
    pub fn from_env() -> Self {
        match std::env::var("AOS_NEURAL_MESH_MODE")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "mock" | "stub" | "fixture" => Self::Mock,
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

/// Which argv family to emit for the resolved runner binary / adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeuralMeshRunnerKind {
    /// `trellis-cli <in> <out> --models <dir> …` (pwilkin/trellis.cpp).
    TrellisCli,
    /// Pack `adapters/trellis_gguf.sh` — same argv as trellis-cli (wraps CLI or LocalAI).
    Adapter,
    /// LocalAI binary — still argv-shaped via adapter preference; direct spawn uses trellis flags.
    LocalAi,
}

impl NeuralMeshRunnerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TrellisCli => "trellis-cli",
            Self::Adapter => "adapter",
            Self::LocalAi => "local-ai",
        }
    }

    pub fn detect(bin: &Path) -> Self {
        let name = bin
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if name.contains("trellis_gguf") || name.contains("adapter") {
            Self::Adapter
        } else if name.contains("local-ai") || name.contains("localai") {
            Self::LocalAi
        } else {
            Self::TrellisCli
        }
    }
}

#[derive(Debug, Clone)]
pub struct NeuralMeshPackStatus {
    pub pack_root: Option<PathBuf>,
    pub runner_bin: Option<PathBuf>,
    pub runner_kind: Option<NeuralMeshRunnerKind>,
    pub weights_dir: Option<PathBuf>,
    pub fixture_glb: Option<PathBuf>,
    pub mode: NeuralMeshRunMode,
    pub ready_for_mock: bool,
    /// Pack + runner + GGUF weights directory (at least one `.gguf` or ready marker).
    pub ready_for_spawn: bool,
}

impl NeuralMeshPackStatus {
    pub fn summary_en(&self) -> String {
        match (
            &self.pack_root,
            self.ready_for_mock,
            self.ready_for_spawn,
            self.weights_dir.is_some(),
            self.runner_bin.is_some(),
        ) {
            (None, _, _, _, _) => {
                "Neural mesh pack: missing (backend=neural fail-closed)".into()
            }
            (Some(_), true, true, _, _) => format!(
                "Neural mesh pack: ready (mode={}, runner={}, weights+fixture)",
                self.mode.as_str(),
                self.runner_kind
                    .map(|k| k.as_str())
                    .unwrap_or("unknown")
            ),
            (Some(_), true, false, false, true) => format!(
                "Neural mesh pack: mock-ready (mode={}; runner present, GGUF weights missing)",
                self.mode.as_str()
            ),
            (Some(_), true, false, true, false) => format!(
                "Neural mesh pack: mock-ready (mode={}; weights present, runner missing)",
                self.mode.as_str()
            ),
            (Some(_), true, false, _, _) => format!(
                "Neural mesh pack: mock-ready (mode={}, fixture GLB; spawn not ready)",
                self.mode.as_str()
            ),
            (Some(_), false, true, _, _) => format!(
                "Neural mesh pack: runner ready (mode={}; no fixture)",
                self.mode.as_str()
            ),
            (Some(_), false, false, _, _) => format!(
                "Neural mesh pack: present but incomplete (mode={})",
                self.mode.as_str()
            ),
        }
    }

    pub fn summary_fr(&self) -> String {
        match (
            &self.pack_root,
            self.ready_for_mock,
            self.ready_for_spawn,
            self.weights_dir.is_some(),
            self.runner_bin.is_some(),
        ) {
            (None, _, _, _, _) => {
                "Pack mesh neural : absent (backend=neural refusé)".into()
            }
            (Some(_), true, true, _, _) => format!(
                "Pack mesh neural : prêt (mode={}, runner={}, poids+fixture)",
                self.mode.as_str(),
                self.runner_kind
                    .map(|k| k.as_str())
                    .unwrap_or("inconnu")
            ),
            (Some(_), true, false, false, true) => format!(
                "Pack mesh neural : mock prêt (mode={} ; runner présent, poids GGUF absents)",
                self.mode.as_str()
            ),
            (Some(_), true, false, true, false) => format!(
                "Pack mesh neural : mock prêt (mode={} ; poids présents, runner absent)",
                self.mode.as_str()
            ),
            (Some(_), true, false, _, _) => format!(
                "Pack mesh neural : mock prêt (mode={}, GLB fixture ; spawn non prêt)",
                self.mode.as_str()
            ),
            (Some(_), false, true, _, _) => format!(
                "Pack mesh neural : runner prêt (mode={} ; pas de fixture)",
                self.mode.as_str()
            ),
            (Some(_), false, false, _, _) => format!(
                "Pack mesh neural : présent mais incomplet (mode={})",
                self.mode.as_str()
            ),
        }
    }
}

/// Resolve Neural Mesh Pack root: `AOS_NEURAL_MESH_PACK` → relative defaults.
/// When the env var is set to a non-empty path that is not a directory, returns
/// `None` (fail-closed) — do not fall through to checkout defaults.
pub fn resolve_pack_root() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AOS_NEURAL_MESH_PACK") {
        let trimmed = p.trim();
        if !trimmed.is_empty() {
            let pb = PathBuf::from(trimmed);
            return pb.is_dir().then_some(pb);
        }
    }
    let candidates = [
        PathBuf::from("share/illustration-neural-mesh-pack"),
        PathBuf::from("../share/illustration-neural-mesh-pack"),
        PathBuf::from("../../share/illustration-neural-mesh-pack"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../share/illustration-neural-mesh-pack"),
    ];
    candidates.into_iter().find(|p| p.is_dir())
}

/// Resolve runner: env → pack adapter → pack `bin/trellis-cli` → PATH.
pub fn resolve_runner_bin(pack_root: Option<&Path>) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AOS_NEURAL_MESH_BIN") {
        let pb = PathBuf::from(p.trim());
        if !p.trim().is_empty() && pb.is_file() {
            return Some(pb);
        }
    }
    {
        let home = std::env::var("AOS_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let runtime = home
            .join("var/illustration-studio/integrations/trellis/runtime/v0.6.0");
        let name = if cfg!(windows) { "trellis-cli.exe" } else { "trellis-cli" };
        if let Some(bin) = find_managed_binary(&runtime, name, 6) {
            return Some(bin);
        }
    }
    if let Some(root) = pack_root {
        let rels: &[&str] = if cfg!(windows) {
            &["bin/trellis-cli.exe", "adapters/trellis_gguf.cmd", "trellis-cli.exe", "bin/localai.exe"]
        } else {
            &["adapters/trellis_gguf.sh", "bin/trellis-cli", "trellis-cli", "bin/local-ai", "bin/localai"]
        };
        for rel in rels {
            let cand = root.join(rel);
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    which_on_path("trellis-cli").or_else(|| which_on_path("local-ai"))
}

/// Fixture GLB for mock / CI: env override → pack fixtures → crate test fixture.
pub fn resolve_fixture_glb(pack_root: Option<&Path>) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AOS_NEURAL_MESH_FIXTURE") {
        let pb = PathBuf::from(p.trim());
        if !p.trim().is_empty() && pb.is_file() {
            return Some(pb);
        }
    }
    if let Some(root) = pack_root {
        let cand = root.join("fixtures/unit_cube.glb");
        if cand.is_file() {
            return Some(cand);
        }
    }
    let crate_fix = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/unit_cube.glb");
    crate_fix.is_file().then_some(crate_fix)
}

/// Resolve optional weights directory (never downloaded at generate-time).
/// A directory counts only when it looks like a GGUF set (`.gguf` present) or
/// carries an explicit `.aos-weights-ready` marker (CI / dry-run installs).
pub fn resolve_weights_dir(pack_root: Option<&Path>) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AOS_NEURAL_MESH_WEIGHTS") {
        let pb = PathBuf::from(p.trim());
        if !p.trim().is_empty() && weights_dir_ready(&pb) {
            return Some(pb);
        }
    }
    {
        let home = std::env::var("AOS_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let base = home
            .join("var/illustration-studio/integrations/trellis/weights");
        if let Ok(current) = std::fs::read_to_string(base.join("current.txt")) {
            let quant = current.trim();
            if quant == "q4" || quant == "q8" {
                let selected = base.join(quant);
                if selected.join(".aos-weights-ready").is_file() && weights_dir_ready(&selected) {
                    return Some(selected);
                }
            }
        }
        for quant in ["q4", "q8"] {
            let selected = base.join(quant);
            if selected.join(".aos-weights-ready").is_file() && weights_dir_ready(&selected) {
                return Some(selected);
            }
        }
    }
    if let Some(root) = pack_root {
        let cand = root.join("weights");
        if weights_dir_ready(&cand) {
            return Some(cand);
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

/// True when `dir` exists and contains at least one `.gguf` (any depth-1) or a ready marker.
pub fn weights_dir_ready(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    if dir.join(".aos-weights-ready").is_file() {
        return true;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("gguf"))
        {
            return true;
        }
        // Quantized layouts often nest under q4/ q8/.
        if path.is_dir() {
            if let Ok(sub) = std::fs::read_dir(&path) {
                for child in sub.flatten() {
                    if child
                        .path()
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("gguf"))
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

pub fn probe_pack_status() -> NeuralMeshPackStatus {
    let mode = NeuralMeshRunMode::from_env();
    let pack_root = resolve_pack_root();
    let runner_bin = resolve_runner_bin(pack_root.as_deref());
    let runner_kind = runner_bin.as_ref().map(|b| NeuralMeshRunnerKind::detect(b));
    let weights_dir = resolve_weights_dir(pack_root.as_deref());
    let fixture_glb = resolve_fixture_glb(pack_root.as_deref());
    let ready_for_mock = pack_root.is_some() && fixture_glb.is_some();
    let ready_for_spawn =
        pack_root.is_some() && runner_bin.is_some() && weights_dir.is_some();
    NeuralMeshPackStatus {
        pack_root,
        runner_bin,
        runner_kind,
        weights_dir,
        fixture_glb,
        mode,
        ready_for_mock,
        ready_for_spawn,
    }
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

/// trellis-cli `--gpu` index: `AOS_NEURAL_MESH_GPU`, or `0` on Windows when unset.
pub fn resolve_neural_mesh_gpu_index() -> Option<u32> {
    if let Ok(s) = std::env::var("AOS_NEURAL_MESH_GPU") {
        let t = s.trim();
        if !t.is_empty() {
            return t.parse().ok();
        }
    }
    #[cfg(windows)]
    {
        Some(0)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// When true, argv omits `--require-gpu` so trellis may fall back to CPU.
pub fn neural_mesh_allow_cpu() -> bool {
    std::env::var("AOS_NEURAL_MESH_ALLOW_CPU")
        .ok()
        .is_some_and(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
}

/// Geometry resolution for `--res` (env `AOS_NEURAL_MESH_RES`, default 512).
pub fn resolve_geometry_res() -> u32 {
    std::env::var("AOS_NEURAL_MESH_RES")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|r| matches!(r, 512 | 1024 | 1536))
        .unwrap_or(DEFAULT_NEURAL_MESH_RES)
}

#[derive(Debug, Clone)]
pub struct NeuralMeshSpawnPlan {
    pub runner_bin: PathBuf,
    pub runner_kind: NeuralMeshRunnerKind,
    pub work_dir: PathBuf,
    pub input_image: PathBuf,
    pub output_glb: PathBuf,
    pub weights_dir: PathBuf,
    pub geometry_res: u32,
    pub use_bwrap: bool,
    pub timeout: Duration,
}

/// Fixed argv matching trellis.cpp / pack adapter:
/// `<bin> <input.png> <output.glb> --models <dir> --res <N>`.
/// Shell adapters are invoked as `bash <adapter.sh> …` so +x is not required.
///
/// LocalAI HTTP is intentionally not used here (caps prefer argv + net deny).
/// Point `AOS_NEURAL_MESH_BIN` at `adapters/trellis_gguf.sh` to wrap LocalAI offline.
pub fn build_trellis_argv(plan: &NeuralMeshSpawnPlan) -> Vec<String> {
    let mut argv = Vec::new();
    let is_shell = plan
        .runner_bin
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("sh"))
        || matches!(plan.runner_kind, NeuralMeshRunnerKind::Adapter);
    if is_shell {
        argv.push("bash".into());
    }
    argv.push(plan.runner_bin.to_string_lossy().into_owned());
    argv.push(plan.input_image.to_string_lossy().into_owned());
    argv.push(plan.output_glb.to_string_lossy().into_owned());
    argv.push("--models".into());
    argv.push(plan.weights_dir.to_string_lossy().into_owned());
    argv.push("--res".into());
    argv.push(plan.geometry_res.to_string());
    if let Some(gpu) = resolve_neural_mesh_gpu_index() {
        argv.push("--gpu".into());
        argv.push(gpu.to_string());
    }
    if !neural_mesh_allow_cpu() {
        argv.push("--require-gpu".into());
    }
    argv
}

#[derive(Debug, Clone)]
pub struct NeuralMeshSpawnResult {
    pub exit_code: i32,
    #[allow(dead_code)]
    pub stderr_tail: String,
    pub argv: Vec<String>,
    pub isolated_with_bwrap: bool,
}

/// Spawn runner (optionally under bubblewrap). Stdin closed; stdout/stderr capped.
pub fn spawn_isolated(plan: &NeuralMeshSpawnPlan) -> Result<NeuralMeshSpawnResult, String> {
    let argv = build_trellis_argv(plan);
    let pack_root_guess = plan
        .runner_bin
        .parent()
        .and_then(|p| p.parent())
        .map(Path::to_path_buf);
    let (program, args, isolated): (PathBuf, Vec<String>, bool) =
        if plan.use_bwrap && bwrap_available() {
            let mut bw: Vec<String> = vec![
                "bwrap".into(),
                "--die-with-parent".into(),
                "--unshare-net".into(),
                "--ro-bind".into(),
                plan.runner_bin.to_string_lossy().into_owned(),
                plan.runner_bin.to_string_lossy().into_owned(),
                "--bind".into(),
                plan.work_dir.to_string_lossy().into_owned(),
                plan.work_dir.to_string_lossy().into_owned(),
                "--ro-bind".into(),
                plan.weights_dir.to_string_lossy().into_owned(),
                plan.weights_dir.to_string_lossy().into_owned(),
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
            if let Some(pack) = &pack_root_guess {
                bw.push("--ro-bind".into());
                bw.push(pack.to_string_lossy().into_owned());
                bw.push(pack.to_string_lossy().into_owned());
            }
            if let Some(sh) = which_on_path("bash").or_else(|| which_on_path("sh")) {
                bw.push("--ro-bind".into());
                bw.push(sh.to_string_lossy().into_owned());
                bw.push(sh.to_string_lossy().into_owned());
            }
            bw.extend(argv.iter().cloned());
            (PathBuf::from("bwrap"), bw[1..].to_vec(), true)
        } else {
            (
                PathBuf::from(&argv[0]),
                argv.iter().skip(1).cloned().collect(),
                false,
            )
        };

    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .envs(minimal_neural_mesh_child_env(&plan.runner_bin, &plan.work_dir))
        .current_dir(&plan.work_dir);
    if let Some(pack) = &pack_root_guess {
        cmd.env("AOS_NEURAL_MESH_PACK", pack.as_os_str());
    }
    // Allowlisted host knobs only — never forward ambient secrets / HF tokens.
    for key in [
        "AOS_NEURAL_MESH_ADAPTER_MOCK",
        "AOS_NEURAL_MESH_FIXTURE",
        "AOS_NEURAL_MESH_TRELLIS_CLI",
    ] {
        if let Ok(v) = std::env::var(key) {
            if !v.is_empty() {
                cmd.env(key, v);
            }
        }
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd.spawn().map_err(|e| format!("spawn: {e}"))?;
    let stdout_capture = Arc::new(Mutex::new(String::new()));
    let stderr_capture = Arc::new(Mutex::new(String::new()));
    let stdout_handle = child.stdout.take().map(|mut pipe| {
        let cap = Arc::clone(&stdout_capture);
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = pipe.read(&mut buf) {
                if n == 0 {
                    break;
                }
                if let Ok(mut s) = cap.lock() {
                    s.push_str(&String::from_utf8_lossy(&buf[..n]));
                }
            }
        })
    });
    let stderr_handle = child.stderr.take().map(|mut pipe| {
        let cap = Arc::clone(&stderr_capture);
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = pipe.read(&mut buf) {
                if n == 0 {
                    break;
                }
                if let Ok(mut s) = cap.lock() {
                    s.push_str(&String::from_utf8_lossy(&buf[..n]));
                }
            }
        })
    });
    let timeout = plan.timeout;
    let start = std::time::Instant::now();
    let work_dir = plan.work_dir.clone();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if let Some(h) = stdout_handle {
                    let _ = h.join();
                }
                if let Some(h) = stderr_handle {
                    let _ = h.join();
                }
                let stdout_full = stdout_capture.lock().map(|s| s.clone()).unwrap_or_default();
                let stderr_full = stderr_capture.lock().map(|s| s.clone()).unwrap_or_default();
                let exit_code = status.code().unwrap_or(-1);
                if exit_code != 0 {
                    persist_spawn_io_tails(&work_dir, &stdout_full, &stderr_full);
                }
                let tail = trim_stderr_tail(&stderr_full, 2000);
                return Ok(NeuralMeshSpawnResult {
                    exit_code,
                    stderr_tail: tail,
                    argv,
                    isolated_with_bwrap: isolated,
                });
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    if let Some(h) = stdout_handle {
                        let _ = h.join();
                    }
                    if let Some(h) = stderr_handle {
                        let _ = h.join();
                    }
                    let stdout_full = stdout_capture.lock().map(|s| s.clone()).unwrap_or_default();
                    let stderr_full = stderr_capture.lock().map(|s| s.clone()).unwrap_or_default();
                    persist_spawn_io_tails(&work_dir, &stdout_full, &stderr_full);
                    return Err(format!(
                        "neural mesh runner timed out after {}s",
                        timeout.as_secs()
                    ));
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("wait: {e}")),
        }
    }
}

const SPAWN_IO_TAIL_CHARS: usize = 32_768;

fn persist_spawn_io_tails(work_dir: &Path, stdout: &str, stderr: &str) {
    let out_tail = trim_stderr_tail(stdout, SPAWN_IO_TAIL_CHARS);
    let err_tail = trim_stderr_tail(stderr, SPAWN_IO_TAIL_CHARS);
    let _ = std::fs::write(work_dir.join("spawn.out"), out_tail);
    let _ = std::fs::write(work_dir.join("spawn.err"), err_tail);
}

/// Trim stderr for DeclUI / error strings (matches Blender isolate tail budget).
pub fn trim_stderr_tail(stderr: &str, max_chars: usize) -> String {
    if stderr.len() <= max_chars {
        return stderr.to_string();
    }
    stderr[stderr.len() - max_chars..].to_string()
}

/// Human-readable spawn failure (exit code, Vulkan device lines, stderr tail).
pub fn format_spawn_failure(result: &NeuralMeshSpawnResult, missing_output_glb: bool) -> String {
    let mut parts = vec![format!("neural mesh runner exit={}", result.exit_code)];
    if missing_output_glb {
        parts.push("output.glb missing".into());
    }
    if let Some(hint) = vulkan_device_lines(&result.stderr_tail) {
        parts.push(hint);
    }
    let tail = trim_stderr_tail(&result.stderr_tail, 2000);
    if !tail.is_empty() {
        parts.push(format!("stderr_tail: {tail}"));
    }
    parts.join("; ")
}

fn vulkan_device_lines(stderr: &str) -> Option<String> {
    let mut hits = Vec::new();
    for line in stderr.lines() {
        let t = line.trim();
        if t.contains("ggml_vulkan:")
            || t.contains("[deform_vk]")
            || t.contains("[trellis]")
            || t.contains("Vulkan devices")
        {
            hits.push(t);
        }
    }
    if hits.is_empty() {
        None
    } else {
        Some(hits.join(" | "))
    }
}

/// Minimal env for trellis-cli after `env_clear()` — secrets scrubbed; Vulkan ICD discovery on Windows.
pub fn minimal_neural_mesh_child_env(runner_bin: &Path, work_dir: &Path) -> HashMap<String, String> {
    let mut env = HashMap::new();
    let mut path_dirs = Vec::new();
    if let Some(parent) = runner_bin.parent() {
        path_dirs.push(parent.to_string_lossy().into_owned());
    }
    #[cfg(windows)]
    {
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
        let work = work_dir.to_string_lossy().into_owned();
        env.insert("TEMP".into(), work.clone());
        env.insert("TMP".into(), work.clone());
        if let Ok(profile) = std::env::var("USERPROFILE") {
            env.insert("HOME".into(), profile);
        } else {
            env.insert("HOME".into(), work);
        }
    }
    #[cfg(not(windows))]
    {
        path_dirs.push("/usr/bin".into());
        path_dirs.push("/bin".into());
        env.insert("PATH".into(), path_dirs.join(":"));
        env.insert("HOME".into(), work_dir.to_string_lossy().into_owned());
        env.insert("TMPDIR".into(), work_dir.to_string_lossy().into_owned());
    }
    env.insert("LANG".into(), "C".into());
    forward_vk_discovery_env(&mut env);
    apply_vk_device_pin(&mut env);
    #[cfg(windows)]
    {
        prefer_discrete_nvidia_vk_icd(&mut env);
    }
    if let Ok(disp) = std::env::var("DISPLAY") {
        if !disp.is_empty() {
            env.insert("DISPLAY".into(), disp);
        }
    }
    env
}

fn forward_vk_discovery_env(env: &mut HashMap<String, String>) {
    for (key, value) in std::env::vars() {
        if value.is_empty() {
            continue;
        }
        let forward = key.starts_with("VK_")
            || matches!(
                key.as_str(),
                "GGML_VK_VISIBLE_DEVICES" | "CUDA_VISIBLE_DEVICES" | "HIP_VISIBLE_DEVICES"
            );
        if forward {
            env.insert(key, value);
        }
    }
}

fn apply_vk_device_pin(env: &mut HashMap<String, String>) {
    if let Ok(pin) = std::env::var("AOS_NEURAL_MESH_VK_DEVICE") {
        let pin = pin.trim();
        if !pin.is_empty() {
            env.insert("GGML_VK_VISIBLE_DEVICES".into(), pin.to_string());
            return;
        }
    }
    if !env.contains_key("GGML_VK_VISIBLE_DEVICES") {
        #[cfg(windows)]
        if let Some(gpu) = resolve_neural_mesh_gpu_index() {
            env.insert("GGML_VK_VISIBLE_DEVICES".into(), gpu.to_string());
        }
    }
}

/// When multiple ICD JSON paths are listed, keep NVIDIA so ggml sees the discrete GPU.
#[cfg(windows)]
fn prefer_nvidia_vk_icd_filenames(filenames: &str) -> String {
    let sep = if filenames.contains(';') { ';' } else { ',' };
    let parts: Vec<&str> = filenames
        .split(sep)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() <= 1 {
        return filenames.to_string();
    }
    for part in &parts {
        let lower = part.to_ascii_lowercase();
        if lower.contains("nvidia") || lower.contains("nv_disp") || lower.contains("\\nv") {
            return (*part).to_string();
        }
    }
    filenames.to_string()
}

#[cfg(windows)]
fn prefer_discrete_nvidia_vk_icd(env: &mut HashMap<String, String>) {
    if std::env::var("AOS_NEURAL_MESH_VK_PREFER_DISCRETE")
        .ok()
        .is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "0" | "false" | "off" | "no"))
    {
        return;
    }
    if let Some(existing) = env.get("VK_ICD_FILENAMES") {
        let narrowed = prefer_nvidia_vk_icd_filenames(existing);
        if narrowed != *existing {
            env.insert("VK_ICD_FILENAMES".into(), narrowed);
        }
        return;
    }
    let system_root = env
        .get("SystemRoot")
        .cloned()
        .unwrap_or_else(|| r"C:\Windows".into());
    if let Some(icd) = discover_windows_nvidia_vk_icd(&system_root) {
        env.insert("VK_ICD_FILENAMES".into(), icd);
    }
}

#[cfg(windows)]
fn discover_windows_nvidia_vk_icd(system_root: &str) -> Option<String> {
    let repo = Path::new(system_root)
        .join("System32")
        .join("DriverStore")
        .join("FileRepository");
    if !repo.is_dir() {
        return None;
    }
    let Ok(entries) = std::fs::read_dir(&repo) else {
        return None;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        for name in ["nv_dispc.json", "nv_dispig.json", "nv_disp.json"] {
            let cand = dir.join(name);
            if cand.is_file() {
                return Some(cand.to_string_lossy().into_owned());
            }
        }
    }
    None
}

/// Shared with `neural_mesh` tests that mutate `AOS_NEURAL_MESH_*` env vars.
#[cfg(test)]
pub(crate) static NEURAL_MESH_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_root_resolves_from_crate() {
        let _guard = NEURAL_MESH_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let prev_pack = std::env::var("AOS_NEURAL_MESH_PACK").ok();
        std::env::remove_var("AOS_NEURAL_MESH_PACK");
        let root = resolve_pack_root();
        match prev_pack {
            Some(v) => std::env::set_var("AOS_NEURAL_MESH_PACK", v),
            None => std::env::remove_var("AOS_NEURAL_MESH_PACK"),
        }
        assert!(
            root.is_some(),
            "share/illustration-neural-mesh-pack should exist"
        );
        let fixture = resolve_fixture_glb(root.as_deref());
        assert!(fixture.is_some());
    }

    fn sample_spawn_plan() -> NeuralMeshSpawnPlan {
        NeuralMeshSpawnPlan {
            runner_bin: PathBuf::from("/opt/trellis-cli"),
            runner_kind: NeuralMeshRunnerKind::TrellisCli,
            work_dir: PathBuf::from("/tmp/work"),
            input_image: PathBuf::from("/tmp/work/in.png"),
            output_glb: PathBuf::from("/tmp/work/out.glb"),
            weights_dir: PathBuf::from("/models/trellis2"),
            geometry_res: 512,
            use_bwrap: false,
            timeout: Duration::from_secs(1),
        }
    }

    #[test]
    fn argv_matches_trellis_cpp_cli() {
        let _guard = NEURAL_MESH_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("AOS_NEURAL_MESH_GPU");
        std::env::remove_var("AOS_NEURAL_MESH_ALLOW_CPU");
        let plan = sample_spawn_plan();
        let argv = build_trellis_argv(&plan);
        let mut expected = vec![
            "/opt/trellis-cli".to_string(),
            "/tmp/work/in.png".to_string(),
            "/tmp/work/out.glb".to_string(),
            "--models".to_string(),
            "/models/trellis2".to_string(),
            "--res".to_string(),
            "512".to_string(),
        ];
        #[cfg(windows)]
        {
            expected.push("--gpu".to_string());
            expected.push("0".to_string());
        }
        expected.push("--require-gpu".to_string());
        assert_eq!(argv, expected);

        let adapter_plan = NeuralMeshSpawnPlan {
            runner_bin: PathBuf::from("/pack/adapters/trellis_gguf.sh"),
            runner_kind: NeuralMeshRunnerKind::Adapter,
            work_dir: plan.work_dir.clone(),
            input_image: plan.input_image.clone(),
            output_glb: plan.output_glb.clone(),
            weights_dir: plan.weights_dir.clone(),
            geometry_res: plan.geometry_res,
            use_bwrap: false,
            timeout: plan.timeout,
        };
        let aargv = build_trellis_argv(&adapter_plan);
        assert_eq!(aargv[0], "bash");
        assert_eq!(aargv[1], "/pack/adapters/trellis_gguf.sh");
        assert!(aargv.contains(&"--models".to_string()));
    }

    #[test]
    fn runner_kind_detects_adapter_and_cli() {
        assert_eq!(
            NeuralMeshRunnerKind::detect(Path::new("adapters/trellis_gguf.sh")),
            NeuralMeshRunnerKind::Adapter
        );
        assert_eq!(
            NeuralMeshRunnerKind::detect(Path::new("/opt/trellis-cli")),
            NeuralMeshRunnerKind::TrellisCli
        );
        assert_eq!(
            NeuralMeshRunnerKind::detect(Path::new("bin/local-ai")),
            NeuralMeshRunnerKind::LocalAi
        );
    }

    #[cfg(windows)]
    #[test]
    fn prefer_nvidia_icd_picks_nv_from_semicolon_list() {
        let both = r"C:\AMD\amd_icd64.json;C:\Windows\System32\DriverStore\FileRepository\nv_dispig\nv_dispig.json";
        let picked = prefer_nvidia_vk_icd_filenames(both);
        assert!(picked.to_ascii_lowercase().contains("nv"));
        assert!(!picked.contains(';'));
    }

    #[test]
    fn linux_child_env_uses_unix_path() {
        let env = minimal_neural_mesh_child_env(
            Path::new("/opt/trellis-cli"),
            Path::new("/tmp/aos-neural-work"),
        );
        let path = env.get("PATH").expect("PATH");
        assert!(path.contains("/usr/bin"));
        assert!(path.contains("/opt"));
        assert!(!path.contains(';'));
        assert_eq!(
            env.get("TMPDIR").map(String::as_str),
            Some("/tmp/aos-neural-work")
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_child_env_uses_semicolon_path_and_systemroot() {
        let _guard = NEURAL_MESH_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("AOS_NEURAL_MESH_VK_DEVICE");
        std::env::remove_var("GGML_VK_VISIBLE_DEVICES");
        std::env::remove_var("AOS_NEURAL_MESH_GPU");
        let env = minimal_neural_mesh_child_env(
            Path::new(r"C:\aos\trellis-cli.exe"),
            Path::new(r"C:\Temp\aos-neural-work"),
        );
        let path = env.get("PATH").expect("PATH");
        assert!(path.contains(';'));
        assert!(!path.contains("/usr/bin"));
        assert!(env.contains_key("SystemRoot"));
        assert!(env.contains_key("WINDIR"));
        assert_eq!(
            env.get("TEMP").map(String::as_str),
            Some(r"C:\Temp\aos-neural-work")
        );
        assert_eq!(
            env.get("GGML_VK_VISIBLE_DEVICES").map(String::as_str),
            Some("0")
        );
    }

    #[test]
    fn argv_gpu_from_env_and_allow_cpu_drops_require_gpu() {
        let _guard = NEURAL_MESH_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        std::env::set_var("AOS_NEURAL_MESH_GPU", "1");
        std::env::set_var("AOS_NEURAL_MESH_ALLOW_CPU", "1");
        let argv = build_trellis_argv(&sample_spawn_plan());
        assert!(argv.contains(&"--gpu".to_string()));
        assert!(argv.contains(&"1".to_string()));
        assert!(!argv.contains(&"--require-gpu".to_string()));
        std::env::remove_var("AOS_NEURAL_MESH_GPU");
        std::env::remove_var("AOS_NEURAL_MESH_ALLOW_CPU");
    }

    #[test]
    fn persist_spawn_io_tails_writes_workdir_files() {
        let tmp = std::env::temp_dir().join(format!("aos-spawn-io-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        persist_spawn_io_tails(&tmp, "hello stdout", "hello stderr");
        assert_eq!(
            std::fs::read_to_string(tmp.join("spawn.out")).unwrap(),
            "hello stdout"
        );
        assert_eq!(
            std::fs::read_to_string(tmp.join("spawn.err")).unwrap(),
            "hello stderr"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn format_spawn_failure_includes_stderr_tail() {
        let result = NeuralMeshSpawnResult {
            exit_code: 1,
            stderr_tail: "ggml_vulkan: Found 1 Vulkan devices:\nggml_vulkan: 0 = AMD Radeon\nfail\n".into(),
            argv: vec![],
            isolated_with_bwrap: false,
        };
        let msg = format_spawn_failure(&result, true);
        assert!(msg.contains("exit=1"));
        assert!(msg.contains("output.glb missing"));
        assert!(msg.contains("AMD Radeon"));
        assert!(msg.contains("stderr_tail"));
    }

    #[test]
    fn weights_ready_requires_gguf_or_marker() {
        let tmp = std::env::temp_dir().join(format!(
            "aos-weights-probe-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        assert!(!weights_dir_ready(&tmp));
        std::fs::write(tmp.join(".aos-weights-ready"), b"ci\n").unwrap();
        assert!(weights_dir_ready(&tmp));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
