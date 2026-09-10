//! Declarative module panel integration.

use crate::cmd::Cmd;
use crate::{decl_ui, i18n, UiApp};
use eframe::egui;

impl UiApp {
    pub(crate) fn ui_decl_module(&mut self, ui: &mut egui::Ui, module: &str) {
        if !self.decl_panels.contains_key(module) {
            self.decl_panels
                .insert(module.to_string(), decl_ui::DeclUiPanelState::new(module));
            let _ = self.cmd_tx.send(Cmd::ModuleUiLoad {
                module: module.to_string(),
            });
        }
        let t = i18n::strings(&self.prefs.language);
        let mut actions = decl_ui::DeclUiActions::default();
        if let Some(panel) = self.decl_panels.get_mut(module) {
            if module == "create" {
                sync_create_generation_defaults(panel);
            }
            actions = panel.ui(
                ui,
                &mut self.decl_md_cache,
                t.decl_ui_refresh,
                &self.prefs.language,
            );
            for (key, value) in actions.local_patch.drain() {
                panel.local_state.insert(key, value);
            }
        }
        if actions.refresh {
            let _ = self.cmd_tx.send(Cmd::ModuleUiRefresh {
                module: module.to_string(),
            });
        }
        if let Some(inv) = actions.invoke {
            if let Some(panel) = self.decl_panels.get_mut(module) {
                panel.set_pending_invoke(true);
                panel.pending_refresh_binds = inv.refresh_binds;
                panel.pending_clear_form_keys = inv.clear_form_keys;
            }
            let _ = self.cmd_tx.send(Cmd::ModuleUiInvoke {
                module: module.to_string(),
                tool: inv.tool,
                args: inv.args,
            });
        }
        if let Some(svc) = actions.service_action {
            if let Some(panel) = self.decl_panels.get_mut(module) {
                panel.set_pending_invoke(true);
                panel.pending_refresh_binds = svc.refresh_binds.clone();
            }
            let _ = self.cmd_tx.send(Cmd::ModuleUiServiceAction {
                module: module.to_string(),
                action_id: svc.action_id,
                service: svc.service,
                tool: svc.tool,
                input: svc.input,
                refresh_binds: svc.refresh_binds,
                subscription_id: svc.subscription_id,
            });
        }
        if let Some((job_id, subscription_id)) = actions.cancel_job {
            let _ = self.cmd_tx.send(Cmd::ModuleUiCancelJob {
                module: module.to_string(),
                job_id,
                subscription_id,
            });
        }
    }
}

/// Keep the declarative Create controls in lockstep with the native panel's
/// model/profile recipe. This runs only when the pair changes, so a user can
/// still tune width, steps, CFG and expert flags without them being reset on
/// each repaint.
fn sync_create_generation_defaults(panel: &mut decl_ui::DeclUiPanelState) {
    let model_id = panel
        .local_state
        .get("model_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    let profile = panel
        .local_state
        .get("profile")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("balanced")
        .to_string();
    let mode = panel
        .local_state
        .get("media_mode")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("image")
        .to_string();
    let key = format!("{mode}::{model_id}::{profile}");
    if key == panel.create_preset_key || model_id.trim().is_empty() {
        return;
    }
    panel.create_preset_key = key;
    let options = crate::media_image_defaults::image_options_for_model(
        Some(model_id.as_str()),
        Some(profile.as_str()),
    );
    let state = &mut panel.local_state;
    if let Some(width) = options.width {
        state.insert("width".into(), serde_json::Value::from(width));
    }
    if let Some(height) = options.height {
        state.insert("height".into(), serde_json::Value::from(height));
    }
    if let Some(steps) = options.steps {
        state.insert("steps".into(), serde_json::Value::from(steps));
    }
    if let Some(cfg) = options.cfg_scale {
        state.insert("cfg_scale".into(), serde_json::Value::from(cfg));
    }
    state.insert(
        "sampling_method".into(),
        options
            .sampling_method
            .map(serde_json::Value::String)
            .unwrap_or_else(|| serde_json::Value::String(String::new())),
    );
    if let Some(value) = options.offload_to_cpu {
        state.insert("offload_to_cpu".into(), serde_json::Value::Bool(value));
    }
    if let Some(value) = options.diffusion_fa {
        state.insert("diffusion_fa".into(), serde_json::Value::Bool(value));
    }
    if let Some(value) = options.stream_layers {
        state.insert("stream_layers".into(), serde_json::Value::Bool(value));
    }
    if let Some(value) = options.max_vram {
        state.insert("max_vram".into(), serde_json::Value::String(value));
    }
    // Native Create exposes the format as an aspect-ratio transform over the
    // current model base size. Apply the same transform when a non-custom
    // format is already selected.
    let format = state
        .get("format")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("custom");
    if format != "custom" {
        let base = state
            .get("width")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(512)
            .max(
                state
                    .get("height")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(512),
            );
        let (width, height) = match format {
            "16:9" => (base, (base * 9 / 16).max(64)),
            "9:16" => ((base * 9 / 16).max(64), base),
            "1:1" => (base, base),
            _ => return,
        };
        state.insert("width".into(), serde_json::Value::from(width));
        state.insert("height".into(), serde_json::Value::from(height));
    }
}
