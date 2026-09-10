//! Create module navigation helpers (#150 lot 4).

use crate::cmd::Cmd;
use crate::decl_ui::DeclUiPanelState;
use crate::{nav, UiApp};
use aos_proto::create_contract::MODULE_NAME;
use serde_json::Value;

pub(crate) fn open_create_module(app: &mut UiApp, prompt: Option<&str>, path: Option<&str>) {
    if prompt.is_some() || path.is_some() {
        let panel = app
            .decl_panels
            .entry(MODULE_NAME.into())
            .or_insert_with(|| DeclUiPanelState::new(MODULE_NAME));
        if let Some(prompt) = prompt.filter(|p| !p.is_empty()) {
            panel
                .local_state
                .insert("prompt".into(), Value::String(prompt.to_string()));
        }
        if let Some(path) = path {
            panel
                .local_state
                .insert("result_path".into(), Value::String(path.to_string()));
        }
    }
    app.on_tab_open(nav::create_module_tab());
    let _ = app.cmd_tx.send(Cmd::ModuleUiLoad {
        module: MODULE_NAME.into(),
    });
}

pub(crate) fn open_create_module_if_installed(
    app: &mut UiApp,
    prompt: Option<&str>,
    path: Option<&str>,
) -> bool {
    if !app.create_module_installed() {
        return false;
    }
    open_create_module(app, prompt, path);
    true
}
