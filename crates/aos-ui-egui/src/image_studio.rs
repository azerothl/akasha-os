//! First-class Image studio page (P09.8) — closed sd.cpp options, no webview.

use crate::cmd::Cmd;
use crate::decl_ui;
use crate::i18n::UiStrings;
use crate::models_page;
use crate::os_open::{aos_home, open_os_folder, open_url, pick_os_file, user_downloads_dir};
use aos_proto::MediaImageOptions;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ImageGenUiState {
    pub enriching: bool,
    pub upscaling: bool,
    pub step: u32,
    pub total_steps: u32,
    pub elapsed_secs: u64,
}

pub struct ImageStudioState {
    pub prompt: String,
    pub negative: String,
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub cfg: f32,
    pub seed: String,
    pub sampler: String,
    pub selected_styles: Vec<String>,
    pub selected_loras: Vec<String>,
    pub vae: String,
    pub model_id: String,
    pub profile: String,
    pub preview: Option<String>,
    /// 0..1 opacity for painting `preview` over the composition canvas.
    pub preview_overlay_opacity: f32,
    pub packs: Vec<(String, String)>,
    pub styles: Vec<String>,
    pub loras: Vec<String>,
    pub vaes: Vec<String>,
    last_preset_key: String,
    pub import_path: String,
    pub import_kind: String,
    pub custom_style_input: String,
    pub import_status: String,
    pub enrich_prompt: bool,
    /// Rewrite prompt via chat LLM into detailed prose (all image models).
    pub enhance_prompt_chat: bool,
    /// Last LLM-enriched caption (JSON or prose). Editable for re-runs.
    pub enriched_prompt: String,
    /// When true, generate uses `enriched_prompt` instead of re-running the LLM.
    pub use_edited_enriched: bool,
    pub show_enriched_prompt: bool,
    pub upscale_enabled: bool,
    pub upscale_model: String,
    pub upscale_repeats: u32,
    pub upscale_tile_size: u32,
    pub upscalers: Vec<String>,
    /// img2img: use current preview (or path) as `--init-img`.
    pub img2img_enabled: bool,
    pub img2img_strength: f32,
    pub img2img_path: String,
    pub offload_to_cpu: bool,
    pub diffusion_fa: bool,
    pub auto_fit: bool,
    pub stream_layers: bool,
    pub max_vram: String,
    pub expert_mode: bool,
    pub flow_shift: String,
    pub sd_mode: String,
    pub video_frames: String,
    pub backend: String,
    pub params_backend: String,
    pub threads: u32,
    /// Full image catalog (installed + available for download).
    catalog_packs: Vec<(String, String)>,
    /// Install confirmation after picking a non-installed pack in the combo.
    install_prompt: Option<(String, String)>,
    /// Visual composition blocks (z-order = vec order, overlaps allowed).
    composition_blocks: Vec<crate::image_composition::CompositionBlock>,
    composition_selected: Option<u64>,
    composition_next_id: u64,
}

impl Default for ImageStudioState {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            negative: String::new(),
            width: 512,
            height: 512,
            steps: 20,
            cfg: 7.0,
            seed: String::new(),
            sampler: String::new(),
            selected_styles: Vec::new(),
            selected_loras: Vec::new(),
            vae: String::new(),
            model_id: String::new(),
            profile: "balanced".to_string(),
            preview: None,
            preview_overlay_opacity: 1.0,
            packs: Vec::new(),
            styles: Vec::new(),
            loras: Vec::new(),
            vaes: Vec::new(),
            last_preset_key: String::new(),
            import_path: String::new(),
            import_kind: "lora".to_string(),
            custom_style_input: String::new(),
            import_status: String::new(),
            enrich_prompt: false,
            enhance_prompt_chat: false,
            enriched_prompt: String::new(),
            use_edited_enriched: false,
            show_enriched_prompt: true,
            upscale_enabled: false,
            upscale_model: String::new(),
            upscale_repeats: 1,
            upscale_tile_size: 128,
            upscalers: Vec::new(),
            img2img_enabled: false,
            img2img_strength: 0.75,
            img2img_path: String::new(),
            offload_to_cpu: false,
            diffusion_fa: false,
            auto_fit: false,
            stream_layers: false,
            max_vram: String::new(),
            expert_mode: false,
            flow_shift: String::new(),
            sd_mode: String::new(),
            video_frames: String::new(),
            backend: String::new(),
            params_backend: String::new(),
            threads: 0,
            catalog_packs: Vec::new(),
            install_prompt: None,
            composition_blocks: Vec::new(),
            composition_selected: None,
            composition_next_id: 1,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ImageAssetsRegistry {
    #[serde(default)]
    styles: Vec<String>,
    #[serde(default)]
    loras: Vec<String>,
    #[serde(default)]
    vaes: Vec<String>,
}

fn image_assets_registry_path() -> std::path::PathBuf {
    aos_home().join("var/run/image-assets.json")
}

fn models_root() -> PathBuf {
    aos_home().join("share/models")
}

fn asset_subdir(kind: &str) -> PathBuf {
    match kind {
        "vae" => models_root().join("vae"),
        "style" => models_root().join("styles"),
        "upscale" => models_root().join("upscale"),
        _ => models_root().join("lora"),
    }
}

fn ensure_image_asset_dirs() {
    for sub in ["lora", "vae", "styles", "upscale"] {
        let _ = std::fs::create_dir_all(models_root().join(sub));
    }
}

fn scan_asset_filenames(dir: &Path, dst: &mut Vec<String>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        if entry.path().is_file() {
            if let Some(name) = entry.file_name().to_str() {
                if !name.starts_with('.') {
                    push_unique(dst, name.to_string());
                }
            }
        }
    }
}

fn migrate_flat_assets(reg: &ImageAssetsRegistry) {
    let root = models_root();
    for name in reg.loras.iter().chain(reg.vaes.iter()) {
        let src = root.join(name);
        if !src.is_file() {
            continue;
        }
        let role = if reg.vaes.iter().any(|v| v == name) {
            "vae"
        } else {
            "lora"
        };
        let dst = asset_subdir(role).join(name);
        if dst.exists() {
            continue;
        }
        if let Some(parent) = dst.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::rename(&src, &dst).or_else(|_| std::fs::copy(&src, &dst).map(|_| ()));
    }
}

fn load_image_assets_registry() -> ImageAssetsRegistry {
    let path = image_assets_registry_path();
    let Ok(raw) = std::fs::read_to_string(path) else {
        return ImageAssetsRegistry::default();
    };
    serde_json::from_str::<ImageAssetsRegistry>(&raw).unwrap_or_default()
}

fn save_image_assets_registry(reg: &ImageAssetsRegistry) {
    let path = image_assets_registry_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(raw) = serde_json::to_string_pretty(reg) {
        let _ = std::fs::write(path, raw);
    }
}

fn push_unique(dst: &mut Vec<String>, value: String) {
    if !value.is_empty() && !dst.iter().any(|x| x == &value) {
        dst.push(value);
    }
}

#[derive(Debug, Clone, Copy)]
struct ImageModelPreset {
    width: u32,
    height: u32,
    steps: u32,
    cfg: f32,
    sampler: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct PresetTriplet {
    fast: ImageModelPreset,
    balanced: ImageModelPreset,
    quality: ImageModelPreset,
}

fn image_model_presets(model_id: &str) -> PresetTriplet {
    match model_id {
        // FLUX schnell converges quickly on sd.cpp.
        "local:flux2" => PresetTriplet {
            fast: ImageModelPreset {
                width: 768,
                height: 768,
                steps: 3,
                cfg: 1.0,
                sampler: "euler",
            },
            balanced: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 4,
                cfg: 1.0,
                sampler: "euler",
            },
            quality: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 6,
                cfg: 1.2,
                sampler: "heun",
            },
        },
        // SD 1.5 needs a bit more steps for usable baseline output.
        "local:sd-v1-5" => PresetTriplet {
            fast: ImageModelPreset {
                width: 512,
                height: 512,
                steps: 16,
                cfg: 6.5,
                sampler: "euler_a",
            },
            balanced: ImageModelPreset {
                width: 512,
                height: 512,
                steps: 24,
                cfg: 7.0,
                sampler: "euler_a",
            },
            quality: ImageModelPreset {
                width: 768,
                height: 768,
                steps: 32,
                cfg: 7.5,
                sampler: "heun",
            },
        },
        // Ideogram 4 DiT — official sd.cpp recipe is 1024² + FA + CPU offload.
        "local:ideogram4" => PresetTriplet {
            fast: ImageModelPreset {
                width: 768,
                height: 768,
                steps: 18,
                cfg: 4.5,
                sampler: "euler",
            },
            balanced: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 28,
                cfg: 5.0,
                sampler: "euler",
            },
            quality: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 36,
                cfg: 5.5,
                sampler: "heun",
            },
        },
        "local:sdxl-base" => PresetTriplet {
            fast: ImageModelPreset {
                width: 768,
                height: 768,
                steps: 20,
                cfg: 7.0,
                sampler: "euler_a",
            },
            balanced: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 25,
                cfg: 7.0,
                sampler: "euler_a",
            },
            quality: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 32,
                cfg: 7.5,
                sampler: "heun",
            },
        },
        "local:z-image-turbo" => PresetTriplet {
            fast: ImageModelPreset {
                width: 768,
                height: 768,
                steps: 6,
                cfg: 1.0,
                sampler: "euler",
            },
            balanced: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 8,
                cfg: 1.0,
                sampler: "euler",
            },
            quality: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 12,
                cfg: 1.2,
                sampler: "heun",
            },
        },
        "local:qwen-image-2512" => PresetTriplet {
            fast: ImageModelPreset {
                width: 768,
                height: 768,
                steps: 16,
                cfg: 2.5,
                sampler: "euler",
            },
            balanced: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 20,
                cfg: 2.5,
                sampler: "euler",
            },
            quality: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 28,
                cfg: 3.0,
                sampler: "heun",
            },
        },
        "local:krea2-raw" => PresetTriplet {
            fast: ImageModelPreset {
                width: 768,
                height: 768,
                steps: 12,
                cfg: 3.5,
                sampler: "euler",
            },
            balanced: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 16,
                cfg: 4.0,
                sampler: "euler",
            },
            quality: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 24,
                cfg: 4.5,
                sampler: "heun",
            },
        },
        "local:wan2.2-t2i" => PresetTriplet {
            fast: ImageModelPreset {
                width: 832,
                height: 480,
                steps: 8,
                cfg: 3.5,
                sampler: "euler",
            },
            balanced: ImageModelPreset {
                width: 832,
                height: 480,
                steps: 10,
                cfg: 3.5,
                sampler: "euler",
            },
            quality: ImageModelPreset {
                width: 1280,
                height: 720,
                steps: 14,
                cfg: 4.0,
                sampler: "heun",
            },
        },
        "local:ltx2.3-dev" => PresetTriplet {
            fast: ImageModelPreset {
                width: 768,
                height: 432,
                steps: 6,
                cfg: 6.0,
                sampler: "euler",
            },
            balanced: ImageModelPreset {
                width: 1280,
                height: 720,
                steps: 8,
                cfg: 6.0,
                sampler: "euler",
            },
            quality: ImageModelPreset {
                width: 1280,
                height: 720,
                steps: 12,
                cfg: 6.5,
                sampler: "heun",
            },
        },
        // Fallback for unknown models.
        _ => PresetTriplet {
            fast: ImageModelPreset {
                width: 512,
                height: 512,
                steps: 12,
                cfg: 6.5,
                sampler: "euler",
            },
            balanced: ImageModelPreset {
                width: 512,
                height: 512,
                steps: 20,
                cfg: 7.0,
                sampler: "",
            },
            quality: ImageModelPreset {
                width: 768,
                height: 768,
                steps: 28,
                cfg: 7.5,
                sampler: "heun",
            },
        },
    }
}

fn enrichment_hint(t: &crate::i18n::UiStrings, model_id: &str) -> &'static str {
    use crate::image_prompt::{prompt_enrichment_kind, PromptEnrichmentKind};
    match prompt_enrichment_kind(model_id) {
        Some(PromptEnrichmentKind::Ideogram4) => t.studio_enriched_hint_ideogram,
        Some(PromptEnrichmentKind::GenericJson) => t.studio_enriched_hint_generic,
        None => t.studio_enriched_hint_generic,
    }
}

fn pick_preset(model_id: &str, profile: &str) -> ImageModelPreset {
    let triplet = image_model_presets(model_id);
    match profile {
        "fast" => triplet.fast,
        "quality" => triplet.quality,
        _ => triplet.balanced,
    }
}

pub fn image_options_for_model(model_id: Option<&str>, profile: Option<&str>) -> MediaImageOptions {
    let id = model_id.unwrap_or_default();
    let preset = pick_preset(id, profile.unwrap_or("balanced"));
    let wants_perf = crate::image_prompt::is_heavy_image_model(id);
    MediaImageOptions {
        width: Some(preset.width),
        height: Some(preset.height),
        steps: Some(preset.steps),
        cfg_scale: Some(preset.cfg),
        sampling_method: if preset.sampler.is_empty() {
            None
        } else {
            Some(preset.sampler.to_string())
        },
        offload_to_cpu: if wants_perf { Some(true) } else { None },
        diffusion_fa: if wants_perf { Some(true) } else { None },
        max_vram: if wants_perf { Some("-1".into()) } else { None },
        stream_layers: if wants_perf { Some(true) } else { None },
        ..MediaImageOptions::default()
    }
}

impl ImageStudioState {
    fn apply_preset_for_current_model(&mut self) {
        if self.model_id.is_empty() {
            return;
        }
        let key = format!("{}::{}", self.model_id, self.profile);
        if key == self.last_preset_key {
            return;
        }
        let prev_model = self
            .last_preset_key
            .split("::")
            .next()
            .unwrap_or("");
        let model_changed = prev_model != self.model_id;
        let preset = pick_preset(&self.model_id, &self.profile);
        let large = crate::image_prompt::is_heavy_image_model(&self.model_id);
        self.width = preset.width;
        self.height = preset.height;
        self.steps = preset.steps;
        self.cfg = preset.cfg;
        self.sampler = preset.sampler.to_string();
        self.offload_to_cpu = large;
        self.diffusion_fa = large;
        self.stream_layers = large;
        self.max_vram = if large { "-1".into() } else { String::new() };
        self.last_preset_key = key;
        if model_changed {
            self.enrich_prompt =
                crate::image_prompt::default_enrich_prompt(Some(&self.model_id));
            self.load_expert_defaults_from_catalog();
        }
    }

    fn load_expert_defaults_from_catalog(&mut self) {
        let Some(args) = catalog_engine_args(&self.model_id) else {
            self.flow_shift.clear();
            self.sd_mode.clear();
            self.video_frames.clear();
            self.backend.clear();
            self.params_backend.clear();
            return;
        };
        self.flow_shift = json_arg_as_string(args.get("flow-shift"));
        self.sd_mode = json_arg_as_string(args.get("mode"));
        self.video_frames = json_arg_as_string(args.get("video-frames"));
        self.backend = json_arg_as_string(args.get("backend"));
        coerce_backend_alias(&mut self.backend);
        self.params_backend = json_arg_as_string(args.get("params-backend"));
        coerce_params_backend_alias(&mut self.params_backend);
    }

    pub fn refresh_catalog(&mut self) {
        ensure_image_asset_dirs();
        self.packs.clear();
        self.catalog_packs.clear();
        self.styles.clear();
        self.loras.clear();
        self.vaes.clear();
        self.upscalers.clear();
        let reg = load_image_assets_registry();
        migrate_flat_assets(&reg);
        let path = aos_home().join("share/models/catalog-offerings.json");
        let Ok(raw) = std::fs::read_to_string(path) else {
            return;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
            return;
        };
        let Some(models) = v.get("models").and_then(|m| m.as_array()) else {
            return;
        };
        for m in models {
            let id = m.get("id").and_then(|x| x.as_str()).unwrap_or("");
            let name = m.get("name").and_then(|x| x.as_str()).unwrap_or(id);
            let image = m
                .get("profiles")
                .and_then(|p| p.as_array())
                .map(|a| a.iter().any(|x| x.as_str() == Some("image")))
                .unwrap_or(false)
                || m.get("modality").and_then(|x| x.as_str()) == Some("image");
            if image && !id.is_empty() {
                self.catalog_packs
                    .push((id.to_string(), name.to_string()));
                if models_page::is_model_installed(id) {
                    self.packs.push((id.to_string(), name.to_string()));
                }
            }
            let upscale = m
                .get("profiles")
                .and_then(|p| p.as_array())
                .map(|a| a.iter().any(|x| x.as_str() == Some("upscale")))
                .unwrap_or(false)
                || m.get("modality").and_then(|x| x.as_str()) == Some("upscale");
            if upscale {
                if let Some(fname) = m.get("filename").and_then(|x| x.as_str()) {
                    push_unique(&mut self.upscalers, fname.to_string());
                }
            }
            if let Some(extras) = m.get("extra_files").and_then(|e| e.as_array()) {
                for f in extras {
                    let fname = f
                        .get("filename")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string();
                    if fname.is_empty() {
                        continue;
                    }
                    match f.get("role").and_then(|x| x.as_str()) {
                        Some("vae") => self.vaes.push(fname),
                        Some("lora") => self.loras.push(fname),
                        Some("style") => self.styles.push(fname),
                        _ => {}
                    }
                }
            }
        }
        if self.model_id.is_empty()
            || (!self.packs.is_empty()
                && !models_page::is_model_installed(&self.model_id))
        {
            if let Some((id, _)) = self.packs.first() {
                self.model_id = id.clone();
                self.last_preset_key.clear();
            }
        }
        scan_asset_filenames(&asset_subdir("lora"), &mut self.loras);
        scan_asset_filenames(&asset_subdir("vae"), &mut self.vaes);
        scan_asset_filenames(&asset_subdir("style"), &mut self.styles);
        scan_asset_filenames(&asset_subdir("upscale"), &mut self.upscalers);
        if self.upscale_model.is_empty() {
            if let Some(name) = self
                .upscalers
                .iter()
                .find(|n| n.contains("RealESRGAN"))
                .or_else(|| self.upscalers.first())
            {
                self.upscale_model = name.clone();
            }
        }
        for s in reg.styles {
            push_unique(&mut self.styles, s);
        }
        for l in reg.loras {
            push_unique(&mut self.loras, l);
        }
        for v in reg.vaes {
            push_unique(&mut self.vaes, v);
        }
        self.apply_preset_for_current_model();
    }

    /// Called after a successful model download so the new pack appears as installed.
    pub fn on_download_finished(&mut self, model_id: &str) {
        self.refresh_catalog();
        self.model_id = model_id.to_string();
        self.last_preset_key.clear();
        self.install_prompt = None;
        self.apply_preset_for_current_model();
    }

    fn ui_install_prompt(
        &mut self,
        ctx: &egui::Context,
        t: &UiStrings,
        cmd: &Sender<Cmd>,
        download_busy: bool,
    ) {
        let Some((id, name)) = self.install_prompt.clone() else {
            return;
        };
        let mut open = true;
        egui::Window::new(t.studio_install_prompt_title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(t.studio_install_prompt_body.replace("{}", &name));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!download_busy, egui::Button::new(t.studio_install_yes))
                        .clicked()
                    {
                        let _ = cmd.send(Cmd::ModelDownload {
                            model_id: id.clone(),
                        });
                        self.install_prompt = None;
                    }
                    if ui.button(t.studio_install_no).clicked() {
                        self.install_prompt = None;
                    }
                });
            });
        if !open {
            self.install_prompt = None;
        }
    }

    fn import_asset_file(&mut self) -> Result<String, String> {
        let src = Path::new(self.import_path.trim());
        if self.import_path.trim().is_empty() {
            return Err("path is empty".into());
        }
        if !src.is_file() {
            return Err("file not found".into());
        }
        let allowed: &[&str] = match self.import_kind.as_str() {
            "style" => &["txt"],
            "upscale" => &["pth", "safetensors", "pt"],
            _ => &["safetensors", "ckpt", "pt", "bin"],
        };
        let ext = src
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !allowed.iter().any(|x| *x == ext) {
            return Err(format!(
                "unsupported extension for {} (allowed: {})",
                self.import_kind,
                allowed.join("/")
            ));
        }
        let dest_dir = asset_subdir(self.import_kind.as_str());
        let filename = copy_import_file(src, &dest_dir)?;
        if self.import_kind == "style" && ext == "txt" {
            Ok(filename)
        } else if self.import_kind == "upscale" {
            push_unique(&mut self.upscalers, filename.clone());
            Ok(filename)
        } else {
            let mut reg = load_image_assets_registry();
            match self.import_kind.as_str() {
                "vae" => push_unique(&mut reg.vaes, filename.clone()),
                "style" => push_unique(&mut reg.styles, filename.clone()),
                _ => push_unique(&mut reg.loras, filename.clone()),
            }
            save_image_assets_registry(&reg);
            Ok(filename)
        }
    }

    fn add_custom_style(&mut self) -> Result<String, String> {
        let style = self.custom_style_input.trim().to_string();
        if style.is_empty() {
            return Err("style is empty".into());
        }
        let mut reg = load_image_assets_registry();
        push_unique(&mut reg.styles, style.clone());
        save_image_assets_registry(&reg);
        Ok(style)
    }

    fn remove_custom_entry(&mut self, kind: &str, value: &str) -> Result<(), String> {
        if value.trim().is_empty() {
            return Err("empty value".into());
        }
        let mut reg = load_image_assets_registry();
        let before = match kind {
            "style" => reg.styles.len(),
            "vae" => reg.vaes.len(),
            _ => reg.loras.len(),
        };
        match kind {
            "style" => reg.styles.retain(|x| x != value),
            "vae" => reg.vaes.retain(|x| x != value),
            _ => reg.loras.retain(|x| x != value),
        }
        let after = match kind {
            "style" => reg.styles.len(),
            "vae" => reg.vaes.len(),
            _ => reg.loras.len(),
        };
        if before == after {
            return Err("entry not found in custom registry".into());
        }
        save_image_assets_registry(&reg);
        Ok(())
    }

    pub fn open_from_chat(
        &mut self,
        prompt: &str,
        path: &str,
        generation_prompt: Option<&str>,
    ) {
        if !prompt.is_empty() {
            self.prompt = prompt.to_string();
        }
        if let Some(json) = generation_prompt.filter(|s| !s.is_empty()) {
            self.enriched_prompt = format_enriched_display(json);
            self.show_enriched_prompt = true;
            self.use_edited_enriched = true;
        }
        self.preview = Some(path.to_string());
        self.refresh_catalog();
    }

    /// Restore prompt / enriched / composition from a sidecar next to `path`.
    pub fn apply_history_for_path(&mut self, path: &str) {
        if let Some(meta) = crate::image_history::load_image_meta(path) {
            self.apply_history(&meta);
        }
    }

    pub fn apply_history(&mut self, meta: &crate::image_history::ImageGenMeta) {
        if !meta.prompt.is_empty() {
            self.prompt = meta.prompt.clone();
        }
        if let Some(gen) = meta
            .generation_prompt
            .as_deref()
            .filter(|s| !s.is_empty())
        {
            self.enriched_prompt = format_enriched_display(gen);
            self.show_enriched_prompt = true;
            self.use_edited_enriched = true;
        } else if meta.generation_prompt.is_none() {
            // Keep existing enriched text only if we already had one for this preview.
        }
        self.set_composition_blocks(meta.composition_blocks.clone());
        if !meta.model_id.is_empty() {
            self.model_id = meta.model_id.clone();
        }
        self.preview = Some(meta.path.clone());
        self.refresh_catalog();
    }

    pub fn set_composition_blocks(
        &mut self,
        blocks: Vec<crate::image_composition::CompositionBlock>,
    ) {
        self.composition_next_id = blocks.iter().map(|b| b.id).max().unwrap_or(0).saturating_add(1);
        if self.composition_next_id == 0 {
            self.composition_next_id = 1;
        }
        self.composition_selected = blocks.last().map(|b| b.id);
        self.composition_blocks = blocks;
    }

    pub fn set_enriched_prompt(&mut self, enriched: &str) {
        if enriched.is_empty() {
            return;
        }
        self.enriched_prompt = format_enriched_display(enriched);
        self.show_enriched_prompt = true;
    }

    pub fn to_options(&self) -> MediaImageOptions {
        MediaImageOptions {
            width: Some(self.width),
            height: Some(self.height),
            steps: Some(self.steps),
            cfg_scale: Some(self.cfg),
            seed: self.seed.parse().ok(),
            sampling_method: if self.sampler.is_empty() {
                None
            } else {
                Some(self.sampler.clone())
            },
            negative_prompt: if self.negative.is_empty() {
                None
            } else {
                Some(self.negative.clone())
            },
            threads: if self.expert_mode && self.threads > 0 {
                Some(self.threads)
            } else {
                None
            },
            styles: self.selected_styles.clone(),
            loras: self.selected_loras.clone(),
            lora_scale: Some(1.0),
            vae: if self.vae.is_empty() {
                None
            } else {
                Some(self.vae.clone())
            },
            offload_to_cpu: (self.offload_to_cpu || self.stream_layers).then_some(true),
            diffusion_fa: self.diffusion_fa.then_some(true),
            auto_fit: self.auto_fit.then_some(true),
            max_vram: if self.max_vram.trim().is_empty() {
                None
            } else {
                Some(self.max_vram.clone())
            },
            stream_layers: self.stream_layers.then_some(true),
            flow_shift: if self.expert_mode {
                parse_opt_f32(&self.flow_shift)
            } else {
                None
            },
            sd_mode: if self.expert_mode && !self.sd_mode.trim().is_empty() {
                Some(self.sd_mode.trim().to_string())
            } else {
                None
            },
            video_frames: if self.expert_mode {
                parse_opt_u32(&self.video_frames)
            } else {
                None
            },
            backend: if self.expert_mode && !self.backend.trim().is_empty() {
                Some(self.backend.trim().to_string())
            } else {
                None
            },
            params_backend: if self.expert_mode && !self.params_backend.trim().is_empty() {
                Some(self.params_backend.trim().to_string())
            } else {
                None
            },
            upscale_model: if self.upscale_enabled && !self.upscale_model.is_empty() {
                Some(self.upscale_model.clone())
            } else {
                None
            },
            upscale_repeats: if self.upscale_enabled {
                Some(self.upscale_repeats.clamp(1, 4))
            } else {
                None
            },
            upscale_tile_size: if self.upscale_enabled {
                Some(self.upscale_tile_size.clamp(32, 512))
            } else {
                None
            },
            init_image: if self.img2img_enabled {
                let path = if !self.img2img_path.trim().is_empty() {
                    self.img2img_path.trim().to_string()
                } else {
                    self.preview.clone().unwrap_or_default()
                };
                if path.is_empty() {
                    None
                } else {
                    Some(path)
                }
            } else {
                None
            },
            strength: if self.img2img_enabled {
                Some(self.img2img_strength.clamp(0.0, 1.0))
            } else {
                None
            },
            ..MediaImageOptions::default()
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        t: &UiStrings,
        cmd: &Sender<Cmd>,
        generating: Option<&ImageGenUiState>,
        download_busy: bool,
    ) {
        if self.catalog_packs.is_empty() {
            self.refresh_catalog();
        }
        self.ui_install_prompt(ui.ctx(), t, cmd, download_busy);
        ui.heading(t.tab_create);
        ui.label(t.tab_hint_image);
        ui.add_space(8.0);
        if let Some(gen) = generating {
            ui_image_progress(ui, t, gen);
            ui.add_space(6.0);
        }
        let avail = ui.available_width();
        let left_w = (avail * 0.48).clamp(280.0, 620.0);
        let right_w = (avail - left_w - 12.0).max(260.0);
        // horizontal_top inherits LTR into children — force top-down inside each column.
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(left_w, ui.available_height().max(400.0)),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("image_studio_form")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_min_width(left_w - 8.0);
                            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                self.ui_form_column(ui, t, cmd, generating);
                            });
                        });
                },
            );
            ui.add_space(10.0);
            ui.allocate_ui_with_layout(
                egui::vec2(right_w, ui.available_height().max(400.0)),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("image_studio_right")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_min_width(right_w - 8.0);
                            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                crate::image_composition::ui_composition_canvas(
                                    ui,
                                    t,
                                    self.width,
                                    self.height,
                                    &mut self.composition_blocks,
                                    &mut self.composition_selected,
                                    &mut self.composition_next_id,
                                    self.preview.as_deref(),
                                    &mut self.preview_overlay_opacity,
                                );
                                if let Some(path) = self.preview.clone() {
                                    let busy = generating.is_some();
                                    let can_upscale = !busy
                                        && !self.upscale_model.is_empty()
                                        && !self.upscalers.is_empty();
                                    let upscale_model = self.upscale_model.clone();
                                    let upscale_repeats = self.upscale_repeats;
                                    let upscale_tile_size = self.upscale_tile_size;
                                    let path_for_cmd = path.clone();
                                    ui_image_preview_actions(ui, t, &path, can_upscale, || {
                                        let _ = cmd.send(Cmd::MediaImageUpscale {
                                            source_path: path_for_cmd,
                                            upscale_model,
                                            upscale_repeats,
                                            upscale_tile_size,
                                        });
                                    });
                                }
                                ui_image_history(ui, t, self);
                            });
                        });
                },
            );
        });
    }

    fn ui_form_column(
        &mut self,
        ui: &mut egui::Ui,
        t: &UiStrings,
        cmd: &Sender<Cmd>,
        generating: Option<&ImageGenUiState>,
    ) {
        ui.horizontal(|ui| {
            ui.label("Prompt");
            help_icon(
                ui,
                "What to generate. Be specific about subject, style, and composition.",
            );
        });
        ui.add(
            egui::TextEdit::multiline(&mut self.prompt)
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        combo_image_pack(
            ui,
            "pack",
            t.settings_image_pack,
            &mut self.model_id,
            &self.packs,
            &self.catalog_packs,
            t.studio_not_installed_suffix,
            &mut self.install_prompt,
            Some(t.studio_image_pack_help),
        );
        if self.packs.is_empty() {
            ui.colored_label(egui::Color32::YELLOW, t.studio_no_models_installed);
        } else if !models_page::is_model_installed(&self.model_id) {
            ui.colored_label(egui::Color32::YELLOW, t.studio_model_not_installed);
        }
        ui.horizontal(|ui| {
            ui.label("W");
            help_icon(ui, "Image width in pixels.");
            ui.add(egui::DragValue::new(&mut self.width).range(64..=2048));
            ui.label("H");
            help_icon(ui, "Image height in pixels.");
            ui.add(egui::DragValue::new(&mut self.height).range(64..=2048));
            ui.label("Profile");
            help_icon(
                ui,
                "Preset quality/speed profile for this model (fast, balanced, quality).",
            );
            egui::ComboBox::from_id_salt("studio_profile")
                .selected_text(self.profile.as_str())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.profile, "fast".to_string(), "fast");
                    ui.selectable_value(&mut self.profile, "balanced".to_string(), "balanced");
                    ui.selectable_value(&mut self.profile, "quality".to_string(), "quality");
                });
        });
        self.apply_preset_for_current_model();

        egui::CollapsingHeader::new(t.studio_negative)
            .default_open(false)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.negative).desired_width(f32::INFINITY),
                );
            });

        egui::CollapsingHeader::new(t.studio_section_enrichment)
            .default_open(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.enhance_prompt_chat, t.studio_enhance_prompt_chat);
                    help_icon(ui, t.studio_enhance_prompt_chat_help);
                });
                if self.enhance_prompt_chat && self.use_edited_enriched {
                    self.use_edited_enriched = false;
                }
                if self.enhance_prompt_chat {
                    self.enrich_prompt = false;
                }
                if crate::image_prompt::supports_json_prompt_enrichment(Some(&self.model_id)) {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.enrich_prompt, t.studio_enrich_prompt);
                        help_icon(ui, t.studio_enrich_prompt_help);
                    });
                    if self.enrich_prompt && self.use_edited_enriched {
                        self.use_edited_enriched = false;
                    }
                    if self.enrich_prompt {
                        self.enhance_prompt_chat = false;
                    }
                }
                let show_enriched_panel = self.enhance_prompt_chat
                    || crate::image_prompt::supports_json_prompt_enrichment(Some(&self.model_id))
                    || !self.enriched_prompt.trim().is_empty();
                if show_enriched_panel {
                    ui.horizontal(|ui| {
                        ui.checkbox(
                            &mut self.use_edited_enriched,
                            t.studio_use_edited_enriched,
                        );
                        help_icon(ui, t.studio_use_edited_enriched_help);
                    })
                    .response
                    .on_hover_text(t.studio_use_edited_enriched_help);
                    if self.use_edited_enriched {
                        self.enrich_prompt = false;
                        self.enhance_prompt_chat = false;
                    }
                    let hint = if self.enrich_prompt {
                        enrichment_hint(t, &self.model_id)
                    } else {
                        t.studio_enriched_hint_prose
                    };
                    ui_enriched_prompt_panel(
                        ui,
                        t,
                        hint,
                        &mut self.enriched_prompt,
                        &mut self.show_enriched_prompt,
                    );
                }
            });

        egui::CollapsingHeader::new(t.studio_section_sampling)
            .default_open(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("steps");
                    help_icon(
                        ui,
                        "Denoising iterations. More steps can improve quality but are slower.",
                    );
                    ui.add(egui::DragValue::new(&mut self.steps).range(1..=150));
                    ui.label("CFG");
                    help_icon(
                        ui,
                        "Prompt guidance strength. Higher = closer to prompt, lower = more creative/flexible.",
                    );
                    ui.add(egui::DragValue::new(&mut self.cfg).range(0.0..=20.0).speed(0.1));
                });
                ui.horizontal(|ui| {
                    ui.label("seed");
                    help_icon(
                        ui,
                        "Random seed. Empty means random each run; fixed value makes outputs reproducible.",
                    );
                    ui.add(egui::TextEdit::singleline(&mut self.seed).desired_width(80.0));
                    ui.label("sampler");
                    help_icon(ui, "Sampling algorithm used by the diffusion engine.");
                    egui::ComboBox::from_id_salt("studio_sampler")
                        .selected_text(if self.sampler.is_empty() {
                            "default"
                        } else {
                            self.sampler.as_str()
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.sampler, String::new(), "default");
                            for s in ["euler", "euler_a", "heun", "dpm2", "lcm", "ddim"] {
                                ui.selectable_value(&mut self.sampler, s.to_string(), s);
                            }
                        });
                });
            });

        egui::CollapsingHeader::new(t.studio_section_styles)
            .default_open(false)
            .show(ui, |ui| {
                multi_select_assets(
                    ui,
                    "style",
                    "Styles",
                    &mut self.selected_styles,
                    &self.styles,
                    Some("Optional style presets (.txt in share/models/styles/) or custom text fragments, comma-joined as prompt prefix."),
                );
                multi_select_assets(
                    ui,
                    "lora",
                    "LoRA",
                    &mut self.selected_loras,
                    &self.loras,
                    Some("LoRA adapters in share/models/lora/. Applied via sd.cpp tags <lora:name:1>."),
                );
                combo_plain(
                    ui,
                    "vae",
                    "VAE",
                    &mut self.vae,
                    &self.vaes,
                    Some("Optional VAE override for decoding latents to image."),
                );
            });

        egui::CollapsingHeader::new(t.studio_img2img_heading)
            .default_open(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.img2img_enabled, t.studio_img2img_enable);
                    help_icon(ui, t.studio_img2img_enable_help);
                });
                ui.weak(t.studio_img2img_blurb);
                if self.img2img_enabled {
                    ui.horizontal(|ui| {
                        ui.label(t.studio_img2img_path);
                        help_icon(ui, t.studio_img2img_path_help);
                    });
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.img2img_path).desired_width(280.0),
                        );
                        if ui.button(t.studio_browse).clicked() {
                            let start = Path::new(self.img2img_path.trim())
                                .parent()
                                .filter(|p| p.is_dir())
                                .map(|p| p.to_path_buf())
                                .or_else(|| {
                                    self.preview.as_deref().and_then(|logical| {
                                        let host = aos_home()
                                            .join("var/storage/data")
                                            .join(logical.trim_start_matches('/'));
                                        host.parent()
                                            .filter(|p| p.is_dir())
                                            .map(|p| p.to_path_buf())
                                    })
                                })
                                .or_else(user_downloads_dir)
                                .unwrap_or_else(aos_home);
                            if let Some(path) = pick_os_file(
                                t.studio_img2img_browse_title,
                                &[
                                    ("Images", &["png", "jpg", "jpeg", "webp", "bmp"]),
                                    ("PNG", &["png"]),
                                    ("All files", &["*"]),
                                ],
                                Some(&start),
                            ) {
                                self.img2img_path = path.to_string_lossy().into_owned();
                            }
                        }
                        if let Some(prev) = self.preview.clone() {
                            if ui.button(t.studio_img2img_use_preview).clicked() {
                                self.img2img_path = prev;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(t.studio_img2img_strength);
                        help_icon(ui, t.studio_img2img_strength_help);
                        ui.add(
                            egui::Slider::new(&mut self.img2img_strength, 0.05..=1.0)
                                .fixed_decimals(2),
                        );
                    });
                }
            });

        egui::CollapsingHeader::new(t.studio_upscale_heading)
            .default_open(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.upscale_enabled, t.studio_upscale_enable);
                    help_icon(ui, t.studio_upscale_enable_help);
                });
                ui.weak(t.studio_upscale_blurb);
                if self.upscalers.is_empty() {
                    ui.colored_label(
                        egui::Color32::from_rgb(240, 190, 100),
                        t.studio_upscale_missing,
                    );
                } else {
                    combo_plain(
                        ui,
                        "upscale_model",
                        t.studio_upscale_model,
                        &mut self.upscale_model,
                        &self.upscalers,
                        Some(t.studio_upscale_model_help),
                    );
                    ui.horizontal(|ui| {
                        ui.label(t.studio_upscale_repeats);
                        help_icon(ui, t.studio_upscale_repeats_help);
                        ui.add(egui::DragValue::new(&mut self.upscale_repeats).range(1..=4));
                        ui.label(t.studio_upscale_tile);
                        help_icon(ui, t.studio_upscale_tile_help);
                        ui.add(egui::DragValue::new(&mut self.upscale_tile_size).range(32..=512));
                    });
                }
                ui.horizontal(|ui| {
                    if ui.button("Open upscale/").clicked() {
                        open_os_folder(&asset_subdir("upscale"));
                    }
                });
            });

        egui::CollapsingHeader::new(t.studio_section_import)
            .default_open(false)
            .show(ui, |ui| {
                ui.weak("Place files in share/models/lora/, vae/, or styles/ — or import below.");
                ui.horizontal(|ui| {
                    if ui.button("Open lora/").clicked() {
                        open_os_folder(&asset_subdir("lora"));
                    }
                    if ui.button("Open vae/").clicked() {
                        open_os_folder(&asset_subdir("vae"));
                    }
                    if ui.button("Open styles/").clicked() {
                        open_os_folder(&asset_subdir("style"));
                    }
                    if ui.button("Civitai (LoRA / styles)").clicked() {
                        open_url("https://civitai.com/models?types=LORA&sort=Most+Downloaded");
                    }
                    if ui.button("Hugging Face (models)").clicked() {
                        open_url("https://huggingface.co/models?pipeline_tag=text-to-image&sort=downloads");
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(t.studio_file_path);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.import_path).desired_width(360.0),
                    );
                    if ui.button(t.studio_browse).clicked() {
                        let start = Path::new(self.import_path.trim())
                            .parent()
                            .filter(|p| p.is_dir())
                            .map(|p| p.to_path_buf())
                            .or_else(user_downloads_dir)
                            .unwrap_or_else(|| asset_subdir("lora"));
                        if let Some(path) = pick_os_file(
                            t.studio_browse,
                            &[
                                ("Weights", &["safetensors", "ckpt", "pt", "bin"]),
                                ("Text preset", &["txt"]),
                                ("Safetensors", &["safetensors"]),
                                ("All files", &["*"]),
                            ],
                            Some(&start),
                        ) {
                            self.import_path = path.to_string_lossy().into_owned();
                            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                                let lower = name.to_ascii_lowercase();
                                if lower.contains("vae") {
                                    self.import_kind = "vae".to_string();
                                } else if lower.contains("lora") {
                                    self.import_kind = "lora".to_string();
                                } else if lower.ends_with(".txt") {
                                    self.import_kind = "style".to_string();
                                } else if lower.ends_with(".pth") {
                                    self.import_kind = "upscale".to_string();
                                }
                            }
                        }
                    }
                    egui::ComboBox::from_id_salt("studio_import_kind")
                        .selected_text(self.import_kind.as_str())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.import_kind, "lora".to_string(), "lora");
                            ui.selectable_value(&mut self.import_kind, "vae".to_string(), "vae");
                            ui.selectable_value(
                                &mut self.import_kind,
                                "style".to_string(),
                                "style (.txt)",
                            );
                            ui.selectable_value(
                                &mut self.import_kind,
                                "upscale".to_string(),
                                "upscale (.pth)",
                            );
                        });
                    if ui.button(t.studio_import_file).clicked() {
                        match self.import_asset_file() {
                            Ok(name) => {
                                self.refresh_catalog();
                                match self.import_kind.as_str() {
                                    "vae" => self.vae = name.clone(),
                                    "style" => push_unique(&mut self.selected_styles, name.clone()),
                                    _ => push_unique(&mut self.selected_loras, name.clone()),
                                }
                                let src_name = Path::new(self.import_path.trim())
                                    .file_name()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("");
                                self.import_status = if src_name == name.as_str() {
                                    format!("Imported {name}")
                                } else {
                                    format!("Imported as {name} (original name was in use)")
                                };
                            }
                            Err(e) => self.import_status = format!("Import failed: {e}"),
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Custom style");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.custom_style_input)
                            .desired_width(380.0),
                    );
                    if ui.button("Add style").clicked() {
                        match self.add_custom_style() {
                            Ok(style) => {
                                self.refresh_catalog();
                                push_unique(&mut self.selected_styles, style.clone());
                                self.import_status = format!("Style added: {style}");
                            }
                            Err(e) => self.import_status = format!("Style add failed: {e}"),
                        }
                    }
                });
                ui.collapsing("Manage custom assets", |ui| {
                    let reg = load_image_assets_registry();
                    let mut pending_remove: Option<(String, String)> = None;
                    custom_list_row(ui, "Styles", &reg.styles, "style", &mut pending_remove);
                    custom_list_row(ui, "LoRA", &reg.loras, "lora", &mut pending_remove);
                    custom_list_row(ui, "VAE", &reg.vaes, "vae", &mut pending_remove);
                    if let Some((kind, item)) = pending_remove {
                        match self.remove_custom_entry(&kind, &item) {
                            Ok(()) => {
                                self.import_status = format!("Removed custom {kind}: {item}");
                                self.refresh_catalog();
                            }
                            Err(e) => {
                                self.import_status = format!("Remove failed for {item}: {e}");
                            }
                        }
                    }
                    ui.weak("Removing unregisters custom text styles. LoRA/VAE files live in share/models/lora/ and vae/.");
                });
                if !self.import_status.is_empty() {
                    ui.weak(&self.import_status);
                }
            });

        egui::CollapsingHeader::new(t.studio_expert_heading)
            .default_open(false)
            .show(ui, |ui| {
                ui.weak(t.studio_expert_blurb);
                ui.horizontal(|ui| {
                    let expert_toggled = ui
                        .checkbox(&mut self.expert_mode, t.studio_expert_mode)
                        .changed();
                    help_icon(ui, t.studio_expert_mode_help);
                    if expert_toggled && self.expert_mode {
                        self.load_expert_defaults_from_catalog();
                    }
                    if self.expert_mode {
                        if ui.button(t.studio_expert_reset).clicked() {
                            self.load_expert_defaults_from_catalog();
                        }
                    }
                });
                if self.expert_mode {
                    ui.horizontal(|ui| {
                        ui.label(t.studio_expert_flow_shift);
                        help_icon(ui, t.studio_expert_flow_shift_help);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.flow_shift)
                                .desired_width(56.0)
                                .hint_text("3"),
                        );
                        ui.label(t.studio_expert_sd_mode);
                        help_icon(ui, t.studio_expert_sd_mode_help);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.sd_mode)
                                .desired_width(72.0)
                                .hint_text("img_gen"),
                        );
                        ui.label(t.studio_expert_video_frames);
                        help_icon(ui, t.studio_expert_video_frames_help);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.video_frames)
                                .desired_width(48.0)
                                .hint_text("1"),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label(t.studio_expert_backend);
                        help_icon(ui, t.studio_expert_backend_help);
                        coerce_backend_alias(&mut self.backend);
                        backend_choice_combo(
                            ui,
                            "studio_backend",
                            &mut self.backend,
                            STUDIO_BACKEND_CHOICES,
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label(t.studio_expert_params_backend);
                        help_icon(ui, t.studio_expert_params_backend_help);
                        coerce_params_backend_alias(&mut self.params_backend);
                        backend_choice_combo(
                            ui,
                            "studio_params_backend",
                            &mut self.params_backend,
                            STUDIO_PARAMS_BACKEND_CHOICES,
                        );
                        ui.label(t.studio_expert_threads);
                        help_icon(ui, t.studio_expert_threads_help);
                        ui.add(egui::DragValue::new(&mut self.threads).range(0..=64).speed(1));
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.offload_to_cpu, t.studio_expert_offload);
                        help_icon(ui, t.studio_expert_offload_help);
                        ui.checkbox(&mut self.diffusion_fa, t.studio_expert_diffusion_fa);
                        help_icon(ui, t.studio_expert_diffusion_fa_help);
                        ui.checkbox(&mut self.auto_fit, t.studio_expert_auto_fit);
                        help_icon(ui, t.studio_expert_auto_fit_help);
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.stream_layers, t.studio_expert_stream_layers);
                        help_icon(ui, t.studio_expert_stream_layers_help);
                        if self.stream_layers {
                            self.offload_to_cpu = true;
                        }
                        ui.label(t.studio_expert_max_vram);
                        help_icon(ui, t.studio_expert_max_vram_help);
                        egui::ComboBox::from_id_salt("studio_max_vram")
                            .selected_text(max_vram_label(&self.max_vram))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.max_vram, String::new(), "off");
                                ui.selectable_value(&mut self.max_vram, "-1".to_string(), "auto (−1)");
                                ui.selectable_value(&mut self.max_vram, "4".to_string(), "4 GiB");
                                ui.selectable_value(&mut self.max_vram, "6".to_string(), "6 GiB");
                                ui.selectable_value(&mut self.max_vram, "8".to_string(), "8 GiB");
                                ui.selectable_value(&mut self.max_vram, "12".to_string(), "12 GiB");
                                ui.selectable_value(
                                    &mut self.max_vram,
                                    "0".to_string(),
                                    "0 (disable cut)",
                                );
                            });
                    });
                }
            });

        ui.add_space(8.0);
        let busy = generating.is_some();
        let model_ready = models_page::is_model_installed(&self.model_id);
        if ui
            .add_enabled(!busy && model_ready, egui::Button::new(t.studio_generate))
            .clicked()
            && !self.prompt.is_empty()
            && model_ready
        {
            let use_edited = self.use_edited_enriched && !self.enriched_prompt.trim().is_empty();
            let wants_json_enrich = self.enrich_prompt
                && crate::image_prompt::supports_json_prompt_enrichment(Some(&self.model_id))
                && !use_edited;
            let wants_chat_enhance = self.enhance_prompt_chat && !use_edited && !wants_json_enrich;
            let _ = cmd.send(Cmd::MediaImage {
                prompt: self.prompt.clone(),
                model_id: if self.model_id.is_empty() {
                    None
                } else {
                    Some(self.model_id.clone())
                },
                options: self.to_options(),
                enrich_prompt: wants_json_enrich,
                enhance_prompt_chat: wants_chat_enhance,
                generation_prompt: if use_edited {
                    Some(self.enriched_prompt.trim().to_string())
                } else {
                    None
                },
                composition_blocks: self.composition_blocks.clone(),
            });
        }
    }
}

fn pretty_json(raw: &str) -> String {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or_else(|| raw.to_string())
}

fn format_enriched_display(raw: &str) -> String {
    let trimmed = raw.trim();
    if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
        pretty_json(trimmed)
    } else {
        trimmed.to_string()
    }
}

fn ui_enriched_prompt_panel(
    ui: &mut egui::Ui,
    t: &UiStrings,
    hint: &str,
    enriched_prompt: &mut String,
    show: &mut bool,
) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.checkbox(show, t.studio_show_enriched);
        if ui.button(t.studio_copy_enhanced).clicked() {
            ui.ctx().copy_text(enriched_prompt.clone());
        }
    });
    if !*show {
        return;
    }
    ui.weak(hint);
    ui.add(
        egui::TextEdit::multiline(enriched_prompt)
            .font(egui::TextStyle::Monospace)
            .desired_rows(8)
            .desired_width(f32::INFINITY),
    );
}

fn ui_image_preview_actions(
    ui: &mut egui::Ui,
    t: &UiStrings,
    path: &str,
    can_upscale: bool,
    on_upscale: impl FnOnce(),
) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.weak(path);
        if let Some(tex) = decl_ui::try_load_png(ui.ctx(), path) {
            let [tw, th] = tex.size();
            ui.weak(format!("{tw}×{th}"));
        }
    });
    ui.horizontal(|ui| {
        if ui.button(t.studio_open_file).clicked() {
            let _ = decl_ui::open_host_path(path);
        }
        if ui
            .add_enabled(can_upscale, egui::Button::new(t.studio_upscale_preview))
            .on_hover_text(t.studio_upscale_preview_help)
            .clicked()
        {
            on_upscale();
        }
    });
}

fn ui_image_history(ui: &mut egui::Ui, t: &UiStrings, studio: &mut ImageStudioState) {
    ui.separator();
    egui::CollapsingHeader::new(t.studio_history_heading)
        .default_open(false)
        .show(ui, |ui| {
            ui.weak(t.studio_history_hint);
            let entries = crate::image_history::list_image_history(40);
            if entries.is_empty() {
                ui.weak(t.studio_history_empty);
                return;
            }
            for meta in &entries {
                let selected = studio.preview.as_deref() == Some(meta.path.as_str());
                let label = meta.truncated_prompt(72);
                let has_comp = !meta.composition_blocks.is_empty();
                let has_enriched = meta
                    .generation_prompt
                    .as_deref()
                    .is_some_and(|s| !s.is_empty());
                let badges = match (has_enriched, has_comp) {
                    (true, true) => format!(" · {} · {}", t.studio_history_badge_enriched, t.studio_history_badge_composition),
                    (true, false) => format!(" · {}", t.studio_history_badge_enriched),
                    (false, true) => format!(" · {}", t.studio_history_badge_composition),
                    (false, false) => String::new(),
                };
                ui.horizontal(|ui| {
                    if let Some(tex) = decl_ui::try_load_png(ui.ctx(), &meta.path) {
                        ui.add(
                            egui::Image::new(&tex)
                                .fit_to_exact_size(egui::vec2(40.0, 40.0))
                                .maintain_aspect_ratio(true),
                        );
                    }
                    let text = format!("{label}{badges}");
                    if ui
                        .selectable_label(selected, text)
                        .on_hover_text(&meta.path)
                        .clicked()
                    {
                        studio.apply_history(meta);
                    }
                });
            }
        });
}

fn combo_image_pack(
    ui: &mut egui::Ui,
    salt: &str,
    label: &str,
    current: &mut String,
    installed: &[(String, String)],
    catalog: &[(String, String)],
    not_installed_suffix: &str,
    install_prompt: &mut Option<(String, String)>,
    help: Option<&str>,
) {
    ui.horizontal(|ui| {
        ui.label(label);
        if let Some(h) = help {
            help_icon(ui, h);
        }
        let selected = catalog
            .iter()
            .find(|(id, _)| id == current)
            .map(|(id, name)| {
                if models_page::is_model_installed(id) {
                    name.clone()
                } else {
                    format!("{name}{not_installed_suffix}")
                }
            })
            .unwrap_or_else(|| current.clone());
        egui::ComboBox::from_id_salt(salt)
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for (id, name) in installed {
                    ui.selectable_value(current, id.clone(), name);
                }
                let available: Vec<_> = catalog
                    .iter()
                    .filter(|(id, _)| !models_page::is_model_installed(id))
                    .collect();
                if !installed.is_empty() && !available.is_empty() {
                    ui.separator();
                }
                for (id, name) in available {
                    let row_label = format!("{name}{not_installed_suffix}");
                    if ui.button(row_label).clicked() {
                        *install_prompt = Some((id.clone(), name.clone()));
                    }
                }
            });
    });
}

fn multi_select_assets(
    ui: &mut egui::Ui,
    salt: &str,
    label: &str,
    selected: &mut Vec<String>,
    items: &[String],
    help: Option<&str>,
) {
    ui.horizontal(|ui| {
        ui.label(label);
        if let Some(h) = help {
            help_icon(ui, h);
        }
    });
    let summary = if selected.is_empty() {
        "— none —".to_string()
    } else if selected.len() <= 2 {
        selected.join(", ")
    } else {
        format!("{} selected", selected.len())
    };
    egui::CollapsingHeader::new(summary)
        .id_salt(salt)
        .default_open(!selected.is_empty())
        .show(ui, |ui| {
            if items.is_empty() {
                ui.weak("No assets found — import or drop files in the folder.");
            }
            for id in items {
                let mut checked = selected.iter().any(|x| x == id);
                if ui.checkbox(&mut checked, id).changed() {
                    if checked {
                        push_unique(selected, id.clone());
                    } else {
                        selected.retain(|x| x != id);
                    }
                }
            }
            if !selected.is_empty() && ui.small_button("Clear all").clicked() {
                selected.clear();
            }
        });
}

fn catalog_engine_args(model_id: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    if model_id.is_empty() {
        return None;
    }
    let path = aos_home().join("share/models/catalog-offerings.json");
    let raw = std::fs::read_to_string(path).ok()?;
    let v = serde_json::from_str::<serde_json::Value>(&raw).ok()?;
    let models = v.get("models")?.as_array()?;
    let m = models
        .iter()
        .find(|x| x.get("id").and_then(|i| i.as_str()) == Some(model_id))?;
    m.get("engine_args")?.as_object().cloned()
}

fn json_arg_as_string(v: Option<&serde_json::Value>) -> String {
    match v {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(serde_json::Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

fn parse_opt_f32(raw: &str) -> Option<f32> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    s.parse().ok()
}

fn parse_opt_u32(raw: &str) -> Option<u32> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    s.parse().ok()
}

fn max_vram_label(value: &str) -> &'static str {
    match value.trim() {
        "" => "off",
        "-1" => "auto (−1)",
        "0" => "0 (disable cut)",
        "4" => "4 GiB",
        "6" => "6 GiB",
        "8" => "8 GiB",
        "12" => "12 GiB",
        _ => "custom",
    }
}

/// Closed `--backend` choices (value → label). Empty = leave catalog/model defaults.
const STUDIO_BACKEND_CHOICES: &[(&str, &str)] = &[
    ("", "catalog"),
    ("cpu", "cpu"),
    ("gpu", "gpu"),
    ("cuda0", "cuda0"),
    (
        "te=cpu,llm=cpu,diffusion=gpu,vae=cpu",
        "mixed DiT+LLM",
    ),
    ("te=cpu,diffusion=gpu,vae=cpu", "mixed TE+diff"),
];

/// Closed `--params-backend` choices.
const STUDIO_PARAMS_BACKEND_CHOICES: &[(&str, &str)] = &[
    ("", "catalog"),
    ("cpu", "cpu"),
    ("cuda0", "cuda0"),
    ("disk", "disk"),
];

fn coerce_backend_alias(value: &mut String) {
    let lower = value.trim().to_ascii_lowercase();
    if lower == "mixte" || lower == "mixed" {
        *value = "te=cpu,llm=cpu,diffusion=gpu,vae=cpu".into();
    }
}

fn coerce_params_backend_alias(value: &mut String) {
    let lower = value.trim().to_ascii_lowercase();
    if lower == "mixte" || lower == "mixed" {
        *value = "cpu".into();
    }
}

fn backend_choice_label<'a>(value: &'a str, choices: &[(&'a str, &'a str)]) -> &'a str {
    choices
        .iter()
        .find(|(v, _)| *v == value)
        .map(|(_, label)| *label)
        .unwrap_or(if value.is_empty() { "catalog" } else { value })
}

fn backend_choice_combo(
    ui: &mut egui::Ui,
    salt: &str,
    current: &mut String,
    choices: &[(&str, &str)],
) {
    let orphan = if !choices.iter().any(|(v, _)| *v == current.as_str()) && !current.is_empty() {
        Some(current.clone())
    } else {
        None
    };
    egui::ComboBox::from_id_salt(salt)
        .selected_text(backend_choice_label(current, choices))
        .width(220.0)
        .show_ui(ui, |ui| {
            for (value, label) in choices {
                ui.selectable_value(current, (*value).to_string(), *label);
            }
            if let Some(extra) = orphan {
                ui.selectable_value(current, extra.clone(), extra.as_str());
            }
        });
}

fn combo_plain(
    ui: &mut egui::Ui,
    salt: &str,
    label: &str,
    current: &mut String,
    items: &[String],
    help: Option<&str>,
) {
    ui.horizontal(|ui| {
        ui.label(label);
        if let Some(h) = help {
            help_icon(ui, h);
        }
        let shown = if current.is_empty() {
            "—"
        } else {
            current.as_str()
        };
        egui::ComboBox::from_id_salt(salt)
            .selected_text(shown)
            .show_ui(ui, |ui| {
                ui.selectable_value(current, String::new(), "—");
                for id in items {
                    ui.selectable_value(current, id.clone(), id);
                }
            });
    });
}

fn help_icon(ui: &mut egui::Ui, text: &str) {
    ui.small_button("?").on_hover_text(text);
}

fn copy_import_file(src: &Path, models_dir: &Path) -> Result<String, String> {
    std::fs::create_dir_all(models_dir).map_err(|e| e.to_string())?;
    let basename = src
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "invalid filename".to_string())?;
    if path_is_under_dir(src, models_dir) {
        return Ok(basename.to_string());
    }
    let data = read_file_with_retry(src, 5)?;
    let dst = models_dir.join(basename);
    let final_path = pick_import_destination(&dst);
    let tmp = models_dir.join(format!(
        ".import-{}-{}.part",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    std::fs::write(&tmp, &data).map_err(|e| format!("write failed: {e}"))?;
    if let Err(e) = std::fs::rename(&tmp, &final_path) {
        let _ = std::fs::remove_file(&final_path);
        std::fs::rename(&tmp, &final_path).map_err(|e2| {
            let _ = std::fs::remove_file(&tmp);
            format!("install failed ({e}; retry {e2})")
        })?;
    }
    final_path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "invalid destination filename".into())
}

fn path_is_under_dir(path: &Path, dir: &Path) -> bool {
    let Ok(path) = path.canonicalize() else {
        return path.starts_with(dir);
    };
    let Ok(dir) = dir.canonicalize() else {
        return path.starts_with(dir);
    };
    path.starts_with(dir)
}

fn pick_import_destination(preferred: &Path) -> PathBuf {
    if !preferred.exists() {
        return preferred.to_path_buf();
    }
    if std::fs::remove_file(preferred).is_ok() {
        return preferred.to_path_buf();
    }
    preferred
        .parent()
        .map(|dir| unique_name_in_dir(dir, preferred))
        .unwrap_or_else(|| preferred.to_path_buf())
}

fn unique_name_in_dir(dir: &Path, path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("asset");
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    for n in 1..=999 {
        let candidate = dir.join(format!("{stem}-{n}{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}-{}.safetensors", std::process::id()))
}

fn read_file_with_retry(path: &Path, attempts: u32) -> Result<Vec<u8>, String> {
    let mut last = String::new();
    for i in 0..attempts {
        match std::fs::read(path) {
            Ok(bytes) => return Ok(bytes),
            Err(e) => {
                last = e.to_string();
                if i + 1 < attempts {
                    std::thread::sleep(Duration::from_millis(200 * (u64::from(i) + 1)));
                }
            }
        }
    }
    Err(format!(
        "cannot read source file (close apps using it, then retry): {last}"
    ))
}

fn ui_image_progress(ui: &mut egui::Ui, t: &UiStrings, gen: &ImageGenUiState) {
    let (frac, label) = if gen.enriching {
        let pulse = ((gen.elapsed_secs % 4) as f32 / 4.0).max(0.05);
        (
            pulse,
            format!("{} ({}s)", t.studio_enriching_progress, gen.elapsed_secs),
        )
    } else if gen.upscaling {
        let pulse = ((gen.elapsed_secs % 3) as f32 / 3.0).max(0.05);
        (
            pulse,
            format!("Upscaling image (ESRGAN)… ({}s)", gen.elapsed_secs),
        )
    } else if gen.step > 0 && gen.total_steps > 0 {
        (
            (gen.step as f32 / gen.total_steps as f32).clamp(0.02, 1.0),
            format!(
                "Generating image: step {}/{} ({}s)",
                gen.step, gen.total_steps, gen.elapsed_secs
            ),
        )
    } else {
        let est = gen.total_steps.max(1) as f32 * 2.5;
        (
            (gen.elapsed_secs as f32 / est).clamp(0.02, 0.92),
            format!(
                "Generating image: {} steps, {}s elapsed…",
                gen.total_steps, gen.elapsed_secs
            ),
        )
    };
    ui.add(
        egui::ProgressBar::new(frac)
            .text(label)
            .animate(gen.enriching || gen.upscaling || gen.step == 0),
    );
}

fn custom_list_row(
    ui: &mut egui::Ui,
    title: &str,
    items: &[String],
    kind: &str,
    pending_remove: &mut Option<(String, String)>,
) {
    ui.group(|ui| {
        ui.label(title);
        if items.is_empty() {
            ui.weak("none");
            return;
        }
        for item in items {
            ui.horizontal(|ui| {
                ui.monospace(item);
                if ui.small_button("Remove").clicked() {
                    *pending_remove = Some((kind.to_string(), item.clone()));
                }
            });
        }
    });
}
