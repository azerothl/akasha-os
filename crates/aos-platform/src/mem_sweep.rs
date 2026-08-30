//! Daily memory sweep — re-extract today's chat sessions, compare, relate (Preview).
//!
//! Walks user-local-day sessions, replays turns through the same classify / dedup /
//! auto-link pipeline as [`crate::extract`], and ensures `similar` edges exist for
//! related facts that would otherwise be skipped as near-duplicates.

use crate::extract::{classify_candidates, should_skip_mem_extract_turn, DEDUP_THRESHOLD};
use crate::memory::{MemoryKind, MemoryStore};
use aos_proto::{
    ChatSessionMessage, ChatSessionMeta, MemExtractOutcomeKind, MemRelation, MemRelationKind,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Seuil cosinus pour auto-link `updates` / `supersedes`.
pub const AUTO_LINK_THRESHOLD: f32 = 0.82;
/// Seuil cosinus pour créer une arête `similar` sans écrire un doublon.
pub const SIMILAR_THRESHOLD: f32 = 0.75;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SweepState {
    pub last_pass_ms: u64,
    pub last_local_day_key: String,
    #[serde(default)]
    pub relations_created: u64,
}

impl SweepState {
    pub fn path_for(memory_dir: &Path) -> PathBuf {
        memory_dir.join("sweep_state.json")
    }

    pub fn load(memory_dir: &Path) -> Self {
        let p = Self::path_for(memory_dir);
        fs_read_json(&p).unwrap_or_default()
    }

    pub fn save(&self, memory_dir: &Path) -> Result<(), String> {
        let p = Self::path_for(memory_dir);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let raw = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&p, raw).map_err(|e| e.to_string())
    }
}

fn fs_read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Bornes UTC (ms) du jour civil local `[start, end)`.
pub fn local_day_bounds_ms(now_ms: u64, offset_minutes: i32) -> (u64, u64) {
    let offset_ms = (offset_minutes as i64) * 60_000;
    let local_ms = now_ms as i64 + offset_ms;
    let day_ms = 86_400_000i64;
    let local_midnight = local_ms - (local_ms % day_ms);
    let start_ms = (local_midnight - offset_ms).max(0) as u64;
    let end_ms = start_ms.saturating_add(86_400_000);
    (start_ms, end_ms)
}

/// Clé stable pour « une passe par jour local ».
pub fn local_day_key(now_ms: u64, offset_minutes: i32) -> String {
    let offset_ms = (offset_minutes as i64) * 60_000;
    let local_ms = now_ms as i64 + offset_ms;
    let day = local_ms.div_euclid(86_400_000);
    format!("day-{day}")
}

/// Décalage fuseau local en minutes (fallback UTC).
pub fn system_tz_offset_minutes() -> i32 {
    if let Ok(out) = std::process::Command::new("date").args(["+%z"]).output() {
        if out.status.success() {
            if let Ok(s) = String::from_utf8(out.stdout) {
                if let Some(mins) = parse_tz_offset_minutes(s.trim()) {
                    return mins;
                }
            }
        }
    }
    0
}

fn parse_tz_offset_minutes(raw: &str) -> Option<i32> {
    let s = raw.trim();
    if s.len() < 3 {
        return None;
    }
    let sign = match s.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let digits: String = s.chars().skip(1).filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 3 {
        return None;
    }
    let hours: i32 = digits[..digits.len().saturating_sub(2)]
        .parse()
        .ok()?;
    let mins: i32 = digits[digits.len().saturating_sub(2)..]
        .parse()
        .ok()?;
    Some(sign * (hours * 60 + mins))
}

/// True si la session a été touchée pendant la fenêtre `[day_start, day_end)`.
pub fn session_active_on_day(
    meta: &ChatSessionMeta,
    messages: &[ChatSessionMessage],
    day_start: u64,
    day_end: u64,
) -> bool {
    if meta.updated_ms >= day_start && meta.updated_ms < day_end {
        return true;
    }
    messages
        .iter()
        .any(|m| m.ts_ms >= day_start && m.ts_ms < day_end)
}

/// Paire `(user, assistant)` pour chaque tour rejouable.
pub fn pair_turns(messages: &[ChatSessionMessage]) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut pending_user: Option<String> = None;
    for m in messages {
        let role = m.role.to_ascii_lowercase();
        if role == "user" || role == "human" {
            pending_user = Some(m.content.clone());
        } else if role == "assistant" {
            if let Some(user) = pending_user.take() {
                if !user.trim().is_empty() || !m.content.trim().is_empty() {
                    pairs.push((user, m.content.clone()));
                }
            }
        }
    }
    pairs
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistFactKind {
    Stored,
    SkippedDuplicate,
}

#[derive(Debug, Clone)]
pub struct PersistFactResult {
    pub kind: PersistFactKind,
    pub id: Option<u64>,
    pub relations: Vec<MemRelation>,
}

/// Compare un fait classifié `Stored` à la mémoire existante, écrit ou skip, relie.
pub fn persist_classified_fact(
    mem: &mut MemoryStore,
    text: &str,
    metadata: serde_json::Value,
    vector: Vec<f32>,
) -> PersistFactResult {
    let text = text.trim().to_string();
    if text.is_empty() {
        return PersistFactResult {
            kind: PersistFactKind::SkippedDuplicate,
            id: None,
            relations: vec![],
        };
    }

    let near = mem.episodic_nearest_cosine(&vector, 3, Some("user:default"));
    let best = near.first();

    if let Some((hit_id, score)) = best {
        if *score >= DEDUP_THRESHOLD {
            let cluster: Vec<aos_proto::MemHit> = near
                .iter()
                .map(|(id, score)| aos_proto::MemHit {
                    id: *id,
                    namespace: "user:default".into(),
                    text: mem.get(*id).map(|e| e.text.clone()).unwrap_or_default(),
                    score: *score,
                    metadata: serde_json::json!({}),
                    pinned: false,
                    kind: None,
                    relations: vec![],
                    superseded: false,
                })
                .collect();
            let relations = link_similar_cluster(mem, &cluster);
            return PersistFactResult {
                kind: PersistFactKind::SkippedDuplicate,
                id: Some(*hit_id),
                relations,
            };
        }
    }

    if let Some((hit_id, score)) = best {
        if *score >= AUTO_LINK_THRESHOLD {
            let (id, auto) = mem.episodic_write_auto_link(
                "user:default",
                &text,
                metadata,
                vector,
                false,
                MemoryKind::Fact,
                AUTO_LINK_THRESHOLD,
            );
            let cluster: Vec<aos_proto::MemHit> = near
                .iter()
                .map(|(id, score)| aos_proto::MemHit {
                    id: *id,
                    namespace: "user:default".into(),
                    text: mem.get(*id).map(|e| e.text.clone()).unwrap_or_default(),
                    score: *score,
                    metadata: serde_json::json!({}),
                    pinned: false,
                    kind: None,
                    relations: vec![],
                    superseded: false,
                })
                .collect();
            let mut relations = auto;
            relations.extend(link_similar_cluster(mem, &cluster));
            return PersistFactResult {
                kind: PersistFactKind::Stored,
                id: Some(id),
                relations,
            };
        }
        if *score >= SIMILAR_THRESHOLD {
            let id = mem.episodic_write_kind(
                "user:default",
                &text,
                metadata,
                vector,
                false,
                MemoryKind::Fact,
            );
            let mut relations = Vec::new();
            if let Ok(edge) = mem.relate(id, MemRelationKind::Similar, *hit_id) {
                relations.push(edge);
            }
            let cluster: Vec<aos_proto::MemHit> = near
                .iter()
                .map(|(id, score)| aos_proto::MemHit {
                    id: *id,
                    namespace: "user:default".into(),
                    text: mem.get(*id).map(|e| e.text.clone()).unwrap_or_default(),
                    score: *score,
                    metadata: serde_json::json!({}),
                    pinned: false,
                    kind: None,
                    relations: vec![],
                    superseded: false,
                })
                .collect();
            relations.extend(link_similar_cluster(mem, &cluster));
            return PersistFactResult {
                kind: PersistFactKind::Stored,
                id: Some(id),
                relations,
            };
        }
    }

    let id = mem.episodic_write_kind(
        "user:default",
        &text,
        metadata,
        vector,
        false,
        MemoryKind::Fact,
    );
    PersistFactResult {
        kind: PersistFactKind::Stored,
        id: Some(id),
        relations: vec![],
    }
}

/// Relie par `similar` les hits proches entre eux (évite les skip muets).
fn link_similar_cluster(
    mem: &mut MemoryStore,
    hits: &[aos_proto::MemHit],
) -> Vec<MemRelation> {
    let mut out = Vec::new();
    let ids: Vec<u64> = hits
        .iter()
        .filter(|h| h.score >= SIMILAR_THRESHOLD)
        .map(|h| h.id)
        .collect();
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let a = ids[i];
            let b = ids[j];
            if let Ok(edge) = mem.relate(a, MemRelationKind::Similar, b) {
                out.push(edge);
            }
        }
    }
    out
}

/// Filtre + persiste un candidat texte (sans LLM) — utilisé par le sweep et les tests.
pub fn persist_candidate_text(
    mem: &mut MemoryStore,
    text: &str,
    metadata: serde_json::Value,
    embed: impl Fn(&str) -> Vec<f32>,
) -> Option<PersistFactResult> {
    let classified = classify_candidates(&[aos_proto::MemExtractedFact {
        text: text.into(),
        supersedes_hint: None,
    }]);
    let outcome = classified.into_iter().next()?;
    if outcome.kind != MemExtractOutcomeKind::Stored {
        return None;
    }
    let vector = embed(text);
    Some(persist_classified_fact(mem, text, metadata, vector))
}

/// True si le tour doit être ignoré par le sweep (mêmes règles que mem.extract).
pub fn should_skip_sweep_turn(user_text: &str, assistant_text: &str) -> bool {
    if should_skip_mem_extract_turn(user_text) {
        return true;
    }
    user_text.trim().is_empty() && assistant_text.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryStore;
    use aos_proto::MemExtractedFact;

    fn v(x: f32) -> Vec<f32> {
        vec![x, 1.0 - x, 0.5]
    }

    fn store() -> (MemoryStore, std::path::PathBuf) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("aos-sweep-{}-{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&dir);
        (MemoryStore::open(&dir).unwrap(), dir)
    }

    #[test]
    fn local_day_bounds_respects_offset() {
        let offset = 120; // UTC+2
        let noon_local = 1_000_000_000_000u64 - (12 * 3_600_000) as u64;
        let (start, end) = local_day_bounds_ms(noon_local, offset);
        assert!(end > start);
        assert_eq!(end - start, 86_400_000);
        assert!(noon_local >= start && noon_local < end);
    }

    #[test]
    fn session_active_on_day_by_message_ts() {
        let meta = ChatSessionMeta {
            id: "s".into(),
            title: "t".into(),
            created_ms: 0,
            updated_ms: 100,
            archived: false,
            message_count: 1,
            model_id: None,
            mode: aos_proto::ChatSessionMode::Direct,
            members: vec![],
            conductor_policy: aos_proto::ChatRoomConductorPolicy::default(),
            canvas_open: false,
            canvas_aspect: aos_proto::CanvasAspect::default(),
        };
        let msgs = vec![ChatSessionMessage {
            role: "user".into(),
            content: "hi".into(),
            ts_ms: 50_000,
            attachments: vec![],
            speaker_id: None,
            speaker_name: None,
                        thinking: None,
                    }];
        assert!(session_active_on_day(&meta, &msgs, 40_000, 60_000));
        assert!(!session_active_on_day(&meta, &msgs, 60_000, 80_000));
    }

    #[test]
    fn pair_turns_user_assistant() {
        let msgs = vec![
            ChatSessionMessage {
                role: "user".into(),
                content: "Je m'appelle Alice".into(),
                ts_ms: 1,
                attachments: vec![],
                speaker_id: None,
                speaker_name: None,
                        thinking: None,
                    },
            ChatSessionMessage {
                role: "assistant".into(),
                content: "Bonjour Alice".into(),
                ts_ms: 2,
                attachments: vec![],
                speaker_id: None,
                speaker_name: None,
                        thinking: None,
                    },
        ];
        let pairs = pair_turns(&msgs);
        assert_eq!(pairs.len(), 1);
        assert!(pairs[0].0.contains("Alice"));
    }

    #[test]
    fn persist_skips_ephemeral_canvas_fact() {
        let (mut mem, dir) = store();
        let r = persist_candidate_text(
            &mut mem,
            "L'utilisateur veut dessiner une maison sur le canvas",
            serde_json::json!({}),
            |_| v(0.5),
        );
        assert!(r.is_none());
        assert_eq!(mem.episodic_len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn persist_stores_new_fact() {
        let (mut mem, dir) = store();
        let r = persist_candidate_text(
            &mut mem,
            "L'utilisateur s'appelle Alice",
            serde_json::json!({"source": "sweep"}),
            |_| v(0.4),
        )
        .unwrap();
        assert_eq!(r.kind, PersistFactKind::Stored);
        assert!(r.id.is_some());
        assert_eq!(mem.episodic_len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn persist_skips_duplicate_and_links_similar() {
        let (mut mem, dir) = store();
        let id_a = mem.episodic_write(
            "user:default",
            "L'utilisateur préfère le français",
            serde_json::json!({}),
            v(0.90),
            false,
        );
        let id_b = mem.episodic_write(
            "user:default",
            "L'utilisateur aime la cuisine italienne",
            serde_json::json!({}),
            v(0.89),
            false,
        );
        let r = persist_classified_fact(
            &mut mem,
            "L'utilisateur préfère le français",
            serde_json::json!({}),
            v(0.901),
        );
        assert_eq!(r.kind, PersistFactKind::SkippedDuplicate);
        assert_eq!(r.id, Some(id_a));
        assert!(mem.relations().iter().any(|rel| {
            rel.rel == MemRelationKind::Similar
                && ((rel.from == id_a && rel.to == id_b) || (rel.from == id_b && rel.to == id_a))
        }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn persist_creates_similar_edge_in_mid_band() {
        let (mut mem, dir) = store();
        let anchor = mem.episodic_write(
            "user:default",
            "L'utilisateur vit à Paris",
            serde_json::json!({}),
            vec![1.0, 0.0, 0.0],
            false,
        );
        let r = persist_classified_fact(
            &mut mem,
            "L'utilisateur habite Paris",
            serde_json::json!({}),
            vec![0.8, 0.6, 0.0],
        );
        assert_eq!(r.kind, PersistFactKind::Stored);
        assert!(r.relations.iter().any(|rel| {
            rel.rel == MemRelationKind::Similar && rel.to == anchor
        }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sweep_state_roundtrip() {
        let dir = std::env::temp_dir().join(format!("aos-sweep-state-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let state = SweepState {
            last_pass_ms: 42,
            last_local_day_key: "day-1".into(),
            relations_created: 3,
        };
        state.save(&dir).unwrap();
        let loaded = SweepState::load(&dir);
        assert_eq!(loaded, state);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_tz_offset() {
        assert_eq!(parse_tz_offset_minutes("+0200"), Some(120));
        assert_eq!(parse_tz_offset_minutes("-0530"), Some(-330));
    }
}
