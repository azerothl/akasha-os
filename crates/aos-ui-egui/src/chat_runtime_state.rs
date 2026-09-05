//! Runtime state for the active chat or room turn.

use crate::cmd::ChatRetryTurn;

#[derive(Debug, Default)]
pub(crate) struct ChatRuntimeState {
    pub(crate) streaming: String,
    pub(crate) pending: bool,
    pub(crate) inference_id: Option<u64>,
    pub(crate) room_turn_text: Option<String>,
    /// Chat turn currently in flight (for load-fail Retry chrome).
    pub(crate) outgoing_turn: Option<ChatRetryTurn>,
    /// Last load-failed turn shown with Retry chrome.
    pub(crate) load_fail_retry: Option<ChatRetryTurn>,
    /// Unix ms when the current pending/streaming turn started.
    pub(crate) started_ms: u64,
}

impl ChatRuntimeState {
    pub(crate) fn begin_turn(&mut self, room_turn_text: Option<String>) {
        self.streaming.clear();
        self.pending = true;
        self.inference_id = None;
        self.room_turn_text = room_turn_text;
        self.started_ms = crate::now_ms();
    }

    pub(crate) fn finish_turn(&mut self) {
        self.streaming.clear();
        self.pending = false;
        self.inference_id = None;
        self.room_turn_text = None;
        self.outgoing_turn = None;
        self.started_ms = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_transitions_reset_transient_state() {
        let mut state = ChatRuntimeState {
            streaming: "partial".into(),
            pending: false,
            inference_id: Some(42),
            room_turn_text: None,
            outgoing_turn: None,
            load_fail_retry: None,
            started_ms: 0,
        };

        state.begin_turn(Some("question".into()));
        assert!(state.pending);
        assert!(state.streaming.is_empty());
        assert_eq!(state.inference_id, None);
        assert_eq!(state.room_turn_text.as_deref(), Some("question"));
        assert!(state.started_ms > 0);

        state.inference_id = Some(7);
        state.streaming = "answer".into();
        state.finish_turn();
        assert!(!state.pending);
        assert!(state.streaming.is_empty());
        assert_eq!(state.inference_id, None);
        assert_eq!(state.room_turn_text, None);
        assert_eq!(state.started_ms, 0);
    }
}
