//! Feedback form behavior and rendering.

use crate::cmd::Cmd;
use crate::os_open::{aos_home, open_os_folder, pick_os_file};
use crate::{i18n, UiApp};
use aos_proto::{FeedbackAttachment, FeedbackSubmitRequest};
use eframe::egui;

impl UiApp {
    pub(crate) fn reset_feedback_form(&mut self) {
        self.feedback_ui.reset_form();
    }

    pub(crate) fn ui_feedback(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        ui.heading(t.feedback_heading);
        ui.label(t.feedback_blurb);
        let previous_category = self.feedback_ui.category.clone();
        ui.horizontal(|ui| {
            ui.label(t.feedback_title);
            ui.text_edit_singleline(&mut self.feedback_ui.title);
        });
        if self.feedback_ui.category != previous_category {
            let category = self.feedback_ui.category.clone();
            self.feedback_ui.select_category(&category);
        }
        ui.horizontal(|ui| {
            ui.label(t.feedback_category);
            egui::ComboBox::from_id_salt("fb_cat")
                .selected_text(&self.feedback_ui.category)
                .show_ui(ui, |ui| {
                    for (code, label) in [
                        ("bug", t.feedback_cat_bug),
                        ("ux", t.feedback_cat_ux),
                        ("perf", t.feedback_cat_perf),
                        ("security", t.feedback_cat_security),
                        ("other", t.feedback_cat_other),
                    ] {
                        ui.selectable_value(&mut self.feedback_ui.category, code.into(), label);
                    }
                });
            ui.label(t.feedback_severity);
            egui::ComboBox::from_id_salt("fb_sev")
                .selected_text(&self.feedback_ui.severity)
                .show_ui(ui, |ui| {
                    for (code, label) in [
                        ("low", t.feedback_sev_low),
                        ("medium", t.feedback_sev_medium),
                        ("high", t.feedback_sev_high),
                    ] {
                        ui.selectable_value(&mut self.feedback_ui.severity, code.into(), label);
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label(t.feedback_scenario);
            ui.text_edit_singleline(&mut self.feedback_ui.scenario);
        });
        ui.add(
            egui::TextEdit::multiline(&mut self.feedback_ui.body)
                .desired_width(f32::INFINITY)
                .desired_rows(10),
        );
        ui.horizontal(|ui| {
            if ui.button(t.feedback_add_file).clicked()
                && self.feedback_ui.attachments.len() < 8
            {
                if let Some(path) = pick_os_file(t.feedback_add_file_dialog, &[], None) {
                    self.feedback_ui.attachments.push(path);
                }
            }
            ui.label(
                t.feedback_attachments
                    .replace("{n}", &self.feedback_ui.attachments.len().to_string()),
            );
        });
        let mut remove = None;
        for (index, path) in self.feedback_ui.attachments.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(t.feedback_file_fallback),
                );
                if ui.small_button(t.feedback_remove).clicked() {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = remove {
            self.feedback_ui.attachments.remove(index);
        }
        ui.weak(t.feedback_no_secrets);
        if ui.button(format!("⎙ {}", t.btn_copy)).clicked() {
            ui.ctx().copy_text(self.feedback_ui.body.clone());
            self.status = t.copied.into();
        }
        let security = self.feedback_ui.category.eq_ignore_ascii_case("security");
        if security {
            self.feedback_ui.publish_github = false;
            ui.weak(t.feedback_security_local);
        } else {
            ui.checkbox(
                &mut self.feedback_ui.publish_github,
                t.feedback_github_issue,
            );
            if self.feedback_ui.publish_github && !self.network_online {
                ui.weak(t.feedback_network_off_github);
            }
        }
        let template_complete = self.feedback_ui.template_complete();
        if !template_complete {
            ui.weak(t.feedback_complete_template);
        }
        if ui.button(t.feedback_send).clicked()
            && !self.feedback_ui.title.is_empty()
            && template_complete
        {
            let mut meta = serde_json::json!({
                "preview_version": self.version,
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "scenarios": {
                    "chat_offline": self.scenario_ui.chat,
                    "note_human": self.scenario_ui.note_human,
                    "note_agent": self.scenario_ui.note_agent,
                    "confirm": self.scenario_ui.confirm,
                    "audit": self.scenario_ui.audit,
                    "module_agent": self.scenario_ui.module_agent,
                },
                "onboarding": self.onboarding,
            });
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
                attachments: self
                    .feedback_ui
                    .attachments
                    .iter()
                    .map(|path| FeedbackAttachment {
                        path: path.to_string_lossy().into_owned(),
                    })
                    .collect(),
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
