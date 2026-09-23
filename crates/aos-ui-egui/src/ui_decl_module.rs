//! Declarative module panel integration.

use crate::cmd::Cmd;
use crate::{decl_ui, i18n, UiApp};
use eframe::egui;

impl UiApp {
    pub(crate) fn ui_decl_module(&mut self, ui: &mut egui::Ui, module: &str) {
        let needs_load = self
            .decl_panels
            .get(module)
            .map(|panel| panel.needs_ui_load())
            .unwrap_or(true);
        if needs_load {
            self.decl_panels
                .entry(module.to_string())
                .or_insert_with(|| decl_ui::DeclUiPanelState::new(module));
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
            // Re-run after patches so Image↔Video / model picks apply in the
            // same interaction (select reconcile updates model_id via patch).
            if module == "create" {
                sync_create_generation_defaults(panel);
            }
            if module == "illustration-studio" {
                if let Some(inv) = &actions.invoke {
                    if inv.tool == "illustration.project.save" {
                        let fr = self.prefs.language.starts_with("fr");
                        panel.status = if fr {
                            "Enregistrement du projet…".into()
                        } else {
                            "Saving project…".into()
                        };
                    }
                }
                if actions.service_action.as_ref().is_some_and(|action| {
                    matches!(
                        action.action_id.as_str(),
                        "cpu_beauty" | "stub_beauty" | "blender_beauty" | "comic_render"
                    )
                }) {
                    let fr = self.prefs.language.starts_with("fr");
                    panel
                        .local_state
                        .insert("beauty_path".into(), serde_json::Value::String(String::new()));
                    panel.local_state.insert(
                        "beauty_summary".into(),
                        serde_json::Value::String(String::new()),
                    );
                    panel.status = if fr { "Rendu en cours…".into() } else { "Rendering…".into() };
                }
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

/// Keep the declarative Create controls in lockstep with the model/profile
/// recipe (catalogue `video_defaults` + hardcoded presets). Runs only when the
/// mode/model/profile triple changes so the user can still tune knobs after.
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
    let defaults = crate::media_image_defaults::create_generation_defaults(
        Some(model_id.as_str()),
        Some(profile.as_str()),
        &mode,
    );
    let state = &mut panel.local_state;
    state.insert("width".into(), serde_json::Value::from(defaults.width));
    state.insert("height".into(), serde_json::Value::from(defaults.height));
    state.insert("steps".into(), serde_json::Value::from(defaults.steps));
    state.insert(
        "cfg_scale".into(),
        serde_json::Value::from(defaults.cfg_scale),
    );
    state.insert(
        "sampling_method".into(),
        serde_json::Value::String(defaults.sampling_method),
    );
    if let Some(format) = defaults.format {
        state.insert("format".into(), serde_json::Value::String(format.into()));
    } else if mode == "image" {
        // Leaving video (`format=custom`) for an image pack: restore a quick
        // square default unless the user already picked a named aspect.
        let current = state
            .get("format")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("custom");
        if current == "custom" {
            state.insert("format".into(), serde_json::Value::String("1:1".into()));
        }
    }
    if let Some(frames) = defaults.video_frames {
        state.insert("video_frames".into(), serde_json::Value::from(frames));
    }
    if let Some(fps) = defaults.fps {
        state.insert("fps".into(), serde_json::Value::from(fps));
    }
    if let Some(shift) = defaults.flow_shift {
        state.insert("flow_shift".into(), serde_json::Value::from(shift));
    }
    if let Some(value) = defaults.offload_to_cpu {
        state.insert("offload_to_cpu".into(), serde_json::Value::Bool(value));
    } else {
        state.insert("offload_to_cpu".into(), serde_json::Value::Bool(false));
    }
    if let Some(value) = defaults.diffusion_fa {
        state.insert("diffusion_fa".into(), serde_json::Value::Bool(value));
    } else {
        state.insert("diffusion_fa".into(), serde_json::Value::Bool(false));
    }
    if let Some(value) = defaults.stream_layers {
        state.insert("stream_layers".into(), serde_json::Value::Bool(value));
    } else {
        state.insert("stream_layers".into(), serde_json::Value::Bool(false));
    }
    if let Some(value) = defaults.max_vram {
        state.insert("max_vram".into(), serde_json::Value::String(value));
    } else {
        state.insert("max_vram".into(), serde_json::Value::String(String::new()));
    }
    state.insert(
        "sd_mode".into(),
        serde_json::Value::String(defaults.sd_mode),
    );

    // Named aspects only for image mode — video keeps catalogue pixels.
    if mode != "video" {
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
}

#[cfg(test)]
mod tests {
    use super::sync_create_generation_defaults;
    use crate::decl_ui::DeclUiPanelState;
    use serde_json::json;

    #[test]
    fn switching_to_ltx_video_sets_catalogue_recipe_not_square() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        std::env::set_var("AOS_HOME", &root);
        let mut panel = DeclUiPanelState::new("create");
        panel
            .local_state
            .insert("media_mode".into(), json!("video"));
        panel
            .local_state
            .insert("model_id".into(), json!("local:ltx2.3-dev"));
        panel
            .local_state
            .insert("profile".into(), json!("balanced"));
        // Simulate the schema default that previously wrecked video aspect.
        panel.local_state.insert("format".into(), json!("1:1"));
        panel.local_state.insert("width".into(), json!(512));
        panel.local_state.insert("height".into(), json!(512));
        sync_create_generation_defaults(&mut panel);
        assert_eq!(
            panel.local_state.get("format").and_then(|v| v.as_str()),
            Some("custom")
        );
        assert_eq!(
            (
                panel.local_state.get("width").and_then(|v| v.as_u64()),
                panel.local_state.get("height").and_then(|v| v.as_u64())
            ),
            (Some(768), Some(512))
        );
        assert_eq!(
            panel.local_state.get("fps").and_then(|v| v.as_u64()),
            Some(24)
        );
        assert!(
            panel
                .local_state
                .get("video_frames")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                >= 33
        );
        assert_eq!(
            panel.local_state.get("sd_mode").and_then(|v| v.as_str()),
            Some("vid_gen")
        );
    }

    #[test]
    fn wan_image_pack_syncs_catalogue_vid_gen_mode() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        std::env::set_var("AOS_HOME", &root);
        let mut panel = DeclUiPanelState::new("create");
        panel
            .local_state
            .insert("media_mode".into(), json!("image"));
        panel
            .local_state
            .insert("model_id".into(), json!("local:wan2.2-t2i"));
        panel
            .local_state
            .insert("profile".into(), json!("balanced"));
        panel.local_state.insert("sd_mode".into(), json!("img_gen"));
        sync_create_generation_defaults(&mut panel);
        assert_eq!(
            panel.local_state.get("sd_mode").and_then(|v| v.as_str()),
            Some("vid_gen")
        );
    }
}
