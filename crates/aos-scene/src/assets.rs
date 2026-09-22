//! Minimal Illustration asset pack format + SceneGraph instantiate (ADR 0011).

use crate::math::{Quat, Vec3};
use crate::scene::{NodeKind, SceneError, SceneGraph, SceneNode, Transform};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Logical asset tree for Illustration Studio packs (fail-closed cap prefix).
pub const ILLUSTRATION_ASSETS_PREFIX: &str = "/assets/illustration/";

/// Cap: read illustration asset packs under `/assets/illustration/**`.
pub const ASSET_ILLUSTRATION_READ_CAP: &str = "asset.read:/assets/illustration/**";

/// Current on-disk pack format version.
pub const ASSET_PACK_FORMAT_VERSION: u32 = 1;

/// Embedded primitives pack (offline; no FS required for tests / default host).
pub const EMBEDDED_PRIMITIVES_PACK_YAML: &str =
    include_str!("../../../share/assets/illustration/primitives/pack.yaml");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetPack {
    pub format_version: u32,
    pub pack_id: String,
    #[serde(default = "adr_default")]
    pub conventions: String,
    #[serde(default)]
    pub entries: Vec<AssetEntry>,
}

fn adr_default() -> String {
    "ADR-0011".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AssetEntry {
    /// Unit box mesh (metres), scale applied at instantiate.
    MeshBox {
        id: String,
        name: String,
        #[serde(default = "one_scale")]
        size: [f32; 3],
    },
    /// Hierarchical placeholder composed of MeshBox nodes (humanoid / prop).
    Prefab {
        id: String,
        name: String,
        nodes: Vec<PrefabNode>,
    },
}

fn one_scale() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrefabNode {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub kind: PrefabNodeKind,
    #[serde(default)]
    pub translation: [f32; 3],
    #[serde(default = "one_scale")]
    pub scale: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PrefabNodeKind {
    #[default]
    Empty,
    MeshBox,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AssetError {
    #[error("unsupported pack format_version {0}")]
    UnsupportedVersion(u32),
    #[error("unknown asset `{0}`")]
    UnknownAsset(String),
    #[error("duplicate node `{0}`")]
    DuplicateNode(String),
    #[error("path outside illustration assets tree: {0}")]
    PathDenied(String),
    #[error("yaml: {0}")]
    Yaml(String),
    #[error("scene: {0}")]
    Scene(String),
}

impl AssetPack {
    pub fn validate(&self) -> Result<(), AssetError> {
        if self.format_version != ASSET_PACK_FORMAT_VERSION {
            return Err(AssetError::UnsupportedVersion(self.format_version));
        }
        let mut ids = std::collections::HashSet::new();
        for e in &self.entries {
            let id = e.id();
            if !ids.insert(id.to_string()) {
                return Err(AssetError::DuplicateNode(id.into()));
            }
        }
        Ok(())
    }

    pub fn get(&self, asset_id: &str) -> Option<&AssetEntry> {
        self.entries.iter().find(|e| e.id() == asset_id)
    }
}

impl AssetEntry {
    pub fn id(&self) -> &str {
        match self {
            Self::MeshBox { id, .. } | Self::Prefab { id, .. } => id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::MeshBox { name, .. } | Self::Prefab { name, .. } => name,
        }
    }
}

/// Fail-closed: pack paths must stay under `/assets/illustration/`.
pub fn assert_asset_path(path: &str) -> Result<(), AssetError> {
    if path.starts_with(ILLUSTRATION_ASSETS_PREFIX) && !path.contains("..") {
        Ok(())
    } else {
        Err(AssetError::PathDenied(path.into()))
    }
}

pub fn load_asset_pack_yaml(yaml: &str) -> Result<AssetPack, AssetError> {
    let pack: AssetPack =
        serde_yaml::from_str(yaml).map_err(|e| AssetError::Yaml(e.to_string()))?;
    pack.validate()?;
    Ok(pack)
}

pub fn embedded_primitives_pack() -> Result<AssetPack, AssetError> {
    load_asset_pack_yaml(EMBEDDED_PRIMITIVES_PACK_YAML)
}

/// Result of instantiating an asset into a SceneGraph.
#[derive(Debug, Clone, PartialEq)]
pub struct InstantiateResult {
    pub root_id: String,
    pub created_ids: Vec<String>,
}

/// Instantiate `asset_id` from `pack` under `parent_id` (or as a new root child of first root).
pub fn instantiate_asset(
    scene: &mut SceneGraph,
    pack: &AssetPack,
    asset_id: &str,
    parent_id: Option<&str>,
    instance_prefix: &str,
) -> Result<InstantiateResult, AssetError> {
    pack.validate()?;
    let entry = pack
        .get(asset_id)
        .ok_or_else(|| AssetError::UnknownAsset(asset_id.into()))?;

    let parent = resolve_parent(scene, parent_id)?;
    match entry {
        AssetEntry::MeshBox { name, size, .. } => {
            let id = unique_id(scene, &format!("{instance_prefix}box"));
            let mut node = SceneNode::empty(&id, name);
            node.kind = NodeKind::MeshBox;
            node.parent = Some(parent.clone());
            node.transform = Transform {
                translation: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                scale: Vec3::new(size[0], size[1], size[2]),
            };
            attach_child(scene, &parent, node)
                .map_err(|e| AssetError::Scene(e.to_string()))?;
            Ok(InstantiateResult {
                root_id: id.clone(),
                created_ids: vec![id],
            })
        }
        AssetEntry::Prefab { name, nodes, .. } => {
            let mut id_map: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            let mut created = Vec::new();
            // First pass: allocate unique ids
            for n in nodes {
                let nid = unique_id(scene, &format!("{instance_prefix}{}", n.id));
                id_map.insert(n.id.clone(), nid.clone());
                created.push(nid);
            }
            // Prefab root = node without parent, or first node.
            let prefab_root_local = nodes
                .iter()
                .find(|n| n.parent.is_none())
                .map(|n| n.id.as_str())
                .unwrap_or_else(|| nodes[0].id.as_str());
            let prefab_root_id = id_map[prefab_root_local].clone();

            for n in nodes {
                let id = id_map[&n.id].clone();
                let mut node = SceneNode::empty(&id, if n.id == prefab_root_local {
                    name.clone()
                } else {
                    n.name.clone()
                });
                node.kind = match n.kind {
                    PrefabNodeKind::Empty => NodeKind::Empty,
                    PrefabNodeKind::MeshBox => NodeKind::MeshBox,
                };
                node.transform = Transform {
                    translation: Vec3::new(n.translation[0], n.translation[1], n.translation[2]),
                    rotation: Quat::IDENTITY,
                    scale: Vec3::new(n.scale[0], n.scale[1], n.scale[2]),
                };
                let parent_for_node = match &n.parent {
                    None => parent.clone(),
                    Some(p) => id_map
                        .get(p)
                        .cloned()
                        .ok_or_else(|| AssetError::Scene(format!("missing prefab parent `{p}`")))?,
                };
                node.parent = Some(parent_for_node.clone());
                // children filled as we attach
                attach_child(scene, &parent_for_node, node)
                    .map_err(|e| AssetError::Scene(e.to_string()))?;
            }
            let _ = prefab_root_id;
            Ok(InstantiateResult {
                root_id: id_map[prefab_root_local].clone(),
                created_ids: created,
            })
        }
    }
}

fn resolve_parent(scene: &SceneGraph, parent_id: Option<&str>) -> Result<String, AssetError> {
    if let Some(p) = parent_id {
        if scene.nodes.contains_key(p) {
            return Ok(p.to_string());
        }
        return Err(AssetError::Scene(format!("unknown parent `{p}`")));
    }
    scene
        .roots
        .first()
        .cloned()
        .ok_or_else(|| AssetError::Scene("scene has no roots".into()))
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

fn attach_child(scene: &mut SceneGraph, parent: &str, node: SceneNode) -> Result<(), SceneError> {
    if scene.nodes.contains_key(&node.id) {
        return Err(SceneError::DuplicateNode(node.id));
    }
    let id = node.id.clone();
    scene.nodes.insert(id.clone(), node);
    let p = scene
        .nodes
        .get_mut(parent)
        .ok_or_else(|| SceneError::UnknownNode(parent.into()))?;
    if !p.children.contains(&id) {
        p.children.push(id);
    }
    scene.validate()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{SceneGraph, SceneNode};

    #[test]
    fn embedded_pack_loads_and_instantiates_humanoid() {
        let pack = embedded_primitives_pack().expect("pack");
        assert!(pack.get("humanoid.placeholder").is_some());
        assert!(pack.get("humanoid.slim").is_some());
        assert!(pack.get("quadruped.cat").is_some());
        assert!(pack.get("prop.box").is_some());
        assert!(pack.get("prop.ground").is_some());
        assert!(pack.get("prop.pedestal").is_some());
        assert!(pack.get("prop.counter").is_some());
        assert!(pack.get("prop.bookshelf").is_some());
        assert!(pack.get("prop.door").is_some());
        assert!(pack.get("prop.chair").is_some());
        assert!(pack.get("scene.starter").is_some());
        let mut scene = SceneGraph::demo_scene();
        let r = instantiate_asset(
            &mut scene,
            &pack,
            "humanoid.placeholder",
            Some("root"),
            "h_",
        )
        .expect("instantiate");
        assert!(scene.nodes.contains_key(&r.root_id));
        assert!(scene.nodes.values().any(|n| {
            n.kind == NodeKind::MeshBox && (n.name.contains("Chest") || n.id.contains("chest") || n.name.contains("Torso") || n.id.contains("torso"))
        }));
        scene.validate().expect("valid");
    }

    #[test]
    fn starter_prefab_instantiates_ground_and_box() {
        let pack = embedded_primitives_pack().expect("pack");
        // Empty root-only for a clean instantiate check.
        let mut scene = SceneGraph {
            nodes: {
                let mut m = std::collections::HashMap::new();
                let root = SceneNode::empty("root", "Scene");
                m.insert(root.id.clone(), root);
                m
            },
            roots: vec!["root".into()],
            active_camera: None,
        };
        let r = instantiate_asset(&mut scene, &pack, "scene.starter", Some("root"), "s_")
            .expect("starter");
        assert!(scene.nodes.contains_key(&r.root_id));
        assert!(scene.nodes.values().any(|n| n.name == "Ground"));
        assert!(scene.nodes.values().any(|n| n.name == "Box"));
        scene.validate().expect("valid");
    }

    #[test]
    fn asset_path_fail_closed() {
        assert!(assert_asset_path("/assets/illustration/primitives/pack.yaml").is_ok());
        assert!(assert_asset_path("/etc/passwd").is_err());
        assert!(assert_asset_path("/assets/illustration/../x").is_err());
    }
}
