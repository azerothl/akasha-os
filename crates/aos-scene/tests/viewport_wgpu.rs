//! wgpu edit-viewport smoke (SceneGraph SoT; not RenderService beauty).

use aos_scene::{
    eye_from_orbit, collect_mesh_instances, SceneGraph, ViewportCamera, ViewportRenderer, Vec3,
    VIEWPORT_ROLE, BEAUTY_ROLE,
};

#[test]
fn viewport_role_is_not_beauty() {
    assert_eq!(VIEWPORT_ROLE, "edit_view");
    assert_eq!(BEAUTY_ROLE, "render_service_beauty");
    assert_ne!(VIEWPORT_ROLE, BEAUTY_ROLE);
}

#[test]
fn starter_scene_mesh_parity() {
    let g = SceneGraph::demo_scene();
    let meshes = collect_mesh_instances(&g, Some("box"));
    let ids: Vec<_> = meshes.iter().map(|m| m.id.as_str()).collect();
    for need in ["ground", "pedestal", "box", "torso", "head", "leg_l", "leg_r", "arm_l", "arm_r"] {
        assert!(ids.contains(&need), "missing {need} in {ids:?}");
    }
    assert!(meshes.iter().any(|m| m.selected && m.id == "box"));
}

#[test]
fn wgpu_renders_starter_scene_png() {
    let Ok(gpu) = ViewportRenderer::new() else {
        eprintln!("skip: no wgpu adapter");
        return;
    };
    let scene = SceneGraph::demo_scene();
    let eye = eye_from_orbit(0.6, 0.45, 8.0, Vec3::new(0.4, 0.8, 0.0));
    let cam = ViewportCamera {
        eye,
        target: Vec3::new(0.4, 0.8, 0.0),
        ..ViewportCamera::default()
    };
    let png = gpu
        .render_png(&scene, &cam, 640, 360, Some("box"))
        .expect("render png");
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']), "not png");
    assert!(png.len() > 800, "png too small: {}", png.len());

    let rgba = gpu
        .render_rgba(&scene, &cam, 320, 180, Some("box"))
        .expect("rgba");
    assert_eq!(rgba.len(), 320 * 180 * 4);
    // Atmosphere clear is ~24,28,34 — lit meshes should move some pixels away.
    let mut non_bg = 0u32;
    for px in rgba.as_chunks::<4>().0 {
        let dr = (px[0] as i16 - 24).unsigned_abs();
        let dg = (px[1] as i16 - 28).unsigned_abs();
        let db = (px[2] as i16 - 34).unsigned_abs();
        if dr + dg + db > 18 {
            non_bg += 1;
        }
    }
    assert!(non_bg > 500, "expected lit mesh pixels, got {non_bg}");
}
