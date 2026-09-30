//! Parsing des actions agent (JSON structuré, DSML/XML tool_call, fallback TOOL:).

use serde::{Deserialize, Serialize};

/// Sentinel `fail_reason` / thread content key — UI maps to localized copy.
pub const THREAD_FAIL_COULD_NOT_ACT: &str = "agent_could_not_act";
/// Prompt/context overflow after compaction retries — UI maps to localized copy.
pub const THREAD_FAIL_COULD_NOT_CONTINUE: &str = "agent_could_not_continue";
/// Live agent killed when aos-agentd reloads — UI maps to localized copy.
pub const THREAD_FAIL_STOPPED_ON_RESTART: &str = "agent_stopped_on_restart";
/// Legacy persisted value before `THREAD_FAIL_STOPPED_ON_RESTART` (FR-only).
pub const LEGACY_FAIL_STOPPED_ON_RESTART: &str = "arrêté au redémarrage";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAction {
    #[serde(default)]
    pub thought: String,
    pub action: String,
    #[serde(default)]
    pub args: serde_json::Value,
}

/// Retire le raisonnement Qwen/DeepSeek (`<think>…</think>`) et renvoie
/// `(raisonnement_extrait, texte_utile)`.
pub fn split_reasoning(text: &str) -> (String, String) {
    let mut rest = text.to_string();
    let mut reasoning = String::new();
    while let Some(start) = find_ignore_ascii_case(&rest, "<think>") {
        let open_len = "<think>".len();
        let after = start + open_len;
        if let Some(rel) = find_ignore_ascii_case(&rest[after..], "</think>") {
            let close_at = after + rel;
            let body = rest[after..close_at].trim();
            if !body.is_empty() {
                if !reasoning.is_empty() {
                    reasoning.push('\n');
                }
                reasoning.push_str(body);
            }
            let end = close_at + "</think>".len();
            rest = format!("{}{}", &rest[..start], &rest[end..]);
        } else {
            // Bloc non fermé (souvent coupé par max_tokens) → tout jeter
            let body = rest[after..].trim();
            if !body.is_empty() {
                if !reasoning.is_empty() {
                    reasoning.push('\n');
                }
                reasoning.push_str(body);
            }
            rest.truncate(start);
            break;
        }
    }
    // Résidu de fermeture seule (préfill / templates)
    while let Some(idx) = find_ignore_ascii_case(&rest, "</think>") {
        rest = format!("{}{}", &rest[..idx], &rest[idx + "</think>".len()..]);
    }
    (reasoning.trim().to_string(), rest.trim().to_string())
}

/// Texte sans balises de raisonnement (pour mémoire / UI / reflect).
pub fn strip_reasoning(text: &str) -> String {
    split_reasoning(text).1
}

fn find_ignore_ascii_case(hay: &str, needle: &str) -> Option<usize> {
    let hay_l = hay.to_ascii_lowercase();
    let needle_l = needle.to_ascii_lowercase();
    // ASCII case folding preserves the byte boundary used by the original text.
    hay_l.find(&needle_l)
}

/// Extrait une ou plusieurs actions depuis la sortie modèle (ordre conservé).
pub fn parse_actions(text: &str) -> Vec<AgentAction> {
    let (reasoning, clean) = split_reasoning(text);
    let mut actions = parse_tool_markup_actions(&clean);
    // `parse_tool_markup_actions` falls back to a single clean JSON object.
    // Models often dump the whole illust/canvas pipeline as consecutive
    // objects — expand when that yields more actions.
    let consecutive = parse_consecutive_json_actions(&clean);
    if consecutive.len() > actions.len() {
        actions = consecutive;
    } else if actions.is_empty() {
        if let Some(a) = parse_action_clean(&clean) {
            actions.push(a);
        }
    }
    if let Some(first) = actions.first_mut() {
        if first.thought.is_empty() && !reasoning.is_empty() {
            first.thought = truncate_chars(&reasoning, 400);
        }
    }
    actions
}

/// Extrait la première action depuis la sortie modèle.
pub fn parse_action(text: &str) -> Option<AgentAction> {
    parse_actions(text).into_iter().next()
}

/// When the model misuses `user.ask` and puts a tool JSON in the question field, recover it.
pub fn parse_embedded_action_question(text: &str) -> Option<AgentAction> {
    let trimmed = text.trim();
    let unfenced = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```JSON"))
        .or_else(|| trimmed.strip_prefix("```"))
        .map(|body| body.strip_suffix("```").unwrap_or(body).trim())
        .unwrap_or(trimmed);
    if let Some(action) = parse_action(unfenced) {
        if !action.action.is_empty() && action.action != "user.ask" {
            return Some(action);
        }
    }
    let value: serde_json::Value = serde_json::from_str(unfenced).ok()?;
    let action = value.get("action")?.as_str()?.trim();
    if action.is_empty() || action == "user.ask" {
        return None;
    }
    Some(AgentAction {
        thought: value
            .get("thought")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        action: action.to_string(),
        args: value.get("args").cloned().unwrap_or(serde_json::json!({})),
    })
}
