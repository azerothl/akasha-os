//! Create module navigation helpers (#150 lot 4).

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
            // Seed survives first ModuleUiLoad via pending_local_seed; on
            // re-entry with an existing document it patches local_state directly.
            panel.seed_local("prompt", Value::String(prompt.to_string()));
        }
        if let Some(path) = path {
            panel.seed_local("result_path", Value::String(path.to_string()));
        }
    }
    // on_tab_open loads only when the panel has no document — do not send a
    // second ModuleUiLoad here (it wiped in-flight Create job UI on re-entry).
    app.on_tab_open(nav::create_module_tab());
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

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::decl_ui::DeclUiDocument;
    use aos_proto::rich_decl_ui::RichJobHandle;
    use serde_json::json;

    fn minimal_doc() -> DeclUiDocument {
        DeclUiDocument::parse_json_with_contract(
            br#"{
              "type": "declarative_ui",
              "contract": 2,
              "title": "Create",
              "subscriptions": [{"id": "generate_job", "action": "generate_image"}],
              "state": {
                "local": {
                  "prompt": {"type": "string", "default": ""},
                  "result_path": {"type": "string", "default": ""}
                }
              },
              "root": {"kind": "column", "children": []}
            }"#,
            2,
        )
        .expect("minimal create doc")
    }

    #[test]
    fn needs_ui_load_only_when_document_missing() {
        let mut panel = DeclUiPanelState::new(MODULE_NAME);
        assert!(panel.needs_ui_load());
        panel.set_document(minimal_doc());
        assert!(!panel.needs_ui_load());
        panel.set_error("boom");
        assert!(panel.needs_ui_load());
    }

    #[test]
    fn seed_local_survives_first_set_document() {
        let mut panel = DeclUiPanelState::new(MODULE_NAME);
        panel.seed_local("prompt", json!("from chat"));
        panel.seed_local("result_path", json!("/downloads/image-1.png"));
        panel.set_document(minimal_doc());
        assert_eq!(
            panel.local_state.get("prompt").and_then(Value::as_str),
            Some("from chat")
        );
        assert_eq!(
            panel
                .local_state
                .get("result_path")
                .and_then(Value::as_str),
            Some("/downloads/image-1.png")
        );
        assert!(panel.pending_local_seed.is_empty());
    }

    #[test]
    fn reentry_keeps_job_handle_when_document_not_reloaded() {
        let mut panel = DeclUiPanelState::new(MODULE_NAME);
        panel.set_document(minimal_doc());
        panel.local_state.insert("prompt".into(), json!("still here"));
        panel.set_job_update(
            "generate_job",
            RichJobHandle {
                job_id: Some("job-1".into()),
                kind: Some("media.image.generate".into()),
                state: Some("running".into()),
                progress: Some(aos_proto::rich_decl_ui::RichJobProgress {
                    completed: 4,
                    total: 10,
                    unit: Some("steps".into()),
                }),
                error: None,
                result: None,
            },
        );
        assert!(!panel.needs_ui_load());
        assert!(panel.subscriptions.job("generate_job").is_some());
        assert_eq!(
            panel.local_state.get("prompt").and_then(Value::as_str),
            Some("still here")
        );
        // Simulating Refresh / forced reload still clears job handles — intentional.
        panel.set_document(minimal_doc());
        assert!(panel.subscriptions.job("generate_job").is_none());
    }
}
