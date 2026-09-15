//! User-visible chat error copy — never leak filesystem paths into bubbles.

use crate::i18n::UiStrings;
use aos_agent::room_runtime::ROOM_ACTION_UNAVAILABLE;
use aos_agent::storage_path::ROOM_HOST_PATH_DISALLOWED;

/// Stable machine code + localized short reason for chat chrome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChatErrorClassified {
    pub code: &'static str,
    pub reason: String,
}

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

/// True when a Tasks open/install failure should use locked human chrome copy.
pub(crate) fn is_tasks_open_or_install_error(msg: &str) -> bool {
    if is_tasks_quarantine_error(msg) {
        return false;
    }
    let lower = msg.to_ascii_lowercase();
    if lower.contains("__tasks_open_failed__") {
        return true;
    }
    if lower.contains("ui déclarative invalide")
        || lower.contains("decluiinvalid")
        || lower.contains("declarative_ui")
        || lower.contains("missing field")
        || lower.contains("type must be declarative_ui")
    {
        return true;
    }
    lower.contains("tasks")
        && (lower.contains("catalogue")
            || lower.contains("hash")
            || lower.contains("install")
            || lower.contains("badrequest")
            || lower.contains(".aospkg"))
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

fn ipc_status_body(msg: &str) -> (Option<&str>, &str) {
    let trimmed = msg.trim();
    let rest = trimmed.strip_prefix("statut ").or_else(|| trimmed.strip_prefix("Statut "));
    if let Some(rest) = rest {
        if let Some(colon) = rest.find(':') {
            let status = rest[..colon].trim();
            let body = rest[colon + 1..].trim_start();
            return (Some(status), body);
        }
    }
    (None, trimmed)
}

fn is_chat_timeout_error(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("timeout chat") || lower.contains("chat timeout")
}

fn is_advisory_construction_refusal(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("évaluation/conseil")
        || lower.contains("evaluation/conseil")
        || lower.contains("evaluation/advice")
        || (lower.contains("conseil") && lower.contains("module") && lower.contains("refus"))
}

fn is_agent_spawn_error(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("agent.create")
        || lower.contains("agent.spawn")
        || lower.contains("brief sous-agent")
        || lower.contains("sous-agent")
        || lower.contains("agent déjà en cours")
        || lower.contains("agent already")
}

fn is_module_scaffold_error(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("module.scaffold")
        || lower.contains("module.package")
        || lower.contains("module.install")
        || lower.contains("module.compile")
}

fn is_cap_or_policy_denial(msg: &str) -> Option<&'static str> {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("permissiondenied") || lower.contains("cap.deny") || lower.contains("cap deny")
    {
        return Some("cap.denied");
    }
    if lower.contains("policy.deny") || lower.contains("policy deny") {
        return Some("policy.denied");
    }
    None
}

pub(crate) fn format_chat_error(classified: &ChatErrorClassified) -> String {
    format!("{} — {}", classified.code, classified.reason)
}

/// Classify a raw runtime error into a stable code and localized reason.
pub(crate) fn classify_chat_error(t: &UiStrings, raw: &str) -> ChatErrorClassified {
    if is_room_host_path_sentinel(raw) {
        return ChatErrorClassified {
            code: "room.path_denied",
            reason: room_host_path_disallowed_toast(t)
                .map(str::to_string)
                .unwrap_or_else(|| ROOM_HOST_PATH_DISALLOWED.to_string()),
        };
    }
    if raw == ROOM_ACTION_UNAVAILABLE || raw.contains(ROOM_ACTION_UNAVAILABLE) {
        return ChatErrorClassified {
            code: "room.action_unavailable",
            reason: t.room_action_unavailable.to_string(),
        };
    }
    let lower = raw.to_ascii_lowercase();
    if lower.contains("indisponible en tour de salon")
        || lower.contains("indisponible en salon")
        || lower.contains("isn't available in the room")
    {
        return ChatErrorClassified {
            code: "room.action_unavailable",
            reason: t.room_action_unavailable.to_string(),
        };
    }
    if raw.starts_with("media.image.generate:") {
        if raw.to_ascii_lowercase().contains("annul") {
            return ChatErrorClassified {
                code: "media.generation_cancelled",
                reason: t.studio_generation_cancelled.to_string(),
            };
        }
        return ChatErrorClassified {
            code: "media.generation_failed",
            reason: t.studio_generation_failed.to_string(),
        };
    }
    if is_tasks_quarantine_error(raw) {
        return ChatErrorClassified {
            code: "tasks.quarantined",
            reason: t.tasks_quarantined.to_string(),
        };
    }
    if is_create_install_error(raw) {
        return ChatErrorClassified {
            code: "create.install_failed",
            reason: t.create_install_failed.to_string(),
        };
    }
    if is_tasks_open_or_install_error(raw) {
        return ChatErrorClassified {
            code: "tasks.open_failed",
            reason: t.tasks_open_failed.to_string(),
        };
    }
    if is_model_load_fail_error(raw) {
        return ChatErrorClassified {
            code: "model.load_failed",
            reason: t.chat_load_fail_message.to_string(),
        };
    }
    if is_chat_timeout_error(raw) {
        return ChatErrorClassified {
            code: "chat.timeout",
            reason: t.chat_error_timeout.to_string(),
        };
    }

    let (ipc_status, ipc_body) = ipc_status_body(raw);
    if let Some(status) = ipc_status {
        let status_lower = status.to_ascii_lowercase();
        if status_lower == "permissiondenied" {
            return ChatErrorClassified {
                code: "cap.denied",
                reason: t.chat_error_cap_denied.to_string(),
            };
        }
        if status_lower == "internalerror" {
            if is_agent_spawn_error(ipc_body) {
                return ChatErrorClassified {
                    code: "agent.create.failed",
                    reason: t.chat_error_agent_create_failed.to_string(),
                };
            }
            return ChatErrorClassified {
                code: "chat.internal_error",
                reason: t.chat_error_internal.to_string(),
            };
        }
        if status_lower == "badrequest" {
            if ipc_body.eq_ignore_ascii_case("payload invalide")
                || ipc_body.eq_ignore_ascii_case("invalid payload")
            {
                return ChatErrorClassified {
                    code: "bad_request.payload",
                    reason: t.chat_error_bad_request.to_string(),
                };
            }
            return classify_chat_error(t, ipc_body);
        }
    }

    if lower.contains("brief sous-agent vide") || lower.contains("empty sub-agent brief") {
        return ChatErrorClassified {
            code: "agent.spawn.empty_brief",
            reason: t.chat_error_agent_spawn_empty_brief.to_string(),
        };
    }
    if is_advisory_construction_refusal(raw) {
        return ChatErrorClassified {
            code: "module.scaffold.advisory",
            reason: t.chat_error_module_scaffold_advisory.to_string(),
        };
    }
    if let Some(code) = is_cap_or_policy_denial(raw) {
        let reason = if code == "cap.denied" {
            t.chat_error_cap_denied.to_string()
        } else {
            t.chat_error_policy_denied.to_string()
        };
        return ChatErrorClassified { code, reason };
    }
    if lower.contains("agent déjà en cours") || lower.contains("agent already running") {
        return ChatErrorClassified {
            code: "agent.spawn.busy",
            reason: t.chat_error_agent_spawn_busy.to_string(),
        };
    }
    if is_agent_spawn_error(raw) {
        return ChatErrorClassified {
            code: "agent.spawn.denied",
            reason: t.chat_error_agent_spawn_denied.to_string(),
        };
    }
    if is_module_scaffold_error(raw) {
        return ChatErrorClassified {
            code: "module.scaffold.denied",
            reason: t.chat_error_module_scaffold_denied.to_string(),
        };
    }

    if raw.contains("BadRequest:") || raw.contains("badrequest:") {
        let stripped = strip_ipc_status_prefix(raw);
        if stripped != raw {
            return classify_chat_error(t, stripped);
        }
        return ChatErrorClassified {
            code: "bad_request",
            reason: t.chat_error_bad_request.to_string(),
        };
    }

    if leaks_filesystem_path(raw) || raw.contains(".aospkg") {
        let lower = raw.to_ascii_lowercase();
        if lower.contains("create") {
            return ChatErrorClassified {
                code: "create.install_failed",
                reason: t.create_install_failed.to_string(),
            };
        }
        if lower.contains("tasks") {
            return ChatErrorClassified {
                code: "tasks.open_failed",
                reason: t.tasks_open_failed.to_string(),
            };
        }
        return ChatErrorClassified {
            code: "chat.error",
            reason: t.chat_error_generic.to_string(),
        };
    }

    let stripped = strip_ipc_status_prefix(raw);
    if stripped != raw {
        return classify_chat_error(t, stripped);
    }

    if raw.trim().is_empty() {
        return ChatErrorClassified {
            code: "chat.error",
            reason: t.chat_error_generic.to_string(),
        };
    }

    ChatErrorClassified {
        code: "chat.error",
        reason: t.chat_error_generic.to_string(),
    }
}

/// Map a raw module UI/runtime error to localized chrome copy (no IPC status leaks).
pub(crate) fn user_visible_module_error(t: &UiStrings, module: &str, raw: &str) -> String {
    let stripped = strip_ipc_status_prefix(raw);
    if module == "tasks" && is_tasks_quarantine_error(stripped) {
        return format_chat_error(&ChatErrorClassified {
            code: "tasks.quarantined",
            reason: t.tasks_quarantined.to_string(),
        });
    }
    if is_tasks_quarantine_error(stripped) {
        return format_chat_error(&ChatErrorClassified {
            code: "tasks.quarantined",
            reason: t.tasks_quarantined.to_string(),
        });
    }
    if module == "tasks" && is_tasks_open_or_install_error(stripped) {
        return format_chat_error(&ChatErrorClassified {
            code: "tasks.open_failed",
            reason: t.tasks_open_failed.to_string(),
        });
    }
    if is_tasks_open_or_install_error(stripped) {
        return format_chat_error(&ChatErrorClassified {
            code: "tasks.open_failed",
            reason: t.tasks_open_failed.to_string(),
        });
    }
    if module == "create" && is_create_install_error(stripped) {
        return format_chat_error(&ChatErrorClassified {
            code: "create.install_failed",
            reason: t.create_install_failed.to_string(),
        });
    }
    if is_create_install_error(stripped) {
        return format_chat_error(&ChatErrorClassified {
            code: "create.install_failed",
            reason: t.create_install_failed.to_string(),
        });
    }
    user_visible_chat_error(t, stripped)
}

/// Map a raw runtime error to localized chat chrome copy (no path leaks).
pub(crate) fn user_visible_chat_error(t: &UiStrings, raw: &str) -> String {
    format_chat_error(&classify_chat_error(t, raw))
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
            format_chat_error(&classify_chat_error(&en, ROOM_ACTION_UNAVAILABLE))
        );
        assert!(user_visible_chat_error(&en, ROOM_ACTION_UNAVAILABLE).contains("room.action_unavailable"));
        assert_eq!(
            user_visible_chat_error(&fr, ROOM_ACTION_UNAVAILABLE),
            format_chat_error(&classify_chat_error(&fr, ROOM_ACTION_UNAVAILABLE))
        );
        assert_eq!(
            user_visible_chat_error(&en, "action notes.create indisponible en tour de salon"),
            format_chat_error(&classify_chat_error(
                &en,
                "action notes.create indisponible en tour de salon"
            ))
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
        assert!(out.contains("model.load_failed"));
        assert_eq!(
            classify_chat_error(&t, "poids introuvables: C:\\share\\models\\foo.gguf").reason,
            t.chat_load_fail_message
        );
        assert!(!out.contains("gguf"));
        assert!(!out.contains('\\'));
    }

    #[test]
    fn generic_path_leak_uses_generic_copy_with_code() {
        let t = crate::i18n::strings("en");
        let out = user_visible_chat_error(&t, "open failed: /var/run/aos-modeld.stderr.log");
        assert!(out.starts_with("chat.error —"));
        assert_eq!(
            classify_chat_error(&t, "open failed: /var/run/aos-modeld.stderr.log").reason,
            t.chat_error_generic
        );
    }

    #[test]
    fn media_generation_error_does_not_look_like_model_load_failure() {
        let t = crate::i18n::strings("fr");
        let out = user_visible_chat_error(
            &t,
            "media.image.generate: failed to load C:\\share\\models\\ltx.gguf",
        );
        assert!(out.contains("media.generation_failed"));
        assert_eq!(
            classify_chat_error(
                &t,
                "media.image.generate: failed to load C:\\share\\models\\ltx.gguf"
            )
            .reason,
            t.studio_generation_failed
        );
    }

    #[test]
    fn media_generation_cancel_maps_to_cancelled_copy() {
        let t = crate::i18n::strings("fr");
        let out = user_visible_chat_error(&t, "media.image.generate: génération annulée");
        assert!(out.contains("media.generation_cancelled"));
    }

    #[test]
    fn tasks_open_failure_maps_to_locked_copy() {
        let en = crate::i18n::strings("en");
        let fr = crate::i18n::strings("fr");
        let raw = "statut BadRequest: UI déclarative invalide: type must be declarative_ui, got missing field `root` at line 6 column 1";
        let out_en = user_visible_module_error(&en, "tasks", raw);
        let out_fr = user_visible_module_error(&fr, "tasks", raw);
        assert!(out_en.contains("tasks.open_failed"));
        assert!(out_fr.contains("tasks.open_failed"));
        assert!(!out_en.contains("BadRequest"));
        assert!(!out_en.contains("root"));
        assert!(!out_en.contains("declarative"));
    }

    #[test]
    fn tasks_catalogue_install_failure_maps_to_locked_copy() {
        let en = crate::i18n::strings("en");
        let fr = crate::i18n::strings("fr");
        let raw = "statut BadRequest: hash catalogue non conforme pour tasks";
        assert!(user_visible_chat_error(&en, raw).contains("tasks.open_failed"));
        assert!(user_visible_chat_error(&fr, raw).contains("tasks.open_failed"));
        assert!(!user_visible_chat_error(&en, raw).contains("BadRequest"));
    }

    #[test]
    fn tasks_quarantine_maps_to_locked_copy() {
        let en = crate::i18n::strings("en");
        let fr = crate::i18n::strings("fr");
        let raw = "statut BadRequest: module en quarantaine: tasks";
        assert!(user_visible_module_error(&en, "tasks", raw).contains("tasks.quarantined"));
        assert!(user_visible_module_error(&fr, "tasks", raw).contains("tasks.quarantined"));
        assert!(!user_visible_module_error(&en, "tasks", raw).contains("BadRequest"));
    }

    #[test]
    fn create_install_failure_maps_to_locked_copy() {
        let en = crate::i18n::strings("en");
        let fr = crate::i18n::strings("fr");
        let raw = "statut BadRequest: hash catalogue non conforme pour create";
        assert!(user_visible_chat_error(&en, raw).contains("create.install_failed"));
        assert!(user_visible_chat_error(&fr, raw).contains("create.install_failed"));
        assert!(!user_visible_chat_error(&en, raw).contains("BadRequest"));
        assert!(!user_visible_chat_error(&en, raw).contains(".aospkg"));
    }

    #[test]
    fn room_host_path_disallowed_maps_to_locked_copy() {
        let en = crate::i18n::strings("en");
        let fr = crate::i18n::strings("fr");
        assert!(user_visible_chat_error(&en, ROOM_HOST_PATH_DISALLOWED).contains("room.path_denied"));
        assert!(user_visible_chat_error(&fr, ROOM_HOST_PATH_DISALLOWED).contains("room.path_denied"));
        assert_eq!(
            room_host_path_disallowed_toast(&en),
            Some(en.room_host_path_disallowed)
        );
        assert!(!en.room_host_path_disallowed.contains('/'));
        assert!(!en.room_host_path_disallowed.contains(':'));
        assert!(!fr.room_host_path_disallowed.contains('/'));
        assert!(!fr.room_host_path_disallowed.contains(':'));
    }

    #[test]
    fn agent_create_bad_request_maps_to_spawn_denied() {
        let en = crate::i18n::strings("en");
        let raw = "statut BadRequest: payload invalide";
        let classified = classify_chat_error(&en, raw);
        assert_eq!(classified.code, "bad_request.payload");
        let spawn = "statut InternalError: spawn worker failed for agent.create";
        let classified_spawn = classify_chat_error(&en, spawn);
        assert_eq!(classified_spawn.code, "agent.create.failed");
        let visible = user_visible_chat_error(&en, spawn);
        assert!(visible.contains("agent.create.failed"));
        assert!(!visible.contains("InternalError"));
        assert!(!visible.contains("statut"));
    }

    #[test]
    fn advisory_scaffold_refusal_has_stable_code() {
        let en = crate::i18n::strings("en");
        let raw = "action refusée : le goal est une évaluation/conseil. Analyse et réponds ; ne construis pas le module sans demande explicite.";
        let classified = classify_chat_error(&en, raw);
        assert_eq!(classified.code, "module.scaffold.advisory");
        assert!(user_visible_chat_error(&en, raw).contains("module.scaffold.advisory"));
    }

    #[test]
    fn chat_timeout_has_stable_code_without_path() {
        let en = crate::i18n::strings("en");
        let raw = "timeout chat (180 s) — modeld a peut-être planté (voir var/run/aos-modeld.stderr.log) ; relancez aos-session";
        let visible = user_visible_chat_error(&en, raw);
        assert!(visible.contains("chat.timeout"));
        assert!(!visible.contains("var/run"));
        assert!(!visible.contains(".log"));
    }
}
