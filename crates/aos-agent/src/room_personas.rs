//! Built-in salon personas (stable roster agent ids).

use aos_proto::{AgentCreateRequest, AgentGoal, AgentKind, AgentSpec};

/// Built-in salon persona definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomPersona {
    pub id: &'static str,
    pub display_name: &'static str,
    pub directive: &'static str,
    pub system_prompt: &'static str,
}

pub const ROOM_PERSONAS: &[RoomPersona] = &[
    RoomPersona {
        id: "researcher",
        display_name: "Researcher",
        directive: "Gather facts and cite sources before recommending action.",
        system_prompt:
            "You are a careful researcher. Prefer evidence, nuance, and clear unknowns.",
    },
    RoomPersona {
        id: "critic",
        display_name: "Critic",
        directive: "Stress-test ideas: risks, gaps, and failure modes.",
        system_prompt:
            "You are a constructive critic. Be direct about weaknesses without being dismissive.",
    },
    RoomPersona {
        id: "coder",
        display_name: "Coder",
        directive: "Propose concrete implementation steps and code-shaped answers.",
        system_prompt:
            "You are a pragmatic coder. Favor small, testable changes and explicit trade-offs.",
    },
    RoomPersona {
        id: "planner",
        display_name: "Planner",
        directive: "Break work into ordered steps with dependencies and checkpoints.",
        system_prompt:
            "You are a planner. Organize work into phases, owners, and clear success criteria.",
    },
];

pub fn persona_by_id(id: &str) -> Option<&'static RoomPersona> {
    ROOM_PERSONAS.iter().find(|p| p.id == id)
}

/// Stable roster id shared across salon sessions (`persona-coder`, …).
pub fn persona_agent_id(persona_id: &str) -> String {
    format!("persona-{persona_id}")
}

pub fn persona_create_request(persona: &RoomPersona, model_id: Option<String>) -> AgentCreateRequest {
    let mut req = AgentCreateRequest::simple(String::new());
    req.kind = AgentKind::Roster;
    req.display_name = Some(persona.display_name.to_string());
    req.persona_id = Some(persona.id.to_string());
    req.system_prompt = Some(persona.system_prompt.to_string());
    req.model_id = model_id;
    req.goal = Some(AgentGoal {
        statement: String::new(),
        success_criteria: vec![],
        max_steps: 0,
        max_subagents: 0,
        timeout_secs: 300,
    });
    req
}

pub fn roster_spec_from_request(agent_id: &str, req: &AgentCreateRequest) -> AgentSpec {
    let mut goal = req.resolved_goal();
    if req.kind == AgentKind::Roster {
        goal.max_steps = 0;
        goal.max_subagents = 0;
    }
    AgentSpec {
        agent_id: agent_id.to_string(),
        goal,
        kind: req.kind,
        display_name: req.display_name.clone(),
        persona_id: req.persona_id.clone(),
        system_prompt: req.system_prompt.clone(),
        skills: req.skills.clone(),
        tools: req.tools.clone(),
        mcp_servers: req.mcp_servers.clone(),
        documents: req.documents.clone(),
        caps: req.caps.clone(),
        model_id: req.model_id.clone(),
        parent_id: req.parent_id.clone(),
        session_id: req.session_id.clone(),
        budget: req.budget.clone(),
        optimize_prompt: req.optimize_prompt,
        gate_mode: req.gate_mode.clone(),
        origin: req.origin.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persona_agent_ids_stable() {
        assert_eq!(persona_agent_id("coder"), "persona-coder");
    }

    #[test]
    fn persona_create_does_not_spawn_worker() {
        let p = persona_by_id("coder").unwrap();
        let req = persona_create_request(p, None);
        assert!(!req.spawns_worker());
        assert_eq!(req.display_name.as_deref(), Some("Coder"));
    }

    #[test]
    fn roster_spec_preserves_user_display_name() {
        let mut req = AgentCreateRequest::simple(String::new());
        req.kind = AgentKind::Roster;
        req.display_name = Some("Skills Auditor".into());
        req.origin = Some("library".into());
        req.system_prompt = Some("You review skill manifests.".into());
        let spec = roster_spec_from_request("agent-42", &req);
        assert_eq!(spec.display_name.as_deref(), Some("Skills Auditor"));
        assert_eq!(spec.kind, AgentKind::Roster);
        assert_eq!(spec.goal.max_steps, 0);
        assert!(spec.goal.statement.is_empty());
        assert_eq!(spec.origin.as_deref(), Some("library"));
    }
}
