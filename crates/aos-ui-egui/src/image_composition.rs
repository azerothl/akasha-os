//! Visual composition canvas + prompt layout injection for Image Studio.

use crate::i18n::UiStrings;
use crate::image_prompt::{prompt_enrichment_kind, PromptEnrichmentKind};
use eframe::egui;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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

/// Right-pane composition editor (aspect frame + overlapping blocks).
pub fn ui_composition_canvas(
    ui: &mut egui::Ui,
    t: &UiStrings,
    frame_w: u32,
    frame_h: u32,
    blocks: &mut Vec<CompositionBlock>,
    selected: &mut Option<u64>,
    next_id: &mut u64,
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
    if response.clicked() {
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

    if response.drag_started() {
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

    if response.dragged() {
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

    if response.drag_stopped() {
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
