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

fn canvas_empty_scene_hint(tool_ids: &[String]) -> String {
    if agent_has_canvas_path(tool_ids) {
        "commence par canvas.get puis canvas.path (silhouettes) ou canvas.stroke/rect/ellipse (fill:true pour remplir ; x,y = coin haut-gauche, y vers le bas — lis la dernière bbox avant la suivante)".into()
    } else {
        "commence par canvas.get puis canvas.stroke/spline/rect/ellipse (fill:true pour remplir ; x,y = coin haut-gauche, y vers le bas — lis la dernière bbox avant la suivante)".into()
    }
}

/// Critic system prompt when the agent draws on the session canvas.
pub fn canvas_critic_system_prompt() -> &'static str {
    "Tu es un critique pour un agent qui dessine sur le canvas vectoriel (coords 0..1, origine coin haut-gauche, y vers le bas). \
     Une capture PNG du canvas actuel est jointe — REGARDE l'image : le dessin ressemble-t-il au goal, \
     ou seulement des arches / traits empilés au même endroit ? \
     Un trait vectoriel ne compte comme progrès que s'il rapproche visuellement du goal ; \
     ne valide pas des répétitions identiques. Ne demande jamais media.image.generate. \
     Ne demande jamais de tout effacer ni de recommencer le dessin (pas canvas.clear, pas « refais le moulin ») : \
     conserve ce qui est déjà sur le canvas et indique seulement la pièce manquante à ajouter \
     (ex. colline, corps, toit, voiles) — une silhouette canvas.path remplie par partie, pas une pile de splines. \
     En 2 phrases en français : ce que tu vois vs le goal, et quelle pièce unique tracer ensuite (ou arrêter si c'est bon). \
     Réponds directement, sans balises <think> ni monologue Thinking Process."
}

/// User content for reflect when canvas tools are available — includes recent canvas ops.
pub fn canvas_reflect_user_content(
    step: u32,
    max_steps: u32,
    goal: &str,
    plan_stack: &[String],
    trace: &[AgentStepRecord],
    canvas_tool_ids: &[String],
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
            "{base}\n[canvas] aucune op encore — {}. \
             Traits vectoriels = progrès ; pas media.image.generate.",
            canvas_empty_scene_hint(canvas_tool_ids)
        )
    } else {
        format!(
            "{base}\n[canvas ops récentes — capture PNG jointe : regarde si ça ressemble au goal, PAS media.image.generate]\n\
             Politique : conserve les ops déjà sur le canvas ; ajoute seulement la pièce manquante (colline, corps, toit, voiles…) — \
             jamais canvas.clear ni redessiner la tour/moulin depuis zéro ; préfère un canvas.path rempli par partie.\n{}",
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
         Poursuis le dessin existant : ajoute la pièce manquante seulement — ne redémarre pas (pas canvas.clear). \
         Silhouettes : un `canvas.path` rempli par partie lisible (colline, corps, toit, voiles), pas des dizaines de splines/rects empilés. \
         Après chaque op canvas réussie : une capture PNG du canvas actuel est jointe \
         au tour suivant (regarde l'image, pas seulement le digest). \
         Placement : coords 0..1 max=1.0 (pas de pixels). \
         Lis le `scene_bbox` et les bbox par seq ; place chaque nouvelle op dans `usable` \
         avec marge ≥0.08 — ne superpose pas au même centre. \
         Chaque outil mutateur renvoie un digest rafraîchi dans `[canvas digest]` \
         et une capture `[canvas scene]` (export canvas.export, toujours jointe)."
    )
}

/// Append refreshed digest after a canvas mutating tool call.
pub fn canvas_tool_outcome_with_digest(base: &str, digest: Option<&str>) -> String {
    match digest {
        Some(d) => format!("{base}\n\n[canvas digest]\n{d}"),
        None => base.to_string(),
    }
}

/// Outcome text + optional PNG path after a canvas mutating tool succeeds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanvasSceneUpdate {
    pub text: String,
    pub png_path: Option<String>,
}

/// True when a canvas tool outcome looks like a successful apply (not bus/perm error).
pub fn canvas_op_succeeded(outcome: &str) -> bool {
    let trimmed = outcome.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("erreur outil:")
        || lower.contains("erreur bus:")
        || lower.starts_with("err:")
        || lower.contains(" err:")
        || lower.contains("permissiondenied")
        || lower.contains("actordenied")
    {
        return false;
    }
    trimmed.starts_with("ok ") || trimmed.starts_with("ok\n")
}

/// Refresh digest + export PNG (via `canvas.export`) after a successful canvas op.
pub async fn refresh_canvas_scene_after_op(
    bus: &BusClient,
    session_id: &str,
    base_outcome: &str,
) -> CanvasSceneUpdate {
    if !canvas_op_succeeded(base_outcome) {
        return CanvasSceneUpdate {
            text: base_outcome.to_string(),
            png_path: None,
        };
    }
    let digest = fetch_canvas_scene_digest(bus, session_id).await;
    let mut text = canvas_tool_outcome_with_digest(base_outcome, digest.as_deref());
    let aspect = fetch_canvas_aspect(bus, session_id).await;
    let png_path = fetch_canvas_live_png(bus, session_id, aspect).await;
    if png_path.is_some() {
        text.push_str(
            "\n\n[canvas scene] Capture PNG du canvas actuel jointe au prochain tour — \
             regarde l'image avant le prochain trait ; corrige si ça ne ressemble pas au goal.",
        );
    }
    CanvasSceneUpdate { text, png_path }
}


/// True when a canvas tool applies a visible trait (stroke/line/spline/path/rect/ellipse/fill/erase).

/// Excludes read/style ops (`canvas.get`, `canvas.export`, `canvas.set_style`) and document resets.
pub fn canvas_draw_tool_applies_trait(tool: &str) -> bool {
    matches!(
        tool,
        "canvas.stroke"
            | "canvas.line"
            | "canvas.spline"
            | "canvas.path"
            | "canvas.rect"
            | "canvas.ellipse"
            | "canvas.fill"
            | "canvas.erase"
    )
}

/// True when a committed canvas op body is a visible trait.
pub fn canvas_op_body_applies_trait(body: &CanvasOpBody) -> bool {
    matches!(
        body,
        CanvasOpBody::Stroke { .. }
            | CanvasOpBody::Line { .. }
            | CanvasOpBody::Spline { .. }
            | CanvasOpBody::Path { .. }
            | CanvasOpBody::Rect { .. }
            | CanvasOpBody::Ellipse { .. }
            | CanvasOpBody::Fill { .. }
            | CanvasOpBody::Erase { .. }
    )
}

/// True when the session canvas document already has at least one trait op.
pub fn session_canvas_has_traits(ops: &[CanvasOp]) -> bool {
    ops.iter()
        .any(|op| canvas_op_body_applies_trait(&op.body))
}

/// True when the agent trace records at least one successful trait apply.
pub fn trace_has_applied_canvas_traits(trace: &AgentTrace) -> bool {
    trace.steps.iter().any(|step| {
        canvas_draw_tool_applies_trait(step.action.trim())
            && canvas_op_succeeded(step.tool_result.trim())
    })
}

/// True when canvas already has traits (session doc and/or agent trace).
pub fn canvas_has_applied_traits(
    session_ops: Option<&[CanvasOp]>,
    trace: Option<&AgentTrace>,
) -> bool {
    if session_ops.is_some_and(session_canvas_has_traits) {
        return true;
    }
    trace.is_some_and(trace_has_applied_canvas_traits)
}

/// True when the tool mutates the canvas document.
pub fn canvas_tool_mutates_scene(tool: &str) -> bool {
    matches!(
        tool,
        "canvas.set_style"
            | "canvas.stroke"
            | "canvas.line"
            | "canvas.spline"
            | "canvas.path"
            | "canvas.rect"
            | "canvas.ellipse"
            | "canvas.fill"
            | "canvas.erase"
            | "canvas.clear"
            | "canvas.undo"
    )
}

/// True when a successful canvas op should auto-complete the current plan node.
/// Style/read ops do not advance the task graph.
pub fn canvas_tool_completes_plan_node(tool: &str) -> bool {
    matches!(
        tool,
        "canvas.stroke"
            | "canvas.line"
            | "canvas.spline"
            | "canvas.path"
            | "canvas.rect"
            | "canvas.ellipse"
            | "canvas.fill"
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

/// Drop PNG/JPEG/WebP paths when the loaded model has no mmproj projector.
pub fn strip_vision_image_paths(refs: &[String]) -> Vec<String> {
    refs.iter()
        .filter(|p| !is_vision_image_path(p))
        .cloned()
        .collect()
}

/// Whether the session model context has a loaded mmproj projector.
pub async fn session_model_has_vision(bus: &BusClient, model_id: Option<&str>) -> bool {
    let Some(id) = model_id.filter(|s| !s.is_empty()) else {
        return false;
    };
    let models: Vec<ModelInfo> = match bus.call("model.list", &(), vec![]).await {
        Ok(m) => m,
        Err(_) => return false,
    };
    models
        .iter()
        .find(|m| m.id == id)
        .is_some_and(|m| m.has_vision)
}

fn is_vision_image_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp")
}

/// Live canvas vision attach: export PNG + enable seeing chrome. Returns PNG path.
/// Skipped when the session model has no loaded mmproj (text-only infer still runs).
pub async fn begin_canvas_vision(
    bus: &BusClient,
    session_id: &str,
    aspect: CanvasAspect,
    model_id: Option<&str>,
) -> Option<String> {
    if !session_model_has_vision(bus, model_id).await {
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

/// Warn when recent canvas strokes stack identical bboxes (blind repeat loop).
pub fn canvas_repeat_stroke_warning(
    trace: &[AgentStepRecord],
    action: &str,
) -> Option<&'static str> {
    canvas_repeat_stroke_verdict(trace, action).and_then(|v| match v {
        CanvasRepeatVerdict::Warn(msg) | CanvasRepeatVerdict::Abort(msg) => Some(msg),
    })
}

/// Backstop when the model still stacks identical strokes despite scene snapshots.
pub fn canvas_repeat_stroke_verdict(
    trace: &[AgentStepRecord],
    action: &str,
) -> Option<CanvasRepeatVerdict> {
    if !matches!(action, "canvas.stroke" | "canvas.line" | "canvas.spline") {
        return None;
    }
    let bboxes: Vec<[f32; 4]> = trace
        .iter()
        .rev()
        .filter(|r| {
            matches!(
                r.action.as_str(),
                "canvas.stroke" | "canvas.line" | "canvas.spline"
            ) && canvas_op_succeeded(&r.tool_result)
        })
        .filter_map(|r| parse_outcome_bbox(&r.tool_result))
        .take(8)
        .collect();
    if bboxes.len() < 3 {
        return None;
    }
    if !bboxes.windows(2).all(|w| bbox_near_duplicate(&w[0], &w[1])) {
        return None;
    }
    if bboxes.len() >= 6 {
        return Some(CanvasRepeatVerdict::Abort(
            "Boucle canvas : traits quasi identiques répétés — regarde la capture, change \
             d'approche ou termine (goal.complete / goal.fail).",
        ));
    }
    Some(CanvasRepeatVerdict::Warn(
        "Tu empiles des traits quasi identiques — regarde la capture canvas et change \
         couleur, position ou forme avant de continuer.",
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasRepeatVerdict {
    Warn(&'static str),
    Abort(&'static str),
}

fn parse_outcome_bbox(outcome: &str) -> Option<[f32; 4]> {
    let marker = "bbox=(";
    let rest = outcome.split_once(marker)?.1;
    let (p0, tail) = rest.split_once(")-(")?;
    let p1 = tail.split_once(')')?.0;
    let (x0, y0) = parse_coord_pair(p0)?;
    let (x1, y1) = parse_coord_pair(p1)?;
    Some([x0, y0, x1, y1])
}

fn parse_coord_pair(s: &str) -> Option<(f32, f32)> {
    let (x, y) = s.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

fn bbox_near_duplicate(a: &[f32; 4], b: &[f32; 4]) -> bool {
    const EPS: f32 = 0.04;
    (a[0] - b[0]).abs() < EPS
        && (a[1] - b[1]).abs() < EPS
        && (a[2] - b[2]).abs() < EPS
        && (a[3] - b[3]).abs() < EPS
}

/// When to run the canvas critic (`reflect`): after each scene change, or every 3 steps.
pub fn should_run_canvas_critic(canvas_agent: bool, canvas_scene_changed: bool, step: u32) -> bool {
    (canvas_agent && canvas_scene_changed) || step % 3 == 0
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
    fn plan_completing_tools_exclude_set_style() {
        assert!(canvas_tool_completes_plan_node("canvas.spline"));
        assert!(canvas_tool_completes_plan_node("canvas.path"));
        assert!(canvas_tool_completes_plan_node("canvas.stroke"));
        assert!(!canvas_tool_completes_plan_node("canvas.set_style"));
        assert!(!canvas_tool_completes_plan_node("canvas.get"));
        assert!(!canvas_tool_completes_plan_node("canvas.export"));
    }

    #[test]
    fn prompt_block_contains_digest() {
        let block = canvas_scene_prompt_block("seq=1 author=human kind=stroke");
        assert!(block.contains("seq=1"));
        assert!(block.contains("canvas.get"));
        assert!(block.contains("scene_bbox"));
        assert!(block.contains("[canvas digest]"));
        assert!(block.contains("capture PNG"));
    }

    #[test]
    fn strip_vision_image_paths_removes_png_jpeg_webp() {
        let refs = vec![
            "/downloads/canvas.png".into(),
            "/documents/note.md".into(),
            "/tmp/photo.jpg".into(),
        ];
        let stripped = strip_vision_image_paths(&refs);
        assert_eq!(stripped, vec!["/documents/note.md".to_string()]);
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
    fn canvas_critic_mentions_visual_goal_check() {
        let prompt = canvas_critic_system_prompt();
        assert!(prompt.contains("REGARDE l'image"));
        assert!(prompt.contains("ressemble"));
        assert!(prompt.contains("canvas.clear"));
        assert!(prompt.contains("pièce manquante"));
        assert!(!prompt.contains("comptent comme progrès dessin"));
    }

    #[test]
    fn canvas_reflect_mentions_top_left_placement() {
        let content = canvas_reflect_user_content(1, 48, "dessine une canette", &[], &[], &["canvas.path".into()]);
        assert!(content.contains("coin haut-gauche"));
        assert!(content.contains("dernière bbox"));
    }

    #[test]
    fn canvas_op_success_detected() {
        assert!(canvas_op_succeeded("ok seq=3 stroke bbox=(0.1,0.2)-(0.3,0.4)"));
        assert!(!canvas_op_succeeded("ERREUR outil: session"));
        assert!(!canvas_op_succeeded("err: invalid"));
    }

    #[test]
    fn canvas_scene_update_text_marks_snapshot() {
        let update = CanvasSceneUpdate {
            text: "ok seq=1\n\n[canvas scene] Capture PNG".into(),
            png_path: Some("/downloads/canvas-s-1.png".into()),
        };
        assert!(update.text.contains("[canvas scene]"));
        assert_eq!(
            update.png_path.as_deref(),
            Some("/downloads/canvas-s-1.png")
        );
    }

    #[test]
    fn parse_outcome_bbox_from_tool_result() {
        let bbox = parse_outcome_bbox("ok seq=12 ellipse bbox=(0.350,0.150)-(0.650,0.270)")
            .expect("bbox");
        assert!((bbox[0] - 0.35).abs() < 0.001);
        assert!((bbox[3] - 0.27).abs() < 0.001);
    }

    #[test]
    fn should_run_canvas_critic_after_stroke_and_every_three_steps() {
        assert!(should_run_canvas_critic(true, true, 1));
        assert!(!should_run_canvas_critic(true, false, 1));
        assert!(should_run_canvas_critic(true, false, 3));
        assert!(should_run_canvas_critic(false, false, 3));
        assert!(!should_run_canvas_critic(false, false, 2));
    }

    #[test]
    fn canvas_repeat_stroke_verdict_aborts_after_six() {
        let trace: Vec<AgentStepRecord> = (1..=7)
            .map(|step| AgentStepRecord {
                step,
                action: "canvas.stroke".into(),
                tool_result: format!(
                    "ok seq={step} stroke bbox=(0.2,0.3)-(0.4,0.5)"
                ),
                ..Default::default()
            })
            .collect();
        match canvas_repeat_stroke_verdict(&trace, "canvas.stroke") {
            Some(CanvasRepeatVerdict::Abort(_)) => {}
            other => panic!("expected abort, got {other:?}"),
        }
    }

    #[test]
    fn canvas_repeat_stroke_warning_on_identical_bboxes() {
        let trace = vec![
            AgentStepRecord {
                step: 1,
                action: "canvas.stroke".into(),
                tool_result: "ok seq=1 stroke bbox=(0.2,0.3)-(0.4,0.5)".into(),
                ..Default::default()
            },
            AgentStepRecord {
                step: 2,
                action: "canvas.stroke".into(),
                tool_result: "ok seq=2 stroke bbox=(0.2,0.3)-(0.4,0.5)".into(),
                ..Default::default()
            },
            AgentStepRecord {
                step: 3,
                action: "canvas.stroke".into(),
                tool_result: "ok seq=3 stroke bbox=(0.21,0.31)-(0.41,0.51)".into(),
                ..Default::default()
            },
        ];
        assert!(canvas_repeat_stroke_warning(&trace, "canvas.stroke").is_some());
        assert!(canvas_repeat_stroke_warning(&trace, "canvas.rect").is_none());
        match canvas_repeat_stroke_verdict(&trace, "canvas.stroke") {
            Some(CanvasRepeatVerdict::Warn(_)) => {}
            other => panic!("expected warn at 3 strokes, got {other:?}"),
        }
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
        let content = canvas_reflect_user_content(2, 48, "dessine une canette", &[], &trace, &["canvas.stroke".into()]);
        assert!(content.contains("canvas.stroke"));
        assert!(content.contains("capture PNG jointe"));
        assert!(content.contains("PAS media.image.generate"));
        assert!(content.contains("pièce manquante"));
        assert!(content.contains("jamais canvas.clear"));
    }

    #[test]
    fn canvas_draw_tool_applies_trait_excludes_style_and_reads() {
        assert!(canvas_draw_tool_applies_trait("canvas.spline"));
        assert!(canvas_draw_tool_applies_trait("canvas.path"));
        assert!(canvas_draw_tool_applies_trait("canvas.rect"));
        assert!(!canvas_draw_tool_applies_trait("canvas.set_style"));
        assert!(!canvas_draw_tool_applies_trait("canvas.get"));
        assert!(!canvas_draw_tool_applies_trait("canvas.export"));
    }

    #[test]
    fn trace_has_applied_canvas_traits_counts_successful_draw_ops() {
        let trace = AgentTrace {
            steps: vec![
                AgentStepRecord {
                    step: 1,
                    action: "canvas.set_style".into(),
                    tool_result: "ok pen=#000".into(),
                    ..Default::default()
                },
                AgentStepRecord {
                    step: 2,
                    action: "canvas.rect".into(),
                    tool_result: "ok seq=1".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert!(trace_has_applied_canvas_traits(&trace));
        let empty = AgentTrace {
            steps: vec![AgentStepRecord {
                step: 1,
                action: "canvas.get".into(),
                tool_result: "ok ops=0".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(!trace_has_applied_canvas_traits(&empty));
    }
}
