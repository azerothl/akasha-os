//! Feedback form behavior and rendering.

use crate::cmd::Cmd;
use crate::os_open::{aos_home, open_os_folder};
use crate::{i18n, UiApp};
use aos_proto::FeedbackSubmitRequest;
use eframe::egui;

impl UiApp {
    pub(crate) fn reset_feedback_form(&mut self) {
        self.feedback_ui.reset_form();
    }

    pub(crate) fn ui_feedback(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        ui.heading(t.feedback_heading);
        ui.label(t.feedback_blurb);
        ui.horizontal(|ui| {
            ui.label(t.feedback_title);
            ui.text_edit_singleline(&mut self.feedback_ui.title);
        });
        ui.horizontal(|ui| {
            ui.label(t.feedback_category);
            egui::ComboBox::from_id_salt("fb_cat")
                .selected_text(&self.feedback_ui.category)
                .show_ui(ui, |ui| {
                    for c in ["bug", "ux", "perf", "security", "other"] {
                        ui.selectable_value(&mut self.feedback_ui.category, c.into(), c);
                    }
                });
            ui.label(t.feedback_severity);
            egui::ComboBox::from_id_salt("fb_sev")
                .selected_text(&self.feedback_ui.severity)
                .show_ui(ui, |ui| {
                    for s in ["low", "medium", "high"] {
                        ui.selectable_value(&mut self.feedback_ui.severity, s.into(), s);
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label(t.feedback_scenario);
            ui.text_edit_singleline(&mut self.feedback_ui.scenario);
        });
        ui.text_edit_multiline(&mut self.feedback_ui.body);
        let t = i18n::strings(&self.prefs.language);
        if ui.button(t.btn_copy).clicked() {
            ui.ctx().copy_text(self.feedback_ui.body.clone());
            self.status = t.copied.into();
        }
        let security = self.feedback_ui.category.eq_ignore_ascii_case("security");
        if security {
            self.feedback_ui.publish_github = false;
            ui.weak(
                "Les rapports security restent locaux (pas d'issue publique). Utilisez GitHub Security Advisories.",
            );
        } else {
            ui.checkbox(&mut self.feedback_ui.publish_github, "Créer une issue GitHub");
            if self.feedback_ui.publish_github && !self.network_online {
                ui.weak(
                    "Réseau in-app coupé : le navigateur ouvrira le formulaire GitHub (compte GitHub requis).",
                );
            }
        }
        if ui.button("Envoyer le retour").clicked() && !self.feedback_ui.title.is_empty() {
            let mut meta = serde_json::json!({
                "preview_version": self.version,
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "scenarios": {
                    "chat_offline": self.scen_chat,
                    "note_human": self.scen_note_human,
                    "note_agent": self.scen_note_agent,
                    "confirm": self.scen_confirm,
                    "audit": self.scen_audit,
                    "module_agent": self.scen_module_agent,
                },
                "onboarding": self.onboarding,
            });
            // Fusionner les champs du rapport de dépannage (source, findings, healthy)
            // pour qu'ils figurent dans l'issue GitHub remontée.
            if let Some(diag) = &self.feedback_ui.diag_meta {
                if let (Some(m), Some(d)) = (meta.as_object_mut(), diag.as_object()) {
                    for (k, v) in d {
                        m.entry(k).or_insert_with(|| v.clone());
                    }
                }
            }
            let _ = self.cmd_tx.send(Cmd::Feedback(FeedbackSubmitRequest {
                title: self.feedback_ui.title.clone(),
                category: self.feedback_ui.category.clone(),
                severity: self.feedback_ui.severity.clone(),
                body: self.feedback_ui.body.clone(),
                scenario: if self.feedback_ui.scenario.is_empty() {
                    None
                } else {
                    Some(self.feedback_ui.scenario.clone())
                },
                meta,
                publish_github: self.feedback_ui.publish_github && !security,
            }));
        }
        if !self.feedback_ui.result.is_empty() {
            ui.separator();
            ui.label(&self.feedback_ui.result);
            if ui.button(t.feedback_open_folder).clicked() {
                let dir = self
                    .feedback_ui
                    .export_dir
                    .clone()
                    .unwrap_or_else(|| aos_home().join("var/feedback"));
                open_os_folder(&dir);
            }
        }
    }
}
