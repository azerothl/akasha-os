//! # aos-proto — types partagés des APIs système (specs-techniques §11).
//!
//! Ces structures sont les payloads CBOR des intents échangés sur le bus
//! (`aos-ipc`). Elles définissent le contrat entre `aos-modeld`, `aos-agentd`,
//! `aos-ui` et les futurs modules.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Model API (§11.1)
// ---------------------------------------------------------------------------

/// Message de chat (rôle/contenu) — format commun aux backends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Paramètres d'inférence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferParams {
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
    #[serde(default)]
    pub seed: Option<u32>,
}

impl Default for InferParams {
    fn default() -> Self {
        Self {
            max_tokens: 256,
            temperature: 0.7,
            top_p: 0.9,
            seed: None,
        }
    }
}

/// `model.infer` — requête (flux de [`TokenEvent`] en réponse).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferRequest {
    /// `None` → modèle par défaut (assistant système).
    pub model_id: Option<String>,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub params: InferParams,
    /// Priorité demandée (0=batch .. 4=system critical, cf. §3.6).
    #[serde(default = "default_priority")]
    pub priority: u8,
}

fn default_priority() -> u8 {
    1
}

/// Élément du flux `model.infer`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TokenEvent {
    /// Inférence acceptée par le scheduler.
    Started { inference_id: u64 },
    /// Position dans la file (si mise en attente).
    Queued { position: usize },
    /// Delta de texte généré.
    Delta { text: String },
    /// Fin avec métriques.
    Done {
        prompt_tokens: u32,
        generated_tokens: u32,
        ttft_ms: f64,
        tok_s: f64,
    },
    /// Erreur en cours d'inférence.
    Error { message: String },
}

/// `model.cancel`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelRequest {
    pub inference_id: u64,
}

/// État de résidence d'un modèle (F-MDL-08).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelState {
    /// Non chargé, poids sur disque uniquement.
    OnDisk,
    /// Chargement en cours.
    Loading,
    /// Chargé intégralement sur le tier le plus rapide.
    Loaded,
    /// Chargé avec offload actif (RAM et/ou disque).
    PartiallyOffloaded,
    /// Erreur de chargement.
    Error,
    /// Modèle distant (pas de résidence locale).
    Remote,
}

/// Information registry + état courant d'un modèle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub privacy_class: String,
    pub state: ModelState,
    /// Résumé du placement effectif (ex. « VRAM 6,5 GiB | RAM 20 GiB »).
    pub placement: Option<String>,
    /// Profil de placement effectif.
    pub profile: Option<String>,
}

/// `model.load`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadRequest {
    pub model_id: String,
    /// Profil demandé (`latency`, `balanced`, `memory-saver`, `cpu-only`).
    pub profile: String,
    /// Contexte KV visé (tokens).
    #[serde(default = "default_kv_tokens")]
    pub kv_tokens: u32,
}

fn default_kv_tokens() -> u32 {
    2048
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadResponse {
    pub model_id: String,
    pub effective_profile: String,
    pub placement: String,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnloadRequest {
    pub model_id: String,
}

/// Requête simple par id de modèle (`model.inspect`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelIdRequest {
    pub model_id: String,
}

/// Métriques live d'un modèle (`model.metrics`, F-PLC-08, F-OBS-02).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetrics {
    pub model_id: String,
    pub state: ModelState,
    pub active_inferences: u32,
    pub queued: u32,
    pub last_ttft_ms: Option<f64>,
    pub last_tok_s: Option<f64>,
    pub vram_bytes: u64,
    pub ram_bytes: u64,
    pub disk_bytes: u64,
}

/// Métriques système agrégées.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub models: Vec<ModelMetrics>,
    pub ram_total: u64,
    pub ram_used: u64,
    pub ram_free: u64,
    pub cpu_percent: f32,
    pub agents_active: u32,
}

// ---------------------------------------------------------------------------
// Agent API (§11.2)
// ---------------------------------------------------------------------------

/// `agent.create`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCreateRequest {
    /// Directive initiale (tâche déléguée).
    pub directive: String,
    /// Capacités initiales demandées (URIs `cap://`).
    #[serde(default)]
    pub caps: Vec<String>,
    /// Modèle préféré (`None` → défaut système).
    #[serde(default)]
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCreateResponse {
    pub agent_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIdRequest {
    pub agent_id: String,
}

/// `agent.steer`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSteerRequest {
    pub agent_id: String,
    pub directive: String,
}

/// État de lifecycle d'un agent (§4.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentState {
    Created,
    Running,
    Paused,
    Done,
    Killed,
    Failed,
}

/// Information sur un agent (`agent.list`, `agent.state`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub agent_id: String,
    pub state: AgentState,
    pub directive: String,
    pub pid: Option<u32>,
    pub caps: Vec<String>,
    pub last_output: String,
}

/// Élément du flux `agent.output` (journal temps réel d'un agent).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentOutputEvent {
    Log { line: String },
    Token { text: String },
    StateChanged { state: AgentState },
    Error { message: String },
}
