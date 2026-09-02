//! Catalogue d'offres GGUF, recommandation hardware, installé, téléchargement.

use crate::bootstrap;
use crate::engines;
use crate::hardware::HardwareInfo;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfferingsFile {
    pub version: String,
    pub models: Vec<ModelOffering>,
    pub packs: BTreeMap<String, TierPack>,
    /// Runtime zips (sd.cpp / piper) fetched with the matching media pack.
    #[serde(default)]
    pub engines: BTreeMap<String, EngineOffering>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelOffering {
    pub id: String,
    pub name: String,
    pub profiles: Vec<String>,
    pub filename: String,
    pub url: String,
    pub bytes: u64,
    #[serde(default)]
    pub sha256: String,
    pub min_vram_mib: u64,
    pub min_disk_bytes: u64,
    pub quality_score: u32,
    pub speed_score: u32,
    pub n_layers: u32,
    pub n_params: f64,
    pub weights_bytes: u64,
    pub embed_bytes: u64,
    pub kv_bytes_per_token: u64,
    #[serde(default)]
    pub replaces: Vec<String>,
    #[serde(default)]
    pub optional: bool,
    /// `text` | `embedding` | `image` | `audio` (E16).
    #[serde(default)]
    pub modality: Option<String>,
    /// Weight format (`gguf`, `safetensors`, `onnx`).
    #[serde(default = "default_format")]
    pub format: String,
    /// Sidecar files (Piper `.onnx.json`, VAE/CLIP/T5/LoRA).
    #[serde(default)]
    pub extra_files: Vec<OfferingFile>,
    /// Catalogue engine id (`sdcpp`, `piper`). Inferred from profiles if empty.
    #[serde(default)]
    pub engine: Option<String>,
    /// Closed engine flags owned by the offering (never user-typed paths).
    #[serde(default)]
    pub engine_args: std::collections::BTreeMap<String, String>,
    /// Optional UI tags (`moe`, `multimodal`, `vision`, …).
    #[serde(default)]
    pub tags: Vec<String>,
    /// Short catalog blurb for the Models page.
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EngineOffering {
    #[serde(default)]
    pub windows: Option<EngineArtifact>,
    #[serde(default)]
    pub linux: Option<EngineArtifact>,
}

impl EngineOffering {
    pub fn current_os(&self) -> Option<&EngineArtifact> {
        if cfg!(windows) {
            self.windows.as_ref()
        } else if cfg!(target_os = "linux") {
            self.linux.as_ref()
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineArtifact {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub sha256: String,
}

fn default_format() -> String {
    "gguf".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OfferingFile {
    pub filename: String,
    pub url: String,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub sha256: String,
    /// `vae` | `clip_l` | `clip_g` | `t5xxl` | `lora` | `style` | `uncond` | `llm` | `mmproj`
    #[serde(default)]
    pub role: Option<String>,
}

impl ModelOffering {
    pub fn is_media(&self) -> bool {
        self.profiles.iter().any(|p| p == "image" || p == "tts")
            || matches!(self.modality.as_deref(), Some("image") | Some("audio"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierPack {
    pub embed: String,
    pub chat: String,
    #[serde(default)]
    pub alternatives: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledModel {
    pub id: String,
    pub filename: String,
    pub profiles: Vec<String>,
    pub bytes: u64,
    #[serde(default)]
    pub user_pinned: bool,
    pub installed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstalledFile {
    pub version: String,
    pub default_chat: Option<String>,
    pub default_embed: Option<String>,
    pub models: Vec<InstalledModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSetupOffer {
    pub hardware: HardwareInfo,
    pub recommended_ids: Vec<String>,
    pub alternative_chat_ids: Vec<String>,
    pub optional_ids: Vec<String>,
    pub models: Vec<ModelOffering>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSetupChoice {
    pub selected_ids: Vec<String>,
    pub default_chat: String,
    pub default_embed: String,
    #[serde(default)]
    pub include_optional: bool,
}

/// Remote model ids use `provider:` / `remote:` prefixes (no local GGUF download).
pub fn is_remote_model_id(id: &str) -> bool {
    id.starts_with("provider:") || id.starts_with("remote:")
}

pub fn validate_setup_choice(choice: &ModelSetupChoice) -> Result<(), String> {
    if choice.default_chat.trim().is_empty() {
        return Err("un modèle chat est requis".into());
    }
    if choice.default_embed.trim().is_empty() {
        return Err("un modèle embedding est requis".into());
    }
    Ok(())
}

pub fn setup_defaults_complete(inst: &InstalledFile) -> bool {
    inst.default_chat
        .as_ref()
        .is_some_and(|s| !s.is_empty())
        && inst.default_embed
            .as_ref()
            .is_some_and(|s| !s.is_empty())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUpdateOffer {
    pub available: Vec<ModelOffering>,
    pub reason: String,
}

pub fn offerings_path(home: &Path) -> PathBuf {
    home.join("share/models/catalog-offerings.json")
}

pub fn installed_path(home: &Path) -> PathBuf {
    home.join("var/models/installed.json")
}

pub fn load_offerings(home: &Path) -> Result<OfferingsFile, String> {
    let p = offerings_path(home);
    let p = if p.exists() {
        p
    } else {
        PathBuf::from("share/models/catalog-offerings.json")
    };
    let raw = fs::read_to_string(&p).map_err(|e| format!("offerings: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("offerings parse: {e}"))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CustomOfferingsFile {
    #[serde(default)]
    models: Vec<ModelOffering>,
}

pub fn custom_offerings_path(home: &Path) -> PathBuf {
    home.join("var/models/custom-offerings.json")
}

fn load_custom_offerings(home: &Path) -> Vec<ModelOffering> {
    let p = custom_offerings_path(home);
    let Ok(raw) = fs::read_to_string(p) else {
        return Vec::new();
    };
    serde_json::from_str::<CustomOfferingsFile>(&raw)
        .map(|f| f.models)
        .unwrap_or_default()
}

fn save_custom_offerings(home: &Path, models: &[ModelOffering]) -> Result<(), String> {
    let dir = home.join("var/models");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = CustomOfferingsFile {
        models: models.to_vec(),
    };
    let raw = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    fs::write(custom_offerings_path(home), raw).map_err(|e| e.to_string())
}

pub fn load_merged_offerings(home: &Path) -> Result<OfferingsFile, String> {
    let mut base = load_offerings(home)?;
    for m in load_custom_offerings(home) {
        if !base.models.iter().any(|x| x.id == m.id) {
            base.models.push(m);
        }
    }
    Ok(base)
}

pub fn load_installed(home: &Path) -> InstalledFile {
    let p = installed_path(home);
    fs::read_to_string(p)
        .ok()
        .and_then(|r| serde_json::from_str(&r).ok())
        .unwrap_or_default()
}

pub fn save_installed(home: &Path, file: &InstalledFile) -> Result<(), String> {
    let dir = home.join("var/models");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let raw = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    fs::write(installed_path(home), raw).map_err(|e| e.to_string())
}

pub fn find_offering<'a>(offerings: &'a OfferingsFile, id: &str) -> Option<&'a ModelOffering> {
    offerings.models.iter().find(|m| m.id == id)
}

pub fn recommend_pack(offerings: &OfferingsFile, hw: &HardwareInfo) -> (Vec<String>, Vec<String>) {
    let key = hw.tier.as_str();
    let pack = offerings
        .packs
        .get(key)
        .or_else(|| offerings.packs.get("mid"))
        .or_else(|| offerings.packs.values().next());
    let Some(pack) = pack else {
        return (Vec::new(), Vec::new());
    };
    let mut recommended = vec![pack.embed.clone(), pack.chat.clone()];
    // Downgrade chat if VRAM too low for the pack chat model.
    if let Some(chat) = find_offering(offerings, &pack.chat) {
        if chat.min_vram_mib > hw.vram_mib || chat.bytes + 700_000_000 > hw.disk_free_bytes {
            for alt in &pack.alternatives {
                if let Some(m) = find_offering(offerings, alt) {
                    if m.min_vram_mib <= hw.vram_mib
                        && m.bytes.saturating_add(700_000_000) <= hw.disk_free_bytes.max(m.bytes)
                    {
                        recommended[1] = alt.clone();
                        break;
                    }
                }
            }
        }
    }
    recommended.dedup();
    (recommended, pack.alternatives.clone())
}

pub fn build_setup_offer(home: &Path, hw: &HardwareInfo) -> Result<ModelSetupOffer, String> {
    let offerings = load_offerings(home)?;
    let (recommended_ids, alternative_chat_ids) = recommend_pack(&offerings, hw);
    let optional_ids: Vec<String> = offerings
        .models
        .iter()
        .filter(|m| m.optional && !recommended_ids.contains(&m.id) && !m.is_media())
        .filter(|m| m.min_vram_mib <= hw.vram_mib.saturating_add(2048))
        .map(|m| m.id.clone())
        .collect();
    Ok(ModelSetupOffer {
        hardware: hw.clone(),
        recommended_ids,
        alternative_chat_ids,
        optional_ids,
        models: offerings.models,
    })
}

pub fn write_setup_offer(home: &Path, offer: &ModelSetupOffer) -> Result<(), String> {
    let dir = home.join("var/run");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let raw = serde_json::to_string_pretty(offer).map_err(|e| e.to_string())?;
    fs::write(dir.join("model_setup_offer.json"), raw).map_err(|e| e.to_string())
}

pub fn read_setup_choice(home: &Path) -> Option<ModelSetupChoice> {
    let raw = fs::read_to_string(home.join("var/models/setup_choice.json")).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn setup_needed(home: &Path) -> bool {
    let inst = load_installed(home);
    if !inst.models.is_empty() {
        return false;
    }
    if setup_defaults_complete(&inst) {
        return false;
    }
    // Legacy: already have old GGUFs without installed.json
    let legacy = home
        .join("share/models/qwen2.5-3b-instruct-q4_k_m.gguf")
        .exists();
    if legacy {
        return false;
    }
    true
}

pub fn migrate_legacy_installed(home: &Path) -> Result<(), String> {
    let mut inst = load_installed(home);
    if !inst.models.is_empty() {
        return Ok(());
    }
    let offerings = load_offerings(home).ok();
    let dir = home.join("share/models");
    let legacy_pairs = [
        (
            "local:embedded-instruct",
            "qwen2.5-3b-instruct-q4_k_m.gguf",
            vec!["chat".into()],
        ),
        (
            "local:embedded-embed",
            "qwen2.5-0.5b-instruct-q4_k_m.gguf",
            vec!["embed".into()],
        ),
    ];
    let now = now_ms();
    for (id, filename, profiles) in legacy_pairs {
        let path = dir.join(filename);
        if path.exists() {
            let bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            inst.models.push(InstalledModel {
                id: id.into(),
                filename: filename.into(),
                profiles,
                bytes,
                user_pinned: false,
                installed_ms: now,
            });
        }
    }
    if inst.models.is_empty() {
        return Ok(());
    }
    inst.version = offerings
        .as_ref()
        .map(|o| o.version.clone())
        .unwrap_or_else(|| "legacy".into());
    inst.default_chat = Some("local:embedded-instruct".into());
    inst.default_embed = Some("local:embedded-embed".into());
    save_installed(home, &inst)
}

pub fn download_ids(home: &Path, ids: &[String]) -> Result<(), String> {
    let offerings = load_merged_offerings(home)?;
    let dir = home.join("share/models");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut inst = load_installed(home);
    let now = now_ms();
    for id in ids {
        let Some(m) = find_offering(&offerings, id) else {
            return Err(format!("modèle inconnu dans offerings: {id}"));
        };
        let path = dir.join(&m.filename);
        bootstrap::download_model_file(&m.url, &path, Some(m.bytes), &m.sha256)?;
        for extra in &m.extra_files {
            let extra_path = dir.join(&extra.filename);
            bootstrap::download_model_file(
                &extra.url,
                &extra_path,
                Some(extra.bytes).filter(|b| *b > 0),
                &extra.sha256,
            )?;
        }
        if let Some(engine_id) =
            engines::engine_id_for(m.engine.as_deref(), &m.profiles, m.modality.as_deref())
        {
            engines::ensure_engine(home, Some(&offerings), &engine_id)?;
        }
        inst.models.retain(|x| x.id != *id);
        inst.models.push(InstalledModel {
            id: id.clone(),
            filename: m.filename.clone(),
            profiles: m.profiles.clone(),
            bytes: m.bytes,
            user_pinned: false,
            installed_ms: now,
        });
    }
    inst.version = offerings.version;
    save_installed(home, &inst)?;
    Ok(())
}

/// Supprime un modèle installé (fichiers + registre ; custom offering si HF).
pub fn remove_installed_model(home: &Path, id: &str) -> Result<(), String> {
    if matches!(id, "local:embedded-instruct" | "local:embedded-embed") {
        return Err("alias système non supprimable".into());
    }
    let mut inst = load_installed(home);
    let Some(idx) = inst.models.iter().position(|m| m.id == id) else {
        return Err(format!("modèle non installé: {id}"));
    };
    let removed = inst.models.remove(idx);
    let dir = home.join("share/models");
    if let Ok(offerings) = load_merged_offerings(home) {
        if let Some(m) = find_offering(&offerings, id) {
            let _ = fs::remove_file(dir.join(&m.filename));
            for extra in &m.extra_files {
                let _ = fs::remove_file(dir.join(&extra.filename));
            }
        } else {
            let _ = fs::remove_file(model_path(home, &removed.filename));
        }
    } else {
        let _ = fs::remove_file(model_path(home, &removed.filename));
    }
    if inst.default_chat.as_deref() == Some(id) {
        inst.default_chat = inst
            .models
            .iter()
            .find(|m| m.profiles.iter().any(|p| p == "chat"))
            .map(|m| m.id.clone());
    }
    if inst.default_embed.as_deref() == Some(id) {
        inst.default_embed = inst
            .models
            .iter()
            .find(|m| m.profiles.iter().any(|p| p == "embed"))
            .map(|m| m.id.clone());
    }
    save_installed(home, &inst)?;
    let mut custom = load_custom_offerings(home);
    if custom.iter().any(|m| m.id == id) {
        custom.retain(|m| m.id != id);
        save_custom_offerings(home, &custom)?;
    }
    Ok(())
}

/// Force le re-téléchargement (efface les poids locaux puis `download_ids`).
pub fn redownload_ids(home: &Path, ids: &[String]) -> Result<(), String> {
    let offerings = load_merged_offerings(home)?;
    let dir = home.join("share/models");
    for id in ids {
        let Some(m) = find_offering(&offerings, id) else {
            return Err(format!("modèle inconnu dans offerings: {id}"));
        };
        let _ = fs::remove_file(dir.join(&m.filename));
        for extra in &m.extra_files {
            let _ = fs::remove_file(dir.join(&extra.filename));
        }
    }
    download_ids(home, ids)
}

pub fn apply_choice(home: &Path, choice: &ModelSetupChoice) -> Result<(), String> {
    validate_setup_choice(choice)?;
    let mut ids = choice.selected_ids.clone();
    if !ids.contains(&choice.default_chat) {
        ids.push(choice.default_chat.clone());
    }
    if !ids.contains(&choice.default_embed) {
        ids.push(choice.default_embed.clone());
    }
    ids.dedup();
    let local_ids: Vec<String> = ids
        .into_iter()
        .filter(|id| !is_remote_model_id(id))
        .collect();
    if !local_ids.is_empty() {
        download_ids(home, &local_ids)?;
    }
    let mut inst = load_installed(home);
    inst.default_chat = Some(choice.default_chat.clone());
    inst.default_embed = Some(choice.default_embed.clone());
    save_installed(home, &inst)?;
    let _ = fs::remove_file(home.join("var/run/model_setup_offer.json"));
    Ok(())
}

pub fn detect_model_updates(home: &Path, hw: &HardwareInfo) -> Option<ModelUpdateOffer> {
    let offerings = load_offerings(home).ok()?;
    let inst = load_installed(home);
    if inst.models.is_empty() {
        return None;
    }
    let (recommended, _) = recommend_pack(&offerings, hw);
    let mut available = Vec::new();
    for id in &recommended {
        if inst.models.iter().any(|m| m.id == *id) {
            continue;
        }
        if let Some(m) = find_offering(&offerings, id) {
            if !inst
                .models
                .iter()
                .any(|x| x.user_pinned && x.profiles.iter().any(|p| m.profiles.contains(p)))
            {
                available.push(m.clone());
            }
        }
    }
    // Also surface optional higher-tier models that fit.
    for m in &offerings.models {
        if m.optional
            && m.min_vram_mib <= hw.vram_mib
            && !inst.models.iter().any(|i| i.id == m.id)
            && !available.iter().any(|a| a.id == m.id)
        {
            available.push(m.clone());
        }
    }
    if available.is_empty() {
        let _ = fs::remove_file(home.join("var/run/model_updates.json"));
        return None;
    }
    let offer = ModelUpdateOffer {
        available,
        reason: format!(
            "Nouveaux modèles pour tier {} ({} MiB VRAM)",
            hw.tier.as_str(),
            hw.vram_mib
        ),
    };
    if let Ok(raw) = serde_json::to_string_pretty(&offer) {
        let _ = fs::create_dir_all(home.join("var/run"));
        let _ = fs::write(home.join("var/run/model_updates.json"), raw);
    }
    Some(offer)
}

pub fn model_path(home: &Path, filename: &str) -> PathBuf {
    for base in ["share/models", "tools/models"] {
        let p = home.join(base).join(filename);
        if p.exists() {
            return p;
        }
    }
    home.join("share/models").join(filename)
}

/// Entrées pour générer modeld.yaml (ids installés + alias legacy).
pub fn runtime_model_entries(
    home: &Path,
) -> (String, String, Vec<(String, ModelOffering, PathBuf)>) {
    let offerings = load_merged_offerings(home).ok();
    let inst = load_installed(home);
    let default_chat = inst
        .default_chat
        .clone()
        .unwrap_or_else(|| "local:embedded-instruct".into());
    let default_embed = inst
        .default_embed
        .clone()
        .unwrap_or_else(|| "local:embedded-embed".into());

    let mut entries: Vec<(String, ModelOffering, PathBuf)> = Vec::new();
    if let Some(off) = &offerings {
        for m in &inst.models {
            if let Some(o) = find_offering(off, &m.id) {
                entries.push((m.id.clone(), o.clone(), model_path(home, &m.filename)));
            } else {
                // Legacy id without offering entry
                let stub = ModelOffering {
                    id: m.id.clone(),
                    name: m.id.clone(),
                    profiles: m.profiles.clone(),
                    filename: m.filename.clone(),
                    url: String::new(),
                    bytes: m.bytes,
                    sha256: String::new(),
                    min_vram_mib: 0,
                    min_disk_bytes: 0,
                    quality_score: 50,
                    speed_score: 50,
                    n_layers: 32,
                    n_params: 1.0e9,
                    weights_bytes: m.bytes,
                    embed_bytes: 100_000_000,
                    kv_bytes_per_token: 40_000,
                    replaces: vec![],
                    optional: false,
                    modality: None,
                    format: "gguf".into(),
                    extra_files: vec![],
                    engine: None,
                    engine_args: Default::default(),
                    tags: vec![],
                    description: None,
                };
                entries.push((m.id.clone(), stub, model_path(home, &m.filename)));
            }
        }
    }

    // Ensure legacy aliases point at defaults when using new ids.
    if !entries
        .iter()
        .any(|(id, _, _)| id == "local:embedded-instruct")
    {
        if let Some((_, o, p)) = entries.iter().find(|(id, _, _)| id == &default_chat) {
            entries.push(("local:embedded-instruct".into(), o.clone(), p.clone()));
        }
    }
    if !entries
        .iter()
        .any(|(id, _, _)| id == "local:embedded-embed")
    {
        if let Some((_, o, p)) = entries.iter().find(|(id, _, _)| id == &default_embed) {
            entries.push(("local:embedded-embed".into(), o.clone(), p.clone()));
        }
    }

    (default_chat, default_embed, entries)
}

fn infer_format(filename: &str) -> String {
    let lower = filename.to_ascii_lowercase();
    if lower.ends_with(".safetensors") {
        "safetensors".into()
    } else if lower.ends_with(".onnx") {
        "onnx".into()
    } else {
        "gguf".into()
    }
}

fn infer_profiles(filename: &str, format: &str) -> Vec<String> {
    let lower = filename.to_ascii_lowercase();
    match format {
        "safetensors" => vec!["image".into()],
        "onnx" => vec!["tts".into()],
        _ if lower.contains("embed") => vec!["embed".into()],
        _ => vec!["chat".into()],
    }
}

fn infer_modality(profiles: &[String], format: &str) -> Option<String> {
    if profiles.iter().any(|p| p == "image") || format == "safetensors" {
        Some("image".into())
    } else if profiles.iter().any(|p| p == "tts") || format == "onnx" {
        Some("audio".into())
    } else {
        None
    }
}

fn sanitize_model_id(filename: &str) -> String {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("model");
    let mut out = String::from("local:hf-");
    for ch in stem.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() || ch == '.' {
            out.push('-');
        }
    }
    if out.len() <= "local:hf-".len() {
        out.push_str("custom");
    }
    out
}

fn parse_hf_download_url(url: &str) -> Result<(String, String), String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("URL vide".into());
    }
    if !url.starts_with("https://huggingface.co/") {
        return Err("URL Hugging Face attendue (https://huggingface.co/…)".into());
    }
    let rest = url
        .trim_start_matches("https://huggingface.co/")
        .trim_start_matches('/');
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.len() < 4 {
        return Err("URL HF invalide — attendu …/org/repo/resolve/main/file.gguf".into());
    }
    let _repo = format!("{}/{}", parts[0], parts[1]);
    let resolve_idx = parts
        .iter()
        .position(|p| *p == "resolve" || *p == "blob")
        .ok_or_else(|| "segmente resolve|blob manquant".to_string())?;
    if resolve_idx + 2 >= parts.len() {
        return Err("nom de fichier manquant dans l'URL".into());
    }
    let filename = parts[resolve_idx + 2..].join("/");
    if filename.is_empty() || filename.contains("..") {
        return Err("nom de fichier invalide".into());
    }
    let download_url = if parts.get(resolve_idx) == Some(&"blob") {
        url.replace("/blob/", "/resolve/")
    } else {
        url.to_string()
    };
    Ok((download_url, filename))
}

/// Download a Hugging Face resolve URL and register it as a custom offering.
pub fn download_hf_url(home: &Path, url: &str, name: Option<&str>) -> Result<String, String> {
    let (download_url, filename) = parse_hf_download_url(url)?;
    let dir = home.join("share/models");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(&filename);
    bootstrap::download_model_file(&download_url, &path, None, "")?;
    let bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let format = infer_format(&filename);
    let profiles = infer_profiles(&filename, &format);
    let modality = infer_modality(&profiles, &format);
    let id = sanitize_model_id(&filename);
    let display = name
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            Path::new(&filename)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&filename)
        });
    let offering = ModelOffering {
        id: id.clone(),
        name: display.to_string(),
        profiles,
        filename: filename.clone(),
        url: download_url,
        bytes,
        sha256: String::new(),
        min_vram_mib: 0,
        min_disk_bytes: bytes.saturating_add(500_000_000),
        quality_score: 50,
        speed_score: 50,
        n_layers: 0,
        n_params: 0.0,
        weights_bytes: bytes,
        embed_bytes: 0,
        kv_bytes_per_token: 0,
        replaces: vec![],
        optional: true,
        modality,
        format,
        extra_files: vec![],
        engine: None,
        engine_args: Default::default(),
        tags: vec!["custom".into(), "huggingface".into()],
        description: Some(format!("Import Hugging Face · {filename}")),
    };
    let mut custom = load_custom_offerings(home);
    custom.retain(|m| m.id != id);
    custom.push(offering.clone());
    save_custom_offerings(home, &custom)?;
    let mut inst = load_installed(home);
    inst.models.retain(|x| x.id != id);
    inst.models.push(InstalledModel {
        id: id.clone(),
        filename,
        profiles: offering.profiles,
        bytes,
        user_pinned: true,
        installed_ms: now_ms(),
    });
    save_installed(home, &inst)?;
    Ok(id)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[cfg(test)]
mod vision_catalog_tests {
    use super::OfferingsFile;

    #[test]
    fn catalog_parses_mmproj_role_and_vision_profile() {
        let raw = include_str!("../../../share/models/catalog-offerings.json");
        let file: OfferingsFile = serde_json::from_str(raw).expect("catalog json");
        assert_eq!(file.version, "0.3.4");
        let gemma = file
            .models
            .iter()
            .find(|m| m.id == "local:gemma-4-e4b")
            .expect("gemma e4b");
        assert!(gemma.profiles.iter().any(|p| p == "vision"));
        let mmproj = gemma
            .extra_files
            .iter()
            .find(|f| f.role.as_deref() == Some("mmproj"))
            .expect("mmproj sidecar");
        assert!(mmproj.filename.contains("mmproj"));
        assert!(mmproj.bytes > 0);
        assert_eq!(file.packs["low"].chat, "local:lfm2.5-8b-a1b");
        assert_eq!(file.packs["low"].alternatives[0], "local:gemma-4-e4b");
    }
}

#[cfg(test)]
mod setup_choice_tests {
    use super::{is_remote_model_id, validate_setup_choice, ModelSetupChoice};

    #[test]
    fn remote_model_ids_are_detected() {
        assert!(is_remote_model_id("provider:openai:gpt-4o"));
        assert!(is_remote_model_id("remote:mock:gpt-x"));
        assert!(!is_remote_model_id("local:qwen-9b"));
    }

    #[test]
    fn validate_setup_choice_requires_chat_and_embed() {
        assert!(validate_setup_choice(&ModelSetupChoice {
            selected_ids: vec![],
            default_chat: String::new(),
            default_embed: "local:embed".into(),
            include_optional: false,
        })
        .is_err());
        assert!(validate_setup_choice(&ModelSetupChoice {
            selected_ids: vec![],
            default_chat: "local:chat".into(),
            default_embed: String::new(),
            include_optional: false,
        })
        .is_err());
        assert!(validate_setup_choice(&ModelSetupChoice {
            selected_ids: vec![],
            default_chat: "provider:openai:gpt-4o".into(),
            default_embed: "local:embed".into(),
            include_optional: false,
        })
        .is_ok());
    }
}
