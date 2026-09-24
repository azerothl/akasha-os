//! Illustration Studio SceneGraph host_call handlers (agent co-edit).
//!
//! Canvas precedent: WASM tools forward to platform via `host_call`. Scene
//! edits stay capability-gated and audit-friendly; SceneGraph YAML is the SoT
//! (no free FS — callers pass `scene_yaml` or we read the project path under
//! `/documents/illustrations/**` with fail-closed caps).

use crate::module_rt::HostCallCtx;
use aos_scene::{
    apply_batch, apply_one, default_mesh_search_roots, load_project_yaml, require_batch_caps,
    require_edit_caps, resolve_mesh_uri, save_project_yaml, AgentEditOp, EditActorKind,
    EditSnapshot, LockKind, LockScope, NodeKind, ProjectFile, SceneGraph, SemanticLock,
    ASSET_ILLUSTRATION_READ_CAP, DEFAULT_SCENE_YAML_PATH,
    SCENE_APPLY_SERVICE, SCENE_CAMERA_SERVICE, SCENE_COMPOSE_CAP, SCENE_COMPOSE_SERVICE,
    SCENE_EDIT_CAP, SCENE_GET_SERVICE, SCENE_LIGHT_SERVICE, SCENE_LOCKS_SERVICE, SCENE_LOCK_CAP,
    SCENE_LOCK_SERVICE, SCENE_POSE_CAP, SCENE_POSE_SERVICE, SCENE_SELECT_SERVICE,
    SCENE_TRS_SERVICE, SCENE_UNLOCK_SERVICE,
};
use serde_json::{json, Value};

fn actor_kind(ctx: &HostCallCtx) -> EditActorKind {
    EditActorKind::from_actor(&ctx.actor)
}

fn require_any_cap(ctx: &HostCallCtx, caps: &[&str]) -> Result<(), String> {
    if caps.iter().any(|need| ctx.granted_caps.iter().any(|c| c == *need)) {
        Ok(())
    } else {
        Err(format!(
            "permission refusée: {} requis",
            caps.first().copied().unwrap_or("cap")
        ))
    }
}

fn load_snap(args: &Value) -> Result<EditSnapshot, String> {
    let yaml = args
        .get("scene_yaml")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    EditSnapshot::from_yaml(yaml).map_err(|e| e.to_string())
}

fn load_project_from_args(args: &Value) -> Result<ProjectFile, String> {
    if let Some(yaml) = args.get("scene_yaml").and_then(|v| v.as_str()) {
        if yaml.trim().is_empty() {
            Ok(ProjectFile::new(SceneGraph::demo_scene()))
        } else {
            load_project_yaml(yaml).map_err(|e| format!("scene_yaml: {e}"))
        }
    } else {
        Ok(ProjectFile::new(SceneGraph::demo_scene()))
    }
}

fn scene_diagnostics(project: &ProjectFile) -> Value {
    let roots = default_mesh_search_roots();
    let search: Vec<_> = roots.iter().map(|path| path.as_path()).collect();
    let mut missing = Vec::new();
    let mut mesh_count = 0;
    for node in project.scene.nodes.values() {
        if node.kind == NodeKind::MeshAsset {
            mesh_count += 1;
            if node
                .mesh_uri
                .as_deref()
                .and_then(|uri| resolve_mesh_uri(uri, &search))
                .is_none()
            {
                missing.push(node.name.clone());
            }
        }
    }
    missing.sort();
    let summary = format!(
        "{} nodes · {} imported meshes · {} effects · {} shots · {} missing assets{}",
        project.scene.nodes.len(),
        mesh_count,
        project.scene.effects.len(),
        project
            .storyboard
            .as_ref()
            .map(|s| s.frames.len())
            .unwrap_or(0),
        missing.len(),
        if missing.is_empty() {
            String::new()
        } else {
            format!(": {}", missing.join(", "))
        }
    );
    json!({"diagnostics": summary})
}

fn scene_history_op(args: &Value) -> Result<Value, String> {
    let mut project = load_project_from_args(args)?;
    let operation = args.get("op").and_then(Value::as_str).unwrap_or("");
    let label = args.get("name").and_then(Value::as_str).unwrap_or("");
    let mut history = project.history.take().unwrap_or_default();
    let mut version_id = None;
    match operation {
        "save_version" => {
            version_id = Some(history.save_version(&project.scene, label).map_err(|e| e.to_string())?);
        }
        "restore_version" => {
            let id = args.get("version_id").and_then(Value::as_f64).unwrap_or(0.0);
            if !id.is_finite() || id < 1.0 || id.fract() != 0.0 {
                return Err("invalid version id".into());
            }
            project.scene = history
                .restore_version(id as u32)
                .map_err(|e| e.to_string())?;
        }
        "save_variant" => history
            .save_variant(&project.scene, label)
            .map_err(|e| e.to_string())?,
        "apply_variant" => {
            project.scene = history.apply_variant(label).map_err(|e| e.to_string())?;
        }
        _ => return Err("unknown history operation".into()),
    }
    if project
        .animation
        .as_ref()
        .is_some_and(|a| a.validate(&project.scene).is_err())
    {
        project.animation = None;
    }
    if project
        .selected_id
        .as_ref()
        .is_some_and(|id| !project.scene.nodes.contains_key(id))
    {
        project.selected_id = None;
    }
    let summary = format!(
        "{} versions · {} variants{}",
        history.versions.len(),
        history.variants.len(),
        history
            .active_variant
            .as_ref()
            .map(|name| format!(" · active: {name}"))
            .unwrap_or_default()
    );
    project.history = Some(history);
    let yaml = save_project_yaml(&project).map_err(|e| e.to_string())?;
    Ok(json!({
        "scene_yaml": yaml,
        "history_summary": summary,
        "version_id": version_id
    }))
}

fn scene_animation_op(args: &Value) -> Result<Value, String> {
    let operation = args.get("op").and_then(Value::as_str).unwrap_or("");
    let root = args
        .get("root_id")
        .and_then(Value::as_str)
        .unwrap_or("humanoid");
    if operation == "play" {
        return Err("scene.animation play requires DeclUI runtime".into());
    }
    let mut project = load_project_from_args(args)?;
    let mut animation = project.animation.take().unwrap_or_default();
    match operation {
        "register" => animation
            .register_rig(&project.scene, root)
            .map_err(|e| e.to_string())?,
        "save_pose" => {
            let name = args.get("name").and_then(Value::as_str).unwrap_or("");
            animation
                .save_pose(&project.scene, root, name)
                .map_err(|e| e.to_string())?;
        }
        "apply_pose" => {
            let name = args.get("name").and_then(Value::as_str).unwrap_or("");
            animation
                .apply_pose(&mut project.scene, root, name)
                .map_err(|e| e.to_string())?;
        }
        "keyframe" => {
            let time_ms = args.get("time_ms").and_then(Value::as_f64).unwrap_or(0.0);
            if !time_ms.is_finite() || !(0.0..=60_000.0).contains(&time_ms) {
                return Err("time_ms must be 0..60000".into());
            }
            animation
                .add_keyframe(&project.scene, root, time_ms.round() as u32)
                .map_err(|e| e.to_string())?;
        }
        "seek" => {
            let time_ms = args.get("time_ms").and_then(Value::as_f64).unwrap_or(0.0);
            if !time_ms.is_finite() || !(0.0..=60_000.0).contains(&time_ms) {
                return Err("time_ms must be 0..60000".into());
            }
            animation
                .seek(&mut project.scene, time_ms.round() as u32)
                .map_err(|e| e.to_string())?;
        }
        "stop" => {}
        _ => return Err("unknown animation operation".into()),
    }
    project.animation = Some(animation.clone());
    let yaml = save_project_yaml(&project).map_err(|e| e.to_string())?;
    Ok(json!({
        "scene_yaml": yaml,
        "animation_summary": format!(
            "{} rigs · {} poses · {} keyframes · {} ms",
            animation.rigs.len(),
            animation.poses.len(),
            animation.keyframes.len(),
            animation.current_ms
        ),
    }))
}

fn parse_ops(args: &Value) -> Result<Vec<AgentEditOp>, String> {
    if let Some(ops) = args.get("ops") {
        return serde_json::from_value(ops.clone()).map_err(|e| format!("ops invalides: {e}"));
    }
    // Single-op convenience: treat the whole args object as one tagged op when
    // `op` is present, otherwise map service-specific fields.
    if args.get("op").is_some() {
        let op: AgentEditOp =
            serde_json::from_value(args.clone()).map_err(|e| format!("op invalide: {e}"))?;
        return Ok(vec![op]);
    }
    Err("ops ou op requis".into())
}

fn lock_scope(args: &Value) -> LockScope {
    match args.get("scope").and_then(|v| v.as_str()).unwrap_or("node") {
        "subtree" => LockScope::Subtree,
        _ => LockScope::Node,
    }
}

fn lock_kind(args: &Value) -> LockKind {
    if args.get("pose").and_then(|v| v.as_bool()).unwrap_or(false) {
        return LockKind::Pose;
    }
    match args.get("kind").and_then(|v| v.as_str()).unwrap_or("semantic") {
        "pose" => LockKind::Pose,
        _ => LockKind::Semantic,
    }
}

/// Handle `scene.*` host_call services. Returns `Ok(None)` when `service` is not a scene tool.
pub fn handle_scene_host_call(
    service: &str,
    ctx: &HostCallCtx,
    args: &Value,
) -> Result<Option<Value>, String> {
    // Restrict to illustration-studio guest (canvas pattern).
    if !service.starts_with("scene.") {
        return Ok(None);
    }
    if ctx.module != "illustration-studio" {
        return Err(format!("{service} réservé au module illustration-studio"));
    }

    match service {
        SCENE_GET_SERVICE => {
            require_any_cap(ctx, &[SCENE_EDIT_CAP, "fs.read:/documents/illustrations/**"])?;
            let snap = load_snap(args)?;
            Ok(Some(snap.result_json().map_err(|e| e.to_string())?))
        }
        SCENE_SELECT_SERVICE => {
            require_any_cap(ctx, &[SCENE_EDIT_CAP])?;
            let id = args
                .get("id")
                .or_else(|| args.get("selected_id"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| "id requis".to_string())?;
            let mut snap = load_snap(args)?;
            let op = AgentEditOp::Select { id: id.into() };
            require_edit_caps(&op, &ctx.granted_caps).map_err(|e| e.to_string())?;
            apply_one(&mut snap, &op, &ctx.actor, actor_kind(ctx)).map_err(|e| e.to_string())?;
            Ok(Some(snap.result_json().map_err(|e| e.to_string())?))
        }
        SCENE_TRS_SERVICE => {
            require_any_cap(ctx, &[SCENE_EDIT_CAP])?;
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "id requis".to_string())?;
            let translation = args
                .get("translation")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| format!("translation: {e}"))?;
            let rotation = args
                .get("rotation")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| format!("rotation: {e}"))?;
            let scale = args
                .get("scale")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| format!("scale: {e}"))?;
            let mut snap = load_snap(args)?;
            let op = AgentEditOp::Trs {
                id: id.into(),
                translation,
                rotation,
                scale,
            };
            require_edit_caps(&op, &ctx.granted_caps).map_err(|e| e.to_string())?;
            apply_one(&mut snap, &op, &ctx.actor, actor_kind(ctx)).map_err(|e| e.to_string())?;
            Ok(Some(snap.result_json().map_err(|e| e.to_string())?))
        }
        SCENE_CAMERA_SERVICE => {
            require_any_cap(ctx, &[SCENE_EDIT_CAP])?;
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let eye = args
                .get("eye")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| format!("eye: {e}"))?;
            let look_at = args
                .get("look_at")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| format!("look_at: {e}"))?;
            let fov_deg = args
                .get("fov_deg")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32);
            let yaw = args.get("yaw").and_then(|v| v.as_f64()).map(|v| v as f32);
            let pitch = args
                .get("pitch")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32);
            let distance = args
                .get("distance")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32);
            let mut snap = load_snap(args)?;
            let op = AgentEditOp::Camera {
                id,
                eye,
                look_at,
                fov_deg,
                yaw,
                pitch,
                distance,
            };
            require_edit_caps(&op, &ctx.granted_caps).map_err(|e| e.to_string())?;
            apply_one(&mut snap, &op, &ctx.actor, actor_kind(ctx)).map_err(|e| e.to_string())?;
            Ok(Some(snap.result_json().map_err(|e| e.to_string())?))
        }
        SCENE_LIGHT_SERVICE => {
            require_any_cap(ctx, &[SCENE_EDIT_CAP])?;
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let add = args.get("add").and_then(|v| v.as_bool()).unwrap_or(false);
            let parent_id = args
                .get("parent_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let name = args
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let translation = args
                .get("translation")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| format!("translation: {e}"))?;
            let light_type = args
                .get("light_type")
                .or_else(|| args.get("type"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let intensity = args
                .get("intensity")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32);
            let color = args
                .get("color")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| format!("color: {e}"))?;
            let color_srgb = args
                .get("color_srgb")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| format!("color_srgb: {e}"))?;
            let range = args.get("range").and_then(|v| v.as_f64()).map(|v| v as f32);
            let spot_angle_deg = args
                .get("spot_angle_deg")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32);
            let mut snap = load_snap(args)?;
            let op = AgentEditOp::Light {
                id,
                add,
                parent_id,
                name,
                translation,
                light_type,
                intensity,
                color,
                color_srgb,
                range,
                spot_angle_deg,
            };
            require_edit_caps(&op, &ctx.granted_caps).map_err(|e| e.to_string())?;
            apply_one(&mut snap, &op, &ctx.actor, actor_kind(ctx)).map_err(|e| e.to_string())?;
            Ok(Some(snap.result_json().map_err(|e| e.to_string())?))
        }
        SCENE_APPLY_SERVICE => {
            require_any_cap(ctx, &[SCENE_EDIT_CAP])?;
            let ops = parse_ops(args)?;
            require_batch_caps(&ops, &ctx.granted_caps).map_err(|e| e.to_string())?;
            // Pose/compose inside a batch still need their caps (checked above).
            let _ = (SCENE_POSE_CAP, SCENE_COMPOSE_CAP, ASSET_ILLUSTRATION_READ_CAP);
            let mut snap = load_snap(args)?;
            apply_batch(&mut snap, &ops, &ctx.actor, actor_kind(ctx))
                .map_err(|e| e.to_string())?;
            let mut out = snap.result_json().map_err(|e| e.to_string())?;
            out["applied"] = json!(ops.len());
            out["ok"] = json!(true);
            Ok(Some(out))
        }
        SCENE_LOCK_SERVICE => {
            require_any_cap(ctx, &[SCENE_LOCK_CAP])?;
            let id = args
                .get("id")
                .or_else(|| args.get("node_id"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| "id requis".to_string())?;
            let mut snap = load_snap(args)?;
            let op = AgentEditOp::Lock {
                id: id.into(),
                scope: lock_scope(args),
                kind: lock_kind(args),
            };
            require_edit_caps(&op, &ctx.granted_caps).map_err(|e| e.to_string())?;
            apply_one(&mut snap, &op, &ctx.actor, actor_kind(ctx)).map_err(|e| e.to_string())?;
            Ok(Some(snap.result_json().map_err(|e| e.to_string())?))
        }
        SCENE_UNLOCK_SERVICE => {
            require_any_cap(ctx, &[SCENE_LOCK_CAP])?;
            let id = args
                .get("id")
                .or_else(|| args.get("node_id"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| "id requis".to_string())?;
            let mut snap = load_snap(args)?;
            let op = AgentEditOp::Unlock { id: id.into() };
            require_edit_caps(&op, &ctx.granted_caps).map_err(|e| e.to_string())?;
            apply_one(&mut snap, &op, &ctx.actor, actor_kind(ctx)).map_err(|e| e.to_string())?;
            Ok(Some(snap.result_json().map_err(|e| e.to_string())?))
        }
        SCENE_LOCKS_SERVICE => {
            require_any_cap(ctx, &[SCENE_LOCK_CAP, SCENE_EDIT_CAP])?;
            let snap = load_snap(args)?;
            Ok(Some(json!({
                "locks": snap.project.locks.list().iter().map(|l: &SemanticLock| {
                    json!({
                        "node_id": l.node_id,
                        "scope": l.scope,
                        "kind": l.kind,
                        "holder": l.holder,
                    })
                }).collect::<Vec<_>>(),
                "scene_yaml": snap.to_yaml().map_err(|e| e.to_string())?,
            })))
        }
        SCENE_COMPOSE_SERVICE | SCENE_POSE_SERVICE => {
            // Agents may also call compose/pose via host_call (mirrors DeclUI).
            let mut snap = load_snap(args)?;
            let op = if service == SCENE_COMPOSE_SERVICE {
                require_any_cap(ctx, &[SCENE_COMPOSE_CAP])?;
                let prompt = args
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "prompt requis".to_string())?;
                AgentEditOp::Compose {
                    prompt: prompt.into(),
                }
            } else {
                require_any_cap(ctx, &[SCENE_POSE_CAP])?;
                let humanoid_root = args
                    .get("humanoid_root")
                    .and_then(|v| v.as_str())
                    .unwrap_or("humanoid")
                    .to_string();
                AgentEditOp::Pose {
                    humanoid_root,
                    preset: args
                        .get("preset")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    look_at: args
                        .get("look_at")
                        .cloned()
                        .map(serde_json::from_value)
                        .transpose()
                        .map_err(|e| format!("look_at: {e}"))?,
                    joint: args
                        .get("joint")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    axis: args
                        .get("axis")
                        .cloned()
                        .map(serde_json::from_value)
                        .transpose()
                        .map_err(|e| format!("axis: {e}"))?,
                    angle_rad: args.get("angle_rad").and_then(|v| v.as_f64()).map(|f| f as f32),
                }
            };
            require_edit_caps(&op, &ctx.granted_caps).map_err(|e| e.to_string())?;
            apply_one(&mut snap, &op, &ctx.actor, actor_kind(ctx)).map_err(|e| e.to_string())?;
            Ok(Some(snap.result_json().map_err(|e| e.to_string())?))
        }
        "scene.instantiate" => {
            require_any_cap(ctx, &[SCENE_EDIT_CAP])?;
            require_any_cap(ctx, &[ASSET_ILLUSTRATION_READ_CAP])?;
            let asset_id = args
                .get("asset_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "asset_id requis".to_string())?;
            let mut snap = load_snap(args)?;
            let op = AgentEditOp::Instantiate {
                asset_id: asset_id.into(),
                parent_id: args
                    .get("parent_id")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                prefix: args
                    .get("prefix")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
            };
            require_edit_caps(&op, &ctx.granted_caps).map_err(|e| e.to_string())?;
            apply_one(&mut snap, &op, &ctx.actor, actor_kind(ctx)).map_err(|e| e.to_string())?;
            Ok(Some(snap.result_json().map_err(|e| e.to_string())?))
        }
        "scene.diagnostics" => {
            require_any_cap(ctx, &[SCENE_EDIT_CAP, "fs.read:/documents/illustrations/**"])?;
            let project = load_project_from_args(args)?;
            Ok(Some(scene_diagnostics(&project)))
        }
        "scene.history" => {
            require_any_cap(ctx, &[SCENE_EDIT_CAP])?;
            Ok(Some(scene_history_op(args)?))
        }
        "scene.animation" => {
            require_any_cap(ctx, &[SCENE_POSE_CAP])?;
            Ok(Some(scene_animation_op(args)?))
        }
        other if other.starts_with("scene.") => {
            Err(format!("service scene inconnu: {other} (path défaut {DEFAULT_SCENE_YAML_PATH})"))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_scene::{save_project_yaml, ProjectFile, SceneGraph};

    fn ctx(caps: &[&str]) -> HostCallCtx {
        HostCallCtx {
            module: "illustration-studio".into(),
            module_dir: std::path::PathBuf::from("/tmp"),
            actor: "agent:test".into(),
            granted_caps: caps.iter().map(|s| (*s).to_string()).collect(),
            trace_id: "t".into(),
        }
    }

    #[test]
    fn apply_batch_respects_lock() {
        let mut project = ProjectFile::new(SceneGraph::demo_scene());
        project.locks.set(SemanticLock {
            node_id: "box".into(),
            scope: LockScope::Node,
            kind: LockKind::Semantic,
            holder: "user".into(),
        });
        let yaml = save_project_yaml(&project).unwrap();
        let args = json!({
            "scene_yaml": yaml,
            "ops": [{
                "op": "trs",
                "id": "box",
                "translation": {"x": 3.0, "y": 0.675, "z": 0.0}
            }]
        });
        let err = handle_scene_host_call(
            SCENE_APPLY_SERVICE,
            &ctx(&[SCENE_EDIT_CAP]),
            &args,
        )
        .unwrap_err();
        assert!(err.contains("blocked") || err.contains("lock") || err.contains("box"));
    }

    #[test]
    fn diagnostics_reports_node_counts() {
        let yaml = save_project_yaml(&ProjectFile::new(SceneGraph::demo_scene())).unwrap();
        let args = json!({ "scene_yaml": yaml });
        let out = handle_scene_host_call("scene.diagnostics", &ctx(&[SCENE_EDIT_CAP]), &args)
            .unwrap()
            .unwrap();
        let summary = out["diagnostics"].as_str().unwrap_or("");
        assert!(summary.contains("nodes"));
    }

    #[test]
    fn select_and_trs_ok() {
        let yaml = save_project_yaml(&ProjectFile::new(SceneGraph::demo_scene())).unwrap();
        let args = json!({
            "scene_yaml": yaml,
            "ops": [
                {"op": "select", "id": "box"},
                {"op": "trs", "id": "box", "translation": {"x": 1.0, "y": 0.675, "z": 0.0}}
            ]
        });
        let out = handle_scene_host_call(SCENE_APPLY_SERVICE, &ctx(&[SCENE_EDIT_CAP]), &args)
            .unwrap()
            .unwrap();
        assert_eq!(out["selected_id"], "box");
        assert_eq!(out["applied"], 2);
    }
}
