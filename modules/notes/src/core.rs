// SPDX-License-Identifier: Apache-2.0
//! Logique pure du module notes (slug, wikilinks, graphe, related) — testable hors WASM.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

pub const GRAPH_PATH: &str = "/documents/notes/.graph.json";
pub const NOTES_DIR: &str = "/documents/notes/";
pub const MEM_NS: &str = "module:notes";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NoteGraph {
    #[serde(default)]
    pub notes: HashMap<String, GraphNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub title: String,
    pub path: String,
    #[serde(default)]
    pub outgoing: Vec<String>,
    #[serde(default)]
    pub memory_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteSummary {
    pub title: String,
    pub path: String,
    pub slug: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkRef {
    pub title: String,
    pub slug: String,
    pub path: String,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedHit {
    pub title: String,
    pub path: String,
    pub slug: String,
    pub relation: String,
    pub hops: u32,
    pub score: f32,
    pub excerpt: String,
}

/// Slug dérivé du titre (identité de fichier).
pub fn slugify(title: &str) -> String {
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
    let collapsed = s
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if collapsed.is_empty() {
        "note".to_string()
    } else {
        collapsed.chars().take(64).collect()
    }
}

pub fn note_path(slug: &str) -> String {
    format!("{NOTES_DIR}{slug}.md")
}

/// Extrait les cibles de `[[Titre]]` et `[[Titre|alias]]`.
pub fn parse_wikilinks(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        if let Some(end) = after.find("]]") {
            let inner = after[..end].trim();
            let target = inner.split('|').next().unwrap_or(inner).trim();
            if !target.is_empty() {
                out.push(target.to_string());
            }
            rest = &after[end + 2..];
        } else {
            break;
        }
    }
    // Déduplique en conservant l'ordre.
    let mut seen = HashSet::new();
    out.into_iter()
        .filter(|t| seen.insert(t.to_string()))
        .collect()
}

/// Sépare le titre H1 éventuel et le corps.
pub fn split_title_body(content: &str) -> (Option<String>, String) {
    let trimmed = content.trim_start();
    if let Some(rest) = trimmed.strip_prefix("# ") {
        let mut lines = rest.lines();
        let title = lines.next().unwrap_or("").trim().to_string();
        let body = lines.collect::<Vec<_>>().join("\n");
        let body = body.trim_start_matches('\n').to_string();
        (Some(title), body)
    } else {
        (None, content.to_string())
    }
}

pub fn format_note_file(title: &str, body: &str) -> String {
    format!("# {}\n\n{}\n", title, body.trim_end())
}

pub fn excerpt_of(body: &str, max: usize) -> String {
    let flat: String = body
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        let truncated: String = flat.chars().take(max.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}

pub fn slug_from_path(path: &str) -> Option<String> {
    let name = path.rsplit('/').next()?;
    let stem = name.strip_suffix(".md")?;
    if stem.is_empty() || stem.starts_with('.') {
        return None;
    }
    Some(stem.to_string())
}

pub fn is_note_path(path: &str) -> bool {
    path.starts_with(NOTES_DIR)
        && path.ends_with(".md")
        && !path.ends_with("/.graph.json")
        && slug_from_path(path).is_some()
}

/// Met à jour le nœud source et ses sortants ; ne touche pas aux autres nœuds.
pub fn upsert_graph_node(
    graph: &mut NoteGraph,
    slug: &str,
    title: &str,
    path: &str,
    outgoing_titles: &[String],
    memory_id: Option<u64>,
) {
    let outgoing: Vec<String> = outgoing_titles.iter().map(|t| slugify(t)).collect();
    let node = GraphNode {
        title: title.to_string(),
        path: path.to_string(),
        outgoing,
        memory_id,
    };
    graph.notes.insert(slug.to_string(), node);
}

/// Backlinks : slug → nœuds qui pointent vers lui.
pub fn incoming_for(graph: &NoteGraph, slug: &str) -> Vec<LinkRef> {
    let mut refs = Vec::new();
    for (src_slug, node) in &graph.notes {
        if node.outgoing.iter().any(|t| t == slug) {
            refs.push(LinkRef {
                title: node.title.clone(),
                slug: src_slug.clone(),
                path: node.path.clone(),
                exists: true,
            });
        }
    }
    refs.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    refs
}

pub fn outgoing_refs(graph: &NoteGraph, slug: &str) -> Vec<LinkRef> {
    let Some(node) = graph.notes.get(slug) else {
        return Vec::new();
    };
    let mut refs = Vec::new();
    for target in &node.outgoing {
        if let Some(t) = graph.notes.get(target) {
            refs.push(LinkRef {
                title: t.title.clone(),
                slug: target.clone(),
                path: t.path.clone(),
                exists: true,
            });
        } else {
            refs.push(LinkRef {
                title: target.replace('-', " "),
                slug: target.clone(),
                path: note_path(target),
                exists: false,
            });
        }
    }
    refs
}

#[derive(Debug, Clone)]
pub struct NeighborWalk {
    pub slug: String,
    pub relation: String,
    pub hops: u32,
}

/// BFS sur le graphe (in + out) jusqu'à `hops` (exclu la source).
pub fn walk_neighbors(graph: &NoteGraph, source: &str, hops: u32) -> Vec<NeighborWalk> {
    if hops == 0 {
        return Vec::new();
    }
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(source.to_string());
    let mut queue: VecDeque<(String, String, u32)> = VecDeque::new();
    queue.push_back((source.to_string(), "both".into(), 0));
    let mut out = Vec::new();

    // Pré-calcule les entrants.
    let mut inbound: HashMap<String, Vec<String>> = HashMap::new();
    for (s, n) in &graph.notes {
        for t in &n.outgoing {
            inbound.entry(t.clone()).or_default().push(s.clone());
        }
    }

    while let Some((cur, _rel, depth)) = queue.pop_front() {
        if depth >= hops {
            continue;
        }
        let outs: Vec<String> = graph
            .notes
            .get(&cur)
            .map(|n| n.outgoing.clone())
            .unwrap_or_default();
        let ins: Vec<String> = inbound.get(&cur).cloned().unwrap_or_default();

        for t in outs {
            if visited.insert(t.clone()) {
                let relation = if ins.contains(&t) {
                    "both"
                } else {
                    "out"
                };
                let hop = depth + 1;
                out.push(NeighborWalk {
                    slug: t.clone(),
                    relation: relation.into(),
                    hops: hop,
                });
                queue.push_back((t, relation.into(), hop));
            }
        }
        for t in ins {
            if visited.insert(t.clone()) {
                let relation = "in";
                let hop = depth + 1;
                out.push(NeighborWalk {
                    slug: t.clone(),
                    relation: relation.into(),
                    hops: hop,
                });
                queue.push_back((t, relation.into(), hop));
            }
        }
    }
    out
}

/// Fusionne voisins graphe + scores sémantiques (path → score).
pub fn merge_related(
    graph: &NoteGraph,
    neighbors: &[NeighborWalk],
    scores_by_path: &HashMap<String, f32>,
    excerpts_by_slug: &HashMap<String, String>,
    k: usize,
) -> Vec<RelatedHit> {
    let mut hits: Vec<RelatedHit> = neighbors
        .iter()
        .filter_map(|n| {
            let node = graph.notes.get(&n.slug)?;
            let score = scores_by_path.get(&node.path).copied().unwrap_or(0.0);
            let excerpt = excerpts_by_slug
                .get(&n.slug)
                .cloned()
                .unwrap_or_else(|| excerpt_of(&node.title, 120));
            Some(RelatedHit {
                title: node.title.clone(),
                path: node.path.clone(),
                slug: n.slug.clone(),
                relation: n.relation.clone(),
                hops: n.hops,
                score,
                excerpt,
            })
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.hops.cmp(&b.hops))
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
    hits.truncate(k);
    hits
}

/// Déduplique des hits mémoire par `metadata.path` (garde l'id le plus élevé).
pub fn dedup_hits_by_path(hits: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut best: HashMap<String, serde_json::Value> = HashMap::new();
    let mut no_path = Vec::new();
    for h in hits {
        let path = h
            .get("metadata")
            .and_then(|m| m.get("path"))
            .and_then(|p| p.as_str())
            .unwrap_or("");
        if path.is_empty() {
            no_path.push(h.clone());
            continue;
        }
        let id = h.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
        match best.get(path) {
            Some(prev) => {
                let prev_id = prev.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                if id >= prev_id {
                    best.insert(path.to_string(), h.clone());
                }
            }
            None => {
                best.insert(path.to_string(), h.clone());
            }
        }
    }
    let mut out: Vec<_> = best.into_values().collect();
    out.extend(no_path);
    out.sort_by(|a, b| {
        let sa = a.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let sb = b.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
        sb.partial_cmp(&sa)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Hello World!"), "hello-world");
        assert_eq!(slugify("  "), "note");
        assert_eq!(slugify("Café"), "café");
    }

    #[test]
    fn parse_wikilinks_and_alias() {
        let body = "See [[Method]] and [[Other|alias]] then [[Method]] again.";
        let links = parse_wikilinks(body);
        assert_eq!(links, vec!["Method", "Other"]);
    }

    #[test]
    fn split_and_format() {
        let (t, b) = split_title_body("# Title\n\nBody here\n");
        assert_eq!(t.as_deref(), Some("Title"));
        assert_eq!(b.trim_end(), "Body here");
        let file = format_note_file("Title", "Body");
        assert!(file.starts_with("# Title\n\nBody\n"));
    }

    #[test]
    fn graph_links_and_related() {
        let mut g = NoteGraph::default();
        upsert_graph_node(
            &mut g,
            "a",
            "A",
            "/documents/notes/a.md",
            &["B".into()],
            Some(1),
        );
        upsert_graph_node(
            &mut g,
            "b",
            "B",
            "/documents/notes/b.md",
            &["C".into()],
            Some(2),
        );
        upsert_graph_node(
            &mut g,
            "c",
            "C",
            "/documents/notes/c.md",
            &[],
            Some(3),
        );
        let out = outgoing_refs(&g, "a");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].slug, "b");
        let incoming = incoming_for(&g, "b");
        assert_eq!(incoming.len(), 1);
        assert_eq!(incoming[0].slug, "a");

        let neighbors = walk_neighbors(&g, "a", 2);
        assert!(neighbors.iter().any(|n| n.slug == "b" && n.hops == 1));
        assert!(neighbors.iter().any(|n| n.slug == "c" && n.hops == 2));

        let mut scores = HashMap::new();
        scores.insert("/documents/notes/c.md".into(), 0.9);
        scores.insert("/documents/notes/b.md".into(), 0.4);
        let related = merge_related(&g, &neighbors, &scores, &HashMap::new(), 10);
        assert_eq!(related[0].slug, "c");
        assert!((related[0].score - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn dedup_keeps_newest() {
        let hits = vec![
            serde_json::json!({"id": 1, "score": 0.9, "metadata": {"path": "/documents/notes/a.md"}}),
            serde_json::json!({"id": 5, "score": 0.5, "metadata": {"path": "/documents/notes/a.md"}}),
            serde_json::json!({"id": 3, "score": 0.8, "metadata": {"path": "/documents/notes/b.md"}}),
        ];
        let d = dedup_hits_by_path(&hits);
        assert_eq!(d.len(), 2);
        let a = d
            .iter()
            .find(|h| h["metadata"]["path"] == "/documents/notes/a.md")
            .unwrap();
        assert_eq!(a["id"], 5);
    }
}
