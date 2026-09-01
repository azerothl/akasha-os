//! Aggregate state owned by the conversation workspace.

use crate::chat_composer_state::ChatComposerState;
use crate::chat_runtime_state::ChatRuntimeState;
use crate::chat_sidebar_state::ChatSidebarState;
use crate::chat_view_state::ChatViewState;

#[derive(Debug, Default)]
pub(crate) struct ChatState {
    pub(crate) composer: ChatComposerState,
    pub(crate) runtime: ChatRuntimeState,
    pub(crate) sidebar: ChatSidebarState,
    pub(crate) view: ChatViewState,
}
