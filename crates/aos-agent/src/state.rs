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
    /// Résultat de `task.assess` : `"simple"` | `"complex"`.
    #[serde(default)]
    pub complexity: Option<String>,
    /// Si vrai, `plan.update` est obligatoire tant que `task_graph` est vide.
    #[serde(default)]
    pub needs_plan: bool,
    /// Recall mémoire déjà fait après le premier `plan.update` (tâches complexes).
    #[serde(default)]
    pub plan_memory_recalled: bool,
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
            complexity: None,
            needs_plan: false,
            plan_memory_recalled: false,
            version: 2,
        }
    }

    /// Vrai si un plan est encore requis avant toute autre action.
    pub fn plan_gate_active(&self) -> bool {
        self.needs_plan && self.task_graph.is_empty()
    }

    /// Actions autorisées sous le gate : `plan.update` et `goal.fail` uniquement.
    pub fn blocks_action(&self, action: &str) -> bool {
        self.plan_gate_active()
            && action != "plan.update"
            && action != "goal.fail"
            && action != "user.ask"
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
        let clipped = clip_working_memory_outcome(outcome);
        self.working_memory
            .push(("tool".to_string(), format!("[{tool}] {clipped}")));
    }

    /// Canvas tool outcomes must use the `user` role so vision PNG attaches to the
    /// latest stroke (mtmd binds images to the last user turn, not `tool`).
    pub fn push_canvas_tool(&mut self, tool: &str, outcome: &str) {
        let clipped = clip_working_memory_outcome(outcome);
        self.working_memory
            .push(("user".to_string(), format!("[{tool}] {clipped}")));
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

fn clip_working_memory_outcome(outcome: &str) -> String {
    if outcome.chars().count() > 1200 {
        format!(
            "{}…",
            outcome.chars().take(1200).collect::<String>()
        )
    } else {
        outcome.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_canvas_tool_uses_user_role() {
        let mut st = CognitiveState::new("agent-1", vec![]);
        st.push_canvas_tool("canvas.stroke", "ok seq=1");
        assert_eq!(st.working_memory.len(), 1);
        assert_eq!(st.working_memory[0].0, "user");
        assert!(st.working_memory[0].1.contains("canvas.stroke"));
    }

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
        assert!(!back.needs_plan);
        assert!(back.complexity.is_none());
    }

    #[test]
    fn plan_gate_active_when_needed_and_empty() {
        let mut st = CognitiveState::new("a", vec![]);
        st.needs_plan = true;
        assert!(st.plan_gate_active());
        assert!(st.blocks_action("web.search"));
        assert!(st.blocks_action("noop"));
        assert!(!st.blocks_action("plan.update"));
        assert!(!st.blocks_action("goal.fail"));
        assert!(!st.blocks_action("user.ask"));
        st.set_plan(vec![TaskNode {
            id: "1".into(),
            title: "étape".into(),
            status: TaskNodeStatus::Pending,
            notes: String::new(),
        }]);
        assert!(!st.plan_gate_active());
        assert!(!st.blocks_action("web.search"));
    }

    #[test]
    fn complexity_fields_default_on_old_json() {
        let json = r#"{"agent_id":"x","working_memory":[],"plan_stack":[],"cap_set_snapshot":[],"version":2}"#;
        let st = CognitiveState::from_json(json).unwrap();
        assert!(st.complexity.is_none());
        assert!(!st.needs_plan);
        assert!(!st.plan_memory_recalled);
    }
}
