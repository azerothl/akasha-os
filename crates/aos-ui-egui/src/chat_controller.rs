//! Chat submission controller and composer-to-runtime transitions.

use crate::cmd::{ChatLine, ChatRetryTurn, Cmd};
use crate::{
    chat_agent_max_steps, chat_canvas, chat_room, chrono_like_stamp, i18n, local_tz_offset_minutes,
    models_page, now_ms, session_chat, session_model_supports_vision, UiApp,
};
use crate::research_ui_state::ResearchPendingChat;
use aos_agent::schedule_parse;
use aos_proto::{chat_tts_request, ChatAttachment};

impl UiApp {
    pub(crate) fn send_chat(&mut self) {
        let text = self.chat_state.composer.input.trim().to_string();
        let pending_images = self.chat_state.composer.pending_images.clone();
        let pending_documents = self.chat_state.composer.pending_documents.clone();
        if text.is_empty() && pending_images.is_empty() && pending_documents.is_empty() {
            return;
        }
        let t = i18n::strings(&self.prefs.language);
        let text = if text.is_empty() && !pending_images.is_empty() {
            t.chat_empty_image_prompt.to_string()
        } else {
            text
        };
        self.chat_state.composer.input.clear();
        self.chat_state.composer.refocus = true;
        if text.starts_with('/') && pending_images.is_empty() && pending_documents.is_empty() {
            self.handle_slash(&text);
            return;
        }
        let Some(session_id) = self.chat_state.active_session.clone() else {
            self.chat.push(ChatLine::plain(
                "système",
                "aucune session — créez-en une dans le panneau Sessions",
            ));
            return;
        };
        if pending_images.is_empty() && pending_documents.is_empty() {
            let tz = local_tz_offset_minutes();
            if let Some(parsed) = schedule_parse::try_parse_phrase(&text, now_ms(), tz) {
                self.handle_schedule_phrase(&session_id, &text, parsed);
                return;
            }
        }
        let explicit_canvas = chat_canvas::chat_should_open_canvas_face(&text);
        if explicit_canvas {
            self.break_stuck_session_agents(&session_id);
            self.open_canvas_face(&session_id);
        }
        if !explicit_canvas {
            if let Some((agent_id, title)) = self
                .blocked_ask_agent()
                .map(|ag| (ag.agent_id.clone(), ag.directive.clone()))
            {
                self.send_ask_reply(session_id, agent_id, title, text);
                return;
            }
        }
        if self
            .chat_state
            .active_session
            .as_deref()
            .is_some_and(|sid| self.chat_state.session_chat.is_pending(sid))
        {
            if self.chat_state.active_session.as_deref() == Some(session_id.as_str())
                && !self.chat_state.runtime.pending
            {
                self.chat_state.session_chat.finish_turn(&session_id);
            } else {
                self.chat.push(ChatLine::plain("user", text));
                self.chat.push(ChatLine::plain(
                    "système",
                    t.chat_previous_in_progress,
                ));
                return;
            }
        }
        if let Some(spoken) = chat_tts_request(&text) {
            self.chat.push(ChatLine::plain("user", text.clone()));
            if let Some(sid) = self.chat_state.active_session.clone() {
                let _ = self.cmd_tx.send(Cmd::SessionAppend {
                    session_id: sid,
                    role: "user".into(),
                    content: text,
                    attachments: vec![],
                });
            }
            if spoken.trim().is_empty() {
                self.chat.push(ChatLine::plain(
                    "système",
                    "usage : /speak <texte> — indiquez le texte à lire.",
                ));
                return;
            }
            self.open_tts_card(&spoken);
            return;
        }
        if !pending_images.is_empty() {
            let model_id = self
                .chat_state
                .sessions
                .iter()
                .find(|s| s.id == session_id)
                .and_then(|s| s.model_id.clone());
            if !session_model_supports_vision(model_id.as_deref()) {
                self.chat.push(ChatLine::plain(
                    "système",
                    i18n::strings(&self.prefs.language).chat_vision_banner,
                ));
                return;
            }
        }
        let image_atts: Vec<ChatAttachment> = pending_images
            .iter()
            .map(|path| ChatAttachment::Image {
                path: path.clone(),
                prompt: String::new(),
            })
            .collect();
        let doc_atts: Vec<ChatAttachment> = pending_documents
            .iter()
            .map(|doc| ChatAttachment::Document {
                path: doc.path.clone(),
                label: doc.label.clone(),
            })
            .collect();
        let mut attachments = image_atts;
        attachments.extend(doc_atts);
        self.chat.push(ChatLine {
            role: "user".into(),
            text: text.clone(),
            attachments,
            speaker_id: None,
            speaker_name: None,
            thinking: None,
        });
        self.chat_state.composer.clear_attachments();
        let room_content =
            aos_proto::chat_document::merge_documents_into_user_content(&text, &pending_documents);
        if chat_room::session_is_room(chat_room::active_session_meta(
            &self.chat_state.sessions,
            self.chat_state.active_session.as_deref(),
        )) {
            let Some(session_id) = self.chat_state.active_session.clone() else {
                return;
            };
            self.chat_state.session_chat.begin_turn(&session_id);
            self.chat_state.runtime.begin_turn(Some(text.clone()));
            let _ = self.cmd_tx.send(Cmd::RoomTurn {
                session_id,
                content: room_content,
                images: pending_images,
            });
            self.mark_onboarding_chat_sent();
            self.scenario_ui.chat = true;
            return;
        }
        let model_id = self
            .chat_state
            .sessions
            .iter()
            .find(|s| s.id == session_id)
            .and_then(|s| s.model_id.clone());
        let canvas_open = self
            .chat_state
            .sessions
            .iter()
            .find(|s| s.id == session_id)
            .map(|s| s.canvas_open)
            .unwrap_or(false)
            || (self.chat_state.active_session.as_deref() == Some(session_id.as_str())
                && (!self.chat_state.view.canvas.ops.is_empty()
                    || self.chat_state.view.canvas.next_seq > 1));
        let canvas_aspect = self
            .chat_state
            .sessions
            .iter()
            .find(|s| s.id == session_id)
            .map(|s| s.canvas_aspect)
            .unwrap_or_default();
        if pending_images.is_empty()
            && pending_documents.is_empty()
            && aos_agent::research_detect::is_research_shaped_ask(&text)
        {
            let history: Vec<(String, String)> = self
                .chat
                .iter()
                .filter(|l| l.role == "user" || l.role == "vous" || l.role == "assistant")
                .map(|l| {
                    (
                        if l.role == "vous" || l.role == "user" {
                            "user".into()
                        } else {
                            "assistant".into()
                        },
                        l.text.clone(),
                    )
                })
                .collect();
            if aos_agent::research_detect::user_requested_document(&text) {
                let choice_id = format!("research-choice-{}", chrono_like_stamp());
                let pending = ResearchPendingChat {
                    session_id: session_id.clone(),
                    history,
                    user_text: text.clone(),
                    model_id: model_id.clone(),
                    images: pending_images,
                    documents: pending_documents,
                    auto_remember: self.prefs.auto_remember_chat,
                    max_steps: chat_agent_max_steps(self.prefs.default_max_steps),
                    routing: self.prefs.routing.clone(),
                    language: self.prefs.language.clone(),
                    canvas_open,
                    canvas_aspect,
                    choice_id,
                };
                let _ = self.cmd_tx.send(Cmd::SessionAppend {
                    session_id: session_id.clone(),
                    role: "user".into(),
                    content: text.clone(),
                    attachments: self
                        .chat
                        .last()
                        .map(|l| l.attachments.clone())
                        .unwrap_or_default(),
                });
                self.start_document_prep(session_id.as_str(), pending);
                return;
            }
            let choice_id = format!("research-choice-{}", chrono_like_stamp());
            let pending = ResearchPendingChat {
                session_id: session_id.clone(),
                history,
                user_text: text.clone(),
                model_id: model_id.clone(),
                images: pending_images,
                documents: pending_documents,
                auto_remember: self.prefs.auto_remember_chat,
                max_steps: chat_agent_max_steps(self.prefs.default_max_steps),
                routing: self.prefs.routing.clone(),
                language: self.prefs.language.clone(),
                canvas_open,
                canvas_aspect,
                choice_id,
            };
            self.offer_research_choice(&session_id, &text, pending);
            return;
        }
        let history: Vec<(String, String)> = self
            .chat
            .iter()
            .filter(|l| l.role == "user" || l.role == "vous" || l.role == "assistant")
            .map(|l| {
                (
                    if l.role == "vous" || l.role == "user" {
                        "user".into()
                    } else {
                        "assistant".into()
                    },
                    l.text.clone(),
                )
            })
            .collect();
        self.chat_state.session_chat.begin_turn(&session_id);
        self.chat_state.runtime.begin_turn(None);
        self.chat_state.runtime.load_fail_retry = None;
        let retry_turn = ChatRetryTurn {
            session_id: session_id.clone(),
            history: history.clone(),
            user_text: text.clone(),
            model_id: model_id.clone(),
            images: pending_images.clone(),
            documents: pending_documents.clone(),
            auto_remember: self.prefs.auto_remember_chat,
            max_steps: chat_agent_max_steps(self.prefs.default_max_steps),
            routing: self.prefs.routing.clone(),
            language: self.prefs.language.clone(),
            canvas_open,
            canvas_aspect,
        };
        self.chat_state.runtime.outgoing_turn = Some(retry_turn);
        self.status = "assistant : génération…".into();
        let _ = self.cmd_tx.send(Cmd::Chat {
            session_id,
            history,
            user_text: text,
            model_id,
            images: pending_images,
            documents: pending_documents,
            auto_remember: self.prefs.auto_remember_chat,
            max_steps: chat_agent_max_steps(self.prefs.default_max_steps),
            routing: self.prefs.routing.clone(),
            language: self.prefs.language.clone(),
            canvas_open,
            canvas_aspect,
            skip_session_append: false,
        });
        self.mark_onboarding_chat_sent();
        self.scenario_ui.chat = true;
    }

    pub(crate) fn queue_chat_document(&mut self, path: String) {
        if !self.chat_state.composer.queue_document(path) {
            return;
        }
        self.status = i18n::strings(&self.prefs.language)
            .chat_attach_document
            .to_string();
    }

    pub(crate) fn queue_chat_image(&mut self, path: String) {
        if !self.chat_state.composer.queue_image(path) {
            return;
        }
        self.status = i18n::strings(&self.prefs.language)
            .chat_attach_image
            .to_string();
    }

    pub(crate) fn load_preferred_vision_model(&mut self) {
        let Some(model_id) = models_page::first_catalog_vision_model_id() else {
            self.status = i18n::strings(&self.prefs.language)
                .chat_vision_banner
                .to_string();
            return;
        };
        if let Some(sid) = self.chat_state.active_session.clone() {
            let _ = self.cmd_tx.send(Cmd::SessionSetModel {
                session_id: sid,
                model_id: Some(model_id.clone()),
            });
        }
        let _ = self.cmd_tx.send(Cmd::ModelLoad {
            model_id: model_id.clone(),
        });
        self.status = format!("vision: {model_id}");
    }

    pub(crate) fn retry_load_failed_turn(&mut self) {
        let Some(retry) = self.chat_state.runtime.load_fail_retry.take() else {
            return;
        };
        if self.chat_state.active_session.as_deref() != Some(retry.session_id.as_str()) {
            self.chat_state.runtime.load_fail_retry = Some(retry);
            return;
        }
        self.chat_state.session_chat.begin_turn(&retry.session_id);
        self.chat_state.runtime.begin_turn(None);
        self.chat_state.runtime.outgoing_turn = Some(ChatRetryTurn {
            session_id: retry.session_id.clone(),
            history: retry.history.clone(),
            user_text: retry.user_text.clone(),
            model_id: retry.model_id.clone(),
            images: retry.images.clone(),
            documents: retry.documents.clone(),
            auto_remember: retry.auto_remember,
            max_steps: retry.max_steps,
            routing: retry.routing.clone(),
            language: retry.language.clone(),
            canvas_open: retry.canvas_open,
            canvas_aspect: retry.canvas_aspect,
        });
        self.status = "assistant : génération…".into();
        let _ = self.cmd_tx.send(retry.to_chat_cmd(true));
        self.scenario_ui.chat = true;
    }

    pub(crate) fn cancel_pending_turn(&mut self) {
        let Some(session_id) = self.chat_state.active_session.clone() else {
            return;
        };
        if let Some(id) = self.chat_state.runtime.inference_id {
            let _ = self.cmd_tx.send(Cmd::ChatCancel {
                inference_id: id,
                session_id,
            });
            return;
        }
        if !self.chat_state.runtime.pending {
            return;
        }
        let on_active = session_chat::on_chat_cancelled(
            &mut self.chat_state.session_chat,
            self.chat_state.active_session.as_deref(),
            &session_id,
            &mut self.chat_state.runtime.streaming,
            &mut self.chat_state.runtime.pending,
            &mut self.chat_state.runtime.inference_id,
            &mut self.chat,
        );
        if on_active {
            self.chat_state.runtime.outgoing_turn = None;
            self.chat_state.runtime.load_fail_retry = None;
            self.chat_state.runtime.room_turn_text = None;
            let t = i18n::strings(&self.prefs.language);
            self.status = t.chat_stopped.into();
        }
    }
}
