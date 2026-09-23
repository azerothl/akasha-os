//! Demo: CPU NPR beauty for Sketch / Pencil / Ink.
//!
//! ```bash
//! cargo run -p aos-scene --example npr_style_demo --release
//! ```

use aos_scene::{
    compose_from_prompt, resolve_style, CpuWireframeBackend, RenderBackend, RenderBackendId,
    RenderPassKind, RenderRequest, RenderService, RenderSubmit,
};
use std::path::PathBuf;

fn main() {
    // CI / demo path: Blender mock (no binary required).
    std::env::set_var("AOS_BLENDER_MODE", "mock");
    let out = PathBuf::from(std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".into()))
        .join("npr-style-demo");
    let _ = std::fs::create_dir_all(&out);
    let composed = compose_from_prompt("A man enters an old library. He stands by the counter.")
        .expect("compose");
    let scene = composed.scene;
    let svc = RenderService::default();

    for id in ["sketch", "pencil", "ink"] {
        let style = resolve_style(id).expect("style");
        let cpu = CpuWireframeBackend
            .render(&RenderRequest {
                scene: scene.clone(),
                pass: RenderPassKind::Beauty,
                width: 480,
                height: 320,
                stub_rgb: (0, 0, 0),
                style: Some(style.clone()),
                preset: None,
            })
            .expect("cpu npr");
        let name = format!("beauty-cpu-{id}.png");
        let path = out.join(&name);
        std::fs::write(&path, &cpu.png).expect("write");
        println!("wrote {} ({} bytes)", path.display(), cpu.png.len());
        let _ = std::fs::copy(&path, format!("/opt/cursor/artifacts/{name}"));
        let _ = std::fs::copy(
            &path,
            format!(
                "/cursor/stores/bc-27b0be1e-7e32-4372-8a8d-6b1d68445b8c/media/illustration-npr-cpu-{id}.png"
            ),
        );

        let mock = svc
            .submit_and_result(RenderSubmit {
                scene: scene.clone(),
                backend: RenderBackendId::Blender,
                pass: RenderPassKind::Beauty,
                width: 256,
                height: 192,
                output_path: format!("/documents/illustrations/beauty-blender-{id}.png"),
                stub_rgb: (0, 0, 0),
                style: Some(style),
                preset: None,
            })
            .expect("blender mock");
        let mock_name = format!("beauty-blender-mock-{id}.png");
        let mock_path = out.join(&mock_name);
        std::fs::write(&mock_path, &mock.png).expect("write mock");
        println!("wrote {} ({} bytes)", mock_path.display(), mock.png.len());
        let _ = std::fs::copy(&mock_path, format!("/opt/cursor/artifacts/{mock_name}"));
        let _ = std::fs::copy(
            &mock_path,
            format!(
                "/cursor/stores/bc-27b0be1e-7e32-4372-8a8d-6b1d68445b8c/media/illustration-npr-blender-mock-{id}.png"
            ),
        );
    }
}
