//! Role classification, styling, and frame layout for chat messages.

use crate::i18n::UiStrings;
use eframe::egui;
use egui_commonmark::CommonMarkViewer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChatBubbleKind {
    User,
    Assistant,
    RoomSpeaker,
    System,
}

pub(crate) fn chat_bubble_kind(
    role: &str,
    speaker_id: Option<&str>,
    room_mode: bool,
) -> ChatBubbleKind {
    match role {
        "user" | "vous" => ChatBubbleKind::User,
        "assistant" if room_mode && speaker_id.is_some() => ChatBubbleKind::RoomSpeaker,
        "assistant" => ChatBubbleKind::Assistant,
        _ => ChatBubbleKind::System,
    }
}

pub(crate) fn chat_role_label(
    kind: ChatBubbleKind,
    strings: &UiStrings,
    raw_role: &str,
) -> String {
    match kind {
        ChatBubbleKind::User => strings.chat_you.to_string(),
        ChatBubbleKind::Assistant => strings.chat_assistant.to_string(),
        ChatBubbleKind::RoomSpeaker => String::new(),
        ChatBubbleKind::System => {
            if raw_role == "système" || raw_role == "system" {
                strings.chat_system.to_string()
            } else {
                raw_role.to_string()
            }
        }
    }
}

/// Returns fill, stroke, and role-label colors.
pub(crate) fn chat_bubble_colors(
    kind: ChatBubbleKind,
    dark: bool,
) -> (egui::Color32, egui::Color32, egui::Color32) {
    match (kind, dark) {
        (ChatBubbleKind::User, true) => (
            egui::Color32::from_rgb(18, 42, 48),
            egui::Color32::from_rgb(62, 224, 196),
            egui::Color32::from_rgb(120, 230, 210),
        ),
        (ChatBubbleKind::User, false) => (
            egui::Color32::from_rgb(220, 242, 238),
            egui::Color32::from_rgb(20, 140, 120),
            egui::Color32::from_rgb(10, 100, 90),
        ),
        (ChatBubbleKind::Assistant | ChatBubbleKind::RoomSpeaker, true) => (
            egui::Color32::from_rgb(28, 32, 40),
            egui::Color32::from_rgb(90, 100, 120),
            egui::Color32::from_rgb(180, 190, 210),
        ),
        (ChatBubbleKind::Assistant | ChatBubbleKind::RoomSpeaker, false) => (
            egui::Color32::from_rgb(236, 238, 244),
            egui::Color32::from_rgb(120, 128, 148),
            egui::Color32::from_rgb(50, 56, 72),
        ),
        (ChatBubbleKind::System, true) => (
            egui::Color32::from_rgb(22, 22, 26),
            egui::Color32::from_rgb(70, 70, 78),
            egui::Color32::from_rgb(150, 150, 160),
        ),
        (ChatBubbleKind::System, false) => (
            egui::Color32::from_rgb(242, 242, 244),
            egui::Color32::from_rgb(170, 170, 178),
            egui::Color32::from_rgb(100, 100, 110),
        ),
    }
}

pub(crate) fn chat_markdown_viewer(ui: &egui::Ui) -> CommonMarkViewer<'static> {
    let width = ui.available_width().max(1.0) as usize;
    CommonMarkViewer::new().default_width(Some(width))
}

pub(crate) fn chat_bubble_max_width(available_w: f32, kind: ChatBubbleKind) -> f32 {
    if available_w <= 0.0 {
        return 0.0;
    }
    let fraction = match kind {
        ChatBubbleKind::User => 0.88,
        ChatBubbleKind::Assistant | ChatBubbleKind::RoomSpeaker => 0.96,
        ChatBubbleKind::System => 0.92,
    };
    let width = (available_w * fraction).min(available_w);
    width.max(available_w.min(48.0))
}

/// Role-colored frame. User messages sit right; all other roles sit left.
pub(crate) fn chat_message_frame(
    ui: &mut egui::Ui,
    kind: ChatBubbleKind,
    color_override: Option<(egui::Color32, egui::Color32)>,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let dark = ui.visuals().dark_mode;
    let (fill, stroke) = color_override.unwrap_or_else(|| {
        let (fill, stroke, _) = chat_bubble_colors(kind, dark);
        (fill, stroke)
    });
    let max_w = chat_bubble_max_width(ui.available_width(), kind);
    let layout = match kind {
        ChatBubbleKind::User => egui::Layout::right_to_left(egui::Align::Min),
        _ => egui::Layout::left_to_right(egui::Align::Min),
    };

    ui.with_layout(layout, |ui| {
        ui.set_max_width(max_w);
        ui.set_width(max_w);
        egui::Frame::NONE
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, stroke))
            .corner_radius(6.0)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_max_width(max_w - 8.0);
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), add_contents);
            });
    });
    ui.add_space(6.0);
}
