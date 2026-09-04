//! Policy Engine (§9.4) : règles déclaratives YAML, 3 effets en v1
//! (`allow` / `deny` / `require_confirmation`) — syntaxe volontairement
//! limitée (mitigation du risque « complexité du moteur de policy »).
//!
//! Évaluation : première règle qui matche gagne (règles ordonnées) ; à défaut,
//! `allow` (les refus structurels restent du ressort des caps).

use aos_proto::{PolicyEffect, PolicyRule};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

/// Le moteur de politique.
pub struct PolicyEngine {
    rules: Vec<PolicyRule>,
    path: Option<std::path::PathBuf>,
    default_confirm_timeout_sec: u64,
}

impl PolicyEngine {
    /// Charge les règles depuis un fichier YAML, sinon règles par défaut.
    pub fn open(
        path: Option<&Path>,
        default_confirm_timeout_sec: u64,
    ) -> Result<Self, PolicyError> {
        let path = path.map(|p| p.to_path_buf());
        let mut engine = Self {
            rules: Vec::new(),
            path: path.clone(),
            default_confirm_timeout_sec,
        };
        if let Some(p) = &path {
            if p.exists() {
                engine.load_from(p)?;
            }
        }
        if engine.rules.is_empty() {
            engine.rules = Self::default_rules(default_confirm_timeout_sec);
        }
        Ok(engine)
    }

    fn load_from(&mut self, path: &Path) -> Result<(), PolicyError> {
        #[derive(serde::Deserialize)]
        struct File {
            rules: Vec<PolicyRule>,
        }
        let file: File = serde_yaml::from_str(&std::fs::read_to_string(path)?)?;
        self.rules = file.rules;
        Ok(())
    }

    /// Recharge depuis le fichier (admin.policy.set en P3+).
    pub fn reload(&mut self) -> Result<usize, PolicyError> {
        let path = self.path.clone();
        if let Some(p) = &path {
            self.load_from(p)?;
        }
        Ok(self.rules.len())
    }

    /// Règles par défaut (§9.4 + F-SEC-07).
    pub fn default_rules(confirm_timeout_sec: u64) -> Vec<PolicyRule> {
        vec![
            PolicyRule {
                name: "deny_remote_secret".into(),
                matches: vec![
                    ("backend.privacy_class".into(), serde_json::json!("remote")),
                    ("data_class".into(), serde_json::json!("secret")),
                ],
                effect: PolicyEffect::Deny,
                timeout_sec: None,
            },
            PolicyRule {
                name: "confirm_sensitive_side_effect".into(),
                matches: vec![(
                    "action.kind".into(),
                    serde_json::json!(["fs.delete", "network.send_external", "payment"]),
                )],
                effect: PolicyEffect::RequireConfirmation,
                timeout_sec: Some(confirm_timeout_sec),
            },
            PolicyRule {
                name: "confirm_device_capture".into(),
                matches: vec![(
                    "action.kind".into(),
                    serde_json::json!([
                        "device.camera.capture",
                        "device.camera.stream",
                        "device.mic.capture",
                        "device.mic.stream"
                    ]),
                )],
                effect: PolicyEffect::RequireConfirmation,
                timeout_sec: Some(confirm_timeout_sec),
            },
        ]
    }

    pub fn rules(&self) -> &[PolicyRule] {
        &self.rules
    }

    /// Évalue un contexte (clés pointées) : première règle matchante.
    pub fn evaluate(&self, ctx: &HashMap<String, String>) -> (PolicyEffect, Option<&PolicyRule>) {
        for rule in &self.rules {
            if rule
                .matches
                .iter()
                .all(|(k, want)| match_one(ctx.get(k), want))
            {
                let effect = rule.effect;
                return (effect, Some(rule));
            }
        }
        (PolicyEffect::Allow, None)
    }

    /// Timeout effectif d'une règle require_confirmation.
    pub fn timeout_of(&self, rule: Option<&PolicyRule>) -> u64 {
        rule.and_then(|r| r.timeout_sec)
            .unwrap_or(self.default_confirm_timeout_sec)
    }
}

/// Un critère : valeur exacte, ou liste (match quelconque).
fn match_one(actual: Option<&String>, want: &serde_json::Value) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    match want {
        serde_json::Value::String(s) => s == actual,
        serde_json::Value::Array(arr) => arr.iter().any(|v| v.as_str() == Some(actual)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> PolicyEngine {
        PolicyEngine {
            rules: PolicyEngine::default_rules(120),
            path: None,
            default_confirm_timeout_sec: 120,
        }
    }

    fn ctx(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn secret_remote_denied() {
        let e = engine();
        let (eff, rule) = e.evaluate(&ctx(&[
            ("backend.privacy_class", "remote"),
            ("data_class", "secret"),
        ]));
        assert_eq!(eff, PolicyEffect::Deny);
        assert_eq!(rule.unwrap().name, "deny_remote_secret");
    }

    #[test]
    fn delete_exige_confirmation() {
        let e = engine();
        let (eff, rule) = e.evaluate(&ctx(&[("action.kind", "fs.delete")]));
        assert_eq!(eff, PolicyEffect::RequireConfirmation);
        assert_eq!(e.timeout_of(rule), 120);
    }

    #[test]
    fn secret_local_autorise() {
        let e = engine();
        let (eff, _) = e.evaluate(&ctx(&[
            ("backend.privacy_class", "local"),
            ("data_class", "secret"),
        ]));
        assert_eq!(eff, PolicyEffect::Allow);
    }

    #[test]
    fn aucun_match_allow_par_defaut() {
        let e = engine();
        let (eff, rule) = e.evaluate(&ctx(&[("action.kind", "fs.read")]));
        assert_eq!(eff, PolicyEffect::Allow);
        assert!(rule.is_none());
    }

    #[test]
    fn capture_peripherique_exige_confirmation() {
        let e = engine();
        let (eff, rule) = e.evaluate(&ctx(&[("action.kind", "device.camera.stream")]));
        assert_eq!(eff, PolicyEffect::RequireConfirmation);
        assert_eq!(rule.unwrap().name, "confirm_device_capture");
    }
}
