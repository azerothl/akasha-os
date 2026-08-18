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
    /// Sidecar files (Piper `.onnx.json`, etc.).
    #[serde(default)]
    pub extra_files: Vec<OfferingFile>,
    /// Catalogue engine id (`sdcpp`, `piper`). Inferred from profiles if empty.
    #[serde(default)]
    pub engine: Option<String>,
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
    let offerings = load_offerings(home)?;
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
        if let Some(engine_id) = engines::engine_id_for(m.engine.as_deref(), &m.profiles, m.modality.as_deref())
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

pub fn apply_choice(home: &Path, choice: &ModelSetupChoice) -> Result<(), String> {
    let mut ids = choice.selected_ids.clone();
    if !ids.contains(&choice.default_chat) {
        ids.push(choice.default_chat.clone());
    }
    if !ids.contains(&choice.default_embed) {
        ids.push(choice.default_embed.clone());
    }
    ids.dedup();
    download_ids(home, &ids)?;
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
            if !inst.models.iter().any(|x| x.user_pinned && x.profiles.iter().any(|p| m.profiles.contains(p))) {
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
    let offerings = load_offerings(home).ok();
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
                };
                entries.push((m.id.clone(), stub, model_path(home, &m.filename)));
            }
        }
    }

    // Ensure legacy aliases point at defaults when using new ids.
    if !entries.iter().any(|(id, _, _)| id == "local:embedded-instruct") {
        if let Some((_, o, p)) = entries.iter().find(|(id, _, _)| id == &default_chat) {
            entries.push((
                "local:embedded-instruct".into(),
                o.clone(),
                p.clone(),
            ));
        }
    }
    if !entries.iter().any(|(id, _, _)| id == "local:embedded-embed") {
        if let Some((_, o, p)) = entries.iter().find(|(id, _, _)| id == &default_embed) {
            entries.push(("local:embedded-embed".into(), o.clone(), p.clone()));
        }
    }

    (default_chat, default_embed, entries)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
