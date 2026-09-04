//! Mutable state owned by the chat composer.

use aos_proto::DocumentRef;

const MAX_PENDING_IMAGES: usize = 4;

#[derive(Debug, Default)]
pub(crate) struct ChatComposerState {
    pub(crate) input: String,
    /// PNG/JPEG paths queued for the next chat turn (vision).
    pub(crate) pending_images: Vec<String>,
    /// PDF/txt/md paths queued for the next chat turn (text extraction at send).
    pub(crate) pending_documents: Vec<DocumentRef>,
    /// Last Create / canvas export image path in this session.
    pub(crate) last_session_image: Option<String>,
    /// Re-focus chat TextEdit after send (Enter clears focus).
    pub(crate) refocus: bool,
    /// Session-header chip: spawn agents in Deep Thinking (hierarchical plan) mode.
    pub(crate) deep_thinking: bool,
}

impl ChatComposerState {
    pub(crate) fn queue_image(&mut self, path: String) -> bool {
        if path.is_empty() {
            return false;
        }
        if !self.pending_images.iter().any(|queued| queued == &path) {
            if self.pending_images.len() >= MAX_PENDING_IMAGES {
                self.pending_images.remove(0);
            }
            self.pending_images.push(path.clone());
        }
        self.last_session_image = Some(path);
        true
    }

    pub(crate) fn queue_document(&mut self, path: String) -> bool {
        if path.is_empty() || !aos_proto::chat_document::is_chat_document_path(&path) {
            return false;
        }
        let document = DocumentRef {
            label: aos_proto::chat_document::document_label_from_path(&path),
            path,
        };
        if !self
            .pending_documents
            .iter()
            .any(|queued| queued.path == document.path)
        {
            if self.pending_documents.len() >= aos_proto::chat_document::CHAT_MAX_PENDING_DOCUMENTS
            {
                self.pending_documents.remove(0);
            }
            self.pending_documents.push(document);
        }
        true
    }

    pub(crate) fn clear_attachments(&mut self) {
        self.pending_images.clear();
        self.pending_documents.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_are_deduplicated_and_bounded() {
        let mut state = ChatComposerState::default();
        for index in 0..=MAX_PENDING_IMAGES {
            assert!(state.queue_image(format!("/downloads/{index}.png")));
        }
        assert_eq!(state.pending_images.len(), MAX_PENDING_IMAGES);
        assert_eq!(state.pending_images[0], "/downloads/1.png");
        assert!(state.queue_image("/downloads/4.png".into()));
        assert_eq!(state.pending_images.len(), MAX_PENDING_IMAGES);
        assert_eq!(
            state.last_session_image.as_deref(),
            Some("/downloads/4.png")
        );
    }

    #[test]
    fn documents_reject_unsupported_paths_and_deduplicate() {
        let mut state = ChatComposerState::default();
        assert!(!state.queue_document("/downloads/archive.zip".into()));
        assert!(state.queue_document("/downloads/note.md".into()));
        assert!(state.queue_document("/downloads/note.md".into()));
        assert_eq!(state.pending_documents.len(), 1);
    }
}
