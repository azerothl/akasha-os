//! Demo: stub mesh assist → SceneGraph → CPU beauty (no neural weights).
//!
//! ```bash
//! AOS_ILLUSTRATION_DEMO_OUT=/path/to/media cargo run -p aos-scene --example neural_mesh_demo --release
//! ```

use aos_scene::{
    mesh_assist, save_project_yaml, CpuWireframeBackend, MeshAssistBackendId, MeshAssistRequest,
    ProjectFile, RenderBackend, RenderPassKind, RenderRequest, SceneGraph,
};
use std::path::{Path, PathBuf};

fn main() {
    let out_dir = std::env::var("AOS_ILLUSTRATION_DEMO_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from("/cursor/stores/bc-27b0be1e-7e32-4372-8a8d-6b1d68445b8c/media")
        });
    let _ = std::fs::create_dir_all(&out_dir);
    let _ = std::fs::create_dir_all("/opt/cursor/artifacts");

    let mut scene = SceneGraph::demo_scene();
    let crate_res = mesh_assist(
        &mut scene,
        &MeshAssistRequest {
            prompt: "wooden crate".into(),
            parent_id: "root".into(),
            prefix: "assist_".into(),
            backend: MeshAssistBackendId::Stub,
        },
    )
    .expect("crate assist");
    let column_res = mesh_assist(
        &mut scene,
        &MeshAssistRequest {
            prompt: "stone column".into(),
            parent_id: "root".into(),
            prefix: "assist2_".into(),
            backend: MeshAssistBackendId::Stub,
        },
    )
    .expect("column assist");

    assert!(crate_res.is_stub && column_res.is_stub);
    println!(
        "stub kinds: crate={} column={} created={}",
        crate_res.kind_id,
        column_res.kind_id,
        crate_res.created_ids.len() + column_res.created_ids.len()
    );

    write_yaml_and_beauty(
        &out_dir,
        &scene,
        "neural-mesh-assisted.scene.yaml",
        "illustration-neural-mesh-stub-cpu-beauty.png",
    );

    let neural_err = mesh_assist(
        &mut scene,
        &MeshAssistRequest {
            prompt: "marble statue".into(),
            backend: MeshAssistBackendId::Neural,
            ..Default::default()
        },
    )
    .unwrap_err();
    println!("neural fail-closed: {neural_err}");
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
        })
        .expect("cpu beauty");
    let beauty_path = out_dir.join(png_name);
    std::fs::write(&beauty_path, &beauty.png).expect("write beauty");
    println!("wrote {} ({} bytes)", beauty_path.display(), beauty.png.len());
    let _ = std::fs::copy(&beauty_path, format!("/opt/cursor/artifacts/{png_name}"));
}
