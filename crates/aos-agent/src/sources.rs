//! Aggregate agent trace sources and append a markdown Sources footer for chat replies.

use aos_proto::{AgentSource, AgentStepRecord};

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
        // Escape brackets that would break markdown links.
        return title.replace('[', "(").replace(']', ")");
    }
    let loc = s.locator.trim();
    if loc.len() > 60 {
        format!("{}…", loc.chars().take(57).collect::<String>())
    } else {
        loc.to_string()
    }
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
}
