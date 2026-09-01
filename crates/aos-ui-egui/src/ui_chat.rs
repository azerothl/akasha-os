//! Conversation workspace, transcript, canvas, and composer.

use crate::*;

impl UiApp {



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
