//! `aos-modeld` — daemon du Model Subsystem (P1.1–P1.3).
//!
//! Usage : `aos-modeld [config.yaml]` (défaut `demo/modeld.dev.yaml`).

use aos_ipc::{BusClient, BusService, StreamHandle};
use aos_model::{media, providers, ModelSubsystem, ModeldConfig};
use aos_placement::{
    BackendKind, DistributedWork, InferencePlanDiagnostic, LanCluster, LanNode, LanPairingRegistry,
    LanSessionKey, LanTcpTransport, LanWorkMessage, LanWorkPlan, NodeTrust, PlacementProfile,
    ThermalPolicy,
};
use aos_proto::{
    CancelRequest, InferRequest, LanClusterAssignment, LanClusterDiscoverRequest,
    LanClusterDispatchRequest, LanClusterDispatchResponse, LanClusterJobRequest, LanClusterNode,
    LanClusterNodeRequest, LanClusterNodesResponse, LanClusterPairRequest, LanClusterPlanRequest,
    LanClusterPlanResponse, LoadRequest, MediaAudioGenerateRequest, MediaImageGenerateRequest,
    MediaImageUpscaleRequest, MigrateRequest, ModelIdRequest, ModelPlanDiagnostic,
    ModelPlanRequest, TokenEvent, UnloadRequest,
};
use aos_registry::ModelRegistry;
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

fn thermal_name(policy: ThermalPolicy) -> &'static str {
    match policy {
        ThermalPolicy::Performance => "performance",
        ThermalPolicy::Balanced => "balanced",
        ThermalPolicy::Quiet => "quiet",
        ThermalPolicy::AlwaysOn => "always-on",
    }
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
    for (index, chunk) in raw.as_bytes().chunks_exact(2).enumerate() {
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
        if let Err(error) = transport.send_message(&message, work).await {
            errors.push(format!("{node_id}: {error}"));
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
        };
        match transport.send_message(&message, work).await {
            Ok(()) => dispatched.push(assignment.node_id.clone()),
            Err(error) => errors.push(format!("{}: {error}", assignment.node_id)),
        }
    }
    (dispatched, errors)
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
    eprintln!(
        "[aos-modeld] {} modèles au registry, bus {}",
        registry.len(),
        config.bus
    );

    // Client bus pour appels sortants (platformd : fs.class, net.check, audit).
    let bus = BusClient::connect(&config.bus, "modeld")
        .await
        .expect("connexion au bus — lancer aos-busd d'abord");

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
        let model_config = config.clone();
        let preference_home = preference_home.clone();
        let bus = bus.clone();
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
                    };
                    match transport.send_message(&message, &work).await {
                        Ok(()) => dispatched_nodes.push(assignment.node_id.clone()),
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
                            .and_then(|mut cluster| cluster.plan(&work, req.kv_tokens));
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
    use super::parse_lan_session_key;

    #[test]
    fn cle_lan_hex_est_strictement_validee() {
        assert!(parse_lan_session_key(&"ab".repeat(32)).is_ok());
        assert!(parse_lan_session_key(&"zz".repeat(32)).is_err());
        assert!(parse_lan_session_key(&"ab".repeat(31)).is_err());
    }
}
