//! Tasks, library, and notes workspace panels.

use crate::cmd::Cmd;
use crate::{guide, i18n, library_panel, notes_panel, os_open, Tab, UiApp};
use eframe::egui;

impl UiApp {
    pub(crate) fn ui_tasks(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        ui.weak(t.tasks_blurb);
        ui.separator();
        let actions = self.workspace_ui.tasks.ui(ui, &t);
        if actions.list {
            let _ = self.cmd_tx.send(Cmd::TasksList);
        }
        if let Some((title, notes)) = actions.create {
            let _ = self.cmd_tx.send(Cmd::TasksCreate { title, notes });
        }
        if let Some((id, done)) = actions.complete {
            let _ = self.cmd_tx.send(Cmd::TasksComplete { id, done });
        }
    }

    pub(crate) fn ui_library(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        let g = guide::strings(&self.prefs.language);
        ui.horizontal(|ui| {
            ui.strong(t.tab_library);
            if guide::tab_help_button(ui, g.help_tooltip) {
                self.guide.open_topic(guide::GuideTopic::Library);
            }
        });
        let actions = library_panel::render(ui, &t, &self.workspace_ui.library);
        if actions.add_clicked {
            let filters = [(t.tab_library, aos_proto::chat_document::CHAT_DOCUMENT_EXTENSIONS)];
            if let Some(path) = os_open::pick_os_file(
                t.tab_library,
                &filters,
                os_open::user_downloads_dir().as_deref(),
            ) {
                let _ = self.cmd_tx.send(Cmd::UserLibraryAdd {
                    path: path.to_string_lossy().into_owned(),
                });
            }
        }
        if let Some(id) = actions.remove_id {
            let _ = self.cmd_tx.send(Cmd::UserLibraryRemove { id });
        }
    }

    pub(crate) fn ui_notes(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        ui.weak(t.notes_blurb);
        ui.separator();
        let actions = notes_panel::show_notes_panel(ui, &mut self.workspace_ui.notes, &t);
        if actions.list {
            let _ = self.cmd_tx.send(Cmd::NotesList);
        }
        if let Some(query) = actions.search {
            let _ = self.cmd_tx.send(Cmd::NotesSearch { query });
        }
        if let Some(path) = actions.read_path {
            let title = actions.read_title.clone().or_else(|| {
                self.workspace_ui
                    .notes
                    .notes
                    .iter()
                    .find(|n| n.path == path)
                    .map(|n| n.title.clone())
            });
            let slug = self
                .workspace_ui
                .notes
                .notes
                .iter()
                .find(|n| n.path == path)
                .map(|n| n.slug.clone());
            let _ = self.cmd_tx.send(Cmd::NotesRead {
                title,
                path: Some(path),
                slug,
            });
        } else if let Some(title) = actions.read_title {
            let _ = self.cmd_tx.send(Cmd::NotesRead {
                title: Some(title),
                path: None,
                slug: None,
            });
        }
        if let Some((title, content)) = actions.save_create {
            let _ = self.cmd_tx.send(Cmd::NotesCreate { title, content });
        }
        if let Some((title, path, content)) = actions.save_update {
            let _ = self.cmd_tx.send(Cmd::NotesUpdate {
                title,
                path,
                content,
            });
        }
        if let Some(path) = actions.attach_path {
            self.agent_ui.attach_document(path);
            self.tab = Tab::Agents;
            self.status = "Note jointe — créez un agent avec ce document.".into();
        }
        if let Some((path, topic)) = actions.related {
            let _ = self.cmd_tx.send(Cmd::NotesRelated { path, topic });
        }
    }
}
