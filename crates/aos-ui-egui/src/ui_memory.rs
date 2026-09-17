//! User memory management panel.

use crate::cmd::Cmd;
use crate::{guide, i18n, icons, memory_relation_lines, overflow_scroll_h, theme, UiApp};
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
            theme::add_form_field(
                ui,
                400.0,
                egui::TextEdit::singleline(&mut self.memory_ui.note)
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
            theme::add_form_field(
                ui,
                400.0,
                egui::TextEdit::singleline(&mut self.memory_ui.query)
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
        if self.memory_ui.v2_available {
            ui.separator();
            ui.heading(t.memory_decision_log);
            let decisions: Vec<_> = self
                .memory_ui
                .objects
                .iter()
                .filter(|object| matches!(&object.kind, aos_proto::MemoryObjectKind::Decision))
                .take(32)
                .collect();
            if decisions.is_empty() {
                ui.weak(t.memory_decision_empty);
            }
            for object in decisions {
                ui.horizontal_wrapped(|ui| {
                    let label = if object.title.trim().is_empty() {
                        object.content.trim()
                    } else {
                        object.title.trim()
                    };
                    ui.strong(label);
                    ui.weak(format!(
                        "#{} · {:?} · {} · {}",
                        object.id,
                        object.status,
                        t.memory_decision_confidence
                            .replace("{:.0}", &format!("{:.0}", object.confidence * 100.0)),
                        t.memory_decision_sources
                            .replace("{}", &object.source_refs.len().to_string())
                    ));
                });
                if let Some(decision) = object.decision.as_ref() {
                    if let Some(selected) = decision.selected_option.as_deref() {
                        ui.label(t.memory_decision_selected.replace("{}", selected));
                    }
                    if let Some(rationale) = decision.rationale.as_deref() {
                        if !rationale.trim().is_empty() {
                            ui.weak(t.memory_decision_rationale.replace("{}", rationale.trim()));
                        }
                    }
                }
                ui.add_space(6.0);
            }

            ui.separator();
            ui.heading(t.memory_mind_palace);
            ui.horizontal(|ui| {
                theme::add_form_field(
                    ui,
                    300.0,
                    egui::TextEdit::singleline(&mut self.memory_ui.palace_namespace)
                        .hint_text(t.memory_mind_palace_namespace),
                );
                theme::add_form_field(
                    ui,
                    220.0,
                    egui::TextEdit::singleline(&mut self.memory_ui.palace_project)
                        .hint_text("project (metadata)"),
                );
                if ui.button(t.memory_mind_palace_explore).clicked() {
                    let namespace = self.memory_ui.palace_namespace.trim().to_string();
                    let project = self.memory_ui.palace_project.trim().to_string();
                    let _ = self.cmd_tx.send(Cmd::MemMindPalace {
                        namespace: (!namespace.is_empty()).then_some(namespace),
                        project: (!project.is_empty()).then_some(project),
                        root_id: None,
                    });
                }
            });
            if let Some(palace) = self.memory_ui.palace.as_ref() {
                ui.weak(
                    t.memory_mind_palace_summary
                        .replacen("{}", &palace.objects.len().to_string(), 1)
                        .replacen("{}", &palace.relations.len().to_string(), 1),
                );
                if palace.objects.is_empty() {
                    ui.weak(t.memory_mind_palace_empty);
                } else {
                    overflow_scroll_h(ui, "memory_mind_palace", 220.0, |ui| {
                        for object in palace.objects.iter().take(64) {
                            ui.horizontal_wrapped(|ui| {
                                let label = if object.title.trim().is_empty() {
                                    object.content.trim()
                                } else {
                                    object.title.trim()
                                };
                                ui.strong(label);
                                ui.weak(format!(
                                    "#{} · {:?} · {:?} · {} source(s)",
                                    object.id,
                                    object.kind,
                                    object.status,
                                    object.source_refs.len()
                                ));
                            });
                        }
                    });
                }
            }
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
