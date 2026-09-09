//! Budget cloud mensuel (S2, phase 1 — UI only).
//!
//! Mesure honnête mais partielle : turns chat sur modèle provider NON-loopback
//! (prompt au `begin_turn`, complétion au `Done`), estimation caractères/4 ×
//! table de prix indicative (marquée ≈ partout). **Agents exclus** : `AgentInfo`
//! ne reporte pas le modèle utilisé — le badge/tooltip le dit explicitement.
//! Phase 2 : usage réel remonté par `modeld` + `model_id` dans `AgentInfo`.
//! Dépassement + cut = bascule `local_only` (fail-closed, loopback inchangé).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::prefs::save_preferences;
use crate::UiApp;
use eframe::egui;

#[allow(dead_code)]
pub const DEFAULT_ALERT_PCT: u8 = 80;

/// Tarif indicatif en cents USD par million de tokens (tarifs publics 2025,
/// approximatifs — affichés avec ≈, table documentée, pas de fausse précision).
pub struct ProviderRate {
    pub in_cents_per_1m: f64,
    pub out_cents_per_1m: f64,
    /// Faux si taux de repli (modèle inconnu) : sur-estimation volontaire.
    #[allow(dead_code)]
    pub known: bool,
}

pub fn rate_for_model(model: &str) -> ProviderRate {
    let m = model.to_ascii_lowercase();
    // Ordre : variantes *-mini avant la forme pleine ("gpt-4o-mini" contient "gpt-4o").
    let table: &[(&str, f64, f64)] = &[
        ("o1-mini", 300.0, 1200.0),
        ("o3-mini", 110.0, 440.0),
        ("o1", 1500.0, 6000.0),
        ("gpt-4o-mini", 15.0, 60.0),
        ("gpt-4o", 250.0, 1000.0),
        ("claude-3-opus", 1500.0, 7500.0),
        ("claude-3-7-sonnet", 300.0, 1500.0),
        ("claude-3-5-sonnet", 300.0, 1500.0),
        ("claude-3-5-haiku", 80.0, 400.0),
        ("claude-3-haiku", 25.0, 125.0),
        ("gemini-2.0-flash", 10.0, 40.0),
        ("gemini-1.5-pro", 125.0, 500.0),
        ("gemini-1.5-flash", 8.0, 30.0),
        ("deepseek-reasoner", 55.0, 219.0),
        ("deepseek-chat", 27.0, 110.0),
        ("deepseek-v3", 27.0, 110.0),
        ("mistral-large", 200.0, 600.0),
        ("mistral-small", 20.0, 60.0),
        ("grok-beta", 500.0, 1500.0),
    ];
    for (key, input, output) in table {
        if m.contains(key) {
            return ProviderRate {
                in_cents_per_1m: *input,
                out_cents_per_1m: *output,
                known: true,
            };
        }
    }
    // Repli conservateur (sur-estime) : le plafond protège même l'inconnu.
    ProviderRate {
        in_cents_per_1m: 500.0,
        out_cents_per_1m: 2000.0,
        known: false,
    }
}

/// `provider:<provider_id>:<model>` → (provider_id, model).
pub fn split_provider_model(model_id: &str) -> Option<(&str, &str)> {
    let rest = model_id.strip_prefix("provider:")?;
    let (pid, model) = rest.split_once(':')?;
    if pid.is_empty() || model.is_empty() {
        return None;
    }
    Some((pid, model))
}

/// Loopback = coût nul (Ollama/vLLM/LM Studio locaux). Parité d'intention avec
/// `aos-model::providers::endpoint_is_loopback` (privé, non réutilisable).
pub fn endpoint_is_loopback(endpoint: &str) -> bool {
    let lower = endpoint.trim().to_ascii_lowercase();
    let after_scheme = lower.split("://").last().unwrap_or(&lower);
    let host = if after_scheme.starts_with('[') {
        after_scheme
            .split(']')
            .next()
            .unwrap_or(after_scheme)
            .trim_start_matches('[')
    } else {
        after_scheme
            .split([':', '/'])
            .next()
            .unwrap_or(after_scheme)
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
        || host.starts_with("127.")
        || host == "0.0.0.0"
}

/// Coût estimé d'un turn, arrondi au cent supérieur.
pub fn cents_for_turn(prompt_chars: u64, completion_chars: u64, rate: &ProviderRate) -> u64 {
    let prompt_tok = prompt_chars as f64 / 4.0;
    let completion_tok = completion_chars as f64 / 4.0;
    ((prompt_tok * rate.in_cents_per_1m + completion_tok * rate.out_cents_per_1m) / 1_000_000.0)
        .ceil() as u64
}

pub fn current_month_key() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    y += i64::from(m <= 2);
    format!("{y:04}-{m:02}")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BillingLedger {
    pub month: String,
    pub cents: u64,
    pub alerted: bool,
    #[serde(default)]
    pub by_model: HashMap<String, u64>,
    #[serde(default)]
    pub turns: u64,
}

pub fn ledger_path() -> PathBuf {
    crate::os_open::aos_home().join("var/run/billing.json")
}

pub fn load_ledger() -> BillingLedger {
    let path = ledger_path();
    let raw = std::fs::read_to_string(&path).unwrap_or_default();
    let mut ledger: BillingLedger = serde_json::from_str(&raw).unwrap_or_default();
    rollover_ledger(&mut ledger);
    ledger
}

pub fn save_ledger(ledger: &BillingLedger) {
    let path = ledger_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(raw) = serde_json::to_string_pretty(ledger) {
        let _ = std::fs::write(&path, raw);
    }
}

/// Nouveau mois calendaire → remise à zéro (alerte incluse).
pub fn rollover_ledger(ledger: &mut BillingLedger) {
    let now = current_month_key();
    if ledger.month != now {
        *ledger = BillingLedger {
            month: now,
            ..Default::default()
        };
    }
}

/// (alerte_due, cut_due). Purs et testés.
pub fn thresholds_due(cents: u64, cap_cents: u32, alert_pct: u8, alerted: bool) -> (bool, bool) {
    if cap_cents == 0 {
        return (false, false);
    }
    let alert_at = (cap_cents as u64).saturating_mul(alert_pct.max(1) as u64) / 100;
    (!alerted && cents >= alert_at, cents >= cap_cents as u64)
}

pub fn format_cents(cents: u64, fr: bool) -> String {
    let s = format!("{:.2}", cents as f64 / 100.0);
    if fr {
        format!("{} €", s.replace('.', ","))
    } else {
        format!("€{s}")
    }
}

impl UiApp {
    /// Modèle provider facturable de la session (ou modèle par défaut),
    /// hors loopback (coût nul). Endpoint inconnu = facturable (fail-closed
    /// sur l'argent : la liste providers peut ne pas être chargée).
    pub(crate) fn session_billable_model(&self, session_id: &str) -> Option<(String, String)> {
        let model_id = self
            .chat_state
            .sessions
            .iter()
            .find(|s| s.id == session_id)
            .and_then(|s| s.model_id.clone())
            .or_else(|| self.prefs.default_agent_model.clone())?;
        let (pid, model) = split_provider_model(&model_id)?;
        let billable = match self.models_ui.providers.iter().find(|p| p.id == pid) {
            Some(rec) => !endpoint_is_loopback(&rec.endpoint),
            None => true,
        };
        billable.then(|| (pid.to_string(), model.to_string()))
    }

    /// Prompt d'un turn (appelé aux deux `begin_turn` de send_chat).
    pub(crate) fn note_provider_prompt(&mut self, session_id: &str, text: &str) {
        if self.session_billable_model(session_id).is_some() {
            self.billing_pending
                .insert(session_id.to_string(), text.len() as u64);
        }
    }

    /// Complétion (appelée dans `on_done`) : clôture le turn, comptabilise,
    /// alerte et coupe si plafond. Montants en cents, arrondi supérieur.
    pub(crate) fn note_provider_completion(&mut self, session_id: &str, text: &str) {
        let prompt_chars = self.billing_pending.remove(session_id).unwrap_or(0);
        let Some((_, model)) = self.session_billable_model(session_id) else {
            return;
        };
        let rate = rate_for_model(&model);
        let cents = cents_for_turn(prompt_chars, text.len() as u64, &rate);
        if cents == 0 {
            return;
        }
        rollover_ledger(&mut self.billing);
        self.billing.cents += cents;
        self.billing.turns += 1;
        *self.billing.by_model.entry(model.clone()).or_insert(0) += cents;
        save_ledger(&self.billing);
        let fr = self.prefs.language == "fr";
        let (alert_due, cut_due) = thresholds_due(
            self.billing.cents,
            self.prefs.cloud_cap_cents,
            self.prefs.cloud_alert_pct,
            self.billing.alerted,
        );
        if alert_due {
            self.billing.alerted = true;
            save_ledger(&self.billing);
            self.toasts.push_error(if fr {
                format!(
                    "Budget cloud {}% atteint (≈{}) — estimation chat uniquement",
                    self.prefs.cloud_alert_pct,
                    format_cents(self.billing.cents, true),
                )
            } else {
                format!(
                    "Cloud budget {}% reached (≈{}) — chat estimate only",
                    self.prefs.cloud_alert_pct,
                    format_cents(self.billing.cents, false),
                )
            });
        }
        if cut_due && self.prefs.cloud_cut_at_cap && self.prefs.routing != "local_only" {
            self.prefs.routing = "local_only".into();
            save_preferences(&self.prefs);
            let msg = if fr {
                format!(
                    "Plafond cloud atteint (≈{}) — routage local_only forcé",
                    format_cents(self.billing.cents, true),
                )
            } else {
                format!(
                    "Cloud cap reached (≈{}) — routing forced to local_only",
                    format_cents(self.billing.cents, false),
                )
            };
            self.push_status(msg.clone());
            self.toasts.push_error(msg);
        }
    }

    /// Segment status bar : `Cloud ≈X €`, couleur au seuil, hover détaillé.
    pub(crate) fn ui_billing_segment(&mut self, ui: &mut egui::Ui) {
        let fr = self.prefs.language == "fr";
        let cents = self.billing.cents;
        let cap = self.prefs.cloud_cap_cents;
        let label = format!("Cloud ≈{}", format_cents(cents, fr));
        let theme_c = crate::theme::button_colors(ui);
        let tip = {
            let mut top: Vec<(&String, &u64)> = self.billing.by_model.iter().collect();
            top.sort_by(|a, b| b.1.cmp(a.1));
            let mut lines: Vec<String> = top
                .iter()
                .take(3)
                .map(|(m, c)| format!("{m}: {}", format_cents(**c, fr)))
                .collect();
            lines.push(if fr {
                format!(
                    "Mois {} · {} turns · estimation chat (caractères/4, tarifs indicatifs) — agents exclus (phase 2)",
                    self.billing.month, self.billing.turns,
                )
            } else {
                format!(
                    "Month {} · {} turns · chat estimate (chars/4, indicative rates) — agents excluded (phase 2)",
                    self.billing.month, self.billing.turns,
                )
            });
            lines.join("\n")
        };
        if cap > 0 && cents >= cap as u64 {
            ui.colored_label(theme_c.danger, &label).on_hover_text(tip);
        } else if cap > 0 && cents * 100 >= cap as u64 * self.prefs.cloud_alert_pct.max(1) as u64 {
            ui.colored_label(theme_c.warning, &label).on_hover_text(tip);
        } else {
            ui.weak(&label).on_hover_text(tip);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mini_variants_match_before_full() {
        assert!((rate_for_model("gpt-4o-mini").out_cents_per_1m - 60.0).abs() < 1e-9);
        assert!((rate_for_model("gpt-4o-2024-11-20").out_cents_per_1m - 1000.0).abs() < 1e-9);
        assert!((rate_for_model("o1-mini").out_cents_per_1m - 1200.0).abs() < 1e-9);
        assert!(!rate_for_model("some-future-model-99").known);
    }

    #[test]
    fn loopback_hosts_are_free() {
        assert!(endpoint_is_loopback("http://localhost:11434"));
        assert!(endpoint_is_loopback("http://127.0.0.1:11434/v1"));
        assert!(endpoint_is_loopback("http://[::1]:11434"));
        assert!(!endpoint_is_loopback("https://api.openai.com/v1"));
        assert!(!endpoint_is_loopback("https://192.168.1.10:8000/v1"));
    }

    #[test]
    fn split_provider_model_parses_and_rejects() {
        assert_eq!(
            split_provider_model("provider:openai:gpt-4o-mini"),
            Some(("openai", "gpt-4o-mini"))
        );
        assert_eq!(split_provider_model("local:qwen"), None);
        assert_eq!(split_provider_model("provider::x"), None);
    }

    #[test]
    fn turn_cost_rounds_up() {
        let rate = rate_for_model("gpt-4o-mini"); // 15/60 per 1M
                                                  // 4000 prompt chars ≈ 1000 tok → 0.015¢ → 1¢.
        assert_eq!(cents_for_turn(4000, 0, &rate), 1);
        assert_eq!(cents_for_turn(0, 0, &rate), 0);
    }

    #[test]
    fn thresholds_and_rollover() {
        assert_eq!(thresholds_due(0, 0, 80, false), (false, false));
        assert_eq!(thresholds_due(80, 100, 80, false), (true, false));
        assert_eq!(thresholds_due(80, 100, 80, true), (false, false));
        assert_eq!(thresholds_due(100, 100, 80, true), (false, true));
        let mut ledger = BillingLedger {
            month: "2000-01".into(),
            cents: 50,
            alerted: true,
            by_model: HashMap::new(),
            turns: 3,
        };
        rollover_ledger(&mut ledger);
        assert_eq!(ledger.cents, 0);
        assert!(!ledger.alerted);
        assert_ne!(ledger.month, "2000-01");
    }

    #[test]
    fn month_key_is_calendar_shaped() {
        let key = current_month_key();
        assert_eq!(key.len(), 7);
        assert_eq!(&key[4..5], "-");
    }
}
