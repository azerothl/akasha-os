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

/// Extrait une action depuis la sortie modèle.
pub fn parse_action(text: &str) -> Option<AgentAction> {
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
}
