//! Pure sizing helpers for the chat composer.

use crate::i18n::UiStrings;
use eframe::egui;

pub(crate) const COMPOSER_MIN_INPUT_W: f32 = 140.0;
pub(crate) const COMPOSER_INPUT_ROW_H: f32 = 44.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ChatSessionsSplit {
    pub(crate) side_w: f32,
    pub(crate) chat_w: f32,
}

/// Sidebar and main-chat widths that never exceed `full_w`.
pub(crate) fn chat_sessions_split(
    full_w: f32,
    gap: f32,
    canvas_open: bool,
) -> ChatSessionsSplit {
    let min_main = if canvas_open { 240.0_f32 } else { 200.0_f32 };
    let max_side = if canvas_open { 160.0_f32 } else { 220.0_f32 };
    let min_side = if canvas_open { 80.0_f32 } else { 120.0_f32 };
    let mut side_w = max_side.min((full_w * 0.30).max(min_side));
    if full_w - side_w - gap < min_main {
        side_w = (full_w - gap - min_main).clamp(80.0, max_side);
    }
    side_w = side_w.min((full_w - gap).max(0.0));
    let chat_w = (full_w - side_w - gap).max(0.0);
    ChatSessionsSplit { side_w, chat_w }
}

/// Actual horizontal space can be smaller than the planned split when a native
/// widget (for example a long selected model id) imposes a larger minimum width.
/// Never allocate a chat workspace beyond what remains in the parent UI.
pub(crate) fn bounded_chat_workspace_width(
    planned_w: f32,
    remaining_w: f32,
    trailing_gutter: f32,
) -> f32 {
    (planned_w.min(remaining_w) - trailing_gutter.max(0.0)).max(0.0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ChatCanvasLayout {
    SideBySide { transcript_w: f32, canvas_w: f32 },
    Stacked { transcript_h: f32, canvas_h: f32 },
}

/// Transcript and canvas layout that fits in the available dimensions.
pub(crate) fn chat_canvas_layout(
    total_w: f32,
    content_h: f32,
    split_gap: f32,
) -> ChatCanvasLayout {
    const CANVAS_MIN: f32 = 96.0;
    const TRANSCRIPT_MIN: f32 = 140.0;
    const STACK_BELOW: f32 = 360.0;
    if total_w < STACK_BELOW {
        let transcript_h = (content_h * 0.55).max(72.0);
        let canvas_h = (content_h - transcript_h - split_gap).max(72.0);
        ChatCanvasLayout::Stacked {
            transcript_h,
            canvas_h,
        }
    } else {
        let max_canvas = (total_w - TRANSCRIPT_MIN - split_gap).max(0.0);
        let canvas_w = ((total_w - split_gap) * 0.40)
            .clamp(CANVAS_MIN.min(max_canvas), max_canvas);
        let transcript_w = (total_w - canvas_w - split_gap).max(0.0);
        ChatCanvasLayout::SideBySide {
            transcript_w,
            canvas_w,
        }
    }
}

/// Conservative button-row width for wrap prediction before egui measurement.
pub(crate) fn estimate_composer_buttons_w(send: &str, show_stop: bool, stop: &str) -> f32 {
    const CHAR_W: f32 = 8.5;
    const BTN_PAD: f32 = 20.0;
    const ITEM_GAP: f32 = 4.0;
    let mut width = send.len() as f32 * CHAR_W + BTN_PAD;
    if show_stop {
        width += ITEM_GAP + stop.len() as f32 * CHAR_W + BTN_PAD;
    }
    width
}

fn ui_button_width(ui: &egui::Ui, label: &str) -> f32 {
    let padding = ui.style().spacing.button_padding;
    let font_id = ui.style().text_styles[&egui::TextStyle::Button].clone();
    let galley = ui.fonts(|fonts| {
        fonts.layout_no_wrap(label.to_owned(), font_id, egui::Color32::PLACEHOLDER)
    });
    galley.size().x + padding.x * 2.0
}

pub(crate) fn send_button_reserved_width(ui: &egui::Ui, strings: &UiStrings) -> f32 {
    let measured = ui_button_width(ui, strings.agent_send);
    let french_floor = estimate_composer_buttons_w("Envoyer", false, "");
    measured.max(french_floor)
}

pub(crate) fn stop_button_reserved_width(ui: &egui::Ui, strings: &UiStrings) -> f32 {
    let measured = ui_button_width(ui, strings.chat_stop);
    let french_floor = estimate_composer_buttons_w("Stop", false, "");
    measured.max(french_floor)
}

/// Field width after reserving fixed send, stop, paperclip, and gap chrome.
pub(crate) fn composer_field_width(
    row_w: f32,
    send_w: f32,
    attach_w: f32,
    stop_w: f32,
    gap: f32,
    show_stop: bool,
) -> f32 {
    let gaps = if show_stop { gap * 3.0 } else { gap * 2.0 };
    let chrome = send_w + attach_w + gaps + if show_stop { stop_w } else { 0.0 };
    (row_w - chrome).max(0.0)
}

#[cfg(test)]
pub(crate) fn chat_composer_wraps(
    available_w: f32,
    attach_w: f32,
    buttons_w: f32,
) -> bool {
    available_w - attach_w - buttons_w < COMPOSER_MIN_INPUT_W
}

/// Vertical space for the fixed send row and optional stacks growing upward.
pub(crate) fn chat_composer_reserve_height(
    chat_w: f32,
    ask_queue_len: usize,
    pending_images_len: usize,
    pending_documents_len: usize,
    show_vision_banner: bool,
) -> f32 {
    const ASK_QUEUE_H: f32 = 22.0;
    const VISION_BANNER_H: f32 = 28.0;
    const CHIP_W: f32 = 48.0;
    const CHIP_ROW_H: f32 = 36.0;

    let mut height = COMPOSER_INPUT_ROW_H;
    if ask_queue_len > 1 {
        height += ASK_QUEUE_H;
    }
    let pending_chips = pending_images_len + pending_documents_len;
    if pending_chips > 0 {
        if show_vision_banner {
            height += VISION_BANNER_H;
        }
        let chips_per_row = (chat_w / CHIP_W).floor().max(1.0) as usize;
        let rows = pending_chips.div_ceil(chips_per_row);
        height += (rows as f32) * CHIP_ROW_H;
    }
    height
}
