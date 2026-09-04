//! Models tab — catalog cards grouped by modality.

use crate::cmd::Cmd;
use crate::os_open::{aos_home, open_url};
use aos_proto::ModelInfo;
use eframe::egui;
use serde::Deserialize;
use std::path::Path;
use std::sync::mpsc::Sender;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCatalogTab {
    Llm,
    Image,
    Audio,
    Installed,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CatalogModel {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub profiles: Vec<String>,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub modality: Option<String>,
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub quality_score: u32,
    #[serde(default)]
    pub speed_score: u32,
    #[serde(default)]
    pub min_vram_mib: u64,
    #[serde(default)]
    pub n_params: f64,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct OfferingsRoot {
    #[serde(default)]
    models: Vec<CatalogModel>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct CustomOfferingsRoot {
    #[serde(default)]
    models: Vec<CatalogModel>,
}

#[derive(Debug, Clone)]
pub struct InstalledRow {
    pub id: String,
    pub name: String,
    pub filename: String,
    pub profiles: Vec<String>,
    pub bytes: u64,
    pub runtime: Option<ModelInfo>,
}

/// User-facing residence state; never expose internal enum/debug names in UI.
pub fn model_state_human(state: &aos_proto::ModelState, french: bool) -> &'static str {
    match (state, french) {
        (aos_proto::ModelState::OnDisk, true) => "Disponible",
        (aos_proto::ModelState::OnDisk, false) => "Available",
        (aos_proto::ModelState::Loading, true) => "Chargement",
        (aos_proto::ModelState::Loading, false) => "Loading",
        (aos_proto::ModelState::Loaded, true) => "Chargé",
        (aos_proto::ModelState::Loaded, false) => "Loaded",
        (aos_proto::ModelState::PartiallyOffloaded, true) => "Partiellement déchargé",
        (aos_proto::ModelState::PartiallyOffloaded, false) => "Partially offloaded",
        (aos_proto::ModelState::Error, true) => "Erreur",
        (aos_proto::ModelState::Error, false) => "Error",
        (aos_proto::ModelState::Remote, true) => "Distant",
        (aos_proto::ModelState::Remote, false) => "Remote",
    }
}

#[derive(Debug, Clone, Deserialize)]
struct InstalledRegistry {
    #[serde(default)]
    models: Vec<InstalledRegistryEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct InstalledRegistryEntry {
    id: String,
    filename: String,
    #[serde(default)]
    profiles: Vec<String>,
    #[serde(default)]
    bytes: u64,
}

pub fn load_installed_rows(runtime: &[ModelInfo]) -> Vec<InstalledRow> {
    let catalog = load_catalog_models();
    let path = aos_home().join("var/models/installed.json");
    let Ok(raw) = std::fs::read_to_string(path) else {
        return runtime
            .iter()
            .map(|m| InstalledRow {
                id: m.id.clone(),
                name: m.name.clone(),
                filename: String::new(),
                profiles: Vec::new(),
                bytes: 0,
                runtime: Some(m.clone()),
            })
            .collect();
    };
    let Ok(reg) = serde_json::from_str::<InstalledRegistry>(&raw) else {
        return Vec::new();
    };
    reg.models
        .into_iter()
        .map(|m| {
            let name = catalog
                .iter()
                .find(|c| c.id == m.id)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| m.id.clone());
            let runtime = runtime.iter().find(|r| r.id == m.id).cloned();
            InstalledRow {
                id: m.id,
                name,
                filename: m.filename,
                profiles: m.profiles,
                bytes: m.bytes,
                runtime,
            }
        })
        .collect()
}

/// First catalog offering tagged with the `vision` profile (catalog order).
pub fn first_catalog_vision_model_id() -> Option<String> {
    load_catalog_models()
        .into_iter()
        .find(|m| m.profiles.iter().any(|p| p == "vision"))
        .map(|m| m.id)
}

pub fn load_catalog_models() -> Vec<CatalogModel> {
    load_catalog_models_from(&aos_home())
}

fn load_catalog_models_from(home: &Path) -> Vec<CatalogModel> {
    let mut out = Vec::new();
    let catalog = home.join("share/models/catalog-offerings.json");
    if let Ok(raw) = std::fs::read_to_string(catalog) {
        if let Ok(v) = serde_json::from_str::<OfferingsRoot>(&raw) {
            out.extend(v.models);
        }
    }
    let custom = home.join("var/models/custom-offerings.json");
    if let Ok(raw) = std::fs::read_to_string(custom) {
        if let Ok(v) = serde_json::from_str::<CustomOfferingsRoot>(&raw) {
            for m in v.models {
                if !out.iter().any(|x| x.id == m.id) {
                    out.push(m);
                }
            }
        }
    }
    out
}

/// True when the offering is registered in `installed.json` or its main weight file exists on disk.
pub fn is_model_installed(model_id: &str) -> bool {
    let path = aos_home().join("var/models/installed.json");
    if let Ok(raw) = std::fs::read_to_string(path) {
        if let Ok(reg) = serde_json::from_str::<InstalledRegistry>(&raw) {
            if reg.models.iter().any(|m| m.id == model_id) {
                return true;
            }
        }
    }
    let catalog = load_catalog_models();
    let Some(m) = catalog.iter().find(|c| c.id == model_id) else {
        return false;
    };
    if m.filename.is_empty() {
        return false;
    }
    aos_home()
        .join("share/models")
        .join(&m.filename)
        .is_file()
}

pub fn category_of(m: &CatalogModel) -> ModelCatalogTab {
    if m.profiles.iter().any(|p| p == "image") || m.modality.as_deref() == Some("image") {
        return ModelCatalogTab::Image;
    }
    if m.profiles.iter().any(|p| p == "tts") || m.modality.as_deref() == Some("audio") {
        return ModelCatalogTab::Audio;
    }
    ModelCatalogTab::Llm
}

pub fn catalog_has_vision(m: &CatalogModel) -> bool {
    m.profiles.iter().any(|p| p == "vision")
}

/// Picker surface only: vision hint in the session model combo (no cpu/gpu/quant chips).
pub fn picker_surface_badges(m: &CatalogModel, t: &crate::i18n::UiStrings) -> Vec<&'static str> {
    if catalog_has_vision(m) {
        vec![t.models_sees_images]
    } else {
        vec![]
    }
}

pub fn model_badges(m: &CatalogModel) -> Vec<String> {
    let mut tags: Vec<String> = m.tags.clone();
    let hay = format!(
        "{} {} {} {}",
        m.id.to_ascii_lowercase(),
        m.name.to_ascii_lowercase(),
        m.profiles.join(" "),
        m.description.clone().unwrap_or_default().to_ascii_lowercase(),
    );
    fn has_tag(tags: &[String], label: &str) -> bool {
        tags.iter().any(|x| x.eq_ignore_ascii_case(label))
    }
    if m.profiles.iter().any(|p| p == "embed") && !has_tag(&tags, "embedding") {
        tags.push("embedding".into());
    }
    if m.profiles.iter().any(|p| p == "reasoning") && !has_tag(&tags, "reasoning") {
        tags.push("reasoning".into());
    }
    if m.profiles.iter().any(|p| p == "code") && !has_tag(&tags, "code") {
        tags.push("code".into());
    }
    if (hay.contains("moe")
        || hay.contains("-a3b")
        || hay.contains("mixtral")
        || hay.contains("8x"))
        && !has_tag(&tags, "moe")
    {
        tags.push("moe".into());
    }
    if (hay.contains("vision")
        || hay.contains("vl-")
        || hay.contains(" multimodal")
        || hay.contains("llava")
        || hay.contains("ideogram"))
        && !has_tag(&tags, "multimodal")
    {
        tags.push("multimodal".into());
    }
    if m.format == "safetensors" && !has_tag(&tags, "image") && !has_tag(&tags, "diffusion") {
        tags.push("diffusion".into());
    }
    if m.format == "onnx" && !has_tag(&tags, "tts") {
        tags.push("tts".into());
    }
    if m.optional && !has_tag(&tags, "optional") {
        tags.push("optional".into());
    }
    if m.n_params >= 2.0e10 && !has_tag(&tags, "large") {
        tags.push("large".into());
    }
    tags
}

fn ui_capability_badge(ui: &mut egui::Ui, label: &str) {
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(40, 48, 64))
        .corner_radius(crate::theme::CARD_RADIUS)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).size(11.0).color(egui::Color32::from_rgb(200, 210, 225)));
        });
}

/// Polished vision chip for catalog cards (localized « sees images » / « voit les images »).
pub fn ui_catalog_vision_chip(ui: &mut egui::Ui, label: &str) {
    use crate::theme::SIGNAL;
    egui::Frame::new()
        .fill(egui::Color32::from_rgba_unmultiplied(SIGNAL.r(), SIGNAL.g(), SIGNAL.b(), 36))
        .stroke(egui::Stroke::new(1.0, SIGNAL))
        .corner_radius(crate::theme::CARD_RADIUS)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).size(11.0).color(SIGNAL));
        });
}

fn hf_repo_url(m: &CatalogModel) -> Option<String> {
    if m.url.contains("huggingface.co/") {
        let rest = m.url.split("huggingface.co/").nth(1)?;
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.len() >= 2 {
            return Some(format!("https://huggingface.co/{}/{}", parts[0], parts[1]));
        }
    }
    None
}

fn format_params(n: f64) -> String {
    if n >= 1.0e9 {
        format!("{:.1}B", n / 1.0e9)
    } else if n >= 1.0e6 {
        format!("{:.0}M", n / 1.0e6)
    } else if n > 0.0 {
        format!("{:.0}", n)
    } else {
        "—".into()
    }
}

pub fn ui_catalog_tab_bar(ui: &mut egui::Ui, tab: &mut ModelCatalogTab, t: &crate::i18n::UiStrings) {
    ui.horizontal(|ui| {
        ui.selectable_value(tab, ModelCatalogTab::Llm, t.models_tab_llm);
        ui.selectable_value(tab, ModelCatalogTab::Image, t.models_tab_image);
        ui.selectable_value(tab, ModelCatalogTab::Audio, t.models_tab_audio);
        ui.selectable_value(tab, ModelCatalogTab::Installed, t.models_tab_installed);
    });
}

pub fn ui_hf_import(
    ui: &mut egui::Ui,
    hf_url: &mut String,
    hf_name: &mut String,
    hf_status: &mut String,
    busy: bool,
    cmd: &Sender<Cmd>,
    t: &crate::i18n::UiStrings,
) {
    ui.collapsing(t.models_hf_import, |ui| {
        ui.weak(t.models_hf_import_hint);
        ui.horizontal(|ui| {
            ui.label("URL");
            ui.add(
                egui::TextEdit::singleline(hf_url)
                    .hint_text("https://huggingface.co/org/repo/resolve/main/model.gguf")
                    .desired_width(420.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label(t.models_hf_name);
            ui.add(
                egui::TextEdit::singleline(hf_name)
                    .hint_text("optional")
                    .desired_width(280.0),
            );
            if ui
                .add_enabled(!busy && !hf_url.trim().is_empty(), egui::Button::new(t.models_hf_download))
                .clicked()
            {
                let name = hf_name.trim();
                let _ = cmd.send(Cmd::ModelDownloadHf {
                    url: hf_url.trim().to_string(),
                    name: if name.is_empty() {
                        None
                    } else {
                        Some(name.to_string())
                    },
                });
                *hf_status = t.models_hf_downloading.to_string();
            }
            if ui.button("huggingface.co").clicked() {
                open_url("https://huggingface.co/models");
            }
        });
        if !hf_status.is_empty() {
            ui.weak(hf_status);
        }
    });
}

#[allow(clippy::too_many_arguments)] // Immediate-mode card callbacks are intentionally explicit.
pub fn ui_model_card(
    ui: &mut egui::Ui,
    m: &CatalogModel,
    installed: bool,
    busy: bool,
    t: &crate::i18n::UiStrings,
    on_download: &mut impl FnMut(),
    on_redownload: &mut impl FnMut(),
    on_remove: &mut impl FnMut(),
    on_hf: &mut impl FnMut(&str),
) {
    let badges = model_badges(m);
    let show_vision = catalog_has_vision(m);
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.strong(&m.name);
                        if installed {
                            ui.colored_label(
                                egui::Color32::from_rgb(120, 200, 120),
                                t.models_installed_badge,
                            );
                        }
                    });
                    ui.weak(&m.id);
                    if let Some(desc) = &m.description {
                        ui.label(desc);
                    }
                    ui.horizontal_wrapped(|ui| {
                        for b in &badges {
                            ui_capability_badge(ui, b);
                        }
                        if show_vision {
                            ui_catalog_vision_chip(ui, t.models_sees_images);
                        }
                        if !m.format.is_empty() {
                            ui.weak(format!("[{}]", m.format));
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.weak(format!(
                            "{} · Q{} / S{} · VRAM ≥ {} MiB · {}",
                            human_gib(m.bytes),
                            m.quality_score,
                            m.speed_score,
                            m.min_vram_mib,
                            format_params(m.n_params),
                        ));
                    });
                });
                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                    if let Some(repo) = hf_repo_url(m) {
                        if ui.small_button("HF").clicked() {
                            on_hf(&repo);
                        }
                    }
                    if installed {
                        if ui
                            .add_enabled(!busy, egui::Button::new(t.models_redownload))
                            .clicked()
                        {
                            on_redownload();
                        }
                        if ui
                            .add_enabled(!busy, egui::Button::new(t.models_remove))
                            .clicked()
                        {
                            on_remove();
                        }
                    } else if ui
                        .add_enabled(!busy, egui::Button::new(t.models_download))
                        .clicked()
                    {
                        on_download();
                    }
                });
            });
        });
}

#[allow(clippy::too_many_arguments)] // Immediate-mode card callbacks are intentionally explicit.
pub fn ui_installed_card(
    ui: &mut egui::Ui,
    m: &InstalledRow,
    busy: bool,
    t: &crate::i18n::UiStrings,
    on_load: &mut impl FnMut(),
    on_default: &mut impl FnMut(),
    on_redownload: &mut impl FnMut(),
    on_remove: &mut impl FnMut(),
) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.strong(&m.name);
                    ui.weak(&m.id);
                    if !m.filename.is_empty() {
                        ui.weak(&m.filename);
                    }
                    if !m.profiles.is_empty() {
                        ui.weak(m.profiles.join(" · "));
                    }
                    if let Some(rt) = &m.runtime {
                        ui.weak(format!(
                            "{} · profile {}",
                            model_state_human(&rt.state, t.models_tab_installed == "Installés"),
                            rt.profile.as_deref().unwrap_or("—")
                        ));
                    }
                    if m.bytes > 0 {
                        ui.weak(human_gib(m.bytes));
                    }
                });
                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                    if m.runtime.is_some() {
                        if ui.button(t.models_load).clicked() {
                            on_load();
                        }
                        if ui.button(t.models_set_default).clicked() {
                            on_default();
                        }
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new(t.models_redownload))
                        .clicked()
                    {
                        on_redownload();
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new(t.models_remove))
                        .clicked()
                    {
                        on_remove();
                    }
                });
            });
        });
}

fn human_gib(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / (1 << 30) as f64)
}

#[cfg(test)]
mod vision_catalog_tests {
    use super::load_catalog_models_from;
    use std::path::PathBuf;

    #[test]
    fn first_vision_model_follows_catalog_order() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let vision_ids: Vec<String> = load_catalog_models_from(&root)
            .into_iter()
            .filter(|model| model.profiles.iter().any(|profile| profile == "vision"))
            .map(|model| model.id)
            .collect();
        assert_eq!(vision_ids.first().map(|s| s.as_str()), Some("local:gemma-4-e4b"));
        for id in ["local:qwen3-vl-4b", "local:qwen3-vl-8b", "local:llava-1.6"] {
            assert!(
                vision_ids.iter().any(|v| v == id),
                "missing vision model {id}"
            );
        }
    }
}
