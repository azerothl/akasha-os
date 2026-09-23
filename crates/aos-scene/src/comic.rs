//! Comic / panel layout for Illustration Studio (spec §162).
//!
//! A comic page is a grid of panels. Each panel owns a SceneGraph snapshot
//! (ADR 0011). Layout templates place normalized rects; `comic.render`
//! composites CPU (or stub) beauty passes into one page PNG.
//!
//! SceneGraph remains the only SoT inside each panel — the page YAML is
//! layout + references, not a second materials system.

use crate::math::{Quat, Vec3};
use crate::png::encode_rgba8_png;
use crate::project::{load_project_yaml, save_project_yaml, ProjectFile};
use crate::render::{
    CpuWireframeBackend, RenderBackend, RenderBackendId, RenderError, RenderPassKind,
    RenderRequest, StubRenderBackend,
};
use crate::scene::{SceneError, SceneGraph};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// DeclUI / host service: apply a comic page layout template.
pub const COMIC_LAYOUT_SERVICE: &str = "comic.layout";
/// DeclUI / host service: composite panel beauties into a page PNG.
pub const COMIC_RENDER_SERVICE: &str = "comic.render";

/// Cap: create / mutate comic page layouts (fail-closed).
pub const COMIC_LAYOUT_CAP: &str = "comic.layout";
/// Cap: render a comic page composite (fail-closed).
pub const COMIC_RENDER_CAP: &str = "comic.render";

/// On-disk / DeclUI comic project format version.
pub const COMIC_FORMAT_VERSION: u32 = 1;

/// Default page beauty path under illustrations documents.
pub const DEFAULT_COMIC_PAGE_PATH: &str = "/documents/illustrations/comic-page.png";

/// Soft caps for page pixel size (CPU composite).
pub const COMIC_MAX_PAGE_EDGE: u32 = 1024;
/// Soft caps for each panel render.
pub const COMIC_MAX_PANEL_EDGE: u32 = 512;

#[derive(Debug, Error, PartialEq)]
pub enum ComicError {
    #[error("empty scene")]
    EmptyScene,
    #[error("unknown layout `{0}`")]
    UnknownLayout(String),
    #[error("unknown page `{0}`")]
    UnknownPage(String),
    #[error("unknown panel `{0}`")]
    UnknownPanel(String),
    #[error("unsupported format_version {0}")]
    UnsupportedVersion(u32),
    #[error("no panels on page")]
    NoPanels,
    #[error("scene: {0}")]
    Scene(String),
    #[error("yaml: {0}")]
    Yaml(String),
    #[error("render: {0}")]
    Render(String),
    #[error("encode: {0}")]
    Encode(String),
}

impl From<SceneError> for ComicError {
    fn from(e: SceneError) -> Self {
        Self::Scene(e.to_string())
    }
}

impl From<RenderError> for ComicError {
    fn from(e: RenderError) -> Self {
        Self::Render(e.to_string())
    }
}

/// Named layout template ids (EN/FR DeclUI wire values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComicLayoutId {
    /// One full-bleed panel.
    Single,
    /// Two panels side by side.
    TwoHorizontal,
    /// Two panels stacked.
    TwoVertical,
    /// Classic 2×2 grid.
    Grid2x2,
    /// Three-panel horizontal strip.
    Strip3,
}

impl ComicLayoutId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::TwoHorizontal => "two_h",
            Self::TwoVertical => "two_v",
            Self::Grid2x2 => "grid_2x2",
            Self::Strip3 => "strip_3",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "single" | "one" | "plein" | "full" => Some(Self::Single),
            "two_h" | "two_horizontal" | "h2" | "split_h" | "deux_h" => Some(Self::TwoHorizontal),
            "two_v" | "two_vertical" | "v2" | "split_v" | "deux_v" => Some(Self::TwoVertical),
            "grid_2x2" | "2x2" | "quad" | "grille" => Some(Self::Grid2x2),
            "strip_3" | "three" | "strip" | "bande_3" | "bande" => Some(Self::Strip3),
            _ => None,
        }
    }

    pub fn panel_count(self) -> usize {
        match self {
            Self::Single => 1,
            Self::TwoHorizontal | Self::TwoVertical => 2,
            Self::Grid2x2 => 4,
            Self::Strip3 => 3,
        }
    }
}

/// Normalized panel rectangle on the page (0…1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PanelRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl PanelRect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn validate(self) -> Result<(), ComicError> {
        if ![self.x, self.y, self.w, self.h]
            .iter()
            .all(|v| v.is_finite())
            || self.w <= 0.0
            || self.h <= 0.0
            || self.x < 0.0
            || self.y < 0.0
            || self.x + self.w > 1.0 + 1e-4
            || self.y + self.h > 1.0 + 1e-4
        {
            return Err(ComicError::Scene(format!(
                "invalid panel rect ({}, {}, {}, {})",
                self.x, self.y, self.w, self.h
            )));
        }
        Ok(())
    }
}

/// One comic panel: layout rect + SceneGraph snapshot (project YAML).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComicPanel {
    pub id: String,
    pub rect: PanelRect,
    /// Full Illustration project YAML (`ProjectFile`) for this panel's scene.
    pub scene_yaml: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// Optional camera variant label (`default`, `orbit_left`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera_variant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_frame_id: Option<String>,
}

/// One comic page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComicPage {
    pub id: String,
    pub layout_id: String,
    /// Logical page width in pixels (render may clamp).
    pub width: u32,
    /// Logical page height in pixels (render may clamp).
    pub height: u32,
    /// Margin as fraction of min(page edge) used when building templates.
    #[serde(default = "default_margin")]
    pub margin: f32,
    /// Gutter as fraction of min(page edge) between panels.
    #[serde(default = "default_gutter")]
    pub gutter: f32,
    pub panels: Vec<ComicPanel>,
}

fn default_margin() -> f32 {
    0.04
}
fn default_gutter() -> f32 {
    0.02
}

/// Comic project root (pages of panels).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComicProject {
    pub format_version: u32,
    #[serde(default = "adr_default")]
    pub conventions: String,
    pub pages: Vec<ComicPage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_page: Option<String>,
}

fn adr_default() -> String {
    "ADR-0011".into()
}

impl ComicProject {
    pub fn validate(&self) -> Result<(), ComicError> {
        if self.format_version != COMIC_FORMAT_VERSION {
            return Err(ComicError::UnsupportedVersion(self.format_version));
        }
        if self.pages.is_empty() {
            return Err(ComicError::NoPanels);
        }
        for page in &self.pages {
            if page.panels.is_empty() {
                return Err(ComicError::NoPanels);
            }
            for panel in &page.panels {
                panel.rect.validate()?;
                let project = load_project_yaml(&panel.scene_yaml)
                    .map_err(|e| ComicError::Yaml(e.to_string()))?;
                project
                    .scene
                    .validate()
                    .map_err(|e| ComicError::Scene(e.to_string()))?;
            }
        }
        Ok(())
    }

    pub fn page_mut(&mut self, page_id: &str) -> Result<&mut ComicPage, ComicError> {
        self.pages
            .iter_mut()
            .find(|p| p.id == page_id)
            .ok_or_else(|| ComicError::UnknownPage(page_id.into()))
    }

    pub fn page(&self, page_id: &str) -> Result<&ComicPage, ComicError> {
        self.pages
            .iter()
            .find(|p| p.id == page_id)
            .ok_or_else(|| ComicError::UnknownPage(page_id.into()))
    }
}

/// Compute normalized panel rects for a template.
pub fn layout_rects(layout: ComicLayoutId, margin: f32, gutter: f32) -> Vec<PanelRect> {
    let m = margin.clamp(0.0, 0.2);
    let g = gutter.clamp(0.0, 0.15);
    let inner_w = 1.0 - 2.0 * m;
    let inner_h = 1.0 - 2.0 * m;
    match layout {
        ComicLayoutId::Single => vec![PanelRect::new(m, m, inner_w, inner_h)],
        ComicLayoutId::TwoHorizontal => {
            let cell_w = (inner_w - g) / 2.0;
            vec![
                PanelRect::new(m, m, cell_w, inner_h),
                PanelRect::new(m + cell_w + g, m, cell_w, inner_h),
            ]
        }
        ComicLayoutId::TwoVertical => {
            let cell_h = (inner_h - g) / 2.0;
            vec![
                PanelRect::new(m, m, inner_w, cell_h),
                PanelRect::new(m, m + cell_h + g, inner_w, cell_h),
            ]
        }
        ComicLayoutId::Grid2x2 => {
            let cell_w = (inner_w - g) / 2.0;
            let cell_h = (inner_h - g) / 2.0;
            vec![
                PanelRect::new(m, m, cell_w, cell_h),
                PanelRect::new(m + cell_w + g, m, cell_w, cell_h),
                PanelRect::new(m, m + cell_h + g, cell_w, cell_h),
                PanelRect::new(m + cell_w + g, m + cell_h + g, cell_w, cell_h),
            ]
        }
        ComicLayoutId::Strip3 => {
            let cell_w = (inner_w - 2.0 * g) / 3.0;
            vec![
                PanelRect::new(m, m, cell_w, inner_h),
                PanelRect::new(m + cell_w + g, m, cell_w, inner_h),
                PanelRect::new(m + 2.0 * (cell_w + g), m, cell_w, inner_h),
            ]
        }
    }
}

/// Camera orbit / framing variants so cloned panels are not identical.
fn camera_variant_for_index(index: usize) -> (&'static str, f32, f32) {
    // (label, yaw_rad around Y, dolly scale on camera translation from origin)
    match index % 4 {
        0 => ("default", 0.0, 1.0),
        1 => ("orbit_left", 0.45, 1.0),
        2 => ("orbit_right", -0.45, 1.0),
        _ => ("close_up", 0.15, 0.72),
    }
}

fn apply_camera_variant(scene: &mut SceneGraph, yaw: f32, dolly: f32) -> Result<(), ComicError> {
    let Some(cam_id) = scene.active_camera.clone() else {
        return Ok(());
    };
    let Some(node) = scene.nodes.get_mut(&cam_id) else {
        return Ok(());
    };
    let yaw_q = Quat::from_axis_angle(Vec3::UNIT_Y, yaw);
    node.transform.rotation = (yaw_q * node.transform.rotation)
        .normalized()
        .ok_or(ComicError::Scene("degenerate camera quat".into()))?;
    node.transform.translation.x *= dolly;
    node.transform.translation.y *= dolly.max(0.85);
    node.transform.translation.z *= dolly;
    node.transform.validate()?;
    Ok(())
}

/// Build a comic project from a layout template and a source SceneGraph.
pub fn apply_comic_layout(
    layout: ComicLayoutId,
    scene: &SceneGraph,
    page_width: u32,
    page_height: u32,
) -> Result<ComicProject, ComicError> {
    scene.validate()?;
    if scene.nodes.is_empty() {
        return Err(ComicError::EmptyScene);
    }
    let margin = default_margin();
    let gutter = default_gutter();
    let rects = layout_rects(layout, margin, gutter);
    let mut panels = Vec::with_capacity(rects.len());
    for (i, rect) in rects.into_iter().enumerate() {
        let mut panel_scene = scene.clone();
        let (variant, yaw, dolly) = camera_variant_for_index(i);
        apply_camera_variant(&mut panel_scene, yaw, dolly)?;
        let yaml = save_project_yaml(&ProjectFile::new(panel_scene))
            .map_err(|e| ComicError::Yaml(e.to_string()))?;
        panels.push(ComicPanel {
            id: format!("panel_{}", i + 1),
            rect,
            scene_yaml: yaml,
            caption: None,
            camera_variant: Some(variant.into()),
            source_frame_id: None,
        });
    }
    let page = ComicPage {
        id: "page_1".into(),
        layout_id: layout.as_str().into(),
        width: page_width.clamp(64, COMIC_MAX_PAGE_EDGE),
        height: page_height.clamp(64, COMIC_MAX_PAGE_EDGE),
        margin,
        gutter,
        panels,
    };
    Ok(ComicProject {
        format_version: COMIC_FORMAT_VERSION,
        conventions: adr_default(),
        pages: vec![page],
        active_page: Some("page_1".into()),
    })
}

/// Bind a SceneGraph into one panel of an existing comic project.
pub fn bind_panel_scene(
    project: &mut ComicProject,
    page_id: &str,
    panel_id: &str,
    scene: &SceneGraph,
) -> Result<(), ComicError> {
    scene.validate()?;
    let page = project.page_mut(page_id)?;
    let panel = page
        .panels
        .iter_mut()
        .find(|p| p.id == panel_id)
        .ok_or_else(|| ComicError::UnknownPanel(panel_id.into()))?;
    panel.scene_yaml = save_project_yaml(&ProjectFile::new(scene.clone()))
        .map_err(|e| ComicError::Yaml(e.to_string()))?;
    panel.camera_variant = Some("bound".into());
    Ok(())
}

pub fn save_comic_yaml(project: &ComicProject) -> Result<String, ComicError> {
    project.validate()?;
    serde_yaml::to_string(project).map_err(|e| ComicError::Yaml(e.to_string()))
}

pub fn load_comic_yaml(yaml: &str) -> Result<ComicProject, ComicError> {
    let project: ComicProject =
        serde_yaml::from_str(yaml).map_err(|e| ComicError::Yaml(e.to_string()))?;
    project.validate()?;
    Ok(project)
}

/// Result of compositing a comic page.
#[derive(Debug, Clone)]
pub struct ComicRenderResult {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub page_id: String,
    pub panel_count: usize,
    pub backend: RenderBackendId,
}

#[allow(clippy::too_many_arguments)]
fn blit_rgba(
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    src: &[u8],
    src_w: u32,
    src_h: u32,
    dst_x: i32,
    dst_y: i32,
) {
    for sy in 0..src_h as i32 {
        let dy = dst_y + sy;
        if dy < 0 || dy >= dst_h as i32 {
            continue;
        }
        for sx in 0..src_w as i32 {
            let dx = dst_x + sx;
            if dx < 0 || dx >= dst_w as i32 {
                continue;
            }
            let si = ((sy as u32 * src_w + sx as u32) * 4) as usize;
            let di = ((dy as u32 * dst_w + dx as u32) * 4) as usize;
            dst[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn stroke_rect_border(
    rgba: &mut [u8],
    w: u32,
    h: u32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    thickness: i32,
    color: [u8; 4],
) {
    for t in 0..thickness {
        for x in x0..=x1 {
            put_px(rgba, w, h, x, y0 + t, color);
            put_px(rgba, w, h, x, y1 - t, color);
        }
        for y in y0..=y1 {
            put_px(rgba, w, h, x0 + t, y, color);
            put_px(rgba, w, h, x1 - t, y, color);
        }
    }
}

fn put_px(rgba: &mut [u8], w: u32, h: u32, x: i32, y: i32, color: [u8; 4]) {
    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
        return;
    }
    let i = ((y as u32 * w + x as u32) * 4) as usize;
    rgba[i..i + 4].copy_from_slice(&color);
}

fn decode_png_rgba(png: &[u8]) -> Result<(u32, u32, Vec<u8>), ComicError> {
    // Minimal IHDR + IDAT inflate for our own encoder output (filter-none + stored zlib).
    if png.len() < 33 || &png[0..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(ComicError::Encode("not a PNG".into()));
    }
    let mut width = 0u32;
    let mut height = 0u32;
    let mut idat = Vec::new();
    let mut offset = 8usize;
    while offset + 12 <= png.len() {
        let len = u32::from_be_bytes(png[offset..offset + 4].try_into().unwrap()) as usize;
        let ty = &png[offset + 4..offset + 8];
        let data_start = offset + 8;
        let data_end = data_start + len;
        if data_end + 4 > png.len() {
            return Err(ComicError::Encode("truncated PNG chunk".into()));
        }
        if ty == b"IHDR" && len >= 8 {
            width = u32::from_be_bytes(png[data_start..data_start + 4].try_into().unwrap());
            height = u32::from_be_bytes(png[data_start + 4..data_start + 8].try_into().unwrap());
        } else if ty == b"IDAT" {
            idat.extend_from_slice(&png[data_start..data_end]);
        } else if ty == b"IEND" {
            break;
        }
        offset = data_end + 4;
    }
    if width == 0 || height == 0 {
        return Err(ComicError::Encode("missing IHDR".into()));
    }
    let raw = inflate_zlib_stored(&idat)?;
    let row_bytes = (width as usize) * 4 + 1;
    let expected = row_bytes * height as usize;
    if raw.len() != expected {
        return Err(ComicError::Encode(format!(
            "PNG raw len {} != expected {expected}",
            raw.len()
        )));
    }
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height as usize {
        let row = &raw[y * row_bytes..(y + 1) * row_bytes];
        if row[0] != 0 {
            return Err(ComicError::Encode("unsupported PNG filter".into()));
        }
        rgba.extend_from_slice(&row[1..]);
    }
    Ok((width, height, rgba))
}

fn inflate_zlib_stored(data: &[u8]) -> Result<Vec<u8>, ComicError> {
    if data.len() < 6 {
        return Err(ComicError::Encode("zlib too short".into()));
    }
    // Skip CMF/FLG; walk stored blocks; ignore trailing Adler-32.
    let mut i = 2usize;
    let mut out = Vec::new();
    loop {
        if i + 5 > data.len() {
            return Err(ComicError::Encode("truncated deflate block".into()));
        }
        let header = data[i];
        let last = header & 1 != 0;
        let btype = (header >> 1) & 3;
        if btype != 0 {
            return Err(ComicError::Encode("only stored deflate supported".into()));
        }
        i += 1;
        let n = u16::from_le_bytes([data[i], data[i + 1]]);
        let ninv = u16::from_le_bytes([data[i + 2], data[i + 3]]);
        if ninv != !n {
            return Err(ComicError::Encode("bad stored length".into()));
        }
        i += 4;
        let n = n as usize;
        if i + n > data.len() {
            return Err(ComicError::Encode("stored block overrun".into()));
        }
        out.extend_from_slice(&data[i..i + n]);
        i += n;
        if last {
            break;
        }
    }
    Ok(out)
}

/// Render each panel via a beauty backend and composite onto a page canvas.
pub fn render_comic_page(
    project: &ComicProject,
    page_id: &str,
    backend: RenderBackendId,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<ComicRenderResult, ComicError> {
    let page = project.page(page_id)?;
    if page.panels.is_empty() {
        return Err(ComicError::NoPanels);
    }
    let page_w = width.unwrap_or(page.width).clamp(64, COMIC_MAX_PAGE_EDGE);
    let page_h = height.unwrap_or(page.height).clamp(64, COMIC_MAX_PAGE_EDGE);

    // Paper / gutter fill (warm off-white — not a purple AI default).
    let paper = [245u8, 241, 232, 255];
    let mut rgba = vec![0u8; (page_w * page_h * 4) as usize];
    for px in rgba.as_chunks_mut::<4>().0 {
        *px = paper;
    }

    let stub = StubRenderBackend;
    let cpu = CpuWireframeBackend;

    for panel in &page.panels {
        let project_file =
            load_project_yaml(&panel.scene_yaml).map_err(|e| ComicError::Yaml(e.to_string()))?;
        let pw = ((panel.rect.w * page_w as f32).round() as u32).clamp(16, COMIC_MAX_PANEL_EDGE);
        let ph = ((panel.rect.h * page_h as f32).round() as u32).clamp(16, COMIC_MAX_PANEL_EDGE);
        let req = RenderRequest {
            scene: project_file.scene,
            pass: RenderPassKind::Beauty,
            width: pw,
            height: ph,
            stub_rgb: (40, 48, 58),
            style: None,
            preset: None,
        };
        let out = match backend {
            RenderBackendId::Stub => stub.render(&req)?,
            RenderBackendId::Cpu => cpu.render(&req)?,
            RenderBackendId::Blender => {
                // Comic composite stays in-process; Blender is beauty-only per panel
                // via CPU fallback when no pack (keeps CI fail-closed without GPL).
                cpu.render(&req)?
            }
        };
        let (sw, sh, src) = decode_png_rgba(&out.png)?;
        let dx = (panel.rect.x * page_w as f32).round() as i32;
        let dy = (panel.rect.y * page_h as f32).round() as i32;
        blit_rgba(&mut rgba, page_w, page_h, &src, sw, sh, dx, dy);
        stroke_rect_border(
            &mut rgba,
            page_w,
            page_h,
            dx,
            dy,
            dx + sw as i32 - 1,
            dy + sh as i32 - 1,
            2,
            [18, 16, 14, 255],
        );
    }

    let png = encode_rgba8_png(page_w, page_h, &rgba).map_err(ComicError::Encode)?;
    Ok(ComicRenderResult {
        png,
        width: page_w,
        height: page_h,
        page_id: page_id.into(),
        panel_count: page.panels.len(),
        backend: match backend {
            RenderBackendId::Blender => RenderBackendId::Cpu,
            other => other,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::SceneGraph;

    #[test]
    fn layout_counts_and_rects_cover_inner() {
        for layout in [
            ComicLayoutId::Single,
            ComicLayoutId::TwoHorizontal,
            ComicLayoutId::TwoVertical,
            ComicLayoutId::Grid2x2,
            ComicLayoutId::Strip3,
        ] {
            let rects = layout_rects(layout, 0.04, 0.02);
            assert_eq!(rects.len(), layout.panel_count());
            for r in rects {
                r.validate().unwrap();
            }
        }
    }

    #[test]
    fn apply_layout_clones_scenes_with_variants() {
        let scene = SceneGraph::demo_scene();
        let comic = apply_comic_layout(ComicLayoutId::Grid2x2, &scene, 640, 960).unwrap();
        assert_eq!(comic.pages[0].panels.len(), 4);
        let yaml = save_comic_yaml(&comic).unwrap();
        let loaded = load_comic_yaml(&yaml).unwrap();
        assert_eq!(loaded.pages[0].layout_id, "grid_2x2");
        let variants: Vec<_> = loaded.pages[0]
            .panels
            .iter()
            .filter_map(|p| p.camera_variant.as_deref())
            .collect();
        assert!(variants.contains(&"default"));
        assert!(variants.contains(&"orbit_left"));
    }

    #[test]
    fn unknown_layout_parse() {
        assert!(ComicLayoutId::parse("hexagon").is_none());
        assert_eq!(ComicLayoutId::parse("grille"), Some(ComicLayoutId::Grid2x2));
        assert_eq!(ComicLayoutId::parse("bande"), Some(ComicLayoutId::Strip3));
    }

    #[test]
    fn render_grid_cpu_produces_png() {
        let scene = SceneGraph::demo_scene();
        let comic = apply_comic_layout(ComicLayoutId::TwoHorizontal, &scene, 320, 240).unwrap();
        let res = render_comic_page(&comic, "page_1", RenderBackendId::Cpu, Some(320), Some(240))
            .unwrap();
        assert!(res.png.starts_with(b"\x89PNG"));
        assert_eq!(res.panel_count, 2);
        assert_eq!(res.width, 320);
    }

    #[test]
    fn bind_panel_replaces_scene() {
        let scene = SceneGraph::demo_scene();
        let mut comic = apply_comic_layout(ComicLayoutId::Single, &scene, 200, 200).unwrap();
        let mut other = scene.clone();
        if let Some(box_n) = other.nodes.get_mut("box") {
            box_n.transform.translation.y = 1.5;
        }
        bind_panel_scene(&mut comic, "page_1", "panel_1", &other).unwrap();
        let loaded = load_project_yaml(&comic.pages[0].panels[0].scene_yaml).unwrap();
        assert!((loaded.scene.nodes["box"].transform.translation.y - 1.5).abs() < 1e-5);
    }

    #[test]
    fn panel_caption_and_storyboard_link_round_trip() {
        let scene = SceneGraph::demo_scene();
        let mut comic = apply_comic_layout(ComicLayoutId::Single, &scene, 200, 200).unwrap();
        comic.pages[0].panels[0].caption = Some("The library".into());
        comic.pages[0].panels[0].source_frame_id = Some("frame_1".into());
        let loaded = load_comic_yaml(&save_comic_yaml(&comic).unwrap()).unwrap();
        assert_eq!(
            loaded.pages[0].panels[0].caption.as_deref(),
            Some("The library")
        );
        assert_eq!(
            loaded.pages[0].panels[0].source_frame_id.as_deref(),
            Some("frame_1")
        );
    }
}
