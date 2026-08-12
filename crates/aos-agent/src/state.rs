//! État cognitif d'un agent (§4.2) — sérialisable (snapshot/restore).

use serde::{Deserialize, Serialize};

/// État cognitif complet d'un agent.
///
/// En v1 : mémoire de travail (messages), pile de plans simplifiée,
/// snapshot des capacités. `suspend`/`resume`/`snapshot`/`restore` sont
/// matérialisés par la sérialisation JSON de cette structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveState {
    pub agent_id: String,
    /// Fenêtre de contexte courante (rôle/contenu).
    pub working_memory: Vec<(String, String)>,
    /// Sous-buts empilés (v1 : directives en attente).
    pub plan_stack: Vec<String>,
    /// Snapshot des capacités détenues (URIs).
    pub cap_set_snapshot: Vec<String>,
    /// Version de schéma (migration future).
    pub version: u32,
}

impl CognitiveState {
    pub fn new(agent_id: impl Into<String>, caps: Vec<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            working_memory: Vec::new(),
            plan_stack: Vec::new(),
            cap_set_snapshot: caps,
            version: 1,
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

    /// Sérialise en JSON (snapshot disque, `var/agents/<id>.json`).
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
        let json = st.to_json().unwrap();
        let back = CognitiveState::from_json(&json).unwrap();
        assert_eq!(back.agent_id, "agent-1");
        assert_eq!(back.working_memory.len(), 2);
        assert_eq!(back.cap_set_snapshot.len(), 1);
    }
}
