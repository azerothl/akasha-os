//! Visual composition canvas + prompt layout injection for Image Studio.

use crate::decl_ui;
use crate::i18n::UiStrings;
use crate::image_prompt::{prompt_enrichment_kind, PromptEnrichmentKind};
use eframe::egui;
use image::{GrayImage, Luma};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Cursor;
use std::path::PathBuf;

/// Placement block in normalized frame coords (0..1). Vec order = z-order (back → front).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionBlock {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub desc: String,
}

impl CompositionBlock {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            x: 0.35,
            y: 0.35,
            w: 0.30,
            h: 0.30,
            desc: String::new(),
        }
    }

    pub fn clamp_in_frame(&mut self) {
        self.w = self.w.clamp(0.05, 1.0);
        self.h = self.h.clamp(0.05, 1.0);
        self.x = self.x.clamp(0.0, 1.0 - self.w);
        self.y = self.y.clamp(0.0, 1.0 - self.h);
    }

    /// Ideogram bbox `[ymin, xmin, ymax, xmax]` in 0–1000.
    pub fn ideogram_bbox(&self) -> [i32; 4] {
        let xmin = (self.x * 1000.0).round() as i32;
        let ymin = (self.y * 1000.0).round() as i32;
        let xmax = ((self.x + self.w) * 1000.0).round() as i32;
        let ymax = ((self.y + self.h) * 1000.0).round() as i32;
        [
            ymin.clamp(0, 1000),
            xmin.clamp(0, 1000),
            ymax.clamp(0, 1000),
            xmax.clamp(0, 1000),
        ]
    }
}

fn active_blocks(blocks: &[CompositionBlock]) -> Vec<&CompositionBlock> {
    blocks
        .iter()
        .filter(|b| !b.desc.trim().is_empty())
        .collect()
}

fn plain_layout_transcript(blocks: &[&CompositionBlock]) -> String {
    let parts: Vec<String> = blocks
        .iter()
        .enumerate()
        .map(|(z, b)| {
            let left = (b.x * 100.0).round() as i32;
            let top = (b.y * 100.0).round() as i32;
            let w = (b.w * 100.0).round() as i32;
            let h = (b.h * 100.0).round() as i32;
            format!(
                "[z{z} left {left}%, top {top}%, {w}%×{h}%] {}",
                b.desc.trim()
            )
        })
        .collect();
    format!("Composition (back to front): {}", parts.join("; "))
}

fn ideogram_elements(blocks: &[&CompositionBlock]) -> Vec<Value> {
    blocks
        .iter()
        .map(|b| {
            json!({
                "type": "obj",
                "desc": b.desc.trim(),
                "bbox": b.ideogram_bbox(),
            })
        })
        .collect()
}

fn generic_layout_array(blocks: &[&CompositionBlock]) -> Vec<Value> {
    blocks
        .iter()
        .enumerate()
        .map(|(z, b)| {
            json!({
                "desc": b.desc.trim(),
                "x": (b.x * 1000.0).round() / 1000.0,
                "y": (b.y * 1000.0).round() / 1000.0,
                "w": (b.w * 1000.0).round() / 1000.0,
                "h": (b.h * 1000.0).round() / 1000.0,
                "z": z,
            })
        })
        .collect()
}

fn minify_json(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "{}".into())
}

/// LLM sometimes emits this typo instead of `compositional_deconstruction`.
const IDEOGRAM_DECONSTRUCTION_ALIASES: &[&str] = &[
    "compositional_destruction",
    "compositional_deconstrution",
    "composition_deconstruction",
];

fn take_ideogram_background(obj: &serde_json::Map<String, Value>) -> Value {
    if let Some(bg) = obj
        .get("compositional_deconstruction")
        .and_then(|c| c.get("background"))
    {
        if !bg.as_str().map(|s| s.trim().is_empty()).unwrap_or(true) {
            return bg.clone();
        }
        // Keep empty only if no richer alias exists below.
        let empty = bg.clone();
        for alias in IDEOGRAM_DECONSTRUCTION_ALIASES {
            if let Some(bg2) = obj.get(*alias).and_then(|c| c.get("background")) {
                if !bg2.as_str().map(|s| s.trim().is_empty()).unwrap_or(true) {
                    return bg2.clone();
                }
            }
        }
        return empty;
    }
    for alias in IDEOGRAM_DECONSTRUCTION_ALIASES {
        if let Some(bg) = obj.get(*alias).and_then(|c| c.get("background")) {
            return bg.clone();
        }
    }
    json!("")
}

fn strip_ideogram_deconstruction_aliases(obj: &mut serde_json::Map<String, Value>) {
    for alias in IDEOGRAM_DECONSTRUCTION_ALIASES {
        obj.remove(*alias);
    }
}

/// Build a full prompt from the global text + layout blocks (no prior LLM pass).
pub fn compose_prompt_with_layout(
    base: &str,
    blocks: &[CompositionBlock],
    model_id: Option<&str>,
) -> String {
    let active = active_blocks(blocks);
    if active.is_empty() {
        return base.to_string();
    }
    let base = base.trim();
    match model_id.and_then(prompt_enrichment_kind) {
        Some(PromptEnrichmentKind::Ideogram4) => {
            let v = json!({
                "high_level_description": base,
                "compositional_deconstruction": {
                    "background": "",
                    "elements": ideogram_elements(&active),
                }
            });
            minify_json(&v)
        }
        Some(PromptEnrichmentKind::GenericJson) => {
            let v = json!({
                "subject": base,
                "composition": plain_layout_transcript(&active),
                "layout": generic_layout_array(&active),
            });
            minify_json(&v)
        }
        None => {
            if base.is_empty() {
                plain_layout_transcript(&active)
            } else {
                format!("{base}\n{}", plain_layout_transcript(&active))
            }
        }
    }
}

/// Merge layout blocks into an already-enriched prompt (JSON or prose).
pub fn merge_layout_into_prompt(
    prompt: &str,
    blocks: &[CompositionBlock],
    model_id: Option<&str>,
) -> String {
    let active = active_blocks(blocks);
    if active.is_empty() {
        return prompt.to_string();
    }
    let trimmed = prompt.trim();
    if let Ok(mut v) = serde_json::from_str::<Value>(trimmed) {
        if let Some(obj) = v.as_object_mut() {
            let has_ideogram_comp = obj.contains_key("compositional_deconstruction")
                || IDEOGRAM_DECONSTRUCTION_ALIASES
                    .iter()
                    .any(|k| obj.contains_key(*k));
            // Ideogram-style
            if has_ideogram_comp
                || matches!(
                    model_id.and_then(prompt_enrichment_kind),
                    Some(PromptEnrichmentKind::Ideogram4)
                )
            {
                let background = take_ideogram_background(obj);
                strip_ideogram_deconstruction_aliases(obj);
                obj.insert(
                    "compositional_deconstruction".into(),
                    json!({
                        "background": background,
                        "elements": ideogram_elements(&active),
                    }),
                );
                if !obj.contains_key("high_level_description") {
                    obj.insert("high_level_description".into(), json!(""));
                }
                return minify_json(&v);
            }
            // Generic JSON
            obj.insert("layout".into(), json!(generic_layout_array(&active)));
            obj.insert(
                "composition".into(),
                json!(plain_layout_transcript(&active)),
            );
            return minify_json(&v);
        }
    }
    // Prose / non-JSON
    format!("{trimmed}\n{}", plain_layout_transcript(&active))
}

/// Apply layout after optional LLM enrich / edited prompt.
pub fn finalize_prompt_with_layout(
    base_or_enriched: &str,
    blocks: &[CompositionBlock],
    model_id: Option<&str>,
    had_prior_enrichment: bool,
) -> String {
    let active = active_blocks(blocks);
    if active.is_empty() {
        return base_or_enriched.to_string();
    }
    if had_prior_enrichment || serde_json::from_str::<Value>(base_or_enriched.trim()).is_ok() {
        merge_layout_into_prompt(base_or_enriched, blocks, model_id)
    } else {
        compose_prompt_with_layout(base_or_enriched, blocks, model_id)
    }
}

#[derive(Clone, Default)]
struct DragState {
    block_id: u64,
    mode: DragMode,
    start_pointer: egui::Pos2,
    orig_x: f32,
    orig_y: f32,
    orig_w: f32,
    orig_h: f32,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum DragMode {
    #[default]
    Move,
    ResizeSe,
}

/// Block drag/resize is disabled above this overlay opacity when a preview is shown.
pub const OVERLAY_BLOCK_EDIT_THRESHOLD: f32 = 0.20;

/// In-memory inpaint mask aligned to the generation frame (255 = regenerate region).
#[derive(Debug, Clone)]
pub struct InpaintMask {
    pub width: u32,
    pub height: u32,
    pixels: Vec<u8>,
}

impl InpaintMask {
    pub fn new(width: u32, height: u32) -> Self {
        let n = (width as usize).saturating_mul(height as usize);
        Self {
            width,
            height,
            pixels: vec![0; n],
        }
    }

    pub fn ensure_size(&mut self, width: u32, height: u32) {
        if self.width != width || self.height != height {
            *self = Self::new(width, height);
        }
    }

    pub fn has_paint(&self) -> bool {
        self.pixels.iter().any(|&v| v > 0)
    }

    pub fn clear(&mut self) {
        self.pixels.fill(0);
    }

    pub fn paint_brush(&mut self, nx: f32, ny: f32, radius_norm: f32) {
        let cx = (nx.clamp(0.0, 1.0) * self.width as f32) as i32;
        let cy = (ny.clamp(0.0, 1.0) * self.height as f32) as i32;
        let r = (radius_norm * self.width.min(self.height) as f32).max(2.0) as i32;
        let r2 = r * r;
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy > r2 {
                    continue;
                }
                let x = cx + dx;
                let y = cy + dy;
                if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
                    continue;
                }
                let idx = (y as u32 * self.width + x as u32) as usize;
                self.pixels[idx] = 255;
            }
        }
    }

    pub fn save_logical_png(&self, logical_path: &str) -> Result<(), String> {
        let host = logical_host_path(logical_path);
        if let Some(parent) = host.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut img = GrayImage::new(self.width, self.height);
        for (i, &v) in self.pixels.iter().enumerate() {
            let x = (i as u32) % self.width;
            let y = (i as u32) / self.width;
            img.put_pixel(x, y, Luma([v]));
        }
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        std::fs::write(&host, buf).map_err(|e| e.to_string())
    }
}

pub fn new_inpaint_mask_path() -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("/downloads/inpaint-mask-{ts}.png")
}

fn logical_host_path(logical: &str) -> PathBuf {
    let trimmed = logical.trim();
    if trimmed.starts_with('/') {
        crate::os_open::aos_home()
            .join("var/storage/data")
            .join(trimmed.trim_start_matches('/'))
    } else {
        PathBuf::from(trimmed)
    }
}

pub fn overlay_allows_block_editing(preview_path: Option<&str>, overlay_opacity: f32) -> bool {
    preview_path.is_none() || overlay_opacity <= OVERLAY_BLOCK_EDIT_THRESHOLD
}

/// Right-pane composition editor (aspect frame + overlapping blocks).
#[allow(clippy::too_many_arguments)] // Composition editor state is explicit at the UI boundary.
pub fn ui_composition_canvas(
    ui: &mut egui::Ui,
    t: &UiStrings,
    frame_w: u32,
    frame_h: u32,
    blocks: &mut Vec<CompositionBlock>,
    selected: &mut Option<u64>,
    next_id: &mut u64,
    preview_path: Option<&str>,
    overlay_opacity: &mut f32,
    inpaint_mode: &mut bool,
    inpaint_mask: &mut Option<InpaintMask>,
    inpaint_brush: &mut f32,
) {
    ui.heading(t.studio_composition_heading);
    ui.weak(t.studio_composition_blurb);
    help_row(ui, t.studio_composition_help);

    ui.horizontal(|ui| {
        if ui.button(t.studio_composition_add).clicked() {
            let id = *next_id;
            *next_id = next_id.saturating_add(1);
            let mut b = CompositionBlock::new(id);
            // Slight offset so stacked adds stay visible
            let n = blocks.len() as f32;
            b.x = (0.35 + n * 0.03) % 0.55;
            b.y = (0.35 + n * 0.03) % 0.55;
            b.clamp_in_frame();
            blocks.push(b);
            *selected = Some(id);
            if preview_path.is_some() && *overlay_opacity > OVERLAY_BLOCK_EDIT_THRESHOLD {
                *overlay_opacity = OVERLAY_BLOCK_EDIT_THRESHOLD;
            }
        }
        let can_del = selected.is_some();
        if ui
            .add_enabled(can_del, egui::Button::new(t.studio_composition_remove))
            .clicked()
        {
            if let Some(id) = *selected {
                blocks.retain(|b| b.id != id);
                *selected = blocks.last().map(|b| b.id);
            }
        }
        if ui
            .add_enabled(!blocks.is_empty(), egui::Button::new(t.studio_composition_clear))
            .clicked()
        {
            blocks.clear();
            *selected = None;
        }
        ui.weak(format!("{}×{}", frame_w.max(1), frame_h.max(1)));
    });

    if preview_path.is_some() {
        ui.horizontal(|ui| {
            ui.label(t.studio_preview_overlay);
            ui.label(t.studio_preview_opacity);
            help_row(ui, t.studio_preview_opacity_help);
            let mut pct = (*overlay_opacity * 100.0).round().clamp(0.0, 100.0);
            if ui
                .add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix("%"))
                .changed()
            {
                *overlay_opacity = (pct / 100.0).clamp(0.0, 1.0);
            }
        });
        ui.horizontal(|ui| {
            if ui.checkbox(inpaint_mode, t.studio_inpaint_mode).changed() && !*inpaint_mode {
                if let Some(mask) = inpaint_mask.as_mut() {
                    mask.clear();
                }
            }
            if *inpaint_mode {
                ui.label(t.studio_inpaint_brush);
                ui.add(
                    egui::Slider::new(inpaint_brush, 4.0..=80.0)
                        .suffix("px")
                        .logarithmic(true),
                );
                if ui.button(t.studio_inpaint_clear).clicked() {
                    if let Some(mask) = inpaint_mask.as_mut() {
                        mask.clear();
                    }
                }
            }
        });
        if *inpaint_mode {
            ui.weak(t.studio_inpaint_help);
        }
        if !overlay_allows_block_editing(preview_path, *overlay_opacity) {
            ui.weak(t.studio_preview_drag_locked);
        }
    }

    if !blocks.is_empty() && active_blocks(blocks).is_empty() {
        ui.weak(t.studio_composition_empty_desc_hint);
    }

    let inpaint_active = preview_path.is_some() && *inpaint_mode;
    if inpaint_active {
        *overlay_opacity = 1.0;
    }
    let allow_block_edit =
        overlay_allows_block_editing(preview_path, *overlay_opacity) && !inpaint_active;
    let avail = ui.available_width().min(520.0);
    let aspect = frame_w.max(1) as f32 / frame_h.max(1) as f32;
    let (canvas_w, canvas_h) = if aspect >= 1.0 {
        (avail, avail / aspect)
    } else {
        (avail * aspect, avail)
    };
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(canvas_w, canvas_h), egui::Sense::click_and_drag());

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::from_gray(28));
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.5_f32, egui::Color32::from_gray(90)),
        egui::StrokeKind::Inside,
    );

    let to_screen = |x: f32, y: f32, w: f32, h: f32| -> egui::Rect {
        egui::Rect::from_min_size(
            egui::pos2(rect.left() + x * rect.width(), rect.top() + y * rect.height()),
            egui::vec2(w * rect.width(), h * rect.height()),
        )
    };

    // Hit-test front → back
    let pointer = response.interact_pointer_pos();
    if allow_block_edit && response.clicked() {
        if let Some(pos) = pointer {
            let mut hit: Option<u64> = None;
            for b in blocks.iter().rev() {
                let r = to_screen(b.x, b.y, b.w, b.h);
                if r.contains(pos) {
                    hit = Some(b.id);
                    break;
                }
            }
            if let Some(id) = hit {
                bring_to_front(blocks, id);
                *selected = Some(id);
            } else {
                *selected = None;
            }
        }
    }

    let drag_id = egui::Id::new("image_comp_drag");
    let mut drag: Option<DragState> = ui.ctx().data(|d| d.get_temp(drag_id));

    if allow_block_edit && response.drag_started() {
        if let Some(pos) = pointer {
            let mut started = false;
            // Prefer resize handle of selected, then body hit front→back
            if let Some(sel) = *selected {
                if let Some(b) = blocks.iter().find(|b| b.id == sel) {
                    let r = to_screen(b.x, b.y, b.w, b.h);
                    let handle = resize_handle_rect(r);
                    if handle.contains(pos) {
                        drag = Some(DragState {
                            block_id: b.id,
                            mode: DragMode::ResizeSe,
                            start_pointer: pos,
                            orig_x: b.x,
                            orig_y: b.y,
                            orig_w: b.w,
                            orig_h: b.h,
                        });
                        started = true;
                    }
                }
            }
            if !started {
                let mut hit_id: Option<u64> = None;
                for b in blocks.iter().rev() {
                    let r = to_screen(b.x, b.y, b.w, b.h);
                    if r.contains(pos) {
                        hit_id = Some(b.id);
                        break;
                    }
                }
                if let Some(id) = hit_id {
                    bring_to_front(blocks, id);
                    *selected = Some(id);
                    if let Some(b) = blocks.iter().find(|b| b.id == id) {
                        drag = Some(DragState {
                            block_id: b.id,
                            mode: DragMode::Move,
                            start_pointer: pos,
                            orig_x: b.x,
                            orig_y: b.y,
                            orig_w: b.w,
                            orig_h: b.h,
                        });
                    }
                }
            }
        }
    }

    if allow_block_edit && response.dragged() {
        if let (Some(d), Some(pos)) = (drag.as_ref(), pointer) {
            let dx = (pos.x - d.start_pointer.x) / rect.width().max(1.0);
            let dy = (pos.y - d.start_pointer.y) / rect.height().max(1.0);
            if let Some(b) = blocks.iter_mut().find(|b| b.id == d.block_id) {
                match d.mode {
                    DragMode::Move => {
                        b.x = d.orig_x + dx;
                        b.y = d.orig_y + dy;
                    }
                    DragMode::ResizeSe => {
                        b.w = d.orig_w + dx;
                        b.h = d.orig_h + dy;
                    }
                }
                b.clamp_in_frame();
            }
        }
    }

    if inpaint_active && response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            if rect.contains(pos) {
                let nx = (pos.x - rect.left()) / rect.width().max(1.0);
                let ny = (pos.y - rect.top()) / rect.height().max(1.0);
                let mask = inpaint_mask.get_or_insert_with(|| InpaintMask::new(frame_w, frame_h));
                mask.ensure_size(frame_w, frame_h);
                let brush_norm = *inpaint_brush / rect.width().max(1.0);
                mask.paint_brush(nx, ny, brush_norm);
            }
        }
    }

    if allow_block_edit && response.drag_stopped() {
        drag = None;
    }
    if !allow_block_edit {
        drag = None;
    }
    ui.ctx().data_mut(|data| {
        if let Some(d) = drag {
            data.insert_temp(drag_id, d);
        } else {
            data.remove::<DragState>(drag_id);
        }
    });

    // Draw back → front
    for (i, b) in blocks.iter().enumerate() {
        let r = to_screen(b.x, b.y, b.w, b.h);
        let selected_here = *selected == Some(b.id);
        let fill = if selected_here {
            egui::Color32::from_rgba_unmultiplied(80, 140, 220, 90)
        } else {
            let hue = ((40 + i * 37) % 180) as u8;
            egui::Color32::from_rgba_unmultiplied(60 + hue / 2, 100, 160, 70)
        };
        let stroke = if selected_here {
            egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(120, 190, 255))
        } else {
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgba_unmultiplied(200, 200, 220, 160))
        };
        painter.rect_filled(r, 3.0, fill);
        painter.rect_stroke(r, 3.0, stroke, egui::StrokeKind::Inside);
        let label = if b.desc.trim().is_empty() {
            format!("#{}", i + 1)
        } else {
            truncate(&b.desc, 28)
        };
        painter.text(
            r.left_top() + egui::vec2(6.0, 4.0),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(12.0),
            egui::Color32::WHITE,
        );
        if selected_here {
            let handle = resize_handle_rect(r);
            painter.rect_filled(handle, 2.0, egui::Color32::from_rgb(220, 230, 255));
        }
    }

    if let Some(path) = preview_path {
        if let Some(tex) = decl_ui::try_load_png(ui.ctx(), path) {
            let alpha = (overlay_opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
            if alpha > 0 {
                painter.image(
                    tex.id(),
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::from_white_alpha(alpha),
                );
            }
        }
    }

    if inpaint_active {
        if let Some(mask) = inpaint_mask.as_ref() {
            paint_mask_overlay(&painter, rect, mask);
        }
    }

    if let Some(id) = *selected {
        if let Some(idx) = blocks.iter().position(|b| b.id == id) {
            ui.add_space(6.0);
            ui.label(t.studio_composition_block_desc);
            ui.add(
                egui::TextEdit::multiline(&mut blocks[idx].desc)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .hint_text(t.studio_composition_block_hint),
            );
        }
    } else if !blocks.is_empty() {
        ui.weak(t.studio_composition_select_hint);
    }
}

fn help_row(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.weak("?");
        ui.weak(text);
    });
}

fn paint_mask_overlay(painter: &egui::Painter, rect: egui::Rect, mask: &InpaintMask) {
    if !mask.has_paint() {
        return;
    }
    let w = mask.width.max(1) as f32;
    let h = mask.height.max(1) as f32;
    for y in (0..mask.height).step_by(2) {
        for x in (0..mask.width).step_by(2) {
            let idx = (y * mask.width + x) as usize;
            if mask.pixels[idx] == 0 {
                continue;
            }
            let px = rect.left() + (x as f32 / w) * rect.width();
            let py = rect.top() + (y as f32 / h) * rect.height();
            let cell_w = rect.width() / w * 2.0;
            let cell_h = rect.height() / h * 2.0;
            painter.rect_filled(
                egui::Rect::from_min_size(egui::pos2(px, py), egui::vec2(cell_w, cell_h)),
                0.0,
                egui::Color32::from_rgba_unmultiplied(255, 60, 60, 120),
            );
        }
    }
}

fn resize_handle_rect(r: egui::Rect) -> egui::Rect {
    let s = 12.0_f32;
    egui::Rect::from_min_size(
        egui::pos2(r.right() - s, r.bottom() - s),
        egui::vec2(s, s),
    )
}

fn bring_to_front(blocks: &mut Vec<CompositionBlock>, id: u64) {
    if let Some(i) = blocks.iter().position(|b| b.id == id) {
        let b = blocks.remove(i);
        blocks.push(b);
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max_chars {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_edit_threshold() {
        assert!(overlay_allows_block_editing(None, 1.0));
        assert!(overlay_allows_block_editing(Some("/downloads/x.png"), 0.0));
        assert!(overlay_allows_block_editing(
            Some("/downloads/x.png"),
            OVERLAY_BLOCK_EDIT_THRESHOLD
        ));
        assert!(!overlay_allows_block_editing(
            Some("/downloads/x.png"),
            OVERLAY_BLOCK_EDIT_THRESHOLD + 0.01
        ));
    }

    #[test]
    fn inpaint_mask_paint_and_save() {
        let mut mask = InpaintMask::new(8, 8);
        assert!(!mask.has_paint());
        mask.paint_brush(0.5, 0.5, 0.25);
        assert!(mask.has_paint());
        let logical = format!("/downloads/test-mask-{}.png", std::process::id());
        mask.save_logical_png(&logical).expect("save mask");
        let host = logical_host_path(&logical);
        assert!(host.is_file());
        let _ = std::fs::remove_file(host);
    }

    #[test]
    fn ideogram_bbox_order() {
        let b = CompositionBlock {
            id: 1,
            x: 0.1,
            y: 0.2,
            w: 0.3,
            h: 0.4,
            desc: "cat".into(),
        };
        assert_eq!(b.ideogram_bbox(), [200, 100, 600, 400]);
    }

    #[test]
    fn overlap_preserved_in_elements() {
        let blocks = vec![
            CompositionBlock {
                id: 1,
                x: 0.0,
                y: 0.0,
                w: 0.8,
                h: 0.8,
                desc: "background tree".into(),
            },
            CompositionBlock {
                id: 2,
                x: 0.3,
                y: 0.3,
                w: 0.4,
                h: 0.4,
                desc: "person in front".into(),
            },
        ];
        let p = compose_prompt_with_layout("scene", &blocks, Some("local:ideogram-4"));
        let v: Value = serde_json::from_str(&p).unwrap();
        let els = v["compositional_deconstruction"]["elements"]
            .as_array()
            .unwrap();
        assert_eq!(els.len(), 2);
        assert_eq!(els[0]["desc"], "background tree");
        assert_eq!(els[1]["desc"], "person in front");
    }

    #[test]
    fn plain_fallback() {
        let blocks = vec![CompositionBlock {
            id: 1,
            x: 0.1,
            y: 0.2,
            w: 0.3,
            h: 0.4,
            desc: "vase".into(),
        }];
        let p = compose_prompt_with_layout("still life", &blocks, Some("local:unknown-model"));
        assert!(p.contains("still life"));
        assert!(p.contains("Composition (back to front)"));
        assert!(p.contains("vase"));
    }

    #[test]
    fn merge_recovers_background_from_destruction_typo() {
        let llm = r#"{"high_level_description":"alley","style_description":{"aesthetics":"cyber"},"compositional_destruction":{"background":"dark cyberpunk alley","elements":[{"type":"obj","desc":"a cat"}]}}"#;
        let blocks = vec![CompositionBlock {
            id: 1,
            x: 0.35,
            y: 0.5,
            w: 0.3,
            h: 0.3,
            desc: "studio cat with cyber paw".into(),
        }];
        let p = merge_layout_into_prompt(llm, &blocks, Some("local:ideogram4"));
        let v: Value = serde_json::from_str(&p).unwrap();
        assert!(v.get("compositional_destruction").is_none());
        assert_eq!(
            v["compositional_deconstruction"]["background"],
            "dark cyberpunk alley"
        );
        let els = v["compositional_deconstruction"]["elements"]
            .as_array()
            .unwrap();
        assert_eq!(els.len(), 1);
        assert_eq!(els[0]["desc"], "studio cat with cyber paw");
    }
}
