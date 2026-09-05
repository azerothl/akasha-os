//! `aos-modeld` — daemon du Model Subsystem (P1.1–P1.3).
//!
//! Usage : `aos-modeld [config.yaml]` (défaut `demo/modeld.dev.yaml`).

use aos_ipc::{BusClient, BusService, StreamHandle};
use aos_model::{media, providers, ModelSubsystem, ModeldConfig};
use aos_placement::PlacementProfile;
use aos_proto::{
    CancelRequest, InferRequest, LoadRequest,
    MediaAudioGenerateRequest, MediaImageGenerateRequest, MediaImageUpscaleRequest,
    ModelIdRequest,
    TokenEvent, UnloadRequest, MigrateRequest,
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

    // --- model.plan (diagnostic, read-only) ---
    {
        let sub = subsystem.clone();
        svc.on("model.plan", move |ctx| {
            let sub = sub.clone();
            async move {
                match ctx.payload::<ModelIdRequest>() {
                    Ok(req) => match sub.diagnose(&req.model_id, sub.config.default_kv_tokens) {
                        Ok(plans) => {
                            let _ = ctx.respond(aos_ipc::msg::Status::Ok, &plans).await;
                        }
                        Err(e) => {
                            let _ = ctx
                                .respond_error(aos_ipc::msg::Status::NotFound, &e)
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
                        let dest = req
                            .path
                            .clone()
                            .filter(|p| !p.is_empty())
                            .unwrap_or_else(|| aos_model::media::default_media_image_dest(&req.options));
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
                        let key = providers::fetch_provider_secret(&bus, p.secret_name.as_deref()).await;
                        let be = aos_model::RemoteOpenAiBackend::new(
                            &p.endpoint,
                            "probe",
                            key,
                        );
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
