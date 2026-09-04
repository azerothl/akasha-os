//! Painted chrome icons — no emoji or special font glyphs.

use eframe::egui::{
    self, Color32, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, Widget,
};

const BTN: f32 = 18.0;

/// Session-bar icon-only control size (activity toggle, etc.).
pub const SESSION_ICON_SZ: f32 = BTN;

/// Fixed slot for activity-list status glyphs (Done / Failed / …).
pub const AGENT_STATUS_ICON_SZ: f32 = 14.0;

/// Width reserved for the chat attach control (icon + spacing).
pub const ATTACH_BTN_W: f32 = BTN + 4.0;

fn hover_color(ui: &Ui, response: &Response) -> Color32 {
    if response.hovered() {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().weak_text_color()
    }
}

/// Small square close control (replaces `×`).
pub fn close_button(ui: &mut Ui) -> Response {
    let size = Vec2::splat(BTN);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let c = rect.center();
        let r = rect.width() * 0.22;
        let stroke = Stroke::new(1.5_f32, hover_color(ui, &response));
        let painter = ui.painter();
        painter.line_segment([c + Vec2::new(-r, -r), c + Vec2::new(r, r)], stroke);
        painter.line_segment([c + Vec2::new(-r, r), c + Vec2::new(r, -r)], stroke);
    }
    response
}

/// Painted help control (circle + question mark) for tab/session headers.
pub fn help_button(ui: &mut Ui) -> Response {
    let size = Vec2::splat(BTN);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        paint_help(ui, rect, hover_color(ui, &response));
    }
    response
}

fn paint_help(ui: &mut Ui, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.4_f32, color);
    let painter = ui.painter();
    let c = rect.center();
    let r = rect.width() * 0.38;
    painter.circle_stroke(c, r, stroke);
    // Hook of the question mark
    let hook_c = c + Vec2::new(0.0, -r * 0.22);
    painter.circle_stroke(hook_c, r * 0.28, stroke);
    // Stem
    painter.line_segment(
        [c + Vec2::new(0.0, r * 0.02), c + Vec2::new(0.0, r * 0.42)],
        stroke,
    );
    // Dot
    painter.circle_filled(c + Vec2::new(0.0, r * 0.62), r * 0.11, color);
}

/// Paperclip attach control for composer / studio menus.
pub struct AttachIcon;

/// Attach menu opener (paperclip icon + popup).
pub fn attach_menu<R>(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash,
    hover: &str,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let popup_id = ui.id().with(id_salt);
    let btn = ui.add(AttachIcon).on_hover_text(hover);
    if btn.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(popup_id));
    }
    let mut out = None;
    egui::popup::popup_below_widget(
        ui,
        popup_id,
        &btn,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            out = Some(add_contents(ui));
        },
    );
    out
}

impl Widget for AttachIcon {
    fn ui(self, ui: &mut Ui) -> Response {
        let size = Vec2::splat(BTN);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        if ui.is_rect_visible(rect) {
            paint_paperclip(ui, rect, hover_color(ui, &response));
        }
        response
    }
}

fn paint_paperclip(ui: &mut Ui, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.4_f32, color);
    let painter = ui.painter();
    let w = rect.width();
    let h = rect.height();
    let left = rect.left() + w * 0.30;
    let top = rect.top() + h * 0.18;
    let bottom = rect.bottom() - h * 0.18;
    let right = rect.right() - w * 0.28;
    let arc_r = w * 0.14;
    painter.add(Shape::line_segment(
        [Pos2::new(left, bottom), Pos2::new(left, top + arc_r)],
        stroke,
    ));
    painter.add(Shape::circle_stroke(
        Pos2::new(left + arc_r, top + arc_r),
        arc_r,
        stroke,
    ));
    painter.add(Shape::line_segment(
        [Pos2::new(left + arc_r * 2.0, top), Pos2::new(right, top)],
        stroke,
    ));
    painter.add(Shape::line_segment(
        [Pos2::new(right, top), Pos2::new(right, bottom - arc_r)],
        stroke,
    ));
    painter.add(Shape::circle_stroke(
        Pos2::new(right - arc_r, bottom - arc_r),
        arc_r,
        stroke,
    ));
}

/// Pinned-memory indicator (replaces `★`).
pub fn pin_indicator(ui: &mut Ui) {
    let size = Vec2::new(14.0, 14.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let c = rect.center();
    let r = rect.width() * 0.34;
    let color = ui.visuals().weak_text_color();
    let stroke = Stroke::new(1.2_f32, color);
    let mut points = Vec::with_capacity(10);
    for i in 0..5 {
        let outer = std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / 5.0;
        let inner = outer + std::f32::consts::PI / 5.0;
        points.push(c + Vec2::angled(outer) * r);
        points.push(c + Vec2::angled(inner) * (r * 0.42));
    }
    ui.painter().add(Shape::closed_line(points, stroke));
}

/// Expand/collapse caret (replaces `▸` / `▾`).
pub fn caret(ui: &mut Ui, expanded: bool) -> Response {
    let size = Vec2::new(12.0, 12.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let c = rect.center();
        let s = rect.width() * 0.22;
        let color = ui.visuals().weak_text_color();
        let stroke = Stroke::new(1.4_f32, color);
        let tri = if expanded {
            vec![
                c + Vec2::new(-s, -s * 0.35),
                c + Vec2::new(s, -s * 0.35),
                c + Vec2::new(0.0, s * 0.85),
            ]
        } else {
            vec![
                c + Vec2::new(-s * 0.35, -s),
                c + Vec2::new(-s * 0.35, s),
                c + Vec2::new(s * 0.85, 0.0),
            ]
        };
        ui.painter().add(Shape::convex_polygon(tri, Color32::TRANSPARENT, stroke));
    }
    response
}

/// Status dot prefix (replaces `●`).
pub fn status_dot(ui: &mut Ui, color: Color32) -> Response {
    let size = Vec2::splat(10.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter()
            .circle_filled(rect.center(), rect.width() * 0.32, color);
    }
    response
}

/// Child-agent tree branch marker (replaces `↳`).
pub fn child_branch(ui: &mut Ui) {
    let size = Vec2::new(12.0, 12.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let stroke = Stroke::new(1.2_f32, ui.visuals().weak_text_color());
    let painter = ui.painter();
    let mid_y = rect.center().y;
    painter.line_segment(
        [Pos2::new(rect.left(), mid_y), Pos2::new(rect.center().x, mid_y)],
        stroke,
    );
    painter.line_segment(
        [
            Pos2::new(rect.center().x, rect.top() + 1.0),
            Pos2::new(rect.center().x, rect.bottom() - 1.0),
        ],
        stroke,
    );
    painter.line_segment(
        [Pos2::new(rect.center().x, mid_y), Pos2::new(rect.right(), mid_y)],
        stroke,
    );
}

/// External-link arrow prefix (replaces `↗`).
pub fn external_arrow(ui: &mut Ui) {
    let size = Vec2::new(12.0, 12.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let color = ui.visuals().weak_text_color();
    let stroke = Stroke::new(1.2_f32, color);
    let painter = ui.painter();
    let bl = Pos2::new(rect.left() + 1.0, rect.bottom() - 1.0);
    let tr = Pos2::new(rect.right() - 1.0, rect.top() + 1.0);
    painter.line_segment([bl, tr], stroke);
    painter.line_segment([tr, Pos2::new(tr.x - 4.0, tr.y)], stroke);
    painter.line_segment([tr, Pos2::new(tr.x, tr.y + 4.0)], stroke);
}

/// Done-task check (replaces `✓`).
pub fn done_check(ui: &mut Ui) {
    let size = Vec2::splat(AGENT_STATUS_ICON_SZ);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    paint_done_check(ui, rect, ui.visuals().weak_text_color());
}

/// Close-all control for the activity detail header (painted × + label, no Unicode).
pub fn close_all_button(ui: &mut Ui, label: &str) -> Response {
    let text_w = ui.fonts(|f| {
        f.layout_no_wrap(
            label.to_owned(),
            egui::FontId::proportional(12.0),
            Color32::PLACEHOLDER,
        )
        .size()
        .x
    });
    let size = Vec2::new(BTN + text_w + 6.0, BTN);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let icon_rect = Rect::from_min_size(rect.min, Vec2::splat(BTN));
        paint_close_mark(ui, icon_rect, hover_color(ui, &response));
        ui.painter().text(
            Pos2::new(icon_rect.right() + 2.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(12.0),
            hover_color(ui, &response),
        );
    }
    response
}

/// Archived-session filter toggle beside the session search field.
pub fn archived_toggle_button(ui: &mut Ui, selected: bool) -> Response {
    let size = Vec2::splat(BTN);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let color = if selected {
            ui.visuals().strong_text_color()
        } else {
            hover_color(ui, &response)
        };
        paint_archived_filter(ui, rect, color, selected);
    }
    response
}

/// Activity panel toggle for the session bar (icon-only when canvas is open).
pub fn activity_toggle_button(ui: &mut Ui, open: bool) -> Response {
    let size = Vec2::splat(BTN);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        if open {
            ui.painter().rect_filled(
                rect,
                3.0,
                ui.visuals().selection.bg_fill,
            );
        } else if response.hovered() {
            ui.painter().rect_filled(
                rect,
                3.0,
                ui.visuals().widgets.hovered.bg_fill,
            );
        }
        let color = if open {
            ui.visuals().strong_text_color()
        } else {
            hover_color(ui, &response)
        };
        paint_activity_list(ui, rect, color);
    }
    response
}

/// Outgoing note link (replaces `->`).
pub fn link_outgoing(ui: &mut Ui) {
    paint_link_arrow(ui, true);
}

/// Backlink / incoming note link (replaces `<-`).
pub fn link_backlink(ui: &mut Ui) {
    paint_link_arrow(ui, false);
}

/// Broken / unresolved note link — straight cut stroke with a clean gap.
pub fn link_broken(ui: &mut Ui) {
    let size = Vec2::new(12.0, 12.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let color = ui.visuals().weak_text_color();
    let stroke = Stroke::new(1.3_f32, color);
    let painter = ui.painter();
    let y = rect.center().y;
    let gap_half = rect.width() * 0.14;
    let cx = rect.center().x;
    painter.line_segment(
        [Pos2::new(rect.left() + 1.0, y), Pos2::new(cx - gap_half, y)],
        stroke,
    );
    painter.line_segment(
        [Pos2::new(cx + gap_half, y), Pos2::new(rect.right() - 1.0, y)],
        stroke,
    );
}

/// Canvas toolbar hit target (matches `chat_canvas::TOOLBAR_CTRL_H`).
pub const TOOLBAR_ICON_SZ: f32 = 20.0;

/// Drawing-tool glyphs for the session canvas toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasToolIcon {
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

/// Non-tool toolbar actions (export, view, z-order, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarActionIcon {
    ArrowDown,
    ArrowUp,
    Undo,
    ResetView,
    Grid,
    Snap,
    Dashed,
    Clear,
    ConfirmYes,
    ConfirmNo,
}

/// Activity / agent-detail leading status glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentActivityIcon {
    Done,
    Failed,
    Blocked,
    Running,
    Pending,
}

/// Icon-only selectable toolbar control (painted glyph or ASCII fallback).
pub fn toolbar_selectable(
    ui: &mut Ui,
    selected: bool,
    tool: CanvasToolIcon,
    tooltip: &str,
) -> bool {
    toolbar_selectable_inner(ui, selected, ToolbarSlot::CanvasTool(tool), tooltip)
}

/// Icon-only toolbar button with a painted action glyph.
pub fn toolbar_action_button(ui: &mut Ui, icon: ToolbarActionIcon, tooltip: &str) -> bool {
    toolbar_button_inner(ui, ToolbarSlot::Action(icon), tooltip)
}

/// Icon-only selectable toolbar control with a painted action glyph.
pub fn toolbar_action_selectable(
    ui: &mut Ui,
    selected: bool,
    icon: ToolbarActionIcon,
    tooltip: &str,
) -> bool {
    toolbar_selectable_inner(ui, selected, ToolbarSlot::Action(icon), tooltip)
}

/// Icon-only toolbar button with an ASCII label (`F`, `P`, align text, …).
pub fn toolbar_text_button(ui: &mut Ui, label: &str, tooltip: &str) -> bool {
    toolbar_button_inner(ui, ToolbarSlot::Ascii(label), tooltip)
}

/// Icon-only selectable toolbar control with an ASCII label (`#`, `G`, `~`, …).
pub fn toolbar_text_selectable(ui: &mut Ui, selected: bool, label: &str, tooltip: &str) -> bool {
    toolbar_selectable_inner(ui, selected, ToolbarSlot::Ascii(label), tooltip)
}

/// Leading status icon for activity / agent lists.
pub fn agent_activity_icon(ui: &mut Ui, icon: AgentActivityIcon) {
    let size = Vec2::splat(AGENT_STATUS_ICON_SZ);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    match icon {
        AgentActivityIcon::Done => paint_done_check(ui, rect, ui.visuals().weak_text_color()),
        AgentActivityIcon::Failed => {
            paint_failed_mark(ui, rect, crate::theme::HYDROGEN)
        }
        AgentActivityIcon::Blocked => paint_help(ui, rect, ui.visuals().weak_text_color()),
        AgentActivityIcon::Running => paint_running_dots(ui, rect, crate::theme::SIGNAL),
        AgentActivityIcon::Pending => {
            ui.painter().circle_filled(
                rect.center(),
                rect.width() * 0.22,
                ui.visuals().weak_text_color(),
            );
        }
    }
}

enum ToolbarSlot<'a> {
    CanvasTool(CanvasToolIcon),
    Action(ToolbarActionIcon),
    Ascii(&'a str),
}

fn toolbar_selectable_inner(
    ui: &mut Ui,
    selected: bool,
    slot: ToolbarSlot<'_>,
    tooltip: &str,
) -> bool {
    let size = Vec2::splat(TOOLBAR_ICON_SZ);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        paint_toolbar_bg(ui, &response, selected);
        paint_toolbar_slot(ui, rect, slot, toolbar_color(ui, &response, selected));
    }
    response.on_hover_text(tooltip).clicked()
}

fn toolbar_button_inner(ui: &mut Ui, slot: ToolbarSlot<'_>, tooltip: &str) -> bool {
    let size = Vec2::splat(TOOLBAR_ICON_SZ);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        paint_toolbar_bg(ui, &response, false);
        paint_toolbar_slot(ui, rect, slot, toolbar_color(ui, &response, false));
    }
    response.on_hover_text(tooltip).clicked()
}

fn toolbar_color(ui: &Ui, response: &Response, selected: bool) -> Color32 {
    if selected {
        ui.visuals().strong_text_color()
    } else {
        hover_color(ui, response)
    }
}

fn paint_toolbar_bg(ui: &mut Ui, response: &Response, selected: bool) {
    if selected {
        ui.painter().rect_filled(
            response.rect,
            3.0,
            ui.visuals().selection.bg_fill,
        );
    } else if response.hovered() {
        ui.painter().rect_filled(
            response.rect,
            3.0,
            ui.visuals().widgets.hovered.bg_fill,
        );
    }
}

fn paint_toolbar_slot(ui: &mut Ui, rect: Rect, slot: ToolbarSlot<'_>, color: Color32) {
    match slot {
        ToolbarSlot::CanvasTool(icon) => paint_canvas_tool(ui, rect, icon, color),
        ToolbarSlot::Action(icon) => paint_toolbar_action(ui, rect, icon, color),
        ToolbarSlot::Ascii(label) => {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(13.0),
                color,
            );
        }
    }
}

fn paint_canvas_tool(ui: &mut Ui, rect: Rect, icon: CanvasToolIcon, color: Color32) {
    let stroke = Stroke::new(1.4_f32, color);
    let painter = ui.painter();
    let c = rect.center();
    let s = rect.width() * 0.30;
    match icon {
        CanvasToolIcon::Select => {
            let r = Rect::from_center_size(c, Vec2::splat(s * 1.6));
            painter.rect_stroke(r, 1.0, stroke, StrokeKind::Outside);
            let h = s * 0.28;
            for corner in [
                r.left_top(),
                r.right_top(),
                r.left_bottom(),
                r.right_bottom(),
            ] {
                painter.rect_filled(
                    Rect::from_center_size(corner, Vec2::splat(h)),
                    0.0,
                    color,
                );
            }
        }
        CanvasToolIcon::Pan => {
            let arm = s * 1.1;
            painter.line_segment([c + Vec2::new(-arm, 0.0), c + Vec2::new(arm, 0.0)], stroke);
            painter.line_segment([c + Vec2::new(0.0, -arm), c + Vec2::new(0.0, arm)], stroke);
            for (dir, tip) in [
                (Vec2::new(-arm, 0.0), Vec2::new(-1.0, 0.0)),
                (Vec2::new(arm, 0.0), Vec2::new(1.0, 0.0)),
                (Vec2::new(0.0, -arm), Vec2::new(0.0, -1.0)),
                (Vec2::new(0.0, arm), Vec2::new(0.0, 1.0)),
            ] {
                let base = c + dir;
                let n = Vec2::new(-tip.y, tip.x) * s * 0.35;
                painter.add(Shape::convex_polygon(
                    vec![base + tip * s * 0.45, base + n, base - n],
                    Color32::TRANSPARENT,
                    stroke,
                ));
            }
        }
        CanvasToolIcon::Pen => {
            let tip = c + Vec2::new(s * 0.55, s * 0.55);
            let tail = c + Vec2::new(-s * 0.75, -s * 0.75);
            painter.line_segment([tail, tip], stroke);
            painter.add(Shape::convex_polygon(
                vec![
                    tip,
                    tip + Vec2::new(-s * 0.35, s * 0.15),
                    tip + Vec2::new(s * 0.1, -s * 0.35),
                ],
                color,
                Stroke::NONE,
            ));
        }
        CanvasToolIcon::Eraser => {
            painter.circle_stroke(c, s * 0.75, stroke);
            painter.line_segment(
                [c + Vec2::angled(-0.25) * s * 0.9, c + Vec2::angled(2.9) * s * 0.9],
                stroke,
            );
        }
        CanvasToolIcon::Line => {
            painter.line_segment(
                [c + Vec2::new(-s, s), c + Vec2::new(s, -s)],
                stroke,
            );
        }
        CanvasToolIcon::Spline => {
            let pts: Vec<Pos2> = (0..=8)
                .map(|i| {
                    let t = i as f32 / 8.0;
                    let x = (t - 0.5) * s * 2.2;
                    let y = (t * std::f32::consts::TAU).sin() * s * 0.55;
                    c + Vec2::new(x, y)
                })
                .collect();
            painter.add(Shape::line(pts, stroke));
        }
        CanvasToolIcon::Path => {
            let pts = vec![
                c + Vec2::new(-s, s * 0.2),
                c + Vec2::new(-s * 0.2, -s * 0.7),
                c + Vec2::new(s * 0.5, -s * 0.1),
                c + Vec2::new(s, s * 0.6),
            ];
            painter.add(Shape::line(pts, stroke));
        }
        CanvasToolIcon::Rect => {
            painter.rect_stroke(
                Rect::from_center_size(c, Vec2::new(s * 1.7, s * 1.1)),
                1.0,
                stroke,
                StrokeKind::Outside,
            );
        }
        CanvasToolIcon::Ellipse => {
            painter.add(Shape::ellipse_stroke(
                c,
                Vec2::new(s * 0.85, s * 0.6),
                stroke,
            ));
        }
    }
}

fn paint_toolbar_action(ui: &mut Ui, rect: Rect, icon: ToolbarActionIcon, color: Color32) {
    let stroke = Stroke::new(1.4_f32, color);
    let painter = ui.painter();
    let c = rect.center();
    let s = rect.width() * 0.28;
    match icon {
        ToolbarActionIcon::ArrowDown => paint_chevron(painter, c, s, Vec2::new(0.0, 1.0), color, stroke),
        ToolbarActionIcon::ArrowUp => paint_chevron(painter, c, s, Vec2::new(0.0, -1.0), color, stroke),
        ToolbarActionIcon::Undo => {
            let r = s * 1.1;
            painter.circle_stroke(c + Vec2::new(r * 0.15, 0.0), r, stroke);
            let tip = c + Vec2::new(-r * 0.95, -r * 0.15);
            painter.line_segment([tip, tip + Vec2::new(s * 0.5, -s * 0.45)], stroke);
            painter.line_segment([tip, tip + Vec2::new(s * 0.5, s * 0.2)], stroke);
        }
        ToolbarActionIcon::ResetView => {
            painter.circle_stroke(c, s * 0.95, stroke);
            painter.circle_filled(c, s * 0.14, color);
            for dir in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
                let inner = c + dir * s * 0.35;
                let outer = c + dir * s * 1.05;
                painter.line_segment([inner, outer], stroke);
            }
        }
        ToolbarActionIcon::Grid => {
            let step = s * 0.75;
            for i in -1..=1 {
                let off = i as f32 * step;
                painter.line_segment(
                    [c + Vec2::new(-step, off), c + Vec2::new(step, off)],
                    stroke,
                );
                painter.line_segment(
                    [c + Vec2::new(off, -step), c + Vec2::new(off, step)],
                    stroke,
                );
            }
        }
        ToolbarActionIcon::Snap => {
            let step = s * 0.85;
            for dx in [-1.0_f32, 1.0] {
                for dy in [-1.0_f32, 1.0] {
                    let corner = c + Vec2::new(dx * step, dy * step);
                    painter.rect_stroke(
                        Rect::from_center_size(corner, Vec2::splat(s * 0.55)),
                        0.0,
                        stroke,
                        StrokeKind::Outside,
                    );
                }
            }
        }
        ToolbarActionIcon::Dashed => {
            let y = c.y;
            let x0 = c.x - s * 1.2;
            let dash = s * 0.45;
            let mut x = x0;
            while x < c.x + s * 1.2 {
                painter.line_segment(
                    [Pos2::new(x, y), Pos2::new((x + dash).min(c.x + s * 1.2), y)],
                    stroke,
                );
                x += dash * 1.6;
            }
        }
        ToolbarActionIcon::Clear | ToolbarActionIcon::ConfirmNo => {
            paint_failed_mark(ui, rect, color);
        }
        ToolbarActionIcon::ConfirmYes => paint_done_check(ui, rect, color),
    }
}

fn paint_chevron(
    painter: &egui::Painter,
    c: Pos2,
    s: f32,
    dir: Vec2,
    color: Color32,
    stroke: Stroke,
) {
    let perp = Vec2::new(-dir.y, dir.x);
    let tip = c + dir * s * 0.85;
    painter.add(Shape::convex_polygon(
        vec![tip, c - dir * s * 0.25 + perp * s, c - dir * s * 0.25 - perp * s],
        Color32::TRANSPARENT,
        stroke,
    ));
    let _ = color;
}

fn paint_done_check(ui: &mut Ui, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width() * 0.30;
    let stroke = Stroke::new(1.6_f32, color);
    let painter = ui.painter();
    painter.line_segment(
        [c + Vec2::new(-s * 0.95, s * 0.05), c + Vec2::new(-s * 0.15, s * 0.75)],
        stroke,
    );
    painter.line_segment(
        [c + Vec2::new(-s * 0.15, s * 0.75), c + Vec2::new(s * 0.95, -s * 0.70)],
        stroke,
    );
}

fn paint_close_mark(ui: &mut Ui, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width() * 0.22;
    let stroke = Stroke::new(1.5_f32, color);
    let painter = ui.painter();
    painter.line_segment([c + Vec2::new(-r, -r), c + Vec2::new(r, r)], stroke);
    painter.line_segment([c + Vec2::new(-r, r), c + Vec2::new(r, -r)], stroke);
}

fn paint_archived_filter(ui: &mut Ui, rect: Rect, color: Color32, selected: bool) {
    let stroke = Stroke::new(1.4_f32, color);
    let painter = ui.painter();
    let c = rect.center();
    let w = rect.width() * 0.34;
    let top = c.y - rect.height() * 0.22;
    let bot = c.y + rect.height() * 0.24;
    painter.line_segment([Pos2::new(c.x - w, top), Pos2::new(c.x + w, top)], stroke);
    painter.line_segment(
        [Pos2::new(c.x - w * 0.72, (top + bot) * 0.5), Pos2::new(c.x + w * 0.72, (top + bot) * 0.5)],
        stroke,
    );
    painter.line_segment([Pos2::new(c.x - w * 0.44, bot), Pos2::new(c.x + w * 0.44, bot)], stroke);
    if selected {
        painter.circle_filled(c + Vec2::new(w * 0.82, -rect.height() * 0.18), rect.width() * 0.09, color);
    }
}

fn paint_activity_list(ui: &mut Ui, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.4_f32, color);
    let painter = ui.painter();
    let c = rect.center();
    let w = rect.width() * 0.30;
    let gap = rect.height() * 0.22;
    for dy in [-gap, 0.0, gap] {
        let y = c.y + dy;
        let half = if dy < 0.0 { w * 0.85 } else if dy > 0.0 { w * 0.65 } else { w };
        painter.line_segment(
            [Pos2::new(c.x - half, y), Pos2::new(c.x + half, y)],
            stroke,
        );
    }
}

fn paint_failed_mark(ui: &mut Ui, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width() * 0.28;
    let stroke = Stroke::new(1.5_f32, color);
    let painter = ui.painter();
    painter.line_segment([c + Vec2::new(-r, -r), c + Vec2::new(r, r)], stroke);
    painter.line_segment([c + Vec2::new(-r, r), c + Vec2::new(r, -r)], stroke);
}

fn paint_running_dots(ui: &mut Ui, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width() * 0.09;
    let gap = rect.width() * 0.22;
    for dx in [-gap, 0.0, gap] {
        ui.painter()
            .circle_filled(c + Vec2::new(dx, 0.0), r, color);
    }
}

fn paint_link_arrow(ui: &mut Ui, outgoing: bool) {
    let size = Vec2::new(12.0, 12.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let color = ui.visuals().weak_text_color();
    let stroke = Stroke::new(1.3_f32, color);
    let painter = ui.painter();
    let y = rect.center().y;
    let head = rect.width() * 0.28;
    if outgoing {
        let tail = Pos2::new(rect.left() + 1.0, y);
        let tip = Pos2::new(rect.right() - 1.0, y);
        painter.line_segment([tail, tip], stroke);
        painter.line_segment([tip, Pos2::new(tip.x - head, y - head * 0.75)], stroke);
        painter.line_segment([tip, Pos2::new(tip.x - head, y + head * 0.75)], stroke);
    } else {
        let tail = Pos2::new(rect.right() - 1.0, y);
        let tip = Pos2::new(rect.left() + 1.0, y);
        painter.line_segment([tail, tip], stroke);
        painter.line_segment([tip, Pos2::new(tip.x + head, y - head * 0.75)], stroke);
        painter.line_segment([tip, Pos2::new(tip.x + head, y + head * 0.75)], stroke);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attach_icon_is_widget() {
        let _ = std::any::type_name::<AttachIcon>();
    }
}
