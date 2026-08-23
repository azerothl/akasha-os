//! Chat session canvas — shared vector drawing (human + agents).

use aos_proto::{CanvasOp, CanvasOpBody, CanvasPoint};
use eframe::egui::epaint::{CircleShape, PathShape, PathStroke, RectShape, Shape, StrokeKind};
use eframe::egui::{Color32, Pos2, Sense, Stroke, Ui, Vec2};

use crate::chat_room;
use crate::i18n::UiStrings;
use crate::theme::{PAPER, SIGNAL, VOID};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasTool {
    Pen,
    Eraser,
    Rect,
    Ellipse,
}

#[derive(Debug, Clone)]
pub enum CanvasUiAction {
    Apply(CanvasOpBody),
    Export,
}

#[derive(Debug, Clone)]
pub struct CanvasPanelState {
    pub ops: Vec<CanvasOp>,
    pub next_seq: u64,
    pub last_seen_seq: u64,
    pub tool: CanvasTool,
    pub color: Color32,
    pub width: f32,
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
            draft_points: Vec::new(),
            drag_origin: None,
            drag_current: None,
            animating: Vec::new(),
            poll_due: 0.0,
            clear_confirm_open: false,
        }
    }
}

impl CanvasPanelState {
    pub fn apply_snapshot(&mut self, ops: Vec<CanvasOp>, next_seq: u64, now: f64) {
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
    if author_id == "human" {
        parse_hex_color(stored)
    } else {
        let (r, g, b) = chat_room::speaker_color_rgb(author_id, dark);
        Color32::from_rgb(r, g, b)
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

/// Drawing tools for the unified session bar (pen, eraser, shapes, tint, thickness).
pub fn ui_canvas_toolbar(
    ui: &mut Ui,
    t: &UiStrings,
    state: &mut CanvasPanelState,
) -> Option<CanvasUiAction> {
    let mut action: Option<CanvasUiAction> = None;

    ui.selectable_value(&mut state.tool, CanvasTool::Pen, t.canvas_tool_pen);
    ui.selectable_value(&mut state.tool, CanvasTool::Eraser, t.canvas_tool_eraser);
    ui.selectable_value(&mut state.tool, CanvasTool::Rect, t.canvas_tool_rect);
    ui.selectable_value(&mut state.tool, CanvasTool::Ellipse, t.canvas_tool_ellipse);
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
    }
    ui.add(
        eframe::egui::Slider::new(&mut state.width, 0.005..=0.06).text(t.canvas_width),
    );

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

    action
}

/// Drawing surface — fills available height in the split pane.
pub fn ui_canvas_surface(
    ui: &mut Ui,
    state: &mut CanvasPanelState,
    empty_hint: &str,
) -> Option<CanvasUiAction> {
    let mut action: Option<CanvasUiAction> = None;
    let dark = ui.visuals().dark_mode;
    let avail = ui.available_size();
    let canvas_h = avail.y.max(120.0);
    let canvas_w = avail.x.max(180.0);
    let (response, painter) =
        ui.allocate_painter(Vec2::new(canvas_w, canvas_h), Sense::click_and_drag());
    let rect = response.rect;
    let bg = canvas_bg(dark);
    painter.rect_filled(rect, 0.0, bg);
    painter.rect_stroke(rect, 0.0, Stroke::new(1.5, SIGNAL), StrokeKind::Inside);

    let now = ui.ctx().input(|i| i.time);
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
        painter.add(Shape::line(screen, PathStroke::new(rad * 2.0, c)));
    }
    if let (Some(a), Some(b)) = (state.drag_origin, state.drag_current) {
        let r = eframe::egui::Rect::from_two_pos(to_screen(rect, a), to_screen(rect, b));
        match state.tool {
            CanvasTool::Rect => {
                painter.rect_stroke(r, 0.0, Stroke::new(2.0, state.color), StrokeKind::Inside);
            }
            CanvasTool::Ellipse => {
                painter.add(Shape::Path(PathShape {
                    points: ellipse_points(r, 48),
                    closed: true,
                    fill: Color32::TRANSPARENT,
                    stroke: PathStroke::new(2.0, state.color),
                }));
            }
            CanvasTool::Pen | CanvasTool::Eraser => {}
        }
    }

    if response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let p = to_norm(rect, pos);
            match state.tool {
                CanvasTool::Pen | CanvasTool::Eraser => {
                    if state
                        .draft_points
                        .last()
                        .map(|q| (q.x - p.x).abs() + (q.y - p.y).abs() > 0.002)
                        .unwrap_or(true)
                    {
                        state.draft_points.push(p);
                    }
                }
                CanvasTool::Rect | CanvasTool::Ellipse => {
                    if state.drag_origin.is_none() {
                        state.drag_origin = Some(p);
                    }
                    state.drag_current = Some(p);
                }
            }
        }
        ui.ctx().request_repaint();
    }

    if response.drag_stopped() && action.is_none() {
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
                        fill: false,
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
                        fill: false,
                        width: state.width,
                    }));
                }
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
        let galley = ui
            .painter()
            .layout_no_wrap(empty_hint.to_string(), font, ui.visuals().weak_text_color());
        let pos = rect.center() - galley.size() * 0.5;
        ui.painter().galley(pos, galley, Color32::TRANSPARENT);
    }

    action
}

#[cfg(test)]
mod routing_tests {
    use super::*;

    #[test]
    fn bare_draw_is_pixel_not_canvas() {
        assert!(chat_user_wants_pixel_draw("dessine une maison"));
        assert!(!chat_user_wants_explicit_canvas("dessine une maison"));
        assert!(!chat_wants_canvas_agent("dessine une maison", true));
    }

    #[test]
    fn explicit_canvas_markers() {
        assert!(chat_user_wants_explicit_canvas("dessine sur le canvas"));
        assert!(chat_user_wants_explicit_canvas("draw on the canvas"));
        assert!(chat_user_wants_explicit_canvas("ajoute au trait une porte"));
        assert!(chat_user_wants_explicit_canvas("/canvas"));
        assert!(!chat_user_wants_pixel_draw("dessine sur le canvas"));
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
        assert!(!chat_user_wants_pixel_draw("withdraw funds"));
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
}

/// Explicit vector-canvas intent: toggle phrase, slash, or stroke wording — not bare « dessine ».
pub fn chat_user_wants_explicit_canvas(text: &str) -> bool {
    let lower = text.to_lowercase();
    if lower.contains("/canvas") {
        return true;
    }
    if lower.contains("sur le canvas") || lower.contains("on the canvas") {
        return true;
    }
    // « au trait » = vector strokes on the session canvas (not pixel diffusion).
    if lower.contains("au trait") {
        return true;
    }
    false
}

fn word_boundary_match(lower: &str, pat: &str) -> bool {
    lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word == pat)
}

/// Bare draw / sketch wording → Image Studio (pixels), unless explicit canvas markers win.
pub fn chat_user_wants_pixel_draw(text: &str) -> bool {
    if chat_user_wants_explicit_canvas(text) {
        return false;
    }
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

/// Déléguer un agent canvas : uniquement sur marqueurs explicites (pas « encore » / « vas-y »).
pub fn chat_wants_canvas_agent(text: &str, _canvas_open: bool) -> bool {
    chat_user_wants_explicit_canvas(text)
}

pub fn canvas_agent_brief(user_text: &str) -> String {
    format!(
        "{user_text}\n\n\
         Contexte: canvas vectoriel de session lié — utilise uniquement \
         canvas.stroke, canvas.rect, canvas.ellipse, canvas.erase, canvas.clear, \
         canvas.undo, canvas.get, canvas.export (coords normalisées 0..1). \
         Commence par canvas.get. Si le dessin est trop pauvre: canvas.clear puis \
         redessine avec plus de traits et formes. Ne génère pas d'image diffusion \
         (pas media.image.generate). Ne pose pas user.ask pour choisir entre canvas \
         et diffusion — dessine directement."
    )
}
