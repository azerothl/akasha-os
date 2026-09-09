//! `aos-modeld` — daemon du Model Subsystem (P1.1–P1.3).
//!
//! Usage : `aos-modeld [config.yaml]` (défaut `demo/modeld.dev.yaml`).

use aos_ipc::{BusClient, BusService, StreamHandle};
use aos_model::{media, providers, ModelSubsystem, ModeldConfig};
use aos_placement::{
    AdapterExecutionPhase, BackendKind, DistributedWork, InferencePlanDiagnostic,
    LanActivationAssembly, LanChatMessage, LanCluster, LanDiscoveryAdvertisement,
    LanDiscoverySocket, LanNode, LanPairingRegistry, LanSessionKey, LanShardManifest,
    LanTcpListener, LanTcpTransport, LanWeightRange, LanWorkMessage, LanWorkPlan,
    LanWorkerRegistry, LayerPipelinePlan, LayerStage, NodeTrust, PlacementProfile, ThermalPolicy,
};
use aos_proto::{
    CancelRequest, InferRequest, LanClusterAssignment, LanClusterDiscoverRequest,
    LanClusterDispatchRequest, LanClusterDispatchResponse, LanClusterInferChatRequest,
    LanClusterInferChatResponse, LanClusterInferTokensRequest, LanClusterInferTokensResponse,
    LanClusterJobRequest, LanClusterKvTransferRequest, LanClusterKvTransferResponse,
    LanClusterLayerInferRequest, LanClusterLayerInferResponse, LanClusterLayerPipelineRequest,
    LanClusterLayerPipelineResponse, LanClusterLayerPipelineStatusResponse, LanClusterLayerStage,
    LanClusterNode, LanClusterNodeRequest, LanClusterNodesResponse, LanClusterPairRequest,
    LanClusterPlanRequest, LanClusterPlanResponse, LanClusterStageLocalModelRequest,
    LanClusterStageLocalModelResponse, LanClusterWeightTransferRequest,
    LanClusterWeightTransferResponse, LoadRequest, MediaAudioGenerateRequest,
    MediaImageGenerateRequest, MediaImageUpscaleRequest, MigrateRequest,
    ModelAdapterExecuteRequest, ModelAdapterExecuteResponse, ModelAdapterStatusRequest,
    ModelAdapterStatusResponse, ModelIdRequest, ModelPlanDiagnostic, ModelPlanRequest, TokenEvent,
    UnloadRequest,
};
use aos_registry::ModelRegistry;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

fn parse_profile(s: &str) -> PlacementProfile {
    match s {
        "latency" => PlacementProfile::Latency,
        "memory-saver" => PlacementProfile::MemorySaver,
        "cpu-only" => PlacementProfile::CpuOnly,
        _ => PlacementProfile::Balanced,
    }
}

fn placement_name(profile: PlacementProfile) -> &'static str {
    match profile {
        PlacementProfile::Latency => "latency",
        PlacementProfile::Balanced => "balanced",
        PlacementProfile::MemorySaver => "memory-saver",
        PlacementProfile::CpuOnly => "cpu-only",
    }
}

fn backend_name(backend: BackendKind) -> &'static str {
    match backend {
        BackendKind::Cpu => "cpu",
        BackendKind::Cuda => "cuda",
        BackendKind::Metal => "metal",
        BackendKind::Npu => "npu",
        BackendKind::WebGpu => "webgpu",
        BackendKind::Lan => "lan",
    }
}

fn endpoint_is_loopback(endpoint: &str) -> bool {
    let address = endpoint
        .strip_prefix("tcp://")
        .or_else(|| endpoint.strip_prefix("akasha://"))
        .unwrap_or_default();
    address.starts_with("127.0.0.1:")
        || address.starts_with("localhost:")
        || address.starts_with("[::1]:")
}

fn thermal_name(policy: ThermalPolicy) -> &'static str {
    match policy {
        ThermalPolicy::Performance => "performance",
        ThermalPolicy::Balanced => "balanced",
        ThermalPolicy::Quiet => "quiet",
        ThermalPolicy::AlwaysOn => "always-on",
    }
}

fn last_token_activation(input: &[u8]) -> Result<Vec<u8>, String> {
    let tensor = aos_placement::F32Tensor::decode(input)?;
    let hidden = tensor
        .shape
        .first()
        .copied()
        .ok_or("activation finale vide")? as usize;
    let tokens = tensor
        .shape
        .get(1)
        .copied()
        .ok_or("activation finale non matricielle")? as usize;
    if tokens == 0 {
        return Err("activation finale sans token".into());
    }
    let values = (0..hidden)
        .map(|row| tensor.values[row * tokens + tokens - 1])
        .collect();
    aos_placement::F32Tensor::new(vec![hidden as u32, 1], values).map(|last| last.encode())
}

fn greedy_token(logits: &[f32]) -> Result<u32, String> {
    let (index, _) = logits
        .iter()
        .enumerate()
        .filter(|(_, value)| value.is_finite())
        .max_by(|(_, left), (_, right)| left.total_cmp(right))
        .ok_or("logits invalides ou vides")?;
    u32::try_from(index).map_err(|_| "index de token trop grand".into())
}

fn sample_token(
    logits: &[f32],
    temperature: f32,
    top_p: f32,
    state: &mut u64,
) -> Result<u32, String> {
    if !temperature.is_finite()
        || !top_p.is_finite()
        || !(0.0..=5.0).contains(&temperature)
        || top_p <= 0.0
        || top_p > 1.0
    {
        return Err("paramètres de sampling invalides".into());
    }
    if temperature == 0.0 {
        return greedy_token(logits);
    }
    let max = logits
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .max_by(f32::total_cmp)
        .ok_or("logits invalides ou vides")?;
    let mut probabilities: Vec<(usize, f32)> = logits
        .iter()
        .enumerate()
        .filter_map(|(index, value)| {
            value
                .is_finite()
                .then_some((index, ((*value - max) / temperature).exp()))
        })
        .collect();
    let total: f32 = probabilities.iter().map(|(_, value)| *value).sum();
    if !total.is_finite() || total <= 0.0 {
        return greedy_token(logits);
    }
    for (_, probability) in &mut probabilities {
        *probability /= total;
    }
    probabilities.sort_by(|left, right| right.1.total_cmp(&left.1));
    let mut cumulative = 0.0;
    let mut cutoff = probabilities.len();
    for (index, (_, probability)) in probabilities.iter().enumerate() {
        cumulative += probability;
        if cumulative >= top_p {
            cutoff = index + 1;
            break;
        }
    }
    let draw = {
        *state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        ((*state >> 11) as f64 / (1u64 << 53) as f64) as f32
    };
    let mut remaining = draw;
    for (index, probability) in probabilities.into_iter().take(cutoff) {
        if remaining <= probability {
            return u32::try_from(index).map_err(|_| "index de token trop grand".into());
        }
        remaining -= probability;
    }
    greedy_token(logits)
}

fn plan_diagnostic_row(row: InferencePlanDiagnostic) -> ModelPlanDiagnostic {
    ModelPlanDiagnostic {
        requested_profile: placement_name(row.requested_profile).into(),
        backend: backend_name(row.plan.backend).into(),
        quantization: row.plan.quantization.as_str().into(),
        placement: placement_name(row.plan.placement).into(),
        kv_cache: format!("{:?}", row.plan.kv_cache).to_ascii_lowercase(),
        kv_tokens: row.plan.kv_tokens,
        speculative: format!("{:?}", row.plan.speculative).to_ascii_lowercase(),
        thermal_policy: thermal_name(row.plan.thermal_policy).into(),
        experimental: row.plan.experimental,
        feasible: row.feasible,
        placement_summary: row.placement_summary,
        error: row.error,
    }
}

fn lan_state_name(state: aos_placement::LanJobState) -> String {
    format!("{state:?}").to_ascii_lowercase()
}

fn lan_plan_response(
    plan: &LanWorkPlan,
    reassigned_shards: Vec<u32>,
    cancelled_nodes: Vec<String>,
) -> LanClusterPlanResponse {
    LanClusterPlanResponse {
        work_id: plan.work_id.clone(),
        state: lan_state_name(plan.state),
        assignments: plan
            .assignments
            .iter()
            .map(|assignment| LanClusterAssignment {
                node_id: assignment.node_id.clone(),
                shard_ids: assignment.shard_ids.clone(),
                kv_tokens: assignment.kv_tokens,
                encrypted_transport: assignment.encrypted_transport,
            })
            .collect(),
        unassigned_shards: plan.unassigned_shards.clone(),
        reassigned_shards,
        cancelled_nodes,
        errors: Vec::new(),
    }
}

fn lan_registry_path(home: &std::path::Path) -> std::path::PathBuf {
    home.join("var/run/lan-pairing.json")
}

fn load_lan_registry(config: &ModeldConfig, home: &std::path::Path) -> LanPairingRegistry {
    if let Ok(raw) = std::fs::read_to_string(lan_registry_path(home)) {
        if let Ok(mut registry) = serde_json::from_str::<LanPairingRegistry>(&raw) {
            for node in config.lan_cluster.nodes.clone() {
                let _ = registry.try_discover(node);
            }
            return registry;
        }
    }
    LanPairingRegistry::from_nodes(config.lan_cluster.nodes.clone()).unwrap_or_default()
}

fn persist_lan_registry(
    registry: &LanPairingRegistry,
    home: &std::path::Path,
) -> Result<(), String> {
    let path = lan_registry_path(home);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("création du répertoire LAN: {e}"))?;
    }
    let raw = serde_json::to_vec_pretty(registry).map_err(|e| format!("sérialisation LAN: {e}"))?;
    std::fs::write(&path, raw).map_err(|e| format!("écriture de l’état LAN: {e}"))
}

fn lan_node_info(node: &LanNode) -> LanClusterNode {
    LanClusterNode {
        node_id: node.node_id.clone(),
        display_name: node.display_name.clone(),
        address: node.address.clone(),
        public_key_fingerprint: node.public_key_fingerprint.clone(),
        trust: format!("{:?}", node.trust).to_ascii_lowercase(),
        capabilities: node.capabilities.clone(),
    }
}

fn parse_lan_session_key(raw: &str) -> Result<LanSessionKey, String> {
    let raw = raw.trim();
    if raw.len() != 64 {
        return Err("la clé de session LAN doit contenir 64 caractères hexadécimaux".into());
    }
    let mut bytes = [0u8; 32];
    for (index, chunk) in raw.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let high = (chunk[0] as char)
            .to_digit(16)
            .ok_or("clé de session LAN non hexadécimale")?;
        let low = (chunk[1] as char)
            .to_digit(16)
            .ok_or("clé de session LAN non hexadécimale")?;
        bytes[index] = ((high << 4) | low) as u8;
    }
    LanSessionKey::from_bytes(&bytes)
}

async fn load_lan_session_key(bus: &BusClient, secret_name: &str) -> Result<LanSessionKey, String> {
    let secret = bus
        .call::<aos_proto::SecretGetRequest, String>(
            "secrets.get",
            &aos_proto::SecretGetRequest {
                name: secret_name.to_string(),
                actor: "service:modeld".into(),
            },
            vec![],
        )
        .await
        .map_err(|error| format!("clé LAN indisponible: {error}"))?;
    parse_lan_session_key(&secret)
}

async fn send_lan_cancel(
    registry: &LanPairingRegistry,
    work: &DistributedWork,
    node_ids: &[String],
    key: LanSessionKey,
) -> Vec<String> {
    let mut errors = Vec::new();
    for node_id in node_ids {
        let Some(node) = registry.get(node_id) else {
            errors.push(format!("{node_id}: nœud inconnu"));
            continue;
        };
        let mut transport = match LanTcpTransport::connect_authenticated(
            "coordinator",
            node_id,
            &node.address,
            registry,
            work,
            key.clone(),
        )
        .await
        {
            Ok(transport) => transport,
            Err(error) => {
                errors.push(format!("{node_id}: {error}"));
                continue;
            }
        };
        let message = LanWorkMessage::Cancel {
            work_id: work.work_id.clone(),
        };
        match transport.send_message(&message, work).await {
            Ok(()) => match transport.receive_message(work).await {
                Ok(LanWorkMessage::Ack { operation, .. }) if operation == "cancel" => {}
                Ok(other) => errors.push(format!("{node_id}: réponse LAN inattendue: {other:?}")),
                Err(error) => {
                    errors.push(format!("{node_id}: accusé annulation LAN absent: {error}"))
                }
            },
            Err(error) => errors.push(format!("{node_id}: {error}")),
        }
    }
    errors
}

async fn send_lan_assignments(
    registry: &LanPairingRegistry,
    work: &DistributedWork,
    assignments: &[aos_placement::LanShardAssignment],
    key: LanSessionKey,
) -> (Vec<String>, Vec<String>) {
    let mut dispatched = Vec::new();
    let mut errors = Vec::new();
    for assignment in assignments {
        let Some(node) = registry.get(&assignment.node_id) else {
            errors.push(format!("{}: nœud inconnu", assignment.node_id));
            continue;
        };
        let mut transport = match LanTcpTransport::connect_authenticated(
            "coordinator",
            &assignment.node_id,
            &node.address,
            registry,
            work,
            key.clone(),
        )
        .await
        {
            Ok(transport) => transport,
            Err(error) => {
                errors.push(format!("{}: {error}", assignment.node_id));
                continue;
            }
        };
        let message = LanWorkMessage::Assign {
            work_id: work.work_id.clone(),
            model_id: work.model_id.clone(),
            assignment: assignment.clone(),
            allow_sensitive_data: work.allow_sensitive_data,
        };
        match transport.send_message(&message, work).await {
            Ok(()) => match transport.receive_message(work).await {
                Ok(LanWorkMessage::Ack { operation, .. }) if operation == "assign" => {
                    dispatched.push(assignment.node_id.clone())
                }
                Ok(other) => errors.push(format!(
                    "{}: réponse LAN inattendue après assignment: {other:?}",
                    assignment.node_id
                )),
                Err(error) => errors.push(format!(
                    "{}: accusé assignment LAN absent: {error}",
                    assignment.node_id
                )),
            },
            Err(error) => errors.push(format!("{}: {error}", assignment.node_id)),
        }
    }
    (dispatched, errors)
}

async fn expect_lan_ack(
    transport: &mut LanTcpTransport,
    work: &DistributedWork,
    operation: &str,
) -> Result<(), String> {
    match transport.receive_message(work).await? {
        LanWorkMessage::Ack {
            operation: received,
            ..
        } if received == operation => Ok(()),
        LanWorkMessage::Nack { reason, .. } => Err(reason),
        other => Err(format!(
            "réponse LAN inattendue pour {operation}: {other:?}"
        )),
    }
}

#[allow(clippy::too_many_arguments)]
async fn send_lan_token_inference(
    local_node_id: &str,
    registry: &LanPairingRegistry,
    work: &DistributedWork,
    node_id: &str,
    assignment_shard_ids: Vec<u32>,
    request_id: String,
    input_tokens: Vec<u32>,
    max_tokens: u32,
    kv_tokens: u32,
    key: LanSessionKey,
) -> Result<(Vec<u32>, bool), String> {
    let node = registry.get(node_id).ok_or("nœud LAN inconnu")?;
    let assignment = aos_placement::LanShardAssignment {
        node_id: node_id.to_string(),
        shard_ids: assignment_shard_ids,
        kv_tokens,
        encrypted_transport: work.encrypted_transport,
    };
    let mut transport = LanTcpTransport::connect_authenticated(
        local_node_id,
        node_id,
        &node.address,
        registry,
        work,
        key,
    )
    .await?;
    transport
        .send_message(
            &LanWorkMessage::Assign {
                work_id: work.work_id.clone(),
                model_id: work.model_id.clone(),
                assignment,
                allow_sensitive_data: work.allow_sensitive_data,
            },
            work,
        )
        .await?;
    expect_lan_ack(&mut transport, work, "assign").await?;

    transport
        .send_message(
            &LanWorkMessage::Prefill {
                work_id: work.work_id.clone(),
                request_id: request_id.clone(),
                input_tokens,
                kv_tokens,
            },
            work,
        )
        .await?;
    expect_lan_ack(&mut transport, work, "prefill").await?;

    transport
        .send_message(
            &LanWorkMessage::Decode {
                work_id: work.work_id.clone(),
                request_id: request_id.clone(),
                max_tokens,
            },
            work,
        )
        .await?;
    match transport.receive_message(work).await? {
        LanWorkMessage::TokenBatch {
            request_id: received,
            tokens,
            finished,
            ..
        } if received == request_id => Ok((tokens, finished)),
        LanWorkMessage::Nack { reason, .. } => Err(reason),
        other => Err(format!("réponse LAN inattendue pour decode: {other:?}")),
    }
}

#[allow(clippy::too_many_arguments)]
async fn send_lan_chat_inference(
    local_node_id: &str,
    registry: &LanPairingRegistry,
    work: &DistributedWork,
    node_id: &str,
    assignment_shard_ids: Vec<u32>,
    request_id: String,
    messages: Vec<LanChatMessage>,
    max_tokens: u32,
    temperature_milli: u32,
    top_p_milli: u32,
    seed: u64,
    key: LanSessionKey,
) -> Result<(String, bool, u32, u32, f64, f64), String> {
    let node = registry.get(node_id).ok_or("nœud LAN inconnu")?;
    let assignment = aos_placement::LanShardAssignment {
        node_id: node_id.to_string(),
        shard_ids: assignment_shard_ids,
        kv_tokens: 0,
        encrypted_transport: work.encrypted_transport,
    };
    let mut transport = LanTcpTransport::connect_authenticated(
        local_node_id,
        node_id,
        &node.address,
        registry,
        work,
        key,
    )
    .await?;
    transport
        .send_message(
            &LanWorkMessage::Assign {
                work_id: work.work_id.clone(),
                model_id: work.model_id.clone(),
                assignment,
                allow_sensitive_data: work.allow_sensitive_data,
            },
            work,
        )
        .await?;
    expect_lan_ack(&mut transport, work, "assign").await?;
    transport
        .send_message(
            &LanWorkMessage::ChatInfer {
                work_id: work.work_id.clone(),
                request_id: request_id.clone(),
                messages,
                max_tokens,
                temperature_milli,
                top_p_milli,
                seed,
            },
            work,
        )
        .await?;
    match transport.receive_message(work).await? {
        LanWorkMessage::TextBatch {
            request_id: received,
            text,
            finished,
            prompt_tokens,
            generated_tokens,
            ttft_ms_milli,
            tok_s_milli,
            ..
        } if received == request_id => Ok((
            text,
            finished,
            prompt_tokens,
            generated_tokens,
            ttft_ms_milli as f64 / 1000.0,
            tok_s_milli as f64 / 1000.0,
        )),
        LanWorkMessage::Nack { reason, .. } => Err(reason),
        other => Err(format!(
            "réponse LAN inattendue pour l'inférence texte: {other:?}"
        )),
    }
}

const LAN_KV_PAGE_BYTES: usize = 512 * 1024;
const LAN_KV_MAX_BYTES: usize = 64 * 1024 * 1024;

struct KvPageAssembly {
    request_id: String,
    next_page: u32,
    data: Vec<u8>,
    seq0_tokens: Vec<i32>,
}

struct WeightPageAssembly {
    request_id: String,
    shard_id: u32,
    offset: u64,
    length: u64,
    total_model_bytes: u64,
    next_page: u32,
    data: Vec<u8>,
    required_ranges: Vec<LanWeightRange>,
}

fn lan_artifact_component(value: &str) -> String {
    let mut output = String::with_capacity(value.len().min(96));
    for byte in value.bytes().take(96) {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') {
            output.push(byte as char);
        } else {
            output.push('_');
        }
    }
    if output.is_empty() {
        output.push_str("unnamed");
    }
    output
}

#[allow(clippy::too_many_arguments)]
fn stage_lan_weight_shard(
    staging_root: &Path,
    work_id: &str,
    model_id: &str,
    shard_id: u32,
    offset: u64,
    total_model_bytes: u64,
    required_ranges: Vec<LanWeightRange>,
    data: &[u8],
) -> Result<(), String> {
    // Worker connections run on separate blocking tasks. Serialize publication
    // so their read/modify/write cycles cannot lose manifest entries.
    static STAGING_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _staging_guard = STAGING_LOCK
        .lock()
        .map_err(|_| "verrou staging poids LAN indisponible".to_string())?;
    if data.is_empty()
        || data.len() > LAN_KV_MAX_BYTES
        || offset
            .checked_add(data.len() as u64)
            .is_none_or(|end| end > total_model_bytes)
    {
        return Err("plage de poids LAN invalide".into());
    }
    let directory = staging_root
        .join("lan-shards")
        .join(lan_artifact_component(work_id));
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("création du staging poids LAN impossible: {error}"))?;
    let coverage_path = directory.join("manifest.json");
    let mut coverage = if coverage_path.exists() {
        let raw = std::fs::read(&coverage_path)
            .map_err(|error| format!("lecture du manifeste poids LAN impossible: {error}"))?;
        serde_json::from_slice::<LanShardManifest>(&raw)
            .map_err(|error| format!("manifeste poids LAN invalide: {error}"))?
    } else {
        LanShardManifest::new(model_id, total_model_bytes)?
    };
    if coverage.model_id != model_id || coverage.total_model_bytes != total_model_bytes {
        return Err("manifeste poids LAN incompatible avec le modèle".into());
    }
    if !required_ranges.is_empty() {
        if coverage.required_ranges.is_empty() {
            coverage.set_required_ranges(required_ranges)?;
        } else if coverage.required_ranges != required_ranges {
            return Err("exigences de shard LAN incompatibles".into());
        }
    }
    coverage.record_range(LanWeightRange {
        shard_id,
        offset,
        length: data.len() as u64,
    })?;
    let stem = format!("shard-{shard_id:05}-offset-{offset}");
    let partial = directory.join(format!("{stem}.part"));
    let final_path = directory.join(format!("{stem}.bin"));
    if final_path.exists() {
        let metadata = std::fs::metadata(&final_path)
            .map_err(|error| format!("lecture du shard existant impossible: {error}"))?;
        if metadata.len() != data.len() as u64 {
            return Err("conflit avec un shard de poids LAN existant".into());
        }
        let existing = std::fs::read(&final_path)
            .map_err(|error| format!("lecture du shard existant impossible: {error}"))?;
        if existing != data {
            return Err("conflit avec un shard de poids LAN existant".into());
        }
    } else {
        std::fs::write(&partial, data)
            .map_err(|error| format!("écriture du staging poids LAN impossible: {error}"))?;
        std::fs::rename(&partial, &final_path)
            .map_err(|error| format!("validation du staging poids LAN impossible: {error}"))?;
    }
    let coverage_partial = directory.join("manifest.json.part");
    let coverage_data = serde_json::to_vec_pretty(&coverage)
        .map_err(|error| format!("sérialisation du manifeste poids LAN impossible: {error}"))?;
    std::fs::write(&coverage_partial, coverage_data)
        .map_err(|error| format!("écriture du manifeste poids LAN impossible: {error}"))?;
    std::fs::rename(&coverage_partial, &coverage_path)
        .map_err(|error| format!("publication du manifeste poids LAN impossible: {error}"))?;
    let manifest = serde_json::json!({
        "model_id": model_id,
        "work_id": work_id,
        "shard_id": shard_id,
        "offset": offset,
        "length": data.len(),
        "total_model_bytes": total_model_bytes,
        "path": final_path.file_name().and_then(|name| name.to_str()).unwrap_or_default(),
    });
    let manifest_partial = directory.join(format!("{stem}.manifest.part"));
    let manifest_path = directory.join(format!("{stem}.manifest.json"));
    let manifest_data = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("métadonnées poids LAN invalides: {error}"))?;
    std::fs::write(&manifest_partial, manifest_data)
        .map_err(|error| format!("écriture métadonnées poids LAN impossible: {error}"))?;
    std::fs::rename(&manifest_partial, &manifest_path)
        .map_err(|error| format!("validation métadonnées poids LAN impossible: {error}"))?;
    Ok(())
}

/// Reassemble un GGUF only after the persisted coverage manifest is complete.
/// Every fragment is addressed by its manifest range; no directory listing or
/// user-controlled path is used to select bytes.
fn materialize_staged_lan_model(
    staging_root: &Path,
    work_id: &str,
    model_id: &str,
) -> Result<Option<PathBuf>, String> {
    let directory = staging_root
        .join("lan-shards")
        .join(lan_artifact_component(work_id));
    let manifest_path = directory.join("manifest.json");
    if !manifest_path.is_file() {
        return Ok(None);
    }
    let manifest: LanShardManifest = serde_json::from_slice(
        &std::fs::read(&manifest_path)
            .map_err(|error| format!("lecture du manifeste staging impossible: {error}"))?,
    )
    .map_err(|error| format!("manifeste staging LAN invalide: {error}"))?;
    if manifest.model_id != model_id {
        return Err("manifeste staging LAN associé à un autre modèle".into());
    }
    if !manifest.is_complete() {
        return Ok(None);
    }
    let output = directory.join("model.gguf");
    if output.is_file()
        && std::fs::metadata(&output)
            .map_err(|error| format!("lecture du GGUF staging impossible: {error}"))?
            .len()
            == manifest.total_model_bytes
    {
        return Ok(Some(output));
    }
    let temporary = directory.join("model.gguf.part");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| format!("création du GGUF staging impossible: {error}"))?;
    file.set_len(manifest.total_model_bytes)
        .map_err(|error| format!("réservation du GGUF staging impossible: {error}"))?;
    let mut ranges = manifest.ranges.clone();
    ranges.sort_by_key(|range| range.offset);
    for range in ranges {
        let fragment = directory.join(format!(
            "shard-{:05}-offset-{}.bin",
            range.shard_id, range.offset
        ));
        let data = std::fs::read(&fragment)
            .map_err(|error| format!("fragment GGUF staging absent: {error}"))?;
        if data.len() as u64 != range.length {
            return Err("fragment GGUF staging de taille inattendue".into());
        }
        file.seek(SeekFrom::Start(range.offset))
            .map_err(|error| format!("positionnement GGUF staging impossible: {error}"))?;
        file.write_all(&data)
            .map_err(|error| format!("écriture GGUF staging impossible: {error}"))?;
    }
    file.sync_all()
        .map_err(|error| format!("synchronisation GGUF staging impossible: {error}"))?;
    std::fs::rename(&temporary, &output)
        .map_err(|error| format!("publication GGUF staging impossible: {error}"))?;
    Ok(Some(output))
}

#[allow(clippy::too_many_arguments)]
async fn send_lan_kv_transfer(
    local_node_id: &str,
    registry: &LanPairingRegistry,
    work: &DistributedWork,
    source_node_id: &str,
    source_shards: Vec<u32>,
    target_node_id: &str,
    target_shards: Vec<u32>,
    request_id: String,
    key: LanSessionKey,
) -> Result<(u32, u64), String> {
    if source_node_id == target_node_id {
        return Err("source et cible KV LAN doivent être distinctes".into());
    }
    let source = registry
        .get(source_node_id)
        .ok_or("nœud source KV LAN inconnu")?;
    let source_assignment = aos_placement::LanShardAssignment {
        node_id: source_node_id.to_string(),
        shard_ids: source_shards,
        kv_tokens: 0,
        encrypted_transport: work.encrypted_transport,
    };
    let mut source_transport = LanTcpTransport::connect_authenticated(
        local_node_id,
        source_node_id,
        &source.address,
        registry,
        work,
        key.clone(),
    )
    .await?;
    source_transport
        .send_message(
            &LanWorkMessage::Assign {
                work_id: work.work_id.clone(),
                model_id: work.model_id.clone(),
                assignment: source_assignment,
                allow_sensitive_data: work.allow_sensitive_data,
            },
            work,
        )
        .await?;
    expect_lan_ack(&mut source_transport, work, "assign").await?;
    source_transport
        .send_message(
            &LanWorkMessage::KvRequest {
                work_id: work.work_id.clone(),
                request_id: request_id.clone(),
                seq_id: 0,
            },
            work,
        )
        .await?;
    let mut pages = Vec::new();
    let mut total_bytes = 0usize;
    loop {
        match source_transport.receive_message(work).await? {
            LanWorkMessage::KvPage {
                request_id: received,
                page_index,
                data,
                seq0_tokens,
                final_page,
                ..
            } if received == request_id => {
                if page_index != pages.len() as u32 {
                    return Err("pages KV LAN reçues hors ordre".into());
                }
                total_bytes = total_bytes.saturating_add(data.len());
                if total_bytes > LAN_KV_MAX_BYTES {
                    return Err("état KV LAN supérieur à 64 MiB".into());
                }
                pages.push(LanWorkMessage::KvPage {
                    work_id: work.work_id.clone(),
                    request_id: request_id.clone(),
                    page_index,
                    data,
                    seq0_tokens,
                    final_page,
                });
                if final_page {
                    break;
                }
            }
            LanWorkMessage::Nack { reason, .. } => return Err(reason),
            other => return Err(format!("réponse KV LAN inattendue: {other:?}")),
        }
    }

    let target = registry
        .get(target_node_id)
        .ok_or("nœud cible KV LAN inconnu")?;
    let target_assignment = aos_placement::LanShardAssignment {
        node_id: target_node_id.to_string(),
        shard_ids: target_shards,
        kv_tokens: 0,
        encrypted_transport: work.encrypted_transport,
    };
    let mut target_transport = LanTcpTransport::connect_authenticated(
        local_node_id,
        target_node_id,
        &target.address,
        registry,
        work,
        key,
    )
    .await?;
    target_transport
        .send_message(
            &LanWorkMessage::Assign {
                work_id: work.work_id.clone(),
                model_id: work.model_id.clone(),
                assignment: target_assignment,
                allow_sensitive_data: work.allow_sensitive_data,
            },
            work,
        )
        .await?;
    expect_lan_ack(&mut target_transport, work, "assign").await?;
    for page in &pages {
        target_transport.send_message(page, work).await?;
    }
    expect_lan_ack(&mut target_transport, work, "kv-page").await?;
    Ok((pages.len() as u32, total_bytes as u64))
}

#[allow(clippy::too_many_arguments)]
async fn send_lan_weight_shard(
    local_node_id: &str,
    registry: &LanPairingRegistry,
    work: &DistributedWork,
    source_node_id: &str,
    source_shards: Vec<u32>,
    target_node_id: &str,
    target_shards: Vec<u32>,
    request_id: String,
    shard_id: u32,
    offset: u64,
    length: u64,
    total_model_bytes: u64,
    key: LanSessionKey,
) -> Result<(u32, u64), String> {
    if source_node_id == target_node_id {
        return Err("source et cible poids LAN doivent être distinctes".into());
    }
    let source = registry
        .get(source_node_id)
        .ok_or("nœud source poids LAN inconnu")?;
    let source_assignment = aos_placement::LanShardAssignment {
        node_id: source_node_id.to_string(),
        shard_ids: source_shards,
        kv_tokens: 0,
        encrypted_transport: work.encrypted_transport,
    };
    let mut source_transport = LanTcpTransport::connect_authenticated(
        local_node_id,
        source_node_id,
        &source.address,
        registry,
        work,
        key.clone(),
    )
    .await?;
    source_transport
        .send_message(
            &LanWorkMessage::Assign {
                work_id: work.work_id.clone(),
                model_id: work.model_id.clone(),
                assignment: source_assignment,
                allow_sensitive_data: work.allow_sensitive_data,
            },
            work,
        )
        .await?;
    expect_lan_ack(&mut source_transport, work, "assign").await?;
    source_transport
        .send_message(
            &LanWorkMessage::WeightRequest {
                work_id: work.work_id.clone(),
                request_id: request_id.clone(),
                shard_id,
                offset,
                length,
                total_model_bytes,
            },
            work,
        )
        .await?;

    let mut pages = Vec::new();
    let mut received_bytes = 0u64;
    loop {
        match source_transport.receive_message(work).await? {
            LanWorkMessage::WeightPage {
                request_id: received,
                shard_id: received_shard,
                page_index,
                offset: page_offset,
                data,
                final_page,
                ..
            } if received == request_id && received_shard == shard_id => {
                if page_index != pages.len() as u32
                    || page_offset != offset.saturating_add(received_bytes)
                {
                    return Err("pages de poids LAN reçues hors ordre".into());
                }
                received_bytes = received_bytes.saturating_add(data.len() as u64);
                if received_bytes > length {
                    return Err("volume de poids LAN reçu supérieur à la demande".into());
                }
                if final_page && received_bytes != length {
                    return Err("page finale de poids LAN prématurée".into());
                }
                pages.push(LanWorkMessage::WeightPage {
                    work_id: work.work_id.clone(),
                    request_id: request_id.clone(),
                    shard_id,
                    page_index,
                    offset: page_offset,
                    data,
                    final_page,
                });
                if final_page {
                    break;
                }
            }
            LanWorkMessage::Nack { reason, .. } => return Err(reason),
            other => return Err(format!("réponse poids LAN inattendue: {other:?}")),
        }
    }

    let target = registry
        .get(target_node_id)
        .ok_or("nœud cible poids LAN inconnu")?;
    let target_assignment = aos_placement::LanShardAssignment {
        node_id: target_node_id.to_string(),
        shard_ids: target_shards,
        kv_tokens: 0,
        encrypted_transport: work.encrypted_transport,
    };
    let mut target_transport = LanTcpTransport::connect_authenticated(
        local_node_id,
        target_node_id,
        &target.address,
        registry,
        work,
        key,
    )
    .await?;
    target_transport
        .send_message(
            &LanWorkMessage::Assign {
                work_id: work.work_id.clone(),
                model_id: work.model_id.clone(),
                assignment: target_assignment,
                allow_sensitive_data: work.allow_sensitive_data,
            },
            work,
        )
        .await?;
    expect_lan_ack(&mut target_transport, work, "assign").await?;
    target_transport
        .send_message(
            &LanWorkMessage::WeightBegin {
                work_id: work.work_id.clone(),
                request_id: request_id.clone(),
                shard_id,
                offset,
                length,
                total_model_bytes,
                required_ranges: Vec::new(),
            },
            work,
        )
        .await?;
    expect_lan_ack(&mut target_transport, work, "weight-begin").await?;
    for page in &pages {
        target_transport.send_message(page, work).await?;
    }
    expect_lan_ack(&mut target_transport, work, "weight-shard").await?;
    Ok((pages.len() as u32, received_bytes))
}

/// Transfer the coordinator-local GGUF, either completely or as the bounded
/// sparse ranges required by one layer segment, to a paired worker. The
/// worker's lazy Assign path lets this happen before any llama.cpp context is
/// created.
async fn resolve_stage_weight_ranges(
    subsystem: &ModelSubsystem,
    model_id: &str,
    shard_id: u32,
    explicit: &[aos_proto::LanClusterWeightSegment],
    first_layer: Option<u32>,
    last_layer: Option<u32>,
) -> Result<Vec<LanWeightRange>, String> {
    if !explicit.is_empty() {
        return Ok(explicit
            .iter()
            .map(|range| LanWeightRange {
                shard_id: range.shard_id,
                offset: range.offset,
                length: range.length,
            })
            .collect());
    }
    let (total, ranges) = match (first_layer, last_layer) {
        (Some(first), Some(last)) => {
            subsystem
                .worker_layer_weight_ranges(model_id, first, last)
                .await?
        }
        (None, None) => return Ok(Vec::new()),
        _ => return Err("bornes de couches GGUF incomplètes".into()),
    };
    if total == 0 || ranges.is_empty() {
        return Err("aucune plage GGUF pour le segment demandé".into());
    }
    let mut bounded = Vec::new();
    for (offset, length) in ranges {
        let mut cursor = offset;
        let end = offset.checked_add(length).ok_or("plage GGUF débordante")?;
        while cursor < end {
            let chunk = (end - cursor).min(LAN_KV_MAX_BYTES as u64);
            bounded.push(LanWeightRange {
                shard_id,
                offset: cursor,
                length: chunk,
            });
            cursor += chunk;
        }
    }
    Ok(bounded)
}

#[allow(clippy::too_many_arguments)]
async fn send_lan_local_weight_model(
    local_node_id: &str,
    registry: &LanPairingRegistry,
    work: &DistributedWork,
    model: &ModelSubsystem,
    target_node_id: &str,
    shard_id: u32,
    request_id: &str,
    required_ranges: Vec<LanWeightRange>,
    key: LanSessionKey,
) -> Result<(u32, u64), String> {
    let total = model.worker_weight_file_size(&work.model_id)?;
    let target = registry
        .get(target_node_id)
        .ok_or("nœud cible poids local LAN inconnu")?;
    let assignment = aos_placement::LanShardAssignment {
        node_id: target_node_id.to_string(),
        shard_ids: vec![shard_id],
        kv_tokens: 0,
        encrypted_transport: work.encrypted_transport,
    };
    let mut transport = LanTcpTransport::connect_authenticated(
        local_node_id,
        target_node_id,
        &target.address,
        registry,
        work,
        key,
    )
    .await?;
    transport
        .send_message(
            &LanWorkMessage::Assign {
                work_id: work.work_id.clone(),
                model_id: work.model_id.clone(),
                assignment,
                allow_sensitive_data: work.allow_sensitive_data,
            },
            work,
        )
        .await?;
    expect_lan_ack(&mut transport, work, "assign").await?;

    let sparse_transfer = !required_ranges.is_empty();
    let transfer_ranges = if !sparse_transfer {
        vec![LanWeightRange {
            shard_id,
            offset: 0,
            length: total,
        }]
    } else {
        let mut ranges = required_ranges;
        ranges.sort_by_key(|range| range.offset);
        if ranges.iter().any(|range| {
            range.shard_id != shard_id
                || range.length == 0
                || range.offset.saturating_add(range.length) > total
        }) {
            return Err("plages GGUF requises incompatibles avec le shard LAN".into());
        }
        ranges
    };
    let mut page_count = 0u32;
    let mut transferred_bytes = 0u64;
    for range in &transfer_ranges {
        let mut offset = range.offset;
        let range_end = range.offset + range.length;
        while offset < range_end {
            let length = (range_end - offset).min(LAN_KV_MAX_BYTES as u64);
            let (reported_total, data) = model
                .worker_read_weight_range(&work.model_id, offset, length)
                .await?;
            if reported_total != total || data.len() as u64 != length {
                return Err("taille GGUF locale incohérente pendant le staging".into());
            }
            let chunk_request = format!("{request_id}-{offset}");
            transport
                .send_message(
                    &LanWorkMessage::WeightBegin {
                        work_id: work.work_id.clone(),
                        request_id: chunk_request.clone(),
                        shard_id,
                        offset,
                        length,
                        total_model_bytes: total,
                        required_ranges: if !sparse_transfer {
                            Vec::new()
                        } else {
                            // The worker records this once and checks every
                            // subsequent fragment against the same contract.
                            transfer_ranges.clone()
                        },
                    },
                    work,
                )
                .await?;
            expect_lan_ack(&mut transport, work, "weight-begin").await?;
            for (index, page) in data.chunks(aos_placement::LAYER_RPC_PAGE_BYTES).enumerate() {
                let page_offset = offset + (index * aos_placement::LAYER_RPC_PAGE_BYTES) as u64;
                transport
                    .send_message(
                        &LanWorkMessage::WeightPage {
                            work_id: work.work_id.clone(),
                            request_id: chunk_request.clone(),
                            shard_id,
                            page_index: index as u32,
                            offset: page_offset,
                            data: page.to_vec(),
                            final_page: page_offset + page.len() as u64 == offset + length,
                        },
                        work,
                    )
                    .await?;
                page_count = page_count.saturating_add(1);
            }
            expect_lan_ack(&mut transport, work, "weight-shard").await?;
            transferred_bytes = transferred_bytes.saturating_add(length);
            offset += length;
        }
    }
    Ok((page_count, transferred_bytes))
}

#[allow(clippy::too_many_arguments)]
async fn send_lan_layer_activation(
    local_node_id: &str,
    registry: &LanPairingRegistry,
    work: &DistributedWork,
    assignment: &aos_placement::LanShardAssignment,
    request_id: String,
    shard_id: u32,
    layer_index: u32,
    sequence: u32,
    position_start: u32,
    activation: Vec<u8>,
    key: LanSessionKey,
) -> Result<Vec<u8>, String> {
    if activation.is_empty() || activation.len() > 64 * 1024 * 1024 {
        return Err("activation couche LAN hors limite".into());
    }
    if !assignment.shard_ids.contains(&shard_id) {
        return Err("shard absent de l'assignment LAN".into());
    }
    let node = registry
        .get(&assignment.node_id)
        .ok_or("nœud cible couche LAN inconnu")?;
    let mut transport = LanTcpTransport::connect_authenticated(
        local_node_id,
        &assignment.node_id,
        &node.address,
        registry,
        work,
        key,
    )
    .await?;
    transport
        .send_message(
            &LanWorkMessage::Assign {
                work_id: work.work_id.clone(),
                model_id: work.model_id.clone(),
                assignment: assignment.clone(),
                allow_sensitive_data: work.allow_sensitive_data,
            },
            work,
        )
        .await?;
    expect_lan_ack(&mut transport, work, "assign").await?;
    let total_bytes = activation.len() as u64;
    for (page_index, page) in activation
        .chunks(aos_placement::LAYER_RPC_PAGE_BYTES)
        .enumerate()
    {
        transport
            .send_message(
                &LanWorkMessage::LayerActivationPage {
                    work_id: work.work_id.clone(),
                    request_id: request_id.clone(),
                    shard_id,
                    layer_index,
                    sequence,
                    position_start,
                    page_index: page_index as u32,
                    total_bytes,
                    data: page.to_vec(),
                    final_page: (page_index + 1) * aos_placement::LAYER_RPC_PAGE_BYTES
                        >= activation.len(),
                },
                work,
            )
            .await?;
    }
    let mut output: Option<LanActivationAssembly> = None;
    loop {
        match transport.receive_message(work).await? {
            LanWorkMessage::LayerActivationResult {
                request_id: received_request,
                shard_id: received_shard,
                layer_index: received_layer,
                sequence: received_sequence,
                page_index,
                total_bytes,
                data,
                final_page,
                ..
            } if received_request == request_id
                && received_shard == shard_id
                && received_layer == layer_index
                && received_sequence == sequence =>
            {
                let assembly = if let Some(assembly) = output.as_mut() {
                    assembly
                } else {
                    output = Some(LanActivationAssembly::new(
                        request_id.clone(),
                        shard_id,
                        layer_index,
                        sequence,
                        total_bytes,
                    )?);
                    output.as_mut().unwrap()
                };
                if let Some(bytes) = assembly.push_page(page_index, &data, final_page)? {
                    return Ok(bytes);
                }
            }
            LanWorkMessage::Nack { reason, .. } => return Err(reason),
            other => return Err(format!("réponse couche LAN inattendue: {other:?}")),
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn send_lan_layer_pipeline(
    local_node_id: &str,
    registry: &LanPairingRegistry,
    work: &DistributedWork,
    assignments: &[(LanClusterLayerStage, aos_placement::LanShardAssignment)],
    request_id: String,
    sequence: u32,
    position_start: u32,
    mut activation: Vec<u8>,
    key: LanSessionKey,
) -> Result<(Vec<u8>, u32, u32), String> {
    if assignments.is_empty() || assignments.len() > 2048 {
        return Err("pipeline de couches LAN vide ou trop long".into());
    }
    let mut previous_layer: Option<u32> = None;
    let mut previous_node = None;
    let mut transfers = 0u32;
    for (index, (stage, assignment)) in assignments.iter().enumerate() {
        let expected_layer = previous_layer.map_or(0, |layer| layer.saturating_add(1));
        if stage.layer_index != expected_layer {
            return Err("pipeline LAN incomplet ou couches hors ordre".into());
        }
        if let Some(node) = previous_node {
            if node != stage.node_id {
                transfers = transfers.saturating_add(1);
            }
        }
        activation = send_lan_layer_activation(
            local_node_id,
            registry,
            work,
            assignment,
            request_id.clone(),
            stage.shard_id,
            stage.layer_index,
            sequence.saturating_add(index as u32),
            position_start,
            activation,
            key.clone(),
        )
        .await?;
        previous_layer = Some(stage.layer_index);
        previous_node = Some(stage.node_id.as_str());
    }
    Ok((activation, assignments.len() as u32, transfers))
}

fn lan_control_work(work_id: &str, shard_ids: Vec<u32>) -> DistributedWork {
    DistributedWork {
        work_id: work_id.to_string(),
        model_id: String::new(),
        shard_ids: if shard_ids.is_empty() {
            vec![0]
        } else {
            shard_ids
        },
        allow_sensitive_data: false,
        encrypted_transport: true,
    }
}

async fn handle_lan_worker_connection(
    mut transport: LanTcpTransport,
    local_node_id: String,
    subsystem: Arc<ModelSubsystem>,
    worker_registry: Arc<Mutex<LanWorkerRegistry>>,
    staging_root: PathBuf,
) -> Result<(), String> {
    let first = transport.receive_message_unchecked().await?;
    match first {
        LanWorkMessage::Assign {
            work_id,
            model_id,
            assignment,
            allow_sensitive_data,
        } => {
            let assigned_kv_tokens = assignment.kv_tokens;
            let work = DistributedWork {
                work_id: work_id.clone(),
                model_id: model_id.clone(),
                shard_ids: assignment.shard_ids.clone(),
                allow_sensitive_data,
                encrypted_transport: true,
            };
            LanWorkMessage::Assign {
                work_id: work_id.clone(),
                model_id: model_id.clone(),
                assignment: assignment.clone(),
                allow_sensitive_data,
            }
            .validate_for(&work, &local_node_id)?;
            let staged_model =
                match materialize_staged_lan_model(&staging_root, &work_id, &model_id) {
                    Ok(staged_model) => staged_model,
                    Err(error) => {
                        let _ = transport
                            .send_message(
                                &LanWorkMessage::Nack {
                                    work_id: work_id.clone(),
                                    operation: "assign".into(),
                                    reason: format!(
                                        "reconstruction GGUF staging impossible: {error}"
                                    ),
                                },
                                &work,
                            )
                            .await;
                        return Err(error);
                    }
                };
            if let Some(path) = &staged_model {
                if let Err(error) = subsystem.worker_register_staged_model(&model_id, path.clone())
                {
                    let _ = transport
                        .send_message(
                            &LanWorkMessage::Nack {
                                work_id: work_id.clone(),
                                operation: "assign".into(),
                                reason: error.clone(),
                            },
                            &work,
                        )
                        .await;
                    return Err(error);
                }
            }
            // Loading is deferred until an operation needs llama.cpp. This is
            // essential for weight staging: the target may not have a local
            // GGUF yet, while the independent Akasha layer path can run from
            // the reconstructed file without a full llama context.
            worker_registry
                .lock()
                .map_err(|_| "état worker LAN verrouillé".to_string())?
                .assign(
                    &local_node_id,
                    transport.peer_node_id(),
                    model_id.clone(),
                    assignment,
                    work_id.clone(),
                    allow_sensitive_data,
                )?;
            transport
                .send_message(
                    &LanWorkMessage::Ack {
                        work_id: work_id.clone(),
                        operation: "assign".into(),
                    },
                    &work,
                )
                .await?;

            let abort = Arc::new(std::sync::atomic::AtomicBool::new(false));
            worker_registry
                .lock()
                .map_err(|_| "état worker LAN verrouillé".to_string())?
                .register_abort(&work_id, abort.clone())?;
            let mut kv_assembly: Option<KvPageAssembly> = None;
            let mut weight_assembly: Option<WeightPageAssembly> = None;
            let mut activation_assembly: Option<LanActivationAssembly> = None;
            loop {
                let message = match transport.receive_message_unchecked().await {
                    Ok(message) => message,
                    Err(error) if error.contains("lecture de trame LAN") => return Ok(()),
                    Err(error) => return Err(error),
                };
                match message {
                    LanWorkMessage::Heartbeat { work_id: id } if id == work_id => {
                        transport
                            .send_message(
                                &LanWorkMessage::Ack {
                                    work_id: work_id.clone(),
                                    operation: "heartbeat".into(),
                                },
                                &work,
                            )
                            .await?;
                    }
                    LanWorkMessage::Cancel { work_id: id } if id == work_id => {
                        abort.store(true, std::sync::atomic::Ordering::SeqCst);
                        worker_registry
                            .lock()
                            .map_err(|_| "état worker LAN verrouillé".to_string())?
                            .cancel(&work_id)?;
                        transport
                            .send_message(
                                &LanWorkMessage::Ack {
                                    work_id: work_id.clone(),
                                    operation: "cancel".into(),
                                },
                                &work,
                            )
                            .await?;
                        return Ok(());
                    }
                    LanWorkMessage::Prefill {
                        work_id: id,
                        request_id,
                        input_tokens,
                        kv_tokens,
                    } if id == work_id => {
                        let message = LanWorkMessage::Prefill {
                            work_id: work_id.clone(),
                            request_id,
                            input_tokens: input_tokens.clone(),
                            kv_tokens,
                        };
                        message.validate_for(&work, transport.peer_node_id())?;
                        if kv_tokens > assigned_kv_tokens {
                            return Err("budget KV prefill supérieur à l'assignment".into());
                        }
                        let input_tokens: Vec<i32> = input_tokens
                            .into_iter()
                            .map(|token| {
                                i32::try_from(token)
                                    .map_err(|_| "identifiant de token LAN hors plage".to_string())
                            })
                            .collect::<Result<_, _>>()?;
                        worker_registry
                            .lock()
                            .map_err(|_| "état worker LAN verrouillé".to_string())?
                            .reset_abort(&work_id)?;
                        if let Err(error) = subsystem
                            .ensure_loaded(&model_id, PlacementProfile::Balanced, kv_tokens)
                            .await
                        {
                            transport
                                .send_message(
                                    &LanWorkMessage::Nack {
                                        work_id: work_id.clone(),
                                        operation: "prefill".into(),
                                        reason: error,
                                    },
                                    &work,
                                )
                                .await?;
                            continue;
                        }
                        match subsystem
                            .worker_prefill_tokens(&model_id, input_tokens)
                            .await
                        {
                            Ok(()) => {
                                transport
                                    .send_message(
                                        &LanWorkMessage::Ack {
                                            work_id: work_id.clone(),
                                            operation: "prefill".into(),
                                        },
                                        &work,
                                    )
                                    .await?;
                                if abort.load(std::sync::atomic::Ordering::SeqCst) {
                                    return Ok(());
                                }
                            }
                            Err(error) => {
                                transport
                                    .send_message(
                                        &LanWorkMessage::Nack {
                                            work_id: work_id.clone(),
                                            operation: "prefill".into(),
                                            reason: error,
                                        },
                                        &work,
                                    )
                                    .await?;
                            }
                        }
                        if abort.load(std::sync::atomic::Ordering::SeqCst) {
                            return Ok(());
                        }
                    }
                    LanWorkMessage::KvRequest {
                        work_id: id,
                        request_id,
                        seq_id,
                    } if id == work_id => {
                        let message = LanWorkMessage::KvRequest {
                            work_id: work_id.clone(),
                            request_id: request_id.clone(),
                            seq_id,
                        };
                        message.validate_for(&work, transport.peer_node_id())?;
                        worker_registry
                            .lock()
                            .map_err(|_| "état worker LAN verrouillé".to_string())?
                            .reset_abort(&work_id)?;
                        if let Err(error) = subsystem
                            .ensure_loaded(
                                &model_id,
                                PlacementProfile::Balanced,
                                assigned_kv_tokens,
                            )
                            .await
                        {
                            transport
                                .send_message(
                                    &LanWorkMessage::Nack {
                                        work_id: work_id.clone(),
                                        operation: "kv-request".into(),
                                        reason: error,
                                    },
                                    &work,
                                )
                                .await?;
                            continue;
                        }
                        match subsystem.worker_export_kv_state(&model_id).await {
                            Ok((data, seq0_tokens)) => {
                                let seq0_tokens: Vec<u32> = seq0_tokens
                                    .into_iter()
                                    .map(|token| {
                                        u32::try_from(token).map_err(|_| {
                                            "identifiant de token KV LAN hors plage".to_string()
                                        })
                                    })
                                    .collect::<Result<_, _>>()?;
                                let page_count = data.len().div_ceil(LAN_KV_PAGE_BYTES).max(1);
                                for (page_index, page_data) in
                                    data.chunks(LAN_KV_PAGE_BYTES).enumerate()
                                {
                                    if abort.load(std::sync::atomic::Ordering::SeqCst) {
                                        return Ok(());
                                    }
                                    transport
                                        .send_message(
                                            &LanWorkMessage::KvPage {
                                                work_id: work_id.clone(),
                                                request_id: request_id.clone(),
                                                page_index: page_index as u32,
                                                data: page_data.to_vec(),
                                                seq0_tokens: if page_index == 0 {
                                                    seq0_tokens.clone()
                                                } else {
                                                    Vec::new()
                                                },
                                                final_page: page_index + 1 == page_count,
                                            },
                                            &work,
                                        )
                                        .await?;
                                }
                            }
                            Err(error) => {
                                transport
                                    .send_message(
                                        &LanWorkMessage::Nack {
                                            work_id: work_id.clone(),
                                            operation: "kv-request".into(),
                                            reason: error,
                                        },
                                        &work,
                                    )
                                    .await?;
                            }
                        }
                    }
                    LanWorkMessage::WeightRequest {
                        work_id: id,
                        request_id,
                        shard_id,
                        offset,
                        length,
                        total_model_bytes,
                    } if id == work_id => {
                        let message = LanWorkMessage::WeightRequest {
                            work_id: work_id.clone(),
                            request_id: request_id.clone(),
                            shard_id,
                            offset,
                            length,
                            total_model_bytes,
                        };
                        message.validate_for(&work, transport.peer_node_id())?;
                        worker_registry
                            .lock()
                            .map_err(|_| "état worker LAN verrouillé".to_string())?
                            .reset_abort(&work_id)?;
                        match subsystem
                            .worker_read_weight_range(&model_id, offset, length)
                            .await
                        {
                            Ok((actual_total, data)) if actual_total == total_model_bytes => {
                                let page_count = data.len().div_ceil(LAN_KV_PAGE_BYTES);
                                for (page_index, page_data) in
                                    data.chunks(LAN_KV_PAGE_BYTES).enumerate()
                                {
                                    if abort.load(std::sync::atomic::Ordering::SeqCst) {
                                        return Ok(());
                                    }
                                    transport
                                        .send_message(
                                            &LanWorkMessage::WeightPage {
                                                work_id: work_id.clone(),
                                                request_id: request_id.clone(),
                                                shard_id,
                                                page_index: page_index as u32,
                                                offset: offset.saturating_add(
                                                    (page_index * LAN_KV_PAGE_BYTES) as u64,
                                                ),
                                                data: page_data.to_vec(),
                                                final_page: page_index + 1 == page_count,
                                            },
                                            &work,
                                        )
                                        .await?;
                                }
                            }
                            Ok((actual_total, _)) => {
                                transport
                                    .send_message(
                                        &LanWorkMessage::Nack {
                                            work_id: work_id.clone(),
                                            operation: "weight-request".into(),
                                            reason: format!(
                                                "taille du modèle source inattendue: {actual_total}"
                                            ),
                                        },
                                        &work,
                                    )
                                    .await?;
                            }
                            Err(error) => {
                                transport
                                    .send_message(
                                        &LanWorkMessage::Nack {
                                            work_id: work_id.clone(),
                                            operation: "weight-request".into(),
                                            reason: error,
                                        },
                                        &work,
                                    )
                                    .await?;
                            }
                        }
                    }
                    LanWorkMessage::ChatInfer {
                        work_id: id,
                        request_id,
                        messages,
                        max_tokens,
                        temperature_milli,
                        top_p_milli,
                        seed,
                    } if id == work_id => {
                        let message = LanWorkMessage::ChatInfer {
                            work_id: work_id.clone(),
                            request_id: request_id.clone(),
                            messages: messages.clone(),
                            max_tokens,
                            temperature_milli,
                            top_p_milli,
                            seed,
                        };
                        message.validate_for(&work, transport.peer_node_id())?;
                        worker_registry
                            .lock()
                            .map_err(|_| "état worker LAN verrouillé".to_string())?
                            .reset_abort(&work_id)?;
                        if let Err(error) = subsystem
                            .ensure_loaded(
                                &model_id,
                                PlacementProfile::Balanced,
                                assigned_kv_tokens,
                            )
                            .await
                        {
                            transport
                                .send_message(
                                    &LanWorkMessage::Nack {
                                        work_id: work_id.clone(),
                                        operation: "chat-infer".into(),
                                        reason: error,
                                    },
                                    &work,
                                )
                                .await?;
                            continue;
                        }
                        let messages = messages
                            .into_iter()
                            .map(|message| (message.role, message.content))
                            .collect();
                        let params = aos_llama::GenParams {
                            max_tokens,
                            temperature: temperature_milli as f32 / 1000.0,
                            top_p: top_p_milli as f32 / 1000.0,
                            seed: seed as u32,
                        };
                        match subsystem
                            .worker_infer_text(&model_id, messages, params, abort.clone())
                            .await
                        {
                            Ok((text, stats)) => {
                                transport
                                    .send_message(
                                        &LanWorkMessage::TextBatch {
                                            work_id: work_id.clone(),
                                            request_id,
                                            text,
                                            finished: !abort
                                                .load(std::sync::atomic::Ordering::SeqCst),
                                            prompt_tokens: stats.prompt_tokens,
                                            generated_tokens: stats.generated_tokens,
                                            ttft_ms_milli: if stats.ttft_ms.is_finite()
                                                && stats.ttft_ms >= 0.0
                                            {
                                                (stats.ttft_ms * 1000.0).round() as u64
                                            } else {
                                                0
                                            },
                                            tok_s_milli: if stats.tok_s.is_finite()
                                                && stats.tok_s >= 0.0
                                            {
                                                (stats.tok_s * 1000.0).round() as u64
                                            } else {
                                                0
                                            },
                                        },
                                        &work,
                                    )
                                    .await?;
                                if abort.load(std::sync::atomic::Ordering::SeqCst) {
                                    return Ok(());
                                }
                            }
                            Err(error) => {
                                transport
                                    .send_message(
                                        &LanWorkMessage::Nack {
                                            work_id: work_id.clone(),
                                            operation: "chat-infer".into(),
                                            reason: error,
                                        },
                                        &work,
                                    )
                                    .await?;
                                if abort.load(std::sync::atomic::Ordering::SeqCst) {
                                    return Ok(());
                                }
                            }
                        }
                    }
                    LanWorkMessage::Decode {
                        work_id: id,
                        request_id,
                        max_tokens,
                    } if id == work_id => {
                        let message = LanWorkMessage::Decode {
                            work_id: work_id.clone(),
                            request_id: request_id.clone(),
                            max_tokens,
                        };
                        message.validate_for(&work, transport.peer_node_id())?;
                        worker_registry
                            .lock()
                            .map_err(|_| "état worker LAN verrouillé".to_string())?
                            .reset_abort(&work_id)?;
                        if let Err(error) = subsystem
                            .ensure_loaded(
                                &model_id,
                                PlacementProfile::Balanced,
                                assigned_kv_tokens,
                            )
                            .await
                        {
                            transport
                                .send_message(
                                    &LanWorkMessage::Nack {
                                        work_id: work_id.clone(),
                                        operation: "decode".into(),
                                        reason: error,
                                    },
                                    &work,
                                )
                                .await?;
                            continue;
                        }
                        match subsystem
                            .worker_decode_tokens(&model_id, max_tokens, abort.clone())
                            .await
                        {
                            Ok((tokens, finished)) => {
                                let tokens: Vec<u32> = tokens
                                    .into_iter()
                                    .map(|token| {
                                        u32::try_from(token).map_err(|_| {
                                            "identifiant de token généré hors plage".to_string()
                                        })
                                    })
                                    .collect::<Result<_, _>>()?;
                                transport
                                    .send_message(
                                        &LanWorkMessage::TokenBatch {
                                            work_id: work_id.clone(),
                                            request_id,
                                            tokens,
                                            finished,
                                        },
                                        &work,
                                    )
                                    .await?;
                                if abort.load(std::sync::atomic::Ordering::SeqCst) {
                                    return Ok(());
                                }
                            }
                            Err(error) => {
                                transport
                                    .send_message(
                                        &LanWorkMessage::Nack {
                                            work_id: work_id.clone(),
                                            operation: "decode".into(),
                                            reason: error,
                                        },
                                        &work,
                                    )
                                    .await?;
                                if abort.load(std::sync::atomic::Ordering::SeqCst) {
                                    return Ok(());
                                }
                            }
                        }
                    }
                    LanWorkMessage::KvPage {
                        work_id: id,
                        request_id,
                        page_index,
                        data,
                        seq0_tokens,
                        final_page,
                    } if id == work_id => {
                        let message = LanWorkMessage::KvPage {
                            work_id: work_id.clone(),
                            request_id: request_id.clone(),
                            page_index,
                            data,
                            seq0_tokens,
                            final_page,
                        };
                        message.validate_for(&work, transport.peer_node_id())?;
                        if abort.load(std::sync::atomic::Ordering::SeqCst) {
                            return Ok(());
                        }
                        let LanWorkMessage::KvPage {
                            request_id,
                            page_index,
                            data,
                            seq0_tokens,
                            final_page,
                            ..
                        } = message
                        else {
                            unreachable!()
                        };
                        let expected_page = kv_assembly
                            .as_ref()
                            .map_or(0, |assembly| assembly.next_page);
                        if page_index != expected_page {
                            return Err("pages KV LAN reçues hors ordre".into());
                        }
                        if let Some(assembly) = &kv_assembly {
                            if assembly.request_id != request_id {
                                return Err("transferts KV LAN simultanés interdits".into());
                            }
                        } else {
                            let seq0_tokens: Vec<i32> = seq0_tokens
                                .into_iter()
                                .map(|token| {
                                    i32::try_from(token).map_err(|_| {
                                        "identifiant de token KV LAN hors plage".to_string()
                                    })
                                })
                                .collect::<Result<_, _>>()?;
                            kv_assembly = Some(KvPageAssembly {
                                request_id: request_id.clone(),
                                next_page: 0,
                                data: Vec::new(),
                                seq0_tokens,
                            });
                        }
                        let assembly = kv_assembly.as_mut().unwrap();
                        if assembly.data.len().saturating_add(data.len()) > LAN_KV_MAX_BYTES {
                            return Err("état KV LAN supérieur à 64 MiB".into());
                        }
                        assembly.data.extend_from_slice(&data);
                        assembly.next_page = assembly.next_page.saturating_add(1);
                        if final_page {
                            let assembly = kv_assembly.take().unwrap();
                            if let Err(error) = subsystem
                                .ensure_loaded(
                                    &model_id,
                                    PlacementProfile::Balanced,
                                    assigned_kv_tokens,
                                )
                                .await
                            {
                                transport
                                    .send_message(
                                        &LanWorkMessage::Nack {
                                            work_id: work_id.clone(),
                                            operation: "kv-page".into(),
                                            reason: error,
                                        },
                                        &work,
                                    )
                                    .await?;
                                continue;
                            }
                            match subsystem
                                .worker_import_kv_state(
                                    &model_id,
                                    assembly.data,
                                    assembly.seq0_tokens,
                                )
                                .await
                            {
                                Ok(()) => {
                                    transport
                                        .send_message(
                                            &LanWorkMessage::Ack {
                                                work_id: work_id.clone(),
                                                operation: "kv-page".into(),
                                            },
                                            &work,
                                        )
                                        .await?;
                                }
                                Err(error) => {
                                    transport
                                        .send_message(
                                            &LanWorkMessage::Nack {
                                                work_id: work_id.clone(),
                                                operation: "kv-page".into(),
                                                reason: error,
                                            },
                                            &work,
                                        )
                                        .await?;
                                }
                            }
                        }
                    }
                    LanWorkMessage::WeightBegin {
                        work_id: id,
                        request_id,
                        shard_id,
                        offset,
                        length,
                        total_model_bytes,
                        required_ranges,
                    } if id == work_id => {
                        let message = LanWorkMessage::WeightBegin {
                            work_id: work_id.clone(),
                            request_id: request_id.clone(),
                            shard_id,
                            offset,
                            length,
                            total_model_bytes,
                            required_ranges: required_ranges.clone(),
                        };
                        message.validate_for(&work, transport.peer_node_id())?;
                        worker_registry
                            .lock()
                            .map_err(|_| "état worker LAN verrouillé".to_string())?
                            .reset_abort(&work_id)?;
                        if weight_assembly.is_some() {
                            return Err("staging de poids LAN simultané interdit".into());
                        }
                        weight_assembly = Some(WeightPageAssembly {
                            request_id,
                            shard_id,
                            offset,
                            length,
                            total_model_bytes,
                            next_page: 0,
                            data: Vec::with_capacity(length as usize),
                            required_ranges,
                        });
                        transport
                            .send_message(
                                &LanWorkMessage::Ack {
                                    work_id: work_id.clone(),
                                    operation: "weight-begin".into(),
                                },
                                &work,
                            )
                            .await?;
                    }
                    LanWorkMessage::WeightPage {
                        work_id: id,
                        request_id,
                        shard_id,
                        page_index,
                        offset,
                        data,
                        final_page,
                    } if id == work_id => {
                        let message = LanWorkMessage::WeightPage {
                            work_id: work_id.clone(),
                            request_id: request_id.clone(),
                            shard_id,
                            page_index,
                            offset,
                            data,
                            final_page,
                        };
                        message.validate_for(&work, transport.peer_node_id())?;
                        if abort.load(std::sync::atomic::Ordering::SeqCst) {
                            return Ok(());
                        }
                        let LanWorkMessage::WeightPage {
                            request_id,
                            shard_id,
                            page_index,
                            offset,
                            data,
                            final_page,
                            ..
                        } = message
                        else {
                            unreachable!()
                        };
                        let assembly = weight_assembly
                            .as_mut()
                            .ok_or("page de poids LAN sans début de staging")?;
                        if assembly.request_id != request_id
                            || assembly.shard_id != shard_id
                            || page_index != assembly.next_page
                            || offset != assembly.offset.saturating_add(assembly.data.len() as u64)
                            || assembly.data.len().saturating_add(data.len())
                                > assembly.length as usize
                        {
                            return Err("page de poids LAN incohérente ou hors ordre".into());
                        }
                        assembly.data.extend_from_slice(&data);
                        assembly.next_page = assembly.next_page.saturating_add(1);
                        if final_page {
                            if assembly.data.len() != assembly.length as usize {
                                return Err("staging de poids LAN incomplet".into());
                            }
                            let assembly = weight_assembly.take().unwrap();
                            let staging_root = staging_root.clone();
                            let staging_work_id = work_id.clone();
                            let staging_model_id = model_id.clone();
                            let result = tokio::task::spawn_blocking(move || {
                                stage_lan_weight_shard(
                                    &staging_root,
                                    &staging_work_id,
                                    &staging_model_id,
                                    assembly.shard_id,
                                    assembly.offset,
                                    assembly.total_model_bytes,
                                    assembly.required_ranges,
                                    &assembly.data,
                                )
                            })
                            .await
                            .map_err(|error| format!("staging poids interrompu: {error}"))?;
                            match result {
                                Ok(()) => {
                                    transport
                                        .send_message(
                                            &LanWorkMessage::Ack {
                                                work_id: work_id.clone(),
                                                operation: "weight-shard".into(),
                                            },
                                            &work,
                                        )
                                        .await?;
                                }
                                Err(error) => {
                                    transport
                                        .send_message(
                                            &LanWorkMessage::Nack {
                                                work_id: work_id.clone(),
                                                operation: "weight-shard".into(),
                                                reason: error,
                                            },
                                            &work,
                                        )
                                        .await?;
                                }
                            }
                        }
                    }
                    LanWorkMessage::TokenBatch { .. } => {
                        return Err("batch de tokens LAN reçu dans le mauvais sens".into());
                    }
                    LanWorkMessage::TextBatch { .. } => {
                        return Err("batch texte LAN reçu dans le mauvais sens".into());
                    }
                    LanWorkMessage::LayerActivationPage {
                        work_id: id,
                        request_id,
                        shard_id,
                        layer_index,
                        sequence,
                        position_start,
                        page_index,
                        total_bytes,
                        data,
                        final_page,
                    } if id == work_id => {
                        let message = LanWorkMessage::LayerActivationPage {
                            work_id: work_id.clone(),
                            request_id: request_id.clone(),
                            shard_id,
                            layer_index,
                            sequence,
                            position_start,
                            page_index,
                            total_bytes,
                            data,
                            final_page,
                        };
                        message.validate_for(&work, transport.peer_node_id())?;
                        if abort.load(std::sync::atomic::Ordering::SeqCst) {
                            return Ok(());
                        }
                        let LanWorkMessage::LayerActivationPage { data, .. } = message else {
                            unreachable!()
                        };
                        if let Some(assembly) = &activation_assembly {
                            if assembly.request_id != request_id
                                || assembly.shard_id != shard_id
                                || assembly.layer_index != layer_index
                                || assembly.sequence != sequence
                            {
                                return Err("activations LAN simultanées incompatibles".into());
                            }
                        } else {
                            activation_assembly = Some(LanActivationAssembly::new(
                                request_id.clone(),
                                shard_id,
                                layer_index,
                                sequence,
                                total_bytes,
                            )?);
                        }
                        let assembly = activation_assembly.as_mut().unwrap();
                        if let Some(input) = assembly.push_page(page_index, &data, final_page)? {
                            match subsystem
                                .worker_layer_activation(
                                    &model_id,
                                    layer_index,
                                    request_id.clone(),
                                    position_start,
                                    input,
                                )
                                .await
                            {
                                Ok(output) => {
                                    for (page_index, page) in output
                                        .chunks(aos_placement::LAYER_RPC_PAGE_BYTES)
                                        .enumerate()
                                    {
                                        transport
                                            .send_message(
                                                &LanWorkMessage::LayerActivationResult {
                                                    work_id: work_id.clone(),
                                                    request_id: request_id.clone(),
                                                    shard_id,
                                                    layer_index,
                                                    sequence,
                                                    position_start,
                                                    page_index: page_index as u32,
                                                    total_bytes: output.len() as u64,
                                                    data: page.to_vec(),
                                                    final_page: (page_index + 1)
                                                        * aos_placement::LAYER_RPC_PAGE_BYTES
                                                        >= output.len(),
                                                },
                                                &work,
                                            )
                                            .await?;
                                    }
                                }
                                Err(reason) => {
                                    transport
                                        .send_message(
                                            &LanWorkMessage::Nack {
                                                work_id: work_id.clone(),
                                                operation: "layer-activation".into(),
                                                reason,
                                            },
                                            &work,
                                        )
                                        .await?;
                                }
                            }
                            activation_assembly = None;
                        }
                    }
                    _ => return Err("message LAN worker inattendu".into()),
                }
            }
        }
        LanWorkMessage::Cancel { work_id } => {
            let work = lan_control_work(&work_id, Vec::new());
            worker_registry
                .lock()
                .map_err(|_| "état worker LAN verrouillé".to_string())?
                .cancel(&work_id)?;
            subsystem.clear_worker_layer_caches();
            transport
                .send_message(
                    &LanWorkMessage::Ack {
                        work_id,
                        operation: "cancel".into(),
                    },
                    &work,
                )
                .await
        }
        LanWorkMessage::Heartbeat { work_id } => {
            let work = lan_control_work(&work_id, Vec::new());
            transport
                .send_message(
                    &LanWorkMessage::Ack {
                        work_id,
                        operation: "heartbeat".into(),
                    },
                    &work,
                )
                .await
        }
        _ => Err("premier message LAN worker inattendu".into()),
    }
}

#[tokio::main]
async fn main() {
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "demo/modeld.dev.yaml".to_string());
    let config = ModeldConfig::load(&config_path).expect("chargement config modeld");
    let catalog = std::env::var("AOS_HOME")
        .map(|h| std::path::PathBuf::from(h).join("data/models/catalog.yaml"))
        .unwrap_or_else(|_| std::path::PathBuf::from("data/models/catalog.yaml"));
    let catalog = if catalog.exists() {
        catalog
    } else {
        std::path::PathBuf::from("data/models/catalog.yaml")
    };
    let registry = ModelRegistry::load(&catalog).expect("catalogue");

    let mut sysinfo = sysinfo::System::new_all();
    sysinfo.refresh_memory();
    let ram_total = sysinfo.total_memory();

    let subsystem = Arc::new(ModelSubsystem::new(config.clone(), &registry, ram_total));
    let preference_home = std::env::var_os("AOS_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let lan_cluster = {
        let registry = load_lan_registry(&config, &preference_home);
        Arc::new(Mutex::new(LanCluster::new(registry)))
    };
    if config.lan_cluster_enabled_at(&preference_home)
        && config.lan_auto_discovery_at(&preference_home)
    {
        let port = config.lan_discovery_port_at(&preference_home);
        let local_node = LanNode {
            node_id: config.lan_node_id_at(&preference_home),
            display_name: "Akasha OS".into(),
            address: config.lan_listen_address_at(&preference_home),
            public_key_fingerprint: config.lan_public_key_fingerprint_at(&preference_home),
            trust: NodeTrust::Unpaired,
            capabilities: Vec::new(),
        };
        let advertisement = if local_node.public_key_fingerprint.trim().is_empty() {
            None
        } else {
            Some(LanDiscoveryAdvertisement::from_node(&local_node))
        };
        let cluster = lan_cluster.clone();
        let discovery_home = preference_home.clone();
        tokio::spawn(async move {
            let bind_address = format!("0.0.0.0:{port}");
            let broadcast_address = format!("255.255.255.255:{port}");
            let socket = match LanDiscoverySocket::bind(&bind_address, &broadcast_address).await {
                Ok(socket) => socket,
                Err(error) => {
                    eprintln!("[aos-modeld] découverte LAN désactivée: {error}");
                    return;
                }
            };
            let mut announce_tick = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                tokio::select! {
                    _ = announce_tick.tick() => {
                        if let Some(advertisement) = advertisement.as_ref() {
                            if let Err(error) = socket.announce(advertisement).await {
                                eprintln!("[aos-modeld] annonce LAN: {error}");
                            }
                        }
                    }
                    received = socket.receive() => {
                        match received {
                            Ok(node) => {
                                if node.node_id == local_node.node_id {
                                    continue;
                                }
                                if let Ok(mut cluster) = cluster.lock() {
                                    if cluster.registry_mut().try_discover(node).is_ok() {
                                        let _ = persist_lan_registry(
                                            cluster.registry(),
                                            &discovery_home,
                                        );
                                    }
                                }
                            }
                            Err(error) => eprintln!("[aos-modeld] réception découverte LAN: {error}"),
                        }
                    }
                }
            }
        });
    }
    eprintln!(
        "[aos-modeld] {} modèles au registry, bus {}",
        registry.len(),
        config.bus
    );

    // Client bus pour appels sortants (platformd : fs.class, net.check, audit).
    let bus = BusClient::connect(&config.bus, "modeld")
        .await
        .expect("connexion au bus — lancer aos-busd d'abord");

    // The worker listener is opt-in and only starts when the LAN session key
    // is available from the secret service. Discovery alone never opens this
    // socket, and an unpaired/revoked peer is rejected before the handshake.
    if config.lan_cluster_enabled_at(&preference_home) {
        let local_node_id = config.lan_node_id_at(&preference_home);
        let listen_address = config.lan_listen_address_at(&preference_home);
        let secret_name = config.lan_session_key_secret_at(&preference_home);
        match load_lan_session_key(&bus, &secret_name).await {
            Ok(session_key) => {
                let empty_registry = LanPairingRegistry::default();
                match LanTcpListener::bind(&local_node_id, &listen_address, &empty_registry).await {
                    Ok(listener) => {
                        eprintln!(
                            "[aos-modeld] worker LAN prêt sur {}",
                            listener
                                .local_addr()
                                .map(|address| address.to_string())
                                .unwrap_or_else(|_| listen_address.clone())
                        );
                        let cluster = lan_cluster.clone();
                        let subsystem_task = subsystem.clone();
                        let worker_registry = Arc::new(Mutex::new(LanWorkerRegistry::default()));
                        let worker_registry_task = worker_registry.clone();
                        let staging_root = preference_home.clone();
                        tokio::spawn(async move {
                            loop {
                                let registry = cluster
                                    .lock()
                                    .map(|cluster| cluster.registry().clone())
                                    .unwrap_or_default();
                                match listener
                                    .accept_authenticated_any_work_with_registry(
                                        &registry,
                                        session_key.clone(),
                                    )
                                    .await
                                {
                                    Ok(transport) => {
                                        let local_node_id = local_node_id.clone();
                                        let subsystem = subsystem_task.clone();
                                        let worker_registry = worker_registry_task.clone();
                                        let staging_root = staging_root.clone();
                                        tokio::spawn(async move {
                                            if let Err(error) = handle_lan_worker_connection(
                                                transport,
                                                local_node_id,
                                                subsystem,
                                                worker_registry,
                                                staging_root,
                                            )
                                            .await
                                            {
                                                eprintln!(
                                                    "[aos-modeld] session worker LAN interrompue: {error}"
                                                );
                                            }
                                        });
                                    }
                                    Err(error) => {
                                        eprintln!(
                                            "[aos-modeld] connexion worker LAN refusée: {error}"
                                        );
                                    }
                                }
                            }
                        });
                    }
                    Err(error) => eprintln!("[aos-modeld] worker LAN désactivé: {error}"),
                }
            }
            Err(error) => eprintln!("[aos-modeld] worker LAN désactivé: {error}"),
        }
    }

    let mut svc = BusService::new("modeld");

    // --- model.list ---
    {
        let sub = subsystem.clone();
        svc.on("model.list", move |ctx| {
            let sub = sub.clone();
            async move {
                let _ = ctx
                    .respond(aos_ipc::msg::Status::Ok, &sub.list_models())
                    .await;
            }
        });
    }

    // --- model.inspect ---
    {
        let sub = subsystem.clone();
        svc.on("model.inspect", move |ctx| {
            let sub = sub.clone();
            async move {
                match ctx.payload::<ModelIdRequest>() {
                    Ok(req) => match sub.inspect(&req.model_id) {
                        Some(info) => {
                            let _ = ctx.respond(aos_ipc::msg::Status::Ok, &info).await;
                        }
                        None => {
                            let _ = ctx
                                .respond_error(aos_ipc::msg::Status::NotFound, "modèle inconnu")
                                .await;
                        }
                    },
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }

    // --- model.load ---
    {
        let sub = subsystem.clone();
        svc.on("model.load", move |ctx| {
            let sub = sub.clone();
            async move {
                match ctx.payload::<LoadRequest>() {
                    Ok(req) => {
                        let profile = parse_profile(&req.profile);
                        match sub
                            .ensure_loaded(&req.model_id, profile, req.kv_tokens)
                            .await
                        {
                            Ok(resp) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::InternalError, &e)
                                    .await;
                            }
                        }
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }

    // --- model.adapter.execute (explicit experimental runtime call) ---
    {
        let sub = subsystem.clone();
        let execute_sub = sub.clone();
        svc.on("model.adapter.status", move |ctx| {
            let sub = sub.clone();
            async move {
                let req = match ctx.payload::<ModelAdapterStatusRequest>() {
                    Ok(req) => req,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                let backend = match req.backend.trim().to_ascii_lowercase().as_str() {
                    "npu" => BackendKind::Npu,
                    "webgpu" => BackendKind::WebGpu,
                    _ => {
                        let _ = ctx
                            .respond_error(
                                aos_ipc::msg::Status::BadRequest,
                                "backend expérimental invalide",
                            )
                            .await;
                        return;
                    }
                };
                let Some((endpoint, memory, operations, quantizations)) =
                    sub.adapter_runtime(backend)
                else {
                    let response = ModelAdapterStatusResponse {
                        backend: backend_name(backend).into(),
                        configured: false,
                        reachable: false,
                        device: None,
                        memory_bytes: None,
                        supported_operations: Vec::new(),
                        supported_quantizations: Vec::new(),
                        reason: "aucun runtime adaptateur configuré".into(),
                    };
                    let _ = ctx.respond(aos_ipc::msg::Status::Ok, &response).await;
                    return;
                };
                if !endpoint_is_loopback(&endpoint) {
                    let response = ModelAdapterStatusResponse {
                        backend: backend_name(backend).into(),
                        configured: true,
                        reachable: false,
                        device: None,
                        memory_bytes: None,
                        supported_operations: Vec::new(),
                        supported_quantizations: Vec::new(),
                        reason: "endpoint refusé : runtime adaptateur limité au loopback".into(),
                    };
                    let _ = ctx.respond(aos_ipc::msg::Status::Ok, &response).await;
                    return;
                }
                match aos_placement::AdapterRpcClient::connect(
                    &endpoint,
                    backend,
                    memory,
                    &operations,
                    &quantizations,
                )
                .await
                {
                    Ok(client) => {
                        let handshake = client.handshake().clone();
                        let response = ModelAdapterStatusResponse {
                            backend: backend_name(backend).into(),
                            configured: true,
                            reachable: true,
                            device: Some(handshake.device),
                            memory_bytes: Some(handshake.memory_bytes),
                            supported_operations: handshake.supported_operations,
                            supported_quantizations: handshake
                                .supported_quantizations
                                .into_iter()
                                .map(|quantization| quantization.as_str().into())
                                .collect(),
                            reason: "handshake adaptateur accepté".into(),
                        };
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &response).await;
                    }
                    Err(error) => {
                        let response = ModelAdapterStatusResponse {
                            backend: backend_name(backend).into(),
                            configured: true,
                            reachable: false,
                            device: None,
                            memory_bytes: None,
                            supported_operations: Vec::new(),
                            supported_quantizations: Vec::new(),
                            reason: error,
                        };
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &response).await;
                    }
                }
            }
        });
        svc.on("model.adapter.execute", move |ctx| {
            let sub = execute_sub.clone();
            async move {
                let req = match ctx.payload::<ModelAdapterExecuteRequest>() {
                    Ok(req) => req,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                let backend = match req.backend.trim().to_ascii_lowercase().as_str() {
                    "npu" => BackendKind::Npu,
                    "webgpu" => BackendKind::WebGpu,
                    _ => {
                        let _ = ctx
                            .respond_error(
                                aos_ipc::msg::Status::BadRequest,
                                "backend expérimental invalide",
                            )
                            .await;
                        return;
                    }
                };
                let quantization = match aos_placement::Quantization::parse(&req.quantization) {
                    Some(quantization) => quantization,
                    None => {
                        let _ = ctx
                            .respond_error(
                                aos_ipc::msg::Status::BadRequest,
                                "quantification adaptateur invalide",
                            )
                            .await;
                        return;
                    }
                };
                let phase = match req.phase.trim().to_ascii_lowercase().as_str() {
                    "prefill" => AdapterExecutionPhase::Prefill,
                    "decode" | "" => AdapterExecutionPhase::Decode,
                    _ => {
                        let _ = ctx
                            .respond_error(
                                aos_ipc::msg::Status::BadRequest,
                                "phase adaptateur invalide (prefill|decode)",
                            )
                            .await;
                        return;
                    }
                };
                let Some((endpoint, memory, operations, quantizations)) =
                    sub.adapter_runtime(backend)
                else {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::NotFound,
                            "aucun runtime adaptateur explicitement configuré",
                        )
                        .await;
                    return;
                };
                if !endpoint_is_loopback(&endpoint) {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::PermissionDenied,
                            "un runtime NPU/WebGPU doit être local au daemon",
                        )
                        .await;
                    return;
                }
                let result = match aos_placement::AdapterRpcClient::connect(
                    &endpoint,
                    backend,
                    memory,
                    &operations,
                    &quantizations,
                )
                .await
                {
                    Ok(mut client) => {
                        client
                            .execute_phase(phase, &req.operation, quantization, req.tensor)
                            .await
                    }
                    Err(error) => Err(error),
                };
                match result {
                    Ok(tensor) => {
                        let response = ModelAdapterExecuteResponse {
                            backend: backend_name(backend).into(),
                            operation: req.operation,
                            tensor,
                        };
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &response).await;
                    }
                    Err(error) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::InternalError, &error)
                            .await;
                    }
                }
            }
        });
    }

    // --- model.plan (diagnostic, read-only) ---
    {
        let sub = subsystem.clone();
        svc.on("model.plan", move |ctx| {
            let sub = sub.clone();
            async move {
                match ctx.payload::<ModelPlanRequest>() {
                    Ok(req) => match sub.diagnose(
                        &req.model_id,
                        if req.kv_tokens == 0 {
                            sub.config.default_kv_tokens
                        } else {
                            req.kv_tokens
                        },
                    ) {
                        Ok(plans) => {
                            let rows: Vec<ModelPlanDiagnostic> =
                                plans.into_iter().map(plan_diagnostic_row).collect();
                            let _ = ctx.respond(aos_ipc::msg::Status::Ok, &rows).await;
                        }
                        Err(e) => {
                            let _ = ctx.respond_error(aos_ipc::msg::Status::NotFound, &e).await;
                        }
                    },
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }

    // --- model.cluster.* (experimental LAN policy; no socket side effect) ---
    {
        let cluster = lan_cluster.clone();
        let model_config = config.clone();
        let preference_home = preference_home.clone();
        svc.on("model.cluster.nodes", move |ctx| {
            let cluster = cluster.clone();
            let model_config = model_config.clone();
            let preference_home = preference_home.clone();
            async move {
                let enabled = model_config.lan_cluster_enabled_at(&preference_home);
                let response = cluster
                    .lock()
                    .map(|cluster| LanClusterNodesResponse {
                        enabled,
                        nodes: cluster.registry().nodes().map(lan_node_info).collect(),
                    })
                    .map_err(|_| "verrou cluster indisponible".to_string());
                match response {
                    Ok(response) => {
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &response).await;
                    }
                    Err(error) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::InternalError, &error)
                            .await;
                    }
                }
            }
        });
    }
    {
        let cluster = lan_cluster.clone();
        let model_config = config.clone();
        let preference_home = preference_home.clone();
        svc.on("model.cluster.layer_pipeline_status", move |ctx| {
            let cluster = cluster.clone();
            let model_config = model_config.clone();
            let preference_home = preference_home.clone();
            async move {
                let enabled = model_config.lan_cluster_enabled_at(&preference_home);
                let paired_nodes = cluster
                    .lock()
                    .map(|cluster| cluster.registry().paired_nodes().count())
                    .unwrap_or(0);
                let native_rpc = matches!(
                    aos_llama::LlamaBackend::distributed_layer_capability(),
                    aos_llama::DistributedLayerCapability::NativeRpc
                );
                let adapter_ready = enabled && paired_nodes > 0;
                let response = LanClusterLayerPipelineStatusResponse {
                    enabled,
                    native_rpc,
                    adapter_ready,
                    reason: if !enabled {
                        "cluster LAN désactivé".into()
                    } else if paired_nodes == 0 {
                        "aucun nœud LAN appairé".into()
                    } else if !native_rpc {
                        "adaptateur RPC Akasha CPU actif (GGUF requis)".into()
                    } else {
                        "backend RPC natif et adaptateur Akasha disponibles".into()
                    },
                };
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &response).await;
            }
        });
    }
    {
        let cluster = lan_cluster.clone();
        let model_config = config.clone();
        let preference_home = preference_home.clone();
        let bus = bus.clone();
        svc.on("model.cluster.weight_transfer", move |ctx| {
            let cluster = cluster.clone();
            let model_config = model_config.clone();
            let preference_home = preference_home.clone();
            let bus = bus.clone();
            async move {
                if !model_config.lan_cluster_enabled_at(&preference_home) {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::PermissionDenied,
                            "cluster LAN désactivé",
                        )
                        .await;
                    return;
                }
                let req = match ctx.payload::<LanClusterWeightTransferRequest>() {
                    Ok(req) => req,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                if !req.allow_sensitive_data || !req.encrypted_transport {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::PermissionDenied,
                            "le transfert de poids LAN exige une autorisation sensible explicite et un transport chiffré",
                        )
                        .await;
                    return;
                }
                if req.source_node_id == req.target_node_id
                    || req.length == 0
                    || req.length > LAN_KV_MAX_BYTES as u64
                    || req.total_model_bytes == 0
                    || req.offset.saturating_add(req.length) > req.total_model_bytes
                {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::BadRequest,
                            "plage de poids LAN invalide",
                        )
                        .await;
                    return;
                }
                let planned = cluster
                    .lock()
                    .map(|cluster| {
                        let plan = cluster.job(&req.work_id).cloned()?;
                        if plan.model_id != req.model_id {
                            return None;
                        }
                        let source = plan
                            .assignments
                            .iter()
                            .find(|assignment| {
                                assignment.node_id == req.source_node_id
                                    && assignment.shard_ids.contains(&req.shard_id)
                            })?
                            .clone();
                        let target = plan
                            .assignments
                            .iter()
                            .find(|assignment| assignment.node_id == req.target_node_id)?
                            .clone();
                        Some((source, target, cluster.registry().clone()))
                    })
                    .map_err(|_| "verrou cluster indisponible".to_string())
                    .and_then(|planned| {
                        planned.ok_or("travail poids LAN non planifié pour ce shard".into())
                    });
                let (source, target, registry) = match planned {
                    Ok(planned) => planned,
                    Err(error) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                            .await;
                        return;
                    }
                };
                let mut shard_ids = source.shard_ids.clone();
                for shard_id in &target.shard_ids {
                    if !shard_ids.contains(shard_id) {
                        shard_ids.push(*shard_id);
                    }
                }
                if !shard_ids.contains(&req.shard_id) {
                    shard_ids.push(req.shard_id);
                }
                let work = DistributedWork {
                    work_id: req.work_id.clone(),
                    model_id: req.model_id.clone(),
                    shard_ids,
                    allow_sensitive_data: true,
                    encrypted_transport: true,
                };
                let key = match load_lan_session_key(&bus, &req.session_key_secret).await {
                    Ok(key) => key,
                    Err(error) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::PermissionDenied, &error)
                            .await;
                        return;
                    }
                };
                let local_node_id = model_config.lan_node_id_at(&preference_home);
                match send_lan_weight_shard(
                    &local_node_id,
                    &registry,
                    &work,
                    &req.source_node_id,
                    source.shard_ids,
                    &req.target_node_id,
                    target.shard_ids,
                    req.request_id.clone(),
                    req.shard_id,
                    req.offset,
                    req.length,
                    req.total_model_bytes,
                    key,
                )
                .await
                {
                    Ok((page_count, total_bytes)) => {
                        let response = LanClusterWeightTransferResponse {
                            work_id: req.work_id,
                            request_id: req.request_id,
                            source_node_id: req.source_node_id,
                            target_node_id: req.target_node_id,
                            shard_id: req.shard_id,
                            page_count,
                            total_bytes,
                        };
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &response).await;
                    }
                    Err(error) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                            .await;
                    }
                }
            }
        });
    }
    {
        let cluster = lan_cluster.clone();
        let sub = subsystem.clone();
        let model_config = config.clone();
        let preference_home = preference_home.clone();
        let bus = bus.clone();
        svc.on("model.cluster.stage_local_model", move |ctx| {
            let cluster = cluster.clone();
            let sub = sub.clone();
            let model_config = model_config.clone();
            let preference_home = preference_home.clone();
            let bus = bus.clone();
            async move {
                if !model_config.lan_cluster_enabled_at(&preference_home) {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::PermissionDenied,
                            "cluster LAN désactivé",
                        )
                        .await;
                    return;
                }
                let req = match ctx.payload::<LanClusterStageLocalModelRequest>() {
                    Ok(req) => req,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                if !req.allow_sensitive_data || !req.encrypted_transport {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::PermissionDenied,
                            "le staging local LAN exige une autorisation sensible et un transport chiffré",
                        )
                        .await;
                    return;
                }
                let planned = cluster
                    .lock()
                    .map(|cluster| {
                        let plan = cluster.job(&req.work_id).cloned()?;
                        if plan.model_id != req.model_id {
                            return None;
                        }
                        let target = plan.assignments.iter().find(|assignment| {
                            assignment.node_id == req.target_node_id
                                && assignment.shard_ids.contains(&req.shard_id)
                        })?;
                        let mut shard_ids = plan
                            .assignments
                            .iter()
                            .flat_map(|assignment| assignment.shard_ids.iter().copied())
                            .collect::<Vec<_>>();
                        shard_ids.sort_unstable();
                        shard_ids.dedup();
                        Some((target.clone(), shard_ids, cluster.registry().clone()))
                    })
                    .map_err(|_| "verrou cluster indisponible".to_string())
                    .and_then(|planned| planned.ok_or("travail LAN non planifié".into()));
                let (target, shard_ids, registry) = match planned {
                    Ok(planned) => planned,
                    Err(error) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                            .await;
                        return;
                    }
                };
                let work = DistributedWork {
                    work_id: req.work_id.clone(),
                    model_id: req.model_id.clone(),
                    shard_ids,
                    allow_sensitive_data: true,
                    encrypted_transport: true,
                };
                let key = match load_lan_session_key(&bus, &req.session_key_secret).await {
                    Ok(key) => key,
                    Err(error) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::PermissionDenied, &error)
                            .await;
                        return;
                    }
                };
                let local_node_id = model_config.lan_node_id_at(&preference_home);
                let required_ranges = match resolve_stage_weight_ranges(
                    &sub,
                    &req.model_id,
                    req.shard_id,
                    &req.required_ranges,
                    req.first_layer,
                    req.last_layer,
                )
                .await
                {
                    Ok(ranges) => ranges,
                    Err(error) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                            .await;
                        return;
                    }
                };
                match send_lan_local_weight_model(
                    &local_node_id,
                    &registry,
                    &work,
                    &sub,
                    &target.node_id,
                    req.shard_id,
                    &req.request_id,
                    required_ranges,
                    key,
                )
                .await
                {
                    Ok((page_count, total_bytes)) => {
                        let response = LanClusterStageLocalModelResponse {
                            work_id: req.work_id,
                            request_id: req.request_id,
                            target_node_id: target.node_id,
                            model_id: work.model_id,
                            shard_id: req.shard_id,
                            page_count,
                            total_bytes,
                        };
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &response).await;
                    }
                    Err(error) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                            .await;
                    }
                }
            }
        });
    }
    {
        let cluster = lan_cluster.clone();
        let model_config = config.clone();
        let preference_home = preference_home.clone();
        let bus = bus.clone();
        svc.on("model.cluster.infer_tokens", move |ctx| {
            let cluster = cluster.clone();
            let model_config = model_config.clone();
            let preference_home = preference_home.clone();
            let bus = bus.clone();
            async move {
                if !model_config.lan_cluster_enabled_at(&preference_home) {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::PermissionDenied,
                            "cluster LAN désactivé",
                        )
                        .await;
                    return;
                }
                let req = match ctx.payload::<LanClusterInferTokensRequest>() {
                    Ok(req) => req,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                if !req.allow_sensitive_data || !req.encrypted_transport {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::PermissionDenied,
                            "l'inférence LAN exige des données sensibles explicitement autorisées et un transport chiffré",
                        )
                        .await;
                    return;
                }
                let key = match load_lan_session_key(&bus, &req.session_key_secret).await {
                    Ok(key) => key,
                    Err(error) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::PermissionDenied, &error)
                            .await;
                        return;
                    }
                };
                let planned = cluster
                    .lock()
                    .map(|cluster| {
                        let plan = cluster.job(&req.work_id).cloned()?;
                        if plan.model_id != req.model_id
                            || req.shard_ids.iter().any(|shard_id| {
                                !plan
                                    .assignments
                                    .iter()
                                    .any(|assignment| assignment.shard_ids.contains(shard_id))
                            })
                        {
                            return None;
                        }
                        let assignment = plan
                            .assignments
                            .iter()
                            .find(|assignment| assignment.node_id == req.node_id)?
                            .clone();
                        Some((assignment, cluster.registry().clone()))
                    })
                    .map_err(|_| "verrou cluster indisponible".to_string())
                    .and_then(|planned| planned.ok_or("travail LAN non planifié pour ce nœud".into()));
                let (assignment, registry) = match planned {
                    Ok(planned) => planned,
                    Err(error) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                            .await;
                        return;
                    }
                };
                let work = DistributedWork {
                    work_id: req.work_id.clone(),
                    model_id: req.model_id.clone(),
                    shard_ids: if req.shard_ids.is_empty() {
                        assignment.shard_ids.clone()
                    } else {
                        req.shard_ids.clone()
                    },
                    allow_sensitive_data: true,
                    encrypted_transport: true,
                };
                let local_node_id = model_config.lan_node_id_at(&preference_home);
                // Never inflate past the cluster plan: worker rejects prefill when
                // kv_tokens > assignment.kv_tokens. Treat 0 as "use assignment".
                let kv_tokens = if req.kv_tokens == 0 {
                    assignment.kv_tokens
                } else {
                    req.kv_tokens.min(assignment.kv_tokens)
                };
                match send_lan_token_inference(
                    &local_node_id,
                    &registry,
                    &work,
                    &req.node_id,
                    assignment.shard_ids,
                    req.request_id.clone(),
                    req.input_tokens,
                    req.max_tokens,
                    kv_tokens,
                    key,
                )
                .await
                {
                    Ok((tokens, finished)) => {
                        let response = LanClusterInferTokensResponse {
                            work_id: req.work_id,
                            request_id: req.request_id,
                            node_id: req.node_id,
                            tokens,
                            finished,
                        };
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &response).await;
                    }
                    Err(error) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                            .await;
                    }
                }
            }
        });
    }
    {
        let cluster = lan_cluster.clone();
        let model_config = config.clone();
        let preference_home = preference_home.clone();
        let bus = bus.clone();
        svc.on("model.cluster.infer_chat", move |ctx| {
            let cluster = cluster.clone();
            let model_config = model_config.clone();
            let preference_home = preference_home.clone();
            let bus = bus.clone();
            async move {
                if !model_config.lan_cluster_enabled_at(&preference_home) {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::PermissionDenied,
                            "cluster LAN désactivé",
                        )
                        .await;
                    return;
                }
                let req = match ctx.payload::<LanClusterInferChatRequest>() {
                    Ok(req) => req,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                if !req.allow_sensitive_data || !req.encrypted_transport {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::PermissionDenied,
                            "l'inférence LAN exige des données sensibles explicitement autorisées et un transport chiffré",
                        )
                        .await;
                    return;
                }
                if !req.params.temperature.is_finite()
                    || !req.params.top_p.is_finite()
                    || req.params.temperature < 0.0
                    || req.params.temperature > 5.0
                    || req.params.top_p <= 0.0
                    || req.params.top_p > 1.0
                {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::BadRequest,
                            "paramètres d'échantillonnage invalides",
                        )
                        .await;
                    return;
                }
                // Float upper bounds above reject values that would round down into
                // range (e.g. top_p=1.00001 → 1000). Milli check catches top_p that
                // rounds to 0 (e.g. 0.0004) which still passes top_p > 0.0.
                let temperature_milli = (req.params.temperature * 1000.0).round() as u32;
                let top_p_milli = (req.params.top_p * 1000.0).round() as u32;
                if temperature_milli > 5000 || top_p_milli == 0 || top_p_milli > 1000 {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::BadRequest,
                            "paramètres d'échantillonnage invalides",
                        )
                        .await;
                    return;
                }
                let key = match load_lan_session_key(&bus, &req.session_key_secret).await {
                    Ok(key) => key,
                    Err(error) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::PermissionDenied, &error)
                            .await;
                        return;
                    }
                };
                let planned = cluster
                    .lock()
                    .map(|cluster| {
                        let plan = cluster.job(&req.work_id).cloned()?;
                        if plan.model_id != req.model_id {
                            return None;
                        }
                        let assignment = plan
                            .assignments
                            .iter()
                            .find(|assignment| assignment.node_id == req.node_id)?
                            .clone();
                        Some((assignment, cluster.registry().clone()))
                    })
                    .map_err(|_| "verrou cluster indisponible".to_string())
                    .and_then(|planned| planned.ok_or("travail LAN non planifié pour ce nœud".into()));
                let (assignment, registry) = match planned {
                    Ok(planned) => planned,
                    Err(error) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                            .await;
                        return;
                    }
                };
                let work = DistributedWork {
                    work_id: req.work_id.clone(),
                    model_id: req.model_id.clone(),
                    shard_ids: assignment.shard_ids.clone(),
                    allow_sensitive_data: true,
                    encrypted_transport: true,
                };
                let messages = req
                    .messages
                    .into_iter()
                    .map(|message| LanChatMessage {
                        role: message.role,
                        content: message.content,
                    })
                    .collect();
                let local_node_id = model_config.lan_node_id_at(&preference_home);
                match send_lan_chat_inference(
                    &local_node_id,
                    &registry,
                    &work,
                    &req.node_id,
                    assignment.shard_ids,
                    req.request_id.clone(),
                    messages,
                    req.params.max_tokens,
                    temperature_milli,
                    top_p_milli,
                    req.params.seed.unwrap_or(42) as u64,
                    key,
                )
                .await
                {
                    Ok((text, finished, prompt_tokens, generated_tokens, ttft_ms, tok_s)) => {
                        let response = LanClusterInferChatResponse {
                            work_id: req.work_id,
                            request_id: req.request_id,
                            node_id: req.node_id,
                            text,
                            finished,
                            prompt_tokens,
                            generated_tokens,
                            ttft_ms,
                            tok_s,
                        };
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &response).await;
                    }
                    Err(error) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                            .await;
                    }
                }
            }
        });
    }
    {
        let cluster = lan_cluster.clone();
        let model_config = config.clone();
        let preference_home = preference_home.clone();
        let bus = bus.clone();
        svc.on("model.cluster.kv_transfer", move |ctx| {
            let cluster = cluster.clone();
            let model_config = model_config.clone();
            let preference_home = preference_home.clone();
            let bus = bus.clone();
            async move {
                if !model_config.lan_cluster_enabled_at(&preference_home) {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::PermissionDenied,
                            "cluster LAN désactivé",
                        )
                        .await;
                    return;
                }
                let req = match ctx.payload::<LanClusterKvTransferRequest>() {
                    Ok(req) => req,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                if !req.allow_sensitive_data || !req.encrypted_transport {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::PermissionDenied,
                            "le transfert KV LAN exige une autorisation sensible explicite et un transport chiffré",
                        )
                        .await;
                    return;
                }
                let key = match load_lan_session_key(&bus, &req.session_key_secret).await {
                    Ok(key) => key,
                    Err(error) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::PermissionDenied, &error)
                            .await;
                        return;
                    }
                };
                let planned = cluster
                    .lock()
                    .map(|cluster| {
                        let plan = cluster.job(&req.work_id).cloned()?;
                        if plan.model_id != req.model_id {
                            return None;
                        }
                        let source = plan
                            .assignments
                            .iter()
                            .find(|assignment| assignment.node_id == req.source_node_id)?
                            .clone();
                        let target = plan
                            .assignments
                            .iter()
                            .find(|assignment| assignment.node_id == req.target_node_id)?
                            .clone();
                        Some((source, target, cluster.registry().clone()))
                    })
                    .map_err(|_| "verrou cluster indisponible".to_string())
                    .and_then(|planned| planned.ok_or("travail KV LAN non planifié pour ces nœuds".into()));
                let (source, target, registry) = match planned {
                    Ok(planned) => planned,
                    Err(error) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                            .await;
                        return;
                    }
                };
                let mut shard_ids = if req.shard_ids.is_empty() {
                    source.shard_ids.clone()
                } else {
                    req.shard_ids.clone()
                };
                for shard_id in &target.shard_ids {
                    if !shard_ids.contains(shard_id) {
                        shard_ids.push(*shard_id);
                    }
                }
                let work = DistributedWork {
                    work_id: req.work_id.clone(),
                    model_id: req.model_id,
                    shard_ids,
                    allow_sensitive_data: true,
                    encrypted_transport: true,
                };
                let local_node_id = model_config.lan_node_id_at(&preference_home);
                match send_lan_kv_transfer(
                    &local_node_id,
                    &registry,
                    &work,
                    &req.source_node_id,
                    source.shard_ids,
                    &req.target_node_id,
                    target.shard_ids,
                    req.request_id.clone(),
                    key,
                )
                .await
                {
                    Ok((page_count, total_bytes)) => {
                        let response = LanClusterKvTransferResponse {
                            work_id: req.work_id,
                            request_id: req.request_id,
                            source_node_id: req.source_node_id,
                            target_node_id: req.target_node_id,
                            page_count,
                            total_bytes,
                        };
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &response).await;
                    }
                    Err(error) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                            .await;
                    }
                }
            }
        });
    }
    {
        let cluster = lan_cluster.clone();
        let preference_home = preference_home.clone();
        svc.on("model.cluster.discover", move |ctx| {
            let cluster = cluster.clone();
            let preference_home = preference_home.clone();
            async move {
                match ctx.payload::<LanClusterDiscoverRequest>() {
                    Ok(req) => {
                        let address_valid = req
                            .address
                            .parse::<std::net::SocketAddr>()
                            .map(|address| {
                                address.ip().is_loopback()
                                    || match address.ip() {
                                        std::net::IpAddr::V4(ip) => {
                                            ip.is_private() || ip.is_link_local()
                                        }
                                        std::net::IpAddr::V6(ip) => {
                                            (ip.segments()[0] & 0xfe00) == 0xfc00
                                                || (ip.segments()[0] & 0xffc0) == 0xfe80
                                        }
                                    }
                            })
                            .unwrap_or(false);
                        if req.node_id.trim().is_empty()
                            || req.display_name.trim().is_empty()
                            || req.public_key_fingerprint.trim().is_empty()
                            || !address_valid
                        {
                            let _ = ctx
                                .respond_error(
                                    aos_ipc::msg::Status::BadRequest,
                                    "identité, empreinte ou adresse LAN invalide",
                                )
                                .await;
                            return;
                        }
                        let result = cluster
                            .lock()
                            .map_err(|_| "verrou cluster indisponible".to_string())
                            .and_then(|mut cluster| {
                                cluster.registry_mut().try_discover(LanNode {
                                    node_id: req.node_id,
                                    display_name: req.display_name,
                                    address: req.address,
                                    public_key_fingerprint: req.public_key_fingerprint,
                                    trust: NodeTrust::Unpaired,
                                    capabilities: req.capabilities,
                                })?;
                                persist_lan_registry(cluster.registry(), &preference_home)
                            });
                        match result {
                            Ok(()) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &true).await;
                            }
                            Err(error) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                                    .await;
                            }
                        }
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }
    {
        let cluster = lan_cluster.clone();
        let sub = subsystem.clone();
        let model_config = config.clone();
        let preference_home = preference_home.clone();
        let bus = bus.clone();
        svc.on("model.cluster.layer_infer", {
            let cluster = cluster.clone();
            let model_config = model_config.clone();
            let preference_home = preference_home.clone();
            let bus = bus.clone();
            move |ctx| {
                let cluster = cluster.clone();
                let model_config = model_config.clone();
                let preference_home = preference_home.clone();
                let bus = bus.clone();
                async move {
                    if !model_config.lan_cluster_enabled_at(&preference_home) {
                        let _ = ctx
                            .respond_error(
                                aos_ipc::msg::Status::PermissionDenied,
                                "cluster LAN désactivé",
                            )
                            .await;
                        return;
                    }
                    let req = match ctx.payload::<LanClusterLayerInferRequest>() {
                        Ok(req) => req,
                        Err(_) => {
                            let _ = ctx
                                .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                                .await;
                            return;
                        }
                    };
                    if !req.allow_sensitive_data || !req.encrypted_transport {
                        let _ = ctx
                            .respond_error(
                                aos_ipc::msg::Status::PermissionDenied,
                                "le pipeline de couches LAN exige une autorisation sensible explicite et un transport chiffré",
                            )
                            .await;
                        return;
                    }
                    let key = match load_lan_session_key(&bus, &req.session_key_secret).await {
                        Ok(key) => key,
                        Err(error) => {
                            let _ = ctx
                                .respond_error(aos_ipc::msg::Status::PermissionDenied, &error)
                                .await;
                            return;
                        }
                    };
                    let mut work = DistributedWork {
                        work_id: req.work_id.clone(),
                        model_id: req.model_id.clone(),
                        shard_ids: vec![req.shard_id],
                        allow_sensitive_data: req.allow_sensitive_data,
                        encrypted_transport: req.encrypted_transport,
                    };
                    let planned = cluster
                        .lock()
                        .map(|cluster| {
                            let plan = cluster.job(&req.work_id).cloned()?;
                            if plan.model_id != req.model_id {
                                return None;
                            }
                            let assignment = plan
                                .assignments
                                .iter()
                                .find(|assignment| {
                                    assignment.node_id == req.node_id
                                        && assignment.shard_ids.contains(&req.shard_id)
                                })?
                                .clone();
                            Some((assignment, cluster.registry().clone()))
                        })
                        .map_err(|_| "verrou cluster indisponible".to_string())
                        .and_then(|planned| {
                            planned.ok_or("assignment couche LAN introuvable".into())
                        });
                    let (assignment, registry) = match planned {
                        Ok(planned) => planned,
                        Err(error) => {
                            let _ = ctx
                                .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                                .await;
                            return;
                    }
                };
                // Le canal est limité à l'assignment sélectionné ; il peut
                // contenir plusieurs shards même si cette requête ne traite
                // qu'une seule activation.
                work.shard_ids = assignment.shard_ids.clone();
                match send_lan_layer_activation(
                        "coordinator",
                        &registry,
                        &work,
                        &assignment,
                        req.request_id.clone(),
                        req.shard_id,
                        req.layer_index,
                        req.sequence,
                        req.position_start,
                        req.activation,
                        key,
                    )
                    .await
                    {
                        Ok(activation) => {
                            let response = LanClusterLayerInferResponse {
                                work_id: req.work_id,
                                request_id: req.request_id,
                                node_id: req.node_id,
                                shard_id: req.shard_id,
                                layer_index: req.layer_index,
                                sequence: req.sequence,
                                activation,
                            };
                            let _ = ctx.respond(aos_ipc::msg::Status::Ok, &response).await;
                        }
                        Err(error) => {
                            let _ = ctx
                                .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                                .await;
                        }
                    }
                }
            }
        });
        svc.on("model.cluster.layer_pipeline_infer", {
            let cluster = cluster.clone();
            let sub = sub.clone();
            let model_config = model_config.clone();
            let preference_home = preference_home.clone();
            let bus = bus.clone();
            move |ctx| {
                let cluster = cluster.clone();
                let sub = sub.clone();
                let model_config = model_config.clone();
                let preference_home = preference_home.clone();
                let bus = bus.clone();
                async move {
                    if !model_config.lan_cluster_enabled_at(&preference_home) {
                        let _ = ctx
                            .respond_error(
                                aos_ipc::msg::Status::PermissionDenied,
                                "cluster LAN désactivé",
                            )
                            .await;
                        return;
                    }
                    let req = match ctx.payload::<LanClusterLayerPipelineRequest>() {
                        Ok(req) => req,
                        Err(_) => {
                            let _ = ctx
                                .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                                .await;
                            return;
                        }
                    };
                    if !req.allow_sensitive_data
                        || !req.encrypted_transport
                        || req.stages.is_empty()
                        || req.stages.len() > 2048
                        || (req.activation.is_empty() && req.input_tokens.is_empty())
                        || (!req.activation.is_empty() && !req.input_tokens.is_empty())
                        || req.input_tokens.len() > 8192
                        || req.max_tokens > 4096
                        || (req.max_tokens > 0 && req.input_tokens.is_empty())
                    {
                        let _ = ctx
                            .respond_error(
                                aos_ipc::msg::Status::PermissionDenied,
                                "pipeline LAN invalide ou non autorisé",
                            )
                            .await;
                        return;
                    }
                    if !req.params.temperature.is_finite()
                        || req.params.temperature < 0.0
                        || req.params.temperature > 5.0
                        || !req.params.top_p.is_finite()
                        || req.params.top_p <= 0.0
                        || req.params.top_p > 1.0
                    {
                        let _ = ctx
                            .respond_error(
                                aos_ipc::msg::Status::BadRequest,
                                "paramètres de sampling invalides",
                            )
                            .await;
                        return;
                    }
                    let pipeline_plan = match LayerPipelinePlan::new(
                        req.total_layers,
                        req.stages
                            .iter()
                            .map(|stage| LayerStage {
                                node_id: stage.node_id.clone(),
                                first_layer: stage.layer_index,
                                last_layer: stage.layer_index,
                            })
                            .collect(),
                    ) {
                        Ok(plan) => plan,
                        Err(error) => {
                            let _ = ctx
                                .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                                .await;
                            return;
                        }
                    };
                    let key = match load_lan_session_key(&bus, &req.session_key_secret).await {
                        Ok(key) => key,
                        Err(error) => {
                            let _ = ctx
                                .respond_error(aos_ipc::msg::Status::PermissionDenied, &error)
                                .await;
                            return;
                        }
                    };
                    let planned = cluster
                        .lock()
                        .map(|cluster| {
                            let plan = cluster.job(&req.work_id).cloned()?;
                            if plan.model_id != req.model_id {
                                return None;
                            }
                            let mut stages = Vec::with_capacity(req.stages.len());
                            for stage in &req.stages {
                                let assignment = plan
                                    .assignments
                                    .iter()
                                    .find(|assignment| {
                                        assignment.node_id == stage.node_id
                                            && assignment.shard_ids.contains(&stage.shard_id)
                                    })?
                                    .clone();
                                stages.push((stage.clone(), assignment));
                            }
                            Some((stages, cluster.registry().clone()))
                        })
                        .map_err(|_| "verrou cluster indisponible".to_string())
                        .and_then(|planned| {
                            planned.ok_or("pipeline LAN non planifié pour ces nœuds".into())
                        });
                    let (stages, registry) = match planned {
                        Ok(planned) => planned,
                        Err(error) => {
                            let _ = ctx
                                .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                                .await;
                            return;
                        }
                    };
                    let activation = if req.activation.is_empty() {
                        match sub
                            .worker_embed_tokens(&req.model_id, req.input_tokens.clone())
                            .await
                        {
                            Ok(activation) => activation,
                            Err(error) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                                    .await;
                                return;
                            }
                        }
                    } else {
                        req.activation.clone()
                    };
                    if pipeline_plan.total_layers != stages.len() as u32 {
                        let _ = ctx
                            .respond_error(
                                aos_ipc::msg::Status::BadRequest,
                                "pipeline LAN incomplet",
                            )
                            .await;
                        return;
                    }
                    let mut shard_ids = Vec::new();
                    for (stage, _) in &stages {
                        if !shard_ids.contains(&stage.shard_id) {
                            shard_ids.push(stage.shard_id);
                        }
                    }
                    let model_id = req.model_id.clone();
                    let work = DistributedWork {
                        work_id: req.work_id.clone(),
                        model_id: model_id.clone(),
                        shard_ids,
                        allow_sensitive_data: req.allow_sensitive_data,
                        encrypted_transport: req.encrypted_transport,
                    };
                    let generation_steps = if req.max_tokens == 0 {
                        1
                    } else {
                        req.max_tokens
                    };
                    let prompt_tokens = req.input_tokens.len() as u32;
                    let generation_started = std::time::Instant::now();
                    let mut sampling_state = u64::from(req.params.seed.unwrap_or(42));
                    let mut current_activation = activation;
                    let mut final_logits = None;
                    let mut generated_tokens = Vec::new();
                    let mut layers_executed = 0u32;
                    let mut transfers = 0u32;
                    let mut pipeline_error = None;
                    for step in 0..generation_steps {
                        let position_start = if step == 0 {
                            req.position_start
                        } else {
                            req.position_start
                                .saturating_add(prompt_tokens)
                                .saturating_add(generated_tokens.len().saturating_sub(1) as u32)
                        };
                        match send_lan_layer_pipeline(
                            "coordinator",
                            &registry,
                            &work,
                            &stages,
                            req.request_id.clone(),
                            req.sequence.saturating_add(step),
                            position_start,
                            std::mem::take(&mut current_activation),
                            key.clone(),
                        )
                        .await
                        {
                            Ok((next_activation, executed, moved)) => {
                                current_activation = next_activation;
                                layers_executed = layers_executed.saturating_add(executed);
                                transfers = transfers.saturating_add(moved);
                            }
                            Err(error) => {
                                pipeline_error = Some(error);
                                break;
                            }
                        }
                        if req.return_logits || req.max_tokens > 0 {
                            let last = match last_token_activation(&current_activation) {
                                Ok(last) => last,
                                Err(error) => {
                                    pipeline_error = Some(error);
                                    break;
                                }
                            };
                            let logits = match sub.worker_logits(&model_id, last).await {
                                Ok(logits) => logits,
                                Err(error) => {
                                    pipeline_error = Some(error);
                                    break;
                                }
                            };
                            if req.return_logits {
                                final_logits = Some(logits.clone());
                            }
                            if req.max_tokens > 0 {
                                let token = match sample_token(
                                    &logits,
                                    req.params.temperature,
                                    req.params.top_p,
                                    &mut sampling_state,
                                ) {
                                    Ok(token) => token,
                                    Err(error) => {
                                        pipeline_error = Some(error);
                                        break;
                                    }
                                };
                                generated_tokens.push(token);
                                if req.eos_token_id == Some(token)
                                    || generated_tokens.len() >= req.max_tokens as usize
                                {
                                    break;
                                }
                                current_activation =
                                    match sub.worker_embed_tokens(&model_id, vec![token]).await {
                                        Ok(activation) => activation,
                                        Err(error) => {
                                            pipeline_error = Some(error);
                                            break;
                                        }
                                    };
                            }
                        }
                    }
                    if let Some(error) = pipeline_error {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                            .await;
                    } else {
                        let generation_ms = generation_started.elapsed().as_secs_f64() * 1000.0;
                        let tok_s = if generation_ms > 0.0 {
                            generated_tokens.len() as f64 / (generation_ms / 1000.0)
                        } else {
                            0.0
                        };
                        let response = LanClusterLayerPipelineResponse {
                            work_id: req.work_id,
                            request_id: req.request_id,
                            activation: current_activation,
                            layers_executed,
                            transfers,
                            logits: final_logits,
                            generated_tokens,
                            prompt_tokens,
                            generation_ms,
                            tok_s,
                        };
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &response).await;
                    }
                }
            }
        });
        svc.on("model.cluster.dispatch", move |ctx| {
            let cluster = cluster.clone();
            let model_config = model_config.clone();
            let preference_home = preference_home.clone();
            let bus = bus.clone();
            async move {
                if !model_config.lan_cluster_enabled_at(&preference_home) {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::PermissionDenied,
                            "cluster LAN désactivé",
                        )
                        .await;
                    return;
                }
                let req = match ctx.payload::<LanClusterDispatchRequest>() {
                    Ok(req) => req,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                let key = match load_lan_session_key(&bus, &req.session_key_secret).await {
                    Ok(key) => key,
                    Err(error) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::PermissionDenied, &error)
                            .await;
                        return;
                    }
                };
                let work = DistributedWork {
                    work_id: req.work_id,
                    model_id: req.model_id,
                    shard_ids: req.shard_ids,
                    allow_sensitive_data: req.allow_sensitive_data,
                    encrypted_transport: req.encrypted_transport,
                };
                let planned = cluster
                    .lock()
                    .map(|cluster| {
                        cluster
                            .job(&work.work_id)
                            .cloned()
                            .map(|plan| (plan, cluster.registry().clone()))
                    })
                    .map_err(|_| "verrou cluster indisponible".to_string());
                let (plan, registry) = match planned {
                    Ok(Some(planned)) => planned,
                    Ok(None) => {
                        let _ = ctx
                            .respond_error(
                                aos_ipc::msg::Status::NotFound,
                                "travail LAN non planifié",
                            )
                            .await;
                        return;
                    }
                    Err(error) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::InternalError, &error)
                            .await;
                        return;
                    }
                };

                let mut dispatched_nodes = Vec::new();
                let mut errors = Vec::new();
                for assignment in &plan.assignments {
                    let Some(node) = registry.get(&assignment.node_id) else {
                        errors.push(format!("{}: nœud inconnu", assignment.node_id));
                        continue;
                    };
                    let mut transport = match LanTcpTransport::connect_authenticated(
                        "coordinator",
                        &assignment.node_id,
                        &node.address,
                        &registry,
                        &work,
                        key.clone(),
                    )
                    .await
                    {
                        Ok(transport) => transport,
                        Err(error) => {
                            errors.push(format!("{}: {error}", assignment.node_id));
                            continue;
                        }
                    };
                    let message = LanWorkMessage::Assign {
                        work_id: work.work_id.clone(),
                        model_id: work.model_id.clone(),
                        assignment: assignment.clone(),
                        allow_sensitive_data: work.allow_sensitive_data,
                    };
                    match transport.send_message(&message, &work).await {
                        Ok(()) => match transport.receive_message(&work).await {
                            Ok(LanWorkMessage::Ack { operation, .. }) if operation == "assign" => {
                                dispatched_nodes.push(assignment.node_id.clone())
                            }
                            Ok(other) => errors.push(format!(
                                "{}: réponse LAN inattendue après assignment: {other:?}",
                                assignment.node_id
                            )),
                            Err(error) => errors.push(format!(
                                "{}: accusé assignment LAN absent: {error}",
                                assignment.node_id
                            )),
                        },
                        Err(error) => errors.push(format!("{}: {error}", assignment.node_id)),
                    }
                }

                let mut state = lan_state_name(plan.state);
                if errors.is_empty() {
                    match cluster.lock() {
                        Ok(mut cluster) => match cluster.set_running(&work.work_id) {
                            Ok(()) => state = "running".into(),
                            Err(error) => errors.push(error),
                        },
                        Err(_) => errors.push("verrou cluster indisponible".into()),
                    }
                }
                let response = LanClusterDispatchResponse {
                    work_id: work.work_id,
                    state,
                    dispatched_nodes,
                    errors,
                    assignments: plan
                        .assignments
                        .iter()
                        .map(|assignment| LanClusterAssignment {
                            node_id: assignment.node_id.clone(),
                            shard_ids: assignment.shard_ids.clone(),
                            kv_tokens: assignment.kv_tokens,
                            encrypted_transport: assignment.encrypted_transport,
                        })
                        .collect(),
                };
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &response).await;
            }
        });
    }
    {
        let cluster = lan_cluster.clone();
        let preference_home = preference_home.clone();
        svc.on("model.cluster.pair", move |ctx| {
            let cluster = cluster.clone();
            let preference_home = preference_home.clone();
            async move {
                match ctx.payload::<LanClusterPairRequest>() {
                    Ok(req) => {
                        let result = cluster
                            .lock()
                            .map_err(|_| "verrou cluster indisponible".to_string())
                            .and_then(|mut cluster| {
                                cluster
                                    .registry_mut()
                                    .pair(&req.node_id, &req.public_key_fingerprint)?;
                                persist_lan_registry(cluster.registry(), &preference_home)
                            });
                        match result {
                            Ok(()) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &true).await;
                            }
                            Err(error) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                                    .await;
                            }
                        }
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }
    {
        let cluster = lan_cluster.clone();
        let preference_home = preference_home.clone();
        svc.on("model.cluster.revoke", move |ctx| {
            let cluster = cluster.clone();
            let preference_home = preference_home.clone();
            async move {
                match ctx.payload::<LanClusterNodeRequest>() {
                    Ok(req) => {
                        let result = cluster
                            .lock()
                            .map_err(|_| "verrou cluster indisponible".to_string())
                            .and_then(|mut cluster| {
                                if !cluster.registry_mut().revoke(&req.node_id) {
                                    return Err("nœud inconnu".into());
                                }
                                persist_lan_registry(cluster.registry(), &preference_home)
                            });
                        match result {
                            Ok(()) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &true).await;
                            }
                            Err(error) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                                    .await;
                            }
                        }
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }
    {
        let cluster = lan_cluster.clone();
        let model_config = config.clone();
        let preference_home = preference_home.clone();
        svc.on("model.cluster.plan", move |ctx| {
            let cluster = cluster.clone();
            let model_config = model_config.clone();
            let preference_home = preference_home.clone();
            async move {
                if !model_config.lan_cluster_enabled_at(&preference_home) {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::PermissionDenied,
                            "cluster LAN désactivé",
                        )
                        .await;
                    return;
                }
                match ctx.payload::<LanClusterPlanRequest>() {
                    Ok(req) => {
                        let work = DistributedWork {
                            work_id: req.work_id,
                            model_id: req.model_id,
                            shard_ids: req.shard_ids,
                            allow_sensitive_data: req.allow_sensitive_data,
                            encrypted_transport: req.encrypted_transport,
                        };
                        let result = cluster
                            .lock()
                            .map_err(|_| "verrou cluster indisponible".to_string())
                            .and_then(|mut cluster| {
                                if req.layer_pipeline {
                                    cluster.plan_layer_pipeline(&work, req.kv_tokens)
                                } else {
                                    cluster.plan(&work, req.kv_tokens)
                                }
                            });
                        match result {
                            Ok(plan) => {
                                let response = lan_plan_response(&plan, Vec::new(), Vec::new());
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &response).await;
                            }
                            Err(error) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                                    .await;
                            }
                        }
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }
    {
        let cluster = lan_cluster.clone();
        let model_config = config.clone();
        let preference_home = preference_home.clone();
        let bus = bus.clone();
        svc.on("model.cluster.recover", move |ctx| {
            let cluster = cluster.clone();
            let model_config = model_config.clone();
            let preference_home = preference_home.clone();
            let bus = bus.clone();
            async move {
                if !model_config.lan_cluster_enabled_at(&preference_home) {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::PermissionDenied,
                            "cluster LAN désactivé",
                        )
                        .await;
                    return;
                }
                match ctx.payload::<LanClusterJobRequest>() {
                    Ok(req) => {
                        let Some(node_id) = req.node_id else {
                            let _ = ctx
                                .respond_error(aos_ipc::msg::Status::BadRequest, "node_id requis")
                                .await;
                            return;
                        };
                        let dispatch_key = match (&req.session_key_secret, &req.model_id) {
                            (Some(secret_name), Some(_))
                                if !secret_name.trim().is_empty() =>
                            {
                                match load_lan_session_key(&bus, secret_name).await {
                                    Ok(key) => Some(key),
                                    Err(error) => {
                                        let _ = ctx
                                            .respond_error(
                                                aos_ipc::msg::Status::PermissionDenied,
                                                &error,
                                            )
                                            .await;
                                        return;
                                    }
                                }
                            }
                            (None, None) => None,
                            _ => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::BadRequest,
                                        "model_id et session_key_secret doivent être fournis ensemble",
                                    )
                                    .await;
                                return;
                            }
                        };
                        if dispatch_key.is_some()
                            && (req.shard_ids.is_empty() || !req.encrypted_transport)
                        {
                            let _ = ctx
                                .respond_error(
                                    aos_ipc::msg::Status::BadRequest,
                                    "shard_ids et encrypted_transport sont requis pour propager la reprise",
                                )
                                .await;
                            return;
                        }
                        let result = cluster
                            .lock()
                            .map_err(|_| "verrou cluster indisponible".to_string())
                            .and_then(|mut cluster| {
                                if let Some(model_id) = req.model_id.as_ref() {
                                    let plan = cluster
                                        .job(&req.work_id)
                                        .ok_or("travail LAN introuvable")?;
                                    if plan.model_id != *model_id {
                                        return Err("modèle de reprise LAN différent du plan".into());
                                    }
                                }
                                let recovery = cluster.recover_node_loss(&req.work_id, &node_id)?;
                                let plan = cluster
                                    .job(&req.work_id)
                                    .ok_or("travail LAN introuvable")?
                                    .clone();
                                Ok((
                                    plan,
                                    recovery.reassigned_shards,
                                    cluster.registry().clone(),
                                ))
                            });
                        match result {
                            Ok((plan, reassigned, registry)) => {
                                let errors = if let (Some(key), Some(model_id)) =
                                    (dispatch_key, req.model_id.as_ref())
                                {
                                    let work = DistributedWork {
                                        work_id: req.work_id.clone(),
                                        model_id: model_id.clone(),
                                        shard_ids: req.shard_ids.clone(),
                                        allow_sensitive_data: req.allow_sensitive_data,
                                        encrypted_transport: req.encrypted_transport,
                                    };
                                    let (_, errors) =
                                        send_lan_assignments(
                                            &registry,
                                            &work,
                                            &plan.assignments,
                                            key,
                                        )
                                        .await;
                                    errors
                                } else {
                                    Vec::new()
                                };
                                let mut response =
                                    lan_plan_response(&plan, reassigned, Vec::new());
                                response.errors = errors;
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &response).await;
                            }
                            Err(error) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                                    .await;
                            }
                        }
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }
    {
        let cluster = lan_cluster.clone();
        let model_config = config.clone();
        let preference_home = preference_home.clone();
        let bus = bus.clone();
        svc.on("model.cluster.cancel", move |ctx| {
            let cluster = cluster.clone();
            let model_config = model_config.clone();
            let preference_home = preference_home.clone();
            let bus = bus.clone();
            async move {
                if !model_config.lan_cluster_enabled_at(&preference_home) {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::PermissionDenied,
                            "cluster LAN désactivé",
                        )
                        .await;
                    return;
                }
                match ctx.payload::<LanClusterJobRequest>() {
                    Ok(req) => {
                        let Some(secret_name) = req
                            .session_key_secret
                            .as_deref()
                            .filter(|name| !name.trim().is_empty())
                        else {
                            let _ = ctx
                                .respond_error(
                                    aos_ipc::msg::Status::BadRequest,
                                    "session_key_secret requis pour propager l'annulation LAN",
                                )
                                .await;
                            return;
                        };
                        let key = match load_lan_session_key(&bus, secret_name).await {
                            Ok(key) => key,
                            Err(error) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::PermissionDenied, &error)
                                    .await;
                                return;
                            }
                        };
                        let result = cluster
                            .lock()
                            .map_err(|_| "verrou cluster indisponible".to_string())
                            .and_then(|mut cluster| {
                                let cancellation = cluster.cancel(&req.work_id)?;
                                let plan = cluster
                                    .job(&req.work_id)
                                    .ok_or("travail LAN introuvable")?
                                    .clone();
                                Ok((
                                    plan,
                                    cancellation.cancelled_nodes,
                                    cluster.registry().clone(),
                                ))
                            });
                        match result {
                            Ok((plan, cancelled_nodes, registry)) => {
                                let work = DistributedWork {
                                    work_id: req.work_id.clone(),
                                    model_id: String::new(),
                                    shard_ids: plan
                                        .assignments
                                        .iter()
                                        .flat_map(|assignment| assignment.shard_ids.iter().copied())
                                        .collect(),
                                    allow_sensitive_data: false,
                                    encrypted_transport: true,
                                };
                                let errors =
                                    send_lan_cancel(&registry, &work, &cancelled_nodes, key).await;
                                let mut response =
                                    lan_plan_response(&plan, Vec::new(), cancelled_nodes);
                                response.errors = errors;
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &response).await;
                            }
                            Err(error) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::BadRequest, &error)
                                    .await;
                            }
                        }
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }

    // --- model.unload ---
    {
        let sub = subsystem.clone();
        svc.on("model.unload", move |ctx| {
            let sub = sub.clone();
            async move {
                match ctx.payload::<UnloadRequest>() {
                    Ok(req) => {
                        let _ = ctx
                            .respond(aos_ipc::msg::Status::Ok, &sub.unload(&req.model_id))
                            .await;
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }

    // --- model.infer (flux, avec routage privacy §3.7) ---
    {
        let sub = subsystem.clone();
        let bus2 = bus.clone();
        svc.on("model.infer", move |ctx| {
            let sub = sub.clone();
            let bus = bus2.clone();
            async move {
                let mut req: InferRequest = match ctx.payload() {
                    Ok(r) => r,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                req.ensure_image_data_refs();
                let stream: StreamHandle = ctx.open_stream();
                // Résolution du modèle + chargement paresseux si besoin.
                let mut model_id = match req
                    .model_id
                    .clone()
                    .or_else(|| sub.config.default_model.clone())
                {
                    Some(id) => id,
                    None => {
                        let _ = stream
                            .send(&TokenEvent::Error {
                                message: "aucun modèle configuré".into(),
                            })
                            .await;
                        let _ = stream.finish(aos_ipc::msg::Status::InternalError).await;
                        return;
                    }
                };

                // --- Routage privacy (§3.7) ---
                // 1. Classes des données référencées (via platformd fs.class).
                let mut max_secret = false;
                for path in &req.data_refs {
                    if let Ok(resp) = bus
                        .call::<aos_proto::FsClassRequest, aos_proto::FsClassResponse>(
                            "fs.class",
                            &aos_proto::FsClassRequest { path: path.clone() },
                            vec![],
                        )
                        .await
                    {
                        if resp.class == aos_proto::DataClass::Secret {
                            max_secret = true;
                        }
                    }
                }
                let mode = req
                    .routing
                    .clone()
                    .unwrap_or_else(|| sub.routing_mode());
                let want_remote = model_id.starts_with("remote:") || sub.has_remote(&model_id);

                if want_remote {
                    // secret → jamais remote (§3.7) : bascule locale auditée.
                    if max_secret {
                        let _ = bus
                            .call::<aos_proto::AuditAppendRequest, bool>(
                                "audit.append",
                                &aos_proto::AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor: "service:modeld".into(),
                                    action: "policy.deny".into(),
                                    target: model_id.clone(),
                                    detail: serde_json::json!({
                                        "rule": "deny_remote_secret",
                                        "data_refs": req.data_refs,
                                    }),
                                },
                                vec![],
                            )
                            .await;
                        let fallback = sub.config.default_model.clone().unwrap_or(model_id);
                        let _ = stream
                            .send(&TokenEvent::Error {
                                message: format!(
                                    "donnée secret → routage local forcé (fallback {fallback})"
                                ),
                            })
                            .await;
                        model_id = fallback;
                    } else {
                        let endpoint = sub.remote_endpoint(&model_id).unwrap_or_default();
                        let loopback = aos_model::providers::endpoint_is_loopback(&endpoint);
                        // local_only still allows loopback (Ollama / vLLM / LM Studio).
                        if mode == "local_only" && !loopback {
                            let _ = stream
                                .send(&TokenEvent::Error {
                                    message: "mode local_only : backend WAN interdit".into(),
                                })
                                .await;
                            let _ = stream.finish(aos_ipc::msg::Status::PermissionDenied).await;
                            return;
                        }
                        let (host, port) = providers::parse_host_port(&endpoint);
                        if !loopback {
                            // 2. Contrôle d'egress (§9.5) via platformd net.check.
                            let allowed = bus
                                .call::<aos_proto::NetCheckRequest, bool>(
                                    "net.check",
                                    &aos_proto::NetCheckRequest {
                                        host: host.clone(),
                                        port,
                                        actor: "service:modeld".into(),
                                        caps: vec![format!("net.connect:{host}:{port}")],
                                    },
                                    vec![],
                                )
                                .await
                                .unwrap_or(false);
                            if !allowed {
                                let _ = stream
                                    .send(&TokenEvent::Error {
                                        message: format!("egress refusé vers {host}:{port}"),
                                    })
                                    .await;
                                let _ = stream.finish(aos_ipc::msg::Status::PermissionDenied).await;
                                return;
                            }
                        }
                        // 3. Exécution distante (flux).
                        let _ = bus
                            .call::<aos_proto::AuditAppendRequest, bool>(
                                "audit.append",
                                &aos_proto::AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor: "service:modeld".into(),
                                    action: "model.route".into(),
                                    target: model_id.clone(),
                                    detail: serde_json::json!({"direction": "remote", "host": host}),
                                },
                                vec![],
                            )
                            .await;
                        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
                        let sub2 = sub.clone();
                        let mid = model_id.clone();
                        let req2 = req.clone();
                        tokio::spawn(async move {
                            let r = sub2.infer_remote(&mid, &req2, tx).await;
                            if let Err(e) = r {
                                // Le flux est fermé côté émetteur ; l'erreur est
                                // loguée via audit par l'appelant si besoin.
                                eprintln!("[modeld] remote infer: {e}");
                            }
                        });
                        while let Some(ev) = rx.recv().await {
                            let terminal = matches!(ev, TokenEvent::Done { .. });
                            if stream.send(&ev).await.is_err() {
                                return;
                            }
                            if terminal {
                                break;
                            }
                        }
                        let _ = stream.finish(aos_ipc::msg::Status::Ok).await;
                        return;
                    }
                }

                if let Err(e) = sub
                    .ensure_loaded(
                        &model_id,
                        PlacementProfile::Balanced,
                        sub.config.default_kv_tokens,
                    )
                    .await
                {
                    let _ = stream.send(&TokenEvent::Error { message: e }).await;
                    let _ = stream.finish(aos_ipc::msg::Status::InternalError).await;
                    return;
                }
                match sub.infer(&model_id, &req).await {
                    Ok((_id, mut deltas, done)) => {
                        while let Some(ev) = deltas.recv().await {
                            if stream.send(&ev).await.is_err() {
                                return; // client parti
                            }
                        }
                        match done.await {
                            Ok(aos_model::subsystem::InferOutcome::Done {
                                prompt_tokens,
                                generated_tokens,
                                ttft_ms,
                                tok_s,
                            }) => {
                                let _ = stream
                                    .send(&TokenEvent::Done {
                                        prompt_tokens,
                                        generated_tokens,
                                        ttft_ms,
                                        tok_s,
                                    })
                                    .await;
                                let _ = stream.finish(aos_ipc::msg::Status::Ok).await;
                            }
                            Ok(aos_model::subsystem::InferOutcome::Cancelled) => {
                                let _ = stream.finish(aos_ipc::msg::Status::Cancelled).await;
                            }
                            Ok(aos_model::subsystem::InferOutcome::Failed(e)) => {
                                let _ = stream.send(&TokenEvent::Error { message: e }).await;
                                let _ = stream.finish(aos_ipc::msg::Status::InternalError).await;
                            }
                            Err(_) => {
                                let _ = stream.finish(aos_ipc::msg::Status::InternalError).await;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = stream.send(&TokenEvent::Error { message: e }).await;
                        let _ = stream.finish(aos_ipc::msg::Status::InternalError).await;
                    }
                }
            }
        });
    }

    // --- model.cancel ---
    {
        let sub = subsystem.clone();
        svc.on("model.cancel", move |ctx| {
            let sub = sub.clone();
            async move {
                match ctx.payload::<CancelRequest>() {
                    Ok(req) => {
                        let _ = ctx
                            .respond(aos_ipc::msg::Status::Ok, &sub.cancel(req.inference_id))
                            .await;
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }

    // --- model.migrate (E18) ---
    {
        let sub = subsystem.clone();
        let bus = bus.clone();
        svc.on("model.migrate", move |ctx| {
            let sub = sub.clone();
            let bus = bus.clone();
            async move {
                match ctx.payload::<MigrateRequest>() {
                    Ok(req) => {
                        let resp = sub.migrate(&req.target).await;
                        if resp.fallback || !resp.ok {
                            let _ = bus
                                .call::<aos_proto::AuditAppendRequest, bool>(
                                    "audit.append",
                                    &aos_proto::AuditAppendRequest {
                                        trace_id: String::new(),
                                        actor: "service:modeld".into(),
                                        action: "model.migrate.fallback".into(),
                                        target: req.target.clone(),
                                        detail: serde_json::json!({
                                            "ok": resp.ok,
                                            "fallback": resp.fallback,
                                            "message": resp.message,
                                        }),
                                    },
                                    vec![],
                                )
                                .await;
                        } else {
                            let _ = bus
                                .call::<aos_proto::AuditAppendRequest, bool>(
                                    "audit.append",
                                    &aos_proto::AuditAppendRequest {
                                        trace_id: String::new(),
                                        actor: "service:modeld".into(),
                                        action: "model.migrate".into(),
                                        target: req.target.clone(),
                                        detail: serde_json::json!({
                                            "profile": resp.profile,
                                            "message": resp.message,
                                        }),
                                    },
                                    vec![],
                                )
                                .await;
                        }
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }

    // --- model.backend.add / model.set_routing (P3) ---
    {
        let sub = subsystem.clone();
        let bus = bus.clone();
        svc.on("model.backend.add", move |ctx| {
            let sub = sub.clone();
            let bus = bus.clone();
            async move {
                match ctx.payload::<aos_proto::BackendAddRequest>() {
                    Ok(req) => {
                        let mut api_key = None;
                        if let Some(name) = req.secret_name.as_deref() {
                            match bus
                                .call::<aos_proto::SecretGetRequest, String>(
                                    "secrets.get",
                                    &aos_proto::SecretGetRequest {
                                        name: name.to_string(),
                                        actor: String::new(),
                                    },
                                    vec![],
                                )
                                .await
                            {
                                Ok(k) => api_key = Some(k),
                                Err(e) => {
                                    let _ = ctx
                                        .respond_error(
                                            aos_ipc::msg::Status::PermissionDenied,
                                            &format!("secret {name}: {e}"),
                                        )
                                        .await;
                                    return;
                                }
                            }
                        }
                        sub.add_remote_backend(
                            &req.model_id,
                            &req.endpoint,
                            req.remote_model.as_deref().unwrap_or("gpt-mock"),
                            api_key,
                        );
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &true).await;
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }
    {
        let sub = subsystem.clone();
        svc.on("model.set_routing", move |ctx| {
            let sub = sub.clone();
            async move {
                match ctx.payload::<aos_proto::SetRoutingRequest>() {
                    Ok(req) => {
                        let r = sub.set_routing(&req.mode);
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &r).await;
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }

    // --- model.metrics ---
    {
        let sub = subsystem.clone();
        svc.on("model.metrics", move |ctx| {
            let sub = sub.clone();
            async move {
                let mut sysinfo = sysinfo::System::new();
                sysinfo.refresh_memory();
                sysinfo.refresh_cpu_all();
                let metrics = sub.metrics(
                    (sysinfo.total_memory(), sysinfo.used_memory()),
                    sysinfo.global_cpu_usage(),
                );
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &metrics).await;
            }
        });
    }

    // --- media.image.generate / media.audio.generate (E16) ---
    {
        let sub = subsystem.clone();
        let bus = bus.clone();
        svc.on("media.image.generate", move |ctx| {
            let sub = sub.clone();
            let bus = bus.clone();
            async move {
                match ctx.payload::<MediaImageGenerateRequest>() {
                    Ok(req) => {
                        if !aos_model::media::actor_may_generate(&req.actor, &req.caps) {
                            let _ = ctx
                                .respond_error(
                                    aos_ipc::msg::Status::PermissionDenied,
                                    "cap media.generate requise",
                                )
                                .await;
                            return;
                        }
                        let dest =
                            req.path
                                .clone()
                                .filter(|p| !p.is_empty())
                                .unwrap_or_else(|| {
                                    aos_model::media::default_media_image_dest(&req.options)
                                });
                        match media::run_image(&sub, &bus, &req, &dest).await {
                            Ok(resp) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::InternalError, &e)
                                    .await;
                            }
                        }
                    }
                    Err(_) => {
                        let _ = bus
                            .call::<aos_proto::AuditAppendRequest, bool>(
                                "audit.append",
                                &aos_proto::AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor: "service:modeld".into(),
                                    action: "media.options.refuse".into(),
                                    target: "media.image.generate".into(),
                                    detail: serde_json::json!({"reason": "unknown_or_invalid"}),
                                },
                                vec![],
                            )
                            .await;
                        let _ = ctx
                            .respond_error(
                                aos_ipc::msg::Status::BadRequest,
                                "payload invalide (clés d'options inconnues refusées)",
                            )
                            .await;
                    }
                }
            }
        });
    }
    {
        let bus = bus.clone();
        svc.on("media.image.upscale", move |ctx| {
            let bus = bus.clone();
            async move {
                match ctx.payload::<MediaImageUpscaleRequest>() {
                    Ok(req) => {
                        if !aos_model::media::actor_may_generate(&req.actor, &req.caps) {
                            let _ = ctx
                                .respond_error(
                                    aos_ipc::msg::Status::PermissionDenied,
                                    "cap media.generate requise",
                                )
                                .await;
                            return;
                        }
                        let dest = req
                            .output_path
                            .clone()
                            .filter(|p| !p.is_empty())
                            .unwrap_or_else(|| {
                                aos_model::media::default_upscaled_path(&req.source_path)
                            });
                        match media::run_image_upscale(&bus, &req, &dest).await {
                            Ok(resp) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::InternalError, &e)
                                    .await;
                            }
                        }
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }
    {
        let sub = subsystem.clone();
        let bus = bus.clone();
        svc.on("media.audio.generate", move |ctx| {
            let sub = sub.clone();
            let bus = bus.clone();
            async move {
                match ctx.payload::<MediaAudioGenerateRequest>() {
                    Ok(req) => {
                        if !aos_model::media::actor_may_generate(&req.actor, &req.caps) {
                            let _ = ctx
                                .respond_error(
                                    aos_ipc::msg::Status::PermissionDenied,
                                    "cap media.generate requise",
                                )
                                .await;
                            return;
                        }
                        let dest = req
                            .path
                            .clone()
                            .filter(|p| !p.is_empty())
                            .unwrap_or_else(aos_model::media::default_audio_path);
                        match media::run_tts(&sub, &bus, &req, &dest).await {
                            Ok(resp) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::InternalError, &e)
                                    .await;
                            }
                        }
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }

    // --- provider.* (P08.12) ---
    {
        svc.on("provider.list", move |ctx| async move {
            let list = aos_model::providers::load_all();
            let _ = ctx
                .respond(
                    aos_ipc::msg::Status::Ok,
                    &aos_proto::ProviderListResponse { providers: list },
                )
                .await;
        });
    }
    {
        let sub = subsystem.clone();
        let bus = bus.clone();
        svc.on("provider.upsert", move |ctx| {
            let sub = sub.clone();
            let bus = bus.clone();
            async move {
                match ctx.payload::<aos_proto::ProviderUpsertRequest>() {
                    Ok(req) => {
                        if let Err(e) = aos_model::providers::save(&req.provider) {
                            let _ = ctx
                                .respond_error(aos_ipc::msg::Status::InternalError, &e)
                                .await;
                            return;
                        }
                        sub.remove_provider_models(&req.provider.id);
                        if req.provider.enabled {
                            providers::apply_provider_models(&sub, &bus, &req.provider).await;
                        }
                        let _ = bus
                            .call::<aos_proto::AuditAppendRequest, bool>(
                                "audit.append",
                                &aos_proto::AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor: "human:ui".into(),
                                    action: "provider.add".into(),
                                    target: req.provider.id.clone(),
                                    detail: serde_json::json!({
                                        "endpoint": req.provider.endpoint,
                                        "preset": req.provider.preset,
                                    }),
                                },
                                vec![],
                            )
                            .await;
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &req.provider).await;
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }
    {
        let sub = subsystem.clone();
        svc.on("provider.remove", move |ctx| {
            let sub = sub.clone();
            async move {
                match ctx.payload::<aos_proto::ProviderIdRequest>() {
                    Ok(req) => {
                        let _ = aos_model::providers::remove(&req.id);
                        sub.remove_provider_models(&req.id);
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &true).await;
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }
    {
        let bus = bus.clone();
        svc.on("provider.test", move |ctx| {
            let bus = bus.clone();
            async move {
                match ctx.payload::<aos_proto::ProviderIdRequest>() {
                    Ok(req) => {
                        let Some(p) = aos_model::providers::load_all()
                            .into_iter()
                            .find(|x| x.id == req.id)
                        else {
                            let _ = ctx
                                .respond_error(aos_ipc::msg::Status::NotFound, "provider inconnu")
                                .await;
                            return;
                        };
                        let key =
                            providers::fetch_provider_secret(&bus, p.secret_name.as_deref()).await;
                        let be = aos_model::RemoteOpenAiBackend::new(&p.endpoint, "probe", key);
                        let models = be.list_models().await.unwrap_or_default();
                        let ok = be.health().await || !models.is_empty();
                        let mut rec = p;
                        if !models.is_empty() {
                            rec.discovered_models = models.clone();
                            let _ = aos_model::providers::save(&rec);
                        }
                        let _ = bus
                            .call::<aos_proto::AuditAppendRequest, bool>(
                                "audit.append",
                                &aos_proto::AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor: "human:ui".into(),
                                    action: "provider.test".into(),
                                    target: rec.id.clone(),
                                    detail: serde_json::json!({ "ok": ok, "n": models.len() }),
                                },
                                vec![],
                            )
                            .await;
                        let _ = ctx
                            .respond(
                                aos_ipc::msg::Status::Ok,
                                &aos_proto::ProviderTestResponse {
                                    ok,
                                    message: if ok {
                                        format!("{} modèle(s)", models.len())
                                    } else {
                                        "injoignable".into()
                                    },
                                    models,
                                },
                            )
                            .await;
                    }
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                    }
                }
            }
        });
    }

    for p in aos_model::providers::load_all() {
        if p.enabled {
            providers::apply_provider_models(&subsystem, &bus, &p).await;
        }
    }

    eprintln!("[aos-modeld] prêt");
    let _ = svc.serve(&config.bus).await;
}

#[cfg(test)]
mod tests {
    use super::{
        greedy_token, lan_artifact_component, last_token_activation, materialize_staged_lan_model,
        parse_lan_session_key, sample_token, stage_lan_weight_shard,
    };

    #[test]
    fn cle_lan_hex_est_strictement_validee() {
        assert!(parse_lan_session_key(&"ab".repeat(32)).is_ok());
        assert!(parse_lan_session_key(&"zz".repeat(32)).is_err());
        assert!(parse_lan_session_key(&"ab".repeat(31)).is_err());
    }

    #[test]
    fn generation_reutilise_le_dernier_token_et_choisit_le_maximum() {
        let tensor = aos_placement::F32Tensor::new(vec![2, 2], vec![1.0, 2.0, 3.0, 4.0]).unwrap();
        let last =
            aos_placement::F32Tensor::decode(&last_token_activation(&tensor.encode()).unwrap())
                .unwrap();
        assert_eq!(last.shape, vec![2, 1]);
        assert_eq!(last.values, vec![2.0, 4.0]);
        assert_eq!(
            greedy_token(&[f32::NEG_INFINITY, 1.0, 3.0, 2.0]).unwrap(),
            2
        );
        assert!(greedy_token(&[f32::NAN, f32::INFINITY]).is_err());
        let mut seed_a = 7;
        let mut seed_b = 7;
        assert_eq!(
            sample_token(&[0.0, 1.0, 2.0], 0.8, 0.95, &mut seed_a).unwrap(),
            sample_token(&[0.0, 1.0, 2.0], 0.8, 0.95, &mut seed_b).unwrap()
        );
    }

    #[test]
    fn staging_poids_lan_est_atomique_et_sanitise() {
        let root = std::env::temp_dir().join(format!("akasha-weight-stage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            lan_artifact_component("work/with spaces"),
            "work_with_spaces"
        );
        stage_lan_weight_shard(
            &root,
            "work/with spaces",
            "model:test",
            7,
            4,
            10,
            Vec::new(),
            b"abc",
        )
        .unwrap();
        let directory = root.join("lan-shards").join("work_with_spaces");
        assert_eq!(
            std::fs::read(directory.join("shard-00007-offset-4.bin")).unwrap(),
            b"abc"
        );
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(directory.join("shard-00007-offset-4.manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["model_id"], "model:test");
        assert_eq!(manifest["length"], 3);
        assert!(!directory.join("shard-00007-offset-4.part").exists());
        // Retrying identical bytes succeeds; different bytes preserve the shard.
        stage_lan_weight_shard(
            &root,
            "work/with spaces",
            "model:test",
            7,
            4,
            10,
            Vec::new(),
            b"abc",
        )
        .unwrap();
        assert!(stage_lan_weight_shard(
            &root,
            "work/with spaces",
            "model:test",
            7,
            4,
            10,
            Vec::new(),
            b"xyz"
        )
        .is_err());
        assert_eq!(
            std::fs::read(directory.join("shard-00007-offset-4.bin")).unwrap(),
            b"abc"
        );
        std::thread::scope(|scope| {
            scope.spawn(|| {
                stage_lan_weight_shard(
                    &root,
                    "work/with spaces",
                    "model:test",
                    0,
                    0,
                    10,
                    Vec::new(),
                    b"0123",
                )
                .unwrap()
            });
            scope.spawn(|| {
                stage_lan_weight_shard(
                    &root,
                    "work/with spaces",
                    "model:test",
                    8,
                    7,
                    10,
                    Vec::new(),
                    b"789",
                )
                .unwrap()
            });
        });
        let coverage: aos_placement::LanShardManifest =
            serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap())
                .unwrap();
        assert!(coverage.is_complete());
        assert_eq!(coverage.ranges.len(), 3);
        let staged = materialize_staged_lan_model(&root, "work/with spaces", "model:test")
            .unwrap()
            .unwrap();
        assert_eq!(std::fs::read(staged).unwrap(), b"0123abc789");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn staging_sparse_materialise_apres_couverture_requise() {
        let root = std::env::temp_dir().join(format!("akasha-sparse-stage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let required = vec![
            aos_placement::LanWeightRange {
                shard_id: 3,
                offset: 0,
                length: 3,
            },
            aos_placement::LanWeightRange {
                shard_id: 3,
                offset: 7,
                length: 3,
            },
        ];
        stage_lan_weight_shard(
            &root,
            "sparse-work",
            "model:sparse",
            3,
            0,
            10,
            required.clone(),
            b"abc",
        )
        .unwrap();
        assert!(
            materialize_staged_lan_model(&root, "sparse-work", "model:sparse")
                .unwrap()
                .is_none()
        );
        stage_lan_weight_shard(
            &root,
            "sparse-work",
            "model:sparse",
            3,
            7,
            10,
            required,
            b"hij",
        )
        .unwrap();
        let staged = materialize_staged_lan_model(&root, "sparse-work", "model:sparse")
            .unwrap()
            .unwrap();
        assert_eq!(std::fs::read(staged).unwrap(), b"abc\0\0\0\0hij");
        std::fs::remove_dir_all(root).unwrap();
    }
}
