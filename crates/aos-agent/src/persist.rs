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
