//! # aos-agent — Agent Runtime v1 (P1.4)
//!
//! - `aos-agentd` : service de lifecycle — chaque agent est un **processus
//!   isolé** (worker), caps logiques via `aos-caps`, état cognitif
//!   sérialisable (§4.2) ;
//! - `aos-agent-worker` : boucle cognitive d'un agent (v1 : conversation
//!   simple via `model.infer`), contrôlable (pause/resume/steer/snapshot) via
//!   son intent `agent.<id>.control` sur le bus.

pub mod state;

pub use state::CognitiveState;

/// Intents exposés par `aos-agentd`.
pub mod intents {
    pub const CREATE: &str = "agent.create";
    pub const PAUSE: &str = "agent.pause";
    pub const RESUME: &str = "agent.resume";
    pub const KILL: &str = "agent.kill";
    pub const STEER: &str = "agent.steer";
    pub const STATE: &str = "agent.state";
    pub const LIST: &str = "agent.list";
    pub const SUBSCRIBE: &str = "agent.subscribe";
    pub const SNAPSHOT: &str = "agent.snapshot";
    /// Appelé par les workers pour remonter leur sortie/état à agentd.
    pub const REPORT: &str = "agent.report";
}

/// Payload de contrôle worker → intent `agent.<id>.control`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ControlCmd {
    Pause,
    Resume,
    Steer { directive: String },
    Snapshot,
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
