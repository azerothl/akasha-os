//! Provider configuration panel.

use crate::cmd::Cmd;
use crate::{i18n, UiApp};
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
        for p in self.models_ui.providers.clone() {
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
                        self.models_ui.load_provider_for_edit(&p);
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
            ui.text_edit_singleline(&mut self.models_ui.provider_id);
        });
        ui.horizontal(|ui| {
            ui.label(t.providers_preset);
            egui::ComboBox::from_id_salt("provider_preset")
                .selected_text(&self.models_ui.provider_preset)
                .show_ui(ui, |ui| {
                    for &(name, endpoint, secret) in aos_proto::PROVIDER_PRESETS {
                        if ui
                            .selectable_label(self.models_ui.provider_preset == name, name)
                            .clicked()
                        {
                            self.models_ui
                                .apply_provider_preset(name, endpoint, secret);
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label(t.providers_endpoint);
            ui.add(
                egui::TextEdit::singleline(&mut self.models_ui.provider_endpoint)
                    .desired_width(420.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label(t.providers_secret);
            ui.text_edit_singleline(&mut self.models_ui.provider_secret_name);
        });
        ui.horizontal(|ui| {
            ui.label("API key (vault)");
            ui.add(
                egui::TextEdit::singleline(&mut self.models_ui.provider_secret_value)
                    .password(true)
                    .desired_width(280.0),
            );
        });
        ui.checkbox(&mut self.models_ui.provider_enabled, t.providers_enabled);
        ui.horizontal(|ui| {
            if ui.button(t.providers_save).clicked() {
                self.send_provider_upsert();
            }
            if ui.button(t.providers_test).clicked()
                && !self.models_ui.provider_id.trim().is_empty()
            {
                let _ = self.cmd_tx.send(Cmd::ProviderTest {
                    id: self.models_ui.provider_id.trim().to_string(),
                });
            }
        });
        if !self.models_ui.provider_test_msg.is_empty() {
            ui.label(&self.models_ui.provider_test_msg);
        }
    }
}
