use aos_scene::{
    apply_one, AgentEditOp, CpuWireframeBackend, EditActorKind, EditSnapshot, RenderBackend,
    RenderPassKind, RenderRequest, SceneGraph, Vec3,
};
use std::fs;

fn main() {
    let mut snap = EditSnapshot {
        project: aos_scene::ProjectFile::new(SceneGraph::demo_scene()),
        selected_id: Some("camera".into()),
    };
    // Wide FOV, pulled back
    apply_one(
        &mut snap,
        &AgentEditOp::Camera {
            id: None,
            eye: Some(Vec3::new(2.0, 2.5, 9.0)),
            look_at: Some(Vec3::new(0.4, 0.8, 0.0)),
            fov_deg: Some(70.0),
            yaw: None,
            pitch: None,
            distance: None,
        },
        "human:demo",
        EditActorKind::Human,
    )
    .unwrap();
    let backend = CpuWireframeBackend;
    let wide = backend
        .render(&RenderRequest {
            scene: snap.project.scene.clone(),
            pass: RenderPassKind::Beauty,
            width: 480,
            height: 320,
            stub_rgb: (0, 0, 0),
            style: Some(aos_scene::resolve_style("pencil").unwrap()),
            preset: None,
        })
        .unwrap();
    fs::write(
        "/opt/cursor/artifacts/camera-controls-wide-fov-cpu.png",
        &wide.png,
    )
    .unwrap();

    apply_one(
        &mut snap,
        &AgentEditOp::Camera {
            id: None,
            eye: Some(Vec3::new(0.5, 1.6, 3.2)),
            look_at: Some(Vec3::new(0.4, 0.9, 0.0)),
            fov_deg: Some(28.0),
            yaw: None,
            pitch: None,
            distance: None,
        },
        "human:demo",
        EditActorKind::Human,
    )
    .unwrap();
    let tight = backend
        .render(&RenderRequest {
            scene: snap.project.scene.clone(),
            pass: RenderPassKind::Beauty,
            width: 480,
            height: 320,
            stub_rgb: (0, 0, 0),
            style: Some(aos_scene::resolve_style("pencil").unwrap()),
            preset: None,
        })
        .unwrap();
    fs::write(
        "/opt/cursor/artifacts/camera-controls-tight-fov-cpu.png",
        &tight.png,
    )
    .unwrap();
    // also persist into project media for the store note
    fs::copy(
        "/opt/cursor/artifacts/camera-controls-wide-fov-cpu.png",
        "/cursor/stores/bc-27b0be1e-7e32-4372-8a8d-6b1d68445b8c/media/illustration-camera-wide-fov.png",
    )
    .ok();
    fs::copy(
        "/opt/cursor/artifacts/camera-controls-tight-fov-cpu.png",
        "/cursor/stores/bc-27b0be1e-7e32-4372-8a8d-6b1d68445b8c/media/illustration-camera-tight-fov.png",
    )
    .ok();
    println!("wrote wide={} tight={}", wide.png.len(), tight.png.len());
}
