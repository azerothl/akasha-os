//! Dump AkashaSceneExport JSON for Renderer Pack / Blender smoke.
//!
//! ```bash
//! cargo run -p aos-scene --example dump_scene_export -- /tmp/scene.json ink
//! ```

fn main() {
    use aos_scene::{AkashaSceneExport, SceneGraph, resolve_style};

    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "scene.json".into());
    let style_id = std::env::args().nth(2).unwrap_or_else(|| "ink".into());
    let scene = SceneGraph::demo_scene();
    let style = if style_id == "none" || style_id.is_empty() {
        None
    } else {
        Some(resolve_style(&style_id).expect("style"))
    };
    let export = AkashaSceneExport::from_scene_with_style(
        &scene,
        256,
        192,
        "beauty",
        style.as_ref(),
    )
    .expect("export");
    let json = export.to_canonical_json().expect("json");
    std::fs::write(&out, &json).expect("write");
    eprintln!("wrote {out} ({} bytes, style={style_id})", json.len());
}
