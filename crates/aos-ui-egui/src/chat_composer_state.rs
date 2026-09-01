//! Mutable state owned by the chat composer.

use aos_proto::DocumentRef;

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
}
