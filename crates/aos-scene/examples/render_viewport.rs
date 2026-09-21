use aos_scene::{eye_from_orbit, SceneGraph, Vec3, ViewportCamera, ViewportRenderer};
fn main() {
    let gpu = ViewportRenderer::new().expect("wgpu");
    let scene = SceneGraph::demo_scene();
    let target = Vec3::new(0.4, 0.8, 0.0);
    let eye = eye_from_orbit(0.6, 0.45, 8.0, target);
    let cam = ViewportCamera { eye, target, ..ViewportCamera::default() };
    for (name, sel, yaw) in [
        ("illustration-wgpu-viewport.png", Some("box"), 0.6f32),
        ("illustration-wgpu-viewport-orbit.png", Some("torso"), 1.35f32),
    ] {
        let eye = eye_from_orbit(yaw, 0.45, 8.0, target);
        let cam = ViewportCamera { eye, target, ..cam };
        let png = gpu.render_png(&scene, &cam, 960, 540, sel).expect("png");
        let path = format!("/cursor/stores/bc-27b0be1e-7e32-4372-8a8d-6b1d68445b8c/media/{name}");
        std::fs::write(&path, &png).expect("write");
        std::fs::copy(&path, format!("/opt/cursor/artifacts/{name}")).ok();
        println!("wrote {path} ({} bytes)", png.len());
    }
}
