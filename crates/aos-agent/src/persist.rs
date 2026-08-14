//! Persistance agents : `var/agents/<id>/spec.json` + `state.json` + registry.

use aos_proto::{AgentInfo, AgentSpec};
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct AgentRegistry {
    pub agents: Vec<String>,
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
}
