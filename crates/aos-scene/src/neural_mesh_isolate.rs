//! Isolated neural-mesh Model Pack spawn (Blender-pack pattern).
//!
//! Model:
//! - Fixed argv only — never libre shell from modules
//! - Work directory quarantines image in + GLB out
//! - Optional Linux `bwrap --unshare-net` when available
//! - Clear env of ambient secrets; keep a minimal PATH
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

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", plan.work_dir.as_os_str())
        .env("TMPDIR", plan.work_dir.as_os_str())
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

    let mut child = cmd.spawn().map_err(|e| format!("spawn: {e}"))?;
    let timeout = plan.timeout;
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stderr = child
                    .stderr
                    .take()
                    .map(|mut s| {
                        let mut buf = String::new();
                        let _ = std::io::Read::read_to_string(&mut s, &mut buf);
                        buf
                    })
                    .unwrap_or_default();
                let tail: String = stderr
                    .chars()
                    .rev()
                    .take(2000)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect();
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

    #[test]
    fn argv_matches_trellis_cpp_cli() {
        let plan = NeuralMeshSpawnPlan {
            runner_bin: PathBuf::from("/opt/trellis-cli"),
            runner_kind: NeuralMeshRunnerKind::TrellisCli,
            work_dir: PathBuf::from("/tmp/work"),
            input_image: PathBuf::from("/tmp/work/in.png"),
            output_glb: PathBuf::from("/tmp/work/out.glb"),
            weights_dir: PathBuf::from("/models/trellis2"),
            geometry_res: 512,
            use_bwrap: false,
            timeout: Duration::from_secs(1),
        };
        let argv = build_trellis_argv(&plan);
        assert_eq!(
            argv,
            vec![
                "/opt/trellis-cli".to_string(),
                "/tmp/work/in.png".to_string(),
                "/tmp/work/out.glb".to_string(),
                "--models".to_string(),
                "/models/trellis2".to_string(),
                "--res".to_string(),
                "512".to_string(),
            ]
        );

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
