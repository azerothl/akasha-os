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
                            self.models_ui.apply_provider_preset(name, endpoint, secret);
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
        // S2 : budget cloud mensuel (estimation chat, tarifs indicatifs).
        ui.separator();
        let fr = self.prefs.language == "fr";
        ui.heading(if fr { "Budget cloud" } else { "Cloud budget" });
        ui.weak(if fr {
            "Estimation ≈ depuis les turns chat sur providers non-locaux (caractères/4). Loopback (Ollama/vLLM/LM Studio) = 0. Agents exclus en phase 1."
        } else {
            "≈ estimate from chat turns on non-local providers (chars/4). Loopback (Ollama/vLLM/LM Studio) = 0. Agents excluded in phase 1."
        });
        ui.horizontal(|ui| {
            ui.label(if fr { "Plafond mensuel" } else { "Monthly cap" });
            let mut euros = self.prefs.cloud_cap_cents as f32 / 100.0;
            if ui
                .add(
                    egui::DragValue::new(&mut euros)
                        .range(0.0..=100_000.0)
                        .speed(5.0)
                        .suffix(" €"),
                )
                .on_hover_text(if fr { "0 = illimité" } else { "0 = unlimited" })
                .changed()
            {
                self.prefs.cloud_cap_cents = (euros.max(0.0) * 100.0).round() as u32;
                crate::prefs::save_preferences(&self.prefs);
            }
        });
        ui.horizontal(|ui| {
            ui.label(if fr { "Alerte à" } else { "Alert at" });
            let mut pct = self.prefs.cloud_alert_pct as u32;
            if ui
                .add(egui::DragValue::new(&mut pct).range(10..=100).suffix(" %"))
                .changed()
            {
                self.prefs.cloud_alert_pct = pct.clamp(10, 100) as u8;
                crate::prefs::save_preferences(&self.prefs);
            }
            let mut cut = self.prefs.cloud_cut_at_cap;
            if ui
                .checkbox(
                    &mut cut,
                    if fr {
                        "Couper (local_only forcé)"
                    } else {
                        "Cut (force local_only)"
                    },
                )
                .changed()
            {
                self.prefs.cloud_cut_at_cap = cut;
                crate::prefs::save_preferences(&self.prefs);
            }
        });
        ui.horizontal(|ui| {
            ui.label(format!(
                "{} · {} · {} turns",
                self.billing.month,
                crate::billing::format_cents(self.billing.cents, fr),
                self.billing.turns,
            ));
            if ui
                .small_button(if fr {
                    "Réinitialiser le mois"
                } else {
                    "Reset month"
                })
                .clicked()
            {
                self.billing = crate::billing::BillingLedger {
                    month: crate::billing::current_month_key(),
                    ..Default::default()
                };
                crate::billing::save_ledger(&self.billing);
            }
        });
    }
}
