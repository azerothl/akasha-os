//! # aos-proto — types partagés des APIs système (specs-techniques §11).
//!
//! Ces structures sont les payloads CBOR des intents échangés sur le bus
//! (`aos-ipc`). Elles définissent le contrat entre `aos-modeld`, `aos-agentd`,
//! `aos-ui` et les futurs modules.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub mod bridge;
pub mod chat_document;
pub mod decl_ui;
pub mod mem_extract;

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
    /// Chemins locaux PNG/JPEG appliqués au dernier message `user` (vision / mtmd).
    /// Doivent aussi figurer dans `data_refs` pour la classif privacy (voir
    /// [`InferRequest::ensure_image_data_refs`]).
    #[serde(default)]
    pub images: Vec<String>,
    /// Forçage de routage : `local_only` | `remote_only` | `balanced` (F-MDL-07).
    #[serde(default)]
    pub routing: Option<String>,
}

impl InferRequest {
    /// Copie chaque chemin de `images` dans `data_refs` s'il n'y est pas déjà.
    pub fn ensure_image_data_refs(&mut self) {
        for path in &self.images {
            if !self.data_refs.iter().any(|r| r == path) {
                self.data_refs.push(path.clone());
            }
        }
    }
}

#[cfg(test)]
mod infer_request_tests {
    use super::{ChatMessage, InferParams, InferRequest};

    #[test]
    fn ensure_image_data_refs_merges_without_dupes() {
        let mut req = InferRequest {
            model_id: None,
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "décris".into(),
            }],
            params: InferParams::default(),
            priority: 1,
            data_refs: vec!["/tmp/doc.txt".into()],
            images: vec!["/tmp/a.png".into(), "/tmp/doc.txt".into()],
            routing: None,
        };
        req.ensure_image_data_refs();
        assert_eq!(
            req.data_refs,
            vec![
                "/tmp/doc.txt".to_string(),
                "/tmp/a.png".to_string(),
            ]
        );
    }
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

/// Named OpenAI-compatible provider (F-MDL-04 / P08.12).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRecord {
    pub id: String,
    pub preset: String,
    pub endpoint: String,
    #[serde(default)]
    pub secret_name: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub discovered_models: Vec<String>,
}

/// Shared presets: `(id, default endpoint, optional vault secret name)`.
pub const PROVIDER_PRESETS: &[(&str, &str, Option<&str>)] = &[
    ("openai", "https://api.openai.com/v1", Some("openai_api_key")),
    (
        "openrouter",
        "https://openrouter.ai/api/v1",
        Some("openrouter_api_key"),
    ),
    (
        "anthropic",
        "https://api.anthropic.com/v1",
        Some("anthropic_api_key"),
    ),
    (
        "deepseek",
        "https://api.deepseek.com/v1",
        Some("deepseek_api_key"),
    ),
    ("z.ai", "https://api.z.ai/api/paas/v4", Some("z_ai_api_key")),
    ("custom", "", None),
    ("ollama", "http://127.0.0.1:11434/v1", None),
    ("vllm", "http://127.0.0.1:8000/v1", None),
    ("lmstudio", "http://127.0.0.1:1234/v1", None),
];

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderUpsertRequest {
    pub provider: ProviderRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderIdRequest {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderListResponse {
    pub providers: Vec<ProviderRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderTestResponse {
    pub ok: bool,
    pub message: String,
    #[serde(default)]
    pub models: Vec<String>,
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

/// `model.migrate` — in-process CPU ↔ GPU re-place without aborting the live stream (E18).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrateRequest {
    /// `auto` | `gpu` | `cpu`
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrateResponse {
    pub ok: bool,
    /// True when the caller should fall back to 0.8 cancel+restart.
    #[serde(default)]
    pub fallback: bool,
    pub message: String,
    #[serde(default)]
    pub profile: String,
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
    /// Current sd.cpp denoise step (image/video generation).
    #[serde(default)]
    pub media_step: Option<u32>,
    #[serde(default)]
    pub media_total_steps: Option<u32>,
    #[serde(default)]
    pub last_step_s: Option<f64>,
    /// E20 : moyenne tokens acceptés / pas de verify speculative (C1).
    #[serde(default)]
    pub draft_accept: Option<f64>,
    /// E20 : tokens de préfixe réutilisés au dernier C1.
    #[serde(default)]
    pub prefix_hit: Option<u32>,
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
    pub kind: AgentKind,
    /// Libellé roster (ex. « Coder ») — distinct de la directive / objectif.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Persona intégrée (`researcher`, `coder`, …) si applicable.
    #[serde(default)]
    pub persona_id: Option<String>,
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
    /// `ask` (défaut) : phrase FR + Allow Once dans le fil ; `autonomous` : pas de gate inline.
    #[serde(default = "default_agent_gate_mode")]
    pub gate_mode: String,
    /// `library` | `form` | `slash` | `assistant` | `room` — provenance de création.
    #[serde(default)]
    pub origin: Option<String>,
}

fn default_agent_gate_mode() -> String {
    "ask".into()
}

impl AgentSpec {
    pub fn roster_display_name(&self) -> &str {
        if let Some(n) = self.display_name.as_deref() {
            let t = n.trim();
            if !t.is_empty() {
                return t;
            }
        }
        let d = self.goal.statement.trim();
        if !d.is_empty() {
            return d;
        }
        self.agent_id.as_str()
    }
}

/// `agent.create`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCreateRequest {
    /// Directive initiale (alias de `goal.statement` pour compat).
    pub directive: String,
    #[serde(default)]
    pub kind: AgentKind,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub persona_id: Option<String>,
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
    /// `ask` | `autonomous` — gate inline des actions mutantes dans le fil chat.
    #[serde(default = "default_agent_gate_mode")]
    pub gate_mode: String,
    /// `library` | `form` | `slash` | `assistant` | `room` — provenance de création.
    #[serde(default)]
    pub origin: Option<String>,
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
            kind: AgentKind::default(),
            display_name: None,
            persona_id: None,
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
            gate_mode: default_agent_gate_mode(),
            origin: None,
        }
    }

    /// `true` when `agent.create` should spawn the multi-step worker.
    pub fn spawns_worker(&self) -> bool {
        if self.kind == AgentKind::Roster {
            return false;
        }
        let g = self.resolved_goal();
        !g.statement.trim().is_empty() && g.max_steps > 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCreateResponse {
    pub agent_id: String,
}

/// `agent.spec.get` — lire la spec persistée d'un agent roster ou worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpecResponse {
    pub spec: AgentSpec,
}

/// `agent.roster.update` — met à jour une entrée bibliothèque sans lancer de worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRosterUpdateRequest {
    pub agent_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    #[serde(default)]
    pub model_id: Option<String>,
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
    /// Spécification roster persistée (salon / Agents) sans worker actif.
    Roster,
}

/// Agent de tâche (boucle worker) ou membre roster réutilisable (salon).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    #[default]
    Task,
    Roster,
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
    /// Libellé court dérivé de la directive (liste / historique).
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub kind: AgentKind,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub persona_id: Option<String>,
    /// `library` | `form` | `slash` | `assistant` | `room` — provenance de création.
    #[serde(default)]
    pub origin: Option<String>,
}

impl AgentInfo {
    /// User-typed Agents tab label (`display_name`), not a built-in persona i18n key.
    pub fn uses_typed_display_name(&self) -> bool {
        self.display_name.as_deref().is_some_and(|n| !n.trim().is_empty())
            && self.persona_id.is_none()
            && matches!(self.origin.as_deref(), Some("library") | Some("form"))
    }

    /// Ephemeral chat spawn (`/agent`, delegate) — excluded from salon picker.
    pub fn is_ephemeral_chat_spawn(&self) -> bool {
        matches!(
            self.origin.as_deref(),
            Some("assistant") | Some("slash")
        )
    }

    pub fn display_title(&self) -> &str {
        if let Some(n) = self.display_name.as_deref() {
            let t = n.trim();
            if !t.is_empty() {
                return t;
            }
        }
        let t = self.title.trim();
        if !t.is_empty() {
            return t;
        }
        let d = self.directive.trim();
        if !d.is_empty() {
            return d;
        }
        self.agent_id.as_str()
    }

    pub fn is_roster(&self) -> bool {
        self.kind == AgentKind::Roster || self.state == AgentState::Roster
    }
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemWorkingRequest {
    pub agent_id: String,
    #[serde(default)]
    pub messages: Vec<(String, String)>,
}

/// `mem.episodic_write`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemEpisodicWriteRequest {
    /// Namespace (`agent:<id>`, `module:<nom>`).
    pub namespace: String,
    pub text: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub pinned: bool,
    /// Kind optionnel : `fact` ou `episode`.
    #[serde(default)]
    pub kind: Option<String>,
    /// Si true, auto-crée `updates`/`supersedes` vers un hit proche du même namespace.
    #[serde(default)]
    pub auto_link: bool,
    #[serde(default = "default_auto_link_threshold")]
    pub auto_link_threshold: f32,
}

impl Default for MemEpisodicWriteRequest {
    fn default() -> Self {
        Self {
            namespace: String::new(),
            text: String::new(),
            metadata: serde_json::Value::Null,
            pinned: false,
            kind: None,
            auto_link: false,
            auto_link_threshold: default_auto_link_threshold(),
        }
    }
}

/// `mem.episodic_query`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemEpisodicQueryRequest {
    pub query: String,
    #[serde(default = "default_k")]
    pub k: usize,
    #[serde(default)]
    pub namespace: Option<String>,
}

/// `mem.episodic_delete` — par id, ou par namespace + métadonnée (`path`, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemStats {
    pub episodic_total: usize,
    pub namespaces: Vec<(String, usize)>,
    pub working_agents: usize,
}

/// Relation typée entre souvenirs (E6 / Preview 0.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemRelationKind {
    Similar,
    Updates,
    Supersedes,
}

impl MemRelationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Similar => "similar",
            Self::Updates => "updates",
            Self::Supersedes => "supersedes",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "similar" => Some(Self::Similar),
            "updates" => Some(Self::Updates),
            "supersedes" => Some(Self::Supersedes),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemRelation {
    pub from: u64,
    pub rel: MemRelationKind,
    pub to: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemHit {
    pub id: u64,
    pub namespace: String,
    pub text: String,
    pub score: f32,
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub pinned: bool,
    /// Kind optionnel (`fact` / `episode`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Relations sortantes (1 hop) pour l'UI / bootstrap.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<MemRelation>,
    /// True si un autre souvenir `supersedes` celui-ci.
    #[serde(default)]
    pub superseded: bool,
}

/// `mem.relate` — crée une arête typée entre deux souvenirs.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemRelateRequest {
    pub from: u64,
    pub rel: MemRelationKind,
    pub to: u64,
}

/// `mem.unrelate` — retire une arête.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemUnrelateRequest {
    pub from: u64,
    pub rel: MemRelationKind,
    pub to: u64,
}

/// `mem.neighbors` — voisinage 1 hop.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemNeighborsRequest {
    pub id: u64,
    #[serde(default)]
    pub rel: Option<MemRelationKind>,
}

/// `mem.list` — liste les entrées d'un namespace (F-MEM-05).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemListRequest {
    pub namespace: String,
    /// Inclure les souvenirs supersédés (défaut false).
    #[serde(default)]
    pub include_superseded: bool,
}

/// `mem.update` — remplace le texte d'un souvenir (et optionnellement supersède l'ancien).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemUpdateRequest {
    pub id: u64,
    pub text: String,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub pinned: Option<bool>,
    /// Si true, crée une nouvelle entrée et `supersedes` l'ancienne.
    #[serde(default)]
    pub supersede: bool,
}

/// Réponse enrichie de `mem.user.remember` / write avec auto-link.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemRememberResponse {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auto_relations: Vec<MemRelation>,
}

// ---------------------------------------------------------------------------
// System Assistant (§4.5) — prompt de connaissance système
// ---------------------------------------------------------------------------

/// Prompt système injecté dans la mémoire de travail de l'assistant et des
/// agents : connaissance d'Akasha OS (architecture, état, capacités).
///
/// Base pour le PromptCompiler agentic ; les agents reçoivent en plus
/// goal, skills, catalogue d'outils et protocole d'actions JSON.
pub const SYSTEM_ASSISTANT_PROMPT: &str = "Tu es l'assistant système d'Akasha OS Preview — une application hôte Windows/Linux agent-native (pas encore un OS bootable).

Architecture (services userspace reliés par un bus IPC sémantique CBOR) :
- aos-busd : broker du bus (intents typés, streams, découverte de services) ;
- aos-modeld : modèles IA locaux via llama.cpp (CUDA) — offload VRAM/RAM/disque, scheduler par priorité ;
- aos-agentd : runtime agentic — boucle goal/plan/outils, skills, MCP, sous-agents isolés par capacités ;
- aos-platformd : modules WASM, mémoire épisodique, FS versionné avec undo, web/files, audit signé, skills.

Extensibilité (uniquement si tu as une boucle d'outils / un catalogue) :
1. skill.create — recette déclarative (prompt + outils existants) ;
2. cap.request — demander une capacité manquante (web, fs, module.install…) ;
3. module.scaffold + module.package (script/ext-rt) ou module.compile (Rust→WASM) puis module.install.
N'écris jamais un manifeste, handlers.yaml, ni un arbre declarative_ui « pour montrer » un module : exécute les outils, ou délègue.

Tu agis via des actions JSON structurées (ou la convention TOOL: pour compat). Tu n'inventes pas d'outils absents du catalogue. Tu respectes les capacités (caps) et les confirmations bloquantes.

Tu réponds en français, de façon concise et factuelle. Pour les questions sur l'UI, les nouveautés ou « ce qui a changé », utilise les extraits « Documentation produit (RAG) » et le micro-brief injectés — ne dis jamais que tu n'as pas accès au changelog ou à la doc produit. Si un point n'y figure pas, dis-le clairement.";

/// Micro-brief produit (pas le changelog) — le détail vient du RAG `product:docs`.
pub const PREVIEW_SURFACE_BRIEF: &str = "\
## Surface produit — Akasha OS Preview (hôte Windows/Linux)
Onglets : Chat, Mémoire, Notes, Tâches, Agents, Modèles, Caps, Audit, Providers, Image (studio), Settings, Réseau (opt-in).
Ce n'est pas un OS bootable. Les détails / nouveautés viennent des extraits RAG (FEATURES, STATUS, TESTER) injectés dans le tour — pas d'invention hors de ces sources.";

/// Addendum injecté uniquement dans le chemin chat (pas les workers).
/// Délégation des tâches complexes via `agent.spawn` sans boucle d'outils.
pub const CHAT_DELEGATION_PROMPT: &str = "
Chat (cette session) — tu n'as PAS de boucle d'outils :
- Questions, explications, conseils (y compris « quoi de neuf » / UI) → réponds en français, sans JSON, en t'appuyant sur le brief + extraits RAG produit s'ils sont présents.
- Synthèse vocale / TTS / « générer un audio » : n'appelle PAS agent.spawn.
  Le hôte ouvre une carte TTS (`/speak <texte>`). Réponds en français, sans JSON.
- Routage dessin (gelé) :
  • Panneau canvas OUVERT + « dessine » / « draw » / « sketch » → canvas vectoriel (canvas.stroke…).
  • Panneau canvas FERMÉ + « dessine » / « draw » / « sketch » sans marqueur → agent image (media.image.generate).
  • Marqueur explicite (« sur le canvas », « dans le canvas », « au trait », `/canvas`… ;
    le mot seul « canvas » ne suffit PAS) → canvas vectoriel ; le hôte ouvre le panneau même s'il était fermé.
  • « encore », « vas-y », « relance » seuls → pas de spawn canvas.
  Pour le canvas explicite ou ouvert : une courte phrase d'accusé puis UNIQUEMENT :
  {\"action\":\"agent.spawn\",\"args\":{\"brief\":\"<demande utilisateur>\"}}
  L'agent utilisera canvas.stroke / canvas.rect / canvas.ellipse — jamais media.image.generate.
  Pour le dessin sans canvas ouvert ni marqueur : une courte phrase d'accusé puis UNIQUEMENT :
  {\"action\":\"agent.spawn\",\"args\":{\"brief\":\"<demande utilisateur>\"}}
  (agent image / media.image.generate — pas le canvas vectoriel).
  N'écris PAS de JSON tronqué, ni de long monologue sans spawn.
- Créer / scaffolder / packager / installer un module ou une skill, ou toute
  autre tâche multi-étapes avec effets de bord (notes, fichiers, web) →
  une courte phrase d'accusé puis UNIQUEMENT cet objet JSON :
  {\"action\":\"agent.spawn\",\"args\":{\"brief\":\"<demande utilisateur>\"}}
  N'écris JAMAIS le manifeste, handlers.yaml, ni un arbre declarative_ui.
  L'agent fera module.scaffold + module.package + module.install.
- Ne lance pas toi-même d'outils (pas de module.scaffold, pas de TOOL:, pas de
  audio.generate ni tool.invoke).
- Mémoire : tu n'enregistres rien toi-même. Les faits durables (nom, préférences…)
  sont extraits après le tour vers l'onglet Mémoire. N'écris jamais que tu as
  « noté », « enregistré » ou mis en « mémoire épisodique ». Si un bloc
  « Mémoire long terme utilisateur » est présent ci-dessous, utilise-le.
  Si l'utilisateur demande d'oublier, oriente-le vers l'onglet Mémoire.";

/// Intention chat : créer / installer un module ou une skill (pas « c'est quoi »).
pub fn chat_user_wants_module_authoring(text: &str) -> bool {
    let lower = text.to_lowercase();
    let mentions_target = lower.contains("module")
        || lower.contains("aospkg")
        || lower.contains("ext-rt")
        || lower.contains("ext_rt")
        || lower.contains("skill");
    if !mentions_target {
        return false;
    }
    let explain = [
        "c'est quoi",
        "c est quoi",
        "qu'est-ce",
        "qu est-ce",
        "quest-ce",
        "explique",
        "expliquer",
        "what is",
        "what's a",
        "whats a",
        "how does",
        "comment fonctionne",
        "comment marche",
        "à quoi sert",
        "a quoi sert",
        "différence",
        "difference",
    ]
    .iter()
    .any(|p| lower.contains(p));
    let create = [
        "crée",
        "creer",
        "créer",
        "création",
        "create",
        "creation",
        "scaffold",
        "ajoute",
        "ajouter",
        "add a ",
        "make a",
        "build a",
        "fabrique",
        "génère",
        "genere",
        "generate",
        "packager",
        "package un",
        "nouveau module",
        "new module",
        "nouvelle skill",
        "new skill",
        "module.scaffold",
        "module.package",
        "module.install",
        "skill.create",
    ]
    .iter()
    .any(|p| lower.contains(p));
    let install_verb = (lower.contains("installe") || lower.contains("install"))
        && !lower.contains("installé")
        && !lower.contains("installed");
    if explain && !create && !install_verb {
        return false;
    }
    create || install_verb
}

/// Demande TTS claire dans le tour chat (P09.8) — ouvrir la carte, pas un agent.
///
/// `Some(texte)` : le texte à lire (éventuellement vide si l'utilisateur n'a
/// pas encore fourni ce qu'il faut synthétiser). `None` : pas une demande TTS.
pub fn chat_tts_request(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if chat_user_wants_module_authoring(trimmed) {
        return None;
    }
    let lower = trimmed.to_lowercase();
    let folded = fold_fr(&lower);
    if !tts_intent(&folded) {
        return None;
    }
    if tts_is_explain_only(&folded) {
        return None;
    }
    Some(extract_tts_spoken_text(trimmed))
}

fn fold_fr(s: &str) -> String {
    s.replace(['é', 'è', 'ê', 'ë'], "e")
        .replace(['à', 'â'], "a")
        .replace(['ù', 'û'], "u")
        .replace(['î', 'ï'], "i")
        .replace('ç', "c")
        .replace('ô', "o")
}

fn tts_intent(lower: &str) -> bool {
    if lower.starts_with("tts")
        || lower.contains("text to speech")
        || lower.contains("text-to-speech")
        || lower.contains("synthèse vocale")
        || lower.contains("synthese vocale")
        || lower.contains("lis ce texte")
        || lower.contains("lire ce texte")
        || lower.contains("read this text")
        || lower.contains("à voix haute")
        || lower.contains("a voix haute")
    {
        return true;
    }
    let audio = lower.contains("audio")
        || lower.contains("tts")
        || lower.contains("wav")
        || lower.contains("speech")
        || (lower.contains("voix")
            && (lower.contains("génér")
                || lower.contains("gener")
                || lower.contains("speak")
                || lower.contains("synth")));
    let verb = lower.contains("génér")
        || lower.contains("gener")
        || lower.contains("create")
        || lower.contains("generate")
        || lower.contains("speak")
        || lower.contains("produi")
        || lower.contains("synth")
        || lower.contains("fais ")
        || lower.contains("fait ")
        || lower.contains("make ")
        || lower.contains("peux-tu")
        || lower.contains("peux tu")
        || lower.contains("can you")
        || lower.contains("pourrais");
    audio && verb
}

fn tts_is_explain_only(lower: &str) -> bool {
    let has_payload = first_quoted(lower).is_some()
        || lower.contains("qui dit")
        || lower.contains("disant")
        || lower.contains("that says")
        || lower.contains("saying ");
    if has_payload {
        return false;
    }
    [
        "c'est quoi",
        "c est quoi",
        "qu'est-ce",
        "qu est-ce",
        "explique",
        "expliquer",
        "what is",
        "what's",
        "whats a",
        "how does",
        "how to",
        "comment fonctionne",
        "comment marche",
        "comment faire",
        "comment gener",
        "a quoi sert",
    ]
    .iter()
    .any(|p| lower.contains(p))
}

fn first_quoted(s: &str) -> Option<String> {
    let pairs = [('«', '»'), ('"', '"'), ('\'', '\''), ('“', '”')];
    for (open, close) in pairs {
        if let Some(start) = s.find(open) {
            let after = &s[start + open.len_utf8()..];
            if let Some(end) = after.find(close) {
                let inner = after[..end].trim();
                if !inner.is_empty() {
                    return Some(inner.to_string());
                }
            }
        }
    }
    None
}

fn extract_tts_spoken_text(text: &str) -> String {
    if let Some(q) = first_quoted(text) {
        return q;
    }
    let lower = text.to_lowercase();
    const MARKERS: &[&str] = &[
        "qui dit ",
        "disant ",
        "that says ",
        "saying ",
        "avec le texte ",
        "with text ",
        "texte :",
        "texte:",
        "text:",
        "text :",
    ];
    for marker in MARKERS {
        if let Some(idx) = lower.find(marker) {
            let after = text[idx + marker.len()..].trim();
            let after = after
                .trim_start_matches([':', '-', '—', '–'])
                .trim();
            if !after.is_empty() {
                return strip_wrapping_quotes(after);
            }
        }
    }
    if let Some(idx) = text.find(':') {
        let after = text[idx + 1..].trim();
        if !after.is_empty() {
            return strip_wrapping_quotes(after);
        }
    }
    strip_tts_preamble(text)
}

fn strip_wrapping_quotes(s: &str) -> String {
    let t = s.trim();
    if let Some(inner) = first_quoted(t) {
        return inner;
    }
    t.trim_matches(|c: char| "«»\"'“”".contains(c))
        .trim()
        .to_string()
}

fn strip_tts_preamble(text: &str) -> String {
    let mut rest = text.trim().to_string();
    const LEAD: &[&str] = &[
        "s'il te plaît ",
        "s'il vous plaît ",
        "s'il te plait ",
        "s'il vous plait ",
        "please ",
        "peux-tu ",
        "peux tu ",
        "peut-tu ",
        "pourrais-tu ",
        "pourrais tu ",
        "can you ",
        "je veux ",
        "j'aimerais ",
        "j aimerais ",
    ];
    let mut lower = rest.to_lowercase();
    for p in LEAD {
        if lower.starts_with(p) {
            rest = rest[p.len()..].trim().to_string();
            lower = rest.to_lowercase();
            break;
        }
    }
    const PHRASES: &[&str] = &[
        "générer un fichier audio",
        "genere un fichier audio",
        "génère un fichier audio",
        "genere-moi un fichier audio",
        "génère-moi un fichier audio",
        "générer un audio",
        "genere un audio",
        "génère un audio",
        "génère-moi un audio",
        "genere-moi un audio",
        "generate an audio",
        "generate audio",
        "create audio",
        "make an audio",
        "make audio",
        "fais un audio",
        "fait un audio",
        "crée un audio",
        "creer un audio",
        "créer un audio",
        "lis ce texte",
        "lire ce texte",
        "read this text",
        "synthèse vocale",
        "synthese vocale",
        "text to speech",
        "text-to-speech",
        "tts",
    ];
    for p in PHRASES {
        if lower.starts_with(p) {
            rest = rest[p.len()..].trim().to_string();
            lower = rest.to_lowercase();
            break;
        }
    }
    for conn in [" de ", " du ", " d'", " of ", " : ", ": "] {
        if lower.starts_with(conn.trim_start()) {
            let skip = if lower.starts_with(conn) {
                conn.len()
            } else {
                conn.trim_start().len()
            };
            rest = rest[skip..].trim().to_string();
            lower = rest.to_lowercase();
            break;
        }
    }
    if matches!(
        lower.as_str(),
        "" | "un audio"
            | "audio"
            | "un wav"
            | "wav"
            | "tts"
            | "la voix"
            | "une voix"
            | "speech"
    ) {
        return String::new();
    }
    rest
}

#[cfg(test)]
mod chat_delegation_tests {
    use super::{chat_tts_request, chat_user_wants_module_authoring};

    #[test]
    fn create_module_spawns() {
        assert!(chat_user_wants_module_authoring("crée un module ping"));
        assert!(chat_user_wants_module_authoring("Créer un module cohorte"));
        assert!(chat_user_wants_module_authoring("create a module named ping"));
        assert!(chat_user_wants_module_authoring("scaffold un module ext-rt"));
        assert!(chat_user_wants_module_authoring("installe un module"));
        assert!(chat_user_wants_module_authoring("ajoute une skill notes"));
        assert!(chat_user_wants_module_authoring("création d'un module ping"));
    }

    #[test]
    fn explain_does_not_spawn() {
        assert!(!chat_user_wants_module_authoring("c'est quoi un module"));
        assert!(!chat_user_wants_module_authoring("explique les modules"));
        assert!(!chat_user_wants_module_authoring("what is a skill"));
        assert!(!chat_user_wants_module_authoring("quels sont les modules installés"));
        assert!(!chat_user_wants_module_authoring("liste les modules"));
    }

    #[test]
    fn tts_request_opens_card() {
        assert_eq!(
            chat_tts_request("génère un audio qui dit bonjour"),
            Some("bonjour".into())
        );
        assert_eq!(
            chat_tts_request("peux-tu générer un audio de « hello »"),
            Some("hello".into())
        );
        assert_eq!(
            chat_tts_request("génère un audio : il fait beau"),
            Some("il fait beau".into())
        );
        assert_eq!(
            chat_tts_request("generate audio that says hello world"),
            Some("hello world".into())
        );
        assert_eq!(chat_tts_request("tts: bonjour"), Some("bonjour".into()));
        assert_eq!(chat_tts_request("génère un audio"), Some(String::new()));
    }

    #[test]
    fn tts_explain_and_module_are_not_cards() {
        assert!(chat_tts_request("c'est quoi la génération audio").is_none());
        assert!(chat_tts_request("comment générer un audio").is_none());
        assert!(chat_tts_request("crée un module ping").is_none());
        assert!(chat_tts_request("génère une image d'un chat").is_none());
    }
}

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

/// Entrée du catalogue local signé (E10 / Preview 0.6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogueEntry {
    pub name: String,
    pub version: String,
    /// `module` | `mcp`
    pub kind: String,
    /// Chemin relatif à la racine Preview (`share/...`).
    pub path: String,
    /// SHA-256 du WASM (`module`) ou du fichier (`mcp`), optionnellement préfixé `sha256:`.
    pub hash: String,
    #[serde(default)]
    pub attested_caps: Vec<String>,
}

/// `module.catalogue` — registre local signé (pas un store réseau).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleCatalogue {
    pub version: u32,
    pub entries: Vec<CatalogueEntry>,
    /// True si `catalogue.yaml.sig` a été vérifiée avec `catalogue.pub`.
    #[serde(default)]
    pub signature_ok: bool,
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
    /// Document UI déclaratif JSON (`declarative_ui`) ; vide → arbre par défaut.
    #[serde(default)]
    pub ui: String,
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

/// `module.uninstall` (F-MOD-01 / P08.7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleUninstallRequest {
    pub module: String,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub actor_caps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModuleInfo {
    pub name: String,
    pub version: String,
    pub granted_caps: Vec<String>,
    pub tools: Vec<String>,
    pub quarantined: bool,
    /// `declarative_ui` | `sandboxed_webview` | empty when no human UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_title: Option<String>,
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
/// L'identité fait foi via `Intent.from` (bus) ; `actor` est ignoré s'il diverge.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SecretGetRequest {
    pub name: String,
    /// Déprécié : conservé pour compat ; le daemon utilise `Intent.from`.
    #[serde(default)]
    pub actor: String,
}

/// `secrets.set` — écriture depuis UI Settings / services.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SecretSetRequest {
    pub name: String,
    /// Valeur vide = suppression.
    pub value: String,
}

/// `secrets.list` — noms uniquement.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SecretListRequest {}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SecretListResponse {
    pub names: Vec<String>,
    pub encrypted: bool,
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

/// Mode de session chat : 1:1 direct (défaut) ou salon multi-agent in-app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChatSessionMode {
    #[default]
    Direct,
    /// Salon multi-agent in-app (pas un canal Telegram/Discord).
    Room,
}

/// Membre d'un salon (`ChatSessionMode::Room`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatRoomMember {
    pub agent_id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona_id: Option<String>,
    pub joined_ms: u64,
}

fn default_max_agent_turns_per_user() -> u32 {
    4
}

/// Politique du conducteur de salon (runtime futur dans `aos-agentd`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatRoomConductorPolicy {
    #[serde(default = "default_max_agent_turns_per_user")]
    pub max_agent_turns_per_user: u32,
    #[serde(default = "default_true")]
    pub allow_peer_debate: bool,
}

impl Default for ChatRoomConductorPolicy {
    fn default() -> Self {
        Self {
            max_agent_turns_per_user: default_max_agent_turns_per_user(),
            allow_peer_debate: true,
        }
    }
}

/// Proportions prédéfinies du canvas de session.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CanvasAspect {
    #[default]
    Square,
    Landscape16x9,
    Landscape16x10,
    Portrait9x16,
    Landscape3x2,
}

impl CanvasAspect {
    /// Largeur / hauteur du cadre de dessin.
    pub fn ratio(&self) -> (f32, f32) {
        match self {
            Self::Square => (1.0, 1.0),
            Self::Landscape16x9 => (16.0, 9.0),
            Self::Landscape16x10 => (16.0, 10.0),
            Self::Portrait9x16 => (9.0, 16.0),
            Self::Landscape3x2 => (3.0, 2.0),
        }
    }

    /// Dimensions PNG (long edge = `long_edge`).
    pub fn export_dimensions(&self, long_edge: u32) -> (u32, u32) {
        let (rw, rh) = self.ratio();
        let long = long_edge.max(64);
        if rw >= rh {
            let w = long;
            let h = ((long as f32 * rh / rw).round() as u32).max(64);
            (w, h)
        } else {
            let h = long;
            let w = ((long as f32 * rw / rh).round() as u32).max(64);
            (w, h)
        }
    }

    /// Libellé court pour agents / brief (FR).
    pub fn agent_label_fr(&self) -> &'static str {
        match self {
            Self::Square => "carré 1:1",
            Self::Landscape16x9 => "16:9 paysage",
            Self::Landscape16x10 => "16:10 paysage",
            Self::Portrait9x16 => "9:16 portrait (vertical)",
            Self::Landscape3x2 => "3:2 paysage (horizontal)",
        }
    }

    /// Libellé court pour agents / brief (EN).
    pub fn agent_label_en(&self) -> &'static str {
        match self {
            Self::Square => "square 1:1",
            Self::Landscape16x9 => "16:9 landscape",
            Self::Landscape16x10 => "16:10 landscape",
            Self::Portrait9x16 => "9:16 portrait (vertical)",
            Self::Landscape3x2 => "3:2 landscape (horizontal)",
        }
    }
}

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
    /// `direct` (défaut) ou `room` — salon in-app uniquement.
    #[serde(default)]
    pub mode: ChatSessionMode,
    /// Membres du salon quand `mode == room`.
    #[serde(default)]
    pub members: Vec<ChatRoomMember>,
    /// Politique du conducteur (sérialisée même en mode direct pour stabilité JSON).
    #[serde(default)]
    pub conductor_policy: ChatRoomConductorPolicy,
    /// Panneau canvas ouvert dans le chat (défaut fermé).
    #[serde(default)]
    pub canvas_open: bool,
    /// Proportions du canvas de session (défaut carré 1:1).
    #[serde(default)]
    pub canvas_aspect: CanvasAspect,
}

/// Point normalisé 0..1 sur le canvas de session.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct CanvasPoint {
    pub x: f32,
    pub y: f32,
}

/// Corps d'une opération de dessin (sans seq / auteur — assignés côté store).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CanvasOpBody {
    Stroke {
        points: Vec<CanvasPoint>,
        /// `#RRGGBB` ou `#RRGGBBAA` — vide = crayon de session.
        #[serde(default)]
        color: String,
        /// Épaisseur relative au petit côté du canvas (0..1) — ≤0 = crayon de session.
        #[serde(default)]
        width: f32,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        #[serde(default)]
        color: String,
        #[serde(default)]
        fill: bool,
        #[serde(default)]
        width: f32,
    },
    Ellipse {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        #[serde(default)]
        color: String,
        #[serde(default)]
        fill: bool,
        #[serde(default)]
        width: f32,
    },
    Erase {
        points: Vec<CanvasPoint>,
        width: f32,
    },
    /// Segment droit (2 points).
    Line {
        p0: CanvasPoint,
        p1: CanvasPoint,
        #[serde(default)]
        color: String,
        #[serde(default)]
        width: f32,
    },
    /// Courbe lisse passant par les points de contrôle.
    Spline {
        points: Vec<CanvasPoint>,
        #[serde(default)]
        color: String,
        #[serde(default)]
        width: f32,
    },
    /// Remplissage par inondation à partir d'un point (coords 0..1).
    Fill {
        x: f32,
        y: f32,
        #[serde(default)]
        color: String,
    },
    Clear,
    Undo,
}

fn default_canvas_color() -> String {
    "#3ee0c4".into()
}

/// Style de crayon courant pour la session (persisté dans `canvas.json`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanvasPenStyle {
    #[serde(default = "default_canvas_color")]
    pub color: String,
    #[serde(default = "default_canvas_pen_width")]
    pub width: f32,
}

fn default_canvas_pen_width() -> f32 {
    0.015
}

impl Default for CanvasPenStyle {
    fn default() -> Self {
        Self {
            color: default_canvas_color(),
            width: default_canvas_pen_width(),
        }
    }
}

/// Normalise `#RRGGBB` (6 hex digits).
pub fn normalize_canvas_color(s: &str) -> Option<String> {
    let t = s.trim().trim_start_matches('#');
    if t.len() >= 6 && t.chars().take(6).all(|c| c.is_ascii_hexdigit()) {
        Some(format!("#{}", &t[..6].to_lowercase()))
    } else {
        None
    }
}

/// Marge recommandée pour le placement agent (coords normalisées 0..1).
pub const CANVAS_LAYOUT_MARGIN: f32 = 0.10;

fn clamp_canvas_unit(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn canvas_op_collect_coords(body: &CanvasOpBody) -> Vec<f32> {
    let mut values = Vec::new();
    match body {
        CanvasOpBody::Stroke { points, .. } | CanvasOpBody::Erase { points, .. } => {
            for p in points {
                values.push(p.x);
                values.push(p.y);
            }
        }
        CanvasOpBody::Line { p0, p1, .. } => {
            values.extend([p0.x, p0.y, p1.x, p1.y]);
        }
        CanvasOpBody::Spline { points, .. } => {
            for p in points {
                values.push(p.x);
                values.push(p.y);
            }
        }
        CanvasOpBody::Rect { x, y, w, h, .. } | CanvasOpBody::Ellipse { x, y, w, h, .. } => {
            values.extend([*x, *y, *x + *w, *y + *h]);
        }
        CanvasOpBody::Fill { x, y, .. } => {
            values.extend([*x, *y]);
        }
        CanvasOpBody::Clear | CanvasOpBody::Undo => {}
    }
    values
}

fn canvas_coord_scale(values: &[f32]) -> f32 {
    let max_val = values
        .iter()
        .copied()
        .filter(|v| v.is_finite() && *v > 0.0)
        .fold(0.0f32, f32::max);
    if max_val <= 1.5 {
        return 1.0;
    }
    for candidate in [200.0_f32, 256.0, 512.0, 1024.0] {
        if max_val <= candidate {
            return candidate;
        }
    }
    max_val
}

fn norm_point_scaled(p: &mut CanvasPoint, scale: f32) {
    p.x = clamp_canvas_unit(p.x / scale);
    p.y = clamp_canvas_unit(p.y / scale);
}

/// Normalise les coords d'une op canvas : clamp 0..1 ; si valeurs >1.5 (pixels), rescale.
/// Retourne `true` quand un rescale pixel→normalisé a été appliqué.
pub fn normalize_canvas_op_coords(body: &mut CanvasOpBody) -> bool {
    let values = canvas_op_collect_coords(body);
    if values.is_empty() {
        return false;
    }
    let scale = canvas_coord_scale(&values);
    let rescaled = scale > 1.0;
    match body {
        CanvasOpBody::Stroke { points, .. } | CanvasOpBody::Erase { points, .. } => {
            for p in points {
                if rescaled {
                    norm_point_scaled(p, scale);
                } else {
                    p.x = clamp_canvas_unit(p.x);
                    p.y = clamp_canvas_unit(p.y);
                }
            }
        }
        CanvasOpBody::Line { p0, p1, .. } => {
            if rescaled {
                norm_point_scaled(p0, scale);
                norm_point_scaled(p1, scale);
            } else {
                p0.x = clamp_canvas_unit(p0.x);
                p0.y = clamp_canvas_unit(p0.y);
                p1.x = clamp_canvas_unit(p1.x);
                p1.y = clamp_canvas_unit(p1.y);
            }
        }
        CanvasOpBody::Spline { points, .. } => {
            for p in points {
                if rescaled {
                    norm_point_scaled(p, scale);
                } else {
                    p.x = clamp_canvas_unit(p.x);
                    p.y = clamp_canvas_unit(p.y);
                }
            }
        }
        CanvasOpBody::Rect { x, y, w, h, .. } | CanvasOpBody::Ellipse { x, y, w, h, .. } => {
            if rescaled {
                *x = clamp_canvas_unit(*x / scale);
                *y = clamp_canvas_unit(*y / scale);
                *w = clamp_canvas_unit(*w / scale);
                *h = clamp_canvas_unit(*h / scale);
            } else {
                *x = clamp_canvas_unit(*x);
                *y = clamp_canvas_unit(*y);
                *w = clamp_canvas_unit(*w);
                *h = clamp_canvas_unit(*h);
            }
        }
        CanvasOpBody::Fill { x, y, .. } => {
            if rescaled {
                *x = clamp_canvas_unit(*x / scale);
                *y = clamp_canvas_unit(*y / scale);
            } else {
                *x = clamp_canvas_unit(*x);
                *y = clamp_canvas_unit(*y);
            }
        }
        CanvasOpBody::Clear | CanvasOpBody::Undo => {}
    }
    rescaled
}

/// True when two bboxes overlap (optionally requiring at least `min_gap` separation).
pub fn canvas_bbox_overlaps(a: CanvasBBox, b: CanvasBBox, min_gap: f32) -> bool {
    !(a.x1 + min_gap <= b.x0
        || b.x1 + min_gap <= a.x0
        || a.y1 + min_gap <= b.y0
        || b.y1 + min_gap <= a.y0)
}

/// Remplit couleur / épaisseur manquantes depuis le crayon de session.
pub fn resolve_canvas_op_style(body: &mut CanvasOpBody, pen: &CanvasPenStyle) {
    match body {
        CanvasOpBody::Stroke { color, width, .. }
        | CanvasOpBody::Line { color, width, .. }
        | CanvasOpBody::Spline { color, width, .. } => {
            if color.is_empty() {
                *color = pen.color.clone();
            }
            if *width <= 0.0 {
                *width = pen.width;
            }
        }
        CanvasOpBody::Rect { color, width, .. } | CanvasOpBody::Ellipse { color, width, .. } => {
            if color.is_empty() {
                *color = pen.color.clone();
            }
            if *width <= 0.0 {
                *width = pen.width;
            }
        }
        CanvasOpBody::Fill { color, .. } => {
            if color.is_empty() {
                *color = pen.color.clone();
            }
        }
        CanvasOpBody::Erase { width, .. } => {
            if *width <= 0.0 {
                *width = pen.width.max(0.03);
            }
        }
        CanvasOpBody::Clear | CanvasOpBody::Undo => {}
    }
}

/// Boîte englobante normalisée 0..1 pour une opération canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasBBox {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl CanvasBBox {
    fn empty() -> Self {
        Self {
            x0: 1.0,
            y0: 1.0,
            x1: 0.0,
            y1: 0.0,
        }
    }

    fn is_valid(self) -> bool {
        self.x0 <= self.x1 && self.y0 <= self.y1
    }

    fn expand_point(&mut self, x: f32, y: f32) {
        let x = x.clamp(0.0, 1.0);
        let y = y.clamp(0.0, 1.0);
        self.x0 = self.x0.min(x);
        self.y0 = self.y0.min(y);
        self.x1 = self.x1.max(x);
        self.y1 = self.y1.max(y);
    }

    fn expand_points(&mut self, points: &[CanvasPoint]) {
        for p in points {
            self.expand_point(p.x, p.y);
        }
    }

    fn expand_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.expand_point(x, y);
        self.expand_point(x + w, y + h);
    }
}

/// Boîte englobante d'une opération (pour digest agent / UI).
pub fn canvas_op_bbox(body: &CanvasOpBody) -> Option<CanvasBBox> {
    let mut b = CanvasBBox::empty();
    match body {
        CanvasOpBody::Stroke { points, .. } | CanvasOpBody::Erase { points, .. } => {
            if points.is_empty() {
                return None;
            }
            b.expand_points(points);
        }
        CanvasOpBody::Line { p0, p1, .. } => {
            b.expand_point(p0.x, p0.y);
            b.expand_point(p1.x, p1.y);
        }
        CanvasOpBody::Spline { points, .. } => {
            if points.len() < 2 {
                return None;
            }
            b.expand_points(points);
        }
        CanvasOpBody::Rect { x, y, w, h, .. } | CanvasOpBody::Ellipse { x, y, w, h, .. } => {
            b.expand_rect(*x, *y, *w, *h);
        }
        CanvasOpBody::Fill { x, y, .. } => {
            b.expand_point(*x, *y);
            // Tiny bbox so digest shows a non-degenerate region.
            b.expand_point((x + 0.01).min(1.0), (y + 0.01).min(1.0));
        }
        CanvasOpBody::Clear | CanvasOpBody::Undo => return None,
    }
    if b.is_valid() {
        Some(b)
    } else {
        None
    }
}

fn canvas_op_kind_label(body: &CanvasOpBody) -> &'static str {
    match body {
        CanvasOpBody::Stroke { .. } => "stroke",
        CanvasOpBody::Rect { .. } => "rect",
        CanvasOpBody::Ellipse { .. } => "ellipse",
        CanvasOpBody::Erase { .. } => "erase",
        CanvasOpBody::Line { .. } => "line",
        CanvasOpBody::Spline { .. } => "spline",
        CanvasOpBody::Fill { .. } => "fill",
        CanvasOpBody::Clear => "clear",
        CanvasOpBody::Undo => "undo",
    }
}

/// Digest compact du canvas pour injection runtime (agents / room turns).
/// Résumé : compteurs par kind + bbox par seq — pas de dump JSON des ops.
pub fn canvas_scene_digest(doc: &CanvasDoc, aspect: CanvasAspect) -> String {
    use std::collections::BTreeMap;

    let mut kind_counts: BTreeMap<&'static str, u32> = BTreeMap::new();
    let mut author_counts: BTreeMap<String, u32> = BTreeMap::new();
    let mut scene_bbox = CanvasBBox::empty();
    let mut has_scene_bbox = false;

    for op in &doc.ops {
        let kind = canvas_op_kind_label(&op.body);
        *kind_counts.entry(kind).or_insert(0) += 1;
        *author_counts.entry(op.author_id.clone()).or_insert(0) += 1;
        if let Some(b) = canvas_op_bbox(&op.body) {
            if has_scene_bbox {
                scene_bbox.x0 = scene_bbox.x0.min(b.x0);
                scene_bbox.y0 = scene_bbox.y0.min(b.y0);
                scene_bbox.x1 = scene_bbox.x1.max(b.x1);
                scene_bbox.y1 = scene_bbox.y1.max(b.y1);
            } else {
                scene_bbox = b;
                has_scene_bbox = true;
            }
        }
    }

    let mut lines = vec![
        format!(
            "next_seq={} aspect={} ops={} pen={} width={:.3}",
            doc.next_seq,
            aspect.agent_label_en(),
            doc.ops.len(),
            doc.pen.color,
            doc.pen.width
        ),
        "coords=normalized 0..1 (origin top-left; x→ right, y↓ down; letterboxed board face — not pixels; max=1.0)"
            .into(),
        format!(
            "margin={:.2} usable=({:.2},{:.2})-({:.2},{:.2})",
            CANVAS_LAYOUT_MARGIN,
            CANVAS_LAYOUT_MARGIN,
            CANVAS_LAYOUT_MARGIN,
            1.0 - CANVAS_LAYOUT_MARGIN,
            1.0 - CANVAS_LAYOUT_MARGIN,
        ),
        "placement=read scene_bbox + per-seq bbox; place new ops inside usable; avoid stacking on the same center"
            .into(),
    ];

    if !kind_counts.is_empty() {
        let counts: Vec<String> = kind_counts
            .iter()
            .map(|(k, n)| format!("{k}={n}"))
            .collect();
        lines.push(format!("counts: {}", counts.join(", ")));
    }
    if !author_counts.is_empty() {
        let authors: Vec<String> = author_counts
            .iter()
            .map(|(a, n)| format!("{a}={n}"))
            .collect();
        lines.push(format!("authors: {}", authors.join(", ")));
    }
    if has_scene_bbox {
        lines.push(format!(
            "scene_bbox=({:.3},{:.3})-({:.3},{:.3})",
            scene_bbox.x0, scene_bbox.y0, scene_bbox.x1, scene_bbox.y1
        ));
    }

    const MAX_OPS: usize = 48;
    let truncated = doc.ops.len() > MAX_OPS;
    let show = if truncated {
        &doc.ops[doc.ops.len() - MAX_OPS..]
    } else {
        &doc.ops[..]
    };
    for op in show {
        let kind = canvas_op_kind_label(&op.body);
        if let Some(b) = canvas_op_bbox(&op.body) {
            lines.push(format!(
                "seq={} {kind} ({:.3},{:.3})-({:.3},{:.3})",
                op.seq, b.x0, b.y0, b.x1, b.y1
            ));
        } else {
            lines.push(format!("seq={} {kind}", op.seq));
        }
    }
    if truncated {
        lines.push(format!(
            "... +{} older ops (use canvas.get after_seq for deltas)",
            doc.ops.len() - MAX_OPS
        ));
    }

    lines.join("\n")
}

/// Opération de dessin persistée (document vectoriel de session).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanvasOp {
    pub seq: u64,
    pub author_id: String,
    pub ts_ms: u64,
    #[serde(flatten)]
    pub body: CanvasOpBody,
}

/// Document canvas d'une session (`var/sessions/<id>/canvas.json`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CanvasDoc {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub next_seq: u64,
    #[serde(default)]
    pub ops: Vec<CanvasOp>,
    /// Crayon courant (couleur / épaisseur) pour humain et agents.
    #[serde(default)]
    pub pen: CanvasPenStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasGetRequest {
    pub session_id: String,
    /// Si défini, ne retourner que les ops avec `seq > after_seq`.
    #[serde(default)]
    pub after_seq: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasGetResponse {
    pub session_id: String,
    pub canvas_open: bool,
    #[serde(default)]
    pub canvas_aspect: CanvasAspect,
    pub next_seq: u64,
    pub ops: Vec<CanvasOp>,
    #[serde(default)]
    pub pen: CanvasPenStyle,
    /// True while a vision model is actively reading this canvas (tester-cohort slice 2).
    #[serde(default)]
    pub canvas_seeing: bool,
}

/// `canvas.seeing` — signal that a vision pass is reading the live canvas (not a mutation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasSeeingRequest {
    pub session_id: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasSetStyleRequest {
    pub session_id: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub width: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasSetStyleResponse {
    pub doc: CanvasDoc,
    pub canvas_open: bool,
    pub pen: CanvasPenStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasApplyRequest {
    pub session_id: String,
    pub author_id: String,
    pub op: CanvasOpBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasApplyResponse {
    pub doc: CanvasDoc,
    pub canvas_open: bool,
    /// Op nouvellement commitée (`None` pour `undo`/`clear` sans nouvelle entrée).
    #[serde(default)]
    pub applied: Option<CanvasOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasSetOpenRequest {
    pub session_id: String,
    pub open: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasSetAspectRequest {
    pub session_id: String,
    pub aspect: CanvasAspect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasExportRequest {
    pub session_id: String,
    /// Chemin logique sous `/downloads` (défaut auto).
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}

/// Pièce jointe d'un message de session (ex. référence agent en fond).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    Image {
        path: String,
        #[serde(default)]
        prompt: String,
    },
    Audio {
        path: String,
    },
    /// In-chat TTS options card (P09.8) — generate after the human confirms.
    TtsDraft {
        text: String,
        #[serde(default)]
        model_id: Option<String>,
        #[serde(default)]
        options: MediaAudioOptions,
    },
    /// Local document attached in chat (text extracted at send — not vision).
    Document {
        path: String,
        #[serde(default)]
        label: String,
    },
    /// Action agent en attente d'Allow Once dans le fil (tester-cohort slice 1).
    AgentAct {
        agent_id: String,
        act_id: String,
        /// Legacy human phrase (pre-i18n sessions); UI prefers `action` + `args`.
        #[serde(default)]
        phrase: String,
        /// Tool action id (e.g. `notes.create`); formatted in UI i18n.
        #[serde(default)]
        action: String,
        /// Tool args for phrase interpolation.
        #[serde(default)]
        args: serde_json::Value,
        /// `pending` | `approved` | `denied` | `done`
        #[serde(default = "default_agent_act_state")]
        state: String,
    },
}

fn default_agent_act_state() -> String {
    "pending".into()
}

impl ChatAttachment {
    pub fn as_agent_ref(&self) -> Option<(&str, &str, &str)> {
        match self {
            Self::AgentRef {
                agent_id,
                title,
                origin,
            } => Some((agent_id.as_str(), title.as_str(), origin.as_str())),
            Self::Image { .. }
            | Self::Audio { .. }
            | Self::TtsDraft { .. }
            | Self::Document { .. }
            | Self::AgentAct { .. } => None,
        }
    }

    pub fn as_agent_act(&self) -> Option<(&str, &str, &str)> {
        match self {
            Self::AgentAct {
                agent_id,
                act_id,
                state,
                ..
            } => Some((agent_id.as_str(), act_id.as_str(), state.as_str())),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionMessage {
    pub role: String,
    pub content: String,
    pub ts_ms: u64,
    #[serde(default)]
    pub attachments: Vec<ChatAttachment>,
    /// Agent membre qui a produit le message en mode salon (`role` peut rester `assistant`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker_name: Option<String>,
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
    #[serde(default)]
    pub speaker_id: Option<String>,
    #[serde(default)]
    pub speaker_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionSetModeRequest {
    pub session_id: String,
    pub mode: ChatSessionMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionMembersAddRequest {
    pub session_id: String,
    pub member: ChatRoomMember,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionMembersRemoveRequest {
    pub session_id: String,
    pub agent_id: String,
}

/// Réponse `chat.session.members.list` — membres persistés du salon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionMembersListResponse {
    pub members: Vec<ChatRoomMember>,
}

/// Tour de salon orchestré par le conducteur (`aos-agentd`).
/// Intent bus `chat.session.room.turn` — platform valide `mode == room`, relay vers `agent.room_conduct`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionRoomTurnRequest {
    pub session_id: String,
    pub content: String,
    /// Chemins PNG/JPEG pour le tour user (vision).
    #[serde(default)]
    pub images: Vec<String>,
}

/// Annule le tour de salon en cours pour une session (`chat.session.room.turn.cancel`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionRoomTurnCancelRequest {
    pub session_id: String,
}

/// Réponse `chat.session.room.turn` après orchestration conducteur.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionRoomTurnResponse {
    pub agent_turns: u32,
    #[serde(default)]
    pub cancelled: bool,
}

/// `agent.room_conduct` — orchestration complète d'un message utilisateur en salon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRoomConductRequest {
    pub session_id: String,
    pub content: String,
    /// Chemins PNG/JPEG du tour user (propagés aux membres vision).
    #[serde(default)]
    pub images: Vec<String>,
}

/// Réponse `agent.room_conduct`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRoomConductResponse {
    pub agent_turns: u32,
    #[serde(default)]
    pub cancelled: bool,
}

/// `agent.room_turn` — inférence one-shot d'un membre du salon (sans spawn worker).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRoomTurnRequest {
    pub session_id: String,
    pub agent_id: String,
    pub display_name: String,
    /// Message utilisateur déclencheur (dernier tour).
    pub user_message: String,
}

/// Réponse `agent.room_turn`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRoomTurnResponse {
    pub content: String,
    pub speaker_id: String,
    pub speaker_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionGetResponse {
    pub meta: ChatSessionMeta,
    pub messages: Vec<ChatSessionMessage>,
}

// ---------------------------------------------------------------------------
// Memory partagée / user (PC.7)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemSharedReadRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemSharedWriteRequest {
    pub name: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemUserRememberRequest {
    pub text: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub pinned: bool,
    /// Si true (défaut), lie automatiquement un hit proche via `updates`/`supersedes`.
    #[serde(default = "default_true")]
    pub auto_link: bool,
    /// Seuil cosinus pour l'auto-link (défaut 0.82).
    #[serde(default = "default_auto_link_threshold")]
    pub auto_link_threshold: f32,
}

impl Default for MemUserRememberRequest {
    fn default() -> Self {
        Self {
            text: String::new(),
            metadata: serde_json::Value::Null,
            pinned: false,
            auto_link: true,
            auto_link_threshold: default_auto_link_threshold(),
        }
    }
}

fn default_auto_link_threshold() -> f32 {
    0.82
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemUserRecallRequest {
    pub query: String,
    #[serde(default = "default_k")]
    pub k: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemContextRequest {
    /// Session chat active (`session:<id>`).
    #[serde(default)]
    pub session_id: Option<String>,
    pub query: String,
    #[serde(default = "default_k")]
    pub k: usize,
    /// Top-k product-doc RAG hits (`product:docs`). 0 = default (4).
    #[serde(default)]
    pub product_k: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemContextResponse {
    pub session_hits: Vec<MemHit>,
    pub user_hits: Vec<MemHit>,
    /// Extraíts docs Preview (FEATURES / STATUS / TESTER).
    #[serde(default)]
    pub product_hits: Vec<MemHit>,
    pub prompt_block: String,
}

/// `mem.extract` — extraction LLM de faits durables depuis un tour de chat (E14).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemExtractRequest {
    pub user_text: String,
    pub assistant_text: String,
    #[serde(default)]
    pub session_id: Option<String>,
    /// Id modèle instruct (défaut = modèle système).
    #[serde(default)]
    pub model_id: Option<String>,
    /// Si false, extrait sans écrire (dry-run / tests).
    #[serde(default = "default_true")]
    pub persist: bool,
}

impl Default for MemExtractRequest {
    fn default() -> Self {
        Self {
            user_text: String::new(),
            assistant_text: String::new(),
            session_id: None,
            model_id: None,
            persist: true,
        }
    }
}

/// Un fait proposé par l'extracteur (avant filtre / persist).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemExtractedFact {
    pub text: String,
    #[serde(default)]
    pub supersedes_hint: Option<String>,
}

/// Issue pour un candidat (stocké, skip dédup, ou filtré secret).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemExtractOutcomeKind {
    Stored,
    SkippedDuplicate,
    FilteredSecret,
    /// One-shot canvas / draw / completion task — not a durable user fact.
    FilteredEphemeral,
    /// Tool trace, agent action log, or delegation prose — not a human fact.
    FilteredTrace,
    SkippedEmpty,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemExtractOutcome {
    pub kind: MemExtractOutcomeKind,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auto_relations: Vec<MemRelation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemExtractResponse {
    pub facts_proposed: Vec<MemExtractedFact>,
    pub outcomes: Vec<MemExtractOutcome>,
    /// Nombre de faits réellement écrits.
    pub stored: usize,
}

/// `mem.sweep` — repasse quotidienne : re-extract des sessions du jour local.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemSweepRequest {
    /// Décalage fuseau local en minutes (ex. Paris été = 120). Défaut = OS / UTC.
    #[serde(default)]
    pub tz_offset_minutes: Option<i32>,
    /// Id modèle instruct (défaut = modèle système).
    #[serde(default)]
    pub model_id: Option<String>,
    /// Si false, parcourt sans infer ni écriture (dry-run / tests).
    #[serde(default = "default_true")]
    pub persist: bool,
    /// Force la passe même si déjà exécutée pour le jour local courant.
    #[serde(default)]
    pub force: bool,
}

impl Default for MemSweepRequest {
    fn default() -> Self {
        Self {
            tz_offset_minutes: None,
            model_id: None,
            persist: true,
            force: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemSweepResponse {
    /// Clé jour local (`day-<n>`) couverte par cette passe.
    pub local_day_key: String,
    pub sessions_scanned: usize,
    pub turns_replayed: usize,
    pub facts_proposed: usize,
    pub stored: usize,
    pub skipped_duplicate: usize,
    pub filtered: usize,
    pub relations_created: usize,
    pub last_pass_ms: u64,
}

/// `mem.sweep.status` — dernière passe quotidienne (pour l'onglet Mémoire).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct MemSweepStatus {
    #[serde(default)]
    pub last_pass_ms: u64,
    #[serde(default)]
    pub last_local_day_key: String,
    #[serde(default)]
    pub relations_created: u64,
}

/// Prompt système pour l'extraction post-tour (local_only, JSON strict).
pub const MEM_EXTRACT_SYSTEM_PROMPT: &str = r#"Extract DURABLE facts about the USER from one chat turn.
The user may write in any language, any grammatical person, or short fragments.
Reply with valid JSON only — no markdown, no chain-of-thought:
{"facts":[{"text":"...","supersedes_hint":null}]}

Rules:
- Max 5 facts. Each "text" is a short third-person sentence in the user's language.
- Extract identity (name, role), standing preferences, stable constraints, decisions that should persist across sessions.
- Do NOT extract: secrets, passwords, tokens, API keys, IBAN, PEM, OTP, vault contents.
- Do NOT extract: greetings, one-off questions, OS commands, tool traces, agent action logs, or assistant prose.
- Do NOT extract one-shot tasks or completions: draw/sketch/canvas requests ("dessine une maison", "draw a cat on the canvas"), image generation asks, or any ephemeral command that should not survive the next session.
- Standing creative preferences ARE durable ("likes watercolor", "prefers French") — but a single draw command is NOT.
- The USER message is the only source of facts. Ignore assistant claims about having "saved" or "remembered" anything.
- If none: {"facts":[]}
- "supersedes_hint" optional: older fact this one replaces (free text).
"#;

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

/// Copie un fichier hôte déjà présent (ex. sortie sd.cpp) vers un chemin logique
/// sans transporter les octets sur le bus IPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsWriteFromPathRequest {
    pub path: String,
    /// Chemin absolu sur l'hôte (doit être sous `%TEMP%` ou `$AOS_HOME/var/tmp`).
    pub source_host_path: String,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsWriteFromPathResponse {
    pub version: u64,
    pub bytes: u64,
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

// ---------------------------------------------------------------------------
// Media API (E16 / Preview 0.8)
// ---------------------------------------------------------------------------

/// Closed sd.cpp option object (P09.3). Unknown keys are refused.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct MediaImageOptions {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub steps: Option<u32>,
    pub cfg_scale: Option<f32>,
    pub seed: Option<i64>,
    pub sampling_method: Option<String>,
    pub negative_prompt: Option<String>,
    pub threads: Option<u32>,
    /// Style preset ids or custom text fragments (not filesystem paths).
    #[serde(default)]
    pub styles: Vec<String>,
    /// LoRA filenames in `share/models/lora/` (stem used for `<lora:name:scale>`).
    #[serde(default)]
    pub loras: Vec<String>,
    /// Shared LoRA strength for all selected LoRAs (default 1.0).
    pub lora_scale: Option<f32>,
    /// Catalogue VAE id in `share/models/vae/` (single override).
    pub vae: Option<String>,
    /// sd.cpp `--backend` (cpu/gpu/cuda0/vulkan0 or mixed `te=cpu,diffusion=cuda0`).
    pub backend: Option<String>,
    /// sd.cpp `--params-backend` (cpu/cuda0/disk or mixed).
    pub params_backend: Option<String>,
    /// sd.cpp `--offload-to-cpu` (weights in RAM, staged to GPU).
    pub offload_to_cpu: Option<bool>,
    /// sd.cpp `--diffusion-fa` (flash attention).
    pub diffusion_fa: Option<bool>,
    /// sd.cpp `--auto-fit` (ignore explicit backend / params-backend).
    pub auto_fit: Option<bool>,
    /// sd.cpp `--max-vram` (`-1` = free VRAM minus 1 GiB, or a GiB cap / `cuda0=8`).
    pub max_vram: Option<String>,
    /// sd.cpp `--stream-layers` (transformer blocks streamed; needs CPU params).
    pub stream_layers: Option<bool>,
    /// sd.cpp `--flow-shift` (flow-matching: Qwen Image, FLUX, Wan…).
    pub flow_shift: Option<f32>,
    /// sd.cpp `-M` mode (`img_gen`, `vid_gen`, …).
    pub sd_mode: Option<String>,
    /// sd.cpp `--video-frames` (Wan/LTX; `1` ≈ single still).
    pub video_frames: Option<u32>,
    /// ESRGAN upscaler filename/id in `share/models/upscale/` (`.pth` / `.safetensors`).
    pub upscale_model: Option<String>,
    /// sd.cpp `--upscale-repeats` (default 1 when upscale_model is set).
    pub upscale_repeats: Option<u32>,
    /// sd.cpp `--upscale-tile-size` (VRAM tiling; default 128 in sd.cpp).
    pub upscale_tile_size: Option<u32>,
    /// Logical path of an init image for img2img (`/downloads/...`).
    pub init_image: Option<String>,
    /// img2img denoise strength 0..=1 (sd.cpp `--strength`; default ~0.75 when init set).
    pub strength: Option<f32>,
    /// Logical path of an inpaint mask PNG (`/downloads/...`; white = regenerate region).
    pub mask_image: Option<String>,
}

/// Closed Piper option object (P09.3). Unknown keys are refused.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct MediaAudioOptions {
    pub length_scale: Option<f32>,
    pub noise_scale: Option<f32>,
    pub noise_w: Option<f32>,
    pub sentence_silence: Option<f32>,
    pub speaker: Option<u32>,
}

/// `media.image.generate` — prompt → PNG sous `/downloads`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaImageGenerateRequest {
    pub prompt: String,
    /// Logical FS path (default `/downloads/image-<ts>.png`).
    #[serde(default)]
    pub path: Option<String>,
    /// Offering id (`local:sd-v1-5`). Empty → first installed image pack.
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub options: MediaImageOptions,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub trace_id: String,
}

/// `media.image.upscale` — ESRGAN upscale of an existing PNG (sd.cpp `--mode upscale`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaImageUpscaleRequest {
    /// Logical path of the source image (e.g. `/downloads/image-123.png`).
    pub source_path: String,
    /// Output logical path (default: `{source}-upscaled.png`).
    #[serde(default)]
    pub output_path: Option<String>,
    /// Upscaler filename/id in `share/models/upscale/`.
    pub upscale_model: String,
    #[serde(default)]
    pub upscale_repeats: Option<u32>,
    #[serde(default)]
    pub upscale_tile_size: Option<u32>,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub trace_id: String,
}

/// `media.audio.generate` — text → WAV TTS sous `/downloads`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaAudioGenerateRequest {
    pub text: String,
    #[serde(default)]
    pub path: Option<String>,
    /// Offering id (`local:piper-en-us` / `local:piper-fr-fr`).
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub options: MediaAudioOptions,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaGenerateResponse {
    pub path: String,
    pub bytes: u64,
    /// `sdcpp` | `piper` | `stub`
    pub engine: String,
    pub model_id: String,
}

#[cfg(test)]
mod media_option_tests {
    use super::{MediaAudioOptions, MediaImageOptions};

    #[test]
    fn image_options_refuse_unknown_keys() {
        let err = serde_json::from_str::<MediaImageOptions>(r#"{"steps":8,"argv":"--foo"}"#)
            .unwrap_err();
        let s = err.to_string();
        assert!(s.contains("unknown") || s.contains("argv"), "{s}");
    }

    #[test]
    fn audio_options_accept_known() {
        let o: MediaAudioOptions =
            serde_json::from_str(r#"{"length_scale":1.1,"speaker":0}"#).unwrap();
        assert_eq!(o.length_scale, Some(1.1));
    }

    #[test]
    fn image_options_accept_img2img() {
        let o: MediaImageOptions = serde_json::from_str(
            r#"{"init_image":"/downloads/base.png","strength":0.65,"steps":12}"#,
        )
        .unwrap();
        assert_eq!(o.init_image.as_deref(), Some("/downloads/base.png"));
        assert_eq!(o.strength, Some(0.65));
    }

    #[test]
    fn image_options_accept_inpaint_mask() {
        let o: MediaImageOptions = serde_json::from_str(
            r#"{"init_image":"/downloads/base.png","mask_image":"/downloads/mask.png","strength":1.0}"#,
        )
        .unwrap();
        assert_eq!(o.mask_image.as_deref(), Some("/downloads/mask.png"));
    }
}

#[cfg(test)]
mod chat_session_room_tests {
    use super::{
        AgentCreateRequest, AgentGoal, AgentInfo, AgentKind, AgentState, CanvasAspect,
        ChatRoomMember, ChatSessionMessage, ChatSessionMeta, ChatSessionMode,
        ChatRoomConductorPolicy,
    };

    #[test]
    fn legacy_meta_without_mode_or_members() {
        let m: ChatSessionMeta = serde_json::from_str(
            r#"{
                "id":"sess-1",
                "title":"Test",
                "created_ms":1,
                "updated_ms":2,
                "archived":false,
                "message_count":0
            }"#,
        )
        .unwrap();
        assert_eq!(m.mode, ChatSessionMode::Direct);
        assert!(m.members.is_empty());
        assert_eq!(m.conductor_policy.max_agent_turns_per_user, 4);
        assert!(m.conductor_policy.allow_peer_debate);
    }

    #[test]
    fn room_meta_with_members_roundtrip() {
        let m = ChatSessionMeta {
            id: "sess-room".into(),
            title: "Salon".into(),
            created_ms: 10,
            updated_ms: 20,
            archived: false,
            message_count: 3,
            model_id: None,
            mode: ChatSessionMode::Room,
            members: vec![
                ChatRoomMember {
                    agent_id: "agent-a".into(),
                    display_name: "Alpha".into(),
                    persona_id: Some("p1".into()),
                    joined_ms: 10,
                },
                ChatRoomMember {
                    agent_id: "agent-b".into(),
                    display_name: "Beta".into(),
                    persona_id: None,
                    joined_ms: 11,
                },
            ],
            conductor_policy: ChatRoomConductorPolicy {
                max_agent_turns_per_user: 2,
                allow_peer_debate: false,
            },
            canvas_open: false,
            canvas_aspect: CanvasAspect::Square,
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: ChatSessionMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(back.mode, ChatSessionMode::Room);
        assert_eq!(back.members.len(), 2);
        assert_eq!(back.members[0].agent_id, "agent-a");
        assert_eq!(back.conductor_policy.max_agent_turns_per_user, 2);
        assert!(!back.conductor_policy.allow_peer_debate);
        assert!(!back.canvas_open);
        assert_eq!(back.canvas_aspect, CanvasAspect::Square);
    }

    #[test]
    fn canvas_aspect_export_dimensions() {
        use super::CanvasAspect;
        assert_eq!(CanvasAspect::Square.export_dimensions(1024), (1024, 1024));
        assert_eq!(CanvasAspect::Landscape16x9.export_dimensions(1024), (1024, 576));
        assert_eq!(CanvasAspect::Portrait9x16.export_dimensions(1024), (576, 1024));
        assert_eq!(CanvasAspect::Landscape3x2.export_dimensions(1024), (1024, 683));
    }

    #[test]
    fn legacy_meta_without_canvas_aspect() {
        let m: ChatSessionMeta = serde_json::from_str(
            r#"{
                "id":"sess-1",
                "title":"Test",
                "created_ms":1,
                "updated_ms":2,
                "archived":false,
                "message_count":0,
                "canvas_open":true
            }"#,
        )
        .unwrap();
        assert_eq!(m.canvas_aspect, CanvasAspect::Square);
    }

    #[test]
    fn canvas_op_stroke_roundtrip() {
        use super::{CanvasOp, CanvasOpBody, CanvasPoint};
        let op = CanvasOp {
            seq: 1,
            author_id: "human".into(),
            ts_ms: 42,
            body: CanvasOpBody::Stroke {
                points: vec![CanvasPoint { x: 0.1, y: 0.2 }, CanvasPoint { x: 0.3, y: 0.4 }],
                color: "#3ee0c4".into(),
                width: 0.02,
            },
        };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("\"kind\":\"stroke\""));
        let back: CanvasOp = serde_json::from_str(&json).unwrap();
        assert_eq!(back, op);
    }

    #[test]
    fn canvas_op_line_spline_fill_roundtrip() {
        use super::{CanvasOp, CanvasOpBody, CanvasPoint};
        let line = CanvasOpBody::Line {
            p0: CanvasPoint { x: 0.1, y: 0.2 },
            p1: CanvasPoint { x: 0.8, y: 0.7 },
            color: "#ff0000".into(),
            width: 0.01,
        };
        let line_json = serde_json::to_string(&line).unwrap();
        assert!(line_json.contains("\"kind\":\"line\""));
        let back: CanvasOpBody = serde_json::from_str(&line_json).unwrap();
        assert_eq!(back, line);

        let spline = CanvasOpBody::Spline {
            points: vec![
                CanvasPoint { x: 0.1, y: 0.5 },
                CanvasPoint { x: 0.4, y: 0.2 },
                CanvasPoint { x: 0.7, y: 0.8 },
            ],
            color: "#00ff00".into(),
            width: 0.015,
        };
        let spline_json = serde_json::to_string(&spline).unwrap();
        assert!(spline_json.contains("\"kind\":\"spline\""));
        assert_eq!(
            serde_json::from_str::<CanvasOpBody>(&spline_json).unwrap(),
            spline
        );

        let fill = CanvasOpBody::Fill {
            x: 0.5,
            y: 0.5,
            color: "#0000ff".into(),
        };
        let fill_json = serde_json::to_string(&fill).unwrap();
        assert!(fill_json.contains("\"kind\":\"fill\""));
        assert_eq!(serde_json::from_str::<CanvasOpBody>(&fill_json).unwrap(), fill);
    }

    #[test]
    fn resolve_canvas_op_style_inherits_pen() {
        use super::{resolve_canvas_op_style, CanvasOpBody, CanvasPenStyle, CanvasPoint};
        let pen = CanvasPenStyle {
            color: "#aabbcc".into(),
            width: 0.022,
        };
        let mut stroke = CanvasOpBody::Stroke {
            points: vec![CanvasPoint { x: 0.0, y: 0.0 }, CanvasPoint { x: 1.0, y: 1.0 }],
            color: String::new(),
            width: 0.0,
        };
        resolve_canvas_op_style(&mut stroke, &pen);
        match stroke {
            CanvasOpBody::Stroke { color, width, .. } => {
                assert_eq!(color, "#aabbcc");
                assert!((width - 0.022).abs() < 0.0001);
            }
            _ => panic!("expected stroke"),
        }
    }

    #[test]
    fn canvas_scene_digest_contains_seq() {
        use super::{
            canvas_scene_digest, CanvasAspect, CanvasDoc, CanvasOp, CanvasOpBody, CanvasPenStyle,
            CanvasPoint,
        };
        let doc = CanvasDoc {
            session_id: "sess-1".into(),
            next_seq: 3,
            pen: CanvasPenStyle::default(),
            ops: vec![
                CanvasOp {
                    seq: 1,
                    author_id: "human".into(),
                    ts_ms: 1,
                    body: CanvasOpBody::Line {
                        p0: CanvasPoint { x: 0.1, y: 0.1 },
                        p1: CanvasPoint { x: 0.2, y: 0.2 },
                        color: "#3ee0c4".into(),
                        width: 0.01,
                    },
                },
                CanvasOp {
                    seq: 2,
                    author_id: "agent-a".into(),
                    ts_ms: 2,
                    body: CanvasOpBody::Fill {
                        x: 0.5,
                        y: 0.5,
                        color: "#ff00ff".into(),
                    },
                },
            ],
        };
        let digest = canvas_scene_digest(&doc, CanvasAspect::Square);
        assert!(digest.contains("coords=normalized"));
        assert!(digest.contains("next_seq=3"));
        assert!(digest.contains("seq=1"));
        assert!(digest.contains("seq=2"));
        assert!(digest.contains("counts:"));
        assert!(digest.contains("line=1"));
        assert!(digest.contains("fill=1"));
        assert!(digest.contains("scene_bbox="));
        assert!(digest.contains("pen=#"));
        assert!(digest.contains("margin=0.10"));
        assert!(digest.contains("usable=(0.10,0.10)-(0.90,0.90)"));
        assert!(digest.contains("placement="));
    }

    #[test]
    fn normalize_pixel_rect_spreads_on_board() {
        use super::{
            canvas_op_bbox, normalize_canvas_op_coords, CanvasOpBody,
        };
        let mut body = CanvasOpBody::Rect {
            x: 100.0,
            y: 50.0,
            w: 200.0,
            h: 150.0,
            color: "#3ee0c4".into(),
            fill: true,
            width: 0.01,
        };
        assert!(normalize_canvas_op_coords(&mut body));
        let bbox = canvas_op_bbox(&body).expect("bbox");
        assert!(bbox.x1 <= 1.0 && bbox.y1 <= 1.0);
        assert!(bbox.x1 - bbox.x0 > 0.05);
        assert!(bbox.x0 < 0.9 && bbox.y0 < 0.9);
    }

    #[test]
    fn pixel_coords_do_not_pile_at_same_corner() {
        use super::{canvas_bbox_overlaps, canvas_op_bbox, normalize_canvas_op_coords, CanvasOpBody};
        let mut r1 = CanvasOpBody::Rect {
            x: 400.0,
            y: 300.0,
            w: 100.0,
            h: 80.0,
            color: "#3ee0c4".into(),
            fill: true,
            width: 0.01,
        };
        let mut r2 = CanvasOpBody::Rect {
            x: 200.0,
            y: 150.0,
            w: 100.0,
            h: 80.0,
            color: "#ff4400".into(),
            fill: true,
            width: 0.01,
        };
        normalize_canvas_op_coords(&mut r1);
        normalize_canvas_op_coords(&mut r2);
        let b1 = canvas_op_bbox(&r1).unwrap();
        let b2 = canvas_op_bbox(&r2).unwrap();
        assert!(!canvas_bbox_overlaps(b1, b2, 0.02));
    }

    #[test]
    fn normalize_clamps_slight_overflow_without_rescale() {
        use super::{normalize_canvas_op_coords, CanvasOpBody, CanvasPoint};
        let mut body = CanvasOpBody::Line {
            p0: CanvasPoint { x: -0.05, y: 0.2 },
            p1: CanvasPoint { x: 1.2, y: 0.8 },
            color: "#3ee0c4".into(),
            width: 0.01,
        };
        assert!(!normalize_canvas_op_coords(&mut body));
        match body {
            CanvasOpBody::Line { p0, p1, .. } => {
                assert_eq!(p0.x, 0.0);
                assert_eq!(p1.x, 1.0);
            }
            other => panic!("expected line, got {other:?}"),
        }
    }

    #[test]
    fn legacy_meta_without_canvas_open() {
        let m: ChatSessionMeta = serde_json::from_str(
            r#"{
                "id":"sess-1",
                "title":"Test",
                "created_ms":1,
                "updated_ms":2,
                "archived":false,
                "message_count":0
            }"#,
        )
        .unwrap();
        assert!(!m.canvas_open);
    }

    #[test]
    fn legacy_message_without_speaker() {
        let m: ChatSessionMessage =
            serde_json::from_str(r#"{"role":"user","content":"hi","ts_ms":1}"#).unwrap();
        assert!(m.speaker_id.is_none());
        assert!(m.speaker_name.is_none());
        assert!(m.attachments.is_empty());
    }

    #[test]
    fn message_with_speaker_roundtrip() {
        let m = ChatSessionMessage {
            role: "assistant".into(),
            content: "reply".into(),
            ts_ms: 42,
            attachments: vec![],
            speaker_id: Some("agent-a".into()),
            speaker_name: Some("Alpha".into()),
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: ChatSessionMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.speaker_id.as_deref(), Some("agent-a"));
        assert_eq!(back.speaker_name.as_deref(), Some("Alpha"));
    }

    #[test]
    fn agent_info_display_title_prefers_display_name() {
        let info = AgentInfo {
            agent_id: "agent-1".into(),
            state: AgentState::Roster,
            directive: "Propose concrete implementation steps.".into(),
            pid: None,
            caps: vec![],
            last_output: String::new(),
            step: 0,
            max_steps: 0,
            current_task: None,
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: vec![],
            tools: vec![],
            mcp_servers: vec![],
            fail_reason: None,
            session_id: None,
            title: String::new(),
            kind: AgentKind::Roster,
            display_name: Some("Coder".into()),
            persona_id: Some("coder".into()),
            origin: None,
        };
        assert_eq!(info.display_title(), "Coder");
        assert!(info.is_roster());
    }

    #[test]
    fn roster_create_request_does_not_spawn_worker() {
        let mut req = AgentCreateRequest::simple(String::new());
        req.kind = AgentKind::Roster;
        req.display_name = Some("Planner".into());
        assert!(!req.spawns_worker());
        req.kind = AgentKind::Task;
        req.goal = Some(AgentGoal {
            statement: "do work".into(),
            ..Default::default()
        });
        assert!(req.spawns_worker());
    }

    #[test]
    fn typed_display_name_and_ephemeral_origin() {
        let custom = AgentInfo {
            agent_id: "agent-1".into(),
            state: AgentState::Roster,
            directive: String::new(),
            pid: None,
            caps: vec![],
            last_output: String::new(),
            step: 0,
            max_steps: 0,
            current_task: None,
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: vec![],
            tools: vec![],
            mcp_servers: vec![],
            fail_reason: None,
            session_id: None,
            title: String::new(),
            kind: AgentKind::Roster,
            display_name: Some("Skills Auditor".into()),
            persona_id: None,
            origin: Some("library".into()),
        };
        assert!(custom.uses_typed_display_name());
        assert!(!custom.is_ephemeral_chat_spawn());

        let delegate = AgentInfo {
            origin: Some("assistant".into()),
            kind: AgentKind::Task,
            display_name: Some("Summarize".into()),
            persona_id: None,
            ..custom.clone()
        };
        assert!(!delegate.uses_typed_display_name());
        assert!(delegate.is_ephemeral_chat_spawn());
    }
}
