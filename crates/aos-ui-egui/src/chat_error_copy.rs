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

/// CM-locked toast copy for disallowed host paths (no raw path in the message).
pub(crate) fn room_host_path_disallowed_toast(t: &UiStrings) -> Option<&'static str> {
    if t.room_host_path_disallowed.is_empty() {
        None
    } else {
        Some(t.room_host_path_disallowed)
    }
}

/// True when the runtime error indicates a quarantined Tasks module.
pub(crate) fn is_tasks_quarantine_error(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("tasks")
        && (lower.contains("quarantaine") || lower.contains("quarantined"))
}

/// True when a catalogue/module install failure targets Create.
pub(crate) fn is_create_install_error(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    if lower.contains(".aospkg") && lower.contains("create") {
        return true;
    }
    if lower.contains("__create_install_failed__") {
        return true;
    }
    lower.contains("create")
        && (lower.contains("catalogue")
            || lower.contains("hash")
            || lower.contains("install")
            || lower.contains("badrequest"))
}

fn strip_ipc_status_prefix(msg: &str) -> &str {
    const PREFIX: &str = "statut BadRequest: ";
    msg.strip_prefix(PREFIX)
        .or_else(|| msg.strip_prefix("statut badrequest: "))
        .unwrap_or(msg)
}

/// Map a raw module UI/runtime error to localized chrome copy (no IPC status leaks).
pub(crate) fn user_visible_module_error(t: &UiStrings, module: &str, raw: &str) -> String {
    let stripped = strip_ipc_status_prefix(raw);
    if module == "tasks" && is_tasks_quarantine_error(stripped) {
        return t.tasks_quarantined.to_string();
    }
    if is_tasks_quarantine_error(stripped) {
        return t.tasks_quarantined.to_string();
    }
    if module == "create" && is_create_install_error(stripped) {
        return t.create_install_failed.to_string();
    }
    if is_create_install_error(stripped) {
        return t.create_install_failed.to_string();
    }
    user_visible_chat_error(t, stripped)
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
    if raw.starts_with("media.image.generate:") {
        if raw.to_ascii_lowercase().contains("annul") {
            return t.studio_generation_cancelled.to_string();
        }
        return t.studio_generation_failed.to_string();
    }
    if is_tasks_quarantine_error(raw) {
        return t.tasks_quarantined.to_string();
    }
    if is_create_install_error(raw) {
        return t.create_install_failed.to_string();
    }
    if raw.contains("BadRequest:") || raw.contains("badrequest:") {
        return t.chat_error_generic.to_string();
    }
    if is_model_load_fail_error(raw) {
        return t.chat_load_fail_message.to_string();
    }
    if leaks_filesystem_path(raw) || raw.contains(".aospkg") {
        if raw.to_ascii_lowercase().contains("create") {
            return t.create_install_failed.to_string();
        }
        return t.chat_error_generic.to_string();
    }
    let stripped = strip_ipc_status_prefix(raw);
    if stripped != raw {
        return user_visible_chat_error(t, stripped);
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

    #[test]
    fn media_generation_error_does_not_look_like_model_load_failure() {
        let t = crate::i18n::strings("fr");
        let out = user_visible_chat_error(
            &t,
            "media.image.generate: failed to load C:\\share\\models\\ltx.gguf",
        );
        assert_eq!(out, t.studio_generation_failed);
    }

    #[test]
    fn media_generation_cancel_maps_to_cancelled_copy() {
        let t = crate::i18n::strings("fr");
        let out = user_visible_chat_error(&t, "media.image.generate: génération annulée");
        assert_eq!(out, t.studio_generation_cancelled);
    }

    #[test]
    fn tasks_quarantine_maps_to_locked_copy() {
        let en = crate::i18n::strings("en");
        let fr = crate::i18n::strings("fr");
        let raw = "statut BadRequest: module en quarantaine: tasks";
        assert_eq!(user_visible_module_error(&en, "tasks", raw), en.tasks_quarantined);
        assert_eq!(user_visible_module_error(&fr, "tasks", raw), fr.tasks_quarantined);
        assert!(!user_visible_module_error(&en, "tasks", raw).contains("BadRequest"));
    }

    #[test]
    fn create_install_failure_maps_to_locked_copy() {
        let en = crate::i18n::strings("en");
        let fr = crate::i18n::strings("fr");
        let raw = "statut BadRequest: hash catalogue non conforme pour create";
        assert_eq!(
            user_visible_chat_error(&en, raw),
            en.create_install_failed
        );
        assert_eq!(
            user_visible_chat_error(&fr, raw),
            fr.create_install_failed
        );
        assert!(!user_visible_chat_error(&en, raw).contains("BadRequest"));
        assert!(!user_visible_chat_error(&en, raw).contains(".aospkg"));
    }

    #[test]
    fn room_host_path_disallowed_maps_to_locked_copy() {
        let en = crate::i18n::strings("en");
        let fr = crate::i18n::strings("fr");
        assert_eq!(
            user_visible_chat_error(&en, ROOM_HOST_PATH_DISALLOWED),
            en.room_host_path_disallowed
        );
        assert_eq!(
            user_visible_chat_error(&fr, ROOM_HOST_PATH_DISALLOWED),
            fr.room_host_path_disallowed
        );
        assert_eq!(
            room_host_path_disallowed_toast(&en),
            Some(en.room_host_path_disallowed)
        );
        assert!(!en.room_host_path_disallowed.contains('/'));
        assert!(!en.room_host_path_disallowed.contains(':'));
        assert!(!fr.room_host_path_disallowed.contains('/'));
        assert!(!fr.room_host_path_disallowed.contains(':'));
    }
}
