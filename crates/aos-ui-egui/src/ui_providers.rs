//! Provider configuration panel.

use crate::cmd::Cmd;
use crate::{i18n, UiApp};
use aos_proto::ProviderRecord;
use eframe::egui;

impl UiApp {
pub(crate) fn ui_providers(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        ui.heading(t.tab_providers);
        ui.weak(t.providers_blurb);
        ui.separator();
        if ui.button(t.providers_refresh).clicked() {
            let _ = self.cmd_tx.send(Cmd::ProviderList);
        }
        ui.add_space(6.0);
        for p in self.providers.clone() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.strong(&p.id);
                    ui.weak(&p.preset);
                    ui.label(&p.endpoint);
                    if p.enabled {
                        ui.weak(t.providers_on);
                    } else {
                        ui.weak(t.providers_off);
                    }
                    if ui.button(t.providers_test).clicked() {
                        let _ = self.cmd_tx.send(Cmd::ProviderTest { id: p.id.clone() });
                    }
                    if ui.button(t.providers_remove).clicked() {
                        let _ = self.cmd_tx.send(Cmd::ProviderRemove { id: p.id.clone() });
                    }
                    if ui.button(t.providers_edit).clicked() {
                        self.provider_id = p.id.clone();
                        self.provider_preset = p.preset.clone();
                        self.provider_endpoint = p.endpoint.clone();
                        self.provider_secret_name =
                            p.secret_name.clone().unwrap_or_default();
                        self.provider_enabled = p.enabled;
                    }
                });
                if !p.discovered_models.is_empty() {
                    ui.weak(p.discovered_models.join(", "));
                }
            });
        }
        ui.separator();
        ui.label(t.providers_add_edit);
        ui.horizontal(|ui| {
            ui.label("id");
            ui.text_edit_singleline(&mut self.provider_id);
        });
        ui.horizontal(|ui| {
            ui.label(t.providers_preset);
            egui::ComboBox::from_id_salt("provider_preset")
                .selected_text(&self.provider_preset)
                .show_ui(ui, |ui| {
                    for &(name, endpoint, secret) in aos_proto::PROVIDER_PRESETS {
                        if ui
                            .selectable_label(self.provider_preset == name, name)
                            .clicked()
                        {
                            self.provider_preset = name.into();
                            if self.provider_id.is_empty() {
                                self.provider_id = name.into();
                            }
                            if !endpoint.is_empty() {
                                self.provider_endpoint = endpoint.into();
                            }
                            if let Some(s) = secret {
                                self.provider_secret_name = s.into();
                            }
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label(t.providers_endpoint);
            ui.add(
                egui::TextEdit::singleline(&mut self.provider_endpoint).desired_width(420.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label(t.providers_secret);
            ui.text_edit_singleline(&mut self.provider_secret_name);
        });
        ui.horizontal(|ui| {
            ui.label("API key (vault)");
            ui.add(
                egui::TextEdit::singleline(&mut self.provider_secret_value)
                    .password(true)
                    .desired_width(280.0),
            );
        });
        ui.checkbox(&mut self.provider_enabled, t.providers_enabled);
        ui.horizontal(|ui| {
            if ui.button(t.providers_save).clicked() && !self.provider_id.trim().is_empty() {
                let rec = ProviderRecord {
                    id: self.provider_id.trim().to_string(),
                    preset: self.provider_preset.clone(),
                    endpoint: self.provider_endpoint.trim().to_string(),
                    secret_name: if self.provider_secret_name.trim().is_empty() {
                        None
                    } else {
                        Some(self.provider_secret_name.trim().to_string())
                    },
                    enabled: self.provider_enabled,
                    discovered_models: Vec::new(),
                };
                let secret = if self.provider_secret_value.is_empty() {
                    None
                } else {
                    Some(std::mem::take(&mut self.provider_secret_value))
                };
                let _ = self.cmd_tx.send(Cmd::ProviderUpsert {
                    provider: rec,
                    secret_value: secret,
                });
            }
            if ui.button(t.providers_test).clicked() && !self.provider_id.trim().is_empty() {
                let _ = self.cmd_tx.send(Cmd::ProviderTest {
                    id: self.provider_id.trim().to_string(),
                });
            }
        });
        if !self.provider_test_msg.is_empty() {
            ui.label(&self.provider_test_msg);
        }
    }

}

