//! Hover chrome for per-message transcript actions (copy / fork / return / continue).
//!
//! Logic stays in `ui_chat_transcript`; this module only paints the chamber-styled
//! instrument strip so the actions are discoverable without a right-click.

use eframe::egui::{self, Id, Order, Pos2, Rect, Sense, Vec2};

use crate::i18n::UiStrings;
use crate::icons::{self, MessageActionIcon};
use crate::theme;

/// Keep the strip open briefly after the pointer leaves so tooltip popups
/// (which can steal hover for a frame) do not collapse the whole bar.
const HOVER_CLOSE_GRACE_SECS: f64 = 0.28;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MessageActionOpts {
    pub can_branch: bool,
    pub can_continue: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MessageActionClicks {
    pub copy: bool,
    pub fork: bool,
    pub return_here: bool,
    pub continue_partial: bool,
}

/// Floating hover strip anchored inside a bubble. Stays open while the pointer
/// is over the bubble, the strip, or for a short grace period after leaving.
pub(crate) fn show_hover_bar(
    ui: &mut egui::Ui,
    message_index: usize,
    bubble: &egui::Response,
    t: &UiStrings,
    opts: MessageActionOpts,
) -> MessageActionClicks {
    let latch_id = ui.id().with(("msg_act_latch", message_index));
    let now = ui.ctx().input(|i| i.time);
    let mut open_until = ui
        .ctx()
        .data(|d| d.get_temp::<f64>(latch_id).unwrap_or(0.0));

    let count = 1usize + usize::from(opts.can_branch) * 2 + usize::from(opts.can_continue);
    let pad = theme::SPACE_UNIT * 0.5;
    let gap = 2.0_f32;
    let bar_w = pad * 2.0 + count as f32 * theme::ICON_HIT + (count.saturating_sub(1) as f32) * gap;
    let bar_h = theme::ICON_HIT + pad * 2.0;
    // Sit inside the bubble (top-right) so moving onto icons never leaves the
    // bubble hit-test — the previous "floating above" placement caused open/close flicker.
    let inset = 4.0_f32;
    let mut pos = Pos2::new(
        bubble.rect.right() - bar_w - inset,
        bubble.rect.top() + inset,
    );
    if pos.x < bubble.rect.left() + inset {
        pos.x = bubble.rect.left() + inset;
    }
    if pos.y + bar_h > bubble.rect.bottom() {
        pos.y = (bubble.rect.bottom() - bar_h).max(bubble.rect.top());
    }

    let planned_bar = Rect::from_min_size(pos, Vec2::new(bar_w, bar_h));
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let over_bubble =
        bubble.hovered() || pointer.is_some_and(|p| bubble.rect.expand(6.0).contains(p));
    let over_planned_bar = pointer.is_some_and(|p| planned_bar.expand(6.0).contains(p));
    if over_bubble || over_planned_bar {
        open_until = now + HOVER_CLOSE_GRACE_SECS;
    }
    let open = now <= open_until;

    let mut clicks = MessageActionClicks::default();
    if !open {
        ui.ctx().data_mut(|d| d.insert_temp(latch_id, 0.0));
        return clicks;
    }

    let area_id = Id::new(("msg_act_area", message_index));
    let inner = egui::Area::new(area_id)
        .order(Order::Foreground)
        .fixed_pos(pos)
        .constrain_to(ui.ctx().screen_rect())
        .sense(Sense::hover())
        .show(ui.ctx(), |ui| {
            let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
            let fill = ui.visuals().panel_fill;
            egui::Frame::NONE
                .fill(fill)
                .stroke(egui::Stroke::new(1.0_f32, stroke.color))
                .corner_radius(theme::RADIUS_MD)
                .inner_margin(egui::Margin::symmetric(pad as i8, pad as i8))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    ui.horizontal(|ui| {
                        if icons::message_action_button(ui, MessageActionIcon::Copy, t.btn_copy) {
                            clicks.copy = true;
                        }
                        if opts.can_branch {
                            if icons::message_action_button(
                                ui,
                                MessageActionIcon::Fork,
                                t.chat_fork_here,
                            ) {
                                clicks.fork = true;
                            }
                            if icons::message_action_button(
                                ui,
                                MessageActionIcon::Return,
                                t.chat_return_here,
                            ) {
                                clicks.return_here = true;
                            }
                        }
                        if opts.can_continue
                            && icons::message_action_button(
                                ui,
                                MessageActionIcon::Continue,
                                t.chat_continue_partial_hint,
                            )
                        {
                            clicks.continue_partial = true;
                        }
                    });
                });
        });

    let over_bar = inner.response.hovered()
        || pointer.is_some_and(|p| inner.response.rect.expand(8.0).contains(p));
    if over_bubble || over_bar {
        open_until = now + HOVER_CLOSE_GRACE_SECS;
    }
    ui.ctx().data_mut(|d| d.insert_temp(latch_id, open_until));
    clicks
}

/// Inline Continue strip for an interrupted assistant turn (transcript tail).
pub(crate) fn show_continue_strip(ui: &mut egui::Ui, t: &UiStrings) -> bool {
    let mut continue_now = false;
    let accent = theme::button_colors(ui).accent;
    egui::Frame::NONE
        .fill(ui.visuals().panel_fill)
        .stroke(egui::Stroke::new(1.0_f32, accent))
        .corner_radius(theme::RADIUS_MD)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if icons::message_action_button(
                    ui,
                    MessageActionIcon::Continue,
                    t.chat_continue_partial_hint,
                ) {
                    continue_now = true;
                }
                ui.add_space(theme::SPACE_UNIT * 0.5);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(t.chat_continue_partial).strong());
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(t.chat_continue_partial_hint)
                                    .small()
                                    .color(accent),
                            )
                            .frame(false)
                            .min_size(egui::vec2(
                                0.0,
                                theme::CONTROL_MIN_H_COMFORTABLE - theme::SPACE_UNIT,
                            )),
                        )
                        .on_hover_text(t.chat_continue_partial_hint)
                        .clicked()
                    {
                        continue_now = true;
                    }
                });
            });
        });
    continue_now
}
