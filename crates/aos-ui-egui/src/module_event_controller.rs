//! Event handlers for module catalogues and declarative module UIs.

use crate::cmd::Cmd;
use crate::{decl_ui, Tab, UiApp};
use aos_proto::decl_ui::ModuleUiResponse;
use aos_proto::{ModuleCatalogue, ModuleInfo, SkillInfo};
use serde_json::Value;

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
    app.status = format!("uninstalled {name}");
    app.decl_panels.remove(&name);
    if matches!(&app.tab, Tab::Module(module) if module == &name) {
        app.tab = Tab::Settings;
    }
    let _ = app.cmd_tx.send(Cmd::ModuleList);
}

pub(crate) fn on_ui_loaded(app: &mut UiApp, response: ModuleUiResponse) {
    let module = response.module.clone();
    let title = response.document.title.clone();
    let binds = {
        let panel = app
            .decl_panels
            .entry(module.clone())
            .or_insert_with(|| decl_ui::DeclUiPanelState::new(&module));
        panel.set_document(response.document);
        decl_ui::ingest_tool_schemas(&response.tools, &mut panel.tool_schemas);
        panel.status = format!("loaded {title}");
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
    if let Some(panel) = app.decl_panels.get_mut(&module) {
        panel.set_bind_result(&tool, result);
        if let Some(error) = error {
            panel.status = format!("{tool}: {error}");
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
    if let Some(panel) = app.decl_panels.get_mut(&module) {
        if ok {
            // Keep invoke results in the bind cache so widgets bound to this
            // tool can update immediately without a full reload.
            panel.set_bind_result(&tool, result);
        }
        panel.status = if ok {
            format!("{tool} ok")
        } else {
            error.unwrap_or_else(|| format!("{tool} failed"))
        };
    }
}
