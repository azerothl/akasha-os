//! Cœur du Model Subsystem : état, scheduler, placement réel.

use crate::config::ModeldConfig;
use aos_llama::{BatchItem, GenParams, LlamaContext, LlamaModel, LoadMode, LoadOptions};
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

/// Job envoyé au dispatcher de continuous batching (P5.1).
struct DispatchJob {
    job_id: u64,
    priority: u8,
    messages: Vec<(String, String)>,
    params: GenParams,
    abort: Arc<AtomicBool>,
    delta_tx: mpsc::Sender<TokenEvent>,
    done_tx: oneshot::Sender<InferOutcome>,
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
    /// Inférences dans le batch GPU courant.
    pub active: u32,
    /// Jobs en attente du prochain batch.
    pub pending: u32,
    dispatch: Option<mpsc::Sender<DispatchJob>>,
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
            active: 0,
            pending: 0,
            dispatch: None,
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
    /// Politique de routage courante (F-MDL-07).
    routing_mode: String,
    /// Backends distants configurés (P3.1).
    remote_backends: HashMap<String, crate::backend::RemoteOpenAiBackend>,
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
                routing_mode: config.routing.clone(),
                remote_backends: HashMap::new(),
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
                    n_ctx: (kv_tokens + 1024).max(4096) * config.n_seq_max.max(1),
                    // Prefill chunké dans aos-llama ; 2048 réduit les round-trips.
                    n_batch: 2048,
                    n_ubatch: 512,
                    n_threads: config.n_threads,
                    flash_attn: true,
                    embeddings: false,
                    n_seq_max: config.n_seq_max.max(1),
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

    /// Enfile une inférence (continuous batching P5.1) et retourne
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
        let (job_id, abort, tx) = {
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
            m.pending += 1;
            if m.dispatch.is_none() {
                let (dtx, drx) = mpsc::channel::<DispatchJob>(64);
                m.dispatch = Some(dtx);
                let inner = self.inner.clone();
                let mid = model_id.to_string();
                let window = std::time::Duration::from_millis(self.config.batch_window_ms);
                let n_seq = self.config.n_seq_max.max(1) as usize;
                tokio::spawn(async move {
                    Self::dispatch_loop(inner, mid, drx, window, n_seq).await;
                });
            }
            let tx = m.dispatch.clone().unwrap();
            (id, abort, tx)
        };

        let (delta_tx, delta_rx) = mpsc::channel::<TokenEvent>(256);
        let (done_tx, done_rx) = oneshot::channel::<InferOutcome>();
        let job = DispatchJob {
            job_id,
            priority: req.priority,
            messages: req
                .messages
                .iter()
                .map(|m| (m.role.clone(), m.content.clone()))
                .collect(),
            params: GenParams {
                max_tokens: req.params.max_tokens,
                temperature: req.params.temperature,
                top_p: req.params.top_p,
                seed: req.params.seed.unwrap_or(42),
            },
            abort,
            delta_tx,
            done_tx,
        };
        if tx.send(job).await.is_err() {
            let mut g = self.inner.lock().unwrap();
            g.job_aborts.remove(&job_id);
            if let Some(m) = g.models.get_mut(model_id) {
                m.pending = m.pending.saturating_sub(1);
            }
            return Err("dispatcher arrêté".into());
        }

        Ok((job_id, delta_rx, done_rx))
    }

    async fn dispatch_loop(
        inner: Arc<StdMutex<Inner>>,
        model_id: String,
        rx: mpsc::Receiver<DispatchJob>,
        window: std::time::Duration,
        n_seq: usize,
    ) {
        let rx = Arc::new(tokio::sync::Mutex::new(rx));
        loop {
            let first = {
                let mut g = rx.lock().await;
                match g.recv().await {
                    Some(j) => j,
                    None => break,
                }
            };
            let mut batch = vec![first];
            {
                let mut g = rx.lock().await;
                while batch.len() < n_seq {
                    match g.try_recv() {
                        Ok(j) => batch.push(j),
                        Err(_) => break,
                    }
                }
                if batch.len() < n_seq {
                    let deadline = tokio::time::Instant::now() + window;
                    while batch.len() < n_seq {
                        match tokio::time::timeout_at(deadline, g.recv()).await {
                            Ok(Some(j)) => {
                                batch.push(j);
                                while batch.len() < n_seq {
                                    match g.try_recv() {
                                        Ok(j) => batch.push(j),
                                        Err(_) => break,
                                    }
                                }
                            }
                            _ => break,
                        }
                    }
                }
            }
            batch.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.job_id.cmp(&b.job_id)));
            eprintln!(
                "[modeld] batch {} job(s) (fenêtre {} ms, n_seq_max={n_seq}) — admit à chaud actif",
                batch.len(),
                window.as_millis()
            );

            let n = batch.len() as u32;
            {
                let mut g = inner.lock().unwrap();
                if let Some(m) = g.models.get_mut(&model_id) {
                    m.pending = m.pending.saturating_sub(n);
                    m.active = n;
                }
            }
            for j in &batch {
                let _ = j
                    .delta_tx
                    .send(TokenEvent::Started {
                        inference_id: j.job_id,
                    })
                    .await;
            }

            let ctx = {
                let g = inner.lock().unwrap();
                g.models.get(&model_id).and_then(|m| m.ctx.clone())
            };

            // Handles I/O indexés comme les items du batch (y compris admits).
            struct JobIo {
                job_id: u64,
                delta_tx: mpsc::Sender<TokenEvent>,
                done_tx: Option<oneshot::Sender<InferOutcome>>,
                abort: Arc<AtomicBool>,
            }
            let ios: Arc<StdMutex<Vec<JobIo>>> = Arc::new(StdMutex::new(
                batch
                    .iter()
                    .map(|j| JobIo {
                        job_id: j.job_id,
                        delta_tx: j.delta_tx.clone(),
                        done_tx: None, // rempli après move
                        abort: j.abort.clone(),
                    })
                    .collect(),
            ));
            // done_tx can't be cloned — store separately after moving batch
            let mut done_txs: Vec<Option<oneshot::Sender<InferOutcome>>> =
                batch.iter().map(|_| None).collect();
            let items: Vec<BatchItem> = batch
                .into_iter()
                .enumerate()
                .map(|(i, j)| {
                    done_txs[i] = Some(j.done_tx);
                    BatchItem {
                        messages: j.messages,
                        params: j.params,
                        abort: j.abort,
                    }
                })
                .collect();
            {
                let mut g = ios.lock().unwrap();
                for (i, tx) in done_txs.into_iter().enumerate() {
                    g[i].done_tx = tx;
                }
            }

            let rx_admit = rx.clone();
            let ios_delta = ios.clone();
            let ios_admit = ios.clone();
            let ios_reject = ios.clone();
            let inner_admit = inner.clone();
            let inner_reject = inner.clone();
            let mid_admit = model_id.clone();
            let mid_reject = model_id.clone();
            let n_seq_u32 = n_seq as u32;

            let results = match ctx {
                Some(ctx) => {
                    tokio::task::spawn_blocking(move || {
                        let mut guard = ctx.lock().unwrap();
                        guard.generate_batch_admit(
                            &items,
                            || {
                                let mut g = rx_admit.blocking_lock();
                                let Ok(j) = g.try_recv() else {
                                    return None;
                                };
                                drop(g);
                                let _ = j.delta_tx.try_send(TokenEvent::Started {
                                    inference_id: j.job_id,
                                });
                                {
                                    let mut st = inner_admit.lock().unwrap();
                                    if let Some(m) = st.models.get_mut(&mid_admit) {
                                        m.pending = m.pending.saturating_sub(1);
                                        m.active = (m.active + 1).min(n_seq_u32);
                                    }
                                }
                                let item = BatchItem {
                                    messages: j.messages,
                                    params: j.params,
                                    abort: j.abort.clone(),
                                };
                                // Aligné sur out[idx] côté llama (push avant
                                // tokenize) ; retiré dans on_reject si échec.
                                ios_admit.lock().unwrap().push(JobIo {
                                    job_id: j.job_id,
                                    delta_tx: j.delta_tx,
                                    done_tx: Some(j.done_tx),
                                    abort: j.abort,
                                });
                                Some(item)
                            },
                            |err| {
                                if let Some(io) = ios_reject.lock().unwrap().pop() {
                                    {
                                        let mut st = inner_reject.lock().unwrap();
                                        st.job_aborts.remove(&io.job_id);
                                        if let Some(m) = st.models.get_mut(&mid_reject) {
                                            m.active = m.active.saturating_sub(1);
                                        }
                                    }
                                    if let Some(tx) = io.done_tx {
                                        let _ = tx.send(InferOutcome::Failed(err.to_string()));
                                    }
                                }
                            },
                            |i, piece| {
                                let tx = {
                                    let g = ios_delta.lock().unwrap();
                                    g.get(i).map(|io| io.delta_tx.clone())
                                };
                                match tx {
                                    Some(tx) => {
                                        match tx.try_send(TokenEvent::Delta {
                                            text: piece.to_string(),
                                        }) {
                                            Ok(()) => true,
                                            Err(
                                                tokio::sync::mpsc::error::TrySendError::Full(ev),
                                            ) => tx.blocking_send(ev).is_ok(),
                                            Err(
                                                tokio::sync::mpsc::error::TrySendError::Closed(_),
                                            ) => false,
                                        }
                                    }
                                    None => false,
                                }
                            },
                        )
                    })
                    .await
                    .unwrap_or_else(|_| Vec::new())
                }
                None => Vec::new(),
            };

            {
                let mut g = inner.lock().unwrap();
                if let Some(m) = g.models.get_mut(&model_id) {
                    m.active = 0;
                }
            }

            let mut ios = ios.lock().unwrap();
            if results.is_empty() && !ios.is_empty() {
                for io in ios.drain(..) {
                    inner.lock().unwrap().job_aborts.remove(&io.job_id);
                    if let Some(tx) = io.done_tx {
                        let _ = tx.send(InferOutcome::Failed("contexte disparu".into()));
                    }
                }
                continue;
            }
            let mut results = results;
            while results.len() < ios.len() {
                results.push(Err(aos_llama::LlamaError::Decode(-5)));
            }
            for (io, res) in ios.drain(..).zip(results.into_iter()) {
                let outcome = match res {
                    Ok(stats) => {
                        let mut g = inner.lock().unwrap();
                        if let Some(m) = g.models.get_mut(&model_id) {
                            m.last_ttft_ms = Some(stats.ttft_ms);
                            m.last_tok_s = Some(stats.tok_s);
                        }
                        g.job_aborts.remove(&io.job_id);
                        InferOutcome::Done {
                            prompt_tokens: stats.prompt_tokens,
                            generated_tokens: stats.generated_tokens,
                            ttft_ms: stats.ttft_ms,
                            tok_s: stats.tok_s,
                        }
                    }
                    Err(e) => {
                        inner.lock().unwrap().job_aborts.remove(&io.job_id);
                        if io.abort.load(Ordering::SeqCst) {
                            InferOutcome::Cancelled
                        } else {
                            InferOutcome::Failed(e.to_string())
                        }
                    }
                };
                if let Some(tx) = io.done_tx {
                    let _ = tx.send(outcome);
                }
            }
        }
    }

    /// `model.cancel` — annulation coopérative (frontière de token, §3.6).
    pub fn cancel(&self, inference_id: u64) -> bool {
        let g = self.inner.lock().unwrap();
        if let Some(flag) = g.job_aborts.get(&inference_id) {
            flag.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    /// `model.unload`.
    pub fn unload(&self, model_id: &str) -> bool {
        let mut g = self.inner.lock().unwrap();
        match g.models.get_mut(model_id) {
            Some(m) if m.active == 0 && m.pending == 0 && !m.loading => {
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

    // --- Routage local/distant (§3.7, P3.2) ---

    /// Politique de routage courante.
    pub fn routing_mode(&self) -> String {
        self.inner.lock().unwrap().routing_mode.clone()
    }

    pub fn set_routing(&self, mode: &str) -> Result<(), String> {
        match mode {
            "balanced" | "local_only" | "remote_only" => {
                self.inner.lock().unwrap().routing_mode = mode.into();
                Ok(())
            }
            _ => Err(format!("mode inconnu: {mode}")),
        }
    }

    /// `model.backend.add` : enregistre un backend distant OpenAI-compatible.
    pub fn add_remote_backend(
        &self,
        model_id: &str,
        endpoint: &str,
        remote_model: &str,
        api_key: Option<String>,
    ) {
        let backend = crate::backend::RemoteOpenAiBackend::new(endpoint, remote_model, api_key);
        let mut g = self.inner.lock().unwrap();
        g.remote_backends.insert(model_id.into(), backend);
        if let Some(m) = g.models.get_mut(model_id) {
            m.state = ModelState::Remote;
        }
    }

    /// Un backend distant est-il configuré pour ce modèle ?
    pub fn has_remote(&self, model_id: &str) -> bool {
        self.inner
            .lock()
            .unwrap()
            .remote_backends
            .contains_key(model_id)
    }

    /// Endpoint du backend distant (pour net.check côté daemon).
    pub fn remote_endpoint(&self, model_id: &str) -> Option<String> {
        self.inner
            .lock()
            .unwrap()
            .remote_backends
            .get(model_id)
            .map(|b| b.endpoint.clone())
    }

    /// Exécute une inférence sur le backend distant (flux).
    pub async fn infer_remote(
        &self,
        model_id: &str,
        req: &aos_proto::InferRequest,
        tx: tokio::sync::mpsc::Sender<aos_proto::TokenEvent>,
    ) -> Result<(), String> {
        let backend = {
            let g = self.inner.lock().unwrap();
            g.remote_backends.get(model_id).cloned()
        };
        let be = backend.ok_or_else(|| format!("backend distant non configuré: {model_id}"))?;
        let abort = Arc::new(AtomicBool::new(false));
        be.infer_stream(req, tx, abort)
            .await
            .map_err(|e| e.to_string())
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
                active_inferences: m.active,
                queued: m.pending,
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
