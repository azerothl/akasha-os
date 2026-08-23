//! Canvas scene digest — runtime fetch + prompt injection for agents.

use aos_ipc::BusClient;
use aos_proto::{canvas_scene_digest, CanvasGetRequest, CanvasGetResponse};

/// Fetch a compact scene digest via `canvas.get` (always call before drawing).
pub async fn fetch_canvas_scene_digest(bus: &BusClient, session_id: &str) -> Option<String> {
    let resp: CanvasGetResponse = bus
        .call(
            "canvas.get",
            &CanvasGetRequest {
                session_id: session_id.to_string(),
                after_seq: None,
            },
            vec![],
        )
        .await
        .ok()?;
    Some(canvas_scene_digest(
        &aos_proto::CanvasDoc {
            session_id: resp.session_id,
            next_seq: resp.next_seq,
            ops: resp.ops,
        },
        resp.canvas_aspect,
    ))
}

/// Block for system prompt injection when canvas tools are available.
pub fn canvas_scene_prompt_block(digest: &str) -> String {
    format!(
        "## Canvas actuel (canvas.get — ne pas deviner)\n\
         Digest compact (compteurs + bbox par seq, pas le JSON brut). \
         Commence par `canvas.get` si tu dessines ; état au début du tour :\n\
         ```\n{digest}\n```"
    )
}

/// Append refreshed digest after a canvas mutating tool call.
pub fn canvas_tool_outcome_with_digest(base: &str, digest: Option<&str>) -> String {
    match digest {
        Some(d) => format!("{base}\n\n[canvas digest]\n{d}"),
        None => base.to_string(),
    }
}

/// True when the tool mutates the canvas document.
pub fn canvas_tool_mutates_scene(tool: &str) -> bool {
    matches!(
        tool,
        "canvas.stroke"
            | "canvas.line"
            | "canvas.spline"
            | "canvas.rect"
            | "canvas.ellipse"
            | "canvas.fill"
            | "canvas.erase"
            | "canvas.clear"
            | "canvas.undo"
    )
}

/// True when the tool is a canvas read (get/export).
pub fn canvas_tool_is_get(tool: &str) -> bool {
    tool == "canvas.get" || tool == "canvas.export"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutating_tools_detected() {
        assert!(canvas_tool_mutates_scene("canvas.line"));
        assert!(canvas_tool_mutates_scene("canvas.fill"));
        assert!(!canvas_tool_mutates_scene("canvas.get"));
    }

    #[test]
    fn prompt_block_contains_digest() {
        let block = canvas_scene_prompt_block("seq=1 author=human kind=stroke");
        assert!(block.contains("seq=1"));
        assert!(block.contains("canvas.get"));
    }
}
