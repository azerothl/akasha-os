//! Chat session canvas — shared vector drawing (human + agents).

use aos_proto::{CanvasAspect, CanvasOp, CanvasOpBody, CanvasPenStyle, CanvasPoint};
use eframe::egui::epaint::{CircleShape, PathShape, PathStroke, RectShape, Shape, StrokeKind};
use eframe::egui::{Color32, Pos2, Sense, Stroke, Ui, Vec2};

use crate::chat_room;
use crate::i18n::UiStrings;
use crate::theme::{PAPER, SIGNAL, VOID};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasTool {
    Pen,
    Eraser,
    Line,
    Spline,
    Rect,
    Ellipse,
    Fill,
}

#[derive(Debug, Clone)]
pub enum CanvasUiAction {
    Apply(CanvasOpBody),
    SetStyle {
        color: Option<String>,
        width: Option<f32>,
    },
    Export,
    SetAspect(CanvasAspect),
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
        self.color = parse_hex_color(&pen.color);
        self.width = pen.width;
    }
}

pub fn parse_hex_color(s: &str) -> Color32 {
    let t = s.trim().trim_start_matches('#');
    if t.len() >= 6 {
        let r = u8::from_str_radix(&t[0..2], 16).unwrap_or(62);
        let g = u8::from_str_radix(&t[2..4], 16).unwrap_or(224);
        let b = u8::from_str_radix(&t[4..6], 16).unwrap_or(196);
        Color32::from_rgb(r, g, b)
    } else {
        Color32::from_rgb(0x3e, 0xe0, 0xc4)
    }
}

pub fn color_to_hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

fn author_stroke_color(author_id: &str, stored: &str, dark: bool) -> Color32 {
    if stored.trim().is_empty() {
        let (r, g, b) = chat_room::speaker_color_rgb(author_id, dark);
        Color32::from_rgb(r, g, b)
    } else {
        parse_hex_color(stored)
    }
}

fn to_screen(rect: eframe::egui::Rect, p: CanvasPoint) -> Pos2 {
    Pos2::new(
        rect.left() + p.x.clamp(0.0, 1.0) * rect.width(),
        rect.top() + p.y.clamp(0.0, 1.0) * rect.height(),
    )
}

fn to_norm(rect: eframe::egui::Rect, p: Pos2) -> CanvasPoint {
    CanvasPoint {
        x: ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0),
        y: ((p.y - rect.top()) / rect.height()).clamp(0.0, 1.0),
    }
}

fn radius_px(rect: eframe::egui::Rect, width: f32) -> f32 {
    let side = rect.width().min(rect.height());
    (width.clamp(0.001, 0.25) * side * 0.5).max(1.0)
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
    dark: bool,
    progress: f32,
) {
    match &op.body {
        CanvasOpBody::Stroke {
            points,
            color,
            width,
        } => {
            if points.len() < 2 {
                return;
            }
            let n = ((points.len() as f32) * progress).ceil().max(2.0) as usize;
            let slice = &points[..n.min(points.len())];
            let c = author_stroke_color(&op.author_id, color, dark);
            let rad = radius_px(rect, *width);
            let screen: Vec<Pos2> = slice.iter().map(|p| to_screen(rect, *p)).collect();
            painter.add(Shape::line(screen, PathStroke::new(rad * 2.0, c)));
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
                painter.add(Shape::line(screen, PathStroke::new(rad * 2.0, bg)));
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
        } => {
            let c = author_stroke_color(&op.author_id, color, dark);
            let a = to_screen(rect, CanvasPoint { x: *x, y: *y });
            let b = to_screen(
                rect,
                CanvasPoint {
                    x: x + w * progress,
                    y: y + h * progress,
                },
            );
            let r = eframe::egui::Rect::from_two_pos(a, b);
            if *fill {
                painter.add(Shape::Rect(RectShape::filled(r, 0.0, c)));
            } else {
                let rad = radius_px(rect, *width);
                painter.add(Shape::Rect(RectShape::stroke(
                    r,
                    0.0,
                    Stroke::new(rad * 2.0, c),
                    StrokeKind::Inside,
                )));
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
        } => {
            let c = author_stroke_color(&op.author_id, color, dark);
            let cx = x + w * 0.5;
            let cy = y + h * 0.5;
            let center = to_screen(rect, CanvasPoint { x: cx, y: cy });
            let rx = (w.abs() * rect.width() * 0.5 * progress).max(1.0);
            let ry = (h.abs() * rect.height() * 0.5 * progress).max(1.0);
            let ellipse_rect =
                eframe::egui::Rect::from_center_size(center, Vec2::new(rx * 2.0, ry * 2.0));
            if *fill {
                painter.add(Shape::Path(PathShape {
                    points: ellipse_points(ellipse_rect, 48),
                    closed: true,
                    fill: c,
                    stroke: PathStroke::NONE,
                }));
            } else {
                let rad = radius_px(rect, *width);
                painter.add(Shape::Path(PathShape {
                    points: ellipse_points(ellipse_rect, 48),
                    closed: true,
                    fill: Color32::TRANSPARENT,
                    stroke: PathStroke::new(rad * 2.0, c),
                }));
            }
        }
        CanvasOpBody::Line { p0, p1, color, width } => {
            let c = author_stroke_color(&op.author_id, color, dark);
            let rad = radius_px(rect, *width);
            let end = CanvasPoint {
                x: p0.x + (p1.x - p0.x) * progress,
                y: p0.y + (p1.y - p0.y) * progress,
            };
            let screen = [to_screen(rect, *p0), to_screen(rect, end)];
            painter.add(Shape::line(screen.to_vec(), PathStroke::new(rad * 2.0, c)));
        }
        CanvasOpBody::Spline { points, color, width } => {
            if points.len() < 2 {
                return;
            }
            let c = author_stroke_color(&op.author_id, color, dark);
            let rad = radius_px(rect, *width);
            let sampled = sample_spline_points(points, 24);
            let n = ((sampled.len() as f32) * progress).ceil().max(2.0) as usize;
            let slice = &sampled[..n.min(sampled.len())];
            let screen: Vec<Pos2> = slice.iter().map(|p| to_screen(rect, *p)).collect();
            painter.add(Shape::line(screen, PathStroke::new(rad * 2.0, c)));
        }
        CanvasOpBody::Path {
            points,
            color,
            width,
            fill,
            closed,
        } => {
            if points.len() < 2 {
                return;
            }
            let c = author_stroke_color(&op.author_id, color, dark);
            let sampled = sample_spline_points(points, 24);
            let n = ((sampled.len() as f32) * progress).ceil().max(2.0) as usize;
            let slice = &sampled[..n.min(sampled.len())];
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
                    fill: c,
                    stroke: PathStroke::NONE,
                }));
            }
            if *width > 0.0 {
                let rad = radius_px(rect, *width);
                let stroke_pts = if *closed {
                    screen
                } else {
                    slice.iter().map(|p| to_screen(rect, *p)).collect()
                };
                painter.add(Shape::line(stroke_pts, PathStroke::new(rad * 2.0, c)));
            }
        }
        CanvasOpBody::Fill { x, y, color } => {
            let c = author_stroke_color(&op.author_id, color, dark);
            let center = to_screen(rect, CanvasPoint { x: *x, y: *y });
            let r = (rect.width().min(rect.height()) * 0.012 * progress).max(2.0);
            painter.add(Shape::Circle(CircleShape::filled(center, r, c)));
        }
        CanvasOpBody::Clear | CanvasOpBody::Undo => {}
    }
}

fn canvas_bg(dark: bool) -> Color32 {
    if dark { VOID } else { PAPER }
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
        let p3 = if i + 2 < n { points[i + 2] } else { points[i + 1] };
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

/// SIGNAL pastille shown while a vision model reads the live canvas board.
fn ui_canvas_seeing_pill(ui: &mut Ui, label: &str) {
    eframe::egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(SIGNAL.r(), SIGNAL.g(), SIGNAL.b(), 28))
        .stroke(Stroke::new(1.0_f32, SIGNAL))
        .corner_radius(0.0)
        .inner_margin(eframe::egui::Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.label(
                eframe::egui::RichText::new(label)
                    .color(SIGNAL)
                    .size(11.0),
            );
        });
}

/// Drawing tools for the unified session bar (pen, eraser, shapes, tint, thickness).
pub fn toolbar_content_min_width(t: &UiStrings, seeing: bool, clear_confirm: bool) -> f32 {
    const CHAR_W: f32 = 8.5;
    const BTN_PAD: f32 = 18.0;
    const GAP: f32 = 4.0;
    let mut w = 0.0;
    if seeing {
        w += t.canvas_seeing_now.len() as f32 * CHAR_W + 24.0 + GAP;
    }
    for label in [
        t.canvas_tool_pen,
        t.canvas_tool_eraser,
        t.canvas_tool_line,
        t.canvas_tool_spline,
        t.canvas_tool_rect,
        t.canvas_tool_ellipse,
        t.canvas_tool_fill,
    ] {
        w += label.len() as f32 * CHAR_W + BTN_PAD + GAP;
    }
    w += t.canvas_fill_toggle.len() as f32 * CHAR_W + BTN_PAD + GAP;
    w += t.canvas_tint.len() as f32 * CHAR_W + 28.0 + GAP;
    w += 88.0; // color picker + width slider
    w += t.canvas_undo.len() as f32 * CHAR_W + BTN_PAD + GAP;
    w += t.canvas_export.len() as f32 * CHAR_W + BTN_PAD + GAP;
    if clear_confirm {
        w += t.canvas_clear_confirm.len() as f32 * CHAR_W
            + t.canvas_clear_confirm_yes.len() as f32 * CHAR_W
            + t.canvas_clear_confirm_no.len() as f32 * CHAR_W
            + BTN_PAD * 2.0
            + GAP * 2.0;
    } else {
        w += t.canvas_clear.len() as f32 * CHAR_W + BTN_PAD;
    }
    w
}

/// Drawing tools for the unified session bar (pen, eraser, shapes, tint, thickness).
pub fn ui_canvas_toolbar(
    ui: &mut Ui,
    t: &UiStrings,
    state: &mut CanvasPanelState,
    help_tooltip: Option<&str>,
    help_clicked: &mut bool,
) -> Option<CanvasUiAction> {
    let mut action: Option<CanvasUiAction> = None;
    ui.horizontal(|ui| {
        let compact = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing = eframe::egui::vec2(4.0, compact.y);

        if state.seeing {
            ui_canvas_seeing_pill(ui, t.canvas_seeing_now);
        }

        ui.selectable_value(&mut state.tool, CanvasTool::Pen, t.canvas_tool_pen);
        ui.selectable_value(&mut state.tool, CanvasTool::Eraser, t.canvas_tool_eraser);
        ui.selectable_value(&mut state.tool, CanvasTool::Line, t.canvas_tool_line);
        ui.selectable_value(&mut state.tool, CanvasTool::Spline, t.canvas_tool_spline);
        ui.selectable_value(&mut state.tool, CanvasTool::Rect, t.canvas_tool_rect);
        ui.selectable_value(&mut state.tool, CanvasTool::Ellipse, t.canvas_tool_ellipse);
        ui.selectable_value(&mut state.tool, CanvasTool::Fill, t.canvas_tool_fill);

        if matches!(state.tool, CanvasTool::Rect | CanvasTool::Ellipse) {
            let fill_label = eframe::egui::RichText::new(t.canvas_fill_toggle).weak();
            ui.toggle_value(&mut state.shape_fill, fill_label);
        }

        ui.label(t.canvas_tint);
        let mut rgba = [
            state.color.r() as f32 / 255.0,
            state.color.g() as f32 / 255.0,
            state.color.b() as f32 / 255.0,
            1.0,
        ];
        if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
            state.color = Color32::from_rgb(
                (rgba[0] * 255.0) as u8,
                (rgba[1] * 255.0) as u8,
                (rgba[2] * 255.0) as u8,
            );
            action = Some(CanvasUiAction::SetStyle {
                color: Some(color_to_hex(state.color)),
                width: None,
            });
        }
        let width_resp = ui.add(
            eframe::egui::Slider::new(&mut state.width, 0.005..=0.06).text(t.canvas_width),
        );
        if width_resp.changed() {
            action = Some(CanvasUiAction::SetStyle {
                color: None,
                width: Some(state.width),
            });
        }

        if ui
            .button(eframe::egui::RichText::new(t.canvas_undo).weak())
            .clicked()
        {
            action = Some(CanvasUiAction::Apply(CanvasOpBody::Undo));
        }
        if ui
            .button(eframe::egui::RichText::new(t.canvas_export).weak())
            .clicked()
        {
            action = Some(CanvasUiAction::Export);
        }

        if state.clear_confirm_open {
            ui.label(eframe::egui::RichText::new(t.canvas_clear_confirm).small());
            if ui
                .button(eframe::egui::RichText::new(t.canvas_clear_confirm_yes).color(crate::theme::HYDROGEN))
                .clicked()
            {
                state.clear_confirm_open = false;
                action = Some(CanvasUiAction::Apply(CanvasOpBody::Clear));
            }
            if ui.button(t.canvas_clear_confirm_no).clicked() {
                state.clear_confirm_open = false;
            }
        } else if ui
            .button(eframe::egui::RichText::new(t.canvas_clear).color(crate::theme::HYDROGEN))
            .clicked()
        {
            state.clear_confirm_open = true;
        }

        if let Some(tip) = help_tooltip {
            if crate::guide::tab_help_button(ui, tip) {
                *help_clicked = true;
            }
        }
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
    let rect = fit_board_rect(outer, aspect);
    let bg = canvas_bg(dark);
    painter.rect_filled(rect, 0.0, bg);
    painter.rect_stroke(rect, 0.0, Stroke::new(1.5_f32, SIGNAL), StrokeKind::Inside);

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
        let p = anim_progress(state, op.seq, now);
        paint_op(&painter, rect, op, dark, p);
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
        let rad = radius_px(rect, state.width);
        match state.tool {
            CanvasTool::Spline if screen.len() >= 2 => {
                let sampled: Vec<Pos2> = sample_spline_points(&state.draft_points, 16)
                    .iter()
                    .map(|p| to_screen(rect, *p))
                    .collect();
                painter.add(Shape::line(sampled, PathStroke::new(rad * 2.0, c)));
            }
            _ => {
                painter.add(Shape::line(screen, PathStroke::new(rad * 2.0, c)));
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
                    painter.rect_stroke(r, 0.0, Stroke::new(2.0_f32, state.color), StrokeKind::Inside);
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
            | CanvasTool::Fill => {}
        }
    }

    if response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            if !rect.contains(pos) {
                return action;
            }
            let p = to_norm(rect, pos);
            match state.tool {
                CanvasTool::Pen | CanvasTool::Eraser | CanvasTool::Spline => {
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
                CanvasTool::Fill => {}
            }
        }
        ui.ctx().request_repaint();
    }

    if response.clicked() && state.tool == CanvasTool::Fill {
        if let Some(pos) = response.interact_pointer_pos() {
            if rect.contains(pos) {
                let p = to_norm(rect, pos);
                action = Some(CanvasUiAction::Apply(CanvasOpBody::Fill {
                    x: p.x,
                    y: p.y,
                    color: color_to_hex(state.color),
                }));
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
            CanvasTool::Pen => {
                if state.draft_points.len() >= 2 {
                    action = Some(CanvasUiAction::Apply(CanvasOpBody::Stroke {
                        points: std::mem::take(&mut state.draft_points),
                        color: color_to_hex(state.color),
                        width: state.width,
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
                    action = Some(CanvasUiAction::Apply(CanvasOpBody::Spline {
                        points: std::mem::take(&mut state.draft_points),
                        color: color_to_hex(state.color),
                        width: state.width,
                    }));
                } else {
                    state.draft_points.clear();
                }
            }
            CanvasTool::Line => {
                if let (Some(a), Some(b)) = (state.drag_origin.take(), state.drag_current.take()) {
                    action = Some(CanvasUiAction::Apply(CanvasOpBody::Line {
                        p0: a,
                        p1: b,
                        color: color_to_hex(state.color),
                        width: state.width,
                    }));
                }
            }
            CanvasTool::Rect => {
                if let (Some(a), Some(b)) = (state.drag_origin.take(), state.drag_current.take()) {
                    let x = a.x.min(b.x);
                    let y = a.y.min(b.y);
                    let w = (a.x - b.x).abs().max(0.01);
                    let h = (a.y - b.y).abs().max(0.01);
                    action = Some(CanvasUiAction::Apply(CanvasOpBody::Rect {
                        x,
                        y,
                        w,
                        h,
                        color: color_to_hex(state.color),
                        fill: state.shape_fill,
                        width: state.width,
                    }));
                }
            }
            CanvasTool::Ellipse => {
                if let (Some(a), Some(b)) = (state.drag_origin.take(), state.drag_current.take()) {
                    let x = a.x.min(b.x);
                    let y = a.y.min(b.y);
                    let w = (a.x - b.x).abs().max(0.01);
                    let h = (a.y - b.y).abs().max(0.01);
                    action = Some(CanvasUiAction::Apply(CanvasOpBody::Ellipse {
                        x,
                        y,
                        w,
                        h,
                        color: color_to_hex(state.color),
                        fill: state.shape_fill,
                        width: state.width,
                    }));
                }
            }
            CanvasTool::Fill => {}
        }
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
        pos.x = pos
            .x
            .clamp(rect.left() + 8.0, rect.right() - galley.size().x - 8.0);
        pos.y = pos
            .y
            .clamp(rect.top() + 8.0, rect.bottom() - galley.size().y - 8.0);
        ui.painter().galley(pos, galley, Color32::TRANSPARENT);
    }

    action
}

#[cfg(test)]
mod routing_tests {
    use super::*;
    use crate::i18n;

    #[test]
    fn toolbar_min_width_covers_fr_clear_label() {
        let fr = i18n::strings("fr");
        let w = toolbar_content_min_width(&fr, false, false);
        assert!(w > 700.0, "toolbar scroll extent should cover FR labels, got {w}");
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
        for msg in [
            "encore",
            "vas-y",
            "vas y",
            "lance",
            "améliore",
            "go ahead",
        ] {
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
            body: CanvasOpBody::Line {
                p0: CanvasPoint { x: 0.2, y: 0.2 },
                p1: CanvasPoint { x: 0.8, y: 0.2 },
                color: "#f40009".into(),
                width: 0.02,
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
            body: CanvasOpBody::Stroke {
                points: vec![CanvasPoint { x: 0.1, y: 0.1 }, CanvasPoint { x: 0.2, y: 0.2 }],
                color: "#3ee0c4".into(),
                width: 0.01,
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
        assert_eq!(state.ops.len(), 8, "late empty poll must not wipe agent strokes");
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
            assert!(chat_should_open_canvas_face(msg), "explicit opens face: {msg}");
            assert!(chat_wants_canvas_agent(msg, false), "explicit routes canvas: {msg}");
            assert!(!chat_user_wants_pixel_draw(msg, false), "explicit not image: {msg}");
        }
        // encore / vas-y → not enough (open or closed).
        for msg in ["encore", "vas-y", "vas y"] {
            assert!(!chat_wants_canvas_agent(msg, true), "follow-up open: {msg}");
            assert!(!chat_wants_canvas_agent(msg, false), "follow-up closed: {msg}");
            assert!(!chat_should_open_canvas_face(msg), "follow-up no face: {msg}");
        }
        // Lone « canvas » → not enough.
        for msg in ["canvas", "le canvas", "dessine sur canvas"] {
            assert!(!chat_user_wants_explicit_canvas(msg), "lone canvas: {msg}");
            assert!(!chat_should_open_canvas_face(msg), "lone canvas no face: {msg}");
        }
    }

    #[test]
    fn author_stroke_color_uses_stored_hex_for_agents() {
        let red = author_stroke_color("agent-a", "#F40009", true);
        assert_eq!(red, parse_hex_color("#F40009"));
        let speaker = author_stroke_color("agent-a", "", true);
        assert_ne!(speaker, parse_hex_color("#F40009"));
    }

    #[test]
    fn canvas_agent_brief_contains_drawing_guide() {
        let brief = super::canvas_agent_brief("dessine sur le canvas une maison", CanvasAspect::Square);
        assert!(brief.starts_with("dessine sur le canvas une maison"));
        assert!(brief.contains("margin"));
        assert!(brief.contains("canvas.get"));
        assert!(brief.contains("canvas.stroke"));
        assert!(brief.contains("canvas.set_style"));
        assert!(brief.contains("export PNG"));
        assert!(brief.contains("scene_bbox"));
        assert!(brief.contains("rectangle+triangle"));
        assert!(brief.contains("Exemple si le sujet est une maison"));
        assert!(brief.contains("toit + murs + porte"));
        assert!(!brief.starts_with("Exemple si le sujet est une maison"));
        assert!(brief.contains("jamais canvas.clear"));
        assert!(brief.contains("carré 1:1"));
    }

    #[test]
    fn canvas_agent_brief_non_house_subject_keeps_user_goal_first() {
        let brief =
            super::canvas_agent_brief("dessine une canette Coca-Cola", CanvasAspect::Square);
        assert!(brief.starts_with("dessine une canette Coca-Cola"));
        assert!(brief.contains("Exemple si le sujet est une maison"));
    }

    #[test]
    fn canvas_agent_system_prompt_forbids_spawn_fanout() {
        let prompt = super::canvas_agent_system_prompt(CanvasAspect::Square);
        assert!(prompt.contains("canvas.set_style"));
        assert!(prompt.contains("Exemple si le sujet est une maison"));
        assert!(prompt.contains("Pas agent.spawn"));
        assert!(prompt.contains("auteur unique"));
        assert!(!prompt.contains("agent.spawn : le brief"));
    }
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
    .any(|k| word_boundary_match(&lower, *k))
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


/// Frozen designer copy for delegated canvas agents (system prompt — not the user goal).
pub const CANVAS_AGENT_DESIGNER_GUIDE: &str = "\
Cible visuelle : lisible à l'export PNG (canvas.export ~512px) — pas un rectangle+triangle.\n\
Espace : coords normalisées 0..1 uniquement (max 1.0) sur le cadre visible (origine coin supérieur gauche) — jamais des pixels.\n\
« 200px » = taille d'export pour la lisibilité humaine, pas l'unité des coords (ne pas dessiner à x=200).\n\
Règles : margin 0.08–0.12 ; sujet centré dans usable ; couches sol → volumes → détails → 2–3 ombres. \
Lis `scene_bbox` dans le digest : ne superpose pas les nouvelles formes au même centre. \
Silhouettes (colline, corps, toit, voiles) : `canvas.path` avec 4–8 points et fill:true — \
un path par forme lisible, pas 20 splines/rects empilés. Traits fins : canvas.stroke/line/spline. \
Commence par canvas.get. Couleur : canvas.set_style {color:\"#RRGGBB\"} ou color= sur chaque op — \
le teal signal n'est pas la seule teinte ; après critique, change de teinte pour ombres/détails.\n\
Après critique : ajoute, jamais canvas.clear sauf si l'humain dit effacer.\n\
Exemple moulin sur colline : path colline (#8B7355) → path corps (#C4A574) → path toit (#8B4513) → \
2–4 paths pour les ailes (#E8DCC8) ; rect/ellipse seulement pour détails (fenêtre, porte).\n\
Outils : canvas.set_style, canvas.path, canvas.stroke, canvas.line, canvas.spline, canvas.rect, canvas.ellipse \
(fill:true sur rect/ellipse pour remplir), canvas.erase, canvas.clear, \
canvas.undo, canvas.get, canvas.export (coords 0..1). Pas media.image.generate. Pas agent.spawn.";

const CANVAS_AGENT_NO_FANOUT_GUIDE: &str = "\
Tu es l'auteur unique de ce dessin : traits séquentiels canvas.* (path pour silhouettes, stroke/rect/ellipse pour détails). \
Pas de agent.spawn ni agent.await — un seul agent, pas de sous-agents parallèles pour le même sujet.";

/// System prompt addendum for delegated canvas agents (designer rules + frame aspect).
pub fn canvas_agent_system_prompt(aspect: CanvasAspect) -> String {
    format!(
        "{CANVAS_AGENT_DESIGNER_GUIDE}\n\
         Proportions actuelles du cadre : {aspect_fr} ({aspect_en}).\n\n\
         {CANVAS_AGENT_NO_FANOUT_GUIDE}",
        aspect_fr = aspect.agent_label_fr(),
        aspect_en = aspect.agent_label_en(),
    )
}

/// Full brief for display / logs: line 1 = user request verbatim, then designer guide.
pub fn canvas_agent_brief(user_text: &str, aspect: CanvasAspect) -> String {
    format!(
        "{}\n\n{}\nProportions actuelles du cadre : {} ({}).",
        user_text.trim(),
        CANVAS_AGENT_DESIGNER_GUIDE,
        aspect.agent_label_fr(),
        aspect.agent_label_en(),
    )
}
