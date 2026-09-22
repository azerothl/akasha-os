//! MeshAsset — load glTF/GLB into a CPU triangle mesh for SceneGraph.
//!
//! ADR 0011: Akasha is Y-up RH. glTF is also Y-up; we import positions as-is
//! (no Blender Z-up conversion in the host). Textures / materials are out of
//! scope for this foundation spike — positions + normals + indices only.

use crate::math::Vec3;
use crate::scene::{NodeKind, SceneError, SceneGraph, SceneNode, Transform};
use std::path::Path;
use thiserror::Error;

/// Soft caps for imported meshes (fail-closed).
pub const MAX_MESH_TRIANGLES: usize = 200_000;
pub const MAX_MESH_VERTICES: usize = 400_000;

#[derive(Debug, Error, PartialEq)]
pub enum MeshAssetError {
    #[error("io: {0}")]
    Io(String),
    #[error("gltf: {0}")]
    Gltf(String),
    #[error("validation: {0}")]
    Validation(String),
    #[error("scene: {0}")]
    Scene(String),
}

impl From<SceneError> for MeshAssetError {
    fn from(e: SceneError) -> Self {
        Self::Scene(e.to_string())
    }
}

/// CPU-side triangle mesh (edit viewport + validation).
#[derive(Debug, Clone, PartialEq)]
pub struct CpuTriangleMesh {
    /// Interleaved position + normal (xyz + nxyz), 6 floats per vertex.
    pub interleaved: Vec<f32>,
    pub indices: Vec<u32>,
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
    pub triangle_count: usize,
}

impl CpuTriangleMesh {
    pub fn vertex_count(&self) -> usize {
        self.interleaved.len() / 6
    }

    /// Axis-aligned half-extents from bounds (for pick / MeshBox proxy).
    pub fn aabb_half_extents(&self) -> Vec3 {
        Vec3::new(
            ((self.bounds_max.x - self.bounds_min.x) * 0.5).max(0.01),
            ((self.bounds_max.y - self.bounds_min.y) * 0.5).max(0.01),
            ((self.bounds_max.z - self.bounds_min.z) * 0.5).max(0.01),
        )
    }

    pub fn aabb_center(&self) -> Vec3 {
        Vec3::new(
            (self.bounds_min.x + self.bounds_max.x) * 0.5,
            (self.bounds_min.y + self.bounds_max.y) * 0.5,
            (self.bounds_min.z + self.bounds_max.z) * 0.5,
        )
    }
}

/// Load a `.glb` / `.gltf` file into a CPU mesh (first scene, all primitives).
pub fn load_gltf_mesh(path: &Path) -> Result<CpuTriangleMesh, MeshAssetError> {
    let (doc, buffers, _images) = gltf::import(path).map_err(|e| MeshAssetError::Gltf(e.to_string()))?;
    let mut interleaved = Vec::new();
    let mut indices = Vec::new();
    let mut bmin = [f32::INFINITY; 3];
    let mut bmax = [f32::NEG_INFINITY; 3];

    for mesh in doc.meshes() {
        for prim in mesh.primitives() {
            let reader = prim.reader(|buf| buffers.get(buf.index()).map(|b| &*b.0));
            let positions: Vec<[f32; 3]> = reader
                .read_positions()
                .ok_or_else(|| MeshAssetError::Gltf("primitive missing POSITION".into()))?
                .collect();
            if positions.is_empty() {
                continue;
            }
            let normals: Vec<[f32; 3]> = if let Some(n) = reader.read_normals() {
                n.collect()
            } else {
                vec![[0.0, 1.0, 0.0]; positions.len()]
            };
            let base = (interleaved.len() / 6) as u32;
            for (i, p) in positions.iter().enumerate() {
                let n = normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
                interleaved.extend_from_slice(&[p[0], p[1], p[2], n[0], n[1], n[2]]);
                for c in 0..3 {
                    bmin[c] = bmin[c].min(p[c]);
                    bmax[c] = bmax[c].max(p[c]);
                }
            }
            if let Some(idx) = reader.read_indices() {
                for i in idx.into_u32() {
                    indices.push(base + i);
                }
            } else {
                for i in 0..positions.len() as u32 {
                    indices.push(base + i);
                }
            }
        }
    }

    if interleaved.is_empty() || indices.len() < 3 {
        return Err(MeshAssetError::Validation("empty mesh".into()));
    }
    let tri = indices.len() / 3;
    if tri > MAX_MESH_TRIANGLES {
        return Err(MeshAssetError::Validation(format!(
            "triangle count {tri} exceeds max {MAX_MESH_TRIANGLES}"
        )));
    }
    if interleaved.len() / 6 > MAX_MESH_VERTICES {
        return Err(MeshAssetError::Validation(format!(
            "vertex count exceeds max {MAX_MESH_VERTICES}"
        )));
    }
    if !bmin.iter().all(|v| v.is_finite()) || !bmax.iter().all(|v| v.is_finite()) {
        return Err(MeshAssetError::Validation("non-finite bounds".into()));
    }

    Ok(CpuTriangleMesh {
        interleaved,
        indices,
        bounds_min: Vec3::new(bmin[0], bmin[1], bmin[2]),
        bounds_max: Vec3::new(bmax[0], bmax[1], bmax[2]),
        triangle_count: tri,
    })
}

/// Insert a MeshAsset node under `parent_id` pointing at `mesh_uri`.
pub fn insert_mesh_asset(
    scene: &mut SceneGraph,
    parent_id: &str,
    id: impl Into<String>,
    name: impl Into<String>,
    mesh_uri: impl Into<String>,
    transform: Transform,
) -> Result<String, MeshAssetError> {
    let parent = if parent_id.trim().is_empty() {
        "root"
    } else {
        parent_id.trim()
    };
    if !scene.nodes.contains_key(parent) {
        return Err(MeshAssetError::Scene(format!("unknown parent `{parent}`")));
    }
    let id = id.into();
    if scene.nodes.contains_key(&id) {
        return Err(MeshAssetError::Scene(format!("duplicate node `{id}`")));
    }
    let mut node = SceneNode::empty(&id, name);
    node.kind = NodeKind::MeshAsset;
    node.parent = Some(parent.to_string());
    node.mesh_uri = Some(mesh_uri.into());
    node.transform = transform;
    scene.nodes.insert(id.clone(), node);
    let p = scene
        .nodes
        .get_mut(parent)
        .ok_or_else(|| MeshAssetError::Scene(format!("unknown parent `{parent}`")))?;
    if !p.children.contains(&id) {
        p.children.push(id.clone());
    }
    scene.validate()?;
    Ok(id)
}

/// Resolve a mesh URI to a filesystem path (pack-relative, CWD-relative, or absolute).
pub fn resolve_mesh_uri(uri: &str, search_roots: &[&Path]) -> Option<std::path::PathBuf> {
    let u = uri.trim();
    if u.is_empty() {
        return None;
    }
    let direct = std::path::PathBuf::from(u);
    if direct.is_file() {
        return Some(direct);
    }
    // Strip virtual document / asset prefixes for local spike resolution.
    let stripped = u
        .strip_prefix("/documents/illustrations/")
        .or_else(|| u.strip_prefix("/assets/illustration/"))
        .unwrap_or(u);
    for root in search_roots {
        let cand = root.join(stripped);
        if cand.is_file() {
            return Some(cand);
        }
        let cand2 = root.join(u);
        if cand2.is_file() {
            return Some(cand2);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_glb() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/unit_cube.glb")
    }

    #[test]
    fn load_unit_cube_fixture() {
        let mesh = load_gltf_mesh(&fixture_glb()).expect("load glb");
        assert!(mesh.triangle_count >= 12);
        assert!(mesh.vertex_count() >= 8);
        assert!((mesh.bounds_min.x + 0.5).abs() < 1e-3);
        assert!((mesh.bounds_max.y - 0.5).abs() < 1e-3);
    }

    #[test]
    fn insert_mesh_asset_into_scenegraph() {
        let mut scene = SceneGraph::demo_scene();
        let id = insert_mesh_asset(
            &mut scene,
            "root",
            "imported_cube",
            "Imported Cube",
            fixture_glb().to_string_lossy(),
            Transform {
                translation: Vec3::new(0.0, 1.0, 0.0),
                ..Default::default()
            },
        )
        .expect("insert");
        let node = scene.nodes.get(&id).expect("node");
        assert_eq!(node.kind, NodeKind::MeshAsset);
        assert!(node.mesh_uri.is_some());
    }
}
