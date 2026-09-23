// SPDX-License-Identifier: Apache-2.0
//! Illustration Studio — SceneGraph project save/load (YAML).
//!
//! Host owns `scene3d` orbit/select/TRS. This guest only persists project YAML
//! under `/documents/illustrations/**` and seeds a Preview starter SceneGraph.

use serde_json::json;

const PROJECT_PATH: &str = "/documents/illustrations/project.scene.yaml";
const STATE_PATH: &str = "/documents/illustrations/state.json";

/// Preview starter (ADR 0011: Y-up, quat xyzw, metres). Mirrors
/// `SceneGraph::demo_scene` articulated humanoid (Empty joints + MeshBox visuals).
const DEMO_PROJECT_YAML: &str = include_str!("../demo.scene.yaml");

fn handle(tool: &str, args: &serde_json::Value) -> Result<serde_json::Value, String> {
    match tool {
        "illustration.project.load" => project_load(),
        "illustration.project.save" => project_save(args),
        "illustration.project.ensure" => project_ensure(),
        "illustration.document.load" => document_load(),
        "illustration.document.save" => document_save(args),
        // Agent co-edit surface (host_call → platform scene_host).
        "scene.get"
        | "scene.select"
        | "scene.trs"
        | "scene.camera"
        | "scene.light"
        | "scene.apply"
        | "scene.lock"
        | "scene.unlock"
        | "scene.locks"
        | "scene.compose"
        | "scene.pose"
        | "scene.instantiate" => scene_tool(tool, args),
        _ => Err(format!("unknown tool: {tool}")),
    }
}

/// Ensure `scene_yaml` is present (from args or project file) then host_call.
fn scene_tool(service: &str, args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let mut payload = args.clone();
    if payload.get("scene_yaml").and_then(|v| v.as_str()).unwrap_or("").is_empty()
        && service != "scene.compose"
    {
        let yaml = project_ensure()?["yaml"]
            .as_str()
            .ok_or_else(|| "project ensure returned no yaml".to_string())?
            .to_string();
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("scene_yaml".into(), json!(yaml));
        }
    }
    let result = aos_module_sdk::call(service, &payload)?;
    // Persist when the host returned an updated scene (audit-friendly SoT on disk).
    if let Some(yaml) = result.get("scene_yaml").and_then(|v| v.as_str()) {
        if !yaml.trim().is_empty() && service != "scene.get" && service != "scene.locks" {
            validate_project_yaml(yaml)?;
            let _ = aos_module_sdk::fs_write(PROJECT_PATH, yaml)?;
        }
    }
    Ok(result)
}

fn project_ensure() -> Result<serde_json::Value, String> {
    match aos_module_sdk::fs_read(PROJECT_PATH) {
        Ok(yaml) => {
            validate_project_yaml(&yaml)?;
            Ok(json!({ "path": PROJECT_PATH, "yaml": yaml }))
        }
        Err(read_error) => {
            let paths = aos_module_sdk::fs_list("/documents/illustrations/")
                .map_err(|list_error| format!("cannot inspect illustration project: {list_error}; read failed: {read_error}"))?;
            if paths.iter().any(|path| path == PROJECT_PATH) {
                return Err(format!("cannot read existing project {PROJECT_PATH}: {read_error}"));
            }
            validate_project_yaml(DEMO_PROJECT_YAML)?;
            aos_module_sdk::fs_write(PROJECT_PATH, DEMO_PROJECT_YAML)?;
            let yaml = aos_module_sdk::fs_read(PROJECT_PATH)?;
            validate_project_yaml(&yaml)?;
            Ok(json!({ "path": PROJECT_PATH, "yaml": yaml }))
        }
    }
}

fn validate_project_yaml(yaml: &str) -> Result<(), String> {
    aos_scene::load_project_yaml(yaml)
        .map(|_| ())
        .map_err(|error| format!("invalid illustration project yaml: {error}"))
}

fn project_load() -> Result<serde_json::Value, String> {
    project_ensure()
}

fn project_save(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let yaml = args
        .get("yaml")
        .and_then(|v| v.as_str())
        .or_else(|| args.get("scene_yaml").and_then(|v| v.as_str()))
        .ok_or_else(|| "missing yaml".to_string())?;
    if yaml.trim().is_empty() {
        return Err("empty yaml".into());
    }
    validate_project_yaml(yaml)?;
    let _ = aos_module_sdk::fs_write(PROJECT_PATH, yaml)?;
    Ok(json!({ "path": PROJECT_PATH, "ok": true }))
}

fn document_load() -> Result<serde_json::Value, String> {
    match aos_module_sdk::fs_read(STATE_PATH) {
        Ok(raw) => {
            let v: serde_json::Value =
                serde_json::from_str(&raw).unwrap_or_else(|_| json!({ "selected_id": "box" }));
            Ok(v)
        }
        Err(_) => Ok(json!({ "selected_id": "box" })),
    }
}

fn document_save(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let raw = serde_json::to_string_pretty(args).map_err(|e| e.to_string())?;
    let _ = aos_module_sdk::fs_write(STATE_PATH, &raw)?;
    Ok(json!({ "ok": true }))
}

aos_module_sdk::export_module!(handle);

#[cfg(test)]
mod tests {
    #[test]
    fn demo_yaml_mentions_adr_and_starter_nodes() {
        assert!(super::DEMO_PROJECT_YAML.contains("ADR-0011"));
        assert!(super::DEMO_PROJECT_YAML.contains("mesh_box"));
        assert!(super::DEMO_PROJECT_YAML.contains("humanoid"));
        assert!(super::DEMO_PROJECT_YAML.contains("pelvis"));
        assert!(super::DEMO_PROJECT_YAML.contains("upper_arm_r"));
        assert!(super::DEMO_PROJECT_YAML.contains("ground"));
        assert!(super::DEMO_PROJECT_YAML.contains("pedestal"));
    }

    #[test]
    fn demo_project_yaml_is_valid() {
        super::validate_project_yaml(super::DEMO_PROJECT_YAML).unwrap();
    }

    #[test]
    fn malformed_project_yaml_is_rejected() {
        assert!(super::validate_project_yaml("scene: [broken").is_err());
    }
}
