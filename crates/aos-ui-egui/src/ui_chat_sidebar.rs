//! Conversation session list and web/file utilities sidebar.

use crate::cmd::Cmd;
use crate::os_open::{aos_home, open_os_folder};
use crate::{i18n, icons, overflow_scroll, theme, UiApp};
use eframe::egui;

impl UiApp {
    pub(crate) fn ui_chat_sidebar(
        &mut self,
        ui: &mut egui::Ui,
        width: f32,
        height: f32,
        t: &i18n::UiStrings,
    ) {
        let side_w = width;
        let full_y = height;
        ui.allocate_ui_with_layout(
            egui::vec2(side_w, full_y),
            egui::Layout::top_down(egui::Align::Min).with_cross_justify(true),
            |ui| {
                ui.set_width(side_w);
                overflow_scroll(ui, "chat_side", |ui| {
                    ui.set_width(side_w);
                    ui.heading(t.tab_chat);
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            egui::vec2((side_w - 44.0).max(80.0), theme::CONTROL_MIN_H_COMFORTABLE),
                            egui::TextEdit::singleline(&mut self.chat_state.sidebar.search)
                                .hint_text(t.session_search),
                        );
                        if ui.button("⋯").on_hover_text(t.session_archived).clicked() {
                            self.chat_state.sidebar.show_archived =
                                !self.chat_state.sidebar.show_archived;
                            if self.chat_state.sidebar.show_archived {
                                let _ = self.cmd_tx.send(Cmd::SessionListArchived);
                            }
                        }
                    });
                    if ui.button(t.session_new).clicked() {
                        let n = self.chat_state.sessions.len() + 1;
                        self.request_session_create(Some(format!("Session {n}")));
                    }
                    let now = crate::ui_format::now_ms();
                    let sessions: Vec<_> = crate::session_nav::filter_and_sort(
                        &self.chat_state.sessions,
                        &self.chat_state.sidebar.search,
                    )
                    .into_iter()
                    .cloned()
                    .collect();
                    let mut last_group = None;
                    for s in sessions {
                        if !self.chat_state.sidebar.show_archived && s.archived {
                            continue;
                        }
                        let group = crate::session_nav::group_for(s.updated_ms, now);
                        if last_group != Some(group) {
                            let label = match group {
                                crate::session_nav::SessionGroup::Today => t.session_today,
                                crate::session_nav::SessionGroup::Yesterday => t.session_yesterday,
                                crate::session_nav::SessionGroup::LastSevenDays => {
                                    t.session_last_seven_days
                                }
                                crate::session_nav::SessionGroup::Older => t.session_older,
                            };
                            ui.add_space(4.0);
                            ui.weak(label);
                            last_group = Some(group);
                        }
                        let selected =
                            self.chat_state.active_session.as_deref() == Some(s.id.as_str());
                        let unread = self.chat_state.session_chat.is_unread(&s.id);
                        let row = ui.horizontal(|ui| {
                            let pin = if s.pinned { "★ " } else { "" };
                            if unread {
                                let t = i18n::strings(&self.prefs.language);
                                icons::status_dot(ui, theme::SIGNAL)
                                    .on_hover_text(t.session_unread_reply);
                            }
                            let title = ui.selectable_label(selected, format!("{pin}{}", s.title));
                            ui.label(egui::RichText::new(format!("({})", s.message_count)).weak());
                            if s.archived {
                                ui.weak(t.session_archived);
                            }
                            title
                        });
                        if row.inner.clicked() || row.response.clicked() {
                            self.request_session_select(s.id.clone());
                        }
                        row.response.context_menu(|ui| {
                            if ui
                                .button(if s.pinned {
                                    t.session_unpin
                                } else {
                                    t.session_pin
                                })
                                .clicked()
                            {
                                let _ = self.cmd_tx.send(Cmd::SessionSetPinned {
                                    id: s.id.clone(),
                                    pinned: !s.pinned,
                                });
                                ui.close_menu();
                            }
                            if s.archived {
                                if ui.button(t.session_restore).clicked() {
                                    let _ = self.cmd_tx.send(Cmd::SessionSetArchived {
                                        id: s.id.clone(),
                                        archived: false,
                                    });
                                    ui.close_menu();
                                }
                            } else if ui.button(t.session_archive).clicked() {
                                let _ = self.cmd_tx.send(Cmd::SessionSetArchived {
                                    id: s.id.clone(),
                                    archived: true,
                                });
                                ui.close_menu();
                            }
                            if ui.button(t.session_export).clicked() {
                                let _ = self.cmd_tx.send(Cmd::SessionExport { id: s.id.clone() });
                                ui.close_menu();
                            }
                            if s.archived && ui.button(t.session_delete_permanently).clicked() {
                                self.chat_state.sidebar.delete_confirm = Some(s.id.clone());
                                ui.close_menu();
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.chat_state.sidebar.rename)
                                .desired_width(120.0)
                                .hint_text(t.sidebar_rename_hint),
                        );
                        if ui.button(t.sidebar_rename).clicked() {
                            if let Some(id) = self.chat_state.active_session.clone() {
                                let _ = self.cmd_tx.send(Cmd::SessionRename {
                                    id,
                                    title: self.chat_state.sidebar.rename.clone(),
                                });
                            }
                        }
                    });
                    if ui.button(t.session_export).clicked() {
                        if let Some(id) = self.chat_state.active_session.clone() {
                            let _ = self.cmd_tx.send(Cmd::SessionExport { id });
                        }
                    }
                    if ui.button(t.sidebar_delete).clicked() {
                        if let Some(id) = self.chat_state.active_session.clone() {
                            self.chat_state.sidebar.delete_confirm = Some(id);
                        }
                    }
                    ui.separator();
                    ui.heading(t.sidebar_web_files);
                    ui.set_min_width(side_w - 16.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.chat_state.sidebar.web_query)
                            .desired_width(side_w - 20.0)
                            .hint_text(t.sidebar_web_search_hint),
                    );
                    if ui.button(t.sidebar_search).clicked()
                        && !self.chat_state.sidebar.web_query.is_empty()
                    {
                        let _ = self.cmd_tx.send(Cmd::WebSearch {
                            query: self.chat_state.sidebar.web_query.clone(),
                            engine: self.prefs.web_search_engine.clone(),
                        });
                    }
                    for hit in &self.chat_state.sidebar.web_results {
                        ui.small(format!("• {} — {}", hit.title, hit.url));
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut self.chat_state.sidebar.fetch_url)
                            .desired_width(side_w - 20.0)
                            .hint_text(t.sidebar_url_hint),
                    );
                    ui.horizontal(|ui| {
                        if ui.button(t.sidebar_fetch_url).clicked()
                            && !self.chat_state.sidebar.fetch_url.is_empty()
                        {
                            let _ = self.cmd_tx.send(Cmd::NetFetch {
                                url: self.chat_state.sidebar.fetch_url.clone(),
                                max_bytes: self.prefs.web_fetch_max_bytes,
                            });
                        }
                        let t = i18n::strings(&self.prefs.language);
                        if ui.button(t.web_browse_btn).clicked()
                            && !self.chat_state.sidebar.fetch_url.is_empty()
                        {
                            let _ = self.cmd_tx.send(Cmd::WebBrowse {
                                url: self.chat_state.sidebar.fetch_url.clone(),
                                max_chars: self.prefs.web_browse_max_chars,
                            });
                        }
                    });
                    if !self.chat_state.sidebar.browse_preview.is_empty() {
                        ui.collapsing(t.sidebar_preview_page, |ui| {
                            ui.small(&self.chat_state.sidebar.browse_preview);
                        });
                    }
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("gen_fmt")
                            .selected_text(&self.chat_state.sidebar.generated_format)
                            .show_ui(ui, |ui| {
                                for f in ["md", "txt", "json", "csv", "png", "pdf"] {
                                    ui.selectable_value(
                                        &mut self.chat_state.sidebar.generated_format,
                                        f.into(),
                                        f,
                                    );
                                }
                            });
                    });
                    ui.add(
                        egui::TextEdit::singleline(&mut self.chat_state.sidebar.generated_path)
                            .desired_width(side_w - 20.0)
                            .hint_text(t.sidebar_gen_path_hint),
                    );
                    ui.add(
                        egui::TextEdit::multiline(&mut self.chat_state.sidebar.generated_content)
                            .desired_width(side_w - 20.0)
                            .desired_rows(3)
                            .hint_text(t.sidebar_gen_content_hint),
                    );
                    if ui.button(t.sidebar_gen_file).clicked()
                        && !self.chat_state.sidebar.generated_path.is_empty()
                    {
                        let _ = self.cmd_tx.send(Cmd::FilesGenerate {
                            format: self.chat_state.sidebar.generated_format.clone(),
                            path: self.chat_state.sidebar.generated_path.clone(),
                            content: self.chat_state.sidebar.generated_content.clone(),
                            title: Some("Akasha OS".into()),
                        });
                    }
                    if ui.button(t.sidebar_open_downloads).clicked() {
                        let dir = aos_home().join("var/storage/data/downloads");
                        open_os_folder(&dir);
                    }
                });
            },
        );
        if let Some(id) = self.chat_state.sidebar.delete_confirm.clone() {
            let title = self
                .chat_state
                .sessions
                .iter()
                .find(|session| session.id == id)
                .map(|session| session.title.clone())
                .unwrap_or_else(|| id.clone());
            let mut decision = None;
            egui::Window::new(t.session_delete_confirm_title)
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.label(t.session_delete_confirm_body.replace("{title}", &title));
                    ui.horizontal(|ui| {
                        if ui.button(t.memory_btn_cancel).clicked() {
                            decision = Some(false);
                        }
                        if ui.button(t.sidebar_delete).clicked() {
                            decision = Some(true);
                        }
                    });
                });
            if let Some(confirm) = decision {
                self.chat_state.sidebar.delete_confirm = None;
                if confirm {
                    self.pending_session_nav = crate::session_nav::PendingSessionNav::AwaitingDelete;
                    self.schedule_ui.clear_transcript_dirty();
                    let _ = self.cmd_tx.send(Cmd::SessionDelete { id });
                }
            }
        }
    }
}
