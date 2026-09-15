//! Research choice + document result/progress cards in the chat thread.

use aos_proto::{AgentInfo, AgentTrace, ChatAttachment};
use eframe::egui;

use crate::i18n::{self, UiStrings};

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
    info: Option<&AgentInfo>,
    trace: Option<&AgentTrace>,
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
        ui.weak(t.document_progress_hint);
        if let Some(activity) = document_prep_activity_line(t, info, trace) {
            ui.label(egui::RichText::new(activity).italics());
        }
        for step in document_prep_recent_steps(t, trace, 4) {
            ui.weak(format!("· {step}"));
        }
    });
    action
}

/// Live line: current task, last tool, or step counter.
pub fn document_prep_activity_line(
    t: &UiStrings,
    info: Option<&AgentInfo>,
    trace: Option<&AgentTrace>,
) -> Option<String> {
    if let Some(task) = info
        .and_then(|a| a.current_task.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(truncate_activity(task, 96));
    }
    if let Some(action) = trace.and_then(last_tool_action) {
        if let Some(label) = i18n::tool_human_label(t, action) {
            return Some(format!("{label}…"));
        }
        let trimmed = action.trim();
        if !trimmed.is_empty() {
            return Some(truncate_activity(trimmed, 64));
        }
    }
    if let Some(a) = info {
        if a.max_steps > 0 && a.step > 0 {
            return Some(
                t.chat_pending_agent_step
                    .replace("{step}", &a.step.to_string())
                    .replace("{max}", &a.max_steps.to_string()),
            );
        }
    }
    None
}

fn document_prep_recent_steps(
    t: &UiStrings,
    trace: Option<&AgentTrace>,
    max: usize,
) -> Vec<String> {
    let Some(trace) = trace else {
        return Vec::new();
    };
    trace
        .steps
        .iter()
        .rev()
        .filter(|s| !s.action.trim().is_empty())
        .take(max)
        .map(|s| {
            i18n::tool_human_label(t, &s.action)
                .map(str::to_string)
                .unwrap_or_else(|| truncate_activity(s.action.trim(), 48))
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn last_tool_action(trace: &AgentTrace) -> Option<&str> {
    trace
        .steps
        .iter()
        .rev()
        .map(|s| s.action.as_str())
        .find(|a| !a.trim().is_empty())
}

fn truncate_activity(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    format!("{}…", s.chars().take(max).collect::<String>())
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
    use aos_proto::{AgentKind, AgentState, AgentStepRecord};

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
        assert_eq!(t_en.document_progress_label, "Writing your document…");
        assert_eq!(t_fr.document_progress_label, "Rédaction du document…");
        assert_eq!(
            t_en.document_progress_hint,
            "Gathering sources, then drafting the file."
        );
        assert_eq!(
            t_fr.document_progress_hint,
            "Je collecte des sources, puis je rédige le fichier."
        );
        assert_eq!(t_en.document_progress_stop, "Stop");
        assert_eq!(t_fr.document_progress_stop, "Arrêter");
        assert_eq!(t_en.document_prep_ack, "I'm writing a document for you.");
        assert_eq!(t_fr.document_prep_ack, "Je rédige un document pour toi.");
        assert_eq!(t_en.document_open_failed, "Couldn't open this document.");
        assert_eq!(
            t_fr.document_open_failed,
            "Impossible d'ouvrir ce document."
        );
    }

    #[test]
    fn activity_prefers_current_task_then_tool() {
        let t = crate::i18n::strings("en");
        let info = AgentInfo {
            agent_id: "a1".into(),
            state: AgentState::Running,
            directive: "doc".into(),
            pid: None,
            caps: vec![],
            last_output: String::new(),
            step: 2,
            max_steps: 20,
            current_task: Some("Scanning product memory".into()),
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: vec![],
            tools: vec![],
            mcp_servers: vec![],
            fail_reason: None,
            session_id: Some("s1".into()),
            model_id: None,
            title: "doc".into(),
            kind: AgentKind::Task,
            display_name: None,
            persona_id: None,
            source_roster_id: None,
            origin: Some("document".into()),
            avatar: None,
            color: None,
            deep_plan: None,
            cognitive_mode: Default::default(),
        };
        assert_eq!(
            document_prep_activity_line(&t, Some(&info), None).as_deref(),
            Some("Scanning product memory")
        );
        let mut info2 = info.clone();
        info2.current_task = None;
        let mut trace = AgentTrace {
            agent_id: "a1".into(),
            ..Default::default()
        };
        trace.steps.push(AgentStepRecord {
            action: "web.search".into(),
            ..Default::default()
        });
        let line = document_prep_activity_line(&t, Some(&info2), Some(&trace)).unwrap();
        assert!(line.to_ascii_lowercase().contains("search"), "{line}");
    }

    #[test]
    fn choice_attachment_pending_state() {
        match choice_attachment("What is SOTA?", "rc-1") {
            ChatAttachment::ResearchChoice {
                state, question, ..
            } => {
                assert_eq!(state, "pending");
                assert_eq!(question, "What is SOTA?");
            }
            _ => panic!("expected ResearchChoice"),
        }
    }

    #[test]
    fn document_result_keeps_question_and_path() {
        match document_result_attachment("Q?", "/downloads/a.md", "a.md") {
            ChatAttachment::DocumentResult {
                question,
                path,
                label,
            } => {
                assert_eq!(question, "Q?");
                assert_eq!(path, "/downloads/a.md");
                assert_eq!(label, "a.md");
            }
            _ => panic!("expected DocumentResult"),
        }
    }
}
