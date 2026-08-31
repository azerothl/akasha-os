//! Budget de prompt avant inférence + trim agressif + garde anti-boucle.

use crate::persist::compact_working_memory;

/// n_ctx typique Preview : `default_kv_tokens` 8192 + padding modeld (+1024).
pub const DEFAULT_N_CTX_HINT: usize = 9216;
/// Génération agent (tours normaux).
pub const AGENT_GEN_TOKENS: u32 = 1536;
/// Génération pour écritures longues (notes / fs / guides).
pub const AGENT_GEN_TOKENS_WRITE: u32 = 2048;
/// Marge template chat + sampler.
pub const GEN_SAFETY_TOKENS: usize = 64;
/// Longueur max d'un brief de sous-agent (caractères).
pub const MAX_SPAWN_BRIEF_CHARS: usize = 600;
/// Retries inférence après PromptTooLong (trim + réduction max_tokens).
pub const MAX_OVERFLOW_INFER_RETRIES: u32 = 2;
/// noop / JSON invalide consécutifs avant fail.
pub const MAX_NOOP_STREAK: u32 = 3;
/// Même action en échec répété avant fail.
pub const MAX_SAME_FAIL_STREAK: u32 = 3;

/// Budget soft dérivé de n_ctx et de la réserve de génération.
pub fn prompt_budget(n_ctx: usize, max_gen: u32) -> usize {
    n_ctx.saturating_sub(max_gen as usize + GEN_SAFETY_TOKENS)
}

/// Budget après overflow (plus serré).
pub fn retry_prompt_budget(n_ctx: usize, max_gen: u32) -> usize {
    (prompt_budget(n_ctx, max_gen) * 6) / 10
}

/// @deprecated alias — préférer `prompt_budget(DEFAULT_N_CTX_HINT, AGENT_GEN_TOKENS)`.
pub const DEFAULT_PROMPT_BUDGET_TOKENS: usize = 7616; // 9216 - 1536 - 64
/// @deprecated alias
pub const RETRY_PROMPT_BUDGET_TOKENS: usize = 4569;

/// Estimation conservative (FR/JSON denser que 4 chars/token).
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(3).max(1)
}

/// Tokens estimés pour la fenêtre working_memory (+ overhead template).
pub fn estimate_messages_tokens(memory: &[(String, String)]) -> usize {
    let body: usize = memory
        .iter()
        .map(|(r, c)| estimate_tokens(r) + estimate_tokens(c) + 4)
        .sum();
    body.saturating_add(64)
}

pub fn is_prompt_too_long_error(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    m.contains("ne tient pas dans le contexte")
        || m.contains("prompttoolong")
        || m.contains("prompt too long")
}

/// Erreur runtime avec détails token (ne pas afficher tel quel à l'utilisateur).
pub fn is_technical_prompt_overflow_message(msg: &str) -> bool {
    is_prompt_too_long_error(msg) || msg.contains("ctx=") || msg.contains("réserve_gen=")
}

/// Erreur vision/mmproj (ne jamais afficher tel quel dans le fil humain).
pub fn is_technical_vision_infer_error(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    m.contains("images fournies")
        || m.contains("mmproj")
        || m.contains("projecteur")
        || m.contains("visionunavailable")
        || m.contains("mtmd")
}

/// `fail_reason` / thread sentinel quand le prompt dépasse le contexte après compaction.
pub fn user_visible_overflow_fail_reason() -> &'static str {
    crate::actions::THREAD_FAIL_COULD_NOT_CONTINUE
}

pub fn is_overflow_fail_reason(reason: &str) -> bool {
    reason == crate::actions::THREAD_FAIL_COULD_NOT_CONTINUE
        || is_technical_prompt_overflow_message(reason)
}

/// Runtime `fail_reason` when the agent exhausts its step budget (never show verbatim).
pub fn is_technical_max_steps_fail_reason(reason: &str) -> bool {
    let lower = reason.to_ascii_lowercase();
    lower.contains("max_steps")
        || (lower.contains("max steps")
            && (lower.contains("atteint") || lower.contains("reached")))
}

/// Compacte silencieusement après PromptTooLong (journal interne via `log_line`).
pub fn compact_after_prompt_overflow(
    memory: &mut Vec<(String, String)>,
    n_ctx: &mut usize,
    max_gen: &mut u32,
    error_msg: &str,
) -> Option<String> {
    *n_ctx = parse_ctx_from_error(error_msg);
    *max_gen = (*max_gen / 2).max(256);
    aggressive_trim_for_overflow(memory, *n_ctx, *max_gen)
}

/// Extrait `ctx=N` du message d'erreur clarifié (sinon hint défaut).
pub fn parse_ctx_from_error(msg: &str) -> usize {
    for part in msg.split([' ', ',', '(', ')']) {
        if let Some(rest) = part.strip_prefix("ctx=") {
            if let Ok(n) = rest.parse::<usize>() {
                if n >= 1024 {
                    return n;
                }
            }
        }
    }
    DEFAULT_N_CTX_HINT
}

/// Choisit max_tokens selon goal / mémoire récente (écritures longues).
pub fn choose_agent_max_tokens(memory: &[(String, String)], goal: &str) -> u32 {
    let g = goal.to_ascii_lowercase();
    let recent: String = memory
        .iter()
        .rev()
        .take(8)
        .map(|(_, c)| c.as_str())
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();
    let hay = format!("{g}\n{recent}");
    let write_hint = [
        "notes.create",
        "notes.update",
        "fs.write",
        "guide",
        "markdown",
        "document",
        "rédige",
        "redige",
        "écrire",
        "ecrire",
        "note",
        "notes",
    ]
    .iter()
    .any(|h| hint_match(&hay, h));
    if write_hint {
        AGENT_GEN_TOKENS_WRITE
    } else {
        AGENT_GEN_TOKENS
    }
}

fn hint_match(hay: &str, hint: &str) -> bool {
    if hint.contains('.') {
        return hay.contains(hint);
    }
    hay.split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|w| w == hint)
}

fn is_injection_system(content: &str) -> bool {
    content.starts_with("[mem.")
        || content.starts_with("[reflect]")
        || content.starts_with("[compaction]")
}

fn truncate_chars(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        format!("{}…", s.chars().take(n).collect::<String>())
    } else {
        s.to_string()
    }
}

/// JSON d'action apparemment tronqué / non parseable.
pub fn looks_like_truncated_action_json(text: &str) -> bool {
    let t = text.trim_start();
    if !(t.starts_with('{') || t.contains("```json")) {
        return false;
    }
    let open = t.chars().filter(|c| *c == '{').count();
    let close = t.chars().filter(|c| *c == '}').count();
    open > close
        || t.contains("\"action\"")
        || t.contains("\"thought\"")
        || t.contains("\"args\"")
}

/// Texte à stocker en working_memory (évite d'empiler des JSON géants tronqués / DSML).
pub fn sanitize_assistant_for_memory(raw: &str, parsed_ok: bool) -> String {
    if raw.trim().is_empty() {
        return "[output sans action JSON — éviter <think>, répondre en JSON]".into();
    }
    if parsed_ok {
        let stripped = crate::actions::strip_tool_markup(raw);
        if stripped.trim().is_empty() {
            return "[action outil exécutée — pas de prose utilisateur]".into();
        }
        return truncate_chars(&stripped, 3500);
    }
    if looks_like_truncated_action_json(raw) {
        return "[output JSON incomplet/tronqué — pour une note longue : \
                notes.create (titre + outline court) puis notes.update section par section \
                (≤ ~1200 car. de content par appel)]"
            .into();
    }
    if crate::actions::looks_like_tool_markup(raw) {
        return "[output tool markup sans action — répondre en JSON structuré \
                {\"thought\":\"…\",\"action\":\"<outil>\",\"args\":{…}}]"
            .into();
    }
    truncate_chars(raw, 3500)
}

#[derive(Clone, Copy)]
struct TrimCaps {
    injection_max: usize,
    tool_max: usize,
    user_max: usize,
    assistant_max: usize,
    primary_system_max: usize,
}

const SOFT_CAPS: TrimCaps = TrimCaps {
    injection_max: 2000,
    tool_max: 1200,
    user_max: 4000,
    assistant_max: 2500,
    primary_system_max: 24_000,
};

const HARD_CAPS: TrimCaps = TrimCaps {
    injection_max: 600,
    tool_max: 400,
    user_max: 1200,
    assistant_max: 800,
    primary_system_max: 8_000,
};

fn trim_oversized_messages(memory: &mut [(String, String)], caps: TrimCaps) -> bool {
    let mut changed = false;
    let mut saw_primary_system = false;
    for (role, content) in memory.iter_mut() {
        let before = content.len();
        match role.as_str() {
            "system" => {
                if is_injection_system(content) {
                    *content = truncate_chars(content, caps.injection_max);
                } else if !saw_primary_system {
                    saw_primary_system = true;
                    *content = truncate_chars(content, caps.primary_system_max);
                } else {
                    *content = truncate_chars(content, caps.injection_max);
                }
            }
            "tool" => *content = truncate_chars(content, caps.tool_max),
            "user" => *content = truncate_chars(content, caps.user_max),
            "assistant" => *content = truncate_chars(content, caps.assistant_max),
            _ => *content = truncate_chars(content, caps.user_max),
        }
        if content.len() != before {
            changed = true;
        }
    }
    changed
}

/// Applique trim + compaction jusqu'à tenir dans `max_prompt_tokens`.
pub fn enforce_prompt_budget(
    memory: &mut Vec<(String, String)>,
    max_prompt_tokens: usize,
    keep_recent: usize,
) -> Option<String> {
    let mut notes: Vec<String> = Vec::new();

    if estimate_messages_tokens(memory) <= max_prompt_tokens {
        return None;
    }

    if trim_oversized_messages(memory, SOFT_CAPS) {
        notes.push("trim soft messages longs".into());
    }
    if estimate_messages_tokens(memory) <= max_prompt_tokens {
        return Some(notes.join("; "));
    }

    if let Some(sum) = compact_working_memory(memory, keep_recent) {
        notes.push(sum);
    }
    if estimate_messages_tokens(memory) <= max_prompt_tokens {
        return Some(notes.join("; "));
    }

    let _ = trim_oversized_messages(memory, HARD_CAPS);
    notes.push("trim hard".into());
    if let Some(sum) = compact_working_memory(memory, keep_recent.min(3)) {
        notes.push(sum);
    }
    if estimate_messages_tokens(memory) <= max_prompt_tokens {
        return Some(notes.join("; "));
    }

    let keep = keep_recent.clamp(1, 2);
    if let Some(sum) = compact_working_memory(memory, keep) {
        notes.push(sum);
    }
    let _ = trim_oversized_messages(memory, HARD_CAPS);
    if !memory.is_empty() && memory[0].0 == "system" && !is_injection_system(&memory[0].1) {
        memory[0].1 = truncate_chars(&memory[0].1, 4_000);
        notes.push("system primaire réduit à 4k chars".into());
    }

    Some(notes.join("; "))
}

pub fn aggressive_trim_for_overflow(
    memory: &mut Vec<(String, String)>,
    n_ctx: usize,
    max_gen: u32,
) -> Option<String> {
    enforce_prompt_budget(memory, retry_prompt_budget(n_ctx, max_gen), 3)
}

pub fn clamp_spawn_brief(brief: &str) -> String {
    let t = brief.trim();
    if t.chars().count() <= MAX_SPAWN_BRIEF_CHARS {
        t.to_string()
    } else {
        truncate_chars(t, MAX_SPAWN_BRIEF_CHARS)
    }
}

/// Garde anti-boucle (noop / même échec).
#[derive(Debug, Default)]
pub struct LoopGuard {
    pub noop_streak: u32,
    pub same_fail_streak: u32,
    last_fail_key: String,
}

/// Structured tool / runtime failure — not a substring hunt in payload text.
pub fn looks_like_tool_failure(tool_result: &str) -> bool {
    let lower = tool_result.to_ascii_lowercase();
    tool_result.contains("aucune action JSON")
        || tool_result.contains("JSON incomplet")
        || tool_result.contains("JSON tronqué")
        || tool_result.contains("ERREUR outil:")
        || tool_result.contains("ERREUR bus:")
        || tool_result.contains("PermissionDenied")
        || tool_result.contains("ActorDenied")
        || tool_result.contains("capacité requise")
        || tool_result.contains("capacité manquante")
        || tool_result.contains("args invalides")
        || lower.contains(" err:")
        || lower.starts_with("err:")
        || lower.starts_with("error:")
}

impl LoopGuard {
    pub fn observe(&mut self, action: &str, tool_result: &str) -> LoopVerdict {
        let is_noop = action == "noop";
        let looks_stuck = is_noop || looks_like_tool_failure(tool_result);

        if is_noop {
            self.noop_streak = self.noop_streak.saturating_add(1);
        } else {
            self.noop_streak = 0;
        }

        if looks_stuck {
            let key = if is_noop {
                "noop".into()
            } else {
                format!("{action}:stuck")
            };
            if key == self.last_fail_key {
                self.same_fail_streak = self.same_fail_streak.saturating_add(1);
            } else {
                self.last_fail_key = key;
                self.same_fail_streak = 1;
            }
        } else {
            self.same_fail_streak = 0;
            self.last_fail_key.clear();
        }

        if self.noop_streak >= MAX_NOOP_STREAK {
            return LoopVerdict::Abort(
                "boucle détectée : JSON d'action invalide/tronqué à répétition \
                 (max_tokens ou note trop longue). Découpez : notes.create court \
                 puis notes.update par sections."
                    .into(),
            );
        }
        if !is_noop && self.same_fail_streak >= MAX_SAME_FAIL_STREAK {
            return LoopVerdict::Abort(format!(
                "boucle détectée : échec répété de `{action}` — changez d'approche \
                 (contenu plus court, notes.update incrémental, ou goal.fail)."
            ));
        }
        if self.noop_streak == 2 || self.same_fail_streak == 2 {
            return LoopVerdict::Warn(
                "Attention boucle : pour une note longue, \
                 notes.create (titre + plan court) puis notes.update section par section \
                 (≤ ~1200 caractères de content)."
                    .into(),
            );
        }
        LoopVerdict::Ok
    }
}

#[derive(Debug, Clone)]
pub enum LoopVerdict {
    Ok,
    Warn(String),
    Abort(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_grows_with_text() {
        assert!(estimate_tokens("abcd") <= 2);
        assert!(estimate_tokens(&"x".repeat(300)) >= 100);
    }

    #[test]
    fn budget_tracks_ctx_and_gen() {
        assert_eq!(prompt_budget(9216, 1536), 9216 - 1536 - 64);
        assert!(retry_prompt_budget(9216, 2048) < prompt_budget(9216, 2048));
    }

    #[test]
    fn enforce_shrinks_bloated_memory() {
        let mut mem = vec![
            ("system".into(), "You are base. ".repeat(800)),
            ("user".into(), "u0".into()),
            ("assistant".into(), "a0".into()),
        ];
        for i in 0..20 {
            mem.push((
                "user".into(),
                format!("user long {}{}", "y".repeat(2000), i),
            ));
            mem.push((
                "tool".into(),
                format!("[web.search] {}", "z".repeat(3000)),
            ));
            mem.push(("assistant".into(), format!("a{i}")));
        }
        let budget = prompt_budget(DEFAULT_N_CTX_HINT, AGENT_GEN_TOKENS);
        let before = estimate_messages_tokens(&mem);
        let note = enforce_prompt_budget(&mut mem, budget, 6);
        assert!(note.is_some());
        let after = estimate_messages_tokens(&mem);
        assert!(after < before);
        assert!(after <= budget + 1200);
    }

    #[test]
    fn clamp_brief() {
        assert_eq!(clamp_spawn_brief("  ok  "), "ok");
        let long = "a".repeat(900);
        assert!(clamp_spawn_brief(&long).chars().count() <= MAX_SPAWN_BRIEF_CHARS + 1);
    }

    #[test]
    fn detects_prompt_too_long() {
        assert!(is_prompt_too_long_error(
            "le prompt ne tient pas dans le contexte (prompt=8845 + réserve_gen=520 = 9365 tokens > ctx=9216)"
        ));
        assert_eq!(
            parse_ctx_from_error(
                "le prompt ne tient pas dans le contexte (prompt=8845 + réserve_gen=520 = 9365 tokens > ctx=9216)"
            ),
            9216
        );
        assert!(!is_prompt_too_long_error("timeout inférence"));
    }

    #[test]
    fn sanitize_truncates_broken_json() {
        let raw = "{\"thought\":\"x\",\"action\":\"notes.create\",\"args\":{\"content\":\"# Hi";
        let s = sanitize_assistant_for_memory(raw, false);
        assert!(!s.contains("\"action\""), "{s}");
        assert!(s.contains("notes.create"), "{s}");
    }

    #[test]
    fn loop_guard_trips_on_noop_streak() {
        let mut g = LoopGuard::default();
        assert!(matches!(g.observe("noop", "aucune action JSON"), LoopVerdict::Ok));
        assert!(matches!(g.observe("noop", "aucune action JSON"), LoopVerdict::Warn(_)));
        assert!(matches!(g.observe("noop", "aucune action JSON"), LoopVerdict::Abort(_)));
    }

    #[test]
    fn loop_guard_ignores_note_content_mentioning_error() {
        let mut g = LoopGuard::default();
        let body = r#"{"title":"Incident","content":"parse error in the logs"}"#;
        assert!(matches!(g.observe("notes.read", body), LoopVerdict::Ok));
        assert!(matches!(g.observe("notes.read", body), LoopVerdict::Ok));
        assert!(matches!(g.observe("notes.update", body), LoopVerdict::Ok));
        assert!(!looks_like_tool_failure(body));
    }

    #[test]
    fn loop_guard_abort_reason_is_runtime_only() {
        let mut g = LoopGuard::default();
        let verdict = g.observe("noop", "aucune action JSON");
        assert!(matches!(verdict, LoopVerdict::Ok));
        let _ = g.observe("noop", "aucune action JSON");
        let verdict = g.observe("noop", "aucune action JSON");
        match verdict {
            LoopVerdict::Abort(msg) => {
                assert!(msg.contains("JSON") || msg.contains("notes.create"));
            }
            other => panic!("expected Abort, got {other:?}"),
        }
        let user_visible = crate::actions::THREAD_FAIL_COULD_NOT_ACT;
        assert!(!user_visible.contains("JSON"));
        assert!(!user_visible.contains("notes.create"));
    }

    #[test]
    fn loop_guard_trips_on_structured_tool_error() {
        let mut g = LoopGuard::default();
        let err = "ERREUR outil: args invalides: missing field `title`";
        assert!(looks_like_tool_failure(err));
        assert!(matches!(g.observe("notes.create", err), LoopVerdict::Ok));
        assert!(matches!(g.observe("notes.create", err), LoopVerdict::Warn(_)));
        assert!(matches!(g.observe("notes.create", err), LoopVerdict::Abort(_)));
    }

    #[test]
    fn overflow_53_tokens_over_compacts_under_retry_budget() {
        let n_ctx = 9216u32;
        let gen_reserve = 520u32;
        let prompt_tokens = 8749usize;
        let err = format!(
            "le prompt ne tient pas dans le contexte (prompt={prompt_tokens} + réserve_gen={gen_reserve} = {} tokens > ctx={n_ctx})",
            prompt_tokens + gen_reserve as usize
        );
        assert!(is_prompt_too_long_error(&err));
        assert_eq!(parse_ctx_from_error(&err), n_ctx as usize);

        let mut mem = vec![("system".into(), "base ".repeat(400))];
        for i in 0..24 {
            mem.push(("user".into(), format!("u{i} {}", "x".repeat(900))));
            mem.push(("assistant".into(), format!("a{i} {}", "y".repeat(700))));
            mem.push(("tool".into(), format!("[web] {}", "z".repeat(500))));
        }
        let before = estimate_messages_tokens(&mem);
        assert!(
            before >= prompt_tokens.saturating_sub(400),
            "fixture should be near overflow: {before}"
        );

        let max_gen = (gen_reserve.saturating_sub(8)).max(256);
        let retry_budget = retry_prompt_budget(n_ctx as usize, max_gen);
        let note = aggressive_trim_for_overflow(&mut mem, n_ctx as usize, max_gen);
        assert!(note.is_some(), "expected compaction note");
        let after = estimate_messages_tokens(&mem);
        assert!(
            after <= retry_budget + 200,
            "after={after} retry_budget={retry_budget}"
        );
    }

    #[test]
    fn technical_overflow_not_user_visible() {
        let raw = "le prompt ne tient pas dans le contexte (prompt=8749 + réserve_gen=520 = 9269 tokens > ctx=9216)";
        assert!(is_technical_prompt_overflow_message(raw));
        let sentinel = user_visible_overflow_fail_reason();
        assert!(!sentinel.contains("ctx="));
        assert!(!sentinel.contains("réserve_gen"));
        assert!(!sentinel.contains("prompt="));
        assert_eq!(sentinel, "agent_could_not_continue");
    }

    #[test]
    fn technical_vision_infer_error_not_user_visible() {
        let raw = "images fournies mais aucun projecteur mmproj chargé";
        assert!(is_technical_vision_infer_error(raw));
        assert!(!raw.contains("ctx="));
    }

    #[test]
    fn technical_vision_infer_error_matches_mtmd_and_projecteur() {
        assert!(is_technical_vision_infer_error("VisionUnavailable"));
        assert!(is_technical_vision_infer_error("mtmd prefill failed"));
        assert!(is_technical_vision_infer_error("projecteur absent"));
        assert!(!is_technical_vision_infer_error("Impossible de continuer."));
    }

    #[test]
    fn technical_max_steps_fail_reason_not_user_visible() {
        let raw = "max_steps (64) atteint";
        assert!(is_technical_max_steps_fail_reason(raw));
        assert!(is_technical_max_steps_fail_reason("max steps (32) reached"));
        assert!(!is_technical_max_steps_fail_reason("Impossible de continuer."));
        assert!(!is_technical_max_steps_fail_reason("timeout goal atteint"));
    }

    #[test]
    fn choose_write_tokens() {
        let mem = vec![("user".into(), "crée une note guide".into())];
        assert_eq!(
            choose_agent_max_tokens(&mem, "rédige un guide"),
            AGENT_GEN_TOKENS_WRITE
        );
        assert_eq!(
            choose_agent_max_tokens(&[], "quelle heure est-il"),
            AGENT_GEN_TOKENS
        );
    }
}
