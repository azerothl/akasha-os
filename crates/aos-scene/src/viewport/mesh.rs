//! SceneGraph → mesh instances + camera math (ADR 0011). Shared by wgpu paint.

use crate::math::{Mat4, Vec3};
use crate::scene::{NodeKind, SceneGraph};

pub use crate::camera::eye_from_orbit;

/// Half-extent of the unit box before node scale (metres).
pub const UNIT_CUBE_HALF: f32 = 0.5;

/// Orbit / perspective camera for the edit viewport (host chrome, not SceneGraph camera).
#[derive(Debug, Clone, Copy)]
pub struct ViewportCamera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fovy_rad: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for ViewportCamera {
    fn default() -> Self {
        Self {
            eye: Vec3::new(0.0, 2.2, 6.5),
            target: Vec3::new(0.4, 0.8, 0.0),
            up: Vec3::UNIT_Y,
            fovy_rad: 50.0_f32.to_radians(),
            near: 0.1,
            far: 200.0,
        }
    }
}

impl ViewportCamera {
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = look_at_rh(self.eye, self.target, self.up);
        let proj = perspective_rh(self.fovy_rad, aspect.max(0.01), self.near, self.far);
        proj * view
    }
}

/// One drawable MeshBox in world space.
#[derive(Debug, Clone)]
pub struct MeshInstance {
    pub id: String,
    pub world: Mat4,
    /// Linear RGB + alpha (working space).
    pub color: [f32; 4],
    pub selected: bool,
}

/// Stable palette so ground / pedestal / props / limbs read as a “dev viewport”.
fn color_for_id(id: &str, selected: bool) -> [f32; 4] {
    let base = match id {
        "ground" => [0.22, 0.26, 0.30, 1.0],
        "pedestal" => [0.38, 0.36, 0.34, 1.0],
        "box" => [0.72, 0.58, 0.38, 1.0],
        id if id.starts_with("torso") || id == "torso" => [0.55, 0.62, 0.78, 1.0],
        id if id.starts_with("head") || id == "head" => [0.78, 0.72, 0.62, 1.0],
        id if id.contains("leg") => [0.42, 0.48, 0.58, 1.0],
        id if id.contains("arm") => [0.48, 0.54, 0.64, 1.0],
        _ => [0.58, 0.56, 0.52, 1.0],
    };
    if selected {
        [
            (base[0] * 0.55 + 0.35_f32).min(1.0),
            (base[1] * 0.55 + 0.55_f32).min(1.0),
            (base[2] * 0.55 + 0.85_f32).min(1.0),
            1.0,
        ]
    } else {
        base
    }
}

pub fn collect_mesh_instances(scene: &SceneGraph, selected: Option<&str>) -> Vec<MeshInstance> {
    let mut out = Vec::new();
    for id in scene.node_ids_depth_first() {
        let Some(node) = scene.nodes.get(&id) else {
            continue;
        };
        if !node.visible || node.kind != NodeKind::MeshBox {
            continue;
        }
        let Ok(world) = scene.world_matrix(&id) else {
            continue;
        };
        let selected_here = selected == Some(id.as_str());
        out.push(MeshInstance {
            id: id.clone(),
            world,
            color: color_for_id(&id, selected_here),
            selected: selected_here,
        });
    }
    out
}

pub fn look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
    let f = (target - eye)
        .normalized()
        .unwrap_or(Vec3::new(0.0, 0.0, -1.0));
    let s = f.cross(up).normalized().unwrap_or(Vec3::UNIT_X);
    let u = s.cross(f);
    Mat4::from_cols(
        [s.x, u.x, -f.x, 0.0],
        [s.y, u.y, -f.y, 0.0],
        [s.z, u.z, -f.z, 0.0],
        [-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0],
    )
}

pub fn perspective_rh(fovy: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fovy * 0.5).tan();
    let mut m = [0.0; 16];
    m[0] = f / aspect;
    m[5] = f;
    m[10] = (far + near) / (near - far);
    m[11] = -1.0;
    m[14] = (2.0 * far * near) / (near - far);
    Mat4 { m }
}

/// Project world point to NDC-ish [-1,1] via column-major view-proj (ADR 0011).
pub fn project_point_ndc(p: Vec3, view_proj: &Mat4) -> Option<(f32, f32, f32)> {
    let clip = view_proj.transform_point(p);
    if !clip.x.is_finite() || !clip.y.is_finite() || !clip.z.is_finite() {
        return None;
    }
    Some((clip.x, clip.y, clip.z))
}
