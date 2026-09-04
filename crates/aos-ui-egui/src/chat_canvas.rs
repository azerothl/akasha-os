//! Chat session canvas — shared vector drawing (human + agents).

use aos_proto::{
    canvas_hit_test, canvas_layer_effective_locked, canvas_op_bbox, canvas_rect_corners,
    canvas_rotate_point, default_canvas_opacity, translate_canvas_op_body, CanvasAspect,
    CanvasEdit, CanvasLayer, CanvasLinearGradient, CanvasOp, CanvasOpBody, CanvasPenStyle,
    CanvasPoint,
};
use eframe::egui::epaint::{CircleShape, PathShape, PathStroke, Shape, StrokeKind};
use eframe::egui::{Align2, Color32, FontId, Pos2, Sense, Stroke, Ui, Vec2};

use crate::canvas_paint::{
    self, body_dash, fill_color, layer_effective_opacity, paint_polyline, path_stroke, stroke_color,
};
use crate::i18n::UiStrings;
use crate::theme::{PAPER, SIGNAL, VOID};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasTool {
    Select,
    Pan,
    Pen,
    Eraser,
    Line,
    Spline,
    Path,
    Rect,
    Ellipse,
}

#[derive(Debug, Clone)]
pub enum CanvasUiAction {
    Apply(CanvasOpBody),
    Edit(CanvasEdit),
    SetStyle {
        color: Option<String>,
        width: Option<f32>,
        opacity: Option<f32>,
        dash: Option<Vec<f32>>,
    },
    ExportPng,
    ExportSvg,
    ExportJson,
    ImportJson,
    SetAspect(CanvasAspect),
    ResetView,
}

#[derive(Debug, Clone)]
pub struct CanvasPanelState {
    pub ops: Vec<CanvasOp>,
    pub next_seq: u64,
    pub last_seen_seq: u64,
    pub tool: CanvasTool,
    pub color: Color32,
    pub width: f32,
    /// Fill closed shapes (rect / ellipse) instead of stroke outline.
    pub shape_fill: bool,
    /// In-progress human stroke (optimistic).
    pub draft_points: Vec<CanvasPoint>,
    /// Shape drag origin (normalized), when using rect/ellipse.
    pub drag_origin: Option<CanvasPoint>,
    pub drag_current: Option<CanvasPoint>,
    /// Remote ops animating in (seq → start time seconds).
    pub animating: Vec<(u64, f64)>,
    pub poll_due: f64,
    /// Awaiting one-step confirmation before clear.
    pub clear_confirm_open: bool,
    /// True while a vision model is reading this canvas (tester-cohort slice 2).
    pub seeing: bool,
    /// Overlay a 10×10 board grid (display only).
    pub show_grid: bool,
    /// Snap pointer and committed coords to 0.01.
    pub snap: bool,
    pub selected_seq: Option<u64>,
    pub layers: Vec<CanvasLayer>,
    pub active_layer_id: String,
    /// Per-op alpha for new strokes (0..1).
    pub pen_opacity: f32,
    /// Dashed outline for line-like tools.
    pub pen_dashed: bool,
    /// Linear gradient fill for closed shapes.
    pub use_gradient: bool,
    pub gradient_color2: Color32,
    /// View pan (screen px) and zoom around the letterboxed board.
    pub view_pan: Vec2,
    pub view_zoom: f32,
    /// Inline rename for active layer.
    pub layer_rename_id: Option<String>,
    pub layer_rename_text: String,
}

impl Default for CanvasPanelState {
    fn default() -> Self {
        Self {
            ops: Vec::new(),
            next_seq: 1,
            last_seen_seq: 0,
            tool: CanvasTool::Pen,
            color: Color32::from_rgb(0x3e, 0xe0, 0xc4),
            width: 0.015,
            shape_fill: false,
            draft_points: Vec::new(),
            drag_origin: None,
            drag_current: None,
            animating: Vec::new(),
            poll_due: 0.0,
            clear_confirm_open: false,
            seeing: false,
            show_grid: false,
            snap: false,
            selected_seq: None,
            layers: Vec::new(),
            active_layer_id: String::new(),
            pen_opacity: default_canvas_opacity(),
            pen_dashed: false,
            use_gradient: false,
            gradient_color2: Color32::from_rgb(0xf4, 0x00, 0x09),
            view_pan: Vec2::ZERO,
            view_zoom: 1.0,
            layer_rename_id: None,
            layer_rename_text: String::new(),
        }
    }
}

impl CanvasPanelState {
    /// `after_seq` for `canvas.get`. Full snapshot when the board is empty so a
    /// wiped panel can recover ops already committed on the server.
    pub fn poll_after_seq(&self) -> Option<u64> {
        if self.ops.is_empty() || self.last_seen_seq == 0 {
            None
        } else {
            Some(self.last_seen_seq)
        }
    }

    pub fn apply_snapshot(&mut self, ops: Vec<CanvasOp>, next_seq: u64, now: f64) {
        // Overlapping `canvas.get` polls: a late empty snapshot (started before
        // the agent drew) must not replace a newer document already on screen.
        if next_seq < self.next_seq {
            return;
        }
        let pending_human: Vec<CanvasOp> = self
            .ops
            .iter()
            .filter(|o| o.author_id == "human" && o.seq == 0)
            .cloned()
            .collect();
        let prior_human_server = self
            .ops
            .iter()
            .filter(|o| o.author_id == "human" && o.seq > 0)
            .count();
        for op in &ops {
            if op.seq > self.last_seen_seq && op.author_id != "human" {
                self.animating.push((op.seq, now));
            }
        }
        if let Some(max) = ops.iter().map(|o| o.seq).max() {
            self.last_seen_seq = self.last_seen_seq.max(max);
        }
        self.ops = ops;
        let new_human_server = self.ops.iter().filter(|o| o.author_id == "human").count();
        if new_human_server <= prior_human_server {
            for p in pending_human {
                self.ops.push(p);
            }
            self.ops.sort_by_key(|o| o.seq);
        }
        self.next_seq = next_seq.max(self.next_seq);
        self.animating
            .retain(|(seq, start)| self.ops.iter().any(|o| o.seq == *seq) && now - start < 0.45);
    }

    pub fn merge_delta(&mut self, ops: Vec<CanvasOp>, next_seq: u64, now: f64) {
        for op in ops {
            if self.ops.iter().any(|o| o.seq == op.seq) {
                continue;
            }
            if op.author_id != "human" {
                self.animating.push((op.seq, now));
            }
            self.last_seen_seq = self.last_seen_seq.max(op.seq);
            self.ops.push(op);
        }
        self.ops.sort_by_key(|o| o.seq);
        self.next_seq = next_seq.max(self.next_seq);
        self.animating
            .retain(|(seq, start)| self.ops.iter().any(|o| o.seq == *seq) && now - start < 0.45);
    }

    pub fn sync_pen(&mut self, pen: &CanvasPenStyle) {
        self.color = canvas_paint::parse_hex_color(&pen.color);
        self.width = pen.width;
        self.pen_opacity = pen.opacity;
        self.pen_dashed = !pen.dash.is_empty();
    }

    pub fn sync_layers(&mut self, layers: Vec<CanvasLayer>, active_layer_id: String) {
        self.layers = layers;
        self.active_layer_id = active_layer_id;
    }
}

pub use canvas_paint::color_to_hex;

fn pen_style_fields(state: &CanvasPanelState) -> (f32, Vec<f32>, Option<CanvasLinearGradient>) {
    let dash = if state.pen_dashed {
        vec![0.03, 0.03]
    } else {
        vec![]
    };
    let gradient = if state.use_gradient && state.shape_fill {
        Some(CanvasLinearGradient {
            color0: color_to_hex(state.color),
            color1: color_to_hex(state.gradient_color2),
            angle_deg: 0.0,
        })
    } else {
        None
    };
    (state.pen_opacity, dash, gradient)
}

fn to_screen(rect: eframe::egui::Rect, p: CanvasPoint) -> Pos2 {
    canvas_paint::to_screen(rect, p)
}

fn radius_px(rect: eframe::egui::Rect, width: f32) -> f32 {
    canvas_paint::radius_px(rect, width)
}

fn to_norm(rect: eframe::egui::Rect, p: Pos2) -> CanvasPoint {
    CanvasPoint {
        x: ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0),
        y: ((p.y - rect.top()) / rect.height()).clamp(0.0, 1.0),
    }
}

/// `f32::clamp` panics when min > max. Empty-canvas hint text can be wider
/// than the pane (unbreakable tokens), which inverts the padding range.
fn clamp_in_range(value: f32, a: f32, b: f32) -> f32 {
    let min = a.min(b);
    let max = a.max(b);
    if !value.is_finite() {
        return min;
    }
    value.clamp(min, max)
}

const SNAP_STEP: f32 = 0.01;

pub(crate) fn snap_unit(v: f32) -> f32 {
    ((v / SNAP_STEP).round() * SNAP_STEP).clamp(0.0, 1.0)
}

pub(crate) fn snap_point(p: CanvasPoint) -> CanvasPoint {
    CanvasPoint {
        x: snap_unit(p.x),
        y: snap_unit(p.y),
    }
}

fn snap_points(points: &mut [CanvasPoint]) {
    for p in points {
        *p = snap_point(*p);
    }
}

fn snap_body(body: &mut CanvasOpBody) {
    match body {
        CanvasOpBody::Stroke { points, .. }
        | CanvasOpBody::Erase { points, .. }
        | CanvasOpBody::Spline { points, .. }
        | CanvasOpBody::Path { points, .. } => snap_points(points),
        CanvasOpBody::Line { p0, p1, .. } => {
            *p0 = snap_point(*p0);
            *p1 = snap_point(*p1);
        }
        CanvasOpBody::Rect { x, y, w, h, .. } | CanvasOpBody::Ellipse { x, y, w, h, .. } => {
            let x1 = snap_unit(*x + *w);
            let y1 = snap_unit(*y + *h);
            *x = snap_unit(*x);
            *y = snap_unit(*y);
            *w = (x1 - *x).abs().max(SNAP_STEP);
            *h = (y1 - *y).abs().max(SNAP_STEP);
        }
        CanvasOpBody::Fill { x, y, .. } => {
            *x = snap_unit(*x);
            *y = snap_unit(*y);
        }
        CanvasOpBody::Clear | CanvasOpBody::Undo => {}
    }
}

fn maybe_snap_point(p: CanvasPoint, snap: bool) -> CanvasPoint {
    if snap {
        snap_point(p)
    } else {
        p
    }
}

fn layer_is_visible(layers: &[CanvasLayer], layer_id: &str) -> bool {
    if layers.is_empty() {
        return true;
    }
    let mut id = Some(layer_id);
    let mut guard = 0;
    while let Some(cur) = id {
        if guard > 32 {
            break;
        }
        guard += 1;
        let Some(layer) = layers.iter().find(|l| l.id == cur) else {
            return false;
        };
        if !layer.visible {
            return false;
        }
        id = layer.parent_id.as_deref();
    }
    true
}

fn layer_is_locked(layers: &[CanvasLayer], layer_id: &str) -> bool {
    if layers.is_empty() {
        return false;
    }
    let mut id = Some(layer_id);
    let mut guard = 0;
    while let Some(cur) = id {
        if guard > 32 {
            break;
        }
        guard += 1;
        let Some(layer) = layers.iter().find(|l| l.id == cur) else {
            return false;
        };
        if layer.locked {
            return true;
        }
        id = layer.parent_id.as_deref();
    }
    false
}

fn paint_board_grid(painter: &eframe::egui::Painter, rect: eframe::egui::Rect) {
    let fine = Color32::from_rgba_unmultiplied(SIGNAL.r(), SIGNAL.g(), SIGNAL.b(), 36);
    let usable = Color32::from_rgba_unmultiplied(SIGNAL.r(), SIGNAL.g(), SIGNAL.b(), 72);
    for i in 1..10 {
        let t = i as f32 / 10.0;
        let stroke = if i == 1 || i == 9 {
            Stroke::new(1.0_f32, usable)
        } else {
            Stroke::new(1.0_f32, fine)
        };
        let x = rect.left() + t * rect.width();
        let y = rect.top() + t * rect.height();
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            stroke,
        );
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            stroke,
        );
    }
}

fn anim_progress(state: &CanvasPanelState, seq: u64, now: f64) -> f32 {
    if let Some((_, start)) = state.animating.iter().find(|(s, _)| *s == seq) {
        ((now - start) / 0.30).clamp(0.0, 1.0) as f32
    } else {
        1.0
    }
}

fn paint_op(
    painter: &eframe::egui::Painter,
    rect: eframe::egui::Rect,
    op: &CanvasOp,
    layers: &[CanvasLayer],
    dark: bool,
    progress: f32,
) {
    let layer_opacity = layer_effective_opacity(layers, &op.layer_id);
    if layer_opacity <= 0.001 {
        return;
    }
    let dash = body_dash(&op.body);
    match &op.body {
        CanvasOpBody::Stroke {
            points,
            color,
            width,
            ..
        } => {
            if points.len() < 2 {
                return;
            }
            let n = ((points.len() as f32) * progress).ceil().max(2.0) as usize;
            let slice = &points[..n.min(points.len())];
            let c = stroke_color(&op.body, &op.author_id, color, dark, layer_opacity);
            paint_polyline(painter, rect, slice, *width, c, dash);
        }
        CanvasOpBody::Erase { points, width } => {
            if points.is_empty() {
                return;
            }
            let n = ((points.len() as f32) * progress).ceil().max(1.0) as usize;
            let slice = &points[..n.min(points.len())];
            let bg = canvas_bg(dark);
            let rad = radius_px(rect, *width);
            let screen: Vec<Pos2> = slice.iter().map(|p| to_screen(rect, *p)).collect();
            if screen.len() == 1 {
                painter.add(Shape::Circle(CircleShape::filled(screen[0], rad, bg)));
            } else {
                painter.add(Shape::line(screen, path_stroke(rad * 2.0, bg)));
            }
        }
        CanvasOpBody::Rect {
            x,
            y,
            w,
            h,
            color,
            fill,
            width,
            rotation,
            ..
        } => {
            let center = CanvasPoint {
                x: x + w * 0.5 * progress,
                y: y + h * 0.5 * progress,
            };
            let c = fill_color(&op.body, &op.author_id, color, dark, layer_opacity, center);
            let stroke_c = stroke_color(&op.body, &op.author_id, color, dark, layer_opacity);
            let corners = canvas_rect_corners(*x, *y, w * progress, h * progress, *rotation);
            let screen: Vec<Pos2> = corners
                .iter()
                .map(|(px, py)| to_screen(rect, CanvasPoint { x: *px, y: *py }))
                .collect();
            if *fill {
                painter.add(Shape::Path(PathShape {
                    points: screen,
                    closed: true,
                    fill: c,
                    stroke: PathStroke::NONE,
                }));
            } else {
                let rad = radius_px(rect, *width);
                painter.add(Shape::Path(PathShape {
                    points: screen,
                    closed: true,
                    fill: Color32::TRANSPARENT,
                    stroke: path_stroke(rad * 2.0, stroke_c),
                }));
            }
        }
        CanvasOpBody::Ellipse {
            x,
            y,
            w,
            h,
            color,
            fill,
            width,
            rotation,
            ..
        } => {
            let cx = x + w * 0.5;
            let cy = y + h * 0.5;
            let center = CanvasPoint { x: cx, y: cy };
            let c = fill_color(&op.body, &op.author_id, color, dark, layer_opacity, center);
            let stroke_c = stroke_color(&op.body, &op.author_id, color, dark, layer_opacity);
            let pts: Vec<Pos2> = (0..48)
                .map(|i| {
                    let t = std::f32::consts::TAU * (i as f32) / 48.0;
                    let px = cx + (w.abs() * 0.5 * progress) * t.cos();
                    let py = cy + (h.abs() * 0.5 * progress) * t.sin();
                    let (rx, ry) = canvas_rotate_point(cx, cy, px, py, *rotation);
                    to_screen(rect, CanvasPoint { x: rx, y: ry })
                })
                .collect();
            if *fill {
                painter.add(Shape::Path(PathShape {
                    points: pts,
                    closed: true,
                    fill: c,
                    stroke: PathStroke::NONE,
                }));
            } else {
                let rad = radius_px(rect, *width);
                painter.add(Shape::Path(PathShape {
                    points: pts,
                    closed: true,
                    fill: Color32::TRANSPARENT,
                    stroke: path_stroke(rad * 2.0, stroke_c),
                }));
            }
        }
        CanvasOpBody::Line {
            p0,
            p1,
            color,
            width,
            ..
        } => {
            let c = stroke_color(&op.body, &op.author_id, color, dark, layer_opacity);
            let end = CanvasPoint {
                x: p0.x + (p1.x - p0.x) * progress,
                y: p0.y + (p1.y - p0.y) * progress,
            };
            paint_polyline(painter, rect, &[*p0, end], *width, c, dash);
        }
        CanvasOpBody::Spline {
            points,
            color,
            width,
            ..
        } => {
            if points.len() < 2 {
                return;
            }
            let c = stroke_color(&op.body, &op.author_id, color, dark, layer_opacity);
            let sampled = sample_spline_points(points, 24);
            let n = ((sampled.len() as f32) * progress).ceil().max(2.0) as usize;
            let slice = &sampled[..n.min(sampled.len())];
            paint_polyline(painter, rect, slice, *width, c, dash);
        }
        CanvasOpBody::Path {
            points,
            color,
            width,
            fill,
            closed,
            ..
        } => {
            if points.len() < 2 {
                return;
            }
            let sampled = sample_spline_points(points, 24);
            let n = ((sampled.len() as f32) * progress).ceil().max(2.0) as usize;
            let slice = &sampled[..n.min(sampled.len())];
            let center = slice
                .first()
                .copied()
                .unwrap_or(CanvasPoint { x: 0.5, y: 0.5 });
            let fill_c = fill_color(&op.body, &op.author_id, color, dark, layer_opacity, center);
            let stroke_c = stroke_color(&op.body, &op.author_id, color, dark, layer_opacity);
            let mut screen: Vec<Pos2> = slice.iter().map(|p| to_screen(rect, *p)).collect();
            if *closed && screen.len() >= 3 {
                if let Some(first) = screen.first().copied() {
                    screen.push(first);
                }
            }
            if *fill && screen.len() >= 3 {
                painter.add(Shape::Path(PathShape {
                    points: screen.clone(),
                    closed: *closed,
                    fill: fill_c,
                    stroke: PathStroke::NONE,
                }));
            }
            if *width > 0.0 {
                let stroke_pts: Vec<CanvasPoint> = slice.to_vec();
                paint_polyline(painter, rect, &stroke_pts, *width, stroke_c, dash);
            }
        }
        CanvasOpBody::Fill { x, y, color, .. } => {
            let c = stroke_color(&op.body, &op.author_id, color, dark, layer_opacity);
            let center = to_screen(rect, CanvasPoint { x: *x, y: *y });
            let arm = (rect.width().min(rect.height()) * 0.008 * progress).max(2.0);
            painter.circle_stroke(center, arm, Stroke::new(1.2_f32, c));
        }
        CanvasOpBody::Clear | CanvasOpBody::Undo => {}
    }
}

fn canvas_bg(dark: bool) -> Color32 {
    if dark {
        VOID
    } else {
        PAPER
    }
}

fn ellipse_points(r: eframe::egui::Rect, n: usize) -> Vec<Pos2> {
    let c = r.center();
    let rx = r.width() * 0.5;
    let ry = r.height() * 0.5;
    (0..n)
        .map(|i| {
            let t = std::f32::consts::TAU * (i as f32) / (n as f32);
            Pos2::new(c.x + rx * t.cos(), c.y + ry * t.sin())
        })
        .collect()
}

fn sample_spline_points(points: &[CanvasPoint], segments_per_span: usize) -> Vec<CanvasPoint> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let mut out = Vec::new();
    let n = points.len();
    for i in 0..n.saturating_sub(1) {
        let p0 = if i == 0 { points[0] } else { points[i - 1] };
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = if i + 2 < n {
            points[i + 2]
        } else {
            points[i + 1]
        };
        let steps = segments_per_span.max(4);
        let start_j = if i == 0 { 0 } else { 1 };
        for j in start_j..=steps {
            let t = j as f32 / steps as f32;
            let t2 = t * t;
            let t3 = t2 * t;
            out.push(CanvasPoint {
                x: 0.5
                    * ((2.0 * p1.x)
                        + (-p0.x + p2.x) * t
                        + (2.0 * p0.x - 5.0 * p1.x + 4.0 * p2.x - p3.x) * t2
                        + (-p0.x + 3.0 * p1.x - 3.0 * p2.x + p3.x) * t3),
                y: 0.5
                    * ((2.0 * p1.y)
                        + (-p0.y + p2.y) * t
                        + (2.0 * p0.y - 5.0 * p1.y + 4.0 * p2.y - p3.y) * t2
                        + (-p0.y + 3.0 * p1.y - 3.0 * p2.y + p3.y) * t3),
            });
        }
    }
    out
}

fn fit_board_rect(outer: eframe::egui::Rect, aspect: CanvasAspect) -> eframe::egui::Rect {
    let (rw, rh) = aspect.ratio();
    let target = rw / rh;
    let outer_aspect = outer.width() / outer.height().max(1.0);
    let (board_w, board_h) = if target > outer_aspect {
        let w = outer.width();
        let h = w / target;
        (w, h)
    } else {
        let h = outer.height();
        let w = h * target;
        (w, h)
    };
    eframe::egui::Rect::from_center_size(outer.center(), Vec2::new(board_w, board_h))
}

fn view_board_rect(
    outer: eframe::egui::Rect,
    aspect: CanvasAspect,
    pan: Vec2,
    zoom: f32,
) -> eframe::egui::Rect {
    let base = fit_board_rect(outer, aspect);
    let zoom = zoom.clamp(0.25, 4.0);
    eframe::egui::Rect::from_center_size(base.center() + pan, base.size() * zoom)
}

/// SIGNAL pastille shown while a vision model reads the live canvas board.
fn ui_canvas_seeing_pill(ui: &mut Ui, label: &str) {
    eframe::egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(
            SIGNAL.r(),
            SIGNAL.g(),
            SIGNAL.b(),
            28,
        ))
        .stroke(Stroke::new(1.0_f32, SIGNAL))
        .corner_radius(0.0)
        .inner_margin(eframe::egui::Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.label(eframe::egui::RichText::new(label).color(SIGNAL).size(11.0));
        });
}

const TOOLBAR_ICON: f32 = 26.0;
const TOOLBAR_GAP: f32 = 4.0;
const TOOLBAR_ROW_H: f32 = 30.0;
const TOOLBAR_MAX_H: f32 = 72.0;

fn toolbar_icon_selectable(ui: &mut Ui, selected: bool, icon: &str, tooltip: &str) -> bool {
    ui.add_sized(
        Vec2::splat(TOOLBAR_ICON),
        eframe::egui::SelectableLabel::new(
            selected,
            eframe::egui::RichText::new(icon).size(14.0),
        ),
    )
    .on_hover_text(tooltip)
    .clicked()
}

fn toolbar_icon_button(ui: &mut Ui, icon: &str, tooltip: &str) -> bool {
    ui.add_sized(
        Vec2::splat(TOOLBAR_ICON),
        eframe::egui::Button::new(eframe::egui::RichText::new(icon).size(14.0)),
    )
    .on_hover_text(tooltip)
    .clicked()
}

/// Max height for the canvas tool strip (scroll when content exceeds this).
pub fn toolbar_max_height() -> f32 {
    TOOLBAR_MAX_H
}

/// Estimated row count for dynamic layout (1–3 icon rows).
pub fn toolbar_row_count(select_active: bool) -> usize {
    if select_active {
        3
    } else {
        2
    }
}

/// Horizontal scroll extent for the icon-only toolbar.
pub fn toolbar_content_min_width(seeing: bool, clear_confirm: bool) -> f32 {
    let mut n = 18usize;
    if seeing {
        n += 2;
    }
    if clear_confirm {
        n += 3;
    }
    n as f32 * (TOOLBAR_ICON + TOOLBAR_GAP) + 140.0
}

/// True when a running agent on this session is using canvas tools.
pub fn canvas_agent_drawing_on_session(agents: &[aos_proto::AgentInfo], session_id: &str) -> bool {
    use aos_proto::AgentState;
    agents.iter().any(|a| {
        a.session_id.as_deref() == Some(session_id)
            && matches!(a.state, AgentState::Running)
            && a.tools.iter().any(|tool| tool.starts_with("canvas."))
    })
}

fn pen_dash_vec(dashed: bool) -> Vec<f32> {
    if dashed {
        vec![0.03, 0.03]
    } else {
        vec![]
    }
}

/// Drawing tools for the unified session bar — icon-only with i18n tooltips.
pub fn ui_canvas_toolbar(
    ui: &mut Ui,
    t: &UiStrings,
    state: &mut CanvasPanelState,
    canvas_agent_drawing: bool,
    help_tooltip: Option<&str>,
    help_clicked: &mut bool,
) -> Option<CanvasUiAction> {
    let mut action: Option<CanvasUiAction> = None;
    let compact = ui.spacing().item_spacing;
    ui.spacing_mut().item_spacing = Vec2::new(TOOLBAR_GAP, compact.y);
    ui.set_min_height(TOOLBAR_ROW_H);

    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            if state.seeing {
                ui_canvas_seeing_pill(ui, t.canvas_seeing_now);
            }
            if canvas_agent_drawing {
                ui.weak(t.canvas_thinking);
            }
            for (tool, icon, tip) in [
                (CanvasTool::Select, "◻", t.canvas_tool_select),
                (CanvasTool::Pan, "✥", t.canvas_tool_pan),
                (CanvasTool::Pen, "✎", t.canvas_tool_pen),
                (CanvasTool::Eraser, "◔", t.canvas_tool_eraser),
                (CanvasTool::Line, "╱", t.canvas_tool_line),
                (CanvasTool::Spline, "~", t.canvas_tool_spline),
                (CanvasTool::Path, "⌇", t.canvas_tool_path),
                (CanvasTool::Rect, "▭", t.canvas_tool_rect),
                (CanvasTool::Ellipse, "○", t.canvas_tool_ellipse),
            ] {
                if toolbar_icon_selectable(ui, state.tool == tool, icon, tip) {
                    state.tool = tool;
                }
            }
            let mut rgba = [
                state.color.r() as f32 / 255.0,
                state.color.g() as f32 / 255.0,
                state.color.b() as f32 / 255.0,
                1.0,
            ];
            if ui
                .color_edit_button_rgba_unmultiplied(&mut rgba)
                .on_hover_text(t.canvas_tint)
                .changed()
            {
                state.color = Color32::from_rgb(
                    (rgba[0] * 255.0) as u8,
                    (rgba[1] * 255.0) as u8,
                    (rgba[2] * 255.0) as u8,
                );
                action = Some(CanvasUiAction::SetStyle {
                    color: Some(color_to_hex(state.color)),
                    width: None,
                    opacity: None,
                    dash: None,
                });
            }
            let width_resp = ui.add(
                eframe::egui::Slider::new(&mut state.width, 0.005..=0.06)
                    .show_value(false)
                    .custom_formatter(|n, _| format!("{n:.3}")),
            );
            if width_resp.on_hover_text(t.canvas_width).changed() {
                action = Some(CanvasUiAction::SetStyle {
                    color: None,
                    width: Some(state.width),
                    opacity: None,
                    dash: None,
                });
            }
            if let Some(tip) = help_tooltip {
                if crate::guide::tab_help_button(ui, tip) {
                    *help_clicked = true;
                }
            }
        });

        if state.tool == CanvasTool::Select {
            ui.horizontal(|ui| {
                if let Some(seq) = state.selected_seq {
                    for (label, edge) in [
                        (t.canvas_align_left, "left"),
                        (t.canvas_align_right, "right"),
                        (t.canvas_align_top, "top"),
                        (t.canvas_align_bottom, "bottom"),
                        (t.canvas_align_cx, "center_x"),
                        (t.canvas_align_cy, "center_y"),
                    ] {
                        if toolbar_icon_button(ui, label, t.canvas_align_to_margin) {
                            action = Some(CanvasUiAction::Edit(CanvasEdit::Align {
                                seq,
                                to_seq: None,
                                edges: vec![edge.into()],
                            }));
                        }
                    }
                    if let Some(idx) = state.ops.iter().position(|o| o.seq == seq) {
                        if toolbar_icon_button(ui, "↓", t.canvas_z_back) && idx > 0 {
                            action = Some(CanvasUiAction::Edit(CanvasEdit::Reorder {
                                seq,
                                z: (idx as i64) - 1,
                            }));
                        }
                        if toolbar_icon_button(ui, "↑", t.canvas_z_forward)
                            && idx + 1 < state.ops.len()
                        {
                            action = Some(CanvasUiAction::Edit(CanvasEdit::Reorder {
                                seq,
                                z: (idx as i64) + 1,
                            }));
                        }
                    }
                    if let Some(op) = state.ops.iter_mut().find(|o| o.seq == seq) {
                        if let CanvasOpBody::Rect { rotation, .. }
                        | CanvasOpBody::Ellipse { rotation, .. } = &mut op.body
                        {
                            let mut rot = *rotation;
                            let rot_resp = ui.add(
                                eframe::egui::DragValue::new(&mut rot)
                                    .suffix("°")
                                    .range(-180.0..=180.0)
                                    .speed(1.0),
                            );
                            if rot_resp.on_hover_text(t.canvas_rotation).changed() {
                                *rotation = rot;
                                action = Some(CanvasUiAction::Edit(CanvasEdit::Rotate {
                                    seq,
                                    rotation: rot,
                                }));
                            }
                        }
                        let mut restyle_opacity = aos_proto::canvas_op_body_opacity(&op.body);
                        let opacity_resp = ui.add(
                            eframe::egui::Slider::new(&mut restyle_opacity, 0.05..=1.0)
                                .show_value(false),
                        );
                        if opacity_resp.on_hover_text(t.canvas_opacity).changed() {
                            action = Some(CanvasUiAction::Edit(CanvasEdit::Restyle {
                                seq,
                                color: None,
                                width: None,
                                fill: None,
                                rotation: None,
                                opacity: Some(restyle_opacity),
                                dash: None,
                                gradient: None,
                            }));
                        }
                    }
                }
            });
        }

        ui.horizontal(|ui| {
            if matches!(
                state.tool,
                CanvasTool::Rect | CanvasTool::Ellipse | CanvasTool::Path
            ) {
                let fill_on = state.shape_fill;
                if toolbar_icon_selectable(ui, fill_on, "F", t.canvas_fill_toggle) {
                    state.shape_fill = !fill_on;
                }
            }
            let opacity_resp = ui.add(
                eframe::egui::Slider::new(&mut state.pen_opacity, 0.05..=1.0).show_value(false),
            );
            if opacity_resp.on_hover_text(t.canvas_opacity).changed() {
                action = Some(CanvasUiAction::SetStyle {
                    color: None,
                    width: None,
                    opacity: Some(state.pen_opacity),
                    dash: None,
                });
            }
            let dashed_on = state.pen_dashed;
            if toolbar_icon_selectable(ui, dashed_on, "—", t.canvas_dashed) {
                state.pen_dashed = !dashed_on;
                action = Some(CanvasUiAction::SetStyle {
                    color: None,
                    width: None,
                    opacity: None,
                    dash: Some(pen_dash_vec(state.pen_dashed)),
                });
            }
            if matches!(
                state.tool,
                CanvasTool::Rect | CanvasTool::Ellipse | CanvasTool::Path
            ) && state.shape_fill
            {
                let grad_on = state.use_gradient;
                if toolbar_icon_selectable(ui, grad_on, "G", t.canvas_gradient) {
                    state.use_gradient = !grad_on;
                }
                if state.use_gradient {
                    let mut rgba = [
                        state.gradient_color2.r() as f32 / 255.0,
                        state.gradient_color2.g() as f32 / 255.0,
                        state.gradient_color2.b() as f32 / 255.0,
                        1.0,
                    ];
                    if ui
                        .color_edit_button_rgba_unmultiplied(&mut rgba)
                        .on_hover_text(t.canvas_gradient)
                        .changed()
                    {
                        state.gradient_color2 = Color32::from_rgb(
                            (rgba[0] * 255.0) as u8,
                            (rgba[1] * 255.0) as u8,
                            (rgba[2] * 255.0) as u8,
                        );
                    }
                }
            }
            if toolbar_icon_button(ui, "↶", t.canvas_undo) {
                action = Some(CanvasUiAction::Apply(CanvasOpBody::Undo));
            }
            if toolbar_icon_button(ui, "P", t.canvas_export) {
                action = Some(CanvasUiAction::ExportPng);
            }
            if toolbar_icon_button(ui, "S", t.canvas_export_svg) {
                action = Some(CanvasUiAction::ExportSvg);
            }
            if toolbar_icon_button(ui, "J", t.canvas_export_json) {
                action = Some(CanvasUiAction::ExportJson);
            }
            if toolbar_icon_button(ui, "I", t.canvas_import) {
                action = Some(CanvasUiAction::ImportJson);
            }
            if toolbar_icon_button(ui, "⊙", t.canvas_reset_view) {
                action = Some(CanvasUiAction::ResetView);
            }
            let grid_on = state.show_grid;
            if toolbar_icon_selectable(ui, grid_on, "#", t.canvas_grid) {
                state.show_grid = !grid_on;
            }
            let snap_on = state.snap;
            if toolbar_icon_selectable(ui, snap_on, "⊞", t.canvas_snap) {
                state.snap = !snap_on;
            }
            if state.clear_confirm_open {
                if toolbar_icon_button(ui, "✓", t.canvas_clear_confirm_yes) {
                    state.clear_confirm_open = false;
                    action = Some(CanvasUiAction::Apply(CanvasOpBody::Clear));
                }
                if toolbar_icon_button(ui, "×", t.canvas_clear_confirm_no) {
                    state.clear_confirm_open = false;
                }
            } else if toolbar_icon_button(ui, "✕", t.canvas_clear) {
                state.clear_confirm_open = true;
            }
        });
    });

    action
}

/// Muted aspect chips on the canvas face, above the letterboxed board.
pub fn ui_canvas_aspect_row(
    ui: &mut Ui,
    t: &UiStrings,
    aspect: CanvasAspect,
) -> Option<CanvasUiAction> {
    let mut action: Option<CanvasUiAction> = None;
    ui.horizontal_wrapped(|ui| {
        for (label, candidate) in canvas_aspect_chip_labels(t) {
            let selected = aspect == candidate;
            let text = eframe::egui::RichText::new(label).weak();
            if ui.selectable_label(selected, text).clicked() && !selected {
                action = Some(CanvasUiAction::SetAspect(candidate));
            }
        }
    });
    action
}

/// Compact layer stack: name, hide, lock, new layer.
pub fn ui_canvas_layers(
    ui: &mut Ui,
    t: &UiStrings,
    state: &mut CanvasPanelState,
) -> Option<CanvasUiAction> {
    let mut action: Option<CanvasUiAction> = None;
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(eframe::egui::RichText::new(t.canvas_layer_add).weak())
            .clicked()
        {
            action = Some(CanvasUiAction::Edit(CanvasEdit::LayerCreate {
                name: None,
                parent_id: None,
            }));
        }
        let layers = state.layers.clone();
        let layer_count = layers.len();
        let active_id = state.active_layer_id.clone();
        for (layer_idx, layer) in layers.iter().enumerate() {
            let selected = active_id == layer.id;
            if ui.selectable_label(selected, &layer.name).clicked() {
                action = Some(CanvasUiAction::Edit(CanvasEdit::LayerActivate {
                    id: layer.id.clone(),
                }));
            }
            if selected {
                if ui.small_button(t.canvas_layer_rename).clicked() {
                    state.layer_rename_id = Some(layer.id.clone());
                    state.layer_rename_text = layer.name.clone();
                }
                if layer_idx > 0
                    && ui
                        .small_button("↑")
                        .on_hover_text(t.canvas_z_forward)
                        .clicked()
                {
                    action = Some(CanvasUiAction::Edit(CanvasEdit::LayerReorder {
                        id: layer.id.clone(),
                        parent_id: None,
                        z: (layer_idx as i64) - 1,
                    }));
                }
                if layer_idx + 1 < layer_count
                    && ui
                        .small_button("↓")
                        .on_hover_text(t.canvas_z_back)
                        .clicked()
                {
                    action = Some(CanvasUiAction::Edit(CanvasEdit::LayerReorder {
                        id: layer.id.clone(),
                        parent_id: None,
                        z: (layer_idx as i64) + 1,
                    }));
                }
            }
            let mut vis = layer.visible;
            if ui
                .toggle_value(&mut vis, t.canvas_layer_vis_short)
                .on_hover_text(t.canvas_layer_visible)
                .changed()
            {
                action = Some(CanvasUiAction::Edit(CanvasEdit::LayerSet {
                    id: layer.id.clone(),
                    visible: Some(vis),
                    locked: None,
                    opacity: None,
                }));
            }
            let mut locked = layer.locked;
            if ui
                .toggle_value(&mut locked, t.canvas_layer_lock_short)
                .on_hover_text(t.canvas_layer_locked)
                .changed()
            {
                action = Some(CanvasUiAction::Edit(CanvasEdit::LayerSet {
                    id: layer.id.clone(),
                    visible: None,
                    locked: Some(locked),
                    opacity: None,
                }));
            }
            let mut opacity = layer.opacity;
            let opacity_resp = ui.add(
                eframe::egui::DragValue::new(&mut opacity)
                    .range(0.0..=1.0)
                    .speed(0.05)
                    .max_decimals(2),
            );
            if opacity_resp.changed() {
                action = Some(CanvasUiAction::Edit(CanvasEdit::LayerSet {
                    id: layer.id.clone(),
                    visible: None,
                    locked: None,
                    opacity: Some(opacity),
                }));
            }
            opacity_resp.on_hover_text(t.canvas_layer_opacity);
            if layer_count > 1
                && ui
                    .button(
                        eframe::egui::RichText::new(t.canvas_layer_delete)
                            .small()
                            .weak(),
                    )
                    .clicked()
            {
                action = Some(CanvasUiAction::Edit(CanvasEdit::LayerDelete {
                    id: layer.id.clone(),
                }));
            }
        }
        if let Some(rename_id) = state.layer_rename_id.clone() {
            ui.horizontal(|ui| {
                ui.label(t.canvas_layer_rename);
                let resp = ui.text_edit_singleline(&mut state.layer_rename_text);
                if resp.lost_focus() {
                    let name = state.layer_rename_text.trim().to_string();
                    if !name.is_empty() {
                        action = Some(CanvasUiAction::Edit(CanvasEdit::LayerRename {
                            id: rename_id,
                            name,
                        }));
                    }
                    state.layer_rename_id = None;
                }
            });
        }
    });
    action
}

/// Drawing surface — letterboxed board inside the split pane.
pub fn ui_canvas_surface(
    ui: &mut Ui,
    state: &mut CanvasPanelState,
    aspect: CanvasAspect,
    empty_hint: &str,
) -> Option<CanvasUiAction> {
    let mut action: Option<CanvasUiAction> = None;
    let dark = ui.visuals().dark_mode;
    let avail = ui.available_size();
    let pane_h = avail.y.max(1.0);
    let pane_w = avail.x.max(1.0);
    let (response, painter) =
        ui.allocate_painter(Vec2::new(pane_w, pane_h), Sense::click_and_drag());
    let outer = response.rect;
    let rect = view_board_rect(outer, aspect, state.view_pan, state.view_zoom);
    let bg = canvas_bg(dark);
    painter.rect_filled(rect, 0.0, bg);
    painter.rect_stroke(rect, 0.0, Stroke::new(1.5_f32, SIGNAL), StrokeKind::Inside);
    if state.show_grid {
        paint_board_grid(&painter, rect);
    }

    let now = ui.ctx().input(|i| i.time);
    if state.seeing {
        let pulse = ((now * 2.4).sin() * 0.12 + 0.88) as f32;
        let alpha = (200.0 * pulse) as u8;
        let contour = Color32::from_rgba_unmultiplied(SIGNAL.r(), SIGNAL.g(), SIGNAL.b(), alpha);
        let outline = rect.expand(4.0);
        painter.rect_stroke(
            outline,
            0.0,
            Stroke::new(2.5_f32, contour),
            StrokeKind::Outside,
        );
    }

    for op in &state.ops {
        if !layer_is_visible(&state.layers, &op.layer_id) {
            continue;
        }
        let p = anim_progress(state, op.seq, now);
        paint_op(&painter, rect, op, &state.layers, dark, p);
    }

    if let Some(seq) = state.selected_seq {
        if let Some(op) = state.ops.iter().find(|o| o.seq == seq) {
            if let Some(b) = canvas_op_bbox(&op.body) {
                let sel = eframe::egui::Rect::from_two_pos(
                    to_screen(rect, CanvasPoint { x: b.x0, y: b.y0 }),
                    to_screen(rect, CanvasPoint { x: b.x1, y: b.y1 }),
                );
                painter.rect_stroke(sel, 0.0, Stroke::new(1.5_f32, SIGNAL), StrokeKind::Outside);
            }
        }
    }

    if state.draft_points.len() >= 2 {
        let screen: Vec<Pos2> = state
            .draft_points
            .iter()
            .map(|p| to_screen(rect, *p))
            .collect();
        let c = if state.tool == CanvasTool::Eraser {
            bg
        } else {
            state.color
        };
        let (opacity, dash, _) = pen_style_fields(state);
        let draft_color = canvas_paint::with_alpha(c, opacity);
        let rad = radius_px(rect, state.width);
        match state.tool {
            CanvasTool::Spline if screen.len() >= 2 => {
                let sampled: Vec<Pos2> = sample_spline_points(&state.draft_points, 16)
                    .iter()
                    .map(|p| to_screen(rect, *p))
                    .collect();
                if dash.is_empty() {
                    painter.add(Shape::line(sampled, path_stroke(rad * 2.0, draft_color)));
                } else {
                    paint_polyline(
                        &painter,
                        rect,
                        &state.draft_points,
                        state.width,
                        draft_color,
                        &dash,
                    );
                }
            }
            CanvasTool::Path if screen.len() >= 2 => {
                let sampled: Vec<Pos2> = sample_spline_points(&state.draft_points, 16)
                    .iter()
                    .map(|p| to_screen(rect, *p))
                    .collect();
                if state.shape_fill {
                    painter.add(Shape::Path(PathShape {
                        points: sampled,
                        closed: true,
                        fill: draft_color,
                        stroke: PathStroke::NONE,
                    }));
                } else if dash.is_empty() {
                    painter.add(Shape::line(sampled, path_stroke(rad * 2.0, draft_color)));
                } else {
                    paint_polyline(
                        &painter,
                        rect,
                        &state.draft_points,
                        state.width,
                        draft_color,
                        &dash,
                    );
                }
            }
            _ => {
                if dash.is_empty() {
                    painter.add(Shape::line(screen, path_stroke(rad * 2.0, draft_color)));
                } else {
                    paint_polyline(
                        &painter,
                        rect,
                        &state.draft_points,
                        state.width,
                        draft_color,
                        &dash,
                    );
                }
            }
        }
    }
    if let (Some(a), Some(b)) = (state.drag_origin, state.drag_current) {
        let r = eframe::egui::Rect::from_two_pos(to_screen(rect, a), to_screen(rect, b));
        match state.tool {
            CanvasTool::Rect => {
                if state.shape_fill {
                    painter.rect_filled(r, 0.0, state.color);
                } else {
                    painter.rect_stroke(
                        r,
                        0.0,
                        Stroke::new(2.0_f32, state.color),
                        StrokeKind::Inside,
                    );
                }
            }
            CanvasTool::Ellipse => {
                painter.add(Shape::Path(PathShape {
                    points: ellipse_points(r, 48),
                    closed: true,
                    fill: if state.shape_fill {
                        state.color
                    } else {
                        Color32::TRANSPARENT
                    },
                    stroke: if state.shape_fill {
                        PathStroke::NONE
                    } else {
                        PathStroke::new(2.0_f32, state.color)
                    },
                }));
            }
            CanvasTool::Line => {
                painter.add(Shape::line(
                    vec![to_screen(rect, a), to_screen(rect, b)],
                    PathStroke::new(radius_px(rect, state.width) * 2.0, state.color),
                ));
            }
            CanvasTool::Pen
            | CanvasTool::Eraser
            | CanvasTool::Spline
            | CanvasTool::Path
            | CanvasTool::Select
            | CanvasTool::Pan => {}
        }
    }

    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.01 {
            let factor = (scroll * 0.002).exp();
            state.view_zoom = (state.view_zoom * factor).clamp(0.25, 4.0);
            ui.ctx().request_repaint();
        }
    }

    if response.dragged() {
        if state.tool == CanvasTool::Pan {
            state.view_pan += response.drag_delta();
            ui.ctx().request_repaint();
        } else if let Some(pos) = response.interact_pointer_pos() {
            if !rect.contains(pos) {
                return action;
            }
            let p = maybe_snap_point(to_norm(rect, pos), state.snap);
            match state.tool {
                CanvasTool::Pan => {}
                CanvasTool::Pen | CanvasTool::Eraser | CanvasTool::Spline | CanvasTool::Path => {
                    if state
                        .draft_points
                        .last()
                        .map(|q| (q.x - p.x).abs() + (q.y - p.y).abs() > 0.002)
                        .unwrap_or(true)
                    {
                        state.draft_points.push(p);
                    }
                }
                CanvasTool::Line | CanvasTool::Rect | CanvasTool::Ellipse => {
                    if state.drag_origin.is_none() {
                        state.drag_origin = Some(p);
                    }
                    state.drag_current = Some(p);
                }
                CanvasTool::Select => {
                    if state.drag_origin.is_none() {
                        state.drag_origin = Some(p);
                    }
                    state.drag_current = Some(p);
                }
            }
        }
        ui.ctx().request_repaint();
    }

    if response.clicked() && state.tool == CanvasTool::Select {
        if let Some(pos) = response.interact_pointer_pos() {
            if rect.contains(pos) {
                let p = maybe_snap_point(to_norm(rect, pos), state.snap);
                let visible: Vec<&CanvasOp> = state
                    .ops
                    .iter()
                    .filter(|op| {
                        layer_is_visible(&state.layers, &op.layer_id)
                            && !layer_is_locked(&state.layers, &op.layer_id)
                    })
                    .collect();
                state.selected_seq = canvas_hit_test(visible, p.x, p.y);
            }
        }
    }

    if response.drag_stopped() && action.is_none() {
        let pointer_in_board = response
            .interact_pointer_pos()
            .map(|p| rect.contains(p))
            .unwrap_or(false);
        if !pointer_in_board {
            state.draft_points.clear();
            state.drag_origin = None;
            state.drag_current = None;
        } else {
            match state.tool {
                CanvasTool::Pan => {
                    state.drag_origin = None;
                    state.drag_current = None;
                }
                CanvasTool::Pen => {
                    if state.draft_points.len() >= 2 {
                        let (opacity, dash, _) = pen_style_fields(state);
                        action = Some(CanvasUiAction::Apply(CanvasOpBody::Stroke {
                            points: std::mem::take(&mut state.draft_points),
                            color: color_to_hex(state.color),
                            width: state.width,
                            opacity,
                            dash,
                        }));
                    } else {
                        state.draft_points.clear();
                    }
                }
                CanvasTool::Eraser => {
                    if !state.draft_points.is_empty() {
                        action = Some(CanvasUiAction::Apply(CanvasOpBody::Erase {
                            points: std::mem::take(&mut state.draft_points),
                            width: state.width.max(0.03),
                        }));
                    }
                }
                CanvasTool::Spline => {
                    if state.draft_points.len() >= 2 {
                        let (opacity, dash, _) = pen_style_fields(state);
                        action = Some(CanvasUiAction::Apply(CanvasOpBody::Spline {
                            points: std::mem::take(&mut state.draft_points),
                            color: color_to_hex(state.color),
                            width: state.width,
                            opacity,
                            dash,
                        }));
                    } else {
                        state.draft_points.clear();
                    }
                }
                CanvasTool::Line => {
                    if let (Some(a), Some(b)) =
                        (state.drag_origin.take(), state.drag_current.take())
                    {
                        let (opacity, dash, _) = pen_style_fields(state);
                        action = Some(CanvasUiAction::Apply(CanvasOpBody::Line {
                            p0: a,
                            p1: b,
                            color: color_to_hex(state.color),
                            width: state.width,
                            opacity,
                            dash,
                        }));
                    }
                }
                CanvasTool::Rect => {
                    if let (Some(a), Some(b)) =
                        (state.drag_origin.take(), state.drag_current.take())
                    {
                        let x = a.x.min(b.x);
                        let y = a.y.min(b.y);
                        let w = (a.x - b.x).abs().max(0.01);
                        let h = (a.y - b.y).abs().max(0.01);
                        let (opacity, dash, gradient) = pen_style_fields(state);
                        action = Some(CanvasUiAction::Apply(CanvasOpBody::Rect {
                            x,
                            y,
                            w,
                            h,
                            color: color_to_hex(state.color),
                            fill: state.shape_fill,
                            width: state.width,
                            rotation: 0.0,
                            opacity,
                            dash,
                            gradient,
                        }));
                    }
                }
                CanvasTool::Ellipse => {
                    if let (Some(a), Some(b)) =
                        (state.drag_origin.take(), state.drag_current.take())
                    {
                        let x = a.x.min(b.x);
                        let y = a.y.min(b.y);
                        let w = (a.x - b.x).abs().max(0.01);
                        let h = (a.y - b.y).abs().max(0.01);
                        let (opacity, dash, gradient) = pen_style_fields(state);
                        action = Some(CanvasUiAction::Apply(CanvasOpBody::Ellipse {
                            x,
                            y,
                            w,
                            h,
                            color: color_to_hex(state.color),
                            fill: state.shape_fill,
                            width: state.width,
                            rotation: 0.0,
                            opacity,
                            dash,
                            gradient,
                        }));
                    }
                }
                CanvasTool::Path => {
                    if state.draft_points.len() >= 3 {
                        let (opacity, dash, gradient) = pen_style_fields(state);
                        action = Some(CanvasUiAction::Apply(CanvasOpBody::Path {
                            points: std::mem::take(&mut state.draft_points),
                            color: color_to_hex(state.color),
                            width: state.width,
                            fill: state.shape_fill,
                            closed: true,
                            opacity,
                            dash,
                            gradient,
                        }));
                    } else {
                        state.draft_points.clear();
                    }
                }
                CanvasTool::Select => {
                    if let (Some(a), Some(b), Some(seq)) = (
                        state.drag_origin.take(),
                        state.drag_current.take(),
                        state.selected_seq,
                    ) {
                        let dx = b.x - a.x;
                        let dy = b.y - a.y;
                        if dx.abs() + dy.abs() > 0.002 {
                            let locked = state
                                .ops
                                .iter()
                                .find(|o| o.seq == seq)
                                .map(|o| {
                                    canvas_layer_effective_locked(
                                        &aos_proto::CanvasDoc {
                                            layers: state.layers.clone(),
                                            active_layer_id: state.active_layer_id.clone(),
                                            ..Default::default()
                                        },
                                        &o.layer_id,
                                    )
                                })
                                .unwrap_or(false);
                            if !locked {
                                if let Some(op) = state.ops.iter_mut().find(|o| o.seq == seq) {
                                    translate_canvas_op_body(&mut op.body, dx, dy);
                                }
                                action =
                                    Some(CanvasUiAction::Edit(CanvasEdit::Move { seq, dx, dy }));
                            }
                        }
                    }
                }
            }
        }
    }

    if action.is_none() && response.hovered() {
        if state.tool == CanvasTool::Select {
            if let Some(seq) = state.selected_seq {
                let delete = ui.input(|i| {
                    i.key_pressed(eframe::egui::Key::Delete)
                        || i.key_pressed(eframe::egui::Key::Backspace)
                });
                if delete
                    && !layer_is_locked(
                        &state.layers,
                        state
                            .ops
                            .iter()
                            .find(|o| o.seq == seq)
                            .map(|o| o.layer_id.as_str())
                            .unwrap_or(""),
                    )
                {
                    action = Some(CanvasUiAction::Edit(CanvasEdit::Delete { seq }));
                    state.ops.retain(|o| o.seq != seq);
                    state.selected_seq = None;
                }
            }
        }
        let undo = ui.input(|i| i.modifiers.command && i.key_pressed(eframe::egui::Key::Z));
        if undo && action.is_none() {
            action = Some(CanvasUiAction::Apply(CanvasOpBody::Undo));
        }
    }

    if !state.animating.is_empty() {
        ui.ctx().request_repaint();
    }

    if state.ops.is_empty()
        && state.draft_points.is_empty()
        && state.drag_origin.is_none()
        && !empty_hint.is_empty()
    {
        let font = eframe::egui::FontId::proportional(13.0);
        let wrap_w = (rect.width() - 20.0).max(40.0);
        let galley = ui.fonts(|f| {
            f.layout(
                empty_hint.to_owned(),
                font,
                ui.visuals().weak_text_color(),
                wrap_w,
            )
        });
        let mut pos = rect.center() - galley.size() * 0.5;
        pos.x = clamp_in_range(
            pos.x,
            rect.left() + 8.0,
            rect.right() - galley.size().x - 8.0,
        );
        pos.y = clamp_in_range(
            pos.y,
            rect.top() + 8.0,
            rect.bottom() - galley.size().y - 8.0,
        );
        ui.painter().galley(pos, galley, Color32::TRANSPARENT);
    }

    if let Some(pos) = response.hover_pos() {
        if rect.contains(pos) {
            let p = maybe_snap_point(to_norm(rect, pos), state.snap);
            let (ew, eh) = aspect.export_dimensions(1024);
            let px = p.x * (ew.saturating_sub(1) as f32);
            let py = p.y * (eh.saturating_sub(1) as f32);
            let label = format!("{:.2}, {:.2}  ·  {:.0}×{:.0} px", p.x, p.y, px, py);
            painter.text(
                Pos2::new(rect.left() + 8.0, rect.bottom() - 6.0),
                Align2::LEFT_BOTTOM,
                label,
                FontId::monospace(11.0),
                SIGNAL,
            );
        }
    }

    if state.snap {
        if let Some(CanvasUiAction::Apply(body)) = action.as_mut() {
            snap_body(body);
        }
    }

    action
}

// ── Frozen routing (designer) ───────────────────────────────────────────────
// 1. Canvas OPEN + dessine/draw/sketch → board (`canvas.*`).
// 2. Canvas CLOSED + dessine/draw/sketch → Create (`media.image.generate`).
// 3. Explicit phrase (dans/sur le canvas, au trait, /canvas, …) → board; opens face even if closed.
// 4. « encore » / « vas-y » alone → not enough for canvas or face.
// 5. Lone « canvas » → not enough.

/// Phrases that beat Create/image routing — lone « canvas » is not enough.
const EXPLICIT_CANVAS_MARKERS: &[&str] = &[
    "/canvas",
    "/canevas",
    "sur le canvas",
    "dans le canvas",
    "on the canvas",
    "in the canvas",
    "to the canvas",
    "sur le canevas",
    "dans le canevas",
    "on the canevas",
    "in the canevas",
    "to the canevas",
    // « au trait » = vector strokes on the session canvas (not pixel diffusion).
    "au trait",
];

/// Explicit phrase → open canvas face (even when toggle was closed).
pub fn chat_should_open_canvas_face(text: &str) -> bool {
    chat_user_wants_explicit_canvas(text)
}

/// Explicit vector-canvas intent: toggle phrase, slash, or stroke wording — not bare « dessine ».
pub fn chat_user_wants_explicit_canvas(text: &str) -> bool {
    let lower = text.to_lowercase();
    EXPLICIT_CANVAS_MARKERS.iter().any(|m| lower.contains(m))
}

fn word_boundary_match(lower: &str, pat: &str) -> bool {
    lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word == pat)
}

/// Bare draw / sketch wording in the message (no canvas-open context).
pub fn chat_user_has_draw_wording(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "dessin",
        "dessine",
        "dessiner",
        "draw",
        "drawing",
        "sketch",
        "trace",
        "tracer",
        "esquisse",
        "redessine",
        "redessiner",
        "illustration",
        "illustrer",
    ]
    .iter()
    .any(|k| word_boundary_match(&lower, k))
}

/// Short follow-ups that must not spawn a new canvas agent from a closed board.
fn chat_is_canvas_followup_steal(text: &str) -> bool {
    let trimmed = text.trim().to_lowercase();
    [
        "encore",
        "vas-y",
        "vas y",
        "go ahead",
        "relance",
        "lance",
        "améliore",
        "ameliore",
    ]
    .iter()
    .any(|k| trimmed == *k)
}

/// Bare draw / sketch → Image Studio only when the session canvas is closed.
pub fn chat_user_wants_pixel_draw(text: &str, canvas_open: bool) -> bool {
    if chat_user_wants_explicit_canvas(text) || canvas_open {
        return false;
    }
    chat_user_has_draw_wording(text)
}

/// Déléguer un agent canvas : marqueurs explicites, ou dessin nu avec le panneau ouvert.
pub fn chat_wants_canvas_agent(text: &str, canvas_open: bool) -> bool {
    if chat_user_wants_explicit_canvas(text) {
        return true;
    }
    canvas_open && chat_user_has_draw_wording(text) && !chat_is_canvas_followup_steal(text)
}

fn canvas_aspect_chip_labels(t: &UiStrings) -> [(&'static str, CanvasAspect); 5] {
    [
        (t.canvas_aspect_square, CanvasAspect::Square),
        (t.canvas_aspect_16_9, CanvasAspect::Landscape16x9),
        (t.canvas_aspect_16_10, CanvasAspect::Landscape16x10),
        (t.canvas_aspect_vertical, CanvasAspect::Portrait9x16),
        (t.canvas_aspect_horizontal, CanvasAspect::Landscape3x2),
    ]
}

/// Designer copy for delegated canvas agents — only lists tools actually
/// granted to an agent, not every low-level export of the canvas module.
pub fn canvas_agent_designer_guide(exported: &[String]) -> String {
    let allowed = aos_agent::tools::filter_canvas_tool_ids(exported);
    let has_path = allowed.iter().any(|t| t == "canvas.path");
    let silhouette = if has_path {
        "Silhouettes (colline, corps, toit, voiles) : `canvas.path` avec 4–8 points et fill:true — \
un path par forme lisible, pas 20 splines/rects empilés. Traits fins : canvas.stroke/line/spline. "
    } else {
        "Silhouettes : empile canvas.spline ou canvas.rect/ellipse (fill:true) — \
un shape par forme lisible. Traits fins : canvas.stroke/line. "
    };
    let windmill = if has_path {
        "Exemple moulin sur colline : path colline (#8B7355) → path corps (#C4A574) → path toit (#8B4513) → \
2–4 paths pour les ailes (#E8DCC8) ; rect/ellipse seulement pour détails (fenêtre, porte).\n"
    } else {
        "Exemple moulin : spline/rect pour colline et corps, ellipse pour voiles — rect pour fenêtre/porte.\n"
    };
    let tools_line = if allowed.is_empty() {
        "Outils canvas : (module non chargé — commence par canvas.get si disponible).".to_string()
    } else {
        let mut names: Vec<&str> = allowed
            .iter()
            .filter(|t| t.starts_with("canvas."))
            .map(|s| s.as_str())
            .collect();
        names.sort_unstable();
        format!(
            "Outils : {} (coords 0..1 ; rect/ellipse : x,y,w,h — alias width/height acceptés). \
Pas media.image.generate. Pas agent.spawn.",
            names.join(", ")
        )
    };
    format!(
        "Cible visuelle : lisible à l'export PNG (canvas.export, long edge 1024) — pas un rectangle+triangle.\n\
Espace : coords normalisées 0..1 uniquement (max 1.0) sur le cadre visible (origine coin supérieur gauche) — jamais des pixels.\n\
« 200px » = taille d'export pour la lisibilité humaine, pas l'unité des coords (ne pas dessiner à x=200).\n\
Règles : margin 0.08–0.12 ; sujet centré dans usable ; couches sol → volumes → détails → 2–3 ombres. \
Lis `scene_bbox` dans le digest : ne superpose pas les nouvelles formes au même centre. \
{silhouette}\
Plan de composition imposé : Analyse (canvas.get seulement, aucun trait), Composition (2 formes d'ancrage), \
Volumes principaux (3 formes distinctes), Détails distinctifs (3 formes), Finitions (2 accents/ombres), Export. \
Chaque volume doit contribuer à la reconnaissance du sujet ; ne redessine jamais la silhouette aux étapes suivantes. \
`width` est l'épaisseur du contour, pas la largeur géométrique : ≤0.04 ; pour rect/ellipse, la taille est `w`,`h`. \
Commence par canvas.get. Couleur : canvas.set_style {{color:\"#RRGGBB\"}} ou color= sur chaque op — \
le teal signal n'est pas la seule teinte ; après critique, change de teinte pour ombres/détails.\n\
Après critique : ajoute la pièce manquante seulement — jamais canvas.clear ni redessiner le sujet depuis zéro sauf si l'humain dit effacer.\n\
{windmill}\
{tools_line}"
    )
}

const CANVAS_AGENT_NO_FANOUT_GUIDE: &str = "\
Tu es l'auteur unique de ce dessin : traits séquentiels canvas.* (path pour silhouettes quand exporté, sinon spline/rect/ellipse). \
Pas de agent.spawn ni agent.await — un seul agent, pas de sous-agents parallèles pour le même sujet.";

/// System prompt addendum for delegated canvas agents (designer rules + frame aspect).
pub fn canvas_agent_system_prompt(aspect: CanvasAspect, exported: &[String]) -> String {
    format!(
        "{}\n\
         Proportions actuelles du cadre : {} ({}).\n\n\
         {CANVAS_AGENT_NO_FANOUT_GUIDE}",
        canvas_agent_designer_guide(exported),
        aspect.agent_label_fr(),
        aspect.agent_label_en(),
    )
}

/// Full brief for display / logs: line 1 = user request verbatim, then designer guide.
#[cfg(test)]
pub fn canvas_agent_brief(user_text: &str, aspect: CanvasAspect, exported: &[String]) -> String {
    format!(
        "{}\n\n{}\nProportions actuelles du cadre : {} ({}).",
        user_text.trim(),
        canvas_agent_designer_guide(exported),
        aspect.agent_label_fr(),
        aspect.agent_label_en(),
    )
}

#[cfg(test)]
mod routing_tests {
    use super::*;

    #[test]
    fn empty_hint_clamp_survives_inverted_bounds() {
        let x = clamp_in_range(491.5, 493.27505, 489.72498);
        assert!(x.is_finite());
        assert!((489.72498..=493.27505).contains(&x));
    }

    #[test]
    fn toolbar_min_width_covers_icon_row() {
        let w = toolbar_content_min_width(false, false);
        assert!(w > 400.0, "icon toolbar scroll extent, got {w}");
    }

    #[test]
    fn bare_draw_is_pixel_not_canvas_when_closed() {
        assert!(chat_user_wants_pixel_draw("dessine une maison", false));
        assert!(!chat_user_wants_explicit_canvas("dessine une maison"));
        assert!(!chat_wants_canvas_agent("dessine une maison", false));
    }

    #[test]
    fn bare_draw_uses_canvas_when_board_open() {
        assert!(!chat_user_wants_pixel_draw("dessine une maison", true));
        assert!(chat_wants_canvas_agent("dessine une maison", true));
        assert!(chat_wants_canvas_agent("dessine moi un chat", true));
    }

    #[test]
    fn explicit_canvas_markers() {
        assert!(chat_user_wants_explicit_canvas("dessine sur le canvas"));
        assert!(chat_user_wants_explicit_canvas("dessine dans le canvas"));
        assert!(chat_user_wants_explicit_canvas("draw on the canvas"));
        assert!(chat_user_wants_explicit_canvas("draw in the canvas"));
        assert!(chat_user_wants_explicit_canvas("add to the canvas a door"));
        assert!(chat_user_wants_explicit_canvas("dessine sur le canevas"));
        assert!(chat_user_wants_explicit_canvas("ajoute au trait une porte"));
        assert!(chat_user_wants_explicit_canvas("/canvas"));
        assert!(!chat_user_wants_pixel_draw("dessine sur le canvas", false));
        assert!(!chat_user_wants_pixel_draw("dessine dans le canvas", false));
        assert!(!chat_user_wants_pixel_draw("dessine sur le canvas", true));
    }

    #[test]
    fn lone_canvas_word_is_not_explicit() {
        for msg in [
            "canvas",
            "le canvas",
            "dessine sur canvas",
            "draw on canvas",
            "parle du canvas",
            "ouvre le canvas",
        ] {
            assert!(
                !chat_user_wants_explicit_canvas(msg),
                "lone or bare canvas must not route: {msg}"
            );
        }
        assert!(chat_user_wants_pixel_draw("dessine sur canvas", false));
        assert!(chat_user_wants_pixel_draw("draw on canvas", false));
        assert!(!chat_user_wants_pixel_draw("dessine sur canvas", true));
    }

    #[test]
    fn dans_le_canvas_routes_canvas_not_image() {
        assert!(chat_wants_canvas_agent("dessine dans le canvas", false));
        assert!(!chat_user_wants_pixel_draw("dessine dans le canvas", false));
    }

    #[test]
    fn bare_dessine_still_routes_image_when_closed() {
        assert!(chat_user_wants_pixel_draw("dessine une maison", false));
        assert!(!chat_user_wants_explicit_canvas("dessine une maison"));
        assert!(!chat_wants_canvas_agent("dessine une maison", false));
    }

    #[test]
    fn followup_keywords_do_not_steal_canvas() {
        for msg in ["encore", "vas-y", "vas y", "lance", "améliore", "go ahead"] {
            assert!(
                !chat_wants_canvas_agent(msg, true),
                "follow-up {msg} must not route to canvas"
            );
        }
    }

    #[test]
    fn withdraw_does_not_match_draw() {
        assert!(!chat_user_wants_pixel_draw("withdraw funds", false));
    }

    fn sample_agent_line(seq: u64) -> CanvasOp {
        CanvasOp {
            seq,
            author_id: "agent-81".into(),
            ts_ms: 0,
            layer_id: String::new(),
            body: CanvasOpBody::Line {
                p0: CanvasPoint { x: 0.2, y: 0.2 },
                p1: CanvasPoint { x: 0.8, y: 0.2 },
                color: "#f40009".into(),
                width: 0.02,
                opacity: 1.0,
                dash: vec![],
            },
        }
    }

    #[test]
    fn apply_snapshot_keeps_pending_human_stroke() {
        let mut state = CanvasPanelState::default();
        state.ops.push(CanvasOp {
            seq: 0,
            author_id: "human".into(),
            ts_ms: 0,
            layer_id: String::new(),
            body: CanvasOpBody::Stroke {
                points: vec![
                    CanvasPoint { x: 0.1, y: 0.1 },
                    CanvasPoint { x: 0.2, y: 0.2 },
                ],
                color: "#3ee0c4".into(),
                width: 0.01,
                opacity: 1.0,
                dash: vec![],
            },
        });
        state.apply_snapshot(vec![], 1, 0.0);
        assert_eq!(state.ops.len(), 1);
        assert_eq!(state.ops[0].author_id, "human");
    }

    #[test]
    fn apply_snapshot_ignores_stale_empty_document() {
        let mut state = CanvasPanelState::default();
        let ops: Vec<CanvasOp> = (1..=8).map(sample_agent_line).collect();
        state.apply_snapshot(ops, 9, 1.0);
        assert_eq!(state.ops.len(), 8);
        assert_eq!(state.next_seq, 9);
        assert_eq!(state.last_seen_seq, 8);

        state.apply_snapshot(vec![], 1, 2.0);
        assert_eq!(
            state.ops.len(),
            8,
            "late empty poll must not wipe agent strokes"
        );
        assert_eq!(state.next_seq, 9);
        assert_eq!(state.last_seen_seq, 8);
    }

    #[test]
    fn poll_after_seq_full_resync_when_board_empty() {
        let mut state = CanvasPanelState::default();
        assert_eq!(state.poll_after_seq(), None);

        state.apply_snapshot((1..=8).map(sample_agent_line).collect(), 9, 1.0);
        assert_eq!(state.poll_after_seq(), Some(8));

        state.ops.clear();
        assert_eq!(
            state.poll_after_seq(),
            None,
            "wiped board must refetch the full document, not seq > last_seen"
        );
    }

    #[test]
    fn apply_snapshot_accepts_same_seq_clear() {
        let mut state = CanvasPanelState::default();
        state.apply_snapshot((1..=3).map(sample_agent_line).collect(), 4, 1.0);
        state.apply_snapshot(vec![], 4, 2.0);
        assert!(state.ops.is_empty());
        assert_eq!(state.next_seq, 4);
    }

    #[test]
    fn snap_unit_rounds_to_hundredths() {
        assert!((snap_unit(0.014) - 0.01).abs() < 1e-6);
        assert!((snap_unit(0.015) - 0.02).abs() < 1e-6);
        assert_eq!(snap_unit(-0.2), 0.0);
        assert_eq!(snap_unit(1.4), 1.0);
        let p = snap_point(CanvasPoint { x: 0.333, y: 0.666 });
        assert!((p.x - 0.33).abs() < 1e-6);
        assert!((p.y - 0.67).abs() < 1e-6);
    }

    #[test]
    fn fit_board_rect_letterboxes_wide_in_tall_pane() {
        let outer = eframe::egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 600.0));
        let board = fit_board_rect(outer, CanvasAspect::Landscape16x9);
        assert!(board.width() <= outer.width());
        assert!(board.height() < outer.height());
        assert!((board.width() / board.height() - 16.0 / 9.0).abs() < 0.01);
    }

    #[test]
    fn frozen_routing_rules() {
        // OPEN + dessine/draw/sketch → board (canvas tools).
        for msg in ["dessine une maison", "draw a cat", "sketch a tree"] {
            assert!(chat_wants_canvas_agent(msg, true), "open board: {msg}");
            assert!(!chat_user_wants_pixel_draw(msg, true), "open board: {msg}");
        }
        // CLOSED + dessine → Create (image).
        for msg in ["dessine une maison", "draw a cat", "sketch a tree"] {
            assert!(chat_user_wants_pixel_draw(msg, false), "closed: {msg}");
            assert!(!chat_wants_canvas_agent(msg, false), "closed: {msg}");
        }
        // Explicit phrase → canvas even when closed; also opens the face.
        for msg in [
            "dessine dans le canvas",
            "dessine sur le canvas",
            "ajoute au trait une porte",
            "/canvas une maison",
        ] {
            assert!(chat_user_wants_explicit_canvas(msg), "explicit: {msg}");
            assert!(
                chat_should_open_canvas_face(msg),
                "explicit opens face: {msg}"
            );
            assert!(
                chat_wants_canvas_agent(msg, false),
                "explicit routes canvas: {msg}"
            );
            assert!(
                !chat_user_wants_pixel_draw(msg, false),
                "explicit not image: {msg}"
            );
        }
        // encore / vas-y → not enough (open or closed).
        for msg in ["encore", "vas-y", "vas y"] {
            assert!(!chat_wants_canvas_agent(msg, true), "follow-up open: {msg}");
            assert!(
                !chat_wants_canvas_agent(msg, false),
                "follow-up closed: {msg}"
            );
            assert!(
                !chat_should_open_canvas_face(msg),
                "follow-up no face: {msg}"
            );
        }
        // Lone « canvas » → not enough.
        for msg in ["canvas", "le canvas", "dessine sur canvas"] {
            assert!(!chat_user_wants_explicit_canvas(msg), "lone canvas: {msg}");
            assert!(
                !chat_should_open_canvas_face(msg),
                "lone canvas no face: {msg}"
            );
        }
    }

    #[test]
    fn author_stroke_color_uses_stored_hex_for_agents() {
        let red = canvas_paint::author_stroke_color("agent-a", "#F40009", true);
        assert_eq!(red, canvas_paint::parse_hex_color("#F40009"));
        let speaker = canvas_paint::author_stroke_color("agent-a", "", true);
        assert_ne!(speaker, canvas_paint::parse_hex_color("#F40009"));
    }

    #[test]
    fn canvas_agent_brief_contains_drawing_guide() {
        let exported: Vec<String> = aos_agent::tools::CANVAS_TOOL_IDS
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let brief = super::canvas_agent_brief(
            "dessine sur le canvas une maison",
            CanvasAspect::Square,
            &exported,
        );
        assert!(brief.starts_with("dessine sur le canvas une maison"));
        assert!(brief.contains("margin"));
        assert!(brief.contains("canvas.get"));
        assert!(brief.contains("canvas.stroke"));
        assert!(brief.contains("canvas.set_style"));
        assert!(brief.contains("export PNG"));
        assert!(brief.contains("scene_bbox"));
        assert!(brief.contains("rectangle+triangle"));
        assert!(brief.contains("jamais canvas.clear"));
        assert!(brief.contains("carré 1:1"));
    }

    #[test]
    fn canvas_agent_brief_non_house_subject_keeps_user_goal_first() {
        let exported: Vec<String> = aos_agent::tools::CANVAS_TOOL_IDS
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let brief = super::canvas_agent_brief(
            "dessine une canette Coca-Cola",
            CanvasAspect::Square,
            &exported,
        );
        assert!(brief.starts_with("dessine une canette Coca-Cola"));
    }

    #[test]
    fn canvas_agent_system_prompt_forbids_spawn_fanout() {
        let exported: Vec<String> = aos_agent::tools::CANVAS_TOOL_IDS
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let prompt = super::canvas_agent_system_prompt(CanvasAspect::Square, &exported);
        assert!(prompt.contains("canvas.set_style"));
        assert!(prompt.contains("Pas agent.spawn"));
        assert!(prompt.contains("auteur unique"));
        assert!(!prompt.contains("agent.spawn : le brief"));
    }

    #[test]
    fn canvas_agent_system_prompt_hides_low_level_fill_and_clear_tools() {
        let exported = vec![
            "canvas.get".into(),
            "canvas.path".into(),
            "canvas.fill".into(),
            "canvas.clear".into(),
        ];
        let prompt = super::canvas_agent_system_prompt(CanvasAspect::Square, &exported);
        assert!(prompt.contains("canvas.path"));
        assert!(!prompt.contains("canvas.fill,"));
    }

    #[test]
    fn canvas_agent_designer_guide_omits_path_when_not_exported() {
        let exported = vec![
            "canvas.stroke".into(),
            "canvas.rect".into(),
            "canvas.get".into(),
        ];
        let guide = super::canvas_agent_designer_guide(&exported);
        assert!(!guide.contains("canvas.path"));
        assert!(guide.contains("canvas.stroke"));
        assert!(guide.contains("spline/rect"));
    }
}
