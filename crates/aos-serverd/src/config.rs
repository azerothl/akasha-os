//! Opt-in serverd config (`etc/serverd.yaml`) — P21.6.
//!
//! Default: do **not** supervise `aos-mcpd` / `aos-bridged` (same as 0.18).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Preview serverd extras (ADR 0012 / P21.6).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerdConfig {
    /// When true, `aos-serverd serve` also spawns `aos-bridged` (loopback HTTP).
    #[serde(default)]
    pub supervise_bridged: bool,
    /// When true, `aos-serverd serve` also spawns `aos-mcpd` (stdio held open;
    /// IDE clients still launch their own mcpd for MCP traffic — see mcp-server.md).
    #[serde(default)]
    pub supervise_mcpd: bool,
    /// When true, `aos-session` may start `aos-serverd serve` then attach UI
    /// instead of legacy-spawning the tree itself.
    #[serde(default)]
    pub session_bootstrap_serverd: bool,
}

impl ServerdConfig {
    /// Merge env overrides (`AOS_SUPERVISE_BRIDGED=1`, `AOS_SUPERVISE_MCPD=1`,
    /// `AOS_SESSION_BOOTSTRAP_SERVERD=1`).
    pub fn with_env_overrides(mut self) -> Self {
        if env_truthy("AOS_SUPERVISE_BRIDGED") {
            self.supervise_bridged = true;
        }
        if env_truthy("AOS_SUPERVISE_MCPD") {
            self.supervise_mcpd = true;
        }
        if env_truthy("AOS_SESSION_BOOTSTRAP_SERVERD") {
            self.session_bootstrap_serverd = true;
        }
        self
    }
}

fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// Load `etc/serverd.yaml` or `etc/serverd.json` if present; else defaults.
pub fn load_serverd_config(home: &Path) -> ServerdConfig {
    let yaml = home.join("etc/serverd.yaml");
    let json = home.join("etc/serverd.json");
    let mut cfg = if yaml.exists() {
        fs::read_to_string(&yaml)
            .ok()
            .and_then(|raw| serde_yaml::from_str(&raw).ok())
            .unwrap_or_default()
    } else if json.exists() {
        fs::read_to_string(&json)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    } else {
        ServerdConfig::default()
    };
    cfg = cfg.with_env_overrides();
    cfg
}

/// Example body written by session bootstrap when missing (all flags false).
pub fn serverd_config_example() -> &'static str {
    r#"# aos-serverd extras (P21.6 / ADR 0012) — all opt-in, default off.
# Desktop install does not enable these.

# Spawn aos-bridged (loopback HTTP↔bus) under serverd.
supervise_bridged: false

# Spawn aos-mcpd under serverd (stdin held open). IDE MCP clients still
# launch their own aos-mcpd over stdio — see docs/mcp-server.md.
supervise_mcpd: false

# Prefer aos-session → start serverd then attach UI (vs legacy tree spawn).
session_bootstrap_serverd: false
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn defaults_are_off() {
        let c = ServerdConfig::default();
        assert!(!c.supervise_bridged);
        assert!(!c.supervise_mcpd);
        assert!(!c.session_bootstrap_serverd);
    }

    #[test]
    fn parses_yaml_flags() {
        let raw = "supervise_bridged: true\nsupervise_mcpd: true\n";
        let c: ServerdConfig = serde_yaml::from_str(raw).unwrap();
        assert!(c.supervise_bridged);
        assert!(c.supervise_mcpd);
    }

    #[test]
    fn load_missing_file_is_default() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home = std::env::temp_dir().join(format!("aos-serverd-cfg-{stamp}"));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(home.join("etc")).unwrap();
        let c = load_serverd_config(&home);
        assert_eq!(c, ServerdConfig::default());
        let _ = fs::remove_dir_all(&home);
    }
}
