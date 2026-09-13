//! E22 instincts — atomic learned procedures with confidence scoring.
//! Prompt hints only; never auto-write skills; not capabilities.

use crate::skill_pass::SkillPassCandidate;
use aos_proto::InstinctInfo;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MAX_INJECT: usize = 3;
pub const MIN_INJECT_CONFIDENCE: f32 = 0.7;
pub const INITIAL_CONFIDENCE: f32 = 0.55;
pub const CONFIDENCE_BUMP: f32 = 0.1;
pub const CONFIDENCE_DECAY: f32 = 0.1;
pub const PRUNE_BELOW: f32 = 0.3;
/// Drop instincts older than 30 days without evidence.
pub const MAX_AGE_MS: u64 = 30 * 86_400_000;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstinctStore {
    #[serde(default)]
    pub instincts: Vec<InstinctInfo>,
}

impl InstinctStore {
    pub fn path_for(skills_dir: &Path) -> PathBuf {
        skills_dir.join("instincts.json")
    }

    pub fn load(skills_dir: &Path) -> Self {
        let p = Self::path_for(skills_dir);
        let raw = std::fs::read_to_string(p).ok();
        raw.and_then(|r| serde_json::from_str(&r).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, skills_dir: &Path) -> Result<(), String> {
        let p = Self::path_for(skills_dir);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let raw = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&p, raw).map_err(|e| e.to_string())
    }

    pub fn prune(&mut self, now_ms: u64) {
        self.instincts.retain(|i| {
            if i.promoted {
                return false;
            }
            if i.confidence < PRUNE_BELOW {
                return false;
            }
            if now_ms.saturating_sub(i.updated_ms) > MAX_AGE_MS {
                return false;
            }
            true
        });
    }
}

fn instinct_id_from_pattern(pattern_id: &str) -> String {
    format!("inst-{pattern_id}")
}

/// Upsert an instinct from a skill-pass candidate. Returns number of instincts written/bumped.
pub fn upsert_from_candidate(
    store: &mut InstinctStore,
    candidate: &SkillPassCandidate,
    now_ms: u64,
) -> u32 {
    store.prune(now_ms);
    let id = instinct_id_from_pattern(&candidate.pattern_id);
    let trigger = if candidate.when_to_use.trim().is_empty() {
        format!("when handling {}", candidate.label_en)
    } else {
        candidate.when_to_use.clone()
    };
    let action = candidate
        .examples
        .first()
        .map(|e| format!("Prefer the reusable pattern for: {e}"))
        .unwrap_or_else(|| {
            format!(
                "Apply the recurring workflow for {}.",
                candidate.label_en
            )
        });
    let domain = candidate
        .skill_name
        .strip_prefix("user-")
        .unwrap_or(candidate.skill_name.as_str())
        .to_string();

    if let Some(existing) = store.instincts.iter_mut().find(|i| i.id == id) {
        existing.evidence_count = existing.evidence_count.saturating_add(1);
        existing.confidence = (existing.confidence + CONFIDENCE_BUMP).min(0.9);
        existing.trigger = trigger;
        existing.action = action;
        existing.domain = domain;
        existing.source_session_id = candidate.source_session_id.clone();
        existing.updated_ms = now_ms;
        existing.promoted = false;
        return 1;
    }

    store.instincts.push(InstinctInfo {
        id,
        trigger,
        action,
        confidence: INITIAL_CONFIDENCE,
        domain,
        evidence_count: candidate.hit_count.max(1),
        scope: "global".into(),
        source_session_id: candidate.source_session_id.clone(),
        updated_ms: now_ms,
        promoted: false,
    });
    1
}

/// Mark instinct promoted when the user Creates a skill from the matching pattern.
pub fn mark_promoted(store: &mut InstinctStore, pattern_id: &str, now_ms: u64) {
    let id = instinct_id_from_pattern(pattern_id);
    if let Some(i) = store.instincts.iter_mut().find(|i| i.id == id) {
        i.promoted = true;
        i.updated_ms = now_ms;
    }
    store.instincts.retain(|i| !i.promoted);
}

/// Decay confidence when a steer appears to contradict an instinct (simple heuristic).
pub fn decay_on_contradiction(store: &mut InstinctStore, steer_text: &str, now_ms: u64) {
    let lower = steer_text.to_ascii_lowercase();
    let negate = [
        "don't",
        "do not",
        "never",
        "stop",
        "instead",
        "not",
        "ne pas",
        "n'utilise",
        "arrête",
        "plutôt",
    ];
    if !negate.iter().any(|n| lower.contains(n)) {
        return;
    }
    for i in store.instincts.iter_mut() {
        let hay = format!("{} {}", i.trigger, i.action).to_ascii_lowercase();
        // Overlap on a domain token → soft decay.
        let domain = i.domain.to_ascii_lowercase();
        if !domain.is_empty() && hay.contains(domain.as_str()) && lower.contains(domain.as_str()) {
            i.confidence = (i.confidence - CONFIDENCE_DECAY).max(0.0);
            i.updated_ms = now_ms;
        }
    }
    store.prune(now_ms);
}

/// Instincts eligible for prompt injection (confidence floor, not promoted).
pub fn active_for_inject(
    store: &InstinctStore,
    max: usize,
    min_confidence: f32,
) -> Vec<InstinctInfo> {
    let mut list: Vec<_> = store
        .instincts
        .iter()
        .filter(|i| !i.promoted && i.confidence >= min_confidence)
        .cloned()
        .collect();
    list.sort_by(|a, b| match b.confidence.partial_cmp(&a.confidence) {
        Some(ord) => ord.then_with(|| b.evidence_count.cmp(&a.evidence_count)),
        None => b.evidence_count.cmp(&a.evidence_count),
    });
    list.truncate(max);
    list
}

/// Compact block for system / working-memory injection.
pub fn format_for_prompt(instincts: &[InstinctInfo]) -> String {
    if instincts.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "## Instincts (learned preferences — not tools, not capabilities)\n\
         Apply when the trigger matches. Prefer these over repeating long corrections.\n",
    );
    for i in instincts {
        out.push_str(&format!(
            "- When {}: {}\n",
            i.trigger.trim(),
            i.action.trim()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_candidate() -> SkillPassCandidate {
        SkillPassCandidate {
            pattern_id: "weather-checks".into(),
            label_en: "weather checks".into(),
            label_fr: "consultations météo".into(),
            skill_name: "user-weather-checks".into(),
            description: "d".into(),
            when_to_use: "user asks about weather".into(),
            body: "body".into(),
            tools: vec![],
            hit_count: 3,
            examples: vec!["météo Paris".into()],
            surface_now: true,
            source_session_id: Some("sess-1".into()),
        }
    }

    #[test]
    fn upsert_bumps_confidence_and_inject_cap() {
        let mut store = InstinctStore::default();
        let now = 1_000u64;
        assert_eq!(upsert_from_candidate(&mut store, &sample_candidate(), now), 1);
        assert_eq!(store.instincts.len(), 1);
        assert!((store.instincts[0].confidence - INITIAL_CONFIDENCE).abs() < f32::EPSILON);
        upsert_from_candidate(&mut store, &sample_candidate(), now + 1);
        assert_eq!(store.instincts.len(), 1);
        assert!(store.instincts[0].confidence > INITIAL_CONFIDENCE);

        // Force high confidence
        store.instincts[0].confidence = 0.85;
        let active = active_for_inject(&store, MAX_INJECT, MIN_INJECT_CONFIDENCE);
        assert_eq!(active.len(), 1);
        let block = format_for_prompt(&active);
        assert!(block.contains("weather"));
        assert!(!block.contains("capability"));
    }

    #[test]
    fn inject_respects_max_three_and_floor() {
        let mut store = InstinctStore::default();
        for i in 0..5 {
            store.instincts.push(InstinctInfo {
                id: format!("inst-{i}"),
                trigger: format!("t{i}"),
                action: format!("a{i}"),
                confidence: 0.5 + (i as f32) * 0.1,
                domain: "d".into(),
                evidence_count: i,
                scope: "global".into(),
                source_session_id: None,
                updated_ms: 1,
                promoted: false,
            });
        }
        let active = active_for_inject(&store, MAX_INJECT, MIN_INJECT_CONFIDENCE);
        assert!(active.len() <= 3);
        assert!(active.iter().all(|i| i.confidence >= MIN_INJECT_CONFIDENCE));
    }

    #[test]
    fn promote_removes_instinct() {
        let mut store = InstinctStore::default();
        upsert_from_candidate(&mut store, &sample_candidate(), 10);
        mark_promoted(&mut store, "weather-checks", 11);
        assert!(store.instincts.is_empty());
    }
}
