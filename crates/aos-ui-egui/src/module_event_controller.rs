//! Event handlers for module catalogues and declarative module UIs.

use crate::cmd::Cmd;
use crate::{decl_ui, Tab, UiApp};
use aos_proto::decl_ui::ModuleUiResponse;
use aos_proto::rich_decl_ui::resolve_action_input;
use aos_proto::{ModuleCatalogue, ModuleInfo, SkillInfo};
use serde_json::Value;

/// Re-run DeclUI `refresh_binds` after a successful action.
///
/// Bind names are historically guest binding *tool* ids (e.g. `illustration.project.list`).
/// Illustration Studio also lists DeclUI *action* ids that map to host services
/// (`mesh_pack_status`, `blender_pack_status`). Those must not go through
/// `module.invoke` — that yields `outil inconnu` and surfaces as an internal error
/// even when the preceding install succeeded.
fn dispatch_refresh_binds(app: &UiApp, module: &str, refresh_binds: Vec<String>) {
    if refresh_binds.is_empty() {
        return;
    }
    let panel = match app.decl_panels.get(module) {
        Some(panel) => panel,
        None => {
            for bind in refresh_binds {
                let _ = app.cmd_tx.send(Cmd::ModuleUiBind {
                    module: module.to_string(),
                    tool: bind,
                });
            }
            return;
        }
    };
    let language = app.prefs.language.clone();
    for bind in refresh_binds {
        if let Some(action) = panel
            .document
            .as_ref()
            .and_then(|doc| doc.actions.iter().find(|a| a.id == bind))
        {
            if action.service.is_some() || action.tool.is_some() {
                let mut input = action
                    .input
                    .as_ref()
                    .map(|t| resolve_action_input(t, &panel.local_state, &panel.document_state))
                    .unwrap_or_else(|| Value::Object(Default::default()));
                if matches!(
                    action.service.as_deref(),
                    Some(aos_proto::MESH_PACK_STATUS_SERVICE)
                        | Some(aos_proto::RENDER_PACK_STATUS_SERVICE)
                ) {
                    if let Some(obj) = input.as_object_mut() {
                        obj.insert("lang".into(), Value::String(language.clone()));
                    }
                }
                let subscription_id = panel.document.as_ref().and_then(|doc| {
                    doc.subscriptions
                        .iter()
                        .find(|s| s.action.as_deref() == Some(action.id.as_str()))
                        .map(|s| s.id.clone())
                });
                let _ = app.cmd_tx.send(Cmd::ModuleUiServiceAction {
                    module: module.to_string(),
                    action_id: action.id.clone(),
                    service: action.service.clone(),
                    tool: action.tool.clone(),
                    input,
                    // Nested refreshes stay empty to avoid action→action loops.
                    refresh_binds: Vec::new(),
                    subscription_id,
                });
                continue;
            }
        }
        let tool = panel
            .document
            .as_ref()
            .and_then(|doc| {
                doc.bindings
                    .iter()
                    .find(|b| b.id == bind || b.tool == bind)
                    .map(|b| b.tool.clone())
            })
            .unwrap_or(bind);
        let _ = app.cmd_tx.send(Cmd::ModuleUiBind {
            module: module.to_string(),
            tool,
        });
    }
}

fn augment_create_catalog(mut result: Value, french: bool) -> Value {
    let Some(root) = result.as_object_mut() else {
        return result;
    };
    let default_image = root
        .get("default_image")
        .and_then(Value::as_str)
        .map(str::to_string);
    let default_video = root
        .get("default_video")
        .and_then(Value::as_str)
        .map(str::to_string);
    // The guest module owns the catalogue ids, while the host owns the
    // install registry and user asset folders. Enrich the binding here so a
    // declarative module keeps the same installed/not-installed semantics as
    // the former native Create panel without granting it ambient filesystem
    // access.
    // Use the same catalogue as the native Models/Create surfaces. The guest
    // list is only an offline fallback and must not drift from installed
    // offerings or modality semantics.
    for mode in ["image", "video"] {
        let rows: Vec<Value> = crate::models_page::load_catalog_models()
            .into_iter()
            .filter(|model| {
                model
                    .modality
                    .as_deref()
                    .map(|modality| modality == mode)
                    .unwrap_or_else(|| model.profiles.iter().any(|profile| profile == mode))
            })
            .map(|model| {
                let state = crate::models_page::model_install_state(&model.id);
                let (installed, suffix) = match state {
                    crate::models_page::ModelInstallState::Complete => {
                        if french {
                            (true, "installé")
                        } else {
                            (true, "installed")
                        }
                    }
                    crate::models_page::ModelInstallState::Incomplete => {
                        if french {
                            (false, "incomplet")
                        } else {
                            (false, "incomplete")
                        }
                    }
                    crate::models_page::ModelInstallState::Missing => {
                        if french {
                            (false, "non installé")
                        } else {
                            (false, "not installed")
                        }
                    }
                };
                serde_json::json!({
                    "id": model.id,
                    "label": format!("{} ({suffix})", model.name),
                    "installed": installed,
                })
            })
            .collect();
        root.insert(mode.to_string(), Value::Array(rows));
    }
    let home = crate::os_open::aos_home();
    let mut assets = serde_json::Map::new();
    for (key, dirs) in [
        ("styles", &["share/models/styles", "share/models/style"][..]),
        ("loras", &["share/models/lora"][..]),
        ("vaes", &["share/models/vae"][..]),
        ("upscalers", &["share/models/upscale"][..]),
    ] {
        let mut names = Vec::new();
        for rel in dirs {
            let dir = home.join(rel);
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if entry.path().is_file() {
                        if let Some(name) = entry.file_name().to_str() {
                            if !name.starts_with('.') && !names.iter().any(|n| n == name) {
                                names.push(name.to_string());
                            }
                        }
                    }
                }
            }
        }
        // Custom text styles/registrations created by the native panel.
        if key != "upscalers" {
            let registry = home.join("var/run/image-assets.json");
            if let Ok(raw) = std::fs::read_to_string(registry) {
                if let Ok(value) = serde_json::from_str::<Value>(&raw) {
                    if let Some(extra) = value.get(key).and_then(Value::as_array) {
                        for item in extra.iter().filter_map(Value::as_str) {
                            if !names.iter().any(|n| n == item) {
                                names.push(item.to_string());
                            }
                        }
                    }
                }
            }
        }
        names.sort_by_key(|n| n.to_ascii_lowercase());
        assets.insert(
            key.to_string(),
            Value::Array(names.into_iter().map(Value::String).collect()),
        );
    }
    root.insert("assets".into(), Value::Object(assets));
    if let Some(id) = default_image {
        root.insert("default_image".into(), Value::String(id));
    }
    if let Some(id) = default_video {
        root.insert("default_video".into(), Value::String(id));
    }
    result
}

pub(crate) fn on_catalogue(app: &mut UiApp, catalogue: ModuleCatalogue) {
    app.on_catalogue(catalogue);
}

pub(crate) fn on_installed_skills(app: &mut UiApp, skills: Vec<SkillInfo>) {
    app.on_installed_skills(skills.into_iter().map(|skill| skill.name).collect());
}

pub(crate) fn on_installed_modules(app: &mut UiApp, modules: Vec<ModuleInfo>) {
    app.on_installed_modules(modules);
}

pub(crate) fn on_installed(app: &mut UiApp, message: String) {
    app.status = message;
    let _ = app.cmd_tx.send(Cmd::CatalogueRefresh);
    let _ = app.cmd_tx.send(Cmd::ModuleList);
}

pub(crate) fn on_uninstalled(app: &mut UiApp, name: String) {
    let t = crate::i18n::strings(&app.prefs.language);
    app.status = crate::i18n::status_module_uninstalled(&t, &name);
    app.settings_ui
        .installed_modules
        .retain(|module| module.name != name);
    if let Some(mut panel) = app.decl_panels.remove(&name) {
        panel.close();
    }
    if matches!(&app.tab, Tab::Module(module) if module == &name) {
        app.tab = Tab::Settings;
    }
    let _ = app.cmd_tx.send(Cmd::ModuleList);
}

pub(crate) fn on_ui_loaded(app: &mut UiApp, response: ModuleUiResponse) {
    let module = response.module.clone();
    let binds = {
        let panel = app
            .decl_panels
            .entry(module.clone())
            .or_insert_with(|| decl_ui::DeclUiPanelState::new(&module));
        panel.set_document(response.document);
        decl_ui::ingest_tool_schemas(&response.tools, &mut panel.tool_schemas);
        panel.status.clear();
        panel.tools_to_bind()
    };
    for tool in binds {
        let _ = app.cmd_tx.send(Cmd::ModuleUiBind {
            module: module.clone(),
            tool,
        });
    }
}

pub(crate) fn on_ui_failed(app: &mut UiApp, module: String, error: String) {
    let t = crate::i18n::strings(&app.prefs.language);
    let visible = crate::chat_error_copy::user_visible_module_error(&t, &module, &error);
    let panel = app
        .decl_panels
        .entry(module.clone())
        .or_insert_with(|| decl_ui::DeclUiPanelState::new(&module));
    panel.set_error(visible);
}

pub(crate) fn on_ui_bind(
    app: &mut UiApp,
    module: String,
    tool: String,
    result: Value,
    error: Option<String>,
) {
    let result = if module == "create" && tool == "create.models.list" {
        augment_create_catalog(result, app.prefs.language.eq_ignore_ascii_case("fr"))
    } else {
        result
    };
    if let Some(panel) = app.decl_panels.get_mut(&module) {
        panel.set_bind_result(&tool, result.clone());
        let binding_updates: Vec<(String, Value)> = panel
            .document
            .as_ref()
            .map(|doc| {
                doc.bindings
                    .iter()
                    .filter(|b| b.tool == tool)
                    .map(|binding| {
                        let mut val = result.clone();
                        if let Some(target) = &binding.target {
                            if let Some(slice) = val.pointer(target) {
                                val = slice.clone();
                            }
                        }
                        (binding.id.clone(), val)
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (id, val) in binding_updates {
            panel.set_binding_result(&id, val);
        }
        if let Some(error) = error {
            let t = crate::i18n::strings(&app.prefs.language);
            panel.status = crate::chat_error_copy::user_visible_module_error(&t, &module, &error);
        }
    }
}

pub(crate) fn on_ui_invoke_done(
    app: &mut UiApp,
    module: String,
    tool: String,
    ok: bool,
    result: Value,
    error: Option<String>,
) {
    let refresh_binds;
    let clear_form_keys;
    {
        let panel = app
            .decl_panels
            .entry(module.clone())
            .or_insert_with(|| decl_ui::DeclUiPanelState::new(&module));
        refresh_binds = std::mem::take(&mut panel.pending_refresh_binds);
        clear_form_keys = std::mem::take(&mut panel.pending_clear_form_keys);
        panel.set_pending_invoke(false);
        if ok {
            panel.set_bind_result(&tool, result.clone());
            if module == "create" && (tool == "create.history.get" || tool == "create.preset.load")
            {
                let params_value = result.get("params").unwrap_or(&result);
                if let Some(params) = params_value.as_object() {
                    for (key, value) in params {
                        panel.local_state.insert(key.clone(), value.clone());
                    }
                }
            }
            if !clear_form_keys.is_empty() {
                panel.clear_form_keys(&clear_form_keys);
            }
            if module == "illustration-studio" && tool == "illustration.project.save" {
                panel.status = if app.prefs.language.starts_with("fr") {
                    "Projet enregistré".into()
                } else {
                    "Project saved".into()
                };
            } else {
                panel.status.clear();
            }
        } else {
            let t = crate::i18n::strings(&app.prefs.language);
            panel.status = match error.as_deref().filter(|s| !s.trim().is_empty()) {
                Some(raw) => {
                    crate::chat_error_copy::user_visible_create_or_module_error(&t, &module, raw)
                }
                None => t.decl_ui_action_failed.to_string(),
            };
        }
    }
    if ok {
        if module == "illustration-studio"
            && tool == "illustration.project.work_area"
            && result.get("compose").and_then(Value::as_bool) == Some(true)
        {
            if let Some(panel) = app.decl_panels.get(&module) {
                let prompt = panel
                    .local_state
                    .get("prompt")
                    .cloned()
                    .unwrap_or(Value::String(String::new()));
                let _ = app.cmd_tx.send(Cmd::ModuleUiServiceAction {
                    module: module.clone(),
                    action_id: "compose_scene".into(),
                    service: Some("scene.compose".into()),
                    tool: None,
                    input: serde_json::json!({ "prompt": prompt }),
                    refresh_binds: Vec::new(),
                    subscription_id: None,
                });
            }
        }
        dispatch_refresh_binds(app, &module, refresh_binds);
    }
}

pub(crate) fn on_ui_service_done(
    app: &mut UiApp,
    module: String,
    action_id: String,
    ok: bool,
    result: Value,
    error: Option<String>,
    refresh_binds: Vec<String>,
) {
    let t = crate::i18n::strings(&app.prefs.language);
    let language = app.prefs.language.clone();
    if let Some(panel) = app.decl_panels.get_mut(&module) {
        panel.set_pending_invoke(false);
        let library_action = module == "illustration-studio" && action_id.starts_with("library_");
        let library_result_matches = result.get("project_id").and_then(Value::as_str)
            .is_none_or(|id| panel.local_state.get("project_id").and_then(Value::as_str) == Some(id));
        if library_action && library_result_matches {
            panel.local_state.insert("library_busy".into(), Value::Bool(false));
        }
        if ok {
            panel.status.clear();
            if library_action && action_id == "library_generate_image" && library_result_matches {
                if let Some(job_id) = result.get("job_id").and_then(Value::as_str) {
                    panel.local_state.insert("library_job_id".into(), Value::String(job_id.into()));
                }
            }
            if module == "illustration-studio" && action_id == "import_project_file" {
                panel.activate_illustration_project(&result);
            }
            if library_action && library_result_matches {
                if let Some(asset) = result.get("asset") {
                    if let Some(asset_id) = asset.get("id").and_then(Value::as_str) {
                        panel.local_state.insert("library_selected_asset_id".into(), Value::String(asset_id.into()));
                        panel.local_state.insert("library_selected_project_id".into(), result.get("project_id").cloned().unwrap_or(Value::Null));
                        panel.local_state.insert("library_section".into(), Value::String("assets".into()));
                        panel.local_state.insert("library_error".into(), Value::String(String::new()));
                        panel.local_state.insert("library_error_action".into(), Value::String(String::new()));
                        let image = asset.get("kind").and_then(Value::as_str) == Some("image");
                        panel.local_state.insert("library_image_uri".into(), Value::String(
                            if image { asset.get("uri").and_then(Value::as_str).unwrap_or("") } else { "" }.into()
                        ));
                        panel.local_state.insert("library_image_prompt".into(), Value::String(
                            if image { asset.get("prompt").and_then(Value::as_str).unwrap_or("") } else { "" }.into()
                        ));
                    }
                }
            }
            if module == "illustration-studio"
                && matches!(
                    action_id.as_str(),
                    "library_import_glb"
                        | "library_import_chair"
                        | "library_import_desk"
                        | "library_import_notebook"
                        | "library_generate_image"
                        | "library_convert_trellis"
                )
                && panel.local_state.get("project_id") == result.get("project_id")
            {
                panel.set_bind_result("illustration.asset.register", result.clone());
            }
            if module == "illustration-studio"
                && (action_id == "stub_beauty"
                    || action_id == "cpu_beauty"
                    || action_id == "blender_beauty"
                    || action_id == "cpu_3d"
                    || action_id == "blender_3d"
                    || action_id == "comic_render"
                    || action_id == aos_proto::RENDER_STUB_SERVICE
                    || action_id == aos_proto::RENDER_SUBMIT_SERVICE
                    || action_id == aos_proto::COMIC_RENDER_SERVICE)
            {
                let current_project = panel.local_state.get("project_id").and_then(Value::as_str);
                let result_project = result.get("project_id").and_then(Value::as_str);
                let source_key = if result.get("kind").and_then(Value::as_str) == Some("comic_render") {
                    "comic"
                } else {
                    "scene"
                };
                let revision_key = if source_key == "comic" { "comic_revision" } else { "scene_revision" };
                let current_revision = panel.local_state.get(source_key).and_then(Value::as_str).map(|yaml| {
                    use sha2::Digest as _;
                    format!("{:x}", sha2::Sha256::digest(yaml.as_bytes()))
                });
                if current_project != result_project
                    || current_revision.as_deref() != result.get(revision_key).and_then(Value::as_str)
                {
                    return;
                }
                if let Some(path) = result.get("path").and_then(|p| p.as_str()) {
                    panel
                        .local_state
                        .insert("beauty_path".into(), Value::String(path.to_string()));
                }
                for key in ["kind", "project_id", "scene_revision", "comic_revision", "camera_id", "page_id", "id_map_path", "id_map_nodes", "id_map_width", "id_map_height"] {
                    panel.local_state.insert(format!("render_{key}"), result.get(key).cloned().unwrap_or(Value::Null));
                }
                let backend = result
                    .get("backend")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let width = result.get("width").and_then(|v| v.as_u64()).unwrap_or(0);
                let height = result.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
                let style = result.get("style").and_then(|v| v.as_str());
                let engine = result
                    .get("engine")
                    .and_then(Value::as_str)
                    .unwrap_or(backend);
                let preset = result
                    .get("preset")
                    .and_then(Value::as_str)
                    .unwrap_or("standard");
                let summary = if app.prefs.language.starts_with("fr") {
                    format!(
                        "Moteur : {engine} · préréglage : {preset} · {width} × {height} · style : {}",
                        style.unwrap_or("défaut")
                    )
                } else {
                    format!(
                        "Engine: {engine} · preset: {preset} · {width} × {height} · style: {}",
                        style.unwrap_or("default")
                    )
                };
                panel
                    .local_state
                    .insert("beauty_summary".into(), Value::String(summary));
            }
            if module == "illustration-studio" && illustration_action_patches_scene(&action_id) {
                apply_illustration_scene_result(&mut panel.local_state, &action_id, &result);
                if result.get("scene_yaml").is_some() || result.get("comic_yaml").is_some() {
                    panel.local_state.insert("beauty_path".into(), Value::String(String::new()));
                }
            }
            if module == "illustration-studio" && action_id == "scene_diagnostics" {
                if let Some(summary) = result.get("diagnostics").and_then(Value::as_str) {
                    panel
                        .local_state
                        .insert("diagnostics".into(), Value::String(summary.into()));
                }
            }
            if module == "illustration-studio" && action_id == "compose_scene" {
                if let Some(candidates) = result.get("candidates").and_then(Value::as_array) {
                    panel.local_state.insert(
                        "compose_pending".into(),
                        Value::Bool(!candidates.is_empty()),
                    );
                    panel.local_state.insert(
                        "compose_two_candidates".into(),
                        Value::Bool(candidates.len() > 1),
                    );
                    panel.local_state.insert(
                        "compose_fallback".into(),
                        Value::Bool(
                            result.get("source").and_then(Value::as_str)
                                == Some("keyword fallback"),
                        ),
                    );
                    for (index, candidate) in candidates.iter().take(2).enumerate() {
                        for (field, key) in [
                            ("scene_yaml", "yaml"),
                            ("root_id", "root"),
                            ("character_id", "character"),
                        ] {
                            panel.local_state.insert(
                                format!("compose_candidate_{}_{}", index + 1, key),
                                candidate.get(field).cloned().unwrap_or(Value::Null),
                            );
                        }
                    }
                }
            }
            if module == "illustration-studio" && action_id.starts_with("apply_compose_candidate_")
            {
                panel
                    .local_state
                    .insert("compose_pending".into(), Value::Bool(false));
                panel
                    .local_state
                    .insert("compose_fallback".into(), Value::Bool(false));
                if result.get("character_id").and_then(Value::as_str).is_none() {
                    panel
                        .local_state
                        .insert("character_id".into(), Value::String(String::new()));
                }
            }
            if module == "illustration-studio"
                && (action_id == "comic_layout"
                    || action_id == "comic_bind_panel"
                    || action_id == aos_proto::COMIC_LAYOUT_SERVICE)
            {
                if let Some(yaml) = result.get("comic_yaml").and_then(|p| p.as_str()) {
                    panel
                        .local_state
                        .insert("comic".into(), Value::String(yaml.to_string()));
                }
                if let Some(n) = result.get("panel_count").and_then(|p| p.as_u64()) {
                    panel
                        .local_state
                        .insert("comic_panel_count".into(), Value::from(n));
                }
            }
            if module == "illustration-studio"
                && (action_id == "list_local_packs"
                    || action_id == aos_proto::ASSET_PACK_LIST_SERVICE
                    || action_id == "describe_primitives_pack"
                    || action_id == aos_proto::ASSET_PACK_DESCRIBE_SERVICE)
            {
                if let Some(summary) = result.get("summary").and_then(|p| p.as_str()) {
                    panel
                        .local_state
                        .insert("pack_summary".into(), Value::String(summary.to_string()));
                } else if let Some(name) = result.get("name").and_then(|p| p.as_str()) {
                    let path = result.get("path").and_then(|p| p.as_str()).unwrap_or("");
                    let entries = result
                        .get("entry_ids")
                        .and_then(|p| p.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    panel.local_state.insert(
                        "pack_summary".into(),
                        Value::String(format!("{name} — {path} ({entries} entries)")),
                    );
                }
            }
            if module == "illustration-studio"
                && (action_id == "mesh_pack_status"
                    || action_id == aos_proto::MESH_PACK_STATUS_SERVICE
                    || action_id == "mesh_assist_stub"
                    || action_id == "mesh_assist_neural"
                    || action_id == aos_proto::MESH_ASSIST_SERVICE)
            {
                if let Some(summary) = result
                    .get("mesh_pack_status")
                    .or_else(|| result.get("summary"))
                    .and_then(|p| p.as_str())
                {
                    panel.local_state.insert(
                        "mesh_pack_status".into(),
                        Value::String(summary.to_string()),
                    );
                }
            }
            if module == "illustration-studio"
                && (action_id == "blender_pack_status"
                    || action_id == aos_proto::RENDER_PACK_STATUS_SERVICE)
            {
                if let Some(summary) = result
                    .get("blender_pack_status")
                    .or_else(|| result.get("summary"))
                    .and_then(|p| p.as_str())
                {
                    panel.local_state.insert(
                        "blender_pack_status".into(),
                        Value::String(summary.to_string()),
                    );
                }
            }
        } else {
            if library_action {
                if library_result_matches {
                    let raw = error.as_deref().unwrap_or("");
                    panel.local_state.insert("library_error".into(), Value::String(
                        illustration_library_error(&language, &action_id, raw)
                    ));
                    panel.local_state.insert("library_error_action".into(), Value::String(action_id.clone()));
                }
                return;
            }
            panel.status = match error.as_deref().filter(|s| !s.trim().is_empty()) {
                Some(raw) if module == "illustration-studio" && action_id == "blender_beauty" => {
                    crate::chat_error_copy::illustration_blender_error(&language, raw)
                }
                Some(raw) => {
                    crate::chat_error_copy::user_visible_create_or_module_error(&t, &module, raw)
                }
                None => t.decl_ui_action_failed.to_string(),
            };
        }
    }
    if ok {
        dispatch_refresh_binds(app, &module, refresh_binds);
    }
}

fn illustration_library_error(language: &str, action: &str, raw: &str) -> String {
    let fr = language.starts_with("fr");
    let lower = raw.to_ascii_lowercase();
    if lower.contains("cancel") || lower.contains("annul") {
        return String::new();
    }
    if action == "library_convert_trellis" {
        eprintln!("Illustration Studio library {action}: {raw}");
        return illustration_trellis_conversion_error(fr, raw);
    }
    let message = if lower.contains("could not be added to the project library") {
        if fr { "L’image a été générée, mais son ajout à la bibliothèque du projet a échoué. Vérifiez l’espace disque et réessayez." }
        else { "The image was generated, but could not be added to the project library. Check disk space and try again." }
    } else if lower.contains("not installed") || lower.contains("modèle incomplet") || lower.contains("model unavailable") {
        if fr { "Le modèle d’image n’est pas prêt. Installez ou complétez le modèle dans Modèles, puis réessayez." }
        else { "The image model is not ready. Install or complete it in Models, then try again." }
    } else if lower.contains("vulkan") || lower.contains("trellis") || lower.contains("mesh pack") || lower.contains("runner") {
        if fr { "La conversion 3D est indisponible. Ouvrez TRELLIS, vérifiez le moteur, le modèle Q4/Q8 et le GPU Vulkan, puis relancez la conversion." }
        else { "3D conversion is unavailable. Open TRELLIS, check the runtime, Q4/Q8 model and Vulkan GPU, then retry." }
    } else if lower.contains("glb") || lower.contains("gltf") || lower.contains("asset source") {
        if fr { "L’asset 3D n’a pas pu être importé. Choisissez un fichier GLB valide de moins de 200 Mo, puis réessayez." }
        else { "The 3D asset could not be imported. Choose a valid GLB under 200 MB, then try again." }
    } else if lower.contains("disk") || lower.contains("space") || lower.contains("write") || lower.contains("copy") {
        if fr { "L’asset a été créé mais n’a pas pu être enregistré. Vérifiez l’espace disque et le dossier du projet, puis réessayez." }
        else { "The asset was created but could not be saved. Check disk space and the project folder, then try again." }
    } else if lower.contains("network") || lower.contains("download") || lower.contains("timeout") || lower.contains("http") {
        if fr { "Le téléchargement a échoué. Vérifiez la connexion et relancez l’opération." }
        else { "The download failed. Check your connection and retry." }
    } else if action == "library_generate_image" {
        if fr { "L’image n’a pas pu être générée. Vérifiez le modèle choisi et relancez la création." }
        else { "The image could not be generated. Check the selected model and try again." }
    } else if action == "library_convert_trellis" {
        if fr { "La conversion 3D a échoué. Vérifiez TRELLIS et relancez la conversion depuis la fiche de l’image." }
        else { "3D conversion failed. Check TRELLIS and retry from the image details." }
    } else {
        if fr { "L’import a échoué. Vérifiez la source de l’asset et réessayez." }
        else { "Import failed. Check the asset source and try again." }
    };
    eprintln!("Illustration Studio library {action}: {raw}");
    message.into()
}

fn illustration_trellis_conversion_error(fr: bool, raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if crate::chat_error_copy::is_trellis_test_model_error(raw) {
        return if fr {
            "TRELLIS a renvoyé un modèle de démonstration. Réessayez avec le moteur réel pour obtenir la cause de son échec."
        } else {
            "TRELLIS returned a demo model. Retry with the real runtime to see why it failed."
        }
        .into();
    }
    if lower.contains("timed out") {
        return if fr { "TRELLIS a dépassé le délai de conversion. Réessayez avec une image plus simple ou vérifiez les diagnostics du moteur." }
        else { "TRELLIS exceeded the conversion time limit. Try a simpler image or inspect the runtime diagnostics." }.into();
    }
    if lower.contains("out of memory") || lower.contains("allocation failed") || lower.contains("cuda error 2") {
        return if fr { "TRELLIS a manqué de mémoire GPU pendant la conversion. Fermez les applications utilisant le GPU puis réessayez avec Q4." }
        else { "TRELLIS ran out of GPU memory during conversion. Close other GPU workloads and retry with Q4." }.into();
    }
    if lower.contains("no vulkan device") || lower.contains("vulkan unavailable") {
        return if fr { "TRELLIS ne détecte aucun GPU Vulkan compatible. Vérifiez le pilote graphique et le GPU sélectionné." }
        else { "TRELLIS cannot detect a compatible Vulkan GPU. Check the graphics driver and selected GPU." }.into();
    }
    if lower.contains("backend unavailable") {
        return if fr { "Le moteur TRELLIS ou ses poids Q4/Q8 ne sont pas accessibles au module. Actualisez l’état dans TRELLIS." }
        else { "The module cannot access the TRELLIS runtime or Q4/Q8 weights. Refresh the status in TRELLIS." }.into();
    }
    let detail = raw
        .split("stderr_tail:")
        .last()
        .unwrap_or(raw)
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(raw);
    let detail: String = detail.chars().filter(|c| !c.is_control()).take(240).collect();
    if fr {
        format!("La conversion TRELLIS a échoué. Détail du moteur : {detail}")
    } else {
        format!("TRELLIS conversion failed. Runtime detail: {detail}")
    }
}

pub(crate) fn on_ui_job_update(
    app: &mut UiApp,
    module: String,
    subscription_id: String,
    job: aos_proto::rich_decl_ui::RichJobHandle,
) {
    if let Some(panel) = app.decl_panels.get_mut(&module) {
        if module == "illustration-studio" && subscription_id == "library_image_job" {
            if panel.local_state.get("project_id") != panel.local_state.get("library_job_project_id") {
                return;
            }
            let current_job = panel.local_state.get("library_job_id").and_then(Value::as_str).unwrap_or("");
            if let Some(incoming) = job.job_id.as_deref() {
                if current_job != incoming {
                    return;
                }
            } else {
                return;
            }
        }
        panel.set_job_update(&subscription_id, job.clone());
        if module == "create" && job.state.as_deref() == Some("succeeded") {
            if let Some(path) = job
                .result
                .as_ref()
                .and_then(|r| r.get("path"))
                .and_then(|p| p.as_str())
            {
                panel
                    .local_state
                    .insert("result_path".into(), Value::String(path.to_string()));
                panel
                    .local_state
                    .insert("preview_cleared".into(), Value::Bool(false));
                if let Some(generated) = job
                    .result
                    .as_ref()
                    .and_then(|r| r.get("generation_prompt"))
                    .and_then(Value::as_str)
                {
                    panel.local_state.insert(
                        "enriched_prompt".into(),
                        Value::String(generated.to_string()),
                    );
                }
            }
        }
    }
}

pub(crate) fn on_ui_service_progress(
    app: &mut UiApp,
    module: String,
    message: String,
    active: bool,
) {
    if let Some(panel) = app.decl_panels.get_mut(&module) {
        panel.status = message;
        if module == "illustration-studio" {
            panel
                .local_state
                .insert("dependency_download_active".into(), Value::Bool(active));
        }
    }
}

pub(crate) fn on_ui_prompt_generated(app: &mut UiApp, module: String, prompt: String) {
    if module == "create" {
        if let Some(panel) = app.decl_panels.get_mut(&module) {
            panel
                .local_state
                .insert("enriched_prompt".into(), Value::String(prompt));
        }
    }
}

pub(crate) fn on_ui_layers_generated(
    app: &mut UiApp,
    module: String,
    layers: Vec<serde_json::Value>,
) {
    if module == "create" {
        if let Some(panel) = app.decl_panels.get_mut(&module) {
            panel.local_state.insert(
                "composition_layers".into(),
                serde_json::Value::Array(layers),
            );
        }
    }
}

/// DeclUI actions whose success payloads may carry `scene_yaml` / selection for
/// Illustration Studio. Prefer `instantiate_*` prefix so prefab Add buttons
/// (#312) patch `$local.scene` without maintaining a brittle per-asset list.
fn illustration_action_patches_scene(action_id: &str) -> bool {
    if action_id.starts_with("instantiate_") || action_id.starts_with("catalogue_add_") {
        return true;
    }
    matches!(
        action_id,
        "compose_scene"
            | "apply_compose_candidate_1"
            | "apply_compose_candidate_2"
            | "import_glb"
            | "catalogue_add"
            | "pose_wave"
            | "pose_look"
            | "pose_rest"
            | "lock_selected"
            | "lock_subtree"
            | "unlock_selected"
            | "mesh_assist_stub"
            | "mesh_assist_neural"
            | "add_light"
            | "apply_light"
            | "add_fx"
            | "clear_fx"
            | "save_finish"
            | "register_rig"
            | "save_named_pose"
            | "apply_named_pose"
            | "add_keyframe"
            | "seek_animation"
            | "play_animation"
            | "stop_animation"
            | "save_version"
            | "restore_version"
            | "save_variant"
            | "apply_variant"
            | "storyboard_capture"
            | "storyboard_prev"
            | "storyboard_next"
            | "storyboard_delete"
            | "storyboard_move_earlier"
            | "storyboard_move_later"
    ) || action_id == aos_proto::ASSET_INSTANTIATE_SERVICE
        || action_id == aos_proto::SCENE_COMPOSE_SERVICE
        || action_id == aos_proto::SCENE_POSE_SERVICE
        || action_id == aos_proto::SCENE_GET_SERVICE
        || action_id == aos_proto::SCENE_SELECT_SERVICE
        || action_id == aos_proto::SCENE_TRS_SERVICE
        || action_id == aos_proto::SCENE_CAMERA_SERVICE
        || action_id == aos_proto::SCENE_LIGHT_SERVICE
        || action_id == aos_proto::SCENE_APPLY_SERVICE
        || action_id == aos_proto::SCENE_LOCK_SERVICE
        || action_id == aos_proto::SCENE_UNLOCK_SERVICE
        || action_id == aos_proto::SCENE_LOCKS_SERVICE
        || action_id == aos_proto::MESH_ASSIST_SERVICE
        || action_id == aos_proto::MESH_PACK_STATUS_SERVICE
        || action_id == aos_proto::RENDER_PACK_STATUS_SERVICE
        || action_id == aos_proto::STORYBOARD_CAPTURE_SERVICE
        || action_id == aos_proto::STORYBOARD_APPLY_SERVICE
        || action_id == aos_proto::STORYBOARD_DELETE_SERVICE
        || action_id == aos_proto::STORYBOARD_MOVE_SERVICE
}

fn apply_illustration_scene_result(
    local_state: &mut std::collections::HashMap<String, Value>,
    action_id: &str,
    result: &Value,
) {
    if let Some(yaml) = result.get("scene_yaml").and_then(|p| p.as_str()) {
        local_state.insert("scene".into(), Value::String(yaml.to_string()));
    }
    if let Some(summary) = result.get("animation_summary").and_then(Value::as_str) {
        local_state.insert("animation_summary".into(), Value::String(summary.into()));
    }
    if let Some(summary) = result.get("history_summary").and_then(Value::as_str) {
        local_state.insert("history_summary".into(), Value::String(summary.into()));
    }
    if let Some(summary) = result.get("storyboard_summary").and_then(Value::as_str) {
        local_state.insert("storyboard_summary".into(), Value::String(summary.into()));
    }
    if let Some(id) = result.get("frame_id").and_then(Value::as_str) {
        local_state.insert("frame_id".into(), Value::String(id.into()));
    }
    if let Some(id) = result.get("version_id").and_then(Value::as_u64) {
        local_state.insert("version_id".into(), Value::from(id));
    }
    if let Some(sel) = result.get("selected_id").and_then(|p| p.as_str()) {
        local_state.insert("selected_id".into(), Value::String(sel.to_string()));
    }
    if let Some(root) = result.get("root_id").and_then(|p| p.as_str()) {
        local_state.insert("selected_id".into(), Value::String(root.to_string()));
    }
    if let Some(cid) = result.get("character_id").and_then(|p| p.as_str()) {
        local_state.insert("character_id".into(), Value::String(cid.to_string()));
        local_state.insert("selected_id".into(), Value::String(cid.to_string()));
    } else if matches!(
        action_id,
        "instantiate_humanoid"
            | "instantiate_slim"
            | "instantiate_female"
            | "instantiate_cat"
            | "instantiate_dog"
    ) {
        if let Some(root) = result.get("root_id").and_then(|p| p.as_str()) {
            local_state.insert("character_id".into(), Value::String(root.to_string()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn library_errors_are_actionable_and_cancellation_is_quiet() {
        assert!(illustration_library_error("fr", "library_convert_trellis", "Vulkan unavailable")
            .contains("TRELLIS"));
        let runtime_error = illustration_library_error(
            "en",
            "library_convert_trellis",
            "validation failed: neural mesh runner exit=1; output.glb missing; stderr_tail: shape decode failed",
        );
        assert!(runtime_error.contains("shape decode failed"));
        assert!(!runtime_error.contains("check the runtime, Q4/Q8 model and Vulkan GPU"));
        assert!(illustration_library_error("en", "library_convert_trellis", "__trellis_test_model__")
            .contains("demo model"));
        assert!(illustration_library_error("en", "library_import_glb", "Invalid GLB")
            .contains("valid GLB"));
        assert!(illustration_library_error("en", "library_generate_image", "Image created but could not be added to the project library")
            .contains("was generated"));
        assert_eq!(illustration_library_error("fr", "library_import_glb", "Import cancelled"), "");
    }

    #[test]
    fn prefab_instantiate_action_ids_patch_scene() {
        for id in [
            "instantiate_humanoid",
            "instantiate_female",
            "instantiate_slim",
            "instantiate_cat",
            "instantiate_dog",
            "instantiate_sofa",
            "instantiate_desk",
            "instantiate_lamp",
            "instantiate_plant",
            "instantiate_book",
            "instantiate_window",
            "instantiate_wall",
            "instantiate_box",
        ] {
            assert!(
                illustration_action_patches_scene(id),
                "{id} must patch $local.scene after asset.instantiate"
            );
        }
        assert!(!illustration_action_patches_scene("list_local_packs"));
        assert!(!illustration_action_patches_scene("stub_beauty"));
    }

    #[test]
    fn apply_instantiate_cat_writes_scene_and_selection() {
        let mut local = HashMap::new();
        local.insert("scene".into(), Value::String(String::new()));
        let result = serde_json::json!({
            "asset_id": "quadruped.cat",
            "root_id": "cat_root",
            "created_ids": ["cat_root", "cat_body"],
            "scene_yaml": "schema_version: 1\nnodes: {}\n",
        });
        assert!(illustration_action_patches_scene("instantiate_cat"));
        apply_illustration_scene_result(&mut local, "instantiate_cat", &result);
        assert_eq!(
            local.get("scene").and_then(Value::as_str),
            Some("schema_version: 1\nnodes: {}\n")
        );
        assert_eq!(
            local.get("selected_id").and_then(Value::as_str),
            Some("cat_root")
        );
        assert_eq!(
            local.get("character_id").and_then(Value::as_str),
            Some("cat_root")
        );
    }

    #[test]
    fn apply_instantiate_sofa_writes_scene_without_character() {
        let mut local = HashMap::new();
        let result = serde_json::json!({
            "asset_id": "prop.sofa",
            "root_id": "sofa_root",
            "scene_yaml": "schema_version: 1\nsofa: true\n",
        });
        apply_illustration_scene_result(&mut local, "instantiate_sofa", &result);
        assert_eq!(
            local.get("scene").and_then(Value::as_str),
            Some("schema_version: 1\nsofa: true\n")
        );
        assert_eq!(
            local.get("selected_id").and_then(Value::as_str),
            Some("sofa_root")
        );
        assert!(!local.contains_key("character_id"));
    }

    #[test]
    fn compose_proposals_do_not_replace_the_scene_before_acceptance() {
        let mut local = HashMap::new();
        local.insert("scene".into(), Value::String("existing scene".into()));
        let proposal = serde_json::json!({
            "candidates": [{"scene_yaml": "new scene", "root_id": "camera"}]
        });
        apply_illustration_scene_result(&mut local, "compose_scene", &proposal);
        assert_eq!(
            local.get("scene").and_then(Value::as_str),
            Some("existing scene")
        );
        let accepted = serde_json::json!({"scene_yaml": "new scene", "root_id": "camera"});
        apply_illustration_scene_result(&mut local, "apply_compose_candidate_1", &accepted);
        assert_eq!(
            local.get("scene").and_then(Value::as_str),
            Some("new scene")
        );
    }
}
