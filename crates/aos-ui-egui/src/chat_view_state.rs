//! Visual state shared by the conversation, room, and canvas panels.

use crate::chat_canvas::CanvasPanelState;
use std::collections::HashSet;

#[derive(Debug)]
pub(crate) struct ChatViewState {
    pub(crate) canvas: CanvasPanelState,
    pub(crate) room_thinking_open: HashSet<usize>,
    pub(crate) deep_plan_open: HashSet<usize>,
    pub(crate) room_members_open: bool,
    /// When true, the transcript ScrollArea sticks to the latest messages.
    pub(crate) follow_bottom: bool,
    /// Last rendered row count; used to detect new messages.
    pub(crate) transcript_row_count: usize,
    /// Last rendered streaming buffer length.
    pub(crate) transcript_streaming_len: usize,
}

impl Default for ChatViewState {
    fn default() -> Self {
        Self {
            canvas: CanvasPanelState::default(),
            room_thinking_open: HashSet::new(),
            deep_plan_open: HashSet::new(),
            room_members_open: false,
            follow_bottom: true,
            transcript_row_count: 0,
            transcript_streaming_len: 0,
        }
    }
}
