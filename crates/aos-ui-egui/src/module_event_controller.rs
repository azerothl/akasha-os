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
    // The guest module owns the catalogue ids, while the host owns the
    // install registry and user asset folders. Enrich the binding here so a
    // declarative module keeps the same installed/not-installed semantics as
    // the former native Create panel without granting it ambient filesystem
    // access.
    for mode in ["image", "video"] {
        if let Some(rows) = root.get_mut(mode).and_then(Value::as_array_mut) {
            for row in rows {
                let Some(obj) = row.as_object_mut() else { continue };
                let Some(id) = obj.get("id").and_then(Value::as_str).map(str::to_owned) else { continue };
                let installed = crate::models_page::is_model_installed(&id);
                obj.insert("installed".into(), Value::Bool(installed));
                let base = obj
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or(&id)
                    .to_string();
                obj.insert(
                    "label".into(),
                    Value::String(format!(
                        "{base} ({})",
                        if installed {
                            if french { "installé" } else { "installed" }
                        } else if french {
                            "non installé"
                        } else {
                            "not installed"
                        }
                    )),
                );
            }
        }
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
        assets.insert(key.to_string(), Value::Array(names.into_iter().map(Value::String).collect()));
    }
    root.insert("assets".into(), Value::Object(assets));
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
    let panel = app
        .decl_panels
        .entry(module.clone())
        .or_insert_with(|| decl_ui::DeclUiPanelState::new(&module));
    panel.set_error(error);
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
            panel.status = error;
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
            if module == "create" && (tool == "create.history.get" || tool == "create.preset.load") {
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
            let _ = error;
            panel.status = t.decl_ui_action_failed.to_string();
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
    ok: bool,
    error: Option<String>,
    refresh_binds: Vec<String>,
) {
    let t = crate::i18n::strings(&app.prefs.language);
    if let Some(panel) = app.decl_panels.get_mut(&module) {
        panel.set_pending_invoke(false);
        if ok {
            panel.status.clear();
        } else {
            let _ = error;
            panel.status = t.decl_ui_action_failed.to_string();
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
            }
        }
    }
}
