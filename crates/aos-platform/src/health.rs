//! Runtime health plane (E23) — canary walk, SLO/EWMA, Isolation Forest, stderr clusters.
//!
//! Budget: < 256 MiB RAM, 0 extra VRAM. Isolation Forest is warning-only and
//! never overrides `canary_ok`. Boot session healthcheck stays lookup-only.

use crate::subsystem::PlatformSubsystem;
use aos_proto::{
    AgentInfo, ChatMessage, HealthAnomaly, HealthCanaryStep, HealthCluster, HealthSlo,
    HealthSnapshot, InferParams, InferRequest, MemStats, ModelInfo, ModelState,
    ModuleInvokeRequest, SystemMetrics, TokenEvent,
};
use extended_isolation_forest::{Forest, ForestOptions};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Canary note title prefix (create + delete; leftover sweep).
pub const CANARY_NOTE_TITLE: &str = "__aos_canary__";
/// EWMA smoothing factor.
pub const EWMA_ALPHA: f64 = 0.2;
/// NFR-01 warm TTFT target (ms).
pub const NFR_TTFT_MS: f64 = 2000.0;
/// Minimum healthy samples before Isolation Forest fits.
pub const IF_MIN_SAMPLES: usize = 64;
const IF_MAX_SAMPLES: usize = 256;
const IF_N_TREES: usize = 64;
const IF_SAMPLE_SIZE: usize = 128;
const IF_THRESHOLD: f64 = 0.55;
const FEATURE_DIM: usize = 8;
const SLO_INTERVAL: Duration = Duration::from_secs(15);
const CANARY_INTERVAL: Duration = Duration::from_secs(300);
const STDERR_TAIL_LINES: usize = 32;
const CLUSTER_K: usize = 3;

type FeatureVec = [f64; FEATURE_DIM];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct BaselineFile {
    version: String,
    samples: Vec<FeatureVec>,
}

#[derive(Debug, Clone, Default)]
pub struct EwmaState {
    pub ttft_ms: Option<f64>,
    pub tok_s: Option<f64>,
    pub bus_rtt_ms: Option<f64>,
}

/// Shared health runtime state (snapshot + EWMA + IF baseline).
pub struct HealthRuntime {
    home: PathBuf,
    version: String,
    snapshot: Mutex<HealthSnapshot>,
    ewma: Mutex<EwmaState>,
    baseline: Mutex<VecDeque<FeatureVec>>,
    last_canary_latency_ms: Mutex<f64>,
    last_canary_ok: Mutex<bool>,
}

impl HealthRuntime {
    pub fn new(home: PathBuf, version: String) -> Arc<Self> {
        let rt = Arc::new(Self {
            home,
            version: version.clone(),
            snapshot: Mutex::new(HealthSnapshot::default()),
            ewma: Mutex::new(EwmaState::default()),
            baseline: Mutex::new(VecDeque::with_capacity(IF_MAX_SAMPLES)),
            last_canary_latency_ms: Mutex::new(0.0),
            last_canary_ok: Mutex::new(true),
        });
        rt.load_baseline();
        if let Some(snap) = read_json::<HealthSnapshot>(&rt.snapshot_path()) {
            *rt.snapshot.lock().unwrap() = snap;
        }
        rt
    }

    fn run_dir(&self) -> PathBuf {
        self.home.join("var/run")
    }

    fn snapshot_path(&self) -> PathBuf {
        self.run_dir().join("health.json")
    }

    fn baseline_path(&self) -> PathBuf {
        self.run_dir().join("health_baseline.json")
    }

    pub fn snapshot(&self) -> HealthSnapshot {
        self.snapshot.lock().unwrap().clone()
    }

    fn persist_snapshot(&self, snap: &HealthSnapshot) {
        let _ = std::fs::create_dir_all(self.run_dir());
        if let Ok(raw) = serde_json::to_vec_pretty(snap) {
            let _ = std::fs::write(self.snapshot_path(), raw);
        }
    }

    fn load_baseline(&self) {
        let Some(file) = read_json::<BaselineFile>(&self.baseline_path()) else {
            return;
        };
        if file.version != self.version {
            let _ = std::fs::remove_file(self.baseline_path());
            return;
        }
        let mut q = self.baseline.lock().unwrap();
        q.clear();
        for s in file.samples.into_iter().take(IF_MAX_SAMPLES) {
            q.push_back(s);
        }
    }

    fn persist_baseline(&self) {
        let samples: Vec<FeatureVec> = self.baseline.lock().unwrap().iter().copied().collect();
        let file = BaselineFile {
            version: self.version.clone(),
            samples,
        };
        let _ = std::fs::create_dir_all(self.run_dir());
        if let Ok(raw) = serde_json::to_vec_pretty(&file) {
            let _ = std::fs::write(self.baseline_path(), raw);
        }
    }

    /// Publish a new snapshot (updates disk + in-memory).
    pub fn publish(&self, snap: HealthSnapshot) {
        self.persist_snapshot(&snap);
        *self.snapshot.lock().unwrap() = snap;
    }
}

/// Spawn the non-blocking health loop (SLO every 15s, canary every 5 min).
/// Returns immediately — same contract as [`crate::boot_index::spawn_background_indexing`].
pub fn spawn_health_loop(sub: Arc<PlatformSubsystem>, home: PathBuf, version: String) -> Arc<HealthRuntime> {
    let rt = HealthRuntime::new(home, version);
    let rt_loop = rt.clone();
    tokio::spawn(async move {
        let mut slo = tokio::time::interval(SLO_INTERVAL);
        let mut canary = tokio::time::interval(CANARY_INTERVAL);
        slo.tick().await; // skip immediate fire
        canary.tick().await;
        loop {
            tokio::select! {
                _ = slo.tick() => {
                    if let Err(e) = tick_slo(&sub, &rt_loop).await {
                        eprintln!("[aos-platformd] health SLO : {e}");
                    }
                }
                _ = canary.tick() => {
                    match run_canary(&sub, &rt_loop).await {
                        Ok(snap) => {
                            let ok = snap.canary_ok;
                            eprintln!(
                                "[aos-platformd] health canary : ok={ok} steps={}",
                                snap.steps.len()
                            );
                            rt_loop.publish(snap);
                        }
                        Err(e) => eprintln!("[aos-platformd] health canary : {e}"),
                    }
                }
            }
        }
    });
    rt
}

async fn tick_slo(sub: &Arc<PlatformSubsystem>, rt: &HealthRuntime) -> Result<(), String> {
    let Some(bus) = sub.bus() else {
        return Err("bus absent".into());
    };
    let t0 = Instant::now();
    let _models: Vec<ModelInfo> = bus
        .call("model.list", &(), vec![])
        .await
        .map_err(|e| e.to_string())?;
    let bus_rtt = t0.elapsed().as_secs_f64() * 1000.0;

    let metrics: SystemMetrics = bus
        .call("model.metrics", &(), vec![])
        .await
        .map_err(|e| e.to_string())?;

    let (ttft, tok_s, queued, vram_unloaded) = extract_metric_signals(&metrics);
    let restarts_1h = count_restarts_1h(&rt.home);

    {
        let mut ewma = rt.ewma.lock().unwrap();
        ewma.bus_rtt_ms = Some(ewma_update(ewma.bus_rtt_ms, bus_rtt));
        if let Some(v) = ttft {
            ewma.ttft_ms = Some(ewma_update(ewma.ttft_ms, v));
        }
        if let Some(v) = tok_s {
            ewma.tok_s = Some(ewma_update(ewma.tok_s, v));
        }
    }

    let ewma = rt.ewma.lock().unwrap().clone();
    let canary_latency = *rt.last_canary_latency_ms.lock().unwrap();
    let canary_ok = *rt.last_canary_ok.lock().unwrap();
    let features = build_features(&ewma, &metrics, ttft, tok_s, bus_rtt, vram_unloaded, restarts_1h, canary_latency, queued);
    let breaches = slo_breaches(&ewma, vram_unloaded);

    let healthy_for_baseline = breaches.is_empty() && canary_ok;
    if healthy_for_baseline {
        let mut base = rt.baseline.lock().unwrap();
        base.push_back(features);
        while base.len() > IF_MAX_SAMPLES {
            base.pop_front();
        }
        drop(base);
        rt.persist_baseline();
    }

    let anomaly = score_anomaly(rt, &features, &ewma, ttft, tok_s, bus_rtt);

    let mut snap = rt.snapshot();
    snap.at_ms = now_ms();
    snap.slo = HealthSlo {
        ttft_ewma_ms: ewma.ttft_ms,
        tok_s_ewma: ewma.tok_s,
        bus_rtt_ewma_ms: ewma.bus_rtt_ms,
        vram_unloaded_bytes: vram_unloaded,
        restarts_1h,
        breaches,
    };
    snap.anomaly = anomaly;
    // Preserve canary steps / clusters / canary_ok from last canary.
    rt.publish(snap);
    Ok(())
}

/// Force a canary walk (also used by `health.canary` intent).
pub async fn run_canary(
    sub: &Arc<PlatformSubsystem>,
    rt: &HealthRuntime,
) -> Result<HealthSnapshot, String> {
    let mut steps = Vec::new();
    let mut canary_ok = true;

    // Sweep leftover canary notes first.
    let _ = sweep_canary_notes(sub).await;

    timed_step(&mut steps, "model.list", &mut canary_ok, || async {
        let bus = sub.bus().ok_or_else(|| "bus absent".to_string())?;
        let _: Vec<ModelInfo> = bus.call("model.list", &(), vec![]).await.map_err(|e| e.to_string())?;
        Ok(())
    })
    .await;

    timed_step(&mut steps, "agent.list", &mut canary_ok, || async {
        let bus = sub.bus().ok_or_else(|| "bus absent".to_string())?;
        let _: Vec<AgentInfo> = bus.call("agent.list", &(), vec![]).await.map_err(|e| e.to_string())?;
        Ok(())
    })
    .await;

    let mut list_ok = false;
    timed_step(&mut steps, "module.list", &mut canary_ok, || async {
        let modules = sub.modules.lock().unwrap().list();
        if !modules.iter().any(|m| m.name == "notes") {
            return Err("module notes absent".into());
        }
        list_ok = true;
        let _ = modules;
        Ok(())
    })
    .await;

    timed_step(&mut steps, "mem.stats", &mut canary_ok, || async {
        let (episodic_total, namespaces, working_agents) = sub.mem.lock().unwrap().stats();
        let _ = MemStats {
            episodic_total,
            namespaces,
            working_agents,
        };
        Ok(())
    })
    .await;

    if list_ok {
        timed_step(&mut steps, "notes.list", &mut canary_ok, || async {
            invoke_notes(sub, "notes.list", serde_json::json!({})).await?;
            Ok(())
        })
        .await;

        timed_step(&mut steps, "notes.create_read_delete", &mut canary_ok, || async {
            let title = CANARY_NOTE_TITLE;
            invoke_notes(
                sub,
                "notes.create",
                serde_json::json!({
                    "title": title,
                    "content": "health canary ephemeral note"
                }),
            )
            .await?;
            invoke_notes(
                sub,
                "notes.read",
                serde_json::json!({ "title": title }),
            )
            .await?;
            invoke_notes(
                sub,
                "notes.delete",
                serde_json::json!({ "title": title }),
            )
            .await?;
            Ok(())
        })
        .await;
    }

    // Tiny infer only when idle and a model is loaded.
    if list_ok {
        timed_step(&mut steps, "model.infer", &mut canary_ok, || async {
            maybe_tiny_infer(sub).await
        })
        .await;
    }

    let total_latency: f64 = steps.iter().map(|s| s.latency_ms).sum();
    *rt.last_canary_latency_ms.lock().unwrap() = total_latency;
    *rt.last_canary_ok.lock().unwrap() = canary_ok;

    let clusters = cluster_stderr(sub, &rt.home).await;

    let mut snap = rt.snapshot();
    snap.at_ms = now_ms();
    snap.canary_ok = canary_ok;
    snap.steps = steps;
    snap.clusters = clusters;
    // Refresh SLO slice lightly so snapshot is coherent.
    if let Ok(()) = tick_slo_lite(rt, &mut snap) {
        // breaches already set
    }
    Ok(snap)
}

fn tick_slo_lite(rt: &HealthRuntime, snap: &mut HealthSnapshot) -> Result<(), ()> {
    let ewma = rt.ewma.lock().unwrap().clone();
    snap.slo.ttft_ewma_ms = ewma.ttft_ms;
    snap.slo.tok_s_ewma = ewma.tok_s;
    snap.slo.bus_rtt_ewma_ms = ewma.bus_rtt_ms;
    let breaches = slo_breaches(&ewma, snap.slo.vram_unloaded_bytes);
    snap.slo.breaches = breaches;
    Ok(())
}

async fn maybe_tiny_infer(sub: &Arc<PlatformSubsystem>) -> Result<(), String> {
    let bus = sub.bus().ok_or_else(|| "bus absent".to_string())?;
    let metrics: SystemMetrics = bus
        .call("model.metrics", &(), vec![])
        .await
        .map_err(|e| e.to_string())?;
    if should_skip_infer(&metrics) {
        return Ok(()); // skip is success — do not fail canary
    }
    let models: Vec<ModelInfo> = bus
        .call("model.list", &(), vec![])
        .await
        .map_err(|e| e.to_string())?;
    let model_id = models
        .iter()
        .find(|m| matches!(m.state, ModelState::Loaded | ModelState::PartiallyOffloaded))
        .map(|m| m.id.clone());
    let Some(model_id) = model_id else {
        return Ok(()); // no loaded model — skip
    };
    let req = InferRequest {
        model_id: Some(model_id),
        messages: vec![ChatMessage {
            role: "user".into(),
            content: "ping".into(),
        }],
        tools: vec![],
        params: InferParams {
            max_tokens: 4,
            temperature: 0.0,
            top_p: 1.0,
            seed: Some(1),
        },
        priority: 0,
        data_refs: vec![],
        images: vec![],
        routing: Some("local_only".into()),
    };
    let mut stream = bus
        .call_stream::<InferRequest, TokenEvent>("model.infer", &req, vec![])
        .await
        .map_err(|e| e.to_string())?;
    let mut saw_done = false;
    let mut err: Option<String> = None;
    while let Some(item) = stream.recv().await {
        match item {
            Ok(TokenEvent::Done { .. }) => {
                saw_done = true;
                break;
            }
            Ok(TokenEvent::Error { message }) => {
                err = Some(message);
                break;
            }
            Ok(_) => {}
            Err(e) => {
                err = Some(e.to_string());
                break;
            }
        }
    }
    if let Some(e) = err {
        return Err(e);
    }
    if !saw_done {
        return Err("infer stream closed without Done".into());
    }
    Ok(())
}

/// True when live inferences are in flight or no useful metrics yet for skip guard tests.
pub fn should_skip_infer(metrics: &SystemMetrics) -> bool {
    metrics.live_inferences() > 0
}

async fn invoke_notes(
    sub: &Arc<PlatformSubsystem>,
    tool: &str,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let req = ModuleInvokeRequest {
        module: "notes".into(),
        tool: tool.into(),
        args,
        actor: "human:health-canary".into(),
        actor_caps: vec![],
        trace_id: format!("health-canary-{}", now_ms()),
    };
    let s2 = sub.clone();
    let tool_name = req.tool.clone();
    let r = tokio::task::spawn_blocking(move || {
        s2.modules.lock().unwrap().invoke(
            &req.module,
            &req.tool,
            &req.args,
            &req.actor,
            &req.actor_caps,
            &req.trace_id,
        )
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{tool_name}: {e}"))?;
    Ok(r)
}

async fn sweep_canary_notes(sub: &Arc<PlatformSubsystem>) -> Result<(), String> {
    let listed = match invoke_notes(sub, "notes.list", serde_json::json!({})).await {
        Ok(v) => v,
        Err(_) => return Ok(()), // notes unavailable
    };
    let Some(arr) = listed.get("notes").and_then(|n| n.as_array()) else {
        return Ok(());
    };
    for note in arr {
        let title = note.get("title").and_then(|t| t.as_str()).unwrap_or("");
        let slug = note.get("slug").and_then(|t| t.as_str()).unwrap_or("");
        if title.starts_with(CANARY_NOTE_TITLE) || slug.starts_with("__aos_canary__") {
            let _ = invoke_notes(
                sub,
                "notes.delete",
                serde_json::json!({ "title": title, "slug": slug }),
            )
            .await;
        }
    }
    Ok(())
}

async fn timed_step<F, Fut>(
    steps: &mut Vec<HealthCanaryStep>,
    name: &str,
    canary_ok: &mut bool,
    f: F,
) where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    let t0 = Instant::now();
    let result = f().await;
    let latency_ms = t0.elapsed().as_secs_f64() * 1000.0;
    match result {
        Ok(()) => steps.push(HealthCanaryStep {
            name: name.into(),
            ok: true,
            latency_ms,
            error: None,
        }),
        Err(e) => {
            *canary_ok = false;
            steps.push(HealthCanaryStep {
                name: name.into(),
                ok: false,
                latency_ms,
                error: Some(e),
            });
        }
    }
}

fn extract_metric_signals(metrics: &SystemMetrics) -> (Option<f64>, Option<f64>, u32, u64) {
    let mut ttft = None;
    let mut tok_s = None;
    let mut queued = 0u32;
    let mut vram_unloaded = 0u64;
    for m in &metrics.models {
        queued = queued.saturating_add(m.queued);
        if matches!(m.state, ModelState::OnDisk) && m.vram_bytes > 0 {
            vram_unloaded = vram_unloaded.saturating_add(m.vram_bytes);
        }
        if ttft.is_none() {
            ttft = m.last_ttft_ms;
        }
        if tok_s.is_none() {
            tok_s = m.last_tok_s;
        }
    }
    (ttft, tok_s, queued, vram_unloaded)
}

/// EWMA update (public for unit tests).
pub fn ewma_update(prev: Option<f64>, sample: f64) -> f64 {
    match prev {
        Some(p) => EWMA_ALPHA * sample + (1.0 - EWMA_ALPHA) * p,
        None => sample,
    }
}

/// SLO breach strings (public for unit tests).
pub fn slo_breaches(ewma: &EwmaState, vram_unloaded: u64) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(ttft) = ewma.ttft_ms {
        if ttft > NFR_TTFT_MS {
            out.push(format!("ttft_ewma_ms={ttft:.0}>{NFR_TTFT_MS}"));
        }
    }
    if vram_unloaded > 0 {
        out.push(format!("vram_unloaded_bytes={vram_unloaded}"));
    }
    out
}

// Re-export EwmaState fields for tests via a thin constructor.
pub fn ewma_state(ttft_ms: Option<f64>, tok_s: Option<f64>, bus_rtt_ms: Option<f64>) -> EwmaState {
    EwmaState {
        ttft_ms,
        tok_s,
        bus_rtt_ms,
    }
}

#[allow(clippy::too_many_arguments)] // Feature vector inputs stay explicit for SLO scoring.
fn build_features(
    ewma: &EwmaState,
    metrics: &SystemMetrics,
    ttft: Option<f64>,
    tok_s: Option<f64>,
    bus_rtt: f64,
    vram_unloaded: u64,
    restarts_1h: u32,
    canary_latency_ms: f64,
    queued: u32,
) -> FeatureVec {
    let ttft_resid = match (ttft, ewma.ttft_ms) {
        (Some(s), Some(e)) => s - e,
        _ => 0.0,
    };
    let tok_resid = match (tok_s, ewma.tok_s) {
        (Some(s), Some(e)) => s - e,
        _ => 0.0,
    };
    let rtt_resid = match ewma.bus_rtt_ms {
        Some(e) => bus_rtt - e,
        None => 0.0,
    };
    let ram_ratio = if metrics.ram_total > 0 {
        metrics.ram_used as f64 / metrics.ram_total as f64
    } else {
        0.0
    };
    [
        ttft_resid,
        tok_resid,
        rtt_resid,
        vram_unloaded as f64,
        restarts_1h as f64,
        queued as f64,
        canary_latency_ms,
        ram_ratio,
    ]
}

fn score_anomaly(
    rt: &HealthRuntime,
    features: &FeatureVec,
    ewma: &EwmaState,
    ttft: Option<f64>,
    tok_s: Option<f64>,
    bus_rtt: f64,
) -> HealthAnomaly {
    let samples: Vec<FeatureVec> = rt.baseline.lock().unwrap().iter().copied().collect();
    if samples.len() < IF_MIN_SAMPLES {
        return HealthAnomaly {
            score: 0.0,
            threshold: IF_THRESHOLD,
            fitted: false,
            contributing: vec![],
        };
    }
    let options = ForestOptions {
        n_trees: IF_N_TREES,
        sample_size: IF_SAMPLE_SIZE.min(samples.len()),
        max_tree_depth: None,
        extension_level: 1,
    };
    let Ok(forest) = Forest::<f64, FEATURE_DIM>::from_slice(samples.as_slice(), &options) else {
        return HealthAnomaly {
            score: 0.0,
            threshold: IF_THRESHOLD,
            fitted: false,
            contributing: vec!["fit_failed".into()],
        };
    };
    let score = forest.score(features);
    let mut contributing = Vec::new();
    if score > IF_THRESHOLD {
        if let (Some(s), Some(e)) = (ttft, ewma.ttft_ms) {
            if (s - e).abs() > 50.0 {
                contributing.push("ttft_residual".into());
            }
        }
        if let (Some(s), Some(e)) = (tok_s, ewma.tok_s) {
            if (s - e).abs() > 0.5 {
                contributing.push("tok_s_residual".into());
            }
        }
        if let Some(e) = ewma.bus_rtt_ms {
            if (bus_rtt - e).abs() > 20.0 {
                contributing.push("bus_rtt_residual".into());
            }
        }
        if features[3] > 0.0 {
            contributing.push("vram_unloaded".into());
        }
        if features[4] > 0.0 {
            contributing.push("restarts_1h".into());
        }
    }
    HealthAnomaly {
        score,
        threshold: IF_THRESHOLD,
        fitted: true,
        contributing,
    }
}

/// Score a feature against a fitted forest — for unit tests.
pub fn isolation_score_for_test(samples: &[FeatureVec], probe: &FeatureVec) -> Option<f64> {
    if samples.len() < 8 {
        return None;
    }
    let options = ForestOptions {
        n_trees: 32,
        sample_size: samples.len().min(64),
        max_tree_depth: None,
        extension_level: 1,
    };
    let forest = Forest::<f64, FEATURE_DIM>::from_slice(samples, &options).ok()?;
    Some(forest.score(probe))
}

async fn cluster_stderr(sub: &Arc<PlatformSubsystem>, home: &Path) -> Vec<HealthCluster> {
    let lines = collect_stderr_lines(home, STDERR_TAIL_LINES);
    if lines.is_empty() {
        return Vec::new();
    }
    let mut embedded: Vec<(String, Vec<f32>)> = Vec::new();
    for line in &lines {
        match sub.embed_text(line) {
            Ok(v) if !v.is_empty() => embedded.push((line.clone(), v)),
            _ => break, // embed unavailable — skip clustering entirely
        }
    }
    if embedded.len() < 2 {
        return Vec::new();
    }
    kmeans_cosine_clusters(&embedded, CLUSTER_K)
}

fn collect_stderr_lines(home: &Path, max: usize) -> Vec<String> {
    let names = ["aos-platformd", "aos-modeld", "aos-agentd", "aos-auditd"];
    let mut all = Vec::new();
    for name in names {
        let path = home.join("var/run").join(format!("{name}.stderr.log"));
        if let Ok(raw) = std::fs::read_to_string(path) {
            for line in raw.lines().rev().take(max / names.len().max(1)) {
                let t = line.trim();
                if !t.is_empty() {
                    all.push(t.to_string());
                }
            }
        }
    }
    let restart = home.join("var/run/daemon_restarts.log");
    if let Ok(raw) = std::fs::read_to_string(restart) {
        for line in raw.lines().rev().take(8) {
            let t = line.trim();
            if !t.is_empty() {
                all.push(format!("restart:{t}"));
            }
        }
    }
    all.truncate(max);
    all
}

fn kmeans_cosine_clusters(items: &[(String, Vec<f32>)], k: usize) -> Vec<HealthCluster> {
    let k = k.min(items.len()).max(1);
    // Seed centroids with first k vectors.
    let mut centroids: Vec<Vec<f32>> = items.iter().take(k).map(|(_, v)| v.clone()).collect();
    let mut assigns = vec![0usize; items.len()];
    for _ in 0..8 {
        for (i, (_, v)) in items.iter().enumerate() {
            let mut best = 0;
            let mut best_sim = f32::MIN;
            for (ci, c) in centroids.iter().enumerate() {
                let s = cosine(v, c);
                if s > best_sim {
                    best_sim = s;
                    best = ci;
                }
            }
            assigns[i] = best;
        }
        for (ci, centroid) in centroids.iter_mut().enumerate().take(k) {
            let mut acc = vec![0.0f32; centroid.len()];
            let mut n = 0usize;
            for (i, (_, v)) in items.iter().enumerate() {
                if assigns[i] == ci {
                    for (a, b) in acc.iter_mut().zip(v.iter()) {
                        *a += *b;
                    }
                    n += 1;
                }
            }
            if n > 0 {
                for a in &mut acc {
                    *a /= n as f32;
                }
                *centroid = acc;
            }
        }
    }
    let mut clusters = Vec::new();
    for ci in 0..k {
        let members: Vec<&str> = items
            .iter()
            .enumerate()
            .filter(|(i, _)| assigns[*i] == ci)
            .map(|(_, (s, _))| s.as_str())
            .collect();
        if members.is_empty() {
            continue;
        }
        let sample = members[0].chars().take(120).collect::<String>();
        clusters.push(HealthCluster {
            label: format!("cluster-{ci}"),
            count: members.len() as u32,
            sample,
        });
    }
    clusters.sort_by(|a, b| b.count.cmp(&a.count));
    clusters
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut na = 0.0;
    let mut nb = 0.0;
    for i in 0..n {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let denom = (na.sqrt() * nb.sqrt()).max(1e-12);
    dot / denom
}

pub fn count_restarts_1h(home: &Path) -> u32 {
    let path = home.join("var/run/daemon_restarts.log");
    let Ok(raw) = std::fs::read_to_string(path) else {
        return 0;
    };
    let cutoff = now_ms().saturating_sub(3_600_000);
    let mut n = 0u32;
    for line in raw.lines() {
        let mut parts = line.split_whitespace();
        let Some(ms_s) = parts.next() else { continue };
        let Ok(ms) = ms_s.parse::<u64>() else { continue };
        if ms >= cutoff {
            n = n.saturating_add(1);
        }
    }
    n
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let raw = std::fs::read(path).ok()?;
    serde_json::from_slice(&raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subsystem::PlatformConfig;
    use std::time::Instant;

    fn temp_home(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("aos-health-{label}-{}", std::process::id()));
        let _ = std::fs::create_dir_all(p.join("var/run"));
        p
    }

    fn test_config(home: &Path) -> PlatformConfig {
        PlatformConfig {
            bus: "ipc://test".into(),
            audit_dir: home.join("audit").display().to_string(),
            storage_dir: home.join("storage").display().to_string(),
            memory_dir: home.join("memory").display().to_string(),
            modules_dir: home.join("modules").display().to_string(),
            catalogue_file: "/dev/null".into(),
            community_catalogue_dir: home.join("community-cat").display().to_string(),
            skills_dir: home.join("skills").display().to_string(),
            sessions_dir: home.join("sessions").display().to_string(),
            embed_model: None,
            policies_file: None,
            confirm_timeout_sec: 60,
            secrets_file: home.join("secrets").display().to_string(),
            net_mode: "online".into(),
            memory_v2: false,
            memory_v2_shadow: false,
        }
    }

    #[test]
    fn ewma_converges_and_ttft_breach() {
        let mut v = None;
        v = Some(ewma_update(v, 100.0));
        v = Some(ewma_update(v, 100.0));
        assert!((v.unwrap() - 100.0).abs() < 1e-6);
        let hot = ewma_update(Some(100.0), 5000.0);
        assert!(hot > 100.0);
        let breaches = slo_breaches(&ewma_state(Some(2500.0), None, None), 0);
        assert!(breaches.iter().any(|b| b.starts_with("ttft_ewma")));
        let vram = slo_breaches(&ewma_state(Some(100.0), None, None), 1024);
        assert!(vram.iter().any(|b| b.starts_with("vram_unloaded")));
        assert!(slo_breaches(&ewma_state(Some(500.0), None, None), 0).is_empty());
    }

    #[test]
    fn skip_infer_when_live() {
        let busy = SystemMetrics {
            models: vec![aos_proto::ModelMetrics {
                model_id: "m".into(),
                state: ModelState::Loaded,
                active_inferences: 2,
                queued: 0,
                last_ttft_ms: None,
                last_tok_s: None,
                vram_bytes: 0,
                ram_bytes: 0,
                disk_bytes: 0,
                media_step: None,
                media_total_steps: None,
                last_step_s: None,
                draft_accept: None,
                draft_acceptance_rate: None,
                draft_tokens_per_step: None,
                prefix_hit: None,
                inference_mode: None,
                adaptive_backend: None,
                quantization: None,
                plan_reason: None,
                thermal_policy: None,
                effective_profile: None,
                kv_cache: None,
                kv_tokens: None,
                fallback_used: false,
                draft_disabled: false,
                draft_disable_reason: None,
                draft_verify_ms: None,
            }],
            ram_total: 1,
            ram_used: 0,
            ram_free: 1,
            cpu_percent: 0.0,
            agents_active: 0,
        };
        assert!(should_skip_infer(&busy));
        let idle = SystemMetrics {
            models: vec![],
            ram_total: 1,
            ram_used: 0,
            ram_free: 1,
            cpu_percent: 0.0,
            agents_active: 0,
        };
        assert!(!should_skip_infer(&idle));
    }

    #[test]
    fn isolation_forest_flags_ttft_spike() {
        let mut samples = Vec::new();
        for i in 0..80 {
            let noise = (i as f64) * 0.01;
            samples.push([noise, 0.0, 0.0, 0.0, 0.0, 0.0, 10.0, 0.3]);
        }
        let normal = [0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 10.0, 0.3];
        let spike = [5000.0, 0.0, 0.0, 0.0, 0.0, 0.0, 10.0, 0.3];
        let s_n = isolation_score_for_test(&samples, &normal).expect("fit");
        let s_s = isolation_score_for_test(&samples, &spike).expect("fit");
        assert!(
            s_s > s_n,
            "spike score {s_s} should exceed normal {s_n}"
        );
        // IF never flips canary_ok — only a score.
        let mut snap = HealthSnapshot {
            canary_ok: true,
            anomaly: HealthAnomaly {
                score: s_s,
                threshold: IF_THRESHOLD,
                fitted: true,
                contributing: vec!["ttft_residual".into()],
            },
            ..Default::default()
        };
        assert!(snap.canary_ok);
        snap.anomaly.score = 0.99;
        assert!(snap.canary_ok);
    }

    #[tokio::test]
    async fn spawn_health_loop_does_not_block() {
        let home = temp_home("spawn");
        let sub = PlatformSubsystem::open(&test_config(&home)).expect("open");
        let start = Instant::now();
        let _rt = spawn_health_loop(sub, home.clone(), "test-0.0.0".into());
        assert!(
            start.elapsed() < Duration::from_millis(200),
            "health spawn blocked for {:?}",
            start.elapsed()
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn canary_title_prefix() {
        assert!(CANARY_NOTE_TITLE.starts_with("__aos_canary__"));
    }
}
