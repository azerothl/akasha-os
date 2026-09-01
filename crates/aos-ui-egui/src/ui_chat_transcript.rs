//! Conversation transcript, message cards, and attachment rendering.

use crate::chat_bubble::{
    chat_bubble_colors, chat_bubble_kind, chat_markdown_viewer, chat_message_frame,
    chat_role_label, ChatBubbleKind,
};
use crate::cmd::Cmd;
use crate::{
    agent_act_phrase, agent_canvas_session_ops, agent_panel, chat_media, chat_room, i18n,
    local_tz_offset_minutes, now_ms, research_choice, research_document, schedule_card,
    skill_offer, Tab, UiApp,
};
use aos_proto::{ChatAttachment, ChatRoomMember};
use eframe::egui;

impl UiApp {
    pub(crate) fn ui_chat_transcript(
        &mut self,
        ui: &mut egui::Ui,
        t: &i18n::UiStrings,
        room_mode: bool,
        room_members: &[ChatRoomMember],
        room_conductor_policy: Option<&aos_proto::ChatRoomConductorPolicy>,
        scroll_h: f32,
    ) {
        egui::ScrollArea::vertical()
            .id_salt("conversation_scroll")
            .auto_shrink([false, false])
            .max_height(scroll_h)
            .min_scrolled_height(scroll_h)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.set_min_height(scroll_h);
                let mut open_agent: Option<String> = None;
                let mut target_reply: Option<String> = None;
                let mut open_studio: Option<(String, String)> = None;
                let mut act_decision: Option<(String, String, bool)> = None;
                let mut research_choice_pick: Option<(
                    String,
                    usize,
                    research_choice::ResearchChoiceAction,
                )> = None;
                let mut document_progress_action: Option<(
                    usize,
                    research_choice::DocumentProgressAction,
                )> = None;
                let mut document_result_open: Option<(String, String)> = None;
                let mut schedule_act: Option<(String, usize, bool)> = None;
                let tz_offset = local_tz_offset_minutes();
                let chat_now = now_ms();
                let reply_id = self.blocked_ask_agent().map(|a| a.agent_id.clone());
                let n = self.chat.len();
                for i in 0..n {
                    let role = self.chat[i].role.clone();
                    let mut text = self.chat[i].text.clone();
                    let attachments = self.chat[i].attachments.clone();
                    let speaker_id = self.chat[i].speaker_id.clone();
                    let speaker_name = self.chat[i].speaker_name.clone();
                    let thinking = self.chat[i].thinking.clone();
                    let is_completion = attachments.iter().any(|a| {
                        matches!(
                            a,
                            ChatAttachment::AgentRef { origin, .. } if origin == "completion"
                        )
                    });
                    if let Some(act_text) = attachments
                        .iter()
                        .find_map(|a| agent_act_phrase::thread_display_from_attachment(t, a))
                    {
                        text = act_text;
                    } else if role == "assistant"
                        && text.trim() == aos_agent::actions::THREAD_FAIL_COULD_NOT_ACT
                    {
                        text = i18n::agent_could_not_act_message(t);
                    } else if role == "assistant"
                        && (text.trim() == aos_agent::actions::THREAD_FAIL_COULD_NOT_CONTINUE
                            || aos_agent::context_budget::is_overflow_fail_reason(text.trim()))
                    {
                        text = i18n::agent_could_not_continue_message(t);
                    }
                    let kind = chat_bubble_kind(&role, speaker_id.as_deref(), room_mode);
                    let text = if kind == ChatBubbleKind::RoomSpeaker {
                        let visible = chat_room::format_room_visible_bubble(&text);
                        chat_room::strip_roster_agent_id_mentions(t, &visible, room_members)
                    } else if role == "assistant" && !is_completion && speaker_id.is_none() {
                        agent_panel::format_chat_assistant_display(&text)
                    } else {
                        text
                    };
                    let mut shown_role = chat_role_label(kind, t, &role);
                    if kind == ChatBubbleKind::RoomSpeaker {
                        if let Some(sid) = speaker_id.as_deref() {
                            shown_role = chat_room::roster_display_name(
                                t,
                                room_members,
                                sid,
                                speaker_name.as_deref(),
                            );
                        }
                    }
                    let (fill, stroke, role_color) = if kind == ChatBubbleKind::RoomSpeaker {
                        if let Some(sid) = speaker_id.as_deref() {
                            let (r, g, b) =
                                chat_room::speaker_color_rgb(sid, ui.visuals().dark_mode);
                            let c = egui::Color32::from_rgb(r, g, b);
                            (c.gamma_multiply(0.25), c, c)
                        } else {
                            chat_bubble_colors(kind, ui.visuals().dark_mode)
                        }
                    } else {
                        chat_bubble_colors(kind, ui.visuals().dark_mode)
                    };
                    let frame_colors = if kind == ChatBubbleKind::RoomSpeaker {
                        Some((fill, stroke))
                    } else {
                        None
                    };
                    chat_message_frame(ui, kind, frame_colors, |ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(
                                role_color,
                                egui::RichText::new(&shown_role).strong().small(),
                            );
                            if ui.small_button(t.btn_copy).clicked() {
                                ui.ctx().copy_text(text.clone());
                                self.status = t.copied.into();
                            }
                        });
                        if kind == ChatBubbleKind::RoomSpeaker {
                            if let Some(th) = thinking.as_deref().filter(|s| !s.trim().is_empty()) {
                                chat_room::room_thinking_toggle(
                                    ui,
                                    t,
                                    i,
                                    th,
                                    &mut self.chat_view.room_thinking_open,
                                );
                            }
                        }
                        if !text.is_empty() {
                            if role == "assistant" {
                                ui.push_id(("chat_md", i), |ui| {
                                    chat_markdown_viewer(ui).show(
                                        ui,
                                        &mut self.chat_md_cache,
                                        &text,
                                    );
                                });
                            } else {
                                ui.add(egui::Label::new(&text).wrap());
                            }
                        }
                        for (j, att) in attachments.iter().enumerate() {
                            match att {
                                ChatAttachment::AgentRef {
                                    agent_id,
                                    title,
                                    origin,
                                } => {
                                    if origin == "room" || origin == "document" {
                                        continue;
                                    }
                                    let info = self.agents.iter().find(|a| a.agent_id == *agent_id);
                                    let selected = reply_id.as_deref() == Some(agent_id.as_str());
                                    let session_ops = info.and_then(|ag| {
                                        agent_canvas_session_ops(
                                            ag,
                                            self.active_session.as_deref(),
                                            &self.chat_view.canvas.ops,
                                        )
                                    });
                                    let trace = self.agent_traces.get(agent_id);
                                    let action = ui
                                        .push_id(
                                            ("chat_agent_card", i, j, agent_id.as_str()),
                                            |ui| {
                                                agent_panel::chat_agent_card(
                                                    ui,
                                                    info,
                                                    agent_id.as_str(),
                                                    title.as_str(),
                                                    origin.as_str(),
                                                    selected && origin == "ask",
                                                    session_ops,
                                                    trace,
                                                    t,
                                                )
                                            },
                                        )
                                        .inner;
                                    match action {
                                        agent_panel::ChatCardAction::OpenDetail => {
                                            open_agent = Some(agent_id.clone());
                                        }
                                        agent_panel::ChatCardAction::TargetReply => {
                                            target_reply = Some(agent_id.clone());
                                        }
                                        agent_panel::ChatCardAction::Export => {
                                            let _ = self.cmd_tx.send(Cmd::AgentExport {
                                                id: agent_id.clone(),
                                            });
                                        }
                                        agent_panel::ChatCardAction::Retry => {
                                            let _ = self.cmd_tx.send(Cmd::AgentRetry {
                                                id: agent_id.clone(),
                                            });
                                        }
                                        agent_panel::ChatCardAction::Continue => {
                                            let _ = self.cmd_tx.send(Cmd::AgentRetry {
                                                id: agent_id.clone(),
                                            });
                                        }
                                        agent_panel::ChatCardAction::None => {}
                                    }
                                }
                                ChatAttachment::Image { path, prompt } => {
                                    chat_media::render_image(
                                        ui,
                                        t,
                                        path.as_str(),
                                        prompt.as_str(),
                                        || {
                                            open_studio = Some((prompt.clone(), path.clone()));
                                        },
                                    );
                                }
                                ChatAttachment::Audio { path } => {
                                    chat_media::render_audio(ui, path.as_str());
                                }
                                ChatAttachment::Document { path, label } => {
                                    chat_media::render_document(
                                        ui,
                                        t,
                                        label.as_str(),
                                        path.as_str(),
                                    );
                                }
                                ChatAttachment::TtsDraft { .. } => {
                                    let piper: Vec<String> = self
                                        .model_infos
                                        .iter()
                                        .filter(|m| m.id.contains("piper"))
                                        .map(|m| m.id.clone())
                                        .collect();
                                    if chat_media::render_tts_card(
                                        ui,
                                        t,
                                        &self.cmd_tx,
                                        &mut self.chat[i].attachments[j],
                                        &piper,
                                    ) {
                                        self.status = "audio : génération…".into();
                                    }
                                }
                                ChatAttachment::AgentAct {
                                    agent_id,
                                    act_id,
                                    state,
                                    ..
                                } => {
                                    if state == "pending" {
                                        ui.horizontal(|ui| {
                                            if ui.button(t.agent_act_allow_once).clicked() {
                                                act_decision =
                                                    Some((agent_id.clone(), act_id.clone(), true));
                                            }
                                            if ui.button(t.agent_act_deny).clicked() {
                                                act_decision =
                                                    Some((agent_id.clone(), act_id.clone(), false));
                                            }
                                        });
                                    }
                                }
                                ChatAttachment::SkillOffer { .. } => {
                                    skill_offer::render_skill_offer_card(
                                        ui,
                                        t,
                                        &self.prefs.language,
                                        &self.cmd_tx,
                                        &mut self.chat[i].attachments[j],
                                    );
                                }
                                ChatAttachment::ResearchChoice {
                                    choice_id, state, ..
                                } => {
                                    let action =
                                        research_choice::render_research_choice(ui, t, state);
                                    if action != research_choice::ResearchChoiceAction::None {
                                        research_choice_pick = Some((choice_id.clone(), i, action));
                                    }
                                }
                                ChatAttachment::DocumentProgress {
                                    question,
                                    agent_id,
                                    state,
                                } => {
                                    let action = research_choice::render_document_progress(
                                        ui, t, question, agent_id, state,
                                    );
                                    if action != research_choice::DocumentProgressAction::None {
                                        document_progress_action = Some((i, action));
                                    }
                                }
                                ChatAttachment::DocumentResult { question, path, .. } => {
                                    if research_choice::render_document_result(ui, t, question)
                                        == research_choice::DocumentResultAction::Open
                                    {
                                        document_result_open =
                                            Some((question.clone(), path.clone()));
                                    }
                                }
                                ChatAttachment::ScheduleAct { act_id, state, .. } => {
                                    if state == "pending" {
                                        ui.horizontal(|ui| {
                                            if ui.button(t.agent_act_allow_once).clicked() {
                                                schedule_act = Some((act_id.clone(), i, true));
                                            }
                                            if ui.button(t.agent_act_deny).clicked() {
                                                schedule_act = Some((act_id.clone(), i, false));
                                            }
                                        });
                                    }
                                }
                                ChatAttachment::ScheduleCard {
                                    schedule_id,
                                    title,
                                    state,
                                    next_fire_ms,
                                    ..
                                } => {
                                    let entry =
                                        self.schedules.iter().find(|s| s.id == *schedule_id);
                                    let display_state =
                                        schedule_card::resolved_card_state(entry, state);
                                    let display_next = entry
                                        .map(|e| schedule_card::next_fire_ms_for_entry(e, chat_now))
                                        .unwrap_or(*next_fire_ms);
                                    let action = ui
                                        .push_id(
                                            ("chat_schedule_card", i, j, schedule_id.as_str()),
                                            |ui| {
                                                schedule_card::render_schedule_card(
                                                    ui,
                                                    t,
                                                    title,
                                                    display_state,
                                                    display_next,
                                                    schedule_id,
                                                    chat_now,
                                                    tz_offset,
                                                )
                                            },
                                        )
                                        .inner;
                                    if !matches!(action, schedule_card::ScheduleCardAction::None) {
                                        self.apply_schedule_card_action_local(action.clone());
                                    }
                                    schedule_card::send_schedule_action(&self.cmd_tx, action);
                                }
                            }
                        }
                    });
                }
                if let Some((agent_id, act_id, approved)) = act_decision {
                    let _ = self.cmd_tx.send(Cmd::AgentActDecision {
                        agent_id,
                        act_id,
                        approved,
                    });
                }
                if let Some((choice_id, msg_idx, action)) = research_choice_pick {
                    match action {
                        research_choice::ResearchChoiceAction::Answer => {
                            self.resolve_research_choice_answer(&choice_id, msg_idx);
                        }
                        research_choice::ResearchChoiceAction::Document => {
                            if let Some(sid) = self.active_session.clone() {
                                self.resolve_research_choice_document(&choice_id, msg_idx, &sid);
                            }
                        }
                        research_choice::ResearchChoiceAction::None => {}
                    }
                }
                if let Some((msg_idx, research_choice::DocumentProgressAction::Stop(agent_id))) =
                    document_progress_action
                {
                    for att in &mut self.chat[msg_idx].attachments {
                        if let ChatAttachment::DocumentProgress { state, .. } = att {
                            *state = "stopped".into();
                        }
                    }
                    self.document_prep_kill_pending += 1;
                    let _ = self.cmd_tx.send(Cmd::AgentKill { id: agent_id });
                }
                if let Some((question, path)) = document_result_open {
                    research_document::open_document(&mut self.document_overlay, &question, &path);
                }
                if let Some((act_id, msg_idx, approved)) = schedule_act {
                    if approved {
                        self.approve_schedule_act(&act_id, msg_idx);
                    } else {
                        self.deny_schedule_act(&act_id, msg_idx);
                    }
                }
                if let Some(id) = open_agent {
                    self.open_agent_tab(&id);
                }
                if let Some((prompt, path)) = open_studio {
                    self.image_studio.open_from_chat(&prompt, &path, None);
                    self.image_studio.apply_history_for_path(&path);
                    self.tab = Tab::Image;
                }
                if let Some(id) = target_reply {
                    self.ask_reply_target = Some(id);
                    self.chat_composer.refocus = true;
                    self.status = "réponse destinée à cet agent".into();
                }
                if !self.chat_runtime.streaming.is_empty() {
                    let (_, _, role_color) =
                        chat_bubble_colors(ChatBubbleKind::Assistant, ui.visuals().dark_mode);
                    chat_message_frame(ui, ChatBubbleKind::Assistant, None, |ui| {
                        ui.colored_label(
                            role_color,
                            egui::RichText::new(t.chat_assistant).strong().small(),
                        );
                        let streaming = agent_panel::format_chat_streaming_preview(
                            &self.chat_runtime.streaming,
                        );
                        ui.push_id("chat_md_stream", |ui| {
                            chat_markdown_viewer(ui).show(ui, &mut self.chat_md_cache, &streaming);
                        });
                    });
                } else if self.chat_runtime.pending {
                    let (_, _, role_color) =
                        chat_bubble_colors(ChatBubbleKind::Assistant, ui.visuals().dark_mode);
                    let thinking = if room_mode {
                        self.chat_runtime
                            .room_turn_text
                            .as_deref()
                            .and_then(|msg| {
                                chat_room::format_turn_speaker_queue(
                                    t,
                                    msg,
                                    room_members,
                                    room_conductor_policy,
                                )
                            })
                            .unwrap_or_else(|| t.chat_assistant.to_string())
                    } else {
                        t.chat_assistant.to_string()
                    };
                    chat_message_frame(ui, ChatBubbleKind::Assistant, None, |ui| {
                        ui.colored_label(
                            role_color,
                            egui::RichText::new(&thinking).strong().small(),
                        );
                        ui.weak("…");
                    });
                }
            });
    }
}
