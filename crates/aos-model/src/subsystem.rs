//! Cœur du Model Subsystem : état, scheduler, placement réel.

use crate::config::ModeldConfig;
use aos_llama::{GenParams, LlamaContext, LlamaModel, LoadMode, LoadOptions};
use aos_placement::{
    Budgets, CostModel, HardwareProfile, ModelDesc, PlacementManager, PlacementPlan,
    PlacementProfile, Tier,
};
use aos_proto::{
    InferRequest, LoadResponse, ModelInfo, ModelMetrics, ModelState, SystemMetrics, TokenEvent,
};
use aos_registry::ModelRegistry;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::{mpsc, oneshot};

/// Job en file, ordonné par (priorité décroissante, ancienneté).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueuedJob {
    priority: u8,
    id: u64,
}

impl Ord for QueuedJob {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // BinaryHeap = max-heap : priorité forte d'abord, puis plus ancien.
        self.priority
            .cmp(&other.priority)
            .then(other.id.cmp(&self.id))
    }
}
impl PartialOrd for QueuedJob {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// État runtime d'un modèle.
pub struct ModelRuntime {
    pub desc: ModelDesc,
    pub path: Option<PathBuf>,
    pub state: ModelState,
    pub plan: Option<PlacementPlan>,
    pub profile: PlacementProfile,
    pub model: Option<Arc<LlamaModel>>,
    pub ctx: Option<Arc<StdMutex<LlamaContext>>>,
    pub ctx_abort: Option<Arc<AtomicBool>>,
    pub running_job: Option<u64>,
    pub queue: std::collections::BinaryHeap<QueuedJob>,
    pub loading: bool,
    pub load_error: Option<String>,
    pub last_ttft_ms: Option<f64>,
    pub last_tok_s: Option<f64>,
    pub est_tok_s: Option<f64>,
}

impl ModelRuntime {
    fn new(desc: ModelDesc, path: Option<PathBuf>) -> Self {
        Self {
            desc,
            path,
            state: ModelState::OnDisk,
            plan: None,
            profile: PlacementProfile::Balanced,
            model: None,
            ctx: None,
            ctx_abort: None,
            running_job: None,
            queue: std::collections::BinaryHeap::new(),
            loading: false,
            load_error: None,
            last_ttft_ms: None,
            last_tok_s: None,
            est_tok_s: None,
        }
    }
}

/// Résultat d'une inférence côté scheduler.
#[derive(Debug)]
pub enum InferOutcome {
    Done {
        prompt_tokens: u32,
        generated_tokens: u32,
        ttft_ms: f64,
        tok_s: f64,
    },
    Cancelled,
    Failed(String),
}

struct Inner {
    models: HashMap<String, ModelRuntime>,
    /// inference_id → flag d'annulation (jobs en file).
    job_aborts: HashMap<u64, Arc<AtomicBool>>,
    next_inference: u64,
}

enum LoadAction {
    Ready,
    Wait,
    Start,
    Fail(String),
}

/// Résultat matérialisé d'un chargement de modèle.
type LoadArtifacts = (
    PlacementPlan,
    Arc<LlamaModel>,
    Arc<StdMutex<LlamaContext>>,
    Arc<AtomicBool>,
    f64,
);

/// Model Subsystem (partagé entre handlers du bus).
pub struct ModelSubsystem {
    inner: Arc<StdMutex<Inner>>,
    pub config: ModeldConfig,
    pm: PlacementManager,
}

impl ModelSubsystem {
    pub fn new(config: ModeldConfig, registry: &ModelRegistry, ram_total: u64) -> Self {
        let gpu = config.gpu && aos_llama::LlamaBackend::supports_gpu_offload();
        let hw = HardwareProfile {
            name: "host-p1".into(),
            has_gpu: gpu,
            vram_total: if gpu { config.vram_total_bytes } else { 0 },
            ram_total,
            disk_total: 1 << 40,
            os_reserve_vram: config.os_reserve_vram_bytes,
            os_reserve_ram: config.os_reserve_ram_bytes,
            // Mesures hôte (ADR 0002/0005) ; 4080S ≈ 736 GB/s théoriques.
            gpu_mem_bw: 736e9,
            ram_mem_bw: 45.24e9,
            disk_seq_bw: 6e9,
            host_to_device_bw: 25e9,
            gpu_flops: 30e12,
            cpu_flops: 2.5e12,
        };
        let pm = PlacementManager::new(hw, CostModel::default());
        let mut models = HashMap::new();
        for entry in registry.entries() {
            if let Some(mut desc) = entry.to_model_desc() {
                let ov = config.models.get(&entry.id);
                if let Some(ov) = ov {
                    if let Some(v) = ov.n_layers {
                        desc.n_layers = v;
                    }
                    if let Some(v) = ov.weights_bytes {
                        desc.weights_bytes = v;
                    }
                    if let Some(v) = ov.embed_bytes {
                        desc.embed_bytes = v;
                    }
                    if let Some(v) = ov.kv_bytes_per_token {
                        desc.kv_bytes_per_token = v;
                    }
                    if let Some(v) = ov.n_params {
                        desc.n_params = v;
                    }
                }
                let path = ov.map(|o| PathBuf::from(&o.path));
                models.insert(entry.id.clone(), ModelRuntime::new(desc, path));
            } else {
                // Modèle distant : entrée registry sans placement local.
                let desc = ModelDesc {
                    id: entry.id.clone(),
                    name: entry.name.clone(),
                    n_layers: 0,
                    n_params: 0.0,
                    weights_bytes: 0,
                    embed_bytes: 0,
                    kv_bytes_per_token: 0,
                    context_length: 0,
                    supports_layer_offload: false,
                    privacy_class: aos_placement::PrivacyClass::Remote,
                };
                let mut rt = ModelRuntime::new(desc, None);
                rt.state = ModelState::Remote;
                models.insert(entry.id.clone(), rt);
            }
        }
        Self {
            inner: Arc::new(StdMutex::new(Inner {
                models,
                job_aborts: HashMap::new(),
                next_inference: 1,
            })),
            config,
            pm,
        }
    }

    fn info_of(rt: &ModelRuntime) -> ModelInfo {
        ModelInfo {
            id: rt.desc.id.clone(),
            name: rt.desc.name.clone(),
            privacy_class: format!("{:?}", rt.desc.privacy_class).to_lowercase(),
            state: rt.state.clone(),
            placement: rt.plan.as_ref().map(|p| p.summary()),
            profile: Some(format!("{:?}", rt.profile).to_lowercase()),
        }
    }

    pub fn list_models(&self) -> Vec<ModelInfo> {
        let inner = self.inner.lock().unwrap();
        inner.models.values().map(Self::info_of).collect()
    }

    pub fn inspect(&self, model_id: &str) -> Option<ModelInfo> {
        let inner = self.inner.lock().unwrap();
        inner.models.get(model_id).map(Self::info_of)
    }

    /// Charge un modèle : calcule le plan réel (P1.2) puis pilote llama.cpp.
    /// Attend (asynchrone) la fin du chargement ou l'erreur.
    pub async fn ensure_loaded(
        &self,
        model_id: &str,
        profile: PlacementProfile,
        kv_tokens: u32,
    ) -> Result<LoadResponse, String> {
        loop {
            let action = {
                let mut inner = self.inner.lock().unwrap();
                let m = inner
                    .models
                    .get_mut(model_id)
                    .ok_or_else(|| format!("modèle inconnu: {model_id}"))?;
                match m.state {
                    ModelState::Loaded | ModelState::PartiallyOffloaded => LoadAction::Ready,
                    ModelState::Remote => LoadAction::Fail("modèle distant non servi en P1".into()),
                    ModelState::Error => LoadAction::Fail(
                        m.load_error
                            .clone()
                            .unwrap_or_else(|| "erreur inconnue".into()),
                    ),
                    _ if m.loading => LoadAction::Wait,
                    _ => {
                        m.loading = true;
                        m.state = ModelState::Loading;
                        m.profile = profile;
                        LoadAction::Start
                    }
                }
            };
            match action {
                LoadAction::Ready => {
                    let inner = self.inner.lock().unwrap();
                    let m = &inner.models[model_id];
                    return Ok(LoadResponse {
                        model_id: model_id.into(),
                        effective_profile: format!("{:?}", m.profile).to_lowercase(),
                        placement: m.plan.as_ref().map(|p| p.summary()).unwrap_or_default(),
                        warning: None,
                    });
                }
                LoadAction::Fail(e) => return Err(e),
                LoadAction::Wait => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
                LoadAction::Start => self.spawn_load_task(model_id, profile, kv_tokens),
            }
        }
    }

    fn spawn_load_task(&self, model_id: &str, profile: PlacementProfile, kv_tokens: u32) {
        let inner = self.inner.clone();
        let pm = self.pm.clone();
        let config = self.config.clone();
        let model_id = model_id.to_string();
        tokio::task::spawn_blocking(move || {
            let outcome: Result<LoadArtifacts, String> = (|| {
                let (desc, path) = {
                    let g = inner.lock().unwrap();
                    let m = g.models.get(&model_id).ok_or("modèle disparu")?;
                    (m.desc.clone(), m.path.clone())
                };
                let path = path.ok_or_else(|| "aucun chemin de poids configuré".to_string())?;
                if !path.exists() {
                    return Err(format!("poids introuvables: {}", path.display()));
                }
                // --- Plan de placement réel (P1.2) ---
                let plan = pm
                    .place_auto(&desc, profile, kv_tokens, Budgets::full(&pm.hw))
                    .map_err(|e| e.to_string())?;
                let ngl = plan.n_layers_on(Tier::Vram) as i32;
                let opts = LoadOptions {
                    n_gpu_layers: ngl,
                    load_mode: LoadMode::Mmap,
                    offload_kqv: plan.kv_bytes_on(Tier::Vram) > 0,
                    n_ctx: (kv_tokens + 1024).max(4096),
                    n_batch: 512,
                    n_ubatch: 512,
                    n_threads: config.n_threads,
                    flash_attn: true,
                };
                let model = Arc::new(LlamaModel::load(&path, &opts).map_err(|e| e.to_string())?);
                let ctx = LlamaContext::new(model.clone(), &opts).map_err(|e| e.to_string())?;
                let abort = ctx.abort_handle();
                let est = pm.estimate(&plan, &desc, 256, kv_tokens).tok_s;
                Ok((plan, model, Arc::new(StdMutex::new(ctx)), abort, est))
            })();

            let mut g = inner.lock().unwrap();
            let m = g.models.get_mut(&model_id).expect("modèle disparu");
            m.loading = false;
            match outcome {
                Ok((plan, model, ctx, abort, est_tok_s)) => {
                    // Réconciliation avec les métadonnées réelles du fichier.
                    m.desc.weights_bytes = model.size_bytes;
                    m.desc.n_layers = model.n_layer as u32;
                    m.est_tok_s = Some(est_tok_s);
                    let offloaded =
                        plan.layer_bytes_on(Tier::Ram) > 0 || plan.layer_bytes_on(Tier::Disk) > 0;
                    m.state = if offloaded {
                        ModelState::PartiallyOffloaded
                    } else {
                        ModelState::Loaded
                    };
                    m.plan = Some(plan);
                    m.model = Some(model);
                    m.ctx = Some(ctx);
                    m.ctx_abort = Some(abort);
                    eprintln!(
                        "[modeld] {} chargé ({}): {}",
                        model_id,
                        m.plan.as_ref().map(|p| p.summary()).unwrap_or_default(),
                        if offloaded {
                            "offload actif"
                        } else {
                            "full GPU/RAM"
                        }
                    );
                }
                Err(e) => {
                    m.state = ModelState::Error;
                    m.load_error = Some(e.clone());
                    eprintln!("[modeld] échec chargement {model_id}: {e}");
                }
            }
        });
    }

    /// Enfile une inférence (scheduler P1.3) et retourne
    /// `(inference_id, rx_deltas, rx_done)`.
    pub async fn infer(
        &self,
        model_id: &str,
        req: &InferRequest,
    ) -> Result<
        (
            u64,
            mpsc::Receiver<TokenEvent>,
            oneshot::Receiver<InferOutcome>,
        ),
        String,
    > {
        let (job_id, abort, initial_position) = {
            let mut g = self.inner.lock().unwrap();
            if !matches!(
                g.models.get(model_id).map(|m| &m.state),
                Some(ModelState::Loaded) | Some(ModelState::PartiallyOffloaded)
            ) {
                return Err(format!("modèle {model_id} non chargé"));
            }
            let id = g.next_inference;
            g.next_inference += 1;
            let abort = Arc::new(AtomicBool::new(false));
            g.job_aborts.insert(id, abort.clone());
            let m = g.models.get_mut(model_id).unwrap();
            m.queue.push(QueuedJob {
                priority: req.priority,
                id,
            });
            let pos = m.queue.len().saturating_sub(1);
            (id, abort, pos)
        };

        let (delta_tx, delta_rx) = mpsc::channel::<TokenEvent>(256);
        let (done_tx, done_rx) = oneshot::channel::<InferOutcome>();
        if initial_position > 0 {
            let _ = delta_tx
                .send(TokenEvent::Queued {
                    position: initial_position,
                })
                .await;
        }

        let inner = self.inner.clone();
        let model_id = model_id.to_string();
        let req = req.clone();
        tokio::spawn(async move {
            // --- Attente du tour (priorité, ancienneté) ---
            loop {
                if abort.load(Ordering::SeqCst) {
                    let mut g = inner.lock().unwrap();
                    let m = g.models.get_mut(&model_id).unwrap();
                    m.queue.retain(|j| j.id != job_id);
                    g.job_aborts.remove(&job_id);
                    let _ = done_tx.send(InferOutcome::Cancelled);
                    return;
                }
                let my_turn = {
                    let mut g = inner.lock().unwrap();
                    let m = g.models.get_mut(&model_id).unwrap();
                    if m.running_job.is_none() && m.queue.peek().map(|j| j.id) == Some(job_id) {
                        m.queue.pop();
                        m.running_job = Some(job_id);
                        true
                    } else {
                        false
                    }
                };
                if my_turn {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }

            let _ = delta_tx
                .send(TokenEvent::Started {
                    inference_id: job_id,
                })
                .await;

            // --- Exécution (bloquante) dans un thread dédié ---
            let ctx = {
                let g = inner.lock().unwrap();
                g.models.get(&model_id).and_then(|m| m.ctx.clone())
            };
            let inner2 = inner.clone();
            let model_id2 = model_id.clone();
            let outcome = match ctx {
                Some(ctx) => {
                    let delta_tx2 = delta_tx.clone();
                    let messages: Vec<(String, String)> = req
                        .messages
                        .iter()
                        .map(|m| (m.role.clone(), m.content.clone()))
                        .collect();
                    let params = GenParams {
                        max_tokens: req.params.max_tokens,
                        temperature: req.params.temperature,
                        top_p: req.params.top_p,
                        seed: req.params.seed.unwrap_or(42),
                    };
                    tokio::task::spawn_blocking(move || {
                        let mut guard = ctx.lock().unwrap();
                        guard.generate(&messages, &params, |piece| {
                            delta_tx2
                                .blocking_send(TokenEvent::Delta {
                                    text: piece.to_string(),
                                })
                                .is_ok()
                        })
                    })
                    .await
                    .map(|r| match r {
                        Ok(stats) => InferOutcome::Done {
                            prompt_tokens: stats.prompt_tokens,
                            generated_tokens: stats.generated_tokens,
                            ttft_ms: stats.ttft_ms,
                            tok_s: stats.tok_s,
                        },
                        Err(e) => InferOutcome::Failed(e.to_string()),
                    })
                    .unwrap_or_else(|e| InferOutcome::Failed(e.to_string()))
                }
                None => InferOutcome::Failed("contexte disparu".into()),
            };

            // --- Libération du tour + métriques ---
            {
                let mut g = inner2.lock().unwrap();
                let m = g.models.get_mut(&model_id2).unwrap();
                m.running_job = None;
                if let InferOutcome::Done { ttft_ms, tok_s, .. } = &outcome {
                    m.last_ttft_ms = Some(*ttft_ms);
                    m.last_tok_s = Some(*tok_s);
                }
                g.job_aborts.remove(&job_id);
            }
            let _ = done_tx.send(outcome);
        });

        Ok((job_id, delta_rx, done_rx))
    }

    /// `model.cancel` — annulation coopérative (frontière de token, §3.6).
    pub fn cancel(&self, inference_id: u64) -> bool {
        let g = self.inner.lock().unwrap();
        if let Some(flag) = g.job_aborts.get(&inference_id) {
            flag.store(true, Ordering::SeqCst);
            // Si le job tourne, son contexte doit avorter aussi.
            for m in g.models.values() {
                if m.running_job == Some(inference_id) {
                    if let Some(abort) = &m.ctx_abort {
                        abort.store(true, Ordering::SeqCst);
                    }
                }
            }
            true
        } else {
            false
        }
    }

    /// `model.unload`.
    pub fn unload(&self, model_id: &str) -> bool {
        let mut g = self.inner.lock().unwrap();
        match g.models.get_mut(model_id) {
            Some(m) if m.running_job.is_none() && !m.loading => {
                m.ctx = None; // LlamaContext::drop libère le contexte
                m.model = None; // puis les poids
                m.ctx_abort = None;
                m.state = ModelState::OnDisk;
                m.plan = None;
                true
            }
            _ => false,
        }
    }

    /// `model.metrics` (F-PLC-08, F-OBS-02).
    pub fn metrics(&self, ram: (u64, u64), cpu_percent: f32) -> SystemMetrics {
        let g = self.inner.lock().unwrap();
        let models = g
            .models
            .values()
            .map(|m| ModelMetrics {
                model_id: m.desc.id.clone(),
                state: m.state.clone(),
                active_inferences: m.running_job.map(|_| 1).unwrap_or(0),
                queued: m.queue.len() as u32,
                last_ttft_ms: m.last_ttft_ms,
                last_tok_s: m.last_tok_s,
                vram_bytes: m.plan.as_ref().map(|p| p.bytes_on(Tier::Vram)).unwrap_or(0),
                ram_bytes: m.plan.as_ref().map(|p| p.bytes_on(Tier::Ram)).unwrap_or(0),
                disk_bytes: m.plan.as_ref().map(|p| p.bytes_on(Tier::Disk)).unwrap_or(0),
            })
            .collect();
        SystemMetrics {
            models,
            ram_total: ram.0,
            ram_used: ram.1,
            ram_free: ram.0.saturating_sub(ram.1),
            cpu_percent,
            agents_active: 0,
        }
    }
}
