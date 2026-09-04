//! Audit journal and capability-management panels.

use crate::cmd::Cmd;
use crate::{agent_cap_holder, i18n, overflow_scroll_h, UiApp};
use aos_proto::CapInfo;
use eframe::egui;

impl UiApp {
pub(crate) fn ui_audit(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        ui.heading(t.audit_heading);
        ui.horizontal(|ui| {
            if ui.button(t.decl_ui_refresh).clicked() {
                let _ = self.cmd_tx.send(Cmd::Audit { last: 50 });
            }
            if ui.button(t.audit_kill_p4).clicked() {
                let _ = self.cmd_tx.send(Cmd::KillAuditd);
            }
        });
        let list_h = ui.available_height().max(120.0);
        overflow_scroll_h(ui, "audit_list", list_h, |ui| {
            for e in &self.security_ui.audit {
                ui.monospace(format!(
                    "#{} {} {} {} → {}",
                    e.seq, e.actor, e.action, e.target, e.hash
                ));
            }
        });
    }

    pub(crate) fn ui_caps(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        ui.heading(t.caps_heading);
        ui.weak(t.caps_blurb);
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(t.caps_subject);
            ui.add(
                egui::TextEdit::singleline(&mut self.security_ui.caps_holder)
                    .desired_width(280.0)
                    .hint_text("agent:<id>"),
            );
            if ui
                .button(t.caps_refresh)
                .on_hover_text(t.tip_caps_refresh)
                .clicked()
                && !self.security_ui.caps_holder.trim().is_empty()
            {
                let holder = self.security_ui.caps_holder.trim().to_string();
                self.security_ui.select_holder(holder.clone());
                let _ = self.cmd_tx.send(Cmd::CapList { holder });
            }
        });
        if let Some(id) = self.agent_ui.active_tab.clone() {
            ui.horizontal(|ui| {
                let holder = agent_cap_holder(&id);
                ui.weak(format!("Agent actif → {holder}"));
                if ui.small_button("Charger").clicked() {
                    self.security_ui.select_holder(holder.clone());
                    let _ = self.cmd_tx.send(Cmd::CapList { holder });
                }
            });
        }
        ui.separator();
        let holder = self.security_ui.caps_holder.clone();
        self.draw_caps_list(ui, &holder);
        if !self.security_ui.device_active.is_empty() {
            ui.separator();
            ui.heading(t.device_active_heading);
            for capture in self.security_ui.device_active.clone() {
                ui.horizontal(|ui| {
                    ui.label(format!("{} · {} · {} ms", capture.agent_id, capture.device_id, capture.duration_ms));
                    if ui.button(t.device_stop).clicked() {
                        let _ = self.cmd_tx.send(Cmd::DeviceCaptureStop {
                            agent_id: capture.agent_id.clone(),
                            capture_id: capture.capture_id.clone(),
                        });
                    }
                });
            }
        }
        if !self.security_ui.device_permissions.is_empty() {
            ui.separator();
            ui.heading(t.device_permissions_heading);
            ui.weak(t.device_permissions_blurb);
            for permission in self.security_ui.device_permissions.clone() {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{} · {} · {}",
                            permission.agent_id, permission.device_id, permission.capability
                        ));
                        if ui.small_button(t.caps_revoke).clicked() {
                            let _ = self.cmd_tx.send(Cmd::DevicePermissionRevoke {
                                agent_id: permission.agent_id.clone(),
                                device_id: permission.device_id.clone(),
                                kind: permission.kind,
                                mode: permission.mode,
                            });
                        }
                    });
                });
            }
        }
    }

    pub(crate) fn draw_caps_list(&mut self, ui: &mut egui::Ui, holder: &str) {
        let t = i18n::strings(&self.prefs.language);
        let matching: Vec<CapInfo> = if holder.is_empty() {
            self.security_ui.caps.clone()
        } else {
            self.security_ui
                .caps
                .iter()
                .filter(|c| c.holder == holder)
                .cloned()
                .collect()
        };
        if matching.is_empty() {
            if let Some(agent_id) = holder.strip_prefix("agent:") {
                if let Some(info) = self.agents.iter().find(|a| a.agent_id == agent_id) {
                    if !info.caps.is_empty() {
                        ui.weak(t.caps_logical);
                        for c in &info.caps {
                            ui.monospace(c);
                        }
                        return;
                    }
                }
            }
            ui.weak(t.caps_empty);
            return;
        }
        let list_h = ui.available_height().max(120.0);
        overflow_scroll_h(ui, format!("caps_list_{holder}"), list_h, |ui| {
            for c in matching {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.monospace(format!("#{}", c.cap_id));
                        ui.label(&c.object);
                        ui.weak(c.rights.join(", "));
                        if ui
                            .small_button(t.caps_revoke)
                            .on_hover_text(t.tip_caps_revoke)
                            .clicked()
                        {
                            let _ = self.cmd_tx.send(Cmd::CapRevoke {
                                holder: c.holder.clone(),
                                cap_id: c.cap_id,
                                tree: false,
                            });
                        }
                    });
                });
            }
        });
    }

}
