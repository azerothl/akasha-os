//! Workspace panel controller — notes / tasks / library event handlers.

use crate::notes_panel::{NoteDetail, NoteListItem, NoteRelatedHit, NoteSearchHit};
use crate::tasks_panel::TaskItem;
use crate::{i18n, UiApp};
use aos_proto::UserLibraryDoc;

impl UiApp {
    pub(crate) fn on_notes_raw(&mut self, s: String) {
        self.workspace_ui.apply_notes_raw(s);
    }

    pub(crate) fn on_notes_listed(&mut self, notes: Vec<NoteListItem>) {
        let had_pending_note = self.scenario_ui.pending_note_agent;
        let t = i18n::strings(&self.prefs.language);
        self.workspace_ui.apply_notes_listed(notes, t.notes_count);
        if !self.workspace_ui.notes.notes.is_empty() {
            self.scenario_ui.note_human = true;
            if had_pending_note {
                self.scenario_ui.mark_note_agent();
            }
        } else if had_pending_note {
            self.scenario_ui.pending_note_agent = false;
        }
    }

    pub(crate) fn on_note_loaded(&mut self, detail: NoteDetail) {
        self.workspace_ui.apply_note_loaded(detail);
    }

    pub(crate) fn on_notes_search_hits(&mut self, hits: Vec<NoteSearchHit>) {
        let t = i18n::strings(&self.prefs.language);
        self.workspace_ui
            .apply_notes_search_hits(hits, t.notes_search_count);
    }

    pub(crate) fn on_notes_related(&mut self, hits: Vec<NoteRelatedHit>) {
        let t = i18n::strings(&self.prefs.language);
        self.workspace_ui
            .apply_notes_related(hits, t.notes_related_count);
    }

    pub(crate) fn on_notes_saved(&mut self, path: String, slug: String, title: String) {
        let t = i18n::strings(&self.prefs.language);
        self.workspace_ui.mark_note_saved(path, slug, title, t.notes_status_saved);
        if !self.workspace_ui.notes.notes.is_empty() {
            self.scenario_ui.note_human = true;
        }
    }

    pub(crate) fn on_notes_save_failed(
        &mut self,
        title: String,
        content: String,
        path: Option<String>,
    ) {
        self.workspace_ui
            .mark_note_save_failed(title, content, path);
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
