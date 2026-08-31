//! Compose structured research documents from web.search + web.browse results.

use aos_proto::{AgentTrace, WebSearchHit};

/// Page text from web.browse (or a recorded fetch failure).
#[derive(Debug, Clone)]
pub struct BrowsePage {
    pub url: String,
    pub title: String,
    pub text: String,
    pub fetch_error: Option<String>,
}

/// Build markdown for `/downloads` via files.generate — footnotes only from supplied sources.
pub fn compose_document(question: &str, search_hits: &[WebSearchHit], pages: &[BrowsePage]) -> String {
    let title = question.trim();
    let mut out = String::new();
    out.push_str(&format!("# {title}\n\n"));
    out.push_str(
        "_Structured research document. External facts below are footnoted; no source was invented._\n\n",
    );

    if search_hits.is_empty() && pages.is_empty() {
        out.push_str("## Findings\n\nNo web results were available for this query.\n");
        return out;
    }

    out.push_str("## Summary\n\n");
    if !search_hits.is_empty() {
        for (i, hit) in search_hits.iter().take(5).enumerate() {
            let n = i + 1;
            let snippet = hit.snippet.trim();
            if snippet.is_empty() {
                out.push_str(&format!(
                    "- Search hit [{}]: {} — see footnote [{n}].\n",
                    n,
                    hit.title.trim()
                ));
            } else {
                out.push_str(&format!("- {snippet} [{n}]\n"));
            }
        }
        out.push('\n');
    } else {
        out.push_str("_No search hits; see browsed pages below._\n\n");
    }

    if !pages.is_empty() {
        out.push_str("## Sources reviewed\n\n");
        for page in pages {
            let heading = if page.title.trim().is_empty() {
                page.url.clone()
            } else {
                page.title.trim().to_string()
            };
            out.push_str(&format!("### {heading}\n\n"));
            if let Some(err) = &page.fetch_error {
                out.push_str(&format!("_Fetch failed: {err}_\n\n"));
            } else if page.text.trim().is_empty() {
                out.push_str("_No extractable text._\n\n");
            } else {
                let excerpt: String = page.text.chars().take(1200).collect();
                out.push_str(&excerpt);
                if page.text.chars().count() > 1200 {
                    out.push('…');
                }
                out.push_str("\n\n");
            }
        }
    }

    out.push_str("## Footnotes\n\n");
    let mut footnote_n = 1usize;
    for hit in search_hits.iter().take(5) {
        if hit.url.trim().is_empty() {
            continue;
        }
        out.push_str(&format!(
            "[^{footnote_n}]: {} — {}\n",
            hit.title.trim(),
            hit.url.trim()
        ));
        footnote_n += 1;
    }
    for page in pages {
        if page.url.trim().is_empty() {
            continue;
        }
        let label = if page.title.trim().is_empty() {
            page.url.trim()
        } else {
            page.title.trim()
        };
        if page.fetch_error.is_some() {
            out.push_str(&format!(
                "[^{footnote_n}]: {label} — {} (_fetch failed_)\n",
                page.url.trim()
            ));
        } else {
            out.push_str(&format!(
                "[^{footnote_n}]: {label} — {}\n",
                page.url.trim()
            ));
        }
        footnote_n += 1;
    }
    if footnote_n == 1 {
        out.push_str("_No external footnotes — no verified URLs were collected._\n");
    }
    out
}

/// Suggested logical path under `/downloads/`.
pub fn default_download_path(question: &str) -> String {
    let slug: String = question
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-');
    let slug: String = slug.chars().take(48).collect();
    let slug = if slug.is_empty() { "research" } else { slug.as_str() };
    format!("/downloads/research-{slug}.md")
}

/// Extract `/downloads/...` path from an agent trace (last files.generate wins).
pub fn path_from_trace(trace: &AgentTrace) -> Option<String> {
    let mut path = None;
    for step in &trace.steps {
        if step.action == "files.generate" {
            if let Some(p) = step.args.get("path").and_then(|v| v.as_str()) {
                if p.starts_with("/downloads/") {
                    path = Some(p.to_string());
                }
            }
            if path.is_none() {
                if let Some(found) = extract_downloads_path(&step.tool_result) {
                    path = Some(found);
                }
            }
        }
    }
    path
}

fn extract_downloads_path(s: &str) -> Option<String> {
    for token in s.split_whitespace() {
        if token.starts_with("/downloads/") {
            return Some(
                token
                    .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '/' && c != '.' && c != '_' && c != '-')
                    .to_string(),
            );
        }
    }
    s.find("/downloads/")
        .map(|i| {
            let rest = &s[i..];
            let end = rest
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',')
                .unwrap_or(rest.len());
            rest[..end].to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::AgentStepRecord;

    fn sample_hit(title: &str, url: &str, snippet: &str) -> WebSearchHit {
        WebSearchHit {
            title: title.into(),
            url: url.into(),
            snippet: snippet.into(),
        }
    }

    #[test]
    fn compose_includes_real_footnote_from_mocked_search() {
        let hits = vec![sample_hit(
            "Agentic AI Survey",
            "https://example.com/survey",
            "Recent frameworks emphasize tool use and planning.",
        )];
        let pages = vec![BrowsePage {
            url: "https://example.com/survey".into(),
            title: "Agentic AI Survey".into(),
            text: "Body text from browse.".into(),
            fetch_error: None,
        }];
        let md = compose_document("What is the state of agentic apps?", &hits, &pages);
        assert!(md.contains("## Footnotes"));
        assert!(md.contains("https://example.com/survey"));
        assert!(md.contains("Agentic AI Survey"));
        assert!(md.contains("[^1]:"));
        assert!(!md.contains("https://invented.example"));
    }

    #[test]
    fn compose_notes_fetch_failure() {
        let pages = vec![BrowsePage {
            url: "https://blocked.example/page".into(),
            title: "Blocked".into(),
            text: String::new(),
            fetch_error: Some("HTTP 403".into()),
        }];
        let md = compose_document("Survey?", &[], &pages);
        assert!(md.contains("Fetch failed"));
        assert!(md.contains("HTTP 403"));
        assert!(md.contains("https://blocked.example/page"));
    }

    #[test]
    fn no_invented_sources_when_empty() {
        let md = compose_document("Empty research?", &[], &[]);
        assert!(!md.contains("http"));
        assert!(md.contains("No web results"));
    }

    #[test]
    fn path_from_trace_reads_files_generate() {
        let mut trace = AgentTrace::default();
        trace.steps.push(AgentStepRecord {
            action: "files.generate".into(),
            args: serde_json::json!({"path": "/downloads/research-agentic.md"}),
            ..Default::default()
        });
        assert_eq!(
            path_from_trace(&trace).as_deref(),
            Some("/downloads/research-agentic.md")
        );
    }

    #[test]
    fn default_download_path_slug() {
        let p = default_download_path("What is agentic?");
        assert!(p.starts_with("/downloads/research-"));
        assert!(p.ends_with(".md"));
    }
}
