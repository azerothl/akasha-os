//! User memory management panel.

use crate::cmd::Cmd;
use crate::{guide, i18n, icons, memory_relation_lines, overflow_scroll_h, UiApp};
use eframe::egui;
use std::collections::HashMap;

impl UiApp {
    pub(crate) fn ui_memory(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        let g = guide::strings(&self.prefs.language);
        ui.horizontal(|ui| {
            ui.heading(t.tab_memory);
            if guide::tab_help_button(ui, g.help_tooltip) {
                self.guide.open_topic(guide::GuideTopic::Memory);
            }
        });
        ui.weak(t.memory_blurb);
        if self.memory_ui.sweep_last_pass_ms > 0 && !self.memory_ui.sweep_last_pass_label.is_empty()
        {
            ui.weak(
                t.memory_updated_at
                    .replace("{}", &self.memory_ui.sweep_last_pass_label),
            );
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.memory_ui.note)
                    .desired_width(400.0)
                    .hint_text(t.memory_hint_remember),
            );
            if ui.button(t.memory_btn_remember).clicked() {
                self.send_mem_remember();
            }
            if ui.button(t.memory_btn_list).clicked() {
                self.send_mem_list();
            }
            if ui.button(t.memory_btn_wipe).clicked() {
                let _ = self.cmd_tx.send(Cmd::MemWipeUser);
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.memory_ui.query)
                    .desired_width(400.0)
                    .hint_text(t.memory_hint_recall),
            );
            if ui.button(t.memory_btn_recall).clicked() && !self.memory_ui.query.is_empty() {
                let _ = self.cmd_tx.send(Cmd::MemRecall {
                    query: self.memory_ui.query.clone(),
                });
            }
            ui.checkbox(
                &mut self.memory_ui.show_superseded,
                t.memory_show_superseded,
            );
        });
        if let Some(edit_id) = self.memory_ui.edit_id {
            ui.horizontal(|ui| {
                ui.label(format!("{} #{edit_id}", t.memory_editing));
                ui.add(
                    egui::TextEdit::singleline(&mut self.memory_ui.edit_text).desired_width(360.0),
                );
                if ui.button(t.memory_btn_save).clicked() && !self.memory_ui.edit_text.is_empty() {
                    let _ = self.cmd_tx.send(Cmd::MemEdit {
                        id: edit_id,
                        text: self.memory_ui.edit_text.clone(),
                    });
                    self.memory_ui.clear_edit();
                }
                if ui.button(t.memory_btn_supersede).clicked()
                    && !self.memory_ui.edit_text.is_empty()
                {
                    let _ = self.cmd_tx.send(Cmd::MemSupersede {
                        id: edit_id,
                        text: self.memory_ui.edit_text.clone(),
                    });
                    self.memory_ui.clear_edit();
                }
                if ui.button(t.memory_btn_cancel).clicked() {
                    self.memory_ui.clear_edit();
                }
            });
        }
        ui.separator();
        let mut edit_req: Option<(u64, String)> = None;
        let mut delete_id: Option<u64> = None;
        let mut supersede_req: Option<(u64, String)> = None;
        let visible_hits: Vec<_> = self
            .memory_ui
            .hits
            .iter()
            .filter(|h| {
                aos_proto::mem_extract::is_human_memory_fact(&h.text)
                    && (self.memory_ui.show_superseded || !h.superseded)
            })
            .collect();
        let fact_texts: HashMap<u64, String> = self
            .memory_ui
            .hits
            .iter()
            .map(|h| (h.id, h.text.clone()))
            .collect();
        let list_h = ui.available_height().max(120.0);
        overflow_scroll_h(ui, "memory_hits", list_h, |ui| {
            if visible_hits.is_empty() {
                ui.weak(t.memory_empty);
            }
            for h in visible_hits {
                ui.horizontal_wrapped(|ui| {
                    if h.pinned {
                        icons::pin_indicator(ui);
                    }
                    let mut fact = egui::RichText::new(h.text.trim());
                    if h.superseded {
                        fact = fact.weak().strikethrough();
                    }
                    ui.label(fact);
                });
                for line in memory_relation_lines(h, &fact_texts, &t) {
                    ui.weak(line);
                }
                ui.horizontal(|ui| {
                    if ui.small_button(t.memory_btn_edit).clicked() {
                        edit_req = Some((h.id, h.text.clone()));
                    }
                    if ui.small_button(t.memory_btn_replace).clicked() {
                        supersede_req = Some((h.id, h.text.clone()));
                    }
                    if ui.small_button(t.memory_btn_delete).clicked() {
                        delete_id = Some(h.id);
                    }
                });
                ui.add_space(6.0);
            }
        });
        if let Some((id, text)) = edit_req {
            self.memory_ui.begin_edit(id, text);
        }
        if let Some((id, text)) = supersede_req {
            self.memory_ui.begin_edit(id, text);
        }
        if let Some(id) = delete_id {
            let _ = self.cmd_tx.send(Cmd::MemDelete { id });
        }
    }
}
