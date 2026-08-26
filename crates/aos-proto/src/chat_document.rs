//! Chat document attach — text extraction and infer-message composition.
//!
//! Documents are **not** vision images: readable text is extracted and merged into the
//! user turn before `model.infer`, so text-only models can answer about attached files.

use crate::DocumentRef;
use std::path::Path;

/// Max pending documents per chat turn (mirrors image queue).
pub const CHAT_MAX_PENDING_DOCUMENTS: usize = 4;

/// Per-document extracted text cap (chars).
pub const CHAT_DOCUMENT_MAX_CHARS_PER_DOC: usize = 12_000;

/// Total extracted text cap across all documents in one turn.
pub const CHAT_DOCUMENT_MAX_CHARS_TOTAL: usize = 24_000;

/// File extensions accepted for chat document attach.
pub const CHAT_DOCUMENT_EXTENSIONS: &[&str] = &["pdf", "txt", "md", "markdown", "text"];

/// Label for UI chips and prompt blocks (basename only — never the full path).
pub fn document_label_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("document")
        .to_string()
}

/// Whether the path extension is an accepted chat document type.
pub fn is_chat_document_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|ext| {
            CHAT_DOCUMENT_EXTENSIONS
                .iter()
                .any(|allowed| ext.eq_ignore_ascii_case(allowed))
        })
        .unwrap_or(false)
}

/// Extract readable text from a local document path.
pub fn extract_document_text(path: &str) -> Result<String, String> {
    let p = Path::new(path);
    if !p.is_file() {
        return Err(format!("not a file: {path}"));
    }
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "txt" | "md" | "markdown" | "text" => std::fs::read_to_string(path)
            .map_err(|e| format!("read failed: {e}")),
        "pdf" => extract_pdf_text(path),
        other => Err(format!("unsupported document type: {other}")),
    }
}

fn extract_pdf_text(path: &str) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read failed: {e}"))?;
    pdf_extract::extract_text_from_mem(&bytes).map_err(|e| format!("pdf extract failed: {e}"))
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push_str("\n… [truncated]");
    out
}

/// Merge extracted document text into the user message for infer / session persistence.
pub fn merge_documents_into_user_content(user_text: &str, docs: &[DocumentRef]) -> String {
    if docs.is_empty() {
        return user_text.to_string();
    }
    let mut blocks = Vec::new();
    let mut total_budget = CHAT_DOCUMENT_MAX_CHARS_TOTAL;
    for doc in docs {
        if total_budget == 0 {
            blocks.push(format!(
                "[Document: {} — skipped (total size cap reached)]",
                doc.label
            ));
            continue;
        }
        let per_doc = total_budget.min(CHAT_DOCUMENT_MAX_CHARS_PER_DOC);
        match extract_document_text(&doc.path) {
            Ok(text) => {
                let excerpt = truncate_chars(text.trim(), per_doc);
                let used = excerpt.chars().count().min(per_doc);
                total_budget = total_budget.saturating_sub(used);
                blocks.push(format!("[Document: {}]\n{excerpt}", doc.label));
            }
            Err(err) => {
                blocks.push(format!("[Document: {} — unreadable: {err}]", doc.label));
            }
        }
    }
    let doc_block = blocks.join("\n\n");
    if user_text.trim().is_empty() {
        doc_block
    } else {
        format!("{user_text}\n\n{doc_block}")
    }
}

/// Patch the last `user` message in an infer history with merged document text.
pub fn apply_documents_to_infer_messages(
    messages: &mut [crate::ChatMessage],
    documents: &[DocumentRef],
) {
    if documents.is_empty() {
        return;
    }
    let Some(last_user) = messages
        .iter_mut()
        .rev()
        .find(|m| m.role == "user")
    else {
        return;
    };
    last_user.content = merge_documents_into_user_content(&last_user.content, documents);
}

/// Build infer `messages` tail content for a chat turn (unit-tested infer/send path).
pub fn chat_infer_user_content(
    history: &[(String, String)],
    user_text: &str,
    documents: &[DocumentRef],
) -> String {
    let mut messages: Vec<crate::ChatMessage> = history
        .iter()
        .map(|(role, content)| crate::ChatMessage {
            role: role.clone(),
            content: content.clone(),
        })
        .collect();
    apply_documents_to_infer_messages(&mut messages, documents);
    messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.clone())
        .unwrap_or_else(|| merge_documents_into_user_content(user_text, documents))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_temp(ext: &str, body: &str) -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "aos-chat-doc-{}-{}-{}.{}",
            ext,
            std::process::id(),
            n,
            ext
        ));
        std::fs::write(&path, body).expect("write temp");
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn extracts_plain_text_file() {
        let path = write_temp("txt", "Hello from the attached note.");
        let text = extract_document_text(&path).expect("txt extract");
        assert!(text.contains("Hello from the attached note."));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn merge_includes_document_excerpt_in_user_turn() {
        let path = write_temp("md", "Quarterly revenue grew 12%.");
        let docs = vec![DocumentRef {
            path: path.clone(),
            label: "merge.md".into(),
        }];
        let merged = merge_documents_into_user_content("Summarize this.", &docs);
        assert!(merged.contains("Summarize this."));
        assert!(merged.contains("[Document: merge.md]"));
        assert!(merged.contains("Quarterly revenue grew 12%."));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn infer_path_receives_extracted_document_text() {
        let path = write_temp("txt", "SECRET_PHRASE_FOR_INFER_TEST");
        let docs = vec![DocumentRef {
            path: path.clone(),
            label: "infer.txt".into(),
        }];
        let history = vec![
            ("user".into(), "What does the file say?".into()),
        ];
        let content = chat_infer_user_content(&history, "What does the file say?", &docs);
        assert!(content.contains("SECRET_PHRASE_FOR_INFER_TEST"));
        assert!(content.contains("What does the file say?"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn merge_document_only_uses_extracted_text() {
        let path = write_temp("txt", "Standalone document body.");
        let docs = vec![DocumentRef {
            path: path.clone(),
            label: "note.txt".into(),
        }];
        let merged = merge_documents_into_user_content("", &docs);
        assert!(!merged.contains("What does this document say?"));
        assert!(!merged.contains("Que dit ce document"));
        assert!(merged.contains("Standalone document body."));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn document_label_is_basename_only() {
        assert_eq!(
            document_label_from_path("/home/user/Downloads/quarterly.pdf"),
            "quarterly.pdf"
        );
        assert_eq!(document_label_from_path("notes.md"), "notes.md");
        assert!(!document_label_from_path("/home/user/Downloads/quarterly.pdf")
            .contains("/home/"));
    }

    #[test]
    fn rejects_unknown_extension() {
        let path = write_temp("bin", "\0\x01\x02");
        let err = extract_document_text(&path).unwrap_err();
        assert!(err.contains("unsupported"));
        let _ = std::fs::remove_file(&path);
    }
}
