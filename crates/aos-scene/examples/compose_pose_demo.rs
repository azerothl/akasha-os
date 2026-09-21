//! Example: compose from prompt → pose → CPU beauty (+ optional wgpu viewport).
//!
//! ```bash
//! AOS_ILLUSTRATION_DEMO_OUT=/path/to/media cargo run -p aos-scene --example compose_pose_demo
//! ```

use aos_scene::{
    apply_pose_preset, compose_from_prompt, eye_from_orbit, save_project_yaml, CpuWireframeBackend,
    ProjectFile, RenderBackend, RenderPassKind, RenderRequest, SceneGraph, Vec3, ViewportCamera,
    ViewportRenderer,
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
        })
        .expect("cpu beauty");
    let beauty_path = out_dir.join(png_name);
    std::fs::write(&beauty_path, &beauty.png).expect("write beauty");
    println!("wrote {} ({} bytes)", beauty_path.display(), beauty.png.len());
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
