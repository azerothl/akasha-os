//! Panneau détail agent : timeline mise en forme, sources, sous-agents.

use aos_proto::{AgentInfo, AgentSource, AgentState, AgentStepRecord, AgentTrace};
#[cfg(test)]
use aos_proto::AgentKind;
use eframe::egui::{self, Color32, RichText, Ui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::icons;

#[derive(Default)]
pub struct PanelActions {
    pub pause: bool,
    pub kill: bool,
    pub resume: bool,
    pub retry: bool,
    pub continue_canvas: bool,
    pub export: bool,
    pub steer: Option<String>,
    pub open_child: Option<String>,
}


pub fn state_color(state: &AgentState) -> Color32 {
    match state {
        AgentState::Failed | AgentState::Killed => Color32::from_rgb(220, 90, 90),
        AgentState::Blocked => Color32::from_rgb(230, 160, 60),
        AgentState::Done => Color32::from_rgb(100, 190, 120),
        AgentState::Paused => Color32::from_rgb(160, 160, 180),
        AgentState::Running => Color32::from_rgb(120, 180, 230),
        AgentState::Created => Color32::GRAY,
        AgentState::Roster => Color32::from_rgb(140, 180, 160),
    }
}

pub fn fmt_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else if ms < 60_000 {
        format!("{:.1} s", ms as f64 / 1000.0)
    } else {
        format!("{} min {:02} s", ms / 60_000, (ms % 60_000) / 1000)
    }
}

pub fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        format!("{}…", s.chars().take(n).collect::<String>())
    } else {
        s.to_string()
    }
}

/// Lit le dernier `task.assess` de la timeline (`simple` / `complex`).
fn complexity_from_trace(trace: Option<&AgentTrace>) -> Option<&'static str> {
    let steps = &trace?.steps;
    for step in steps.iter().rev() {
        if step.action == "task.assess" {
            if let Some(c) = step.args.get("complexity").and_then(|v| v.as_str()) {
                if c.eq_ignore_ascii_case("complex") {
                    return Some("complex");
                }
                if c.eq_ignore_ascii_case("simple") {
                    return Some("simple");
                }
            }
            let r = step.tool_result.to_ascii_lowercase();
            if r.starts_with("complex") {
                return Some("complex");
            }
            if r.starts_with("simple") {
                return Some("simple");
            }
        }
    }
    None
}

/// Retire les blocs `<think>…</think>` (Qwen3/3.5) pour l'affichage.
fn strip_think_tags(text: &str) -> String {
    let mut rest = text.to_string();
    loop {
        let lower = rest.to_ascii_lowercase();
        let Some(start) = lower.find("<think>") else {
            break;
        };
        let after = start + "<think>".len();
        let lower_tail = rest[after..].to_ascii_lowercase();
        if let Some(rel) = lower_tail.find("</think>") {
            let end = after + rel + "</think>".len();
            rest = format!("{}{}", &rest[..start], &rest[end..]);
        } else {
            rest.truncate(start);
            break;
        }
    }
    let lower = rest.to_ascii_lowercase();
    if let Some(idx) = lower.find("</think>") {
        rest = format!("{}{}", &rest[..idx], &rest[idx + "</think>".len()..]);
    }
    rest.trim().to_string()
}

/// Retire jetons de contrôle modèle (`<channel>`, etc.) ; garde la prose après le dernier canal.
fn strip_chat_control_tokens(text: &str) -> String {
    let mut out = text.to_string();
    let lower = out.to_ascii_lowercase();
    if let Some(i) = lower.rfind("<channel>") {
        out = out[i + "<channel>".len()..].trim_start().to_string();
    }
    for token in [
        "<channel>",
        "</channel>",
        "<|channel|>",
        "<|im_start|>",
        "<|im_end|>",
    ] {
        loop {
            let l = out.to_ascii_lowercase();
            let Some(i) = l.find(token) else {
                break;
            };
            out = format!("{}{}", &out[..i], &out[i + token.len()..]);
        }
    }
    out.trim().to_string()
}

fn find_canvas_tool_leak(lower: &str) -> Option<usize> {
    let mut search_from = 0;
    while let Some(i) = lower[search_from..].find("canvas.") {
        let pos = search_from + i;
        let rest = &lower[pos + "canvas.".len()..];
        if rest
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase())
        {
            return Some(pos);
        }
        search_from = pos + 1;
    }
    None
}

const SELF_NARRATION: &[&str] = &[
    "je vais répondre",
    "je vais expliquer",
    "je vais décrire",
    "je vais présenter",
    "je vais donc",
    "i'm going to answer",
    "i am going to answer",
    "i will answer",
    "i will respond",
    "i will explain",
    "i'll answer",
    "i'll explain",
    "let me explain",
    "allow me to explain",
];

fn self_narration_boundary(lower: &str, pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    let before = lower[..pos].trim_end();
    before.is_empty()
        || before.ends_with('\n')
        || before.ends_with('.')
        || before.ends_with('!')
        || before.ends_with('?')
}

fn find_self_narration_cut(lower: &str) -> Option<usize> {
    let mut earliest = None::<usize>;
    for pat in SELF_NARRATION {
        let mut search = 0;
        while let Some(rel) = lower[search..].find(pat) {
            let pos = search + rel;
            if self_narration_boundary(lower, pos) {
                earliest = Some(earliest.map_or(pos, |e| e.min(pos)));
            }
            search = pos + 1;
        }
    }
    earliest
}

fn chat_self_narration_leak(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    SELF_NARRATION.iter().any(|p| lower.contains(p))
}

fn chat_sentence_is_meta_leak(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return true;
    }
    let lower = t.to_ascii_lowercase();
    chat_self_narration_leak(t) || chat_meta_substring_pos(&lower).is_some()
}

fn chat_meta_substring_pos(lower: &str) -> Option<usize> {
    const FORBIDDEN: &[&str] = &[
        "media.image.generate",
        "media.image.",
        "@agent-",
        "agent.spawn",
        "canvas.stroke",
        "canvas.rect",
        "canvas.ellipse",
        "canvas.undo",
        "canvas.get",
        "canvas.export",
        "module.scaffold",
        "tool.invoke",
        "tool invocation loop",
        "erreur bus",
        "internalerror",
        "args invalides",
        "missing field",
        "[canvas digest]",
        "canvas digest",
        "[canvas scene]",
        "canvas scene",
        "statut internal",
        "bbox=",
        "ok seq=",
        "next_seq=",
        "scene_bbox",
        "coords=normalized",
        "coin haut-gauche",
        "established rules",
        "established rules for the canvas tool",
        "no json is needed",
        "no json needed",
        "since this is a general question",
        "i will provide a concise",
        "<channel>",
    ];
    let mut earliest = None::<usize>;
    for f in FORBIDDEN {
        if let Some(i) = lower.find(f) {
            earliest = Some(earliest.map_or(i, |e| e.min(i)));
        }
    }
    if let Some(i) = find_canvas_tool_leak(lower) {
        earliest = Some(earliest.map_or(i, |e| e.min(i)));
    }
    for word in ["json", "rules", "règles"] {
        if let Some(i) = lower.find(word) {
            earliest = Some(earliest.map_or(i, |e| e.min(i)));
        }
    }
    const CHAIN_OF_THOUGHT: &[&str] = &[
        "orientant l'utilisateur",
        "orienting the user",
        "chain-of-thought",
        "thinking process",
        "selon les règles",
        "conformément aux règles",
        "conformément aux consignes",
    ];
    for p in CHAIN_OF_THOUGHT {
        if let Some(i) = lower.find(p) {
            earliest = Some(earliest.map_or(i, |e| e.min(i)));
        }
    }
    earliest
}

/// True when the agent kit includes session canvas drawing tools.
pub fn agent_is_canvas_draw(info: &AgentInfo) -> bool {
    info.tools.iter().any(|t| t.starts_with("canvas."))
}

/// True when canvas draw failure chrome should be suppressed (traits already on canvas).
pub fn canvas_draw_failure_muted(
    info: Option<&AgentInfo>,
    session_ops: Option<&[aos_proto::CanvasOp]>,
    trace: Option<&aos_proto::AgentTrace>,
) -> bool {
    let Some(a) = info else {
        return false;
    };
    if !agent_is_canvas_draw(a) {
        return false;
    }
    let overflow = a
        .fail_reason
        .as_deref()
        .is_some_and(aos_agent::context_budget::is_overflow_fail_reason);
    if overflow {
        return false;
    }
    aos_agent::canvas_scene::canvas_has_applied_traits(session_ops, trace)
}

/// True when a canvas draw agent hit max_steps but already applied traits — offer Continue.
pub fn canvas_draw_step_cap_continue(
    info: Option<&AgentInfo>,
    session_ops: Option<&[aos_proto::CanvasOp]>,
    trace: Option<&AgentTrace>,
) -> bool {
    let Some(a) = info else {
        return false;
    };
    if a.state != AgentState::Failed {
        return false;
    }
    if !agent_is_canvas_draw(a) {
        return false;
    }
    if a
        .fail_reason
        .as_deref()
        .is_some_and(aos_agent::context_budget::is_overflow_fail_reason)
    {
        return false;
    }
    if !aos_agent::canvas_scene::canvas_has_applied_traits(session_ops, trace) {
        return false;
    }
    a.fail_reason
        .as_deref()
        .is_some_and(aos_agent::context_budget::is_technical_max_steps_fail_reason)
}

/// Canvas draw agent stopped without context overflow — use locked chrome, not runtime strings.
/// Muted when the session canvas or agent trace already has applied trait ops.
pub fn canvas_draw_fail_chrome(
    info: Option<&AgentInfo>,
    session_ops: Option<&[aos_proto::CanvasOp]>,
    trace: Option<&aos_proto::AgentTrace>,
) -> bool {
    let Some(a) = info else {
        return false;
    };
    if a.state != AgentState::Failed {
        return false;
    }
    let overflow = a
        .fail_reason
        .as_deref()
        .is_some_and(aos_agent::context_budget::is_overflow_fail_reason);
    if !agent_is_canvas_draw(a) || overflow {
        return false;
    }
    !canvas_draw_failure_muted(info, session_ops, trace)
}

/// Localized fail reason for agent cards and lists (canvas draw uses locked copy).
pub fn resolve_visible_fail_reason(
    t: &crate::i18n::UiStrings,
    info: Option<&AgentInfo>,
    reason: &str,
    session_ops: Option<&[aos_proto::CanvasOp]>,
    trace: Option<&aos_proto::AgentTrace>,
) -> String {
    if canvas_draw_failure_muted(info, session_ops, trace) {
        return String::new();
    }
    if canvas_draw_fail_chrome(info, session_ops, trace) {
        return t.canvas_draw_failed.to_string();
    }
    crate::i18n::resolve_agent_fail_reason(t, Some(reason))
}

/// True when the action is a mutating canvas tool (not get/export).
pub fn is_canvas_draw_tool(action: &str) -> bool {
    action.starts_with("canvas.")
        && !matches!(action, "canvas.get" | "canvas.export")
}

/// True when a tool result looks like a runtime/parse failure.
pub fn canvas_tool_failed(tool_result: &str) -> bool {
    aos_agent::context_budget::looks_like_tool_failure(tool_result)
}

/// Human journal line for a canvas stroke: act label on success, hidden on per-stroke failure.
pub fn human_canvas_journal_result(
    action: &str,
    tool_result: &str,
    args: &serde_json::Value,
    t: &crate::i18n::UiStrings,
) -> Option<String> {
    if !is_canvas_draw_tool(action) {
        return None;
    }
    if canvas_tool_failed(tool_result) {
        return None;
    }
    Some(crate::agent_act_phrase::format_agent_act_phrase(t, action, args))
}

/// Truncate before the first forbidden meta substring (including mid-stream tokens).
fn truncate_before_chat_meta_leak(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut cut = chat_meta_substring_pos(&lower).unwrap_or(text.len());
    if let Some(i) = find_self_narration_cut(&lower) {
        cut = cut.min(i);
    }
    text[..cut].trim_end().to_string()
}

fn normalize_para_key(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

/// Collapse consecutive duplicate paragraphs (model often repeats the same block verbatim).
fn collapse_consecutive_duplicate_paragraphs(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut prev_key: Option<String> = None;
    for para in text.split("\n\n") {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }
        let key = normalize_para_key(para);
        if prev_key.as_deref() == Some(key.as_str()) {
            continue;
        }
        out.push(para.to_string());
        prev_key = Some(key);
    }
    out.join("\n\n")
}

/// Retire les fuites meta (identifiants outils, JSON, rules, raisonnement à voix haute).
pub fn sanitize_chat_visible_bubble(text: &str) -> String {
    let text = strip_chat_control_tokens(text.trim());
    let text = truncate_before_chat_meta_leak(&text);
    if text.is_empty() {
        return String::new();
    }
    let mut kept_paras = Vec::new();
    for para in text.split("\n\n") {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }
        let mut kept_sentences = Vec::new();
        for line in para.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut buf = String::new();
            for ch in line.chars() {
                buf.push(ch);
                if matches!(ch, '.' | '!' | '?') {
                    if !chat_sentence_is_meta_leak(&buf) {
                        kept_sentences.push(buf.trim().to_string());
                    }
                    buf.clear();
                }
            }
            if !buf.trim().is_empty() && !chat_sentence_is_meta_leak(&buf) {
                kept_sentences.push(buf.trim().to_string());
            }
        }
        if !kept_sentences.is_empty() {
            kept_paras.push(kept_sentences.join(" "));
        }
    }
    collapse_consecutive_duplicate_paragraphs(&kept_paras.join("\n\n")).trim().to_string()
}

/// Extrait la prose hors blocs JSON / TOOL: / DSML tool_call.
pub fn prose_without_json(response: &str) -> String {
    let mut out = strip_think_tags(response);
    out = aos_agent::actions::strip_tool_markup_tags(&out);
    if let Some(start) = out.find("```json") {
        if let Some(end_rel) = out[start + 7..].find("```") {
            let end = start + 7 + end_rel + 3;
            out = format!("{}{}", &out[..start], &out[end..]);
        }
    } else if let Some(start) = out.find('{') {
        // Retire le premier objet JSON équilibré
        if let Some(end) = find_json_object_end(&out[start..]) {
            out = format!("{}{}", &out[..start], &out[start + end..]);
        }
    }
    out.lines()
        .filter(|l| !l.trim().starts_with("TOOL:"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Clés d'args dont la valeur string est du contenu utilisateur (souvent markdown).
const CONTENT_ARG_KEYS: &[&str] = &[
    "content",
    "text",
    "body",
    "markdown",
    "summary",
    "message",
    "result",
    "answer",
];

fn looks_like_action_json(s: &str) -> bool {
    if aos_agent::actions::looks_like_tool_markup(s) {
        return true;
    }
    let t = s.trim_start();
    t.starts_with('{')
        && (t.contains("\"action\"") || t.contains("\"thought\""))
        && (t.contains("\"args\"") || t.contains("\"action\""))
}

fn pick_content_from_args(args: &serde_json::Value) -> Option<(Option<String>, String)> {
    for key in CONTENT_ARG_KEYS {
        if let Some(s) = args.get(*key).and_then(|v| v.as_str()) {
            let s = s.trim();
            if s.is_empty() {
                continue;
            }
            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty());
            return Some((title, s.to_string()));
        }
    }
    None
}

/// Affichage chat / timeline : interprète une action JSON en markdown lisible.
pub fn format_assistant_display(raw: &str) -> String {
    let cleaned = strip_think_tags(raw);
    if let Some(action) = aos_agent::actions::parse_action(&cleaned) {
        return format_action_as_markdown(&action, &prose_without_json(&cleaned));
    }
    let prose = prose_without_json(&cleaned);
    if !prose.is_empty() {
        return prose;
    }
    cleaned.trim().to_string()
}

fn format_action_as_markdown(action: &aos_agent::actions::AgentAction, outer_prose: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !outer_prose.is_empty() {
        parts.push(outer_prose.to_string());
    }
    if !action.thought.is_empty() {
        let already = parts.iter().any(|p| p.contains(&action.thought));
        if !already {
            parts.push(format!("_{}_", action.thought.trim()));
        }
    }
    if let Some((title, content)) = pick_content_from_args(&action.args) {
        if let Some(t) = title {
            parts.push(format!("## {t}\n\n{content}"));
        } else {
            parts.push(content);
        }
    } else {
        match action.action.as_str() {
            "goal.complete" | "noop" | "" => {}
            other => {
                let mut line = format!("✓ `{other}`");
                if let Some(path) = action.args.get("path").and_then(|v| v.as_str()) {
                    line.push_str(&format!(" → `{path}`"));
                }
                if let Some(q) = action
                    .args
                    .get("query")
                    .or_else(|| action.args.get("brief"))
                    .and_then(|v| v.as_str())
                {
                    line.push_str(&format!(" — {}", truncate(q, 120)));
                }
                parts.push(line);
            }
        }
    }
    let joined = parts.join("\n\n");
    if joined.trim().is_empty() {
        if action.action.is_empty() {
            String::new()
        } else {
            format!("`{}`", action.action)
        }
    } else {
        joined
    }
}

/// Affichage chat final : prose lisible sans fuites meta internes.
pub fn format_chat_assistant_display(raw: &str) -> String {
    sanitize_chat_visible_bubble(&format_assistant_display(raw))
}

/// Pendant le stream chat : évite JSON/outils bruts et fuites meta.
pub fn format_chat_streaming_preview(raw: &str) -> String {
    sanitize_chat_visible_bubble(&format_streaming_preview(raw))
}

/// Pendant le stream : évite d'afficher l'objet JSON brut incomplet.
pub fn format_streaming_preview(raw: &str) -> String {
    if aos_agent::actions::looks_like_tool_markup(raw) {
        return "…".into();
    }
    if !looks_like_action_json(raw) {
        return raw.to_string();
    }
    if let Some(action) = aos_agent::actions::parse_action(raw) {
        return format_action_as_markdown(&action, &prose_without_json(raw));
    }
    // JSON encore incomplet : extraire thought partiel si possible
    if let Some(thought) = extract_partial_json_string(raw, "thought") {
        return format!("_{thought}_");
    }
    "…".into()
}

fn extract_partial_json_string(hay: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let idx = hay.find(&needle)?;
    let after = &hay[idx + needle.len()..];
    let colon = after.find(':')?;
    let mut rest = after[colon + 1..].trim_start();
    if !rest.starts_with('"') {
        return None;
    }
    rest = &rest[1..];
    let mut out = String::new();
    let mut escape = false;
    for ch in rest.chars() {
        if escape {
            out.push(ch);
            escape = false;
            continue;
        }
        match ch {
            '\\' => escape = true,
            '"' => break,
            c => out.push(c),
        }
        if out.chars().count() > 280 {
            out.push('…');
            break;
        }
    }
    let out = out.trim().to_string();
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Contenu markdown à mettre en avant dans la timeline (args d'outil).
pub fn step_content_markdown(rec: &AgentStepRecord) -> Option<String> {
    if let Some((title, content)) = pick_content_from_args(&rec.args) {
        return Some(match title {
            Some(t) => format!("## {t}\n\n{content}"),
            None => content,
        });
    }
    let display = format_assistant_display(&rec.response);
    if !display.is_empty() && !looks_like_action_json(&display) {
        // Si response était du JSON, format_assistant_display a déjà extrait le fond
        if looks_like_action_json(&rec.response) {
            return Some(display);
        }
    }
    None
}

/// Carte compacte d'un agent lié au chat (état live via `agent.list`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChatCardAction {
    None,
    OpenDetail,
    TargetReply,
    Export,
    Retry,
    Continue,
}

#[allow(clippy::too_many_arguments)] // Card state remains explicit at this immediate-mode UI boundary.
pub fn chat_agent_card(
    ui: &mut Ui,
    info: Option<&AgentInfo>,
    agent_id: &str,
    title: &str,
    origin: &str,
    selected_for_reply: bool,
    session_ops: Option<&[aos_proto::CanvasOp]>,
    trace: Option<&aos_proto::AgentTrace>,
    t: &crate::i18n::UiStrings,
) -> ChatCardAction {
    let mut action = ChatCardAction::None;
    let canvas_muted = canvas_draw_failure_muted(info, session_ops, trace);
    let canvas_fail = canvas_draw_fail_chrome(info, session_ops, trace);
    let canvas_continue = canvas_draw_step_cap_continue(info, session_ops, trace);
    let (state_label, color, step, max_steps, task, fail, is_blocked) = if let Some(a) = info {
        (
            if canvas_fail || canvas_muted {
                String::new()
            } else {
                format!("{:?}", a.state)
            },
            state_color(&a.state),
            a.step,
            a.max_steps,
            a.current_task.clone().unwrap_or_default(),
            a.fail_reason.clone(),
            a.state == AgentState::Blocked,
        )
    } else {
        (
            "…".into(),
            Color32::GRAY,
            0,
            0,
            String::new(),
            None,
            false,
        )
    };
    let ask_card = origin == "ask" && is_blocked;
    let stroke_color = if selected_for_reply && ask_card {
        Color32::from_rgb(250, 190, 80)
    } else {
        color
    };
    let fill = if selected_for_reply && ask_card {
        Color32::from_rgb(48, 42, 28)
    } else {
        Color32::from_rgb(32, 36, 44)
    };
    egui::Frame::NONE
        .fill(fill)
        .stroke(egui::Stroke::new(
            if selected_for_reply && ask_card {
                2.0_f32
            } else {
                1.0_f32
            },
            stroke_color,
        ))
        .inner_margin(8.0)
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                icons::status_dot(ui, color);
                if !state_label.is_empty() {
                    ui.colored_label(color, RichText::new(state_label).strong());
                }
                let shown = if title.is_empty() {
                    agent_id
                } else {
                    title
                };
                ui.strong(truncate(shown, 64));
                ui.weak(agent_id);
                if max_steps > 0 && !canvas_fail && !canvas_muted {
                    ui.label(format!("step {step}/{max_steps}"));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(t.agent_export).small()).clicked() {
                        action = ChatCardAction::Export;
                    }
                    if ui.add(egui::Button::new(t.agent_detail).small()).clicked() {
                        action = ChatCardAction::OpenDetail;
                    }
                    if ask_card {
                        if selected_for_reply {
                            ui.colored_label(
                                Color32::from_rgb(240, 190, 100),
                                RichText::new(t.agent_reply_here).small(),
                            );
                        } else if ui.add(egui::Button::new(t.agent_reply).small()).clicked() {
                            action = ChatCardAction::TargetReply;
                        }
                    }
                });
            });
            let goal = if title.is_empty() {
                info.map(|a| a.directive.as_str()).unwrap_or("")
            } else {
                title
            };
            if !goal.is_empty() {
                ui.label(RichText::new(truncate(goal, 120)).italics());
            }
            if !task.is_empty() && !canvas_fail && !canvas_muted {
                ui.weak(format!("tâche : {}", truncate(&task, 80)));
            }
            if canvas_fail {
                ui.colored_label(
                    Color32::from_rgb(220, 90, 90),
                    RichText::new(t.canvas_draw_failed).strong(),
                );
                if ui.add(egui::Button::new(t.canvas_draw_retry).small()).clicked() {
                    action = ChatCardAction::Retry;
                }
            } else if canvas_continue {
                if ui.add(egui::Button::new(t.canvas_draw_continue).small()).clicked() {
                    action = ChatCardAction::Continue;
                }
            } else if let Some(reason) = fail {
                let visible = resolve_visible_fail_reason(t, info, reason.as_str(), session_ops, trace);
                if !visible.is_empty() {
                    ui.colored_label(Color32::from_rgb(220, 90, 90), truncate(&visible, 100));
                }
            }
        });
    action
}

fn find_json_object_end(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    for (i, ch) in s.char_indices() {
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
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn aggregate_sources(trace: &AgentTrace) -> Vec<AgentSource> {
    let mut out = Vec::new();
    for step in &trace.steps {
        for s in &step.sources {
            if !out.iter().any(|x: &AgentSource| {
                x.locator == s.locator && x.kind == s.kind
            }) {
                out.push(s.clone());
            }
        }
    }
    out
}

fn tool_kind_color(kind: &str) -> Color32 {
    match kind {
        "mcp" => Color32::from_rgb(100, 200, 180),
        "module" => Color32::from_rgb(120, 190, 230),
        "native" => Color32::from_rgb(140, 170, 220),
        "runtime" => Color32::from_rgb(200, 160, 120),
        _ => Color32::LIGHT_GRAY,
    }
}

fn card_frame(fill: Color32) -> egui::Frame {
    egui::Frame::NONE
        .fill(fill)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(8, 6))
}

#[allow(clippy::too_many_arguments)] // Detail-panel inputs are UI-local and intentionally explicit.
pub fn draw_agent_detail(
    ui: &mut Ui,
    info: Option<&AgentInfo>,
    trace: Option<&AgentTrace>,
    session_ops: Option<&[aos_proto::CanvasOp]>,
    steer_buf: &mut String,
    md_cache: &mut CommonMarkCache,
    open_in_browser: &dyn Fn(&str),
    t: &crate::i18n::UiStrings,
) -> PanelActions {
    let mut actions = PanelActions::default();
    let id = info
        .map(|a| a.agent_id.as_str())
        .or_else(|| trace.map(|t| t.agent_id.as_str()))
        .unwrap_or("?");

    if let Some(a) = info {
        ui.label(RichText::new(&a.directive).strong());
        let canvas_muted = canvas_draw_failure_muted(Some(a), session_ops, trace);
        ui.horizontal(|ui| {
            if !canvas_muted {
                ui.colored_label(state_color(&a.state), format!("{:?}", a.state));
                ui.separator();
            }
            if canvas_muted {
                ui.label(format!("tour {}", a.step));
            } else {
                ui.label(format!("tour {}/{}", a.step, a.max_steps));
            }
            let tokens = trace.map(|t| t.tokens_used).unwrap_or(a.tokens_used);
            ui.separator();
            ui.label(format!("{tokens} tok"));
            if let Some(complexity) = complexity_from_trace(trace) {
                ui.separator();
                let color = if complexity == "complex" {
                    Color32::from_rgb(230, 160, 60)
                } else {
                    Color32::from_rgb(100, 190, 120)
                };
                ui.colored_label(color, complexity);
            }
            if let Some(t) = trace {
                ui.separator();
                ui.label(fmt_ms(t.total_duration_ms));
            }
        });

        let fail = a
            .fail_reason
            .clone()
            .or_else(|| trace.and_then(|t| t.fail_reason.clone()));
        if a.state == AgentState::Blocked {
            card_frame(Color32::from_rgb(70, 55, 28)).show(ui, |ui| {
                ui.label(
                    RichText::new(t.agent_ask_user)
                        .color(Color32::from_rgb(240, 190, 100))
                        .strong(),
                );
                let q = if a.last_output.trim().is_empty() {
                    t.agent_ask_waiting
                } else {
                    a.last_output.trim()
                };
                ui.label(q);
            });
            ui.add_space(4.0);
        } else if matches!(a.state, AgentState::Failed | AgentState::Killed) || fail.is_some() {
            let overflow = fail
                .as_deref()
                .is_some_and(aos_agent::context_budget::is_overflow_fail_reason);
            let canvas_muted = canvas_draw_failure_muted(Some(a), session_ops, trace);
            let canvas_draw = canvas_draw_fail_chrome(Some(a), session_ops, trace);
            if !canvas_muted || overflow {
                card_frame(Color32::from_rgb(60, 30, 30)).show(ui, |ui| {
                    if overflow {
                        ui.label(
                            RichText::new(t.agent_could_not_continue)
                                .color(Color32::from_rgb(240, 140, 120))
                                .strong(),
                        );
                    } else if canvas_draw {
                        ui.label(
                            RichText::new(t.canvas_draw_failed)
                                .color(Color32::from_rgb(240, 140, 120))
                                .strong(),
                        );
                    } else {
                        ui.label(
                            RichText::new(t.agent_failure)
                                .color(Color32::from_rgb(240, 140, 120))
                                .strong(),
                        );
                        ui.label(
                            fail.map(|r| {
                                crate::i18n::resolve_agent_fail_reason(t, Some(r.as_str()))
                            })
                            .unwrap_or_else(|| t.agent_fail_unknown.into()),
                        );
                    }
                });
                ui.add_space(4.0);
            }
        }

        ui.horizontal(|ui| {
            if !a.is_roster() && !a.skills.is_empty() {
                ui.small(format!("skills: {}", a.skills.join(", ")));
            }
            if !a.is_roster() && !a.mcp_servers.is_empty() {
                ui.small(format!("MCP: {}", a.mcp_servers.join(", ")));
            }
        });
        if let Some(parent) = &a.parent_id {
            ui.small(format!("parent: {parent}"));
        }

        ui.horizontal(|ui| {
            match a.state {
                AgentState::Running => {
                    if ui.button(t.agent_pause).clicked() {
                        actions.pause = true;
                    }
                }
                AgentState::Paused | AgentState::Blocked => {
                    if ui.button(t.agent_resume).clicked() {
                        actions.resume = true;
                    }
                    if a.state == AgentState::Blocked {
                        ui.add(
                            egui::TextEdit::singleline(steer_buf)
                                .desired_width(180.0)
                                .hint_text(t.agent_reply_hint),
                        );
                        if ui.button(t.agent_reply).clicked() && !steer_buf.is_empty() {
                            actions.steer = Some(steer_buf.clone());
                        }
                    }
                }
                AgentState::Failed | AgentState::Killed => {
                    let overflow = a
                        .fail_reason
                        .as_deref()
                        .is_some_and(aos_agent::context_budget::is_overflow_fail_reason);
                    let canvas_muted = canvas_draw_failure_muted(Some(a), session_ops, trace);
                    let canvas_draw = canvas_draw_fail_chrome(Some(a), session_ops, trace);
                    let canvas_continue =
                        canvas_draw_step_cap_continue(Some(a), session_ops, trace);
                    if canvas_continue {
                        if ui.button(t.canvas_draw_continue).clicked() {
                            actions.continue_canvas = true;
                        }
                    } else if !canvas_muted {
                        let retry_label = if overflow || canvas_draw {
                            t.canvas_draw_retry
                        } else {
                            t.agent_retry_step
                        };
                        if ui.button(retry_label).clicked() {
                            actions.retry = true;
                        }
                    }
                }
                AgentState::Done => {
                    if ui.button(t.agent_retry).clicked() {
                        actions.retry = true;
                    }
                }
                AgentState::Created | AgentState::Roster => {}
            }
            if !matches!(a.state, AgentState::Killed | AgentState::Done | AgentState::Roster)
                && ui.button(t.agent_kill).clicked() {
                    actions.kill = true;
                }
            if ui.button(t.agent_export).clicked() {
                actions.export = true;
            }
            if a.state != AgentState::Blocked {
                ui.label(t.agent_steer);
                ui.add(
                    egui::TextEdit::singleline(steer_buf)
                        .desired_width(160.0)
                        .hint_text(t.agent_steer_hint),
                );
                if ui.button(t.agent_send).clicked() && !steer_buf.is_empty() {
                    actions.steer = Some(steer_buf.clone());
                }
            }
        });

        if !a.children.is_empty() {
            ui.separator();
            ui.label(RichText::new(t.agent_subagents).strong());
            ui.horizontal_wrapped(|ui| {
                for child in &a.children {
                    let child_info_label = child.clone();
                    ui.horizontal(|ui| {
                        icons::external_arrow(ui);
                        if ui
                            .add(
                                egui::Button::new(RichText::new(&child_info_label)
                                    .color(Color32::from_rgb(160, 200, 255)))
                                .frame(true),
                            )
                            .clicked()
                        {
                            actions.open_child = Some(child.clone());
                        }
                    });
                }
            });
        }
    } else {
        ui.weak(t.agent_partial_info.replace("{id}", id));
    }

    ui.separator();

    if let Some(t) = trace {
        let sources = aggregate_sources(t);
        if !sources.is_empty() {
            ui.collapsing(format!("Sources ({})", sources.len()), |ui| {
                for s in &sources {
                    ui.horizontal(|ui| {
                        ui.colored_label(
                            match s.kind.as_str() {
                                "web" => Color32::from_rgb(100, 180, 255),
                                "document" => Color32::from_rgb(180, 200, 120),
                                "fetch" => Color32::from_rgb(200, 160, 100),
                                _ => Color32::GRAY,
                            },
                            format!("[{}]", s.kind),
                        );
                        let label = if s.title.is_empty() {
                            s.locator.as_str()
                        } else {
                            s.title.as_str()
                        };
                        if s.kind == "web" || s.locator.starts_with("http") {
                            if ui.link(truncate(label, 60)).clicked() {
                                open_in_browser(&s.locator);
                            }
                        } else {
                            ui.label(truncate(label, 80));
                        }
                    });
                    if !s.snippet.is_empty() {
                        ui.small(truncate(&s.snippet, 160));
                    }
                }
            });
            ui.add_space(4.0);
        }
    }

    egui::ScrollArea::vertical()
        .id_salt(format!("agent_timeline_{id}"))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if let Some(tr) = trace {
                if tr.steps.is_empty() {
                    ui.weak(t.agent_no_turns);
                    if !tr.working_memory.is_empty() {
                        for (role, content) in &tr.working_memory {
                            if role == "system" && content.len() > 300 {
                                ui.collapsing(role.to_string(), |ui| {
                                    ui.small(content);
                                });
                            } else {
                                ui.label(
                                    RichText::new(format!(
                                        "{role}: {}",
                                        truncate(content, 300)
                                    ))
                                    .small(),
                                );
                            }
                        }
                    }
                } else {
                    let last = tr.steps.len().saturating_sub(1);
                    for (i, rec) in tr.steps.iter().enumerate() {
                        draw_step(ui, id, rec, i == last, md_cache, t, &mut actions);
                    }
                }
                if let Some(a) = info {
                    if matches!(a.state, AgentState::Running) && !a.last_output.is_empty() {
                        ui.separator();
                        ui.collapsing(t.agent_live_stream, |ui| {
                            ui.label(RichText::new(truncate(&a.last_output, 800)).small());
                        });
                    }
                }
            } else {
                ui.weak(t.agent_trace_loading);
            }
        });

    actions
}

fn draw_step(
    ui: &mut Ui,
    agent_id: &str,
    rec: &AgentStepRecord,
    default_open: bool,
    md_cache: &mut CommonMarkCache,
    t: &crate::i18n::UiStrings,
    actions: &mut PanelActions,
) {
    let header = format!(
        "Tour {} · {} · {} · {} tok",
        rec.step,
        fmt_ms(rec.duration_ms),
        rec.action,
        rec.prompt_tokens + rec.generated_tokens,
    );
    egui::CollapsingHeader::new(header)
        .id_salt(format!("step-{agent_id}-{}", rec.step))
        .default_open(default_open)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.small(format!("inférence {}", fmt_ms(rec.infer_ms)));
                ui.small(format!("outil {}", fmt_ms(rec.tool_ms)));
                if rec.tok_s > 0.0 {
                    ui.small(format!("{:.1} tok/s", rec.tok_s));
                }
                if let Some(task) = &rec.current_task {
                    ui.small(format!("tâche: {task}"));
                }
            });

            if let Some(reason) = &rec.fail_reason {
                let overflow = aos_agent::context_budget::is_overflow_fail_reason(reason);
                let text = crate::i18n::resolve_agent_fail_reason(t, Some(reason.as_str()));
                card_frame(Color32::from_rgb(55, 28, 28)).show(ui, |ui| {
                    ui.label(
                        RichText::new(if overflow {
                            text
                        } else {
                            format!("{} {}", t.agent_failure, text)
                        })
                        .color(Color32::from_rgb(240, 140, 120)),
                    );
                });
            }

            if !rec.thought.is_empty() {
                card_frame(Color32::from_rgb(40, 36, 55)).show(ui, |ui| {
                    ui.label(
                        RichText::new("Réflexion")
                            .color(Color32::from_rgb(180, 170, 220))
                            .small()
                            .strong(),
                    );
                    ui.label(RichText::new(&rec.thought).italics());
                });
            }

            let content_md = step_content_markdown(rec);
            let prose = prose_without_json(&rec.response);
            let show_prose = !prose.is_empty()
                && content_md
                    .as_ref()
                    .map(|c| !c.contains(prose.trim()))
                    .unwrap_or(true);

            if show_prose {
                card_frame(Color32::from_rgb(32, 40, 36)).show(ui, |ui| {
                    ui.label(
                        RichText::new("Réponse")
                            .color(Color32::from_rgb(160, 210, 170))
                            .small()
                            .strong(),
                    );
                    ui.push_id(("step_prose", agent_id, rec.step), |ui| {
                        CommonMarkViewer::new().show(ui, md_cache, &prose);
                    });
                });
            }

            if let Some(md) = &content_md {
                card_frame(Color32::from_rgb(28, 38, 32)).show(ui, |ui| {
                    ui.label(
                        RichText::new("Contenu")
                            .color(Color32::from_rgb(150, 220, 180))
                            .small()
                            .strong(),
                    );
                    ui.push_id(("step_content", agent_id, rec.step), |ui| {
                        CommonMarkViewer::new().show(ui, md_cache, md);
                    });
                });
            }

            if looks_like_action_json(&rec.response) {
                ui.collapsing("JSON brut", |ui| {
                    ui.monospace(truncate(&rec.response, 3000));
                });
            } else if !rec.response.is_empty()
                && content_md.is_none()
                && prose.is_empty()
                && (rec.response.contains('{') || rec.response.contains("```"))
            {
                ui.collapsing("JSON brut", |ui| {
                    ui.monospace(truncate(&rec.response, 3000));
                });
            }

            if rec.action == "agent.spawn" {
                let child = rec
                    .child_id
                    .clone()
                    .or_else(|| {
                        rec.tool_result
                            .strip_prefix("sous-agent créé: ")
                            .map(|s| s.trim().to_string())
                    });
                let brief = rec
                    .args
                    .get("brief")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let spawn_ok = child.as_ref().is_some_and(|c| !c.is_empty() && c != "?");
                let fail_msg = if spawn_ok {
                    None
                } else if !rec.tool_result.is_empty() {
                    Some(rec.tool_result.clone())
                } else {
                    Some("spawn non lancé (brief/args manquants ?)".into())
                };
                card_frame(if spawn_ok {
                    Color32::from_rgb(30, 45, 60)
                } else {
                    Color32::from_rgb(55, 32, 32)
                })
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(if spawn_ok {
                            "Sous-agent spawn"
                        } else {
                            "Sous-agent spawn — échec"
                        })
                        .color(if spawn_ok {
                            Color32::from_rgb(140, 190, 255)
                        } else {
                            Color32::from_rgb(240, 140, 140)
                        })
                        .strong(),
                    );
                    if !brief.is_empty() {
                        ui.small(&brief);
                    }
                    if let Some(msg) = fail_msg {
                        ui.small(RichText::new(truncate(&msg, 240)).color(Color32::from_rgb(220, 160, 160)));
                    }
                    if let Some(child) = child.filter(|c| !c.is_empty() && c != "?") {
                        if ui
                            .button(
                                RichText::new(format!("Ouvrir {child}"))
                                    .color(Color32::from_rgb(160, 200, 255)),
                            )
                            .clicked()
                        {
                            actions.open_child = Some(child);
                        }
                    }
                });
            } else if rec.action == "agent.await" {
                card_frame(Color32::from_rgb(50, 42, 30)).show(ui, |ui| {
                    ui.label(
                        RichText::new("Attente sous-agent")
                            .color(Color32::from_rgb(230, 180, 100))
                            .strong(),
                    );
                    if let Some(c) = &rec.child_id {
                        if ui.link(format!("attendre {c}")).clicked() {
                            actions.open_child = Some(c.clone());
                        }
                    }
                    if !rec.tool_result.is_empty() {
                        ui.small(truncate(&rec.tool_result, 200));
                    }
                });
            } else if rec.action != "noop" {
                card_frame(Color32::from_rgb(28, 36, 48)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(
                            tool_kind_color(&rec.tool_kind),
                            RichText::new(format!("{} · {}", rec.action, rec.tool_kind)).strong(),
                        );
                        if let Some(s) = &rec.skill {
                            ui.label(format!("skill: {s}"));
                        }
                        if let Some(m) = &rec.mcp_server {
                            ui.label(format!("MCP: {m}"));
                        }
                    });
                    if let Some(display) =
                        human_canvas_journal_result(&rec.action, &rec.tool_result, &rec.args, t)
                    {
                        ui.label(RichText::new(display).small());
                    } else if !rec.tool_result.is_empty() && !is_canvas_draw_tool(&rec.action) {
                        let tr = truncate(&rec.tool_result, 1200);
                        if tr.contains('#') || tr.contains("**") || tr.contains('\n') {
                            ui.push_id(("step_tool", agent_id, rec.step), |ui| {
                                CommonMarkViewer::new().show(ui, md_cache, &tr);
                            });
                        } else {
                            ui.label(RichText::new(tr).small());
                        }
                    }
                    let mut args = rec.args.clone();
                    if let Some(obj) = args.as_object_mut() {
                        for k in CONTENT_ARG_KEYS {
                            obj.remove(*k);
                        }
                    }
                    let args_s = serde_json::to_string_pretty(&args)
                        .unwrap_or_else(|_| args.to_string());
                    if args_s != "null" && args_s != "{}" && args_s != "[]" {
                        ui.collapsing("Arguments", |ui| {
                            ui.monospace(&args_s);
                        });
                    }
                });
            }

            if !rec.sources.is_empty() {
                ui.small(format!(
                    "sources: {}",
                    rec.sources
                        .iter()
                        .map(|s| {
                            if s.title.is_empty() {
                                s.locator.as_str()
                            } else {
                                s.title.as_str()
                            }
                        })
                        .take(4)
                        .collect::<Vec<_>>()
                        .join(" · ")
                ));
            }

            if let Some(r) = &rec.reflection {
                let r = strip_think_tags(r);
                if !r.is_empty() {
                    card_frame(Color32::from_rgb(48, 40, 28)).show(ui, |ui| {
                        ui.label(
                            RichText::new("Critique / reflect")
                                .color(Color32::from_rgb(220, 180, 120))
                                .small()
                                .strong(),
                        );
                        ui.label(RichText::new(r).italics());
                    });
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_notes_create_json_as_markdown() {
        let raw = serde_json::json!({
            "thought": "Je complète le guide.",
            "action": "notes.create",
            "args": {
                "title": "Guide CPU/GPU",
                "content": "# Intro\n\n- **INT4**\n- table"
            }
        })
        .to_string();
        let out = format_assistant_display(&raw);
        assert!(out.contains("Guide CPU/GPU"), "{out}");
        assert!(out.contains("**INT4**"), "{out}");
        assert!(!out.contains("\"action\""), "{out}");
        assert!(out.contains("Je complète"), "{out}");
    }

    #[test]
    fn streaming_hides_dsml_tool_call_markup() {
        let raw = r#"<｜DSML｜tool_call><tool_call>{"name":"canvas.rect","arguments":{"x":0.2}}</tool_call>"#;
        let prev = format_streaming_preview(raw);
        assert!(!prev.contains("DSML"), "{prev}");
        assert!(!prev.contains("tool_call"), "{prev}");
        assert!(!prev.contains("canvas.rect"), "{prev}");
    }

    #[test]
    fn streaming_hides_partial_json() {
        let partial =
            "{\"thought\":\"En cours\",\"action\":\"notes.create\",\"args\":{\"content\":\"# Hi\"";
        let prev = format_streaming_preview(partial);
        assert!(!prev.contains("\"action\""), "{prev}");
        assert!(prev.contains("En cours") || prev == "…", "{prev}");
    }

    #[test]
    fn plain_markdown_passthrough() {
        let md = "## Hello\n\nWorld";
        assert_eq!(format_assistant_display(md), md);
    }

    #[test]
    fn canvas_ui_answer_strips_tool_ids_and_meta() {
        let raw = r#"Le Canvas est un panneau vectoriel pour dessiner au trait.

Selon les established rules for the Canvas tool, media.image.generate is used when closed. No JSON is needed as there is no spawn.

Le canvas sert aux esquisses vectorielles."#;
        let out = format_chat_assistant_display(raw);
        let lower = out.to_ascii_lowercase();
        assert!(!lower.contains("media.image.generate"), "{out}");
        assert!(!out.contains("@agent-"), "{out}");
        assert!(!lower.contains("json"), "{out}");
        assert!(!lower.contains("rules"), "{out}");
        assert!(out.contains("vectoriel"), "{out}");
    }

    #[test]
    fn chat_streaming_preview_strips_trailing_meta() {
        let raw = r#"Le Canvas permet des esquisses vectorielles. Je vais donc répondre en orientant l'utilisateur vers media.image.generate. No JSON is needed."#;
        let out = format_chat_streaming_preview(raw);
        let lower = out.to_ascii_lowercase();
        assert!(!lower.contains("media.image.generate"), "{out}");
        assert!(!lower.contains("json"), "{out}");
        assert!(!lower.contains("rules"), "{out}");
        assert!(out.contains("vectoriel"), "{out}");
    }

    #[test]
    fn streaming_truncates_mid_token_before_tool_id() {
        let raw = "Le Canvas est un panneau vectoriel. Selon media.image.generate";
        let out = format_chat_streaming_preview(raw);
        assert!(!out.to_ascii_lowercase().contains("media.image"), "{out}");
        assert!(out.contains("vectoriel"), "{out}");
    }

    #[test]
    fn streaming_truncates_before_rules_and_no_json() {
        let raw =
            "Bonne réponse. established rules for the Canvas tool. No JSON is needed as there";
        let out = format_chat_streaming_preview(raw);
        let lower = out.to_ascii_lowercase();
        assert!(!lower.contains("rules"), "{out}");
        assert!(!lower.contains("json"), "{out}");
        assert!(out.contains("Bonne réponse"), "{out}");
    }

    #[test]
    fn streaming_truncates_chain_of_thought_before_it_streams() {
        let raw = "Le Canvas sert au trait vectoriel. Je vais donc répondre en orientant";
        let out = format_chat_streaming_preview(raw);
        assert!(!out.to_ascii_lowercase().contains("je vais donc"), "{out}");
        assert!(out.contains("vectoriel"), "{out}");
    }

    #[test]
    fn session26_canvas_leak_keeps_only_human_answer() {
        let raw = r#"l'agent va ouvrir le panneau s'il est fermé et utiliser des fonctions comme `canvas.stroke` ou `canvas.rect` … (`media.image.generate`) pour créer une image.
Since this is a general question about how it works, I will provide a concise, natural language explanation based on the established rules for the Canvas tool. No JSON is needed as there is no tool invocation loop in this chat session.<channel>Le Canvas est un outil qui permet le dessin vectoriel. Tu peux y dessiner au trait lorsque le panneau est ouvert."#;
        let out = format_chat_assistant_display(raw);
        let lower = out.to_ascii_lowercase();
        assert!(!out.contains("canvas.stroke"), "{out}");
        assert!(!out.contains("canvas.rect"), "{out}");
        assert!(!lower.contains("media.image.generate"), "{out}");
        assert!(!lower.contains("json"), "{out}");
        assert!(!lower.contains("rules"), "{out}");
        assert!(!lower.contains("tool invocation loop"), "{out}");
        assert!(!lower.contains("<channel>"), "{out}");
        assert!(!lower.contains("Since this is"), "{out}");
        assert!(out.contains("dessin vectoriel"), "{out}");
    }

    #[test]
    fn session32_canvas_strips_trailing_french_planning() {
        let raw = r#"Le Canvas est un panneau vectoriel où vous pouvez dessiner. Si vous voulez dessiner avec un marqueur spécifique, vous devez explicitement le mentionner (par exemple, "sur le canvas"). Si vous ne le mentionnez pas, le dessin sera généré comme une image.

Je vais répondre de manière naturelle et concise, en expliquant le concept humain du Canvas."#;
        let out = format_chat_assistant_display(raw);
        let lower = out.to_ascii_lowercase();
        assert!(out.contains("panneau vectoriel"), "{out}");
        assert!(!lower.contains("je vais répondre"), "{out}");
        assert!(!lower.contains("concept humain"), "{out}");
        assert!(!lower.contains("let me explain"), "{out}");
    }

    #[test]
    fn session32_streaming_truncates_before_planning_sentence() {
        let raw = r#"Le Canvas est un panneau vectoriel où vous pouvez dessiner.

Je vais répondre de manière naturelle"#;
        let out = format_chat_streaming_preview(raw);
        assert!(out.contains("panneau vectoriel"), "{out}");
        assert!(!out.to_ascii_lowercase().contains("je vais répondre"), "{out}");
    }

    #[test]
    fn session33_collapses_verbatim_duplicate_paragraph() {
        let para = "Le Canvas est un panneau vectoriel où vous pouvez dessiner ou esquisser. Si vous le dessinez en utilisant un marqueur spécifique, les traits apparaissent sur ce panneau. Si vous le dessinez sans marqueur et que le panneau est fermé, l'agent utilisera un outil de génération d'images pour créer le dessin.";
        let raw = format!("{para}\n\n{para}");
        let out = format_chat_assistant_display(&raw);
        assert_eq!(out, para, "{out}");
        assert!(out.contains("outil de génération d'images"), "{out}");
        assert_eq!(out.matches("panneau vectoriel").count(), 1, "{out}");
    }

    #[test]
    fn collapse_keeps_distinct_consecutive_paragraphs() {
        let raw = "Premier paragraphe.\n\nDeuxième paragraphe différent.";
        let out = sanitize_chat_visible_bubble(raw);
        assert!(out.contains("Premier"), "{out}");
        assert!(out.contains("Deuxième"), "{out}");
    }

    #[test]
    fn locked_canvas_draw_failure_copy_fr_en() {
        let t_en = crate::i18n::strings("en");
        let t_fr = crate::i18n::strings("fr");
        assert_eq!(t_en.canvas_draw_failed, "Couldn't draw.");
        assert_eq!(t_fr.canvas_draw_failed, "Impossible de dessiner.");
        assert_eq!(t_en.canvas_draw_retry, "Try again");
        assert_eq!(t_fr.canvas_draw_retry, "Réessayer");
        assert_eq!(t_en.canvas_draw_continue, "Continue");
        assert_eq!(t_fr.canvas_draw_continue, "Continuer");
        assert!(!t_en.canvas_draw_failed.contains("InternalError"));
        assert!(!t_fr.canvas_draw_failed.contains("digest"));
        assert!(!t_en.canvas_draw_failed.contains('{'));
    }

    fn canvas_draw_agent_info(agent_id: &str) -> AgentInfo {
        AgentInfo {
            agent_id: agent_id.into(),
            state: AgentState::Failed,
            directive: "dessine un moulin".into(),
            pid: None,
            caps: vec![],
            last_output: String::new(),
            step: 64,
            max_steps: 64,
            current_task: None,
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: vec![],
            tools: vec!["canvas.spline".into(), "canvas.rect".into()],
            mcp_servers: vec![],
            fail_reason: Some("max_steps (64) atteint".into()),
            session_id: Some("sess-1".into()),
            title: String::new(),
            kind: AgentKind::Task,
            display_name: None,
            persona_id: None,
            origin: None,
        }
    }

    #[test]
    fn canvas_draw_step_cap_continue_when_traits_on_session() {
        let info = canvas_draw_agent_info("agent-102");
        let ops = vec![aos_proto::CanvasOp {
            seq: 1,
            author_id: "agent-102".into(),
            ts_ms: 0,
            layer_id: String::new(),
            body: aos_proto::CanvasOpBody::Rect {
                x: 0.1,
                y: 0.2,
                w: 0.3,
                h: 0.4,
                color: "#3ee0c4".into(),
                width: 0.01,
                fill: false,
                rotation: 0.0,
                    opacity: 1.0, dash: vec![], gradient: None
            },
        }];
        assert!(canvas_draw_step_cap_continue(Some(&info), Some(&ops), None));
        assert!(canvas_draw_failure_muted(Some(&info), Some(&ops), None));
        assert!(!canvas_draw_fail_chrome(Some(&info), Some(&ops), None));
    }

    #[test]
    fn canvas_draw_step_cap_continue_false_on_empty_canvas() {
        let info = canvas_draw_agent_info("agent-90");
        assert!(!canvas_draw_step_cap_continue(Some(&info), None, None));
        assert!(canvas_draw_fail_chrome(Some(&info), None, None));
    }

    #[test]
    fn resolve_visible_fail_reason_hides_max_steps_for_canvas_draw() {
        let t_en = crate::i18n::strings("en");
        let t_fr = crate::i18n::strings("fr");
        let info = AgentInfo {
            agent_id: "agent-90".into(),
            state: AgentState::Failed,
            directive: "dessine un moulin".into(),
            pid: None,
            caps: vec![],
            last_output: String::new(),
            step: 64,
            max_steps: 64,
            current_task: None,
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: vec![],
            tools: vec!["canvas.stroke".into(), "canvas.rect".into()],
            mcp_servers: vec![],
            fail_reason: Some("max_steps (64) atteint".into()),
            session_id: None,
            title: String::new(),
            kind: AgentKind::Task,
            display_name: None,
            persona_id: None,
            origin: None,
        };
        assert!(canvas_draw_fail_chrome(Some(&info), None, None));
        let raw = "max_steps (64) atteint";
        let en = resolve_visible_fail_reason(&t_en, Some(&info), raw, None, None);
        let fr = resolve_visible_fail_reason(&t_fr, Some(&info), raw, None, None);
        assert_eq!(en, t_en.canvas_draw_failed);
        assert_eq!(fr, t_fr.canvas_draw_failed);
        assert!(!en.contains("max_steps"));
        assert!(!fr.contains("atteint"));
        assert!(!en.contains("Failed"));
    }

    #[test]
    fn canvas_draw_max_steps_muted_when_session_has_traits() {
        let t_en = crate::i18n::strings("en");
        let info = AgentInfo {
            agent_id: "agent-99".into(),
            state: AgentState::Failed,
            directive: "dessine un moulin".into(),
            pid: None,
            caps: vec![],
            last_output: String::new(),
            step: 64,
            max_steps: 64,
            current_task: None,
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: vec![],
            tools: vec!["canvas.spline".into(), "canvas.rect".into()],
            mcp_servers: vec![],
            fail_reason: Some("max_steps (64) atteint".into()),
            session_id: None,
            title: String::new(),
            kind: AgentKind::Task,
            display_name: None,
            persona_id: None,
            origin: None,
        };
        let ops = vec![aos_proto::CanvasOp {
            seq: 1,
            author_id: "agent-99".into(),
            ts_ms: 0,
            layer_id: String::new(),
            body: aos_proto::CanvasOpBody::Rect {
                x: 0.1,
                y: 0.2,
                w: 0.3,
                h: 0.4,
                color: String::new(),
                fill: false,
                width: 0.0,
                rotation: 0.0,
                    opacity: 1.0, dash: vec![], gradient: None
            },
        }];
        assert!(!canvas_draw_fail_chrome(Some(&info), Some(&ops), None));
        let visible = resolve_visible_fail_reason(
            &t_en,
            Some(&info),
            "max_steps (64) atteint",
            Some(&ops),
            None,
        );
        assert!(visible.is_empty());
    }

    #[test]
    fn canvas_draw_max_steps_muted_when_trace_has_traits() {
        let t_en = crate::i18n::strings("en");
        let info = AgentInfo {
            agent_id: "agent-99".into(),
            state: AgentState::Failed,
            directive: "dessine un moulin".into(),
            pid: None,
            caps: vec![],
            last_output: String::new(),
            step: 64,
            max_steps: 64,
            current_task: None,
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: vec![],
            tools: vec!["canvas.spline".into(), "canvas.rect".into()],
            mcp_servers: vec![],
            fail_reason: Some("max_steps (64) atteint".into()),
            session_id: None,
            title: String::new(),
            kind: AgentKind::Task,
            display_name: None,
            persona_id: None,
            origin: None,
        };
        let trace = AgentTrace {
            agent_id: "agent-99".into(),
            steps: vec![AgentStepRecord {
                step: 1,
                action: "canvas.spline".into(),
                tool_result: "ok seq=1".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(!canvas_draw_fail_chrome(Some(&info), None, Some(&trace)));
        let visible = resolve_visible_fail_reason(
            &t_en,
            Some(&info),
            "max_steps (64) atteint",
            None,
            Some(&trace),
        );
        assert!(visible.is_empty());
    }

    #[test]
    fn resolve_visible_fail_reason_hides_max_steps_for_other_agents() {
        let t_en = crate::i18n::strings("en");
        let info = AgentInfo {
            agent_id: "agent-1".into(),
            state: AgentState::Failed,
            directive: "write a note".into(),
            pid: None,
            caps: vec![],
            last_output: String::new(),
            step: 64,
            max_steps: 64,
            current_task: None,
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: vec![],
            tools: vec!["notes.create".into()],
            mcp_servers: vec![],
            fail_reason: Some("max_steps (64) atteint".into()),
            session_id: None,
            title: String::new(),
            kind: AgentKind::Task,
            display_name: None,
            persona_id: None,
            origin: None,
        };
        let raw = "max_steps (64) atteint";
        let en = resolve_visible_fail_reason(&t_en, Some(&info), raw, None, None);
        assert_eq!(en, t_en.agent_could_not_continue);
        assert!(!en.contains("max_steps"));
        assert!(!en.contains("atteint"));
    }

    #[test]
    fn human_canvas_journal_hides_per_stroke_failure() {
        let t = crate::i18n::strings("en");
        let raw = "ERREUR bus: statut InternalError: args invalides: missing field x";
        let out = human_canvas_journal_result("canvas.ellipse", raw, &serde_json::json!({}), &t);
        assert!(out.is_none());
    }

    #[test]
    fn human_canvas_journal_success_is_act_label_not_bbox() {
        let t = crate::i18n::strings("en");
        let raw = "ok seq=2 ellipse bbox=(0.350,0.150)-(0.650,0.270)";
        let out = human_canvas_journal_result("canvas.ellipse", raw, &serde_json::json!({}), &t)
            .expect("act label");
        assert_eq!(out, t.agent_act_canvas_ellipse);
        assert!(!out.contains("bbox"));
        assert!(!out.contains("seq="));
    }

    #[test]
    fn chat_bubble_strips_bbox_echo_leak() {
        let raw = "ok seq=12 ellipse bbox=(0.350,0.150)-(0.650,0.270)";
        let out = sanitize_chat_visible_bubble(raw);
        assert!(out.is_empty() || !out.contains("bbox="), "{out}");
    }

    #[test]
    fn chat_bubble_strips_canvas_scene_caption() {
        let raw = "Trait appliqué.\n\n[canvas scene] Capture PNG du canvas actuel jointe au prochain tour";
        let out = sanitize_chat_visible_bubble(raw);
        assert!(out.contains("Trait appliqué"), "{out}");
        assert!(!out.to_ascii_lowercase().contains("canvas scene"), "{out}");
        assert!(!out.contains("[canvas scene]"), "{out}");
    }

    #[test]
    fn chat_bubble_strips_bus_error_leak() {
        let raw = "Dessin en cours. ERREUR bus: statut InternalError: args invalides.";
        let out = sanitize_chat_visible_bubble(raw);
        assert!(!out.to_ascii_lowercase().contains("erreur bus"), "{out}");
        assert!(!out.to_ascii_lowercase().contains("internalerror"), "{out}");
        assert!(out.contains("Dessin en cours"), "{out}");
    }
}
