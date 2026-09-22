//! Event handlers for module catalogues and declarative module UIs.

use crate::cmd::Cmd;
use crate::{decl_ui, Tab, UiApp};
use aos_proto::decl_ui::ModuleUiResponse;
use aos_proto::{ModuleCatalogue, ModuleInfo, SkillInfo};
use serde_json::Value;

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
            panel.status.clear();
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
        for bind in refresh_binds {
            let _ = app.cmd_tx.send(Cmd::ModuleUiBind {
                module: module.clone(),
                tool: bind,
            });
        }
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
    if let Some(panel) = app.decl_panels.get_mut(&module) {
        panel.set_pending_invoke(false);
        if ok {
            panel.status.clear();
            if module == "illustration-studio"
                && (action_id == "stub_beauty"
                    || action_id == "cpu_beauty"
                    || action_id == "blender_beauty"
                    || action_id == "comic_render"
                    || action_id == aos_proto::RENDER_STUB_SERVICE
                    || action_id == aos_proto::RENDER_SUBMIT_SERVICE
                    || action_id == aos_proto::COMIC_RENDER_SERVICE)
            {
                if let Some(path) = result.get("path").and_then(|p| p.as_str()) {
                    panel
                        .local_state
                        .insert("beauty_path".into(), Value::String(path.to_string()));
                }
            }
            if module == "illustration-studio"
                && (action_id == "instantiate_humanoid"
                    || action_id == "instantiate_box"
                    || action_id == "instantiate_slim"
                    || action_id == "compose_scene"
                    || action_id == "pose_wave"
                    || action_id == "pose_look"
                    || action_id == "pose_rest"
                    || action_id == "lock_selected"
                    || action_id == "lock_subtree"
                    || action_id == "unlock_selected"
                    || action_id == "mesh_assist_stub"
                    || action_id == "storyboard_capture"
                    || action_id == "storyboard_prev"
                    || action_id == "storyboard_next"
                    || action_id == "storyboard_delete"
                    || action_id == "storyboard_move_earlier"
                    || action_id == "storyboard_move_later"
                    || action_id == aos_proto::ASSET_INSTANTIATE_SERVICE
                    || action_id == aos_proto::SCENE_COMPOSE_SERVICE
                    || action_id == aos_proto::SCENE_POSE_SERVICE
                    || action_id == aos_proto::SCENE_GET_SERVICE
                    || action_id == aos_proto::SCENE_SELECT_SERVICE
                    || action_id == aos_proto::SCENE_TRS_SERVICE
                    || action_id == aos_proto::SCENE_APPLY_SERVICE
                    || action_id == aos_proto::SCENE_LOCK_SERVICE
                    || action_id == aos_proto::SCENE_UNLOCK_SERVICE
                    || action_id == aos_proto::SCENE_LOCKS_SERVICE
                    || action_id == aos_proto::MESH_ASSIST_SERVICE
                    || action_id == aos_proto::STORYBOARD_CAPTURE_SERVICE
                    || action_id == aos_proto::STORYBOARD_APPLY_SERVICE
                    || action_id == aos_proto::STORYBOARD_DELETE_SERVICE
                    || action_id == aos_proto::STORYBOARD_MOVE_SERVICE)
            {
                if let Some(yaml) = result.get("scene_yaml").and_then(|p| p.as_str()) {
                    panel
                        .local_state
                        .insert("scene".into(), Value::String(yaml.to_string()));
                }
                if let Some(sel) = result.get("selected_id").and_then(|p| p.as_str()) {
                    panel
                        .local_state
                        .insert("selected_id".into(), Value::String(sel.to_string()));
                }
                if let Some(root) = result.get("root_id").and_then(|p| p.as_str()) {
                    panel
                        .local_state
                        .insert("selected_id".into(), Value::String(root.to_string()));
                }
                if let Some(cid) = result.get("character_id").and_then(|p| p.as_str()) {
                    panel
                        .local_state
                        .insert("character_id".into(), Value::String(cid.to_string()));
                    panel
                        .local_state
                        .insert("selected_id".into(), Value::String(cid.to_string()));
                } else if matches!(
                    action_id.as_str(),
                    "instantiate_humanoid" | "instantiate_slim"
                ) {
                    if let Some(root) = result.get("root_id").and_then(|p| p.as_str()) {
                        panel
                            .local_state
                            .insert("character_id".into(), Value::String(root.to_string()));
                    }
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
                    let path = result
                        .get("path")
                        .and_then(|p| p.as_str())
                        .unwrap_or("");
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
        } else {
            panel.status = match error.as_deref().filter(|s| !s.trim().is_empty()) {
                Some(raw) => {
                    crate::chat_error_copy::user_visible_create_or_module_error(&t, &module, raw)
                }
                None => t.decl_ui_action_failed.to_string(),
            };
        }
    }
    if ok {
        for bind in refresh_binds {
            let _ = app.cmd_tx.send(Cmd::ModuleUiBind {
                module: module.clone(),
                tool: bind,
            });
        }
    }
}

pub(crate) fn on_ui_job_update(
    app: &mut UiApp,
    module: String,
    subscription_id: String,
    job: aos_proto::rich_decl_ui::RichJobHandle,
) {
    if let Some(panel) = app.decl_panels.get_mut(&module) {
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
