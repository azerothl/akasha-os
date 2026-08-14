//! Parsing des actions agent (JSON structuré + fallback TOOL:).

use serde::{Deserialize, Serialize};

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
    loop {
        let Some(start) = find_ignore_ascii_case(&rest, "<think>") else {
            break;
        };
        let open_len = "<think>".len();
        let after = start + open_len;
        if let Some(rel) = find_ignore_ascii_case(&rest[after..], "</think>") {
            let close_at = after + rel;
            let body = rest[after..close_at].trim();
            if !body.is_empty() {
                if !reasoning.is_empty() {
                    reasoning.push_str("\n");
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
                    reasoning.push_str("\n");
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
    hay_l.find(&needle_l).map(|i| {
        // Aligner sur la même frontière d'octet (ASCII tags)
        i
    })
}

/// Extrait une action depuis la sortie modèle.
pub fn parse_action(text: &str) -> Option<AgentAction> {
    let (reasoning, clean) = split_reasoning(text);
    let mut action = parse_action_clean(&clean)?;
    if action.thought.is_empty() && !reasoning.is_empty() {
        action.thought = truncate_chars(&reasoning, 400);
    }
    Some(action)
}

fn parse_action_clean(text: &str) -> Option<AgentAction> {
    // 1. Bloc ```json ... ```
    if let Some(json) = extract_json_fence(text) {
        if let Ok(a) = serde_json::from_str::<AgentAction>(json) {
            if !a.action.is_empty() {
                return Some(a);
            }
        }
    }
    // 2. Premier objet JSON dans le texte
    if let Some(obj) = extract_first_json_object(text) {
        if let Ok(a) = serde_json::from_str::<AgentAction>(&obj) {
            if !a.action.is_empty() {
                return Some(a);
            }
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
}
