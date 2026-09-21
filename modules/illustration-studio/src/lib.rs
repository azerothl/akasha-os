// SPDX-License-Identifier: Apache-2.0
//! Illustration Studio — SceneGraph project save/load (YAML).
//!
//! Host owns `scene3d` orbit/select/TRS. This guest only persists project YAML
//! under `/documents/illustrations/**` and seeds a Preview starter SceneGraph.

use serde_json::json;

const PROJECT_PATH: &str = "/documents/illustrations/project.scene.yaml";
const STATE_PATH: &str = "/documents/illustrations/state.json";

/// Preview starter (ADR 0011: Y-up, quat xyzw, metres). Kept inline so the
/// guest does not depend on host crates. Mirrors `SceneGraph::demo_scene`.
const DEMO_PROJECT_YAML: &str = r#"format_version: 1
conventions: ADR-0011
scene:
  roots:
    - root
  active_camera: camera
  nodes:
    root:
      id: root
      name: Scene
      kind: empty
      children: [ground, pedestal, box, humanoid, camera]
      transform:
        translation: { x: 0.0, y: 0.0, z: 0.0 }
        rotation: { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }
        scale: { x: 1.0, y: 1.0, z: 1.0 }
      visible: true
    ground:
      id: ground
      name: Ground
      kind: mesh_box
      parent: root
      children: []
      transform:
        translation: { x: 0.0, y: 0.04, z: 0.0 }
        rotation: { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }
        scale: { x: 6.0, y: 0.08, z: 6.0 }
      visible: true
    pedestal:
      id: pedestal
      name: Pedestal
      kind: mesh_box
      parent: root
      children: []
      transform:
        translation: { x: 0.0, y: 0.175, z: 0.0 }
        rotation: { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }
        scale: { x: 1.2, y: 0.35, z: 1.2 }
      visible: true
    box:
      id: box
      name: Box
      kind: mesh_box
      parent: root
      children: []
      transform:
        translation: { x: 0.0, y: 0.675, z: 0.0 }
        rotation: { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }
        scale: { x: 0.7, y: 0.7, z: 0.7 }
      visible: true
    humanoid:
      id: humanoid
      name: Humanoid
      kind: empty
      parent: root
      children: [torso, head, leg_l, leg_r, arm_l, arm_r]
      transform:
        translation: { x: 1.4, y: 0.0, z: 0.3 }
        rotation: { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }
        scale: { x: 1.0, y: 1.0, z: 1.0 }
      visible: true
    torso:
      id: torso
      name: Torso
      kind: mesh_box
      parent: humanoid
      children: []
      transform:
        translation: { x: 0.0, y: 1.0, z: 0.0 }
        rotation: { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }
        scale: { x: 0.45, y: 0.55, z: 0.25 }
      visible: true
    head:
      id: head
      name: Head
      kind: mesh_box
      parent: humanoid
      children: []
      transform:
        translation: { x: 0.0, y: 1.55, z: 0.0 }
        rotation: { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }
        scale: { x: 0.22, y: 0.22, z: 0.22 }
      visible: true
    leg_l:
      id: leg_l
      name: LegL
      kind: mesh_box
      parent: humanoid
      children: []
      transform:
        translation: { x: -0.12, y: 0.35, z: 0.0 }
        rotation: { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }
        scale: { x: 0.14, y: 0.7, z: 0.14 }
      visible: true
    leg_r:
      id: leg_r
      name: LegR
      kind: mesh_box
      parent: humanoid
      children: []
      transform:
        translation: { x: 0.12, y: 0.35, z: 0.0 }
        rotation: { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }
        scale: { x: 0.14, y: 0.7, z: 0.14 }
      visible: true
    arm_l:
      id: arm_l
      name: ArmL
      kind: mesh_box
      parent: humanoid
      children: []
      transform:
        translation: { x: -0.38, y: 1.05, z: 0.0 }
        rotation: { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }
        scale: { x: 0.12, y: 0.5, z: 0.12 }
      visible: true
    arm_r:
      id: arm_r
      name: ArmR
      kind: mesh_box
      parent: humanoid
      children: []
      transform:
        translation: { x: 0.38, y: 1.05, z: 0.0 }
        rotation: { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }
        scale: { x: 0.12, y: 0.5, z: 0.12 }
      visible: true
    camera:
      id: camera
      name: Camera
      kind: camera
      parent: root
      children: []
      transform:
        translation: { x: 0.0, y: 2.2, z: 6.5 }
        rotation: { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }
        scale: { x: 1.0, y: 1.0, z: 1.0 }
      camera:
        focal_mm: 50.0
        sensor_width_mm: 36.0
        near: 0.1
        far: 100.0
      visible: true
"#;

fn handle(tool: &str, args: &serde_json::Value) -> Result<serde_json::Value, String> {
    match tool {
        "illustration.project.load" => project_load(),
        "illustration.project.save" => project_save(args),
        "illustration.project.ensure" => project_ensure(),
        "illustration.document.load" => document_load(),
        "illustration.document.save" => document_save(args),
        _ => Err(format!("unknown tool: {tool}")),
    }
}

fn project_ensure() -> Result<serde_json::Value, String> {
    if aos_module_sdk::fs_read(PROJECT_PATH).is_err() {
        let _ = aos_module_sdk::fs_write(PROJECT_PATH, DEMO_PROJECT_YAML)?;
    }
    let yaml = aos_module_sdk::fs_read(PROJECT_PATH)?;
    Ok(json!({ "path": PROJECT_PATH, "yaml": yaml }))
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
        assert!(super::DEMO_PROJECT_YAML.contains("ground"));
        assert!(super::DEMO_PROJECT_YAML.contains("pedestal"));
    }
}
