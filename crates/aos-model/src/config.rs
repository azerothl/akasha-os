//! Configuration de `aos-modeld` (fichier YAML dev, chemins réels des poids).

use aos_placement::{LanNode, QuantizationMetadata};
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
    /// Politique de routage local/distant : balanced | local_only | remote_only
    /// (§3.7, F-MDL-07). Défaut offline-first : balanced avec préférence locale.
    #[serde(default = "default_routing")]
    pub routing: String,
    /// Séquences simultanées (continuous batching P5.1, NFR-04).
    #[serde(default = "default_seq_max")]
    pub n_seq_max: u32,
    /// Fenêtre de rassemblement des jobs compatibles (µs → ms, §3.6).
    #[serde(default = "default_batch_window")]
    pub batch_window_ms: u64,
    /// Politique d'optimisation de l'inférence (E22).
    #[serde(default)]
    pub inference_optimization: InferenceOptimizationConfig,
    /// Adaptive backend/quantization planner. Enabled by default; experimental
    /// adapters remain opt-in independently.
    #[serde(default = "default_true")]
    pub adaptive_planner: bool,
    #[serde(default)]
    pub experimental_backends: bool,
    #[serde(default = "default_min_quality")]
    pub min_quantization_quality: f32,
    /// `performance`, `balanced`, `quiet` or `always-on`.
    #[serde(default = "default_thermal_policy")]
    pub thermal_policy: String,
    /// Explicitly configured, already paired LAN nodes. No discovery or
    /// network listener is enabled by this configuration alone.
    #[serde(default)]
    pub lan_cluster: LanClusterConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LanClusterConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_lan_node_id")]
    pub local_node_id: String,
    #[serde(default = "default_lan_listen_address")]
    pub listen_address: String,
    #[serde(default = "default_lan_session_secret")]
    pub session_key_secret: String,
    #[serde(default)]
    pub nodes: Vec<LanNode>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InferenceOptimizationConfig {
    /// `auto`, `on` ou `off` pour la réutilisation du préfixe KV.
    #[serde(default = "default_auto")]
    pub prefix_cache: String,
    /// `auto`, `on` ou `off`.
    #[serde(default = "default_auto")]
    pub speculation: String,
    /// Nombre maximal de tokens proposés par le prompt-lookup.
    #[serde(default = "default_draft_tokens")]
    pub max_draft_tokens: usize,
    /// Priorité minimale pour activer la spéculation en mode auto.
    #[serde(default = "default_spec_priority")]
    pub min_spec_priority: u8,
    /// Active la fenêtre de rassemblement du continuous batching.
    #[serde(default = "default_true")]
    pub adaptive_batching: bool,
}

impl Default for InferenceOptimizationConfig {
    fn default() -> Self {
        Self {
            prefix_cache: default_auto(),
            speculation: default_auto(),
            max_draft_tokens: default_draft_tokens(),
            min_spec_priority: default_spec_priority(),
            adaptive_batching: true,
        }
    }
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
    #[serde(default)]
    pub quantization: Option<QuantizationMetadata>,
    /// Explicitly calibrated, installed alternatives for the same architecture.
    #[serde(default)]
    pub variants: Vec<crate::variants::ModelVariant>,
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
    8192
}
fn default_threads() -> i32 {
    8
}
fn default_routing() -> String {
    "balanced".into()
}
fn default_seq_max() -> u32 {
    8
}
fn default_auto() -> String {
    "auto".into()
}
fn default_draft_tokens() -> usize {
    12
}
fn default_spec_priority() -> u8 {
    2
}
fn default_batch_window() -> u64 {
    150
}
fn default_min_quality() -> f32 {
    0.85
}
fn default_thermal_policy() -> String {
    "balanced".into()
}
fn default_lan_node_id() -> String {
    "local".into()
}
fn default_lan_listen_address() -> String {
    "127.0.0.1:9001".into()
}
fn default_lan_session_secret() -> String {
    "lan_cluster_session_key".into()
}

impl ModeldConfig {
    /// UI preferences take effect on the next load, without restarting modeld.
    /// Missing/legacy/invalid preferences preserve the YAML configuration.
    pub fn adaptive_planner_at(&self, home: &Path) -> bool {
        std::fs::read_to_string(home.join("var/run/preferences.json"))
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .and_then(|v| v.get("adaptive_planner").and_then(|v| v.as_bool()))
            .unwrap_or(self.adaptive_planner)
    }

    /// UI preferences can gate the configured LAN adapter without changing
    /// the YAML node inventory. Missing preferences preserve YAML behavior.
    pub fn lan_cluster_enabled_at(&self, home: &Path) -> bool {
        std::fs::read_to_string(home.join("var/run/preferences.json"))
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .and_then(|v| v.get("lan_cluster").and_then(|v| v.as_bool()))
            .unwrap_or(self.lan_cluster.enabled)
    }

    pub fn lan_node_id_at(&self, home: &Path) -> String {
        preference_string(home, "lan_node_id")
            .unwrap_or_else(|| self.lan_cluster.local_node_id.clone())
    }

    pub fn lan_listen_address_at(&self, home: &Path) -> String {
        preference_string(home, "lan_listen_address")
            .unwrap_or_else(|| self.lan_cluster.listen_address.clone())
    }

    pub fn lan_session_key_secret_at(&self, home: &Path) -> String {
        preference_string(home, "lan_session_key_secret")
            .unwrap_or_else(|| self.lan_cluster.session_key_secret.clone())
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(serde_yaml::from_str(&std::fs::read_to_string(path)?)?)
    }
}

fn preference_string(home: &Path, key: &str) -> Option<String> {
    std::fs::read_to_string(home.join("var/run/preferences.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|value| {
            value
                .get(key)
                .and_then(|value| value.as_str())
                .map(str::to_owned)
        })
        .filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptive_preference_is_reread_and_legacy_preserves_config() {
        let root = std::env::temp_dir().join(format!(
            "aos-adaptive-pref-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("var/run")).unwrap();
        let path = root.join("var/run/preferences.json");
        let cfg: ModeldConfig = serde_yaml::from_str("adaptive_planner: false").unwrap();
        assert!(!cfg.adaptive_planner_at(&root));
        for (raw, expected) in [
            (r#"{"adaptive_planner":true}"#, true),
            (r#"{"adaptive_planner":false}"#, false),
            (r#"{"inference_mode":"auto"}"#, false),
            (r#"{"adaptive_planner":"true"}"#, false),
            ("invalid", false),
        ] {
            std::fs::write(&path, raw).unwrap();
            assert_eq!(cfg.adaptive_planner_at(&root), expected);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn optimization_defaults_are_backward_compatible() {
        let cfg: ModeldConfig = serde_yaml::from_str("{}").expect("config minimale");
        assert_eq!(cfg.inference_optimization.prefix_cache, "auto");
        assert_eq!(cfg.inference_optimization.speculation, "auto");
        assert_eq!(cfg.inference_optimization.max_draft_tokens, 12);
        assert_eq!(cfg.inference_optimization.min_spec_priority, 2);
        assert!(cfg.inference_optimization.adaptive_batching);
        assert!(cfg.adaptive_planner);
        assert!(!cfg.experimental_backends);
    }

    #[test]
    fn optimization_values_are_loaded() {
        let cfg: ModeldConfig = serde_yaml::from_str(
            "inference_optimization:\n  prefix_cache: off\n  speculation: off\n  max_draft_tokens: 4\n  min_spec_priority: 1\n  adaptive_batching: false\n",
        )
        .expect("config optimization");
        assert_eq!(cfg.inference_optimization.speculation, "off");
        assert_eq!(cfg.inference_optimization.prefix_cache, "off");
        assert_eq!(cfg.inference_optimization.max_draft_tokens, 4);
        assert_eq!(cfg.inference_optimization.min_spec_priority, 1);
        assert!(!cfg.inference_optimization.adaptive_batching);
    }
}
