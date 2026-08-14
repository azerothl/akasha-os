//! Préférences utilisateur persistées (`var/run/preferences.json`).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_routing")]
    pub routing: String,
    #[serde(default = "default_trust")]
    pub trust_default: String,
    #[serde(default)]
    pub network_online: bool,
    #[serde(default)]
    pub default_agent_model: Option<String>,
    #[serde(default = "default_max_steps")]
    pub default_max_steps: u32,
    #[serde(default = "default_timeout_secs")]
    pub default_timeout_secs: u64,
    /// `auto` | `brave` | `duckduckgo` | `bing`
    #[serde(default = "default_search_engine")]
    pub web_search_engine: String,
    #[serde(default = "default_fetch_max")]
    pub web_fetch_max_bytes: u64,
    #[serde(default = "default_browse_chars")]
    pub web_browse_max_chars: usize,
}

fn default_language() -> String {
    "en".into()
}
fn default_routing() -> String {
    "local_only".into()
}
fn default_trust() -> String {
    "medium".into()
}
fn default_max_steps() -> u32 {
    32
}
fn default_timeout_secs() -> u64 {
    3600
}
fn default_search_engine() -> String {
    "auto".into()
}
fn default_fetch_max() -> u64 {
    50 * 1024 * 1024
}
fn default_browse_chars() -> usize {
    12_000
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            language: default_language(),
            routing: default_routing(),
            trust_default: default_trust(),
            network_online: false,
            default_agent_model: None,
            default_max_steps: default_max_steps(),
            default_timeout_secs: default_timeout_secs(),
            web_search_engine: default_search_engine(),
            web_fetch_max_bytes: default_fetch_max(),
            web_browse_max_chars: default_browse_chars(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct OnboardingSlice {
    #[serde(default)]
    language: String,
    #[serde(default)]
    routing: String,
    #[serde(default)]
    trust_default: String,
}

fn aos_run_dir() -> PathBuf {
    let home = std::env::var("AOS_HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join("var/run")
}

pub fn preferences_path() -> PathBuf {
    aos_run_dir().join("preferences.json")
}

fn onboarding_path() -> PathBuf {
    aos_run_dir().join("onboarding.json")
}

/// Charge les préférences ; migre depuis `onboarding.json` si le fichier est absent.
pub fn load_preferences() -> Preferences {
    let p = preferences_path();
    if let Ok(raw) = std::fs::read_to_string(&p) {
        if let Ok(prefs) = serde_json::from_str::<Preferences>(&raw) {
            return prefs;
        }
    }
    let mut prefs = Preferences::default();
    if let Ok(raw) = std::fs::read_to_string(onboarding_path()) {
        if let Ok(onb) = serde_json::from_str::<OnboardingSlice>(&raw) {
            if !onb.language.is_empty() {
                prefs.language = onb.language;
            }
            if !onb.routing.is_empty() {
                prefs.routing = onb.routing;
            }
            if !onb.trust_default.is_empty() {
                prefs.trust_default = onb.trust_default;
            }
        }
    }
    save_preferences(&prefs);
    prefs
}

pub fn save_preferences(prefs: &Preferences) {
    let p = preferences_path();
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(raw) = serde_json::to_string_pretty(prefs) {
        let _ = std::fs::write(&p, raw);
    }
    sync_onboarding_language(prefs);
}

/// Aligne les champs partagés dans onboarding.json (sans écraser le tutoriel).
fn sync_onboarding_language(prefs: &Preferences) {
    let path = onboarding_path();
    let mut map: serde_json::Map<String, serde_json::Value> = if let Ok(raw) =
        std::fs::read_to_string(&path)
    {
        serde_json::from_str(&raw).unwrap_or_default()
    } else {
        serde_json::Map::new()
    };
    map.insert(
        "language".into(),
        serde_json::Value::String(prefs.language.clone()),
    );
    map.insert(
        "routing".into(),
        serde_json::Value::String(prefs.routing.clone()),
    );
    map.insert(
        "trust_default".into(),
        serde_json::Value::String(prefs.trust_default.clone()),
    );
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(raw) = serde_json::to_string_pretty(&serde_json::Value::Object(map)) {
        let _ = std::fs::write(&path, raw);
    }
}
