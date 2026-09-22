//! Isolated neural-mesh Model Pack spawn (Blender-pack pattern).
//!
//! Model:
//! - Fixed argv only — never libre shell from modules
//! - Work directory quarantines prompt/image in + GLB out
//! - Optional Linux `bwrap --unshare-net` when available
//! - Clear env of ambient secrets; keep a minimal PATH
//!
//! Weights (TRELLIS.2 GGUF multi-file sets) and the trellis.cpp / LocalAI
//! runner are **never** vendored into the Akasha git tree. Point
//! `AOS_NEURAL_MESH_PACK` / `AOS_NEURAL_MESH_BIN` / `AOS_NEURAL_MESH_WEIGHTS`
//! at an offline install. See `share/illustration-neural-mesh-pack/README.md`.
//!
//! Gaps (same class as Blender P0-B): Windows AppContainer and macOS
//! sandbox-exec are not wired yet.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// Default wall-clock timeout for a neural mesh child (minutes-scale jobs).
pub const DEFAULT_NEURAL_MESH_TIMEOUT_SECS: u64 = 600;

/// How the host resolves the neural mesh runner / mock path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeuralMeshRunMode {
    /// Prefer real runner when found; otherwise fixture mock when available.
    Auto,
    /// Always use fixture GLB — never spawn trellis / LocalAI.
    Mock,
    /// Require a real runner binary; error if missing.
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

#[derive(Debug, Clone)]
pub struct NeuralMeshPackStatus {
    pub pack_root: Option<PathBuf>,
    pub runner_bin: Option<PathBuf>,
    pub fixture_glb: Option<PathBuf>,
    pub mode: NeuralMeshRunMode,
    pub ready_for_mock: bool,
    pub ready_for_spawn: bool,
}

impl NeuralMeshPackStatus {
    pub fn summary_en(&self) -> String {
        match (&self.pack_root, self.ready_for_mock, self.ready_for_spawn) {
            (None, _, _) => "Neural mesh pack: missing (backend=neural fail-closed)".into(),
            (Some(_), true, true) => format!(
                "Neural mesh pack: ready (mode={}, runner+fixture)",
                self.mode.as_str()
            ),
            (Some(_), true, false) => format!(
                "Neural mesh pack: mock-ready (mode={}, fixture GLB; no runner)",
                self.mode.as_str()
            ),
            (Some(_), false, true) => format!(
                "Neural mesh pack: runner ready (mode={}; no fixture)",
                self.mode.as_str()
            ),
            (Some(_), false, false) => format!(
                "Neural mesh pack: present but incomplete (mode={})",
                self.mode.as_str()
            ),
        }
    }

    pub fn summary_fr(&self) -> String {
        match (&self.pack_root, self.ready_for_mock, self.ready_for_spawn) {
            (None, _, _) => "Pack mesh neural : absent (backend=neural refusé)".into(),
            (Some(_), true, true) => format!(
                "Pack mesh neural : prêt (mode={}, runner+fixture)",
                self.mode.as_str()
            ),
            (Some(_), true, false) => format!(
                "Pack mesh neural : mock prêt (mode={}, GLB fixture ; pas de runner)",
                self.mode.as_str()
            ),
            (Some(_), false, true) => format!(
                "Pack mesh neural : runner prêt (mode={} ; pas de fixture)",
                self.mode.as_str()
            ),
            (Some(_), false, false) => format!(
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

/// Resolve runner: `AOS_NEURAL_MESH_BIN` → pack `bin/trellis-cli` → PATH.
pub fn resolve_runner_bin(pack_root: Option<&Path>) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AOS_NEURAL_MESH_BIN") {
        let pb = PathBuf::from(p.trim());
        if !p.trim().is_empty() && pb.is_file() {
            return Some(pb);
        }
    }
    if let Some(root) = pack_root {
        for rel in [
            "bin/trellis-cli",
            "bin/trellis-cli.exe",
            "trellis-cli",
            "bin/local-ai",
            "bin/localai",
        ] {
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

pub fn probe_pack_status() -> NeuralMeshPackStatus {
    let mode = NeuralMeshRunMode::from_env();
    let pack_root = resolve_pack_root();
    let runner_bin = resolve_runner_bin(pack_root.as_deref());
    let fixture_glb = resolve_fixture_glb(pack_root.as_deref());
    let ready_for_mock = pack_root.is_some() && fixture_glb.is_some();
    let ready_for_spawn = pack_root.is_some() && runner_bin.is_some();
    NeuralMeshPackStatus {
        pack_root,
        runner_bin,
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

#[derive(Debug, Clone)]
pub struct NeuralMeshSpawnPlan {
    pub runner_bin: PathBuf,
    pub work_dir: PathBuf,
    pub input_image: PathBuf,
    pub output_glb: PathBuf,
    pub weights_dir: Option<PathBuf>,
    pub use_bwrap: bool,
    pub timeout: Duration,
}

/// Fixed argv for trellis-cli style: `trellis-cli --input <img> --output <glb> [--weights <dir>]`.
/// Real LocalAI / trellis.cpp flags may differ — adapter scripts in the pack may wrap this.
pub fn build_trellis_argv(plan: &NeuralMeshSpawnPlan) -> Vec<String> {
    let mut argv = vec![
        plan.runner_bin.to_string_lossy().into_owned(),
        "--input".into(),
        plan.input_image.to_string_lossy().into_owned(),
        "--output".into(),
        plan.output_glb.to_string_lossy().into_owned(),
    ];
    if let Some(w) = &plan.weights_dir {
        argv.push("--weights".into());
        argv.push(w.to_string_lossy().into_owned());
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
            if let Some(w) = &plan.weights_dir {
                bw.push("--ro-bind".into());
                bw.push(w.to_string_lossy().into_owned());
                bw.push(w.to_string_lossy().into_owned());
            }
            bw.extend(argv.iter().cloned());
            (PathBuf::from("bwrap"), bw[1..].to_vec(), true)
        } else {
            (
                plan.runner_bin.clone(),
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
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", plan.work_dir.as_os_str())
        .env("TMPDIR", plan.work_dir.as_os_str())
        .current_dir(&plan.work_dir);

    let mut child = cmd.spawn().map_err(|e| format!("spawn: {e}"))?;
    let timeout = plan.timeout;
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stderr = child
                    .stderr
                    .take()
                    .and_then(|mut s| {
                        let mut buf = String::new();
                        let _ = std::io::Read::read_to_string(&mut s, &mut buf);
                        Some(buf)
                    })
                    .unwrap_or_default();
                let tail: String = stderr.chars().rev().take(2000).collect::<String>().chars().rev().collect();
                return Ok(NeuralMeshSpawnResult {
                    exit_code: status.code().unwrap_or(-1),
                    stderr_tail: tail,
                    argv,
                    isolated_with_bwrap: isolated,
                });
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "neural mesh runner timed out after {}s",
                        timeout.as_secs()
                    ));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("wait: {e}")),
        }
    }
}

/// Resolve optional weights directory (never downloaded at generate-time).
pub fn resolve_weights_dir(pack_root: Option<&Path>) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AOS_NEURAL_MESH_WEIGHTS") {
        let pb = PathBuf::from(p.trim());
        if !p.trim().is_empty() && pb.is_dir() {
            return Some(pb);
        }
    }
    if let Some(root) = pack_root {
        let cand = root.join("weights");
        if cand.is_dir() {
            return Some(cand);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_root_resolves_from_crate() {
        let root = resolve_pack_root();
        assert!(root.is_some(), "share/illustration-neural-mesh-pack should exist");
        let fixture = resolve_fixture_glb(root.as_deref());
        assert!(fixture.is_some());
    }

    #[test]
    fn argv_is_fixed_shape() {
        let plan = NeuralMeshSpawnPlan {
            runner_bin: PathBuf::from("/opt/trellis-cli"),
            work_dir: PathBuf::from("/tmp/work"),
            input_image: PathBuf::from("/tmp/work/in.png"),
            output_glb: PathBuf::from("/tmp/work/out.glb"),
            weights_dir: Some(PathBuf::from("/models/trellis2")),
            use_bwrap: false,
            timeout: Duration::from_secs(1),
        };
        let argv = build_trellis_argv(&plan);
        assert_eq!(argv[0], "/opt/trellis-cli");
        assert!(argv.contains(&"--input".into()));
        assert!(argv.contains(&"--output".into()));
        assert!(argv.contains(&"--weights".into()));
    }
}
