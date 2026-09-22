//! SceneGraph camera helpers (ADR 0011).
//!
//! Identity camera looks along **−Z**. Focal length + sensor width define
//! horizontal FOV; human UI uses degrees at the boundary only.

use crate::math::{Mat4, Quat, Vec3, EPSILON};
use crate::scene::{CameraParams, NodeKind, SceneError, SceneGraph, Transform};
use std::f32::consts::FRAC_PI_2;

/// Horizontal FOV (radians) from focal length + sensor width.
pub fn hfov_rad(params: &CameraParams) -> f32 {
    let focal = params.focal_mm.max(1e-3);
    2.0 * (params.sensor_width_mm / (2.0 * focal)).atan()
}

/// Set `focal_mm` from a desired horizontal FOV (radians).
pub fn set_focal_from_hfov(params: &mut CameraParams, hfov: f32) {
    let half = (hfov * 0.5).clamp(0.01, FRAC_PI_2 - 0.05).tan();
    if half > EPSILON {
        params.focal_mm = (params.sensor_width_mm / (2.0 * half)).clamp(8.0, 400.0);
    }
}

/// Vertical FOV from horizontal FOV + aspect (`width / height`).
pub fn fovy_from_hfov(hfov: f32, aspect: f32) -> f32 {
    2.0 * ((hfov * 0.5).tan() / aspect.max(0.01)).atan()
}

/// Horizontal FOV from vertical FOV + aspect.
pub fn hfov_from_fovy(fovy: f32, aspect: f32) -> f32 {
    2.0 * ((fovy * 0.5).tan() * aspect.max(0.01)).atan()
}

/// Orbit eye from yaw / pitch / distance about `target` (Y-up RH).
pub fn eye_from_orbit(yaw: f32, pitch: f32, distance: f32, target: Vec3) -> Vec3 {
    let cp = pitch.cos();
    let offset = Vec3::new(yaw.sin() * cp, pitch.sin(), yaw.cos() * cp) * distance;
    target + offset
}

/// Invert [`eye_from_orbit`] — returns `(yaw, pitch, distance)`.
pub fn orbit_from_eye_target(eye: Vec3, target: Vec3) -> (f32, f32, f32) {
    let offset = eye - target;
    let distance = offset.length().max(0.01);
    let yaw = offset.x.atan2(offset.z);
    let pitch = (offset.y / distance)
        .clamp(-1.0 + EPSILON, 1.0 - EPSILON)
        .asin()
        .clamp(-FRAC_PI_2 + 0.05, FRAC_PI_2 - 0.05);
    (yaw, pitch, distance)
}

/// World rotation so local **−Z** aims at `target` from `eye` (up ≈ +Y).
pub fn rotation_look_at(eye: Vec3, target: Vec3, up: Vec3) -> Quat {
    let f = (target - eye)
        .normalized()
        .unwrap_or(Vec3::new(0.0, 0.0, -1.0));
    let s = f.cross(up).normalized().unwrap_or(Vec3::UNIT_X);
    let u = s.cross(f);
    let m = Mat4::from_cols(
        [s.x, s.y, s.z, 0.0],
        [u.x, u.y, u.z, 0.0],
        [-f.x, -f.y, -f.z, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    );
    Quat::from_mat4_rotation(m)
}

/// Active camera eye + look target (target = eye + forward × distance hint).
pub fn active_camera_eye_target(scene: &SceneGraph, look_distance: f32) -> Option<(Vec3, Vec3)> {
    let cam_id = scene.active_camera.as_deref()?;
    let eye = scene.world_translation(cam_id).ok()?;
    let forward = scene
        .camera_forward_world(cam_id)
        .unwrap_or(Vec3::new(0.0, 0.0, -1.0));
    let dist = look_distance.max(0.01);
    Some((eye, eye + forward * dist))
}

/// Write orbit framing into the active SceneGraph camera (transform + FOV).
pub fn apply_orbit_to_active_camera(
    scene: &mut SceneGraph,
    yaw: f32,
    pitch: f32,
    distance: f32,
    target: Vec3,
    hfov: f32,
) -> Result<(), SceneError> {
    let Some(id) = scene.active_camera.clone() else {
        return Err(SceneError::UnknownNode("active_camera".into()));
    };
    let node = scene
        .nodes
        .get(&id)
        .ok_or_else(|| SceneError::UnknownNode(id.clone()))?;
    if node.kind != NodeKind::Camera {
        return Err(SceneError::UnknownNode(id));
    }
    let eye = eye_from_orbit(yaw, pitch, distance, target);
    let mut transform = node.transform.clone();
    transform.translation = eye;
    transform.rotation = rotation_look_at(eye, target, Vec3::UNIT_Y);
    transform.scale = Vec3::ONE;
    let mut params = node.camera.clone().unwrap_or_default();
    set_focal_from_hfov(&mut params, hfov);
    params.near = params.near.max(0.01);
    params.far = params.far.max(params.near + 1.0);
    scene.set_transform(&id, transform)?;
    scene.set_camera_params(&id, params)?;
    Ok(())
}

/// Pull orbit framing from the active camera. Uses `fallback_distance` when
/// recovering look-at along forward (beauty / load path).
pub fn orbit_from_active_camera(
    scene: &SceneGraph,
    fallback_distance: f32,
) -> Option<(f32, f32, f32, Vec3, f32, CameraParams)> {
    let cam_id = scene.active_camera.as_deref()?;
    let node = scene.nodes.get(cam_id)?;
    if node.kind != NodeKind::Camera {
        return None;
    }
    let eye = scene.world_translation(cam_id).ok()?;
    let forward = scene
        .camera_forward_world(cam_id)
        .unwrap_or(Vec3::new(0.0, 0.0, -1.0));
    let distance = fallback_distance.max(0.01);
    let target = eye + forward * distance;
    let (yaw, pitch, dist) = orbit_from_eye_target(eye, target);
    let params = node.camera.clone().unwrap_or_default();
    let hfov = hfov_rad(&params);
    Some((yaw, pitch, dist, target, hfov, params))
}

/// Build a local camera transform for eye → look-at.
pub fn camera_transform_look_at(eye: Vec3, target: Vec3) -> Transform {
    Transform {
        translation: eye,
        rotation: rotation_look_at(eye, target, Vec3::UNIT_Y),
        scale: Vec3::ONE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::SceneGraph;

    #[test]
    fn focal_hfov_roundtrip() {
        let mut p = CameraParams::default();
        let h0 = hfov_rad(&p);
        set_focal_from_hfov(&mut p, h0);
        let h1 = hfov_rad(&p);
        assert!((h0 - h1).abs() < 1e-4);
    }

    #[test]
    fn orbit_eye_roundtrip() {
        let target = Vec3::new(0.4, 0.8, 0.0);
        let eye = eye_from_orbit(0.6, 0.45, 8.0, target);
        let (yaw, pitch, dist) = orbit_from_eye_target(eye, target);
        assert!((yaw - 0.6).abs() < 1e-3);
        assert!((pitch - 0.45).abs() < 1e-3);
        assert!((dist - 8.0).abs() < 1e-3);
    }

    #[test]
    fn apply_orbit_writes_active_camera() {
        let mut scene = SceneGraph::demo_scene();
        apply_orbit_to_active_camera(
            &mut scene,
            1.0,
            0.3,
            7.0,
            Vec3::new(0.0, 1.0, 0.0),
            40.0_f32.to_radians(),
        )
        .expect("apply");
        let (yaw, pitch, dist, target, hfov, _) =
            orbit_from_active_camera(&scene, 7.0).expect("pull");
        assert!((yaw - 1.0).abs() < 0.05);
        assert!((pitch - 0.3).abs() < 0.05);
        assert!((dist - 7.0).abs() < 0.1);
        assert!((target.y - 1.0).abs() < 0.15);
        assert!((hfov - 40.0_f32.to_radians()).abs() < 0.02);
    }

    #[test]
    fn look_at_forward_matches() {
        let eye = Vec3::new(0.0, 2.0, 6.0);
        let target = Vec3::new(0.0, 1.0, 0.0);
        let q = rotation_look_at(eye, target, Vec3::UNIT_Y);
        let forward = q.rotate_vec(Vec3::new(0.0, 0.0, -1.0));
        let want = (target - eye).normalized().unwrap();
        assert!((forward - want).length() < 1e-3);
    }
}
