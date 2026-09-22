//! Neural / AI mesh assist foundation for Illustration Studio.
//!
//! Spec §110–114: AI 3D mesh generation is **not** an MVP dependency. This
//! module ships the fail-closed host surface so Preview can grow a real
//! backend later without a Blender-only path or opaque weights in-tree.
//!
//! | Backend | Status | Behaviour |
//! |---------|--------|-----------|
//! | `stub` (default) | **Real Preview path** | Deterministic procedural MeshBox assembly from EN/FR prompt keywords. No model weights. |
//! | `neural` | **Stub interface only** | Fail-closed (`BackendUnavailable`) until model infra exists. |
//!
//! All proposals are validated (part count, scale, bbox) before insertion into
//! the SceneGraph (sole SoT). wgpu edit view and RenderService beauty consume
//! the same graph — this module never speaks Blender.

use crate::math::{Quat, Vec3};
use crate::scene::{NodeKind, SceneError, SceneGraph, SceneNode, Transform};
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
    /// Future neural weights path — unavailable until model infra ships.
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
    #[error("neural mesh backend unavailable (no model infra; use backend=stub)")]
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
    /// Human-readable template / kind id (`crate`, `column`, `lamp`, …).
    pub kind_id: String,
    pub parts: Vec<MeshPart>,
    pub notes: Vec<String>,
}

/// Request for [`mesh_assist`].
#[derive(Debug, Clone, PartialEq)]
pub struct MeshAssistRequest {
    pub prompt: String,
    pub parent_id: String,
    pub prefix: String,
    pub backend: MeshAssistBackendId,
}

impl Default for MeshAssistRequest {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            parent_id: "root".into(),
            prefix: "mesh_".into(),
            backend: MeshAssistBackendId::Stub,
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
}

/// Fail-closed validation of a proposal (poly/part count, scale, bbox).
pub fn validate_proposal(proposal: &MeshAssistProposal) -> Result<(), NeuralMeshError> {
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
        MeshAssistBackendId::Neural => Err(NeuralMeshError::BackendUnavailable),
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

/// Insert a validated proposal as MeshBox children under `parent_id`.
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
    })
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
        notes: vec![
            "backend=stub (procedural MeshBox; no neural weights)".into(),
            format!("prompt_kind={}", kind.id()),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::SceneGraph;

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
    fn neural_backend_fail_closed() {
        let req = MeshAssistRequest {
            prompt: "a marble statue".into(),
            backend: MeshAssistBackendId::Neural,
            ..Default::default()
        };
        let err = propose_mesh_assist(&req).unwrap_err();
        assert_eq!(err, NeuralMeshError::BackendUnavailable);
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
