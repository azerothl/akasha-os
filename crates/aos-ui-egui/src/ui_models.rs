//! Model catalogue, downloads, and installed-model panel.

use crate::cmd::Cmd;
use crate::os_open::{open_url, request_preview_restart};
use crate::ui_format::{format_model_infer_line, human_bytes};
use crate::{i18n, icons, models_page, UiApp};
use eframe::egui;

impl UiApp {
    pub(crate) fn ui_model_download_restart(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.models_ui.model_download_restart.is_none() {
            return;
        }
        let t = i18n::strings(&self.prefs.language);
        ui.horizontal(|ui| {
            ui.colored_label(
                egui::Color32::from_rgb(120, 200, 140),
                self.models_ui.download_status.as_str(),
            );
            if ui.button(t.models_restart_preview).clicked() {
                request_preview_restart(ctx);
                self.models_ui.dismiss_download_restart(false);
            }
            if icons::close_button(ui).clicked() {
                self.models_ui.dismiss_download_restart(true);
            }
        });
    }

    pub(crate) fn ui_models(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let t = i18n::strings(&self.prefs.language);
        ui.heading(t.tab_models);
        ui.weak(t.tab_hint_models);
        ui.horizontal(|ui| {
            if ui.button(t.models_refresh_list).clicked() {
                let _ = self.cmd_tx.send(Cmd::ModelsRefresh);
            }
        });
        if !self.models_ui.model_updates_msg.is_empty() {
            ui.colored_label(
                egui::Color32::from_rgb(180, 220, 120),
                &self.models_ui.model_updates_msg,
            );
        }
        if !self.models_ui.download_status.is_empty()
            && self.models_ui.model_download_restart.is_none()
        {
            ui.label(&self.models_ui.download_status);
        }
        if let Some(dl) = &self.models_ui.model_download {
            let frac = (dl.percent as f32 / 100.0).clamp(0.0, 1.0);
            let txt = if dl.total_bytes > 0 {
                format!(
                    "{} · {} / {}",
                    dl.model_id,
                    human_bytes(dl.done_bytes),
                    human_bytes(dl.total_bytes)
                )
            } else {
                format!("{} · {}%", dl.model_id, dl.percent)
            };
            ui.add(egui::ProgressBar::new(frac).text(txt));
        }
        self.ui_model_download_restart(ui, ctx);

        let hf_busy = self.models_ui.download_busy();
        models_page::ui_hf_import(
            ui,
            &mut self.models_ui.hf_download_url,
            &mut self.models_ui.hf_download_name,
            &mut self.models_ui.hf_download_status,
            hf_busy,
            &self.cmd_tx,
            &t,
        );

        ui.separator();
        models_page::ui_catalog_tab_bar(ui, &mut self.models_ui.catalog_tab, &t);
        if matches!(
            self.models_ui.catalog_tab,
            models_page::ModelCatalogTab::Image | models_page::ModelCatalogTab::Audio
        ) {
            ui.weak(t.models_media_packs);
        }

        let catalog = models_page::load_catalog_models();
        let installed_rows = models_page::load_installed_rows(&self.models_ui.model_infos);
        let busy = self.models_ui.download_busy();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match self.models_ui.catalog_tab {
                models_page::ModelCatalogTab::Installed => {
                    if installed_rows.is_empty() {
                        ui.weak(t.models_catalog_empty);
                    }
                    for m in installed_rows {
                        let id = m.id.clone();
                        let mut load = false;
                        let mut set_default = false;
                        let mut redownload = false;
                        let mut remove = false;
                        models_page::ui_installed_card(
                            ui,
                            &m,
                            busy,
                            &t,
                            &mut || load = true,
                            &mut || set_default = true,
                            &mut || redownload = true,
                            &mut || remove = true,
                        );
                        if load {
                            self.models_ui.begin_transition(&id);
                            let _ = self.cmd_tx.send(Cmd::ModelLoad {
                                model_id: id.clone(),
                            });
                        }
                        if set_default {
                            if let Some(sid) = self.chat_state.active_session.clone() {
                                let _ = self.cmd_tx.send(Cmd::SessionSetModel {
                                    session_id: sid,
                                    model_id: Some(id.clone()),
                                });
                            }
                        }
                        if redownload {
                            let _ = self.cmd_tx.send(Cmd::ModelRedownload {
                                model_id: id.clone(),
                            });
                        }
                        if remove {
                            let _ = self.cmd_tx.send(Cmd::ModelRemove {
                                model_id: id.clone(),
                            });
                        }
                        if let Some(rt) = &m.runtime {
                            let transitioning = self.models_ui.is_transitioning(&id) || busy;
                            ui.horizontal(|ui| {
                                let state_label = models_page::model_state_human(
                                    &rt.state,
                                    t.models_tab_installed == "Installés",
                                );
                                ui.label(format!("{} : {state_label}", t.models_state_prefix));
                                match &rt.state {
                                    aos_proto::ModelState::Loaded
                                    | aos_proto::ModelState::PartiallyOffloaded => {
                                        if ui
                                            .add_enabled(
                                                !transitioning,
                                                egui::Button::new(t.models_unload),
                                            )
                                            .clicked()
                                        {
                                            self.models_ui.begin_transition(&id);
                                            let _ = self.cmd_tx.send(Cmd::ModelUnload {
                                                model_id: id.clone(),
                                            });
                                        }
                                    }
                                    aos_proto::ModelState::Error => {
                                        if ui
                                            .add_enabled(
                                                !transitioning,
                                                egui::Button::new(t.models_retry),
                                            )
                                            .clicked()
                                        {
                                            self.models_ui.begin_transition(&id);
                                            let _ = self.cmd_tx.send(Cmd::ModelLoad {
                                                model_id: id.clone(),
                                            });
                                        }
                                        if ui
                                            .add_enabled(
                                                !transitioning,
                                                egui::Button::new(t.models_reload_clean),
                                            )
                                            .clicked()
                                        {
                                            self.models_ui.begin_transition(&id);
                                            let _ = self.cmd_tx.send(Cmd::ModelReload {
                                                model_id: id.clone(),
                                            });
                                        }
                                    }
                                    _ => {}
                                }
                            });
                            if let Some(error) = self.models_ui.last_errors.get(&id) {
                                ui.colored_label(
                                    egui::Color32::from_rgb(255, 130, 110),
                                    format!("{} : {error}", t.models_state_prefix),
                                );
                                ui.weak(t.models_error_detail);
                            }
                        }
                        ui.add_space(6.0);
                    }
                    ui.separator();
                    ui.label(t.metrics_live);
                    if let Some(m) = &self.metrics {
                        for mm in &m.models {
                            ui.group(|ui| {
                                ui.strong(format!(
                                    "{} · {}",
                                    mm.model_id,
                                    models_page::model_state_human(
                                        &mm.state,
                                        t.models_tab_installed == "Installés",
                                    )
                                ));
                                ui.label(format_model_infer_line(mm, &t));
                            });
                        }
                    }
                }
                tab => {
                    let filtered: Vec<_> = catalog
                        .iter()
                        .filter(|m| models_page::category_of(m) == tab)
                        .collect();
                    if filtered.is_empty() {
                        ui.weak(t.models_catalog_empty);
                    }
                    for m in filtered {
                        let installed = installed_rows.iter().any(|x| x.id == m.id);
                        let id = m.id.clone();
                        let mut download = false;
                        let mut redownload = false;
                        let mut remove = false;
                        let mut open_hf = None;
                        models_page::ui_model_card(
                            ui,
                            m,
                            installed,
                            busy,
                            &t,
                            &mut || download = true,
                            &mut || redownload = true,
                            &mut || remove = true,
                            &mut |url| open_hf = Some(url.to_string()),
                        );
                        if download {
                            let _ = self.cmd_tx.send(Cmd::ModelDownload {
                                model_id: id.clone(),
                            });
                        }
                        if redownload {
                            let _ = self.cmd_tx.send(Cmd::ModelRedownload {
                                model_id: id.clone(),
                            });
                        }
                        if remove {
                            let _ = self.cmd_tx.send(Cmd::ModelRemove {
                                model_id: id.clone(),
                            });
                        }
                        if let Some(url) = open_hf {
                            open_url(&url);
                        }
                        ui.add_space(6.0);
                    }
                }
            });
    }
}
