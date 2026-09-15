//! Chat submission controller and composer-to-runtime transitions.

use crate::chat_error_copy;
use crate::cmd::{ChatLine, ChatRetryTurn, Cmd};
use crate::research_ui_state::ResearchPendingChat;
use crate::{
    chat_agent_max_steps, chat_ask, chat_canvas, chat_room, chrono_like_stamp, i18n,
    local_tz_offset_minutes, models_page, now_ms, session_chat, session_model_supports_vision,
    UiApp,
};
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
        // S7.1 : envoyé = plus de brouillon pour cette session.
        if let Some(sid) = self.chat_state.active_session.clone() {
            if self.drafts.remove(&sid).is_some() {
                self.drafts_dirty = true;
            }
        }
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
        // A new user turn supersedes a pending one-click continuation.
        self.chat_state.runtime.continue_retry = None;
        if pending_images.is_empty() && pending_documents.is_empty() {
            let tz = local_tz_offset_minutes();
            if let Some(parsed) = schedule_parse::try_parse_phrase(&text, now_ms(), tz) {
                self.handle_schedule_phrase(&session_id, &text, parsed);
                return;
            }
            if let Some(cmd) = crate::deep_plan_ui::parse_deep_plan_command(&text) {
                self.handle_deep_plan_command(&session_id, &text, cmd);
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
            if chat_room::session_is_room(chat_room::active_session_meta(
                &self.chat_state.sessions,
                self.chat_state.active_session.as_deref(),
            )) && self.chat_state.session_chat.is_pending(&session_id)
            {
                if let Some((agent_id, title)) = chat_ask::open_ask_target(&self.chat) {
                    self.send_room_ask_reply(session_id, agent_id, title, text);
                    return;
                }
            }
        }
        if aos_agent::storage_path::text_contains_disallowed_storage_path(&text) {
            if let Some(msg) = chat_error_copy::room_host_path_disallowed_toast(&t) {
                self.toasts.push_error(msg);
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
                self.chat.push(ChatLine::plain("user", text.clone()));
                let notice = t.chat_previous_in_progress.to_string();
                crate::chat_event_controller::push_persisted_system_chrome(self, notice);
                if let Some(sid) = self.chat_state.active_session.clone() {
                    let _ = self.cmd_tx.send(Cmd::SessionAppend {
                        session_id: sid,
                        role: "user".into(),
                        content: text,
                        attachments: vec![],
                    });
                }
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
            ..Default::default()
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
            // S2 : prompt facturable (si modèle provider non-loopback).
            self.note_provider_prompt(&session_id, &text);
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
        if pending_images.is_empty() && pending_documents.is_empty() {
            let wants_document = aos_agent::research_detect::user_requested_document(&text);
            let research_shaped = aos_agent::research_detect::is_research_shaped_ask(&text);
            if wants_document || research_shaped {
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
                let choice_id = format!("research-choice-{}", chrono_like_stamp());
                let pending = ResearchPendingChat {
                    session_id: session_id.clone(),
                    history,
                    user_text: text.clone(),
                    model_id: model_id.clone(),
                    images: pending_images,
                    documents: pending_documents,
                    auto_remember: self.prefs.auto_remember_chat,
                    instincts_in_session: self.prefs.instincts_in_session,
                    max_steps: chat_agent_max_steps(self.prefs.default_max_steps),
                    routing: self.prefs.routing.clone(),
                    language: self.prefs.language.clone(),
                    canvas_open,
                    canvas_aspect,
                    deep_thinking: self.chat_state.composer.deep_thinking,
                    choice_id,
                };
                // Explicit document ask → prep agent (fichier /downloads/), même hors
                // question « research-shaped ».
                if wants_document {
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
                self.offer_research_choice(&session_id, &text, pending);
                return;
            }
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
        // S2 : prompt facturable (si modèle provider non-loopback).
        self.note_provider_prompt(&session_id, &text);
        self.chat_state.runtime.load_fail_retry = None;
        let retry_turn = ChatRetryTurn {
            session_id: session_id.clone(),
            history: history.clone(),
            user_text: text.clone(),
            model_id: model_id.clone(),
            images: pending_images.clone(),
            documents: pending_documents.clone(),
            auto_remember: self.prefs.auto_remember_chat,
            instincts_in_session: self.prefs.instincts_in_session,
            max_steps: chat_agent_max_steps(self.prefs.default_max_steps),
            routing: self.prefs.routing.clone(),
            language: self.prefs.language.clone(),
            canvas_open,
            canvas_aspect,
            deep_thinking: self.chat_state.composer.deep_thinking,
        };
        self.chat_state.runtime.outgoing_turn = Some(retry_turn);
        self.status = t.status_assistant_generating.into();
        let _ = self.cmd_tx.send(Cmd::Chat {
            session_id,
            history,
            user_text: text,
            model_id,
            images: pending_images,
            documents: pending_documents,
            auto_remember: self.prefs.auto_remember_chat,
            instincts_in_session: self.prefs.instincts_in_session,
            max_steps: chat_agent_max_steps(self.prefs.default_max_steps),
            routing: self.prefs.routing.clone(),
            language: self.prefs.language.clone(),
            canvas_open,
            canvas_aspect,
            deep_thinking: self.chat_state.composer.deep_thinking,
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
            profile: self.prefs.placement_profile.clone(),
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
            instincts_in_session: retry.instincts_in_session,
            max_steps: retry.max_steps,
            routing: retry.routing.clone(),
            language: retry.language.clone(),
            canvas_open: retry.canvas_open,
            canvas_aspect: retry.canvas_aspect,
            deep_thinking: retry.deep_thinking,
        });
        let t = i18n::strings(&retry.language);
        self.status = t.status_assistant_generating.into();
        let _ = self.cmd_tx.send(retry.to_chat_cmd(true));
        self.scenario_ui.chat = true;
    }

    /// Resume a response that stopped after emitting some text. The partial
    /// answer is already part of the transcript; the hidden continuation
    /// instruction keeps the composer and persisted conversation clean.
    pub(crate) fn continue_partial_turn(&mut self) {
        let Some(retry) = self.chat_state.runtime.continue_retry.take() else {
            return;
        };
        if self.chat_state.active_session.as_deref() != Some(retry.session_id.as_str()) {
            self.chat_state.runtime.continue_retry = Some(retry);
            return;
        }
        self.chat_state.session_chat.begin_turn(&retry.session_id);
        self.chat_state.runtime.begin_turn(None);
        self.chat_state.runtime.outgoing_turn = Some(retry.clone());
        let t = i18n::strings(&retry.language);
        self.status = t.status_assistant_generating.into();
        let _ = self.cmd_tx.send(retry.to_chat_cmd(true));
        self.scenario_ui.chat = true;
    }

    pub(crate) fn offer_partial_continuation(
        &mut self,
        retry: ChatRetryTurn,
        partial: String,
    ) {
        self.set_partial_continuation(retry, partial, true);
    }

    /// Arm recovery when the cancellation handler has already placed the
    /// partial answer in the visible transcript.
    pub(crate) fn arm_partial_continuation(
        &mut self,
        retry: ChatRetryTurn,
        partial: String,
    ) {
        self.set_partial_continuation(retry, partial, false);
    }

    fn set_partial_continuation(
        &mut self,
        mut retry: ChatRetryTurn,
        partial: String,
        append_to_chat: bool,
    ) {
        if partial.trim().is_empty() {
            return;
        }
        let display = crate::agent_panel::format_chat_assistant_display(
            &partial,
            &i18n::strings(&self.prefs.language),
        );
        if display.trim().is_empty() {
            return;
        }
        let t = i18n::strings(&self.prefs.language);
        if append_to_chat {
            self.chat.push(ChatLine::plain("assistant", display.clone()));
        }
        let _ = self.cmd_tx.send(Cmd::SessionAppend {
            session_id: retry.session_id.clone(),
            role: "assistant".into(),
            content: display,
            attachments: vec![],
        });
        retry.history.push(("assistant".into(), partial));
        let instruction = if retry.language.eq_ignore_ascii_case("fr") {
            "Continue la réponse exactement là où elle s’est interrompue, sans répéter le texte déjà produit.".to_string()
        } else {
            "Continue the answer exactly where it stopped, without repeating the text already produced.".to_string()
        };
        retry.history.push(("user".into(), instruction.clone()));
        retry.user_text = instruction;
        self.chat_state.runtime.continue_retry = Some(retry);
        self.status = t.chat_continue_partial.into();
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
        let partial = self.chat_state.runtime.streaming.clone();
        let retry = self.chat_state.runtime.outgoing_turn.take();
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
            self.chat_state.runtime.load_fail_retry = None;
            self.chat_state.runtime.room_turn_text = None;
            let t = i18n::strings(&self.prefs.language);
            self.status = t.chat_stopped.into();
            if let Some(retry) = retry {
                self.arm_partial_continuation(retry, partial);
            }
        }
    }

    fn handle_deep_plan_command(
        &mut self,
        session_id: &str,
        user_text: &str,
        cmd: crate::deep_plan_ui::DeepPlanCommand,
    ) {
        self.chat.push(ChatLine::plain("user", user_text));
        let _ = self.cmd_tx.send(Cmd::SessionAppend {
            session_id: session_id.to_string(),
            role: "user".into(),
            content: user_text.to_string(),
            attachments: vec![],
        });
        // Prefer latest DeepPlan attachment in transcript; else build from AgentInfo.
        let mut applied = false;
        let mut open_idx = None;
        let chat_len = self.chat.len();
        for (idx, line) in self.chat.iter_mut().enumerate().rev() {
            for att in line.attachments.iter_mut() {
                if crate::deep_plan_ui::apply_command_to_attachment(att, &cmd) {
                    applied = true;
                    open_idx = Some(idx.min(chat_len.saturating_sub(1)));
                    break;
                }
            }
            if applied {
                break;
            }
        }
        if let Some(idx) = open_idx {
            self.chat_state.view.deep_plan_open.insert(idx);
        }
        if !applied {
            if let Some(info) = self
                .agents
                .iter()
                .find(|a| a.session_id.as_deref() == Some(session_id) && a.deep_plan.is_some())
            {
                if let Some(plan) = info.deep_plan.clone() {
                    let mut att = ChatAttachment::DeepPlan {
                        agent_id: info.agent_id.clone(),
                        plan_id: plan.id.clone(),
                        title: plan.title.clone(),
                        version: plan.version,
                        steps: plan.steps.clone(),
                        expand_step_ids: vec![],
                        show_logs_step_id: None,
                    };
                    crate::deep_plan_ui::apply_command_to_attachment(&mut att, &cmd);
                    let reply = match &cmd {
                        crate::deep_plan_ui::DeepPlanCommand::ShowFull => {
                            "Plan Deep Thinking affiché.".to_string()
                        }
                        crate::deep_plan_ui::DeepPlanCommand::ExpandStep { step_id } => {
                            format!("Étape {step_id} dépliée.")
                        }
                        crate::deep_plan_ui::DeepPlanCommand::ShowLogs { step_id } => {
                            format!("Logs internes de l'étape {step_id}.")
                        }
                    };
                    let idx = self.chat.len();
                    self.chat.push(ChatLine {
                        role: "assistant".into(),
                        text: reply.clone(),
                        attachments: vec![att],
                        speaker_id: None,
                        speaker_name: None,
                        thinking: None,
                        ..Default::default()
                    });
                    self.chat_state.view.deep_plan_open.insert(idx);
                    let _ = self.cmd_tx.send(Cmd::SessionAppend {
                        session_id: session_id.to_string(),
                        role: "assistant".into(),
                        content: reply,
                        attachments: self
                            .chat
                            .last()
                            .map(|l| l.attachments.clone())
                            .unwrap_or_default(),
                    });
                    return;
                }
            }
            self.chat.push(ChatLine::plain(
                "système",
                "Aucun plan Deep Thinking dans cette session.",
            ));
            return;
        }
        let reply = match &cmd {
            crate::deep_plan_ui::DeepPlanCommand::ShowFull => {
                "Plan Deep Thinking déplié.".to_string()
            }
            crate::deep_plan_ui::DeepPlanCommand::ExpandStep { step_id } => {
                format!("Étape {step_id} dépliée.")
            }
            crate::deep_plan_ui::DeepPlanCommand::ShowLogs { step_id } => {
                format!("Logs internes de l'étape {step_id}.")
            }
        };
        self.chat.push(ChatLine::plain("assistant", reply));
    }
}
