//! Assemblage plateforme : audit + storage + memory + modules + embeddings.
//!
//! `PlatformSubsystem` implémente [`HostServices`] : c'est lui qui exécute
//! les appels système émis par les modules WASM, avec vérification des caps
//! et émission d'audit (chaîne intent → agent → outil → fs, Gate P2).

use crate::audit::AuditJournal;
use crate::chat_session::ChatSessionStore;
use crate::confirm::ConfirmManager;
use crate::memory::MemoryStore;
use crate::module_rt::{read_module_asset_from_dir, HostCallCtx, HostServices, ModuleRuntime};
use crate::net::EgressControl;
use crate::policy::PolicyEngine;
use crate::secrets::SecretStore;
use crate::storage::{glob_match, StorageFs};
use crate::trust::TrustManager;
use aos_proto::AuditAppendRequest;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[cfg(feature = "embeddings")]
use aos_llama::{LlamaContext, LlamaModel, LoadOptions};
#[cfg(feature = "embeddings")]
use std::path::PathBuf;

/// Configuration du daemon plateforme.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PlatformConfig {
    #[serde(default = "default_bus")]
    pub bus: String,
    #[serde(default = "default_audit_dir")]
    pub audit_dir: String,
    #[serde(default = "default_storage_dir")]
    pub storage_dir: String,
    #[serde(default = "default_memory_dir")]
    pub memory_dir: String,
    #[serde(default = "default_modules_dir")]
    pub modules_dir: String,
    /// Catalogue local signé (E10). Défaut `share/modules/catalogue.yaml`.
    #[serde(default = "default_catalogue_file")]
    pub catalogue_file: String,
    #[serde(default = "default_skills_dir")]
    pub skills_dir: String,
    #[serde(default = "default_sessions_dir")]
    pub sessions_dir: String,
    /// Modèle d'embeddings (embedded-embed, §3.4).
    pub embed_model: Option<EmbedModelConfig>,
    /// Fichier de règles du Policy Engine (§9.4).
    #[serde(default)]
    pub policies_file: Option<String>,
    /// Timeout par défaut des confirmations (§9.4, fail-closed).
    #[serde(default = "default_confirm_timeout")]
    pub confirm_timeout_sec: u64,
    /// Fichier des secrets (§9.2).
    #[serde(default = "default_secrets_file")]
    pub secrets_file: String,
    /// Mode réseau au démarrage : online | offline_strict (§9.5).
    #[serde(default = "default_net_mode")]
    pub net_mode: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct EmbedModelConfig {
    pub path: String,
    #[serde(default)]
    pub n_gpu_layers: i32,
    #[serde(default = "default_threads")]
    pub n_threads: i32,
}

fn default_bus() -> String {
    format!("127.0.0.1:{}", aos_ipc::DEFAULT_BUS_PORT)
}
fn default_audit_dir() -> String {
    "var/audit".into()
}
fn default_storage_dir() -> String {
    "var/storage".into()
}
fn default_memory_dir() -> String {
    "var/memory".into()
}
fn default_modules_dir() -> String {
    "var/modules".into()
}
fn default_catalogue_file() -> String {
    "share/modules/catalogue.yaml".into()
}
fn default_skills_dir() -> String {
    "var/skills".into()
}
fn default_sessions_dir() -> String {
    "var/sessions".into()
}
fn default_confirm_timeout() -> u64 {
    120
}
fn default_secrets_file() -> String {
    "var/secrets/keys.yaml".into()
}
fn default_net_mode() -> String {
    "online".into()
}
fn default_threads() -> i32 {
    8
}

impl PlatformConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(serde_yaml::from_str(&std::fs::read_to_string(path)?)?)
    }
}

/// Le sous-système plateforme (partagé entre handlers du bus).
pub struct PlatformSubsystem {
    pub audit: Mutex<AuditJournal>,
    pub fs: Mutex<StorageFs>,
    pub mem: Mutex<MemoryStore>,
    pub sessions: Mutex<ChatSessionStore>,
    pub modules: Mutex<ModuleRuntime>,
    pub skills: Mutex<crate::skill::SkillStore>,
    pub author: Mutex<crate::module_compile::ModuleAuthor>,
    #[cfg(feature = "embeddings")]
    embed: Mutex<Option<Arc<std::sync::Mutex<LlamaContext>>>>,
    pub policy: Mutex<PolicyEngine>,
    pub confirm: Arc<ConfirmManager>,
    pub trust: Mutex<TrustManager>,
    pub net: Mutex<EgressControl>,
    pub secrets: Mutex<SecretStore>,
    /// Caps accordées par `cap.request` (registre logique par agent).
    pub granted_caps: Mutex<std::collections::HashMap<String, Vec<String>>>,
    /// Agent superviseur v1 (§4.6).
    pub supervisor: Arc<crate::supervisor::Supervisor>,
    /// Client bus : forwarding audit → `aos-auditd` et checks → `aos-capkd`.
    bus: Mutex<Option<Arc<aos_ipc::BusClient>>>,
    /// Mutex par session pour sérialiser `canvas.apply` (évite interleaving JSON).
    canvas_apply_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// Sessions où un modèle vision lit le canvas en direct (refcount).
    canvas_seeing: Mutex<HashMap<String, u32>>,
}

impl PlatformSubsystem {
    pub fn open(config: &PlatformConfig) -> Result<Arc<Self>, String> {
        let audit = AuditJournal::open(&config.audit_dir).map_err(|e| e.to_string())?;
        let fs = StorageFs::open(&config.storage_dir).map_err(|e| e.to_string())?;
        let mem = MemoryStore::open(&config.memory_dir).map_err(|e| e.to_string())?;
        let sessions = ChatSessionStore::open(&config.sessions_dir).map_err(|e| e.to_string())?;
        #[cfg(feature = "embeddings")]
        let embed = {
            let embed = config.embed_model.as_ref().map(|cfg| {
                let opts = LoadOptions {
                    n_gpu_layers: cfg.n_gpu_layers,
                    n_threads: cfg.n_threads,
                    embeddings: true,
                    n_ctx: 2048,
                    ..Default::default()
                };
                let model = LlamaModel::load(PathBuf::from(&cfg.path).as_path(), &opts)
                    .map_err(|e| e.to_string())?;
                let ctx = LlamaContext::new(Arc::new(model), &opts).map_err(|e| e.to_string())?;
                Ok::<_, String>(Arc::new(std::sync::Mutex::new(ctx)))
            });
            match embed {
                Some(Ok(c)) => Some(c),
                Some(Err(e)) => return Err(format!("embed model: {e}")),
                None => None,
            }
        };

        // Liaison en deux temps propre : le ModuleRuntime délègue les appels
        // système via `LateBoundServices`, résolu une fois le sous-système créé.
        let late = Arc::new(LateBoundServices::default());
        let mut rt =
            ModuleRuntime::open(&config.modules_dir, late.clone()).map_err(|e| e.to_string())?;
        if Path::new(&config.catalogue_file).is_file() {
            match crate::catalogue::SignedCatalogue::load(&config.catalogue_file) {
                Ok(cat) => rt.set_catalogue(cat),
                Err(e) => eprintln!("[aos-platform] catalogue: {e}"),
            }
        }
        let skills = crate::skill::SkillStore::open(&config.skills_dir).map_err(|e| e.to_string())?;
        let author =
            crate::module_compile::ModuleAuthor::open(&config.modules_dir).map_err(|e| e.to_string())?;
        let policy = PolicyEngine::open(
            config.policies_file.as_deref().map(Path::new),
            config.confirm_timeout_sec,
        )
        .map_err(|e| e.to_string())?;
        let secrets = SecretStore::open(&config.secrets_file).map_err(|e| e.to_string())?;
        let secrets_backend = secrets.master_backend().as_str().to_string();
        let mut net = EgressControl::new();
        if config.net_mode == "offline_strict" {
            net.set_mode(crate::net::NetMode::OfflineStrict);
        }
        // Caps réseau de base pour la recherche (activées seulement si online).
        net.grant("net.connect:html.duckduckgo.com:443".into());
        net.grant("net.connect:api.search.brave.com:443".into());
        net.grant("net.connect:www.bing.com:443".into());
        let sub = Arc::new(Self {
            audit: Mutex::new(audit),
            fs: Mutex::new(fs),
            mem: Mutex::new(mem),
            sessions: Mutex::new(sessions),
            modules: Mutex::new(rt),
            skills: Mutex::new(skills),
            author: Mutex::new(author),
            #[cfg(feature = "embeddings")]
            embed: Mutex::new(embed),
            policy: Mutex::new(policy),
            confirm: ConfirmManager::new(config.confirm_timeout_sec),
            trust: Mutex::new(TrustManager::new()),
            net: Mutex::new(net),
            secrets: Mutex::new(secrets),
            granted_caps: Mutex::new(std::collections::HashMap::new()),
            supervisor: crate::supervisor::Supervisor::new(),
            bus: Mutex::new(None),
            canvas_apply_locks: Mutex::new(HashMap::new()),
            canvas_seeing: Mutex::new(HashMap::new()),
        });
        let _ = late.0.set(sub.clone());
        sub.audit(AuditAppendRequest {
            trace_id: "boot".into(),
            actor: "service:platformd".into(),
            action: "secrets.backend".into(),
            target: secrets_backend,
            detail: serde_json::json!({}),
        });
        Ok(sub)
    }

    /// Client bus pour `aos-auditd` (P4.4) et `aos-capkd` (P4.2).
    pub fn set_bus(&self, bus: Arc<aos_ipc::BusClient>) {
        *self.bus.lock().unwrap() = Some(bus);
    }

    pub fn bus(&self) -> Option<Arc<aos_ipc::BusClient>> {
        self.bus.lock().unwrap().clone()
    }

    /// Verrou d'application canvas par session (sérialise les écritures concurrentes).
    pub fn canvas_apply_lock(&self, session_id: &str) -> Arc<Mutex<()>> {
        let mut locks = self.canvas_apply_locks.lock().unwrap();
        locks
            .entry(session_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Incrémente le compteur « vision lit le canvas » pour une session.
    pub fn canvas_seeing_set(&self, session_id: &str, active: bool) {
        let mut map = self.canvas_seeing.lock().unwrap();
        if active {
            *map.entry(session_id.to_string()).or_insert(0) += 1;
        } else {
            if let Some(n) = map.get_mut(session_id) {
                *n = n.saturating_sub(1);
                if *n == 0 {
                    map.remove(session_id);
                }
            }
        }
    }

    /// True si au moins un lecteur vision actif sur cette session.
    pub fn canvas_seeing_active(&self, session_id: &str) -> bool {
        self.canvas_seeing
            .lock()
            .unwrap()
            .get(session_id)
            .copied()
            .unwrap_or(0)
            > 0
    }

    /// Autorise via le noyau de capacités si l'enveloppe porte des
    /// `cap://kernel/<id>`.
    ///
    /// - `None` : aucune cap kernel → fallback caps logiques P1-P3 ;
    /// - `Some(Ok(()))` : au moins une cap kernel autorise l'objet ;
    /// - `Some(Err(_))` : caps kernel présentées mais refusées (fail-closed,
    ///   y compris si `aos-capkd` est injoignable).
    pub async fn authorize_kernel(
        &self,
        envelope_caps: &[String],
        holder: &str,
        object: &str,
        rights: &[String],
    ) -> Option<Result<(), String>> {
        let ids: Vec<u64> = envelope_caps
            .iter()
            .filter_map(|c| aos_ipc::parse_kernel_cap(c))
            .collect();
        if ids.is_empty() {
            return None;
        }
        let Some(bus) = self.bus() else {
            return Some(Err("noyau de capacités injoignable".into()));
        };
        for cap in ids {
            let r = bus
                .call::<aos_proto::CapCheckRequest, aos_proto::CapCheckResponse>(
                    "cap.check",
                    &aos_proto::CapCheckRequest {
                        holder: holder.into(),
                        cap,
                        rights: rights.to_vec(),
                        object: Some(object.into()),
                    },
                    vec![],
                )
                .await;
            match r {
                Ok(resp) if resp.allowed => return Some(Ok(())),
                Ok(_) => {}
                Err(e) => return Some(Err(e.to_string())),
            }
        }
        Some(Err("capacité kernel refusée ou révoquée".into()))
    }

    /// Append d'audit (point unique) + alimentation du superviseur.
    ///
    /// P4.4 : l'événement est aussi forwardé à `aos-auditd` (journal canonique)
    /// si un client bus est configuré. Fire-and-forget : la panne d'auditd
    /// n'affecte pas le service (isolation de panne, Gate P4).
    pub fn audit(&self, req: AuditAppendRequest) {
        let ev = self.audit.lock().unwrap().append(req.clone());
        let sup = self.supervisor.clone();
        tokio::spawn(async move {
            sup.feed(&ev.actor, &ev.action, &ev.target).await;
        });
        // Forwarding au service d'audit autonome (tolérant à la panne).
        let bus = self.bus();
        if let Some(bus) = bus {
            tokio::spawn(async move {
                let _ = bus
                    .call::<AuditAppendRequest, u64>("audit.append", &req, vec![])
                    .await;
            });
        }
    }

    /// Embedding d'un texte (service interne, bloquant).
    pub fn embed_text(&self, text: &str) -> Result<Vec<f32>, String> {
        #[cfg(feature = "embeddings")]
        {
            let ctx = {
                let guard = self.embed.lock().unwrap();
                guard.as_ref().cloned()
            };
            let ctx = ctx.ok_or_else(|| "modèle d'embeddings non configuré".to_string())?;
            return ctx.lock().unwrap().embed(text).map_err(|e| e.to_string());
        }
        #[cfg(not(feature = "embeddings"))]
        {
            let _ = text;
            Err("embeddings désactivés dans ce binaire".into())
        }
    }

    /// Évalue la politique pour une action ; gère `require_confirmation`
    /// (bloquant, fail-closed : timeout → refus audité, §9.4).
    /// Retourne `true` si l'action peut procéder.
    pub async fn policy_gate(
        &self,
        mut context: std::collections::HashMap<String, String>,
        actor: &str,
        action: &str,
        target: &str,
        trace_id: &str,
    ) -> bool {
        context
            .entry("action.kind".into())
            .or_insert_with(|| action.into());
        let (effect, rule_name, timeout) = {
            let p = self.policy.lock().unwrap();
            let (e, r) = p.evaluate(&context);
            (e, r.map(|r| r.name.clone()), r.and_then(|r| r.timeout_sec))
        };
        match effect {
            aos_proto::PolicyEffect::Allow => true,
            aos_proto::PolicyEffect::Deny => {
                self.audit(AuditAppendRequest {
                    trace_id: trace_id.into(),
                    actor: actor.into(),
                    action: "policy.deny".into(),
                    target: target.into(),
                    detail: serde_json::json!({"rule": rule_name, "action": action}),
                });
                false
            }
            aos_proto::PolicyEffect::RequireConfirmation => {
                let (id, rx) = self
                    .confirm
                    .ask(
                        actor.into(),
                        action.into(),
                        target.into(),
                        rule_name.unwrap_or_else(|| "require_confirmation".into()),
                        timeout,
                    )
                    .await;
                let approved = rx.await.unwrap_or(false);
                self.audit(AuditAppendRequest {
                    trace_id: trace_id.into(),
                    actor: actor.into(),
                    action: "confirmation.resolved".into(),
                    target: target.into(),
                    detail: serde_json::json!({"confirmation_id": id, "approved": approved}),
                });
                if !approved {
                    self.trust.lock().unwrap().record_confirmation_denial(actor);
                }
                approved
            }
        }
    }

    /// Demande de capacité par un agent (§4.7 paliers, Trust consultatif).
    pub fn decide_cap_request(&self, agent_id: &str, cap: &str) -> CapDecision {
        const CRITICAL: &[&str] = &[
            "fs.reclassify",
            "net.connect",
            "secrets",
            "module.install",
            "module.compile",
            "module.uninstall",
            "media.generate",
        ];
        let tier = self.trust.lock().unwrap().tier(agent_id);
        let critical = CRITICAL.iter().any(|c| cap.starts_with(c));
        match (tier, critical) {
            (crate::trust::Tier::High, false) => CapDecision::Grant,
            (crate::trust::Tier::High, true) => CapDecision::Confirm,
            (crate::trust::Tier::Medium, _) => CapDecision::Confirm,
            (crate::trust::Tier::Low, _) => CapDecision::Deny,
        }
    }

    /// Enregistre une cap accordée (registre logique).
    pub fn grant_cap(&self, agent_id: &str, cap: &str) {
        self.granted_caps
            .lock()
            .unwrap()
            .entry(agent_id.into())
            .or_default()
            .push(cap.into());
    }

    /// Vérifie une cap `kind:resource` dans les caps du module.
    fn require_cap(ctx: &HostCallCtx, kind: &str, resource: &str) -> Result<(), String> {
        for cap in &ctx.granted_caps {
            if let Some(pattern) = cap.strip_prefix(&format!("{kind}:")) {
                if glob_match(pattern, resource) || pattern == resource {
                    return Ok(());
                }
            }
        }
        Err(format!(
            "permission refusée: {kind}:{resource} (module {})",
            ctx.module
        ))
    }
}

/// Issue d'une demande de capacité.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapDecision {
    Grant,
    Confirm,
    Deny,
}

/// Délégation des appels système des modules vers le sous-système, résolue
/// après construction (le ModuleRuntime a besoin d'un `Arc<dyn HostServices>`
/// avant que le sous-système existe).
#[derive(Default)]
struct LateBoundServices(std::sync::OnceLock<Arc<PlatformSubsystem>>);

impl HostServices for LateBoundServices {
    fn call(
        &self,
        ctx: &HostCallCtx,
        service: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.0
            .get()
            .ok_or_else(|| "services non initialisés".to_string())?
            .call(ctx, service, args)
    }
}

impl HostServices for PlatformSubsystem {
    fn call(
        &self,
        ctx: &HostCallCtx,
        service: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        match service {
            "fs.read" => {
                let path = args["path"].as_str().unwrap_or("");
                Self::require_cap(ctx, "fs.read", path)?;
                let (content, class, version) = self
                    .fs
                    .lock()
                    .unwrap()
                    .read(path, &ctx.granted_caps)
                    .map_err(|e| e.to_string())?;
                self.audit(AuditAppendRequest {
                    trace_id: ctx.trace_id.clone(),
                    actor: format!("module:{}", ctx.module),
                    action: "fs.read".into(),
                    target: path.into(),
                    detail: serde_json::json!({"on_behalf_of": ctx.actor}),
                });
                Ok(serde_json::json!({"content": content, "class": class, "version": version}))
            }
            "fs.write" => {
                let path = args["path"].as_str().unwrap_or("");
                let content = args["content"].as_str().unwrap_or("");
                Self::require_cap(ctx, "fs.write", path)?;
                let version = self
                    .fs
                    .lock()
                    .unwrap()
                    .write(
                        path,
                        content,
                        &format!("module:{}", ctx.module),
                        &ctx.granted_caps,
                    )
                    .map_err(|e| e.to_string())?;
                self.audit(AuditAppendRequest {
                    trace_id: ctx.trace_id.clone(),
                    actor: format!("module:{}", ctx.module),
                    action: "fs.write".into(),
                    target: path.into(),
                    detail: serde_json::json!({"on_behalf_of": ctx.actor, "version": version}),
                });
                Ok(serde_json::json!({"version": version}))
            }
            "fs.list" => {
                let prefix = args["prefix"].as_str().unwrap_or("/");
                let entries = self.fs.lock().unwrap().list(prefix, &ctx.granted_caps);
                Ok(serde_json::json!({"entries": entries}))
            }
            "mem.episodic_write" => {
                let ns = args["namespace"].as_str().unwrap_or("");
                let text = args["text"].as_str().unwrap_or("");
                Self::require_cap(ctx, "mem.write", ns)?;
                let vector = self.embed_text(text)?;
                let id = self.mem.lock().unwrap().episodic_write(
                    ns,
                    text,
                    args["metadata"].clone(),
                    vector,
                    false,
                );
                self.audit(AuditAppendRequest {
                    trace_id: ctx.trace_id.clone(),
                    actor: format!("module:{}", ctx.module),
                    action: "mem.episodic_write".into(),
                    target: ns.into(),
                    detail: serde_json::json!({"id": id, "on_behalf_of": ctx.actor}),
                });
                Ok(serde_json::json!({"id": id}))
            }
            "mem.episodic_query" => {
                let ns = args["namespace"].as_str().unwrap_or("");
                let query = args["query"].as_str().unwrap_or("");
                let k = args["k"].as_u64().unwrap_or(5) as usize;
                Self::require_cap(ctx, "mem.query", ns)?;
                let vector = self.embed_text(query)?;
                let hits = self.mem.lock().unwrap().episodic_query(
                    &vector,
                    k,
                    if ns.is_empty() { None } else { Some(ns) },
                );
                Ok(serde_json::json!({"hits": hits}))
            }
            "mem.episodic_delete" => {
                let ns = args["namespace"].as_str().unwrap_or("");
                if ns.is_empty() {
                    return Err("mem.episodic_delete: namespace requis".into());
                }
                Self::require_cap(ctx, "mem.write", ns)?;
                let mut mem = self.mem.lock().unwrap();
                let (deleted, count) = if let Some(id) = args["id"].as_u64() {
                    // Vérifie que l'entrée appartient au namespace autorisé.
                    let ok = mem
                        .export(ns)
                        .iter()
                        .any(|e| e.id == id)
                        && mem.episodic_delete(id);
                    (ok, if ok { 1 } else { 0 })
                } else {
                    let key = args["meta_key"].as_str().unwrap_or("path");
                    let value = args["meta_value"]
                        .as_str()
                        .or_else(|| args["path"].as_str())
                        .unwrap_or("");
                    if value.is_empty() {
                        return Err(
                            "mem.episodic_delete: id ou path/meta_value requis".into(),
                        );
                    }
                    let n = mem.episodic_delete_by_meta(ns, key, value);
                    (n > 0, n)
                };
                drop(mem);
                self.audit(AuditAppendRequest {
                    trace_id: ctx.trace_id.clone(),
                    actor: format!("module:{}", ctx.module),
                    action: "mem.episodic_delete".into(),
                    target: ns.into(),
                    detail: serde_json::json!({"deleted": deleted, "count": count, "on_behalf_of": ctx.actor}),
                });
                Ok(serde_json::json!({"deleted": deleted, "count": count}))
            }
            "mem.shared_read" => {
                let name = args["name"].as_str().unwrap_or("");
                Self::require_cap(ctx, "mem.query", &format!("shared:{name}"))
                    .or_else(|_| Self::require_cap(ctx, "mem.query", "shared:*"))
                    .or_else(|_| Self::require_cap(ctx, "mem.query", "*"))?;
                let v = self.mem.lock().unwrap().shared_read(name);
                Ok(serde_json::json!({"value": v}))
            }
            "mem.shared_write" => {
                let name = args["name"].as_str().unwrap_or("");
                Self::require_cap(ctx, "mem.write", &format!("shared:{name}"))
                    .or_else(|_| Self::require_cap(ctx, "mem.write", "shared:*"))
                    .or_else(|_| Self::require_cap(ctx, "mem.write", "*"))?;
                self.mem
                    .lock()
                    .unwrap()
                    .shared_write(name, args["value"].clone());
                Ok(serde_json::json!({"ok": true}))
            }
            "mem.user.remember" | "mem.user_remember" => {
                let text = args["text"].as_str().unwrap_or("");
                Self::require_cap(ctx, "mem.write", "user:default")?;
                let emb = self.embed_text(text).unwrap_or_default();
                let id = self.mem.lock().unwrap().episodic_write(
                    "user:default",
                    text,
                    args.get("metadata").cloned().unwrap_or(serde_json::json!({})),
                    emb,
                    args["pinned"].as_bool().unwrap_or(false),
                );
                Ok(serde_json::json!({"id": id}))
            }
            "mem.user.recall" | "mem.user_recall" => {
                let query = args["query"].as_str().unwrap_or("");
                let k = args["k"].as_u64().unwrap_or(5) as usize;
                Self::require_cap(ctx, "mem.query", "user:default")?;
                let emb = self.embed_text(query).unwrap_or_default();
                let hits = self
                    .mem
                    .lock()
                    .unwrap()
                    .episodic_query(&emb, k, Some("user:default"));
                Ok(serde_json::json!({"hits": hits}))
            }
            "mem.context" => {
                let query = args["query"].as_str().unwrap_or("");
                let k = args["k"].as_u64().unwrap_or(5) as usize;
                let product_k = args["product_k"].as_u64().unwrap_or(4) as usize;
                let user_doc_k = args["user_doc_k"].as_u64().unwrap_or(3) as usize;
                let emb = self.embed_text(query).unwrap_or_default();
                let (hits, product_hits, user_doc_hits) = {
                    let mem = self.mem.lock().unwrap();
                    let hits = mem.episodic_query(&emb, k, None);
                    let product_hits = crate::product_rag::recall(&mem, &emb, product_k);
                    let user_doc_hits = crate::user_docs::recall(&mem, &emb, user_doc_k);
                    (hits, product_hits, user_doc_hits)
                };
                let mut prompt_block = crate::product_rag::format_prompt_block(&product_hits);
                let user_doc_block = crate::user_docs::format_prompt_block(&user_doc_hits);
                if !user_doc_block.is_empty() {
                    if !prompt_block.is_empty() {
                        prompt_block.push('\n');
                    }
                    prompt_block.push_str(&user_doc_block);
                }
                if prompt_block.is_empty() {
                    prompt_block.push_str("Contexte mémoire:\n");
                } else {
                    prompt_block.push_str("Contexte mémoire:\n");
                }
                for h in &hits {
                    prompt_block.push_str(&format!("- {}\n", h.text));
                }
                Ok(serde_json::json!({
                    "prompt_block": prompt_block,
                    "hits": hits,
                    "product_hits": product_hits,
                    "user_doc_hits": user_doc_hits,
                }))
            }
            "web.search" => {
                // Cap réseau requise
                let has_net = ctx.granted_caps.iter().any(|c| c.starts_with("net.connect:"));
                if !has_net {
                    return Err("permission refusée: net.connect requis pour web.search".into());
                }
                let query = args["query"].as_str().unwrap_or("");
                let max = args["max_results"].as_u64().unwrap_or(5) as usize;
                let brave = self
                    .secrets
                    .lock()
                    .unwrap()
                    .get("brave_search_api_key", "service:platformd")
                    .ok();
                let mut net = self.net.lock().unwrap();
                let engine = args["engine"].as_str().unwrap_or("auto");
                let resp = crate::net_services::web_search(
                    &mut net,
                    &format!("module:{}", ctx.module),
                    &ctx.granted_caps,
                    query,
                    max,
                    brave.as_deref(),
                    engine,
                )
                .map_err(|e| e.to_string())?;
                self.audit(AuditAppendRequest {
                    trace_id: ctx.trace_id.clone(),
                    actor: format!("module:{}", ctx.module),
                    action: "web.search".into(),
                    target: query.into(),
                    detail: serde_json::json!({"on_behalf_of": ctx.actor}),
                });
                Ok(serde_json::to_value(resp).unwrap_or_default())
            }
            "web.browse" => {
                let has_net = ctx.granted_caps.iter().any(|c| c.starts_with("net.connect:"));
                if !has_net {
                    return Err("permission refusée: net.connect requis pour web.browse".into());
                }
                let url = args["url"].as_str().unwrap_or("");
                let max_chars = args["max_chars"].as_u64().unwrap_or(12_000) as usize;
                let mut net = self.net.lock().unwrap();
                let resp = crate::net_services::web_browse(
                    &mut net,
                    &format!("module:{}", ctx.module),
                    &ctx.granted_caps,
                    url,
                    max_chars,
                )
                .map_err(|e| e.to_string())?;
                self.audit(AuditAppendRequest {
                    trace_id: ctx.trace_id.clone(),
                    actor: format!("module:{}", ctx.module),
                    action: "web.browse".into(),
                    target: url.into(),
                    detail: serde_json::json!({"on_behalf_of": ctx.actor}),
                });
                Ok(serde_json::to_value(resp).unwrap_or_default())
            }
            "net.fetch" => {
                let has_net = ctx.granted_caps.iter().any(|c| c.starts_with("net.connect:"));
                if !has_net {
                    return Err("permission refusée: net.connect requis pour net.fetch".into());
                }
                let url = args["url"].as_str().unwrap_or("");
                let dest = args["dest_path"].as_str().unwrap_or("");
                if dest.is_empty() {
                    return Err("dest_path requis".into());
                }
                Self::require_cap(ctx, "fs.write", dest)?;
                let mut net = self.net.lock().unwrap();
                let (bytes, _ctype) = crate::net_services::http_fetch_bytes(
                    &mut net,
                    &format!("module:{}", ctx.module),
                    &ctx.granted_caps,
                    url,
                    50 * 1024 * 1024,
                )
                .map_err(|e| e.to_string())?;
                drop(net);
                let version = self
                    .fs
                    .lock()
                    .unwrap()
                    .write_bytes(
                        dest,
                        &bytes,
                        &format!("module:{}", ctx.module),
                        &ctx.granted_caps,
                    )
                    .map_err(|e| e.to_string())?;
                Ok(serde_json::json!({"bytes": bytes.len(), "version": version, "path": dest}))
            }
            "files.generate" => {
                let path = args["path"].as_str().unwrap_or("");
                let format = args["format"].as_str().unwrap_or("md");
                let content = args["content"].as_str().unwrap_or("");
                Self::require_cap(ctx, "fs.write", path)?;
                let bytes = crate::files_gen::generate(
                    format,
                    content,
                    args["title"].as_str(),
                )
                .map_err(|e| e.to_string())?;
                let text = if format == "png" || format == "pdf" {
                    // Write via write_bytes if available; fallback base64 note
                    String::from_utf8_lossy(&bytes).to_string()
                } else {
                    String::from_utf8_lossy(&bytes).to_string()
                };
                let version = self
                    .fs
                    .lock()
                    .unwrap()
                    .write(
                        path,
                        &text,
                        &format!("module:{}", ctx.module),
                        &ctx.granted_caps,
                    )
                    .map_err(|e| e.to_string())?;
                self.audit(AuditAppendRequest {
                    trace_id: ctx.trace_id.clone(),
                    actor: format!("module:{}", ctx.module),
                    action: "files.generate".into(),
                    target: path.into(),
                    detail: serde_json::json!({"format": format, "on_behalf_of": ctx.actor}),
                });
                Ok(serde_json::json!({"version": version, "path": path}))
            }
            "ext.asset_read" | "ext.load_handlers" => {
                let rel = args["path"]
                    .as_str()
                    .or_else(|| args["rel"].as_str())
                    .unwrap_or("handlers.yaml");
                // Ne pas re-lock `modules` ici : `module.invoke` tient déjà le mutex.
                let bytes =
                    read_module_asset_from_dir(&ctx.module_dir, rel).map_err(|e| e.to_string())?;
                let text = String::from_utf8_lossy(&bytes).to_string();
                Ok(serde_json::json!({"content": text}))
            }
            "canvas.get" => {
                if ctx.module != "canvas" {
                    return Err("canvas.get réservé au module canvas".into());
                }
                let session_id = args["session_id"].as_str().unwrap_or("").to_string();
                if session_id.is_empty() {
                    return Err("session_id requis".into());
                }
                let after_seq = args["after_seq"].as_u64();
                let (meta, doc, ops) = self
                    .sessions
                    .lock()
                    .unwrap()
                    .canvas_get(&session_id, after_seq)
                    .map_err(|e| e.to_string())?;
                Ok(serde_json::json!({
                    "session_id": meta.id,
                    "canvas_open": meta.canvas_open,
                    "canvas_aspect": meta.canvas_aspect,
                    "next_seq": doc.next_seq,
                    "ops": ops,
                    "pen": doc.pen,
                }))
            }
            "canvas.set_style" => {
                if ctx.module != "canvas" {
                    return Err("canvas.set_style réservé au module canvas".into());
                }
                let session_id = args["session_id"].as_str().unwrap_or("").to_string();
                if session_id.is_empty() {
                    return Err("session_id requis".into());
                }
                let color = args.get("color").and_then(|v| v.as_str());
                let width = args.get("width").and_then(|v| v.as_f64()).map(|w| w as f32);
                let apply_lock = self.canvas_apply_lock(&session_id);
                let _guard = apply_lock.lock().unwrap();
                let (meta, doc) = self
                    .sessions
                    .lock()
                    .unwrap()
                    .canvas_set_style(&session_id, color, width)
                    .map_err(|e| e.to_string())?;
                Ok(serde_json::json!({
                    "canvas_open": meta.canvas_open,
                    "pen": doc.pen,
                    "next_seq": doc.next_seq,
                }))
            }
            "canvas.apply" => {
                if ctx.module != "canvas" {
                    return Err("canvas.apply réservé au module canvas".into());
                }
                let session_id = args["session_id"].as_str().unwrap_or("").to_string();
                if session_id.is_empty() {
                    return Err("session_id requis".into());
                }
                let author_id = args["author_id"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        ctx.actor
                            .strip_prefix("agent:")
                            .unwrap_or(ctx.actor.as_str())
                            .to_string()
                    });
                let op_val = if args.get("op").is_some() {
                    args["op"].clone()
                } else {
                    args.clone()
                };
                let body: aos_proto::CanvasOpBody =
                    serde_json::from_value(op_val).map_err(|e| format!("op invalide: {e}"))?;
                let apply_lock = self.canvas_apply_lock(&session_id);
                let _guard = apply_lock.lock().unwrap();
                let (meta, doc, applied) = self
                    .sessions
                    .lock()
                    .unwrap()
                    .canvas_apply(&session_id, &author_id, body)
                    .map_err(|e| e.to_string())?;
                Ok(serde_json::json!({
                    "canvas_open": meta.canvas_open,
                    "canvas_aspect": meta.canvas_aspect,
                    "next_seq": doc.next_seq,
                    "ops_len": doc.ops.len(),
                    "applied": applied,
                }))
            }
            "canvas.export" => {
                if ctx.module != "canvas" {
                    return Err("canvas.export réservé au module canvas".into());
                }
                let session_id = args["session_id"].as_str().unwrap_or("").to_string();
                if session_id.is_empty() {
                    return Err("session_id requis".into());
                }
                let width = args["width"].as_u64().unwrap_or(768) as u32;
                let height = args["height"].as_u64().unwrap_or(512) as u32;
                let (_, doc, _) = self
                    .sessions
                    .lock()
                    .unwrap()
                    .canvas_get(&session_id, None)
                    .map_err(|e| e.to_string())?;
                let bytes = crate::canvas_raster::export_png(&doc, width, height)?;
                let path = args["path"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        format!(
                            "/downloads/canvas-{}-{}.png",
                            session_id,
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_millis())
                                .unwrap_or(0)
                        )
                    });
                Self::require_cap(ctx, "fs.write", &path)?;
                let version = self
                    .fs
                    .lock()
                    .unwrap()
                    .write_bytes(
                        &path,
                        &bytes,
                        &format!("module:{}", ctx.module),
                        &ctx.granted_caps,
                    )
                    .map_err(|e| e.to_string())?;
                Ok(serde_json::json!({
                    "path": path,
                    "bytes": bytes.len(),
                    "version": version,
                }))
            }
            // Escalade interdite depuis WASM
            "module.install"
            | "module.compile"
            | "module.scaffold"
            | "module.package"
            | "secrets.get"
            | "trust.set"
            | "agent.create"
            | "agent.grant" => Err(format!(
                "service interdit depuis host_call WASM: {service}"
            )),
            other => Err(format!("service inconnu: {other}")),
        }
    }
}

#[cfg(test)]
mod canvas_seeing_tests {
    use super::PlatformSubsystem;
    use crate::subsystem::PlatformConfig;
    use std::path::PathBuf;

    fn temp_path(label: &str) -> String {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "aos-canvas-seeing-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&p);
        p.display().to_string()
    }

    fn test_config() -> PlatformConfig {
        PlatformConfig {
            bus: "ipc://test".into(),
            audit_dir: temp_path("audit"),
            storage_dir: temp_path("storage"),
            memory_dir: temp_path("memory"),
            modules_dir: temp_path("modules"),
            catalogue_file: "/dev/null".into(),
            skills_dir: temp_path("skills"),
            sessions_dir: temp_path("sessions"),
            embed_model: None,
            policies_file: None,
            confirm_timeout_sec: 60,
            secrets_file: PathBuf::from(temp_path("secrets"))
                .join("secrets")
                .display()
                .to_string(),
            net_mode: "online".into(),
        }
    }

    #[tokio::test]
    async fn canvas_seeing_refcount_tracks_active_readers() {
        let sub = PlatformSubsystem::open(&test_config()).expect("platform open");
        let sid = "sess-1";
        assert!(!sub.canvas_seeing_active(sid));
        sub.canvas_seeing_set(sid, true);
        assert!(sub.canvas_seeing_active(sid));
        sub.canvas_seeing_set(sid, true);
        sub.canvas_seeing_set(sid, false);
        assert!(sub.canvas_seeing_active(sid));
        sub.canvas_seeing_set(sid, false);
        assert!(!sub.canvas_seeing_active(sid));
    }
}
