//! More → Library / Bibliothèque — personal document list (separate from research docs).

use aos_proto::UserLibraryDoc;
use eframe::egui::Ui;

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
    ui.heading(t.tab_library);
    ui.label(t.library_hint);
    ui.add_space(6.0);
    if ui.button(t.library_add).clicked() {
        actions.add_clicked = true;
    }
    ui.separator();
    if state.docs.is_empty() {
        ui.weak(t.library_empty);
    } else {
        for doc in &state.docs {
            ui.horizontal(|ui| {
                ui.label(&doc.label);
                if ui.small_button(t.library_remove).clicked() {
                    actions.remove_id = Some(doc.id.clone());
                }
            });
            ui.weak(format_size(doc.size_bytes));
            ui.add_space(4.0);
        }
    }
    actions
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_size_units() {
        assert_eq!(format_size(500), "500 B");
        assert!(format_size(2048).contains("KB"));
    }
}
