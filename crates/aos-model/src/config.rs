//! Configuration de `aos-modeld` (fichier YAML dev, chemins réels des poids).

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct ModeldConfig {
    #[serde(default = "default_bus")]
    pub bus: String,
    #[serde(default = "default_true")]
    pub gpu: bool,
    /// Budget VRAM offert au Placement Manager (octets) — la contrainte du
    /// gate P1 (8 GiB) se règle ici, indépendamment de la VRAM physique.
    #[serde(default = "default_vram")]
    pub vram_total_bytes: u64,
    #[serde(default = "default_reserve_vram")]
    pub os_reserve_vram_bytes: u64,
    #[serde(default = "default_reserve_ram")]
    pub os_reserve_ram_bytes: u64,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default = "default_kv")]
    pub default_kv_tokens: u32,
    #[serde(default = "default_threads")]
    pub n_threads: i32,
    /// Overrides par id de modèle (chemin GGUF réel + métadonnées mesurées).
    #[serde(default)]
    pub models: HashMap<String, ModelOverride>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelOverride {
    pub path: String,
    #[serde(default)]
    pub n_layers: Option<u32>,
    #[serde(default)]
    pub weights_bytes: Option<u64>,
    #[serde(default)]
    pub embed_bytes: Option<u64>,
    #[serde(default)]
    pub kv_bytes_per_token: Option<u64>,
    #[serde(default)]
    pub n_params: Option<f64>,
}

fn default_bus() -> String {
    format!("127.0.0.1:{}", aos_ipc::DEFAULT_BUS_PORT)
}
fn default_true() -> bool {
    true
}
fn default_vram() -> u64 {
    8 << 30
}
fn default_reserve_vram() -> u64 {
    1 << 30
}
fn default_reserve_ram() -> u64 {
    4 << 30
}
fn default_kv() -> u32 {
    2048
}
fn default_threads() -> i32 {
    8
}

impl ModeldConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(serde_yaml::from_str(&std::fs::read_to_string(path)?)?)
    }
}
