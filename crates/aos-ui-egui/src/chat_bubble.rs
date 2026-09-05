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

/// Returns fill, stroke, and role-label colors. Tout est dérivé du thème
/// courant (`button_colors` + `Visuals`) : aucune valeur en dur, `custom`
/// inclus. En mode clair, l'accent est assombri vers le texte pour le label.
pub(crate) fn chat_bubble_colors(
    ui: &egui::Ui,
    kind: ChatBubbleKind,
) -> (egui::Color32, egui::Color32, egui::Color32) {
    let v = ui.visuals();
    let tc = crate::theme::button_colors(ui);
    let text = v.strong_text_color();
    let weak = v.weak_text_color();
    // Mélange local (Visuals n'expose pas de mix public).
    let mix = |a: egui::Color32, b: egui::Color32, t: f32| {
        let t = t.clamp(0.0, 1.0);
        let inv = 1.0 - t;
        egui::Color32::from_rgb(
            (f32::from(a.r()) * inv + f32::from(b.r()) * t) as u8,
            (f32::from(a.g()) * inv + f32::from(b.g()) * t) as u8,
            (f32::from(a.b()) * inv + f32::from(b.b()) * t) as u8,
        )
    };
    match kind {
        ChatBubbleKind::User => {
            let fill = mix(v.panel_fill, tc.accent, 0.14);
            let role = if v.dark_mode {
                tc.accent
            } else {
                mix(tc.accent, text, 0.45)
            };
            (fill, tc.accent, role)
        }
        ChatBubbleKind::Assistant | ChatBubbleKind::RoomSpeaker => {
            (v.faint_bg_color, weak, v.strong_text_color())
        }
        ChatBubbleKind::System => (v.extreme_bg_color, weak, weak),
    }
}

/// Compat tests : mêmes teintes que le thème dark, sans contexte egui.
#[cfg(test)]
pub(crate) fn chat_bubble_colors_legacy(
    kind: ChatBubbleKind,
    dark: bool,
) -> (egui::Color32, egui::Color32, egui::Color32) {
    chat_bubble_colors_static(kind, dark)
}

/// Returns fill, stroke, and role-label colors (static dark/light reference).
fn chat_bubble_colors_static(
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
/// Largeur contenu : `max_w` est un plafond, jamais une largeur forcée —
/// un "ok" ne s'étire plus à 88% de la vue.
pub(crate) fn chat_message_frame(
    ui: &mut egui::Ui,
    kind: ChatBubbleKind,
    color_override: Option<(egui::Color32, egui::Color32)>,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let (fill, stroke) = color_override.unwrap_or_else(|| {
        let (fill, stroke, _) = chat_bubble_colors(ui, kind);
        (fill, stroke)
    });
    let max_w = chat_bubble_max_width(ui.available_width(), kind);
    let layout = match kind {
        ChatBubbleKind::User => egui::Layout::right_to_left(egui::Align::Min),
        _ => egui::Layout::left_to_right(egui::Align::Min),
    };

    ui.with_layout(layout, |ui| {
        ui.set_max_width(max_w);
        egui::Frame::NONE
            .fill(fill)
            .stroke(egui::Stroke::new(1.0_f32, stroke))
            .corner_radius(crate::theme::RADIUS_MD)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_max_width((max_w - 8.0).max(1.0));
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), add_contents);
            });
    });
    ui.add_space(6.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_bubble_roles_stay_distinct() {
        let (user_fill, _, _) = chat_bubble_colors_legacy(ChatBubbleKind::User, true);
        let (asst_fill, _, _) = chat_bubble_colors_legacy(ChatBubbleKind::Assistant, true);
        assert_ne!(user_fill, asst_fill);
    }
}
