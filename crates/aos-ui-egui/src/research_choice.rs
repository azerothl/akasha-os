//! Research choice + document result/progress cards in the chat thread.

use aos_proto::ChatAttachment;
use eframe::egui;

use crate::i18n::UiStrings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchChoiceAction {
    None,
    Answer,
    Document,
}

#[derive(PartialEq, Eq)]
pub enum DocumentProgressAction {
    None,
    Stop(String),
}

#[derive(PartialEq, Eq)]
pub enum DocumentResultAction {
    None,
    Open,
}

pub fn choice_actions_enabled(state: &str) -> bool {
    state == "pending"
}

pub fn render_research_choice(
    ui: &mut egui::Ui,
    t: &UiStrings,
    state: &str,
) -> ResearchChoiceAction {
    if !choice_actions_enabled(state) {
        return ResearchChoiceAction::None;
    }
    let mut action = ResearchChoiceAction::None;
    ui.group(|ui| {
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

pub fn render_document_progress(
    ui: &mut egui::Ui,
    t: &UiStrings,
    question: &str,
    agent_id: &str,
    state: &str,
) -> DocumentProgressAction {
    if state == "stopped" {
        ui.group(|ui| {
            ui.label(egui::RichText::new(question.trim()).strong());
        });
        return DocumentProgressAction::None;
    }
    let mut action = DocumentProgressAction::None;
    ui.group(|ui| {
        ui.label(egui::RichText::new(question.trim()).strong());
        ui.horizontal(|ui| {
            ui.weak(t.document_progress_label);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t.document_progress_stop).clicked() {
                    action = DocumentProgressAction::Stop(agent_id.to_string());
                }
            });
        });
    });
    action
}

pub fn render_document_result(
    ui: &mut egui::Ui,
    t: &UiStrings,
    question: &str,
) -> DocumentResultAction {
    let mut action = DocumentResultAction::None;
    ui.group(|ui| {
        ui.label(egui::RichText::new(question.trim()).strong());
        ui.weak(t.document_result_ready);
        ui.horizontal(|ui| {
            if ui.button(t.document_result_open).clicked() {
                action = DocumentResultAction::Open;
            }
        });
    });
    action
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
        assert_eq!(
            t_en.research_choice_prompt,
            "I can answer here, or prepare a document."
        );
        assert_eq!(
            t_fr.research_choice_prompt,
            "Je peux répondre ici, ou préparer un document."
        );
        assert_eq!(t_en.research_choice_answer, "Reply");
        assert_eq!(t_fr.research_choice_answer, "Répondre");
        assert_eq!(t_en.research_choice_document, "Prepare a document");
        assert_eq!(t_fr.research_choice_document, "Préparer un document");
        assert_eq!(t_en.document_result_ready, "Ready");
        assert_eq!(t_fr.document_result_ready, "Prêt");
        assert_eq!(t_en.document_result_open, "Open");
        assert_eq!(t_fr.document_result_open, "Ouvrir");
        assert_eq!(t_en.document_progress_label, "Researching…");
        assert_eq!(t_fr.document_progress_label, "Recherche en cours…");
        assert_eq!(t_en.document_progress_stop, "Stop");
        assert_eq!(t_fr.document_progress_stop, "Arrêter");
        assert_eq!(t_en.document_prep_ack, "I'm preparing a document.");
        assert_eq!(t_fr.document_prep_ack, "Je prépare un document.");
        assert_eq!(t_en.document_open_failed, "Couldn't open this document.");
        assert_eq!(
            t_fr.document_open_failed,
            "Impossible d'ouvrir ce document."
        );
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
