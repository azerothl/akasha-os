//! User-visible chat error copy — never leak filesystem paths into bubbles.

use crate::i18n::UiStrings;
use aos_agent::room_runtime::ROOM_ACTION_UNAVAILABLE;
use aos_agent::storage_path::ROOM_HOST_PATH_DISALLOWED;

/// True when the runtime error is a model weight load failure (often embeds a `.gguf` path).
pub(crate) fn is_model_load_fail_error(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains(".gguf")
        || lower.contains("poids introuvables")
        || lower.contains("aucun chemin de poids")
        || lower.contains("échec chargement")
        || lower.contains("failed to load")
        || lower.contains("could not load")
        || lower.contains("couldn't load")
        || lower.contains("unable to load")
        || lower.contains("load failed")
        || (lower.contains("share/models") || lower.contains("share\\models"))
}

/// True when a string likely exposes a local path to the user.
pub(crate) fn leaks_filesystem_path(msg: &str) -> bool {
    if msg.contains(".gguf") {
        return true;
    }
    if msg.contains('\\') {
        return true;
    }
    if msg.contains("/share/models/") || msg.contains("share/models/") {
        return true;
    }
    if msg.contains("/var/models/") || msg.contains("var/models/") {
        return true;
    }
    // Windows drive letter (e.g. `C:\Users\...`).
    let bytes = msg.as_bytes();
    bytes
        .windows(2)
        .any(|w| w[0].is_ascii_alphabetic() && w[1] == b':')
}

/// True when the runtime posted the host-path sentinel (toast hook only).
pub(crate) fn is_room_host_path_sentinel(msg: &str) -> bool {
    msg.trim() == ROOM_HOST_PATH_DISALLOWED
}

/// CM-locked toast copy for disallowed host paths — `None` until strings are set.
pub(crate) fn room_host_path_disallowed_toast(t: &UiStrings) -> Option<&'static str> {
    if t.room_host_path_disallowed.is_empty() {
        None
    } else {
        Some(t.room_host_path_disallowed)
    }
}

/// Map a raw runtime error to localized chat chrome copy (no path leaks).
pub(crate) fn user_visible_chat_error(t: &UiStrings, raw: &str) -> String {
    if is_room_host_path_sentinel(raw) {
        return room_host_path_disallowed_toast(t)
            .map(str::to_string)
            .unwrap_or_else(|| ROOM_HOST_PATH_DISALLOWED.to_string());
    }
    if raw == ROOM_ACTION_UNAVAILABLE || raw.contains(ROOM_ACTION_UNAVAILABLE) {
        return t.room_action_unavailable.to_string();
    }
    let lower = raw.to_ascii_lowercase();
    if lower.contains("indisponible en tour de salon")
        || lower.contains("indisponible en salon")
        || lower.contains("isn't available in the room")
    {
        return t.room_action_unavailable.to_string();
    }
    if is_model_load_fail_error(raw) {
        return t.chat_load_fail_message.to_string();
    }
    if leaks_filesystem_path(raw) {
        return t.chat_error_generic.to_string();
    }
    raw.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_action_unavailable_maps_to_locked_copy() {
        let en = crate::i18n::strings("en");
        let fr = crate::i18n::strings("fr");
        assert_eq!(
            user_visible_chat_error(&en, ROOM_ACTION_UNAVAILABLE),
            en.room_action_unavailable
        );
        assert_eq!(
            user_visible_chat_error(&fr, ROOM_ACTION_UNAVAILABLE),
            fr.room_action_unavailable
        );
        assert_eq!(
            user_visible_chat_error(&en, "action notes.create indisponible en tour de salon"),
            en.room_action_unavailable
        );
    }

    #[test]
    fn model_load_fail_detects_gguf_path() {
        assert!(is_model_load_fail_error(
            "poids introuvables: C:\\Users\\me\\share\\models\\qwen.gguf"
        ));
    }

    #[test]
    fn sanitize_replaces_path_with_i18n_load_fail() {
        let t = crate::i18n::strings("fr");
        let out = user_visible_chat_error(&t, "poids introuvables: C:\\share\\models\\foo.gguf");
        assert_eq!(out, t.chat_load_fail_message);
        assert!(!out.contains("gguf"));
        assert!(!out.contains('\\'));
    }

    #[test]
    fn generic_path_leak_uses_generic_copy() {
        let t = crate::i18n::strings("en");
        let out = user_visible_chat_error(&t, "open failed: /var/run/aos-modeld.stderr.log");
        assert_eq!(out, t.chat_error_generic);
    }
}
