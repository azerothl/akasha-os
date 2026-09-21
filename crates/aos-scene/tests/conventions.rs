//! ADR 0011 golden tests for SceneGraph conventions.

use aos_scene::{Mat4, ProjectFile, Quat, SceneGraph, SceneOp, UndoStack, Vec3, EPSILON};
use approx::abs_diff_eq;

fn assert_vec_close(a: Vec3, b: Vec3, eps: f32) {
    assert!(
        abs_diff_eq!(a.x, b.x, epsilon = eps)
            && abs_diff_eq!(a.y, b.y, epsilon = eps)
            && abs_diff_eq!(a.z, b.z, epsilon = eps),
        "vec mismatch: {:?} vs {:?}",
        a,
        b
    );
}

#[test]
fn identity_transform_world_equals_local() {
    let g = SceneGraph::demo_scene();
    let local = g.local_matrix("root").unwrap();
    let world = g.world_matrix("root").unwrap();
    for i in 0..16 {
        assert!(
            abs_diff_eq!(local.m[i], world.m[i], epsilon = EPSILON),
            "m[{i}]"
        );
    }
    assert_eq!(
        g.nodes["root"].transform.rotation.to_array(),
        [0.0, 0.0, 0.0, 1.0]
    );
}

#[test]
fn rotate_90_about_y_maps_x_to_minus_z() {
    // Right-handed Y-up: +90° about Y takes +X → −Z.
    let q = Quat::from_axis_angle(Vec3::UNIT_Y, std::f32::consts::FRAC_PI_2);
    let m = Mat4::from_trs(Vec3::ZERO, q, Vec3::ONE);
    let p = m.transform_point(Vec3::UNIT_X);
    assert_vec_close(p, Vec3::new(0.0, 0.0, -1.0), 1e-4);
}

#[test]
fn rotate_90_about_x_maps_y_to_z() {
    let q = Quat::from_axis_angle(Vec3::UNIT_X, std::f32::consts::FRAC_PI_2);
    let m = Mat4::from_trs(Vec3::ZERO, q, Vec3::ONE);
    let p = m.transform_point(Vec3::UNIT_Y);
    assert_vec_close(p, Vec3::new(0.0, 0.0, 1.0), 1e-4);
}

#[test]
fn rotate_90_about_z_maps_x_to_y() {
    let q = Quat::from_axis_angle(Vec3::UNIT_Z, std::f32::consts::FRAC_PI_2);
    let m = Mat4::from_trs(Vec3::ZERO, q, Vec3::ONE);
    let p = m.transform_point(Vec3::UNIT_X);
    assert_vec_close(p, Vec3::new(0.0, 1.0, 0.0), 1e-4);
}

#[test]
fn parent_child_world_composition() {
    let mut g = SceneGraph::demo_scene();
    // Move root up 1m; box local y=0.675 → world y=1.675
    g.nodes.get_mut("root").unwrap().transform.translation = Vec3::new(0.0, 1.0, 0.0);
    let world = g.world_translation("box").unwrap();
    assert_vec_close(world, Vec3::new(0.0, 1.675, 0.0), 1e-4);
}

#[test]
fn identity_camera_forward_is_neg_z() {
    let mut g = SceneGraph::demo_scene();
    // Place camera at origin with identity rotation.
    g.nodes.get_mut("camera").unwrap().transform.translation = Vec3::ZERO;
    g.nodes.get_mut("camera").unwrap().transform.rotation = Quat::IDENTITY;
    let f = g.camera_forward_world("camera").unwrap();
    assert_vec_close(f, Vec3::new(0.0, 0.0, -1.0), 1e-4);
}

#[test]
fn undo_set_transform_round_trips() {
    let mut g = SceneGraph::demo_scene();
    let before = g.nodes["box"].transform.clone();
    let mut after = before.clone();
    after.translation = Vec3::new(1.0, 0.5, -2.0);
    let mut stack = UndoStack::default();
    stack
        .push_apply(
            &mut g,
            SceneOp::SetTransform {
                id: "box".into(),
                before: before.clone(),
                after: after.clone(),
            },
        )
        .unwrap();
    assert_vec_close(g.nodes["box"].transform.translation, after.translation, 1e-5);
    assert!(stack.undo(&mut g).unwrap());
    assert_vec_close(g.nodes["box"].transform.translation, before.translation, 1e-5);
    assert!(stack.redo(&mut g).unwrap());
    assert_vec_close(g.nodes["box"].transform.translation, after.translation, 1e-5);
}

#[test]
fn project_yaml_path_prefix() {
    assert!(aos_scene::DEFAULT_SCENE_YAML_PATH.starts_with(aos_scene::ILLUSTRATIONS_DOCUMENTS_PREFIX));
    let _ = ProjectFile::new(SceneGraph::demo_scene());
}
