//! Module « notes » — module de référence double-surface (P2.6).
//!
//! Outils exposés aux agents et à l'UI humaine via `module.invoke` :
//! - `notes.create` / `notes.update` / `notes.list` / `notes.read`
//! - `notes.search` / `notes.links` / `notes.related`
//!
//! Stockage : markdown sous `/documents/notes/` + graphe `.graph.json`
//! + index épisodique `module:notes`. Wikilinks : `[[Titre]]`.

mod core;

use core::{
    dedup_hits_by_path, excerpt_of, format_note_file, incoming_for, is_note_path, merge_related,
    note_path, outgoing_refs, parse_wikilinks, slug_from_path, slugify, split_title_body,
    upsert_graph_node, walk_neighbors, NoteGraph, NoteSummary, RelatedHit, GRAPH_PATH, MEM_NS,
    NOTES_DIR,
};
use serde::Deserialize;
use std::collections::HashMap;

fn handle(tool: &str, args: &serde_json::Value) -> Result<serde_json::Value, String> {
    match tool {
        "notes.create" => create(args),
        "notes.update" => update(args),
        "notes.list" => list(),
        "notes.read" => read(args),
        "notes.search" => search(args),
        "notes.links" => links(args),
        "notes.related" => related(args),
        _ => Err(format!("outil inconnu: {tool}")),
    }
}

#[derive(Deserialize)]
struct CreateArgs {
    title: String,
    #[serde(default, alias = "body")]
    content: String,
}

fn create(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let a: CreateArgs = aos_module_sdk::parse_args(args)?;
    write_note(&a.title, &a.content, None)
}

#[derive(Deserialize)]
struct UpdateArgs {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    slug: Option<String>,
    #[serde(alias = "body")]
    content: String,
    #[serde(default)]
    new_title: Option<String>,
}

fn update(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let a: UpdateArgs = aos_module_sdk::parse_args(args)?;
    let (old_title, old_path, old_slug) = resolve_identity(a.title.as_deref(), a.path.as_deref(), a.slug.as_deref())?;
    let new_title = a
        .new_title
        .as_deref()
        .filter(|t| !t.is_empty())
        .unwrap_or(&old_title);
    // Phase 1 : rename non supporté (réécriture globale des wikilinks hors scope).
    if slugify(new_title) != old_slug {
        return Err(
            "notes.update: rename (new_title → autre slug) non supporté en v1 ; créez une nouvelle note"
                .into(),
        );
    }
    let _ = old_path; // identité déjà résolue
    write_note(new_title, &a.content, Some(&old_slug))
}

fn write_note(
    title: &str,
    content: &str,
    existing_slug: Option<&str>,
) -> Result<serde_json::Value, String> {
    let slug = existing_slug
        .map(|s| s.to_string())
        .unwrap_or_else(|| slugify(title));
    let path = note_path(&slug);
    // Si le contenu inclut déjà un H1, on le traite comme corps complet.
    let body = {
        let (parsed_title, rest) = split_title_body(content);
        if parsed_title.is_some() {
            rest
        } else {
            content.to_string()
        }
    };
    let file = format_note_file(title, &body);
    let outgoing = parse_wikilinks(&body);

    let _ = aos_module_sdk::mem_delete_by_path(MEM_NS, &path);

    let version = aos_module_sdk::fs_write(&path, &file)?;
    let mem_id = aos_module_sdk::mem_write(
        MEM_NS,
        &format!("{} — {}", title, body),
        serde_json::json!({"path": path, "title": title, "slug": slug}),
    )?;

    let mut graph = load_graph()?;
    upsert_graph_node(
        &mut graph,
        &slug,
        title,
        &path,
        &outgoing,
        Some(mem_id),
    );
    save_graph(&graph)?;

    aos_module_sdk::json_ok(&serde_json::json!({
        "path": path,
        "slug": slug,
        "title": title,
        "version": version,
        "memory_id": mem_id,
        "outgoing": outgoing,
    }))
}

fn list() -> Result<serde_json::Value, String> {
    let paths = aos_module_sdk::fs_list(NOTES_DIR)?;
    let mut notes = Vec::new();
    for path in paths {
        if !is_note_path(&path) {
            continue;
        }
        let Some(slug) = slug_from_path(&path) else {
            continue;
        };
        let content = aos_module_sdk::fs_read(&path).unwrap_or_default();
        let (title_opt, body) = split_title_body(&content);
        let title = title_opt.unwrap_or_else(|| slug.replace('-', " "));
        notes.push(NoteSummary {
            title,
            path,
            slug,
            excerpt: excerpt_of(&body, 160),
        });
    }
    notes.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    aos_module_sdk::json_ok(&serde_json::json!({ "notes": notes }))
}

#[derive(Deserialize)]
struct ReadArgs {
    #[serde(default)]
    title: Option<String>,
    /// Alias legacy / UI (certains clients envoyaient `name`).
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    slug: Option<String>,
}

fn read(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    // Tolérance : si le payload est une string, la traiter comme path ou title.
    let args = normalize_read_args(args);
    let a: ReadArgs = aos_module_sdk::parse_args(&args)?;
    let title = a
        .title
        .filter(|s| !s.is_empty())
        .or_else(|| a.name.filter(|s| !s.is_empty()));
    let (title_hint, path, slug) =
        resolve_identity(title.as_deref(), a.path.as_deref(), a.slug.as_deref())?;
    let content = aos_module_sdk::fs_read(&path)?;
    let (title_opt, body) = split_title_body(&content);
    let title = title_opt.unwrap_or(title_hint);
    let graph = load_graph().unwrap_or_default();
    let outgoing = outgoing_refs(&graph, &slug);
    let incoming = incoming_for(&graph, &slug);
    aos_module_sdk::json_ok(&serde_json::json!({
        "title": title,
        "path": path,
        "slug": slug,
        "content": content,
        "body": body,
        "outgoing": outgoing,
        "incoming": incoming,
    }))
}

fn normalize_read_args(args: &serde_json::Value) -> serde_json::Value {
    if let Some(s) = args.as_str() {
        let s = s.trim();
        if s.starts_with('/') || s.contains('/') {
            return serde_json::json!({ "path": s });
        }
        return serde_json::json!({ "title": s });
    }
    // Anciens clients : objet vide ou clés inattendues — laisser tel quel.
    args.clone()
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
    // Sur-échantillonner pour dédupliquer proprement.
    let raw = aos_module_sdk::mem_query(MEM_NS, &a.query, a.k.saturating_mul(4).max(a.k))?;
    let hits_arr = raw
        .get("hits")
        .and_then(|h| h.as_array())
        .cloned()
        .unwrap_or_default();
    let mut hits = dedup_hits_by_path(&hits_arr);
    hits.truncate(a.k);
    Ok(serde_json::json!({ "hits": hits }))
}

#[derive(Deserialize)]
struct LinksArgs {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    slug: Option<String>,
}

fn links(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let a: LinksArgs = aos_module_sdk::parse_args(args)?;
    let (_title, _path, slug) =
        resolve_identity(a.title.as_deref(), a.path.as_deref(), a.slug.as_deref())?;
    let graph = load_graph()?;
    aos_module_sdk::json_ok(&serde_json::json!({
        "slug": slug,
        "outgoing": outgoing_refs(&graph, &slug),
        "incoming": incoming_for(&graph, &slug),
    }))
}

#[derive(Deserialize)]
struct RelatedArgs {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    slug: Option<String>,
    #[serde(default)]
    topic: Option<String>,
    #[serde(default = "default_hops")]
    hops: u32,
    #[serde(default = "default_related_k")]
    k: usize,
}

fn default_hops() -> u32 {
    1
}
fn default_related_k() -> usize {
    10
}

fn related(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let a: RelatedArgs = aos_module_sdk::parse_args(args)?;
    let (title, path, slug) =
        resolve_identity(a.title.as_deref(), a.path.as_deref(), a.slug.as_deref())?;
    let content = aos_module_sdk::fs_read(&path).unwrap_or_default();
    let (_t, body) = split_title_body(&content);
    let topic = a
        .topic
        .as_deref()
        .filter(|t| !t.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{} — {}", title, excerpt_of(&body, 200)));

    let mut graph = load_graph()?;
    ensure_graph_has_note(&mut graph, &slug, &title, &path, &body)?;

    let neighbors = walk_neighbors(&graph, &slug, a.hops.max(1));
    let mut scores_by_path: HashMap<String, f32> = HashMap::new();
    let mut excerpts_by_slug: HashMap<String, String> = HashMap::new();

    if let Ok(raw) = aos_module_sdk::mem_query(MEM_NS, &topic, 50) {
        if let Some(hits) = raw.get("hits").and_then(|h| h.as_array()) {
            for h in dedup_hits_by_path(hits) {
                if let Some(p) = h
                    .get("metadata")
                    .and_then(|m| m.get("path"))
                    .and_then(|p| p.as_str())
                {
                    let score = h.get("score").and_then(|s| s.as_f64()).unwrap_or(0.0) as f32;
                    scores_by_path.insert(p.to_string(), score);
                }
            }
        }
    }

    for n in &neighbors {
        if let Some(node) = graph.notes.get(&n.slug) {
            if let Ok(c) = aos_module_sdk::fs_read(&node.path) {
                let (_, b) = split_title_body(&c);
                excerpts_by_slug.insert(n.slug.clone(), excerpt_of(&b, 160));
            }
        }
    }

    let related: Vec<RelatedHit> =
        merge_related(&graph, &neighbors, &scores_by_path, &excerpts_by_slug, a.k);

    aos_module_sdk::json_ok(&serde_json::json!({
        "source": { "title": title, "path": path, "slug": slug },
        "topic": topic,
        "related": related,
    }))
}

fn resolve_identity(
    title: Option<&str>,
    path: Option<&str>,
    slug: Option<&str>,
) -> Result<(String, String, String), String> {
    if let Some(p) = path.filter(|p| !p.is_empty()) {
        let s = slug_from_path(p).ok_or_else(|| format!("chemin note invalide: {p}"))?;
        let t = title
            .filter(|t| !t.is_empty())
            .map(|t| t.to_string())
            .unwrap_or_else(|| s.replace('-', " "));
        return Ok((t, p.to_string(), s));
    }
    if let Some(s) = slug.filter(|s| !s.is_empty()) {
        let p = note_path(s);
        let t = title
            .filter(|t| !t.is_empty())
            .map(|t| t.to_string())
            .unwrap_or_else(|| s.replace('-', " "));
        return Ok((t, p, s.to_string()));
    }
    if let Some(t) = title.filter(|t| !t.is_empty()) {
        let s = slugify(t);
        return Ok((t.to_string(), note_path(&s), s));
    }
    Err("title, path ou slug requis".into())
}

fn load_graph() -> Result<NoteGraph, String> {
    match aos_module_sdk::fs_read(GRAPH_PATH) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| e.to_string()),
        Err(_) => {
            let g = rebuild_graph_from_fs()?;
            let _ = save_graph(&g);
            Ok(g)
        }
    }
}

fn save_graph(graph: &NoteGraph) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(graph).map_err(|e| e.to_string())?;
    let _ = aos_module_sdk::fs_write(GRAPH_PATH, &raw)?;
    Ok(())
}

fn rebuild_graph_from_fs() -> Result<NoteGraph, String> {
    let paths = aos_module_sdk::fs_list(NOTES_DIR).unwrap_or_default();
    let mut graph = NoteGraph::default();
    for path in paths {
        if !is_note_path(&path) {
            continue;
        }
        let Some(slug) = slug_from_path(&path) else {
            continue;
        };
        let content = aos_module_sdk::fs_read(&path).unwrap_or_default();
        let (title_opt, body) = split_title_body(&content);
        let title = title_opt.unwrap_or_else(|| slug.replace('-', " "));
        let outgoing = parse_wikilinks(&body);
        upsert_graph_node(&mut graph, &slug, &title, &path, &outgoing, None);
    }
    Ok(graph)
}

fn ensure_graph_has_note(
    graph: &mut NoteGraph,
    slug: &str,
    title: &str,
    path: &str,
    body: &str,
) -> Result<(), String> {
    if !graph.notes.contains_key(slug) {
        let outgoing = parse_wikilinks(body);
        upsert_graph_node(graph, slug, title, path, &outgoing, None);
        save_graph(graph)?;
    }
    Ok(())
}

aos_module_sdk::export_module!(handle);