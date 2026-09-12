//! Parsing des actions agent (JSON structuré, DSML/XML tool_call, fallback TOOL:).

use serde::{Deserialize, Serialize};

/// Sentinel `fail_reason` / thread content key — UI maps to localized copy.
pub const THREAD_FAIL_COULD_NOT_ACT: &str = "agent_could_not_act";
/// Prompt/context overflow after compaction retries — UI maps to localized copy.
pub const THREAD_FAIL_COULD_NOT_CONTINUE: &str = "agent_could_not_continue";

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
    if actions.is_empty() {
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

/// Retire balises DSML / `<tool_call>` sans compacter les sauts de ligne (prose).
pub fn strip_tool_markup_tags(text: &str) -> String {
    let mut out = text.to_string();
    for (open, close) in TOOL_MARKUP_WRAPPERS {
        out = remove_empty_wrapper_tags(&out, open, close);
        out = remove_all_wrapper_tags(&out, open, close);
    }
    out = remove_tag_blocks(&out, "tool_call");
    out = remove_tag_blocks(&out, "function_call");
    // Native DSML emitted by some local models uses asymmetric delimiters
    // (`<|tool_call>…<tool_call|>`) and channel wrappers. If these markers are
    // kept in assistant memory, the next inference tends to echo them forever
    // instead of producing the next Canvas action.
    for (open, close) in NATIVE_TOOL_MARKUP_WRAPPERS {
        out = remove_empty_wrapper_tags(&out, open, close);
        out = remove_all_wrapper_tags(&out, open, close);
    }
    for marker in NATIVE_CONTROL_MARKERS {
        out = remove_literal_marker(&out, marker);
    }
    out.trim().to_string()
}

/// Retire balises DSML / `<tool_call>` pour mémoire ou affichage utilisateur.
pub fn strip_tool_markup(text: &str) -> String {
    collapse_blank_lines(&strip_tool_markup_tags(text))
}

/// Sortie modèle avec marqueurs d'appel d'outil (DSML, XML tool_call).
pub fn looks_like_tool_markup(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("tool_call")
        || lower.contains("function_call")
        || lower.contains("dsml")
        || lower.contains("｜dsml｜")
        || lower.contains("<|dsml|")
        || lower.contains("<function_calls>")
}

const TOOL_MARKUP_WRAPPERS: &[(&str, &str)] = &[
    ("<｜dsml｜tool_call>", "</｜dsml｜tool_call>"),
    ("<|dsml|tool_call>", "</|dsml|tool_call>"),
    ("<｜dsml｜function_calls>", "</｜dsml｜function_calls>"),
    ("<|dsml|function_calls>", "</|dsml|function_calls>"),
    ("<function_calls>", "</function_calls>"),
];

const NATIVE_TOOL_MARKUP_WRAPPERS: &[(&str, &str)] = &[
    ("<|tool_call>", "<tool_call|>"),
    ("<|function_call>", "<function_call|>"),
    ("<|channel>", "<channel|>"),
];

const NATIVE_CONTROL_MARKERS: &[&str] = &[
    "<|tool_call>",
    "<tool_call|>",
    "<|function_call>",
    "<function_call|>",
    "<|channel>",
    "<channel|>",
];

fn remove_literal_marker(text: &str, marker: &str) -> String {
    let mut out = text.to_string();
    loop {
        let lower = out.to_ascii_lowercase();
        let marker_lower = marker.to_ascii_lowercase();
        let Some(start) = lower.find(&marker_lower) else {
            break;
        };
        out = format!("{}{}", &out[..start], &out[start + marker.len()..]);
    }
    out
}

fn parse_tool_markup_actions(text: &str) -> Vec<AgentAction> {
    let stripped = strip_tool_markup(text);
    let payloads = extract_tag_block_payloads(text, "tool_call");
    let mut actions: Vec<AgentAction> = payloads
        .iter()
        .filter_map(|p| action_from_openai_tool_json(p))
        .collect();
    if actions.is_empty() {
        actions = extract_tag_block_payloads(text, "function_call")
            .iter()
            .filter_map(|p| action_from_openai_tool_json(p))
            .collect();
    }
    // Some local instruct models emit their native DSML dialect instead of
    // JSON/XML: `call:canvas.get{session_id: "..."}`. Treat it as a tool call
    // only when a balanced argument object follows the action token.
    if actions.is_empty() {
        actions = parse_colon_call_actions(text);
    }
    if actions.is_empty() && !stripped.trim().is_empty() {
        if let Some(a) = parse_action_clean(&stripped) {
            actions.push(a);
        }
    }
    actions
}

fn parse_colon_call_actions(text: &str) -> Vec<AgentAction> {
    let lower = text.to_ascii_lowercase();
    let mut actions = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = lower[cursor..].find("call:") {
        let marker = cursor + relative;
        let action_start = marker + "call:".len();
        let action_end = text[action_start..]
            .char_indices()
            .take_while(|(_, ch)| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '-'))
            .last()
            .map(|(index, ch)| action_start + index + ch.len_utf8())
            .unwrap_or(action_start);
        if action_end == action_start {
            cursor = action_start;
            continue;
        }
        let object_start = text[action_end..]
            .char_indices()
            .find(|(_, ch)| !ch.is_ascii_whitespace())
            .map(|(index, _)| action_end + index);
        let Some(object_start) = object_start.filter(|index| text[*index..].starts_with('{'))
        else {
            cursor = action_end;
            continue;
        };
        let Some(object_end) = find_balanced_object_end(text, object_start) else {
            break;
        };
        let raw_args = &text[object_start..=object_end];
        if let Some(args) = parse_loose_json_object(raw_args) {
            actions.push(AgentAction {
                thought: String::new(),
                action: text[action_start..action_end].to_string(),
                args,
            });
        }
        cursor = object_end + 1;
    }
    actions
}

fn find_balanced_object_end(text: &str, start: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, ch) in text[start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_loose_json_object(raw: &str) -> Option<serde_json::Value> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
        return if value.is_object() { Some(value) } else { None };
    }
    // The native dialect leaves object keys unquoted while keeping values in
    // JSON syntax. Quote only identifier-like tokens followed by `:` and
    // leave strings untouched, including escaped quotes.
    let mut normalized = String::with_capacity(raw.len() + 8);
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let start = i;
            i += 1;
            let mut escaped = false;
            while i < bytes.len() {
                if escaped {
                    escaped = false;
                } else if bytes[i] == b'\\' {
                    escaped = true;
                } else if bytes[i] == b'"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            normalized.push_str(&raw[start..i]);
            continue;
        }
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            let start = i;
            i += 1;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'-' | b'.'))
            {
                i += 1;
            }
            let token_end = i;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b':' {
                normalized.push('"');
                normalized.push_str(&raw[start..token_end]);
                normalized.push('"');
                normalized.push_str(&raw[token_end..i]);
                normalized.push(':');
                i += 1;
                continue;
            }
            normalized.push_str(&raw[start..token_end]);
            continue;
        }
        normalized.push(bytes[i] as char);
        i += 1;
    }
    let value = serde_json::from_str::<serde_json::Value>(&normalized).ok()?;
    if value.is_object() {
        Some(value)
    } else {
        None
    }
}

fn action_from_openai_tool_json(json: &str) -> Option<AgentAction> {
    let value: serde_json::Value = serde_json::from_str(json.trim()).ok()?;
    if let Some(obj) = value.get("function").and_then(|v| v.as_object()) {
        let action = obj.get("name")?.as_str()?.trim().to_string();
        if action.is_empty() {
            return None;
        }
        let args = obj
            .get("arguments")
            .cloned()
            .unwrap_or(serde_json::json!({}));
        let args = if args.is_object() {
            args
        } else if args.is_null() {
            serde_json::json!({})
        } else {
            serde_json::json!({ "value": args })
        };
        return Some(AgentAction {
            thought: String::new(),
            action,
            args,
        });
    }
    let action = value.get("name")?.as_str()?.trim().to_string();
    if action.is_empty() {
        return None;
    }
    let args = value
        .get("arguments")
        .or_else(|| value.get("args"))
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let args = if args.is_object() {
        args
    } else if args.is_null() {
        serde_json::json!({})
    } else {
        serde_json::json!({ "value": args })
    };
    Some(AgentAction {
        thought: String::new(),
        action,
        args,
    })
}

fn extract_tag_block_payloads(text: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start_rel) = find_ignore_ascii_case(rest, &open) {
        let start = start_rel + open.len();
        let tail = &rest[start..];
        let Some(end_rel) = find_ignore_ascii_case(tail, &close) else {
            break;
        };
        let payload = tail[..end_rel].trim();
        if !payload.is_empty() {
            out.push(payload.to_string());
        }
        rest = &tail[end_rel + close.len()..];
    }
    out
}

fn remove_empty_wrapper_tags(text: &str, open: &str, close: &str) -> String {
    let mut out = text.to_string();
    loop {
        let lower = out.to_ascii_lowercase();
        let open_l = open.to_ascii_lowercase();
        let close_l = close.to_ascii_lowercase();
        let Some(start) = lower.find(&open_l) else {
            break;
        };
        let after = start + open.len();
        let tail_lower = out[after..].to_ascii_lowercase();
        let Some(end_rel) = tail_lower.find(&close_l) else {
            break;
        };
        let inner = out[after..after + end_rel].trim();
        if !inner.is_empty() {
            break;
        }
        let end = after + end_rel + close.len();
        out = format!("{}{}", &out[..start], &out[end..]);
    }
    out
}

fn remove_all_wrapper_tags(text: &str, open: &str, close: &str) -> String {
    let mut out = text.to_string();
    loop {
        let lower = out.to_ascii_lowercase();
        let open_l = open.to_ascii_lowercase();
        let close_l = close.to_ascii_lowercase();
        let Some(start) = lower.find(&open_l) else {
            break;
        };
        let after = start + open.len();
        let tail_lower = out[after..].to_ascii_lowercase();
        let Some(end_rel) = tail_lower.find(&close_l) else {
            out = format!("{}{}", &out[..start], &out[after..]);
            break;
        };
        let end = after + end_rel + close.len();
        out = format!("{}{}", &out[..start], &out[end..]);
    }
    out
}

fn remove_tag_blocks(text: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = text.to_string();
    loop {
        let lower = out.to_ascii_lowercase();
        let open_l = open.to_ascii_lowercase();
        let close_l = close.to_ascii_lowercase();
        let Some(start) = lower.find(&open_l) else {
            break;
        };
        let after = start + open.len();
        let tail_lower = out[after..].to_ascii_lowercase();
        let Some(end_rel) = tail_lower.find(&close_l) else {
            out = format!("{}{}", &out[..start], &out[after..]);
            break;
        };
        let end = after + end_rel + close.len();
        out = format!("{}{}", &out[..start], &out[end..]);
    }
    out
}

fn collapse_blank_lines(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn parse_action_clean(text: &str) -> Option<AgentAction> {
    // 1. Bloc ```json ... ```
    if let Some(json) = extract_json_fence(text) {
        if let Some(a) = action_from_json_str(json) {
            return Some(a);
        }
    }
    // 2. Premier objet JSON dans le texte
    if let Some(obj) = extract_first_json_object(text) {
        if let Some(a) = action_from_json_str(&obj) {
            return Some(a);
        }
    }
    // 3. Compat TOOL:
    if let Some((tool, args)) = parse_tool_line(text) {
        return Some(AgentAction {
            thought: String::new(),
            action: tool,
            args,
        });
    }
    None
}

/// Clés réservées du wrapper d'action — tout le reste peut être un arg aplati.
const ACTION_WRAPPER_KEYS: &[&str] = &[
    "action",
    "args",
    "action_input",
    "thought",
    "thinking",
    "reasoning",
];

/// Parse un objet JSON en `AgentAction`, en remontant les champs frères dans `args`.
///
/// Les modèles produisent souvent (guidés par les exemples courts du prompt) :
/// `{"action":"agent.spawn","brief":"…","tools":["canvas.get"]}`
/// au lieu de `{"action":"agent.spawn","args":{"brief":"…"}}`. Sans lift,
/// `args` reste vide → spawn refusé (« brief sous-agent vide ») alors que
/// l'UI affiche quand même « Sous-agent spawn ».
fn action_from_json_str(json: &str) -> Option<AgentAction> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    coerce_action_from_value(value)
}

fn coerce_action_from_value(value: serde_json::Value) -> Option<AgentAction> {
    let obj = value.as_object()?;
    let action = obj.get("action")?.as_str()?.trim().to_string();
    if action.is_empty() {
        return None;
    }
    let thought = obj
        .get("thought")
        .or_else(|| obj.get("thinking"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let action_input = obj
        .get("action_input")
        .and_then(|value| value.as_object())
        .cloned();
    let mut args = match obj.get("args") {
        Some(a) if a.is_object() => a.clone(),
        Some(a) if !a.is_null() => serde_json::json!({ "value": a.clone() }),
        _ => action_input
            .clone()
            .map(serde_json::Value::Object)
            .unwrap_or_else(|| serde_json::json!({})),
    };

    if let Some(map) = args.as_object_mut() {
        if let Some(input) = action_input {
            for (k, v) in input {
                map.entry(k).or_insert(v);
            }
        }
        for (k, v) in obj {
            if ACTION_WRAPPER_KEYS.contains(&k.as_str()) {
                continue;
            }
            map.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }

    Some(AgentAction {
        thought,
        action,
        args,
    })
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let t: String = s.chars().take(max).collect();
    format!("{t}…")
}

fn extract_json_fence(text: &str) -> Option<&str> {
    let start = text.find("```json")?;
    let after = &text[start + 7..];
    let end = after.find("```")?;
    Some(after[..end].trim())
}

fn extract_first_json_object(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    for (i, ch) in text[start..].char_indices() {
        if in_str {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(text[start..start + i + 1].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

pub fn parse_tool_line(text: &str) -> Option<(String, serde_json::Value)> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("TOOL:") {
            let rest = rest.trim();
            let (tool, args_str) = match rest.find(char::is_whitespace) {
                Some(i) => (rest[..i].to_string(), rest[i..].trim()),
                None => (rest.to_string(), "{}"),
            };
            let args = serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
            return Some((tool, args));
        }
    }
    None
}

/// Résumé affiché à l'utilisateur quand l'agent appelle `goal.complete`.
/// Si `summary` est vide ou générique, on réutilise le raisonnement ou le dernier résultat d'outil.
pub fn resolve_goal_complete_summary(
    args: &serde_json::Value,
    thought: &str,
    prior_tool_result: &str,
) -> String {
    if let Some(s) = args
        .get("summary")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty() && !is_placeholder_complete_summary(s))
    {
        return s.to_string();
    }
    let thought = thought.trim();
    let prior = prior_tool_result.trim();
    if !thought.is_empty() && !is_internal_complete_thought(thought) {
        return thought.to_string();
    }
    if !prior.is_empty() {
        return prior.to_string();
    }
    if !thought.is_empty() {
        return thought.to_string();
    }
    "terminé".to_string()
}

fn is_internal_complete_thought(s: &str) -> bool {
    let lower = s.trim().to_ascii_lowercase();
    lower.contains("aucune action supplémentaire")
        || lower.contains("aucune action supplementaire")
        || lower.contains("no further action")
        || lower.contains("nothing more to do")
        || lower.contains("no additional action")
}

fn is_placeholder_complete_summary(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "terminé" | "termine" | "done" | "ok" | "fini" | "complete" | "completed"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_json_action() {
        let text = r#"Je vais créer une note.
```json
{"thought":"ok","action":"notes.create","args":{"title":"x","body":"y"}}
```
"#;
        let a = parse_action(text).unwrap();
        assert_eq!(a.action, "notes.create");
        assert_eq!(a.args["title"], "x");
    }

    #[test]
    fn parse_tool_compat() {
        let a = parse_action(r#"TOOL: notes.list {}"#).unwrap();
        assert_eq!(a.action, "notes.list");
    }

    #[test]
    fn parse_raw_object() {
        let a = parse_action(r#"{"action":"goal.complete","args":{"summary":"done"}}"#).unwrap();
        assert_eq!(a.action, "goal.complete");
    }

    #[test]
    fn parse_flat_agent_spawn_lifts_brief_and_tools() {
        // Forme courante : args aplatis au top-level (pas sous "args").
        let text = r#"{
  "action": "agent.spawn",
  "brief": "Identifier le carré bleu dans le canvas et obtenir ses coordonnées.",
  "tools": ["canvas.get"],
  "skills": []
}"#;
        let a = parse_action(text).unwrap();
        assert_eq!(a.action, "agent.spawn");
        assert_eq!(
            a.args["brief"],
            "Identifier le carré bleu dans le canvas et obtenir ses coordonnées."
        );
        assert_eq!(a.args["tools"], serde_json::json!(["canvas.get"]));
        assert_eq!(a.args["skills"], serde_json::json!([]));
    }

    #[test]
    fn parse_action_input_wrapper_as_tool_args() {
        let text = r#"{
  "action": "canvas.path",
  "action_input": {
    "session_id": "sess-1",
    "points": [{"x": 0.1, "y": 0.2}],
    "fill": true
  }
}"#;
        let a = parse_action(text).unwrap();
        assert_eq!(a.action, "canvas.path");
        assert_eq!(a.args["session_id"], "sess-1");
        assert_eq!(a.args["points"][0]["x"], 0.1);
        assert_eq!(a.args["fill"], true);
        assert!(a.args.get("action_input").is_none());
    }

    #[test]
    fn nested_args_take_precedence_over_action_input_wrapper() {
        let text = r##"{
  "action": "canvas.set_style",
  "args": {"color": "#111111"},
  "action_input": {"color": "#222222", "width": 0.03}
}"##;
        let a = parse_action(text).unwrap();
        assert_eq!(a.args["color"], "#111111");
        assert_eq!(a.args["width"], 0.03);
    }

    #[test]
    fn parse_nested_args_wins_over_flat_sibling() {
        let text = r#"{
  "action": "agent.spawn",
  "brief": "flat-ignored-when-nested",
  "args": {"brief": "nested-wins", "tools": ["canvas.get"]}
}"#;
        let a = parse_action(text).unwrap();
        assert_eq!(a.args["brief"], "nested-wins");
        assert_eq!(a.args["tools"], serde_json::json!(["canvas.get"]));
    }

    #[test]
    fn strip_qwen_think_then_json() {
        let text = r#"<think>
Thinking Process:
1. Analyze…
</think>
{"thought":"ok","action":"web.search","args":{"query":"agentic OS"}}"#;
        let a = parse_action(text).unwrap();
        assert_eq!(a.action, "web.search");
        assert!(a.thought.contains("ok") || a.thought.contains("Analyze"));
        let clean = strip_reasoning(text);
        assert!(!clean.contains("<think>"));
        assert!(clean.contains("web.search"));
    }

    #[test]
    fn strip_unclosed_think() {
        let text = "<think>\nThinking Process:\n1. Analyze the Request:\n";
        let (r, clean) = split_reasoning(text);
        assert!(clean.is_empty());
        assert!(r.contains("Analyze"));
        assert!(parse_action(text).is_none());
    }

    #[test]
    fn parse_dsml_xml_canvas_rect_and_line() {
        let text = concat!(
            "<｜DSML｜tool_call>\n\n",
            "<tool_call> {\"name\": \"canvas.rect\", \"arguments\":{\"x\": 0.2, \"y\": 0.3, \"width\": 0.5, \"height\": 0.4, \"color\": \"#2E8B57\"}} </tool_call>\n",
            "<tool_call> {\"name\": \"canvas.line\", \"arguments\":{\"x1\": 0.1, \"y1\": 0.2, \"x2\": 0.5, \"y2\": 0.6}} </tool_call>"
        );
        let actions = parse_actions(text);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].action, "canvas.rect");
        assert_eq!(actions[0].args["x"], 0.2);
        assert_eq!(actions[0].args["color"], "#2E8B57");
        assert_eq!(actions[1].action, "canvas.line");
        assert_eq!(actions[1].args["x1"], 0.1);
    }

    #[test]
    fn parse_native_dsml_colon_call_canvas_get() {
        let text = r#"<|channel>thought
<channel|><|tool_call>call:canvas.get{session_id: "sess-1"}<tool_call|>"#;
        let actions = parse_actions(text);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action, "canvas.get");
        assert_eq!(actions[0].args["session_id"], "sess-1");
    }

    #[test]
    fn empty_dsml_wrapper_is_not_an_action() {
        let text = "<｜DSML｜tool_call>\n\n</｜DSML｜tool_call>";
        assert!(parse_actions(text).is_empty());
        let lone = "<｜DSML｜tool_call>";
        assert!(parse_actions(lone).is_empty());
    }

    #[test]
    fn strip_tool_markup_removes_dsml_and_tool_call() {
        let raw = r#"<｜DSML｜tool_call><tool_call>{"name":"canvas.rect","arguments":{}}</tool_call></｜DSML｜tool_call>"#;
        let stripped = strip_tool_markup(raw);
        assert!(!stripped.contains("tool_call"));
        assert!(!stripped.contains("DSML"));
        assert!(!stripped.contains("canvas.rect"));
    }

    #[test]
    fn strip_tool_markup_removes_native_dsml_from_assistant_memory() {
        let raw = r#"<|channel>thought
<channel|><|tool_call>call:canvas.get{session_id: "sess-1"}<tool_call|>"#;
        let stripped = strip_tool_markup(raw);
        assert!(stripped.is_empty());
    }

    #[test]
    fn resolve_goal_complete_summary_prefers_explicit_summary() {
        let args = serde_json::json!({"summary": "3 ports USB trouvés"});
        assert_eq!(
            resolve_goal_complete_summary(&args, "pensée", "outil"),
            "3 ports USB trouvés"
        );
    }

    #[test]
    fn resolve_goal_complete_summary_falls_back_to_thought() {
        let args = serde_json::json!({});
        assert_eq!(
            resolve_goal_complete_summary(&args, "COM3 : Arduino Uno", "raw enumerate output"),
            "COM3 : Arduino Uno"
        );
    }

    #[test]
    fn resolve_goal_complete_summary_falls_back_to_prior_tool_result() {
        let args = serde_json::json!({"summary": ""});
        assert_eq!(
            resolve_goal_complete_summary(&args, "", "COM3 (win:Serial:...)"),
            "COM3 (win:Serial:...)"
        );
    }

    #[test]
    fn resolve_goal_complete_summary_rejects_placeholder_only() {
        let args = serde_json::json!({"summary": "terminé"});
        assert_eq!(
            resolve_goal_complete_summary(&args, "liste USB", ""),
            "liste USB"
        );
    }

    #[test]
    fn resolve_goal_complete_summary_prefers_prior_over_internal_thought() {
        let args = serde_json::json!({"summary": ""});
        assert_eq!(
            resolve_goal_complete_summary(
                &args,
                "Aucune action supplémentaire requise.",
                "COM3 (win:Serial:COM3)",
            ),
            "COM3 (win:Serial:COM3)"
        );
    }

    #[test]
    fn thread_fail_sentinel_is_stable() {
        assert_eq!(THREAD_FAIL_COULD_NOT_ACT, "agent_could_not_act");
    }

    #[test]
    fn parse_embedded_action_question_from_fenced_json() {
        let q = r##"```json
{
"action": "canvas.set_style",
"args": {
"color": "#8D6E63"
}
}
```"##;
        let a = parse_embedded_action_question(q).unwrap();
        assert_eq!(a.action, "canvas.set_style");
        assert_eq!(a.args["color"], "#8D6E63");
    }
}
