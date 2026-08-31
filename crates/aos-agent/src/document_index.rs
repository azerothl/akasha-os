//! Persisted index of prepared research documents (recoverable outside chat).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResearchDocumentEntry {
    pub question: String,
    pub path: String,
    #[serde(default)]
    pub label: String,
    pub created_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ResearchDocumentIndex {
    #[serde(default)]
    entries: Vec<ResearchDocumentEntry>,
}

fn index_path(home: &Path) -> PathBuf {
    home.join("var/documents/research-index.json")
}

fn index_dir(home: &Path) -> PathBuf {
    home.join("var/documents")
}

/// Load all indexed research documents, newest first.
pub fn load_research_documents(home: &Path) -> Vec<ResearchDocumentEntry> {
    let path = index_path(home);
    let raw = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mut index: ResearchDocumentIndex = serde_json::from_str(&raw).unwrap_or_default();
    index.entries.sort_by(|a, b| b.created_ms.cmp(&a.created_ms));
    index.entries
}

/// Append or update an entry for `path` (dedupe by path).
pub fn record_research_document(
    home: &Path,
    question: &str,
    path: &str,
    label: &str,
    created_ms: u64,
) -> Result<(), String> {
    if !path.starts_with("/downloads/") {
        return Err("research documents must live under /downloads/".into());
    }
    let dir = index_dir(home);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut entries = load_research_documents(home);
    entries.retain(|e| e.path != path);
    entries.push(ResearchDocumentEntry {
        question: question.trim().to_string(),
        path: path.to_string(),
        label: if label.trim().is_empty() {
            path.rsplit('/').next().unwrap_or(path).to_string()
        } else {
            label.trim().to_string()
        },
        created_ms,
    });
    entries.sort_by(|a, b| b.created_ms.cmp(&a.created_ms));
    let index = ResearchDocumentIndex { entries };
    let json = serde_json::to_string_pretty(&index).map_err(|e| e.to_string())?;
    fs::write(index_path(home), json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_home() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "aos-research-index-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        base
    }

    #[test]
    fn record_and_load_roundtrip() {
        let home = temp_home();
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        record_research_document(
            &home,
            "What is agentic?",
            "/downloads/research-agentic.md",
            "research-agentic.md",
            ms,
        )
        .expect("record");
        let list = load_research_documents(&home);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].question, "What is agentic?");
        assert_eq!(list[0].path, "/downloads/research-agentic.md");
        let _ = fs::remove_dir_all(&home);
    }
}
