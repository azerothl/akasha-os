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
    /// Chemins de données référencés (classification privacy, §3.7/§6.4).
    #[serde(default)]
    pub data_refs: Vec<String>,
    /// Forçage de routage : `local_only` | `remote_only` | `balanced` (F-MDL-07).
    #[serde(default)]
    pub routing: Option<String>,
}

fn default_priority() -> u8 {
    1
}

/// `model.backend.add` — configure un backend distant OpenAI-compatible (P3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendAddRequest {
    /// Id du modèle exposé, ex. `remote:mock:gpt-x`.
    pub model_id: String,
    pub endpoint: String,
    /// Nom du secret (lu via `secrets.get`, §9.2) — jamais la clé en clair.
    #[serde(default)]
    pub secret_name: Option<String>,
    /// Nom du modèle côté API distante.
    #[serde(default)]
    pub remote_model: Option<String>,
}

/// `model.set_routing` — politique globale (F-MDL-07).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetRoutingRequest {
    pub mode: String,
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
    8192
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

impl SystemMetrics {
    /// In-flight inferences across loaded models (`model.metrics`).
    pub fn live_inferences(&self) -> u32 {
        self.models.iter().map(|m| m.active_inferences).sum()
    }
}

// ---------------------------------------------------------------------------
// Agent API (§11.2)
// ---------------------------------------------------------------------------

/// Référence à un document fourni à un agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRef {
    pub path: String,
    #[serde(default)]
    pub label: String,
}

fn default_max_steps() -> u32 {
    32
}

fn default_max_subagents() -> u32 {
    4
}

fn default_timeout_secs() -> u64 {
    3600
}

/// Objectif d'un agent (boucle agentic).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentGoal {
    pub statement: String,
    #[serde(default)]
    pub success_criteria: Vec<String>,
    #[serde(default = "default_max_steps")]
    pub max_steps: u32,
    #[serde(default = "default_max_subagents")]
    pub max_subagents: u32,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

impl Default for AgentGoal {
    fn default() -> Self {
        Self {
            statement: String::new(),
            success_criteria: Vec::new(),
            max_steps: default_max_steps(),
            max_subagents: default_max_subagents(),
            timeout_secs: default_timeout_secs(),
        }
    }
}

/// Budget optionnel (tokens / steps).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentBudget {
    #[serde(default)]
    pub max_tokens: Option<u64>,
    #[serde(default)]
    pub max_steps: Option<u32>,
}

/// Spec complète d'un agent (persistée dans `var/agents/<id>/spec.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpec {
    pub agent_id: String,
    pub goal: AgentGoal,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    #[serde(default)]
    pub documents: Vec<DocumentRef>,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    /// Session chat d'origine (carte / résumé dans le fil).
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub budget: AgentBudget,
    /// Optimiser le prompt système avant le premier step.
    #[serde(default)]
    pub optimize_prompt: bool,
}

/// `agent.create`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCreateRequest {
    /// Directive initiale (alias de `goal.statement` pour compat).
    pub directive: String,
    /// Capacités initiales demandées (URIs `cap://` ou `tool.invoke:*`).
    #[serde(default)]
    pub caps: Vec<String>,
    /// Modèle préféré (`None` → défaut système).
    #[serde(default)]
    pub model_id: Option<String>,
    /// Objectif structuré (si absent → dérivé de `directive`).
    #[serde(default)]
    pub goal: Option<AgentGoal>,
    /// Override / delta du prompt système.
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    #[serde(default)]
    pub documents: Vec<DocumentRef>,
    #[serde(default)]
    pub parent_id: Option<String>,
    /// Session chat d'origine (`None` = Agents UI / spawn parent).
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub budget: AgentBudget,
    #[serde(default)]
    pub optimize_prompt: bool,
}

impl AgentCreateRequest {
    /// Construit un `AgentGoal` à partir de la requête (compat directive).
    pub fn resolved_goal(&self) -> AgentGoal {
        let mut g = self.goal.clone().unwrap_or_default();
        if g.statement.is_empty() {
            g.statement = self.directive.clone();
        }
        if let Some(ms) = self.budget.max_steps {
            g.max_steps = ms;
        }
        g
    }

    /// Création minimale (compat gates / slash-command).
    pub fn simple(directive: impl Into<String>) -> Self {
        Self {
            directive: directive.into(),
            caps: Vec::new(),
            model_id: None,
            goal: None,
            system_prompt: None,
            skills: Vec::new(),
            tools: Vec::new(),
            mcp_servers: Vec::new(),
            documents: Vec::new(),
            parent_id: None,
            session_id: None,
            budget: AgentBudget::default(),
            optimize_prompt: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCreateResponse {
    pub agent_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIdRequest {
    pub agent_id: String,
}

/// `agent.start` — relance depuis un snapshot persisté.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStartRequest {
    pub agent_id: String,
}

/// `agent.steer`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSteerRequest {
    pub agent_id: String,
    pub directive: String,
}

/// `agent.prompt.optimize`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPromptOptimizeRequest {
    pub goal: String,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub current_prompt: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPromptOptimizeResponse {
    pub optimized_prompt: String,
}

/// Skill catalogue entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub when_to_use: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub required_caps: Vec<String>,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub body: String,
}

/// `skill.create` — création d'une skill déclarative par un agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCreateRequest {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub when_to_use: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub required_caps: Vec<String>,
    /// Corps markdown (instructions injectées au prompt).
    pub body: String,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub actor_caps: Vec<String>,
}

/// `skill.activate` / `skill.uninstall` / `skill.describe`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillNameRequest {
    pub name: String,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub actor_caps: Vec<String>,
}

/// `agent.grant` — hot-grant d'une capacité à un agent vivant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentGrantRequest {
    pub agent_id: String,
    pub cap: String,
}

/// MCP server catalogue entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// État de lifecycle d'un agent (§4.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentState {
    Created,
    Running,
    Paused,
    Blocked,
    Done,
    Killed,
    Failed,
}

/// Nœud du graphe de tâches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskNodeStatus {
    Pending,
    Running,
    Blocked,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNode {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub status: TaskNodeStatus,
    #[serde(default)]
    pub notes: String,
}

impl Default for TaskNodeStatus {
    fn default() -> Self {
        Self::Pending
    }
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
    #[serde(default)]
    pub step: u32,
    #[serde(default)]
    pub max_steps: u32,
    #[serde(default)]
    pub current_task: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub children: Vec<String>,
    #[serde(default)]
    pub tokens_used: u64,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    /// Motif d'échec / blocage (bandeau UI).
    #[serde(default)]
    pub fail_reason: Option<String>,
    /// Session chat liée (pour carte / notification).
    #[serde(default)]
    pub session_id: Option<String>,
}

/// Source citée par un tour (web, document, fetch).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentSource {
    /// `web` | `document` | `fetch`
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub title: String,
    /// URL ou chemin logique.
    #[serde(default)]
    pub locator: String,
    #[serde(default)]
    pub snippet: String,
}

/// Un tour Observe / Think / Act / Reflect (transparence F-UI-04).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentStepRecord {
    pub step: u32,
    #[serde(default)]
    pub thought: String,
    #[serde(default)]
    pub response: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub args: serde_json::Value,
    /// `native` | `module` | `mcp` | `runtime` | `unknown`
    #[serde(default)]
    pub tool_kind: String,
    #[serde(default)]
    pub mcp_server: Option<String>,
    #[serde(default)]
    pub skill: Option<String>,
    #[serde(default)]
    pub tool_result: String,
    #[serde(default)]
    pub reflection: Option<String>,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub infer_ms: u64,
    #[serde(default)]
    pub tool_ms: u64,
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub generated_tokens: u32,
    #[serde(default)]
    pub ttft_ms: f64,
    #[serde(default)]
    pub tok_s: f64,
    #[serde(default)]
    pub current_task: Option<String>,
    #[serde(default)]
    pub ts_ms: u64,
    #[serde(default)]
    pub fail_reason: Option<String>,
    #[serde(default)]
    pub child_id: Option<String>,
    #[serde(default)]
    pub sources: Vec<AgentSource>,
}

/// Journal complet d'un agent (`agent.trace`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentTrace {
    pub agent_id: String,
    #[serde(default)]
    pub steps: Vec<AgentStepRecord>,
    #[serde(default)]
    pub tokens_used: u64,
    #[serde(default)]
    pub total_duration_ms: u64,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    #[serde(default)]
    pub reflections: Vec<String>,
    /// Mémoire de travail (repli si `steps` est vide).
    #[serde(default)]
    pub working_memory: Vec<(String, String)>,
    #[serde(default)]
    pub fail_reason: Option<String>,
}

/// Élément du flux `agent.output` (journal temps réel d'un agent).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentOutputEvent {
    Log { line: String },
    Token { text: String },
    StateChanged { state: AgentState },
    Error { message: String },
    Progress {
        step: u32,
        max_steps: u32,
        current_task: Option<String>,
    },
    ChildSpawned { child_id: String, brief: String },
    ChildDone { child_id: String, result: String },
    Reflection { text: String },
    PlanUpdated { nodes: Vec<TaskNode> },
    /// Tour agentic terminé.
    Step(AgentStepRecord),
}

// ---------------------------------------------------------------------------
// Audit (§9.3, §12) — P2
// ---------------------------------------------------------------------------

/// Événement d'audit (journal append-only signé, chaîne de hash).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub seq: u64,
    pub ts_ms: u64,
    /// Identifiant de chaîne causale (intent → agent → outil → fs).
    pub trace_id: String,
    /// Acteur (`agent:<id>`, `service:<nom>`, `human:ui`, `module:<nom>`).
    pub actor: String,
    /// Action (`tool.invoke`, `fs.write`, `policy.deny`, `cap.grant`, ...).
    pub action: String,
    /// Cible (`notes.create`, `/documents/notes/x.md`, ...).
    pub target: String,
    /// Détails structurés (JSON).
    pub detail: serde_json::Value,
    /// Hash de l'événement précédent (chaîne).
    pub prev_hash: String,
    /// Hash de cet événement (sha256 des champs canoniques).
    pub hash: String,
    /// HMAC-SHA256(clé système, hash).
    pub signature: String,
}

/// `audit.append` — nouvel événement (champs seq/hash/signature remplis par
/// le service d'audit).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditAppendRequest {
    pub trace_id: String,
    pub actor: String,
    pub action: String,
    pub target: String,
    #[serde(default)]
    pub detail: serde_json::Value,
}

/// `audit.query`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditQueryRequest {
    #[serde(default)]
    pub trace_id: Option<String>,
    #[serde(default)]
    pub actor: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default = "default_audit_last")]
    pub last: usize,
}

fn default_audit_last() -> usize {
    50
}

// ---------------------------------------------------------------------------
// Storage (§6) — P2
// ---------------------------------------------------------------------------

/// Classe de sensibilité (§6.4, F-FS-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DataClass {
    Public,
    #[default]
    Private,
    Secret,
}

/// `fs.write` (avec transaction optionnelle).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsWriteRequest {
    pub path: String,
    pub content: String,
    #[serde(default)]
    pub tx_id: Option<String>,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub trace_id: String,
}

/// `fs.read`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsReadRequest {
    pub path: String,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub caps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsReadResponse {
    pub path: String,
    pub content: String,
    pub class: DataClass,
    pub version: u64,
}

/// `fs.list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsListRequest {
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub caps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsEntry {
    pub path: String,
    pub class: DataClass,
    pub version: u64,
    pub size_bytes: u64,
}

/// `fs.begin_tx` / `fs.commit` / `fs.rollback`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsTxRequest {
    #[serde(default)]
    pub tx_id: Option<String>,
    #[serde(default)]
    pub actor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsTxResponse {
    pub tx_id: String,
    pub committed_ops: u32,
}

/// `fs.undo` — restaure la version précédente d'un fichier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsUndoRequest {
    pub path: String,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsUndoResponse {
    pub path: String,
    pub restored_version: Option<u64>,
    pub description: String,
}

/// `fs.delete`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsDeleteRequest {
    pub path: String,
    #[serde(default)]
    pub tx_id: Option<String>,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub trace_id: String,
}

/// `fs.set_class` (capacité `fs.reclassify` distincte, §6.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsSetClassRequest {
    pub path: String,
    pub class: DataClass,
    #[serde(default)]
    pub caps: Vec<String>,
}

// ---------------------------------------------------------------------------
// Memory (§5) — P2
// ---------------------------------------------------------------------------

/// `mem.working_set` / `mem.working_get`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemWorkingRequest {
    pub agent_id: String,
    #[serde(default)]
    pub messages: Vec<(String, String)>,
}

/// `mem.episodic_write`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemEpisodicWriteRequest {
    /// Namespace (`agent:<id>`, `module:<nom>`).
    pub namespace: String,
    pub text: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub pinned: bool,
}

/// `mem.episodic_query`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemEpisodicQueryRequest {
    pub query: String,
    #[serde(default = "default_k")]
    pub k: usize,
    #[serde(default)]
    pub namespace: Option<String>,
}

/// `mem.episodic_delete` — par id, ou par namespace + métadonnée (`path`, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemEpisodicDeleteRequest {
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub namespace: Option<String>,
    /// Clé de métadonnée (ex. `"path"`) lorsque `id` est absent.
    #[serde(default)]
    pub meta_key: Option<String>,
    #[serde(default)]
    pub meta_value: Option<String>,
}

fn default_k() -> usize {
    5
}

/// `mem.stats` — compteurs pour le /help de l'UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemStats {
    pub episodic_total: usize,
    pub namespaces: Vec<(String, usize)>,
    pub working_agents: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemHit {
    pub id: u64,
    pub namespace: String,
    pub text: String,
    pub score: f32,
    pub metadata: serde_json::Value,
}

// ---------------------------------------------------------------------------
// System Assistant (§4.5) — prompt de connaissance système
// ---------------------------------------------------------------------------

/// Prompt système injecté dans la mémoire de travail de l'assistant et des
/// agents : connaissance d'Akasha OS (architecture, état, capacités).
///
/// Base pour le PromptCompiler agentic ; les agents reçoivent en plus
/// goal, skills, catalogue d'outils et protocole d'actions JSON.
pub const SYSTEM_ASSISTANT_PROMPT: &str = "Tu es l'assistant système d'Akasha OS, un système d'exploitation agent-natif.

Architecture (services userspace reliés par un bus IPC sémantique CBOR) :
- aos-busd : broker du bus (intents typés, streams, découverte de services) ;
- aos-modeld : modèles IA locaux via llama.cpp (CUDA) — offload VRAM/RAM/disque, scheduler par priorité ;
- aos-agentd : runtime agentic — boucle goal/plan/outils, skills, MCP, sous-agents isolés par capacités ;
- aos-platformd : modules WASM, mémoire épisodique, FS versionné avec undo, web/files, audit signé, skills.

Extensibilité : si tu butes sur une limitation, tu peux étendre l'OS :
1. skill.create — recette déclarative (prompt + outils existants) ;
2. cap.request — demander une capacité manquante (web, fs, module.install…) ;
3. module.scaffold + module.package (script/ext-rt) ou module.compile (Rust→WASM) puis module.install.

Tu agis via des actions JSON structurées (ou la convention TOOL: pour compat). Tu n'inventes pas d'outils absents du catalogue. Tu respectes les capacités (caps) et les confirmations bloquantes.

Tu réponds en français, de façon concise et factuelle. Si tu ne sais pas, dis-le honnêtement.";

/// Addendum injecté uniquement dans le chemin chat (pas les workers).
/// Délégation des tâches complexes via `agent.spawn` sans boucle d'outils.
pub const CHAT_DELEGATION_PROMPT: &str = "
Chat (cette session) :
- Questions, explications, conseils → réponds directement en français, sans JSON.
- Tâche multi-étapes, outils, notes, fichiers, recherche web, effets de bord →
  une courte phrase d'accusé puis un objet JSON seul :
  {\"action\":\"agent.spawn\",\"args\":{\"brief\":\"…\"}}
  (skills/tools optionnels dans args). Ne lance pas toi-même d'outils.";

/// Manifeste double-surface (§7.3).
// ---------------------------------------------------------------------------
// Modules (§7, §11.4) — P2
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleManifest {
    pub name: String,
    pub version: String,
    pub hash: String,
    #[serde(default)]
    pub permissions: ModulePermissions,
    #[serde(default)]
    pub tools: Vec<ModuleTool>,
    #[serde(default)]
    pub ui: Option<ModuleUi>,
    #[serde(default)]
    pub min_os_api: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModulePermissions {
    #[serde(default)]
    pub required_caps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleTool {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub input_schema: serde_json::Value,
    #[serde(default)]
    pub output_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleUi {
    pub entry: String,
    pub mode: String,
}

/// `module.install` (depuis un répertoire `.aospkg`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInstallRequest {
    pub source_dir: String,
    /// Caps approuvées par l'utilisateur (revue d'installation, §7.3).
    #[serde(default)]
    pub approved_caps: Option<Vec<String>>,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub actor_caps: Vec<String>,
}

/// `module.scaffold` — génère un squelette de module (script ou rust).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleScaffoldRequest {
    pub name: String,
    /// `script` (handlers.yaml + ext-rt) ou `rust` (crate wasm).
    #[serde(default = "default_scaffold_kind")]
    pub kind: String,
    pub description: String,
    #[serde(default)]
    pub tools: Vec<ModuleTool>,
    #[serde(default)]
    pub required_caps: Vec<String>,
    /// Contenu handlers.yaml (kind=script) ou src/lib.rs (kind=rust).
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub actor_caps: Vec<String>,
}

fn default_scaffold_kind() -> String {
    "script".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleScaffoldResponse {
    pub path: String,
    pub kind: String,
}

/// `module.package` — produit un `.aospkg` depuis un scaffold script.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModulePackageRequest {
    pub name: String,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub actor_caps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModulePackageResponse {
    pub package_dir: String,
    pub hash: String,
}

/// `module.compile` — compile un crate Rust → wasm32 puis package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleCompileRequest {
    pub name: String,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub actor_caps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleCompileResponse {
    pub package_dir: String,
    pub hash: String,
    pub log: String,
}

/// `module.invoke` — appel d'un outil du module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInvokeRequest {
    pub module: String,
    pub tool: String,
    #[serde(default)]
    pub args: serde_json::Value,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub actor_caps: Vec<String>,
    #[serde(default)]
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInvokeResponse {
    pub ok: bool,
    pub result: serde_json::Value,
    #[serde(default)]
    pub error: Option<String>,
}

/// `module.describe` (introspection, F-MOD-03).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleIdRequest {
    pub module: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfo {
    pub name: String,
    pub version: String,
    pub granted_caps: Vec<String>,
    pub tools: Vec<String>,
    pub quarantined: bool,
}

// ---------------------------------------------------------------------------
// Policy / Trust / Confirm / Egress (§9.4, §9.5, §4.7) — P3
// ---------------------------------------------------------------------------

/// Effet d'une règle de politique (§9.4) — 3 effets seulement en v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyEffect {
    Allow,
    Deny,
    RequireConfirmation,
}

/// Une règle déclarative (fichier `var/policies/rules.yaml`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    pub name: String,
    /// Clés à matcher dans le contexte (ex. `data_class: secret`).
    /// Valeur = string, ou liste de strings (match quelconque).
    #[serde(default)]
    pub matches: Vec<(String, serde_json::Value)>,
    pub effect: PolicyEffect,
    /// Timeout de confirmation (si l'effet est require_confirmation).
    #[serde(default)]
    pub timeout_sec: Option<u64>,
}

/// `policy.evaluate` — contexte aplati (clés en points) → effet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEvalRequest {
    /// Contexte d'évaluation, clés en notation pointée
    /// (ex. `{"action.kind": "fs.delete", "data_class": "secret"}`).
    pub context: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEvalResponse {
    pub effect: PolicyEffect,
    pub rule: Option<String>,
    pub timeout_sec: Option<u64>,
}

/// Une confirmation en attente (§9.4 `pending_confirmation`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingConfirmation {
    pub id: String,
    pub actor: String,
    pub action: String,
    pub target: String,
    pub reason: String,
    pub deadline_ts_ms: u64,
}

/// `confirm.respond`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmResponseRequest {
    pub id: String,
    pub approved: bool,
}

/// Profil de confiance d'un agent (§4.7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustProfile {
    pub agent_id: String,
    pub score: f32,
    pub tier: String,
    pub success_count: u64,
    pub failure_count: u64,
    pub override_count: u64,
    pub confirmation_denials: u64,
}

/// `trust.set` (admin / gouvernance utilisateur).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustSetRequest {
    pub agent_id: String,
    pub score: f32,
}

/// `trust.get`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustGetRequest {
    pub agent_id: String,
}

/// `cap.request` — demande de capacité par un agent (§4.7 paliers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapRequestRequest {
    pub agent_id: String,
    pub cap: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CapRequestOutcome {
    Granted,
    ConfirmationRequired { confirmation_id: String },
    Denied { reason: String },
}

/// `net.check` — contrôle d'egress (§9.5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetCheckRequest {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub caps: Vec<String>,
}

/// Entrée du journal d'egress (monitoring, Gate P3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EgressEntry {
    pub ts_ms: u64,
    pub actor: String,
    pub host: String,
    pub port: u16,
    pub allowed: bool,
}

/// `net.set_mode` — online / offline_strict (§9.5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetModeRequest {
    pub mode: String,
}

/// `secrets.get` — usage restreint aux services (jamais aux agents, §9.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretGetRequest {
    pub name: String,
    pub actor: String,
}

/// `fs.class` — classe de sensibilité d'un chemin (routage §3.7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsClassRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsClassResponse {
    pub path: String,
    pub class: DataClass,
}

// ---------------------------------------------------------------------------
// Noyau de capacités (§2.3, P4.2) — aos-capkd
// ---------------------------------------------------------------------------

/// Droits sous forme de liste de noms (sérialisable IPC).
/// Correspondance avec `aos_caps::Rights` : read/write/execute/grant/revoke.
pub type CapRights = Vec<String>;

/// `cap.mint` — crée une capacité racine (réservé aux services de confiance).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapMintRequest {
    pub holder: String,
    pub object: String,
    pub rights: CapRights,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapMintResponse {
    pub cap_id: u64,
}

/// `cap.derive` — atténuation (droits ⊆ parent).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapDeriveRequest {
    pub holder: String,
    pub parent: u64,
    pub rights: CapRights,
}

/// `cap.grant` — transfert à un autre détenteur.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapGrantRequest {
    pub holder: String,
    pub cap: u64,
    pub to: String,
}

/// `cap.revoke` — révocation unitaire ou en arbre (cascade).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapRevokeRequest {
    pub holder: String,
    pub cap: u64,
    #[serde(default)]
    pub tree: bool,
}

/// `cap.check` — vérification d'autorisation (point d'application kernel).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapCheckRequest {
    pub holder: String,
    pub cap: u64,
    pub rights: CapRights,
    /// Objet visé (optionnel) : doit correspondre à l'objet de la cap
    /// (égalité ou glob `/**`). Absent = pas de contrainte d'objet.
    #[serde(default)]
    pub object: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapCheckResponse {
    pub allowed: bool,
    pub reason: String,
}

/// `cap.list` — capacités d'un détenteur.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapListRequest {
    pub holder: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapInfo {
    pub cap_id: u64,
    pub object: String,
    pub rights: CapRights,
    pub holder: String,
}

// ---------------------------------------------------------------------------
// Feedback testeurs (Preview 0.1) — pas de télémétrie silencieuse
// ---------------------------------------------------------------------------

/// `feedback.submit` — retour cohorte écrit en local (`var/feedback/`),
/// optionnellement publié comme issue GitHub.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackSubmitRequest {
    pub title: String,
    /// bug | ux | perf | security | other
    pub category: String,
    /// low | medium | high
    pub severity: String,
    pub body: String,
    /// Scénario cohorte coché (ex. `chat_offline`).
    #[serde(default)]
    pub scenario: Option<String>,
    /// Métadonnées fournies par l'UI (version, OS, GPU…).
    #[serde(default)]
    pub meta: serde_json::Value,
    /// Si vrai, tente de créer une issue sur le dépôt public.
    #[serde(default)]
    pub publish_github: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackSubmitResponse {
    pub id: String,
    pub path: String,
    pub export_dir: String,
    /// Issue créée ou formulaire GitHub à ouvrir.
    #[serde(default)]
    pub github_issue_url: Option<String>,
    #[serde(default)]
    pub github_issue_number: Option<u64>,
    /// `created` | `form` | `skipped_security` | `local_only` | message d'erreur
    #[serde(default)]
    pub github_status: String,
}

// ---------------------------------------------------------------------------
// Chat sessions (Preview PC.6) — conversations parallèles persistées
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionMeta {
    pub id: String,
    pub title: String,
    pub created_ms: u64,
    pub updated_ms: u64,
    pub archived: bool,
    pub message_count: usize,
    /// Modèle instruct pour cette session (`None` = default_model).
    #[serde(default)]
    pub model_id: Option<String>,
}

/// Pièce jointe d'un message de session (ex. référence agent en fond).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChatAttachment {
    AgentRef {
        agent_id: String,
        #[serde(default)]
        title: String,
        /// `slash` | `assistant` | `completion`
        #[serde(default)]
        origin: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionMessage {
    pub role: String,
    pub content: String,
    pub ts_ms: u64,
    #[serde(default)]
    pub attachments: Vec<ChatAttachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionCreateRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionIdRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionRenameRequest {
    pub session_id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionSetModelRequest {
    pub session_id: String,
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionAppendRequest {
    pub session_id: String,
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub attachments: Vec<ChatAttachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionGetResponse {
    pub meta: ChatSessionMeta,
    pub messages: Vec<ChatSessionMessage>,
}

// ---------------------------------------------------------------------------
// Memory partagée / user (PC.7)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemSharedReadRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemSharedWriteRequest {
    pub name: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemUserRememberRequest {
    pub text: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemUserRecallRequest {
    pub query: String,
    #[serde(default = "default_k")]
    pub k: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemContextRequest {
    /// Session chat active (`session:<id>`).
    #[serde(default)]
    pub session_id: Option<String>,
    pub query: String,
    #[serde(default = "default_k")]
    pub k: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemContextResponse {
    pub session_hits: Vec<MemHit>,
    pub user_hits: Vec<MemHit>,
    pub prompt_block: String,
}

// ---------------------------------------------------------------------------
// Web / fetch / files (PC.8–PC.9)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchRequest {
    pub query: String,
    #[serde(default = "default_search_n")]
    pub max_results: usize,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub actor: String,
    /// `auto` | `brave` | `duckduckgo` | `bing` (défaut `auto`).
    #[serde(default = "default_search_engine")]
    pub engine: String,
}

fn default_search_n() -> usize {
    5
}

fn default_search_engine() -> String {
    "auto".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchResponse {
    pub results: Vec<WebSearchHit>,
}

/// `web.browse` — lecture HTML → texte (sans JS).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebBrowseRequest {
    pub url: String,
    #[serde(default = "default_browse_chars")]
    pub max_chars: usize,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub actor: String,
}

fn default_browse_chars() -> usize {
    12_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebBrowseResponse {
    pub url: String,
    pub final_url: String,
    pub title: String,
    pub text: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetFetchRequest {
    pub url: String,
    /// Chemin logique FS (défaut `/downloads/<filename>`).
    #[serde(default)]
    pub dest_path: Option<String>,
    #[serde(default = "default_max_fetch")]
    pub max_bytes: u64,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub actor: String,
}

fn default_max_fetch() -> u64 {
    50 * 1024 * 1024
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetFetchResponse {
    pub path: String,
    pub bytes: u64,
    pub content_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsWriteBytesRequest {
    pub path: String,
    /// Contenu en base64.
    pub content_b64: String,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsReadBytesRequest {
    pub path: String,
    #[serde(default)]
    pub caps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsReadBytesResponse {
    pub path: String,
    pub content_b64: String,
    pub class: DataClass,
    pub version: u64,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesGenerateRequest {
    /// md | txt | json | csv | png | pdf
    pub format: String,
    pub path: String,
    /// Contenu texte ou spécification JSON (png/pdf).
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub actor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesGenerateResponse {
    pub path: String,
    pub bytes: u64,
}
