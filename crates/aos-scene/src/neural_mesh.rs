//! Neural / AI mesh assist foundation for Illustration Studio.
//!
//! Spec §110–114: AI 3D mesh generation is **not** an MVP dependency. This
//! module ships the fail-closed host surface so Preview can grow a real
//! TRELLIS.2 GGUF Model Pack without a Blender-only path or opaque weights
//! in-tree.
//!
//! | Backend | Status | Behaviour |
//! |---------|--------|-----------|
//! | `stub` (default) | **Real Preview path** | Deterministic procedural MeshBox assembly from EN/FR prompt keywords. No model weights. |
//! | `neural` | **Real pack path** | Pack missing → `BackendUnavailable`. Mock → fixture GLB. Require/auto+ready → isolated `trellis-cli` / adapter spawn (`--models` GGUF dir) → validated GLB → `MeshAsset`. Weights never in git. |
//!
//! All proposals are validated before insertion into the SceneGraph (sole SoT).
//! wgpu edit view and RenderService beauty consume the same graph.

use crate::math::{Quat, Vec3};
use crate::mesh_asset::{insert_mesh_asset, load_gltf_mesh};
use crate::neural_mesh_isolate::{
    probe_pack_status, resolve_fixture_glb, resolve_geometry_res, resolve_runner_bin,
    format_spawn_failure, resolve_weights_dir, spawn_isolated, NeuralMeshRunMode,
    NeuralMeshRunnerKind,
    NeuralMeshSpawnPlan, DEFAULT_NEURAL_MESH_TIMEOUT_SECS,
};
use crate::scene::{NodeKind, SceneError, SceneGraph, SceneNode, Transform};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

/// Cap: run neural / AI mesh assist (fail-closed).
pub const MESH_NEURAL_CAP: &str = "mesh.neural";

/// DeclUI / host service id: propose + apply a mesh assist result into SceneGraph.
pub const MESH_ASSIST_SERVICE: &str = "mesh.assist";

/// Soft limits for Assist proposals (fail-closed when exceeded).
pub const MAX_MESH_PARTS: usize = 24;
pub const MAX_ABS_TRANSLATION: f32 = 50.0;
pub const MAX_ABS_SCALE: f32 = 20.0;
pub const MIN_ABS_SCALE: f32 = 0.01;

/// Which assist backend to invoke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshAssistBackendId {
    /// Deterministic procedural MeshBox assembly (Preview default; no weights).
    Stub,
    /// Opt-in Neural Mesh Model Pack (TRELLIS.2 GGUF path) — fail-closed without pack.
    Neural,
}

impl MeshAssistBackendId {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "stub" | "procedural" | "placeholder" => Some(Self::Stub),
            "neural" | "ai" | "model" | "weights" => Some(Self::Neural),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stub => "stub",
            Self::Neural => "neural",
        }
    }

    pub fn is_stub(self) -> bool {
        matches!(self, Self::Stub)
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum NeuralMeshError {
    #[error("empty prompt")]
    EmptyPrompt,
    #[error("unknown mesh assist backend `{0}`")]
    UnknownBackend(String),
    #[error("neural mesh backend unavailable (no model pack / runner; use backend=stub or install illustration-neural-mesh-pack)")]
    BackendUnavailable,
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("scene: {0}")]
    Scene(String),
}

impl From<SceneError> for NeuralMeshError {
    fn from(e: SceneError) -> Self {
        Self::Scene(e.to_string())
    }
}

/// One MeshBox part in a proposal (metres, ADR 0011).
#[derive(Debug, Clone, PartialEq)]
pub struct MeshPart {
    pub local_id: String,
    pub name: String,
    pub translation: [f32; 3],
    pub scale: [f32; 3],
}

/// Validated mesh assist proposal before SceneGraph insert.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshAssistProposal {
    pub backend: MeshAssistBackendId,
    /// True when the proposal came from the stub / procedural path.
    pub is_stub: bool,
    /// Human-readable template / kind id (`crate`, `column`, `lamp`, … / `mesh_asset`).
    pub kind_id: String,
    pub parts: Vec<MeshPart>,
    /// When set, apply inserts a single [`NodeKind::MeshAsset`] instead of MeshBox parts.
    pub mesh_uri: Option<String>,
    pub notes: Vec<String>,
}

/// Request for [`mesh_assist`].
#[derive(Debug, Clone, PartialEq)]
pub struct MeshAssistRequest {
    pub prompt: String,
    pub parent_id: String,
    pub prefix: String,
    pub backend: MeshAssistBackendId,
    /// Optional conditioning image for `backend=neural` spawn (image→GLB).
    /// Required for `AOS_NEURAL_MESH_MODE=require`. Paths only — never URLs.
    pub image_path: Option<String>,
}

impl Default for MeshAssistRequest {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            parent_id: "root".into(),
            prefix: "mesh_".into(),
            backend: MeshAssistBackendId::Stub,
            image_path: None,
        }
    }
}

/// Result after applying a proposal into the SceneGraph.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshAssistResult {
    pub root_id: String,
    pub created_ids: Vec<String>,
    pub backend: MeshAssistBackendId,
    pub is_stub: bool,
    pub kind_id: String,
    pub notes: Vec<String>,
    pub mesh_uri: Option<String>,
}

/// Fail-closed validation of a proposal (poly/part count, scale, bbox).
pub fn validate_proposal(proposal: &MeshAssistProposal) -> Result<(), NeuralMeshError> {
    if let Some(uri) = &proposal.mesh_uri {
        if uri.trim().is_empty() {
            return Err(NeuralMeshError::Validation("empty mesh_uri".into()));
        }
        if !proposal.parts.is_empty() {
            return Err(NeuralMeshError::Validation(
                "mesh_uri proposals must not also carry MeshBox parts".into(),
            ));
        }
        return Ok(());
    }
    if proposal.parts.is_empty() {
        return Err(NeuralMeshError::Validation("empty parts".into()));
    }
    if proposal.parts.len() > MAX_MESH_PARTS {
        return Err(NeuralMeshError::Validation(format!(
            "part count {} exceeds max {MAX_MESH_PARTS}",
            proposal.parts.len()
        )));
    }
    let mut ids = std::collections::HashSet::new();
    for part in &proposal.parts {
        if part.local_id.trim().is_empty() {
            return Err(NeuralMeshError::Validation("empty part id".into()));
        }
        if !ids.insert(part.local_id.clone()) {
            return Err(NeuralMeshError::Validation(format!(
                "duplicate part id `{}`",
                part.local_id
            )));
        }
        for (i, t) in part.translation.iter().enumerate() {
            if !t.is_finite() || t.abs() > MAX_ABS_TRANSLATION {
                return Err(NeuralMeshError::Validation(format!(
                    "part `{}` translation[{i}] out of range",
                    part.local_id
                )));
            }
        }
        for (i, s) in part.scale.iter().enumerate() {
            if !s.is_finite() || s.abs() < MIN_ABS_SCALE || s.abs() > MAX_ABS_SCALE {
                return Err(NeuralMeshError::Validation(format!(
                    "part `{}` scale[{i}] out of range",
                    part.local_id
                )));
            }
        }
    }
    Ok(())
}

/// Propose a mesh assist result without mutating the scene.
pub fn propose_mesh_assist(req: &MeshAssistRequest) -> Result<MeshAssistProposal, NeuralMeshError> {
    let prompt = req.prompt.trim();
    if prompt.is_empty() {
        return Err(NeuralMeshError::EmptyPrompt);
    }
    match req.backend {
        MeshAssistBackendId::Stub => {
            let proposal = stub_propose(prompt);
            validate_proposal(&proposal)?;
            Ok(proposal)
        }
        MeshAssistBackendId::Neural => {
            let proposal = neural_propose(prompt, req.image_path.as_deref())?;
            validate_proposal(&proposal)?;
            Ok(proposal)
        }
    }
}

/// Propose + insert into SceneGraph (sole SoT). Returns created node ids.
pub fn mesh_assist(
    scene: &mut SceneGraph,
    req: &MeshAssistRequest,
) -> Result<MeshAssistResult, NeuralMeshError> {
    let proposal = propose_mesh_assist(req)?;
    apply_proposal(scene, &proposal, &req.parent_id, &req.prefix)
}

/// Insert a validated proposal under `parent_id` (MeshBox parts or MeshAsset).
pub fn apply_proposal(
    scene: &mut SceneGraph,
    proposal: &MeshAssistProposal,
    parent_id: &str,
    prefix: &str,
) -> Result<MeshAssistResult, NeuralMeshError> {
    validate_proposal(proposal)?;
    let parent = resolve_parent(scene, parent_id)?;
    let prefix = if prefix.trim().is_empty() {
        "mesh_"
    } else {
        prefix
    };

    if let Some(uri) = &proposal.mesh_uri {
        let root_id = unique_id(scene, &format!("{prefix}{}", proposal.kind_id));
        insert_mesh_asset(
            scene,
            &parent,
            &root_id,
            format!("Assist {}", proposal.kind_id),
            uri,
            Transform {
                translation: Vec3::new(0.0, 0.5, 0.0),
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            },
        )
        .map_err(|e| NeuralMeshError::Scene(e.to_string()))?;
        return Ok(MeshAssistResult {
            root_id: root_id.clone(),
            created_ids: vec![root_id],
            backend: proposal.backend,
            is_stub: proposal.is_stub,
            kind_id: proposal.kind_id.clone(),
            notes: proposal.notes.clone(),
            mesh_uri: Some(uri.clone()),
        });
    }

    let root_id = unique_id(scene, &format!("{prefix}{}", proposal.kind_id));
    let mut created = Vec::new();

    let mut root = SceneNode::empty(&root_id, format!("Assist {}", proposal.kind_id));
    root.kind = NodeKind::Empty;
    root.parent = Some(parent.clone());
    attach_child(scene, &parent, root)?;
    created.push(root_id.clone());

    for part in &proposal.parts {
        let id = unique_id(scene, &format!("{root_id}_{}", part.local_id));
        let mut node = SceneNode::empty(&id, &part.name);
        node.kind = NodeKind::MeshBox;
        node.parent = Some(root_id.clone());
        node.transform = Transform {
            translation: Vec3::new(part.translation[0], part.translation[1], part.translation[2]),
            rotation: Quat::IDENTITY,
            scale: Vec3::new(part.scale[0], part.scale[1], part.scale[2]),
        };
        attach_child(scene, &root_id, node)?;
        created.push(id);
    }

    Ok(MeshAssistResult {
        root_id,
        created_ids: created,
        backend: proposal.backend,
        is_stub: proposal.is_stub,
        kind_id: proposal.kind_id.clone(),
        notes: proposal.notes.clone(),
        mesh_uri: None,
    })
}

fn neural_propose(
    prompt: &str,
    image_path: Option<&str>,
) -> Result<MeshAssistProposal, NeuralMeshError> {
    let status = probe_pack_status();
    let Some(pack_root) = status.pack_root.clone() else {
        return Err(NeuralMeshError::BackendUnavailable);
    };

    match status.mode {
        NeuralMeshRunMode::Mock => neural_from_fixture(&pack_root, prompt, true),
        NeuralMeshRunMode::Require => {
            if !status.ready_for_spawn {
                return Err(NeuralMeshError::BackendUnavailable);
            }
            neural_from_spawn(&pack_root, prompt, image_path, true)
        }
        NeuralMeshRunMode::Auto => {
            if status.ready_for_spawn {
                match neural_from_spawn(&pack_root, prompt, image_path, false) {
                    Ok(p) => Ok(p),
                    Err(_) if status.ready_for_mock => neural_from_fixture(&pack_root, prompt, true),
                    Err(_) => Err(NeuralMeshError::BackendUnavailable),
                }
            } else if status.ready_for_mock {
                neural_from_fixture(&pack_root, prompt, true)
            } else {
                Err(NeuralMeshError::BackendUnavailable)
            }
        }
    }
}

fn neural_from_fixture(
    pack_root: &std::path::Path,
    prompt: &str,
    is_mock: bool,
) -> Result<MeshAssistProposal, NeuralMeshError> {
    let Some(fixture) = resolve_fixture_glb(Some(pack_root)) else {
        return Err(NeuralMeshError::BackendUnavailable);
    };
    let uri = fixture.to_string_lossy().into_owned();
    let mut notes = vec![
        if is_mock {
            "backend=neural (mock/fixture GLB; no GGUF weights loaded)".into()
        } else {
            "backend=neural (fixture GLB)".into()
        },
        format!("pack={}", pack_root.display()),
        format!("mesh_uri={uri}"),
        format!("prompt_chars={}", prompt.chars().count()),
        "weights: point AOS_NEURAL_MESH_WEIGHTS at LocalAI-io / ilintar TRELLIS.2 GGUF (never in git)".into(),
    ];
    notes.push(probe_pack_status().summary_en());
    Ok(MeshAssistProposal {
        backend: MeshAssistBackendId::Neural,
        is_stub: false,
        kind_id: "mesh_asset".into(),
        parts: vec![],
        mesh_uri: Some(uri),
        notes,
    })
}

fn neural_from_spawn(
    pack_root: &Path,
    prompt: &str,
    image_path: Option<&str>,
    require_image: bool,
) -> Result<MeshAssistProposal, NeuralMeshError> {
    let Some(runner) = resolve_runner_bin(Some(pack_root)) else {
        return Err(NeuralMeshError::BackendUnavailable);
    };
    let Some(weights) = resolve_weights_dir(Some(pack_root)) else {
        return Err(NeuralMeshError::BackendUnavailable);
    };

    let source_image = match image_path.map(str::trim).filter(|s| !s.is_empty()) {
        Some(p) => {
            let pb = PathBuf::from(p);
            // Fail-closed: local file only — reject URL-looking schemes.
            let lower = p.to_ascii_lowercase();
            if lower.starts_with("http://")
                || lower.starts_with("https://")
                || lower.starts_with("ftp://")
            {
                return Err(NeuralMeshError::Validation(
                    "image_path must be a local file path (no URLs)".into(),
                ));
            }
            if !pb.is_file() {
                return Err(NeuralMeshError::Validation(format!(
                    "image_path not found: {p}"
                )));
            }
            pb
        }
        None if require_image => {
            return Err(NeuralMeshError::Validation(
                "backend=neural require mode needs image_path (local conditioning image)".into(),
            ));
        }
        None => {
            return Err(NeuralMeshError::BackendUnavailable);
        }
    };

    let work = std::env::temp_dir().join(format!(
        "aos-neural-mesh-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&work).map_err(|_| NeuralMeshError::BackendUnavailable)?;
    let ext = source_image
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");
    let input = work.join(format!("input.{ext}"));
    std::fs::copy(&source_image, &input).map_err(|_| NeuralMeshError::BackendUnavailable)?;
    let output = work.join("output.glb");
    let timeout_secs = std::env::var("AOS_NEURAL_MESH_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_NEURAL_MESH_TIMEOUT_SECS);
    let runner_kind = NeuralMeshRunnerKind::detect(&runner);
    let geometry_res = resolve_geometry_res();
    let plan = NeuralMeshSpawnPlan {
        runner_bin: runner,
        runner_kind,
        work_dir: work.clone(),
        input_image: input,
        output_glb: output.clone(),
        weights_dir: weights,
        geometry_res,
        use_bwrap: want_bwrap(),
        timeout: Duration::from_secs(timeout_secs),
    };
    let spawn = spawn_isolated(&plan).map_err(|e| {
        NeuralMeshError::Validation(format!("neural mesh spawn failed: {e}"))
    })?;
    if spawn.exit_code != 0 || !output.is_file() {
        let missing_glb = !output.is_file();
        let detail = format_spawn_failure(&spawn, missing_glb);
        // Keep work dir (spawn.out / spawn.err) for post-mortem diagnosis.
        return Err(NeuralMeshError::Validation(detail));
    }
    // Fail-closed: reject non-mesh / over-budget GLB before SceneGraph insert.
    let mesh = load_gltf_mesh(&output).map_err(|e| {
        let _ = std::fs::remove_dir_all(&work);
        NeuralMeshError::Validation(format!("spawned GLB invalid: {e}"))
    })?;
    let dest = pack_root
        .join("workdir")
        .join(format!("assist_{}.glb", std::process::id()));
    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::copy(&output, &dest).map_err(|_| NeuralMeshError::BackendUnavailable)?;
    let _ = std::fs::remove_dir_all(&work);
    let uri = dest.to_string_lossy().into_owned();
    Ok(MeshAssistProposal {
        backend: MeshAssistBackendId::Neural,
        is_stub: false,
        kind_id: "mesh_asset".into(),
        parts: vec![],
        mesh_uri: Some(uri.clone()),
        notes: vec![
            "backend=neural (trellis.cpp / adapter spawn → validated GLB → MeshAsset)".into(),
            format!("prompt_chars={}", prompt.chars().count()),
            format!("mesh_uri={uri}"),
            format!("runner_kind={}", runner_kind.as_str()),
            format!("triangles={}", mesh.triangle_count),
            format!("argv_len={}", spawn.argv.len()),
            format!("bwrap={}", spawn.isolated_with_bwrap),
            format!("res={geometry_res}"),
        ],
    })
}

fn want_bwrap() -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }
    match std::env::var("AOS_NEURAL_MESH_BWRAP")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "0" | "false" | "off" | "no" => false,
        "1" | "true" | "on" | "yes" => true,
        // Default on for Linux; CI mock adapter opts out via AOS_NEURAL_MESH_BWRAP=0.
        _ => std::env::var("AOS_NEURAL_MESH_ADAPTER_MOCK").ok().as_deref() != Some("1"),
    }
}

fn resolve_parent(scene: &SceneGraph, parent_id: &str) -> Result<String, NeuralMeshError> {
    let p = if parent_id.trim().is_empty() {
        "root"
    } else {
        parent_id.trim()
    };
    if scene.nodes.contains_key(p) {
        return Ok(p.to_string());
    }
    scene
        .roots
        .first()
        .cloned()
        .ok_or_else(|| NeuralMeshError::Scene("scene has no roots".into()))
}

fn unique_id(scene: &SceneGraph, base: &str) -> String {
    if !scene.nodes.contains_key(base) {
        return base.to_string();
    }
    for i in 2..10_000 {
        let cand = format!("{base}_{i}");
        if !scene.nodes.contains_key(&cand) {
            return cand;
        }
    }
    format!("{base}_{}", scene.nodes.len())
}

fn attach_child(scene: &mut SceneGraph, parent: &str, node: SceneNode) -> Result<(), NeuralMeshError> {
    if scene.nodes.contains_key(&node.id) {
        return Err(NeuralMeshError::Scene(format!(
            "duplicate node `{}`",
            node.id
        )));
    }
    let id = node.id.clone();
    scene.nodes.insert(id.clone(), node);
    let p = scene
        .nodes
        .get_mut(parent)
        .ok_or_else(|| NeuralMeshError::Scene(format!("unknown parent `{parent}`")))?;
    if !p.children.contains(&id) {
        p.children.push(id);
    }
    scene
        .validate()
        .map_err(|e| NeuralMeshError::Scene(e.to_string()))?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StubKind {
    Crate,
    Column,
    Lamp,
    Table,
    Block,
}

impl StubKind {
    fn id(self) -> &'static str {
        match self {
            Self::Crate => "crate",
            Self::Column => "column",
            Self::Lamp => "lamp",
            Self::Table => "table",
            Self::Block => "block",
        }
    }

    fn from_prompt(prompt: &str) -> Self {
        let p = prompt.to_lowercase();
        let has = |words: &[&str]| words.iter().any(|w| p.contains(w));
        if has(&[
            "column", "pillar", "colonne", "pilier", "obelisk", "obélisque", "obelisque",
        ]) {
            Self::Column
        } else if has(&["lamp", "lantern", "lampe", "lanterne", "light", "lumière", "lumiere"]) {
            Self::Lamp
        } else if has(&["table", "desk", "bureau", "comptoir", "counter"]) {
            Self::Table
        } else if has(&[
            "crate", "box", "chest", "caisse", "coffre", "boîte", "boite", "carton",
        ]) {
            Self::Crate
        } else {
            Self::Block
        }
    }
}

fn stub_propose(prompt: &str) -> MeshAssistProposal {
    let kind = StubKind::from_prompt(prompt);
    let parts = match kind {
        StubKind::Crate => vec![
            MeshPart {
                local_id: "body".into(),
                name: "Crate body".into(),
                translation: [0.0, 0.35, 0.0],
                scale: [0.7, 0.7, 0.7],
            },
            MeshPart {
                local_id: "lid".into(),
                name: "Crate lid".into(),
                translation: [0.0, 0.72, 0.0],
                scale: [0.72, 0.08, 0.72],
            },
        ],
        StubKind::Column => vec![
            MeshPart {
                local_id: "base".into(),
                name: "Column base".into(),
                translation: [0.0, 0.1, 0.0],
                scale: [0.55, 0.2, 0.55],
            },
            MeshPart {
                local_id: "shaft".into(),
                name: "Column shaft".into(),
                translation: [0.0, 1.1, 0.0],
                scale: [0.35, 1.8, 0.35],
            },
            MeshPart {
                local_id: "cap".into(),
                name: "Column capital".into(),
                translation: [0.0, 2.05, 0.0],
                scale: [0.5, 0.2, 0.5],
            },
        ],
        StubKind::Lamp => vec![
            MeshPart {
                local_id: "stand".into(),
                name: "Lamp stand".into(),
                translation: [0.0, 0.45, 0.0],
                scale: [0.08, 0.9, 0.08],
            },
            MeshPart {
                local_id: "shade".into(),
                name: "Lamp shade".into(),
                translation: [0.0, 1.0, 0.0],
                scale: [0.35, 0.25, 0.35],
            },
        ],
        StubKind::Table => vec![
            MeshPart {
                local_id: "top".into(),
                name: "Table top".into(),
                translation: [0.0, 0.75, 0.0],
                scale: [1.2, 0.08, 0.7],
            },
            MeshPart {
                local_id: "leg_fl".into(),
                name: "Leg FL".into(),
                translation: [-0.5, 0.35, 0.25],
                scale: [0.08, 0.7, 0.08],
            },
            MeshPart {
                local_id: "leg_fr".into(),
                name: "Leg FR".into(),
                translation: [0.5, 0.35, 0.25],
                scale: [0.08, 0.7, 0.08],
            },
            MeshPart {
                local_id: "leg_bl".into(),
                name: "Leg BL".into(),
                translation: [-0.5, 0.35, -0.25],
                scale: [0.08, 0.7, 0.08],
            },
            MeshPart {
                local_id: "leg_br".into(),
                name: "Leg BR".into(),
                translation: [0.5, 0.35, -0.25],
                scale: [0.08, 0.7, 0.08],
            },
        ],
        StubKind::Block => vec![MeshPart {
            local_id: "body".into(),
            name: "Assist block".into(),
            translation: [0.0, 0.4, 0.0],
            scale: [0.8, 0.8, 0.8],
        }],
    };

    MeshAssistProposal {
        backend: MeshAssistBackendId::Stub,
        is_stub: true,
        kind_id: kind.id().into(),
        parts,
        mesh_uri: None,
        notes: vec![
            "backend=stub (procedural MeshBox; no neural weights)".into(),
            format!("prompt_kind={}", kind.id()),
        ],
    }
}

/// Re-export pack probe for DeclUI / host status chrome.
pub use crate::neural_mesh_isolate::probe_pack_status as neural_mesh_pack_status;
pub use crate::neural_mesh_isolate::NeuralMeshPackStatus;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::neural_mesh_isolate::NEURAL_MESH_ENV_LOCK;
    use crate::scene::SceneGraph;
    use std::path::PathBuf;

    struct EnvRestore {
        pack: Option<String>,
        mode: Option<String>,
    }

    impl EnvRestore {
        fn capture() -> Self {
            Self {
                pack: std::env::var("AOS_NEURAL_MESH_PACK").ok(),
                mode: std::env::var("AOS_NEURAL_MESH_MODE").ok(),
            }
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            match &self.pack {
                Some(v) => std::env::set_var("AOS_NEURAL_MESH_PACK", v),
                None => std::env::remove_var("AOS_NEURAL_MESH_PACK"),
            }
            match &self.mode {
                Some(v) => std::env::set_var("AOS_NEURAL_MESH_MODE", v),
                None => std::env::remove_var("AOS_NEURAL_MESH_MODE"),
            }
        }
    }

    #[test]
    fn stub_crate_from_en_fr_prompts() {
        for prompt in ["old wooden crate", "une caisse antique", "boîte en bois"] {
            let req = MeshAssistRequest {
                prompt: prompt.into(),
                ..Default::default()
            };
            let p = propose_mesh_assist(&req).expect("propose");
            assert!(p.is_stub);
            assert_eq!(p.backend, MeshAssistBackendId::Stub);
            assert_eq!(p.kind_id, "crate");
            assert!(p.parts.len() >= 2);
        }
    }

    #[test]
    fn neural_backend_fail_closed_without_pack() {
        let _lock = NEURAL_MESH_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _restore = EnvRestore::capture();
        // Force missing pack via env override to a non-existent path.
        std::env::set_var("AOS_NEURAL_MESH_PACK", "/tmp/aos-missing-neural-pack-spike");
        std::env::set_var("AOS_NEURAL_MESH_MODE", "mock");
        let req = MeshAssistRequest {
            prompt: "a marble statue".into(),
            backend: MeshAssistBackendId::Neural,
            ..Default::default()
        };
        let err = propose_mesh_assist(&req).unwrap_err();
        assert_eq!(err, NeuralMeshError::BackendUnavailable);
    }

    #[test]
    fn neural_mock_inserts_mesh_asset_when_pack_present() {
        let _lock = NEURAL_MESH_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _restore = EnvRestore::capture();
        let pack = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../share/illustration-neural-mesh-pack");
        assert!(pack.is_dir());
        std::env::set_var("AOS_NEURAL_MESH_PACK", &pack);
        std::env::set_var("AOS_NEURAL_MESH_MODE", "mock");
        let mut scene = SceneGraph::demo_scene();
        let req = MeshAssistRequest {
            prompt: "fixture cube prop".into(),
            parent_id: "root".into(),
            prefix: "neural_".into(),
            backend: MeshAssistBackendId::Neural,
            image_path: None,
        };
        let res = mesh_assist(&mut scene, &req).expect("neural mock assist");
        assert!(!res.is_stub);
        assert_eq!(res.kind_id, "mesh_asset");
        assert!(res.mesh_uri.is_some());
        let node = scene.nodes.get(&res.root_id).expect("node");
        assert_eq!(node.kind, NodeKind::MeshAsset);
        assert!(node.mesh_uri.is_some());
    }

    #[test]
    fn neural_require_fail_closed_without_weights() {
        let _lock = NEURAL_MESH_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _restore = EnvRestore::capture();
        let pack = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../share/illustration-neural-mesh-pack");
        std::env::set_var("AOS_NEURAL_MESH_PACK", &pack);
        std::env::set_var("AOS_NEURAL_MESH_MODE", "require");
        std::env::remove_var("AOS_NEURAL_MESH_WEIGHTS");
        std::env::remove_var("AOS_NEURAL_MESH_BIN");
        let err = propose_mesh_assist(&MeshAssistRequest {
            prompt: "chair".into(),
            backend: MeshAssistBackendId::Neural,
            image_path: Some("/tmp/does-not-matter.png".into()),
            ..Default::default()
        })
        .unwrap_err();
        assert_eq!(err, NeuralMeshError::BackendUnavailable);
    }

    #[cfg(unix)]
    #[test]
    fn neural_gguf_adapter_spawn_inserts_validated_mesh_asset() {
        let _lock = NEURAL_MESH_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let prev_bin = std::env::var("AOS_NEURAL_MESH_BIN").ok();
        let prev_weights = std::env::var("AOS_NEURAL_MESH_WEIGHTS").ok();
        let _restore = EnvRestore::capture();

        let pack = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../share/illustration-neural-mesh-pack");
        let adapter = pack.join("adapters/trellis_gguf.sh");
        assert!(adapter.is_file(), "pack adapter missing");

        let tmp = std::env::temp_dir().join(format!(
            "aos-neural-spawn-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("weights")).unwrap();
        std::fs::write(tmp.join("weights/.aos-weights-ready"), b"ci\n").unwrap();
        // Tiny valid PNG as conditioning image.
        let png = [
            0x89u8, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08,
            0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x05, 0xFE,
            0xD4, 0xEF, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        let img = tmp.join("cond.png");
        std::fs::write(&img, png).unwrap();

        std::env::set_var("AOS_NEURAL_MESH_PACK", &pack);
        std::env::set_var("AOS_NEURAL_MESH_BIN", &adapter);
        std::env::set_var("AOS_NEURAL_MESH_WEIGHTS", tmp.join("weights"));
        std::env::set_var("AOS_NEURAL_MESH_MODE", "require");
        // Force mock path inside adapter (no real trellis-cli / GGUF in CI).
        std::env::set_var("AOS_NEURAL_MESH_ADAPTER_MOCK", "1");

        let mut scene = SceneGraph::demo_scene();
        let res = mesh_assist(
            &mut scene,
            &MeshAssistRequest {
                prompt: "bookstore chair".into(),
                parent_id: "root".into(),
                prefix: "gguf_".into(),
                backend: MeshAssistBackendId::Neural,
                image_path: Some(img.to_string_lossy().into_owned()),
            },
        )
        .expect("gguf adapter spawn");
        assert!(!res.is_stub);
        assert_eq!(res.kind_id, "mesh_asset");
        assert!(res.mesh_uri.as_ref().is_some_and(|u| u.ends_with(".glb")));
        let node = scene.nodes.get(&res.root_id).expect("node");
        assert_eq!(node.kind, NodeKind::MeshAsset);

        std::env::remove_var("AOS_NEURAL_MESH_ADAPTER_MOCK");
        match prev_bin {
            Some(v) => std::env::set_var("AOS_NEURAL_MESH_BIN", v),
            None => std::env::remove_var("AOS_NEURAL_MESH_BIN"),
        }
        match prev_weights {
            Some(v) => std::env::set_var("AOS_NEURAL_MESH_WEIGHTS", v),
            None => std::env::remove_var("AOS_NEURAL_MESH_WEIGHTS"),
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn unknown_backend_parse() {
        assert!(MeshAssistBackendId::parse("blender").is_none());
        assert_eq!(
            MeshAssistBackendId::parse("neural"),
            Some(MeshAssistBackendId::Neural)
        );
    }

    #[test]
    fn apply_inserts_mesh_boxes_into_scenegraph() {
        let mut scene = SceneGraph::demo_scene();
        let req = MeshAssistRequest {
            prompt: "stone column".into(),
            parent_id: "root".into(),
            prefix: "assist_".into(),
            backend: MeshAssistBackendId::Stub,
            image_path: None,
        };
        let res = mesh_assist(&mut scene, &req).expect("assist");
        assert!(res.is_stub);
        assert_eq!(res.kind_id, "column");
        assert!(scene.nodes.contains_key(&res.root_id));
        assert!(res.created_ids.len() >= 4); // root + 3 parts
        let shaft = format!("{}_shaft", res.root_id);
        let node = scene.nodes.get(&shaft).expect("shaft");
        assert_eq!(node.kind, NodeKind::MeshBox);
    }

    #[test]
    fn validation_rejects_oversized_scale() {
        let bad = MeshAssistProposal {
            backend: MeshAssistBackendId::Stub,
            is_stub: true,
            kind_id: "bad".into(),
            parts: vec![MeshPart {
                local_id: "x".into(),
                name: "x".into(),
                translation: [0.0, 0.0, 0.0],
                scale: [100.0, 1.0, 1.0],
            }],
            mesh_uri: None,
            notes: vec![],
        };
        assert!(matches!(
            validate_proposal(&bad),
            Err(NeuralMeshError::Validation(_))
        ));
    }

    #[test]
    fn empty_prompt_rejected() {
        let err = propose_mesh_assist(&MeshAssistRequest::default()).unwrap_err();
        assert_eq!(err, NeuralMeshError::EmptyPrompt);
    }
}
