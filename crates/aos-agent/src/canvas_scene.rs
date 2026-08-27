//! Canvas scene digest — runtime fetch + prompt injection for agents.

use aos_ipc::BusClient;
use aos_proto::{
    canvas_scene_digest, AgentStepRecord, CanvasAspect, CanvasExportRequest, CanvasGetRequest,
    CanvasGetResponse, CanvasSeeingRequest,
};
use std::path::PathBuf;

/// True when the agent kit includes session canvas drawing tools.
pub fn agent_has_canvas_tools(tool_ids: &[String]) -> bool {
    tool_ids.iter().any(|t| t.starts_with("canvas."))
}

/// Critic system prompt when the agent draws on the session canvas.
pub fn canvas_critic_system_prompt() -> &'static str {
    "Tu es un critique pour un agent qui dessine sur le canvas vectoriel (coords 0..1). \
     Les ops canvas.stroke, canvas.line, canvas.spline, canvas.rect, canvas.ellipse \
     (fill:true pour remplir), canvas.set_style comptent comme progrès dessin — \
     ne demande jamais de générer une image (media.image.generate). \
     En 2 phrases en français : est-ce que l'agent progresse vers le goal par des traits vectoriels ? \
     Que dessiner ensuite ? Réponds directement, sans balises <think> ni monologue Thinking Process."
}

/// User content for reflect when canvas tools are available — includes recent canvas ops.
pub fn canvas_reflect_user_content(
    step: u32,
    max_steps: u32,
    goal: &str,
    plan_stack: &[String],
    trace: &[AgentStepRecord],
) -> String {
    let base = format!(
        "step {}/{} goal={} tasks={:?}",
        step, max_steps, goal, plan_stack
    );
    let mut canvas_lines: Vec<String> = Vec::new();
    for rec in trace.iter().rev().take(8) {
        if rec.action.starts_with("canvas.") {
            let failed = rec.tool_result.contains("err:")
                || rec.tool_result.contains("outil inconnu")
                || rec.tool_result.contains("spawn err");
            canvas_lines.push(format!(
                "step {} {} applied={} snippet={}",
                rec.step,
                rec.action,
                !failed,
                truncate_reflect(&rec.tool_result, 100)
            ));
        }
    }
    if canvas_lines.is_empty() {
        format!(
            "{base}\n[canvas] aucune op encore — commence par canvas.get puis canvas.stroke/rect/ellipse \
             (fill:true pour remplir). Traits vectoriels = progrès ; pas media.image.generate."
        )
    } else {
        format!(
            "{base}\n[canvas ops récentes — traits vectoriels = progrès dessin, PAS media.image.generate]\n{}",
            canvas_lines.join("\n")
        )
    }
}

fn truncate_reflect(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", s[..max].trim_end())
    }
}

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
            pen: resp.pen,
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
         ```\n{digest}\n```\n\
         Avec un modèle vision : une capture PNG live du canvas est jointe à chaque inférence \
         (pas le bureau, pas un digest différé). \
         Placement : coords 0..1 max=1.0 (pas de pixels). \
         Lis le `scene_bbox` et les bbox par seq ; place chaque nouvelle op dans `usable` \
         avec marge ≥0.08 — ne superpose pas au même centre. \
         Chaque outil mutateur renvoie un digest rafraîchi dans `[canvas digest]`."
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
        "canvas.set_style"
            | "canvas.stroke"
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

fn aos_home() -> PathBuf {
    std::env::var("AOS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

/// True when the catalog lists `vision` for this model id.
pub fn catalog_model_supports_vision(model_id: Option<&str>) -> bool {
    let Some(id) = model_id.filter(|s| !s.is_empty()) else {
        return false;
    };
    let catalog = aos_home().join("share/models/catalog-offerings.json");
    let Ok(raw) = std::fs::read_to_string(catalog) else {
        return false;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    v.get("models")
        .and_then(|m| m.as_array())
        .into_iter()
        .flatten()
        .any(|m| {
            m.get("id").and_then(|x| x.as_str()) == Some(id)
                && m
                    .get("profiles")
                    .and_then(|p| p.as_array())
                    .into_iter()
                    .flatten()
                    .any(|p| p.as_str() == Some("vision"))
        })
}

/// Signal that a vision pass is reading the live canvas (Preview outline).
pub async fn set_canvas_seeing(bus: &BusClient, session_id: &str, active: bool) {
    let _ = bus
        .call::<CanvasSeeingRequest, serde_json::Value>(
            "canvas.seeing",
            &CanvasSeeingRequest {
                session_id: session_id.to_string(),
                active,
            },
            vec![],
        )
        .await;
}

/// Export the current canvas document to a PNG path (live raster, not desktop capture).
pub async fn fetch_canvas_live_png(
    bus: &BusClient,
    session_id: &str,
    aspect: CanvasAspect,
) -> Option<String> {
    let (width, height) = aspect.export_dimensions(1024);
    let v: serde_json::Value = bus
        .call(
            "canvas.export",
            &CanvasExportRequest {
                session_id: session_id.to_string(),
                path: None,
                width: Some(width),
                height: Some(height),
            },
            vec![],
        )
        .await
        .ok()?;
    let path = v.get("path")?.as_str()?.trim();
    if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    }
}

/// Merge a live canvas PNG into infer data refs (deduped, vision paths only).
pub fn merge_canvas_vision_refs(base: &[String], canvas_png: &str) -> Vec<String> {
    let mut out: Vec<String> = base
        .iter()
        .filter(|p| !is_vision_image_path(p))
        .cloned()
        .collect();
    if !canvas_png.is_empty() && !out.iter().any(|p| p == canvas_png) {
        out.push(canvas_png.to_string());
    }
    out
}

fn is_vision_image_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp")
}

/// Live canvas vision attach: export PNG + enable seeing chrome. Returns PNG path.
pub async fn begin_canvas_vision(
    bus: &BusClient,
    session_id: &str,
    model_id: Option<&str>,
    aspect: CanvasAspect,
) -> Option<String> {
    if !catalog_model_supports_vision(model_id) {
        return None;
    }
    let png = fetch_canvas_live_png(bus, session_id, aspect).await?;
    set_canvas_seeing(bus, session_id, true).await;
    Some(png)
}

/// End live canvas vision chrome after infer completes.
pub async fn end_canvas_vision(bus: &BusClient, session_id: &str) {
    set_canvas_seeing(bus, session_id, false).await;
}

/// Fetch canvas aspect for a session (for export dimensions).
pub async fn fetch_canvas_aspect(bus: &BusClient, session_id: &str) -> CanvasAspect {
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
        .unwrap_or(CanvasGetResponse {
            session_id: session_id.to_string(),
            canvas_open: false,
            canvas_aspect: CanvasAspect::default(),
            next_seq: 0,
            ops: vec![],
            pen: Default::default(),
            canvas_seeing: false,
        });
    resp.canvas_aspect
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
        assert!(block.contains("scene_bbox"));
        assert!(block.contains("[canvas digest]"));
        assert!(block.contains("PNG live"));
    }

    #[test]
    fn merge_canvas_vision_refs_replaces_images() {
        let base = vec![
            "/downloads/old.png".into(),
            "/documents/note.md".into(),
        ];
        let merged = merge_canvas_vision_refs(&base, "/downloads/canvas-live.png");
        assert_eq!(merged.len(), 2);
        assert!(merged.contains(&"/documents/note.md".into()));
        assert!(merged.contains(&"/downloads/canvas-live.png".into()));
        assert!(!merged.contains(&"/downloads/old.png".into()));
    }

    #[test]
    fn canvas_reflect_user_content_lists_recent_ops() {
        let trace = vec![
            AgentStepRecord {
                step: 1,
                action: "canvas.stroke".into(),
                tool_result: "ok seq=1".into(),
                ..Default::default()
            },
            AgentStepRecord {
                step: 2,
                action: "canvas.ellipse".into(),
                tool_result: "ok seq=2".into(),
                ..Default::default()
            },
        ];
        let content = canvas_reflect_user_content(2, 48, "dessine une canette", &[], &trace);
        assert!(content.contains("canvas.stroke"));
        assert!(content.contains("traits vectoriels"));
        assert!(content.contains("PAS media.image.generate"));
    }
}
