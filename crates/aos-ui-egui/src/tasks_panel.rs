//! Onglet Tasks — liste dual-surface (humain + agent).

use eframe::egui::{self, Ui};
use serde::{Deserialize, Serialize};

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
    pub fn apply_listed(&mut self, tasks: Vec<TaskItem>) {
        self.tasks = tasks;
        self.status = format!("{} tâche(s)", self.tasks.len());
    }

    pub fn ui(&mut self, ui: &mut Ui, heading: &str) -> TasksActions {
        let mut actions = TasksActions::default();
        ui.heading(heading);
        ui.horizontal(|ui| {
            if ui.button("Rafraîchir").clicked() {
                actions.list = true;
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Nouvelle");
            ui.add(
                egui::TextEdit::singleline(&mut self.new_title)
                    .desired_width(220.0)
                    .hint_text("titre"),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.new_notes)
                    .desired_width(180.0)
                    .hint_text("notes"),
            );
            if ui.button("Créer").clicked() && !self.new_title.trim().is_empty() {
                actions.create = Some((self.new_title.trim().to_string(), self.new_notes.clone()));
                self.new_title.clear();
                self.new_notes.clear();
            }
        });
        ui.separator();
        if self.tasks.is_empty() {
            ui.weak("Aucune tâche — créez-en une ou demandez à un agent `tasks.create`.");
        } else {
            egui::ScrollArea::vertical()
                .id_salt("tasks_list")
                .max_height(420.0)
                .show(ui, |ui| {
                    for t in &self.tasks {
                        ui.horizontal(|ui| {
                            let label = if t.done {
                                format!("✓ {}", t.title)
                            } else {
                                t.title.clone()
                            };
                            ui.label(&label);
                            if !t.notes.is_empty() {
                                ui.weak(&t.notes);
                            }
                            ui.monospace(&t.id);
                            let btn = if t.done { "Réouvrir" } else { "Terminer" };
                            if ui.small_button(btn).clicked() {
                                actions.complete = Some((t.id.clone(), !t.done));
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
