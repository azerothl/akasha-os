//! Mutable state owned by the Tasks / Library / Notes workspace panels.

use crate::library_panel::LibraryPanelState;
use crate::notes_panel::{NoteDetail, NoteListItem, NoteRelatedHit, NoteSearchHit, NotesPanelState};
use crate::tasks_panel::{TaskItem, TasksPanelState};
use aos_proto::UserLibraryDoc;

#[derive(Default)]
pub(crate) struct WorkspaceUiState {
    pub(crate) notes: NotesPanelState,
    /// Last raw notes payload (scenarios / debug).
    pub(crate) notes_out: String,
    pub(crate) tasks: TasksPanelState,
    pub(crate) library: LibraryPanelState,
}

impl WorkspaceUiState {
    /// Apply a raw notes dump. Returns true when the payload looks newly produced
    /// (useful for scenario detection of agent-authored notes).
    pub(crate) fn apply_notes_raw(&mut self, s: String) -> bool {
        let is_new = !s.is_empty() && !s.contains("aucune note") && s != self.notes_out;
        self.notes_out = s;
        is_new
    }

    pub(crate) fn apply_notes_listed(&mut self, notes: Vec<NoteListItem>) {
        self.notes.apply_listed(notes);
    }

    pub(crate) fn apply_note_loaded(&mut self, detail: NoteDetail) {
        self.notes.apply_loaded(detail);
    }

    pub(crate) fn apply_notes_search_hits(&mut self, hits: Vec<NoteSearchHit>) {
        self.notes.apply_search_hits(hits);
    }

    pub(crate) fn apply_notes_related(&mut self, hits: Vec<NoteRelatedHit>) {
        self.notes.apply_related(hits);
    }

    pub(crate) fn mark_note_saved(
        &mut self,
        path: String,
        slug: String,
        title: String,
        saved_label: &str,
    ) {
        self.notes.mark_saved(path, slug, title, saved_label);
    }

    pub(crate) fn mark_note_save_failed(
        &mut self,
        title: String,
        content: String,
        path: Option<String>,
    ) {
        self.notes.mark_save_failed(title, content, path);
    }

    pub(crate) fn set_library_docs(&mut self, docs: Vec<UserLibraryDoc>) {
        self.library.docs = docs;
    }

    pub(crate) fn apply_tasks_listed(&mut self, tasks: Vec<TaskItem>, count_tpl: &str) {
        self.tasks.apply_listed(tasks, count_tpl);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_notes_raw_detects_new_payload() {
        let mut state = WorkspaceUiState::default();
        assert!(!state.apply_notes_raw(String::new()));
        assert!(state.notes_out.is_empty());

        assert!(!state.apply_notes_raw("aucune note".into()));
        assert_eq!(state.notes_out, "aucune note");

        assert!(state.apply_notes_raw("note A créée".into()));
        assert_eq!(state.notes_out, "note A créée");

        assert!(!state.apply_notes_raw("note A créée".into()));
        assert!(state.apply_notes_raw("note B créée".into()));
        assert_eq!(state.notes_out, "note B créée");
    }

    #[test]
    fn set_library_docs_replaces_list() {
        let mut state = WorkspaceUiState::default();
        state.set_library_docs(vec![UserLibraryDoc {
            id: "1".into(),
            label: "a.pdf".into(),
            added_ms: 0,
            size_bytes: 0,
            added_date: String::new(),
        }]);
        assert_eq!(state.library.docs.len(), 1);
        state.set_library_docs(Vec::new());
        assert!(state.library.docs.is_empty());
    }

    #[test]
    fn apply_tasks_listed_updates_status() {
        let mut state = WorkspaceUiState::default();
        state.apply_tasks_listed(
            vec![TaskItem {
                id: "t1".into(),
                title: "do it".into(),
                notes: String::new(),
                done: false,
            }],
            "{n} task(s)",
        );
        assert_eq!(state.tasks.tasks.len(), 1);
        assert_eq!(state.tasks.status, "1 task(s)");
    }
}
