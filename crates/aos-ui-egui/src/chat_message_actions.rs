//! Hover chrome for per-message transcript actions (copy / fork / return / continue).
//!
//! Logic stays in `ui_chat_transcript`; this module only paints the chamber-styled
//! instrument strip so the actions are discoverable without a right-click.

use eframe::egui::{self, Id, Order, Pos2, Sense};

use crate::i18n::UiStrings;
use crate::icons::{self, MessageActionIcon};
use crate::theme;

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

/// Floating hover strip anchored to a bubble. Stays open while the pointer
/// is over the bubble or the strip itself.
pub(crate) fn show_hover_bar(
    ui: &mut egui::Ui,
    message_index: usize,
    bubble: &egui::Response,
    t: &UiStrings,
    opts: MessageActionOpts,
) -> MessageActionClicks {
    let latch_id = ui.id().with(("msg_act_latch", message_index));
    let mut open = ui.ctx().data(|d| d.get_temp::<bool>(latch_id).unwrap_or(false));
    if bubble.hovered() {
        open = true;
    }
    let mut clicks = MessageActionClicks::default();
    if !open {
        ui.ctx().data_mut(|d| d.insert_temp(latch_id, false));
        return clicks;
    }

    let count = 1usize + usize::from(opts.can_branch) * 2 + usize::from(opts.can_continue);
    let pad = theme::SPACE_UNIT * 0.5;
    let gap = 2.0_f32;
    let bar_w = pad * 2.0 + count as f32 * theme::ICON_HIT + (count.saturating_sub(1) as f32) * gap;
    let bar_h = theme::ICON_HIT + pad * 2.0;
    let mut pos = Pos2::new(
        bubble.rect.right() - bar_w,
        bubble.rect.top() - bar_h + theme::SPACE_UNIT,
    );
    if pos.x < bubble.rect.left() {
        pos.x = bubble.rect.left();
    }
    // Keep the strip inside the scroll viewport when the bubble is flush to the top.
    if pos.y < ui.clip_rect().top() {
        pos.y = bubble.rect.top() + 2.0;
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
                .stroke(egui::Stroke::new(1.0, stroke.color))
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

    open = bubble.hovered() || inner.response.hovered();
    ui.ctx().data_mut(|d| d.insert_temp(latch_id, open));
    clicks
}

/// Inline Continue strip for an interrupted assistant turn (transcript tail).
pub(crate) fn show_continue_strip(ui: &mut egui::Ui, t: &UiStrings) -> bool {
    let mut continue_now = false;
    let accent = theme::button_colors(ui).accent;
    egui::Frame::NONE
        .fill(ui.visuals().panel_fill)
        .stroke(egui::Stroke::new(1.0, accent))
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
