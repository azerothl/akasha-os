//! Research document controller — choice cards, prep agents, and document index.

use crate::cmd::{ChatLine, Cmd};
use crate::os_open::aos_home;
use crate::research_ui_state::ResearchPendingChat;
use crate::{i18n, research_choice, research_document, UiApp};
use aos_proto::ChatAttachment;

impl UiApp {
    pub(crate) fn dispatch_pending_chat(&mut self, pending: ResearchPendingChat) {
        self.chat_state.session_chat.begin_turn(&pending.session_id);
        self.chat_state.runtime.begin_turn(None);
        self.chat_state.runtime.load_fail_retry = None;
        self.chat_state.runtime.outgoing_turn = Some(crate::cmd::ChatRetryTurn {
            session_id: pending.session_id.clone(),
            history: pending.history.clone(),
            user_text: pending.user_text.clone(),
            model_id: pending.model_id.clone(),
            images: pending.images.clone(),
            documents: pending.documents.clone(),
            auto_remember: pending.auto_remember,
            max_steps: pending.max_steps,
            routing: pending.routing.clone(),
            language: pending.language.clone(),
            canvas_open: pending.canvas_open,
            canvas_aspect: pending.canvas_aspect,
        });
        let t = i18n::strings(&pending.language);
        self.status = t.status_assistant_generating.into();
        let _ = self.cmd_tx.send(Cmd::Chat {
            session_id: pending.session_id,
            history: pending.history,
            user_text: pending.user_text,
            model_id: pending.model_id,
            images: pending.images,
            documents: pending.documents,
            auto_remember: pending.auto_remember,
            max_steps: pending.max_steps,
            routing: pending.routing,
            language: pending.language,
            canvas_open: pending.canvas_open,
            canvas_aspect: pending.canvas_aspect,
            skip_session_append: false,
        });
        self.mark_onboarding_chat_sent();
        self.scenario_ui.chat = true;
    }

    pub(crate) fn offer_research_choice(
        &mut self,
        session_id: &str,
        user_text: &str,
        pending: ResearchPendingChat,
    ) {
        let att = research_choice::choice_attachment(user_text, &pending.choice_id);
        self.chat.push(ChatLine {
            role: "assistant".into(),
            text: String::new(),
            attachments: vec![att.clone()],
            speaker_id: None,
            speaker_name: None,
            thinking: None,
        });
        let _ = self.cmd_tx.send(Cmd::SessionAppend {
            session_id: session_id.to_string(),
            role: "assistant".into(),
            content: String::new(),
            attachments: vec![att],
        });
        self.research_ui.set_pending(pending);
        self.mark_onboarding_chat_sent();
        self.scenario_ui.chat = true;
    }

    pub(crate) fn start_document_prep(&mut self, session_id: &str, pending: ResearchPendingChat) {
        let question = pending.user_text.clone();
        let t = i18n::strings(&pending.language);
        let ack = t.document_prep_ack;
        let att = research_document::progress_attachment(&question, "pending");
        self.chat.push(ChatLine {
            role: "assistant".into(),
            text: ack.into(),
            attachments: vec![att.clone()],
            speaker_id: None,
            speaker_name: None,
            thinking: None,
        });
        let _ = self.cmd_tx.send(Cmd::SessionAppend {
            session_id: session_id.to_string(),
            role: "assistant".into(),
            content: ack.into(),
            attachments: vec![att],
        });
        let _ = self.cmd_tx.send(Cmd::DocumentPrepSpawn {
            session_id: session_id.to_string(),
            question: pending.user_text,
            language: pending.language,
            model_id: pending.model_id,
            max_steps: pending.max_steps,
        });
        self.mark_onboarding_chat_sent();
        self.scenario_ui.chat = true;
    }

    pub(crate) fn attach_document_progress_agent(&mut self, agent_id: &str, question: &str) {
        let att = research_document::progress_attachment(question, agent_id);
        for line in &mut self.chat {
            let has_placeholder = line.attachments.iter().any(|a| {
                matches!(
                    a,
                    ChatAttachment::DocumentProgress {
                        agent_id: id,
                        ..
                    } if id == "pending"
                )
            });
            if has_placeholder {
                line.attachments.retain(|a| {
                    !matches!(
                        a,
                        ChatAttachment::DocumentProgress {
                            agent_id: id,
                            ..
                        } if id == "pending"
                    )
                });
                line.attachments.push(att.clone());
                return;
            }
        }
        self.chat.push(ChatLine {
            role: "assistant".into(),
            text: String::new(),
            attachments: vec![att],
            speaker_id: None,
            speaker_name: None,
            thinking: None,
        });
    }

    pub(crate) fn replace_progress_with_result(&mut self, question: &str, path: &str) {
        let label = research_choice::label_from_path(path);
        let result = research_choice::document_result_attachment(question, path, &label);
        let mut replaced = false;
        for line in &mut self.chat {
            if line
                .attachments
                .iter()
                .any(|a| matches!(a, ChatAttachment::DocumentProgress { .. }))
            {
                line.attachments
                    .retain(|a| !matches!(a, ChatAttachment::DocumentProgress { .. }));
                line.attachments.push(result.clone());
                replaced = true;
                break;
            }
        }
        if !replaced {
            self.chat.push(ChatLine {
                role: "assistant".into(),
                text: String::new(),
                attachments: vec![result.clone()],
                speaker_id: None,
                speaker_name: None,
                thinking: None,
            });
        }
        if let Some(sid) = self.chat_state.active_session.clone() {
            let _ = self.cmd_tx.send(Cmd::SessionAppend {
                session_id: sid,
                role: "assistant".into(),
                content: String::new(),
                attachments: vec![result],
            });
        }
    }

    pub(crate) fn record_prepared_document(&mut self, question: &str, path: &str, label: &str) {
        let home = aos_home();
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let _ = aos_agent::document_index::record_research_document(
            &home, question, path, label, ms,
        );
        self.research_ui.reload_documents();
    }

    pub(crate) fn resolve_research_choice_answer(&mut self, choice_id: &str, msg_idx: usize) {
        let Some(pending) = self.research_ui.take_pending_for_choice(choice_id) else {
            return;
        };
        if let Some(att) = self.chat[msg_idx].attachments.iter_mut().find_map(|a| {
            if let ChatAttachment::ResearchChoice {
                choice_id: id,
                state,
                ..
            } = a
            {
                if id == choice_id {
                    Some(state)
                } else {
                    None
                }
            } else {
                None
            }
        }) {
            *att = "answer".into();
        }
        self.dispatch_pending_chat(pending);
    }

    pub(crate) fn resolve_research_choice_document(
        &mut self,
        choice_id: &str,
        msg_idx: usize,
        session_id: &str,
    ) {
        let Some(pending) = self.research_ui.take_pending_for_choice(choice_id) else {
            return;
        };
        if let Some(att) = self.chat[msg_idx].attachments.iter_mut().find_map(|a| {
            if let ChatAttachment::ResearchChoice {
                choice_id: id,
                state,
                ..
            } = a
            {
                if id == choice_id {
                    Some(state)
                } else {
                    None
                }
            } else {
                None
            }
        }) {
            *att = "document".into();
        }
        let _ = self.cmd_tx.send(Cmd::SessionAppend {
            session_id: session_id.to_string(),
            role: "user".into(),
            content: pending.user_text.clone(),
            attachments: vec![],
        });
        self.start_document_prep(session_id, pending);
    }

    pub(crate) fn attach_document_result_card(&mut self, question: &str, path: &str) {
        let label = research_choice::label_from_path(path);
        self.replace_progress_with_result(question, path);
        self.record_prepared_document(question, path, &label);
    }
}
