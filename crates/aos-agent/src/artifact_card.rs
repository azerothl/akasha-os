//! Detect agent-produced notes, documents, and images for in-chat artifact cards.

use aos_proto::{AgentStepRecord, ChatAttachment};

/// Logical artifact kind surfaced in the chat bubble card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    Note,
    Document,
    Image,
}

/// One artifact the user can open from the producing message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProducedArtifact {
    pub title: String,
    pub path: String,
    pub kind: ArtifactKind,
    pub slug: String,
}

impl ProducedArtifact {
    pub fn attachment(&self) -> ChatAttachment {
        ChatAttachment::ArtifactCard {
            title: self.title.clone(),
            artifact_type: kind_tag(self.kind),
            path: self.path.clone(),
            slug: self.slug.clone(),
        }
    }
}

fn kind_tag(kind: ArtifactKind) -> String {
    match kind {
        ArtifactKind::Note => "note".to_string(),
        ArtifactKind::Document => "document".to_string(),
        ArtifactKind::Image => "image".to_string(),
    }
}

/// Parse a single tool step into an artifact, when the tool succeeded.
pub fn detect_from_tool(
    action: &str,
    args: &serde_json::Value,
    tool_result: &str,
) -> Option<ProducedArtifact> {
    if looks_like_tool_failure(tool_result) {
        return None;
    }
    match action.trim() {
        "notes.create" | "notes.update" => detect_note(args, tool_result),
        "files.generate" => detect_document(args, tool_result),
        "media.image.generate" | "media.generate" => detect_image(args, tool_result),
        _ => detect_from_path_token(tool_result),
    }
}

/// Collect artifacts from an agent trace (last path wins per kind+path).
pub fn artifacts_from_trace(trace: &[AgentStepRecord]) -> Vec<ProducedArtifact> {
    let mut out: Vec<ProducedArtifact> = Vec::new();
    for step in trace {
        if let Some(art) = detect_from_tool(&step.action, &step.args, &step.tool_result) {
            if let Some(idx) = out.iter().position(|a| a.path == art.path) {
                out[idx] = art;
            } else {
                out.push(art);
            }
        }
    }
    out
}

pub fn attachments_from_artifacts(artifacts: &[ProducedArtifact]) -> Vec<ChatAttachment> {
    artifacts.iter().map(|a| a.attachment()).collect()
}

/// Remove artifact paths and tool ids from visible bubble prose.
pub fn strip_paths_from_prose(text: &str, artifacts: &[ProducedArtifact]) -> String {
    let mut out = text.to_string();
    for art in artifacts {
        for token in path_tokens(&art.path) {
            out = replace_token(&out, &token);
        }
        if !art.title.is_empty() {
            out = replace_token(&out, &art.title);
        }
    }
    out = strip_tool_id_leaks(&out);
    collapse_blank_lines(out)
}

fn detect_note(args: &serde_json::Value, tool_result: &str) -> Option<ProducedArtifact> {
    let json = parse_json_payload(tool_result).or_else(|| Some(args.clone()));
    let obj = json.as_ref().and_then(|v| v.as_object())?;
    let path = obj
        .get("path")
        .and_then(|v| v.as_str())
        .filter(|p| is_note_path(p))?;
    let slug = obj
        .get("slug")
        .and_then(|v| v.as_str())
        .or_else(|| slug_from_note_path(path))
        .unwrap_or("")
        .to_string();
    let title = obj
        .get("title")
        .and_then(|v| v.as_str())
        .or_else(|| args.get("title").and_then(|v| v.as_str()))
        .or_else(|| args.get("new_title").and_then(|v| v.as_str()))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| human_title_from_path(path))
        .unwrap_or_else(|| "Note".to_string());
    Some(ProducedArtifact {
        title,
        path: path.to_string(),
        kind: ArtifactKind::Note,
        slug,
    })
}

fn detect_document(args: &serde_json::Value, tool_result: &str) -> Option<ProducedArtifact> {
    let path = parse_json_payload(tool_result)
        .and_then(|v| v.get("path").and_then(|p| p.as_str()).map(str::to_string))
        .or_else(|| {
            args.get("path")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .filter(|p| p.starts_with("/downloads/"))?;
    if is_image_path(&path) {
        return detect_image_at_path(args, &path);
    }
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| human_title_from_path(&path))
        .unwrap_or_else(|| "Document".to_string());
    Some(ProducedArtifact {
        title,
        path,
        kind: ArtifactKind::Document,
        slug: String::new(),
    })
}

fn detect_image(args: &serde_json::Value, tool_result: &str) -> Option<ProducedArtifact> {
    let path = extract_image_path(tool_result)
        .or_else(|| {
            parse_json_payload(tool_result)
                .and_then(|v| v.get("path").and_then(|p| p.as_str()).map(str::to_string))
        })
        .or_else(|| {
            args.get("path")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .filter(|p| is_image_path(p))?;
    detect_image_at_path(args, &path)
}

fn detect_image_at_path(args: &serde_json::Value, path: &str) -> Option<ProducedArtifact> {
    let title = args
        .get("prompt")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| human_title_from_path(path))
        .unwrap_or_else(|| "Image".to_string());
    Some(ProducedArtifact {
        title: truncate_chars(&title, 120),
        path: path.to_string(),
        kind: ArtifactKind::Image,
        slug: String::new(),
    })
}

fn detect_from_path_token(tool_result: &str) -> Option<ProducedArtifact> {
    for token in tool_result.split_whitespace() {
        let cleaned = token.trim_matches(|c: char| {
            matches!(c, '`' | '"' | '\'' | '(' | ')' | ',' | ';' | ':')
        });
        if is_note_path(cleaned) {
            return Some(ProducedArtifact {
                title: human_title_from_path(cleaned).unwrap_or_else(|| "Note".to_string()),
                path: cleaned.to_string(),
                kind: ArtifactKind::Note,
                slug: slug_from_note_path(cleaned).unwrap_or_default().to_string(),
            });
        }
        if cleaned.starts_with("/downloads/") && is_image_path(cleaned) {
            return detect_image_at_path(&serde_json::json!({}), cleaned);
        }
        if cleaned.starts_with("/downloads/") {
            return Some(ProducedArtifact {
                title: human_title_from_path(cleaned).unwrap_or_else(|| "Document".to_string()),
                path: cleaned.to_string(),
                kind: ArtifactKind::Document,
                slug: String::new(),
            });
        }
    }
    None
}

fn parse_json_payload(s: &str) -> Option<serde_json::Value> {
    let trimmed = s.trim();
    if !trimmed.starts_with('{') {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

fn extract_image_path(s: &str) -> Option<String> {
    let lower = s.to_ascii_lowercase();
    if !lower.contains("image") {
        return None;
    }
    for token in s.split_whitespace() {
        let cleaned = token.trim_matches(|c: char| {
            matches!(c, '`' | '"' | '\'' | '(' | ')' | ',' | ';')
        });
        if is_image_path(cleaned) {
            return Some(cleaned.to_string());
        }
    }
    None
}

fn is_note_path(path: &str) -> bool {
    path.starts_with("/documents/notes/")
        && path.ends_with(".md")
        && !path.ends_with("/.graph.json")
}

fn is_image_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp")
        || lower.ends_with(".gif")
}

fn slug_from_note_path(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next()?;
    name.strip_suffix(".md")
}

fn human_title_from_path(path: &str) -> Option<String> {
    let name = path.rsplit('/').next()?.trim();
    if name.is_empty() {
        return None;
    }
    let stem = name
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(name);
    if stem.is_empty() {
        return None;
    }
    Some(stem.replace('-', " ").replace('_', " "))
}

fn path_tokens(path: &str) -> Vec<String> {
    let mut tokens = vec![path.to_string()];
    if let Some(name) = path.rsplit('/').next() {
        tokens.push(name.to_string());
    }
    tokens
}

fn replace_token(text: &str, token: &str) -> String {
    if token.is_empty() {
        return text.to_string();
    }
    let mut out = text.replace(token, "");
    let ticked = format!("`{token}`");
    out = out.replace(&ticked, "");
    out
}

fn strip_tool_id_leaks(text: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.contains("notes.create")
            || lower.contains("notes.update")
            || lower.contains("notes.read")
            || lower.contains("notes.list")
            || lower.contains("files.generate")
            || lower.contains("media.image.generate")
        {
            continue;
        }
        if trimmed.starts_with('/') && trimmed.chars().all(|c| !c.is_whitespace()) {
            continue;
        }
        lines.push(trimmed.to_string());
    }
    lines.join("\n")
}

fn collapse_blank_lines(text: String) -> String {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    format!("{}…", s.chars().take(max).collect::<String>())
}

fn looks_like_tool_failure(tool_result: &str) -> bool {
    let lower = tool_result.to_ascii_lowercase();
    lower.contains("erreur outil")
        || lower.contains("err:")
        || lower.contains("outil inconnu")
        || lower.starts_with("permissiondenied")
        || lower.contains("room_host_path_disallowed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::AgentStepRecord;

    #[test]
    fn detect_note_from_create_json() {
        let result = r#"{"path":"/documents/notes/cohort.md","slug":"cohort","title":"Cohort plan"}"#;
        let art = detect_from_tool("notes.create", &serde_json::json!({}), result).unwrap();
        assert_eq!(art.kind, ArtifactKind::Note);
        assert_eq!(art.title, "Cohort plan");
        assert_eq!(art.path, "/documents/notes/cohort.md");
        assert_eq!(art.slug, "cohort");
    }

    #[test]
    fn detect_document_from_files_generate() {
        let args = serde_json::json!({
            "path": "/downloads/research-sota.md",
            "content": "# SOTA"
        });
        let art = detect_from_tool(
            "files.generate",
            &args,
            r#"{"path":"/downloads/research-sota.md"}"#,
        )
        .unwrap();
        assert_eq!(art.kind, ArtifactKind::Document);
        assert_eq!(art.title, "research sota");
        assert!(!art.path.contains("notes.create"));
    }

    #[test]
    fn detect_image_from_media_result() {
        let art = detect_from_tool(
            "media.image.generate",
            &serde_json::json!({"prompt": "A red fox"}),
            "image /downloads/fox.png (1200 octets, moteur sdcpp)",
        )
        .unwrap();
        assert_eq!(art.kind, ArtifactKind::Image);
        assert_eq!(art.path, "/downloads/fox.png");
        assert_eq!(art.title, "A red fox");
    }

    #[test]
    fn strip_paths_and_tool_ids_from_prose() {
        let arts = vec![ProducedArtifact {
            title: "Cohort plan".into(),
            path: "/documents/notes/cohort.md".into(),
            kind: ArtifactKind::Note,
            slug: "cohort".into(),
        }];
        let raw = "J'ai créé la note.\n✓ `notes.create` → `/documents/notes/cohort.md`\nVoir cohort.md";
        let cleaned = strip_paths_from_prose(raw, &arts);
        assert!(!cleaned.contains("/documents/"));
        assert!(!cleaned.contains("notes.create"));
        assert!(!cleaned.contains("cohort.md"));
        assert!(cleaned.contains("J'ai créé la note."));
    }

    #[test]
    fn attachment_has_no_tool_id_fields() {
        let art = ProducedArtifact {
            title: "Report".into(),
            path: "/downloads/report.md".into(),
            kind: ArtifactKind::Document,
            slug: String::new(),
        };
        let att = art.attachment();
        let json = serde_json::to_string(&att).unwrap();
        assert!(!json.contains("files.generate"));
        assert!(!json.contains("notes."));
        match att {
            ChatAttachment::ArtifactCard {
                title,
                artifact_type,
                path,
                ..
            } => {
                assert_eq!(title, "Report");
                assert_eq!(artifact_type, "document");
                assert_eq!(path, "/downloads/report.md");
            }
            _ => panic!("expected ArtifactCard"),
        }
    }

    #[test]
    fn artifacts_from_trace_dedupes_by_path() {
        let trace = vec![
            AgentStepRecord {
                step: 1,
                action: "notes.create".into(),
                args: serde_json::json!({"title": "Draft"}),
                tool_result: r#"{"path":"/documents/notes/x.md","slug":"x","title":"Draft"}"#
                    .into(),
                ..Default::default()
            },
            AgentStepRecord {
                step: 2,
                action: "notes.update".into(),
                tool_result: r#"{"path":"/documents/notes/x.md","slug":"x","title":"Final title"}"#
                    .into(),
                ..Default::default()
            },
        ];
        let arts = artifacts_from_trace(&trace);
        assert_eq!(arts.len(), 1);
        assert_eq!(arts[0].title, "Final title");
    }
}
