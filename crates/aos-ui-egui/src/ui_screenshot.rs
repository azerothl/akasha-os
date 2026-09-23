//! Designer / CI screenshots via egui `ViewportCommand::Screenshot`.
//!
//! Set `AOS_UI_SCREENSHOT_DIR` to a writable directory; the app captures:
//! 1. Rail with painted icons (FR)
//! 2. Settings → Police de l'interface (default font + live preview)
//! 3. Same panel with Inter selected (preview updates)
//! 4. Rail with clearer icon metaphors (FR)
//! 5. Chat sidebar Web / fichiers painted checkbox (unchecked + checked)
//! 6. Create layer list with painted z-order / visibility / grip icons (FR)

use crate::cmd::ChatLine;
use crate::decl_ui::DeclUiPanelState;
use crate::prefs::save_preferences;
use crate::{Tab, UiApp};
use aos_proto::create_contract::MODULE_NAME;
use aos_proto::decl_ui::DeclUiDocument;
use aos_proto::rich_app_contract::UI_CONTRACT_V2;
use aos_proto::rich_composition::{layers_to_value, RichLayer};
use aos_proto::{ChatSessionMeta, ModuleInfo};
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

/// Minimal split surface that reproduces the preset/upscale collision region.
const CREATE_LAYOUT_SHOT_DOC: &str = r#"{
  "type": "declarative_ui",
  "contract": 2,
  "title": "Create",
  "title_key": "app_title",
  "labels": {
    "fallback": "fr",
    "fr": {
      "app_title": "Créer",
      "advanced_disclosure": "Avancé",
      "preset_name_label": "Nom du préréglage",
      "save_preset_label": "Enregistrer le préréglage",
      "load_preset_label": "Charger le préréglage",
      "upscale_section": "Agrandir le résultat",
      "upscale_model_label": "Modèle d'agrandissement",
      "upscale_repeats_label": "Passes",
      "upscale_tile_label": "Taille des tuiles",
      "upscale_label": "Agrandir l'image actuelle",
      "result_toolbar_section": "Résultat",
      "save_label": "Enregistrer l'image",
      "regenerate_label": "Régénérer",
      "variant_label": "Nouvelle variante"
    },
    "en": {
      "app_title": "Create",
      "advanced_disclosure": "Advanced",
      "preset_name_label": "Preset name",
      "save_preset_label": "Save preset",
      "load_preset_label": "Load preset",
      "upscale_section": "Upscale result",
      "upscale_model_label": "Upscaler model",
      "upscale_repeats_label": "Passes",
      "upscale_tile_label": "Tile size",
      "upscale_label": "Upscale current image",
      "result_toolbar_section": "Result",
      "save_label": "Save image",
      "regenerate_label": "Regenerate",
      "variant_label": "New variant"
    }
  },
  "root": {
    "kind": "split",
    "split_ratio": 0.48,
    "children": [
      {
        "kind": "column",
        "children": [
          {
            "kind": "section",
            "label_key": "advanced_disclosure",
            "children": [
              {
                "kind": "text_input",
                "state_key": "saved_preset_name",
                "label_key": "preset_name_label"
              },
              {
                "kind": "row",
                "toolbar": true,
                "children": [
                  {
                    "kind": "button",
                    "action": "save_preset",
                    "label_key": "save_preset_label",
                    "icon_key": "save_preset"
                  },
                  {
                    "kind": "button",
                    "action": "load_preset",
                    "label_key": "load_preset_label",
                    "icon_key": "load_preset"
                  }
                ]
              }
            ]
          }
        ]
      },
      {
        "kind": "column",
        "children": [
          {
            "kind": "section",
            "label_key": "upscale_section",
            "children": [
              {
                "kind": "text_input",
                "state_key": "upscale_model",
                "label_key": "upscale_model_label"
              },
              {
                "kind": "row",
                "children": [
                  {
                    "kind": "number",
                    "state_key": "upscale_repeats",
                    "min": 1,
                    "max": 4,
                    "label_key": "upscale_repeats_label"
                  },
                  {
                    "kind": "number",
                    "state_key": "upscale_tile_size",
                    "min": 32,
                    "max": 512,
                    "label_key": "upscale_tile_label"
                  }
                ]
              },
              {
                "kind": "button",
                "action": "upscale",
                "label_key": "upscale_label",
                "icon_key": "upscale"
              }
            ]
          },
          {
            "kind": "section",
            "label_key": "result_toolbar_section",
            "children": [
              {
                "kind": "row",
                "toolbar": true,
                "children": [
                  {
                    "kind": "button",
                    "action": "save_result",
                    "label_key": "save_label",
                    "icon_key": "save"
                  },
                  {
                    "kind": "button",
                    "action": "regenerate",
                    "label_key": "regenerate_label",
                    "icon_key": "regenerate"
                  },
                  {
                    "kind": "button",
                    "action": "variant",
                    "label_key": "variant_label",
                    "icon_key": "variant"
                  }
                ]
              }
            ]
          }
        ]
      }
    ]
  }
}"#;

/// Seed a focused Create layout shot for the preset/upscale overlap region.
pub fn seed_screenshot_create_layout(app: &mut UiApp) {
    seed_screenshot_modules(app);
    let panel = app
        .decl_panels
        .entry(MODULE_NAME.into())
        .or_insert_with(|| DeclUiPanelState::new(MODULE_NAME));
    match DeclUiDocument::parse_json_with_contract(
        CREATE_LAYOUT_SHOT_DOC.as_bytes(),
        UI_CONTRACT_V2,
    ) {
        Ok(doc) => {
            panel.set_document(doc);
            panel
                .local_state
                .insert("advanced_open".into(), json!(true));
            panel.local_state.insert("upscale_open".into(), json!(true));
            panel
                .local_state
                .insert("saved_preset_name".into(), json!("demo-preset"));
            panel
                .local_state
                .insert("upscale_model".into(), json!("realesrgan"));
            panel.local_state.insert("upscale_repeats".into(), json!(1));
            panel
                .local_state
                .insert("upscale_tile_size".into(), json!(128));
        }
        Err(err) => {
            eprintln!("seed_screenshot_create_layout: parse failed: {err}");
        }
    }
    app.prefs.language = "fr".into();
    save_preferences(&app.prefs);
    app.open_module_tab(MODULE_NAME.into());
}

const ILLUSTRATION_MODULE: &str = "illustration-studio";

/// Seed Illustration Studio DeclUI for compose→edit→beauty layout QA.
pub fn seed_screenshot_illustration_layout(app: &mut UiApp, language: &str) {
    seed_screenshot_modules(app);
    if let Some(info) = app.settings_ui.installed_modules.iter_mut().find(|info| info.name == ILLUSTRATION_MODULE) {
        info.version = "0.7.21".into();
    } else {
        app.settings_ui.installed_modules.push(ModuleInfo {
            name: ILLUSTRATION_MODULE.into(),
            version: "0.7.21".into(),
            granted_caps: Vec::new(),
            tools: Vec::new(),
            quarantined: false,
            ui_mode: Some("declarative_ui".into()),
            ui_title: Some("Illustration Studio".into()),
        });
    }
    let panel = app
        .decl_panels
        .entry(ILLUSTRATION_MODULE.into())
        .or_insert_with(|| DeclUiPanelState::new(ILLUSTRATION_MODULE));
    const RAW: &str = include_str!("../../../modules/illustration-studio/ui/index.json");
    match DeclUiDocument::parse_json_with_contract(RAW.as_bytes(), UI_CONTRACT_V2) {
        Ok(doc) => {
            panel.set_document(doc);
            const DEMO_SCENE: &str =
                include_str!("../../../modules/illustration-studio/demo.scene.yaml");
            panel
                .local_state
                .insert("scene".into(), json!(DEMO_SCENE));
            panel
                .local_state
                .insert("selected_id".into(), json!("box"));
            panel
                .local_state
                .insert("prompt".into(), json!("A man enters an old library."));
            panel.local_state.insert("style_id".into(), json!("pencil"));
            panel.local_state.insert("pose_open".into(), json!(false));
            panel.local_state.insert("locks_open".into(), json!(false));
            panel.local_state.insert("mesh_open".into(), json!(false));
            panel.local_state.insert("comic_open".into(), json!(false));
            panel
                .local_state
                .insert("storyboard_open".into(), json!(false));
            panel.local_state.insert("packs_open".into(), json!(false));
            panel.local_state.insert("camera_open".into(), json!(false));
            panel.local_state.insert("lights_open".into(), json!(false));
            panel.local_state.insert("light_type".into(), json!("point"));
            panel.local_state.insert("light_intensity".into(), json!(1.5));
            panel.local_state.insert("color_r".into(), json!(255));
            panel.local_state.insert("color_g".into(), json!(242));
            panel.local_state.insert("color_b".into(), json!(224));
            panel.local_state.insert("project_open".into(), json!(true));
            panel.local_state.insert("project_id".into(), json!("project-1"));
            panel.local_state.insert("project_title".into(), json!("Studio preview"));
            panel.local_state.insert("work_area".into(), json!("scene3d"));
            // Logical virtual-fs path; host resolves under AOS_HOME/var/storage/data.
            panel.local_state.insert(
                "beauty_path".into(),
                json!("/documents/illustrations/beauty-cpu.png"),
            );
        }
        Err(err) => {
            eprintln!("seed_screenshot_illustration_layout: parse failed: {err}");
        }
    }
    app.prefs.language = language.into();
    app.prefs.ui_layout.activity_panel_open = false;
    app.prefs.ui_layout.context_panel_open = false;
    save_preferences(&app.prefs);
    app.open_module_tab(ILLUSTRATION_MODULE.into());
}

pub fn screenshot_dir_from_env() -> Option<PathBuf> {
    std::env::var("AOS_UI_SCREENSHOT_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
}

fn screenshot_focus_create_layout() -> bool {
    matches!(
        std::env::var("AOS_UI_SCREENSHOT_FOCUS").ok().as_deref(),
        Some("create-layout") | Some("create_layout")
    )
}

fn screenshot_focus_illustration_layout() -> bool {
    matches!(
        std::env::var("AOS_UI_SCREENSHOT_FOCUS").ok().as_deref(),
        Some("illustration-layout") | Some("illustration_layout")
    )
}

fn screenshot_focus_marketing() -> bool {
    matches!(
        std::env::var("AOS_UI_SCREENSHOT_FOCUS").ok().as_deref(),
        Some("marketing")
    )
}

fn screenshot_result_path() -> String {
    std::env::var("AOS_UI_SCREENSHOT_RESULT")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "/downloads/images/journey-teapot.png".into())
}

/// Minimal Create surface for marketing: prompt + large result preview.
const MARKETING_CREATE_DOC: &str = r#"{
  "type": "declarative_ui",
  "contract": 2,
  "title": "Create",
  "title_key": "app_title",
  "labels": {
    "fallback": "fr",
    "fr": {
      "app_title": "Créer",
      "prompt_label": "Invite",
      "generate_label": "Générer",
      "result_section": "Résultat",
      "history_section": "Historique"
    },
    "en": {
      "app_title": "Create",
      "prompt_label": "Prompt",
      "generate_label": "Generate",
      "result_section": "Result",
      "history_section": "History"
    }
  },
  "root": {
    "kind": "split",
    "split_ratio": 0.30,
    "children": [
      {
        "kind": "column",
        "children": [
          {
            "kind": "textarea",
            "state_key": "prompt",
            "label_key": "prompt_label"
          },
          {
            "kind": "row",
            "toolbar": true,
            "children": [
              {
                "kind": "button",
                "action": "generate_image",
                "label_key": "generate_label"
              }
            ]
          },
          {
            "kind": "section",
            "label_key": "history_section",
            "children": [
              {
                "kind": "text",
                "text": "théière rouge · il y a un instant"
              }
            ]
          }
        ]
      },
      {
        "kind": "column",
        "children": [
          {
            "kind": "section",
            "label_key": "result_section",
            "children": [
              {
                "kind": "image_view",
                "resource": "$local.result_path"
              }
            ]
          }
        ]
      }
    ]
  }
}"#;

fn seed_marketing_sessions(app: &mut UiApp) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    app.chat_state.sessions = vec![
        ChatSessionMeta {
            id: "shot-a".into(),
            title: "Théière — atelier".into(),
            created_ms: now.saturating_sub(120_000),
            updated_ms: now,
            archived: false,
            pinned: true,
            message_count: 4,
            model_id: Some("local:qwen3.5-9b-instruct".into()),
            mode: Default::default(),
            members: vec![],
            conductor_policy: Default::default(),
            canvas_open: false,
            canvas_aspect: Default::default(),
        },
        ChatSessionMeta {
            id: "shot-b".into(),
            title: "Voyage Tokyo".into(),
            created_ms: now.saturating_sub(90_000),
            updated_ms: now.saturating_sub(30_000),
            archived: false,
            pinned: false,
            message_count: 2,
            model_id: None,
            mode: Default::default(),
            members: vec![],
            conductor_policy: Default::default(),
            canvas_open: false,
            canvas_aspect: Default::default(),
        },
        ChatSessionMeta {
            id: "shot-c".into(),
            title: "Notes Rust".into(),
            created_ms: now.saturating_sub(60_000),
            updated_ms: now.saturating_sub(45_000),
            archived: false,
            pinned: false,
            message_count: 1,
            model_id: None,
            mode: Default::default(),
            members: vec![],
            conductor_policy: Default::default(),
            canvas_open: false,
            canvas_aspect: Default::default(),
        },
    ];
    app.chat_state.active_session = Some("shot-a".into());
}

/// Rail marketing shot: Memory selected so the primary rail selection differs from Chat.
pub fn seed_screenshot_marketing_rail(app: &mut UiApp) {
    seed_screenshot_modules(app);
    seed_marketing_sessions(app);
    app.prefs.language = "fr".into();
    app.prefs.ui_layout.activity_panel_open = false;
    app.prefs.ui_layout.context_panel_open = false;
    app.prefs.ui_layout.chat_sidebar_width = 200.0;
    save_preferences(&app.prefs);
    app.memory_ui.query = "préférence".into();
    app.memory_ui.note = "Je préfère le français pour l'interface.".into();
    app.tab = Tab::Memory;
}

/// Chat marketing shot: conversation transcript with session list visible.
pub fn seed_screenshot_marketing_chat(app: &mut UiApp) {
    seed_screenshot_modules(app);
    seed_marketing_sessions(app);
    app.prefs.language = "fr".into();
    app.prefs.ui_layout.activity_panel_open = false;
    app.prefs.ui_layout.context_panel_open = false;
    app.prefs.ui_layout.chat_sidebar_width = 220.0;
    save_preferences(&app.prefs);
    app.chat_state.sidebar.tools_open = false;
    app.chat = vec![
        ChatLine::plain(
            "user",
            "Peux-tu m’aider à décrire une théière rouge pour Create ?",
        ),
        ChatLine::plain(
            "assistant",
            "Oui — lumière douce, table en bois clair, photo produit, sans texte. Tu peux lancer Générer dans Créer.",
        ),
        ChatLine::plain(
            "user",
            "Parfait. Garde aussi « pas de texte dans l’image ».",
        ),
        ChatLine::plain(
            "assistant",
            "Noté. Invite prête — ouvre Créer et lance Générer quand tu veux.",
        ),
    ];
    app.tab = Tab::Chat;
}

pub fn seed_screenshot_marketing_create(app: &mut UiApp) {
    use crate::rich_decl::ImageViewInteractionState;

    seed_screenshot_modules(app);
    let panel = app
        .decl_panels
        .entry(MODULE_NAME.into())
        .or_insert_with(|| DeclUiPanelState::new(MODULE_NAME));
    match DeclUiDocument::parse_json_with_contract(MARKETING_CREATE_DOC.as_bytes(), UI_CONTRACT_V2)
    {
        Ok(doc) => {
            panel.set_document(doc);
            panel.seed_local(
                "prompt",
                json!(
                    "A red ceramic teapot on a pale wooden table, soft daylight, product photography, no text"
                ),
            );
            panel.seed_local("result_path", json!(screenshot_result_path()));
            // Fill the preview panel (slight zoom past 1:1 so 512² reads larger).
            panel.image_views.insert(
                "preview".into(),
                ImageViewInteractionState {
                    zoom: 1.45,
                    ..Default::default()
                },
            );
        }
        Err(err) => {
            eprintln!("seed_screenshot_marketing_create: parse failed: {err}");
        }
    }
    app.prefs.language = "fr".into();
    app.prefs.ui_layout.activity_panel_open = false;
    app.prefs.ui_layout.context_panel_open = false;
    save_preferences(&app.prefs);
    app.open_module_tab(MODULE_NAME.into());
}

pub struct UiScreenshotHarness {
    dir: PathBuf,
    /// Settle frames before requesting the next capture.
    settle_left: u32,
    step: u8,
    waiting: bool,
    focus_create_layout: bool,
    focus_illustration_layout: bool,
    focus_marketing: bool,
}

impl UiScreenshotHarness {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            settle_left: 4,
            step: 0,
            waiting: false,
            focus_create_layout: screenshot_focus_create_layout(),
            focus_illustration_layout: screenshot_focus_illustration_layout(),
            focus_marketing: screenshot_focus_marketing(),
        }
    }

    pub fn drain_events(&mut self, ctx: &egui::Context) {
        let shots: Vec<(String, ColorImage)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|ev| {
                    if let Event::Screenshot {
                        image, user_data, ..
                    } = ev
                    {
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
                eprintln!(
                    "AOS_UI_SCREENSHOT_DIR: failed to write {}: {err}",
                    path.display()
                );
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

        if self.focus_create_layout {
            match self.step {
                0 => {
                    seed_screenshot_create_layout(app);
                    self.request(ctx, "07-create-preset-upscale-layout-fr");
                    self.step = 1;
                    self.settle_left = 6;
                }
                _ => {
                    eprintln!("AOS_UI_SCREENSHOT_DIR: create-layout capture complete — exiting");
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            return;
        }

        if self.focus_illustration_layout {
            match self.step {
                0 => {
                    seed_screenshot_illustration_layout(app, "en");
                    if let Some(panel) = app.decl_panels.get_mut(ILLUSTRATION_MODULE) {
                        panel.local_state.insert("project_id".into(), json!(""));
                        panel.local_state.insert("project_title".into(), json!(""));
                    }
                    self.request(ctx, "illustration-home-en");
                    self.step = 1;
                    self.settle_left = 8;
                }
                1 => {
                    seed_screenshot_illustration_layout(app, "en");
                    self.request(ctx, "illustration-stage-en");
                    self.step = 2;
                    self.settle_left = 8;
                }
                2 => {
                    seed_screenshot_illustration_layout(app, "fr");
                    self.request(ctx, "illustration-stage-fr");
                    self.step = 3;
                    self.settle_left = 8;
                }
                3 => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(900.0, 650.0)));
                    self.step = 4;
                    self.settle_left = 8;
                }
                4 => {
                    self.request(ctx, "illustration-stage-compact-fr");
                    self.step = 5;
                    self.settle_left = 8;
                }
                _ => {
                    eprintln!(
                        "AOS_UI_SCREENSHOT_DIR: illustration-layout capture complete — exiting"
                    );
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            return;
        }

        if self.focus_marketing {
            match self.step {
                0 => {
                    seed_screenshot_marketing_rail(app);
                    self.request(ctx, "m1-rail");
                    self.step = 1;
                    self.settle_left = 6;
                }
                1 => {
                    seed_screenshot_marketing_chat(app);
                    self.request(ctx, "m2-chat");
                    self.step = 2;
                    self.settle_left = 6;
                }
                2 => {
                    seed_screenshot_marketing_create(app);
                    self.request(ctx, "m3-create");
                    self.step = 3;
                    self.settle_left = 10;
                }
                _ => {
                    eprintln!("AOS_UI_SCREENSHOT_DIR: marketing captures complete — exiting");
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
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
            pixels: vec![egui::Color32::RED, egui::Color32::BLUE],
        };
        let dir = std::env::temp_dir().join("aos_ui_shot_test");
        let path = dir.join("test.png");
        save_png(&path, &img).expect("write png");
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
