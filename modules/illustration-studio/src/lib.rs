// SPDX-License-Identifier: Apache-2.0
//! Managed Illustration Studio projects. SceneGraph YAML remains the scene source of truth.
#![allow(clippy::not_unsafe_ptr_arg_deref)] // Generated WASM ABI export in aos_module_sdk.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const ROOT: &str = "/documents/illustrations";
const INDEX_PATH: &str = "/documents/illustrations/projects/index.json";
const LEGACY_PATH: &str = "/documents/illustrations/project.scene.yaml";

#[derive(Debug, Default, Serialize, Deserialize)]
struct ProjectIndex {
    #[serde(default)]
    next_id: u64,
    #[serde(default)]
    sequence: u64,
    #[serde(default)]
    projects: Vec<ProjectEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectEntry {
    id: String,
    title: String,
    last_opened: u64,
    #[serde(default = "default_work_area")]
    work_area: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectAsset {
    project_id: String,
    id: String,
    name: String,
    kind: String,
    uri: String,
    #[serde(default)]
    metadata: Value,
    #[serde(default)]
    prompt: Option<String>,
}

fn default_work_area() -> String {
    "start".into()
}

fn is_scene_tool(tool: &str) -> bool {
    matches!(
        tool,
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
            | "scene.instantiate"
            | "scene.animation"
            | "scene.history"
            | "scene.diagnostics"
    )
}

fn handle(tool: &str, args: &Value) -> Result<Value, String> {
    match tool {
        "illustration.project.list" => project_list(),
        "illustration.project.create" => project_create(args),
        "illustration.project.open" | "illustration.project.load" => project_open(args),
        "illustration.project.save" => project_save(args),
        "illustration.project.close" => {
            project_save(args)?;
            Ok(json!({ "closed": true }))
        }
        "illustration.project.work_area" => project_work_area(args),
        "illustration.project.import_legacy" => project_import_legacy(),
        "illustration.asset.list" => asset_list(args),
        "illustration.asset.register" => asset_register(args),
        "illustration.asset.update" => asset_update(args),
        "illustration.asset.add" => asset_add(args),
        "illustration.asset.select" => asset_select(args),
        "illustration.document.load" => document_load(args),
        "illustration.document.save" => document_save(args),
        tool if is_scene_tool(tool) => scene_tool(tool, args),
        _ => Err(format!("unknown tool: {tool}")),
    }
}

fn project_path(id: &str) -> Result<String, String> {
    if !id.starts_with("project-")
        || !id["project-".len()..].chars().all(|c| c.is_ascii_digit())
        || id.len() > 32
    {
        return Err("invalid project id".into());
    }
    Ok(format!("{ROOT}/projects/{id}/scene.yaml"))
}

fn state_path(id: &str) -> Result<String, String> {
    project_path(id)?;
    Ok(format!("{ROOT}/projects/{id}/state.json"))
}

fn assets_path(id: &str) -> Result<String, String> {
    project_path(id)?;
    Ok(format!("{ROOT}/projects/{id}/assets.json"))
}

fn read_index() -> Result<ProjectIndex, String> {
    match aos_module_sdk::fs_read(INDEX_PATH) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| format!("invalid project index: {e}")),
        Err(read_error) => {
            let paths = aos_module_sdk::fs_list(&format!("{ROOT}/projects/"))?;
            if paths.iter().any(|p| p == INDEX_PATH) {
                Err(format!("cannot read project index: {read_error}"))
            } else {
                Ok(ProjectIndex::default())
            }
        }
    }
}

fn write_index(index: &ProjectIndex) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(index).map_err(|e| e.to_string())?;
    aos_module_sdk::fs_write(INDEX_PATH, &raw).map(|_| ())
}

fn required_project_id(args: &Value) -> Result<&str, String> {
    args.get("project_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| "open a project first".into())
}

fn known_project<'a>(index: &'a ProjectIndex, id: &str) -> Result<&'a ProjectEntry, String> {
    index
        .projects
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| format!("project {id} does not exist"))
}

fn validate_project_yaml(yaml: &str) -> Result<(), String> {
    aos_scene::load_project_yaml(yaml)
        .map(|_| ())
        .map_err(|error| format!("invalid illustration project yaml: {error}"))
}

fn empty_project_yaml() -> Result<String, String> {
    let scene = aos_scene::SceneGraph {
        effects: Vec::new(),
        nodes: Default::default(),
        roots: Vec::new(),
        active_camera: None,
    };
    aos_scene::save_project_yaml(&aos_scene::ProjectFile::new(scene))
        .map_err(|e| e.to_string())
}

fn project_list() -> Result<Value, String> {
    let mut entries = read_index()?.projects;
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.last_opened));
    let legacy_available = aos_module_sdk::fs_list(&format!("{ROOT}/"))?
        .iter()
        .any(|path| path == LEGACY_PATH);
    Ok(json!({ "projects": entries, "legacy_available": legacy_available }))
}

fn project_create(args: &Value) -> Result<Value, String> {
    let title = args
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if title.is_empty() || title.chars().count() > 80 {
        return Err("project name must contain 1–80 characters".into());
    }
    let yaml = match args.get("yaml").and_then(Value::as_str) {
        Some(yaml) => {
            validate_project_yaml(yaml)?;
            yaml.to_owned()
        }
        None => empty_project_yaml()?,
    };
    let mut index = read_index()?;
    let existing = aos_module_sdk::fs_list(&format!("{ROOT}/projects/"))?;
    let id = loop {
        index.next_id = index.next_id.saturating_add(1);
        let candidate = format!("project-{:06}", index.next_id);
        if !index.projects.iter().any(|p| p.id == candidate)
            && !existing.iter().any(|p| p == &project_path(&candidate).unwrap())
        {
            break candidate;
        }
    };
    index.sequence = index.sequence.saturating_add(1);
    let entry = ProjectEntry {
        id: id.clone(),
        title: title.to_owned(),
        last_opened: index.sequence,
        work_area: default_work_area(),
    };
    aos_module_sdk::fs_write(&project_path(&id)?, &yaml)?;
    index.projects.push(entry.clone());
    write_index(&index)?;
    Ok(json!({ "project_id": id, "title": title, "yaml": yaml, "work_area": entry.work_area, "assets": [] }))
}

fn project_open(args: &Value) -> Result<Value, String> {
    let id = required_project_id(args)?;
    let mut index = read_index()?;
    let entry = known_project(&index, id)?.clone();
    let yaml = aos_module_sdk::fs_read(&project_path(id)?)?;
    validate_project_yaml(&yaml)?;
    index.sequence = index.sequence.saturating_add(1);
    if let Some(item) = index.projects.iter_mut().find(|p| p.id == id) {
        item.last_opened = index.sequence;
    }
    write_index(&index)?;
    Ok(json!({ "project_id": id, "title": entry.title, "yaml": yaml, "work_area": entry.work_area, "assets": read_assets(id)? }))
}

fn project_save(args: &Value) -> Result<Value, String> {
    let id = required_project_id(args)?;
    known_project(&read_index()?, id)?;
    let yaml = args
        .get("yaml")
        .and_then(Value::as_str)
        .or_else(|| args.get("scene_yaml").and_then(Value::as_str))
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "missing project yaml".to_string())?;
    validate_project_yaml(yaml)?;
    let path = project_path(id)?;
    aos_module_sdk::fs_write(&path, yaml)?;
    Ok(json!({ "project_id": id, "path": path, "ok": true }))
}

fn project_work_area(args: &Value) -> Result<Value, String> {
    let id = required_project_id(args)?;
    let area = args
        .get("work_area")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing work_area".to_string())?;
    if !matches!(area, "start" | "scene3d" | "illustration" | "comic" | "library" | "final") {
        return Err("unknown work area".into());
    }
    let mut index = read_index()?;
    let entry = index
        .projects
        .iter_mut()
        .find(|entry| entry.id == id)
        .ok_or_else(|| format!("project {id} does not exist"))?;
    entry.work_area = area.into();
    write_index(&index)?;
    Ok(json!({ "project_id": id, "work_area": area, "compose": args.get("compose").and_then(Value::as_bool).unwrap_or(false) }))
}

fn project_import_legacy() -> Result<Value, String> {
    let yaml = aos_module_sdk::fs_read(LEGACY_PATH)?;
    validate_project_yaml(&yaml)?;
    project_create(&json!({ "title": "Imported legacy project", "yaml": yaml }))
}

fn document_load(args: &Value) -> Result<Value, String> {
    let id = required_project_id(args)?;
    known_project(&read_index()?, id)?;
    match aos_module_sdk::fs_read(&state_path(id)?) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| format!("invalid project state: {e}")),
        Err(_) => Ok(json!({})),
    }
}

fn document_save(args: &Value) -> Result<Value, String> {
    let id = required_project_id(args)?;
    known_project(&read_index()?, id)?;
    let raw = serde_json::to_string_pretty(args).map_err(|e| e.to_string())?;
    aos_module_sdk::fs_write(&state_path(id)?, &raw)?;
    Ok(json!({ "project_id": id, "ok": true }))
}

fn read_assets(id: &str) -> Result<Vec<ProjectAsset>, String> {
    known_project(&read_index()?, id)?;
    let path = assets_path(id)?;
    match aos_module_sdk::fs_read(&path) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| format!("invalid asset inventory: {e}")),
        Err(read_error) => {
            if aos_module_sdk::fs_list(&format!("{ROOT}/projects/{id}/"))?
                .iter()
                .any(|existing| existing == &path)
            {
                Err(format!("cannot read asset inventory: {read_error}"))
            } else {
                Ok(Vec::new())
            }
        }
    }
}

fn write_assets(id: &str, assets: &[ProjectAsset]) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(assets).map_err(|e| e.to_string())?;
    aos_module_sdk::fs_write(&assets_path(id)?, &raw).map(|_| ())
}

fn asset_list(args: &Value) -> Result<Value, String> {
    let id = required_project_id(args)?;
    Ok(json!({ "project_id": id, "items": read_assets(id)? }))
}

fn asset_register(args: &Value) -> Result<Value, String> {
    let id = required_project_id(args)?;
    let name = args.get("name").and_then(Value::as_str).unwrap_or("").trim();
    let kind = args.get("kind").and_then(Value::as_str).unwrap_or("");
    let uri = args.get("uri").and_then(Value::as_str).unwrap_or("");
    if name.is_empty() || name.chars().count() > 120 {
        return Err("asset name must contain 1–120 characters".into());
    }
    if !matches!(kind, "image" | "mesh") {
        return Err("asset kind must be image or mesh".into());
    }
    if !uri.starts_with(&format!("{ROOT}/projects/{id}/assets/"))
        && !uri.starts_with(&format!("{ROOT}/assets/"))
    {
        return Err("asset URI is outside Illustration Studio storage".into());
    }
    if uri.contains("..") || uri.contains('\\') {
        return Err("invalid asset URI".into());
    }
    let mut assets = read_assets(id)?;
    if let Some(existing) = assets.iter().find(|asset| asset.uri == uri) {
        return Ok(json!({ "project_id": id, "asset": existing, "items": assets }));
    }
    let next = assets
        .iter()
        .filter_map(|asset| asset.id.strip_prefix("asset-")?.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let asset = ProjectAsset {
        project_id: id.to_owned(),
        id: format!("asset-{next:06}"),
        name: name.to_owned(),
        kind: kind.to_owned(),
        uri: uri.to_owned(),
        metadata: args.get("metadata").cloned().unwrap_or(Value::Null),
        prompt: args.get("prompt").and_then(Value::as_str).map(str::to_owned),
    };
    assets.push(asset.clone());
    write_assets(id, &assets)?;
    Ok(json!({ "project_id": id, "asset": asset, "items": assets }))
}

fn asset_update(args: &Value) -> Result<Value, String> {
    let project_id = required_project_id(args)?;
    let asset_id = args.get("asset_id").and_then(Value::as_str)
        .ok_or_else(|| "missing asset_id".to_string())?;
    let mut assets = read_assets(project_id)?;
    let asset = assets.iter_mut().find(|asset| asset.id == asset_id)
        .ok_or_else(|| "asset not found in this project".to_string())?;
    if let Some(name) = args.get("name").and_then(Value::as_str) {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 120 {
            return Err("asset name must contain 1–120 characters".into());
        }
        asset.name = name.into();
    }
    if let Some(updates) = args.get("metadata").and_then(Value::as_object) {
        let mut metadata = asset.metadata.as_object().cloned().unwrap_or_default();
        for (key, value) in updates {
            match key.as_str() {
                "category" | "orientation" => {
                    let text = value.as_str().ok_or_else(|| format!("{key} must be text"))?.trim();
                    if text.chars().count() > 80 {
                        return Err(format!("{key} is too long"));
                    }
                    metadata.insert(key.clone(), json!(text));
                }
                "tags" => {
                    let tags = value.as_array().ok_or("tags must be a list")?;
                    if tags.len() > 16 || tags.iter().any(|tag| tag.as_str().is_none_or(|text| text.chars().count() > 32)) {
                        return Err("tags must contain at most 16 short labels".into());
                    }
                    metadata.insert(key.clone(), value.clone());
                }
                "dimensions_m" | "pivot_m" => {
                    if value.is_null() {
                        metadata.remove(key);
                        continue;
                    }
                    let values = value.as_array().ok_or_else(|| format!("{key} must be a 3D vector"))?;
                    if values.len() != 3 || values.iter().any(|item| item.as_f64().is_none_or(|number| !number.is_finite() || number.abs() > 1000.0 || key == "dimensions_m" && number < 0.0)) {
                        return Err(format!("{key} must contain three finite metre values"));
                    }
                    metadata.insert(key.clone(), value.clone());
                }
                _ => return Err(format!("{key} cannot be edited")),
            }
        }
        asset.metadata = Value::Object(metadata);
    }
    let updated = asset.clone();
    write_assets(project_id, &assets)?;
    Ok(json!({ "project_id": project_id, "asset": updated, "items": assets }))
}

fn asset_select(args: &Value) -> Result<Value, String> {
    let id = required_project_id(args)?;
    let asset_id = args
        .get("asset_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing asset_id".to_string())?;
    let asset = read_assets(id)?
        .into_iter()
        .find(|asset| asset.id == asset_id)
        .ok_or_else(|| "asset not found in this project".to_string())?;
    Ok(json!({ "project_id": id, "asset": asset }))
}

fn asset_add(args: &Value) -> Result<Value, String> {
    let id = required_project_id(args)?;
    let asset_id = args
        .get("asset_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing asset_id".to_string())?;
    let asset = read_assets(id)?
        .into_iter()
        .find(|asset| asset.id == asset_id)
        .ok_or_else(|| "asset not found in this project".to_string())?;
    if asset.kind != "mesh" {
        return Err("convert the image to a GLB before adding it to the 3D scene".into());
    }
    let yaml = aos_module_sdk::fs_read(&project_path(id)?)?;
    let mut project = aos_scene::load_project_yaml(&yaml).map_err(|e| e.to_string())?;
    if !project.scene.nodes.contains_key("root") {
        project.scene.nodes.insert("root".into(), aos_scene::SceneNode::empty("root", "Scene"));
        project.scene.roots.push("root".into());
    }
    let mut ordinal = 1u64;
    let node_id = loop {
        let candidate = format!("library_asset_{ordinal}");
        if !project.scene.nodes.contains_key(&candidate) {
            break candidate;
        }
        ordinal = ordinal.saturating_add(1);
    };
    aos_scene::insert_mesh_asset(
        &mut project.scene,
        "root",
        &node_id,
        asset.name,
        &asset.uri,
        aos_scene::Transform::default(),
    )
    .map_err(|e| e.to_string())?;
    let yaml = aos_scene::save_project_yaml(&project).map_err(|e| e.to_string())?;
    aos_module_sdk::fs_write(&project_path(id)?, &yaml)?;
    Ok(json!({ "project_id": id, "scene_yaml": yaml, "root_id": node_id }))
}

fn scene_tool(service: &str, args: &Value) -> Result<Value, String> {
    let mut payload = args.clone();
    let id = required_project_id(args)?;
    known_project(&read_index()?, id)?;
    if payload.get("scene_yaml").and_then(Value::as_str).unwrap_or("").is_empty()
        && service != "scene.compose"
    {
        let yaml = aos_module_sdk::fs_read(&project_path(id)?)?;
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("scene_yaml".into(), json!(yaml));
        }
    }
    let result = aos_module_sdk::call(service, &payload)?;
    if let Some(yaml) = result.get("scene_yaml").and_then(Value::as_str) {
        if !yaml.trim().is_empty()
            && service != "scene.get"
            && service != "scene.locks"
            && service != "scene.diagnostics"
        {
            validate_project_yaml(yaml)?;
            aos_module_sdk::fs_write(&project_path(id)?, yaml)?;
        }
    }
    Ok(result)
}

aos_module_sdk::export_module!(handle);

#[cfg(test)]
mod tests {
    #[test]
    fn empty_project_is_valid_and_contains_no_demo_assets() {
        let yaml = super::empty_project_yaml().unwrap();
        let project = aos_scene::load_project_yaml(&yaml).unwrap();
        assert!(project.scene.nodes.is_empty());
    }

    #[test]
    fn project_ids_cannot_escape_managed_root() {
        assert!(super::project_path("../project-1").is_err());
        assert!(super::project_path("project-000001").is_ok());
    }

    #[test]
    fn manifest_scene_tools_are_dispatched() {
        let manifest = include_str!("../manifest.yaml");
        let mut scene_tools = Vec::new();
        let mut in_tools = false;
        for line in manifest.lines() {
            let trimmed = line.trim();
            if trimmed == "tools:" {
                in_tools = true;
                continue;
            }
            if in_tools && trimmed.starts_with("- name:") {
                let name = trimmed.strip_prefix("- name:").unwrap().trim();
                if name.starts_with("scene.") {
                    scene_tools.push(name);
                }
            }
        }
        for tool in scene_tools {
            assert!(
                super::is_scene_tool(tool),
                "manifest advertises {tool} but handle() does not dispatch scene tools"
            );
        }
    }
}
