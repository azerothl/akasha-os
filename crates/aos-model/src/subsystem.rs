//! Cœur du Model Subsystem : état, scheduler, placement réel.

use crate::config::ModeldConfig;
use aos_llama::{BatchItem, GenParams, LlamaContext, LlamaModel, LoadMode, LoadOptions, StopReason};
use aos_placement::{
    CostModel, HardwareProfile, ModelDesc, PlacementPlan, PlacementProfile, PlacementSim,
    Priority, Tier,
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
    pause: Arc<AtomicBool>,
    resumed: bool,
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
    /// Image/video generation in flight (sd.cpp).
    pub media_step: Option<u32>,
    pub media_total_steps: Option<u32>,
    pub media_started_ms: Option<u64>,
    pub last_step_s: Option<f64>,
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
            media_step: None,
            media_total_steps: None,
            media_started_ms: None,
            last_step_s: None,
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
    /// inference_id → pause (E18 migrate, distinct from abort).
    job_pauses: HashMap<u64, Arc<AtomicBool>>,
    next_inference: u64,
    /// Politique de routage courante (F-MDL-07).
    routing_mode: String,
    /// Backends distants configurés (P3.1).
    remote_backends: HashMap<String, crate::backend::RemoteOpenAiBackend>,
    /// Pin live `auto` | `gpu` | `cpu` (overrides `AOS_INFERENCE` for reload).
    inference_pin: String,
    /// Jobs paused mid-stream waiting for a new context (prefix replay).
    paused_jobs: Vec<DispatchJob>,
}

enum LoadAction {
    Ready,
    Wait,
    Start,
    Fail(String),
}

/// Model Subsystem (partagé entre handlers du bus).
#[derive(Clone)]
pub struct ModelSubsystem {
    inner: Arc<StdMutex<Inner>>,
    pub config: ModeldConfig,
    sim: Arc<StdMutex<PlacementSim>>,
}

/// Prefix already streamed so a new llama context continues the same turn (E18).
pub fn resume_messages(
    messages: &[(String, String)],
    generated: &str,
) -> Vec<(String, String)> {
    let mut out = messages.to_vec();
    if !generated.is_empty() {
        out.push(("assistant".into(), generated.to_string()));
    }
    out
}

impl ModelSubsystem {
    pub fn new(config: ModeldConfig, registry: &ModelRegistry, ram_total: u64) -> Self {
        let gpu = config.gpu && aos_llama::LlamaBackend::supports_gpu_offload();
        let n_gpus = if gpu {
            aos_llama::LlamaBackend::gpu_device_count().max(1)
        } else {
            0
        };
        let gpus = if n_gpus == 0 {
            vec![]
        } else {
            let each = config.vram_total_bytes / n_gpus as u64;
            let rem = config.vram_total_bytes % n_gpus as u64;
            (0..n_gpus)
                .map(|i| aos_placement::GpuDevice {
                    id: i as u32,
                    name: format!("gpu{i}"),
                    vram_total: each + if i == 0 { rem } else { 0 },
                })
                .collect()
        };
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
            gpus,
        };
        let sim = Arc::new(StdMutex::new(PlacementSim::new(
            hw,
            CostModel::default(),
        )));
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
                job_pauses: HashMap::new(),
                next_inference: 1,
                routing_mode: config.routing.clone(),
                remote_backends: HashMap::new(),
                inference_pin: std::env::var("AOS_INFERENCE")
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
                paused_jobs: Vec::new(),
            })),
            config,
            sim,
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
        let sim = self.sim.clone();
        let config = self.config.clone();
        let model_id = model_id.to_string();
        tokio::task::spawn_blocking(move || {
            let (desc, path) = {
                let g = inner.lock().unwrap();
                match g.models.get(&model_id) {
                    Some(m) => (m.desc.clone(), m.path.clone()),
                    None => {
                        return;
                    }
                }
            };
            let apply_err = |e: String| {
                let mut g = inner.lock().unwrap();
                if let Some(m) = g.models.get_mut(&model_id) {
                    m.loading = false;
                    m.state = ModelState::Error;
                    m.load_error = Some(e.clone());
                }
                eprintln!("[modeld] échec chargement {model_id}: {e}");
            };
            let Some(path) = path else {
                apply_err("aucun chemin de poids configuré".into());
                return;
            };
            if !path.exists() {
                apply_err(format!("poids introuvables: {}", path.display()));
                return;
            }
            let plan = {
                let mut sim = sim.lock().unwrap();
                if sim.get(&model_id).is_some() {
                    sim.unload(&model_id);
                }
                let inference = {
                    let g = inner.lock().unwrap();
                    let pin = g.inference_pin.trim().to_ascii_lowercase();
                    if pin.is_empty() {
                        std::env::var("AOS_INFERENCE")
                            .unwrap_or_default()
                            .to_ascii_lowercase()
                    } else {
                        pin
                    }
                };
                let effective_profile = match inference.as_str() {
                    "cpu" => PlacementProfile::CpuOnly,
                    "auto" => sim.auto_hysteresis_profile(),
                    _ => profile,
                };
                match sim.place(&desc, effective_profile, Priority::Interactive, kv_tokens) {
                    Ok(()) => sim.get(&model_id).map(|p| p.plan.clone()),
                    Err(e) => {
                        drop(sim);
                        apply_err(e.to_string());
                        return;
                    }
                }
            };
            let Some(plan) = plan else {
                apply_err("placement disparu".into());
                return;
            };

            if desc.is_media() {
                let mut g = inner.lock().unwrap();
                let m = g.models.get_mut(&model_id).expect("modèle disparu");
                m.loading = false;
                m.state = ModelState::Loaded;
                m.plan = Some(plan);
                m.est_tok_s = None;
                eprintln!("[modeld] {model_id} média placé (Placement Manager)");
                return;
            }

            let ngl = plan.n_layers_on(Tier::Vram) as i32;
            let opts = LoadOptions {
                n_gpu_layers: ngl,
                load_mode: LoadMode::Mmap,
                offload_kqv: plan.kv_bytes_on(Tier::Vram) > 0,
                n_ctx: (kv_tokens + 1024).max(4096) * config.n_seq_max.max(1),
                n_batch: 2048,
                n_ubatch: 512,
                n_threads: config.n_threads,
                flash_attn: true,
                embeddings: false,
                n_seq_max: config.n_seq_max.max(1),
                tensor_split: plan.tensor_split.clone(),
                main_gpu: plan.main_gpu,
            };
            let model = match LlamaModel::load(&path, &opts) {
                Ok(m) => Arc::new(m),
                Err(e) => {
                    sim.lock().unwrap().unload(&model_id);
                    apply_err(e.to_string());
                    return;
                }
            };
            let ctx = match LlamaContext::new(model.clone(), &opts) {
                Ok(c) => c,
                Err(e) => {
                    sim.lock().unwrap().unload(&model_id);
                    apply_err(e.to_string());
                    return;
                }
            };
            let abort = ctx.abort_handle();
            let est = sim
                .lock()
                .unwrap()
                .estimate(&model_id, 256, kv_tokens)
                .map(|e| e.tok_s)
                .unwrap_or(0.0);
            let mut g = inner.lock().unwrap();
            let m = g.models.get_mut(&model_id).expect("modèle disparu");
            m.loading = false;
            m.desc.weights_bytes = model.size_bytes;
            m.desc.n_layers = model.n_layer as u32;
            m.est_tok_s = Some(est);
            let offloaded =
                plan.layer_bytes_on(Tier::Ram) > 0 || plan.layer_bytes_on(Tier::Disk) > 0;
            m.state = if offloaded {
                ModelState::PartiallyOffloaded
            } else {
                ModelState::Loaded
            };
            m.plan = Some(plan);
            m.model = Some(model);
            m.ctx = Some(Arc::new(StdMutex::new(ctx)));
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
        let (job_id, abort, pause, tx) = {
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
            let pause = Arc::new(AtomicBool::new(false));
            g.job_aborts.insert(id, abort.clone());
            g.job_pauses.insert(id, pause.clone());
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
            (id, abort, pause, tx)
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
            pause,
            resumed: false,
            delta_tx,
            done_tx,
        };
        if tx.send(job).await.is_err() {
            let mut g = self.inner.lock().unwrap();
            g.job_aborts.remove(&job_id);
            g.job_pauses.remove(&job_id);
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
                if j.resumed {
                    continue;
                }
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
                priority: u8,
                messages: Vec<(String, String)>,
                params: GenParams,
                delta_tx: mpsc::Sender<TokenEvent>,
                done_tx: Option<oneshot::Sender<InferOutcome>>,
                abort: Arc<AtomicBool>,
                pause: Arc<AtomicBool>,
                generated: String,
            }
            let ios: Arc<StdMutex<Vec<JobIo>>> = Arc::new(StdMutex::new(
                batch
                    .iter()
                    .map(|j| JobIo {
                        job_id: j.job_id,
                        priority: j.priority,
                        messages: j.messages.clone(),
                        params: j.params.clone(),
                        delta_tx: j.delta_tx.clone(),
                        done_tx: None, // rempli après move
                        abort: j.abort.clone(),
                        pause: j.pause.clone(),
                        generated: String::new(),
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
                        pause: j.pause,
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
                                if !j.resumed {
                                    let _ = j.delta_tx.try_send(TokenEvent::Started {
                                        inference_id: j.job_id,
                                    });
                                }
                                {
                                    let mut st = inner_admit.lock().unwrap();
                                    if let Some(m) = st.models.get_mut(&mid_admit) {
                                        m.pending = m.pending.saturating_sub(1);
                                        m.active = (m.active + 1).min(n_seq_u32);
                                    }
                                }
                                let item = BatchItem {
                                    messages: j.messages.clone(),
                                    params: j.params.clone(),
                                    abort: j.abort.clone(),
                                    pause: j.pause.clone(),
                                };
                                // Aligné sur out[idx] côté llama (push avant
                                // tokenize) ; retiré dans on_reject si échec.
                                ios_admit.lock().unwrap().push(JobIo {
                                    job_id: j.job_id,
                                    priority: j.priority,
                                    messages: j.messages,
                                    params: j.params,
                                    delta_tx: j.delta_tx,
                                    done_tx: Some(j.done_tx),
                                    abort: j.abort,
                                    pause: j.pause,
                                    generated: String::new(),
                                });
                                Some(item)
                            },
                            |err| {
                                if let Some(io) = ios_reject.lock().unwrap().pop() {
                                    {
                                        let mut st = inner_reject.lock().unwrap();
                                        st.job_aborts.remove(&io.job_id);
                                        st.job_pauses.remove(&io.job_id);
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
                                    let mut g = ios_delta.lock().unwrap();
                                    if let Some(io) = g.get_mut(i) {
                                        io.generated.push_str(piece);
                                    }
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
                    let mut g = inner.lock().unwrap();
                    g.job_aborts.remove(&io.job_id);
                    g.job_pauses.remove(&io.job_id);
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
                let paused = matches!(
                    &res,
                    Ok(stats) if stats.stopped == StopReason::Paused
                );
                if paused {
                    io.pause.store(false, Ordering::SeqCst);
                    let remaining = io
                        .params
                        .max_tokens
                        .saturating_sub(match &res {
                            Ok(s) => s.generated_tokens,
                            Err(_) => 0,
                        })
                        .max(1);
                    let mut params = io.params.clone();
                    params.max_tokens = remaining;
                    if let Some(done_tx) = io.done_tx {
                        inner.lock().unwrap().paused_jobs.push(DispatchJob {
                            job_id: io.job_id,
                            priority: io.priority,
                            messages: resume_messages(&io.messages, &io.generated),
                            params,
                            abort: io.abort,
                            pause: io.pause,
                            resumed: true,
                            delta_tx: io.delta_tx,
                            done_tx,
                        });
                    }
                    continue;
                }
                let outcome = match res {
                    Ok(stats) => {
                        let mut g = inner.lock().unwrap();
                        if let Some(m) = g.models.get_mut(&model_id) {
                            m.last_ttft_ms = Some(stats.ttft_ms);
                            m.last_tok_s = Some(stats.tok_s);
                        }
                        g.job_aborts.remove(&io.job_id);
                        g.job_pauses.remove(&io.job_id);
                        InferOutcome::Done {
                            prompt_tokens: stats.prompt_tokens,
                            generated_tokens: stats.generated_tokens,
                            ttft_ms: stats.ttft_ms,
                            tok_s: stats.tok_s,
                        }
                    }
                    Err(e) => {
                        let mut g = inner.lock().unwrap();
                        g.job_aborts.remove(&io.job_id);
                        g.job_pauses.remove(&io.job_id);
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

    /// Prefix already shown so a new context can continue the same turn (E18).
    pub fn resume_prefix(
        messages: &[(String, String)],
        generated: &str,
    ) -> Vec<(String, String)> {
        resume_messages(messages, generated)
    }

    /// In-process device migrate (E18). Fail-closed: caller falls back to 0.8 restart.
    pub async fn migrate(&self, target: &str) -> aos_proto::MigrateResponse {
        let pin = match target.trim().to_ascii_lowercase().as_str() {
            "auto" | "gpu" | "cpu" => target.trim().to_ascii_lowercase(),
            other => {
                return aos_proto::MigrateResponse {
                    ok: false,
                    fallback: true,
                    message: format!("cible migrate inconnue: {other}"),
                    profile: String::new(),
                };
            }
        };
        {
            let mut g = self.inner.lock().unwrap();
            g.inference_pin = pin.clone();
            for flag in g.job_pauses.values() {
                flag.store(true, Ordering::SeqCst);
            }
        }
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let busy = {
                let g = self.inner.lock().unwrap();
                g.models.values().any(|m| {
                    !m.desc.is_media() && (m.active > 0 || m.pending > 0 || m.loading)
                })
            };
            if !busy {
                break;
            }
            if tokio::time::Instant::now() > deadline {
                return aos_proto::MigrateResponse {
                    ok: false,
                    fallback: true,
                    message: "migrate timeout — fallback cancel+restart".into(),
                    profile: pin,
                };
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let model_id = {
            let g = self.inner.lock().unwrap();
            g.models
                .iter()
                .find(|(_, m)| {
                    !m.desc.is_media()
                        && matches!(
                            m.state,
                            ModelState::Loaded
                                | ModelState::PartiallyOffloaded
                                | ModelState::OnDisk
                        )
                        && m.path.is_some()
                })
                .map(|(id, _)| id.clone())
                .or_else(|| self.config.default_model.clone())
        };
        let Some(model_id) = model_id else {
            return aos_proto::MigrateResponse {
                ok: true,
                fallback: false,
                message: "aucun modèle local à migrer".into(),
                profile: pin,
            };
        };
        if let Err(e) = self.force_reload(&model_id).await {
            let jobs = {
                let mut g = self.inner.lock().unwrap();
                std::mem::take(&mut g.paused_jobs)
            };
            for j in jobs {
                let _ = j.done_tx.send(InferOutcome::Cancelled);
            }
            return aos_proto::MigrateResponse {
                ok: false,
                fallback: true,
                message: e,
                profile: pin,
            };
        }
        let jobs = {
            let mut g = self.inner.lock().unwrap();
            std::mem::take(&mut g.paused_jobs)
        };
        let n = jobs.len();
        for job in jobs {
            let tx = {
                let g = self.inner.lock().unwrap();
                g.models.get(&model_id).and_then(|m| m.dispatch.clone())
            };
            match tx {
                Some(tx) => {
                    {
                        let mut g = self.inner.lock().unwrap();
                        if let Some(m) = g.models.get_mut(&model_id) {
                            m.pending += 1;
                        }
                    }
                    if tx.send(job).await.is_err() {
                        return aos_proto::MigrateResponse {
                            ok: false,
                            fallback: true,
                            message: "dispatcher arrêté après reload".into(),
                            profile: pin,
                        };
                    }
                }
                None => {
                    let _ = job.done_tx.send(InferOutcome::Cancelled);
                    return aos_proto::MigrateResponse {
                        ok: false,
                        fallback: true,
                        message: "pas de dispatcher après reload".into(),
                        profile: pin,
                    };
                }
            }
        }
        let profile = {
            let g = self.inner.lock().unwrap();
            g.models
                .get(&model_id)
                .map(|m| format!("{:?}", m.profile).to_lowercase())
                .unwrap_or(pin.clone())
        };
        aos_proto::MigrateResponse {
            ok: true,
            fallback: false,
            message: format!("migré {n} job(s) → {profile}"),
            profile,
        }
    }

    async fn force_reload(&self, model_id: &str) -> Result<(), String> {
        {
            let mut g = self.inner.lock().unwrap();
            let m = g
                .models
                .get_mut(model_id)
                .ok_or_else(|| format!("modèle inconnu: {model_id}"))?;
            m.ctx = None;
            m.model = None;
            m.ctx_abort = None;
            m.state = ModelState::OnDisk;
            m.plan = None;
            m.loading = false;
        }
        self.sim.lock().unwrap().unload(model_id);
        let profile = {
            let g = self.inner.lock().unwrap();
            match g.inference_pin.as_str() {
                "cpu" => PlacementProfile::CpuOnly,
                _ => PlacementProfile::Balanced,
            }
        };
        self.ensure_loaded(model_id, profile, self.config.default_kv_tokens)
            .await
            .map(|_| ())
    }

    pub fn inference_pin(&self) -> String {
        self.inner.lock().unwrap().inference_pin.clone()
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
    ///
    /// Never hold `inner` while waiting on `PlacementSim`: `spawn_load_task`
    /// holds `sim` during `place` then takes `inner`. Nested `inner` → `sim`
    /// deadlocks aos-modeld when one model unloads while another is placing.
    pub fn unload(&self, model_id: &str) -> bool {
        {
            let mut g = self.inner.lock().unwrap();
            match g.models.get_mut(model_id) {
                Some(m) if m.active == 0 && m.pending == 0 && !m.loading => {
                    m.ctx = None;
                    m.model = None;
                    m.ctx_abort = None;
                    m.state = ModelState::OnDisk;
                    m.plan = None;
                    // Block a concurrent ensure_loaded from placing until sim is updated.
                    m.loading = true;
                }
                _ => return false,
            }
        }
        self.sim.lock().unwrap().unload(model_id);
        let mut g = self.inner.lock().unwrap();
        if let Some(m) = g.models.get_mut(model_id) {
            if m.loading && m.state == ModelState::OnDisk && m.active == 0 && m.pending == 0 {
                m.loading = false;
            }
        }
        true
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
        } else {
            let desc = ModelDesc {
                id: model_id.to_string(),
                name: model_id.to_string(),
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
            g.models.insert(model_id.into(), rt);
        }
    }

    pub fn remove_remote_backend(&self, model_id: &str) {
        let mut g = self.inner.lock().unwrap();
        g.remote_backends.remove(model_id);
        if model_id.starts_with("remote:") || model_id.starts_with("provider:") {
            g.models.remove(model_id);
        }
    }

    pub fn remove_provider_models(&self, provider_id: &str) {
        let prefix = format!("provider:{provider_id}:");
        let mut g = self.inner.lock().unwrap();
        let ids: Vec<String> = g
            .models
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();
        for id in ids {
            g.remote_backends.remove(&id);
            g.models.remove(&id);
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
                media_step: m.media_step,
                media_total_steps: m.media_total_steps,
                last_step_s: m.last_step_s,
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

    /// First installed media pack of `kind` (`image` | `tts`), or `requested` id.
    /// Honors persisted prefs (`default_image_model` / `default_audio_model`) when
    /// the caller did not pass an id (P09.5).
    pub fn find_media_model(
        &self,
        kind: &str,
        requested: Option<&str>,
    ) -> Result<(String, PathBuf), String> {
        let preferred = requested
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .or_else(|| preferred_media_id(kind));
        let g = self.inner.lock().unwrap();
        if let Some(id) = preferred.filter(|s| !s.is_empty()) {
            let m = g
                .models
                .get(&id)
                .ok_or_else(|| format!("modèle média inconnu: {id}"))?;
            if !m.desc.is_media() {
                return Err(format!("{id} n'est pas un pack média"));
            }
            let path = m
                .path
                .clone()
                .ok_or_else(|| format!("pas de poids pour {id}"))?;
            return Ok((id, path));
        }
        let mut found: Option<(String, PathBuf)> = None;
        for (id, m) in &g.models {
            if !m.desc.is_media() {
                continue;
            }
            let Some(path) = &m.path else {
                continue;
            };
            let matches = match kind {
                "image" => {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("");
                    matches!(ext, "safetensors" | "gguf" | "ckpt") || id.contains("sd-")
                }
                "tts" => {
                    path.extension().and_then(|e| e.to_str()) == Some("onnx")
                        || id.contains("piper")
                }
                _ => false,
            };
            if matches {
                found = Some((id.clone(), path.clone()));
                break;
            }
        }
        found.ok_or_else(|| {
            format!("aucun pack {kind} installé — téléchargez-le depuis Models (P08.6)")
        })
    }

    pub fn media_gen_begin(&self, model_id: &str, total_steps: u32) {
        let mut g = self.inner.lock().unwrap();
        let Some(m) = g.models.get_mut(model_id) else {
            return;
        };
        m.active = m.active.saturating_add(1);
        m.media_step = Some(0);
        m.media_total_steps = Some(total_steps.max(1));
        m.media_started_ms = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        );
    }

    pub fn media_gen_progress(&self, model_id: &str, step: u32, total: u32) {
        let mut g = self.inner.lock().unwrap();
        let Some(m) = g.models.get_mut(model_id) else {
            return;
        };
        m.media_step = Some(step);
        m.media_total_steps = Some(total.max(1));
        if let Some(start) = m.media_started_ms {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(start);
            let elapsed = (now.saturating_sub(start)) as f64 / 1000.0;
            if elapsed > 0.0 && step > 0 {
                m.last_step_s = Some(step as f64 / elapsed);
            }
        }
    }

    pub fn media_gen_end(&self, model_id: &str) {
        let mut g = self.inner.lock().unwrap();
        let Some(m) = g.models.get_mut(model_id) else {
            return;
        };
        m.active = m.active.saturating_sub(1);
        m.media_step = None;
        m.media_total_steps = None;
        m.media_started_ms = None;
    }

    pub fn has_live_infer(&self) -> bool {
        let g = self.inner.lock().unwrap();
        g.models
            .values()
            .any(|m| m.active > 0 || m.pending > 0)
    }
}

fn preferred_media_id(kind: &str) -> Option<String> {
    let home = std::env::var("AOS_HOME").ok()?;
    let raw = std::fs::read_to_string(std::path::PathBuf::from(home).join("var/run/preferences.json"))
        .ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let key = match kind {
        "image" => "default_image_model",
        "tts" => "default_audio_model",
        _ => return None,
    };
    v.get(key)
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::resume_messages;
    use aos_llama::StopReason;

    #[test]
    fn prefix_replay_keeps_history_and_appends_assistant() {
        let msgs = vec![
            ("system".into(), "tu es un assistant".into()),
            ("user".into(), "bonjour".into()),
        ];
        let out = resume_messages(&msgs, "Salut");
        assert_eq!(out.len(), 3);
        assert_eq!(out[2], ("assistant".into(), "Salut".into()));
        let same = resume_messages(&msgs, "");
        assert_eq!(same, msgs);
    }

    #[test]
    fn paused_is_not_cancelled_reason() {
        assert_ne!(StopReason::Paused, StopReason::Aborted);
    }
}
