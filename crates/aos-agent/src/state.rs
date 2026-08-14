//! État cognitif d'un agent (§4.2) — sérialisable (snapshot/restore).

use aos_proto::{AgentGoal, AgentStepRecord, TaskNode, TaskNodeStatus};
use serde::{Deserialize, Serialize};

/// État cognitif complet d'un agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveState {
    pub agent_id: String,
    /// Fenêtre de contexte courante (rôle/contenu).
    pub working_memory: Vec<(String, String)>,
    /// Sous-buts empilés (titres de tâches).
    pub plan_stack: Vec<String>,
    /// Graphe de tâches structuré.
    #[serde(default)]
    pub task_graph: Vec<TaskNode>,
    /// Snapshot des capacités détenues (URIs).
    pub cap_set_snapshot: Vec<String>,
    #[serde(default)]
    pub goal: Option<AgentGoal>,
    #[serde(default)]
    pub step: u32,
    #[serde(default)]
    pub reflections: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub children: Vec<String>,
    #[serde(default)]
    pub tokens_used: u64,
    /// Journal des tours (transparence F-UI-04).
    #[serde(default)]
    pub trace: Vec<AgentStepRecord>,
    /// Version de schéma (migration future).
    pub version: u32,
}

impl CognitiveState {
    pub fn new(agent_id: impl Into<String>, caps: Vec<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            working_memory: Vec::new(),
            plan_stack: Vec::new(),
            task_graph: Vec::new(),
            cap_set_snapshot: caps,
            goal: None,
            step: 0,
            reflections: Vec::new(),
            artifacts: Vec::new(),
            parent_id: None,
            children: Vec::new(),
            tokens_used: 0,
            trace: Vec::new(),
            version: 2,
        }
    }

    pub fn push_user(&mut self, content: &str) {
        self.working_memory
            .push(("user".to_string(), content.to_string()));
    }

    pub fn push_assistant(&mut self, content: &str) {
        self.working_memory
            .push(("assistant".to_string(), content.to_string()));
    }

    pub fn push_tool(&mut self, tool: &str, outcome: &str) {
        let clipped = if outcome.chars().count() > 1500 {
            format!(
                "{}…",
                outcome.chars().take(1500).collect::<String>()
            )
        } else {
            outcome.to_string()
        };
        self.working_memory
            .push(("tool".to_string(), format!("[{tool}] {clipped}")));
    }

    pub fn set_plan(&mut self, nodes: Vec<TaskNode>) {
        self.plan_stack = nodes.iter().map(|n| n.title.clone()).collect();
        self.task_graph = nodes;
    }

    pub fn current_task_title(&self) -> Option<String> {
        self.task_graph
            .iter()
            .find(|n| n.status == TaskNodeStatus::Running)
            .or_else(|| {
                self.task_graph
                    .iter()
                    .find(|n| n.status == TaskNodeStatus::Pending)
            })
            .map(|n| n.title.clone())
    }

    /// Sérialise en JSON (snapshot disque, `var/agents/<id>/state.json`).
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_roundtrip() {
        let mut st = CognitiveState::new("agent-1", vec!["cap://fs/read/x".into()]);
        st.push_user("résume note.md");
        st.push_assistant("voici le résumé…");
        st.plan_stack.push("terminer la synthèse".into());
        st.step = 3;
        st.trace.push(aos_proto::AgentStepRecord {
            step: 1,
            action: "notes.list".into(),
            generated_tokens: 12,
            ..Default::default()
        });
        let json = st.to_json().unwrap();
        let back = CognitiveState::from_json(&json).unwrap();
        assert_eq!(back.agent_id, "agent-1");
        assert_eq!(back.working_memory.len(), 2);
        assert_eq!(back.cap_set_snapshot.len(), 1);
        assert_eq!(back.step, 3);
        assert_eq!(back.trace.len(), 1);
        assert_eq!(back.trace[0].action, "notes.list");
    }
}
