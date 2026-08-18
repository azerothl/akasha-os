//! Onglet Tasks — liste dual-surface (humain + agent).

use eframe::egui::{self, Ui};
use serde::{Deserialize, Serialize};

use crate::i18n::UiStrings;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskItem {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub done: bool,
}

#[derive(Debug, Default)]
pub struct TasksActions {
    pub list: bool,
    pub create: Option<(String, String)>,
    pub complete: Option<(String, bool)>,
}

#[derive(Default)]
pub struct TasksPanelState {
    pub tasks: Vec<TaskItem>,
    pub new_title: String,
    pub new_notes: String,
    pub status: String,
}

impl TasksPanelState {
    pub fn apply_listed(&mut self, tasks: Vec<TaskItem>, count_tpl: &str) {
        self.tasks = tasks;
        self.status = count_tpl.replace("{n}", &self.tasks.len().to_string());
    }

    pub fn ui(&mut self, ui: &mut Ui, t: &UiStrings) -> TasksActions {
        let mut actions = TasksActions::default();
        ui.heading(t.tab_tasks);
        ui.horizontal(|ui| {
            if ui.button(t.decl_ui_refresh).clicked() {
                actions.list = true;
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(t.tasks_new);
            ui.add(
                egui::TextEdit::singleline(&mut self.new_title)
                    .desired_width(220.0)
                    .hint_text(t.tasks_title_hint),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.new_notes)
                    .desired_width(180.0)
                    .hint_text(t.tasks_notes_hint),
            );
            if ui.button(t.tasks_create).clicked() && !self.new_title.trim().is_empty() {
                actions.create = Some((self.new_title.trim().to_string(), self.new_notes.clone()));
                self.new_title.clear();
                self.new_notes.clear();
            }
        });
        ui.separator();
        if self.tasks.is_empty() {
            ui.weak(t.tasks_empty);
        } else {
            egui::ScrollArea::vertical()
                .id_salt("tasks_list")
                .max_height(420.0)
                .show(ui, |ui| {
                    for item in &self.tasks {
                        ui.horizontal(|ui| {
                            let label = if item.done {
                                format!("✓ {}", item.title)
                            } else {
                                item.title.clone()
                            };
                            ui.label(&label);
                            if !item.notes.is_empty() {
                                ui.weak(&item.notes);
                            }
                            ui.monospace(&item.id);
                            let btn = if item.done {
                                t.tasks_reopen
                            } else {
                                t.tasks_complete
                            };
                            if ui.small_button(btn).clicked() {
                                actions.complete = Some((item.id.clone(), !item.done));
                            }
                        });
                    }
                });
        }
        if !self.status.is_empty() {
            ui.weak(&self.status);
        }
        actions
    }
}

pub fn parse_list_result(v: &serde_json::Value) -> Vec<TaskItem> {
    v.get("tasks")
        .and_then(|t| serde_json::from_value(t.clone()).ok())
        .unwrap_or_default()
}
