//! Persist Image Studio generation metadata next to PNGs under `/downloads`.

use crate::image_composition::CompositionBlock;
use crate::os_open::aos_home;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const META_VERSION: u32 = 1;
const HISTORY_CAP: usize = 40;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenMeta {
    pub version: u32,
    pub created_unix: u64,
    /// Logical path (`/downloads/image-….png`).
    pub path: String,
    #[serde(default)]
    pub model_id: String,
    #[serde(default)]
    pub engine: String,
    /// Original studio / slash prompt.
    pub prompt: String,
    /// Final prompt sent to the engine (enriched / layout-merged), if different.
    #[serde(default)]
    pub generation_prompt: Option<String>,
    #[serde(default)]
    pub composition_blocks: Vec<CompositionBlock>,
}

impl ImageGenMeta {
    pub fn new(
        path: impl Into<String>,
        prompt: impl Into<String>,
        generation_prompt: Option<String>,
        composition_blocks: Vec<CompositionBlock>,
        model_id: impl Into<String>,
        engine: impl Into<String>,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            version: META_VERSION,
            created_unix: now,
            path: path.into(),
            model_id: model_id.into(),
            engine: engine.into(),
            prompt: prompt.into(),
            generation_prompt,
            composition_blocks,
        }
    }

    pub fn truncated_prompt(&self, max: usize) -> String {
        let s = self.prompt.trim();
        if s.is_empty() {
            return self.path.clone();
        }
        if s.chars().count() <= max {
            return s.to_string();
        }
        let take: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{take}…")
    }
}

fn downloads_host_dir() -> PathBuf {
    aos_home().join("var/storage/data/downloads")
}

fn host_png_from_logical(logical: &str) -> PathBuf {
    let rel = logical.trim_start_matches('/');
    aos_home().join("var/storage/data").join(rel)
}

pub fn meta_host_path_for_logical(logical: &str) -> PathBuf {
    let png = host_png_from_logical(logical);
    let stem = png
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image");
    png.with_file_name(format!("{stem}.meta.json"))
}

pub fn write_image_meta(meta: &ImageGenMeta) -> Result<(), String> {
    let path = meta_host_path_for_logical(&meta.path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(meta).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

pub fn load_image_meta(logical: &str) -> Option<ImageGenMeta> {
    let path = meta_host_path_for_logical(logical);
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Copy meta from an existing PNG to a new logical path (e.g. after upscale).
pub fn clone_meta_for_new_path(source_logical: &str, new_logical: &str, engine: &str) -> Option<()> {
    let mut meta = load_image_meta(source_logical)?;
    meta.path = new_logical.to_string();
    if !engine.is_empty() {
        meta.engine = engine.to_string();
    }
    meta.created_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(meta.created_unix);
    write_image_meta(&meta).ok()
}

pub fn list_image_history(limit: usize) -> Vec<ImageGenMeta> {
    let dir = downloads_host_dir();
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut metas: Vec<ImageGenMeta> = rd
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.ends_with(".meta.json"))
                .unwrap_or(false)
        })
        .filter_map(|e| {
            let raw = std::fs::read_to_string(e.path()).ok()?;
            serde_json::from_str::<ImageGenMeta>(&raw).ok()
        })
        .filter(|m| {
            let png = host_png_from_logical(&m.path);
            png.is_file() || Path::new(&m.path).is_file()
        })
        .collect();
    metas.sort_by(|a, b| b.created_unix.cmp(&a.created_unix));
    metas.truncate(limit.clamp(1, HISTORY_CAP));
    metas
}
