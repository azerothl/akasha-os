//! Demo: compose a 2×2 comic page from the demo SceneGraph and write a PNG.
//!
//! ```text
//! cargo run -p aos-scene --example comic_panels_demo --release
//! ```

use aos_scene::{
    apply_comic_layout, render_comic_page, save_comic_yaml, ComicLayoutId, RenderBackendId,
    SceneGraph,
};
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scene = SceneGraph::demo_scene();
    let comic = apply_comic_layout(ComicLayoutId::Grid2x2, &scene, 640, 960)?;
    let yaml = save_comic_yaml(&comic)?;
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/comic-demo");
    fs::create_dir_all(&out_dir)?;
    let yaml_path = out_dir.join("page_grid_2x2.comic.yaml");
    fs::write(&yaml_path, &yaml)?;
    let rendered = render_comic_page(
        &comic,
        "page_1",
        RenderBackendId::Cpu,
        Some(640),
        Some(960),
    )?;
    let png_path = out_dir.join("comic-page.png");
    fs::write(&png_path, &rendered.png)?;
    println!(
        "wrote {} ({} panels) and {}",
        png_path.display(),
        rendered.panel_count,
        yaml_path.display()
    );
    Ok(())
}
