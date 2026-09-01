//! Mutable state owned by the Memory panel.

use aos_proto::MemHit;

#[derive(Debug)]
pub(crate) struct MemoryUiState {
    pub(crate) query: String,
    pub(crate) note: String,
    pub(crate) hits: Vec<MemHit>,
    pub(crate) show_superseded: bool,
    pub(crate) sweep_last_pass_ms: u64,
    pub(crate) sweep_last_pass_label: String,
    pub(crate) edit_id: Option<u64>,
    pub(crate) edit_text: String,
}

impl Default for MemoryUiState {
    fn default() -> Self {
        Self {
            query: String::new(),
            note: String::new(),
            hits: Vec::new(),
            show_superseded: true,
            sweep_last_pass_ms: 0,
            sweep_last_pass_label: String::new(),
            edit_id: None,
            edit_text: String::new(),
        }
    }
}

impl MemoryUiState {
    pub(crate) fn set_hits(&mut self, hits: Vec<MemHit>) {
        self.hits = hits;
    }

    pub(crate) fn apply_sweep_status(&mut self, last_pass_ms: u64, last_pass_label: String) {
        self.sweep_last_pass_ms = last_pass_ms;
        self.sweep_last_pass_label = last_pass_label;
    }

    pub(crate) fn begin_edit(&mut self, id: u64, text: String) {
        self.edit_id = Some(id);
        self.edit_text = text;
    }

    pub(crate) fn clear_edit(&mut self) {
        self.edit_id = None;
        self.edit_text.clear();
    }

    /// Take a non-empty remember note, clearing the field.
    pub(crate) fn take_remember_note(&mut self) -> Option<String> {
        let text = self.note.trim().to_string();
        if text.is_empty() {
            return None;
        }
        self.note.clear();
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_hits_replaces_list() {
        let mut state = MemoryUiState::default();
        state.set_hits(vec![MemHit {
            id: 1,
            namespace: "user".into(),
            text: "fact".into(),
            score: 1.0,
            metadata: serde_json::Value::Null,
            pinned: false,
            kind: None,
            relations: Vec::new(),
            superseded: false,
        }]);
        assert_eq!(state.hits.len(), 1);
        state.set_hits(Vec::new());
        assert!(state.hits.is_empty());
    }

    #[test]
    fn apply_sweep_status_stores_label() {
        let mut state = MemoryUiState::default();
        state.apply_sweep_status(42, "just now".into());
        assert_eq!(state.sweep_last_pass_ms, 42);
        assert_eq!(state.sweep_last_pass_label, "just now");
    }

    #[test]
    fn begin_and_clear_edit() {
        let mut state = MemoryUiState::default();
        state.begin_edit(7, "old text".into());
        assert_eq!(state.edit_id, Some(7));
        assert_eq!(state.edit_text, "old text");
        state.clear_edit();
        assert!(state.edit_id.is_none());
        assert!(state.edit_text.is_empty());
    }

    #[test]
    fn take_remember_note_requires_text_and_clears() {
        let mut state = MemoryUiState::default();
        assert!(state.take_remember_note().is_none());

        state.note = "  ".into();
        assert!(state.take_remember_note().is_none());

        state.note = " remember this ".into();
        assert_eq!(state.take_remember_note().as_deref(), Some("remember this"));
        assert!(state.note.is_empty());
    }
}
