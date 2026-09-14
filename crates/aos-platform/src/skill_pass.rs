//! Nightly skill-pattern pass (Preview 0.15) + in-session consider (E22).
//! Scan recent chats, persist candidates, surface at most one human card
//! (morning catch-up or live under context pressure).

use crate::extract::should_skip_mem_extract_turn;
use crate::instincts::{self, InstinctStore};
use crate::skill::{SkillError, SkillStore};
use aos_proto::{ChatSessionMessage, ChatSessionMeta, SkillCreateRequest, SkillPassPendingOffer};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Minimum repeated user asks before suggesting a skill.
pub const MIN_PATTERN_HITS: usize = 3;
/// Night window [start, end) in local hours — catch-up only (E22).
pub const NIGHT_PASS_HOUR_START: i32 = 2;
pub const NIGHT_PASS_HOUR_END: i32 = 4;
/// Earliest local hour to surface the morning (catch-up) card.
pub const MORNING_SURFACE_HOUR: i32 = 5;
/// Soft context-pressure fraction of `prompt_budget` (aligned with aos-agent).
pub const CONTEXT_PRESSURE_FRACTION: f32 = 0.75;

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
    /// Short example user asks shown on the morning card (not the draft body).
    #[serde(default)]
    pub examples: Vec<String>,
    /// E22: surface immediately in the live thread (ignore morning hour).
    #[serde(default)]
    pub surface_now: bool,
    #[serde(default)]
    pub source_session_id: Option<String>,
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
    /// Sessions that already received an in-session offer (anti-spam).
    #[serde(default)]
    pub offered_session_ids: Vec<String>,
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
            if text.starts_with("[steer]") || text.starts_with("[Steer]") {
                let n = normalize_steer_text(text);
                if n.len() >= 8 {
                    out.push(n);
                }
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
    // Keep steers — they are corrections / procedure hints (E22).
    if t.starts_with("[steer]") {
        return false;
    }
    if t.starts_with('/') {
        return true;
    }
    if should_skip_mem_extract_turn(t) {
        return true;
    }
    false
}

/// Normalize a steer line for clustering (strip `[steer]` prefix).
pub fn normalize_steer_text(text: &str) -> String {
    let t = text.trim();
    let stripped = t
        .strip_prefix("[steer]")
        .or_else(|| t.strip_prefix("[Steer]"))
        .unwrap_or(t)
        .trim();
    stripped.to_string()
}

/// Collect messages from a single session (user/human + steers).
pub fn collect_session_messages(
    messages: &[ChatSessionMessage],
    extra_steers: &[String],
) -> Vec<String> {
    let mut out = Vec::new();
    for m in messages {
        let role = m.role.to_ascii_lowercase();
        if role != "user" && role != "human" {
            continue;
        }
        let text = m.content.trim();
        if should_skip_user_message(text) {
            continue;
        }
        if text.starts_with("[steer]") {
            let n = normalize_steer_text(text);
            if n.len() >= 8 {
                out.push(n);
            }
            continue;
        }
        out.push(text.to_string());
    }
    for s in extra_steers {
        let n = normalize_steer_text(s);
        if n.len() >= 8 {
            out.push(n);
        }
    }
    out
}

/// Soft pressure threshold (75% of typical Preview prompt budget).
pub fn soft_pressure_token_threshold() -> usize {
    // Mirror aos_agent::context_budget::DEFAULT_N_CTX_HINT / AGENT_GEN_TOKENS without depending on aos-agent.
    const N_CTX: usize = 9216;
    const GEN: usize = 1536;
    const SAFETY: usize = 64;
    let budget = N_CTX.saturating_sub(GEN + SAFETY);
    ((budget as f32) * CONTEXT_PRESSURE_FRACTION) as usize
}

pub fn estimate_text_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(3).max(1)
}

pub fn estimate_session_tokens(messages: &[ChatSessionMessage]) -> usize {
    let body: usize = messages
        .iter()
        .map(|m| estimate_text_tokens(&m.role) + estimate_text_tokens(&m.content) + 4)
        .sum();
    body.saturating_add(64)
}

const STOP_WORDS: &[&str] = &[
    "a",
    "an",
    "the",
    "and",
    "or",
    "to",
    "for",
    "of",
    "in",
    "on",
    "at",
    "is",
    "are",
    "was",
    "were",
    "be",
    "been",
    "it",
    "this",
    "that",
    "with",
    "from",
    "as",
    "by",
    "i",
    "me",
    "my",
    "you",
    "your",
    "we",
    "our",
    "they",
    "their",
    "he",
    "she",
    "his",
    "her",
    "do",
    "does",
    "did",
    "can",
    "could",
    "would",
    "should",
    "will",
    "just",
    "please",
    "thanks",
    "thank",
    "hi",
    "hello",
    "hey",
    "ok",
    "okay",
    "yes",
    "no",
    "le",
    "la",
    "les",
    "un",
    "une",
    "des",
    "de",
    "du",
    "et",
    "ou",
    "pour",
    "dans",
    "sur",
    "avec",
    "est",
    "sont",
    "je",
    "tu",
    "il",
    "elle",
    "nous",
    "vous",
    "ils",
    "elles",
    "mon",
    "ma",
    "mes",
    "ton",
    "ta",
    "tes",
    "ce",
    "cette",
    "ces",
    "qui",
    "que",
    "quoi",
    "comment",
    "peux",
    "peut",
    "faire",
    "fait",
    "merci",
    "bonjour",
    "salut",
    "svp",
    "stp",
    "create",
    "creates",
    "creating",
    "make",
    "build",
    "generate",
    "write",
    "draft",
    "crée",
    "créer",
    "cree",
    "creer",
    "fais",
    "génère",
    "générer",
    "genere",
    "generer",
    "rédige",
    "rédiger",
    "redige",
    "rediger",
    "dessine",
    "dessiner",
];

const GENERIC_ACTION_LABELS: &[&str] = &[
    "create",
    "creates",
    "creating",
    "make",
    "build",
    "generate",
    "write",
    "draft",
    "crée",
    "créer",
    "cree",
    "creer",
    "fais",
    "génère",
    "générer",
    "genere",
    "generer",
    "rédige",
    "rédiger",
    "redige",
    "rediger",
    "dessine",
    "dessiner",
];

fn tokenize(text: &str) -> HashSet<String> {
    text.to_lowercase()
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
    let lower = text.to_lowercase();
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
    if lower.contains("module") {
        return Some("modules".into());
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
        " météo",
        " calcul",
        " bonjour",
        " merci",
        " pourquoi",
        " comment",
        " quelle",
        " quels",
        " une ",
        " des ",
        " dans ",
        " avec ",
        " peux",
        " puis",
        " créer",
        " génère",
        " météo",
    ];
    let mut fr = 0usize;
    for t in texts {
        let lower = t.to_lowercase();
        if fr_markers.iter().any(|m| lower.contains(m)) {
            fr += 1;
        }
    }
    fr * 2 >= texts.len().max(1)
}

fn infer_labels(messages: &[String]) -> (String, String) {
    let joined = messages.join(" ").to_lowercase();
    let french = looks_french(messages);
    if joined.contains("météo")
        || joined.contains("meteo")
        || joined.contains("weather")
        || joined.contains("forecast")
    {
        return ("weather checks".into(), "consultations météo".into());
    }
    if joined.contains("calcul")
        || joined.contains("math")
        || joined.contains("compute")
        || joined.contains("equation")
        || joined.contains("arithm")
    {
        return ("calculations help".into(), "aide aux calculs".into());
    }
    if joined.contains("note") || joined.contains("notes") {
        return ("note management".into(), "gestion des notes".into());
    }
    if joined.contains("task") || joined.contains("tâche") || joined.contains("tache") {
        return ("task management".into(), "gestion des tâches".into());
    }
    if joined.contains("module") {
        let development = [
            "create",
            "build",
            "develop",
            "code",
            "crée",
            "créer",
            "cree",
            "creer",
            "développ",
            "developp",
        ]
        .iter()
        .any(|marker| joined.contains(marker));
        return if development {
            (
                "module development".into(),
                "développement de modules".into(),
            )
        } else {
            ("module questions".into(), "questions sur les modules".into())
        };
    }
    need_phrase_from_tokens(messages, french)
}

/// Build a short need phrase from top tokens — never a lone opaque word.
fn need_phrase_from_tokens(messages: &[String], french: bool) -> (String, String) {
    let mut freq: HashMap<String, usize> = HashMap::new();
    for msg in messages {
        for tok in tokenize(msg) {
            *freq.entry(tok).or_default() += 1;
        }
    }
    let mut ranked: Vec<_> = freq.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let words: Vec<&str> = ranked
        .iter()
        .take(3)
        .map(|(w, _)| w.as_str())
        .collect();
    if words.is_empty() {
        return (
            "recurring requests".into(),
            "demandes récurrentes".into(),
        );
    }
    if words.len() == 1 {
        let w = words[0];
        let en = format!("{w} requests");
        let fr = if french {
            format!("demandes liées à « {w} »")
        } else {
            en.clone()
        };
        return (en, fr);
    }
    let topic = words.join(" ");
    let en = format!("{topic} requests");
    let fr = if french {
        format!("demandes sur {topic}")
    } else {
        en.clone()
    };
    (en, fr)
}

fn pick_card_examples(messages: &[String], limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for msg in messages {
        let trimmed = msg.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = normalize_signature(trimmed);
        if !seen.insert(key) {
            continue;
        }
        out.push(truncate_example(trimmed, 96));
        if out.len() >= limit {
            break;
        }
    }
    out
}

fn truncate_example(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max_chars.saturating_sub(1)).collect();
    while out.ends_with(|c: char| c.is_whitespace() || c == ',') {
        out.pop();
    }
    out.push('…');
    out
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
    let joined = messages.join(" ").to_lowercase();
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
    if joined.contains("module") {
        tools.push("module.list".into());
        tools.push("module.describe".into());
        if [
            "create",
            "build",
            "develop",
            "code",
            "crée",
            "créer",
            "cree",
            "creer",
            "développ",
            "developp",
        ]
        .iter()
        .any(|marker| joined.contains(marker))
        {
            tools.push("module.scaffold".into());
            tools.push("module.compile".into());
            tools.push("module.package".into());
            tools.push("module.install".into());
        }
        if ["uninstall", "remove", "désinstall", "desinstall", "supprim"]
            .iter()
            .any(|marker| joined.contains(marker))
        {
            tools.push("module.uninstall".into());
        }
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
    let examples = pick_card_examples(&cluster.messages, 2);
    let example_lines: Vec<String> = examples.iter().map(|m| format!("- {m}")).collect();
    let description_en = format!("Reusable help for: {label_en}");
    let body = format!(
        "# {label_en}\n\n\
When the user asks about {label_en}, follow a repeatable workflow.\n\n\
## Goal\n\
Handle recurring {label_en} consistently.\n\n\
## Examples from recent chats\n\
{examples}\n\n\
## Steps\n\
1. Confirm what the user needs.\n\
2. Use the listed tools when appropriate.\n\
3. Keep answers concise and actionable.\n",
        examples = example_lines.join("\n")
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
        examples,
        surface_now: false,
        source_session_id: None,
    }
}

fn stable_pattern_id(messages: &[String]) -> String {
    let mut sorted: Vec<_> = messages.iter().map(|m| normalize_signature(m)).collect();
    sorted.sort();
    sorted.dedup();
    let joined = sorted.join("|");
    format!("pat-{:x}", fnv1a(&joined))
}

fn normalize_signature(text: &str) -> String {
    let mut tokens: Vec<_> = tokenize(text).into_iter().collect();
    tokens.sort();
    tokens.join(" ")
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
            candidate_label_is_actionable(c)
                && !existing_skill_names.contains(&c.skill_name)
                && !created_pattern_ids.contains(&c.pattern_id)
        })
        .cloned()
}

fn candidate_label_is_actionable(candidate: &SkillPassCandidate) -> bool {
    let label = candidate.label_en.trim().to_lowercase();
    if label.is_empty() || GENERIC_ACTION_LABELS.contains(&label.as_str()) {
        return false;
    }
    // Reject opaque single-token topics (e.g. "agentic") — the card must name a need.
    let token_count = label
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .count();
    token_count >= 2
}

/// Card copy for the chat thread — human label only, no draft body or analysis.
pub fn surface_card_title(label: &str) -> String {
    label.trim().to_string()
}

pub fn surface_card_mute_line(lang: &str) -> String {
    if lang.starts_with("fr") {
        "Créer ajoute une recette réutilisable pour ce type de demande. Rien ne s’exécute automatiquement."
            .to_string()
    } else {
        "Creating adds a reusable recipe for this kind of request. Nothing runs automatically."
            .to_string()
    }
}

/// Returns the pending offer to surface in chat, if any.
pub fn pending_surface_offer(
    state: &SkillPassState,
    now_ms: u64,
    offset_minutes: i32,
) -> Option<&SkillPassCandidate> {
    let candidate = state.pending.as_ref()?;
    if !candidate_label_is_actionable(candidate) {
        return None;
    }
    let today = local_day_key(now_ms, offset_minutes);
    if state.last_pass_local_day_key != today && !candidate.surface_now {
        return None;
    }
    // Morning catch-up still waits until 05:00; in-session (surface_now) does not.
    if !candidate.surface_now && !past_morning_surface_hour(now_ms, offset_minutes) {
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

pub fn candidate_to_pending_offer(c: &SkillPassCandidate) -> SkillPassPendingOffer {
    SkillPassPendingOffer {
        pattern_id: c.pattern_id.clone(),
        label_en: c.label_en.clone(),
        label_fr: c.label_fr.clone(),
        hit_count: c.hit_count,
        examples: c.examples.clone(),
        surface_now: c.surface_now,
        source_session_id: c.source_session_id.clone(),
    }
}

/// Result of an in-session consider pass (E22).
#[derive(Debug, Clone)]
pub struct ConsiderResult {
    pub local_day_key: String,
    pub candidates_found: usize,
    pub pending_pattern_id: Option<String>,
    pub skipped_already_offered: bool,
    pub offer: Option<SkillPassPendingOffer>,
    pub instincts_upserted: u32,
}

/// In-session heuristic scan — prefer current session, fall back to 14-day lookback.
#[allow(clippy::too_many_arguments)] // Pass inputs stay explicit across skill-store / session sources.
pub fn run_consider_pass(
    state: &mut SkillPassState,
    skills_dir: &Path,
    skill_store: &SkillStore,
    session_id: &str,
    session_messages: Option<&[ChatSessionMessage]>,
    all_sessions: &[(ChatSessionMeta, Vec<ChatSessionMessage>)],
    extra_steers: &[String],
    now_ms: u64,
    offset_minutes: i32,
) -> Result<ConsiderResult, String> {
    let day_key = local_day_key(now_ms, offset_minutes);
    if state
        .offered_session_ids
        .iter()
        .any(|id| id == session_id)
    {
        return Ok(ConsiderResult {
            local_day_key: day_key,
            candidates_found: 0,
            pending_pattern_id: state.pending.as_ref().map(|c| c.pattern_id.clone()),
            skipped_already_offered: true,
            offer: pending_surface_offer(state, now_ms, offset_minutes)
                .map(candidate_to_pending_offer),
            instincts_upserted: 0,
        });
    }

    let mut messages = if let Some(msgs) = session_messages {
        collect_session_messages(msgs, extra_steers)
    } else {
        collect_session_messages(&[], extra_steers)
    };

    let mut candidates = find_pattern_candidates(&messages, MIN_PATTERN_HITS);
    if candidates.is_empty() {
        let lookback_ms = 14u64 * 86_400_000;
        let since_ms = now_ms.saturating_sub(lookback_ms);
        messages = collect_user_messages(all_sessions, since_ms, now_ms);
        for s in extra_steers {
            let n = normalize_steer_text(s);
            if n.len() >= 8 {
                messages.push(n);
            }
        }
        candidates = find_pattern_candidates(&messages, MIN_PATTERN_HITS);
    }

    let existing = existing_skill_names(skill_store);
    let created: HashSet<String> = state.created_pattern_ids.iter().cloned().collect();
    let mut best = pick_best_candidate(&candidates, &existing, &created);

    let mut instincts_upserted = 0u32;
    if let Some(ref mut cand) = best {
        cand.surface_now = true;
        cand.source_session_id = Some(session_id.to_string());
        let mut store = InstinctStore::load(skills_dir);
        instincts_upserted = instincts::upsert_from_candidate(&mut store, cand, now_ms);
        let _ = store.save(skills_dir);
    }

    // Always mark session offered so we do not re-scan every turn under pressure.
    if !state.offered_session_ids.iter().any(|id| id == session_id) {
        state.offered_session_ids.push(session_id.to_string());
    }

    if best.is_some() {
        state.pending = best.clone();
        state.last_pass_ms = now_ms;
        state.last_pass_local_day_key = day_key.clone();
    }
    state.save(skills_dir)?;

    let offer = pending_surface_offer(state, now_ms, offset_minutes).map(candidate_to_pending_offer);

    Ok(ConsiderResult {
        local_day_key: day_key,
        candidates_found: candidates.len(),
        pending_pattern_id: best.map(|c| c.pattern_id),
        skipped_already_offered: false,
        offer,
        instincts_upserted,
    })
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

/// After Create: mark skill created and promote/remove matching instinct.
pub fn mark_created_and_promote(
    state: &mut SkillPassState,
    skills_dir: &Path,
    pattern_id: &str,
    now_ms: u64,
) {
    mark_created(state, pattern_id);
    let mut store = InstinctStore::load(skills_dir);
    instincts::mark_promoted(&mut store, pattern_id, now_ms);
    let _ = store.save(skills_dir);
}

pub fn candidate_to_create_request(
    candidate: &SkillPassCandidate,
    actor: &str,
) -> SkillCreateRequest {
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
    if !candidate_label_is_actionable(candidate) {
        return Err(SkillError::CandidateTooGeneric(candidate.label_en.clone()));
    }
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
        assert_eq!(best.label_fr, "consultations météo");
        assert_eq!(best.label_en, "weather checks");
        assert!(best.hit_count >= 3);
        assert!(!best.body.is_empty());
        assert!(!best.examples.is_empty());
        assert!(best.examples[0].contains("météo") || best.examples[0].contains("Météo"));
    }

    #[test]
    fn module_creation_requests_do_not_create_a_cree_skill() {
        let messages = vec![
            "crée un module de développement".into(),
            "crée un module de développement".into(),
            "crée un module de code".into(),
        ];
        let candidates = find_pattern_candidates(&messages, MIN_PATTERN_HITS);
        let best = candidates.first().expect("module candidate");
        assert_eq!(best.label_en, "module development");
        assert_eq!(best.label_fr, "développement de modules");
        assert_eq!(best.skill_name, "user-module-development");
        assert!(best.tools.iter().any(|tool| tool == "module.scaffold"));
        assert!(best.tools.iter().any(|tool| tool == "module.compile"));
        assert!(!best.tools.is_empty());
    }

    #[test]
    fn note_offer_names_the_reusable_workflow_not_just_the_object() {
        let messages = vec![
            "Crée une note de réunion".into(),
            "Ajoute une note avec mes idées".into(),
            "Retrouve mes notes de projet".into(),
        ];
        let candidates = find_pattern_candidates(&messages, MIN_PATTERN_HITS);
        let best = candidates.first().expect("note candidate");
        assert_eq!(best.label_en, "note management");
        assert_eq!(best.label_fr, "gestion des notes");
        assert_eq!(best.hit_count, 3);
    }

    #[test]
    fn fallback_label_names_the_need_not_a_lone_token() {
        let messages = vec![
            "what is the state of agentic apps today?".into(),
            "state of agentic apps survey please".into(),
            "agentic apps state of the art overview".into(),
        ];
        let candidates = find_pattern_candidates(&messages, MIN_PATTERN_HITS);
        let best = candidates.first().expect("agentic cluster");
        assert!(
            best.label_en.split_whitespace().count() >= 2,
            "expected multi-word need, got {:?}",
            best.label_en
        );
        assert_ne!(best.label_en, "agentic");
        assert_ne!(best.label_fr, "agentic");
        assert!(
            best.label_en.contains("agentic") || best.label_fr.contains("agentic"),
            "topic should remain visible: en={} fr={}",
            best.label_en,
            best.label_fr
        );
        assert!(!best.examples.is_empty());
        assert!(candidate_label_is_actionable(best));
    }

    #[test]
    fn opaque_single_word_label_is_not_surfaced() {
        let now = 86_400_000u64 * 2 + 6 * 3_600_000;
        let mut candidate = build_candidate(&MessageCluster {
            messages: msgs_weather_fr(),
            tokens: tokenize("météo paris lyon marseille"),
        });
        candidate.label_en = "agentic".into();
        candidate.label_fr = "agentic".into();
        candidate.skill_name = "user-agentic".into();
        let state = SkillPassState {
            last_pass_local_day_key: local_day_key(now, 0),
            pending: Some(candidate),
            ..Default::default()
        };
        assert!(pending_surface_offer(&state, now, 0).is_none());
    }

    #[test]
    fn legacy_generic_action_offer_is_not_surfaced() {
        let now = 86_400_000u64 * 2 + 6 * 3_600_000;
        let mut candidate = build_candidate(&MessageCluster {
            messages: msgs_weather_fr(),
            tokens: tokenize("météo paris lyon marseille"),
        });
        candidate.label_en = "crée".into();
        candidate.label_fr = "crée".into();
        candidate.skill_name = "user-cre".into();
        let state = SkillPassState {
            last_pass_local_day_key: local_day_key(now, 0),
            pending: Some(candidate),
            ..Default::default()
        };
        assert!(pending_surface_offer(&state, now, 0).is_none());

        let dir =
            std::env::temp_dir().join(format!("aos-skill-pass-generic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = SkillStore::open(&dir).unwrap();
        let error =
            create_skill_from_candidate(&store, state.pending.as_ref().unwrap(), "human:ui")
                .unwrap_err();
        assert!(matches!(error, SkillError::CandidateTooGeneric(_)));
        assert!(store.list().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pattern_signature_is_stable_across_word_order() {
        assert_eq!(
            normalize_signature("module code rust"),
            normalize_signature("rust module code")
        );
    }

    #[test]
    fn card_copy_en_fr_no_json() {
        let en = surface_card_mute_line("en");
        let fr = surface_card_mute_line("fr");
        assert_eq!(
            en,
            "Creating adds a reusable recipe for this kind of request. Nothing runs automatically."
        );
        assert_eq!(
            fr,
            "Créer ajoute une recette réutilisable pour ce type de demande. Rien ne s’exécute automatiquement."
        );
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
    fn soft_pressure_threshold_is_seventy_five_percent() {
        const N_CTX: usize = 9216;
        const GEN: usize = 1536;
        const SAFETY: usize = 64;
        let budget = N_CTX.saturating_sub(GEN + SAFETY);
        let expected = ((budget as f32) * CONTEXT_PRESSURE_FRACTION) as usize;
        assert_eq!(soft_pressure_token_threshold(), expected);
        assert!(expected > 5_000);
        assert!(expected < budget);
    }

    #[test]
    fn night_and_live_same_pattern_no_double_pending() {
        let offset = 0;
        let now = 86_400_000u64 * 5 + 10 * 3_600_000; // mid-morning
        let mut candidate = build_candidate(&MessageCluster {
            messages: msgs_weather_fr(),
            tokens: tokenize("météo paris lyon marseille"),
        });
        let pattern_id = candidate.pattern_id.clone();
        candidate.surface_now = true;
        candidate.source_session_id = Some("sess-live".into());
        let mut state = SkillPassState {
            last_pass_local_day_key: local_day_key(now, offset),
            pending: Some(candidate.clone()),
            offered_session_ids: vec!["sess-live".into()],
            ..Default::default()
        };
        assert!(pending_surface_offer(&state, now, offset).is_some());
        // Morning catch-up for the same pattern: already pending / offered — dismiss then night
        // candidate with same id must not re-open after created.
        mark_created(&mut state, &pattern_id);
        assert!(state.pending.is_none());
        assert!(state.created_pattern_ids.contains(&pattern_id));
        let mut night = candidate;
        night.surface_now = false;
        night.source_session_id = None;
        state.pending = Some(night);
        // created_ids blocks surface
        assert!(pending_surface_offer(&state, now, offset).is_none());
    }

    #[test]
    fn surface_now_ignores_morning_hour() {
        let offset = 0;
        // 03:00 local — before MORNING_SURFACE_HOUR
        let now = 86_400_000u64 * 2 + 3 * 3_600_000;
        let mut candidate = build_candidate(&MessageCluster {
            messages: msgs_weather_fr(),
            tokens: tokenize("météo paris lyon marseille"),
        });
        candidate.surface_now = true;
        candidate.source_session_id = Some("sess-live".into());
        let state = SkillPassState {
            last_pass_local_day_key: local_day_key(now, offset),
            pending: Some(candidate),
            ..Default::default()
        };
        assert!(pending_surface_offer(&state, now, offset).is_some());
    }

    #[test]
    fn consider_fire_once_per_session() {
        let dir =
            std::env::temp_dir().join(format!("aos-skill-consider-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = SkillStore::open(&dir).unwrap();
        let mut state = SkillPassState::default();
        let now = 86_400_000u64 * 3 + 14 * 3_600_000;
        let msgs: Vec<ChatSessionMessage> = msgs_weather_fr()
            .into_iter()
            .enumerate()
            .map(|(i, content)| ChatSessionMessage {
                role: "user".into(),
                content,
                ts_ms: now - 1000 + i as u64,
                attachments: vec![],
                speaker_id: None,
                speaker_name: None,
                thinking: None,
            })
            .collect();
        let first = run_consider_pass(
            &mut state,
            &dir,
            &store,
            "sess-a",
            Some(&msgs),
            &[],
            &[],
            now,
            0,
        )
        .unwrap();
        assert!(!first.skipped_already_offered);
        assert!(first.offer.is_some() || first.candidates_found > 0 || first.pending_pattern_id.is_some());
        let second = run_consider_pass(
            &mut state,
            &dir,
            &store,
            "sess-a",
            Some(&msgs),
            &[],
            &[],
            now + 1,
            0,
        )
        .unwrap();
        assert!(second.skipped_already_offered);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn steers_are_collected_for_clustering() {
        let messages = vec![
            ChatSessionMessage {
                role: "user".into(),
                content: "[steer] use hooks not classes for React".into(),
                ts_ms: 1,
                attachments: vec![],
                speaker_id: None,
                speaker_name: None,
                thinking: None,
            },
            ChatSessionMessage {
                role: "user".into(),
                content: "[steer] prefer hooks over class components".into(),
                ts_ms: 2,
                attachments: vec![],
                speaker_id: None,
                speaker_name: None,
                thinking: None,
            },
        ];
        let collected = collect_session_messages(&messages, &["[steer] hooks not classes".into()]);
        assert_eq!(collected.len(), 3);
        assert!(!collected[0].starts_with("[steer]"));
    }

    #[test]
    fn create_skill_from_candidate_idempotent_when_exists() {
        let dir = std::env::temp_dir().join(format!("aos-skill-pass-idem-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = SkillStore::open(&dir).unwrap();
        let candidates = find_pattern_candidates(&msgs_weather_fr(), MIN_PATTERN_HITS);
        let best = pick_best_candidate(&candidates, &existing_skill_names(&store), &HashSet::new())
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
        let best = pick_best_candidate(&candidates, &existing_skill_names(&store), &HashSet::new())
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
            offered_session_ids: vec![],
        };
        state.save(&dir).unwrap();
        assert_eq!(SkillPassState::load(&dir), state);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
