//! Designer / CI screenshots via egui `ViewportCommand::Screenshot`.
//!
//! Set `AOS_UI_SCREENSHOT_DIR` to a writable directory; the app captures:
//! 1. Rail with painted icons (FR)
//! 2. Settings → Police de l'interface (default font + live preview)
//! 3. Same panel with Inter selected (preview updates)
//! 4. Rail with clearer icon metaphors (FR)
//! 5. Chat sidebar Web / fichiers painted checkbox (unchecked + checked)
//! 6. Create layer list with painted z-order / visibility / grip icons (FR)

use crate::decl_ui::DeclUiPanelState;
use crate::prefs::save_preferences;
use crate::{Tab, UiApp};
use aos_proto::create_contract::MODULE_NAME;
use aos_proto::decl_ui::DeclUiDocument;
use aos_proto::rich_app_contract::UI_CONTRACT_V2;
use aos_proto::rich_composition::{layers_to_value, RichLayer};
use aos_proto::ModuleInfo;
use eframe::egui::{self, ColorImage, Event, UserData};
use serde_json::json;
use std::path::{Path, PathBuf};

/// Ensure designer rail shots include primary tabs that depend on module list RPC.
pub fn seed_screenshot_modules(app: &mut UiApp) {
    if app.create_module_installed() {
        return;
    }
    app.settings_ui.installed_modules.push(ModuleInfo {
        name: MODULE_NAME.into(),
        version: "0.1.0".into(),
        granted_caps: Vec::new(),
        tools: Vec::new(),
        quarantined: false,
        ui_mode: Some("declarative_ui".into()),
        ui_title: None,
    });
}

/// Minimal Create surface for layer-list QA captures (labels locked to shipped FR copy).
const LAYER_LIST_SHOT_DOC: &str = r#"{
  "type": "declarative_ui",
  "contract": 2,
  "title": "Create",
  "title_key": "app_title",
  "labels": {
    "fallback": "fr",
    "fr": {
      "app_title": "Créer",
      "layers_list_label": "Ordre des calques (arrière vers avant)"
    }
  },
  "root": {
    "kind": "column",
    "children": [
      {
        "kind": "layer_list",
        "layers_key": "composition_layers",
        "selected_key": "composition_selected",
        "next_id_key": "composition_next_id",
        "label_key": "layers_list_label"
      }
    ]
  }
}"#;

/// Load Create declarative UI and seed composition layers for layer-list captures.
pub fn seed_screenshot_create_layers(app: &mut UiApp) {
    seed_screenshot_modules(app);
    let panel = app
        .decl_panels
        .entry(MODULE_NAME.into())
        .or_insert_with(|| DeclUiPanelState::new(MODULE_NAME));
    match DeclUiDocument::parse_json_with_contract(LAYER_LIST_SHOT_DOC.as_bytes(), UI_CONTRACT_V2) {
        Ok(doc) => {
            panel.set_document(doc);
            let layers: Vec<RichLayer> = (0..5)
                .map(|i| {
                    let mut layer = RichLayer::new(i as u64 + 1);
                    layer.label = if i == 2 {
                        "arrière-plan".into()
                    } else {
                        format!("Calque {}", i + 1)
                    };
                    layer
                })
                .collect();
            panel
                .local_state
                .insert("composition_layers".into(), layers_to_value(&layers));
            panel
                .local_state
                .insert("composition_selected".into(), json!(3));
            panel
                .local_state
                .insert("composition_next_id".into(), json!(6));
        }
        Err(err) => {
            eprintln!("seed_screenshot_create_layers: parse failed: {err}");
        }
    }
    app.open_module_tab(MODULE_NAME.into());
}

pub fn screenshot_dir_from_env() -> Option<PathBuf> {
    std::env::var("AOS_UI_SCREENSHOT_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
}

pub struct UiScreenshotHarness {
    dir: PathBuf,
    /// Settle frames before requesting the next capture.
    settle_left: u32,
    step: u8,
    waiting: bool,
}

impl UiScreenshotHarness {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            settle_left: 4,
            step: 0,
            waiting: false,
        }
    }

    pub fn drain_events(&mut self, ctx: &egui::Context) {
        let shots: Vec<(String, ColorImage)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|ev| {
                    if let Event::Screenshot { image, user_data, .. } = ev {
                        user_data
                            .data
                            .as_ref()
                            .and_then(|d| d.downcast_ref::<String>())
                            .map(|tag| (tag.clone(), image.as_ref().clone()))
                    } else {
                        None
                    }
                })
                .collect()
        });
        for (tag, image) in shots {
            let path = self.dir.join(format!("{tag}.png"));
            if let Err(err) = save_png(&path, &image) {
                eprintln!("AOS_UI_SCREENSHOT_DIR: failed to write {}: {err}", path.display());
            } else {
                eprintln!("AOS_UI_SCREENSHOT_DIR: wrote {}", path.display());
            }
            self.waiting = false;
        }
    }

    pub fn tick(&mut self, app: &mut UiApp, ctx: &egui::Context) {
        if self.waiting {
            return;
        }
        if self.settle_left > 0 {
            self.settle_left -= 1;
            return;
        }

        match self.step {
            0 => {
                seed_screenshot_modules(app);
                app.tab = Tab::Chat;
                self.request(ctx, "01-rail-painted-icons-fr");
                self.step = 1;
                self.settle_left = 3;
            }
            1 => {
                app.tab = Tab::Settings;
                app.settings_ui.section = "me".into();
                app.prefs.language = "fr".into();
                app.prefs.ui_font = "default".into();
                save_preferences(&app.prefs);
                self.request(ctx, "02-settings-police-interface-default-fr");
                self.step = 2;
                self.settle_left = 3;
            }
            2 => {
                app.prefs.ui_font = "inter".into();
                save_preferences(&app.prefs);
                self.request(ctx, "03-settings-police-interface-inter-fr");
                self.step = 3;
                self.settle_left = 3;
            }
            3 => {
                seed_screenshot_modules(app);
                app.tab = Tab::Chat;
                app.prefs.language = "fr".into();
                app.prefs.ui_font = "default".into();
                app.chat_state.sidebar.tools_open = false;
                save_preferences(&app.prefs);
                self.request(ctx, "04-rail-metaphors-fr");
                self.step = 4;
                self.settle_left = 3;
            }
            4 => {
                app.tab = Tab::Chat;
                app.chat_state.sidebar.tools_open = false;
                self.request(ctx, "05-composer-web-files-fr");
                self.step = 5;
                self.settle_left = 3;
            }
            5 => {
                app.chat_state.sidebar.tools_open = true;
                self.request(ctx, "05-composer-web-files-checked-fr");
                self.step = 6;
                self.settle_left = 3;
            }
            6 => {
                seed_screenshot_create_layers(app);
                app.prefs.language = "fr".into();
                self.request(ctx, "06-create-layer-list-fr");
                self.step = 7;
                self.settle_left = 5;
            }
            _ => {
                eprintln!("AOS_UI_SCREENSHOT_DIR: captures complete — exiting");
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn request(&mut self, ctx: &egui::Context, tag: &str) {
        self.waiting = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(UserData::new(
            tag.to_string(),
        )));
    }
}

fn save_png(path: &Path, image: &ColorImage) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let w = image.width() as u32;
    let h = image.height() as u32;
    if w == 0 || h == 0 {
        return Err("empty screenshot".into());
    }
    let mut buf = image::RgbaImage::new(w, h);
    for (i, px) in image.pixels.iter().enumerate() {
        let x = (i as u32) % w;
        let y = (i as u32) / w;
        buf.put_pixel(x, y, image::Rgba([px.r(), px.g(), px.b(), px.a()]));
    }
    buf.save(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_png_writes_valid_dimensions() {
        let img = ColorImage {
            size: [2, 1],
            pixels: vec![
                egui::Color32::RED,
                egui::Color32::BLUE,
            ],
        };
        let dir = std::env::temp_dir().join("aos_ui_shot_test");
        let path = dir.join("test.png");
        save_png(&path, &img).expect("write png");
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
