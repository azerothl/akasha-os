//! Module « notes » — module de référence double-surface (P2.6).
//!
//! Outils exposés aux agents (surface agent) et consommés aussi par l'UI
//! humaine (surface humaine, via `module.invoke` depuis `aos-ui`) :
//!
//! - `notes.create {title, content}` → écrit `/documents/notes/<slug>.md`
//!   (fs.write) + indexe en mémoire épisodique (mem.episodic_write) ;
//! - `notes.list {}` → liste des notes (fs.list) ;
//! - `notes.read {title}` → contenu (fs.read) ;
//! - `notes.search {query, k?}` → recherche sémantique (mem.episodic_query).

use serde::Deserialize;

fn handle(tool: &str, args: &serde_json::Value) -> Result<serde_json::Value, String> {
    match tool {
        "notes.create" => create(args),
        "notes.list" => list(),
        "notes.read" => read(args),
        "notes.search" => search(args),
        _ => Err(format!("outil inconnu: {tool}")),
    }
}

fn slugify(title: &str) -> String {
    let s: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let collapsed = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if collapsed.is_empty() {
        "note".to_string()
    } else {
        collapsed.chars().take(64).collect()
    }
}

#[derive(Deserialize)]
struct CreateArgs {
    title: String,
    content: String,
}

fn create(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let a: CreateArgs = aos_module_sdk::parse_args(args)?;
    let slug = slugify(&a.title);
    let path = format!("/documents/notes/{slug}.md");
    let body = format!("# {}\n\n{}\n", a.title, a.content);
    let version = aos_module_sdk::fs_write(&path, &body)?;
    let mem_id = aos_module_sdk::mem_write(
        "module:notes",
        &format!("{} — {}", a.title, a.content),
        serde_json::json!({"path": path, "title": a.title}),
    )?;
    aos_module_sdk::json_ok(&serde_json::json!({
        "path": path,
        "version": version,
        "memory_id": mem_id,
    }))
}

fn list() -> Result<serde_json::Value, String> {
    let paths = aos_module_sdk::fs_list("/documents/notes/")?;
    aos_module_sdk::json_ok(&serde_json::json!({"notes": paths}))
}

#[derive(Deserialize)]
struct ReadArgs {
    title: String,
}

fn read(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let a: ReadArgs = aos_module_sdk::parse_args(args)?;
    let path = format!("/documents/notes/{}.md", slugify(&a.title));
    let content = aos_module_sdk::fs_read(&path)?;
    aos_module_sdk::json_ok(&serde_json::json!({"path": path, "content": content}))
}

#[derive(Deserialize)]
struct SearchArgs {
    query: String,
    #[serde(default = "default_k")]
    k: usize,
}

fn default_k() -> usize {
    5
}

fn search(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let a: SearchArgs = aos_module_sdk::parse_args(args)?;
    let hits = aos_module_sdk::mem_query("module:notes", &a.query, a.k)?;
    Ok(hits)
}

aos_module_sdk::export_module!(handle);
