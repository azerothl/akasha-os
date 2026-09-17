//! Aggregate agent trace sources, cite markers in summaries, and Sources footers.

use aos_proto::{AgentSource, AgentStepRecord, WebSearchHit};
use std::collections::HashMap;

/// Max sources listed in a chat completion footer.
pub const MAX_CHAT_SOURCES: usize = 8;

/// Collect unique sources from completed steps, preferring browsed/fetched pages
/// over raw search hits. Capped at [`MAX_CHAT_SOURCES`].
pub fn aggregate_trace_sources(steps: &[AgentStepRecord]) -> Vec<AgentSource> {
    let mut primary: Vec<AgentSource> = Vec::new();
    let mut search_hits: Vec<AgentSource> = Vec::new();

    for step in steps {
        let action = step.action.trim();
        for s in &step.sources {
            if s.locator.trim().is_empty() {
                continue;
            }
            // Prefer browse / fetch / document over raw search-result clutter.
            let bucket = if action == "web.search" {
                &mut search_hits
            } else {
                &mut primary
            };
            push_unique(bucket, s);
        }
    }

    let mut out = primary;
    for s in search_hits {
        if out.len() >= MAX_CHAT_SOURCES {
            break;
        }
        push_unique(&mut out, &s);
    }
    out.truncate(MAX_CHAT_SOURCES);
    out
}

fn push_unique(out: &mut Vec<AgentSource>, s: &AgentSource) {
    if out
        .iter()
        .any(|x| x.locator == s.locator && x.kind == s.kind)
    {
        return;
    }
    out.push(s.clone());
}

/// Numbered search hits for the agent + trailing JSON so `collect_sources` can parse.
pub fn format_search_hits_for_agent(hits: &[WebSearchHit]) -> String {
    if hits.is_empty() {
        return "[]".into();
    }
    let mut out = String::from(
        "Résultats numérotés — ne cite que les hits réellement utiles au sujet \
         (ignore dictionnaires / pages hors-sujet). Dans goal.complete, place [n] \
         après chaque fait appuyé par un hit, puis une liste Sources.\n\n",
    );
    for (i, h) in hits.iter().enumerate() {
        let n = i + 1;
        let title = h.title.trim();
        let url = h.url.trim();
        let snippet = h.snippet.trim();
        out.push_str(&format!("[{n}] {title}\n    {url}\n"));
        if !snippet.is_empty() {
            out.push_str(&format!("    {snippet}\n"));
        }
        out.push('\n');
    }
    out.push_str(&serde_json::to_string(hits).unwrap_or_else(|_| "[]".into()));
    out
}

/// Parse `web.search` tool outcome (plain JSON array, or numbered preamble + JSON).
pub fn parse_web_search_hits(outcome: &str) -> Vec<WebSearchHit> {
    let trimmed = outcome.trim();
    if let Ok(hits) = serde_json::from_str::<Vec<WebSearchHit>>(trimmed) {
        return hits;
    }
    if let Some(i) = trimmed.rfind("\n[") {
        let json = trimmed[i + 1..].trim();
        if let Ok(hits) = serde_json::from_str::<Vec<WebSearchHit>>(json) {
            return hits;
        }
    }
    if let Some(i) = trimmed.find('[') {
        if let Ok(hits) = serde_json::from_str::<Vec<WebSearchHit>>(&trimmed[i..]) {
            return hits;
        }
    }
    Vec::new()
}

/// Annotate body with `[n]` markers (when missing) then append Sources footer.
/// Only keeps sources that actually relate to the summary text (drops dictionary
/// noise / off-topic search hits).
pub fn finalize_summary_with_sources(summary: &str, sources: &[AgentSource]) -> String {
    let relevant = filter_relevant_sources(summary, sources);
    let cleaned = strip_inline_citation_markers(summary);
    let annotated = annotate_with_citation_markers(&cleaned, &relevant);
    append_sources_footer(&annotated, &relevant)
}

/// Keep sources that share content tokens with the summary, or browsed docs.
/// Drops off-topic dictionary hits that would otherwise pollute citations.
pub fn filter_relevant_sources(summary: &str, sources: &[AgentSource]) -> Vec<AgentSource> {
    if sources.is_empty() {
        return Vec::new();
    }
    let content = content_tokens(summary);
    let mut out = Vec::new();
    for s in sources {
        if source_is_relevant(s, summary, &content) {
            push_unique(&mut out, s);
        }
        if out.len() >= MAX_CHAT_SOURCES {
            break;
        }
    }
    out
}

/// True when a `web.search` query is too weak (stopwords / dictionary lookups).
pub fn is_weak_search_query(query: &str) -> bool {
    let q = query.trim();
    if q.is_empty() {
        return true;
    }
    let tokens = tokenize(q);
    if tokens.is_empty() {
        return true;
    }
    // Single dictionary-ish word.
    if tokens.len() == 1 {
        let t = tokens[0].as_str();
        if is_stopword(t) || is_dictionary_lookup_term(t) {
            return true;
        }
    }
    // All tokens are stopwords or tiny.
    tokens
        .iter()
        .all(|t| is_stopword(t) || is_dictionary_lookup_term(t) || t.len() < 3)
}

fn source_is_relevant(s: &AgentSource, summary: &str, content: &std::collections::HashSet<String>) -> bool {
    if looks_like_dictionary_hit(s) && !dictionary_hit_matches_topic(s, content) {
        return false;
    }
    let evidence = evidence_text(s);
    if !content.is_empty() {
        let ev_lower = evidence.to_ascii_lowercase();
        let shared = content
            .iter()
            .filter(|t| ev_lower.contains(t.as_str()))
            .count();
        if shared >= 1 {
            return true;
        }
    }
    // Browsed/fetched pages are trusted more than raw search clutter.
    if matches!(s.kind.as_str(), "document" | "fetch") {
        return overlap_score(summary, &evidence) >= 0.04;
    }
    overlap_score(summary, &evidence) >= 0.10
}

fn dictionary_hit_matches_topic(
    s: &AgentSource,
    content: &std::collections::HashSet<String>,
) -> bool {
    if content.is_empty() {
        return false;
    }
    let ev = evidence_text(s).to_ascii_lowercase();
    content.iter().filter(|t| ev.contains(t.as_str())).count() >= 1
}

fn looks_like_dictionary_hit(s: &AgentSource) -> bool {
    let u = s.locator.to_ascii_lowercase();
    let t = s.title.to_ascii_lowercase();
    u.contains("larousse.fr")
        || u.contains("wiktionnaire")
        || u.contains("cnrtl.fr")
        || u.contains("academie-francaise")
        || u.contains("dictionary.com")
        || u.contains("merriam-webster")
        || u.contains("le-robert")
        || u.contains("lerobert")
        || u.contains("/dictionnaires/")
        || t.contains("dictionnaire")
        || t.contains("dictionary")
        || (t.contains("definition") || t.contains("définition")) && !contentish_title(&t)
}

fn contentish_title(title_lower: &str) -> bool {
    title_lower.contains("agentic")
        || title_lower.contains("operating system")
        || title_lower.contains("système d'exploitation")
        || title_lower.contains("systeme d'exploitation")
}

fn content_tokens(text: &str) -> std::collections::HashSet<String> {
    tokenize(text)
        .into_iter()
        .filter(|t| t.len() >= 5 && !is_stopword(t) && !is_dictionary_lookup_term(t))
        .collect()
}

fn is_dictionary_lookup_term(t: &str) -> bool {
    matches!(
        t,
        "definition"
            | "dictionnaire"
            | "dictionary"
            | "synonyme"
            | "synonym"
            | "prononciation"
            | "etymologie"
    ) || t == "définition"
        || t == "étymologie"
}

fn is_stopword(t: &str) -> bool {
    matches!(
        t,
        "the"
            | "and"
            | "for"
            | "with"
            | "that"
            | "this"
            | "from"
            | "are"
            | "was"
            | "were"
            | "what"
            | "when"
            | "where"
            | "which"
            | "how"
            | "why"
            | "who"
            | "into"
            | "about"
            | "les"
            | "des"
            | "une"
            | "est"
            | "sont"
            | "pour"
            | "dans"
            | "avec"
            | "sur"
            | "par"
            | "pas"
            | "plus"
            | "comme"
            | "cette"
            | "ces"
            | "aux"
            | "dont"
            | "que"
            | "qui"
            | "quoi"
            | "qu"
            | "ce"
            | "se"
            | "ne"
            | "en"
            | "un"
            | "du"
            | "de"
            | "la"
            | "le"
            | "et"
            | "ou"
            | "il"
            | "elle"
            | "nous"
            | "vous"
            | "ils"
            | "elles"
            | "son"
            | "sa"
            | "ses"
            | "mon"
            | "ma"
            | "mes"
            | "ton"
            | "ta"
            | "tes"
            | "rechercher"
            | "recherche"
            | "resume"
            | "résume"
            | "resumer"
            | "résumer"
            | "phrases"
            | "sources"
            | "source"
            | "agent"
    )
}

fn strip_inline_citation_markers(summary: &str) -> String {
    let (body, suffix) = split_off_sources_section(summary);
    let mut out = String::with_capacity(body.len());
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b']' {
                // Skip `[12]` but keep markdown links `[1](http…)` handled separately —
                // if next is '(', leave intact for rewrite_markdown_links.
                if j + 1 < bytes.len() && bytes[j + 1] == b'(' {
                    out.push('[');
                    i += 1;
                    continue;
                }
                i = j + 1;
                continue;
            }
        }
        let ch = body[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    // Collapse spaces left by stripped markers before punctuation: "word [1]." → "word ."
    let collapsed = collapse_space_before_punct(out.trim_end());
    join_body_suffix(&collapsed, suffix)
}

fn collapse_space_before_punct(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c == ' ' {
            if let Some(next) = chars.get(i + 1) {
                if matches!(next, '.' | '!' | '?' | ',' | ';' | ':') {
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

/// Insert `[n]` after sentences that best match each source snippet/title/url.
/// No-op when the summary already has inline citation markers.
pub fn annotate_with_citation_markers(summary: &str, sources: &[AgentSource]) -> String {
    if sources.is_empty() {
        return summary.to_string();
    }
    let summary = summary.trim_end();
    if summary.is_empty() {
        return summary.to_string();
    }
    if has_inline_citation_markers(summary) {
        return rewrite_markdown_links_to_markers(summary, sources);
    }

    let (body, suffix) = split_off_sources_section(summary);
    let body = rewrite_markdown_links_to_markers(body.trim_end(), sources);
    if has_inline_citation_markers(&body) {
        return join_body_suffix(&body, suffix);
    }

    let sentences = split_sentences(&body);
    if sentences.is_empty() {
        return summary.to_string();
    }

    let mut markers_by_sent: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut used_sentences = std::collections::HashSet::new();

    for (si, source) in sources.iter().enumerate() {
        let n = si + 1;
        let evidence = evidence_text(source);
        if evidence.chars().count() < 12 {
            continue;
        }
        let mut best: Option<(usize, f32)> = None;
        for (i, sent) in sentences.iter().enumerate() {
            let score = overlap_score(sent, &evidence);
            if score < 0.08 {
                continue;
            }
            let better = match best {
                None => true,
                Some((_, prev)) => score > prev + 0.001,
            };
            if better {
                best = Some((i, score));
            }
        }
        if let Some((i, score)) = best {
            if used_sentences.contains(&i) && score < 0.18 {
                continue;
            }
            markers_by_sent.entry(i).or_default().push(n);
            used_sentences.insert(i);
        }
    }

    if markers_by_sent.is_empty() {
        return join_body_suffix(&body, suffix);
    }

    let mut out = String::with_capacity(body.len() + markers_by_sent.len() * 8);
    for (i, sent) in sentences.iter().enumerate() {
        if i > 0 {
            // Preserve a space between sentences when the next doesn't start with whitespace.
            if !out.ends_with([' ', '\n']) && !sent.starts_with(char::is_whitespace) {
                out.push(' ');
            }
        }
        if let Some(markers) = markers_by_sent.get(&i) {
            out.push_str(&insert_markers_before_trailing_punct(sent, markers));
        } else {
            out.push_str(sent);
        }
    }
    join_body_suffix(out.trim_end(), suffix)
}

/// Append a numbered markdown Sources section when sources exist and are not
/// already referenced in `summary` (avoids double-append from worker + UI).
pub fn append_sources_footer(summary: &str, sources: &[AgentSource]) -> String {
    let summary = summary.trim_end();
    if sources.is_empty() {
        return summary.to_string();
    }
    let lower = summary.to_ascii_lowercase();
    if lower.contains("## sources") {
        return summary.to_string();
    }
    let missing: Vec<&AgentSource> = sources
        .iter()
        .filter(|s| {
            let loc = s.locator.trim();
            !loc.is_empty() && !summary.contains(loc)
        })
        .collect();
    if missing.is_empty() {
        return summary.to_string();
    }

    let mut out = String::with_capacity(summary.len() + 128 + missing.len() * 80);
    out.push_str(summary);
    if !summary.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str("## Sources\n\n");
    for (i, s) in missing.iter().enumerate() {
        let n = i + 1;
        let title = source_label(s);
        let locator = s.locator.trim();
        if locator.starts_with("http://") || locator.starts_with("https://") {
            out.push_str(&format!("{n}. [{title}]({locator})\n"));
        } else {
            out.push_str(&format!("{n}. `{locator}`\n"));
        }
    }
    out
}

fn source_label(s: &AgentSource) -> String {
    let title = s.title.trim();
    if !title.is_empty() {
        return title.replace('[', "(").replace(']', ")");
    }
    let loc = s.locator.trim();
    if loc.len() > 60 {
        format!("{}…", loc.chars().take(57).collect::<String>())
    } else {
        loc.to_string()
    }
}

fn evidence_text(s: &AgentSource) -> String {
    format!("{} {} {}", s.title, s.snippet, s.locator)
}

fn has_inline_citation_markers(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        if bytes[i] == b'[' && bytes[i + 1].is_ascii_digit() {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b']' {
                // Ignore markdown links `[1](http…)` — still counts as a marker.
                return true;
            }
        }
        i += 1;
    }
    false
}

fn rewrite_markdown_links_to_markers(summary: &str, sources: &[AgentSource]) -> String {
    let mut out = summary.to_string();
    for (i, s) in sources.iter().enumerate() {
        let n = i + 1;
        let url = s.locator.trim();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            continue;
        }
        // Replace any `[label](url)` with `[n]`.
        let needle = format!("]({url})");
        while let Some(end) = out.find(&needle) {
            let before = &out[..end];
            if let Some(open) = before.rfind('[') {
                let mut rebuilt = String::with_capacity(out.len());
                rebuilt.push_str(&out[..open]);
                rebuilt.push_str(&format!("[{n}]"));
                rebuilt.push_str(&out[end + needle.len()..]);
                out = rebuilt;
            } else {
                break;
            }
        }
    }
    out
}

fn split_off_sources_section(summary: &str) -> (&str, &str) {
    let lower = summary.to_ascii_lowercase();
    if let Some(i) = lower.find("\n## sources") {
        return (&summary[..i], &summary[i..]);
    }
    if lower.starts_with("## sources") {
        return ("", summary);
    }
    (summary, "")
}

fn join_body_suffix(body: &str, suffix: &str) -> String {
    if suffix.is_empty() {
        return body.to_string();
    }
    if body.is_empty() {
        return suffix.to_string();
    }
    format!("{body}{suffix}")
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (k, (idx, ch)) in chars.iter().enumerate() {
        let is_end = matches!(ch, '.' | '!' | '?' | '。' | '！' | '？');
        if !is_end {
            continue;
        }
        // Skip decimal-like 3.14
        if *ch == '.' {
            let prev_digit = k
                .checked_sub(1)
                .and_then(|p| chars.get(p))
                .is_some_and(|(_, c)| c.is_ascii_digit());
            let next_digit = chars
                .get(k + 1)
                .is_some_and(|(_, c)| c.is_ascii_digit());
            if prev_digit && next_digit {
                continue;
            }
        }
        let end = idx + ch.len_utf8();
        // Include following closing quotes/parens
        let mut end = end;
        for (j, c) in text[end..].char_indices() {
            if matches!(c, '"' | '\'' | '»' | ')' | ']' | '”') {
                end = end + j + c.len_utf8();
            } else {
                break;
            }
        }
        let sent = text[start..end].trim();
        if !sent.is_empty() {
            out.push(sent.to_string());
        }
        start = end;
        while start < text.len() && text[start..].starts_with(char::is_whitespace) {
            let c = text[start..].chars().next().unwrap();
            start += c.len_utf8();
        }
    }
    if start < text.len() {
        let rest = text[start..].trim();
        if !rest.is_empty() {
            out.push(rest.to_string());
        }
    }
    out
}

fn tokenize(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3)
        .map(|t| t.to_ascii_lowercase())
        .collect()
}

fn overlap_score(sentence: &str, evidence: &str) -> f32 {
    let sent_tokens = tokenize(sentence);
    let ev_tokens = tokenize(evidence);
    if sent_tokens.is_empty() || ev_tokens.is_empty() {
        return 0.0;
    }
    let ev_set: std::collections::HashSet<&str> =
        ev_tokens.iter().map(String::as_str).collect();
    let mut hit = 0usize;
    for t in &sent_tokens {
        if ev_set.contains(t.as_str()) {
            hit += 1;
        }
    }
    hit as f32 / sent_tokens.len() as f32
}

fn insert_markers_before_trailing_punct(sentence: &str, markers: &[usize]) -> String {
    let mut sorted = markers.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let marker_str: String = sorted.iter().map(|n| format!("[{n}]")).collect();

    let trimmed = sentence.trim_end();
    let mut cut = trimmed.len();
    while cut > 0 {
        let c = trimmed[..cut].chars().last().unwrap();
        if matches!(c, '.' | '!' | '?' | '。' | '！' | '？' | '"' | '\'' | '»' | ')' | '”') {
            cut -= c.len_utf8();
        } else {
            break;
        }
    }
    if cut == 0 || cut == trimmed.len() {
        return format!("{trimmed}{marker_str}");
    }
    if trimmed[..cut].ends_with(&marker_str) {
        return trimmed.to_string();
    }
    format!("{}{}{}", &trimmed[..cut], marker_str, &trimmed[cut..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(kind: &str, title: &str, locator: &str) -> AgentSource {
        AgentSource {
            kind: kind.into(),
            title: title.into(),
            locator: locator.into(),
            snippet: String::new(),
        }
    }

    fn src_snip(kind: &str, title: &str, locator: &str, snippet: &str) -> AgentSource {
        AgentSource {
            kind: kind.into(),
            title: title.into(),
            locator: locator.into(),
            snippet: snippet.into(),
        }
    }

    fn step(action: &str, sources: Vec<AgentSource>) -> AgentStepRecord {
        AgentStepRecord {
            action: action.into(),
            sources,
            ..Default::default()
        }
    }

    #[test]
    fn append_adds_numbered_markdown_links() {
        let sources = vec![src(
            "web",
            "Agentic AI Survey",
            "https://example.com/survey",
        )];
        let out = append_sources_footer("Voici le résumé.", &sources);
        assert!(out.contains("## Sources"));
        assert!(out.contains("1. [Agentic AI Survey](https://example.com/survey)"));
        assert!(out.starts_with("Voici le résumé."));
    }

    #[test]
    fn append_skips_when_urls_already_present() {
        let sources = vec![src("web", "Survey", "https://example.com/survey")];
        let summary = "See https://example.com/survey for details.";
        let out = append_sources_footer(summary, &sources);
        assert_eq!(out, summary);
        assert!(!out.contains("## Sources"));
    }

    #[test]
    fn append_skips_when_sources_heading_present() {
        let sources = vec![src("web", "Survey", "https://example.com/a")];
        let summary = "Body\n\n## Sources\n\n1. [x](https://other.example)";
        assert_eq!(append_sources_footer(summary, &sources), summary);
    }

    #[test]
    fn append_documents_as_backtick_paths() {
        let sources = vec![src("document", "readme", "/docs/README.md")];
        let out = append_sources_footer("ok", &sources);
        assert!(out.contains("1. `/docs/README.md`"));
        assert!(!out.contains("](/docs/README.md)"));
    }

    #[test]
    fn aggregate_dedups_and_caps() {
        let mut hits = Vec::new();
        for i in 0..12 {
            hits.push(src(
                "web",
                &format!("Hit {i}"),
                &format!("https://example.com/{i}"),
            ));
        }
        let steps = vec![
            step(
                "web.browse",
                vec![src("web", "Page", "https://example.com/page")],
            ),
            step("web.search", hits),
            step(
                "web.search",
                vec![src("web", "Page dup", "https://example.com/page")],
            ),
        ];
        let out = aggregate_trace_sources(&steps);
        assert!(out.len() <= MAX_CHAT_SOURCES);
        assert_eq!(out[0].locator, "https://example.com/page");
        assert_eq!(
            out.iter()
                .filter(|s| s.locator == "https://example.com/page")
                .count(),
            1
        );
    }

    #[test]
    fn aggregate_prefers_browse_over_search() {
        let steps = vec![
            step(
                "web.search",
                vec![src("web", "Hit", "https://example.com/hit")],
            ),
            step(
                "web.browse",
                vec![src("web", "Page", "https://example.com/page")],
            ),
        ];
        let out = aggregate_trace_sources(&steps);
        assert_eq!(out[0].locator, "https://example.com/page");
        assert!(out.iter().any(|s| s.locator == "https://example.com/hit"));
    }

    #[test]
    fn append_empty_sources_is_noop() {
        assert_eq!(append_sources_footer("hello", &[]), "hello");
    }

    #[test]
    fn format_search_hits_includes_numbers_and_json() {
        let hits = vec![WebSearchHit {
            title: "Agentic OS".into(),
            url: "https://example.com/aos".into(),
            snippet: "Autonomous agents on the OS.".into(),
        }];
        let s = format_search_hits_for_agent(&hits);
        assert!(s.contains("[1] Agentic OS"));
        assert!(s.contains("https://example.com/aos"));
        assert!(s.contains(r#""url":"https://example.com/aos""#) || s.contains("example.com/aos"));
        let parsed = parse_web_search_hits(&s);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].url, "https://example.com/aos");
    }

    #[test]
    fn annotate_inserts_markers_from_snippet_overlap() {
        let sources = vec![
            src_snip(
                "web",
                "Agentic OS Protocol",
                "https://example.com/protocol",
                "autonomous agents perceive plan and act on hardware",
            ),
            src_snip(
                "web",
                "IPC bus",
                "https://example.com/bus",
                "semantic IPC bus CBOR services userspace",
            ),
        ];
        let summary = "Un Agentic OS intègre des autonomous agents qui plan and act on hardware. \
                       Il s'appuie sur un semantic IPC bus CBOR entre services userspace.";
        let out = annotate_with_citation_markers(summary, &sources);
        assert!(out.contains("[1]"), "expected [1] in: {out}");
        assert!(out.contains("[2]"), "expected [2] in: {out}");
        assert!(!out.contains("## Sources"));
    }

    #[test]
    fn finalize_annotates_and_appends_footer() {
        let sources = vec![src_snip(
            "web",
            "Guide",
            "https://example.com/guide",
            "agentic operating system automates security",
        )];
        let summary = "An agentic operating system automates security tasks in real time.";
        let out = finalize_summary_with_sources(summary, &sources);
        assert!(out.contains("[1]"));
        assert!(out.contains("## Sources"));
        assert!(out.contains("https://example.com/guide"));
    }

    #[test]
    fn annotate_does_not_force_markers_without_overlap() {
        let sources = vec![
            src(
                "web",
                "Définition — Larousse",
                "https://www.larousse.fr/dictionnaires/francais/definition/1",
            ),
            src(
                "web",
                "Définition — Wikipédia",
                "https://fr.wikipedia.org/wiki/Definition",
            ),
        ];
        let summary =
            "Un Agentic OS intègre des autonomous agents sur le hardware et un bus IPC.";
        let out = annotate_with_citation_markers(summary, &sources);
        assert!(!out.contains("[1]"), "{out}");
        assert!(!out.contains("[2]"), "{out}");
    }

    #[test]
    fn filter_drops_dictionary_hits_unrelated_to_summary() {
        let sources = vec![
            src(
                "web",
                "Définition — Larousse",
                "https://www.larousse.fr/dictionnaires/francais/definition/1",
            ),
            src_snip(
                "web",
                "Agentic OS Protocol",
                "https://example.com/agentic-os",
                "agentic operating system autonomous agents",
            ),
        ];
        let summary =
            "An agentic operating system runs autonomous agents on local hardware.";
        let kept = filter_relevant_sources(summary, &sources);
        assert_eq!(kept.len(), 1);
        assert!(kept[0].locator.contains("agentic-os"));
    }

    #[test]
    fn finalize_omits_offtopic_dictionary_sources() {
        let sources = vec![src(
            "web",
            "définition - Larousse",
            "https://www.larousse.fr/dictionnaires/francais/definition/1",
        )];
        let summary = "Un Agentic OS coordonne des agents autonomes via un bus IPC.";
        let out = finalize_summary_with_sources(summary, &sources);
        assert!(!out.contains("## Sources"), "{out}");
        assert!(!out.to_ascii_lowercase().contains("larousse"), "{out}");
        assert!(!out.contains("[1]"), "{out}");
    }

    #[test]
    fn weak_search_query_detects_dictionary_lookups() {
        assert!(is_weak_search_query("définition"));
        assert!(is_weak_search_query("qu'"));
        assert!(is_weak_search_query("ce qu est"));
        assert!(!is_weak_search_query("agentic OS"));
        assert!(!is_weak_search_query("agentic operating system"));
    }

    #[test]
    fn rewrite_markdown_link_to_marker() {
        let sources = vec![src("web", "Guide", "https://example.com/guide")];
        let summary = "See the [Guide](https://example.com/guide) for details.";
        let out = annotate_with_citation_markers(summary, &sources);
        assert!(out.contains("[1]"));
        assert!(!out.contains("](https://example.com/guide)"));
    }
}
