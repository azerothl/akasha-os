//! # aos-agent — Agent Runtime agentic
//!
//! - `aos-agentd` : lifecycle, skills, MCP catalogue, prompt optimize, spawn
//! - `aos-agent-worker` : boucle goal Observe/Think/Act/Reflect/Checkpoint

pub mod actions;
pub mod agent_act;
pub mod assess;
pub mod canvas_scene;
pub mod context_budget;
pub mod mcp;
pub mod persist;
pub mod prompt;
pub mod room_conductor;
pub mod room_personas;
pub mod room_runtime;
pub mod schedule;
pub mod skills;
pub mod state;
pub mod tool_exec;
pub mod tools;

pub use assess::{parse_assess_response, AssessResult};
pub use state::CognitiveState;

/// Intents exposés par `aos-agentd`.
pub mod intents {
    pub const CREATE: &str = "agent.create";
    pub const START: &str = "agent.start";
    pub const PAUSE: &str = "agent.pause";
    pub const RESUME: &str = "agent.resume";
    pub const KILL: &str = "agent.kill";
    pub const STEER: &str = "agent.steer";
    pub const STATE: &str = "agent.state";
    pub const LIST: &str = "agent.list";
    pub const SUBSCRIBE: &str = "agent.subscribe";
    pub const SNAPSHOT: &str = "agent.snapshot";
    pub const TRACE: &str = "agent.trace";
    pub const REPORT: &str = "agent.report";
    pub const RETRY: &str = "agent.retry";
    pub const PROMPT_OPTIMIZE: &str = "agent.prompt.optimize";
    pub const GRANT: &str = "agent.grant";
    pub const SKILL_LIST: &str = "skill.list";
    pub const SKILL_GET: &str = "skill.get";
    pub const MCP_LIST: &str = "mcp.list";
    pub const SCHEDULE_CREATE: &str = "schedule.create";
    pub const SCHEDULE_LIST: &str = "schedule.list";
    pub const SCHEDULE_CANCEL: &str = "schedule.cancel";
    pub const ROOM_TURN: &str = "agent.room_turn";
    pub const ROOM_CONDUCT: &str = "agent.room_conduct";
    pub const ROOM_CONDUCT_CANCEL: &str = "agent.room_conduct.cancel";
    pub const SPEC_GET: &str = "agent.spec.get";
    pub const ROSTER_UPDATE: &str = "agent.roster.update";
}

/// Payload de contrôle worker → intent `agent.<id>.control`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ControlCmd {
    Pause,
    Resume,
    Steer { directive: String },
    Snapshot,
    /// Hot-grant d'une capacité (mise à jour caps du worker).
    GrantCap { cap: String },
    /// Réponse inline Allow Once / Refuser pour une action agent (slice 1).
    ActDecision { act_id: String, approved: bool },
}

/// Réponse d'un contrôle worker.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ControlResp {
    Ack,
    State(CognitiveState),
    Error(String),
}

/// Payload de `agent.report` (worker → agentd).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReportPayload {
    pub agent_id: String,
    pub event: aos_proto::AgentOutputEvent,
}

/// Payload de `agent.subscribe` (UI → agentd).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SubscribeRequest {
    pub agent_id: String,
}
