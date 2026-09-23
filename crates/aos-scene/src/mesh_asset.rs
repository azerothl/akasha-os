//! MeshAsset — load glTF/GLB into a CPU triangle mesh for SceneGraph.
//!
//! ADR 0011: Akasha is Y-up RH. glTF is also Y-up; we import positions as-is
//! (no Blender Z-up conversion in the host). The CPU mesh keeps positions and
//! normals for edit previews; Blender imports the original GLB with PBR data.

use crate::math::{Mat4, Vec3};
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
    /// First glTF material base color for lightweight previews. Full PBR stays in GLB.
    pub base_color: [f32; 4],
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
    let (doc, buffers, _images) =
        gltf::import(path).map_err(|e| MeshAssetError::Gltf(e.to_string()))?;
    struct Acc {
        interleaved: Vec<f32>,
        indices: Vec<u32>,
        bmin: [f32; 3],
        bmax: [f32; 3],
        base_color: [f32; 4],
        material_seen: bool,
    }
    let mut acc = Acc {
        interleaved: Vec::new(),
        indices: Vec::new(),
        bmin: [f32::INFINITY; 3],
        bmax: [f32::NEG_INFINITY; 3],
        base_color: [0.65, 0.67, 0.69, 1.0],
        material_seen: false,
    };

    let scene = doc
        .default_scene()
        .or_else(|| doc.scenes().next())
        .ok_or_else(|| MeshAssetError::Validation("glTF contains no scene".into()))?;
    fn collect_node(
        node: gltf::Node<'_>,
        parent: Mat4,
        buffers: &[gltf::buffer::Data],
        acc: &mut Acc,
    ) -> Result<(), MeshAssetError> {
        let cols = node.transform().matrix();
        let local = Mat4::from_cols(cols[0], cols[1], cols[2], cols[3]);
        let world = parent * local;
        if let Some(mesh) = node.mesh() {
            for prim in mesh.primitives() {
                if prim.mode() != gltf::mesh::Mode::Triangles {
                    return Err(MeshAssetError::Validation(
                        "only triangle primitives are supported".into(),
                    ));
                }
                if !acc.material_seen {
                    acc.base_color = prim.material().pbr_metallic_roughness().base_color_factor();
                    acc.material_seen = true;
                }
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
                let base = (acc.interleaved.len() / 6) as u32;
                for (i, p) in positions.iter().enumerate() {
                    let n = normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
                    let position = world.transform_point(Vec3::new(p[0], p[1], p[2]));
                    let normal = world
                        .transform_vector(Vec3::new(n[0], n[1], n[2]))
                        .normalized()
                        .unwrap_or(Vec3::UNIT_Y);
                    acc.interleaved.extend_from_slice(&[
                        position.x, position.y, position.z, normal.x, normal.y, normal.z,
                    ]);
                    for c in 0..3 {
                        let value = [position.x, position.y, position.z][c];
                        acc.bmin[c] = acc.bmin[c].min(value);
                        acc.bmax[c] = acc.bmax[c].max(value);
                    }
                }
                if let Some(idx) = reader.read_indices() {
                    for i in idx.into_u32() {
                        if i as usize >= positions.len() {
                            return Err(MeshAssetError::Validation(
                                "mesh index out of bounds".into(),
                            ));
                        }
                        acc.indices.push(base + i);
                    }
                } else {
                    for i in 0..positions.len() as u32 {
                        acc.indices.push(base + i);
                    }
                }
            }
        }
        for child in node.children() {
            collect_node(child, world, buffers, acc)?;
        }
        Ok(())
    }
    for node in scene.nodes() {
        collect_node(node, Mat4::IDENTITY, &buffers, &mut acc)?;
    }

    let Acc {
        interleaved,
        indices,
        bmin,
        bmax,
        base_color,
        ..
    } = acc;
    if interleaved.is_empty() || indices.len() < 3 || indices.len() % 3 != 0 {
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
        base_color,
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

/// Shared lookup roots for edit and render previews.
pub fn default_mesh_search_roots() -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    if let Ok(p) = std::env::var("AOS_NEURAL_MESH_PACK") {
        let pb = std::path::PathBuf::from(p.trim());
        if pb.is_dir() {
            roots.push(pb);
        }
    }
    let home = std::env::var("AOS_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")));
    roots.push(home.join("var/storage/data/documents/illustrations"));
    roots.push(home.join("share/assets/illustration"));
    roots.push(home.join("share/illustration-neural-mesh-pack"));
    for cand in [
        std::path::PathBuf::from("share/illustration-neural-mesh-pack"),
        std::path::PathBuf::from("share/assets/illustration"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../share/illustration-neural-mesh-pack"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"),
    ] {
        if cand.is_dir() {
            roots.push(cand);
        }
    }
    roots
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
    fn load_textured_glb_preserves_node_hierarchy() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hierarchy_textured.glb");
        let mesh = load_gltf_mesh(&path).expect("load textured GLB");
        assert_eq!(mesh.triangle_count, 1);
        assert!((mesh.bounds_min.x - 2.0).abs() < 1e-4);
        assert!((mesh.bounds_max.x - 3.0).abs() < 1e-4);
        assert!((mesh.bounds_min.y - 1.0).abs() < 1e-4);
        assert!((mesh.bounds_max.y - 2.0).abs() < 1e-4);
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
