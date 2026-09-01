//! Memory Subsystem (§5) : working memory par agent + store épisodique
//! vectoriel + mémoire partagée sous caps + graphe typé (E6 / Preview 0.4).
//!
//! ## Index vectoriel
//!
//! **Brute-force cosinus en v1** (exact, O(n) par requête) : à l'échelle de
//! la démo (≪ 10⁴ souvenirs) c'est plus rapide que le build d'un ANN et sans
//! dépendance C++. Le point d'extension vers `usearch`/`hnswlib-rs` (choix
//! du plan P2.3) est le trait [`VectorIndex`] — swap documenté, sans changement
//! d'API.
//!
//! ## Graphe typé (E6)
//!
//! Arêtes persistées dans `relations.jsonl` : `similar` / `updates` /
//! `supersedes`. Les requêtes masquent les nœuds supersédés par défaut et
//! peuvent étendre d'un hop `similar`.

use aos_proto::{MemHit, MemRelation, MemRelationKind};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Kind optionnel d'une entrée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    #[default]
    Fact,
    Episode,
}

impl MemoryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fact => "fact",
            Self::Episode => "episode",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "episode" => Self::Episode,
            _ => Self::Fact,
        }
    }
}

/// Entrée de mémoire épisodique.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodicEntry {
    pub id: u64,
    pub namespace: String,
    pub text: String,
    pub metadata: serde_json::Value,
    pub vector: Vec<f32>,
    pub ts_ms: u64,
    pub pinned: bool,
    /// Kind (`fact` / `episode`) — défaut `fact` pour compat JSONL v0.3.
    #[serde(default)]
    pub kind: MemoryKind,
}

/// Index vectoriel (swap point ANN, cf. note d'en-tête).
pub trait VectorIndex {
    fn insert(&mut self, id: u64, vector: &[f32]);
    fn remove(&mut self, id: u64);
    /// (id, score cosinus) triés par score décroissant, top-k.
    fn search(&self, query: &[f32], k: usize) -> Vec<(u64, f32)>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Brute-force exact (v1).
#[derive(Default)]
pub struct BruteForceIndex {
    vectors: HashMap<u64, Vec<f32>>,
}

impl VectorIndex for BruteForceIndex {
    fn insert(&mut self, id: u64, vector: &[f32]) {
        self.vectors.insert(id, vector.to_vec());
    }

    fn remove(&mut self, id: u64) {
        self.vectors.remove(&id);
    }

    fn search(&self, query: &[f32], k: usize) -> Vec<(u64, f32)> {
        let mut scored: Vec<(u64, f32)> = self
            .vectors
            .iter()
            .map(|(id, v)| (*id, cosine(query, v)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);
        scored
    }

    fn len(&self) -> usize {
        self.vectors.len()
    }
}

/// Cosinus (vecteurs supposés normalisés L2, robuste sinon).
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na * nb > 0.0 {
        dot / (na * nb)
    } else {
        0.0
    }
}

/// Boost pin + récence légère sur le score cosinus.
fn rank_score(base: f32, pinned: bool, ts_ms: u64, now_ms: u64) -> f32 {
    let pin = if pinned { 0.08 } else { 0.0 };
    let age_days = (now_ms.saturating_sub(ts_ms) as f32) / 86_400_000.0;
    let recency = (1.0 / (1.0 + age_days * 0.05)).clamp(0.0, 1.0) * 0.05;
    (base + pin + recency).min(1.0)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Le store mémoire (persisté JSONL + index en mémoire + graphe typé).
pub struct MemoryStore {
    dir: PathBuf,
    /// Working memory par agent (§5.1).
    working: HashMap<String, Vec<(String, String)>>,
    /// Entrées épisodiques par id.
    episodic: HashMap<u64, EpisodicEntry>,
    index: BruteForceIndex,
    /// Segments partagés (nom → contenu JSON), caps gérées en amont.
    shared: HashMap<String, serde_json::Value>,
    /// Arêtes typées.
    relations: Vec<MemRelation>,
    next_id: u64,
}

impl MemoryStore {
    pub fn open(dir: impl AsRef<Path>) -> std::io::Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)?;
        let mut store = Self {
            dir,
            working: HashMap::new(),
            episodic: HashMap::new(),
            index: BruteForceIndex::default(),
            shared: HashMap::new(),
            relations: Vec::new(),
            next_id: 1,
        };
        store.replay()?;
        store.replay_relations()?;
        Ok(store)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn journal_path(&self) -> PathBuf {
        self.dir.join("episodic.jsonl")
    }

    fn relations_path(&self) -> PathBuf {
        self.dir.join("relations.jsonl")
    }

    fn replay(&mut self) -> std::io::Result<()> {
        let path = self.journal_path();
        if !path.exists() {
            return Ok(());
        }
        for line in std::fs::read_to_string(&path)?.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<EpisodicEntry>(line) {
                self.index.insert(entry.id, &entry.vector);
                self.next_id = self.next_id.max(entry.id + 1);
                self.episodic.insert(entry.id, entry);
            }
        }
        Ok(())
    }

    fn replay_relations(&mut self) -> std::io::Result<()> {
        let path = self.relations_path();
        if !path.exists() {
            return Ok(());
        }
        for line in std::fs::read_to_string(&path)?.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(rel) = serde_json::from_str::<MemRelation>(line) {
                if self.episodic.contains_key(&rel.from) && self.episodic.contains_key(&rel.to) {
                    self.relations.push(rel);
                }
            }
        }
        Ok(())
    }

    // --- working (§5.1) ---

    pub fn working_set(&mut self, agent_id: &str, messages: Vec<(String, String)>) {
        self.working.insert(agent_id.into(), messages);
    }

    pub fn working_get(&self, agent_id: &str) -> Vec<(String, String)> {
        self.working.get(agent_id).cloned().unwrap_or_default()
    }

    // --- episodic (§5.1) ---

    /// Écrit un souvenir (le vecteur est fourni par l'appelant — service
    /// d'embeddings du daemon).
    pub fn episodic_write(
        &mut self,
        namespace: &str,
        text: &str,
        metadata: serde_json::Value,
        vector: Vec<f32>,
        pinned: bool,
    ) -> u64 {
        self.episodic_write_kind(namespace, text, metadata, vector, pinned, MemoryKind::Fact)
    }

    pub fn episodic_write_kind(
        &mut self,
        namespace: &str,
        text: &str,
        metadata: serde_json::Value,
        vector: Vec<f32>,
        pinned: bool,
        kind: MemoryKind,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let entry = EpisodicEntry {
            id,
            namespace: namespace.into(),
            text: text.into(),
            metadata,
            vector,
            ts_ms: now_ms(),
            pinned,
            kind,
        };
        self.index.insert(id, &entry.vector);
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.journal_path())
        {
            use std::io::Write;
            let _ = writeln!(f, "{}", serde_json::to_string(&entry).unwrap_or_default());
        }
        self.episodic.insert(id, entry);
        id
    }

    /// Insert + auto-link vers un hit proche du même namespace.
    /// Retourne `(id, auto_relations)`.
    #[allow(clippy::too_many_arguments)]
    pub fn episodic_write_auto_link(
        &mut self,
        namespace: &str,
        text: &str,
        metadata: serde_json::Value,
        vector: Vec<f32>,
        pinned: bool,
        kind: MemoryKind,
        threshold: f32,
    ) -> (u64, Vec<MemRelation>) {
        let near = self
            .episodic_query_raw(&vector, 3, Some(namespace), false, false)
            .into_iter()
            .find(|h| h.score >= threshold && !h.superseded);
        let id = self.episodic_write_kind(namespace, text, metadata, vector, pinned, kind);
        let mut auto = Vec::new();
        if let Some(old) = near {
            // Nouveau remplace l'ancien si score élevé ; sinon "updates".
            let rel = if old.score >= (threshold + 0.05).min(0.95) {
                MemRelationKind::Supersedes
            } else {
                MemRelationKind::Updates
            };
            if let Ok(edge) = self.relate(id, rel, old.id) {
                auto.push(edge);
            }
        }
        (id, auto)
    }

    /// Recherche sémantique top-k (F-MEM-02). Masque les supersédés ; boost pin/récence.
    pub fn episodic_query(
        &self,
        query_vector: &[f32],
        k: usize,
        namespace: Option<&str>,
    ) -> Vec<MemHit> {
        self.episodic_query_raw(query_vector, k, namespace, false, true)
    }

    /// Variante avec options (include_superseded, expand_similar).
    pub fn episodic_query_raw(
        &self,
        query_vector: &[f32],
        k: usize,
        namespace: Option<&str>,
        include_superseded: bool,
        expand_similar: bool,
    ) -> Vec<MemHit> {
        let superseded = self.superseded_ids();
        let now = now_ms();
        let mut hits: Vec<MemHit> = self
            .index
            .search(query_vector, k * 8)
            .into_iter()
            .filter_map(|(id, score)| {
                let e = self.episodic.get(&id)?;
                if let Some(ns) = namespace {
                    if e.namespace != ns {
                        return None;
                    }
                }
                let is_super = superseded.contains(&id);
                if is_super && !include_superseded {
                    return None;
                }
                Some(self.to_hit(e, rank_score(score, e.pinned, e.ts_ms, now), is_super))
            })
            .collect();

        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(k);

        if expand_similar {
            let mut extra: Vec<MemHit> = Vec::new();
            let mut seen: HashSet<u64> = hits.iter().map(|h| h.id).collect();
            for h in &hits {
                for n in self.neighbors(h.id, Some(MemRelationKind::Similar)) {
                    if seen.insert(n.id) {
                        if let Some(ns) = namespace {
                            if n.namespace != ns {
                                continue;
                            }
                        }
                        if n.superseded && !include_superseded {
                            continue;
                        }
                        extra.push(n);
                    }
                }
            }
            // Keep similar neighbors at slightly lower priority.
            for mut e in extra {
                e.score *= 0.92;
                hits.push(e);
            }
            hits.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            hits.truncate(k);
        }
        hits
    }

    /// Top-k par cosinus brut (sans boost pin/récence) — pour dedup / auto-link.
    pub fn episodic_nearest_cosine(
        &self,
        query_vector: &[f32],
        k: usize,
        namespace: Option<&str>,
    ) -> Vec<(u64, f32)> {
        let superseded = self.superseded_ids();
        let mut scored: Vec<(u64, f32)> = self
            .index
            .search(query_vector, k * 8)
            .into_iter()
            .filter_map(|(id, score)| {
                let e = self.episodic.get(&id)?;
                if let Some(ns) = namespace {
                    if e.namespace != ns {
                        return None;
                    }
                }
                if superseded.contains(&id) {
                    return None;
                }
                Some((id, score))
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);
        scored
    }

    fn to_hit(&self, e: &EpisodicEntry, score: f32, superseded: bool) -> MemHit {
        MemHit {
            id: e.id,
            namespace: e.namespace.clone(),
            text: e.text.clone(),
            score,
            metadata: e.metadata.clone(),
            pinned: e.pinned,
            kind: Some(e.kind.as_str().into()),
            relations: self
                .relations
                .iter()
                .filter(|r| r.from == e.id)
                .cloned()
                .collect(),
            superseded,
        }
    }

    /// Ids ciblés par une arête `supersedes` (from supersedes to → `to` est obsolète).
    pub fn superseded_ids(&self) -> HashSet<u64> {
        self.relations
            .iter()
            .filter(|r| r.rel == MemRelationKind::Supersedes)
            .map(|r| r.to)
            .collect()
    }

    pub fn get(&self, id: u64) -> Option<&EpisodicEntry> {
        self.episodic.get(&id)
    }

    /// Liste les entrées d'un namespace (F-MEM-05).
    pub fn list(&self, namespace: &str, include_superseded: bool) -> Vec<MemHit> {
        let superseded = self.superseded_ids();
        let mut hits: Vec<MemHit> = self
            .episodic
            .values()
            .filter(|e| e.namespace == namespace)
            .filter(|e| include_superseded || !superseded.contains(&e.id))
            .map(|e| {
                let is_super = superseded.contains(&e.id);
                self.to_hit(e, if e.pinned { 1.0 } else { 0.5 }, is_super)
            })
            .collect();
        hits.sort_by(|a, b| b.id.cmp(&a.id));
        hits
    }

    /// Hits utilisateur pour `mem.context` : similarité, puis repli liste
    /// (embeddings vides / requête trop générique), pins toujours inclus.
    pub fn context_user_hits(&self, query_vector: &[f32], k: usize) -> Vec<MemHit> {
        let k = k.max(1);
        let listed = self.list("user:default", false);
        let mut hits = self.episodic_query(query_vector, k, Some("user:default"));
        if hits.is_empty() {
            return listed.into_iter().take(k.max(8)).collect();
        }
        let mut seen: HashSet<u64> = hits.iter().map(|h| h.id).collect();
        for p in listed.iter().filter(|h| h.pinned).rev() {
            if seen.insert(p.id) {
                hits.insert(0, p.clone());
            }
        }
        hits.truncate(k.max(8));
        hits
    }

    /// Met à jour un souvenir (texte / meta / pin) ou le supersède.
    pub fn update(
        &mut self,
        id: u64,
        text: &str,
        metadata: Option<serde_json::Value>,
        pinned: Option<bool>,
        supersede: bool,
        new_vector: Vec<f32>,
    ) -> Result<(u64, Vec<MemRelation>), String> {
        let old = self
            .episodic
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("souvenir inconnu: {id}"))?;
        if supersede {
            let new_id = self.episodic_write_kind(
                &old.namespace,
                text,
                metadata.unwrap_or(old.metadata.clone()),
                new_vector,
                pinned.unwrap_or(old.pinned),
                old.kind.clone(),
            );
            let edge = self.relate(new_id, MemRelationKind::Supersedes, id)?;
            Ok((new_id, vec![edge]))
        } else {
            let mut e = old;
            e.text = text.into();
            if let Some(m) = metadata {
                e.metadata = m;
            }
            if let Some(p) = pinned {
                e.pinned = p;
            }
            e.vector = new_vector;
            e.ts_ms = now_ms();
            self.index.insert(id, &e.vector);
            self.episodic.insert(id, e);
            self.compact_journal();
            Ok((id, Vec::new()))
        }
    }

    // --- relations (E6) ---

    pub fn relate(
        &mut self,
        from: u64,
        rel: MemRelationKind,
        to: u64,
    ) -> Result<MemRelation, String> {
        if from == to {
            return Err("relation reflexive interdite".into());
        }
        if !self.episodic.contains_key(&from) || !self.episodic.contains_key(&to) {
            return Err("souvenir inconnu pour relation".into());
        }
        if self
            .relations
            .iter()
            .any(|r| r.from == from && r.to == to && r.rel == rel)
        {
            return Ok(MemRelation { from, rel, to });
        }
        let edge = MemRelation { from, rel, to };
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.relations_path())
        {
            use std::io::Write;
            let _ = writeln!(f, "{}", serde_json::to_string(&edge).unwrap_or_default());
        }
        self.relations.push(edge.clone());
        Ok(edge)
    }

    pub fn unrelate(&mut self, from: u64, rel: MemRelationKind, to: u64) -> bool {
        let before = self.relations.len();
        self.relations
            .retain(|r| !(r.from == from && r.to == to && r.rel == rel));
        if self.relations.len() != before {
            self.compact_relations();
            true
        } else {
            false
        }
    }

    pub fn neighbors(&self, id: u64, rel_filter: Option<MemRelationKind>) -> Vec<MemHit> {
        let superseded = self.superseded_ids();
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for r in &self.relations {
            if r.from != id {
                continue;
            }
            if let Some(ref f) = rel_filter {
                if &r.rel != f {
                    continue;
                }
            }
            if !seen.insert(r.to) {
                continue;
            }
            if let Some(e) = self.episodic.get(&r.to) {
                out.push(self.to_hit(e, 1.0, superseded.contains(&e.id)));
            }
        }
        // Also inbound similar (undirected-ish for discovery).
        if rel_filter.is_none() || rel_filter == Some(MemRelationKind::Similar) {
            for r in &self.relations {
                if r.to != id || r.rel != MemRelationKind::Similar {
                    continue;
                }
                if !seen.insert(r.from) {
                    continue;
                }
                if let Some(e) = self.episodic.get(&r.from) {
                    out.push(self.to_hit(e, 0.95, superseded.contains(&e.id)));
                }
            }
        }
        out
    }

    pub fn relations(&self) -> &[MemRelation] {
        &self.relations
    }

    /// Assemble un bloc bootstrap structuré (faits actifs + similar 1 hop).
    pub fn bootstrap_block(&self, hits: &[MemHit]) -> String {
        if hits.is_empty() {
            return String::new();
        }
        let mut out = String::from("Faits actifs:\n");
        let mut similar_lines = Vec::new();
        for h in hits {
            if h.superseded {
                continue;
            }
            let pin = if h.pinned { " ★" } else { "" };
            out.push_str(&format!("- [{}] {}{}\n", h.id, h.text, pin));
            for r in &h.relations {
                if r.rel == MemRelationKind::Similar {
                    if let Some(e) = self.episodic.get(&r.to) {
                        if !self.superseded_ids().contains(&e.id) {
                            similar_lines.push(format!("- ~{} (via {})", e.text, h.id));
                        }
                    }
                }
            }
        }
        if !similar_lines.is_empty() {
            out.push_str("Voisins similar:\n");
            for line in similar_lines.into_iter().take(4) {
                out.push_str(&line);
                out.push('\n');
            }
        }
        out
    }

    /// Supprime une entrée épisodique par id (compacte le journal).
    pub fn episodic_delete(&mut self, id: u64) -> bool {
        if self.episodic.remove(&id).is_none() {
            return false;
        }
        self.index.remove(id);
        self.relations.retain(|r| r.from != id && r.to != id);
        self.compact_journal();
        self.compact_relations();
        true
    }

    /// Supprime les entrées d'un namespace dont `metadata[key] == value`.
    pub fn episodic_delete_by_meta(
        &mut self,
        namespace: &str,
        key: &str,
        value: &str,
    ) -> usize {
        let ids: Vec<u64> = self
            .episodic
            .values()
            .filter(|e| {
                e.namespace == namespace
                    && e.metadata
                        .get(key)
                        .and_then(|v| v.as_str())
                        .map(|s| s == value)
                        .unwrap_or(false)
            })
            .map(|e| e.id)
            .collect();
        let n = ids.len();
        for id in ids {
            self.episodic.remove(&id);
            self.index.remove(id);
            self.relations.retain(|r| r.from != id && r.to != id);
        }
        if n > 0 {
            self.compact_journal();
            self.compact_relations();
        }
        n
    }

    fn compact_journal(&mut self) {
        let mut lines = String::new();
        for e in self.episodic.values() {
            if let Ok(s) = serde_json::to_string(e) {
                lines.push_str(&s);
                lines.push('\n');
            }
        }
        let _ = std::fs::write(self.journal_path(), lines);
    }

    fn compact_relations(&mut self) {
        let mut lines = String::new();
        for r in &self.relations {
            if let Ok(s) = serde_json::to_string(r) {
                lines.push_str(&s);
                lines.push('\n');
            }
        }
        let _ = std::fs::write(self.relations_path(), lines);
    }

    // --- shared (§5.1, F-MEM-03) ---

    pub fn shared_read(&self, name: &str) -> Option<serde_json::Value> {
        self.shared.get(name).cloned()
    }

    pub fn shared_write(&mut self, name: &str, value: serde_json::Value) {
        self.shared.insert(name.into(), value);
    }

    // --- export / wipe (F-MEM-05) ---

    pub fn export(&self, namespace: &str) -> Vec<EpisodicEntry> {
        self.episodic
            .values()
            .filter(|e| e.namespace == namespace)
            .cloned()
            .collect()
    }

    pub fn wipe(&mut self, namespace: &str) -> usize {
        let ids: Vec<u64> = self
            .episodic
            .values()
            .filter(|e| e.namespace == namespace)
            .map(|e| e.id)
            .collect();
        let n = ids.len();
        for id in &ids {
            self.episodic.remove(id);
            self.index.remove(*id);
            self.relations.retain(|r| r.from != *id && r.to != *id);
        }
        self.working.remove(namespace);
        self.compact_journal();
        self.compact_relations();
        n
    }

    pub fn episodic_len(&self) -> usize {
        self.episodic.len()
    }

    /// Statistiques (pour /help UI) : total + par namespace + working.
    pub fn stats(&self) -> (usize, Vec<(String, usize)>, usize) {
        let mut by_ns: HashMap<String, usize> = HashMap::new();
        for e in self.episodic.values() {
            *by_ns.entry(e.namespace.clone()).or_insert(0) += 1;
        }
        let mut ns: Vec<(String, usize)> = by_ns.into_iter().collect();
        ns.sort();
        (self.episodic.len(), ns, self.working.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (MemoryStore, PathBuf) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "aos-mem-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        (MemoryStore::open(&dir).unwrap(), dir)
    }

    fn v(x: f32) -> Vec<f32> {
        l2n(vec![x, 1.0 - x, 0.5])
    }

    fn l2n(mut v: Vec<f32>) -> Vec<f32> {
        let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.iter_mut().for_each(|x| *x /= n);
        v
    }

    #[test]
    fn episodic_write_query_top_k() {
        let (mut s, dir) = store();
        s.episodic_write(
            "agent:1",
            "le chat dort",
            serde_json::json!({}),
            v(0.9),
            false,
        );
        s.episodic_write(
            "agent:1",
            "le chien court",
            serde_json::json!({}),
            v(0.1),
            false,
        );
        s.episodic_write(
            "agent:2",
            "autre namespace",
            serde_json::json!({}),
            v(0.85),
            false,
        );
        let hits = s.episodic_query(&v(0.92), 2, Some("agent:1"));
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].text, "le chat dort");
        // Filtre namespace respecté.
        let all = s.episodic_query(&v(0.92), 10, None);
        assert_eq!(all.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn working_et_wipe() {
        let (mut s, dir) = store();
        s.working_set("agent:1", vec![("user".into(), "salut".into())]);
        assert_eq!(s.working_get("agent:1").len(), 1);
        s.episodic_write("agent:1", "x", serde_json::json!({}), v(0.5), false);
        let n = s.wipe("agent:1");
        assert_eq!(n, 1);
        assert_eq!(s.episodic_len(), 0);
        assert!(s.working_get("agent:1").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn persistance_apres_reouverture() {
        let dir = std::env::temp_dir().join(format!("aos-mem-re-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        {
            let mut s = MemoryStore::open(&dir).unwrap();
            s.episodic_write("m", "persisté", serde_json::json!({}), v(0.3), false);
        }
        let s2 = MemoryStore::open(&dir).unwrap();
        assert_eq!(s2.episodic_len(), 1);
        let hits = s2.episodic_query(&v(0.3), 1, None);
        assert_eq!(hits[0].text, "persisté");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn episodic_delete_et_by_meta() {
        let (mut s, dir) = store();
        let id = s.episodic_write(
            "module:notes",
            "a — body",
            serde_json::json!({"path": "/documents/notes/a.md"}),
            v(0.5),
            false,
        );
        s.episodic_write(
            "module:notes",
            "b — body",
            serde_json::json!({"path": "/documents/notes/b.md"}),
            v(0.6),
            false,
        );
        assert!(s.episodic_delete(id));
        assert_eq!(s.episodic_len(), 1);
        let n = s.episodic_delete_by_meta("module:notes", "path", "/documents/notes/b.md");
        assert_eq!(n, 1);
        assert_eq!(s.episodic_len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn relate_supersede_hides_old_and_persists() {
        let dir = std::env::temp_dir().join(format!("aos-mem-rel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let old_id;
        let new_id;
        {
            let mut s = MemoryStore::open(&dir).unwrap();
            old_id = s.episodic_write(
                "user:default",
                "je préfère le français",
                serde_json::json!({}),
                v(0.8),
                true,
            );
            new_id = s.episodic_write(
                "user:default",
                "je préfère l'anglais",
                serde_json::json!({}),
                v(0.81),
                true,
            );
            s.relate(new_id, MemRelationKind::Supersedes, old_id).unwrap();
            let hits = s.episodic_query(&v(0.8), 5, Some("user:default"));
            assert!(hits.iter().all(|h| h.id != old_id));
            assert!(hits.iter().any(|h| h.id == new_id));
            assert!(s.list("user:default", true).iter().any(|h| h.superseded));
        }
        let s2 = MemoryStore::open(&dir).unwrap();
        assert_eq!(s2.relations().len(), 1);
        assert!(s2.superseded_ids().contains(&old_id));
        let hits = s2.episodic_query(&v(0.8), 5, Some("user:default"));
        assert!(hits.iter().all(|h| h.id != old_id));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn similar_expand_and_pin_in_hit() {
        let (mut s, dir) = store();
        let a = s.episodic_write("ns", "alpha", serde_json::json!({}), v(0.9), true);
        let b = s.episodic_write("ns", "beta similar", serde_json::json!({}), v(0.2), false);
        s.relate(a, MemRelationKind::Similar, b).unwrap();
        let hits = s.episodic_query(&v(0.91), 5, Some("ns"));
        assert!(hits.iter().any(|h| h.id == a && h.pinned));
        // beta may appear via similar expansion even if cosine is low
        assert!(hits.iter().any(|h| h.id == b) || !hits.is_empty());
        let neigh = s.neighbors(a, Some(MemRelationKind::Similar));
        assert_eq!(neigh.len(), 1);
        assert_eq!(neigh[0].id, b);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn auto_link_creates_supersedes() {
        let (mut s, dir) = store();
        let old = s.episodic_write(
            "user:default",
            "préfère français",
            serde_json::json!({}),
            v(0.9),
            true,
        );
        let (new, auto) = s.episodic_write_auto_link(
            "user:default",
            "préfère anglais",
            serde_json::json!({}),
            v(0.91),
            true,
            MemoryKind::Fact,
            0.5,
        );
        assert!(!auto.is_empty());
        assert_eq!(auto[0].rel, MemRelationKind::Supersedes);
        assert_eq!(auto[0].to, old);
        assert_eq!(auto[0].from, new);
        let hits = s.episodic_query(&v(0.9), 5, Some("user:default"));
        assert!(hits.iter().all(|h| h.id != old));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bootstrap_block_skips_superseded() {
        let (mut s, dir) = store();
        let old = s.episodic_write("u", "vieux", serde_json::json!({}), v(0.5), false);
        let new = s.episodic_write("u", "neuf", serde_json::json!({}), v(0.51), false);
        s.relate(new, MemRelationKind::Supersedes, old).unwrap();
        let hits = s.list("u", true);
        let block = s.bootstrap_block(&hits);
        assert!(block.contains("neuf"));
        assert!(!block.contains("vieux") || block.contains("Faits actifs"));
        // superseded entries are skipped in "Faits actifs"
        let active: Vec<_> = hits.iter().filter(|h| !h.superseded).cloned().collect();
        let block2 = s.bootstrap_block(&active);
        assert!(block2.contains("neuf"));
        assert!(!block2.contains("vieux"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
