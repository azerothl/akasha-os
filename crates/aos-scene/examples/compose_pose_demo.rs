//! Example: compose → pose presets / IK → CPU beauty (+ optional wgpu).
//!
//! ```bash
//! AOS_ILLUSTRATION_DEMO_OUT=/path/to/media cargo run -p aos-scene --example compose_pose_demo
//! ```

use aos_scene::{
    apply_ik_chain, apply_pose_preset, compose_from_prompt, eye_from_orbit, save_project_yaml,
    CpuWireframeBackend, ProjectFile, RenderBackend, RenderPassKind, RenderRequest, SceneGraph,
    Vec3, ViewportCamera, ViewportRenderer,
};
use std::path::{Path, PathBuf};

fn main() {
    let out_dir = std::env::var("AOS_ILLUSTRATION_DEMO_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from("/cursor/stores/bc-27b0be1e-7e32-4372-8a8d-6b1d68445b8c/media")
        });
    let _ = std::fs::create_dir_all(&out_dir);

    // 1) Library prompt → humanoid wave → CPU beauty
    let composed = compose_from_prompt(
        "Un homme entre dans une vieille librairie. Il se tient près du comptoir.",
    )
    .expect("compose");
    println!(
        "template={} placed={:?} character={:?}",
        composed.template_id, composed.placed_assets, composed.character_id
    );
    let mut scene = composed.scene;
    let root = composed
        .character_id
        .clone()
        .unwrap_or_else(|| "humanoid".into());
    apply_pose_preset(&mut scene, &root, "wave_right", None).expect("pose");
    write_yaml_and_beauty(
        &out_dir,
        &scene,
        "composed-library.scene.yaml",
        "illustration-compose-cpu-beauty.png",
    );
    try_viewport(&out_dir, &scene, &root, "illustration-compose-viewport.png");

    // 2) Child + chair → slim variant → look-at
    let child = compose_from_prompt("a child stands near a chair").expect("child compose");
    let mut child_scene = child.scene;
    let child_root = child.character_id.clone().expect("slim root");
    apply_pose_preset(&mut child_scene, &child_root, "look_left", None).expect("look");
    write_yaml_and_beauty(
        &out_dir,
        &child_scene,
        "composed-child.scene.yaml",
        "illustration-compose-slim-cpu-beauty.png",
    );

    // 3) Articulated sit + two-bone IK reach → beauty
    let mut ik_scene = SceneGraph::demo_scene();
    apply_pose_preset(&mut ik_scene, "humanoid", "sitting", None).expect("sit");
    apply_ik_chain(
        &mut ik_scene,
        "humanoid",
        "arm_r",
        Vec3::new(0.95, 1.25, 0.55),
        Some(Vec3::new(0.5, 1.8, 1.0)),
        None,
    )
    .expect("ik");
    write_yaml_and_beauty(
        &out_dir,
        &ik_scene,
        "posed-ik-sit.scene.yaml",
        "illustration-ik-sit-cpu-beauty.png",
    );
    try_viewport(
        &out_dir,
        &ik_scene,
        "humanoid",
        "illustration-ik-sit-viewport.png",
    );

    // 4) Cat + man library → quad_sit
    let with_cat =
        compose_from_prompt("A man enters an old library. A cat is sitting on the counter.")
            .expect("cat compose");
    let mut cat_scene = with_cat.scene;
    let cat_root = cat_scene
        .node_ids_depth_first()
        .into_iter()
        .find(|id| id.contains("quadruped") || id.starts_with("cat_"))
        .expect("cat root");
    apply_pose_preset(&mut cat_scene, &cat_root, "quad_sit", None).expect("quad sit");
    write_yaml_and_beauty(
        &out_dir,
        &cat_scene,
        "composed-cat.scene.yaml",
        "illustration-cat-sit-cpu-beauty.png",
    );

    // 5) Bookstore demo (§143) → expanded prefab pack beauty
    let bookstore = compose_from_prompt(
        "An old bookstore. A man enters through the door while a cat lies on the counter watching him.",
    )
    .expect("bookstore compose");
    println!(
        "bookstore template={} placed={:?}",
        bookstore.template_id, bookstore.placed_assets
    );
    let mut bookstore_scene = bookstore.scene;
    if let Some(root) = bookstore.character_id.clone() {
        let _ = apply_pose_preset(&mut bookstore_scene, &root, "wave_right", None);
    }
    write_yaml_and_beauty(
        &out_dir,
        &bookstore_scene,
        "composed-bookstore.scene.yaml",
        "illustration-bookstore-cpu-beauty.png",
    );

    // Dump demo_scene seed for package / docs.
    let demo_yaml = save_project_yaml(&ProjectFile::new(SceneGraph::demo_scene())).expect("demo");
    std::fs::write(out_dir.join("demo-articulated.scene.yaml"), &demo_yaml).expect("write demo");
}

fn write_yaml_and_beauty(out_dir: &Path, scene: &SceneGraph, yaml_name: &str, png_name: &str) {
    let yaml = save_project_yaml(&ProjectFile::new(scene.clone())).expect("yaml");
    std::fs::write(out_dir.join(yaml_name), &yaml).expect("write yaml");
    let beauty = CpuWireframeBackend
        .render(&RenderRequest {
            scene: scene.clone(),
            pass: RenderPassKind::Beauty,
            width: 480,
            height: 320,
            stub_rgb: (48, 72, 96),
            style: None,
            preset: None,
        })
        .expect("cpu beauty");
    let beauty_path = out_dir.join(png_name);
    std::fs::write(&beauty_path, &beauty.png).expect("write beauty");
    println!(
        "wrote {} ({} bytes)",
        beauty_path.display(),
        beauty.png.len()
    );
    let _ = std::fs::copy(&beauty_path, format!("/opt/cursor/artifacts/{png_name}"));
}

fn try_viewport(out_dir: &Path, scene: &SceneGraph, selected: &str, name: &str) {
    let target = Vec3::new(0.2, 1.0, 0.2);
    let eye = eye_from_orbit(0.85, 0.4, 9.0, target);
    let cam = ViewportCamera {
        eye,
        target,
        ..ViewportCamera::default()
    };
    match ViewportRenderer::new() {
        Ok(gpu) => {
            let png = gpu
                .render_png(scene, &cam, 960, 540, Some(selected))
                .expect("viewport png");
            let vp_path = out_dir.join(name);
            std::fs::write(&vp_path, &png).expect("write viewport");
            let _ = std::fs::copy(&vp_path, format!("/opt/cursor/artifacts/{name}"));
            println!("wrote {} ({} bytes)", vp_path.display(), png.len());
        }
        Err(e) => eprintln!("viewport skipped: {e}"),
    }
}
