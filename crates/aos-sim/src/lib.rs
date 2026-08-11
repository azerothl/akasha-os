//! # aos-sim — Banc d'essai des scénarios de placement (P0.4)
//!
//! Automatise les 6 scénarios obligatoires de `specs-techniques.md` §17.2 :
//!
//! | # | Scénario | Attente clé |
//! |---|----------|-------------|
//! | S1 | Modèle < VRAM | full GPU |
//! | S2 | VRAM < modèle < RAM | hybride GPU+RAM, pas de disque |
//! | S3 | Modèle > RAM | streaming GPU+RAM+DISK, tok/s ≥ 25 % du full-RAM |
//! | S4 | 2 modèles concurrents | éviction fair/priority |
//! | S5 | latency → memory-saver à chaud | migration sans rechargement |
//! | S6 | Agent haute priorité pendant batch | échelle de pression §3.5.5 |

pub mod scenarios;
pub mod xval;

/// Une vérification atomique d'un scénario.
#[derive(Debug, Clone)]
pub struct Check {
    pub label: String,
    pub passed: bool,
    pub detail: String,
}

impl Check {
    pub fn ok(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            passed: true,
            detail: detail.into(),
        }
    }

    pub fn fail(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            passed: false,
            detail: detail.into(),
        }
    }

    pub fn expect(label: impl Into<String>, cond: bool, detail: impl Into<String>) -> Self {
        if cond {
            Self::ok(label, detail)
        } else {
            Self::fail(label, detail)
        }
    }
}

/// Rapport d'un scénario.
#[derive(Debug)]
pub struct ScenarioReport {
    pub id: &'static str,
    pub title: String,
    pub checks: Vec<Check>,
    /// Lignes d'estimation (tok/s, TTFT, répartition tiers) pour le rapport.
    pub notes: Vec<String>,
}

impl ScenarioReport {
    pub fn new(id: &'static str, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            checks: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn passed(&self) -> bool {
        !self.checks.is_empty() && self.checks.iter().all(|c| c.passed)
    }

    pub fn render(&self) -> String {
        let mut out = format!(
            "[{}] {} — {}\n",
            self.id,
            self.title,
            if self.passed() { "PASS" } else { "FAIL" }
        );
        for c in &self.checks {
            out.push_str(&format!(
                "  {} {} ({})\n",
                if c.passed { "✓" } else { "✗" },
                c.label,
                c.detail
            ));
        }
        for n in &self.notes {
            out.push_str(&format!("  · {n}\n"));
        }
        out
    }
}

/// Exécute les 6 scénarios §17.2.
pub fn run_all() -> Vec<ScenarioReport> {
    vec![
        scenarios::s1_full_gpu(),
        scenarios::s2_hybrid_gpu_ram(),
        scenarios::s3_disk_streaming(),
        scenarios::s4_concurrent_eviction(),
        scenarios::s5_hot_reprofile(),
        scenarios::s6_priority_pressure(),
    ]
}
