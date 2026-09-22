//! Storyboard demo — capture pose/camera shots and apply the timeline.

use aos_scene::{
    apply_frame, apply_pose_preset, capture_frame, delete_frame, move_active_frame,
    save_project_yaml, ApplyTarget, ProjectFile, SceneGraph, Storyboard, UndoStack, Vec3,
};

fn main() {
    let mut scene = SceneGraph::demo_scene();
    let mut undo = UndoStack::default();
    let mut board = Storyboard::default();

    // Shot 01 — rest wide.
    capture_frame(&mut board, &scene, Some("Wide rest"), Some(1200)).expect("capture 1");

    // Shot 02 — wave + closer camera.
    apply_pose_preset(&mut scene, "humanoid", "wave_right", Some(&mut undo)).expect("wave");
    scene
        .nodes
        .get_mut("camera")
        .unwrap()
        .transform
        .translation = Vec3::new(0.5, 1.8, 4.0);
    capture_frame(&mut board, &scene, Some("Wave close-up"), Some(1500)).expect("capture 2");

    // Shot 03 — look left.
    apply_pose_preset(&mut scene, "humanoid", "look_left", Some(&mut undo)).expect("look");
    capture_frame(&mut board, &scene, Some("Look left"), None).expect("capture 3");

    move_active_frame(&mut board, "earlier").expect("reorder");
    assert_eq!(board.frames[1].label, "Look left");
    assert_eq!(board.frames[2].label, "Wave close-up");

    apply_frame(&mut board, &mut scene, ApplyTarget::Index(0)).expect("apply wide");
    assert!((scene.nodes["camera"].transform.translation.z - 6.5).abs() < 1e-3);

    let project = ProjectFile::with_storyboard(scene.clone(), board.clone());
    let yaml = save_project_yaml(&project).expect("yaml");
    println!("storyboard summary: {}", board.summary());
    println!("project yaml bytes: {}", yaml.len());
    println!("frames: {}", board.frames.len());
    for (i, f) in board.frames.iter().enumerate() {
        println!("  [{i}] {} — {} ({} ms)", f.id, f.label, f.duration_ms);
    }

    if let Ok(out) = std::env::var("AOS_STORYBOARD_OUT") {
        std::fs::write(&out, &yaml).expect("write storyboard yaml");
        println!("wrote {out}");
    }

    delete_frame(&mut board, Some("shot_03")).ok();
    println!("after delete: {}", board.summary());
}
