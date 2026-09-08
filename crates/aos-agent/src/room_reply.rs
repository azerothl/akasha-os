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
    if has_substantive_plan_args(obj_map.get("args")) {
        return None;
    }
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
        let keep = serde_json::from_str::<serde_json::Value>(&obj)
            .ok()
            .is_some_and(|value| !is_internal_agent_envelope(&value));
        if keep {
            break;
        }
        if let Some(start) = out.find(&obj) {
            out = format!("{}{}", &out[..start], &out[start + obj.len()..]);
            out = out.trim().to_string();
            continue;
        }
        break;
    }
    collapse_blank_lines(&sanitize_visible_chars(&strip_salon_transcript_prefix(&out)))
}

fn has_substantive_plan_args(args: Option<&serde_json::Value>) -> bool {
    match args {
        Some(serde_json::Value::Object(map)) => !map.is_empty(),
        Some(serde_json::Value::Array(arr)) => !arr.is_empty(),
        _ => false,
    }
}

fn is_internal_agent_envelope(value: &serde_json::Value) -> bool {
    let obj = match value.as_object() {
        Some(obj) => obj,
        None => return false,
    };
    let has_thought = obj.contains_key("thought")
        || obj.contains_key("thinking")
        || obj.contains_key("reasoning");
    if !has_thought {
        return false;
    }
    !has_substantive_plan_args(obj.get("args"))
}

/// Extract the first balanced `{…}` JSON object substring from `text`.
pub fn extract_first_json_object(text: &str) -> Option<String> {
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

/// Remove echoed transcript attribution — the bubble header already names the speaker.
pub fn strip_salon_transcript_prefix(text: &str) -> String {
    let mut lines_out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("[Salon — tour de ") {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("[Salon — ") {
            if let Some(idx) = rest.find(']') {
                let after = rest[idx + 1..].trim_start();
                if !after.is_empty() {
                    lines_out.push(after.to_string());
                }
                continue;
            }
        }
        lines_out.push(line.to_string());
    }
    lines_out.join("\n").trim().to_string()
}

/// Drop replacement glyphs and control chars that paint as □ in the UI.
pub fn sanitize_visible_chars(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch == '\u{FFFD}' || is_private_use_char(ch) {
            continue;
        }
        if ch.is_control() && ch != '\n' && ch != '\t' && ch != '\r' {
            continue;
        }
        out.push(ch);
    }
    out
}

fn is_private_use_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{E000}'..='\u{F8FF}' | '\u{F0000}'..='\u{FFFFD}' | '\u{100000}'..='\u{10FFFD}'
    )
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
    fn strips_salon_transcript_prefix_from_visible_reply() {
        let raw = "[Salon — Researcher] @supervisor, voici le protocole.";
        let (visible, _) = split_room_reply(raw);
        assert_eq!(visible, "@supervisor, voici le protocole.");
        assert!(!visible.contains("[Salon —"));
    }

    #[test]
    fn strips_replacement_glyphs_from_visible_reply() {
        let raw = "Pour un rapport technique ou\u{FFFD}\u{FFFD}\u{FFFD} fin.";
        let (visible, _) = split_room_reply(raw);
        assert_eq!(visible, "Pour un rapport technique ou fin.");
        assert!(!visible.contains('\u{FFFD}'));
    }

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
    fn plan_json_with_args_stays_in_visible_reply() {
        let raw = r#"Voici le plan :
json {"thought":"Planification des phases","action":"","args":{"nodes":[{"id":"phase-1","status":"pending"}]}}
Suite."#;
        let (visible, thinking) = split_room_reply(raw);
        assert!(visible.contains("\"nodes\""));
        assert!(visible.contains("\"thought\""));
        assert!(thinking.is_none());
    }

    #[test]
    fn utf8_accents_preserved_in_visible_and_thinking() {
        let raw = "La mémoire garde la réponse système déjà déployée.";
        let (visible, thinking) = split_room_reply(raw);
        assert_eq!(visible, raw);
        assert!(thinking.is_none());
    }
}
