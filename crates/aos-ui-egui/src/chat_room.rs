//! In-app chat room helpers (slice 3): personas, roster labels, speaker colors, @ mentions.

use crate::i18n::{self, UiStrings};
use aos_agent::room_conductor::resolve_mention_token;
use aos_agent::room_conductor::{build_initial_queue, effective_max_turns};
use aos_agent::room_runtime::ROOM_ACTION_UNAVAILABLE;
use aos_agent::storage_path::ROOM_HOST_PATH_DISALLOWED;
use aos_proto::{
    AgentInfo, AgentKind, AgentState, ChatRoomMember, ChatSessionMeta, ChatSessionMode,
};
use eframe::egui;

pub use aos_agent::room_personas::{persona_agent_id, persona_by_id, ROOM_PERSONAS};

/// Localized persona chip / roster label (EN + FR via i18n).
pub fn persona_label(t: &UiStrings, persona_id: &str) -> &'static str {
    i18n::persona_label(t, persona_id)
}

/// True when the wire agent id is a built-in salon persona (`persona-researcher`, …).
pub fn is_persona_agent_id(agent_id: &str) -> bool {
    agent_id.starts_with("persona-")
}

/// Roster name for UI: exact user label when set on Agents tab; else localized persona.
pub fn member_display_label(t: &UiStrings, member: &ChatRoomMember) -> String {
    if member.persona_id.is_none() {
        return member.display_name.clone();
    }
    if let Some(pid) = member.persona_id.as_deref() {
        if let Some(persona) = persona_by_id(pid) {
            if member.display_name != persona.display_name {
                return member.display_name.clone();
            }
            return persona_label(t, pid).to_string();
        }
    }
    member.display_name.clone()
}

/// Speaker queue for an in-flight room turn (`Researcher puis Critic` / `Researcher then Critic`).
pub fn format_turn_speaker_queue(
    t: &UiStrings,
    user_message: &str,
    members: &[ChatRoomMember],
    conductor_policy: Option<&aos_proto::ChatRoomConductorPolicy>,
) -> Option<String> {
    if user_message.trim().is_empty() || members.is_empty() {
        return None;
    }
    let mut queue = build_initial_queue(user_message, members);
    if let Some(policy) = conductor_policy {
        queue.truncate(effective_max_turns(policy) as usize);
    }
    if queue.is_empty() {
        return None;
    }
    let names: Vec<String> = queue
        .iter()
        .filter_map(|id| {
            members
                .iter()
                .find(|m| m.agent_id == *id)
                .map(|m| member_display_label(t, m))
        })
        .collect();
    if names.is_empty() {
        return None;
    }
    Some(names.join(t.room_queue_joiner))
}

pub fn active_session_meta<'a>(
    sessions: &'a [ChatSessionMeta],
    active_id: Option<&str>,
) -> Option<&'a ChatSessionMeta> {
    let id = active_id?;
    sessions.iter().find(|s| s.id == id)
}

pub fn session_is_room(meta: Option<&ChatSessionMeta>) -> bool {
    meta.is_some_and(|m| m.mode == ChatSessionMode::Room)
}

/// Merge built-in persona placeholders so the Agents library always lists all four.
pub fn agents_with_library_placeholders(agents: &[AgentInfo], _t: &UiStrings) -> Vec<AgentInfo> {
    let mut out = agents.to_vec();
    for persona in ROOM_PERSONAS {
        let id = persona_agent_id(persona.id);
        if out.iter().any(|a| a.agent_id == id) {
            continue;
        }
        let label = persona.display_name;
        out.push(AgentInfo {
            agent_id: id,
            state: AgentState::Roster,
            directive: String::new(),
            pid: None,
            caps: vec![],
            last_output: String::new(),
            step: 0,
            max_steps: 0,
            current_task: None,
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: vec![],
            tools: vec![],
            mcp_servers: vec![],
            fail_reason: None,
            session_id: None,
            model_id: None,
            title: label.to_string(),
            kind: AgentKind::Roster,
            display_name: Some(label.to_string()),
            persona_id: Some(persona.id.to_string()),
            origin: None,
            deep_plan: None,
            cognitive_mode: aos_proto::CognitiveMode::Normal,
        });
    }
    out
}

/// Label for a library roster agent (exact user label or localized persona).
pub fn roster_agent_label(t: &UiStrings, agent: &AgentInfo) -> String {
    if agent.uses_typed_display_name() {
        return agent.display_name.as_ref().unwrap().trim().to_string();
    }
    if let Some(pid) = agent.persona_id.as_deref() {
        return persona_label(t, pid).to_string();
    }
    if let Some(n) = agent
        .display_name
        .as_deref()
        .filter(|n| !n.trim().is_empty())
    {
        return n.to_string();
    }
    agent.display_title().to_string()
}

/// Salon picker entry: built-in personas, roster library, and page-created Task agents.
pub fn is_salon_picker_candidate(agent: &AgentInfo) -> bool {
    if agent.is_ephemeral_chat_spawn() {
        return false;
    }
    if agent.persona_id.is_some() {
        return agent.is_roster();
    }
    if agent.is_roster() {
        return true;
    }
    agent.kind == AgentKind::Task
        && matches!(agent.origin.as_deref(), Some("library") | Some("form"))
}

/// Roster library entries not yet in this session.
pub fn library_add_candidates(
    agents: &[AgentInfo],
    members: &[ChatRoomMember],
    t: &UiStrings,
) -> Vec<AgentInfo> {
    let present = member_ids(members);
    agents_with_library_placeholders(agents, t)
        .into_iter()
        .filter(is_salon_picker_candidate)
        .filter(|a| !present.contains(a.agent_id.as_str()))
        .collect()
}

pub fn member_ids(members: &[ChatRoomMember]) -> std::collections::HashSet<&str> {
    members.iter().map(|m| m.agent_id.as_str()).collect()
}

/// Display name from roster (`speaker_id`); never trust a free-text spoof field on the message.
pub fn roster_display_name(
    t: &UiStrings,
    members: &[ChatRoomMember],
    speaker_id: &str,
    stored_speaker_name: Option<&str>,
) -> String {
    members
        .iter()
        .find(|m| m.agent_id == speaker_id)
        .map(|m| member_display_label(t, m))
        .or_else(|| {
            stored_speaker_name
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(|n| n.to_string())
        })
        .unwrap_or_else(|| t.room_member_fallback.to_string())
}

/// Visible salon bubble text (UTF-8 safe, no raw thought JSON).
pub fn format_room_visible_bubble(text: &str) -> String {
    let (visible, _) = aos_agent::room_reply::split_room_reply(text);
    if !visible.is_empty() {
        visible
    } else {
        text.trim().to_string()
    }
}

/// Full salon bubble paint pipeline: visible prose, no transcript prefix, no tool ids, human `@`.
pub fn prepare_room_bubble_text(
    t: &UiStrings,
    text: &str,
    members: &[ChatRoomMember],
    from_speaker_bubble: bool,
) -> String {
    let base = if from_speaker_bubble {
        format_room_visible_bubble(text)
    } else {
        text.trim().to_string()
    };
    let base = aos_agent::room_reply::strip_salon_transcript_prefix(&base);
    let base = humanize_tool_id_tokens(&base, t);
    let base = strip_tool_id_tokens(&base);
    let base = format_salon_json_for_display(&base);
    let base = strip_salon_markdown_markers(&base);
    let base = strip_salon_sentinels(&base);
    let base = localize_roster_persona_names_in_prose(t, &base, members);
    let base = aos_agent::room_reply::sanitize_visible_chars(&base);
    format_room_mention_destinations(t, &base, members)
}

/// Remove runtime sentinels from visible salon prose (toast copy is shown separately).
pub fn strip_salon_sentinels(text: &str) -> String {
    let mut work = text.to_string();
    for sentinel in [ROOM_HOST_PATH_DISALLOWED, ROOM_ACTION_UNAVAILABLE] {
        work = work.replace(&format!("`{sentinel}`"), "");
        work = remove_bare_token(&work, sentinel);
    }
    collapse_paint_spaces(&work)
}

fn remove_bare_token(text: &str, token: &str) -> String {
    if token.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while i < text.len() {
        let mut matched = false;
        if let Some(rest) = text.get(i..) {
            if let Some(candidate) = rest.get(..token.len()) {
                if candidate.eq_ignore_ascii_case(token) {
                    let before_ok = i == 0
                        || text[..i]
                            .chars()
                            .next_back()
                            .map(|c| !is_ident_char(c))
                            .unwrap_or(true);
                    let after_idx = i + token.len();
                    let after_ok = text
                        .get(after_idx..)
                        .and_then(|s| s.chars().next())
                        .map(|c| !is_ident_char(c))
                        .unwrap_or(true);
                    if before_ok && after_ok {
                        i = after_idx;
                        matched = true;
                    }
                }
            }
        }
        if !matched {
            let ch = text[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    collapse_paint_spaces(&out)
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Swap built-in English persona ids for the roster member's localized header label.
fn localize_roster_persona_names_in_prose(
    t: &UiStrings,
    text: &str,
    members: &[ChatRoomMember],
) -> String {
    let mut out = text.to_string();
    for member in members {
        let pid = member.persona_id.as_deref().filter(|p| !p.is_empty());
        let Some(pid) = pid else {
            continue;
        };
        let Some(persona) = persona_by_id(pid) else {
            continue;
        };
        if member.display_name.trim() != persona.display_name {
            continue;
        }
        let localized = persona_label(t, pid);
        if localized != persona.display_name {
            out = replace_word_ignore_case(&out, persona.display_name, localized);
        }
    }
    out
}

fn replace_word_ignore_case(haystack: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return haystack.to_string();
    }
    let mut out = String::with_capacity(haystack.len());
    let mut i = 0usize;
    while i < haystack.len() {
        let mut matched = false;
        if let Some(rest) = haystack.get(i..) {
            if let Some(candidate) = rest.get(..needle.len()) {
                if candidate.eq_ignore_ascii_case(needle) {
                    let before_ok = i == 0
                        || haystack[..i]
                            .chars()
                            .next_back()
                            .map(|c| !c.is_ascii_alphanumeric())
                            .unwrap_or(true);
                    let after_idx = i + needle.len();
                    let after_ok = haystack
                        .get(after_idx..)
                        .and_then(|s| s.chars().next())
                        .map(|c| !c.is_ascii_alphanumeric())
                        .unwrap_or(true);
                    if before_ok && after_ok {
                        out.push_str(replacement);
                        i = after_idx;
                        matched = true;
                    }
                }
            }
        }
        if !matched {
            let ch = haystack[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Replace known tool ids with human labels; strip unknown tool-like tokens.
pub fn humanize_tool_id_tokens(text: &str, t: &crate::i18n::UiStrings) -> String {
    let mut work = text.to_string();
    while let Some(start) = work.find('`') {
        if let Some(end_rel) = work[start + 1..].find('`') {
            let inner = work[start + 1..start + 1 + end_rel].trim();
            if looks_like_tool_id(inner) {
                let replacement = crate::i18n::tool_human_label(t, inner)
                    .map(|label| format!("{label}"))
                    .unwrap_or_default();
                let before = work[..start].trim_end();
                let after = work[start + 1 + end_rel + 1..].trim_start();
                work = match (before.is_empty(), after.is_empty(), replacement.is_empty()) {
                    (true, true, _) => String::new(),
                    (true, false, _) => after.to_string(),
                    (false, true, _) => before.to_string(),
                    (false, false, true) => format!("{before} {after}"),
                    (false, false, false) => format!("{before} {replacement} {after}"),
                };
                continue;
            }
        }
        break;
    }
    collapse_paint_spaces(&humanize_bare_tool_id_tokens(&work, t))
}

fn humanize_bare_tool_id_tokens(text: &str, t: &crate::i18n::UiStrings) -> String {
    let mut out = String::new();
    let mut token = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-' {
            token.push(ch);
        } else {
            if !token.is_empty() {
                push_humanized_or_drop_token(&mut out, &token, t);
                token.clear();
            }
            out.push(ch);
        }
    }
    if !token.is_empty() {
        push_humanized_or_drop_token(&mut out, &token, t);
    }
    out
}

fn push_humanized_or_drop_token(out: &mut String, token: &str, t: &crate::i18n::UiStrings) {
    if !looks_like_tool_id(token) {
        out.push_str(token);
        return;
    }
    if let Some(label) = crate::i18n::tool_human_label(t, token) {
        if !out.is_empty() && !out.ends_with(' ') && !out.ends_with('\n') {
            out.push(' ');
        }
        out.push_str(label);
    }
}

/// Never paint `tool.id` tokens (inline code or bare) in salon bubbles.
pub fn strip_tool_id_tokens(text: &str) -> String {
    let mut work = text.to_string();
    while let Some(start) = work.find('`') {
        if let Some(end_rel) = work[start + 1..].find('`') {
            let inner = work[start + 1..start + 1 + end_rel].trim();
            if looks_like_tool_id(inner) {
                let before = work[..start].trim_end();
                let after = work[start + 1 + end_rel + 1..].trim_start();
                work = match (before.is_empty(), after.is_empty()) {
                    (true, true) => String::new(),
                    (true, false) => after.to_string(),
                    (false, true) => before.to_string(),
                    (false, false) => format!("{before} {after}"),
                };
                continue;
            }
        }
        break;
    }
    collapse_paint_spaces(&strip_bare_tool_id_tokens(&work))
}

fn looks_like_tool_id(token: &str) -> bool {
    let token = token.trim();
    if token.is_empty() || !token.contains('.') {
        return false;
    }
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 || parts.len() > 4 {
        return false;
    }
    parts.iter().all(|part| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    })
}

fn strip_bare_tool_id_tokens(text: &str) -> String {
    let mut out = String::new();
    let mut token = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-' {
            token.push(ch);
        } else {
            if !token.is_empty() {
                if !looks_like_tool_id(&token) {
                    out.push_str(&token);
                }
                token.clear();
            }
            out.push(ch);
        }
    }
    if !token.is_empty() && !looks_like_tool_id(&token) {
        out.push_str(&token);
    }
    out
}

fn collapse_paint_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_space_on_line = false;
    for ch in text.chars() {
        if ch == '\n' {
            out.push('\n');
            prev_space_on_line = false;
        } else if ch.is_whitespace() {
            if !prev_space_on_line {
                out.push(' ');
                prev_space_on_line = true;
            }
        } else {
            out.push(ch);
            prev_space_on_line = false;
        }
    }
    out.trim().to_string()
}

/// Pretty-print embedded JSON and preserve line breaks for wrapped salon bubbles.
pub fn format_salon_json_for_display(text: &str) -> String {
    let work = expand_fenced_json_blocks(text);
    let work = pretty_format_embedded_json_objects(&work);
    soft_wrap_long_lines(&work, 96)
}

fn expand_fenced_json_blocks(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("```") {
        out.push_str(&rest[..start]);
        let after_ticks = &rest[start + 3..];
        let body_start = match after_ticks.find('\n') {
            Some(nl) => start + 3 + nl + 1,
            None => {
                out.push_str(&rest[start..]);
                return out;
            }
        };
        let close_rel = rest[body_start..].find("```");
        let Some(close_rel) = close_rel else {
            out.push_str(&rest[start..]);
            return out;
        };
        let body = rest[body_start..body_start + close_rel].trim();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&pretty_json_text(body));
        out.push('\n');
        rest = &rest[body_start + close_rel + 3..];
    }
    out.push_str(rest);
    out
}

fn pretty_format_embedded_json_objects(text: &str) -> String {
    let mut out = String::new();
    let mut i = 0usize;
    while i < text.len() {
        let rel = text[i..].find('{');
        let rel = match rel {
            Some(r) => r,
            None => {
                out.push_str(&text[i..]);
                break;
            }
        };
        let start = i + rel;
        out.push_str(&text[i..start]);
        trim_trailing_json_label(&mut out);
        if let Some(obj) = aos_agent::room_reply::extract_first_json_object(&text[start..]) {
            if serde_json::from_str::<serde_json::Value>(&obj).is_ok() {
                out.push_str(&pretty_json_text(&obj));
                i = start + obj.len();
                continue;
            }
        }
        let ch = text[start..].chars().next().unwrap();
        out.push(ch);
        i = start + ch.len_utf8();
    }
    out
}

fn trim_trailing_json_label(out: &mut String) {
    let trimmed = out.trim_end();
    if trimmed.ends_with("json") {
        let keep = trimmed.len() - 4;
        *out = trimmed[..keep].trim_end().to_string();
    }
}

fn pretty_json_text(raw: &str) -> String {
    serde_json::from_str::<serde_json::Value>(raw.trim())
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or_else(|| raw.trim().to_string())
}

const JSON_SOFT_BREAK: char = '\u{200B}';

fn soft_wrap_long_lines(text: &str, max_len: usize) -> String {
    text.lines()
        .map(|line| {
            if line.len() <= max_len {
                line.to_string()
            } else {
                insert_json_soft_breaks(line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn insert_json_soft_breaks(line: &str) -> String {
    let mut out = String::with_capacity(line.len() + line.len() / 8);
    for ch in line.chars() {
        out.push(ch);
        if matches!(ch, ',' | ':' | '{' | '}' | '[' | ']') {
            out.push(JSON_SOFT_BREAK);
        }
    }
    out
}

/// Strip common markdown markers from salon bubble prose (not rendered as markdown).
pub fn strip_salon_markdown_markers(text: &str) -> String {
    let mut work = text.to_string();
    while let Some(start) = work.find("**") {
        let rest = &work[start + 2..];
        if let Some(end) = rest.find("**") {
            let inner = rest[..end].to_string();
            let tail = rest[end + 2..].to_string();
            work = format!("{}{}{}", &work[..start], inner, tail);
        } else {
            break;
        }
    }
    work = work
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("### ") {
                rest
            } else if let Some(rest) = trimmed.strip_prefix("## ") {
                rest
            } else if let Some(rest) = trimmed.strip_prefix("# ") {
                rest
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    while let Some(start) = work.find('`') {
        if let Some(end_rel) = work[start + 1..].find('`') {
            let inner = work[start + 1..start + 1 + end_rel].to_string();
            let tail = work[start + 1 + end_rel + 1..].to_string();
            work = format!("{}{}{}", &work[..start], inner, tail);
        } else {
            break;
        }
    }
    work = strip_inline_single_asterisk_emphasis(&work);
    collapse_paint_spaces(&work)
}

fn prose_list_marker_len(text: &str, at: usize) -> Option<usize> {
    if text.get(at..).is_some_and(|tail| tail.starts_with("* ")) {
        Some(2)
    } else if text.get(at..).is_some_and(|tail| tail.starts_with("- ")) {
        Some(2)
    } else {
        None
    }
}

fn normalize_prose_paint_input(text: &str) -> String {
    text.replace('\u{200b}', "")
}

fn trim_prose_boundary_tail(s: &str) -> &str {
    s.trim_end_matches(|c: char| c.is_whitespace() || c == '\u{200b}')
}

fn is_prose_list_boundary_before(text: &str, at: usize) -> bool {
    if at == 0 {
        return true;
    }
    let before = &text[..at];
    if before.ends_with(". ") {
        return true;
    }
    if before.ends_with(": ") || before.ends_with(":\n") {
        return true;
    }
    if let Some(line_start) = before.rfind('\n') {
        if before[line_start + 1..].chars().all(|c| c.is_whitespace()) {
            return true;
        }
    } else if before.chars().all(|c| c.is_whitespace()) {
        return true;
    }
    trim_prose_boundary_tail(before).ends_with(':')
}

fn is_prose_list_marker(text: &str, at: usize) -> bool {
    prose_list_marker_len(text, at).is_some() && is_prose_list_boundary_before(text, at)
}

fn is_prose_bullet_label_start(text: &str, at: usize) -> bool {
    if !is_prose_list_boundary_before(text, at) {
        return false;
    }
    let Some(rest) = text.get(at..) else {
        return false;
    };
    let Some(colon) = rest.find(" :").or_else(|| rest.find(':')) else {
        return false;
    };
    let label = rest[..colon].trim();
    let word_count = label.split_whitespace().count();
    word_count <= 2
        && !label.is_empty()
        && label
            .chars()
            .all(|c| c.is_alphabetic() || c == ' ' || c == '-' || c == '\'')
}

fn strip_inline_single_asterisk_emphasis(text: &str) -> String {
    let mut work = text.to_string();
    let mut i = 0usize;
    while i < work.len() {
        if work.as_bytes().get(i) != Some(&b'*') {
            i += work[i..].chars().next().unwrap().len_utf8();
            continue;
        }
        if work[i..].starts_with("**") {
            i += 2;
            continue;
        }
        if is_prose_list_marker(&work, i) {
            i += 1;
            continue;
        }
        let rest = &work[i + 1..];
        let Some(close_rel) = rest.find('*') else {
            i += 1;
            continue;
        };
        if rest[close_rel..].starts_with("**") {
            i += 1;
            continue;
        }
        let inner = rest[..close_rel].to_string();
        if inner.is_empty() || inner.contains('*') {
            i += 1;
            continue;
        }
        let end = i + 1 + close_rel + 1;
        work = format!("{}{}{}", &work[..i], inner, &work[end..]);
        i += inner.len();
    }
    work
}

fn mention_chip_style(ui: &egui::Ui) -> (egui::Color32, egui::Color32, egui::Stroke) {
    let visuals = ui.visuals();
    (
        visuals.widgets.inactive.bg_fill,
        visuals.text_color(),
        visuals.widgets.noninteractive.bg_stroke,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BubbleSegment {
    Text(String),
    Mention(String),
}

fn mention_match_labels_longest_first(
    t: &UiStrings,
    members: &[ChatRoomMember],
) -> Vec<(String, String)> {
    let mut labels: Vec<(String, String)> = Vec::new();
    for member in members {
        let human = member_display_label(t, member);
        if human.trim().is_empty() {
            continue;
        }
        labels.push((human.clone(), human.clone()));
        if let Some(pid) = member.persona_id.as_deref() {
            for alias in aos_agent::room_personas::persona_mention_labels(pid) {
                if alias.eq_ignore_ascii_case(&human) {
                    continue;
                }
                if mention_alias_claimed_by_other_member(t, members, alias, &member.agent_id) {
                    continue;
                }
                labels.push((alias.to_string(), human.clone()));
            }
        }
    }
    labels.sort_by_key(|(match_label, _)| std::cmp::Reverse(match_label.len()));
    labels.dedup_by(|a, b| a.0.eq_ignore_ascii_case(&b.0));
    labels
}

fn is_json_block_paragraph(para: &str) -> bool {
    para.contains('\n') && (para.trim_start().starts_with('{') || para.contains("\": "))
}

fn split_prose_prefix_from_json_block(para: &str) -> (&str, &str) {
    let trimmed = para.trim_start();
    if trimmed.starts_with('{') {
        return ("", trimmed);
    }
    if let Some(rel) = para.find("\n{") {
        return (para[..rel].trim_end(), para[rel + 1..].trim_start());
    }
    if para.contains("\": ") {
        if let Some(rel) = para.find('{') {
            return (para[..rel].trim_end(), para[rel..].trim_start());
        }
    }
    (para, "")
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProsePaintUnit {
    Text(String),
    Bullet(String),
}

fn push_prose_text_unit(units: &mut Vec<ProsePaintUnit>, text: &str) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Some(ProsePaintUnit::Text(prev)) = units.last_mut() {
        if !prev.is_empty() {
            prev.push('\n');
        }
        prev.push_str(trimmed);
    } else {
        units.push(ProsePaintUnit::Text(trimmed.to_string()));
    }
}

/// Split salon prose into text runs and bullet bodies, including inline `* ` / `- `
/// markers after `:` or between sentences on one line.
fn split_prose_paint_units(text: &str) -> Vec<ProsePaintUnit> {
    let normalized = normalize_prose_paint_input(text);
    let text = normalized.as_str();
    let mut units = Vec::new();
    let mut segment_start = 0usize;
    let mut i = 0usize;
    while i < text.len() {
        let marker_len = prose_list_marker_len(text, i);
        let bullet = if marker_len.is_some() && is_prose_list_marker(text, i) {
            Some((i, i + marker_len.unwrap()))
        } else if is_prose_bullet_label_start(text, i) {
            Some((i, i))
        } else {
            None
        };
        if let Some((marker_at, body_start)) = bullet {
            push_prose_text_unit(&mut units, &text[segment_start..marker_at]);
            let body_end = find_inline_bullet_body_end(text, body_start);
            let body = text[body_start..body_end].trim();
            if !body.is_empty() {
                units.push(ProsePaintUnit::Bullet(body.to_string()));
            }
            i = body_end;
            segment_start = i;
            continue;
        }
        i += text[i..].chars().next().unwrap().len_utf8();
    }
    push_prose_text_unit(&mut units, &text[segment_start..]);
    units
}

fn find_inline_bullet_body_end(text: &str, body_start: usize) -> usize {
    let mut i = body_start;
    while i < text.len() {
        if i > body_start && (is_prose_list_marker(text, i) || is_prose_bullet_label_start(text, i))
        {
            return i;
        }
        i += text[i..].chars().next().unwrap().len_utf8();
    }
    text.len()
}

fn paint_prose_lines(
    ui: &mut egui::Ui,
    text: &str,
    body_w: f32,
    labels: &[(String, String)],
    chip_fill: egui::Color32,
    chip_text: egui::Color32,
    chip_stroke: egui::Stroke,
) {
    for unit in split_prose_paint_units(text) {
        match unit {
            ProsePaintUnit::Text(s) => {
                for line in s.split('\n') {
                    if line.trim().is_empty() {
                        continue;
                    }
                    paint_line_with_mention_chips(
                        ui,
                        line.trim_start(),
                        labels,
                        chip_fill,
                        chip_text,
                        chip_stroke,
                    );
                    ui.add_space(2.0);
                }
            }
            ProsePaintUnit::Bullet(body) => {
                ui.horizontal_top(|ui| {
                    ui.set_max_width(body_w);
                    ui.label("•");
                    paint_line_with_mention_chips(
                        ui,
                        &body,
                        labels,
                        chip_fill,
                        chip_text,
                        chip_stroke,
                    );
                });
                ui.add_space(2.0);
            }
        }
    }
}

fn paint_bubble_paragraph(
    ui: &mut egui::Ui,
    para: &str,
    body_w: f32,
    labels: &[(String, String)],
    chip_fill: egui::Color32,
    chip_text: egui::Color32,
    chip_stroke: egui::Stroke,
) {
    if is_json_block_paragraph(para) {
        let (prose, json) = split_prose_prefix_from_json_block(para);
        if !prose.is_empty() {
            paint_prose_lines(ui, prose, body_w, labels, chip_fill, chip_text, chip_stroke);
        }
        if !json.is_empty() {
            paint_wrapped_prose_block(ui, json, body_w);
        }
        return;
    }
    paint_prose_lines(ui, para, body_w, labels, chip_fill, chip_text, chip_stroke);
}

fn split_mention_segments(text: &str, labels: &[(String, String)]) -> Vec<BubbleSegment> {
    let mut out = Vec::new();
    let mut plain_start = 0usize;
    let mut i = 0usize;
    while i < text.len() {
        if text.as_bytes().get(i) != Some(&b'@') {
            i += text[i..].chars().next().unwrap().len_utf8();
            continue;
        }
        let tail = &text[i + 1..];
        let mut matched = None::<(usize, String)>;
        for (match_label, chip_label) in labels {
            if match_label.trim().is_empty() {
                continue;
            }
            // `match_label.len()` is bytes; `get` refuses mid-codepoint slices (e.g. `—`).
            let Some(prefix) = tail.get(..match_label.len()) else {
                continue;
            };
            if !prefix.eq_ignore_ascii_case(match_label) {
                continue;
            }
            let boundary = tail.get(match_label.len()..).and_then(|s| s.chars().next());
            if boundary.is_some_and(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                continue;
            }
            matched = Some((match_label.len(), chip_label.clone()));
            break;
        }
        if let Some((label_len, label)) = matched.filter(|(_, l)| !l.trim().is_empty()) {
            push_text_segment(&mut out, &text[plain_start..i]);
            out.push(BubbleSegment::Mention(label));
            i += 1 + label_len;
            plain_start = i;
        } else {
            push_text_segment(&mut out, &text[plain_start..i]);
            let end = mention_token_end(text, i);
            let token = text.get(i + 1..end).map(str::trim).unwrap_or("");
            if token.is_empty() {
                i += 1;
                if text
                    .get(i..)
                    .and_then(|s| s.chars().next())
                    .is_some_and(|c| c.is_whitespace())
                {
                    i += text[i..].chars().next().unwrap().len_utf8();
                }
            } else {
                push_text_segment(&mut out, token);
                i = end;
            }
            plain_start = i;
        }
    }
    push_text_segment(&mut out, &text[plain_start..]);
    out
}

fn push_text_segment(out: &mut Vec<BubbleSegment>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(BubbleSegment::Text(prev)) = out.last_mut() {
        prev.push_str(text);
    } else {
        out.push(BubbleSegment::Text(text.to_string()));
    }
}

/// Collapse blank-line paragraph gaps inside JSON `{`…`}` spans.
fn seal_json_intrablock_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0u32;
    let mut in_string = false;
    let mut escape = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                out.push(ch);
            }
            '{' => {
                depth += 1;
                out.push(ch);
            }
            '}' => {
                if depth > 0 {
                    depth -= 1;
                }
                out.push(ch);
            }
            '\n' if depth > 0 => {
                out.push('\n');
                while chars.peek() == Some(&'\n') {
                    chars.next();
                }
            }
            _ => out.push(ch),
        }
    }
    out
}

fn is_orphan_json_brace_paragraph(para: &str) -> bool {
    let trimmed = para.trim();
    trimmed == "{" || trimmed == "}"
}

/// Split salon bubble prose on paragraph breaks without orphan JSON braces.
fn bubble_paragraphs(text: &str) -> Vec<String> {
    let sealed = seal_json_intrablock_blank_lines(text);
    let mut paragraphs: Vec<String> = sealed
        .split("\n\n")
        .map(str::trim)
        .filter(|para| !para.is_empty())
        .map(str::to_string)
        .collect();
    let mut i = 0usize;
    while i < paragraphs.len() {
        if is_orphan_json_brace_paragraph(&paragraphs[i]) {
            if paragraphs[i].trim() == "{" && i + 1 < paragraphs.len() {
                let next = paragraphs.remove(i + 1);
                paragraphs[i] = format!("{}\n\n{}", paragraphs[i].trim(), next);
                continue;
            }
            if paragraphs[i].trim() == "}" && i > 0 {
                let brace = paragraphs.remove(i);
                paragraphs[i - 1] = format!("{}\n\n{}", paragraphs[i - 1], brace.trim());
                continue;
            }
        }
        i += 1;
    }
    paragraphs
}

/// Render salon bubble body with `@` mention chips (human labels only).
pub fn paint_room_bubble_body(
    ui: &mut egui::Ui,
    text: &str,
    _accent: egui::Color32,
    t: &UiStrings,
    members: &[ChatRoomMember],
) {
    if text.trim().is_empty() {
        return;
    }
    let body_w = ui.available_width().max(1.0);
    ui.set_max_width(body_w);
    let labels = mention_match_labels_longest_first(t, members);
    let (chip_fill, chip_text, chip_stroke) = mention_chip_style(ui);
    for para in bubble_paragraphs(text) {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }
        paint_bubble_paragraph(ui, para, body_w, &labels, chip_fill, chip_text, chip_stroke);
        ui.add_space(4.0);
    }
}

fn normalize_mention_chip_segments(segments: Vec<BubbleSegment>) -> Vec<BubbleSegment> {
    let mut out = Vec::new();
    for seg in trim_prose_after_leading_mention_run(segments) {
        match seg {
            BubbleSegment::Mention(label) => {
                if label.trim().is_empty() {
                    continue;
                }
                if let Some(BubbleSegment::Text(prev)) = out.last_mut() {
                    *prev = trim_trailing_mention_separator(prev);
                    if prev.trim().is_empty() {
                        out.pop();
                    }
                }
                out.push(BubbleSegment::Mention(label));
            }
            BubbleSegment::Text(s) => {
                let body = if matches!(out.last(), Some(BubbleSegment::Mention(_))) {
                    trim_leading_mention_prose_separator(&s)
                } else if let Some(BubbleSegment::Text(prev)) = out.last_mut() {
                    prev.push_str(&s);
                    continue;
                } else {
                    s
                };
                if body.trim().is_empty() {
                    continue;
                }
                out.push(BubbleSegment::Text(body));
            }
        }
    }
    out
}

fn trim_prose_after_leading_mention_run(segments: Vec<BubbleSegment>) -> Vec<BubbleSegment> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < segments.len() {
        match &segments[i] {
            BubbleSegment::Mention(label) => {
                if label.trim().is_empty() {
                    i += 1;
                    continue;
                }
                out.push(BubbleSegment::Mention(label.clone()));
                i += 1;
            }
            BubbleSegment::Text(s) if s.trim().is_empty() || is_mention_row_separator(s) => {
                i += 1;
            }
            BubbleSegment::Text(_) => break,
        }
    }
    if !out
        .iter()
        .any(|seg| matches!(seg, BubbleSegment::Mention(_)))
    {
        return segments;
    }
    let mut first_text = true;
    while i < segments.len() {
        match segments[i].clone() {
            BubbleSegment::Mention(label) if label.trim().is_empty() => {}
            BubbleSegment::Mention(label) => out.push(BubbleSegment::Mention(label)),
            BubbleSegment::Text(s) => {
                let body = if first_text {
                    first_text = false;
                    trim_leading_mention_prose_separator(&s)
                } else {
                    s
                };
                if !body.trim().is_empty() {
                    out.push(BubbleSegment::Text(body));
                }
            }
        }
        i += 1;
    }
    out
}

fn is_mention_row_separator(s: &str) -> bool {
    s.chars().all(|c| c == ',' || c.is_whitespace())
}

fn trim_leading_mention_prose_separator(s: &str) -> String {
    s.trim_start()
        .trim_start_matches(',')
        .trim_start()
        .to_string()
}

fn mention_following_text_display(s: &str) -> String {
    let body = trim_leading_mention_prose_separator(s);
    if body.is_empty() {
        return body;
    }
    if body.starts_with(' ') {
        body
    } else {
        format!(" {body}")
    }
}

fn trim_trailing_mention_separator(s: &str) -> String {
    s.trim_end()
        .trim_end_matches(|c: char| c == ',' || c.is_whitespace())
        .to_string()
}

fn paint_line_with_mention_chips(
    ui: &mut egui::Ui,
    line: &str,
    labels: &[(String, String)],
    chip_fill: egui::Color32,
    chip_text: egui::Color32,
    chip_stroke: egui::Stroke,
) {
    let line_w = ui.available_width().max(1.0);
    let segments = normalize_mention_chip_segments(split_mention_segments(line, labels));
    ui.set_max_width(line_w);
    ui.spacing_mut().item_spacing = egui::vec2(2.0, 1.0);
    ui.horizontal_wrapped(|ui| {
        ui.set_max_width(line_w);
        let mut after_mention = false;
        for seg in segments {
            match seg {
                BubbleSegment::Text(s) if !s.trim().is_empty() => {
                    let display = if after_mention {
                        mention_following_text_display(&s)
                    } else {
                        s.trim().to_string()
                    };
                    after_mention = false;
                    if display.trim().is_empty() {
                        continue;
                    }
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(display).color(ui.visuals().text_color()),
                        )
                        .wrap_mode(egui::TextWrapMode::Wrap),
                    );
                }
                BubbleSegment::Mention(label) if !label.trim().is_empty() => {
                    after_mention = true;
                    egui::Frame::NONE
                        .fill(chip_fill)
                        .stroke(chip_stroke)
                        .corner_radius(3.0)
                        .inner_margin(egui::Margin::symmetric(3, 1))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!("@{label}"))
                                    .small()
                                    .color(chip_text),
                            );
                        });
                }
                BubbleSegment::Mention(_) => {
                    after_mention = false;
                }
                BubbleSegment::Text(_) => {}
            }
        }
    });
}

fn paint_wrapped_prose_block(ui: &mut egui::Ui, text: &str, max_w: f32) {
    ui.set_max_width(max_w);
    ui.add(
        egui::Label::new(egui::RichText::new(text).color(ui.visuals().text_color()))
            .wrap_mode(egui::TextWrapMode::Wrap),
    );
}

/// Collapsible thinking block inside a salon speaker bubble.
pub fn room_thinking_toggle(
    ui: &mut egui::Ui,
    t: &UiStrings,
    line_index: usize,
    thinking: &str,
    open: &mut std::collections::HashSet<usize>,
) {
    let expanded = open.contains(&line_index);
    let response = ui.add(
        egui::Label::new(
            egui::RichText::new(t.room_thinking_label)
                .small()
                .color(ui.visuals().weak_text_color()),
        )
        .sense(egui::Sense::click()),
    );
    if response.clicked() {
        if expanded {
            open.remove(&line_index);
        } else {
            open.insert(line_index);
        }
    }
    if expanded {
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(thinking)
                .italics()
                .small()
                .color(ui.visuals().weak_text_color()),
        );
    }
}

/// Salon turn completion must not surface a count banner in session chrome.
pub fn room_turn_done_status(_agent_turns: u32, _cancelled: bool) -> Option<String> {
    None
}

/// Paint `@` destinations with human roster labels only — never technical `agent_id` tokens.
pub fn format_room_mention_destinations(
    t: &UiStrings,
    text: &str,
    members: &[ChatRoomMember],
) -> String {
    let mut out = text.to_string();
    for m in members {
        let id = &m.agent_id;
        let human = member_display_label(t, m);
        if human.trim().is_empty() {
            for pattern in [format!("(@{id})"), format!("@{id}")] {
                out = out.replace(&pattern, "");
            }
            continue;
        }
        let mention = format!("@{human}");
        for pattern in [format!("(@{id})"), format!("@{id}")] {
            out = out.replace(&pattern, &mention);
        }
        let stored = m.display_name.trim();
        if !stored.is_empty() && !stored.eq_ignore_ascii_case(&human) {
            for pattern in [format!("(@{stored})"), format!("@{stored}")] {
                out = out.replace(&pattern, &mention);
            }
        }
        if let Some(pid) = m.persona_id.as_deref() {
            for alias in aos_agent::room_personas::persona_mention_labels(pid) {
                if alias.eq_ignore_ascii_case(&human) || alias.eq_ignore_ascii_case(stored) {
                    continue;
                }
                if mention_alias_claimed_by_other_member(t, members, alias, &m.agent_id) {
                    continue;
                }
                out = out.replace(&format!("@{alias}"), &mention);
                out = out.replace(&format!("(@{alias})"), &mention);
            }
        }
    }
    replace_remaining_technical_mentions(t, &out, members)
}

/// Persona aliases must not steal `@tokens` that match another member's header label.
fn mention_alias_claimed_by_other_member(
    t: &UiStrings,
    members: &[ChatRoomMember],
    alias: &str,
    self_agent_id: &str,
) -> bool {
    members.iter().any(|other| {
        other.agent_id != self_agent_id
            && member_display_label(t, other).eq_ignore_ascii_case(alias)
    })
}

/// Back-compat alias — prefer [`format_room_mention_destinations`].
#[allow(dead_code)]
pub fn strip_roster_agent_id_mentions(
    t: &UiStrings,
    text: &str,
    members: &[ChatRoomMember],
) -> String {
    format_room_mention_destinations(t, text, members)
}

fn replace_remaining_technical_mentions(
    t: &UiStrings,
    text: &str,
    members: &[ChatRoomMember],
) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'@' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() {
                let c = bytes[end];
                if c.is_ascii_alphanumeric() || c == b'-' || c == b'_' {
                    end += 1;
                } else {
                    break;
                }
            }
            if end > start {
                if let Some(token) = text.get(start..end) {
                    if looks_like_technical_agent_token(token) {
                        if let Some(id) = resolve_mention_token(token, members) {
                            if let Some(m) = members.iter().find(|m| m.agent_id == id) {
                                out.push('@');
                                out.push_str(&member_display_label(t, m));
                            }
                        }
                        i = end;
                        continue;
                    }
                }
            }
        }
        let ch = text[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn looks_like_technical_agent_token(token: &str) -> bool {
    token.starts_with("agent-") || token.starts_with("persona-") || token.starts_with("agent_id")
}

fn mention_token_end(input: &str, at: usize) -> usize {
    let bytes = input.as_bytes();
    let mut end = at + 1;
    while end < bytes.len() {
        let c = bytes[end];
        if c.is_ascii_alphanumeric() || c == b'-' || c == b'_' {
            end += 1;
        } else {
            break;
        }
    }
    end
}

/// Replace the partial `@token` at `at` with `@display_name ` (drops the unfinished token tail).
pub fn insert_mention(input: &str, at: usize, display_name: &str) -> String {
    let end = mention_token_end(input, at);
    let mut out = String::new();
    out.push_str(&input[..at]);
    out.push('@');
    out.push_str(display_name);
    out.push(' ');
    if end < input.len() {
        out.push_str(input[end..].trim_start());
    }
    out
}

/// Stable per-speaker RGB derived from `speaker_id` (orrery hues, no purple glow).
pub fn speaker_color_rgb(speaker_id: &str, dark: bool) -> (u8, u8, u8) {
    let h = stable_hash(speaker_id);
    if dark {
        (
            40 + (h % 80) as u8,
            90 + ((h >> 8) % 70) as u8,
            100 + ((h >> 16) % 60) as u8,
        )
    } else {
        (
            180 + (h % 60) as u8,
            200 + ((h >> 8) % 40) as u8,
            210 + ((h >> 16) % 30) as u8,
        )
    }
}

fn stable_hash(s: &str) -> u32 {
    let mut h: u32 = 2_166_136_261;
    for b in s.bytes() {
        h = h.wrapping_mul(16_777_619).wrapping_add(u32::from(b));
    }
    h
}

pub fn joined_ms_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// `@` mention completions against the salon roster (display name or agent id prefix).
pub fn mention_completions(
    input: &str,
    members: &[ChatRoomMember],
    t: &UiStrings,
) -> Vec<(String, String)> {
    let Some(at) = input.rfind('@') else {
        return Vec::new();
    };
    let tail = &input[at + 1..];
    if tail.contains(' ') {
        return Vec::new();
    }
    let needle = tail.to_ascii_lowercase();
    let mut out = Vec::new();
    for m in members {
        let label = member_display_label(t, m);
        let mention_token = m.display_name.trim();
        let name_match = !needle.is_empty() && label.to_ascii_lowercase().starts_with(&needle);
        let stored_name_match =
            !needle.is_empty() && m.display_name.to_ascii_lowercase().starts_with(&needle);
        let id_match = !needle.is_empty() && m.agent_id.to_ascii_lowercase().starts_with(&needle);
        let persona_match = m
            .persona_id
            .as_deref()
            .is_some_and(|p| !needle.is_empty() && p.to_ascii_lowercase().starts_with(&needle));
        if needle.is_empty() || name_match || stored_name_match || id_match || persona_match {
            out.push((insert_mention(input, at, mention_token), label));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::ChatSessionMode;

    fn salon_still_bullet_excerpt_newlines() -> &'static str {
        "Voici mon analyse des manques critiques et des ajustements :\n\
* Manque : L'OS ne connaît pas les schémas MERN, T3 ou Django-DRF par cœur.\n\
* Solution : Créer une bibliothèque de Skills déclaratives.\n\
* Manque : de \"Contextual Reasoning\" pour la génération.\n\
* Solution : Intégrer un prompt engineering interne."
    }

    fn salon_still_bullet_excerpt_inline() -> &'static str {
        "Voici mon analyse des manques critiques et des ajustements : \
* Manque : L'OS ne connaît pas les schémas MERN, T3 ou Django-DRF par cœur. \
* Solution : Créer une bibliothèque de Skills déclaratives. \
* Manque : de \"Contextual Reasoning\" pour la génération. \
* Solution : Intégrer un prompt engineering interne."
    }

    fn bubble_paint_units_from_prepared(prepared: &str) -> Vec<ProsePaintUnit> {
        let mut units = Vec::new();
        for para in bubble_paragraphs(prepared) {
            let para = para.trim();
            if para.is_empty() {
                continue;
            }
            if is_json_block_paragraph(para) {
                let (prose, _json) = split_prose_prefix_from_json_block(para);
                units.extend(split_prose_paint_units(prose));
            } else {
                units.extend(split_prose_paint_units(para));
            }
        }
        units
    }

    fn assert_salon_bullet_excerpt_paints_without_raw_asterisks(units: &[ProsePaintUnit]) {
        for unit in units {
            match unit {
                ProsePaintUnit::Text(s) => {
                    assert!(
                        !s.contains("* Manque"),
                        "raw '* Manque' remained in painted text: {s}"
                    );
                    assert!(
                        !s.contains("* Solution"),
                        "raw '* Solution' remained in painted text: {s}"
                    );
                }
                ProsePaintUnit::Bullet(b) => {
                    assert!(!b.contains('*'), "raw '*' remained in bullet body: {b}");
                }
            }
        }
        let manque = units
            .iter()
            .filter(|unit| matches!(unit, ProsePaintUnit::Bullet(b) if b.starts_with("Manque")))
            .count();
        let solution = units
            .iter()
            .filter(|unit| matches!(unit, ProsePaintUnit::Bullet(b) if b.starts_with("Solution")))
            .count();
        assert_eq!(manque, 2, "expected two Manque bullets, got {units:?}");
        assert_eq!(solution, 2, "expected two Solution bullets, got {units:?}");
    }

    fn member(id: &str, name: &str) -> ChatRoomMember {
        ChatRoomMember {
            agent_id: id.into(),
            display_name: name.into(),
            persona_id: None,
            joined_ms: 0,
        }
    }

    #[test]
    fn library_add_candidates_skips_current_members() {
        let t = i18n::strings("en");
        let mut m1 = member("persona-coder", "Coder");
        m1.persona_id = Some("coder".into());
        let members = vec![m1];
        let candidates = library_add_candidates(&[], &members, &t);
        assert!(!candidates
            .iter()
            .any(|a| a.persona_id.as_deref() == Some("coder")));
        assert!(candidates
            .iter()
            .any(|a| a.persona_id.as_deref() == Some("researcher")));
    }

    #[test]
    fn roster_lookup_beats_spoof_name() {
        let t = i18n::strings("en");
        let members = vec![member("agent-a", "Researcher")];
        assert_eq!(
            roster_display_name(&t, &members, "agent-a", None),
            "Researcher"
        );
        assert_eq!(
            roster_display_name(&t, &members, "agent-b", Some("Beta")),
            "Beta"
        );
        assert_eq!(
            roster_display_name(&t, &members, "agent-b", None),
            t.room_member_fallback
        );
    }

    #[test]
    fn room_turn_done_status_never_shows_banner() {
        assert!(room_turn_done_status(3, false).is_none());
        assert!(room_turn_done_status(0, true).is_none());
    }

    #[test]
    fn strip_roster_agent_id_preserves_utf8_accents() {
        let t = i18n::strings("fr");
        let members = vec![member("agent-2", "Maya")];
        let raw = "La mémoire garde la réponse système déjà déployée.";
        let painted = strip_roster_agent_id_mentions(&t, raw, &members);
        assert!(painted.contains("mémoire"));
        assert!(painted.contains("réponse"));
        assert!(painted.contains("système"));
        assert!(!painted.contains("mÃ"));
        assert!(!painted.contains("rÃ"));
    }

    #[test]
    fn format_room_visible_strips_thought_json() {
        let raw = r#"{"thought":"plan interne"} Voici la réponse."#;
        let visible = format_room_visible_bubble(raw);
        assert_eq!(visible, "Voici la réponse.");
        assert!(!visible.contains("thought"));
    }
    #[test]
    fn room_thinking_label_is_muted_noun_not_action() {
        let en = i18n::strings("en");
        let fr = i18n::strings("fr");
        assert_eq!(en.room_thinking_label, "Reflection");
        assert_eq!(fr.room_thinking_label, "Réflexion");
        assert!(!en.room_thinking_label.to_ascii_lowercase().contains("show"));
        assert!(!en.room_thinking_label.to_ascii_lowercase().contains("hide"));
        assert!(!fr.room_thinking_label.contains("Afficher"));
        assert!(!fr.room_thinking_label.contains("Masquer"));
    }

    #[test]
    fn strip_roster_agent_id_mentions_from_body() {
        let t = i18n::strings("fr");
        let members = vec![member("agent-2", "Maya"), member("agent-3", "Leo")];
        let raw = "Bonjour! Leo (@agent-3) et moi, Maya (@agent-2), sommes là.";
        let painted = format_room_mention_destinations(&t, raw, &members);
        assert!(!painted.contains("@agent-"));
        assert!(painted.contains("@Maya"));
        assert!(painted.contains("@Leo"));
    }

    #[test]
    fn format_room_mention_destinations_uses_localized_persona_label() {
        let t = i18n::strings("fr");
        let mut m1 = member("persona-critic", "Critic");
        m1.persona_id = Some("critic".into());
        let members = vec![m1];
        let painted = format_room_mention_destinations(&t, "@persona-critic confirme ?", &members);
        assert!(painted.contains("@Critique"));
        assert!(!painted.contains("persona-critic"));
    }

    #[test]
    fn room_action_unavailable_copy_locked() {
        let en = i18n::strings("en");
        let fr = i18n::strings("fr");
        assert_eq!(
            en.room_action_unavailable,
            "That action isn't available in the room."
        );
        assert_eq!(
            fr.room_action_unavailable,
            "Action indisponible dans le salon."
        );
        assert_eq!(en.room_policy_one_agent, "One agent");
        assert_eq!(fr.room_policy_one_agent, "Un agent");
        assert_eq!(en.room_policy_open_floor, "Open floor");
        assert_eq!(fr.room_policy_open_floor, "Tout le salon");
    }

    #[test]
    fn insert_mention_drops_partial_token_tail() {
        let out = insert_mention("bonjour @agent-2?", 8, "Maya");
        assert_eq!(out, "bonjour @Maya ?");
        assert!(!out.contains("agent-2"));
    }

    #[test]
    fn composer_clear_after_send_simulation() {
        let mut input = "bonjour, qui est là?".to_string();
        let text = input.trim().to_string();
        input.clear();
        assert!(input.is_empty());
        assert_eq!(text, "bonjour, qui est là?");
    }

    #[test]
    fn speaker_color_stable() {
        let a = speaker_color_rgb("agent-alpha", true);
        let b = speaker_color_rgb("agent-alpha", true);
        let c = speaker_color_rgb("agent-beta", true);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn mention_completions_insert_roster_display_name_not_localized_label() {
        let mut m1 = member("persona-researcher", "Researcher");
        m1.persona_id = Some("researcher".into());
        let members = vec![m1];
        let t_fr = i18n::strings("fr");
        let hits = mention_completions("hello @Cher", &members, &t_fr);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].0.contains("@Researcher"));
        assert!(!hits[0].0.contains("@Chercheur"));
        assert_eq!(hits[0].1, t_fr.persona_researcher);
    }

    #[test]
    fn turn_queue_directed_mention_only_target() {
        let t = i18n::strings("en");
        let mut m1 = member("a1", "Researcher");
        m1.persona_id = Some("researcher".into());
        let mut m2 = member("a2", "Coder");
        m2.persona_id = Some("coder".into());
        let members = vec![m1, m2];
        let q = format_turn_speaker_queue(&t, "@Coder review this", &members, None).expect("queue");
        assert!(q.contains(t.persona_coder));
        assert!(!q.contains(t.persona_researcher));
    }

    #[test]
    fn mention_completions_prefix() {
        let members = vec![member("a1", "Researcher"), member("a2", "Coder")];
        let hits = mention_completions("hello @Res", &members, &i18n::strings("en"));
        assert_eq!(hits.len(), 1);
        assert!(hits[0].0.contains("@Researcher"));
    }

    #[test]
    fn session_is_room_flag() {
        let meta = ChatSessionMeta {
            id: "s".into(),
            title: "t".into(),
            created_ms: 0,
            updated_ms: 0,
            archived: false,
            pinned: false,
            message_count: 0,
            model_id: None,
            mode: ChatSessionMode::Room,
            members: vec![],
            conductor_policy: Default::default(),
            canvas_open: false,
            canvas_aspect: aos_proto::CanvasAspect::Square,
        };
        assert!(session_is_room(Some(&meta)));
    }

    #[test]
    fn library_placeholders_include_four_personas() {
        let t = i18n::strings("fr");
        let list = agents_with_library_placeholders(&[], &t);
        assert_eq!(list.len(), 4);
        assert!(list
            .iter()
            .any(|a| a.persona_id.as_deref() == Some("coder")));
        assert_eq!(
            list.iter()
                .find(|a| a.persona_id.as_deref() == Some("coder"))
                .and_then(|a| a.display_name.as_deref()),
            Some("Coder")
        );
        assert_eq!(
            roster_agent_label(
                &t,
                list.iter()
                    .find(|a| a.persona_id.as_deref() == Some("coder"))
                    .unwrap(),
            ),
            "Codeur"
        );
    }

    #[test]
    fn all_personas_defined() {
        for id in ["researcher", "critic", "coder", "planner"] {
            assert!(persona_by_id(id).is_some());
        }
    }

    #[test]
    fn ghost_mention_no_speaker_queue_label() {
        let t = i18n::strings("fr");
        let mut m1 = member("persona-critic", "Critic");
        m1.persona_id = Some("critic".into());
        let members = vec![m1];
        assert!(format_turn_speaker_queue(&t, "@Dessinateur", &members, None).is_none());
        assert!(format_turn_speaker_queue(&t, "@agent_id_123", &members, None).is_none());
    }

    #[test]
    fn canvas_update_queues_strip_member_without_at() {
        let t = i18n::strings("fr");
        let mut m1 = member("persona-critic", "Critic");
        m1.persona_id = Some("critic".into());
        let members = vec![m1];
        let q =
            format_turn_speaker_queue(&t, "Mets à jour le dessin", &members, None).expect("queue");
        assert!(q.contains(t.persona_critic));
    }

    #[test]
    fn turn_queue_joins_all_strip_members_without_at() {
        let t = i18n::strings("en");
        let mut m1 = member("a1", "Researcher");
        m1.persona_id = Some("researcher".into());
        let mut m2 = member("a2", "Critic");
        m2.persona_id = Some("critic".into());
        let members = vec![m1, m2];
        let q = format_turn_speaker_queue(&t, "Review this sketch", &members, None).expect("queue");
        assert!(q.contains(t.room_queue_joiner));
        assert!(q.contains(t.persona_researcher));
        assert!(q.contains(t.persona_critic));
    }

    #[test]
    fn turn_queue_joins_speakers() {
        let t = i18n::strings("fr");
        let mut m1 = member("a1", "Researcher");
        m1.persona_id = Some("researcher".into());
        let mut m2 = member("a2", "Coder");
        m2.persona_id = Some("coder".into());
        let members = vec![m1, m2];
        let q = format_turn_speaker_queue(&t, "@Researcher @Coder", &members, None).expect("queue");
        assert!(q.contains(t.room_queue_joiner));
        assert!(q.contains(t.persona_researcher));
        assert!(q.contains(t.persona_coder));
    }

    fn custom_library_agent(id: &str, label: &str) -> AgentInfo {
        AgentInfo {
            agent_id: id.into(),
            state: AgentState::Roster,
            directive: String::new(),
            pid: None,
            caps: vec![],
            last_output: String::new(),
            step: 0,
            max_steps: 0,
            current_task: None,
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: vec![],
            tools: vec![],
            mcp_servers: vec![],
            fail_reason: None,
            session_id: None,
            model_id: None,
            title: label.into(),
            kind: AgentKind::Roster,
            display_name: Some(label.into()),
            persona_id: None,
            origin: Some("library".into()),
            deep_plan: None,
            cognitive_mode: aos_proto::CognitiveMode::Normal,
        }
    }

    fn page_task_agent(id: &str, label: &str) -> AgentInfo {
        AgentInfo {
            agent_id: id.into(),
            state: AgentState::Running,
            directive: "Review skill manifests".into(),
            pid: Some(42),
            caps: vec![],
            last_output: String::new(),
            step: 1,
            max_steps: 32,
            current_task: None,
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: vec![],
            tools: vec![],
            mcp_servers: vec![],
            fail_reason: None,
            session_id: None,
            model_id: None,
            title: label.into(),
            kind: AgentKind::Task,
            display_name: Some(label.into()),
            persona_id: None,
            origin: Some("form".into()),
            deep_plan: None,
            cognitive_mode: aos_proto::CognitiveMode::Normal,
        }
    }

    fn chat_delegate_agent(id: &str, label: &str) -> AgentInfo {
        AgentInfo {
            agent_id: id.into(),
            state: AgentState::Running,
            directive: "Summarize this thread".into(),
            pid: Some(99),
            caps: vec![],
            last_output: String::new(),
            step: 1,
            max_steps: 32,
            current_task: None,
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: vec![],
            tools: vec![],
            mcp_servers: vec![],
            fail_reason: None,
            session_id: Some("session-1".into()),
            model_id: None,
            title: label.into(),
            kind: AgentKind::Task,
            display_name: Some(label.into()),
            persona_id: None,
            origin: Some("assistant".into()),
            deep_plan: None,
            cognitive_mode: aos_proto::CognitiveMode::Normal,
        }
    }

    #[test]
    fn typed_display_name_not_overridden_by_goal_title() {
        let t = i18n::strings("en");
        let mut agent = custom_library_agent("agent-7", "Skills Auditor");
        agent.directive = "Analyze the skills registry in depth".into();
        agent.title = aos_agent::persist::agent_title(&agent.directive);
        assert_eq!(agent.display_title(), "Skills Auditor");
        assert_eq!(roster_agent_label(&t, &agent), "Skills Auditor");
        let member = ChatRoomMember {
            agent_id: agent.agent_id.clone(),
            display_name: "Skills Auditor".into(),
            persona_id: None,
            joined_ms: 0,
        };
        assert_eq!(member_display_label(&t, &member), "Skills Auditor");
    }

    #[test]
    fn built_in_persona_keeps_i18n_without_user_label() {
        let t = i18n::strings("fr");
        let list = agents_with_library_placeholders(&[], &t);
        let coder = list
            .iter()
            .find(|a| a.persona_id.as_deref() == Some("coder"))
            .unwrap();
        assert_eq!(roster_agent_label(&t, coder), "Codeur");
    }

    #[test]
    fn page_created_roster_agent_in_library_add_candidates() {
        let t = i18n::strings("en");
        let agents = vec![custom_library_agent("agent-7", "Skills Auditor")];
        let candidates = library_add_candidates(&agents, &[], &t);
        assert!(candidates.iter().any(|a| a.agent_id == "agent-7"));
        assert_eq!(
            roster_agent_label(
                &t,
                candidates.iter().find(|a| a.agent_id == "agent-7").unwrap()
            ),
            "Skills Auditor"
        );
    }

    #[test]
    fn page_created_task_agent_in_library_add_candidates() {
        let t = i18n::strings("en");
        let agents = vec![page_task_agent("agent-10", "Module Author")];
        let candidates = library_add_candidates(&agents, &[], &t);
        assert!(candidates.iter().any(|a| a.agent_id == "agent-10"));
        assert_eq!(
            roster_agent_label(
                &t,
                candidates
                    .iter()
                    .find(|a| a.agent_id == "agent-10")
                    .unwrap(),
            ),
            "Module Author"
        );
    }

    #[test]
    fn active_page_task_agent_with_library_origin_in_picker() {
        let t = i18n::strings("en");
        let mut agent = page_task_agent("agent-11", "Skills Runner");
        agent.origin = Some("library".into());
        agent.state = AgentState::Running;
        agent.pid = Some(4242);
        assert!(is_salon_picker_candidate(&agent));
        let candidates = library_add_candidates(&[agent], &[], &t);
        assert!(candidates.iter().any(|a| a.agent_id == "agent-11"));
    }

    #[test]
    fn strip_tool_id_tokens_removes_inline_and_bare_ids() {
        let raw = "J'ai consulté `notes.read` et notes.list pour la synthèse.";
        let stripped = strip_tool_id_tokens(raw);
        assert!(!stripped.contains("notes.read"));
        assert!(!stripped.contains("notes.list"));
        assert!(stripped.contains("synthèse"));
    }

    #[test]
    fn humanize_tool_id_tokens_replaces_locked_tasks_labels() {
        let t_en = i18n::strings("en");
        let t_fr = i18n::strings("fr");
        let raw = "J'ai utilisé `tasks.list` pour vérifier.";
        let en = humanize_tool_id_tokens(raw, &t_en);
        assert!(en.contains(t_en.agents_tool_tasks_list));
        assert!(!en.contains("tasks.list"), "{en}");
        let fr = humanize_tool_id_tokens(raw, &t_fr);
        assert!(fr.contains(t_fr.agents_tool_tasks_list));
        assert!(!fr.contains("tasks.list"), "{fr}");
    }

    #[test]
    fn humanize_tool_id_tokens_drops_unknown_module_tools() {
        let t = i18n::strings("en");
        let raw = "Module `windmill.run` a échoué.";
        let out = humanize_tool_id_tokens(raw, &t);
        assert!(!out.contains("windmill.run"), "{out}");
        assert!(out.contains("échoué"), "{out}");
    }

    #[test]
    fn strip_salon_markdown_markers_removes_bold_and_headings() {
        let raw = "**Supervisor** Merci @Codeur.\n## Ce que j'ai fait\n- point";
        let stripped = strip_salon_markdown_markers(raw);
        assert!(!stripped.contains("**"));
        assert!(!stripped.contains("##"));
        assert!(stripped.contains("Supervisor"));
        assert!(stripped.contains("Ce que j'ai fait"));
    }

    #[test]
    fn strip_salon_markdown_markers_removes_single_asterisk_emphasis() {
        let raw = "La *sécurité par isolation* reste centrale.";
        let stripped = strip_salon_markdown_markers(raw);
        assert!(!stripped.contains('*'));
        assert!(stripped.contains("sécurité par isolation"));
    }

    fn match_labels(labels: &[&str]) -> Vec<(String, String)> {
        labels
            .iter()
            .map(|label| (label.to_string(), label.to_string()))
            .collect()
    }

    #[test]
    fn normalize_mention_chip_segments_drops_inline_comma() {
        let t = i18n::strings("fr");
        let mut researcher = member("persona-researcher", "Researcher");
        researcher.persona_id = Some("researcher".into());
        let members = vec![researcher];
        let labels = mention_match_labels_longest_first(&t, &members);
        let segs = normalize_mention_chip_segments(split_mention_segments(
            "C'est pertinent de la part de @Chercheur , mais je tempère.",
            &labels,
        ));
        assert_eq!(
            segs,
            vec![
                BubbleSegment::Text("C'est pertinent de la part de".to_string()),
                BubbleSegment::Mention("Chercheur".to_string()),
                BubbleSegment::Text("mais je tempère.".to_string()),
            ]
        );
    }

    #[test]
    fn supervisor_display_name_preserved_in_mentions_and_chips() {
        let t = i18n::strings("fr");
        let mut m1 = member("persona-critic", "supervisor");
        m1.persona_id = Some("critic".into());
        let members = vec![m1];
        let painted =
            format_room_mention_destinations(&t, "@supervisor, peux-tu valider ?", &members);
        assert!(painted.contains("@supervisor"));
        assert!(!painted.contains("@Critique"));
        let labels = mention_match_labels_longest_first(&t, &members);
        assert!(labels.iter().any(|(_, chip)| chip == "supervisor"));
        let segs = split_mention_segments("@supervisor confirme", &labels);
        assert_eq!(
            segs.first(),
            Some(&BubbleSegment::Mention("supervisor".to_string()))
        );
    }

    #[test]
    fn persona_alias_does_not_steal_another_members_header_label() {
        let t = i18n::strings("fr");
        let mut critic = member("persona-critic", "Critic");
        critic.persona_id = Some("critic".into());
        let mut supervisor = member("persona-planner", "supervisor");
        supervisor.persona_id = Some("planner".into());
        let members = vec![critic, supervisor];
        let painted = format_room_mention_destinations(&t, "@supervisor organise", &members);
        assert!(painted.contains("@supervisor"));
        assert!(!painted.contains("@Planificateur"));
    }

    #[test]
    fn prepare_room_bubble_text_strips_prefix_and_tool_ids() {
        let t = i18n::strings("fr");
        let mut m1 = member("persona-critic", "Critic");
        m1.persona_id = Some("critic".into());
        let mut m2 = member("persona-researcher", "Researcher");
        m2.persona_id = Some("researcher".into());
        let members = vec![m1, m2];
        let raw = "[Salon — Critique]\n@Chercheur, j'ai utilisé `web.search` — peux-tu confirmer ?";
        let painted = prepare_room_bubble_text(&t, raw, &members, true);
        assert!(!painted.contains("[Salon —"));
        assert!(!painted.contains("web.search"));
        assert!(painted.contains("@Chercheur"));
        assert!(!painted.contains("@Researcher"));
    }

    #[test]
    fn split_mention_segments_finds_localized_labels() {
        let t = i18n::strings("fr");
        let mut m1 = member("persona-researcher", "Researcher");
        m1.persona_id = Some("researcher".into());
        let members = vec![m1];
        let labels = mention_match_labels_longest_first(&t, &members);
        let segs = split_mention_segments("@Chercheur, peux-tu détailler ?", &labels);
        assert_eq!(
            segs,
            vec![
                BubbleSegment::Mention("Chercheur".to_string()),
                BubbleSegment::Text(", peux-tu détailler ?".to_string()),
            ]
        );
    }

    #[test]
    fn split_mention_segments_skips_utf8_mid_char_prefix() {
        // Longer ASCII label ("supervisor", 10 bytes) must not panic when the
        // text after `@` has an em dash at byte 9 (`Critique — …`).
        let labels = match_labels(&["supervisor", "Critique"]);
        let segs = split_mention_segments(
            "@Critique — le graphe doit refléter des relations *décidées*",
            &labels,
        );
        assert_eq!(
            segs.first(),
            Some(&BubbleSegment::Mention("Critique".to_string()))
        );
        assert!(matches!(segs.get(1), Some(BubbleSegment::Text(t)) if t.starts_with(" —")));
    }

    #[test]
    fn trim_prose_after_leading_mention_run_drops_stray_comma() {
        let t = i18n::strings("fr");
        let mut supervisor = member("persona-critic", "supervisor");
        supervisor.persona_id = Some("critic".into());
        let mut researcher = member("persona-researcher", "Researcher");
        researcher.persona_id = Some("researcher".into());
        let mut planner = member("persona-planner", "Planner");
        planner.persona_id = Some("planner".into());
        let members = vec![supervisor, researcher, planner];
        let labels = mention_match_labels_longest_first(&t, &members);
        let raw = "@supervisor @Chercheur @Planificateur, Je valide l'arrêt.";
        let segs = normalize_mention_chip_segments(split_mention_segments(raw, &labels));
        let prose = segs
            .iter()
            .find_map(|seg| match seg {
                BubbleSegment::Text(s) => Some(s.as_str()),
                _ => None,
            })
            .unwrap();
        assert!(
            !prose.starts_with(','),
            "prose must not start with comma: {prose:?}"
        );
        assert!(prose.starts_with("Je valide"));
    }

    #[test]
    fn strip_salon_sentinels_removes_host_path_token() {
        let raw = "Le blocage **room_host_path_disallowed** est absolu.";
        let stripped = strip_salon_sentinels(&strip_salon_markdown_markers(raw));
        assert!(!stripped.contains("room_host_path_disallowed"));
        assert!(stripped.contains("Le blocage"));
        assert!(stripped.contains("est absolu"));
    }

    #[test]
    fn prepare_room_bubble_text_strips_sentinel_and_localizes_persona_names() {
        let t = i18n::strings("fr");
        let mut planner = member("persona-planner", "Planner");
        planner.persona_id = Some("planner".into());
        let mut researcher = member("persona-researcher", "Researcher");
        researcher.persona_id = Some("researcher".into());
        let members = vec![planner, researcher];
        let raw = "En tant que **Planner**, j'appuie **Researcher** sur room_host_path_disallowed.";
        let painted = prepare_room_bubble_text(&t, raw, &members, true);
        assert!(!painted.contains("room_host_path_disallowed"));
        assert!(painted.contains("Planificateur"));
        assert!(painted.contains("Chercheur"));
        assert!(!painted.contains("Planner"));
        assert!(!painted.contains("Researcher"));
    }

    #[test]
    fn split_mention_segments_skips_lone_at_without_chip() {
        let labels = match_labels(&["Critique"]);
        let segs = normalize_mention_chip_segments(split_mention_segments(
            "Merci @ pour cette mise en garde.",
            &labels,
        ));
        assert!(!segs
            .iter()
            .any(|seg| matches!(seg, BubbleSegment::Mention(_))));
        assert_eq!(
            segs,
            vec![BubbleSegment::Text(
                "Merci pour cette mise en garde.".to_string()
            )]
        );
    }

    #[test]
    fn split_mention_segments_drops_unresolvable_at_token() {
        let labels = match_labels(&["Chercheur"]);
        let segs = split_mention_segments("Merci @Inconnu pour l'alerte.", &labels);
        assert!(!segs
            .iter()
            .any(|seg| matches!(seg, BubbleSegment::Mention(_))));
        assert_eq!(
            segs,
            vec![BubbleSegment::Text(
                "Merci Inconnu pour l'alerte.".to_string()
            )]
        );
    }

    #[test]
    fn split_prose_paint_units_splits_inline_asterisk_after_colon() {
        let units = split_prose_paint_units(salon_still_bullet_excerpt_inline());
        assert_salon_bullet_excerpt_paints_without_raw_asterisks(&units);
    }

    #[test]
    fn split_prose_paint_units_splits_manque_after_colon_zwsp_without_asterisk() {
        let prepared =
            "Voici mon analyse des manques critiques et des ajustements :\u{200b} Manque :\u{200b} L'OS ne connaît pas les schémas MERN,\u{200b} T3 ou Django-DRF par cœur. * Solution :\u{200b} Créer une bibliothèque.";
        let manque_at = prepared.find("Manque").expect("Manque");
        assert!(
            is_prose_bullet_label_start(prepared, manque_at),
            "expected Manque label start"
        );
        let units = split_prose_paint_units(prepared);
        assert!(
            units
                .iter()
                .any(|unit| matches!(unit, ProsePaintUnit::Bullet(b) if b.starts_with("Manque"))),
            "expected Manque bullet, got {units:?}"
        );
        assert!(
            !units.iter().any(|unit| match unit {
                ProsePaintUnit::Text(s) => s.contains("* Manque"),
                _ => false,
            }),
            "raw '* Manque' remained: {units:?}"
        );
    }

    #[test]
    fn split_prose_paint_units_splits_newline_asterisk_bullets() {
        let units = split_prose_paint_units(salon_still_bullet_excerpt_newlines());
        assert_salon_bullet_excerpt_paints_without_raw_asterisks(&units);
    }

    #[test]
    fn prepare_room_bubble_text_still_excerpt_newlines_paint_without_raw_asterisks() {
        let t = i18n::strings("fr");
        let mut planner = member("persona-planner", "Planner");
        planner.persona_id = Some("planner".into());
        let mut researcher = member("persona-researcher", "Researcher");
        researcher.persona_id = Some("researcher".into());
        let members = vec![planner, researcher];
        let prepared =
            prepare_room_bubble_text(&t, salon_still_bullet_excerpt_newlines(), &members, true);
        let units = bubble_paint_units_from_prepared(&prepared);
        assert_salon_bullet_excerpt_paints_without_raw_asterisks(&units);
    }

    #[test]
    fn prepare_room_bubble_text_still_excerpt_inline_paint_without_raw_asterisks() {
        let t = i18n::strings("fr");
        let mut planner = member("persona-planner", "Planner");
        planner.persona_id = Some("planner".into());
        let mut researcher = member("persona-researcher", "Researcher");
        researcher.persona_id = Some("researcher".into());
        let members = vec![planner, researcher];
        let prepared =
            prepare_room_bubble_text(&t, salon_still_bullet_excerpt_inline(), &members, true);
        let units = bubble_paint_units_from_prepared(&prepared);
        assert_salon_bullet_excerpt_paints_without_raw_asterisks(&units);
    }

    #[test]
    fn mention_following_text_display_keeps_word_space_after_chip() {
        assert_eq!(
            mention_following_text_display(" ta proposition"),
            " ta proposition"
        );
        assert_eq!(
            mention_following_text_display("ta proposition"),
            " ta proposition"
        );
        assert_eq!(
            mention_following_text_display(", ta proposition"),
            " ta proposition"
        );
    }

    #[test]
    fn split_mention_segments_maps_planner_alias_to_header_chip() {
        let t = i18n::strings("fr");
        let mut planner = member("persona-planner", "Planner");
        planner.persona_id = Some("planner".into());
        let members = vec![planner];
        let labels = mention_match_labels_longest_first(&t, &members);
        let segs = split_mention_segments("@Planner, peux-tu valider ?", &labels);
        assert_eq!(
            segs.first(),
            Some(&BubbleSegment::Mention("Planificateur".to_string()))
        );
    }

    #[test]
    fn split_prose_prefix_from_json_block_splits_leading_mention() {
        let raw = "@Planificateur\n{\n  \"thought\": \"Plan\"\n}";
        let (prose, json) = split_prose_prefix_from_json_block(raw);
        assert_eq!(prose, "@Planificateur");
        assert!(json.starts_with('{'));
        assert!(json.contains("\"thought\""));
    }

    #[test]
    fn bubble_paragraphs_keeps_mention_before_json_together_when_sealed() {
        let raw = "@Planificateur\n{\n  \"thought\": \"Plan\"\n}";
        let paras = bubble_paragraphs(raw);
        assert_eq!(paras.len(), 1);
        assert!(paras[0].starts_with("@Planificateur"));
        assert!(paras[0].contains("\"thought\""));
    }

    #[test]
    fn format_salon_json_for_display_pretty_prints_plan_block() {
        let raw = r#"Voici le plan :
json {"args":{"nodes":[{"id":"phase-1","status":"pending"}]},"thought":"Plan"}"#;
        let formatted = format_salon_json_for_display(raw);
        assert!(formatted.contains("\"nodes\""));
        assert!(formatted.contains("phase-1"));
        assert!(formatted.contains('\n'));
        assert!(!formatted.contains("json {"));
    }

    #[test]
    fn format_salon_json_for_display_expands_fenced_block() {
        let raw = "Prologue\n```json\n{\"id\":\"phase-1\",\"status\":\"pending\"}\n```\nEpilogue";
        let formatted = format_salon_json_for_display(raw);
        assert!(formatted.contains("\"id\": \"phase-1\""));
        assert!(!formatted.contains("```"));
        assert!(formatted.contains("Prologue"));
        assert!(formatted.contains("Epilogue"));
    }

    #[test]
    fn collapse_paint_spaces_preserves_newlines() {
        let raw = "{\n  \"id\": \"phase-1\"\n}";
        let collapsed = collapse_paint_spaces(raw);
        assert!(collapsed.contains('\n'));
        assert!(collapsed.contains("\"id\": \"phase-1\""));
    }

    #[test]
    fn bubble_paragraphs_coalesces_orphan_opening_brace() {
        let raw = "{\n\n  \"thought\": \"Plan\",\n  \"args\": {}\n}";
        let paras = bubble_paragraphs(raw);
        assert_eq!(paras.len(), 1);
        assert!(paras[0].starts_with('{'));
        assert!(paras[0].contains("\"thought\""));
    }

    #[test]
    fn bubble_paragraphs_coalesces_orphan_closing_brace() {
        let raw = "{\n  \"thought\": \"Plan\"\n\n}";
        let paras = bubble_paragraphs(raw);
        assert_eq!(paras.len(), 1);
        assert!(paras[0].ends_with('}'));
        assert!(paras[0].contains("\"thought\""));
    }

    #[test]
    fn seal_json_intrablock_blank_lines_keeps_prose_breaks() {
        let raw = "Prologue\n\n{\n  \"id\": 1\n}\n\nEpilogue";
        let sealed = seal_json_intrablock_blank_lines(raw);
        let paras: Vec<_> = sealed
            .split("\n\n")
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();
        assert_eq!(paras.len(), 3);
        assert_eq!(paras[0], "Prologue");
        assert!(paras[1].starts_with('{'));
        assert_eq!(paras[2], "Epilogue");
    }

    #[test]
    fn chat_history_reloaded_banner_copy_locked() {
        let en = i18n::strings("en");
        let fr = i18n::strings("fr");
        assert_eq!(en.chat_history_reloaded, "History reloaded.");
        assert_eq!(fr.chat_history_reloaded, "Historique rechargé.");
        assert!(!en.chat_history_reloaded.contains("sess-"));
        assert!(!fr.chat_history_reloaded.contains("sess-"));
    }

    #[test]
    fn chat_delegate_excluded_from_library_add_candidates() {
        let t = i18n::strings("en");
        let agents = vec![chat_delegate_agent("agent-8", "Summarize thread")];
        let candidates = library_add_candidates(&agents, &[], &t);
        assert!(!candidates.iter().any(|a| a.agent_id == "agent-8"));
        assert!(!is_salon_picker_candidate(&agents[0]));
    }
}
