//! Canvas scene digest — runtime fetch + prompt injection for agents.

use aos_ipc::BusClient;
use aos_proto::{
    canvas_scene_digest, AgentStepRecord, AgentTrace, CanvasAspect, CanvasExportRequest,
    CanvasGetRequest, CanvasGetResponse, CanvasOp, CanvasOpBody, CanvasSeeingRequest, ModelInfo,
};
use std::path::PathBuf;

/// True when the agent kit includes session canvas drawing tools.
pub fn agent_has_canvas_tools(tool_ids: &[String]) -> bool {
    tool_ids.iter().any(|t| t.starts_with("canvas."))
}

pub fn agent_has_canvas_path(tool_ids: &[String]) -> bool {
    tool_ids.iter().any(|t| t == "canvas.path")
}

// CHUNK1_PROGRESSIVE_RESTORE - not final
