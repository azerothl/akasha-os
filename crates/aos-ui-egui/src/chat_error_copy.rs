//! User-visible chat error copy — never leak filesystem paths into bubbles.

use crate::i18n::UiStrings;
use aos_agent::room_runtime::ROOM_ACTION_UNAVAILABLE;
use aos_agent::storage_path::ROOM_HOST_PATH_DISALLOWED;
use std::collections::HashSet;

/// Stable machine code (audit only) + localized short human cause for chat chrome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChatErrorClassified {
    pub code: &'static str,
    pub cause: String,
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
    aos_agent::storage_path::is_host_path_disallowed_outcome(msg)
}

/// Last-segment folder from `room_host_path_disallowed:{folder}`, if safe.
pub(crate) fn room_host_path_folder(msg: &str) -> Option<&str> {
    let rest = msg.trim().strip_prefix(ROOM_HOST_PATH_DISALLOWED)?;
    let name = rest.strip_prefix(':')?.trim();
    if name.is_empty() || name == "?" {
        return None;
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return None;
    }
    Some(name)
}

/// Localized toast: an agent tried to open a folder outside the workspace.
pub(crate) fn room_host_path_disallowed_toast(t: &UiStrings, folder: Option<&str>) -> String {
    if let Some(folder) = folder {
        if !t.room_host_path_disallowed_named.is_empty() {
            return t
                .room_host_path_disallowed_named
                .replace("{folder}", folder);
        }
    }
    t.room_host_path_disallowed.to_string()
}

pub(crate) fn room_host_path_notice_key(ts_ms: u64, text: &str) -> String {
    format!("{ts_ms}\u{1e}{}", text.trim())
}

/// Remember historical sentinels so reopening a session does not toast them.
pub(crate) fn remember_room_host_path_notices(
    toasted: &mut HashSet<String>,
    lines: &[(u64, &str)],
) {
    for (ts, text) in lines {
        if is_room_host_path_sentinel(text) {
            toasted.insert(room_host_path_notice_key(*ts, text));
        }
    }
}

/// Toast copy for sentinels not yet seen this session (one per unique token per ingest).
pub(crate) fn take_new_room_host_path_toasts(
    toasted: &mut HashSet<String>,
    lines: &[(u64, &str)],
    t: &UiStrings,
) -> Vec<String> {
    let mut batch = HashSet::new();
    let mut out = Vec::new();
    for (ts, text) in lines {
        if !is_room_host_path_sentinel(text) {
            continue;
        }
        if !toasted.insert(room_host_path_notice_key(*ts, text)) {
            continue;
        }
        let trimmed = text.trim();
        if !batch.insert(trimmed.to_string()) {
            continue;
        }
        out.push(room_host_path_disallowed_toast(
            t,
            room_host_path_folder(text),
        ));
    }
    out
}

/// True when the runtime error indicates a quarantined Tasks module.
pub(crate) fn is_tasks_quarantine_error(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("tasks") && (lower.contains("quarantaine") || lower.contains("quarantined"))
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
    let rest = trimmed
        .strip_prefix("statut ")
        .or_else(|| trimmed.strip_prefix("Statut "));
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

fn is_room_ask_not_waiting(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("aucune question salon en attente")
        || lower.contains("aucun tour salon en attente")
        || lower.contains("no pending room ask")
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
    if lower.contains("permissiondenied")
        || lower.contains("cap.deny")
        || lower.contains("cap deny")
    {
        return Some("cap.denied");
    }
    if lower.contains("policy.deny") || lower.contains("policy deny") {
        return Some("policy.denied");
    }
    None
}

/// CM-locked toast when a salon ask-reply cannot be delivered.
pub(crate) fn room_ask_unmatched_toast(t: &UiStrings) -> String {
    t.room_ask_failed_toast.to_string()
}

/// Designer chrome: fallback headline plus optional cause line (no wire codes).
pub(crate) fn format_chat_error(t: &UiStrings, classified: &ChatErrorClassified) -> String {
    if classified.code == "chat.error" {
        return t.chat_error_generic.to_string();
    }
    if classified.code == "room.ask_not_waiting" {
        return format!(
            "{}\n{}",
            t.room_ask_failed_toast, classified.cause
        );
    }
    format!("{}\n{}", t.chat_error_generic, classified.cause)
}

/// Classify a raw runtime error into a stable code and localized cause phrase.
pub(crate) fn classify_chat_error(t: &UiStrings, raw: &str) -> ChatErrorClassified {
    if is_room_host_path_sentinel(raw) {
        return ChatErrorClassified {
            code: "room.path_denied",
            cause: room_host_path_disallowed_toast(t, room_host_path_folder(raw)),
        };
    }
    if is_room_ask_not_waiting(raw) {
        return ChatErrorClassified {
            code: "room.ask_not_waiting",
            cause: t.room_ask_not_waiting.to_string(),
        };
    }
    if raw == ROOM_ACTION_UNAVAILABLE || raw.contains(ROOM_ACTION_UNAVAILABLE) {
        return ChatErrorClassified {
            code: "room.action_unavailable",
            cause: t.room_action_unavailable.to_string(),
        };
    }
    let lower = raw.to_ascii_lowercase();
    if lower.contains("indisponible en tour de salon")
        || lower.contains("indisponible en salon")
        || lower.contains("isn't available in the room")
    {
        return ChatErrorClassified {
            code: "room.action_unavailable",
            cause: t.room_action_unavailable.to_string(),
        };
    }
    if raw.starts_with("media.image.generate:") {
        if raw.to_ascii_lowercase().contains("annul") {
            return ChatErrorClassified {
                code: "media.generation_cancelled",
                cause: t.studio_generation_cancelled.to_string(),
            };
        }
        return ChatErrorClassified {
            code: "media.generation_failed",
            cause: t.studio_generation_failed.to_string(),
        };
    }
    if is_tasks_quarantine_error(raw) {
        return ChatErrorClassified {
            code: "tasks.quarantined",
            cause: t.chat_error_tasks_quarantined.to_string(),
        };
    }
    if is_create_install_error(raw) {
        return ChatErrorClassified {
            code: "create.install_failed",
            cause: t.chat_error_create_install.to_string(),
        };
    }
    if is_tasks_open_or_install_error(raw) {
        return ChatErrorClassified {
            code: "tasks.open_failed",
            cause: t.chat_error_tasks_open.to_string(),
        };
    }
    if is_model_load_fail_error(raw) {
        return ChatErrorClassified {
            code: "model.load_failed",
            cause: t.chat_load_fail_message.to_string(),
        };
    }
    if is_chat_timeout_error(raw) {
        return ChatErrorClassified {
            code: "chat.timeout",
            cause: t.chat_error_timeout.to_string(),
        };
    }

    let (ipc_status, ipc_body) = ipc_status_body(raw);
    if let Some(status) = ipc_status {
        let status_lower = status.to_ascii_lowercase();
        if status_lower == "permissiondenied" {
            return ChatErrorClassified {
                code: "cap.denied",
                cause: t.chat_error_cap_denied.to_string(),
            };
        }
        if status_lower == "internalerror" {
            if is_room_ask_not_waiting(ipc_body) {
                return ChatErrorClassified {
                    code: "room.ask_not_waiting",
                    cause: t.room_ask_not_waiting.to_string(),
                };
            }
            if is_agent_spawn_error(ipc_body) {
                return ChatErrorClassified {
                    code: "agent.create.failed",
                    cause: t.chat_error_agent_create_failed.to_string(),
                };
            }
            return ChatErrorClassified {
                code: "chat.internal_error",
                cause: t.chat_error_internal.to_string(),
            };
        }
        if status_lower == "badrequest" {
            if ipc_body.eq_ignore_ascii_case("payload invalide")
                || ipc_body.eq_ignore_ascii_case("invalid payload")
            {
                return ChatErrorClassified {
                    code: "bad_request.payload",
                    cause: t.chat_error_bad_request.to_string(),
                };
            }
            return classify_chat_error(t, ipc_body);
        }
    }

    if lower.contains("brief sous-agent vide") || lower.contains("empty sub-agent brief") {
        return ChatErrorClassified {
            code: "agent.spawn.empty_brief",
            cause: t.chat_error_agent_spawn_empty_brief.to_string(),
        };
    }
    if is_advisory_construction_refusal(raw) {
        return ChatErrorClassified {
            code: "module.scaffold.advisory",
            cause: t.chat_error_module_scaffold_advisory.to_string(),
        };
    }
    if let Some(code) = is_cap_or_policy_denial(raw) {
        let cause = if code == "cap.denied" {
            t.chat_error_cap_denied.to_string()
        } else {
            t.chat_error_policy_denied.to_string()
        };
        return ChatErrorClassified { code, cause };
    }
    if lower.contains("agent déjà en cours") || lower.contains("agent already running") {
        return ChatErrorClassified {
            code: "agent.spawn.busy",
            cause: t.chat_error_agent_spawn_busy.to_string(),
        };
    }
    if is_agent_spawn_error(raw) {
        return ChatErrorClassified {
            code: "agent.spawn.denied",
            cause: t.chat_error_agent_spawn_denied.to_string(),
        };
    }
    if is_module_scaffold_error(raw) {
        return ChatErrorClassified {
            code: "module.scaffold.denied",
            cause: t.chat_error_module_scaffold_denied.to_string(),
        };
    }

    if raw.contains("BadRequest:") || raw.contains("badrequest:") {
        let stripped = strip_ipc_status_prefix(raw);
        if stripped != raw {
            return classify_chat_error(t, stripped);
        }
        return ChatErrorClassified {
            code: "bad_request",
            cause: t.chat_error_bad_request.to_string(),
        };
    }

    if leaks_filesystem_path(raw) || raw.contains(".aospkg") {
        let lower = raw.to_ascii_lowercase();
        if lower.contains("create") {
            return ChatErrorClassified {
                code: "create.install_failed",
                cause: t.chat_error_create_install.to_string(),
            };
        }
        if lower.contains("tasks") {
            return ChatErrorClassified {
                code: "tasks.open_failed",
                cause: t.chat_error_tasks_open.to_string(),
            };
        }
        return ChatErrorClassified {
            code: "chat.error",
            cause: t.chat_error_generic.to_string(),
        };
    }

    let stripped = strip_ipc_status_prefix(raw);
    if stripped != raw {
        return classify_chat_error(t, stripped);
    }

    if raw.trim().is_empty() {
        return ChatErrorClassified {
            code: "chat.error",
            cause: t.chat_error_generic.to_string(),
        };
    }

    ChatErrorClassified {
        code: "chat.error",
        cause: t.chat_error_generic.to_string(),
    }
}

/// Map a raw module UI/runtime error to localized chrome copy (no IPC status leaks).
pub(crate) fn user_visible_module_error(t: &UiStrings, module: &str, raw: &str) -> String {
    let stripped = strip_ipc_status_prefix(raw);
    if module == "tasks" && is_tasks_quarantine_error(stripped) {
        return format_chat_error(
            t,
            &ChatErrorClassified {
                code: "tasks.quarantined",
                cause: t.chat_error_tasks_quarantined.to_string(),
            },
        );
    }
    if is_tasks_quarantine_error(stripped) {
        return format_chat_error(
            t,
            &ChatErrorClassified {
                code: "tasks.quarantined",
                cause: t.chat_error_tasks_quarantined.to_string(),
            },
        );
    }
    if module == "tasks" && is_tasks_open_or_install_error(stripped) {
        return format_chat_error(
            t,
            &ChatErrorClassified {
                code: "tasks.open_failed",
                cause: t.chat_error_tasks_open.to_string(),
            },
        );
    }
    if is_tasks_open_or_install_error(stripped) {
        return format_chat_error(
            t,
            &ChatErrorClassified {
                code: "tasks.open_failed",
                cause: t.chat_error_tasks_open.to_string(),
            },
        );
    }
    if module == "create" && is_create_install_error(stripped) {
        return format_chat_error(
            t,
            &ChatErrorClassified {
                code: "create.install_failed",
                cause: t.chat_error_create_install.to_string(),
            },
        );
    }
    if is_create_install_error(stripped) {
        return format_chat_error(
            t,
            &ChatErrorClassified {
                code: "create.install_failed",
                cause: t.chat_error_create_install.to_string(),
            },
        );
    }
    user_visible_chat_error(t, stripped)
}

/// Create DeclUI status: keep incomplete-pack / missing-annex lists readable.
/// Bare filenames (even `.gguf`) are intentional here; absolute paths still go
/// through the generic scrubber.
pub(crate) fn user_visible_create_or_module_error(
    t: &UiStrings,
    module: &str,
    raw: &str,
) -> String {
    let stripped = strip_ipc_status_prefix(raw).trim();
    if module == "create" && is_incomplete_model_message(stripped) && !has_absolute_path(stripped) {
        return stripped.to_string();
    }
    user_visible_module_error(t, module, raw)
}

fn is_incomplete_model_message(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("modèle incomplet")
        || lower.contains("modele incomplet")
        || lower.contains("incomplete model")
        || lower.contains("fichiers auxiliaires")
        || lower.contains("auxiliary")
        || lower.contains("n'est pas installé")
        || lower.contains("not installed")
}

fn has_absolute_path(msg: &str) -> bool {
    msg.contains('/') || msg.contains('\\')
}

/// Map a raw runtime error to localized chat chrome copy (no path leaks).
pub(crate) fn user_visible_chat_error(t: &UiStrings, raw: &str) -> String {
    format_chat_error(t, &classify_chat_error(t, raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_path_sentinel_copy_names_folder_without_path() {
        let en = crate::i18n::strings("en");
        let named = room_host_path_disallowed_toast(&en, Some("agents"));
        assert!(named.contains("agents"));
        assert!(named.contains("agent"));
        assert!(!named.contains("e:/"));
        assert!(!named.contains("out of reach"));
        assert_eq!(
            room_host_path_folder("room_host_path_disallowed:agents"),
            Some("agents")
        );
        assert_eq!(
            room_host_path_folder("room_host_path_disallowed:C:\\Windows"),
            None
        );
        assert!(is_room_host_path_sentinel("room_host_path_disallowed"));
        assert!(is_room_host_path_sentinel(
            "room_host_path_disallowed:agents"
        ));
        assert!(!is_room_host_path_sentinel(
            "room_host_path_disallowed_other"
        ));
    }

    #[test]
    fn host_path_toasts_once_per_sentinel_then_stick() {
        let en = crate::i18n::strings("en");
        let mut toasted = HashSet::new();
        let first = take_new_room_host_path_toasts(
            &mut toasted,
            &[(1, "room_host_path_disallowed:agents")],
            &en,
        );
        assert_eq!(first.len(), 1);
        assert!(first[0].contains("agents"));
        let second = take_new_room_host_path_toasts(
            &mut toasted,
            &[
                (1, "room_host_path_disallowed:agents"),
                (1, "room_host_path_disallowed:agents"),
            ],
            &en,
        );
        assert!(second.is_empty());
        let mut remembered = HashSet::new();
        remember_room_host_path_notices(
            &mut remembered,
            &[(9, "room_host_path_disallowed:agents")],
        );
        let historical = take_new_room_host_path_toasts(
            &mut remembered,
            &[(9, "room_host_path_disallowed:agents")],
            &en,
        );
        assert!(historical.is_empty());
    }

    #[test]
    fn room_action_unavailable_maps_to_locked_copy() {
        let en = crate::i18n::strings("en");
        let fr = crate::i18n::strings("fr");
        let classified = classify_chat_error(&en, ROOM_ACTION_UNAVAILABLE);
        let out = user_visible_chat_error(&en, ROOM_ACTION_UNAVAILABLE);
        assert_eq!(out, format_chat_error(&en, &classified));
        assert!(out.starts_with(en.chat_error_generic));
        assert!(out.contains(en.room_action_unavailable));
        assert!(!out.contains("BadRequest"));
        assert_eq!(
            user_visible_chat_error(&fr, ROOM_ACTION_UNAVAILABLE),
            format_chat_error(&fr, &classify_chat_error(&fr, ROOM_ACTION_UNAVAILABLE))
        );
    }

    #[test]
    fn room_ask_not_waiting_is_not_internal_error() {
        let en = crate::i18n::strings("en");
        let wrapped = "statut InternalError: statut BadRequest: aucune question salon en attente";
        let classified = classify_chat_error(&en, wrapped);
        assert_eq!(classified.code, "room.ask_not_waiting");
        let out = user_visible_chat_error(&en, wrapped);
        assert!(out.starts_with(en.room_ask_failed_toast));
        assert!(out.contains(en.room_ask_not_waiting));
        assert!(!out.contains("InternalError"));
        assert!(!out.contains("BadRequest"));
        assert_eq!(room_ask_unmatched_toast(&en), en.room_ask_failed_toast);
        let missing_round =
            classify_chat_error(&en, "statut NotFound: aucun tour salon en attente");
        assert_eq!(missing_round.code, "room.ask_not_waiting");
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
        assert!(out.starts_with(t.chat_error_generic));
        assert!(out.contains(t.chat_load_fail_message));
        assert_eq!(
            classify_chat_error(&t, "poids introuvables: C:\\share\\models\\foo.gguf").cause,
            t.chat_load_fail_message
        );
        assert!(!out.contains("gguf"));
        assert!(!out.contains('\\'));
    }

    #[test]
    fn generic_path_leak_uses_fallback_only() {
        let t = crate::i18n::strings("en");
        let out = user_visible_chat_error(&t, "open failed: /var/run/aos-modeld.stderr.log");
        assert_eq!(out, t.chat_error_generic);
        assert!(!out.contains("var/run"));
    }

    #[test]
    fn media_generation_error_does_not_look_like_model_load_failure() {
        let t = crate::i18n::strings("fr");
        let out = user_visible_chat_error(
            &t,
            "media.image.generate: failed to load C:\\share\\models\\ltx.gguf",
        );
        assert!(out.contains(t.studio_generation_failed));
        assert!(!out.contains("gguf"));
    }

    #[test]
    fn tasks_open_failure_maps_to_human_cause() {
        let en = crate::i18n::strings("en");
        let raw = "statut BadRequest: UI déclarative invalide: type must be declarative_ui, got missing field `root` at line 6 column 1";
        let out = user_visible_module_error(&en, "tasks", raw);
        assert!(out.contains(en.chat_error_generic));
        assert!(out.contains(en.chat_error_tasks_open));
        assert!(!out.contains("BadRequest"));
        assert!(!out.contains("root"));
    }

    #[test]
    fn create_install_failure_maps_to_human_cause() {
        let en = crate::i18n::strings("en");
        let raw = "statut BadRequest: hash catalogue non conforme pour create";
        let out = user_visible_chat_error(&en, raw);
        assert!(out.contains(en.chat_error_create_install));
        assert!(!out.contains("BadRequest"));
    }

    #[test]
    fn agent_create_failure_shows_human_cause_not_wire_jargon() {
        let en = crate::i18n::strings("en");
        let spawn = "statut InternalError: spawn worker failed for agent.create";
        let visible = user_visible_chat_error(&en, spawn);
        assert!(visible.contains(en.chat_error_generic));
        assert!(visible.contains(en.chat_error_agent_create_failed));
        assert!(!visible.contains("InternalError"));
        assert!(!visible.contains("agent.create"));
    }

    #[test]
    fn advisory_scaffold_refusal_has_human_cause() {
        let en = crate::i18n::strings("en");
        let raw = "action refusée : le goal est une évaluation/conseil. Analyse et réponds ; ne construis pas le module sans demande explicite.";
        let visible = user_visible_chat_error(&en, raw);
        assert!(visible.contains(en.chat_error_module_scaffold_advisory));
        assert!(!visible.contains("évaluation/conseil"));
    }

    #[test]
    fn chat_timeout_has_human_cause_without_path() {
        let en = crate::i18n::strings("en");
        let raw = "timeout chat (180 s) — modeld a peut-être planté (voir var/run/aos-modeld.stderr.log) ; relancez aos-session";
        let visible = user_visible_chat_error(&en, raw);
        assert!(visible.contains(en.chat_error_timeout));
        assert!(!visible.contains("var/run"));
    }

    #[test]
    fn create_incomplete_model_status_keeps_annex_list() {
        let t = crate::i18n::strings("fr");
        let raw =
            "modèle incomplet : fichiers auxiliaires manquants (qwen_image_vae.safetensors, foo.gguf)";
        let visible = user_visible_create_or_module_error(&t, "create", raw);
        assert!(
            visible.contains("qwen_image_vae.safetensors"),
            "expected annex list in status, got {visible}"
        );
        assert!(visible.contains("foo.gguf"));
    }
}
