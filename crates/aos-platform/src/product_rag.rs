//! Product-doc RAG : chunk markdown Preview → namespace `product:docs`.
//!
//! Réutilise l'index vectoriel épisodique + `embed_text`. Ré-indexe si la
//! version Preview ou le fingerprint des fichiers docs change.

use crate::memory::MemoryStore;
use crate::subsystem::PlatformSubsystem;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const PRODUCT_NS: &str = "product:docs";

const CHUNK_SOFT_MAX: usize = 900;
const CHUNK_HARD_MAX: usize = 1400;
const MIN_CHUNK_CHARS: usize = 48;
const META_FILE: &str = "product_rag.json";
const DEFAULT_PRODUCT_K: usize = 4;
const MIN_SCORE: f32 = 0.22;
const PROMPT_BUDGET: usize = 2800;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexMeta {
    version: String,
    fingerprint: String,
    chunks: usize,
}

#[derive(Debug, Clone)]
struct IndexedChunk {
    text: String,
    metadata: serde_json::Value,
    vector: Vec<f32>,
}

/// Ensure the product namespace is indexed for the current Preview docs.
pub fn ensure_indexed(sub: &PlatformSubsystem, version: &str) -> Result<usize, String> {
    let roots = docs_roots();
    let files = collect_doc_files(&roots);
    if files.is_empty() {
        return Err("aucun docs/FEATURES.md trouvé (AOS_HOME ou cwd)".into());
    }
    let fingerprint = fingerprint_files(&files);
    let meta_path = {
        let mem = sub.mem.lock().unwrap();
        mem.dir().join(META_FILE)
    };
    if let Ok(raw) = std::fs::read_to_string(&meta_path) {
        if let Ok(prev) = serde_json::from_str::<IndexMeta>(&raw) {
            if prev.version == version && prev.fingerprint == fingerprint && prev.chunks > 0 {
                let n = sub
                    .mem
                    .lock()
                    .unwrap()
                    .list(PRODUCT_NS, false)
                    .len();
                if n > 0 {
                    return Ok(n);
                }
            }
        }
    }

    let prepared = prepare_index(sub, version, &files)?;
    commit_index(sub, version, fingerprint, &prepared, &meta_path)
}

/// Embed all doc chunks without touching the live `product:docs` namespace.
fn prepare_index(
    sub: &PlatformSubsystem,
    version: &str,
    files: &[PathBuf],
) -> Result<Vec<IndexedChunk>, String> {
    // Probe embeddings before any destructive work.
    let probe = sub
        .embed_text("Akasha OS Preview")
        .map_err(|e| format!("product RAG: embeddings indisponibles ({e})"))?;
    if probe.is_empty() {
        return Err("product RAG: vecteur d'embedding vide".into());
    }

    let chunks = build_chunks(files);
    if chunks.is_empty() {
        return Err("product RAG: aucun chunk".into());
    }

    let mut prepared = Vec::with_capacity(chunks.len());
    for (i, chunk) in chunks.iter().enumerate() {
        let vector = match sub.embed_text(&chunk.text) {
            Ok(v) if !v.is_empty() => v,
            Ok(_) => continue,
            Err(e) => {
                eprintln!("[aos-platformd] product RAG embed skip: {e}");
                continue;
            }
        };
        prepared.push(IndexedChunk {
            text: chunk.text.clone(),
            metadata: serde_json::json!({
                "source": chunk.source,
                "heading": chunk.heading,
                "chunk": i,
                "version": version,
                "kind": "product_doc",
            }),
            vector,
        });
    }
    if prepared.is_empty() {
        return Err("product RAG: aucun chunk indexé".into());
    }
    Ok(prepared)
}

/// Replace `product:docs` with prepared chunks and stamp metadata.
fn commit_index(
    sub: &PlatformSubsystem,
    version: &str,
    fingerprint: String,
    prepared: &[IndexedChunk],
    meta_path: &std::path::Path,
) -> Result<usize, String> {
    {
        let mut mem = sub.mem.lock().unwrap();
        let _ = mem.wipe(PRODUCT_NS);
    }

    let mut written = 0usize;
    for chunk in prepared {
        let mut mem = sub.mem.lock().unwrap();
        mem.episodic_write(
            PRODUCT_NS,
            &chunk.text,
            chunk.metadata.clone(),
            chunk.vector.clone(),
            true,
        );
        written += 1;
    }

    let meta = IndexMeta {
        version: version.into(),
        fingerprint,
        chunks: written,
    };
    if let Some(parent) = meta_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(
        meta_path,
        serde_json::to_string_pretty(&meta).unwrap_or_default(),
    );
    Ok(written)
}

/// Semantic recall over product docs (no similar-hop expansion).
pub fn recall(
    mem: &MemoryStore,
    query_vector: &[f32],
    k: usize,
) -> Vec<aos_proto::MemHit> {
    if query_vector.is_empty() {
        return Vec::new();
    }
    let k = if k == 0 { DEFAULT_PRODUCT_K } else { k };
    mem.episodic_query_raw(query_vector, k, Some(PRODUCT_NS), false, false)
        .into_iter()
        .filter(|h| h.score >= MIN_SCORE)
        .collect()
}

/// Format hits for the chat system prompt (budget-capped).
pub fn format_prompt_block(hits: &[aos_proto::MemHit]) -> String {
    if hits.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "Documentation produit (extraits RAG — base-toi dessus pour UI / nouveautés) :\n",
    );
    for h in hits {
        let source = h
            .metadata
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("doc");
        let heading = h
            .metadata
            .get("heading")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let header = if heading.is_empty() {
            format!("### {source}\n")
        } else {
            format!("### {source} — {heading}\n")
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

fn docs_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(home) = std::env::var("AOS_HOME") {
        roots.push(PathBuf::from(home).join("docs"));
    }
    // Preview install layout often sets cwd to the Preview root.
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd.join("docs"));
        // Dev: platformd started from repo root.
        if cwd.join("crates").is_dir() {
            roots.push(cwd.join("docs"));
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

fn collect_doc_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    let names = [
        "FEATURES.md",
        "STATUS.md",
        "TESTER.md",
        "fr/FEATURES.md",
        "fr/STATUS.md",
        "fr/TESTER.md",
    ];
    let mut out = Vec::new();
    for root in roots {
        for name in names {
            let p = root.join(name);
            if p.is_file() {
                out.push(p);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn fingerprint_files(files: &[PathBuf]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    for p in files {
        p.to_string_lossy().hash(&mut hasher);
        if let Ok(meta) = std::fs::metadata(p) {
            meta.len().hash(&mut hasher);
            if let Ok(m) = meta.modified() {
                if let Ok(d) = m.duration_since(std::time::UNIX_EPOCH) {
                    d.as_secs().hash(&mut hasher);
                }
            }
        }
    }
    format!("{:x}", hasher.finish())
}

#[derive(Debug, Clone)]
struct DocChunk {
    source: String,
    heading: String,
    text: String,
}

fn build_chunks(files: &[PathBuf]) -> Vec<DocChunk> {
    let mut out = Vec::new();
    for path in files {
        let Ok(raw) = std::fs::read_to_string(path) else {
            continue;
        };
        let source = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("doc")
            .to_string();
        // Prefer relative-ish label when under fr/
        let source = if path.components().any(|c| c.as_os_str() == "fr") {
            format!("fr/{source}")
        } else {
            source
        };
        out.extend(chunk_markdown(&source, &raw));
    }
    out
}

/// Split markdown on headings; further split oversized sections.
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
                source: source.into(),
                heading: heading.clone(),
                text,
            });
        }
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
            // Prefer paragraph break between soft and hard.
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

#[cfg(test)]
mod tests {
    use super::chunk_markdown;
    use super::{commit_index, prepare_index, IndexedChunk, IndexMeta, META_FILE, PRODUCT_NS};
    use crate::subsystem::{PlatformConfig, PlatformSubsystem};
    use std::path::PathBuf;

    fn temp_path(label: &str) -> String {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "aos-product-rag-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&p);
        p.display().to_string()
    }

    fn seed_docs_home() -> (String, String) {
        let home = temp_path("home");
        let docs = PathBuf::from(&home).join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(
            docs.join("FEATURES.md"),
            "## Preview\n\nAkasha OS Preview features with enough characters for chunk indexing tests.\n",
        )
        .unwrap();
        std::env::set_var("AOS_HOME", &home);
        let memory_dir = PathBuf::from(&home).join("memory").display().to_string();
        (home, memory_dir)
    }

    fn test_config(memory_dir: &str) -> PlatformConfig {
        PlatformConfig {
            bus: "ipc://test".into(),
            audit_dir: temp_path("audit"),
            storage_dir: temp_path("storage"),
            memory_dir: memory_dir.into(),
            modules_dir: temp_path("modules"),
            catalogue_file: "/dev/null".into(),
            skills_dir: temp_path("skills"),
            sessions_dir: temp_path("sessions"),
            embed_model: None,
            policies_file: None,
            confirm_timeout_sec: 60,
            secrets_file: PathBuf::from(temp_path("secrets"))
                .join("secrets")
                .display()
                .to_string(),
            net_mode: "online".into(),
        }
    }

    #[test]
    fn chunks_by_heading() {
        let md = "# Title\n\nintro para that is long enough to keep as a chunk abcdefghijklmnop\n\n## 1. Chat\n\nchat details go here with enough characters for the filter threshold xyz\n\n### What's new in 0.10.0\n\n- TPM vault\n- bridge aos-bridged\n- multi-GPU path and more text for length\n";
        let chunks = chunk_markdown("FEATURES.md", md);
        assert!(chunks.len() >= 2);
        assert!(chunks.iter().any(|c| c.heading.contains("0.10.0") || c.text.contains("TPM")));
    }

    #[tokio::test]
    async fn failed_reindex_keeps_existing_product_chunks() {
        let (_home, memory_dir) = seed_docs_home();
        let cfg = test_config(&memory_dir);
        let sub = PlatformSubsystem::open(&cfg).expect("platform open");
        {
            let mut mem = sub.mem.lock().unwrap();
            mem.episodic_write(
                PRODUCT_NS,
                "stale but usable product chunk with enough characters to count",
                serde_json::json!({"kind": "product_doc", "version": "0.14.0"}),
                vec![0.1, 0.2, 0.3],
                true,
            );
        }
        let meta_path = PathBuf::from(&memory_dir).join(META_FILE);
        let meta = IndexMeta {
            version: "0.14.0".into(),
            fingerprint: "old".into(),
            chunks: 1,
        };
        std::fs::write(
            &meta_path,
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();

        let before = sub.mem.lock().unwrap().list(PRODUCT_NS, false).len();
        assert_eq!(before, 1);

        let err = super::ensure_indexed(&sub, "0.15.0").unwrap_err();
        assert!(
            err.contains("embeddings") || err.contains("embedding"),
            "expected embed failure, got: {err}"
        );

        let after = sub.mem.lock().unwrap().list(PRODUCT_NS, false).len();
        assert_eq!(after, 1, "reindex must not wipe live chunks before commit");
    }

    #[tokio::test]
    async fn prepare_index_does_not_wipe_existing_chunks() {
        let memory_dir = temp_path("memory-prepare");
        let cfg = test_config(&memory_dir);
        let sub = PlatformSubsystem::open(&cfg).expect("platform open");
        {
            let mut mem = sub.mem.lock().unwrap();
            mem.episodic_write(
                PRODUCT_NS,
                "existing chunk preserved during slow embed phase",
                serde_json::json!({"kind": "product_doc"}),
                vec![0.5, 0.5],
                true,
            );
        }
        let roots = super::docs_roots();
        let files = super::collect_doc_files(&roots);
        if files.is_empty() {
            return;
        }
        let before = sub.mem.lock().unwrap().list(PRODUCT_NS, false).len();
        let _ = prepare_index(&sub, "0.15.0", &files);
        let after = sub.mem.lock().unwrap().list(PRODUCT_NS, false).len();
        assert_eq!(before, after, "prepare_index must not touch product:docs");
    }

    #[tokio::test]
    async fn commit_index_replaces_stale_chunks() {
        let dir = temp_path("memory-commit");
        let sub_cfg = test_config(&dir);
        let sub = PlatformSubsystem::open(&sub_cfg).expect("platform open");
        {
            let mut mem = sub.mem.lock().unwrap();
            mem.episodic_write(
                PRODUCT_NS,
                "old chunk to replace",
                serde_json::json!({"kind": "product_doc"}),
                vec![0.1],
                true,
            );
        }
        let prepared = vec![IndexedChunk {
            text: "fresh chunk after version bump".into(),
            metadata: serde_json::json!({"kind": "product_doc", "version": "0.15.0"}),
            vector: vec![0.9, 0.1],
        }];
        let meta_path = PathBuf::from(&dir).join(META_FILE);
        let n = commit_index(&sub, "0.15.0", "fp-new".into(), &prepared, &meta_path).unwrap();
        assert_eq!(n, 1);
        let hits = sub.mem.lock().unwrap().list(PRODUCT_NS, false);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].text.contains("fresh chunk"));
        let raw = std::fs::read_to_string(&meta_path).unwrap();
        assert!(raw.contains("0.15.0"));
    }
}
