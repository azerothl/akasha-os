//! Deterministic SceneGraph → Blender-pack input (ADR 0011).
//!
//! This module emits **Akasha Y-up** data only. The GPL Renderer Pack adapter
//! (`akasha_beauty.py`) performs Y-up → Blender Z-up conversion. Host crates
//! never import `bpy` and never speak Blender axes in core types.

use crate::scene::{NodeKind, SceneGraph, SceneNode};
use serde::Serialize;
use std::collections::BTreeMap;

/// Stable export format version for the Renderer Pack contract.
pub const AKASHA_SCENE_EXPORT_VERSION: u32 = 1;

/// Deterministic render package written into the isolated work directory.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AkashaSceneExport {
    pub format: &'static str,
    pub version: u32,
    /// Always `"y_up_rh"` — Blender adapter must convert.
    pub conventions: &'static str,
    pub width: u32,
    pub height: u32,
    pub pass: String,
    pub active_camera: Option<String>,
    /// Nodes keyed by id in sorted order for byte-stable JSON.
    pub nodes: BTreeMap<String, ExportNode>,
    pub roots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExportNode {
    pub id: String,
    pub name: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub children: Vec<String>,
    pub translation: [f32; 3],
    pub rotation_xyzw: [f32; 4],
    pub scale: [f32; 3],
    pub visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera: Option<ExportCamera>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExportCamera {
    pub focal_mm: f32,
    pub sensor_width_mm: f32,
    pub near: f32,
    pub far: f32,
}

impl AkashaSceneExport {
    pub fn from_scene(
        scene: &SceneGraph,
        width: u32,
        height: u32,
        pass: &str,
    ) -> Result<Self, String> {
        scene.validate().map_err(|e| e.to_string())?;
        let mut nodes = BTreeMap::new();
        let mut ids: Vec<_> = scene.nodes.keys().cloned().collect();
        ids.sort();
        for id in ids {
            let Some(n) = scene.nodes.get(&id) else {
                continue;
            };
            nodes.insert(id, export_node(n));
        }
        let mut roots = scene.roots.clone();
        roots.sort();
        Ok(Self {
            format: "akasha_scene_export",
            version: AKASHA_SCENE_EXPORT_VERSION,
            conventions: "y_up_rh",
            width,
            height,
            pass: pass.to_string(),
            active_camera: scene.active_camera.clone(),
            nodes,
            roots,
        })
    }

    /// Canonical JSON bytes (sorted keys via `BTreeMap` + serde_json compact).
    pub fn to_canonical_json(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(self).map_err(|e| e.to_string())
    }

    /// Stable digest of the export payload (FNV-1a 64-bit, hex).
    pub fn digest_hex(&self) -> Result<String, String> {
        let bytes = self.to_canonical_json()?;
        Ok(format!("{:016x}", fnv1a64(&bytes)))
    }
}

fn export_node(n: &SceneNode) -> ExportNode {
    let kind = match n.kind {
        NodeKind::Empty => "empty",
        NodeKind::MeshBox => "mesh_box",
        NodeKind::Camera => "camera",
        NodeKind::Light => "light",
    };
    ExportNode {
        id: n.id.clone(),
        name: n.name.clone(),
        kind: kind.into(),
        parent: n.parent.clone(),
        children: {
            let mut c = n.children.clone();
            c.sort();
            c
        },
        translation: [n.transform.translation.x, n.transform.translation.y, n.transform.translation.z],
        rotation_xyzw: [
            n.transform.rotation.x,
            n.transform.rotation.y,
            n.transform.rotation.z,
            n.transform.rotation.w,
        ],
        scale: [n.transform.scale.x, n.transform.scale.y, n.transform.scale.z],
        visible: n.visible,
        camera: n.camera.as_ref().map(|c| ExportCamera {
            focal_mm: c.focal_mm,
            sensor_width_mm: c.sensor_width_mm,
            near: c.near,
            far: c.far,
        }),
    }
}

fn fnv1a64(data: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut hash = OFFSET;
    for &b in data {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::SceneGraph;

    #[test]
    fn export_is_byte_stable() {
        let scene = SceneGraph::demo_scene();
        let a = AkashaSceneExport::from_scene(&scene, 64, 48, "beauty")
            .unwrap()
            .to_canonical_json()
            .unwrap();
        let b = AkashaSceneExport::from_scene(&scene, 64, 48, "beauty")
            .unwrap()
            .to_canonical_json()
            .unwrap();
        assert_eq!(a, b);
        let dig = AkashaSceneExport::from_scene(&scene, 64, 48, "beauty")
            .unwrap()
            .digest_hex()
            .unwrap();
        assert_eq!(dig.len(), 16);
    }
}
