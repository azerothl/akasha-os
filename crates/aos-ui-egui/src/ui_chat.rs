//! Conversation workspace, transcript, canvas, and composer.

use crate::*;

impl UiApp {
    pub(crate) fn ui_decl_module(&mut self, ui: &mut egui::Ui, module: &str) {
        if !self.decl_panels.contains_key(module) {
            self.decl_panels
                .insert(module.to_string(), decl_ui::DeclUiPanelState::new(module));
            let _ = self.cmd_tx.send(Cmd::ModuleUiLoad {
                module: module.to_string(),
            });
        }
        let t = i18n::strings(&self.prefs.language);
        let mut actions = decl_ui::DeclUiActions::default();
        if let Some(panel) = self.decl_panels.get_mut(module) {
            actions = panel.ui(ui, &mut self.decl_md_cache, t.decl_ui_refresh);
        }
        if actions.refresh {
            let _ = self.cmd_tx.send(Cmd::ModuleUiRefresh {
                module: module.to_string(),
            });
        }
        if let Some((tool, args)) = actions.invoke {
            let _ = self.cmd_tx.send(Cmd::ModuleUiInvoke {
                module: module.to_string(),
                tool,
                args,
            });
        }
    }


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
                                    &mut self.room_thinking_open,
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
                                            &self.canvas_panel.ops,
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
                    self.chat_refocus = true;
                    self.status = "réponse destinée à cet agent".into();
                }
                if !self.streaming.is_empty() {
                    let (_, _, role_color) =
                        chat_bubble_colors(ChatBubbleKind::Assistant, ui.visuals().dark_mode);
                    chat_message_frame(ui, ChatBubbleKind::Assistant, None, |ui| {
                        ui.colored_label(
                            role_color,
                            egui::RichText::new(t.chat_assistant).strong().small(),
                        );
                        let streaming = agent_panel::format_chat_streaming_preview(&self.streaming);
                        ui.push_id("chat_md_stream", |ui| {
                            chat_markdown_viewer(ui).show(ui, &mut self.chat_md_cache, &streaming);
                        });
                    });
                } else if self.chat_pending {
                    let (_, _, role_color) =
                        chat_bubble_colors(ChatBubbleKind::Assistant, ui.visuals().dark_mode);
                    let thinking = if room_mode {
                        self.room_turn_pending_text
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

    pub(crate) fn ui_chat(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        let full = ui.available_size();
        let gap = 8.0_f32;
        let canvas_open =
            chat_room::active_session_meta(&self.sessions, self.active_session.as_deref())
                .map(|m| m.canvas_open)
                .unwrap_or(false);
        let ChatSessionsSplit { side_w, chat_w } = chat_sessions_split(full.x, gap, canvas_open);

        ui.horizontal(|ui| {
            ui.set_min_height(full.y);
            ui.allocate_ui_with_layout(
                egui::vec2(side_w, full.y),
                egui::Layout::top_down(egui::Align::Min).with_cross_justify(true),
                |ui| {
                    ui.set_width(side_w);
                    overflow_scroll(ui, "chat_side", |ui| {
                        ui.set_width(side_w);
                        ui.heading("Sessions");
                        ui.label("Model");
                        {
                            let sid = self.active_session.clone();
                            let mut current = self
                                .sessions
                                .iter()
                                .find(|s| Some(s.id.as_str()) == sid.as_deref())
                                .and_then(|s| s.model_id.clone())
                                .unwrap_or_default();
                            egui::ComboBox::from_id_salt("session_model")
                                .selected_text(if current.is_empty() {
                                    "default".to_string()
                                } else {
                                    current.clone()
                                })
                                .width(side_w - 12.0)
                                .show_ui(ui, |ui| {
                                    if ui
                                        .selectable_value(&mut current, String::new(), "default")
                                        .changed()
                                    {
                                        if let Some(id) = sid.clone() {
                                            let _ = self.cmd_tx.send(Cmd::SessionSetModel {
                                                session_id: id,
                                                model_id: None,
                                            });
                                        }
                                    }
                                    let local_only = self.prefs.routing == "local_only";
                                    ui.weak("Local");
                                    for m in &self.model_infos {
                                        if m.id.starts_with("provider:") {
                                            continue;
                                        }
                                        if ui
                                            .selectable_value(
                                                &mut current,
                                                m.id.clone(),
                                                format!("{} [{:?}]", m.id, m.state),
                                            )
                                            .changed()
                                        {
                                            if let Some(id) = sid.clone() {
                                                let _ = self.cmd_tx.send(Cmd::SessionSetModel {
                                                    session_id: id,
                                                    model_id: Some(m.id.clone()),
                                                });
                                            }
                                        }
                                    }
                                    ui.weak("Providers");
                                    for m in &self.model_infos {
                                        if !m.id.starts_with("provider:") {
                                            continue;
                                        }
                                        let pid = m.id.split(':').nth(1).unwrap_or("");
                                        let loopback = self
                                            .providers
                                            .iter()
                                            .find(|p| p.id == pid)
                                            .map(|p| {
                                                let h = p
                                                    .endpoint
                                                    .trim_start_matches("https://")
                                                    .trim_start_matches("http://")
                                                    .split(['/', ':'])
                                                    .next()
                                                    .unwrap_or("");
                                                matches!(
                                                    h,
                                                    "127.0.0.1" | "localhost" | "::1" | "[::1]"
                                                )
                                            })
                                            .unwrap_or(false);
                                        if local_only && !loopback {
                                            continue;
                                        }
                                        if ui
                                            .selectable_value(
                                                &mut current,
                                                m.id.clone(),
                                                format!("{} [{:?}]", m.id, m.state),
                                            )
                                            .changed()
                                        {
                                            if let Some(id) = sid.clone() {
                                                let _ = self.cmd_tx.send(Cmd::SessionSetModel {
                                                    session_id: id,
                                                    model_id: Some(m.id.clone()),
                                                });
                                            }
                                        }
                                    }
                                });
                        }
                        if ui.button("+ Nouvelle").clicked() {
                            let n = self.sessions.len() + 1;
                            self.request_session_create(Some(format!("Session {n}")));
                        }
                        for s in self.sessions.clone() {
                            let selected = self.active_session.as_deref() == Some(s.id.as_str());
                            let unread = self.session_chat.is_unread(&s.id);
                            let row = ui.horizontal(|ui| {
                                if unread {
                                    let t = i18n::strings(&self.prefs.language);
                                    icons::status_dot(ui, theme::SIGNAL)
                                        .on_hover_text(t.session_unread_reply);
                                }
                                let title = ui.selectable_label(selected, &s.title);
                                ui.label(
                                    egui::RichText::new(format!("({})", s.message_count)).weak(),
                                );
                                title
                            });
                            if row.inner.clicked() || row.response.clicked() {
                                self.request_session_select(s.id.clone());
                            }
                        }
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.rename_buf)
                                    .desired_width(120.0)
                                    .hint_text("titre"),
                            );
                            if ui.button("Renommer").clicked() {
                                if let Some(id) = self.active_session.clone() {
                                    let _ = self.cmd_tx.send(Cmd::SessionRename {
                                        id,
                                        title: self.rename_buf.clone(),
                                    });
                                }
                            }
                        });
                        if ui.button(t.session_export).clicked() {
                            if let Some(id) = self.active_session.clone() {
                                let _ = self.cmd_tx.send(Cmd::SessionExport { id });
                            }
                        }
                        if ui.button("Supprimer").clicked() {
                            if let Some(id) = self.active_session.clone() {
                                self.request_session_delete(id);
                            }
                        }
                        ui.separator();
                        ui.heading("Web / fichiers");
                        ui.set_min_width(side_w - 16.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.web_query)
                                .desired_width(side_w - 20.0)
                                .hint_text("recherche web"),
                        );
                        if ui.button("Rechercher").clicked() && !self.web_query.is_empty() {
                            let _ = self.cmd_tx.send(Cmd::WebSearch {
                                query: self.web_query.clone(),
                                engine: self.prefs.web_search_engine.clone(),
                            });
                        }
                        for hit in &self.web_results {
                            ui.small(format!("• {} — {}", hit.title, hit.url));
                        }
                        ui.add(
                            egui::TextEdit::singleline(&mut self.fetch_url)
                                .desired_width(side_w - 20.0)
                                .hint_text("https://…"),
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Télécharger URL").clicked() && !self.fetch_url.is_empty()
                            {
                                let _ = self.cmd_tx.send(Cmd::NetFetch {
                                    url: self.fetch_url.clone(),
                                    max_bytes: self.prefs.web_fetch_max_bytes,
                                });
                            }
                            let t = i18n::strings(&self.prefs.language);
                            if ui.button(t.web_browse_btn).clicked() && !self.fetch_url.is_empty() {
                                let _ = self.cmd_tx.send(Cmd::WebBrowse {
                                    url: self.fetch_url.clone(),
                                    max_chars: self.prefs.web_browse_max_chars,
                                });
                            }
                        });
                        if !self.browse_preview.is_empty() {
                            ui.collapsing("Aperçu page", |ui| {
                                ui.small(&self.browse_preview);
                            });
                        }
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt("gen_fmt")
                                .selected_text(&self.gen_format)
                                .show_ui(ui, |ui| {
                                    for f in ["md", "txt", "json", "csv", "png", "pdf"] {
                                        ui.selectable_value(&mut self.gen_format, f.into(), f);
                                    }
                                });
                        });
                        ui.add(
                            egui::TextEdit::singleline(&mut self.gen_path)
                                .desired_width(side_w - 20.0)
                                .hint_text("/downloads/…"),
                        );
                        ui.add(
                            egui::TextEdit::multiline(&mut self.gen_content)
                                .desired_width(side_w - 20.0)
                                .desired_rows(3)
                                .hint_text("contenu"),
                        );
                        if ui.button("Générer fichier").clicked() && !self.gen_path.is_empty() {
                            let _ = self.cmd_tx.send(Cmd::FilesGenerate {
                                format: self.gen_format.clone(),
                                path: self.gen_path.clone(),
                                content: self.gen_content.clone(),
                                title: Some("Akasha OS".into()),
                            });
                        }
                        if ui.button("Ouvrir downloads").clicked() {
                            let dir = aos_home().join("var/storage/data/downloads");
                            open_os_folder(&dir);
                        }
                    });
                },
            );

            ui.add_space(gap);

            ui.allocate_ui_with_layout(
                egui::vec2(chat_w, full.y),
                egui::Layout::top_down(egui::Align::Min).with_cross_justify(true),
                |ui| {
                    ui.set_min_width(chat_w);
                    ui.set_min_height(full.y);
                    let room_mode = chat_room::session_is_room(chat_room::active_session_meta(
                        &self.sessions,
                        self.active_session.as_deref(),
                    ));
                    let room_session_meta = chat_room::active_session_meta(
                        &self.sessions,
                        self.active_session.as_deref(),
                    );
                    let room_members: Vec<ChatRoomMember> = room_session_meta
                        .map(|m| m.members.clone())
                        .unwrap_or_default();
                    let room_conductor_policy =
                        room_session_meta.map(|m| m.conductor_policy.clone());
                    let canvas_open = chat_room::active_session_meta(
                        &self.sessions,
                        self.active_session.as_deref(),
                    )
                    .map(|m| m.canvas_open)
                    .unwrap_or(false);
                    let canvas_aspect = chat_room::active_session_meta(
                        &self.sessions,
                        self.active_session.as_deref(),
                    )
                    .map(|m| m.canvas_aspect)
                    .unwrap_or_default();
                    let active_sid = self.active_session.clone();

                    let ask_queue = self.pending_ask_queue();
                    let session_model = self
                        .sessions
                        .iter()
                        .find(|s| self.active_session.as_deref() == Some(s.id.as_str()))
                        .and_then(|s| s.model_id.clone());
                    let show_vision_banner = !self.chat_pending_images.is_empty()
                        && !session_model_supports_vision(session_model.as_deref());
                    let composer_h = chat_composer_reserve_height(
                        chat_w,
                        ask_queue.len(),
                        self.chat_pending_images.len(),
                        self.chat_pending_documents.len(),
                        show_vision_banner,
                    );
                    let pane_h = ui.available_height();
                    let body_h = (pane_h - composer_h).max(120.0);

                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), body_h),
                        egui::Layout::top_down(egui::Align::Min).with_cross_justify(true),
                        |ui| {
                            self.ui_session_bar(ui, &t);
                            let content_h = ui.available_height().max(80.0);

                            if canvas_open {
                                let split_gap = 8.0_f32;
                                let total_w = ui.available_width();
                                match chat_canvas_layout(total_w, content_h, split_gap) {
                                    ChatCanvasLayout::SideBySide {
                                        transcript_w,
                                        canvas_w,
                                    } => {
                                        ui.horizontal(|ui| {
                                            ui.set_min_height(content_h);
                                            ui.allocate_ui_with_layout(
                                                egui::vec2(transcript_w, content_h),
                                                egui::Layout::top_down(egui::Align::Min)
                                                    .with_cross_justify(true),
                                                |ui| {
                                                    self.ui_chat_transcript(
                                                        ui,
                                                        &t,
                                                        room_mode,
                                                        &room_members,
                                                        room_conductor_policy.as_ref(),
                                                        content_h,
                                                    );
                                                },
                                            );
                                            ui.add_space(split_gap);
                                            ui.allocate_ui_with_layout(
                                                egui::vec2(canvas_w, content_h),
                                                egui::Layout::top_down(egui::Align::Min)
                                                    .with_cross_justify(true),
                                                |ui| {
                                                    if let Some(ref sid) = active_sid {
                                                        let aspect_action =
                                                            chat_canvas::ui_canvas_aspect_row(
                                                                ui,
                                                                &t,
                                                                canvas_aspect,
                                                            );
                                                        self.dispatch_canvas_ui_action(
                                                            aspect_action,
                                                            sid,
                                                        );
                                                        let action = chat_canvas::ui_canvas_surface(
                                                            ui,
                                                            &mut self.canvas_panel,
                                                            canvas_aspect,
                                                            t.canvas_empty_hint,
                                                        );
                                                        self.dispatch_canvas_ui_action(action, sid);
                                                        self.canvas_poll_if_due(ui, sid);
                                                    }
                                                },
                                            );
                                        });
                                    }
                                    ChatCanvasLayout::Stacked {
                                        transcript_h,
                                        canvas_h,
                                    } => {
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(total_w, transcript_h),
                                            egui::Layout::top_down(egui::Align::Min)
                                                .with_cross_justify(true),
                                            |ui| {
                                                self.ui_chat_transcript(
                                                    ui,
                                                    &t,
                                                    room_mode,
                                                    &room_members,
                                                    room_conductor_policy.as_ref(),
                                                    transcript_h,
                                                );
                                            },
                                        );
                                        ui.add_space(split_gap);
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(total_w, canvas_h),
                                            egui::Layout::top_down(egui::Align::Min)
                                                .with_cross_justify(true),
                                            |ui| {
                                                if let Some(ref sid) = active_sid {
                                                    let aspect_action =
                                                        chat_canvas::ui_canvas_aspect_row(
                                                            ui,
                                                            &t,
                                                            canvas_aspect,
                                                        );
                                                    self.dispatch_canvas_ui_action(
                                                        aspect_action,
                                                        sid,
                                                    );
                                                    let action = chat_canvas::ui_canvas_surface(
                                                        ui,
                                                        &mut self.canvas_panel,
                                                        canvas_aspect,
                                                        t.canvas_empty_hint,
                                                    );
                                                    self.dispatch_canvas_ui_action(action, sid);
                                                    self.canvas_poll_if_due(ui, sid);
                                                }
                                            },
                                        );
                                    }
                                }
                            } else {
                                self.ui_chat_transcript(
                                    ui,
                                    &t,
                                    room_mode,
                                    &room_members,
                                    room_conductor_policy.as_ref(),
                                    content_h,
                                );
                            }
                        },
                    );

                    let completions = slash_completions(&self.input);
                    let mention_hits = if room_mode {
                        chat_room::mention_completions(&self.input, &room_members, &t)
                    } else {
                        Vec::new()
                    };
                    let mut chat_sent_this_frame = false;
                    let input_row = ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), composer_h),
                        egui::Layout::bottom_up(egui::Align::Min),
                        |ui| {
                            let t = i18n::strings(&self.prefs.language);
                            let hint = match ask_queue.len() {
                                0 => t.chat_hint.to_string(),
                                1 => t.chat_hint_agent_ask.to_string(),
                                n => {
                                    let title = self
                                        .blocked_ask_agent()
                                        .map(agent_display_title)
                                        .unwrap_or_default();
                                    t.chat_hint_agent_ask_many
                                        .replace("{agent}", &title)
                                        .replace("{n}", &n.to_string())
                                }
                            };
                            let show_stop = self.chat_pending
                                && (room_mode || self.chat_inference_id.is_some());
                            let item_gap = ui.spacing().item_spacing.x;
                            let send_w = send_button_reserved_width(ui, &t);
                            let stop_w = if show_stop {
                                stop_button_reserved_width(ui, &t)
                            } else {
                                0.0
                            };

                            let mut attach_from_menu = false;
                            let mut attach_document_from_menu = false;
                            let mut reuse_last_image = false;
                            let mut send_clicked = false;
                            let mut input_response: Option<egui::Response> = None;

                            let mut run_attach_menu = |ui: &mut egui::Ui| {
                                icons::attach_menu(ui, "chat_attach", t.chat_attach_image, |ui| {
                                    if self.last_session_image.is_some()
                                        && ui.button(t.chat_last_session_image).clicked()
                                    {
                                        reuse_last_image = true;
                                    }
                                    if ui.button(t.chat_attach_image).clicked() {
                                        attach_from_menu = true;
                                    }
                                    if ui.button(t.chat_attach_document).clicked() {
                                        attach_document_from_menu = true;
                                    }
                                });
                            };

                            let row_w = ui.available_width();
                            let input_h = ui.spacing().interact_size.y;
                            let field_w = composer_field_width(
                                row_w,
                                send_w,
                                icons::ATTACH_BTN_W,
                                stop_w,
                                item_gap,
                                show_stop,
                            );

                            ui.allocate_ui_with_layout(
                                egui::vec2(row_w, input_h),
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if show_stop {
                                        if room_mode {
                                            if ui
                                                .add_sized(
                                                    egui::vec2(stop_w, input_h),
                                                    egui::Button::new(t.chat_stop),
                                                )
                                                .clicked()
                                            {
                                                if let Some(sid) = self.active_session.clone() {
                                                    let _ = self.cmd_tx.send(Cmd::RoomTurnCancel {
                                                        session_id: sid,
                                                    });
                                                }
                                            }
                                        } else if let Some(id) = self.chat_inference_id {
                                            if ui
                                                .add_sized(
                                                    egui::vec2(stop_w, input_h),
                                                    egui::Button::new(t.chat_stop),
                                                )
                                                .clicked()
                                            {
                                                if let Some(sid) = self.active_session.clone() {
                                                    let _ = self.cmd_tx.send(Cmd::ChatCancel {
                                                        inference_id: id,
                                                        session_id: sid,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                    let send_btn = ui
                                        .add_sized(
                                            egui::vec2(send_w, input_h),
                                            egui::Button::new(t.agent_send),
                                        )
                                        .on_hover_text(t.tip_send);
                                    send_clicked |= send_btn.clicked();

                                    ui.allocate_ui_with_layout(
                                        egui::vec2(icons::ATTACH_BTN_W, input_h),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            run_attach_menu(ui);
                                        },
                                    );

                                    ui.set_width(field_w);
                                    let r = ui.add(
                                        egui::TextEdit::singleline(&mut self.input)
                                            .id_salt("chat_input")
                                            .desired_width(field_w)
                                            .hint_text(&hint),
                                    );
                                    input_response = Some(r);
                                },
                            );

                            if attach_from_menu {
                                if let Some(path) = os_open::pick_os_file(
                                    t.chat_attach_image,
                                    &[("Images", &["png", "jpg", "jpeg", "webp"])],
                                    os_open::user_downloads_dir().as_deref(),
                                ) {
                                    self.queue_chat_image(path.to_string_lossy().into_owned());
                                }
                            } else if attach_document_from_menu {
                                if let Some(path) = os_open::pick_os_file(
                                    t.chat_attach_document,
                                    &[(
                                        "Documents",
                                        aos_proto::chat_document::CHAT_DOCUMENT_EXTENSIONS,
                                    )],
                                    os_open::user_downloads_dir().as_deref(),
                                ) {
                                    self.queue_chat_document(path.to_string_lossy().into_owned());
                                }
                            } else if reuse_last_image {
                                if let Some(last) = self.last_session_image.clone() {
                                    self.queue_chat_image(last);
                                }
                            }

                            if let Some(r) = input_response {
                                if self.chat_refocus {
                                    r.request_focus();
                                    self.chat_refocus = false;
                                }
                                let send = send_clicked
                                    || (r.lost_focus()
                                        && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                                if send {
                                    self.send_chat();
                                    chat_sent_this_frame = true;
                                    self.chat_refocus = true;
                                }
                            }

                            let composer_input_rect = ui.min_rect();
                            if !self.chat_pending_images.is_empty()
                                || !self.chat_pending_documents.is_empty()
                            {
                                let ctx = ui.ctx().clone();
                                chat_media::render_pending_attachment_chips(
                                    ui,
                                    &ctx,
                                    &mut self.chat_pending_images,
                                    &mut self.chat_pending_documents,
                                );
                            }
                            if show_vision_banner {
                                let t = i18n::strings(&self.prefs.language);
                                ui.horizontal(|ui| {
                                    ui.weak(t.chat_vision_banner);
                                    if ui.small_button(t.chat_load_vision_model).clicked() {
                                        self.load_preferred_vision_model();
                                    }
                                });
                            }
                            if ask_queue.len() > 1 {
                                let t = i18n::strings(&self.prefs.language);
                                let title = self
                                    .blocked_ask_agent()
                                    .map(agent_display_title)
                                    .unwrap_or_default();
                                ui.colored_label(
                                    egui::Color32::from_rgb(240, 190, 100),
                                    t.chat_ask_queue
                                        .replace("{n}", &ask_queue.len().to_string())
                                        .replace("{agent}", &title),
                                );
                            }
                            composer_input_rect
                        },
                    );
                    let input_rect = input_row.inner;

                    // Popup au-dessus de l'input, en overlay sur le chat (pas sous le cadre)
                    if !mention_hits.is_empty() {
                        let popup_w = input_rect.width().clamp(240.0, chat_w);
                        let max_h = 180.0_f32;
                        let mut picked: Option<String> = None;
                        egui::Area::new(egui::Id::new("mention_completions_popup"))
                            .order(egui::Order::Foreground)
                            .fixed_pos(egui::pos2(input_rect.left(), input_rect.top() - 6.0))
                            .pivot(egui::Align2::LEFT_BOTTOM)
                            .interactable(true)
                            .show(ui.ctx(), |ui| {
                                egui::Frame::popup(ui.style())
                                    .inner_margin(egui::Margin::same(8))
                                    .show(ui, |ui| {
                                        ui.set_min_width(popup_w * 0.85);
                                        ui.set_max_width(popup_w);
                                        ui.label(
                                            egui::RichText::new(t.room_mention_pick)
                                                .small()
                                                .strong(),
                                        );
                                        egui::ScrollArea::vertical().max_height(max_h).show(
                                            ui,
                                            |ui| {
                                                for (text, name) in &mention_hits {
                                                    if ui
                                                        .selectable_label(false, name.as_str())
                                                        .clicked()
                                                    {
                                                        picked = Some(text.clone());
                                                    }
                                                }
                                            },
                                        );
                                    });
                            });
                        if let Some(text) = picked {
                            if !chat_sent_this_frame {
                                self.input = text;
                                self.chat_refocus = true;
                            }
                        }
                    } else if !completions.is_empty() {
                        let t = i18n::strings(&self.prefs.language);
                        let popup_w = input_rect.width().clamp(240.0, chat_w);
                        let max_h = 220.0_f32;
                        let mut picked: Option<String> = None;
                        egui::Area::new(egui::Id::new("slash_completions_popup"))
                            .order(egui::Order::Foreground)
                            .fixed_pos(egui::pos2(input_rect.left(), input_rect.top() - 6.0))
                            .pivot(egui::Align2::LEFT_BOTTOM)
                            .interactable(true)
                            .show(ui.ctx(), |ui| {
                                egui::Frame::popup(ui.style())
                                    .inner_margin(egui::Margin::same(8))
                                    .show(ui, |ui| {
                                        ui.set_min_width(popup_w * 0.85);
                                        ui.set_max_width(popup_w);
                                        ui.label(
                                            egui::RichText::new(t.slash_pick).small().strong(),
                                        );
                                        egui::ScrollArea::vertical().max_height(max_h).show(
                                            ui,
                                            |ui| {
                                                for (cmd, desc) in &completions {
                                                    if ui
                                                        .selectable_label(
                                                            false,
                                                            format!("{cmd} — {desc}"),
                                                        )
                                                        .clicked()
                                                    {
                                                        picked = Some(slash_insert_text(cmd));
                                                    }
                                                }
                                            },
                                        );
                                    });
                            });
                        if let Some(text) = picked {
                            if !chat_sent_this_frame {
                                self.input = text;
                                self.chat_refocus = true;
                            }
                        }
                    }
                },
            );
        });
    }
}
