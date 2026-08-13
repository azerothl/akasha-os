//! Memory Subsystem (§5) : working memory par agent + store épisodique
//! vectoriel + mémoire partagée sous caps.
//!
//! ## Index vectoriel
//!
//! **Brute-force cosinus en v1** (exact, O(n) par requête) : à l'échelle de
//! la démo (≪ 10⁴ souvenirs) c'est plus rapide que le build d'un ANN et sans
//! dépendance C++. Le point d'extension vers `usearch`/`hnswlib-rs` (choix
//! du plan P2.3) est le trait [`VectorIndex`] — swap documenté, sans changement
//! d'API.

use aos_proto::MemHit;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

/// Le store mémoire (persisté JSONL + index en mémoire).
pub struct MemoryStore {
    dir: PathBuf,
    /// Working memory par agent (§5.1).
    working: HashMap<String, Vec<(String, String)>>,
    /// Entrées épisodiques par id.
    episodic: HashMap<u64, EpisodicEntry>,
    index: BruteForceIndex,
    /// Segments partagés (nom → contenu JSON), caps gérées en amont.
    shared: HashMap<String, serde_json::Value>,
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
            next_id: 1,
        };
        store.replay()?;
        Ok(store)
    }

    fn journal_path(&self) -> PathBuf {
        self.dir.join("episodic.jsonl")
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
        let id = self.next_id;
        self.next_id += 1;
        let entry = EpisodicEntry {
            id,
            namespace: namespace.into(),
            text: text.into(),
            metadata,
            vector,
            ts_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            pinned,
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

    /// Recherche sémantique top-k (F-MEM-02).
    pub fn episodic_query(
        &self,
        query_vector: &[f32],
        k: usize,
        namespace: Option<&str>,
    ) -> Vec<MemHit> {
        self.index
            .search(query_vector, k * 4) // sur-échantillonne avant filtre
            .into_iter()
            .filter_map(|(id, score)| {
                let e = self.episodic.get(&id)?;
                if let Some(ns) = namespace {
                    if e.namespace != ns {
                        return None;
                    }
                }
                Some(MemHit {
                    id,
                    namespace: e.namespace.clone(),
                    text: e.text.clone(),
                    score,
                    metadata: e.metadata.clone(),
                })
            })
            .take(k)
            .collect()
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
        }
        self.working.remove(namespace);
        // Réécriture du journal sans les entrées effacées (le journal est
        // notre seule persistance ; compactage simple en v1).
        if let Ok(content) = serde_json::to_string(&self.episodic.values().collect::<Vec<_>>()) {
            let _ = std::fs::write(self.journal_path(), content);
        }
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
}
