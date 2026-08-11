//! Plan de placement : unités (shards), tiers, profils (§3.5).

use serde::{Deserialize, Serialize};

/// Tier de résidence d'un shard (§3.5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tier {
    /// T0 — VRAM GPU/NPU.
    Vram,
    /// T1 — RAM système.
    Ram,
    /// T2 — Disque (mmap / streaming).
    Disk,
}

impl Tier {
    /// Tier suivant vers lequel migrer sous pression (VRAM→RAM→DISK).
    pub fn colder(self) -> Option<Tier> {
        match self {
            Tier::Vram => Some(Tier::Ram),
            Tier::Ram => Some(Tier::Disk),
            Tier::Disk => None,
        }
    }
}

/// Type d'unité de placement (§3.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShardKind {
    /// Couche transformer n° `u32`.
    Layer(u32),
    /// Blocs de KV cache (paginés).
    KvCache,
    /// Tables embedding / output.
    Embed,
}

/// Descripteur de shard (§3.5.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shard {
    pub model_id: String,
    pub shard_id: u32,
    pub kind: ShardKind,
    pub size_bytes: u64,
    pub residency: Tier,
    /// Épinglage (modèle système, semantic pin §3.5.8).
    pub pin_count: u32,
    /// Dernier usage (tick logique du simulateur) — base LRU.
    pub last_use_tick: u64,
    /// Boost de priorité anti-éviction.
    pub priority_boost: i32,
}

impl Shard {
    /// Évictable = non épinglé.
    pub fn evictable(&self) -> bool {
        self.pin_count == 0
    }
}

/// Profil de placement (§3.5.6, F-PLC-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlacementProfile {
    /// Max layers + KV en VRAM.
    Latency,
    /// Layers hot + KV en VRAM, warm en RAM, cold sur disque (défaut).
    Balanced,
    /// KV minimal / micro-batch ; majorité des poids sur disque.
    MemorySaver,
    /// Pas de GPU : max RAM, overflow disque.
    CpuOnly,
}

impl PlacementProfile {
    /// Ordre de repli automatique en cas de `PlacementImpossible` (§16).
    pub fn fallback(self) -> Option<PlacementProfile> {
        use PlacementProfile::*;
        match self {
            Latency => Some(Balanced),
            Balanced => Some(MemorySaver),
            MemorySaver => Some(CpuOnly),
            CpuOnly => None,
        }
    }
}

/// Priorité d'inférence (§3.6) — utilisée pour l'éviction et la préemption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Batch = 0,
    AgentNormal = 1,
    AgentHigh = 2,
    Interactive = 3,
    SystemCritical = 4,
}

/// Plan de placement complet d'un modèle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementPlan {
    pub model_id: String,
    pub profile: PlacementProfile,
    pub shards: Vec<Shard>,
    /// Fenêtre de prefetch (nb de couches anticipées, §3.5.4).
    pub prefetch_window: u32,
    /// Contexte KV supposé pour le dimensionnement du cache.
    pub kv_tokens: u32,
}

impl PlacementPlan {
    pub fn bytes_on(&self, tier: Tier) -> u64 {
        self.shards
            .iter()
            .filter(|s| s.residency == tier)
            .map(|s| s.size_bytes)
            .sum()
    }

    /// Octets de couches (hors KV/embed) par tier — base du modèle de coût.
    pub fn layer_bytes_on(&self, tier: Tier) -> u64 {
        self.shards
            .iter()
            .filter(|s| s.residency == tier && matches!(s.kind, ShardKind::Layer(_)))
            .map(|s| s.size_bytes)
            .sum()
    }

    pub fn embed_bytes_on(&self, tier: Tier) -> u64 {
        self.shards
            .iter()
            .filter(|s| s.residency == tier && s.kind == ShardKind::Embed)
            .map(|s| s.size_bytes)
            .sum()
    }

    pub fn kv_bytes_on(&self, tier: Tier) -> u64 {
        self.shards
            .iter()
            .filter(|s| s.residency == tier && s.kind == ShardKind::KvCache)
            .map(|s| s.size_bytes)
            .sum()
    }

    /// Synthèse texte (rapport de simulation).
    pub fn summary(&self) -> String {
        const GIB: f64 = (1 << 30) as f64;
        format!(
            "VRAM {:5.2} GiB | RAM {:5.2} GiB | DISK {:5.2} GiB | couches V/R/D: {}/{}/{}",
            self.bytes_on(Tier::Vram) as f64 / GIB,
            self.bytes_on(Tier::Ram) as f64 / GIB,
            self.bytes_on(Tier::Disk) as f64 / GIB,
            self.shards
                .iter()
                .filter(|s| matches!(s.kind, ShardKind::Layer(_)) && s.residency == Tier::Vram)
                .count(),
            self.shards
                .iter()
                .filter(|s| matches!(s.kind, ShardKind::Layer(_)) && s.residency == Tier::Ram)
                .count(),
            self.shards
                .iter()
                .filter(|s| matches!(s.kind, ShardKind::Layer(_)) && s.residency == Tier::Disk)
                .count(),
        )
    }
}
