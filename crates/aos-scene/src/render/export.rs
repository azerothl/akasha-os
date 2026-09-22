//! Deterministic SceneGraph → Blender-pack input (ADR 0011).
//!
//! This module emits **Akasha Y-up** data only. The GPL Renderer Pack adapter
//! (`akasha_beauty.py`) performs Y-up → Blender Z-up conversion. Host crates
//! never import `bpy` and never speak Blender axes in core types.

use crate::scene::{NodeKind, SceneGraph, SceneNode};
use crate::style::ResolvedStyle;
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
    /// Optional NPR style payload for Renderer Pack adapters (Sketch / Pencil / Ink).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<ExportStyle>,
    pub active_camera: Option<String>,
    /// Nodes keyed by id in sorted order for byte-stable JSON.
    pub nodes: BTreeMap<String, ExportNode>,
    pub roots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExportStyle {
    pub id: String,
    pub family: String,
    pub line_width: f32,
    pub jitter: f32,
    pub opacity: f32,
    pub shading: String,
    pub contrast: f32,
    pub paper_tint: [u8; 3],
    pub paper_texture: String,
}

impl ExportStyle {
    pub fn from_resolved(s: &ResolvedStyle) -> Self {
        Self {
            id: s.id.clone(),
            family: s.family.as_str().into(),
            line_width: s.line_width,
            jitter: s.jitter,
            opacity: s.opacity,
            shading: s.shading.clone(),
            contrast: s.contrast,
            paper_tint: s.paper_tint,
            paper_texture: s.paper_texture.clone(),
        }
    }
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
    pub mesh_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera: Option<ExportCamera>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light: Option<ExportLight>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExportCamera {
    pub focal_mm: f32,
    pub sensor_width_mm: f32,
    pub near: f32,
    pub far: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExportLight {
    #[serde(rename = "type")]
    pub light_type: String,
    pub intensity: f32,
    pub color: [f32; 3],
    pub range: f32,
    pub spot_angle_rad: f32,
}

impl AkashaSceneExport {
    pub fn from_scene(
        scene: &SceneGraph,
        width: u32,
        height: u32,
        pass: &str,
    ) -> Result<Self, String> {
        Self::from_scene_with_style(scene, width, height, pass, None)
    }

    pub fn from_scene_with_style(
        scene: &SceneGraph,
        width: u32,
        height: u32,
        pass: &str,
        style: Option<&ResolvedStyle>,
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
            style: style.map(ExportStyle::from_resolved),
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
        NodeKind::MeshAsset => "mesh_asset",
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
        mesh_uri: n.mesh_uri.clone(),
        camera: n.camera.as_ref().map(|c| ExportCamera {
            focal_mm: c.focal_mm,
            sensor_width_mm: c.sensor_width_mm,
            near: c.near,
            far: c.far,
        }),
        light: n.light.as_ref().map(|l| ExportLight {
            light_type: crate::light::light_type_as_str(l.light_type).into(),
            intensity: l.intensity,
            color: l.color,
            range: l.range,
            spot_angle_rad: l.spot_angle_rad,
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
