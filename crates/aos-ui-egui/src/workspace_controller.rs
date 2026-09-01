//! Workspace panel controller — notes / tasks / library event handlers.

use crate::notes_panel::{NoteDetail, NoteListItem, NoteRelatedHit, NoteSearchHit};
use crate::tasks_panel::TaskItem;
use crate::{i18n, UiApp};
use aos_proto::UserLibraryDoc;

impl UiApp {
    pub(crate) fn on_notes_raw(&mut self, s: String) {
        let is_new = self.workspace_ui.apply_notes_raw(s);
        if self.scenario_ui.pending_note_agent && is_new {
            self.scenario_ui.mark_note_agent();
        }
        self.scenario_ui.note_human = true;
    }

    pub(crate) fn on_notes_listed(&mut self, notes: Vec<NoteListItem>) {
        self.workspace_ui.apply_notes_listed(notes);
        self.scenario_ui.note_human = true;
    }

    pub(crate) fn on_note_loaded(&mut self, detail: NoteDetail) {
        self.workspace_ui.apply_note_loaded(detail);
    }

    pub(crate) fn on_notes_search_hits(&mut self, hits: Vec<NoteSearchHit>) {
        self.workspace_ui.apply_notes_search_hits(hits);
    }

    pub(crate) fn on_notes_related(&mut self, hits: Vec<NoteRelatedHit>) {
        self.workspace_ui.apply_notes_related(hits);
    }

    pub(crate) fn on_notes_saved(&mut self, path: String, slug: String, title: String) {
        self.workspace_ui.mark_note_saved(path, slug, title);
        self.scenario_ui.note_human = true;
    }

    pub(crate) fn on_user_library_listed(&mut self, docs: Vec<UserLibraryDoc>) {
        self.workspace_ui.set_library_docs(docs);
    }

    pub(crate) fn on_tasks_listed(&mut self, tasks: Vec<TaskItem>) {
        let t = i18n::strings(&self.prefs.language);
        self.workspace_ui
            .apply_tasks_listed(tasks, t.tasks_count);
    }
}
