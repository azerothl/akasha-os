//! `aos-platformd` — daemon plateforme P2 : audit, storage, memory, modules.
//!
//! Usage : `aos-platformd [config.yaml]` (défaut `demo/platformd.dev.yaml`).

use aos_ipc::BusService;
use aos_platform::subsystem::{PlatformConfig, PlatformSubsystem};
use aos_proto::*;

/// Résout les caps FS : si l'enveloppe porte des `cap://kernel/<id>`,
/// le noyau `aos-capkd` est le seul juge (fail-closed). Sinon, caps
/// logiques P1-P3 du payload.
async fn resolve_fs_caps(
    s: &PlatformSubsystem,
    envelope: &[String],
    holder: &str,
    path: &str,
    kernel_right: &str,
    string_kind: &str,
    string_caps: Vec<String>,
) -> Result<Vec<String>, String> {
    match s
        .authorize_kernel(
            envelope,
            holder,
            &format!("fs:{path}"),
            &[kernel_right.to_string()],
        )
        .await
    {
        Some(Ok(())) => Ok(vec![format!("{string_kind}:{path}")]),
        Some(Err(e)) => Err(e),
        None => Ok(string_caps),
    }
}

#[tokio::main]
async fn main() {
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "demo/platformd.dev.yaml".to_string());
    let config = PlatformConfig::load(&config_path).expect("config platformd");
    let sub = PlatformSubsystem::open(&config).expect("ouverture plateforme");
    eprintln!("[aos-platformd] bus {}", config.bus);

    // Client bus : forwarding de l'audit vers aos-auditd (P4.4).
    match aos_ipc::BusClient::connect(&config.bus, "platformd").await {
        Ok(bus) => sub.set_bus(bus),
        Err(e) => eprintln!("[aos-platformd] bus injoignable ({e}) — audit local uniquement"),
    }

    let mut svc = BusService::new("platformd");

    // Note P4.4 : `audit.append` est servi par `aos-auditd` (service autonome,
    // isolation de panne) ; platformd y forwarde ses événements (voir
    // `PlatformSubsystem::audit`). `audit.query/verify` restent servis ici
    // depuis le journal local (synchrone, pas de race avec les gates).

    // --- audit.query / audit.verify (journal local) ---
    {
        let s = sub.clone();
        svc.on("audit.query", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<AuditQueryRequest>() {
                    Ok(req) => {
                        let events = s.audit.lock().unwrap().query(&req);
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &events).await;
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
        let s = sub.clone();
        svc.on("audit.verify", move |ctx| {
            let s = s.clone();
            async move {
                let ok = s.audit.lock().unwrap().verify().is_ok();
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &ok).await;
            }
        });
    }

    // --- fs.* ---
    {
        let s = sub.clone();
        svc.on("fs.read", move |ctx| {
            let s = s.clone();
            async move {
                let envelope = ctx.intent.caps.clone();
                let from = ctx.intent.from.clone();
                match ctx.payload::<FsReadRequest>() {
                    Ok(req) => {
                        let holder = if req.actor.is_empty() {
                            from
                        } else {
                            req.actor.clone()
                        };
                        let caps = match resolve_fs_caps(
                            &s,
                            &envelope,
                            &holder,
                            &req.path,
                            "read",
                            "fs.read",
                            req.caps.clone(),
                        )
                        .await
                        {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::PermissionDenied, &e)
                                    .await;
                                return;
                            }
                        };
                        let r = s.fs.lock().unwrap().read(&req.path, &caps);
                        match r {
                            Ok((content, class, version)) => {
                                let _ = ctx
                                    .respond(
                                        aos_ipc::msg::Status::Ok,
                                        &FsReadResponse {
                                            path: req.path,
                                            content,
                                            class,
                                            version,
                                        },
                                    )
                                    .await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::PermissionDenied,
                                        &e.to_string(),
                                    )
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
        let s = sub.clone();
        svc.on("fs.write", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<FsWriteRequest>() {
                    Ok(req) => {
                        let envelope = ctx.intent.caps.clone();
                        let from = ctx.intent.from.clone();
                        let trace = req.trace_id.clone();
                        let actor = if req.actor.is_empty() {
                            "human:ui".to_string()
                        } else {
                            req.actor.clone()
                        };
                        let holder = if req.actor.is_empty() {
                            from
                        } else {
                            req.actor.clone()
                        };
                        let caps = match resolve_fs_caps(
                            &s,
                            &envelope,
                            &holder,
                            &req.path,
                            "write",
                            "fs.write",
                            req.caps.clone(),
                        )
                        .await
                        {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::PermissionDenied, &e)
                                    .await;
                                return;
                            }
                        };
                        let r =
                            s.fs.lock()
                                .unwrap()
                                .write(&req.path, &req.content, &actor, &caps);
                        match r {
                            Ok(version) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: trace,
                                    actor,
                                    action: "fs.write".into(),
                                    target: req.path.clone(),
                                    detail: serde_json::json!({"version": version}),
                                });
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &version).await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::PermissionDenied,
                                        &e.to_string(),
                                    )
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
        let s = sub.clone();
        svc.on("fs.list", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<FsListRequest>() {
                    Ok(req) => {
                        let entries = s.fs.lock().unwrap().list(&req.prefix, &req.caps);
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &entries).await;
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
        let s = sub.clone();
        svc.on("fs.begin_tx", move |ctx| {
            let s = s.clone();
            async move {
                let actor = ctx
                    .payload::<FsTxRequest>()
                    .map(|r| r.actor)
                    .unwrap_or_else(|_| "human:ui".into());
                let tx = s.fs.lock().unwrap().begin_tx(&actor);
                let _ = ctx
                    .respond(
                        aos_ipc::msg::Status::Ok,
                        &FsTxResponse {
                            tx_id: tx,
                            committed_ops: 0,
                        },
                    )
                    .await;
            }
        });
    }
    {
        let s = sub.clone();
        svc.on("fs.commit", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<FsTxRequest>() {
                    Ok(req) => {
                        let id = req.tx_id.unwrap_or_default();
                        let r = s.fs.lock().unwrap().commit(&id);
                        match r {
                            Ok(n) => {
                                let _ = ctx
                                    .respond(
                                        aos_ipc::msg::Status::Ok,
                                        &FsTxResponse {
                                            tx_id: id,
                                            committed_ops: n as u32,
                                        },
                                    )
                                    .await;
                            }
                            Err(e) => {
                                if matches!(e, aos_platform::storage::FsError::Conflict(_)) {
                                    // Arbitrage superviseur (§4.6) : le perdant
                                    // est notifié via le flux supervisor.
                                    s.audit(AuditAppendRequest {
                                        trace_id: String::new(),
                                        actor: "service:platformd".into(),
                                        action: "fs.conflict".into(),
                                        target: id.clone(),
                                        detail: serde_json::json!({"error": e.to_string()}),
                                    });
                                }
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::BadRequest, &e.to_string())
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
        let s = sub.clone();
        svc.on("fs.rollback", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<FsTxRequest>() {
                    Ok(req) => {
                        let id = req.tx_id.unwrap_or_default();
                        let r = s.fs.lock().unwrap().rollback(&id);
                        let _ = ctx
                            .respond(aos_ipc::msg::Status::Ok, &r.map_err(|e| e.to_string()))
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
    // --- fs.delete (confirmée par politique, §9.4) ---
    {
        let s = sub.clone();
        svc.on("fs.delete", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<FsDeleteRequest>() {
                    Ok(req) => {
                        let envelope = ctx.intent.caps.clone();
                        let from = ctx.intent.from.clone();
                        let trace = req.trace_id.clone();
                        let actor = if req.actor.is_empty() {
                            "human:ui".to_string()
                        } else {
                            req.actor.clone()
                        };
                        let holder = if req.actor.is_empty() {
                            from
                        } else {
                            req.actor.clone()
                        };
                        // Policy gate : require_confirmation pour fs.delete.
                        let allowed = s
                            .policy_gate(
                                std::collections::HashMap::from([(
                                    "actor".to_string(),
                                    actor.clone(),
                                )]),
                                &actor,
                                "fs.delete",
                                &req.path,
                                &trace,
                            )
                            .await;
                        if !allowed {
                            let _ = ctx
                                .respond_error(
                                    aos_ipc::msg::Status::PermissionDenied,
                                    "action refusée (politique ou timeout de confirmation)",
                                )
                                .await;
                            return;
                        }
                        let caps = match resolve_fs_caps(
                            &s,
                            &envelope,
                            &holder,
                            &req.path,
                            "write",
                            "fs.write",
                            req.caps.clone(),
                        )
                        .await
                        {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::PermissionDenied, &e)
                                    .await;
                                return;
                            }
                        };
                        let r = s.fs.lock().unwrap().delete(&req.path, &actor, &caps);
                        match r {
                            Ok(v) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: trace,
                                    actor,
                                    action: "fs.delete".into(),
                                    target: req.path.clone(),
                                    detail: serde_json::json!({}),
                                });
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &v).await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::PermissionDenied,
                                        &e.to_string(),
                                    )
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
        let s = sub.clone();
        svc.on("fs.set_class", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<FsSetClassRequest>() {
                    Ok(req) => {
                        let r =
                            s.fs.lock()
                                .unwrap()
                                .set_class(&req.path, req.class, &req.caps);
                        let _ = ctx
                            .respond(aos_ipc::msg::Status::Ok, &r.map_err(|e| e.to_string()))
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

    // --- mem.* ---
    {
        let s = sub.clone();
        svc.on("mem.working_set", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<MemWorkingRequest>() {
                    Ok(req) => {
                        s.mem
                            .lock()
                            .unwrap()
                            .working_set(&req.agent_id, req.messages);
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
        let s = sub.clone();
        svc.on("mem.working_get", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<MemWorkingRequest>() {
                    Ok(req) => {
                        let msgs = s.mem.lock().unwrap().working_get(&req.agent_id);
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &msgs).await;
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
        let s = sub.clone();
        svc.on("mem.episodic_write", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<MemEpisodicWriteRequest>() {
                    Ok(req) => {
                        let s2 = s.clone();
                        let r = tokio::task::spawn_blocking(move || {
                            let vector = s2.embed_text(&req.text)?;
                            Ok::<_, String>(s2.mem.lock().unwrap().episodic_write(
                                &req.namespace,
                                &req.text,
                                req.metadata,
                                vector,
                                req.pinned,
                            ))
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()));
                        match r {
                            Ok(id) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &id).await;
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
        let s = sub.clone();
        svc.on("mem.episodic_query", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<MemEpisodicQueryRequest>() {
                    Ok(req) => {
                        let s2 = s.clone();
                        let r = tokio::task::spawn_blocking(move || {
                            let vector = s2.embed_text(&req.query)?;
                            Ok::<_, String>(s2.mem.lock().unwrap().episodic_query(
                                &vector,
                                req.k,
                                req.namespace.as_deref(),
                            ))
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()));
                        match r {
                            Ok(hits) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &hits).await;
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
        let s = sub.clone();
        svc.on("mem.export", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<MemWorkingRequest>() {
                    Ok(req) => {
                        let entries = s.mem.lock().unwrap().export(&req.agent_id);
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &entries).await;
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
        let s = sub.clone();
        svc.on("mem.wipe", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<MemWorkingRequest>() {
                    Ok(req) => {
                        let n = s.mem.lock().unwrap().wipe(&req.agent_id);
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &n).await;
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
        let s = sub.clone();
        svc.on("mem.stats", move |ctx| {
            let s = s.clone();
            async move {
                let (total, namespaces, working) = s.mem.lock().unwrap().stats();
                let _ = ctx
                    .respond(
                        aos_ipc::msg::Status::Ok,
                        &MemStats {
                            episodic_total: total,
                            namespaces,
                            working_agents: working,
                        },
                    )
                    .await;
            }
        });
    }

    // --- module.* ---
    {
        let s = sub.clone();
        svc.on("module.install", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<ModuleInstallRequest>() {
                    Ok(req) => {
                        let s2 = s.clone();
                        let r = tokio::task::spawn_blocking(move || {
                            s2.modules
                                .lock()
                                .unwrap()
                                .install(std::path::Path::new(&req.source_dir), req.approved_caps)
                        })
                        .await
                        .unwrap_or_else(|e| {
                            Err(aos_platform::module_rt::ModuleError::Io(e.to_string()))
                        });
                        match r {
                            Ok(info) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor: "human:ui".into(),
                                    action: "module.install".into(),
                                    target: info.name.clone(),
                                    detail: serde_json::json!({"caps": info.granted_caps}),
                                });
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &info).await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::InternalError,
                                        &e.to_string(),
                                    )
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
        let s = sub.clone();
        svc.on("module.list", move |ctx| {
            let s = s.clone();
            async move {
                let list = s.modules.lock().unwrap().list();
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &list).await;
            }
        });
    }
    {
        let s = sub.clone();
        svc.on("module.describe", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<ModuleIdRequest>() {
                    Ok(req) => {
                        let payload = {
                            let mods = s.modules.lock().unwrap();
                            match mods.describe(&req.module) {
                                Ok((manifest, caps)) => Ok(serde_json::json!({
                                    "manifest": manifest,
                                    "granted_caps": caps,
                                })),
                                Err(e) => Err(e.to_string()),
                            }
                        };
                        match payload {
                            Ok(v) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &v).await;
                            }
                            Err(e) => {
                                let _ = ctx.respond_error(aos_ipc::msg::Status::NotFound, &e).await;
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
        let s = sub.clone();
        svc.on("module.invoke", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<ModuleInvokeRequest>() {
                    Ok(req) => {
                        let s2 = s.clone();
                        let req2 = req.clone();
                        let r = tokio::task::spawn_blocking(move || {
                            s2.modules.lock().unwrap().invoke(
                                &req2.module,
                                &req2.tool,
                                &req2.args,
                                &req2.actor,
                                &req2.actor_caps,
                                &req2.trace_id,
                            )
                        })
                        .await
                        .unwrap_or_else(|e| {
                            Err(aos_platform::module_rt::ModuleError::Io(e.to_string()))
                        });
                        // Audit de l'appel d'outil (succès ou refus).
                        s.audit(AuditAppendRequest {
                            trace_id: req.trace_id.clone(),
                            actor: req.actor.clone(),
                            action: "tool.invoke".into(),
                            target: format!("{}.{}", req.module, req.tool),
                            detail: serde_json::json!({
                                "ok": r.is_ok(),
                                "error": r.as_ref().err().map(|e| e.to_string()),
                            }),
                        });
                        match r {
                            Ok(result) => {
                                let _ = ctx
                                    .respond(
                                        aos_ipc::msg::Status::Ok,
                                        &ModuleInvokeResponse {
                                            ok: true,
                                            result,
                                            error: None,
                                        },
                                    )
                                    .await;
                            }
                            Err(e) => {
                                let status = if e.to_string().contains("permission refusée")
                                    || e.to_string().contains("ActorDenied")
                                {
                                    aos_ipc::msg::Status::PermissionDenied
                                } else {
                                    aos_ipc::msg::Status::InternalError
                                };
                                let _ = ctx
                                    .respond(
                                        status,
                                        &ModuleInvokeResponse {
                                            ok: false,
                                            result: serde_json::Value::Null,
                                            error: Some(e.to_string()),
                                        },
                                    )
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
        let s = sub.clone();
        svc.on("module.uninstall", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<ModuleIdRequest>() {
                    Ok(req) => {
                        let r = s.modules.lock().unwrap().uninstall(&req.module);
                        let _ = ctx
                            .respond(aos_ipc::msg::Status::Ok, &r.map_err(|e| e.to_string()))
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

    // --- policy.evaluate / policy.reload ---
    {
        let s = sub.clone();
        svc.on("policy.evaluate", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<PolicyEvalRequest>() {
                    Ok(req) => {
                        let (effect, rule, timeout) = {
                            let p = s.policy.lock().unwrap();
                            let (e, r) = p.evaluate(&req.context);
                            (e, r.map(|r| r.name.clone()), p.timeout_of(r))
                        };
                        let _ = ctx
                            .respond(
                                aos_ipc::msg::Status::Ok,
                                &PolicyEvalResponse {
                                    effect,
                                    rule,
                                    timeout_sec: Some(timeout),
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
    {
        let s = sub.clone();
        svc.on("policy.reload", move |ctx| {
            let s = s.clone();
            async move {
                let r = s.policy.lock().unwrap().reload();
                let _ = ctx
                    .respond(aos_ipc::msg::Status::Ok, &r.map_err(|e| e.to_string()))
                    .await;
            }
        });
    }

    // --- confirm.subscribe (flux) / confirm.respond / confirm.list ---
    {
        let s = sub.clone();
        svc.on("confirm.subscribe", move |ctx| {
            let s = s.clone();
            async move {
                let mut rx = s.confirm.subscribe().await;
                let stream = ctx.open_stream();
                tokio::spawn(async move {
                    while let Some(p) = rx.recv().await {
                        if stream.send(&p).await.is_err() {
                            return;
                        }
                    }
                    let _ = stream.finish(aos_ipc::msg::Status::Ok).await;
                });
            }
        });
    }
    {
        let s = sub.clone();
        svc.on("confirm.respond", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<ConfirmResponseRequest>() {
                    Ok(req) => {
                        let found = s.confirm.respond(&req.id, req.approved).await;
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &found).await;
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
        let s = sub.clone();
        svc.on("confirm.list", move |ctx| {
            let s = s.clone();
            async move {
                let list = s.confirm.pending_list().await;
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &list).await;
            }
        });
    }

    // --- trust.get / trust.set / trust.reset ---
    {
        let s = sub.clone();
        svc.on("trust.get", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<TrustGetRequest>() {
                    Ok(req) => {
                        let p = s.trust.lock().unwrap().profile(&req.agent_id);
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &p).await;
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
        let s = sub.clone();
        svc.on("trust.set", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<TrustSetRequest>() {
                    Ok(req) => {
                        s.trust.lock().unwrap().set(&req.agent_id, req.score);
                        s.audit(AuditAppendRequest {
                            trace_id: String::new(),
                            actor: "human:ui".into(),
                            action: "trust.set".into(),
                            target: req.agent_id.clone(),
                            detail: serde_json::json!({"score": req.score}),
                        });
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
        let s = sub.clone();
        svc.on("trust.reset", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<TrustGetRequest>() {
                    Ok(req) => {
                        s.trust.lock().unwrap().reset(&req.agent_id);
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

    // --- cap.request (paliers de confiance, §4.7) ---
    {
        let s = sub.clone();
        svc.on("cap.request", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<CapRequestRequest>() {
                    Ok(req) => {
                        let decision = s.decide_cap_request(&req.agent_id, &req.cap);
                        match decision {
                            aos_platform::subsystem::CapDecision::Grant => {
                                s.grant_cap(&req.agent_id, &req.cap);
                                s.audit(AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor: req.agent_id.clone(),
                                    action: "cap.grant".into(),
                                    target: req.cap.clone(),
                                    detail: serde_json::json!({"via": "trust_tier", "confirmed": false}),
                                });
                                let _ = ctx
                                    .respond(
                                        aos_ipc::msg::Status::Ok,
                                        &CapRequestOutcome::Granted,
                                    )
                                    .await;
                            }
                            aos_platform::subsystem::CapDecision::Confirm => {
                                let trace = format!("cap-request-{}", std::process::id());
                                let allowed = s
                                    .policy_gate(
                                        std::collections::HashMap::from([(
                                            "action.kind".to_string(),
                                            "cap.request".to_string(),
                                        )]),
                                        &req.agent_id,
                                        "cap.request",
                                        &req.cap,
                                        &trace,
                                    )
                                    .await;
                                if allowed {
                                    s.grant_cap(&req.agent_id, &req.cap);
                                    let _ = ctx
                                        .respond(
                                            aos_ipc::msg::Status::Ok,
                                            &CapRequestOutcome::Granted,
                                        )
                                        .await;
                                } else {
                                    let _ = ctx
                                        .respond(
                                            aos_ipc::msg::Status::Ok,
                                            &CapRequestOutcome::Denied {
                                                reason: "confirmation refusée/timeout".into(),
                                            },
                                        )
                                        .await;
                                }
                            }
                            aos_platform::subsystem::CapDecision::Deny => {
                                s.audit(AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor: req.agent_id.clone(),
                                    action: "cap.deny".into(),
                                    target: req.cap.clone(),
                                    detail: serde_json::json!({"reason": "trust tier insuffisant"}),
                                });
                                let _ = ctx
                                    .respond(
                                        aos_ipc::msg::Status::Ok,
                                        &CapRequestOutcome::Denied {
                                            reason: "score de confiance insuffisant".into(),
                                        },
                                    )
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

    // --- net.check / net.set_mode / net.egress_log ---
    {
        let s = sub.clone();
        svc.on("net.check", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<NetCheckRequest>() {
                    Ok(req) => {
                        let allowed = s
                            .net
                            .lock()
                            .unwrap()
                            .check(&req.actor, &req.host, req.port, &req.caps);
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &allowed).await;
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
        let s = sub.clone();
        svc.on("net.set_mode", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<NetModeRequest>() {
                    Ok(req) => {
                        let mode = if req.mode == "offline_strict" {
                            aos_platform::net::NetMode::OfflineStrict
                        } else {
                            aos_platform::net::NetMode::Online
                        };
                        s.net.lock().unwrap().set_mode(mode);
                        s.audit(AuditAppendRequest {
                            trace_id: String::new(),
                            actor: "human:ui".into(),
                            action: "net.set_mode".into(),
                            target: req.mode.clone(),
                            detail: serde_json::json!({}),
                        });
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
        let s = sub.clone();
        svc.on("net.egress_log", move |ctx| {
            let s = s.clone();
            async move {
                let log: Vec<EgressEntry> = s.net.lock().unwrap().log().to_vec();
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &log).await;
            }
        });
    }

    // --- secrets.get (services seulement, §9.2) ---
    {
        let s = sub.clone();
        svc.on("secrets.get", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<SecretGetRequest>() {
                    Ok(req) => {
                        let r = {
                            let store = s.secrets.lock().unwrap();
                            store.get(&req.name, &req.actor)
                        };
                        match r {
                            Ok(v) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &v).await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::PermissionDenied,
                                        &e.to_string(),
                                    )
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

    // --- fs.class (routage privacy, §3.7/§6.4) ---
    {
        let s = sub.clone();
        svc.on("fs.class", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<FsClassRequest>() {
                    Ok(req) => {
                        let class = s.fs.lock().unwrap().class_of(&req.path).unwrap_or_default();
                        let _ = ctx
                            .respond(
                                aos_ipc::msg::Status::Ok,
                                &FsClassResponse {
                                    path: req.path,
                                    class,
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

    // --- supervisor.notifications (flux, §4.6) ---
    {
        let s = sub.clone();
        svc.on("supervisor.notifications", move |ctx| {
            let s = s.clone();
            async move {
                let mut rx = s.supervisor.subscribe().await;
                let stream = ctx.open_stream();
                tokio::spawn(async move {
                    while let Some(n) = rx.recv().await {
                        if stream.send(&n).await.is_err() {
                            return;
                        }
                    }
                    let _ = stream.finish(aos_ipc::msg::Status::Ok).await;
                });
            }
        });
    }

    eprintln!("[aos-platformd] prêt");
    let _ = svc.serve(&config.bus).await;
}
