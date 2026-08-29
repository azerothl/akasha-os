//! More → Library / Bibliothèque — personal document list (separate from research docs).

use aos_proto::UserLibraryDoc;
use eframe::egui::{self, Ui};

use crate::i18n::UiStrings;

#[derive(Debug, Default)]
pub struct LibraryPanelState {
    pub docs: Vec<UserLibraryDoc>,
}

#[derive(Debug, Default)]
pub struct LibraryPanelActions {
    pub add_clicked: bool,
    pub remove_id: Option<String>,
}

pub fn render(ui: &mut Ui, t: &UiStrings, state: &LibraryPanelState) -> LibraryPanelActions {
    let mut actions = LibraryPanelActions::default();
    if ui.button(t.library_add).clicked() {
        actions.add_clicked = true;
    }
    ui.separator();
    if state.docs.is_empty() {
        ui.weak(t.library_empty);
    } else {
        for doc in &state.docs {
            ui.horizontal(|ui| {
                ui.strong(&doc.label);
                if !doc.added_date.is_empty() {
                    ui.weak(&doc.added_date);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(t.library_remove).clicked() {
                        actions.remove_id = Some(doc.id.clone());
                    }
                });
            });
            ui.add_space(4.0);
        }
    }
    actions
}
