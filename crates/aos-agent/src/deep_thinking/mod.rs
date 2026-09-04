//! Deep Thinking Engine — plans hiérarchiques versionnés + délégation.
//!
//! Hébergé par `aos-agentd` ; le worker appelle les intents `plan.*`.

mod delegate;
mod engine;
mod store;
mod summary;

pub use delegate::{bind_child_to_step, find_step, find_step_for_child};
pub use engine::{deep_thinking_caps, DeepThinkingEngine, EngineError};
pub use store::PlanStore;
pub use summary::{
    count_in_progress, format_plan_updated_trace, format_spawn_trace, light_plan_summary,
};

pub mod intents {
    pub const CREATE: &str = "plan.create";
    pub const GET: &str = "plan.get";
    pub const UPDATE_STEP: &str = "plan.update_step";
    pub const REPLACE_TREE: &str = "plan.replace_tree";
    pub const DELEGATE_STEP: &str = "plan.delegate_step";
    pub const APPEND_LOG: &str = "plan.append_log";
}
