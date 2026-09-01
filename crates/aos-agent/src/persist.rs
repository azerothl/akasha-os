//! Persistance agents : `var/agents/<id>/spec.json` + `state.json` + registry.

use aos_proto::{AgentInfo, AgentKind, AgentSpec, AgentState, AgentTrace};
use std::path::{Path, PathBuf};

use crate::CognitiveState;

pub fn agents_root() -> PathBuf {
    PathBuf::from("var/agents")
}

pub fn agent_dir(agent_id: &str) -> PathBuf {
    agents_root().join(agent_id)
}

pub fn ensure_agent_dir(agent_id: &str) -> std::io::Result<PathBuf> {
    let d = agent_dir(agent_id);
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

pub fn write_spec(spec: &AgentSpec) -> std::io::Result<PathBuf> {
    let dir = ensure_agent_dir(&spec.agent_id)?;
    let path = dir.join("spec.json");
    std::fs::write(&path, serde_json::to_string_pretty(spec).unwrap())?;
    Ok(path)
}

pub fn read_spec(agent_id: &str) -> Option<AgentSpec> {
    let path = agent_dir(agent_id).join("spec.json");
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn write_state(state: &CognitiveState) -> std::io::Result<()> {
    let dir = ensure_agent_dir(&state.agent_id)?;
    std::fs::write(dir.join("state.json"), state.to_json().unwrap())?;
    Ok(())
}

pub fn read_state(agent_id: &str) -> Option<CognitiveState> {
    let path = agent_dir(agent_id).join("state.json");
    let raw = std::fs::read_to_string(path).ok()?;
    CognitiveState::from_json(&raw).ok()
}

/// Journal des tours (mémoire + spec disque).
pub fn load_trace(agent_id: &str) -> AgentTrace {
    let spec = read_spec(agent_id);
    let state = read_state(agent_id);
    assemble_trace(agent_id, spec.as_ref(), state.as_ref(), None)
}

/// Assemble un `AgentTrace` depuis spec / état cognitif / tours déjà en mémoire.
pub fn assemble_trace(
    agent_id: &str,
    spec: Option<&AgentSpec>,
    state: Option<&CognitiveState>,
    live_steps: Option<&[aos_proto::AgentStepRecord]>,
) -> AgentTrace {
    let steps = live_steps
        .map(|s| s.to_vec())
        .or_else(|| state.map(|s| s.trace.clone()))
        .unwrap_or_default();
    let tokens_used = live_steps
        .map(|s| s.iter().map(|x| x.generated_tokens as u64).sum())
        .or_else(|| state.map(|s| s.tokens_used))
        .unwrap_or(0);
    let fail_reason = steps
        .iter()
        .rev()
        .find_map(|s| s.fail_reason.clone())
        .or_else(|| {
            std::fs::read_to_string(agent_dir(agent_id).join("info.json"))
                .ok()
                .and_then(|s| serde_json::from_str::<AgentInfo>(&s).ok())
                .and_then(|i| i.fail_reason)
        });
    AgentTrace {
        agent_id: agent_id.to_string(),
        tokens_used,
        total_duration_ms: steps.iter().map(|s| s.duration_ms).sum(),
        skills: spec.map(|s| s.skills.clone()).unwrap_or_default(),
        tools: spec.map(|s| s.tools.clone()).unwrap_or_default(),
        mcp_servers: spec.map(|s| s.mcp_servers.clone()).unwrap_or_default(),
        reflections: state.map(|s| s.reflections.clone()).unwrap_or_default(),
        working_memory: state.map(|s| s.working_memory.clone()).unwrap_or_default(),
        steps,
        fail_reason,
    }
}

/// True when `fail_reason` must not appear verbatim in shared markdown export.
fn is_technical_export_fail_reason(reason: &str) -> bool {
    crate::context_budget::is_overflow_fail_reason(reason)
        || crate::context_budget::is_technical_vision_infer_error(reason)
        || crate::context_budget::is_technical_max_steps_fail_reason(reason)
        || {
            let lower = reason.to_ascii_lowercase();
            lower.contains("internalerror")
                || lower.contains("prompttoolong")
                || lower.contains("prompt too long")
                || reason.contains("ctx=")
                || reason.contains("réserve_gen=")
                || reason.contains("Échec")
                || reason.contains("échec")
        }
}

fn agent_is_canvas_draw(info: &AgentInfo) -> bool {
    info.tools.iter().any(|t| t.starts_with("canvas."))
}

/// Localized `fail_reason` for shared markdown export (no sentinels or token math).
pub fn export_fail_reason(
    lang: &str,
    reason: &str,
    info: Option<&AgentInfo>,
    trace: Option<&AgentTrace>,
    session_ops: Option<&[aos_proto::CanvasOp]>,
) -> String {
    let en = lang.eq_ignore_ascii_case("en");
    let overflow = crate::context_budget::is_overflow_fail_reason(reason);
    let canvas_draw = info.is_some_and(|a| agent_is_canvas_draw(a) && !overflow);
    if canvas_draw {
        if crate::canvas_scene::canvas_has_applied_traits(session_ops, trace) {
            return String::new();
        }
        return if en {
            "Couldn't draw.".into()
        } else {
            "Impossible de dessiner.".into()
        };
    }
    if reason == crate::actions::THREAD_FAIL_COULD_NOT_ACT {
        return if en {
            "The agent could not act.".into()
        } else {
            "L'agent n'a pas pu agir.".into()
        };
    }
    if reason == crate::actions::THREAD_FAIL_COULD_NOT_CONTINUE
        || is_technical_export_fail_reason(reason)
    {
        return if en {
            "Couldn't keep going.".into()
        } else {
            "Impossible de continuer.".into()
        };
    }
    if reason.trim().is_empty() {
        return if en {
            "reason not recorded".into()
        } else {
            "motif non renseigné".into()
        };
    }
    reason.to_string()
}

/// Export lisible d'un journal agent (`agent.trace` / `state.json`) en markdown.
pub fn export_trace_markdown(
    trace: &AgentTrace,
    info: Option<&AgentInfo>,
    lang: &str,
    session_ops: Option<&[aos_proto::CanvasOp]>,
) -> String {
    let title = info
        .map(|a| a.title.as_str())
        .filter(|s| !s.trim().is_empty());
    let display_name = info
        .and_then(|a| a.display_name.as_deref())
        .filter(|s| !s.trim().is_empty());
    let session_id = info
        .and_then(|a| a.session_id.as_deref())
        .filter(|s| !s.trim().is_empty());
    let directive = info
        .map(|a| a.directive.as_str())
        .filter(|s| !s.trim().is_empty());

    let mut out = format!("# Agent {}\n\n", trace.agent_id);
    if let Some(t) = title {
        out.push_str(&format!("_Title: {t}_\n\n"));
    }
    if let Some(name) = display_name {
        out.push_str(&format!("_Display name: {name}_\n\n"));
    }
    if let Some(sid) = session_id {
        out.push_str(&format!("_Session: `{sid}`_\n\n"));
    }
    if let Some(goal) = directive {
        out.push_str(&format!("_{goal}_\n\n"));
    }
    if let Some(reason) = trace
        .fail_reason
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        let visible = export_fail_reason(lang, reason, info, Some(trace), session_ops);
        if !visible.is_empty() {
            out.push_str(&format!("**Fail reason:** {visible}\n\n"));
        }
    }

    if trace.steps.is_empty() {
        if !trace.working_memory.is_empty() {
            out.push_str("## Working memory\n\n");
            for (role, content) in &trace.working_memory {
                out.push_str(&format!("### {role}\n\n{content}\n\n"));
            }
        }
        return out;
    }

    for rec in &trace.steps {
        out.push_str(&format!("## Step {}\n\n", rec.step));
        if let Some(task) = rec.current_task.as_deref().filter(|s| !s.is_empty()) {
            out.push_str(&format!("_Task: {task}_\n\n"));
        }
        if !rec.thought.trim().is_empty() {
            out.push_str("### Thought\n\n");
            out.push_str(rec.thought.trim());
            out.push_str("\n\n");
        }
        if !rec.response.trim().is_empty() {
            out.push_str("### Response\n\n");
            out.push_str(rec.response.trim());
            out.push_str("\n\n");
        }
        if !rec.action.trim().is_empty() {
            out.push_str("### Action\n\n");
            out.push_str(&format!("`{}`\n\n", rec.action.trim()));
        }
        let arg_lines = export_step_args(&rec.args);
        if !arg_lines.is_empty() {
            out.push_str("### Args\n\n");
            for line in arg_lines {
                out.push_str(&format!("- {line}\n"));
            }
            out.push('\n');
        }
        if !rec.tool_result.trim().is_empty() {
            out.push_str("### Tool result\n\n");
            out.push_str(rec.tool_result.trim());
            out.push_str("\n\n");
        }
        if let Some(child) = rec.child_id.as_deref().filter(|s| !s.is_empty()) {
            out.push_str(&format!("### Child agent\n\n`{child}`\n\n"));
        }
        if let Some(reason) = rec.fail_reason.as_deref().filter(|s| !s.is_empty()) {
            let visible = export_fail_reason(lang, reason, info, Some(trace), session_ops);
            if !visible.is_empty() {
                out.push_str(&format!("**Step fail reason:** {visible}\n\n"));
            }
        }
        if let Some(reflection) = rec.reflection.as_deref().filter(|s| !s.is_empty()) {
            out.push_str("### Reflection\n\n");
            out.push_str(reflection.trim());
            out.push_str("\n\n");
        }
    }
    out
}

fn export_step_args(args: &serde_json::Value) -> Vec<String> {
    let Some(obj) = args.as_object() else {
        return Vec::new();
    };
    if obj.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    for (key, value) in obj {
        let rendered = match value {
            serde_json::Value::String(s) if !s.trim().is_empty() => s.trim().to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Null => continue,
            other if other.is_array() || other.is_object() => {
                other.to_string()
            }
            other => other.to_string(),
        };
        if rendered.chars().count() > 240 {
            lines.push(format!(
                "{key}: {}…",
                rendered.chars().take(240).collect::<String>()
            ));
        } else {
            lines.push(format!("{key}: {rendered}"));
        }
    }
    lines
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct AgentRegistry {
    #[serde(default)]
    pub agents: Vec<String>,
    /// Prochain numéro `agent-{n}` (jamais réutilisé après redémarrage).
    #[serde(default)]
    pub next_id: u64,
}

pub fn load_registry() -> AgentRegistry {
    let path = agents_root().join("registry.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_registry(reg: &AgentRegistry) {
    let _ = std::fs::create_dir_all(agents_root());
    let path = agents_root().join("registry.json");
    if let Ok(j) = serde_json::to_string_pretty(reg) {
        let _ = std::fs::write(path, j);
    }
}

pub fn registry_add(agent_id: &str) {
    let mut reg = load_registry();
    if !reg.agents.iter().any(|a| a == agent_id) {
        reg.agents.push(agent_id.to_string());
        save_registry(&reg);
    }
}

/// Suffixe numérique de `agent-12` (0 si absent).
pub fn seq_from_id(agent_id: &str) -> u64 {
    agent_id
        .strip_prefix("agent-")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

pub fn agent_title(directive: &str) -> String {
    let t = directive.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() {
        return String::new();
    }
    let count = t.chars().count();
    let mut s: String = t.chars().take(56).collect();
    if count > 56 {
        s.push('…');
    }
    s
}

pub fn list_agent_ids() -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(agents_root()) else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    for e in rd.flatten() {
        let path = e.path();
        if !path.is_dir() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        if name.is_empty() {
            continue;
        }
        if path.join("spec.json").is_file() || path.join("info.json").is_file() {
            ids.push(name);
        }
    }
    ids.sort();
    ids
}

pub fn read_info(agent_id: &str) -> Option<AgentInfo> {
    let path = agent_dir(agent_id).join("info.json");
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn info_from_spec(agent_id: &str) -> Option<AgentInfo> {
    let spec = read_spec(agent_id)?;
    let is_roster = spec.kind == AgentKind::Roster || spec.goal.max_steps == 0;
    let title = spec
        .display_name
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| agent_title(&spec.goal.statement));
    Some(AgentInfo {
        agent_id: spec.agent_id.clone(),
        state: if is_roster {
            AgentState::Roster
        } else {
            AgentState::Killed
        },
        directive: spec.goal.statement.clone(),
        pid: None,
        caps: spec.caps.clone(),
        last_output: String::new(),
        step: 0,
        max_steps: spec.goal.max_steps,
        current_task: None,
        parent_id: spec.parent_id.clone(),
        children: Vec::new(),
        tokens_used: 0,
        skills: spec.skills.clone(),
        tools: spec.tools.clone(),
        mcp_servers: spec.mcp_servers.clone(),
        fail_reason: if is_roster {
            None
        } else {
            Some("arrêté au redémarrage".into())
        },
        session_id: spec.session_id.clone(),
        title,
        kind: spec.kind,
        display_name: spec.display_name.clone(),
        persona_id: spec.persona_id.clone(),
        origin: spec.origin.clone(),
    })
}

/// Alloue un id unique `agent-{n}` (compteur disque, jamais recyclé).
pub fn alloc_agent_id() -> String {
    let mut reg = load_registry();
    let disk_max = list_agent_ids()
        .iter()
        .map(|id| seq_from_id(id))
        .max()
        .unwrap_or(0);
    let n = reg.next_id.max(disk_max + 1).max(1);
    reg.next_id = n + 1;
    let id = format!("agent-{n}");
    if !reg.agents.iter().any(|a| a == &id) {
        reg.agents.push(id.clone());
    }
    save_registry(&reg);
    id
}

pub fn update_info_sidecar(info: &AgentInfo) {
    let dir = agent_dir(&info.agent_id);
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(j) = serde_json::to_string_pretty(info) {
        let _ = std::fs::write(dir.join("info.json"), j);
    }
}

/// Compaction : garde system + goal context + N derniers messages, résume le reste.
pub fn compact_working_memory(
    memory: &mut Vec<(String, String)>,
    keep_recent: usize,
) -> Option<String> {
    if memory.len() <= keep_recent + 2 {
        return None;
    }
    // Keep first system message(s) and last keep_recent
    let system: Vec<_> = memory
        .iter()
        .take_while(|(r, _)| r == "system")
        .cloned()
        .collect();
    let after_system = memory.len().saturating_sub(system.len());
    if after_system <= keep_recent {
        return None;
    }
    let drop_count = after_system - keep_recent;
    let dropped: Vec<_> = memory
        .iter()
        .skip(system.len())
        .take(drop_count)
        .cloned()
        .collect();
    let summary = format!(
        "[compaction] {} messages résumés : {}",
        dropped.len(),
        dropped
            .iter()
            .map(|(r, c)| format!("{r}: {}", truncate(c, 80)))
            .collect::<Vec<_>>()
            .join(" | ")
    );
    let mut new_mem = system;
    new_mem.push(("system".into(), summary.clone()));
    new_mem.extend(
        memory
            .iter()
            .skip(memory.len() - keep_recent)
            .cloned(),
    );
    *memory = new_mem;
    Some(summary)
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        format!("{}…", s.chars().take(n).collect::<String>())
    } else {
        s.to_string()
    }
}

pub fn spec_path(agent_id: &str) -> PathBuf {
    agent_dir(agent_id).join("spec.json")
}

pub fn exists_spec(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::{AgentKind, AgentState};

    #[test]
    fn export_trace_markdown_includes_steps_and_fail_reason() {
        use aos_proto::AgentStepRecord;
        let trace = AgentTrace {
            agent_id: "agent-7".into(),
            fail_reason: Some("agent_could_not_act".into()),
            steps: vec![AgentStepRecord {
                step: 1,
                thought: "planifier".into(),
                action: "notes.create".into(),
                args: serde_json::json!({"title": "cohort"}),
                tool_result: "ok".into(),
                child_id: Some("agent-8".into()),
                fail_reason: None,
                ..AgentStepRecord::default()
            }],
            ..AgentTrace::default()
        };
        let info = AgentInfo {
            agent_id: "agent-7".into(),
            state: AgentState::Failed,
            directive: "Plan the cohort".into(),
            pid: None,
            caps: vec![],
            last_output: String::new(),
            step: 1,
            max_steps: 8,
            current_task: None,
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: vec![],
            tools: vec![],
            mcp_servers: vec![],
            fail_reason: Some("agent_could_not_act".into()),
            session_id: Some("session-1".into()),
            title: "Planner".into(),
            kind: AgentKind::Task,
            display_name: Some("Alpha".into()),
            persona_id: None,
            origin: None,
        };
        let md = export_trace_markdown(&trace, Some(&info), "en", None);
        assert!(md.contains("# Agent agent-7"));
        assert!(md.contains("_Display name: Alpha_"));
        assert!(md.contains("**Fail reason:** The agent could not act."));
        assert!(!md.contains("agent_could_not_act"));
        assert!(md.contains("### Thought"));
        assert!(md.contains("planifier"));
        assert!(md.contains("`notes.create`"));
        assert!(md.contains("- title: cohort"));
        assert!(md.contains("### Tool result"));
        assert!(md.contains("### Child agent"));
        assert!(md.contains("`agent-8`"));

        let overflow = "le prompt ne tient pas dans le contexte (prompt=8749 + réserve_gen=520 = 9269 tokens > ctx=9216)";
        let trace_overflow = AgentTrace {
            agent_id: "agent-9".into(),
            fail_reason: Some(overflow.into()),
            steps: vec![AgentStepRecord {
                step: 2,
                thought: "réfléchir".into(),
                fail_reason: Some("PromptTooLong: ctx=8192".into()),
                ..AgentStepRecord::default()
            }],
            ..AgentTrace::default()
        };
        let md_overflow = export_trace_markdown(&trace_overflow, None, "fr", None);
        assert!(md_overflow.contains("**Fail reason:** Impossible de continuer."));
        assert!(md_overflow.contains("**Step fail reason:** Impossible de continuer."));
        assert!(!md_overflow.contains("ctx="));
        assert!(!md_overflow.to_ascii_lowercase().contains("prompttoolong"));
        assert!(!md_overflow.contains("réserve_gen"));
        assert!(!md_overflow.contains("Échec"));
    }

    #[test]
    fn export_trace_markdown_hides_max_steps_fail_reason() {
        let raw = "max_steps (64) atteint";
        let trace = AgentTrace {
            agent_id: "agent-90".into(),
            fail_reason: Some(raw.into()),
            ..AgentTrace::default()
        };
        let md_en = export_trace_markdown(&trace, None, "en", None);
        assert!(md_en.contains("**Fail reason:** Couldn't keep going."));
        assert!(!md_en.contains("max_steps"));
        assert!(!md_en.contains("atteint"));
        let md_fr = export_trace_markdown(&trace, None, "fr", None);
        assert!(md_fr.contains("**Fail reason:** Impossible de continuer."));
        assert!(!md_fr.contains("max_steps"));
        assert!(!md_fr.contains("atteint"));
    }

    #[test]
    fn export_trace_markdown_canvas_draw_stop_uses_locked_copy() {
        let raw = "max_steps (64) atteint";
        let trace = AgentTrace {
            agent_id: "agent-90".into(),
            fail_reason: Some(raw.into()),
            ..AgentTrace::default()
        };
        let info = AgentInfo {
            agent_id: "agent-90".into(),
            state: AgentState::Failed,
            directive: "dessine un moulin sur une coline".into(),
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
            fail_reason: Some(raw.into()),
            session_id: None,
            title: String::new(),
            kind: AgentKind::Task,
            display_name: None,
            persona_id: None,
            origin: None,
        };
        let md_en = export_trace_markdown(&trace, Some(&info), "en", None);
        assert!(md_en.contains("**Fail reason:** Couldn't draw."));
        assert!(!md_en.contains("max_steps"));
        assert!(!md_en.contains("atteint"));
        assert!(!md_en.contains("Failed"));
        let md_fr = export_trace_markdown(&trace, Some(&info), "fr", None);
        assert!(md_fr.contains("**Fail reason:** Impossible de dessiner."));
        assert!(!md_fr.contains("max_steps"));
        assert!(!md_fr.contains("atteint"));
    }

    #[test]
    fn export_trace_markdown_canvas_draw_max_steps_muted_when_traits_applied() {
        let raw = "max_steps (64) atteint";
        let trace = AgentTrace {
            agent_id: "agent-99".into(),
            fail_reason: Some(raw.into()),
            steps: vec![aos_proto::AgentStepRecord {
                step: 1,
                action: "canvas.spline".into(),
                tool_result: "ok seq=1".into(),
                ..Default::default()
            }],
            ..AgentTrace::default()
        };
        let info = AgentInfo {
            agent_id: "agent-99".into(),
            state: AgentState::Failed,
            directive: "dessine un moulin sur une coline".into(),
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
            fail_reason: Some(raw.into()),
            session_id: None,
            title: String::new(),
            kind: AgentKind::Task,
            display_name: None,
            persona_id: None,
            origin: None,
        };
        let md_en = export_trace_markdown(&trace, Some(&info), "en", None);
        assert!(!md_en.contains("**Fail reason:**"));
        assert!(!md_en.contains("Couldn't draw."));
        assert!(!md_en.contains("max_steps"));
        let md_fr = export_trace_markdown(&trace, Some(&info), "fr", None);
        assert!(!md_fr.contains("**Fail reason:**"));
        assert!(!md_fr.contains("Impossible de dessiner."));
    }

    #[test]
    fn export_trace_markdown_canvas_draw_max_steps_muted_from_session_ops() {
        let raw = "max_steps (64) atteint";
        let trace = AgentTrace {
            agent_id: "agent-99".into(),
            fail_reason: Some(raw.into()),
            ..AgentTrace::default()
        };
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
            tools: vec!["canvas.spline".into()],
            mcp_servers: vec![],
            fail_reason: Some(raw.into()),
            session_id: Some("sess-1".into()),
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
            body: aos_proto::CanvasOpBody::Stroke {
                points: vec![aos_proto::CanvasPoint { x: 0.1, y: 0.2 }],
                color: "#3ee0c4".into(),
                width: 0.01,
            },
        }];
        let md_en = export_trace_markdown(&trace, Some(&info), "en", Some(&ops));
        assert!(!md_en.contains("**Fail reason:**"));
        assert!(!md_en.contains("Couldn't draw."));
        let md_fr = export_trace_markdown(&trace, Some(&info), "fr", Some(&ops));
        assert!(!md_fr.contains("Impossible de dessiner."));
    }

    #[test]
    fn compaction_keeps_recent() {
        let mut mem = vec![
            ("system".into(), "base".into()),
            ("user".into(), "u1".into()),
            ("assistant".into(), "a1".into()),
            ("user".into(), "u2".into()),
            ("assistant".into(), "a2".into()),
            ("user".into(), "u3".into()),
            ("assistant".into(), "a3".into()),
        ];
        let s = compact_working_memory(&mut mem, 2);
        assert!(s.is_some());
        assert!(mem[0].1.contains("base"));
        assert!(mem.last().unwrap().1.contains("a3"));
    }

    #[test]
    fn seq_and_title() {
        assert_eq!(seq_from_id("agent-12"), 12);
        assert_eq!(seq_from_id("agent-1"), 1);
        assert_eq!(seq_from_id("other"), 0);
        let t = agent_title("  Analyse   de l'état des skills  ");
        assert!(t.starts_with("Analyse"));
        assert!(!t.contains("  "));
    }
}
