//! Parsing des actions agent (JSON structuré, DSML/XML tool_call, fallback TOOL:).

use serde::{Deserialize, Serialize};

/// Sentinel `fail_reason` / thread content key — UI maps to localized copy.
pub const THREAD_FAIL_COULD_NOT_ACT: &str = "agent_could_not_act";
/// Prompt/context overflow after compaction retries — UI maps to localized copy.
pub const THREAD_FAIL_COULD_NOT_CONTINUE: &str = "agent_could_not_continue";
/// Live agent killed when aos-agentd reloads — UI maps to localized copy.
pub const THREAD_FAIL_STOPPED_ON_RESTART: &str = "agent_stopped_on_restart";
/// Legacy persisted value before `THREAD_FAIL_STOPPED_ON_RESTART` (FR-only).
pub const LEGACY_FAIL_STOPPED_ON_RESTART: &str = "arrêté au redémarrage";
