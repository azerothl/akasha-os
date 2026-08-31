//! Guard session view changes so background reloads do not hijack the active chat.

/// User-initiated session navigation intent.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum PendingSessionNav {
    #[default]
    None,
    /// User selected or opened a concrete session.
    Explicit(String),
    /// `+ Nouvelle` clicked; waiting for runtime to name the new session.
    AwaitingCreate,
    /// Active session deleted; waiting for runtime fallback id (`list[0]`).
    AwaitingDelete,
}

impl PendingSessionNav {
    pub fn explicit_id(&self) -> Option<&str> {
        match self {
            PendingSessionNav::Explicit(id) => Some(id.as_str()),
            _ => None,
        }
    }
}

/// Whether an incoming `SessionLoaded` may switch the visible chat to `loaded_id`.
pub fn should_switch_session_view(
    active_session: Option<&str>,
    pending: &PendingSessionNav,
    loaded_id: &str,
) -> bool {
    if active_session == Some(loaded_id) {
        return true;
    }
    if active_session.is_none() {
        return true;
    }
    pending.explicit_id() == Some(loaded_id)
}

/// Apply a runtime load intent only when it matches an in-flight create/delete.
pub fn apply_session_load_intent(pending: &mut PendingSessionNav, loaded_id: &str) {
    match pending {
        PendingSessionNav::AwaitingCreate | PendingSessionNav::AwaitingDelete => {
            *pending = PendingSessionNav::Explicit(loaded_id.to_string());
        }
        PendingSessionNav::Explicit(_) | PendingSessionNav::None => {}
    }
}

/// Whether a same-session reload may replace the in-memory transcript.
pub fn should_replace_chat_on_same_session_reload(schedule_transcript_dirty: bool) -> bool {
    !schedule_transcript_dirty
}

/// Simulate handling a cross-session `SessionLoaded` (e.g. stale create after Pause).
#[cfg(test)]
pub fn apply_session_loaded_id(
    active_session: &mut Option<String>,
    pending: &mut PendingSessionNav,
    loaded_id: &str,
) -> bool {
    let session_changed = active_session.as_deref() != Some(loaded_id);
    if session_changed && !should_switch_session_view(active_session.as_deref(), pending, loaded_id)
    {
        return false;
    }
    *active_session = Some(loaded_id.to_string());
    *pending = PendingSessionNav::None;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_unsolicited_switch_to_newest_session() {
        assert!(!should_switch_session_view(
            Some("session-6"),
            &PendingSessionNav::None,
            "session-8",
        ));
    }

    #[test]
    fn blocks_stale_create_load_while_on_session_12() {
        assert!(!should_switch_session_view(
            Some("session-12"),
            &PendingSessionNav::None,
            "session-14",
        ));
    }

    #[test]
    fn allows_explicit_session_select() {
        assert!(should_switch_session_view(
            Some("session-6"),
            &PendingSessionNav::Explicit("session-8".into()),
            "session-8",
        ));
    }

    #[test]
    fn allows_bootstrap_when_no_active_session() {
        assert!(should_switch_session_view(
            None,
            &PendingSessionNav::None,
            "session-1",
        ));
    }

    #[test]
    fn stale_create_intent_ignored_after_user_selected_other_session() {
        let mut pending = PendingSessionNav::Explicit("session-12".into());
        apply_session_load_intent(&mut pending, "session-14");
        assert_eq!(pending, PendingSessionNav::Explicit("session-12".into()));
        assert!(!should_switch_session_view(
            Some("session-12"),
            &pending,
            "session-14",
        ));
    }

    #[test]
    fn create_intent_binds_concrete_id_before_load() {
        let mut pending = PendingSessionNav::AwaitingCreate;
        apply_session_load_intent(&mut pending, "session-14");
        assert_eq!(pending, PendingSessionNav::Explicit("session-14".into()));
        assert!(should_switch_session_view(
            Some("session-12"),
            &pending,
            "session-14",
        ));
    }

    #[test]
    fn same_session_reload_skipped_when_schedule_dirty() {
        assert!(!should_replace_chat_on_same_session_reload(true));
        assert!(should_replace_chat_on_same_session_reload(false));
    }

    #[test]
    fn pause_stale_load_does_not_change_active_session() {
        let mut active = Some("session-12".to_string());
        let mut pending = PendingSessionNav::None;
        assert!(!apply_session_loaded_id(
            &mut active,
            &mut pending,
            "session-14",
        ));
        assert_eq!(active.as_deref(), Some("session-12"));
        assert_eq!(pending, PendingSessionNav::None);
    }
}
