//! Trust Manager (§4.7, F-AGT-09) : score de confiance par agent, paliers,
//! gouvernance utilisateur. Service **consultatif** — le Policy Engine reste
//! l'autorité finale.

use aos_proto::TrustProfile;
use std::collections::HashMap;

/// Paliers de confiance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Low,
    Medium,
    High,
}

impl Tier {
    pub fn of(score: f32) -> Self {
        if score >= 0.7 {
            Tier::High
        } else if score >= 0.4 {
            Tier::Medium
        } else {
            Tier::Low
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Tier::Low => "low",
            Tier::Medium => "medium",
            Tier::High => "high",
        }
    }
}

/// Profil interne (facteurs bruts).
#[derive(Debug, Clone, Default)]
struct Factors {
    score: f32,
    success: u64,
    failure: u64,
    overrides: u64,
    confirmation_denials: u64,
    /// Score figé manuellement (gouvernance).
    pinned: bool,
}

/// Le Trust Manager.
#[derive(Default)]
pub struct TrustManager {
    profiles: HashMap<String, Factors>,
}

impl TrustManager {
    pub fn new() -> Self {
        Self {
            profiles: HashMap::new(),
        }
    }

    fn compute(f: &Factors) -> f32 {
        if f.pinned {
            return f.score.clamp(0.0, 1.0);
        }
        let total = f.success + f.failure;
        let success_rate = if total > 0 {
            f.success as f32 / total as f32
        } else {
            0.5 // pas d'historique → neutre
        };
        // Fonction pondérée (§4.7) : succès ↑, échecs/annulations/dénis ↓.
        let penalty = 0.15 * f.overrides as f32 + 0.25 * f.confirmation_denials as f32;
        (success_rate - penalty / (total.max(1) as f32 + 10.0)).clamp(0.0, 1.0)
    }

    /// Événement observé (depuis le journal d'audit).
    pub fn record_success(&mut self, agent_id: &str) {
        let f = self.profiles.entry(agent_id.into()).or_default();
        f.success += 1;
        f.score = Self::compute(f);
    }

    pub fn record_failure(&mut self, agent_id: &str) {
        let f = self.profiles.entry(agent_id.into()).or_default();
        f.failure += 1;
        f.score = Self::compute(f);
    }

    pub fn record_override(&mut self, agent_id: &str) {
        let f = self.profiles.entry(agent_id.into()).or_default();
        f.overrides += 1;
        f.score = Self::compute(f);
    }

    pub fn record_confirmation_denial(&mut self, agent_id: &str) {
        let f = self.profiles.entry(agent_id.into()).or_default();
        f.confirmation_denials += 1;
        f.score = Self::compute(f);
    }

    /// Fixe/fige un score (gouvernance utilisateur, §4.7).
    pub fn set(&mut self, agent_id: &str, score: f32) {
        let f = self.profiles.entry(agent_id.into()).or_default();
        f.score = score.clamp(0.0, 1.0);
        f.pinned = true;
    }

    /// Remise à zéro (gouvernance).
    pub fn reset(&mut self, agent_id: &str) {
        self.profiles.remove(agent_id);
    }

    pub fn score(&self, agent_id: &str) -> f32 {
        self.profiles
            .get(agent_id)
            .map(Self::compute)
            .unwrap_or(0.5)
    }

    pub fn tier(&self, agent_id: &str) -> Tier {
        Tier::of(self.score(agent_id))
    }

    pub fn profile(&self, agent_id: &str) -> TrustProfile {
        let f = self.profiles.get(agent_id).cloned().unwrap_or_default();
        let score = self.score(agent_id);
        TrustProfile {
            agent_id: agent_id.into(),
            score,
            tier: Tier::of(score).as_str().into(),
            success_count: f.success,
            failure_count: f.failure,
            override_count: f.overrides,
            confirmation_denials: f.confirmation_denials,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paliers_et_evolution() {
        let mut tm = TrustManager::new();
        assert_eq!(tm.tier("a"), Tier::Medium); // neutre sans historique
        for _ in 0..10 {
            tm.record_success("a");
        }
        assert_eq!(tm.tier("a"), Tier::High);
        for _ in 0..20 {
            tm.record_failure("a");
        }
        assert_eq!(tm.tier("a"), Tier::Low);
        tm.set("a", 0.95);
        assert_eq!(tm.tier("a"), Tier::High);
        tm.reset("a");
        assert_eq!(tm.tier("a"), Tier::Medium);
    }

    #[test]
    fn les_denis_de_confirmation_penalisent() {
        let mut tm = TrustManager::new();
        for _ in 0..5 {
            tm.record_success("b");
        }
        let before = tm.score("b");
        tm.record_confirmation_denial("b");
        tm.record_confirmation_denial("b");
        assert!(tm.score("b") < before);
    }
}
