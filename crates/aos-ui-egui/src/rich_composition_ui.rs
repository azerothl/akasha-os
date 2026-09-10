//! Generic layer canvas / list / undo widgets for rich declarative UI (issue #150 lot 5).

use aos_proto::decl_ui::{DeclUiDocument, DeclUiWidget};
use aos_proto::rich_composition::{
    bring_layer_to_front, layers_from_value, layers_to_value, reorder_layer, BoundedUndoStack,
    LayerCanvasSnapshot, RichLayer, MAX_LAYERS_PER_CANVAS, MAX_UNDO_DEPTH,
};
use aos_proto::rich_decl_ui::RichInteractionEvent;
use crate::icons;
use eframe::egui;
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragMode {
    Move,
    ResizeSe,
}

#[derive(Clone, Debug)]
struct DragState {
    block_id: u64,
    mode: DragMode,
    start_pointer: egui::Pos2,
    orig_x: f32,
    orig_y: f32,
    orig_w: f32,
    orig_h: f32,
}

/// Host-owned per-canvas interaction + undo state (not serialized to package).
#[derive(Debug, Default)]
pub struct LayerCanvasHostState {
    pub undo: BoundedUndoStack,
    drag: Option<DragState>,
    pub interaction_id: String,
    pub focused: bool,
    drag_layer_index: Option<usize>,
}

impl LayerCanvasHostState {
    pub fn new() -> Self {
        Self {
            undo: BoundedUndoStack::new(MAX_UNDO_DEPTH),
            drag: None,
            interaction_id: String::new(),
            focused: false,
            drag_layer_index: None,
        }
    }
}

pub struct LayerCanvasPatch {
    pub layers_key: String,
    pub selected_key: String,
    pub next_id_key: Option<String>,
    pub layers: Value,
    pub selected_id: Value,
    pub next_id: Option<Value>,
    pub interaction: Option<RichInteractionEvent>,
}

fn widget_label(w: &DeclUiWidget, doc: &DeclUiDocument, language: &str) -> Option<String> {
    let key = w.label_key.as_deref().filter(|k| !k.is_empty())?;
    doc.labels.as_ref()?.resolve(language, key)
}

fn read_u64(local: &HashMap<String, Value>, key: &str) -> Option<u64> {
    local.get(key).and_then(|v| v.as_u64())
}

fn read_layers(local: &HashMap<String, Value>, key: &str) -> Vec<RichLayer> {
    local.get(key).map(layers_from_value).unwrap_or_default()
}

fn snapshot_from_local(
    local: &HashMap<String, Value>,
    layers_key: &str,
    selected_key: &str,
    next_id_key: Option<&str>,
) -> LayerCanvasSnapshot {
    let layers = read_layers(local, layers_key);
    let selected_id = read_u64(local, selected_key);
    let next_id = next_id_key
        .and_then(|k| read_u64(local, k))
        .unwrap_or_else(|| layers.iter().map(|l| l.id).max().unwrap_or(0) + 1);
    LayerCanvasSnapshot::from_parts(layers, selected_id, next_id)
}

fn apply_snapshot_patch(
    snap: &LayerCanvasSnapshot,
    layers_key: &str,
    selected_key: &str,
    next_id_key: Option<&str>,
) -> LayerCanvasPatch {
    LayerCanvasPatch {
        layers_key: layers_key.to_string(),
        selected_key: selected_key.to_string(),
        next_id_key: next_id_key.map(str::to_string),
        layers: layers_to_value(&snap.layers),
        selected_id: snap.selected_id.map(Value::from).unwrap_or(Value::Null),
        next_id: next_id_key.map(|_| Value::from(snap.next_id)),
        interaction: None,
    }
}

fn push_undo(
    host: &mut LayerCanvasHostState,
    local: &HashMap<String, Value>,
    layers_key: &str,
    selected_key: &str,
    next_id_key: Option<&str>,
) {
    let snap = snapshot_from_local(local, layers_key, selected_key, next_id_key);
    host.undo.push(snap);
}

pub fn patch_to_local_map(patch: &LayerCanvasPatch) -> HashMap<String, Value> {
    let mut out = HashMap::new();
    out.insert(patch.layers_key.clone(), patch.layers.clone());
    out.insert(patch.selected_key.clone(), patch.selected_id.clone());
    if let (Some(key), Some(val)) = (&patch.next_id_key, &patch.next_id) {
        out.insert(key.clone(), val.clone());
    }
    out
}

/// Render interactive layer canvas; returns local-state patches when edits commit.
pub fn ui_layer_canvas(
    ui: &mut egui::Ui,
    w: &DeclUiWidget,
    doc: &DeclUiDocument,
    language: &str,
    local_state: &HashMap<String, Value>,
    host: &mut LayerCanvasHostState,
    canvas_id: &str,
    background_path: Option<&str>,
    layer_opacity: f32,
    aspect_override: Option<(u32, u32)>,
) -> Option<LayerCanvasPatch> {
    let layers_key = w.layers_key.as_deref().unwrap_or("layers");
    let selected_key = w.selected_key.as_deref().unwrap_or("selected_id");
    let next_id_key = w.next_id_key.as_deref();
    let (aspect_w, aspect_h) = aspect_override
        .or_else(|| w.aspect_w.zip(w.aspect_h))
        .unwrap_or((16, 9));
    let aspect_w = aspect_w.max(1);
    let aspect_h = aspect_h.max(1);

    let mut layers = read_layers(local_state, layers_key);
    let mut selected = read_u64(local_state, selected_key);
    let mut next_id = next_id_key
        .and_then(|k| read_u64(local_state, k))
        .unwrap_or_else(|| layers.iter().map(|l| l.id).max().unwrap_or(0) + 1);

    let mut patch: Option<LayerCanvasPatch> = None;
    let t = crate::i18n::strings(language);

    if let Some(label) = widget_label(w, doc, language) {
        ui.heading(label);
    }

    ui.horizontal(|ui| {
        let add_label = widget_label_from_key(doc, language, "layer_add")
            .unwrap_or_else(|| t.decl_layer_add.to_string());
        if ui.button(add_label).clicked() && layers.len() < MAX_LAYERS_PER_CANVAS as usize {
            push_undo(host, local_state, layers_key, selected_key, next_id_key);
            let id = next_id;
            next_id += 1;
            let mut layer = RichLayer::new(id);
            let n = layers.len() as f32;
            layer.x = (0.35 + n * 0.03) % 0.55;
            layer.y = (0.35 + n * 0.03) % 0.55;
            layer.clamp_in_frame();
            layer.label = layer_display_name(&layer, layers.len(), doc, language, &t);
            layers.push(layer);
            selected = Some(id);
            patch = Some(build_patch(
                layers_key,
                selected_key,
                next_id_key,
                &layers,
                selected,
                next_id,
                canvas_id,
                host,
                "add",
            ));
        }
        let remove_label = widget_label_from_key(doc, language, "layer_remove")
            .unwrap_or_else(|| t.decl_layer_remove.to_string());
        if ui
            .add_enabled(selected.is_some(), egui::Button::new(remove_label))
            .clicked()
        {
            push_undo(host, local_state, layers_key, selected_key, next_id_key);
            if let Some(id) = selected {
                layers.retain(|l| l.id != id);
                selected = layers.last().map(|l| l.id);
                patch = Some(build_patch(
                    layers_key,
                    selected_key,
                    next_id_key,
                    &layers,
                    selected,
                    next_id,
                    canvas_id,
                    host,
                    "remove",
                ));
            }
        }
    });

    if host.focused {
        // `command` maps to Cmd on macOS and Ctrl on other platforms.
        let undo =
            ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z) && !i.modifiers.shift);
        let redo = ui.input(|i| {
            (i.modifiers.command && i.key_pressed(egui::Key::Y))
                || (i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::Z))
        });
        if undo {
            let current = snapshot_from_local(local_state, layers_key, selected_key, next_id_key);
            if let Some(prev) = host.undo.undo(current) {
                patch = Some(apply_snapshot_patch(
                    &prev,
                    layers_key,
                    selected_key,
                    next_id_key,
                ));
            }
        } else if redo {
            let current = snapshot_from_local(local_state, layers_key, selected_key, next_id_key);
            if let Some(next) = host.undo.redo(current) {
                patch = Some(apply_snapshot_patch(
                    &next,
                    layers_key,
                    selected_key,
                    next_id_key,
                ));
            }
        }
    }

    let avail = ui.available_width().min(520.0);
    let aspect = aspect_w as f32 / aspect_h as f32;
    let (canvas_w, canvas_h) = if aspect >= 1.0 {
        (avail, avail / aspect)
    } else {
        (avail * aspect, avail)
    };
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(canvas_w, canvas_h),
        egui::Sense::click_and_drag(),
    );
    if response.clicked() || response.dragged() || response.gained_focus() {
        host.focused = true;
    }
    if response.lost_focus() {
        host.focused = false;
    }

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::from_gray(28));
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.5_f32, egui::Color32::from_gray(90)),
        egui::StrokeKind::Inside,
    );

    // The generated result is the canvas background. Layers are painted above
    // it so users can position and compare composition elements in context.
    if let Some(path) = background_path.filter(|path| !path.is_empty()) {
        if let Some(texture) = crate::decl_ui::try_load_png(ui.ctx(), path) {
            let base = texture.size_vec2();
            let fit = ((rect.width() - 4.0) / base.x.max(1.0))
                .min((rect.height() - 4.0) / base.y.max(1.0))
                .min(1.0);
            let image_rect = egui::Rect::from_center_size(rect.center(), base * fit);
            painter.image(
                texture.id(),
                image_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
    }

    let to_screen = |x: f32, y: f32, w: f32, h: f32| -> egui::Rect {
        egui::Rect::from_min_size(
            egui::pos2(
                rect.left() + x * rect.width(),
                rect.top() + y * rect.height(),
            ),
            egui::vec2(w * rect.width(), h * rect.height()),
        )
    };

    let pointer = response.interact_pointer_pos();
    let drag_id = egui::Id::new(("layer_canvas_drag", canvas_id));

    if response.drag_started() {
        push_undo(host, local_state, layers_key, selected_key, next_id_key);
        host.interaction_id = format!("lc-{}-{}", canvas_id, ui.id().value());
    }

    if response.clicked() {
        if let Some(pos) = pointer {
            let mut hit: Option<u64> = None;
            for layer in layers.iter().rev() {
                if !layer.visible {
                    continue;
                }
                let r = to_screen(layer.x, layer.y, layer.w, layer.h);
                if r.contains(pos) {
                    hit = Some(layer.id);
                    break;
                }
            }
            if let Some(id) = hit {
                bring_layer_to_front(&mut layers, id);
                selected = Some(id);
                patch = Some(build_patch(
                    layers_key,
                    selected_key,
                    next_id_key,
                    &layers,
                    selected,
                    next_id,
                    canvas_id,
                    host,
                    "select",
                ));
            } else {
                selected = None;
                patch = Some(build_patch(
                    layers_key,
                    selected_key,
                    next_id_key,
                    &layers,
                    selected,
                    next_id,
                    canvas_id,
                    host,
                    "select",
                ));
            }
        }
    }

    if response.drag_started() {
        if let Some(pos) = pointer {
            if let Some(sel) = selected {
                if let Some(layer) = layers.iter().find(|l| l.id == sel && !l.locked) {
                    let r = to_screen(layer.x, layer.y, layer.w, layer.h);
                    let handle = resize_handle_rect(r);
                    if handle.contains(pos) {
                        host.drag = Some(DragState {
                            block_id: layer.id,
                            mode: DragMode::ResizeSe,
                            start_pointer: pos,
                            orig_x: layer.x,
                            orig_y: layer.y,
                            orig_w: layer.w,
                            orig_h: layer.h,
                        });
                    }
                }
            }
            if host.drag.is_none() {
                let hit = layers
                    .iter()
                    .rev()
                    .find(|layer| {
                        layer.visible
                            && !layer.locked
                            && to_screen(layer.x, layer.y, layer.w, layer.h).contains(pos)
                    })
                    .map(|layer| (layer.id, layer.x, layer.y, layer.w, layer.h));
                if let Some((id, x, y, w, h)) = hit {
                    bring_layer_to_front(&mut layers, id);
                    selected = Some(id);
                    host.drag = Some(DragState {
                        block_id: id,
                        mode: DragMode::Move,
                        start_pointer: pos,
                        orig_x: x,
                        orig_y: y,
                        orig_w: w,
                        orig_h: h,
                    });
                }
            }
        }
    }

    if response.dragged() {
        if let (Some(d), Some(pos)) = (host.drag.as_ref(), pointer) {
            let dx = (pos.x - d.start_pointer.x) / rect.width().max(1.0);
            let dy = (pos.y - d.start_pointer.y) / rect.height().max(1.0);
            if let Some(layer) = layers.iter_mut().find(|l| l.id == d.block_id) {
                match d.mode {
                    DragMode::Move => {
                        layer.x = d.orig_x + dx;
                        layer.y = d.orig_y + dy;
                    }
                    DragMode::ResizeSe => {
                        layer.w = d.orig_w + dx;
                        layer.h = d.orig_h + dy;
                    }
                }
                layer.clamp_in_frame();
            }
            // Keep the declarative local state in sync during the gesture as
            // well as on release; this makes resize/move survive the next
            // frame and avoids losing the edit when another control redraws.
            patch = Some(build_patch(
                layers_key,
                selected_key,
                next_id_key,
                &layers,
                selected,
                next_id,
                canvas_id,
                host,
                "transform_update",
            ));
        }
    }

    if response.drag_stopped() {
        if host.drag.is_some() {
            patch = Some(build_patch(
                layers_key,
                selected_key,
                next_id_key,
                &layers,
                selected,
                next_id,
                canvas_id,
                host,
                "transform_commit",
            ));
        }
        host.drag = None;
    }

    for (i, layer) in layers.iter().enumerate() {
        if !layer.visible {
            continue;
        }
        let r = to_screen(layer.x, layer.y, layer.w, layer.h);
        let selected_here = selected == Some(layer.id);
        let alpha = |base: u8| -> u8 { ((base as f32) * layer_opacity.clamp(0.0, 1.0)) as u8 };
        let fill = if selected_here {
            egui::Color32::from_rgba_unmultiplied(80, 140, 220, alpha(90))
        } else {
            let hue = ((40 + i * 37) % 180) as u8;
            egui::Color32::from_rgba_unmultiplied(60 + hue / 2, 100, 160, alpha(70))
        };
        let stroke = if selected_here {
            egui::Stroke::new(
                2.0_f32,
                egui::Color32::from_rgba_unmultiplied(120, 190, 255, alpha(255)),
            )
        } else {
            egui::Stroke::new(
                1.0_f32,
                egui::Color32::from_rgba_unmultiplied(200, 200, 220, alpha(160)),
            )
        };
        painter.rect_filled(r, 3.0, fill);
        painter.rect_stroke(r, 3.0, stroke, egui::StrokeKind::Inside);
        let label = layer_display_name(layer, i, doc, language, &t);
        let text_pos = r.left_top() + egui::vec2(6.0, 4.0);
        let font_id = egui::FontId::proportional(13.0);
        let text_color = egui::Color32::from_white_alpha(alpha(255));
        painter.text(
            text_pos + egui::vec2(1.0, 1.0),
            egui::Align2::LEFT_TOP,
            &label,
            font_id.clone(),
            egui::Color32::from_black_alpha(alpha(200)),
        );
        painter.text(
            text_pos,
            egui::Align2::LEFT_TOP,
            label,
            font_id,
            text_color,
        );
        if selected_here && !layer.locked {
            let handle = resize_handle_rect(r);
            painter.rect_filled(
                handle,
                2.0,
                egui::Color32::from_rgba_unmultiplied(220, 230, 255, alpha(255)),
            );
        }
    }

    if let Some(selected_id) = selected {
        if let Some(layer) = layers.iter_mut().find(|layer| layer.id == selected_id) {
            ui.add_space(4.0);
            ui.label("Prompt du calque sélectionné");
            let mut prompt = layer.prompt.clone();
            if ui
                .add_sized(
                    [ui.available_width(), 56.0],
                    egui::TextEdit::multiline(&mut prompt)
                        .hint_text("Décrivez cet élément à placer dans la composition…"),
                )
                .changed()
            {
                push_undo(host, local_state, layers_key, selected_key, next_id_key);
                layer.prompt = prompt;
                if layer.label.trim().is_empty() {
                    layer.label = layer.prompt.chars().take(28).collect();
                }
                patch = Some(build_patch(
                    layers_key,
                    selected_key,
                    next_id_key,
                    &layers,
                    selected,
                    next_id,
                    canvas_id,
                    host,
                    "prompt_edit",
                ));
            }
        }
    }

    ui.ctx().data_mut(|data| {
        if let Some(d) = host.drag.clone() {
            data.insert_temp(drag_id, d);
        } else {
            data.remove::<DragState>(drag_id);
        }
    });

    patch
}

pub fn ui_layer_list(
    ui: &mut egui::Ui,
    w: &DeclUiWidget,
    doc: &DeclUiDocument,
    language: &str,
    local_state: &HashMap<String, Value>,
    host: &mut LayerCanvasHostState,
) -> Option<LayerCanvasPatch> {
    let layers_key = w.layers_key.as_deref().unwrap_or("layers");
    let selected_key = w.selected_key.as_deref().unwrap_or("selected_id");
    let next_id_key = w.next_id_key.as_deref();
    let mut layers = read_layers(local_state, layers_key);
    let selected = read_u64(local_state, selected_key);
    let next_id = next_id_key
        .and_then(|k| read_u64(local_state, k))
        .unwrap_or_else(|| layers.iter().map(|l| l.id).max().unwrap_or(0) + 1);
    let t = crate::i18n::strings(language);

    if let Some(label) = widget_label(w, doc, language) {
        ui.heading(label);
    }

    let mut patch: Option<LayerCanvasPatch> = None;
    let mut reorder_from: Option<usize> = host.drag_layer_index;
    let row_meta: Vec<(usize, u64, String, bool)> = layers
        .iter()
        .enumerate()
        .map(|(idx, layer)| {
            let name = layer_display_name(layer, idx, doc, language, &t);
            (idx, layer.id, name, layer.visible)
        })
        .collect();

    for (idx, layer_id, name, visible) in row_meta.into_iter().rev() {
        ui.horizontal(|ui| {
            let sel = selected == Some(layer_id);
            if ui.selectable_label(sel, &name).clicked() {
                patch = Some(build_patch(
                    layers_key,
                    selected_key,
                    next_id_key,
                    &layers,
                    Some(layer_id),
                    next_id,
                    "",
                    host,
                    "select",
                ));
            }
            let vis_tip = if visible {
                t.decl_layer_hide
            } else {
                t.decl_layer_show
            };
            if icons::visibility_toggle_button(ui, visible)
                .on_hover_text(vis_tip)
                .clicked()
            {
                push_undo(host, local_state, layers_key, selected_key, next_id_key);
                if let Some(l) = layers.iter_mut().find(|l| l.id == layer_id) {
                    l.visible = !l.visible;
                }
                patch = Some(build_patch(
                    layers_key,
                    selected_key,
                    next_id_key,
                    &layers,
                    selected,
                    next_id,
                    "",
                    host,
                    "visibility",
                ));
            }
            if idx + 1 < layers.len()
                && icons::chevron_up_button(ui)
                    .on_hover_text(t.decl_layer_move_up)
                    .clicked()
            {
                push_undo(host, local_state, layers_key, selected_key, next_id_key);
                reorder_layer(&mut layers, idx, idx + 1);
                patch = Some(build_patch(
                    layers_key,
                    selected_key,
                    next_id_key,
                    &layers,
                    selected,
                    next_id,
                    "",
                    host,
                    "z_index",
                ));
            }
            if idx > 0
                && icons::chevron_down_button(ui)
                    .on_hover_text(t.decl_layer_move_down)
                    .clicked()
            {
                push_undo(host, local_state, layers_key, selected_key, next_id_key);
                reorder_layer(&mut layers, idx, idx - 1);
                patch = Some(build_patch(
                    layers_key,
                    selected_key,
                    next_id_key,
                    &layers,
                    selected,
                    next_id,
                    "",
                    host,
                    "z_index",
                ));
            }
            if icons::layer_delete_button(ui)
                .on_hover_text(t.decl_layer_delete)
                .clicked()
            {
                push_undo(host, local_state, layers_key, selected_key, next_id_key);
                layers.retain(|l| l.id != layer_id);
                let new_selected = if selected == Some(layer_id) {
                    layers.last().map(|l| l.id)
                } else {
                    selected
                };
                patch = Some(build_patch(
                    layers_key,
                    selected_key,
                    next_id_key,
                    &layers,
                    new_selected,
                    next_id,
                    "",
                    host,
                    "remove",
                ));
            }
            if icons::move_vertical_button(ui)
                .on_hover_text(t.decl_layer_drag_hint)
                .clicked()
            {
                reorder_from = Some(idx);
                host.drag_layer_index = Some(idx);
            } else if let Some(from) = reorder_from {
                if from != idx
                    && icons::chevron_down_button(ui)
                        .on_hover_text(t.decl_layer_move_down)
                        .clicked()
                {
                    push_undo(host, local_state, layers_key, selected_key, next_id_key);
                    reorder_layer(&mut layers, from, idx);
                    host.drag_layer_index = None;
                    reorder_from = None;
                    patch = Some(build_patch(
                        layers_key,
                        selected_key,
                        next_id_key,
                        &layers,
                        selected,
                        next_id,
                        "",
                        host,
                        "reorder",
                    ));
                }
            }
        });
    }

    patch
}

pub fn ui_undo_redo(
    ui: &mut egui::Ui,
    w: &DeclUiWidget,
    doc: &DeclUiDocument,
    language: &str,
    local_state: &HashMap<String, Value>,
    host: &mut LayerCanvasHostState,
) -> Option<LayerCanvasPatch> {
    let layers_key = w.layers_key.as_deref().unwrap_or("layers");
    let selected_key = w.selected_key.as_deref().unwrap_or("selected_id");
    let next_id_key = w.next_id_key.as_deref();
    let t = crate::i18n::strings(language);
    let undo_label = widget_label_from_key(doc, language, "undo_label")
        .unwrap_or_else(|| t.decl_undo.to_string());
    let redo_label = widget_label_from_key(doc, language, "redo_label")
        .unwrap_or_else(|| t.decl_redo.to_string());

    let mut patch = None;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(host.undo.can_undo(), egui::Button::new(undo_label))
            .clicked()
        {
            let current = snapshot_from_local(local_state, layers_key, selected_key, next_id_key);
            if let Some(prev) = host.undo.undo(current) {
                patch = Some(apply_snapshot_patch(
                    &prev,
                    layers_key,
                    selected_key,
                    next_id_key,
                ));
            }
        }
        if ui
            .add_enabled(host.undo.can_redo(), egui::Button::new(redo_label))
            .clicked()
        {
            let current = snapshot_from_local(local_state, layers_key, selected_key, next_id_key);
            if let Some(next) = host.undo.redo(current) {
                patch = Some(apply_snapshot_patch(
                    &next,
                    layers_key,
                    selected_key,
                    next_id_key,
                ));
            }
        }
    });
    patch
}

fn build_patch(
    layers_key: &str,
    selected_key: &str,
    next_id_key: Option<&str>,
    layers: &[RichLayer],
    selected: Option<u64>,
    next_id: u64,
    canvas_id: &str,
    host: &LayerCanvasHostState,
    operation: &str,
) -> LayerCanvasPatch {
    LayerCanvasPatch {
        layers_key: layers_key.to_string(),
        selected_key: selected_key.to_string(),
        next_id_key: next_id_key.map(str::to_string),
        layers: layers_to_value(layers),
        selected_id: selected.map(Value::from).unwrap_or(Value::Null),
        next_id: next_id_key.map(|_| Value::from(next_id)),
        interaction: Some(RichInteractionEvent {
            phase: if operation.ends_with("commit") {
                "commit".into()
            } else {
                "update".into()
            },
            interaction_id: host.interaction_id.clone(),
            value: json!({
                "operation": operation,
                "canvas_id": canvas_id,
                "layers": layers,
                "selected_id": selected,
            }),
        }),
    }
}

fn widget_label_from_key(doc: &DeclUiDocument, language: &str, key: &str) -> Option<String> {
    doc.labels.as_ref()?.resolve(language, key)
}

fn layer_display_name(
    layer: &RichLayer,
    idx: usize,
    doc: &DeclUiDocument,
    language: &str,
    t: &crate::i18n::UiStrings,
) -> String {
    if !layer.label.trim().is_empty() {
        return truncate(&layer.label, 28);
    }
    let base = widget_label_from_key(doc, language, "layer_default_name")
        .unwrap_or_else(|| t.decl_layer_add.to_string());
    format!("{base} {}", idx + 1)
}

fn resize_handle_rect(r: egui::Rect) -> egui::Rect {
    let s = 10.0;
    egui::Rect::from_min_size(egui::pos2(r.right() - s, r.bottom() - s), egui::vec2(s, s))
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}
