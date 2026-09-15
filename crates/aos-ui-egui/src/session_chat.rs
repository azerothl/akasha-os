//! Per-session chat inflight state and unread marks (#82).
//!
//! Assistant tokens and completion events bind to the originating session id,
//! not whichever session is currently painted in the Chat view.

use std::collections::{HashMap, HashSet};

use aos_proto::ChatAttachment;

use crate::agent_panel;
use crate::chat_pending_status::ChatInferPhase;
use crate::cmd::ChatLine;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SessionInflight {
    pub pending: bool,
    pub streaming: String,
    pub inference_id: Option<u64>,
    pub started_ms: u64,
    pub infer_phase: ChatInferPhase,
}

#[derive(Debug, Default)]
pub(crate) struct SessionChatState {
    inflight: HashMap<String, SessionInflight>,
    unread: HashSet<String>,
}

impl SessionChatState {
    pub fn begin_turn(&mut self, session_id: &str) {
        let inf = self.inflight_mut(session_id);
        inf.pending = true;
        inf.streaming.clear();
        inf.inference_id = None;
        inf.started_ms = crate::now_ms();
        inf.infer_phase = ChatInferPhase::Preparing;
    }

    pub fn push_delta(&mut self, session_id: &str, text: &str) {
        self.inflight_mut(session_id).streaming.push_str(text);
    }

    pub fn set_inference_id(&mut self, session_id: &str, inference_id: u64) {
        self.inflight_mut(session_id).inference_id = Some(inference_id);
    }

    pub fn set_infer_phase(&mut self, session_id: &str, phase: ChatInferPhase) {
        self.inflight_mut(session_id).infer_phase = phase;
    }

    pub fn finish_turn(&mut self, session_id: &str) {
        if let Some(inf) = self.inflight.get_mut(session_id) {
            inf.pending = false;
            inf.streaming.clear();
            inf.inference_id = None;
            inf.infer_phase = ChatInferPhase::Preparing;
        }
    }

    pub fn inflight(&self, session_id: &str) -> Option<&SessionInflight> {
        self.inflight.get(session_id)
    }

    pub fn is_pending(&self, session_id: &str) -> bool {
        self.inflight
            .get(session_id)
            .map(|inf| inf.pending)
            .unwrap_or(false)
    }

    pub fn mark_unread(&mut self, session_id: &str) {
        self.unread.insert(session_id.to_string());
    }

    pub fn clear_unread(&mut self, session_id: &str) {
        self.unread.remove(session_id);
    }

    pub fn is_unread(&self, session_id: &str) -> bool {
        self.unread.contains(session_id)
    }

    pub fn sync_active_view(
        &self,
        active_session: Option<&str>,
        streaming: &mut String,
        chat_pending: &mut bool,
        chat_inference_id: &mut Option<u64>,
        chat_infer_phase: &mut ChatInferPhase,
    ) {
        if let Some(sid) = active_session {
            if let Some(inf) = self.inflight.get(sid) {
                *streaming = inf.streaming.clone();
                *chat_pending = inf.pending;
                *chat_inference_id = inf.inference_id;
                *chat_infer_phase = inf.infer_phase;
                return;
            }
        }
        streaming.clear();
        *chat_pending = false;
        *chat_inference_id = None;
        *chat_infer_phase = ChatInferPhase::Preparing;
    }

    fn inflight_mut(&mut self, session_id: &str) -> &mut SessionInflight {
        self.inflight.entry(session_id.to_string()).or_default()
    }
}

/// Route a streaming token to the originating session; paint only when active.
pub(crate) fn on_delta(
    state: &mut SessionChatState,
    active_session: Option<&str>,
    session_id: &str,
    text: &str,
    streaming: &mut String,
) {
    state.push_delta(session_id, text);
    if active_session == Some(session_id) {
        streaming.push_str(text);
    }
}

/// Complete an assistant turn for `session_id`; paint or mark unread.
#[allow(clippy::too_many_arguments)] // Session completion updates several coordinated view fields.
pub(crate) fn on_done(
    state: &mut SessionChatState,
    active_session: Option<&str>,
    session_id: &str,
    text: &str,
    attachments: Vec<ChatAttachment>,
    chat: &mut Vec<ChatLine>,
    streaming: &mut String,
    chat_pending: &mut bool,
    chat_inference_id: &mut Option<u64>,
    model_id: Option<String>,
) {
    let started_ms = state
        .inflight(session_id)
        .map(|inf| inf.started_ms)
        .unwrap_or(0);
    state.finish_turn(session_id);
    if active_session == Some(session_id) {
        if !text.is_empty() {
            let ts_ms = crate::now_ms();
            let duration_ms = if started_ms > 0 && ts_ms >= started_ms {
                ts_ms - started_ms
            } else {
                0
            };
            chat.push(ChatLine {
                role: "assistant".into(),
                text: text.to_string(),
                attachments,
                speaker_id: None,
                speaker_name: None,
                thinking: None,
                ts_ms,
                duration_ms,
                model_id,
            });
        }
        streaming.clear();
        *chat_pending = false;
        *chat_inference_id = None;
    } else if !text.is_empty() {
        state.mark_unread(session_id);
    }
}

pub(crate) fn on_infer_started(
    state: &mut SessionChatState,
    active_session: Option<&str>,
    session_id: &str,
    inference_id: u64,
    chat_inference_id: &mut Option<u64>,
    chat_infer_phase: &mut ChatInferPhase,
) {
    state.set_inference_id(session_id, inference_id);
    state.set_infer_phase(session_id, ChatInferPhase::WaitingFirstToken);
    if active_session == Some(session_id) {
        *chat_inference_id = Some(inference_id);
        *chat_infer_phase = ChatInferPhase::WaitingFirstToken;
    }
}

pub(crate) fn on_chat_progress(
    state: &mut SessionChatState,
    active_session: Option<&str>,
    session_id: &str,
    phase: ChatInferPhase,
    chat_infer_phase: &mut ChatInferPhase,
) {
    state.set_infer_phase(session_id, phase);
    if active_session == Some(session_id) {
        *chat_infer_phase = phase;
    }
}

pub(crate) fn on_chat_cancelled(
    state: &mut SessionChatState,
    active_session: Option<&str>,
    session_id: &str,
    streaming: &mut String,
    chat_pending: &mut bool,
    chat_inference_id: &mut Option<u64>,
    chat: &mut Vec<ChatLine>,
) -> bool {
    let partial = state
        .inflight(session_id)
        .map(|inf| inf.streaming.clone())
        .unwrap_or_default();
    state.finish_turn(session_id);
    let on_active = active_session == Some(session_id);
    if on_active {
        *chat_pending = false;
        *chat_inference_id = None;
        if !partial.is_empty() {
            let t = crate::i18n::strings(&crate::prefs::load_preferences().language);
            let display = agent_panel::format_chat_assistant_display(&partial, &t);
            if !display.is_empty() {
                chat.push(ChatLine::plain("assistant", display));
            }
        }
        streaming.clear();
    }
    on_active
}

fn is_ephemeral_ui_line(line: &ChatLine) -> bool {
    // Persisted session chrome uses role `system` on disk; UI-only notices use `système`.
    line.role == "système"
}

fn content_message_count(lines: &[ChatLine]) -> usize {
    lines.iter().filter(|line| !is_ephemeral_ui_line(line)).count()
}

/// How many persisted transcript messages to keep when branching from UI index
/// `ui_index` (inclusive). Skips ephemeral `système` chrome that exists only in
/// the painted transcript (e.g. "Historique rechargé."); persisted `system`
/// lines from disk are counted.
pub(crate) fn persisted_prefix_len(chat: &[ChatLine], ui_index: usize) -> usize {
    if chat.is_empty() {
        return 0;
    }
    let end = ui_index.min(chat.len().saturating_sub(1));
    chat[..=end]
        .iter()
        .filter(|line| !is_ephemeral_ui_line(line))
        .count()
}

/// A mid-turn salon snapshot may replace the painted transcript once disk has
/// caught up with the optimistic user line (and any later speaker bubbles).
pub(crate) fn should_apply_room_snapshot(local: &[ChatLine], loaded: &[ChatLine]) -> bool {
    content_message_count(loaded) >= content_message_count(local)
}

/// Fill assistant `duration_ms` from the previous timestamped message when missing.
pub(crate) fn infer_reply_durations(chat: &mut [ChatLine]) {
    let mut prev_ts = 0u64;
    for line in chat.iter_mut() {
        if line.duration_ms == 0 && line.ts_ms > prev_ts && prev_ts > 0 && line.role == "assistant"
        {
            line.duration_ms = line.ts_ms - prev_ts;
        }
        if line.ts_ms > 0 {
            prev_ts = line.ts_ms;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_a() -> &'static str {
        "session-a"
    }

    fn session_b() -> &'static str {
        "session-b"
    }

    #[test]
    fn delta_for_background_session_does_not_paint_active_streaming() {
        let mut state = SessionChatState::default();
        let mut streaming = String::new();
        state.begin_turn(session_a());

        on_delta(
            &mut state,
            Some(session_b()),
            session_a(),
            "hello",
            &mut streaming,
        );

        assert_eq!(state.inflight(session_a()).unwrap().streaming, "hello");
        assert!(streaming.is_empty());
    }

    #[test]
    fn delta_for_active_session_updates_streaming() {
        let mut state = SessionChatState::default();
        let mut streaming = String::new();
        state.begin_turn(session_a());

        on_delta(
            &mut state,
            Some(session_a()),
            session_a(),
            "token",
            &mut streaming,
        );

        assert_eq!(streaming, "token");
    }

    #[test]
    fn done_in_background_session_marks_unread_not_active_chat() {
        let mut state = SessionChatState::default();
        let mut chat = vec![ChatLine::plain("user", "question in A")];
        let mut streaming = String::new();
        let mut pending = false;
        let mut inference_id = None;

        state.begin_turn(session_a());
        on_delta(
            &mut state,
            Some(session_b()),
            session_a(),
            "partial",
            &mut streaming,
        );

        on_done(
            &mut state,
            Some(session_b()),
            session_a(),
            "assistant reply for A",
            vec![],
            &mut chat,
            &mut streaming,
            &mut pending,
            &mut inference_id,
            None,
        );

        assert!(
            chat.iter()
                .all(|l| l.role != "assistant" || l.text != "assistant reply for A"),
            "B's transcript must not receive A's assistant turn"
        );
        assert!(state.is_unread(session_a()));
        assert!(!pending);
        assert!(inference_id.is_none());
        assert!(streaming.is_empty());
    }

    #[test]
    fn done_in_active_session_paints_chat_and_clears_pending() {
        let mut state = SessionChatState::default();
        let mut chat = vec![ChatLine::plain("user", "hi")];
        let mut streaming = String::new();
        let mut pending = true;
        let mut inference_id = None;

        state.begin_turn(session_a());
        on_done(
            &mut state,
            Some(session_a()),
            session_a(),
            "reply",
            vec![],
            &mut chat,
            &mut streaming,
            &mut pending,
            &mut inference_id,
            Some("local:test-model".into()),
        );

        assert_eq!(chat.len(), 2);
        assert_eq!(chat[1].role, "assistant");
        assert_eq!(chat[1].text, "reply");
        assert!(chat[1].ts_ms > 0);
        assert_eq!(chat[1].model_id.as_deref(), Some("local:test-model"));
        assert!(!state.is_unread(session_a()));
        assert!(!pending);
    }

    #[test]
    fn selecting_session_clears_unread_and_syncs_view() {
        let mut state = SessionChatState::default();
        state.mark_unread(session_a());

        let mut streaming = String::new();
        let mut pending = false;
        let mut inference_id = None;
        let mut infer_phase = ChatInferPhase::WaitingFirstToken;

        state.clear_unread(session_a());
        state.sync_active_view(
            Some(session_a()),
            &mut streaming,
            &mut pending,
            &mut inference_id,
            &mut infer_phase,
        );

        assert!(!state.is_unread(session_a()));
        assert!(streaming.is_empty());
        assert!(!pending);
        assert_eq!(infer_phase, ChatInferPhase::Preparing);
    }

    #[test]
    fn repro_issue_82_send_in_a_switch_to_b_before_completion() {
        let mut state = SessionChatState::default();
        let mut chat_b = vec![ChatLine::plain("user", "message in B")];
        let mut streaming = String::new();
        let mut pending = false;
        let mut inference_id = None;
        let mut infer_phase = ChatInferPhase::Preparing;

        // User sends in session A.
        state.begin_turn(session_a());
        state.sync_active_view(
            Some(session_a()),
            &mut streaming,
            &mut pending,
            &mut inference_id,
            &mut infer_phase,
        );
        assert!(pending);

        // User opens session B before the assistant responds.
        state.sync_active_view(
            Some(session_b()),
            &mut streaming,
            &mut pending,
            &mut inference_id,
            &mut infer_phase,
        );
        assert!(!pending);
        assert!(streaming.is_empty());

        // Tokens still belong to A.
        on_delta(
            &mut state,
            Some(session_b()),
            session_a(),
            "reply ",
            &mut streaming,
        );
        on_delta(
            &mut state,
            Some(session_b()),
            session_a(),
            "text",
            &mut streaming,
        );
        assert!(streaming.is_empty());

        on_done(
            &mut state,
            Some(session_b()),
            session_a(),
            "reply text",
            vec![],
            &mut chat_b,
            &mut streaming,
            &mut pending,
            &mut inference_id,
            None,
        );

        assert!(
            chat_b.iter().all(|l| l.role != "assistant"),
            "session B transcript must not show A's assistant reply"
        );
        assert!(state.is_unread(session_a()));
        assert!(!state.inflight(session_a()).unwrap().pending);

        // Returning to A clears unread and restores inflight view (completed).
        state.clear_unread(session_a());
        let chat_a = [
            ChatLine::plain("user", "question in A"),
            ChatLine::plain("assistant", "reply text"),
        ];
        state.sync_active_view(
            Some(session_a()),
            &mut streaming,
            &mut pending,
            &mut inference_id,
            &mut infer_phase,
        );
        assert!(!state.is_unread(session_a()));
        assert!(!pending);
        assert_eq!(chat_a[1].text, "reply text");
    }

    #[test]
    fn switch_active_view_while_background_session_still_pending() {
        let mut state = SessionChatState::default();
        state.begin_turn(session_a());

        let mut streaming = String::new();
        let mut pending = false;
        let mut inference_id = None;
        let mut infer_phase = ChatInferPhase::Preparing;

        state.sync_active_view(
            Some(session_b()),
            &mut streaming,
            &mut pending,
            &mut inference_id,
            &mut infer_phase,
        );
        assert!(!pending);
        assert!(streaming.is_empty());

        on_delta(
            &mut state,
            Some(session_b()),
            session_a(),
            "hidden",
            &mut streaming,
        );
        assert!(streaming.is_empty());

        state.sync_active_view(
            Some(session_a()),
            &mut streaming,
            &mut pending,
            &mut inference_id,
            &mut infer_phase,
        );
        assert!(pending);
        assert_eq!(streaming, "hidden");
    }

    #[test]
    fn chat_progress_survives_session_switch() {
        let mut state = SessionChatState::default();
        let mut infer_phase = ChatInferPhase::Preparing;
        state.begin_turn(session_a());
        on_chat_progress(
            &mut state,
            Some(session_a()),
            session_a(),
            ChatInferPhase::Queued { position: 4 },
            &mut infer_phase,
        );
        assert_eq!(infer_phase, ChatInferPhase::Queued { position: 4 });

        let mut streaming = String::new();
        let mut pending = false;
        let mut inference_id = None;
        state.sync_active_view(
            Some(session_b()),
            &mut streaming,
            &mut pending,
            &mut inference_id,
            &mut infer_phase,
        );
        assert_eq!(infer_phase, ChatInferPhase::Preparing);

        state.sync_active_view(
            Some(session_a()),
            &mut streaming,
            &mut pending,
            &mut inference_id,
            &mut infer_phase,
        );
        assert!(pending);
        assert_eq!(infer_phase, ChatInferPhase::Queued { position: 4 });
    }

    #[test]
    fn infer_reply_durations_from_previous_stamp() {
        let mut chat = vec![
            ChatLine {
                role: "user".into(),
                text: "q".into(),
                ts_ms: 1_000,
                ..Default::default()
            },
            ChatLine {
                role: "assistant".into(),
                text: "a".into(),
                ts_ms: 4_200,
                ..Default::default()
            },
        ];
        infer_reply_durations(&mut chat);
        assert_eq!(chat[1].duration_ms, 3_200);
        assert_eq!(chat[0].duration_ms, 0);
    }

    #[test]
    fn persisted_prefix_skips_history_banner() {
        let chat = vec![
            ChatLine::plain("système", "Historique rechargé."),
            ChatLine::plain("user", "first"),
            ChatLine::plain("assistant", "answer"),
            ChatLine::plain("user", "relance"),
        ];
        // Fork on the assistant bubble (UI index 2) must keep 2 disk messages,
        // not 3 (which would wrongly include the following user relaunch).
        assert_eq!(persisted_prefix_len(&chat, 2), 2);
        assert_eq!(persisted_prefix_len(&chat, 3), 3);
        assert_eq!(persisted_prefix_len(&chat, 0), 0);
    }

    #[test]
    fn room_snapshot_waits_for_optimistic_user_then_accepts_new_speakers() {
        let local = vec![
            ChatLine::plain("système", "Historique rechargé."),
            ChatLine::plain("user", "hello everyone"),
        ];
        let disk_user_only = vec![ChatLine::plain("user", "hello everyone")];
        assert!(should_apply_room_snapshot(&local, &disk_user_only));

        let mut first_agent = disk_user_only.clone();
        first_agent.push(ChatLine {
            role: "assistant".into(),
            text: "first".into(),
            speaker_id: Some("persona-researcher".into()),
            speaker_name: Some("Researcher".into()),
            ..Default::default()
        });
        assert!(should_apply_room_snapshot(&local, &first_agent));

        let stale = vec![ChatLine::plain("user", "older")];
        let optimistic = vec![
            ChatLine::plain("user", "older"),
            ChatLine::plain("user", "just typed"),
        ];
        assert!(!should_apply_room_snapshot(&optimistic, &stale));
    }
}
