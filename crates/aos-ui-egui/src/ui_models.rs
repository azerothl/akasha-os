//! Model catalogue, downloads, and installed-model panel.

use crate::cmd::Cmd;
use crate::os_open::{open_url, request_preview_restart};
use crate::ui_format::human_bytes;
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
                    self.ui_model_metrics(ui, &t);
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

    /// Live metrics deliberately follow the loader's effective state, not the
    /// requested setting: a CPU fallback or a pressure-reduced KV cache must
    /// be visible where users decide whether to retry a model load.
    fn ui_model_metrics(&self, ui: &mut egui::Ui, t: &i18n::UiStrings) {
        ui.add_space(crate::theme::SPACE_UNIT * 2.0);
        ui.separator();
        ui.add_space(crate::theme::SPACE_UNIT);
        ui.heading(t.metrics_live);
        ui.weak(t.metrics_hint);
        let Some(system) = &self.metrics else {
            ui.add_space(crate::theme::SPACE_UNIT);
            ui.weak(t.metrics_empty);
            return;
        };
        if system.models.is_empty() {
            ui.add_space(crate::theme::SPACE_UNIT);
            ui.weak(t.metrics_empty);
            return;
        }

        for mm in &system.models {
            ui.add_space(crate::theme::SPACE_UNIT);
            let colors = crate::theme::button_colors(ui);
            egui::Frame::group(ui.style())
                .stroke(egui::Stroke::new(
                    crate::theme::STROKE_SUBTLE,
                    colors.accent,
                ))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(&mm.model_id);
                        ui.weak(models_page::model_state_human(
                            &mm.state,
                            t.models_tab_installed == "Installés",
                        ));
                        if mm.fallback_used {
                            ui.colored_label(colors.warning, t.metrics_fallback_used);
                        }
                    });

                    if mm.media_total_steps.is_some() || mm.media_step.is_some() {
                        ui.label(media_metrics_line(mm, t));
                    } else {
                        ui.horizontal_wrapped(|ui| {
                            metric_reading(
                                ui,
                                t.metrics_ttft,
                                mm.last_ttft_ms.map(|v| format!("{v:.0} ms")),
                            );
                            metric_reading(
                                ui,
                                t.metrics_tok_s,
                                mm.last_tok_s.map(|v| format!("{v:.1}")),
                            );
                            metric_reading(
                                ui,
                                t.metrics_active,
                                Some(mm.active_inferences.to_string()),
                            );
                            metric_reading(ui, t.metrics_queued, Some(mm.queued.to_string()));
                            if let Some(draft) = mm.draft_accept {
                                metric_reading(ui, t.metrics_draft, Some(format!("{draft:.1}")));
                            }
                            if let Some(prefix) = mm.prefix_hit.filter(|tokens| *tokens > 0) {
                                metric_reading(ui, t.metrics_prefix, Some(prefix.to_string()));
                            }
                        });
                    }

                    ui.horizontal_wrapped(|ui| {
                        metric_reading(ui, t.metrics_vram, Some(human_bytes(mm.vram_bytes)));
                        metric_reading(ui, t.metrics_ram, Some(human_bytes(mm.ram_bytes)));
                        metric_reading(ui, t.metrics_disk, Some(human_bytes(mm.disk_bytes)));
                    });

                    if mm.adaptive_backend.is_some()
                        || mm.quantization.is_some()
                        || mm.effective_profile.is_some()
                        || mm.kv_cache.is_some()
                    {
                        ui.add_space(4.0);
                        ui.separator();
                        ui.add_space(4.0);
                        ui.strong(t.metrics_effective_plan);
                        ui.horizontal_wrapped(|ui| {
                            metric_reading(ui, t.metrics_backend, mm.adaptive_backend.clone());
                            metric_reading(ui, t.metrics_quantization, mm.quantization.clone());
                            metric_reading(ui, t.metrics_profile, mm.effective_profile.clone());
                            let kv = mm.kv_cache.as_ref().map(|kind| match mm.kv_tokens {
                                Some(tokens) => format!("{kind} · {tokens}"),
                                None => kind.clone(),
                            });
                            metric_reading(ui, t.metrics_kv, kv);
                        });
                        if let Some(reason) = mm
                            .plan_reason
                            .as_deref()
                            .filter(|reason| !reason.is_empty())
                        {
                            ui.add_space(2.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.weak(format!("{}:", t.metrics_plan_reason));
                                ui.label(reason);
                            });
                        }
                    }
                });
        }
    }
}

fn metric_reading(ui: &mut egui::Ui, label: &str, value: Option<String>) {
    let Some(value) = value else {
        return;
    };
    ui.horizontal(|ui| {
        ui.weak(format!("{label}:"));
        ui.monospace(value);
    });
}

fn media_metrics_line(mm: &aos_proto::ModelMetrics, t: &i18n::UiStrings) -> String {
    let step = mm
        .media_step
        .map(|v| v.to_string())
        .unwrap_or_else(|| "—".into());
    let total = mm
        .media_total_steps
        .map(|v| v.to_string())
        .unwrap_or_else(|| "—".into());
    let step_s = mm
        .last_step_s
        .map(|v| format!("{v:.2}"))
        .unwrap_or_else(|| "—".into());
    format!(
        "{} {}/{} · {} {}",
        t.metrics_step, step, total, t.metrics_step_s, step_s
    )
}
