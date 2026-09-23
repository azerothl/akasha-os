//! Lights v0 demo — CPU beauty with dim/warm vs bright/cool key light.

use aos_scene::{
    apply_one, AgentEditOp, CpuWireframeBackend, EditActorKind, EditSnapshot, RenderBackend,
    RenderPassKind, RenderRequest, SceneGraph,
};
use std::fs;
use std::path::PathBuf;

fn main() {
    let out_dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/opt/cursor/artifacts"));
    let _ = fs::create_dir_all(&out_dir);
    let media = PathBuf::from("/cursor/stores/bc-27b0be1e-7e32-4372-8a8d-6b1d68445b8c/media");
    let _ = fs::create_dir_all(&media);

    let backend = CpuWireframeBackend;
    let mut snap = EditSnapshot {
        project: aos_scene::ProjectFile::new(SceneGraph::demo_scene()),
        selected_id: Some("key_light".into()),
    };

    apply_one(
        &mut snap,
        &AgentEditOp::Light {
            id: Some("key_light".into()),
            add: false,
            parent_id: None,
            name: None,
            translation: None,
            light_type: Some("point".into()),
            intensity: Some(0.3),
            color: None,
            color_srgb: Some([255, 90, 70]),
            range: Some(6.0),
            spot_angle_deg: None,
        },
        "human:demo",
        EditActorKind::Human,
    )
    .unwrap();
    let dim = backend
        .render(&RenderRequest {
            scene: snap.project.scene.clone(),
            pass: RenderPassKind::Beauty,
            width: 480,
            height: 320,
            stub_rgb: (0, 0, 0),
            style: None,
            preset: None,
        })
        .unwrap();
    let dim_path = out_dir.join("lights-v0-dim-warm.png");
    fs::write(&dim_path, &dim.png).unwrap();

    apply_one(
        &mut snap,
        &AgentEditOp::Light {
            id: Some("key_light".into()),
            add: false,
            parent_id: None,
            name: None,
            translation: None,
            light_type: Some("point".into()),
            intensity: Some(4.5),
            color: None,
            color_srgb: Some([180, 220, 255]),
            range: Some(12.0),
            spot_angle_deg: None,
        },
        "human:demo",
        EditActorKind::Human,
    )
    .unwrap();
    let bright = backend
        .render(&RenderRequest {
            scene: snap.project.scene.clone(),
            pass: RenderPassKind::Beauty,
            width: 480,
            height: 320,
            stub_rgb: (0, 0, 0),
            style: None,
            preset: None,
        })
        .unwrap();
    let bright_path = out_dir.join("lights-v0-bright-cool.png");
    fs::write(&bright_path, &bright.png).unwrap();

    let _ = fs::copy(&dim_path, media.join("illustration-lights-v0-dim.png"));
    let _ = fs::copy(
        &bright_path,
        media.join("illustration-lights-v0-bright.png"),
    );
    println!(
        "wrote dim={} bright={} (bytes {} vs {})",
        dim_path.display(),
        bright_path.display(),
        dim.png.len(),
        bright.png.len()
    );
}
