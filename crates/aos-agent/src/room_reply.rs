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
    // Peel fenced agent envelopes before shaping so ```json markers do not linger.
    work = strip_agent_json_fences(&work);

    while let Some((thought, rest)) = take_shaped_agent_envelope(&work) {
        if !thought.is_empty() {
            thinking.push(thought);
        }
        work = rest;
    }

    // Tool envelopes keep `thought` even when action is a real tool — fold it
    // into Reflection without painting the JSON as the live bubble.
    if let Some(obj) = extract_first_json_object(&work) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&obj) {
            if let Some(map) = value.as_object() {
                let action = map
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if !action.is_empty() && action != "user.ask" {
                    let thought = envelope_thought(map);
                    if !thought.is_empty() {
                        thinking.push(thought);
                    }
                }
            }
        }
    }

    let visible = visible_prose(&work);
    let thinking_text = if thinking.is_empty() {
        None
    } else {
        Some(thinking.join("\n\n"))
    };
    (visible, thinking_text)
}

/// Live salon preview: only stable visible prose — never incomplete or tool JSON.
///
/// Returns `None` while the model is still emitting a thought/tool envelope so the
/// UI stays on a status line instead of flashing JSON that then clears.
pub fn stream_visible_partial(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Incomplete `{…}` — wait for a balanced object before shaping.
    if trimmed.starts_with('{') && extract_first_json_object(trimmed).is_none() {
        return None;
    }

    if let Some(obj) = extract_first_json_object(trimmed) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&obj) {
            if let Some(map) = value.as_object() {
                let action = map
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let has_thought = map.contains_key("thought")
                    || map.contains_key("thinking")
                    || map.contains_key("reasoning");
                // Real tool call (or thought-only / ask envelope): don't stream JSON.
                if has_thought || !action.is_empty() {
                    let (visible, _) = split_room_reply(raw);
                    let visible = visible.trim();
                    if visible.is_empty() || visible.starts_with('{') {
                        return None;
                    }
                    return Some(visible.to_string());
                }
            }
        }
    }

    let (visible, _) = split_room_reply(raw);
    let visible = visible.trim();
    if visible.is_empty() {
        None
    } else {
        Some(visible.to_string())
    }
}

/// Merge thinking fragments from multiple tool-loop steps into one Reflection body.
pub fn merge_room_thinking(parts: &[String]) -> Option<String> {
    let mut out = Vec::new();
    for part in parts {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        if out.iter().any(|p: &String| p == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    if out.is_empty() {
        None
    } else {
        Some(out.join("\n\n"))
    }
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

/// Peel one agent-protocol JSON object into thinking + remaining visible text.
///
/// - Empty/`user.ask`-like envelopes with `args.question` / `args.summary` → promote that
///   string as visible prose and stash `thought` as thinking.
/// - Thought-only envelopes (empty or absent args) → strip JSON, keep thought.
/// - Real plan payloads (`args.nodes`, etc.) → leave untouched for display.
fn take_shaped_agent_envelope(text: &str) -> Option<(String, String)> {
    let start = text.find('{')?;
    let obj = extract_first_json_object(&text[start..])
        .or_else(|| try_close_unbalanced_json_object(&text[start..]))?;
    let value: serde_json::Value = serde_json::from_str(&obj).ok()?;
    let obj_map = value.as_object()?;

    if is_plan_display_args(obj_map.get("args")) {
        return None;
    }

    let thought = envelope_thought(obj_map);
    let action = obj_map
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();

    // Only peel display envelopes (empty action / user.ask). Real tools stay intact.
    let promote_ok = action.is_empty() || action == "user.ask";
    let promoted = if promote_ok {
        promote_conversational_args(obj_map.get("args"))
    } else {
        None
    };

    if !promote_ok {
        // Thought-only with a real tool action still belongs to the tool loop.
        return None;
    }

    if thought.is_empty() && promoted.is_none() {
        return None;
    }

    // When we repaired missing braces, consume from `start` through the original
    // unbalanced span (usually to end of string for truncated tool JSON).
    let raw_span = if let Some(balanced) = extract_first_json_object(&text[start..]) {
        balanced.len()
    } else {
        text.len().saturating_sub(start)
    };
    let obj_end = start + raw_span;
    let mut rest = String::new();
    rest.push_str(text[..start].trim_end());
    if let Some(ref body) = promoted {
        if !rest.is_empty() {
            rest.push_str("\n\n");
        }
        rest.push_str(body);
    }
    let tail = if obj_end < text.len() {
        text[obj_end..].trim_start()
    } else {
        ""
    };
    if !rest.is_empty() && !tail.is_empty() {
        rest.push_str("\n\n");
    }
    rest.push_str(tail);
    Some((thought, rest.trim().to_string()))
}

fn envelope_thought(obj: &serde_json::Map<String, serde_json::Value>) -> String {
    obj.get("thought")
        .or_else(|| obj.get("thinking"))
        .or_else(|| obj.get("reasoning"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string()
}

fn promote_conversational_args(args: Option<&serde_json::Value>) -> Option<String> {
    let map = args?.as_object()?;
    if map.is_empty() || is_plan_display_args(args) {
        return None;
    }
    // Prefer ask/summary fields only — avoid promoting tool payloads like notes `content`.
    for key in ["question", "summary", "message"] {
        if let Some(s) = map.get(key).and_then(|v| v.as_str()) {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn visible_prose(text: &str) -> String {
    let mut out = strip_tool_markup_tags(text);
    out = strip_agent_json_fences(&out);
    out = strip_trailing_incomplete_agent_envelope(&out);
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
    collapse_blank_lines(&sanitize_visible_chars(&strip_salon_transcript_prefix(
        &out,
    )))
}

/// Drop ```json fences that wrap agent protocol envelopes (thought/action).
fn strip_agent_json_fences(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    loop {
        let lower = rest.to_ascii_lowercase();
        let Some(fence_at) = lower.find("```") else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..fence_at]);
        let after_ticks = fence_at + 3;
        let (body_start, json_fence) = if lower[after_ticks..].starts_with("json") {
            let mut i = after_ticks + 4;
            while rest.as_bytes().get(i).copied().is_some_and(|b| b == b' ' || b == b'\t') {
                i += 1;
            }
            if rest.as_bytes().get(i).copied() == Some(b'\n')
                || rest.as_bytes().get(i).copied() == Some(b'\r')
            {
                i += 1;
                if rest.as_bytes().get(i - 1).copied() == Some(b'\r')
                    && rest.as_bytes().get(i).copied() == Some(b'\n')
                {
                    i += 1;
                }
            }
            (i, true)
        } else {
            (after_ticks, false)
        };
        let Some(rel_end) = rest[body_start..].find("```") else {
            let body = rest[body_start..].trim_start();
            if json_fence && body_looks_like_agent_envelope(body) {
                out.push_str(&fence_body_replacement(body));
            } else {
                out.push_str(&rest[fence_at..]);
            }
            break;
        };
        let body = rest[body_start..body_start + rel_end].trim();
        let after = &rest[body_start + rel_end + 3..];
        if (json_fence || body_looks_like_agent_envelope(body))
            && body_looks_like_agent_envelope(body)
        {
            out.push_str(&fence_body_replacement(body));
            rest = after.trim_start();
            continue;
        }
        // Keep non-agent fences intact.
        out.push_str(&rest[fence_at..body_start + rel_end + 3]);
        rest = after;
    }
    out
}

fn fence_body_replacement(body: &str) -> String {
    if let Some(obj) = extract_first_json_object(body)
        .or_else(|| try_close_unbalanced_json_object(body))
    {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&obj) {
            if let Some(map) = value.as_object() {
                if let Some(q) = promote_conversational_args(map.get("args")) {
                    return format!("\n\n{q}\n\n");
                }
            }
        }
    }
    "\n\n".into()
}

fn body_looks_like_agent_envelope(body: &str) -> bool {
    let trimmed = body.trim_start();
    if !(trimmed.starts_with('{')
        && (trimmed.contains("\"action\"")
            || trimmed.contains("\"thought\"")
            || trimmed.contains("\"thinking\"")))
    {
        return false;
    }
    if let Some(obj) = extract_first_json_object(trimmed)
        .or_else(|| try_close_unbalanced_json_object(trimmed))
    {
        return serde_json::from_str::<serde_json::Value>(&obj)
            .ok()
            .is_some_and(|v| is_internal_agent_envelope(&v) || action_is_user_ask(&v));
    }
    // Incomplete trailing envelope still must not paint in the bubble.
    true
}

fn action_is_user_ask(value: &serde_json::Value) -> bool {
    value
        .get("action")
        .and_then(|v| v.as_str())
        .is_some_and(|a| a.trim() == "user.ask")
}

/// Models often truncate the closing `}` on long user.ask envelopes — hide the tail.
fn strip_trailing_incomplete_agent_envelope(text: &str) -> String {
    let markers = [
        "{\"thought\"",
        "{\"action\"",
        "{ \"thought\"",
        "{ \"action\"",
    ];
    let mut cut: Option<usize> = None;
    for marker in markers {
        if let Some(idx) = text.rfind(marker) {
            let slice = &text[idx..];
            if extract_first_json_object(slice).is_some() {
                continue;
            }
            if try_close_unbalanced_json_object(slice).is_some() {
                // Repairable — leave for peel/promote; still strip if peel fails later.
                continue;
            }
            if slice.contains("\"action\"") || slice.contains("\"thought\"") {
                cut = Some(match cut {
                    Some(prev) => prev.min(idx),
                    None => idx,
                });
            }
        }
    }
    match cut {
        Some(idx) => text[..idx].trim_end().to_string(),
        None => text.to_string(),
    }
}

/// If a leading `{…` fragment is missing closing braces, try to repair it (depth ≤ 3).
pub fn try_close_unbalanced_json_object(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let fragment = &text[start..];
    if extract_first_json_object(fragment).is_some() {
        return extract_first_json_object(fragment);
    }
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    for ch in fragment.chars() {
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
            '}' => depth -= 1,
            _ => {}
        }
    }
    if depth <= 0 || depth > 3 || in_str {
        return None;
    }
    let mut repaired = fragment.to_string();
    for _ in 0..depth {
        repaired.push('}');
    }
    serde_json::from_str::<serde_json::Value>(&repaired).ok()?;
    Some(repaired)
}

/// True when args look like a salon plan / canvas payload that should stay visible as JSON.
fn is_plan_display_args(args: Option<&serde_json::Value>) -> bool {
    let Some(serde_json::Value::Object(map)) = args else {
        return false;
    };
    const PLAN_KEYS: &[&str] = &[
        "nodes",
        "edges",
        "phases",
        "steps",
        "plan",
        "canvas",
        "widgets",
        "scene",
    ];
    PLAN_KEYS.iter().any(|k| map.contains_key(*k))
}

fn is_internal_agent_envelope(value: &serde_json::Value) -> bool {
    let obj = match value.as_object() {
        Some(obj) => obj,
        None => return false,
    };
    if action_is_user_ask(value) {
        return !is_plan_display_args(obj.get("args"));
    }
    let has_thought = obj.contains_key("thought")
        || obj.contains_key("thinking")
        || obj.contains_key("reasoning");
    if !has_thought {
        return false;
    }
    if is_plan_display_args(obj.get("args")) {
        return false;
    }
    // Thought-only, or conversational args already peeled by take_shaped_agent_envelope.
    let args = obj.get("args");
    match args {
        None => true,
        Some(serde_json::Value::Object(m)) if m.is_empty() => true,
        Some(serde_json::Value::Null) => true,
        _ => promote_conversational_args(args).is_some(),
    }
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
    fn empty_action_question_promotes_to_visible_and_folds_thought() {
        let raw = r#"{
  "thought": "Je synthétise en tant que Critic.",
  "action": "",
  "args": {
    "question": "La recommandation manque de preuve d'adoption.",
    "choices": ["Accepter", "Chercher une preuve"]
  }
}"#;
        let (visible, thinking) = split_room_reply(raw);
        assert_eq!(visible, "La recommandation manque de preuve d'adoption.");
        assert!(!visible.contains("\"thought\""));
        assert!(!visible.contains("\"choices\""));
        assert_eq!(
            thinking.as_deref(),
            Some("Je synthétise en tant que Critic.")
        );
    }

    #[test]
    fn user_ask_action_question_also_promotes_for_display() {
        let raw = r#"{"thought":"besoin d'avis","action":"user.ask","args":{"question":"On continue ?","choices":["oui","non"]}}"#;
        let (visible, thinking) = split_room_reply(raw);
        assert_eq!(visible, "On continue ?");
        assert_eq!(thinking.as_deref(), Some("besoin d'avis"));
    }

    #[test]
    fn summary_args_promoted_when_question_absent() {
        let raw = r#"{"thought":"fin","action":"","args":{"summary":"**Bilan** du tour."}}"#;
        let (visible, thinking) = split_room_reply(raw);
        assert_eq!(visible, "**Bilan** du tour.");
        assert_eq!(thinking.as_deref(), Some("fin"));
    }

    #[test]
    fn real_tool_action_envelope_left_for_tool_loop() {
        let raw = r#"{"thought":"je note","action":"notes.create","args":{"title":"x","content":"y"}}"#;
        let (visible, thinking) = split_room_reply(raw);
        // Not peeled as salon prose — stays as JSON for the tool path / display.
        assert!(visible.contains("notes.create") || visible.contains("\"title\""));
        // Thought is folded into Reflection so tool passes do not lose reasoning.
        assert_eq!(thinking.as_deref(), Some("je note"));
    }

    #[test]
    fn stream_partial_suppresses_incomplete_and_tool_json() {
        assert!(stream_visible_partial(r#"{"thought":"enc"#).is_none());
        assert!(stream_visible_partial(
            r#"{"thought":"je cherche","action":"web.search","args":{"query":"x"}}"#
        )
        .is_none());
        assert_eq!(
            stream_visible_partial("Voici la synthèse finale du salon."),
            Some("Voici la synthèse finale du salon.".into())
        );
        assert_eq!(
            stream_visible_partial(
                r#"{"thought":"ok","action":"","args":{"question":"On continue ?"}}"#
            ),
            Some("On continue ?".into())
        );
    }

    #[test]
    fn merge_room_thinking_dedupes() {
        assert_eq!(
            merge_room_thinking(&["a".into(), "a".into(), "b".into()]).as_deref(),
            Some("a\n\nb")
        );
        assert!(merge_room_thinking(&["  ".into()]).is_none());
    }

    #[test]
    fn utf8_accents_preserved_in_visible_and_thinking() {
        let raw = "La mémoire garde la réponse système déjà déployée.";
        let (visible, thinking) = split_room_reply(raw);
        assert_eq!(visible, raw);
        assert!(thinking.is_none());
    }

    #[test]
    fn truncated_user_ask_json_not_shown_in_visible_bubble() {
        let prose = "Je lance une interrogation à l'utilisateur.";
        let raw = format!(
            "{prose}\n{{\"thought\":\"x\",\"action\":\"user.ask\",\"args\":{{\"question\":\"Temp OK ?\",\"choices\":[\"oui\",\"non\"]}}"
        );
        let (visible, thinking) = split_room_reply(&raw);
        assert!(!visible.contains("\"action\""));
        assert!(!visible.contains("user.ask"));
        assert!(visible.contains(prose) || visible.contains("Temp OK ?"));
        assert!(thinking.is_some());
    }

    #[test]
    fn json_fence_user_ask_stripped_from_visible() {
        let raw = "Voici ma proposition.\n```json\n{\"thought\":\"ask\",\"action\":\"user.ask\",\"args\":{\"question\":\"On continue ?\",\"choices\":[\"oui\",\"non\"]}}\n```";
        let (visible, _) = split_room_reply(raw);
        assert!(!visible.contains("```"));
        assert!(!visible.contains("\"action\""));
        assert!(visible.contains("Voici ma proposition") || visible.contains("On continue ?"));
    }

    #[test]
    fn try_close_unbalanced_repairs_missing_brace() {
        let frag = r#"{"thought":"x","action":"user.ask","args":{"question":"OK?","choices":["a","b"]}"#;
        let repaired = try_close_unbalanced_json_object(frag).expect("repair");
        let v: serde_json::Value = serde_json::from_str(&repaired).unwrap();
        assert_eq!(v["action"], "user.ask");
    }
}
