//! Visual state shared by the conversation, room, and canvas panels.

use crate::chat_canvas::CanvasPanelState;
use std::collections::HashSet;

#[derive(Debug, Default)]
pub(crate) struct ChatViewState {
    pub(crate) canvas: CanvasPanelState,
    pub(crate) room_thinking_open: HashSet<usize>,
    pub(crate) room_members_open: bool,
}
