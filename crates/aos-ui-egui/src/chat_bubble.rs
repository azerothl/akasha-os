//! Role classification, styling, and frame layout for chat messages.

use crate::i18n::UiStrings;
use eframe::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

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

pub(crate) fn chat_role_label(kind: ChatBubbleKind, strings: &UiStrings, raw_role: &str) -> String {
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
            let fill = mix(v.panel_fill, tc.accent, 0.10);
            let stroke = mix(v.panel_fill, tc.accent, 0.22);
            let role = if v.dark_mode {
                tc.accent
            } else {
                mix(tc.accent, text, 0.45)
            };
            (fill, stroke, role)
        }
        ChatBubbleKind::Assistant | ChatBubbleKind::RoomSpeaker => {
            (v.panel_fill, v.panel_fill, v.strong_text_color())
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
#[cfg(test)]
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
    CommonMarkViewer::new()
        .default_width(Some(width))
        .max_image_width(Some(width))
        .syntax_theme_dark("base16-ocean.dark")
        .syntax_theme_light("base16-ocean.light")
}

/// Paint chat markdown with list indent; fences use syntect directly
/// (egui_commonmark only looks up languages as *file extensions*).
pub(crate) fn show_chat_markdown(ui: &mut egui::Ui, cache: &mut CommonMarkCache, md: &str) {
    ui.spacing_mut().indent = ui.spacing().indent.max(22.0);
    crate::chat_room::for_each_markdown_part(md, |prose, fence| {
        if let Some((lang, body)) = fence {
            crate::chat_code::show_code_fence(ui, lang, body);
        } else if !prose.trim().is_empty() {
            chat_markdown_viewer(ui).show(ui, cache, prose);
        }
    });
}

/// Quiet duration / token line for assistant and agent replies.
pub(crate) fn reply_meta_line(duration_ms: u64, text: &str, exact_tokens: u64) -> String {
    let mut parts = Vec::new();
    if duration_ms > 0 {
        parts.push(crate::agent_panel::fmt_ms(duration_ms));
    }
    if exact_tokens > 0 {
        parts.push(format!("{exact_tokens} tok"));
    } else {
        let approx = (text.chars().count() as u64) / 4;
        if approx > 0 {
            parts.push(format!("≈{approx} tok"));
        }
    }
    parts.join(" · ")
}

pub(crate) fn chat_bubble_max_width(available_w: f32, kind: ChatBubbleKind) -> f32 {
    if available_w <= 0.0 {
        return 0.0;
    }
    let fraction = match kind {
        ChatBubbleKind::User => 0.72,
        ChatBubbleKind::Assistant | ChatBubbleKind::RoomSpeaker => 0.92,
        ChatBubbleKind::System => 0.88,
    };
    let width = (available_w * fraction).min(available_w);
    width.max(available_w.min(48.0))
}

/// Role-colored frame. User messages sit right; assistant acks are unframed
/// body copy; cards (agents, grants, artifacts) supply their own chrome.
/// Largeur contenu : `max_w` est un plafond, jamais une largeur forcée —
/// un "ok" ne s'étire plus à 88% de la vue.
pub(crate) fn chat_message_frame(
    ui: &mut egui::Ui,
    kind: ChatBubbleKind,
    color_override: Option<(egui::Color32, egui::Color32)>,
    speaker_rail: Option<egui::Color32>,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let (fill, stroke) = color_override.unwrap_or_else(|| {
        let (fill, stroke, _) = chat_bubble_colors(ui, kind);
        (fill, stroke)
    });
    let max_w = chat_bubble_max_width(ui.available_width(), kind);
    let layout = match kind {
        ChatBubbleKind::User => egui::Layout::right_to_left(egui::Align::Min),
        _ => egui::Layout::left_to_right(egui::Align::Min),
    };

    let framed = matches!(kind, ChatBubbleKind::User | ChatBubbleKind::System);
    let rail_w = if speaker_rail.is_some() { 14.0 } else { 0.0 };
    let response = ui
        .with_layout(layout, |ui| {
            ui.set_max_width(max_w);
            if framed {
                egui::Frame::NONE
                    .fill(fill)
                    .stroke(egui::Stroke::new(1.0_f32, stroke))
                    .corner_radius(crate::theme::RADIUS_MD)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_max_width((max_w - 8.0).max(1.0));
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), add_contents);
                    })
                    .response
            } else {
                let inner = ui.horizontal(|ui| {
                    if speaker_rail.is_some() {
                        ui.add_space(rail_w);
                    }
                    ui.vertical(|ui| {
                        ui.set_max_width((max_w - rail_w).max(1.0));
                        add_contents(ui);
                    });
                });
                if let Some(color) = speaker_rail {
                    let r = inner.response.rect;
                    if r.height() > 6.0 {
                        let bar = egui::Rect::from_min_max(
                            egui::pos2(r.left(), r.top() + 1.0),
                            egui::pos2(r.left() + 4.0, r.bottom() - 1.0),
                        );
                        ui.painter().rect_filled(bar, 2.0, color);
                    }
                }
                inner.response
            }
        })
        .inner;
    ui.add_space(if matches!(kind, ChatBubbleKind::User) {
        10.0
    } else {
        8.0
    });
    response
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

    #[test]
    fn reply_meta_line_includes_duration_and_approx_tokens() {
        let line = reply_meta_line(3_200, "abcdefghij", 0);
        assert!(line.contains("s"));
        assert!(line.contains("tok"));
        assert!(line.contains('≈'));
        let exact = reply_meta_line(0, "abcdefghij", 42);
        assert_eq!(exact, "42 tok");
    }
}
