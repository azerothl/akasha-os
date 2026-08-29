//! Painted chrome icons — no emoji or special font glyphs.

use eframe::egui::{
    self, Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, Widget,
};

const BTN: f32 = 18.0;

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
    let size = Vec2::splat(14.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let stroke = Stroke::new(1.5_f32, ui.visuals().weak_text_color());
    let l = rect.left();
    let r = rect.right();
    let b = rect.bottom();
    let t = rect.top();
    ui.painter().line_segment(
        [
            Pos2::new(l + 2.0, (b + t) * 0.55),
            Pos2::new((l + r) * 0.45, b - 2.0),
        ],
        stroke,
    );
    ui.painter().line_segment(
        [
            Pos2::new((l + r) * 0.45, b - 2.0),
            Pos2::new(r - 2.0, t + 3.0),
        ],
        stroke,
    );
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
