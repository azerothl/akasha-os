//! User document library — chunk + embed into namespace `user:docs`.
//!
//! Separate from product RAG (`product:docs`). Files live under
//! `{memory_dir}/library/`; retrieval is consultative via `mem.context`.

use crate::memory::MemoryStore;
use crate::subsystem::PlatformSubsystem;
use aos_proto::chat_document;
use aos_proto::UserLibraryDoc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const USER_DOCS_NS: &str = "user:docs";

const MANIFEST_FILE: &str = "manifest.json";
const FILES_DIR: &str = "files";
const CHUNK_SOFT_MAX: usize = 900;
const CHUNK_HARD_MAX: usize = 1400;
const MIN_CHUNK_CHARS: usize = 48;
const DEFAULT_USER_DOC_K: usize = 3;
const MIN_SCORE: f32 = 0.22;
const PROMPT_BUDGET: usize = 2400;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Manifest {
    #[serde(default)]
    docs: Vec<UserLibraryDoc>,
}

pub fn library_root(memory_dir: &Path) -> PathBuf {
    memory_dir.join("library")
}

fn manifest_path(memory_dir: &Path) -> PathBuf {
    library_root(memory_dir).join(MANIFEST_FILE)
}

fn files_dir(memory_dir: &Path) -> PathBuf {
    library_root(memory_dir).join(FILES_DIR)
}

fn load_manifest(memory_dir: &Path) -> Manifest {
    let path = manifest_path(memory_dir);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_manifest(memory_dir: &Path, manifest: &Manifest) -> Result<(), String> {
    let root = library_root(memory_dir);
    std::fs::create_dir_all(&root).map_err(|e| format!("library dir: {e}"))?;
    let path = manifest_path(memory_dir);
    let body = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| format!("manifest write: {e}"))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn doc_id_for(label: &str, bytes: &[u8]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    label.hash(&mut hasher);
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn sanitize_filename(label: &str) -> String {
    let mut out = String::new();
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "document".into()
    } else {
        out
    }
}

/// List documents in the user library (manifest only). Backfills `added_date` once.
pub fn list_docs(memory_dir: &Path) -> Vec<UserLibraryDoc> {
    let mut manifest = load_manifest(memory_dir);
    let mut dirty = false;
    for doc in &mut manifest.docs {
        if doc.added_date.is_empty() && doc.added_ms > 0 {
            if let Some(d) = format_utc_date(doc.added_ms) {
                doc.added_date = d;
                dirty = true;
            }
        }
    }
    if dirty {
        let _ = save_manifest(memory_dir, &manifest);
    }
    let mut docs = manifest.docs;
    docs.sort_by_key(|a| std::cmp::Reverse(a.added_ms));
    docs
}

/// Add a local document to the library, copy it, and index chunks.
pub fn add_document(
    sub: &PlatformSubsystem,
    memory_dir: &Path,
    source_path: &str,
) -> Result<(UserLibraryDoc, usize), String> {
    if !chat_document::is_chat_document_path(source_path) {
        return Err("unsupported document type (pdf, txt, md)".into());
    }
    let src = Path::new(source_path);
    if !src.is_file() {
        return Err(format!("not a file: {source_path}"));
    }
    let label = chat_document::document_label_from_path(source_path);
    let bytes = std::fs::read(src).map_err(|e| format!("read failed: {e}"))?;
    let id = doc_id_for(&label, &bytes);
    let stored_name = format!("{}_{}", id, sanitize_filename(&label));
    let dest = files_dir(memory_dir).join(&stored_name);
    std::fs::create_dir_all(files_dir(memory_dir)).map_err(|e| e.to_string())?;
    std::fs::copy(src, &dest).map_err(|e| format!("copy failed: {e}"))?;

    let text = chat_document::extract_document_text(source_path)?;
    let chunks = index_document(sub, memory_dir, &id, &label, &text)?;

    let added_ms = now_ms();
    let added_date = format_utc_date(added_ms).unwrap_or_default();

    let doc = UserLibraryDoc {
        id: id.clone(),
        label,
        added_ms,
        size_bytes: bytes.len() as u64,
        added_date,
    };
    let mut manifest = load_manifest(memory_dir);
    manifest.docs.retain(|d| d.id != id);
    manifest.docs.push(doc.clone());
    save_manifest(memory_dir, &manifest)?;
    Ok((doc, chunks))
}

/// Remove a document from the library and wipe its indexed chunks.
pub fn remove_document(
    sub: &PlatformSubsystem,
    memory_dir: &Path,
    id: &str,
) -> Result<(), String> {
    let mut manifest = load_manifest(memory_dir);
    let Some(pos) = manifest.docs.iter().position(|d| d.id == id) else {
        return Err("document not found".into());
    };
    let doc = manifest.docs.remove(pos);
    save_manifest(memory_dir, &manifest)?;

    if let Ok(entries) = std::fs::read_dir(files_dir(memory_dir)) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(&format!("{}_", doc.id)) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    let mut mem = sub.mem.lock().unwrap();
    mem.episodic_delete_by_meta(USER_DOCS_NS, "doc_id", id);
    Ok(())
}

/// Re-index all manifest documents (startup recovery).
pub fn ensure_indexed(sub: &PlatformSubsystem, memory_dir: &Path) -> usize {
    let docs = list_docs(memory_dir);
    let mut total = 0usize;
    for doc in docs {
        let stored = find_stored_file(memory_dir, &doc.id);
        let Some(path) = stored else { continue };
        let text = match chat_document::extract_document_text(path.to_str().unwrap_or("")) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[aos-platformd] user library skip {}: {e}", doc.label);
                continue;
            }
        };
        match index_document(sub, memory_dir, &doc.id, &doc.label, &text) {
            Ok(n) => total += n,
            Err(e) => eprintln!("[aos-platformd] user library index {}: {e}", doc.label),
        }
    }
    total
}

fn find_stored_file(memory_dir: &Path, doc_id: &str) -> Option<PathBuf> {
    let dir = files_dir(memory_dir);
    let entries = std::fs::read_dir(&dir).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&format!("{doc_id}_")) {
            return Some(entry.path());
        }
    }
    None
}

fn index_document(
    sub: &PlatformSubsystem,
    memory_dir: &Path,
    doc_id: &str,
    label: &str,
    text: &str,
) -> Result<usize, String> {
    let is_markdown = label.ends_with(".md") || label.ends_with(".markdown");
    let chunks = if is_markdown {
        chunk_markdown(label, text)
    } else {
        chunk_plain(label, text)
    };
    if chunks.is_empty() {
        return Err("no indexable text".into());
    }

    {
        let mut mem = sub.mem.lock().unwrap();
        mem.episodic_delete_by_meta(USER_DOCS_NS, "doc_id", doc_id);
    }

    let mut written = 0usize;
    for (i, chunk) in chunks.iter().enumerate() {
        let vector = match sub.embed_text(&chunk.text) {
            Ok(v) if !v.is_empty() => v,
            Ok(_) => continue,
            Err(e) => {
                eprintln!("[aos-platformd] user library embed skip: {e}");
                continue;
            }
        };
        let metadata = serde_json::json!({
            "doc_id": doc_id,
            "label": label,
            "heading": chunk.heading,
            "chunk": i,
            "kind": "user_doc",
        });
        let mut mem = sub.mem.lock().unwrap();
        mem.episodic_write(USER_DOCS_NS, &chunk.text, metadata, vector, false);
        written += 1;
    }
    let _ = memory_dir;
    if written == 0 {
        return Err("embeddings unavailable".into());
    }
    Ok(written)
}

#[derive(Debug, Clone)]
struct DocChunk {
    heading: String,
    text: String,
}

fn chunk_markdown(source: &str, md: &str) -> Vec<DocChunk> {
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut heading = String::from("(intro)");
    let mut body = String::new();
    for line in md.lines() {
        if line.starts_with("## ") || line.starts_with("### ") {
            if !body.trim().is_empty() {
                sections.push((heading.clone(), std::mem::take(&mut body)));
            }
            heading = line.trim_start_matches('#').trim().to_string();
        } else {
            body.push_str(line);
            body.push('\n');
        }
    }
    if !body.trim().is_empty() {
        sections.push((heading, body));
    }

    let mut out = Vec::new();
    for (heading, body) in sections {
        for piece in split_oversized(&body) {
            let trimmed = piece.trim();
            if trimmed.chars().count() < MIN_CHUNK_CHARS {
                continue;
            }
            let text = format!("[{source} / {heading}]\n{trimmed}");
            out.push(DocChunk {
                heading: heading.clone(),
                text,
            });
        }
    }
    out
}

fn chunk_plain(source: &str, body: &str) -> Vec<DocChunk> {
    let mut out = Vec::new();
    for (i, piece) in split_oversized(body).into_iter().enumerate() {
        let trimmed = piece.trim();
        if trimmed.chars().count() < MIN_CHUNK_CHARS {
            continue;
        }
        let heading = format!("part {i}");
        let text = format!("[{source}]\n{trimmed}");
        out.push(DocChunk { heading, text });
    }
    out
}

fn split_oversized(body: &str) -> Vec<String> {
    let chars: Vec<char> = body.chars().collect();
    if chars.len() <= CHUNK_HARD_MAX {
        return vec![body.to_string()];
    }
    let mut out = Vec::new();
    let mut start = 0usize;
    while start < chars.len() {
        let soft_end = (start + CHUNK_SOFT_MAX).min(chars.len());
        let hard_end = (start + CHUNK_HARD_MAX).min(chars.len());
        let mut end = hard_end;
        if hard_end < chars.len() {
            let window: String = chars[soft_end..hard_end].iter().collect();
            if let Some(rel) = window.rfind("\n\n") {
                end = soft_end + rel + 2;
            } else if let Some(rel) = window.rfind('\n') {
                end = soft_end + rel + 1;
            }
        }
        if end <= start {
            end = hard_end;
        }
        out.push(chars[start..end].iter().collect());
        start = end;
    }
    out
}

/// Semantic recall over user library (no similar-hop expansion).
pub fn recall(mem: &MemoryStore, query_vector: &[f32], k: usize) -> Vec<aos_proto::MemHit> {
    if query_vector.is_empty() {
        return Vec::new();
    }
    let k = if k == 0 { DEFAULT_USER_DOC_K } else { k };
    mem.episodic_query_raw(query_vector, k, Some(USER_DOCS_NS), false, false)
        .into_iter()
        .filter(|h| h.score >= MIN_SCORE)
        .collect()
}

/// Format hits for the chat system prompt (budget-capped, no RAG jargon).
pub fn format_prompt_block(hits: &[aos_proto::MemHit]) -> String {
    if hits.is_empty() {
        return String::new();
    }
    let mut out = String::from("Bibliothèque personnelle (extraits utiles) :\n");
    for h in hits {
        let label = h
            .metadata
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("document");
        let heading = h
            .metadata
            .get("heading")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let header = if heading.is_empty() {
            format!("### {label}\n")
        } else {
            format!("### {label} — {heading}\n")
        };
        if out.len() + header.len() + h.text.len() + 2 > PROMPT_BUDGET {
            break;
        }
        out.push_str(&header);
        out.push_str(&h.text);
        out.push_str("\n\n");
    }
    out
}

/// UTC calendar date (`YYYY-MM-DD`) from epoch ms — pure function, offset 0 only.
pub fn format_utc_date(added_ms: u64) -> Option<String> {
    if added_ms == 0 {
        return None;
    }
    let days = (added_ms as i64 / 1000).div_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_097 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = (yoe + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (mp + if mp < 10 { 3 } else { -9 }) as u32;
    if m <= 2 {
        y += 1;
    }
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryStore;

    fn v(x: f32) -> Vec<f32> {
        vec![x, 1.0 - x, 0.5]
    }

    fn temp_memory_dir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let mut p = std::env::temp_dir();
        p.push(format!("aos-user-lib-{}-{}", std::process::id(), n));
        let _ = std::fs::create_dir_all(&p);
        p
    }

    #[test]
    fn empty_library_recall_is_empty() {
        let dir = temp_memory_dir();
        let mem = MemoryStore::open(&dir).unwrap();
        let hits = recall(&mem, &v(0.9), 3);
        assert!(hits.is_empty());
        assert!(format_prompt_block(&hits).is_empty());
    }

    #[test]
    fn manifest_roundtrip() {
        let dir = temp_memory_dir();
        let doc = UserLibraryDoc {
            id: "abc".into(),
            label: "note.txt".into(),
            added_ms: 1_788_004_800_000,
            size_bytes: 42,
            added_date: "2026-08-29".into(),
        };
        save_manifest(&dir, &Manifest { docs: vec![doc.clone()] }).unwrap();
        let listed = list_docs(&dir);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0], doc);
    }

    #[test]
    fn same_added_ms_formats_identically() {
        let ms = 1_788_004_800_000;
        let a = format_utc_date(ms).expect("date");
        let b = format_utc_date(ms).expect("date");
        assert_eq!(a, b);
        assert_eq!(a, "2026-08-29");
    }

    #[test]
    fn aug_29_2026_noon_utc() {
        assert_eq!(
            format_utc_date(1_788_004_800_000).as_deref(),
            Some("2026-08-29")
        );
    }

    #[test]
    fn list_backfills_added_date_once() {
        let dir = temp_memory_dir();
        let doc = UserLibraryDoc {
            id: "abc".into(),
            label: "note.txt".into(),
            added_ms: 1_788_004_800_000,
            size_bytes: 42,
            added_date: String::new(),
        };
        save_manifest(&dir, &Manifest { docs: vec![doc] }).unwrap();
        let listed = list_docs(&dir);
        assert_eq!(listed[0].added_date, "2026-08-29");
        let listed2 = list_docs(&dir);
        assert_eq!(listed2[0].added_date, "2026-08-29");
    }

    #[test]
    fn indexed_doc_yields_hit() {
        let dir = temp_memory_dir();
        let mut mem = MemoryStore::open(&dir).unwrap();
        let phrase = "USER_LIBRARY_UNIQUE_PHRASE_FOR_RETRIEVAL_TEST";
        let text = format!("{phrase} and enough padding text to pass the minimum chunk size threshold easily.");
        mem.episodic_write(
            USER_DOCS_NS,
            &format!("[test.txt]\n{text}"),
            serde_json::json!({
                "doc_id": "doc1",
                "label": "test.txt",
                "heading": "part 0",
                "kind": "user_doc",
            }),
            v(0.91),
            false,
        );
        let hits = recall(&mem, &v(0.9), 3);
        assert!(!hits.is_empty());
        assert!(hits[0].text.contains(phrase));
    }

    #[test]
    fn product_namespace_untouched() {
        let dir = temp_memory_dir();
        let mut mem = MemoryStore::open(&dir).unwrap();
        mem.episodic_write(
            crate::product_rag::PRODUCT_NS,
            "product only",
            serde_json::json!({"kind": "product_doc"}),
            v(0.5),
            true,
        );
        mem.episodic_write(
            USER_DOCS_NS,
            "user only",
            serde_json::json!({"kind": "user_doc"}),
            v(0.5),
            false,
        );
        assert_eq!(mem.list(crate::product_rag::PRODUCT_NS, false).len(), 1);
        assert_eq!(mem.list(USER_DOCS_NS, false).len(), 1);
        let user_hits = recall(&mem, &v(0.5), 5);
        assert_eq!(user_hits.len(), 1);
        assert!(user_hits[0].text.contains("user only"));
        assert!(!user_hits[0].text.contains("product only"));
    }

    #[test]
    fn prompt_block_avoids_rag_jargon() {
        let hit = aos_proto::MemHit {
            id: 1,
            namespace: USER_DOCS_NS.into(),
            text: "Some excerpt".into(),
            score: 0.9,
            metadata: serde_json::json!({"label": "notes.md", "heading": "intro"}),
            pinned: false,
            kind: None,
            relations: vec![],
            superseded: false,
        };
        let block = format_prompt_block(&[hit]);
        let lower = block.to_lowercase();
        assert!(!lower.contains("rag"));
        assert!(!lower.contains("features"));
        assert!(!lower.contains("json"));
    }

    #[test]
    fn mem_context_empty_user_docs_adds_nothing() {
        let dir = temp_memory_dir();
        let mem = MemoryStore::open(&dir).unwrap();
        let emb = v(0.5);
        let user_doc_hits = recall(&mem, &emb, 3);
        let block = format_prompt_block(&user_doc_hits);
        assert!(user_doc_hits.is_empty());
        assert!(block.is_empty());
        // Consultative: empty hits must not produce a blocking prompt section.
    }
}
