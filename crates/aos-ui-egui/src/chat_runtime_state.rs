//! Runtime state for the active chat or room turn.

use crate::chat_error_recovery::ChatErrorRecovery;
use crate::chat_pending_status::ChatInferPhase;
use crate::cmd::ChatRetryTurn;
use aos_proto::AgentRoomConductProgress;

#[derive(Debug, Default)]
pub(crate) struct ChatRuntimeState {
    pub(crate) streaming: String,
    pub(crate) pending: bool,
    pub(crate) inference_id: Option<u64>,
    pub(crate) room_turn_text: Option<String>,
    /// Live salon speaker progress while a room turn is pending.
    pub(crate) room_progress: Option<AgentRoomConductProgress>,
    /// Chat turn currently in flight (for load-fail Retry chrome).
    pub(crate) outgoing_turn: Option<ChatRetryTurn>,
    /// Last load-failed turn shown with Retry chrome.
    pub(crate) load_fail_retry: Option<ChatRetryTurn>,
    /// Classified chat failure with Retry / Activity chrome.
    pub(crate) chat_error_recovery: Option<ChatErrorRecovery>,
    /// A stream that stopped after emitting content; keeps enough context for
    /// a one-click continuation instead of forcing the user to restate the ask.
    pub(crate) continue_retry: Option<ChatRetryTurn>,
    /// Unix ms when the current pending/streaming turn started.
    pub(crate) started_ms: u64,
    /// Live inference phase while pending (before first token).
    pub(crate) infer_phase: ChatInferPhase,
}

impl ChatRuntimeState {
    pub(crate) fn begin_turn(&mut self, room_turn_text: Option<String>) {
        self.streaming.clear();
        self.pending = true;
        self.inference_id = None;
        self.room_turn_text = room_turn_text;
        self.room_progress = None;
        self.started_ms = crate::now_ms();
        self.infer_phase = ChatInferPhase::Preparing;
    }

    pub(crate) fn finish_turn(&mut self) {
        self.streaming.clear();
        self.pending = false;
        self.inference_id = None;
        self.room_turn_text = None;
        self.room_progress = None;
        self.outgoing_turn = None;
        self.started_ms = 0;
        self.infer_phase = ChatInferPhase::Preparing;
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
            room_progress: None,
            outgoing_turn: None,
            load_fail_retry: None,
            chat_error_recovery: None,
            continue_retry: None,
            started_ms: 0,
            infer_phase: ChatInferPhase::WaitingFirstToken,
        };

        state.begin_turn(Some("question".into()));
        assert!(state.pending);
        assert!(state.streaming.is_empty());
        assert_eq!(state.inference_id, None);
        assert_eq!(state.room_turn_text.as_deref(), Some("question"));
        assert!(state.started_ms > 0);
        assert_eq!(state.infer_phase, ChatInferPhase::Preparing);

        state.inference_id = Some(7);
        state.streaming = "answer".into();
        state.infer_phase = ChatInferPhase::Queued { position: 2 };
        state.finish_turn();
        assert!(!state.pending);
        assert!(state.streaming.is_empty());
        assert_eq!(state.inference_id, None);
        assert_eq!(state.room_turn_text, None);
        assert_eq!(state.started_ms, 0);
        assert_eq!(state.infer_phase, ChatInferPhase::Preparing);
    }
}
