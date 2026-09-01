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
                    ui.heading("Sessions");
                    ui.label("Model");
                    {
                        let sid = self.chat_state.active_session.clone();
                        let mut current = self
                            .chat_state
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
                                            matches!(h, "127.0.0.1" | "localhost" | "::1" | "[::1]")
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
                        let n = self.chat_state.sessions.len() + 1;
                        self.request_session_create(Some(format!("Session {n}")));
                    }
                    for s in self.chat_state.sessions.clone() {
                        let selected =
                            self.chat_state.active_session.as_deref() == Some(s.id.as_str());
                        let unread = self.chat_state.session_chat.is_unread(&s.id);
                        let row = ui.horizontal(|ui| {
                            if unread {
                                let t = i18n::strings(&self.prefs.language);
                                icons::status_dot(ui, theme::SIGNAL)
                                    .on_hover_text(t.session_unread_reply);
                            }
                            let title = ui.selectable_label(selected, &s.title);
                            ui.label(egui::RichText::new(format!("({})", s.message_count)).weak());
                            title
                        });
                        if row.inner.clicked() || row.response.clicked() {
                            self.request_session_select(s.id.clone());
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.chat_state.sidebar.rename)
                                .desired_width(120.0)
                                .hint_text("titre"),
                        );
                        if ui.button("Renommer").clicked() {
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
                    if ui.button("Supprimer").clicked() {
                        if let Some(id) = self.chat_state.active_session.clone() {
                            self.request_session_delete(id);
                        }
                    }
                    ui.separator();
                    ui.heading("Web / fichiers");
                    ui.set_min_width(side_w - 16.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.chat_state.sidebar.web_query)
                            .desired_width(side_w - 20.0)
                            .hint_text("recherche web"),
                    );
                    if ui.button("Rechercher").clicked()
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
                            .hint_text("https://…"),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Télécharger URL").clicked()
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
                        ui.collapsing("Aperçu page", |ui| {
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
                            .hint_text("/downloads/…"),
                    );
                    ui.add(
                        egui::TextEdit::multiline(&mut self.chat_state.sidebar.generated_content)
                            .desired_width(side_w - 20.0)
                            .desired_rows(3)
                            .hint_text("contenu"),
                    );
                    if ui.button("Générer fichier").clicked()
                        && !self.chat_state.sidebar.generated_path.is_empty()
                    {
                        let _ = self.cmd_tx.send(Cmd::FilesGenerate {
                            format: self.chat_state.sidebar.generated_format.clone(),
                            path: self.chat_state.sidebar.generated_path.clone(),
                            content: self.chat_state.sidebar.generated_content.clone(),
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
    }
}
