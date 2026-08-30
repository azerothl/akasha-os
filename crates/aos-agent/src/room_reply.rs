//! Salon reply shaping: visible bubble text vs hidden thinking.

use crate::actions::{split_reasoning, strip_tool_markup_tags};

/// Split model output into visible salon reply and optional thinking (hidden by default).
pub fn split_room_reply(raw: &str) -> (String, Option<String>) {
    let (tag_reasoning, mut work) = split_reasoning(raw);
    let mut thinking = Vec::new();
    if !tag_reasoning.is_empty() {
        thinking.push(tag_reasoning);
    }

    work = strip_speaker_label_prefix(&work);

    while let Some((thought, rest)) = take_thought_json_object(&work) {
        if !thought.is_empty() {
            thinking.push(thought);
        }
        work = rest;
    }

    let visible = visible_prose(&work);
    let thinking_text = if thinking.is_empty() {
        None
    } else {
        Some(thinking.join("\n\n"))
    };
    (visible, thinking_text)
}

fn strip_speaker_label_prefix(text: &str) -> String {
    let trimmed = text.trim_start();
    if !trimmed.starts_with('(') {
        return text.to_string();
    }
    let Some(close_rel) = trimmed[1..].find(')') else {
        return text.to_string();
    };
    let label = trimmed[1..1 + close_rel].trim();
    let after = trimmed[1 + close_rel + 1..].trim_start();
    if label.is_empty() || after.is_empty() {
        return text.to_string();
    }
    // Transcript labels from `format_transcript_messages` — not prose like "(Note: …)".
    if label.contains(':') || label.len() > 48 {
        return text.to_string();
    }
    after.to_string()
}

fn take_thought_json_object(text: &str) -> Option<(String, String)> {
    let start = text.find('{')?;
    let obj = extract_first_json_object(&text[start..])?;
    let value: serde_json::Value = serde_json::from_str(&obj).ok()?;
    let obj_map = value.as_object()?;
    let thought = obj_map
        .get("thought")
        .or_else(|| obj_map.get("thinking"))
        .or_else(|| obj_map.get("reasoning"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if thought.is_empty() {
        return None;
    }
    let obj_start = start;
    let obj_end = start + obj.len();
    let mut rest = String::new();
    rest.push_str(text[..obj_start].trim_end());
    let tail = text[obj_end..].trim_start();
    if !rest.is_empty() && !tail.is_empty() {
        rest.push_str("\n\n");
    }
    rest.push_str(tail);
    Some((thought, rest.trim().to_string()))
}

fn visible_prose(text: &str) -> String {
    let mut out = strip_tool_markup_tags(text);
    while let Some(obj) = extract_first_json_object(&out) {
        if serde_json::from_str::<serde_json::Value>(&obj)
            .ok()
            .and_then(|v| v.as_object().cloned())
            .is_some_and(|o| o.contains_key("thought") || o.contains_key("action"))
        {
            if let Some(start) = out.find(&obj) {
                out = format!("{}{}", &out[..start], &out[start + obj.len()..]);
                out = out.trim().to_string();
                continue;
            }
        }
        break;
    }
    collapse_blank_lines(&out)
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

fn collapse_blank_lines(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_thought_json_from_visible_reply() {
        let raw = r#"{"thought":"Je consulte ma mémoire et synthétise une réponse."}

Voici ce qu'il faut retenir sur Akasha OS."#;
        let (visible, thinking) = split_room_reply(raw);
        assert!(visible.contains("Akasha OS"));
        assert!(!visible.contains("{\"thought\""));
        assert!(!visible.contains("mémoire"));
        let t = thinking.expect("thinking");
        assert!(t.contains("mémoire"));
    }

    #[test]
    fn redacted_thinking_hidden_from_visible() {
        let raw = "<think>Plan interne</think>\nRéponse finale.";
        let (visible, thinking) = split_room_reply(raw);
        assert_eq!(visible, "Réponse finale.");
        assert_eq!(thinking.as_deref(), Some("Plan interne"));
    }

    #[test]
    fn speaker_label_prefix_stripped_before_json() {
        let raw = r#"(supervisor) {"thought":"plan long"} Réponse courte."#;
        let (visible, thinking) = split_room_reply(raw);
        assert_eq!(visible, "Réponse courte.");
        assert_eq!(thinking.as_deref(), Some("plan long"));
    }

    #[test]
    fn transcript_speaker_label_stripped_from_visible_prose() {
        let raw = "(Researcher) @supervisor, voici mon analyse distincte.";
        let (visible, thinking) = split_room_reply(raw);
        assert_eq!(visible, "@supervisor, voici mon analyse distincte.");
        assert!(thinking.is_none());
    }

    #[test]
    fn prose_parenthetical_with_colon_not_stripped() {
        let raw = "(Note: important) garde ce détail.";
        let (visible, _) = split_room_reply(raw);
        assert_eq!(visible, raw);
    }

    #[test]
    fn utf8_accents_preserved_in_visible_and_thinking() {
        let raw = "La mémoire garde la réponse système déjà déployée.";
        let (visible, thinking) = split_room_reply(raw);
        assert_eq!(visible, raw);
        assert!(thinking.is_none());
    }
}
