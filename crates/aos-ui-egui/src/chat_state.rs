//! Aggregate state owned by the conversation workspace.

use crate::chat_composer_state::ChatComposerState;
use crate::chat_runtime_state::ChatRuntimeState;
use crate::chat_sidebar_state::ChatSidebarState;
use crate::chat_view_state::ChatViewState;
use crate::session_chat::SessionChatState;
use aos_proto::ChatSessionMeta;

#[derive(Debug, Default)]
pub(crate) struct ChatState {
    pub(crate) sessions: Vec<ChatSessionMeta>,
    pub(crate) active_session: Option<String>,
    pub(crate) session_chat: SessionChatState,
    pub(crate) composer: ChatComposerState,
    pub(crate) runtime: ChatRuntimeState,
    pub(crate) sidebar: ChatSidebarState,
    pub(crate) view: ChatViewState,
}
