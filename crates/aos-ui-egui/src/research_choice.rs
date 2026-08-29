//! Research choice + document result cards in the chat thread.

use aos_proto::ChatAttachment;
use eframe::egui;

use crate::decl_ui;
use crate::i18n::UiStrings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchChoiceAction {
    None,
    Answer,
    Document,
}

pub fn choice_actions_enabled(state: &str) -> bool {
    state == "pending"
}

pub fn render_research_choice(
    ui: &mut egui::Ui,
    t: &UiStrings,
    question: &str,
    state: &str,
) -> ResearchChoiceAction {
    if !choice_actions_enabled(state) {
        return ResearchChoiceAction::None;
    }
    let mut action = ResearchChoiceAction::None;
    egui::Frame::NONE
        .fill(egui::Color32::from_rgb(32, 36, 44))
        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(70, 90, 120)))
        .inner_margin(10.0_f32)
        .corner_radius(4.0_f32)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(question.trim()).strong());
            ui.weak(t.research_choice_prompt);
            ui.horizontal(|ui| {
                if ui.button(t.research_choice_answer).clicked() {
                    action = ResearchChoiceAction::Answer;
                }
                if ui.button(t.research_choice_document).clicked() {
                    action = ResearchChoiceAction::Document;
                }
            });
        });
    action
}

pub fn render_document_result(
    ui: &mut egui::Ui,
    t: &UiStrings,
    question: &str,
    path: &str,
    label: &str,
) {
    let shown = if label.trim().is_empty() {
        path
    } else {
        label
    };
    egui::Frame::NONE
        .fill(egui::Color32::from_rgb(30, 38, 34))
        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(90, 140, 110)))
        .inner_margin(10.0_f32)
        .corner_radius(4.0_f32)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(question.trim()).strong());
            ui.weak(t.document_result_ready);
            ui.horizontal(|ui| {
                ui.weak(shown);
                if ui.button(t.document_result_open).clicked() {
                    let _ = decl_ui::open_host_path(path);
                }
            });
        });
}

pub fn choice_attachment(question: &str, choice_id: &str) -> ChatAttachment {
    ChatAttachment::ResearchChoice {
        choice_id: choice_id.to_string(),
        question: question.to_string(),
        state: "pending".into(),
    }
}

pub fn document_result_attachment(question: &str, path: &str, label: &str) -> ChatAttachment {
    ChatAttachment::DocumentResult {
        question: question.to_string(),
        path: path.to_string(),
        label: label.to_string(),
    }
}

pub fn label_from_path(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_labels_en_fr() {
        let t_en = crate::i18n::strings("en");
        let t_fr = crate::i18n::strings("fr");
        assert_eq!(t_en.research_choice_answer, "Answer");
        assert_eq!(t_fr.research_choice_answer, "Répondre");
        assert_eq!(t_en.research_choice_document, "Prepare a document");
        assert_eq!(t_fr.research_choice_document, "Préparer un document");
        assert_ne!(t_en.research_choice_document, t_fr.research_choice_document);
    }

    #[test]
    fn choice_attachment_pending_state() {
        match choice_attachment("What is SOTA?", "rc-1") {
            ChatAttachment::ResearchChoice {
                state,
                question,
                ..
            } => {
                assert_eq!(state, "pending");
                assert_eq!(question, "What is SOTA?");
            }
            _ => panic!("expected ResearchChoice"),
        }
    }

    #[test]
    fn document_result_label_from_path() {
        assert_eq!(
            label_from_path("/downloads/research-agentic.md"),
            "research-agentic.md"
        );
    }

    #[test]
    fn answer_skips_when_state_not_pending() {
        assert!(choice_actions_enabled("pending"));
        assert!(!choice_actions_enabled("answer"));
        assert!(!choice_actions_enabled("document"));
    }
}
