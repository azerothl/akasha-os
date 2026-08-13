//! `aos-modeld` — daemon du Model Subsystem (P1.1–P1.3).
//!
//! Usage : `aos-modeld [config.yaml]` (défaut `demo/modeld.dev.yaml`).

use aos_ipc::{BusClient, BusService, StreamHandle};
use aos_model::{ModelSubsystem, ModeldConfig};
use aos_placement::PlacementProfile;
use aos_proto::{
    CancelRequest, InferRequest, LoadRequest, ModelIdRequest, TokenEvent, UnloadRequest,
};
use aos_registry::ModelRegistry;
use std::sync::Arc;

fn parse_profile(s: &str) -> PlacementProfile {
    match s {
        "latency" => PlacementProfile::Latency,
        "memory-saver" => PlacementProfile::MemorySaver,
        "cpu-only" => PlacementProfile::CpuOnly,
        _ => PlacementProfile::Balanced,
    }
}

/// Extrait host/port d'un endpoint (`http(s)://hote[:port]/...`).
fn parse_host_port(endpoint: &str) -> (String, u16) {
    let without_scheme = endpoint
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let hostport = without_scheme.split('/').next().unwrap_or("");
    match hostport.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().unwrap_or(443)),
        None => (
            hostport.to_string(),
            if endpoint.starts_with("https") {
                443
            } else {
                80
            },
        ),
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
                let req: InferRequest = match ctx.payload() {
                    Ok(r) => r,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
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
                    } else if mode == "local_only" {
                        let _ = stream
                            .send(&TokenEvent::Error {
                                message: "mode local_only : backend distant interdit".into(),
                            })
                            .await;
                        let _ = stream.finish(aos_ipc::msg::Status::PermissionDenied).await;
                        return;
                    } else {
                        // 2. Contrôle d'egress (§9.5) via platformd net.check.
                        let endpoint = sub.remote_endpoint(&model_id).unwrap_or_default();
                        let (host, port) = parse_host_port(&endpoint);
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

    // --- model.backend.add / model.set_routing (P3) ---
    {
        let sub = subsystem.clone();
        svc.on("model.backend.add", move |ctx| {
            let sub = sub.clone();
            async move {
                match ctx.payload::<aos_proto::BackendAddRequest>() {
                    Ok(req) => {
                        sub.add_remote_backend(
                            &req.model_id,
                            &req.endpoint,
                            req.remote_model.as_deref().unwrap_or("gpt-mock"),
                            None,
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

    eprintln!("[aos-modeld] prêt");
    let _ = svc.serve(&config.bus).await;
}
