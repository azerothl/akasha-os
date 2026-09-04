//! Mutable state owned by research-document flows (choice pending, overlay, list).

use crate::research_document::{DocumentOverlayState, DocumentsListState};
use aos_agent::document_index::ResearchDocumentEntry;
use aos_proto::DocumentRef;

#[derive(Clone, Debug)]
pub(crate) struct ResearchPendingChat {
    pub(crate) session_id: String,
    pub(crate) history: Vec<(String, String)>,
    pub(crate) user_text: String,
    pub(crate) model_id: Option<String>,
    pub(crate) images: Vec<String>,
    pub(crate) documents: Vec<DocumentRef>,
    pub(crate) auto_remember: bool,
    pub(crate) max_steps: u32,
    pub(crate) routing: String,
    pub(crate) language: String,
    pub(crate) canvas_open: bool,
    pub(crate) canvas_aspect: aos_proto::CanvasAspect,
    pub(crate) deep_thinking: bool,
    pub(crate) choice_id: String,
}

#[derive(Debug)]
pub(crate) struct ResearchUiState {
    pub(crate) pending_chat: Option<ResearchPendingChat>,
    pub(crate) documents: Vec<ResearchDocumentEntry>,
    pub(crate) overlay: DocumentOverlayState,
    pub(crate) documents_list: DocumentsListState,
}

impl Default for ResearchUiState {
    fn default() -> Self {
        Self {
            pending_chat: None,
            documents: crate::research_document::load_index_entries(),
            overlay: DocumentOverlayState::default(),
            documents_list: DocumentsListState::default(),
        }
    }
}

impl ResearchUiState {
    pub(crate) fn set_pending(&mut self, pending: ResearchPendingChat) {
        self.pending_chat = Some(pending);
    }

    /// Take pending chat only when `choice_id` matches; otherwise restore it.
    pub(crate) fn take_pending_for_choice(
        &mut self,
        choice_id: &str,
    ) -> Option<ResearchPendingChat> {
        match self.pending_chat.take() {
            Some(p) if p.choice_id == choice_id => Some(p),
            other => {
                self.pending_chat = other;
                None
            }
        }
    }

    pub(crate) fn reload_documents(&mut self) {
        self.documents = crate::research_document::load_index_entries();
    }

    pub(crate) fn open_documents_list(&mut self) {
        self.documents_list.open = true;
        self.reload_documents();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pending(choice_id: &str) -> ResearchPendingChat {
        ResearchPendingChat {
            session_id: "s1".into(),
            history: Vec::new(),
            user_text: "q".into(),
            model_id: None,
            images: Vec::new(),
            documents: Vec::new(),
            auto_remember: false,
            max_steps: 8,
            routing: "local".into(),
            language: "en".into(),
            canvas_open: false,
            canvas_aspect: aos_proto::CanvasAspect::Square,
            deep_thinking: false,
            choice_id: choice_id.into(),
        }
    }

    #[test]
    fn take_pending_for_choice_matches_and_restores() {
        let mut state = ResearchUiState {
            pending_chat: None,
            documents: Vec::new(),
            overlay: DocumentOverlayState::default(),
            documents_list: DocumentsListState::default(),
        };
        assert!(state.take_pending_for_choice("x").is_none());

        state.set_pending(sample_pending("c1"));
        assert!(state.take_pending_for_choice("other").is_none());
        assert_eq!(
            state.pending_chat.as_ref().map(|p| p.choice_id.as_str()),
            Some("c1")
        );

        let taken = state.take_pending_for_choice("c1").expect("match");
        assert_eq!(taken.choice_id, "c1");
        assert!(state.pending_chat.is_none());
    }

    #[test]
    fn open_documents_list_sets_flag() {
        let mut state = ResearchUiState {
            pending_chat: None,
            documents: Vec::new(),
            overlay: DocumentOverlayState::default(),
            documents_list: DocumentsListState::default(),
        };
        state.open_documents_list();
        assert!(state.documents_list.open);
    }

    #[test]
    fn set_documents_replaces_list() {
        let mut state = ResearchUiState {
            pending_chat: None,
            documents: Vec::new(),
            overlay: DocumentOverlayState::default(),
            documents_list: DocumentsListState::default(),
        };
        state.documents = vec![ResearchDocumentEntry {
            question: "q".into(),
            path: "p".into(),
            label: "l".into(),
            created_ms: 1,
        }];
        assert_eq!(state.documents.len(), 1);
        state.documents.clear();
        assert!(state.documents.is_empty());
    }
}
