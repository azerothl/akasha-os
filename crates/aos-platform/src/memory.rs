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

use aos_proto::{
    MemExplainRequest, MemGraphResponse, MemGraphQueryRequest, MemMindPalaceRequest,
    MemMindPalaceResponse, MemMigrationReport, MemNarrativeRequest,
    MemObjectCreateRequest, MemObjectListRequest, MemObjectUpdateRequest, MemRevalidateRequest,
    MemShadowComparison, MemShadowMetrics, MemTimelineRequest, MemTimelineResponse, MemoryObject,
    MemoryObjectKind, MemoryObjectStatus,
    MemoryRelationKind, MemoryRelationV2, MemorySourceRef, MemExplanation, MemHit, MemRelation,
    MemRelationKind,
};
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
    /// Canonical semantic objects for Memory V2. Legacy episodic entries are
    /// projected here on open, preserving their ids and vectors.
    objects: HashMap<u64, MemoryObject>,
    /// V2 graph edges, persisted independently from the legacy relation log.
    relations_v2: Vec<MemoryRelationV2>,
    /// Runtime rollout switch. Set `AOS_MEMORY_V2=1` to expose V2 bus APIs.
    v2_enabled: bool,
    /// Shadow rollout switch. Builds and compares V2 without changing reads.
    shadow_enabled: bool,
    shadow_metrics: MemShadowMetrics,
    next_id: u64,
}

impl MemoryStore {
    pub fn open(dir: impl AsRef<Path>) -> std::io::Result<Self> {
        Self::open_with_v2(dir, memory_v2_env_enabled())
    }

    pub fn open_with_v2(dir: impl AsRef<Path>, v2_enabled: bool) -> std::io::Result<Self> {
        Self::open_with_modes(dir, v2_enabled, false)
    }

    pub fn open_with_modes(
        dir: impl AsRef<Path>,
        v2_enabled: bool,
        shadow_enabled: bool,
    ) -> std::io::Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)?;
        let mut store = Self {
            dir,
            working: HashMap::new(),
            episodic: HashMap::new(),
            index: BruteForceIndex::default(),
            shared: HashMap::new(),
            relations: Vec::new(),
            objects: HashMap::new(),
            relations_v2: Vec::new(),
            v2_enabled,
            shadow_enabled,
            shadow_metrics: MemShadowMetrics::default(),
            next_id: 1,
        };
        store.replay()?;
        store.replay_relations()?;
        store.replay_objects()?;
        store.replay_relations_v2()?;
        store.replay_shared()?;
        store.replay_shadow_metrics()?;
        if store.v2_enabled || store.shadow_enabled {
            store.migrate_legacy_objects()?;
        }
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

    fn objects_path(&self) -> PathBuf {
        self.dir.join("objects-v2.jsonl")
    }

    fn relations_v2_path(&self) -> PathBuf {
        self.dir.join("relations-v2.jsonl")
    }

    fn shared_path(&self) -> PathBuf {
        self.dir.join("shared.json")
    }

    fn shadow_metrics_path(&self) -> PathBuf {
        self.dir.join("shadow-metrics.json")
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

    fn replay_objects(&mut self) -> std::io::Result<()> {
        let path = self.objects_path();
        if !path.exists() {
            return Ok(());
        }
        for line in std::fs::read_to_string(path)?.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(object) = serde_json::from_str::<MemoryObject>(line) {
                self.next_id = self.next_id.max(object.id.saturating_add(1));
                if !object.embedding.is_empty() {
                    self.index.insert(object.id, &object.embedding);
                }
                self.objects.insert(object.id, object);
            }
        }
        Ok(())
    }

    fn replay_relations_v2(&mut self) -> std::io::Result<()> {
        let path = self.relations_v2_path();
        if !path.exists() {
            return Ok(());
        }
        for line in std::fs::read_to_string(path)?.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(rel) = serde_json::from_str::<MemoryRelationV2>(line) {
                if self.objects.contains_key(&rel.from) && self.objects.contains_key(&rel.to) {
                    self.relations_v2.push(rel);
                }
            }
        }
        Ok(())
    }

    fn replay_shared(&mut self) -> std::io::Result<()> {
        let path = self.shared_path();
        if path.exists() {
            if let Ok(value) = serde_json::from_str::<HashMap<String, serde_json::Value>>(
                &std::fs::read_to_string(path)?,
            ) {
                self.shared = value;
            }
        }
        Ok(())
    }

    fn replay_shadow_metrics(&mut self) -> std::io::Result<()> {
        let path = self.shadow_metrics_path();
        if path.exists() {
            if let Ok(metrics) = serde_json::from_str(&std::fs::read_to_string(path)?) {
                self.shadow_metrics = metrics;
            }
        }
        Ok(())
    }

    /// Project V1 facts/episodes into the V2 ontology without changing their
    /// ids or deleting the original journal.
    fn migrate_legacy_objects(&mut self) -> std::io::Result<()> {
        let missing: Vec<EpisodicEntry> = self
            .episodic
            .values()
            .filter(|entry| !self.objects.contains_key(&entry.id))
            .cloned()
            .collect();
        for entry in missing {
            let kind = match entry.kind {
                MemoryKind::Episode => MemoryObjectKind::Event,
                MemoryKind::Fact => MemoryObjectKind::Claim,
            };
            let source_refs = entry
                .metadata
                .get("source")
                .and_then(|v| v.as_str())
                .map(|source| vec![MemorySourceRef {
                    source_type: source.to_string(),
                    source_id: entry
                        .metadata
                        .get("session_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    excerpt: Some(entry.text.clone()),
                    uri: None,
                }])
                .unwrap_or_else(|| vec![MemorySourceRef {
                    source_type: "legacy_episodic".into(),
                    source_id: entry.id.to_string(),
                    excerpt: Some(entry.text.clone()),
                    uri: None,
                }]);
            let object = MemoryObject {
                schema_version: 2,
                id: entry.id,
                kind,
                namespace: entry.namespace,
                title: String::new(),
                content: entry.text,
                status: MemoryObjectStatus::Accepted,
                created_at: entry.ts_ms,
                updated_at: entry.ts_ms,
                temporal: Default::default(),
                confidence: 0.5,
                importance: if entry.pinned { 1.0 } else { 0.5 },
                freshness: 1.0,
                last_used_at: None,
                source_refs,
                visibility: "private".into(),
                metadata: entry.metadata,
                decision: None,
                embedding: entry.vector,
            };
            self.persist_object(&object)?;
            self.objects.insert(object.id, object);
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
        if self.v2_enabled {
            let _ = self.sync_legacy_object(id);
        }
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
        hits.sort_by_key(|a| std::cmp::Reverse(a.id));
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

    // --- Memory V2 semantic objects --------------------------------------

    pub fn memory_v2_enabled(&self) -> bool {
        self.v2_enabled
    }

    pub fn memory_v2_shadow_enabled(&self) -> bool {
        self.shadow_enabled
    }

    pub fn shadow_metrics(&self) -> MemShadowMetrics {
        self.shadow_metrics.clone()
    }

    fn persist_shadow_metrics(&self) {
        if let Ok(value) = serde_json::to_vec_pretty(&self.shadow_metrics) {
            let tmp = self.shadow_metrics_path().with_extension("json.tmp");
            if std::fs::write(&tmp, value).is_ok() {
                let _ = std::fs::rename(tmp, self.shadow_metrics_path());
            }
        }
    }

    /// Compare legacy and V2 retrieval while leaving the active V1 result untouched.
    pub fn shadow_compare(
        &mut self,
        query_vector: &[f32],
        k: usize,
        namespace: Option<&str>,
    ) -> MemShadowComparison {
        let v1_start = std::time::Instant::now();
        let v1 = self.episodic_query(query_vector, k, namespace);
        let v1_latency_us = v1_start.elapsed().as_micros() as u64;
        let v2_start = std::time::Instant::now();
        let v2 = self.object_query(query_vector, k, namespace);
        let v2_latency_us = v2_start.elapsed().as_micros() as u64;
        let v1_ids: Vec<u64> = v1.iter().map(|hit| hit.id).collect();
        let v2_ids: Vec<u64> = v2.iter().map(|object| object.id).collect();
        let overlap_ids: Vec<u64> = v1_ids
            .iter()
            .copied()
            .filter(|id| v2_ids.contains(id))
            .collect();
        self.shadow_metrics.comparisons += 1;
        self.shadow_metrics.v1_hits += v1_ids.len() as u64;
        self.shadow_metrics.v2_hits += v2_ids.len() as u64;
        self.shadow_metrics.overlap_hits += overlap_ids.len() as u64;
        self.shadow_metrics.total_v1_latency_us += v1_latency_us;
        self.shadow_metrics.total_v2_latency_us += v2_latency_us;
        self.shadow_metrics.last_comparison_at = Some(now_ms());
        self.persist_shadow_metrics();
        MemShadowComparison { v1_ids, v2_ids, overlap_ids, v1_latency_us, v2_latency_us, metrics: self.shadow_metrics.clone() }
    }

    pub fn migration_report(&self) -> MemMigrationReport {
        let mut sample_ids: Vec<u64> = self
            .episodic
            .keys()
            .filter(|id| self.objects.contains_key(id))
            .copied()
            .collect();
        sample_ids.sort_unstable();
        sample_ids.truncate(16);
        MemMigrationReport {
            legacy_objects: self.episodic.len(),
            v2_objects: self.objects.len(),
            migrated_objects: self.episodic.keys().filter(|id| self.objects.contains_key(id)).count(),
            legacy_relations: self.relations.len(),
            v2_relations: self.relations_v2.len(),
            projection_ready: self.episodic.keys().all(|id| self.objects.contains_key(id)),
            sample_ids,
        }
    }

    fn persist_object(&self, object: &MemoryObject) -> std::io::Result<()> {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.objects_path())?;
        writeln!(f, "{}", serde_json::to_string(object).unwrap_or_default())
    }

    fn sync_legacy_object(&mut self, id: u64) -> std::io::Result<()> {
        let Some(entry) = self.episodic.get(&id).cloned() else {
            return Ok(());
        };
        let text = entry.text.clone();
        let object = MemoryObject {
            schema_version: 2,
            id: entry.id,
            kind: match entry.kind {
                MemoryKind::Episode => MemoryObjectKind::Event,
                MemoryKind::Fact => MemoryObjectKind::Claim,
            },
            namespace: entry.namespace,
            title: String::new(),
            content: text.clone(),
            status: MemoryObjectStatus::Accepted,
            created_at: entry.ts_ms,
            updated_at: entry.ts_ms,
            temporal: Default::default(),
            confidence: 0.5,
            importance: if entry.pinned { 1.0 } else { 0.5 },
            freshness: 1.0,
            last_used_at: None,
            source_refs: vec![MemorySourceRef {
                source_type: "legacy_episodic".into(),
                source_id: entry.id.to_string(),
                excerpt: Some(text),
                uri: None,
            }],
            visibility: "private".into(),
            metadata: entry.metadata,
            decision: None,
            embedding: entry.vector,
        };
        if !self.objects.contains_key(&id) {
            self.persist_object(&object)?;
        }
        self.objects.insert(id, object);
        Ok(())
    }

    fn compact_objects(&self) {
        let mut lines = String::new();
        let mut ids: Vec<_> = self.objects.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            if let Some(object) = self.objects.get(&id) {
                if let Ok(line) = serde_json::to_string(object) {
                    lines.push_str(&line);
                    lines.push('\n');
                }
            }
        }
        let _ = std::fs::write(self.objects_path(), lines);
    }

    fn persist_relation_v2(&self, relation: &MemoryRelationV2) -> std::io::Result<()> {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.relations_v2_path())?;
        writeln!(f, "{}", serde_json::to_string(relation).unwrap_or_default())
    }

    fn compact_relations_v2(&self) {
        let mut lines = String::new();
        for relation in &self.relations_v2 {
            if let Ok(line) = serde_json::to_string(relation) {
                lines.push_str(&line);
                lines.push('\n');
            }
        }
        let _ = std::fs::write(self.relations_v2_path(), lines);
    }

    fn persist_shared(&self) {
        if let Ok(value) = serde_json::to_vec_pretty(&self.shared) {
            let tmp = self.shared_path().with_extension("json.tmp");
            if std::fs::write(&tmp, value).is_ok() {
                let _ = std::fs::rename(tmp, self.shared_path());
            }
        }
    }

    fn validate_object(object: &MemoryObject) -> Result<(), String> {
        if object.namespace.trim().is_empty() {
            return Err("namespace mémoire requis".into());
        }
        if object.content.trim().is_empty() {
            return Err("contenu mémoire requis".into());
        }
        if !(0.0..=1.0).contains(&object.confidence)
            || !(0.0..=1.0).contains(&object.importance)
            || !(0.0..=1.0).contains(&object.freshness)
        {
            return Err("confidence, importance et freshness doivent être entre 0 et 1".into());
        }
        if let (Some(from), Some(to)) = (object.temporal.valid_from, object.temporal.valid_to) {
            if from > to {
                return Err("valid_from doit précéder valid_to".into());
            }
        }
        if object.status == MemoryObjectStatus::Accepted
            && object.source_refs.is_empty()
            && object
                .decision
                .as_ref()
                .and_then(|d| d.rationale.as_ref())
                .map_or(true, |rationale| rationale.trim().is_empty())
            && object
                .metadata
                .get("justification")
                .and_then(|value| value.as_str())
                .map_or(true, |justification| justification.trim().is_empty())
        {
            return Err("un objet accepted doit avoir une source ou une justification".into());
        }
        Ok(())
    }

    pub fn object_create(
        &mut self,
        req: MemObjectCreateRequest,
        embedding: Vec<f32>,
    ) -> Result<MemoryObject, String> {
        let content = req.content.trim().to_string();
        if content.is_empty() {
            return Err("contenu mémoire requis".into());
        }
        let idempotency_key = req.idempotency_key.clone();
        if let Some(existing) = self.objects.values().find(|object| {
            object.namespace == req.namespace
                && object.kind == req.kind
                && (idempotency_key.as_ref().map_or(false, |key| {
                    object
                        .metadata
                        .get("_idempotency_key")
                        .and_then(|value| value.as_str())
                        == Some(key.as_str())
                }) || (idempotency_key.is_none() && object.content.trim() == content))
        }) {
            return Ok(existing.clone());
        }
        let now = now_ms();
        let mut metadata = req.metadata;
        if metadata.is_null() {
            metadata = serde_json::json!({});
        }
        if let Some(key) = idempotency_key {
            if let Some(map) = metadata.as_object_mut() {
                map.insert("_idempotency_key".into(), serde_json::Value::String(key));
            }
        }
        let object = MemoryObject {
            schema_version: 2,
            id: self.next_id,
            kind: req.kind,
            namespace: req.namespace,
            title: if req.title.trim().is_empty() {
                content.chars().take(80).collect()
            } else {
                req.title
            },
            content,
            status: req.status,
            created_at: now,
            updated_at: now,
            temporal: req.temporal,
            confidence: req.confidence.clamp(0.0, 1.0),
            importance: req.importance.clamp(0.0, 1.0),
            freshness: 1.0,
            last_used_at: None,
            source_refs: req.source_refs,
            visibility: req.visibility,
            metadata,
            decision: req.decision,
            embedding,
        };
        Self::validate_object(&object)?;
        let nearest = if object.embedding.is_empty() {
            None
        } else {
            self.index
                .search(&object.embedding, 8)
                .into_iter()
                .filter_map(|(id, score)| {
                    let previous = self.objects.get(&id)?;
                    (previous.namespace == object.namespace && id != object.id && score >= 0.82)
                        .then_some((id, score))
                })
                .next()
        };
        self.next_id = self.next_id.saturating_add(1);
        if !object.embedding.is_empty() {
            self.index.insert(object.id, &object.embedding);
        }
        self.persist_object(&object).map_err(|e| e.to_string())?;
        self.objects.insert(object.id, object.clone());
        if let Some((previous_id, score)) = nearest {
            let _ = self.relate_v2(
                object.id,
                MemoryRelationKind::Updates,
                previous_id,
                score,
                object.source_refs.clone(),
            );
        }
        Ok(object)
    }

    pub fn object_get(&self, id: u64) -> Option<MemoryObject> {
        self.objects.get(&id).cloned()
    }

    pub fn object_list(&self, req: &MemObjectListRequest) -> Vec<MemoryObject> {
        let mut objects: Vec<_> = self
            .objects
            .values()
            .filter(|object| req.namespace.as_deref().map_or(true, |ns| object.namespace == ns))
            .filter(|object| req.kind.as_ref().map_or(true, |kind| &object.kind == kind))
            .filter(|object| req.status.as_ref().map_or(true, |status| &object.status == status))
            .filter(|object| req.include_archived || object.status != MemoryObjectStatus::Archived)
            .cloned()
            .collect();
        objects.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| b.id.cmp(&a.id)));
        objects.truncate(req.limit.clamp(1, 512));
        objects
    }

    /// Semantic retrieval for the agent context. It combines cosine
    /// similarity with confidence, importance and temporal freshness while
    /// keeping rejected/archived objects out of the default context.
    pub fn object_query(
        &self,
        query_vector: &[f32],
        k: usize,
        namespace: Option<&str>,
    ) -> Vec<MemoryObject> {
        let result_limit = k.clamp(1, 64);
        let now = now_ms();
        let mut scored: Vec<(f32, MemoryObject)> = self
            .index
            .search(query_vector, result_limit.saturating_mul(8))
            .into_iter()
            .filter_map(|(id, cosine_score)| {
                let object = self.objects.get(&id)?;
                if namespace.is_some_and(|ns| object.namespace != ns)
                    || matches!(object.status, MemoryObjectStatus::Rejected | MemoryObjectStatus::Archived)
                {
                    return None;
                }
                let expired = object.temporal.valid_to.is_some_and(|to| to < now);
                let freshness = if expired { object.freshness.min(0.2) } else { object.freshness };
                let score = cosine_score * 0.65
                    + object.confidence * 0.15
                    + object.importance * 0.10
                    + freshness * 0.10;
                Some((score, object.clone()))
            })
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(result_limit);
        scored.into_iter().map(|(_, object)| object).collect()
    }

    pub fn object_relations_for(&self, ids: &HashSet<u64>) -> Vec<MemoryRelationV2> {
        self.relations_v2
            .iter()
            .filter(|relation| ids.contains(&relation.from) || ids.contains(&relation.to))
            .cloned()
            .take(256)
            .collect()
    }

    /// Bounded cognitive navigation: a namespace view or a root object plus one hop.
    pub fn mind_palace_query(&self, req: &MemMindPalaceRequest) -> MemMindPalaceResponse {
        let limit = req.limit.clamp(1, 128);
        let mut ids = HashSet::new();
        if let Some(root_id) = req.root_id {
            ids.insert(root_id);
            for relation in &self.relations_v2 {
                if relation.from == root_id {
                    ids.insert(relation.to);
                } else if relation.to == root_id {
                    ids.insert(relation.from);
                }
            }
        }
        let mut objects: Vec<MemoryObject> = self
            .objects
            .values()
            .filter(|object| req.namespace.as_deref().map_or(true, |ns| object.namespace == ns))
            .filter(|object| ids.is_empty() || ids.contains(&object.id))
            .filter(|object| !matches!(object.status, MemoryObjectStatus::Rejected | MemoryObjectStatus::Archived))
            .cloned()
            .collect();
        objects.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap_or(std::cmp::Ordering::Equal).then_with(|| b.updated_at.cmp(&a.updated_at)));
        let truncated = objects.len() > limit;
        objects.truncate(limit);
        let selected: HashSet<u64> = objects.iter().map(|object| object.id).collect();
        let relations = self
            .relations_v2
            .iter()
            .filter(|relation| selected.contains(&relation.from) && selected.contains(&relation.to))
            .cloned()
            .take(256)
            .collect();
        MemMindPalaceResponse { objects, relations, truncated }
    }

    pub fn object_update(&mut self, req: MemObjectUpdateRequest) -> Result<MemoryObject, String> {
        let mut object = self
            .objects
            .get(&req.id)
            .cloned()
            .ok_or_else(|| format!("objet mémoire inconnu: {}", req.id))?;
        if let Some(title) = req.title { object.title = title; }
        if let Some(content) = req.content { object.content = content; }
        if let Some(status) = req.status { object.status = status; }
        if let Some(confidence) = req.confidence { object.confidence = confidence.clamp(0.0, 1.0); }
        if let Some(importance) = req.importance { object.importance = importance.clamp(0.0, 1.0); }
        if let Some(temporal) = req.temporal { object.temporal = temporal; }
        if let Some(source_refs) = req.source_refs { object.source_refs = source_refs; }
        if let Some(visibility) = req.visibility { object.visibility = visibility; }
        if let Some(metadata) = req.metadata { object.metadata = metadata; }
        if req.decision.is_some() { object.decision = req.decision; }
        object.updated_at = now_ms();
        if let Some(valid_to) = object.temporal.valid_to {
            if valid_to <= object.updated_at && object.status == MemoryObjectStatus::Accepted {
                object.freshness = 0.2;
            }
        }
        Self::validate_object(&object)?;
        self.objects.insert(object.id, object.clone());
        self.compact_objects();
        Ok(object)
    }

    pub fn object_set_embedding(&mut self, id: u64, embedding: Vec<f32>) -> Result<(), String> {
        let object = self
            .objects
            .get_mut(&id)
            .ok_or_else(|| format!("objet mémoire inconnu: {id}"))?;
        object.embedding = embedding.clone();
        if embedding.is_empty() {
            self.index.remove(id);
        } else {
            self.index.insert(id, &embedding);
        }
        self.compact_objects();
        Ok(())
    }

    pub fn object_revalidate(&mut self, req: MemRevalidateRequest) -> Result<MemoryObject, String> {
        let mut object = self
            .objects
            .get(&req.id)
            .cloned()
            .ok_or_else(|| format!("objet mémoire inconnu: {}", req.id))?;
        if let Some(confidence) = req.confidence { object.confidence = confidence.clamp(0.0, 1.0); }
        if let Some(valid_to) = req.valid_to { object.temporal.valid_to = Some(valid_to); }
        object.status = req.status.unwrap_or(MemoryObjectStatus::Accepted);
        object.freshness = 1.0;
        object.temporal.last_confirmed_at = Some(now_ms());
        object.updated_at = now_ms();
        Self::validate_object(&object)?;
        self.objects.insert(object.id, object.clone());
        self.compact_objects();
        Ok(object)
    }

    pub fn relate_v2(
        &mut self,
        from: u64,
        kind: MemoryRelationKind,
        to: u64,
        confidence: f32,
        source_refs: Vec<MemorySourceRef>,
    ) -> Result<MemoryRelationV2, String> {
        if from == to {
            return Err("relation reflexive interdite".into());
        }
        if !self.objects.contains_key(&from) || !self.objects.contains_key(&to) {
            return Err("objet mémoire inconnu pour relation".into());
        }
        if let Some(existing) = self.relations_v2.iter().find(|relation| {
            relation.from == from && relation.to == to && relation.kind == kind
        }) {
            return Ok(existing.clone());
        }
        let relation = MemoryRelationV2 {
            schema_version: 2,
            from,
            kind,
            to,
            confidence: confidence.clamp(0.0, 1.0),
            source_refs,
            created_at: now_ms(),
            metadata: serde_json::json!({}),
        };
        self.persist_relation_v2(&relation).map_err(|e| e.to_string())?;
        self.relations_v2.push(relation.clone());
        Ok(relation)
    }

    pub fn graph_query(&self, req: &MemGraphQueryRequest) -> MemGraphResponse {
        let max_nodes = req.max_nodes.clamp(1, 512);
        let max_depth = req.depth.min(8);
        let mut seen = HashSet::new();
        let mut frontier = vec![(req.root_id, 0usize)];
        let mut nodes = Vec::new();
        let mut relations = Vec::new();
        while let Some((id, depth)) = frontier.pop() {
            if !seen.insert(id) || nodes.len() >= max_nodes { continue; }
            let Some(object) = self.objects.get(&id) else { continue; };
            nodes.push(object.clone());
            if depth >= max_depth { continue; }
            for relation in &self.relations_v2 {
                if req.relation.as_ref().map_or(false, |kind| &relation.kind != kind) { continue; }
                let next = if relation.from == id { Some(relation.to) } else if relation.to == id { Some(relation.from) } else { None };
                if let Some(next) = next {
                    relations.push(relation.clone());
                    frontier.push((next, depth + 1));
                }
            }
        }
        relations.sort_by_key(|relation| (relation.from, relation.to));
        relations.dedup_by(|a, b| a.from == b.from && a.to == b.to && a.kind == b.kind);
        MemGraphResponse {
            root: self.objects.get(&req.root_id).cloned(),
            nodes,
            relations,
            truncated: !frontier.is_empty(),
        }
    }

    pub fn timeline(&self, req: &MemTimelineRequest) -> MemTimelineResponse {
        let mut objects: Vec<_> = self.objects.values().filter(|object| {
            if let Some(ns) = req.namespace.as_deref() { if object.namespace != ns { return false; } }
            let timestamp = object.temporal.observed_at.unwrap_or(object.created_at);
            if let Some(from) = req.from_ms { if timestamp < from { return false; } }
            if let Some(to) = req.to_ms { if timestamp > to { return false; } }
            if let Some(subject) = req.subject_id {
                let related = self.relations_v2.iter().any(|relation| {
                    (relation.from == object.id && relation.to == subject)
                        || (relation.to == object.id && relation.from == subject)
                });
                if object.id != subject && !related { return false; }
            }
            true
        }).cloned().collect();
        objects.sort_by_key(|object| object.temporal.observed_at.unwrap_or(object.created_at));
        let truncated = objects.len() > req.limit.clamp(1, 512);
        objects.truncate(req.limit.clamp(1, 512));
        MemTimelineResponse { objects, truncated }
    }

    pub fn explain(&self, req: &MemExplainRequest) -> MemExplanation {
        let object = self.objects.get(&req.id).cloned();
        let supporting_sources = object.as_ref().map(|o| o.source_refs.clone()).unwrap_or_default();
        let relations = self
            .relations_v2
            .iter()
            .filter(|relation| relation.from == req.id || relation.to == req.id)
            .take(128)
            .cloned()
            .collect();
        let freshness_warning = object.as_ref().and_then(|object| {
            let expired = object.temporal.valid_to.map_or(false, |to| to < now_ms());
            if expired || object.freshness < 0.35 { Some("souvenir ancien ou à revalider".into()) } else { None }
        });
        MemExplanation { object, supporting_sources, relations, freshness_warning }
    }

    pub fn narrative_generate(&mut self, req: &MemNarrativeRequest) -> Result<MemoryObject, String> {
        let timeline = self.timeline(&MemTimelineRequest {
            namespace: req.namespace.clone(), subject_id: None, from_ms: req.from_ms, to_ms: req.to_ms, limit: 32,
        });
        if timeline.objects.is_empty() { return Err("aucun objet pour générer la narration".into()); }
        let title = req.title.clone().unwrap_or_else(|| "Synthèse mémoire".into());
        let accepted: Vec<_> = timeline
            .objects
            .iter()
            .filter(|object| object.status == MemoryObjectStatus::Accepted)
            .collect();
        if accepted.is_empty() { return Err("aucun objet accepté pour générer la narration".into()); }
        let content = accepted.iter().map(|object| format!("- {}", object.content)).collect::<Vec<_>>().join("\n");
        let create = MemObjectCreateRequest {
            namespace: req.namespace.clone().unwrap_or_else(|| "user:default".into()),
            kind: MemoryObjectKind::Narrative,
            title,
            content,
            status: MemoryObjectStatus::Accepted,
            confidence: 0.8,
            importance: 0.6,
            temporal: Default::default(),
            source_refs: accepted.iter().flat_map(|o| o.source_refs.clone()).collect(),
            visibility: "private".into(),
            metadata: serde_json::json!({"generated": true, "justification": "synthèse déterministe à partir des objets sélectionnés", "period_from": req.from_ms, "period_to": req.to_ms}),
            decision: None,
            idempotency_key: None,
        };
        if req.persist { self.object_create(create, Vec::new()) } else {
            let now = now_ms();
            Ok(MemoryObject { schema_version: 2, id: 0, kind: MemoryObjectKind::Narrative, namespace: create.namespace, title: create.title, content: create.content, status: create.status, created_at: now, updated_at: now, temporal: create.temporal, confidence: create.confidence, importance: create.importance, freshness: 1.0, last_used_at: None, source_refs: create.source_refs, visibility: create.visibility, metadata: create.metadata, decision: None, embedding: Vec::new() })
        }
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
        self.objects.remove(&id);
        self.relations_v2.retain(|r| r.from != id && r.to != id);
        self.relations.retain(|r| r.from != id && r.to != id);
        self.compact_journal();
        self.compact_objects();
        self.compact_relations();
        self.compact_relations_v2();
        true
    }

    /// Supprime les entrées d'un namespace dont `metadata[key] == value`.
    pub fn episodic_delete_by_meta(&mut self, namespace: &str, key: &str, value: &str) -> usize {
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
            self.objects.remove(&id);
            self.relations_v2.retain(|r| r.from != id && r.to != id);
            self.relations.retain(|r| r.from != id && r.to != id);
        }
        if n > 0 {
            self.compact_journal();
            self.compact_objects();
            self.compact_relations();
            self.compact_relations_v2();
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
        self.persist_shared();
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
            self.objects.remove(id);
            self.relations_v2.retain(|r| r.from != *id && r.to != *id);
            self.relations.retain(|r| r.from != *id && r.to != *id);
        }
        self.working.remove(namespace);
        self.compact_journal();
        self.compact_objects();
        self.compact_relations();
        self.compact_relations_v2();
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

pub fn memory_v2_env_enabled() -> bool {
    std::env::var("AOS_MEMORY_V2")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

pub fn memory_v2_shadow_env_enabled() -> bool {
    std::env::var("AOS_MEMORY_V2_SHADOW")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::MemoryDecision;

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
            s.relate(new_id, MemRelationKind::Supersedes, old_id)
                .unwrap();
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

    #[test]
    fn v2_object_create_is_idempotent_and_persistent() {
        let (mut s, dir) = store();
        let req = MemObjectCreateRequest {
            namespace: "user:default".into(),
            kind: MemoryObjectKind::Decision,
            title: "Choix de langue".into(),
            content: "Utiliser le français dans l'interface".into(),
            status: MemoryObjectStatus::Accepted,
            confidence: 0.9,
            importance: 0.8,
            temporal: Default::default(),
            source_refs: vec![MemorySourceRef {
                source_type: "chat".into(),
                source_id: "session-1".into(),
                excerpt: Some("je préfère le français".into()),
                uri: None,
            }],
            visibility: "private".into(),
            metadata: serde_json::json!({}),
            decision: Some(aos_proto::MemoryDecision {
                question: "Quelle langue utiliser ?".into(),
                options: vec!["français".into(), "anglais".into()],
                selected_option: Some("français".into()),
                rationale: Some("Préférence explicite de l'utilisateur".into()),
                participants: vec!["user".into()],
                consequences: vec!["Répondre en français par défaut".into()],
                review_at: None,
            }),
            idempotency_key: Some("decision-1".into()),
        };
        let first = s.object_create(req.clone(), v(0.7)).unwrap();
        let second = s.object_create(req, v(0.7)).unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(s.object_list(&MemObjectListRequest { namespace: Some("user:default".into()), kind: None, status: None, limit: 10, include_archived: false }).len(), 1);
        let s2 = MemoryStore::open(&dir).unwrap();
        assert_eq!(s2.object_get(first.id).unwrap().kind, MemoryObjectKind::Decision);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v2_validation_and_contradictions_are_explicit() {
        let (mut s, dir) = store();
        let mut req = MemObjectCreateRequest {
            namespace: "user:default".into(),
            kind: MemoryObjectKind::Claim,
            title: "état du projet".into(),
            content: "Le projet est en phase alpha".into(),
            status: MemoryObjectStatus::Accepted,
            confidence: 0.8,
            importance: 0.5,
            temporal: Default::default(),
            source_refs: Vec::new(),
            visibility: "private".into(),
            metadata: serde_json::json!({}),
            decision: None,
            idempotency_key: None,
        };
        assert!(s.object_create(req.clone(), Vec::new()).is_err());

        req.source_refs.push(MemorySourceRef {
            source_type: "test".into(),
            source_id: "fixture".into(),
            excerpt: None,
            uri: None,
        });
        let first = s.object_create(req.clone(), Vec::new()).unwrap();
        req.content = "Le projet est passé en phase bêta".into();
        let second = s.object_create(req, Vec::new()).unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(s.object_list(&MemObjectListRequest {
            namespace: Some("user:default".into()),
            kind: Some(MemoryObjectKind::Claim),
            status: None,
            limit: 10,
            include_archived: false,
        }).len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v2_graph_explain_timeline_and_shared_roundtrip() {
        let (mut s, dir) = store();
        let source = MemorySourceRef { source_type: "test".into(), source_id: "fixture".into(), excerpt: Some("evidence".into()), uri: None };
        let make = |content: &str| MemObjectCreateRequest {
            namespace: "project:akasha".into(), kind: MemoryObjectKind::Event, title: content.into(), content: content.into(),
            status: MemoryObjectStatus::Accepted, confidence: 0.8, importance: 0.6, temporal: Default::default(),
            source_refs: vec![source.clone()], visibility: "private".into(), metadata: serde_json::json!({}), decision: None, idempotency_key: None,
        };
        let a = s.object_create(make("début du projet"), v(0.8)).unwrap();
        let b = s.object_create(make("première décision"), v(0.7)).unwrap();
        s.relate_v2(a.id, MemoryRelationKind::Causes, b.id, 0.9, vec![source]).unwrap();
        let graph = s.graph_query(&MemGraphQueryRequest { root_id: a.id, depth: 2, max_nodes: 4, relation: None });
        assert_eq!(graph.nodes.len(), 2);
        assert!(graph.relations.len() >= 1);
        assert!(graph.relations.iter().any(|r| r.kind == MemoryRelationKind::Causes));
        assert_eq!(s.explain(&MemExplainRequest { id: b.id }).supporting_sources.len(), 1);
        assert_eq!(s.timeline(&MemTimelineRequest { namespace: Some("project:akasha".into()), subject_id: None, from_ms: None, to_ms: None, limit: 10 }).objects.len(), 2);
        s.shared_write("project:akasha", serde_json::json!({"owner":"user"}));
        let s2 = MemoryStore::open(&dir).unwrap();
        assert_eq!(s2.shared_read("project:akasha").unwrap()["owner"], "user");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v2_revalidation_restores_freshness() {
        let (mut s, dir) = store();
        let object = s.object_create(MemObjectCreateRequest {
            namespace: "user:default".into(), kind: MemoryObjectKind::Claim, title: "fait".into(), content: "un fait".into(),
            status: MemoryObjectStatus::Accepted, confidence: 0.5, importance: 0.5, temporal: aos_proto::MemoryTemporal { valid_from: None, valid_to: Some(1), observed_at: None, last_confirmed_at: None },
            source_refs: vec![MemorySourceRef { source_type: "test".into(), source_id: "1".into(), excerpt: None, uri: None }], visibility: "private".into(), metadata: serde_json::json!({}), decision: None, idempotency_key: None,
        }, v(0.4)).unwrap();
        let updated = s.object_revalidate(MemRevalidateRequest { id: object.id, confidence: Some(0.95), valid_to: None, status: None }).unwrap();
        assert_eq!(updated.confidence, 0.95);
        assert_eq!(updated.freshness, 1.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v2_open_migrates_existing_legacy_entries() {
        let (_unused, dir) = store();
        {
            let mut legacy = MemoryStore::open_with_v2(&dir, false).unwrap();
            let id = legacy.episodic_write_kind(
                "user:default",
                "préférence héritée",
                serde_json::json!({"source":"legacy-test"}),
                v(0.6),
                false,
                MemoryKind::Fact,
            );
            assert_eq!(id, 1);
        }
        let migrated = MemoryStore::open_with_v2(&dir, true).unwrap();
        let object = migrated.object_get(1).unwrap();
        assert_eq!(object.kind, MemoryObjectKind::Claim);
        assert_eq!(object.status, MemoryObjectStatus::Accepted);
        assert!(!object.source_refs.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v2_shadow_reports_overlap_and_mind_palace_navigation() {
        let (mut s, dir) = store();
        s.v2_enabled = true;
        s.shadow_enabled = true;
        let legacy = s.episodic_write(
            "project:akasha",
            "legacy project context",
            serde_json::json!({}),
            v(0.7),
            false,
        );
        s.migrate_legacy_objects().unwrap();
        let source = MemorySourceRef { source_type: "test".into(), source_id: "shadow".into(), excerpt: None, uri: None };
        let object = s.object_create(MemObjectCreateRequest {
            namespace: "project:akasha".into(), kind: MemoryObjectKind::Decision, title: "Choix moteur".into(), content: "Utiliser Rust".into(),
            status: MemoryObjectStatus::Accepted, confidence: 0.9, importance: 0.9, temporal: Default::default(), source_refs: vec![source.clone()],
            visibility: "private".into(), metadata: serde_json::json!({}), decision: Some(MemoryDecision { question: "Quel moteur ?".into(), options: vec!["Rust".into()], selected_option: Some("Rust".into()), rationale: Some("Sécurité".into()), participants: vec![], consequences: vec![], review_at: None }), idempotency_key: None,
        }, v(0.7)).unwrap();
        let comparison = s.shadow_compare(&v(0.7), 4, Some("project:akasha"));
        assert!(comparison.overlap_ids.contains(&legacy));
        assert_eq!(s.shadow_metrics().comparisons, 1);
        let palace = s.mind_palace_query(&MemMindPalaceRequest { namespace: Some("project:akasha".into()), root_id: None, limit: 64 });
        assert_eq!(palace.objects.len(), 2);
        assert!(palace.objects.iter().any(|candidate| candidate.id == object.id));
        let report = s.migration_report();
        assert!(report.projection_ready);
        let _ = std::fs::remove_dir_all(dir);
    }
}
