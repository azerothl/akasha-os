//! Guard session view changes so background reloads do not hijack the active chat.

/// Whether an incoming `SessionLoaded` may switch the visible chat to `loaded_id`.
pub fn should_switch_session_view(
    active_session: Option<&str>,
    pending_session_load: Option<&str>,
    allow_session_load: bool,
    loaded_id: &str,
) -> bool {
    if active_session == Some(loaded_id) {
        return true;
    }
    if active_session.is_none() {
        return true;
    }
    if allow_session_load {
        return true;
    }
    pending_session_load == Some(loaded_id)
}

/// Whether a same-session reload may replace the in-memory transcript.
pub fn should_replace_chat_on_same_session_reload(schedule_transcript_dirty: bool) -> bool {
    !schedule_transcript_dirty
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_unsolicited_switch_to_newest_session() {
        assert!(!should_switch_session_view(
            Some("session-6"),
            None,
            false,
            "session-8",
        ));
    }

    #[test]
    fn allows_explicit_session_select() {
        assert!(should_switch_session_view(
            Some("session-6"),
            Some("session-8"),
            false,
            "session-8",
        ));
    }

    #[test]
    fn allows_bootstrap_when_no_active_session() {
        assert!(should_switch_session_view(None, None, false, "session-1"));
    }

    #[test]
    fn allows_create_or_delete_fallback_load() {
        assert!(should_switch_session_view(
            Some("session-6"),
            None,
            true,
            "session-9",
        ));
    }

    #[test]
    fn same_session_reload_skipped_when_schedule_dirty() {
        assert!(!should_replace_chat_on_same_session_reload(true));
        assert!(should_replace_chat_on_same_session_reload(false));
    }
}
