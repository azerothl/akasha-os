//! Préférences utilisateur persistées (`var/run/preferences.json`).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Density of the application chrome.  `comfortable` is the default and keeps
/// every interactive target at least 36 px high; `compact` is intended for
/// experienced users and never goes below 32 px.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UiDensity {
    #[default]
    Comfortable,
    Compact,
}

impl UiDensity {
    pub fn control_height(self) -> f32 {
        match self {
            Self::Comfortable => 36.0,
            Self::Compact => 32.0,
        }
    }

    pub fn rail_width(self) -> f32 {
        match self {
            Self::Comfortable => 88.0,
            Self::Compact => 64.0,
        }
    }
}

/// Persisted panel geometry and open/closed states.  New fields deliberately
/// use serde defaults so existing preference files remain valid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiLayoutPreferences {
    #[serde(default = "default_chat_sidebar_width")]
    pub chat_sidebar_width: f32,
    #[serde(default = "default_context_panel_width")]
    pub context_panel_width: f32,
    #[serde(default)]
    pub context_panel_open: bool,
    #[serde(default)]
    pub activity_panel_open: bool,
    #[serde(default)]
    pub notifications_open: bool,
    #[serde(default)]
    pub canvas_focus: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomThemePreferences {
    #[serde(default = "default_custom_background")]
    pub background: String,
    #[serde(default = "default_custom_panel")]
    pub panel: String,
    #[serde(default = "default_custom_text")]
    pub text: String,
    #[serde(default = "default_custom_accent")]
    pub accent: String,
    #[serde(default = "default_custom_danger")]
    pub danger: String,
    /// Boutons/toasts succès (ex. model removed). Surchargable, défaut emerald.
    #[serde(default = "default_custom_success")]
    pub success: String,
    /// Badges warning. Surchargable, défaut amber.
    #[serde(default = "default_custom_warning")]
    pub warning: String,
}

impl Default for CustomThemePreferences {
    fn default() -> Self {
        Self {
            background: default_custom_background(),
            panel: default_custom_panel(),
            text: default_custom_text(),
            accent: default_custom_accent(),
            danger: default_custom_danger(),
            success: default_custom_success(),
            warning: default_custom_warning(),
        }
    }
}

fn default_custom_background() -> String { "#070B14".into() }
fn default_custom_panel() -> String { "#101622".into() }
fn default_custom_text() -> String { "#E8EEF6".into() }
fn default_custom_accent() -> String { "#2EF0C8".into() }
fn default_custom_danger() -> String { "#FF5A48".into() }
fn default_custom_success() -> String { "#34D399".into() }
fn default_custom_warning() -> String { "#FBBF24".into() }

fn default_context_panel_width() -> f32 {
    320.0
}

fn default_chat_sidebar_width() -> f32 {
    112.0
}

impl Default for UiLayoutPreferences {
    fn default() -> Self {
        Self {
            chat_sidebar_width: default_chat_sidebar_width(),
            context_panel_width: default_context_panel_width(),
            context_panel_open: false,
            activity_panel_open: false,
            notifications_open: false,
            canvas_focus: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_routing")]
    pub routing: String,
    #[serde(default = "default_trust")]
    pub trust_default: String,
    /// `auto` | `gpu` | `cpu` — applied on next session boot.
    #[serde(default = "default_inference")]
    pub inference_mode: String,
    /// Applied by modeld when a model is next loaded.
    #[serde(default = "default_adaptive_planner")]
    pub adaptive_planner: bool,
    /// `light` | `dark` | `soft` | `high_contrast`
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub network_online: bool,
    /// E14 : extraire automatiquement des faits durables après chaque tour de chat.
    #[serde(default = "default_auto_remember_chat")]
    pub auto_remember_chat: bool,
    /// Download a newer Release into `var/updates/` when detected (apply on next launch).
    #[serde(default)]
    pub auto_download_updates: bool,
    #[serde(default)]
    pub default_agent_model: Option<String>,
    #[serde(default = "default_max_steps")]
    pub default_max_steps: u32,
    #[serde(default = "default_timeout_secs")]
    pub default_timeout_secs: u64,
    /// `auto` | `brave` | `searxng` | `duckduckgo` | `bing`
    #[serde(default = "default_search_engine")]
    pub web_search_engine: String,
    /// Optional SearXNG instance base URL (`https://searx.example`). Empty = unused.
    #[serde(default)]
    pub searxng_url: String,
    #[serde(default = "default_fetch_max")]
    pub web_fetch_max_bytes: u64,
    #[serde(default = "default_browse_chars")]
    pub web_browse_max_chars: usize,
    /// Default image offering id (`local:sd-v1-5`, `local:flux2`, …).
    #[serde(default)]
    pub default_image_model: Option<String>,
    /// Default Piper offering id.
    #[serde(default)]
    pub default_audio_model: Option<String>,
    #[serde(default = "default_image_size")]
    pub image_width: u32,
    #[serde(default = "default_image_size")]
    pub image_height: u32,
    #[serde(default = "default_image_steps")]
    pub image_steps: u32,
    /// Interface scale as a percentage (90, 100, 110, 125, 150). Applied via egui `zoom_factor`.
    #[serde(default = "default_ui_scale_percent")]
    pub ui_scale_percent: u32,
    /// `ask` (default) | `autonomous` — inline Allow Once gate for chat agents.
    #[serde(default = "default_agent_gate_mode")]
    pub agent_gate_mode: String,
    /// Opt-in extra signed Git catalogue (community/). Off by default.
    #[serde(default)]
    pub community_catalogue_enabled: bool,
    #[serde(default)]
    pub ui_density: UiDensity,
    #[serde(default)]
    pub ui_layout: UiLayoutPreferences,
    #[serde(default)]
    pub custom_theme: CustomThemePreferences,
}

/// Preset scale steps exposed in Settings → Me.
pub const UI_SCALE_PRESETS: [u32; 5] = [90, 100, 110, 125, 150];

/// Preview UI language from OS locale when possible (`en` or `fr`).
pub fn detect_os_language() -> String {
    detect_language_from_locales(
        ["LC_ALL", "LC_MESSAGES", "LANG"].map(|key| std::env::var(key).ok()),
    )
}

fn detect_language_from_locales(locales: impl IntoIterator<Item = Option<String>>) -> String {
    for raw in locales.into_iter().flatten() {
        let tag = raw.split('.').next().unwrap_or(&raw).to_ascii_lowercase();
        if tag.starts_with("fr") {
            return "fr".into();
        }
        if tag.starts_with("en") {
            return "en".into();
        }
    }
    "en".into()
}

fn default_language() -> String {
    detect_os_language()
}
fn default_routing() -> String {
    "local_only".into()
}
fn default_trust() -> String {
    "medium".into()
}
fn default_inference() -> String {
    "auto".into()
}

fn default_adaptive_planner() -> bool {
    true
}
fn default_theme() -> String {
    "dark".into()
}
fn default_max_steps() -> u32 {
    64
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
fn default_auto_remember_chat() -> bool {
    true
}
fn default_image_size() -> u32 {
    512
}
fn default_image_steps() -> u32 {
    20
}
fn default_ui_scale_percent() -> u32 {
    100
}

fn default_agent_gate_mode() -> String {
    "ask".into()
}

/// Clamp persisted or deserialized scale to supported Preview presets.
pub fn clamp_ui_scale_percent(percent: u32) -> u32 {
    UI_SCALE_PRESETS
        .iter()
        .min_by_key(|preset| preset.abs_diff(percent))
        .copied()
        .unwrap_or(default_ui_scale_percent())
}

/// Multiplier for egui `pixels_per_point` (1.0 = 100%).
pub fn ui_scale_factor(percent: u32) -> f32 {
    clamp_ui_scale_percent(percent) as f32 / 100.0
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            language: default_language(),
            routing: default_routing(),
            trust_default: default_trust(),
            inference_mode: default_inference(),
            adaptive_planner: default_adaptive_planner(),
            theme: default_theme(),
            network_online: false,
            auto_remember_chat: default_auto_remember_chat(),
            auto_download_updates: false,
            default_agent_model: None,
            default_max_steps: default_max_steps(),
            default_timeout_secs: default_timeout_secs(),
            web_search_engine: default_search_engine(),
            searxng_url: String::new(),
            web_fetch_max_bytes: default_fetch_max(),
            web_browse_max_chars: default_browse_chars(),
            default_image_model: None,
            default_audio_model: None,
            image_width: default_image_size(),
            image_height: default_image_size(),
            image_steps: default_image_steps(),
            ui_scale_percent: default_ui_scale_percent(),
            agent_gate_mode: default_agent_gate_mode(),
            community_catalogue_enabled: false,
            ui_density: UiDensity::default(),
            ui_layout: UiLayoutPreferences::default(),
            custom_theme: CustomThemePreferences::default(),
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
        if let Ok(mut prefs) = serde_json::from_str::<Preferences>(&raw) {
            prefs.ui_scale_percent = clamp_ui_scale_percent(prefs.ui_scale_percent);
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
    let mut map: serde_json::Map<String, serde_json::Value> =
        if let Ok(raw) = std::fs::read_to_string(&path) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_os_language_defaults_to_en_without_locale() {
        assert_eq!(detect_language_from_locales([None, None, None]), "en");
    }

    #[test]
    fn detect_os_language_reads_fr_locale() {
        assert_eq!(
            detect_language_from_locales([None, None, Some("fr_FR.UTF-8".into())]),
            "fr"
        );
    }

    #[test]
    fn detect_os_language_reads_en_locale() {
        assert_eq!(
            detect_language_from_locales([Some("en_GB.UTF-8".into()), None, None]),
            "en"
        );
    }

    #[test]
    fn ui_scale_defaults_to_one_hundred() {
        assert_eq!(default_ui_scale_percent(), 100);
        assert_eq!(Preferences::default().ui_scale_percent, 100);
        assert!((ui_scale_factor(100) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn ui_scale_clamps_to_nearest_preset() {
        assert_eq!(clamp_ui_scale_percent(100), 100);
        assert_eq!(clamp_ui_scale_percent(95), 90);
        assert_eq!(clamp_ui_scale_percent(113), 110);
        assert_eq!(clamp_ui_scale_percent(118), 125);
        assert_eq!(clamp_ui_scale_percent(140), 150);
        assert_eq!(clamp_ui_scale_percent(200), 150);
        assert_eq!(clamp_ui_scale_percent(0), 90);
    }

    #[test]
    fn ui_scale_persists_in_preferences_json() {
        let prefs = Preferences {
            ui_scale_percent: 110,
            ..Preferences::default()
        };
        let raw = serde_json::to_string(&prefs).expect("serialize prefs");
        assert!(raw.contains("\"ui_scale_percent\":110"));
        let loaded: Preferences = serde_json::from_str(&raw).expect("deserialize prefs");
        assert_eq!(loaded.ui_scale_percent, 110);
    }

    #[test]
    fn new_layout_fields_migrate_from_legacy_json() {
        let raw = r#"{"language":"en","theme":"dark","ui_scale_percent":100}"#;
        let prefs: Preferences = serde_json::from_str(raw).expect("legacy preferences");
        assert_eq!(prefs.ui_density, UiDensity::Comfortable);
        assert_eq!(prefs.ui_layout.context_panel_width, 320.0);
        assert!(!prefs.ui_layout.activity_panel_open);
        assert_eq!(prefs.custom_theme.accent, "#2EF0C8");
        // Nouveaux champs boutons : défauts stables même sans migration.
        assert_eq!(prefs.custom_theme.success, "#34D399");
        assert_eq!(prefs.custom_theme.warning, "#FBBF24");
    }

    #[test]
    fn compact_density_keeps_minimum_targets() {
        assert!(UiDensity::Compact.control_height() >= 32.0);
        assert!(UiDensity::Comfortable.control_height() >= 36.0);
        assert!(UiDensity::Comfortable.rail_width() > UiDensity::Compact.rail_width());
    }
}
