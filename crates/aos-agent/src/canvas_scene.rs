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

// NOTE: progressive restore in progress — full file follows in next commit
