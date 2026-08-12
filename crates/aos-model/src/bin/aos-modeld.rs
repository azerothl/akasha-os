//! `aos-modeld` — daemon du Model Subsystem (P1.1–P1.3).
//!
//! Usage : `aos-modeld [config.yaml]` (défaut `demo/modeld.dev.yaml`).

use aos_ipc::{BusService, StreamHandle};
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

#[tokio::main]
async fn main() {
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "demo/modeld.dev.yaml".to_string());
    let config = ModeldConfig::load(&config_path).expect("chargement config modeld");
    let registry = ModelRegistry::load("data/models/catalog.yaml").expect("catalogue");

    let mut sysinfo = sysinfo::System::new_all();
    sysinfo.refresh_memory();
    let ram_total = sysinfo.total_memory();

    let subsystem = Arc::new(ModelSubsystem::new(config.clone(), &registry, ram_total));
    eprintln!(
        "[aos-modeld] {} modèles au registry, bus {}",
        registry.len(),
        config.bus
    );

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

    // --- model.infer (flux) ---
    {
        let sub = subsystem.clone();
        svc.on("model.infer", move |ctx| {
            let sub = sub.clone();
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
                let model_id = match req
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
