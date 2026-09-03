//! Guard session view changes so background reloads do not hijack the active chat.

use aos_proto::ChatSessionMeta;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionGroup {
    Today,
    Yesterday,
    LastSevenDays,
    Older,
}

/// Local, deterministic grouping used by the sidebar. Timestamps are epoch ms
/// and are compared against the current local day boundaries supplied by the UI.
pub fn group_for(updated_ms: u64, now_ms: u64) -> SessionGroup {
    const DAY: u64 = 86_400_000;
    let age = now_ms.saturating_sub(updated_ms);
    if age < DAY {
        SessionGroup::Today
    } else if age < DAY * 2 {
        SessionGroup::Yesterday
    } else if age < DAY * 7 {
        SessionGroup::LastSevenDays
    } else {
        SessionGroup::Older
    }
}

pub fn filter_and_sort<'a>(
    sessions: &'a [ChatSessionMeta],
    query: &str,
) -> Vec<&'a ChatSessionMeta> {
    let needle = query.trim().to_lowercase();
    let mut out: Vec<_> = sessions
        .iter()
        .filter(|s| {
            needle.is_empty()
                || s.title.to_lowercase().contains(&needle)
                || s.id.to_lowercase().contains(&needle)
        })
        .collect();
    out.sort_by_key(|s| (!s.pinned, std::cmp::Reverse(s.updated_ms)));
    out
}

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

    #[test]
    fn session_search_is_case_insensitive_and_pinned_first() {
        let sessions = vec![
            ChatSessionMeta {
                id: "a".into(),
                title: "Project Alpha".into(),
                created_ms: 1,
                updated_ms: 10,
                archived: false,
                pinned: false,
                message_count: 1,
                model_id: None,
                mode: Default::default(),
                members: vec![],
                conductor_policy: Default::default(),
                canvas_open: false,
                canvas_aspect: Default::default(),
            },
            ChatSessionMeta {
                id: "b".into(),
                title: "ALPHA notes".into(),
                created_ms: 1,
                updated_ms: 5,
                archived: false,
                pinned: true,
                message_count: 1,
                model_id: None,
                mode: Default::default(),
                members: vec![],
                conductor_policy: Default::default(),
                canvas_open: false,
                canvas_aspect: Default::default(),
            },
        ];
        let result = filter_and_sort(&sessions, "alpha");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, "b");
    }
}
