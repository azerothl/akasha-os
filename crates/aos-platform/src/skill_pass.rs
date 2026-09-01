//! Nightly skill-pattern pass (Preview 0.15) — scan recent chats, persist candidates,
//! surface at most one human card in the Chat thread the next morning.

use crate::extract::should_skip_mem_extract_turn;
use crate::skill::{SkillError, SkillStore};
use aos_proto::{ChatSessionMessage, ChatSessionMeta, SkillCreateRequest};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Minimum repeated user asks before suggesting a skill.
pub const MIN_PATTERN_HITS: usize = 3;
/// Night window [start, end) in local hours — one pass per calendar day.
pub const NIGHT_PASS_HOUR_START: i32 = 2;
pub const NIGHT_PASS_HOUR_END: i32 = 4;
/// Earliest local hour to surface the morning card.
pub const MORNING_SURFACE_HOUR: i32 = 5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillPassDismissRecord {
    pub pattern_id: String,
    pub local_day_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillPassCandidate {
    pub pattern_id: String,
    pub label_en: String,
    pub label_fr: String,
    pub skill_name: String,
    pub description: String,
    #[serde(default)]
    pub when_to_use: String,
    /// Draft instructions — never shown in chat; used only on explicit Create.
    pub body: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub hit_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SkillPassState {
    pub last_pass_ms: u64,
    pub last_pass_local_day_key: String,
    #[serde(default)]
    pub pending: Option<SkillPassCandidate>,
    #[serde(default)]
    pub dismissed: Option<SkillPassDismissRecord>,
    #[serde(default)]
    pub created_pattern_ids: Vec<String>,
}

impl SkillPassState {
    pub fn path_for(skills_dir: &Path) -> PathBuf {
        skills_dir.join("skill_pass_state.json")
    }

    pub fn load(skills_dir: &Path) -> Self {
        let p = Self::path_for(skills_dir);
        fs_read_json(&p).unwrap_or_default()
    }

    pub fn save(&self, skills_dir: &Path) -> Result<(), String> {
        let p = Self::path_for(skills_dir);
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

/// Local civil hour 0–23.
pub fn local_hour(now_ms: u64, offset_minutes: i32) -> i32 {
    let offset_ms = (offset_minutes as i64) * 60_000;
    let local_ms = now_ms as i64 + offset_ms;
    let day_ms = 86_400_000i64;
    let since_midnight = local_ms.rem_euclid(day_ms);
    (since_midnight / 3_600_000) as i32
}

pub fn local_day_key(now_ms: u64, offset_minutes: i32) -> String {
    let offset_ms = (offset_minutes as i64) * 60_000;
    let local_ms = now_ms as i64 + offset_ms;
    let day = local_ms.div_euclid(86_400_000);
    format!("day-{day}")
}

/// True during the nightly analysis window (once per local day).
pub fn in_night_pass_window(now_ms: u64, offset_minutes: i32) -> bool {
    let hour = local_hour(now_ms, offset_minutes);
    (NIGHT_PASS_HOUR_START..NIGHT_PASS_HOUR_END).contains(&hour)
}

/// True once local morning has started (card may surface).
pub fn past_morning_surface_hour(now_ms: u64, offset_minutes: i32) -> bool {
    local_hour(now_ms, offset_minutes) >= MORNING_SURFACE_HOUR
}

/// Collect user messages from sessions active within `[since_ms, now_ms)`.
pub fn collect_user_messages(
    sessions: &[(ChatSessionMeta, Vec<ChatSessionMessage>)],
    since_ms: u64,
    now_ms: u64,
) -> Vec<String> {
    let mut out = Vec::new();
    for (meta, messages) in sessions {
        if meta.updated_ms < since_ms && !messages.iter().any(|m| m.ts_ms >= since_ms) {
            continue;
        }
        for m in messages {
            if m.ts_ms < since_ms || m.ts_ms >= now_ms {
                continue;
            }
            let role = m.role.to_ascii_lowercase();
            if role != "user" && role != "human" {
                continue;
            }
            let text = m.content.trim();
            if should_skip_user_message(text) {
                continue;
            }
            out.push(text.to_string());
        }
    }
    out
}

fn should_skip_user_message(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() || t.len() < 8 {
        return true;
    }
    if t.starts_with('/') {
        return true;
    }
    if should_skip_mem_extract_turn(t) {
        return true;
    }
    false
}

const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "to", "for", "of", "in", "on", "at", "is", "are", "was",
    "were", "be", "been", "it", "this", "that", "with", "from", "as", "by", "i", "me", "my",
    "you", "your", "we", "our", "they", "their", "he", "she", "his", "her", "do", "does", "did",
    "can", "could", "would", "should", "will", "just", "please", "thanks", "thank", "hi",
    "hello", "hey", "ok", "okay", "yes", "no", "le", "la", "les", "un", "une", "des", "de",
    "du", "et", "ou", "pour", "dans", "sur", "avec", "est", "sont", "je", "tu", "il", "elle",
    "nous", "vous", "ils", "elles", "mon", "ma", "mes", "ton", "ta", "tes", "ce", "cette",
    "ces", "qui", "que", "quoi", "comment", "peux", "peut", "faire", "fait", "merci", "bonjour",
    "salut", "svp", "stp",
];

fn tokenize(text: &str) -> HashSet<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .filter(|w| !STOP_WORDS.contains(w))
        .map(|w| w.to_string())
        .collect()
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count() as f32;
    let union = a.union(b).count() as f32;
    inter / union
}

#[derive(Debug, Clone)]
struct MessageCluster {
    messages: Vec<String>,
    tokens: HashSet<String>,
}

fn domain_key(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    if lower.contains("météo") || lower.contains("meteo") || lower.contains("weather") {
        return Some("weather".into());
    }
    if lower.contains("calcul")
        || lower.contains("math")
        || lower.contains("compute")
        || lower.contains("equation")
    {
        return Some("calculations".into());
    }
    if lower.contains("note") {
        return Some("notes".into());
    }
    if lower.contains("task") || lower.contains("tâche") || lower.contains("tache") {
        return Some("tasks".into());
    }
    None
}

fn cluster_messages(messages: &[String], min_hits: usize) -> Vec<MessageCluster> {
    let mut domain_buckets: HashMap<String, Vec<String>> = HashMap::new();
    let mut generic: Vec<String> = Vec::new();
    for msg in messages {
        if let Some(key) = domain_key(msg) {
            domain_buckets.entry(key).or_default().push(msg.clone());
        } else {
            generic.push(msg.clone());
        }
    }
    let mut clusters: Vec<MessageCluster> = domain_buckets
        .into_values()
        .filter(|msgs| msgs.len() >= min_hits)
        .map(|messages| {
            let tokens = messages.iter().flat_map(|m| tokenize(m)).collect();
            MessageCluster { messages, tokens }
        })
        .collect();

    let mut token_clusters: Vec<MessageCluster> = Vec::new();
    for msg in generic {
        let tokens = tokenize(&msg);
        if tokens.len() < 2 {
            continue;
        }
        let mut best_idx = None;
        let mut best_score = 0.0f32;
        for (i, cluster) in token_clusters.iter().enumerate() {
            let score = jaccard(&tokens, &cluster.tokens);
            if score > best_score {
                best_score = score;
                best_idx = Some(i);
            }
        }
        if let Some(i) = best_idx {
            if best_score >= 0.35 {
                token_clusters[i].messages.push(msg);
                token_clusters[i].tokens.extend(tokens);
                continue;
            }
        }
        token_clusters.push(MessageCluster {
            messages: vec![msg],
            tokens,
        });
    }
    clusters.extend(
        token_clusters
            .into_iter()
            .filter(|c| c.messages.len() >= min_hits),
    );
    clusters
}

fn looks_french(texts: &[String]) -> bool {
    let fr_markers = [
        " météo", " calcul", " bonjour", " merci", " pourquoi", " comment", " quelle", " quels",
        " une ", " des ", " dans ", " avec ", " peux", " puis", " créer", " génère", " météo",
    ];
    let mut fr = 0usize;
    for t in texts {
        let lower = t.to_ascii_lowercase();
        if fr_markers.iter().any(|m| lower.contains(m)) {
            fr += 1;
        }
    }
    fr * 2 >= texts.len().max(1)
}

fn infer_labels(messages: &[String]) -> (String, String) {
    let joined = messages.join(" ").to_ascii_lowercase();
    if joined.contains("météo")
        || joined.contains("meteo")
        || joined.contains("weather")
        || joined.contains("forecast")
    {
        return ("weather".into(), "météo".into());
    }
    if joined.contains("calcul")
        || joined.contains("math")
        || joined.contains("compute")
        || joined.contains("equation")
        || joined.contains("arithm")
    {
        return ("calculations".into(), "calculs".into());
    }
    if joined.contains("note") || joined.contains("notes") {
        return ("notes".into(), "notes".into());
    }
    if joined.contains("task") || joined.contains("tâche") || joined.contains("tache") {
        return ("tasks".into(), "tâches".into());
    }
    // Fallback: most frequent significant token.
    let mut freq: HashMap<String, usize> = HashMap::new();
    for msg in messages {
        for tok in tokenize(msg) {
            *freq.entry(tok).or_default() += 1;
        }
    }
    let mut ranked: Vec<_> = freq.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let word = ranked
        .first()
        .map(|(w, _)| w.as_str())
        .unwrap_or("requests");
    let en = word.to_string();
    let fr = if looks_french(messages) {
        match word {
            "weather" => "météo".into(),
            "calculation" | "calculations" => "calculs".into(),
            other => other.to_string(),
        }
    } else {
        en.clone()
    };
    (en, fr)
}

fn slugify_label(label_en: &str) -> String {
    let mut out = String::from("user-");
    for ch in label_en.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if (ch.is_whitespace() || ch == '-' || ch == '_') && !out.ends_with('-') {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.len() <= "user-".len() {
        out.push_str("pattern");
    }
    out.truncate(33);
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn infer_tools(messages: &[String]) -> Vec<String> {
    let joined = messages.join(" ").to_ascii_lowercase();
    let mut tools = Vec::new();
    if joined.contains("note") {
        tools.push("notes.list".into());
        tools.push("notes.create".into());
    }
    if joined.contains("task") || joined.contains("tâche") || joined.contains("tache") {
        tools.push("tasks.list".into());
        tools.push("tasks.create".into());
    }
    if joined.contains("search") || joined.contains("recherche") || joined.contains("web") {
        tools.push("web.search".into());
    }
    tools.sort();
    tools.dedup();
    tools
}

fn build_candidate(cluster: &MessageCluster) -> SkillPassCandidate {
    let (label_en, label_fr) = infer_labels(&cluster.messages);
    let skill_name = slugify_label(&label_en);
    let pattern_id = stable_pattern_id(&cluster.messages);
    let tools = infer_tools(&cluster.messages);
    let examples: Vec<String> = cluster
        .messages
        .iter()
        .take(3)
        .map(|m| format!("- {m}"))
        .collect();
    let description_en = format!("Help with {label_en} requests");
    let body = format!(
        "# {label_en}\n\n\
When the user asks about {label_en}, follow a repeatable workflow.\n\n\
## Goal\n\
Handle recurring {label_en} requests consistently.\n\n\
## Examples from recent chats\n\
{examples}\n\n\
## Steps\n\
1. Confirm what the user needs.\n\
2. Use the listed tools when appropriate.\n\
3. Keep answers concise and actionable.\n",
        examples = examples.join("\n")
    );
    SkillPassCandidate {
        pattern_id,
        label_en: label_en.clone(),
        label_fr: label_fr.clone(),
        skill_name,
        description: description_en,
        when_to_use: format!("When the user asks about {label_en}"),
        body,
        tools,
        hit_count: cluster.messages.len() as u32,
    }
}

fn stable_pattern_id(messages: &[String]) -> String {
    let mut sorted: Vec<_> = messages
        .iter()
        .map(|m| normalize_signature(m))
        .collect();
    sorted.sort();
    sorted.dedup();
    let joined = sorted.join("|");
    format!("pat-{:x}", fnv1a(&joined))
}

fn normalize_signature(text: &str) -> String {
    tokenize(text)
        .into_iter()
        .collect::<Vec<_>>()
        .join(" ")
}

fn fnv1a(s: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for b in s.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Find skill candidates from recent user messages (heuristic, no LLM).
pub fn find_pattern_candidates(messages: &[String], min_hits: usize) -> Vec<SkillPassCandidate> {
    let clusters = cluster_messages(messages, min_hits);
    let mut candidates: Vec<SkillPassCandidate> = clusters.iter().map(build_candidate).collect();
    candidates.sort_by(|a, b| {
        b.hit_count
            .cmp(&a.hit_count)
            .then_with(|| a.label_en.cmp(&b.label_en))
    });
    candidates.dedup_by(|a, b| a.pattern_id == b.pattern_id);
    candidates
}

/// Pick the best candidate not already installed as a user skill.
pub fn pick_best_candidate(
    candidates: &[SkillPassCandidate],
    existing_skill_names: &HashSet<String>,
    created_pattern_ids: &HashSet<String>,
) -> Option<SkillPassCandidate> {
    candidates
        .iter()
        .find(|c| {
            !existing_skill_names.contains(&c.skill_name)
                && !created_pattern_ids.contains(&c.pattern_id)
        })
        .cloned()
}

/// Card copy for the chat thread — human label only, no draft body or analysis.
pub fn surface_card_title(label: &str) -> String {
    label.trim().to_string()
}

pub fn surface_card_mute_line(lang: &str) -> String {
    if lang.starts_with("fr") {
        "Tu demandes souvent ça. Je peux en faire une skill.".to_string()
    } else {
        "You ask for this often. I can turn it into a skill.".to_string()
    }
}

/// Returns the pending offer to surface in chat, if any.
pub fn pending_surface_offer(
    state: &SkillPassState,
    now_ms: u64,
    offset_minutes: i32,
) -> Option<&SkillPassCandidate> {
    let candidate = state.pending.as_ref()?;
    let today = local_day_key(now_ms, offset_minutes);
    if state.last_pass_local_day_key != today {
        return None;
    }
    if !past_morning_surface_hour(now_ms, offset_minutes) {
        return None;
    }
    if state
        .dismissed
        .as_ref()
        .is_some_and(|d| d.local_day_key == today && d.pattern_id == candidate.pattern_id)
    {
        return None;
    }
    if state.created_pattern_ids.contains(&candidate.pattern_id) {
        return None;
    }
    Some(candidate)
}

pub fn dismiss_for_today(
    state: &mut SkillPassState,
    pattern_id: &str,
    now_ms: u64,
    offset_minutes: i32,
) {
    state.dismissed = Some(SkillPassDismissRecord {
        pattern_id: pattern_id.to_string(),
        local_day_key: local_day_key(now_ms, offset_minutes),
    });
}

pub fn mark_created(state: &mut SkillPassState, pattern_id: &str) {
    if !state.created_pattern_ids.contains(&pattern_id.to_string()) {
        state.created_pattern_ids.push(pattern_id.to_string());
    }
    if state
        .pending
        .as_ref()
        .is_some_and(|p| p.pattern_id == pattern_id)
    {
        state.pending = None;
    }
}

pub fn candidate_to_create_request(candidate: &SkillPassCandidate, actor: &str) -> SkillCreateRequest {
    SkillCreateRequest {
        name: candidate.skill_name.clone(),
        description: candidate.description.clone(),
        when_to_use: candidate.when_to_use.clone(),
        tools: candidate.tools.clone(),
        required_caps: vec![],
        body: candidate.body.clone(),
        actor: actor.to_string(),
        actor_caps: vec![],
    }
}

/// Create from a pass candidate, or return the existing skill when already installed.
pub fn create_skill_from_candidate(
    store: &SkillStore,
    candidate: &SkillPassCandidate,
    actor: &str,
) -> Result<aos_proto::SkillInfo, SkillError> {
    let req = candidate_to_create_request(candidate, actor);
    match store.create(&req) {
        Ok(info) => Ok(info),
        Err(SkillError::Exists(name)) => store.describe(&name),
        Err(e) => Err(e),
    }
}

pub fn existing_skill_names(store: &SkillStore) -> HashSet<String> {
    store.list().into_iter().map(|s| s.name).collect()
}

/// Internal analysis summary — must never be posted to chat.
pub fn analysis_summary(candidates: &[SkillPassCandidate]) -> String {
    serde_json::to_string(candidates).unwrap_or_else(|_| "[]".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill::SkillStore;

    fn msgs_weather_fr() -> Vec<String> {
        vec![
            "Quelle est la météo à Paris demain ?".into(),
            "Météo pour Lyon ce week-end".into(),
            "Donne-moi la météo à Marseille".into(),
        ]
    }

    #[test]
    fn find_weather_pattern_cluster() {
        let candidates = find_pattern_candidates(&msgs_weather_fr(), MIN_PATTERN_HITS);
        assert!(!candidates.is_empty());
        let best = &candidates[0];
        assert_eq!(best.label_fr, "météo");
        assert_eq!(best.label_en, "weather");
        assert!(best.hit_count >= 3);
        assert!(!best.body.is_empty());
    }

    #[test]
    fn card_copy_en_fr_no_json() {
        let en = surface_card_mute_line("en");
        let fr = surface_card_mute_line("fr");
        assert_eq!(en, "You ask for this often. I can turn it into a skill.");
        assert_eq!(fr, "Tu demandes souvent ça. Je peux en faire une skill.");
        assert!(!en.contains('{'));
        assert!(!fr.contains('{'));
        assert!(!en.contains("pat-"));
        assert!(!en.contains("workflow"));
    }

    #[test]
    fn later_suppresses_same_morning() {
        let mut state = SkillPassState::default();
        let offset = 0;
        let now = 86_400_000u64 * 2 + 6 * 3_600_000; // 6 AM local
        let candidate = build_candidate(&MessageCluster {
            messages: msgs_weather_fr(),
            tokens: tokenize("météo paris lyon marseille"),
        });
        state.pending = Some(candidate.clone());
        state.last_pass_local_day_key = local_day_key(now, offset);
        assert!(pending_surface_offer(&state, now, offset).is_some());
        dismiss_for_today(&mut state, &candidate.pattern_id, now, offset);
        assert!(pending_surface_offer(&state, now, offset).is_none());
    }

    #[test]
    fn create_skill_from_candidate_idempotent_when_exists() {
        let dir = std::env::temp_dir().join(format!("aos-skill-pass-idem-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = SkillStore::open(&dir).unwrap();
        let candidates = find_pattern_candidates(&msgs_weather_fr(), MIN_PATTERN_HITS);
        let best = pick_best_candidate(
            &candidates,
            &existing_skill_names(&store),
            &HashSet::new(),
        )
        .unwrap();
        let first = create_skill_from_candidate(&store, &best, "human:ui").unwrap();
        assert_eq!(first.name, best.skill_name);
        let again = create_skill_from_candidate(&store, &best, "human:ui").unwrap();
        assert_eq!(again.name, best.skill_name);
        assert_eq!(store.list().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_auto_create_without_user_action() {
        let dir = std::env::temp_dir().join(format!("aos-skill-pass-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = SkillStore::open(&dir).unwrap();
        let candidates = find_pattern_candidates(&msgs_weather_fr(), MIN_PATTERN_HITS);
        let best = pick_best_candidate(
            &candidates,
            &existing_skill_names(&store),
            &HashSet::new(),
        )
        .unwrap();
        assert_eq!(store.list().len(), 0);
        let req = candidate_to_create_request(&best, "human:ui");
        let info = store.create(&req).unwrap();
        assert_eq!(info.name, best.skill_name);
        assert_eq!(store.list().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn night_pass_window_bounds() {
        let offset = 0;
        let midnight = 86_400_000u64 * 1000;
        let three_am = midnight + 3 * 3_600_000;
        assert!(in_night_pass_window(three_am, offset));
        let noon = midnight + 12 * 3_600_000;
        assert!(!in_night_pass_window(noon, offset));
    }

    #[test]
    fn analysis_summary_not_for_chat_display() {
        let candidates = find_pattern_candidates(&msgs_weather_fr(), MIN_PATTERN_HITS);
        let summary = analysis_summary(&candidates);
        assert!(summary.starts_with('['));
        assert!(summary.contains("pattern_id"));
        assert!(summary.contains("body"));
    }

    #[test]
    fn state_roundtrip() {
        let dir = std::env::temp_dir().join(format!("aos-skill-pass-state-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let state = SkillPassState {
            last_pass_ms: 99,
            last_pass_local_day_key: "day-1".into(),
            pending: None,
            dismissed: None,
            created_pattern_ids: vec!["pat-abc".into()],
        };
        state.save(&dir).unwrap();
        assert_eq!(SkillPassState::load(&dir), state);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
