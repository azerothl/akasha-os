//! Per-task capability prompt guard (issue #480).
//!
//! Model-facing strings stay technical-free where they surface to users via UI
//! sentinels; never embed capability ids in user-visible copy.

use crate::policy::NET_TOOLS;
use crate::tools::canonicalize_tool_name;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Chat sentinel — UI maps to localized copy (no capability ids).
pub const WEB_ACCESS_DENIED_SENTINEL: &str = "chat_web_access_denied";
pub const WEB_ACCESS_UNAVAILABLE_SENTINEL: &str = "chat_web_access_unavailable";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WebAccessMode {
    #[default]
    Normal,
    DeniedForTask,
    UnavailableForTask,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskCapabilityState {
    pub web_access: WebAccessMode,
    pub web_notice_posted: bool,
    #[serde(default)]
    pub cap_requests: HashSet<String>,
    #[serde(default)]
    pub cap_denied: HashSet<String>,
    #[serde(default)]
    pub cap_granted: HashSet<String>,
    #[serde(default)]
    pub blocked_user_ask_keys: HashSet<String>,
    pub interactive_auth_flagged: bool,
    pub interactive_auth_notice_posted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebNoticeKind {
    Denied,
    Unavailable,
}

pub fn parse_web_notice_sentinel(text: &str) -> Option<WebNoticeKind> {
    let trimmed = text.trim();
    if trimmed == WEB_ACCESS_DENIED_SENTINEL {
        Some(WebNoticeKind::Denied)
    } else if trimmed == WEB_ACCESS_UNAVAILABLE_SENTINEL {
        Some(WebNoticeKind::Unavailable)
    } else {
        None
    }
}

pub fn web_notice_sentinel(kind: WebNoticeKind) -> &'static str {
    match kind {
        WebNoticeKind::Denied => WEB_ACCESS_DENIED_SENTINEL,
        WebNoticeKind::Unavailable => WEB_ACCESS_UNAVAILABLE_SENTINEL,
    }
}

pub fn is_web_tool(name: &str) -> bool {
    let canonical = canonicalize_tool_name(name);
    NET_TOOLS.contains(&canonical.as_str())
        || canonical.starts_with("web.")
        || canonical == "cap.request"
}

pub fn is_net_capability(cap: &str) -> bool {
    let lower = cap.to_ascii_lowercase();
    lower.starts_with("net.connect")
        || lower.starts_with("net.")
        || lower.contains("web.")
}

/// Map human tool labels (UI i18n) back to catalog ids when models emit them as `action`.
pub fn resolve_display_action_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.contains('.') {
        return trimmed.to_string();
    }
    match trimmed.to_ascii_lowercase().as_str() {
        "spawn sub-agent" | "spawn sub agent" | "sub-agent" | "sub agent" => {
            "agent.spawn".into()
        }
        "await sub-agent" | "await sub agent" => "agent.await".into(),
        "web search" => "web.search".into(),
        "browse page" | "web browse" => "web.browse".into(),
        "fetch url" | "net fetch" => "net.fetch".into(),
        _ => match trimmed {
            "Spawn sub-agent" => "agent.spawn".into(),
            "Await sub-agent" => "agent.await".into(),
            "Web search" => "web.search".into(),
            "Browse page" => "web.browse".into(),
            "Fetch URL" => "net.fetch".into(),
            "Déléguer à un sous-agent" => "agent.spawn".into(),
            "Attendre un sous-agent" => "agent.await".into(),
            "Recherche web" => "web.search".into(),
            "Parcourir une page" => "web.browse".into(),
            "Télécharger une URL" => "net.fetch".into(),
            other => other.to_string(),
        },
    }
}

pub fn goal_needs_interactive_auth(goal: &str) -> bool {
    let lower = goal.to_ascii_lowercase();
    let auth = lower.contains("login")
        || lower.contains("log in")
        || lower.contains("sign in")
        || lower.contains("sign up")
        || lower.contains("register")
        || lower.contains("create an account")
        || lower.contains("create a account")
        || lower.contains("créer un compte")
        || lower.contains("creer un compte")
        || lower.contains("inscription")
        || lower.contains("s'inscrire")
        || lower.contains("se connecter")
        || lower.contains("connexion")
        || lower.contains("mot de passe")
        || lower.contains("password");
    let webish = lower.contains("account")
        || lower.contains("compte")
        || lower.contains("website")
        || lower.contains("site web")
        || lower.contains("http")
        || lower.contains("www.");
    auth && webish
}

pub fn interactive_auth_runtime_message() -> &'static str {
    "Cette tâche nécessite une authentification interactive sur un site (connexion ou inscription). \
     Les outils web actuels ne font que la recherche et la lecture HTML statique — pas de JavaScript \
     ni de formulaire interactif. Explique la limite à l'utilisateur et termine avec goal.complete \
     ou goal.fail ; ne redemande pas l'accès web ni cap.request."
}

pub fn user_reply_not_os_grant_message() -> &'static str {
    "La réponse utilisateur dans le chat n'accorde pas une capacité OS. \
     Si une autorisation est nécessaire, appelle cap.request une seule fois et attends le résultat \
     (accord UI ou refus). Sinon continue sans accès web."
}

pub fn question_looks_like_capability_prompt(question: &str) -> bool {
    let lower = question.to_ascii_lowercase();
    (lower.contains("capac")
        || lower.contains("autor")
        || lower.contains("activ")
        || lower.contains("accès")
        || lower.contains("acces")
        || lower.contains("access")
        || lower.contains("permission")
        || lower.contains("réseau")
        || lower.contains("reseau")
        || lower.contains("network")
        || lower.contains("navigation")
        || lower.contains("web"))
        && (lower.contains('?')
            || lower.contains("voulez")
            || lower.contains("souhaite")
            || lower.contains("puis-je")
            || lower.contains("do you")
            || lower.contains("should i")
            || lower.contains("enable")
            || lower.contains("activer"))
}

pub fn user_ask_fingerprint(question: &str) -> String {
    question
        .split_whitespace()
        .take(12)
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

pub fn block_capability_user_ask(state: &TaskCapabilityState, question: &str) -> Option<String> {
    if !question_looks_like_capability_prompt(question) {
        return None;
    }
    let key = user_ask_fingerprint(question);
    if state.blocked_user_ask_keys.contains(&key) {
        return Some(
            "user.ask bloqué : cette question d'autorisation a déjà été posée. \
             N'utilise pas user.ask pour les capacités — cap.request une seule fois ou continue sans web."
                .into(),
        );
    }
    if state.web_access != WebAccessMode::Normal {
        return Some(
            "user.ask bloqué : l'accès web est déjà refusé ou indisponible pour cette tâche. \
             Continue sans web ou termine avec une explication claire."
                .into(),
        );
    }
    None
}

pub fn note_capability_user_ask(state: &mut TaskCapabilityState, question: &str) {
    if question_looks_like_capability_prompt(question) {
        state
            .blocked_user_ask_keys
            .insert(user_ask_fingerprint(question));
    }
}

pub fn cap_request_allowed(state: &TaskCapabilityState, cap: &str) -> bool {
    if state.interactive_auth_flagged {
        return false;
    }
    if state.web_access != WebAccessMode::Normal && is_net_capability(cap) {
        return false;
    }
    !state.cap_requests.contains(cap)
}

pub fn record_cap_request(state: &mut TaskCapabilityState, cap: &str) {
    state.cap_requests.insert(cap.to_string());
}

pub fn record_cap_outcome(state: &mut TaskCapabilityState, cap: &str, granted: bool, denied: bool) {
    if granted {
        state.cap_granted.insert(cap.to_string());
    }
    if denied {
        state.cap_denied.insert(cap.to_string());
        if is_net_capability(cap) {
            state.web_access = WebAccessMode::DeniedForTask;
        }
    }
}

pub fn mark_web_denied(state: &mut TaskCapabilityState) {
    state.web_access = WebAccessMode::DeniedForTask;
}

pub fn mark_web_unavailable(state: &mut TaskCapabilityState) {
    if state.web_access == WebAccessMode::Normal {
        state.web_access = WebAccessMode::UnavailableForTask;
    }
}

pub fn take_web_notice_kind(state: &mut TaskCapabilityState) -> Option<WebNoticeKind> {
    if state.web_notice_posted {
        return None;
    }
    let kind = match state.web_access {
        WebAccessMode::DeniedForTask => WebNoticeKind::Denied,
        WebAccessMode::UnavailableForTask => WebNoticeKind::Unavailable,
        WebAccessMode::Normal => return None,
    };
    state.web_notice_posted = true;
    Some(kind)
}

pub fn should_append_cap_request_hint(
    state: &TaskCapabilityState,
    tool: &str,
    outcome: &str,
) -> bool {
    if state.interactive_auth_flagged {
        return false;
    }
    if state.web_access != WebAccessMode::Normal && is_web_tool(tool) {
        return false;
    }
    if !outcome_looks_like_missing_cap(outcome) {
        return false;
    }
    true
}

pub fn cap_request_hint(state: &TaskCapabilityState, tool: &str, hint_cap: &str) -> Option<String> {
    if !should_append_cap_request_hint(state, tool, "permissiondenied") {
        return None;
    }
    if !cap_request_allowed(state, hint_cap) {
        return None;
    }
    Some(format!(
        "\n[hint] Demande la capacité une seule fois : cap.request {{\"cap\":\"{hint_cap}\",\"reason\":\"…\"}}"
    ))
}

pub fn follow_up_missing_cap(
    state: &mut TaskCapabilityState,
    tool: &str,
    outcome: &str,
    hint_cap: &str,
) -> String {
    let mut out = outcome.to_string();
    if state.interactive_auth_flagged {
        return out;
    }
    if !outcome_looks_like_missing_cap(outcome) {
        return out;
    }
    if state.web_access != WebAccessMode::Normal && (is_web_tool(tool) || is_net_capability(hint_cap)) {
        if !out.to_ascii_lowercase().contains("indisponible") {
            out.push_str(
                "\n[runtime] Accès web indisponible pour le reste de la tâche — continue sans web ou termine.",
            );
        }
        return out;
    }
    if is_web_tool(tool) || is_net_capability(hint_cap) {
        if state.cap_requests.contains(hint_cap) || state.cap_denied.contains(hint_cap) {
            mark_web_unavailable(state);
            out.push_str(
                "\n[runtime] Accès web indisponible pour le reste de la tâche — continue sans web ou termine.",
            );
            return out;
        }
    }
    if let Some(hint) = cap_request_hint(state, tool, hint_cap) {
        out.push_str(&hint);
    } else if is_web_tool(tool) {
        mark_web_unavailable(state);
        out.push_str(
            "\n[runtime] Ne redemande pas la même autorisation — explique la limite puis goal.complete ou goal.fail.",
        );
    }
    out
}

pub fn unknown_catalog_action_message(action: &str) -> String {
    let resolved = resolve_display_action_name(action);
    let canonical = canonicalize_tool_name(&resolved);
    if canonical != action.trim() && canonical.contains('.') {
        return format!(
            "ERREUR action : « {action} » n'est pas un outil du catalogue. \
             Utilise l'id exact (ex. `{canonical}`) ou un outil listé — pas de libellé inventé."
        );
    }
    "ERREUR action : outil absent du catalogue actif. \
     Choisis un outil proposé dans ta liste d'outils ou termine avec une explication claire — \
     n'invente pas de noms de capacité."
        .to_string()
}

pub fn interactive_auth_block_message() -> &'static str {
    "Tâche non supportée : création de compte / connexion interactive sur un site. \
     Les outils web ne remplissent pas de formulaires. Termine avec goal.fail ou goal.complete \
     en expliquant la limite — sans cap.request ni user.ask sur l'accès web."
}

/// True for ActorDenied / PermissionDenied — not JSON fields like `os_permission`.
pub fn outcome_looks_like_missing_cap(outcome: &str) -> bool {
    let lower = outcome.to_ascii_lowercase();
    if lower.contains("périphérique occupé")
        || lower.contains("peripherique occupe")
        || lower.contains("device busy")
    {
        return false;
    }
    lower.contains("permissiondenied")
        || lower.contains("actordenied")
        || lower.contains("permission refus")
        || lower.contains("capacité requise")
        || lower.contains("capacité manquante")
        || lower.contains("confiance trop faible")
        || lower.contains("confiance faible")
        || lower.contains("bloqué par la politique réseau")
        || lower.contains("net:deny")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_notice_sentinels_are_stable() {
        assert_eq!(parse_web_notice_sentinel("chat_web_access_denied"), Some(WebNoticeKind::Denied));
        assert_eq!(
            parse_web_notice_sentinel("chat_web_access_unavailable"),
            Some(WebNoticeKind::Unavailable)
        );
        assert_eq!(web_notice_sentinel(WebNoticeKind::Denied), WEB_ACCESS_DENIED_SENTINEL);
    }

    #[test]
    fn spawn_sub_agent_label_resolves_to_catalog_id() {
        assert_eq!(resolve_display_action_name("Spawn sub-agent"), "agent.spawn");
        assert_eq!(
            resolve_display_action_name("Déléguer à un sous-agent"),
            "agent.spawn"
        );
    }

    #[test]
    fn interactive_auth_goals_detected() {
        assert!(goal_needs_interactive_auth(
            "Créer un compte sur agent-arena.com pour moi"
        ));
        assert!(!goal_needs_interactive_auth("Résume les notes du projet"));
    }

    #[test]
    fn cap_hint_suppressed_after_denied_web_mode() {
        let mut state = TaskCapabilityState {
            web_access: WebAccessMode::DeniedForTask,
            ..Default::default()
        };
        assert!(!should_append_cap_request_hint(
            &state,
            "web.search",
            "permissiondenied"
        ));
        let out = follow_up_missing_cap(&mut state, "web.search", "PermissionDenied", "net.connect:*");
        assert!(!out.contains("cap.request"));
        assert!(out.contains("indisponible"));
    }

    #[test]
    fn cap_request_not_repeated_for_same_cap() {
        let mut state = TaskCapabilityState::default();
        assert!(cap_request_allowed(&state, "net.connect:*"));
        record_cap_request(&mut state, "net.connect:*");
        assert!(!cap_request_allowed(&state, "net.connect:*"));
    }

    #[test]
    fn unknown_action_message_omits_invented_capability_list() {
        let msg = unknown_catalog_action_message("Spawn sub-agent");
        assert!(!msg.contains("net.connect"));
        assert!(!msg.contains("tool.invoke"));
    }

    #[test]
    fn capability_user_ask_blocked_after_first_prompt() {
        let mut state = TaskCapabilityState::default();
        let q = "Voulez-vous activer l'accès web ?";
        assert!(block_capability_user_ask(&state, q).is_none());
        note_capability_user_ask(&mut state, q);
        assert!(block_capability_user_ask(&state, q).is_some());
    }

    #[test]
    fn take_web_notice_only_once() {
        let mut state = TaskCapabilityState {
            web_access: WebAccessMode::DeniedForTask,
            ..Default::default()
        };
        assert_eq!(take_web_notice_kind(&mut state), Some(WebNoticeKind::Denied));
        assert_eq!(take_web_notice_kind(&mut state), None);
    }
}
