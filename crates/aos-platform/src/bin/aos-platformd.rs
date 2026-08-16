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
                            let kind = req
                                .kind
                                .as_deref()
                                .map(aos_platform::memory::MemoryKind::parse)
                                .unwrap_or_default();
                            let mut mem = s2.mem.lock().unwrap();
                            let (id, auto) = if req.auto_link {
                                mem.episodic_write_auto_link(
                                    &req.namespace,
                                    &req.text,
                                    req.metadata,
                                    vector,
                                    req.pinned,
                                    kind,
                                    req.auto_link_threshold,
                                )
                            } else {
                                let id = mem.episodic_write_kind(
                                    &req.namespace,
                                    &req.text,
                                    req.metadata,
                                    vector,
                                    req.pinned,
                                    kind,
                                );
                                (id, Vec::new())
                            };
                            Ok::<_, String>(MemRememberResponse {
                                id,
                                auto_relations: auto,
                            })
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()));
                        match r {
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
        svc.on("mem.episodic_delete", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<MemEpisodicDeleteRequest>() {
                    Ok(req) => {
                        let result = {
                            let mut mem = s.mem.lock().unwrap();
                            if let Some(id) = req.id {
                                let ok = mem.episodic_delete(id);
                                serde_json::json!({"deleted": ok, "count": if ok { 1 } else { 0 }})
                            } else if let (Some(ns), Some(key), Some(val)) = (
                                req.namespace.as_deref(),
                                req.meta_key.as_deref(),
                                req.meta_value.as_deref(),
                            ) {
                                let n = mem.episodic_delete_by_meta(ns, key, val);
                                serde_json::json!({"deleted": n > 0, "count": n})
                            } else {
                                drop(mem);
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::BadRequest,
                                        "id ou (namespace + meta_key + meta_value) requis",
                                    )
                                    .await;
                                return;
                            }
                        };
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &result).await;
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

    // --- mem.shared_* / mem.user.* / mem.context (PC.7) ---
    {
        let s = sub.clone();
        svc.on("mem.shared_read", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<MemSharedReadRequest>() {
                    Ok(req) => {
                        let v = s.mem.lock().unwrap().shared_read(&req.name);
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &v).await;
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
        svc.on("mem.shared_write", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<MemSharedWriteRequest>() {
                    Ok(req) => {
                        s.mem.lock().unwrap().shared_write(&req.name, req.value);
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
        svc.on("mem.user.remember", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<MemUserRememberRequest>() {
                    Ok(req) => {
                        let emb = s.embed_text(&req.text).unwrap_or_default();
                        let resp = {
                            let mut mem = s.mem.lock().unwrap();
                            let (id, auto) = if req.auto_link {
                                mem.episodic_write_auto_link(
                                    "user:default",
                                    &req.text,
                                    req.metadata,
                                    emb,
                                    req.pinned,
                                    aos_platform::memory::MemoryKind::Fact,
                                    req.auto_link_threshold,
                                )
                            } else {
                                let id = mem.episodic_write(
                                    "user:default",
                                    &req.text,
                                    req.metadata,
                                    emb,
                                    req.pinned,
                                );
                                (id, Vec::new())
                            };
                            MemRememberResponse {
                                id,
                                auto_relations: auto,
                            }
                        };
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
    {
        let s = sub.clone();
        svc.on("mem.user.recall", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<MemUserRecallRequest>() {
                    Ok(req) => {
                        let emb = s.embed_text(&req.query).unwrap_or_default();
                        let hits = s.mem.lock().unwrap().episodic_query(
                            &emb,
                            req.k,
                            Some("user:default"),
                        );
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &hits).await;
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
        svc.on("mem.context", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<MemContextRequest>() {
                    Ok(req) => {
                        let emb = s.embed_text(&req.query).unwrap_or_default();
                        let sess_ns = req
                            .session_id
                            .as_ref()
                            .map(|id| format!("session:{id}"));
                        let (session_hits, user_hits) = {
                            let mem = s.mem.lock().unwrap();
                            let session_hits = if let Some(ref ns) = sess_ns {
                                mem.episodic_query(&emb, req.k, Some(ns))
                            } else {
                                Vec::new()
                            };
                            let user_hits =
                                mem.episodic_query(&emb, req.k, Some("user:default"));
                            (session_hits, user_hits)
                        };
                        let mut prompt_block = String::new();
                        if !session_hits.is_empty() {
                            prompt_block.push_str("Mémoire session:\n");
                            for h in &session_hits {
                                prompt_block.push_str(&format!("- {}\n", h.text));
                            }
                        }
                        if !user_hits.is_empty() {
                            let structured = {
                                let mem = s.mem.lock().unwrap();
                                mem.bootstrap_block(&user_hits)
                            };
                            if structured.is_empty() {
                                prompt_block.push_str("Mémoire long terme utilisateur:\n");
                                for h in &user_hits {
                                    prompt_block.push_str(&format!("- {}\n", h.text));
                                }
                            } else {
                                prompt_block.push_str("Mémoire long terme utilisateur:\n");
                                prompt_block.push_str(&structured);
                            }
                        }
                        let _ = ctx
                            .respond(
                                aos_ipc::msg::Status::Ok,
                                &MemContextResponse {
                                    session_hits,
                                    user_hits,
                                    prompt_block,
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

    // --- mem.relate / neighbors / list / update (E6 / Preview 0.4) ---
    {
        let s = sub.clone();
        svc.on("mem.relate", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<MemRelateRequest>() {
                    Ok(req) => {
                        let result = s.mem.lock().unwrap().relate(req.from, req.rel, req.to);
                        match result {
                            Ok(edge) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &edge).await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::BadRequest, &e)
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
        svc.on("mem.unrelate", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<MemUnrelateRequest>() {
                    Ok(req) => {
                        let ok = s.mem.lock().unwrap().unrelate(req.from, req.rel, req.to);
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &ok).await;
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
        svc.on("mem.neighbors", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<MemNeighborsRequest>() {
                    Ok(req) => {
                        let hits = s.mem.lock().unwrap().neighbors(req.id, req.rel);
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &hits).await;
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
        svc.on("mem.list", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<MemListRequest>() {
                    Ok(req) => {
                        let hits = s
                            .mem
                            .lock()
                            .unwrap()
                            .list(&req.namespace, req.include_superseded);
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &hits).await;
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
        svc.on("mem.update", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<MemUpdateRequest>() {
                    Ok(req) => {
                        let s2 = s.clone();
                        let r = tokio::task::spawn_blocking(move || {
                            let vector = s2.embed_text(&req.text)?;
                            s2.mem.lock().unwrap().update(
                                req.id,
                                &req.text,
                                req.metadata,
                                req.pinned,
                                req.supersede,
                                vector,
                            )
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()));
                        match r {
                            Ok((id, auto)) => {
                                let _ = ctx
                                    .respond(
                                        aos_ipc::msg::Status::Ok,
                                        &MemRememberResponse {
                                            id,
                                            auto_relations: auto,
                                        },
                                    )
                                    .await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::BadRequest, &e)
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

    // --- chat.session.* (PC.6) ---
    {
        let s = sub.clone();
        svc.on("chat.session.create", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<ChatSessionCreateRequest>() {
                    Ok(req) => {
                        let result = s
                            .sessions
                            .lock()
                            .unwrap()
                            .create(req.title, req.model_id);
                        match result {
                            Ok(m) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &m).await;
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
        svc.on("chat.session.list", move |ctx| {
            let s = s.clone();
            async move {
                let result = s.sessions.lock().unwrap().list(false);
                match result {
                    Ok(list) => {
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &list).await;
                    }
                    Err(e) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::InternalError, &e.to_string())
                            .await;
                    }
                }
            }
        });
    }
    {
        let s = sub.clone();
        svc.on("chat.session.get", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<ChatSessionIdRequest>() {
                    Ok(req) => {
                        let result = s.sessions.lock().unwrap().get(&req.session_id);
                        match result {
                            Ok((meta, messages)) => {
                                let _ = ctx
                                    .respond(
                                        aos_ipc::msg::Status::Ok,
                                        &ChatSessionGetResponse { meta, messages },
                                    )
                                    .await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::NotFound,
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
        svc.on("chat.session.append", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<ChatSessionAppendRequest>() {
                    Ok(req) => {
                        let append_res = s.sessions.lock().unwrap().append(
                            &req.session_id,
                            &req.role,
                            &req.content,
                            req.attachments.clone(),
                        );
                        match append_res {
                            Ok(msg) => {
                                let wm = {
                                    let sessions = s.sessions.lock().unwrap();
                                    sessions.get(&req.session_id).ok().map(|(_, messages)| {
                                        messages
                                            .iter()
                                            .rev()
                                            .take(24)
                                            .rev()
                                            .map(|m| (m.role.clone(), m.content.clone()))
                                            .collect::<Vec<_>>()
                                    })
                                };
                                if let Some(wm) = wm {
                                    s.mem.lock().unwrap().working_set(
                                        &format!("session:{}", req.session_id),
                                        wm,
                                    );
                                }
                                // Faits épisodiques de session (assistant) pour recall.
                                if req.role == "assistant" && req.content.len() > 40 {
                                    let emb = s.embed_text(&req.content).unwrap_or_default();
                                    let excerpt: String =
                                        req.content.chars().take(400).collect();
                                    s.mem.lock().unwrap().episodic_write(
                                        &format!("session:{}", req.session_id),
                                        &excerpt,
                                        serde_json::json!({"role": "assistant"}),
                                        emb,
                                        false,
                                    );
                                }
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &msg).await;
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
        svc.on("chat.session.rename", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<ChatSessionRenameRequest>() {
                    Ok(req) => {
                        let result = s
                            .sessions
                            .lock()
                            .unwrap()
                            .rename(&req.session_id, &req.title);
                        match result {
                            Ok(m) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &m).await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::NotFound,
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
        svc.on("chat.session.set_model", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<ChatSessionSetModelRequest>() {
                    Ok(req) => {
                        let result = s
                            .sessions
                            .lock()
                            .unwrap()
                            .set_model(&req.session_id, req.model_id);
                        match result {
                            Ok(m) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &m).await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::NotFound,
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
        svc.on("chat.session.archive", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<ChatSessionIdRequest>() {
                    Ok(req) => {
                        let result = s.sessions.lock().unwrap().archive(&req.session_id);
                        match result {
                            Ok(m) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &m).await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::NotFound,
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
        svc.on("chat.session.delete", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<ChatSessionIdRequest>() {
                    Ok(req) => {
                        let result = s.sessions.lock().unwrap().delete(&req.session_id);
                        match result {
                            Ok(()) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &true).await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::NotFound,
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
        svc.on("chat.session.export", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<ChatSessionIdRequest>() {
                    Ok(req) => {
                        let result =
                            s.sessions.lock().unwrap().export_markdown(&req.session_id);
                        match result {
                            Ok(md) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &md).await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::NotFound,
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

    // --- web.search / net.fetch / files.generate / fs.*_bytes (PC.8–9) ---
    {
        let s = sub.clone();
        svc.on("web.search", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<WebSearchRequest>() {
                    Ok(req) => {
                        let actor = if req.actor.is_empty() {
                            "human:ui".into()
                        } else {
                            req.actor.clone()
                        };
                        let key = s
                            .secrets
                            .lock()
                            .unwrap()
                            .get("brave_search_api_key", "service:platformd")
                            .ok();
                        let search_res = {
                            let mut net = s.net.lock().unwrap();
                            aos_platform::net_services::web_search(
                                &mut net,
                                &actor,
                                &req.caps,
                                &req.query,
                                req.max_results,
                                key.as_deref(),
                                &req.engine,
                            )
                        };
                        match search_res {
                            Ok(resp) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: format!("web-search-{}", req.query.len()),
                                    actor,
                                    action: "web.search".into(),
                                    target: req.query,
                                    detail: serde_json::json!({ "n": resp.results.len() }),
                                });
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
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
        svc.on("net.fetch", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<NetFetchRequest>() {
                    Ok(req) => {
                        let actor = if req.actor.is_empty() {
                            "human:ui".into()
                        } else {
                            req.actor.clone()
                        };
                        let mut caps = req.caps.clone();
                        if !caps.iter().any(|c| c.starts_with("fs.write:")) {
                            caps.push("fs.write:/downloads/**".into());
                        }
                        let fetch_res = {
                            let mut net = s.net.lock().unwrap();
                            aos_platform::net_services::http_fetch_bytes(
                                &mut net,
                                &actor,
                                &caps,
                                &req.url,
                                req.max_bytes,
                            )
                        };
                        match fetch_res {
                            Ok((bytes, ctype)) => {
                                let name =
                                    aos_platform::net_services::safe_download_name(&req.url);
                                let path = req
                                    .dest_path
                                    .unwrap_or_else(|| format!("/downloads/{name}"));
                                let write_res = s.fs.lock().unwrap().write_bytes(
                                    &path,
                                    &bytes,
                                    &actor,
                                    &caps,
                                );
                                match write_res {
                                    Ok(_) => {
                                        s.audit(AuditAppendRequest {
                                            trace_id: "net-fetch".into(),
                                            actor,
                                            action: "net.fetch".into(),
                                            target: path.clone(),
                                            detail: serde_json::json!({
                                                "bytes": bytes.len(),
                                                "content_type": ctype,
                                            }),
                                        });
                                        let _ = ctx
                                            .respond(
                                                aos_ipc::msg::Status::Ok,
                                                &NetFetchResponse {
                                                    path,
                                                    bytes: bytes.len() as u64,
                                                    content_type: ctype,
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
        svc.on("web.browse", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<WebBrowseRequest>() {
                    Ok(req) => {
                        let actor = if req.actor.is_empty() {
                            "human:ui".into()
                        } else {
                            req.actor.clone()
                        };
                        let browse_res = {
                            let mut net = s.net.lock().unwrap();
                            aos_platform::net_services::web_browse(
                                &mut net,
                                &actor,
                                &req.caps,
                                &req.url,
                                req.max_chars,
                            )
                        };
                        match browse_res {
                            Ok(resp) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: format!("web-browse-{}", req.url.len()),
                                    actor,
                                    action: "web.browse".into(),
                                    target: req.url,
                                    detail: serde_json::json!({
                                        "title": resp.title,
                                        "chars": resp.text.len(),
                                    }),
                                });
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
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
        svc.on("files.generate", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<FilesGenerateRequest>() {
                    Ok(req) => {
                        let actor = if req.actor.is_empty() {
                            "human:ui".into()
                        } else {
                            req.actor.clone()
                        };
                        let mut caps = req.caps.clone();
                        if caps.is_empty() {
                            caps.push("fs.write:/downloads/**".into());
                            caps.push("fs.write:/documents/**".into());
                        }
                        match aos_platform::files_gen::generate(
                            &req.format,
                            &req.content,
                            req.title.as_deref(),
                        ) {
                            Ok(bytes) => {
                                let write_res = s.fs.lock().unwrap().write_bytes(
                                    &req.path,
                                    &bytes,
                                    &actor,
                                    &caps,
                                );
                                match write_res {
                                    Ok(_) => {
                                        s.audit(AuditAppendRequest {
                                            trace_id: "files-gen".into(),
                                            actor,
                                            action: "files.generate".into(),
                                            target: req.path.clone(),
                                            detail: serde_json::json!({
                                                "format": req.format,
                                                "bytes": bytes.len(),
                                            }),
                                        });
                                        let _ = ctx
                                            .respond(
                                                aos_ipc::msg::Status::Ok,
                                                &FilesGenerateResponse {
                                                    path: req.path,
                                                    bytes: bytes.len() as u64,
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
                            Err(e) => {
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
        svc.on("fs.write_bytes", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<FsWriteBytesRequest>() {
                    Ok(req) => {
                        use base64::Engine;
                        let bytes = match base64::engine::general_purpose::STANDARD
                            .decode(&req.content_b64)
                        {
                            Ok(b) => b,
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::BadRequest,
                                        &e.to_string(),
                                    )
                                    .await;
                                return;
                            }
                        };
                        let actor = if req.actor.is_empty() {
                            "human:ui".into()
                        } else {
                            req.actor
                        };
                        let write_res =
                            s.fs.lock()
                                .unwrap()
                                .write_bytes(&req.path, &bytes, &actor, &req.caps);
                        match write_res {
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
    {
        let s = sub.clone();
        svc.on("fs.read_bytes", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<FsReadBytesRequest>() {
                    Ok(req) => {
                        let read_res = s.fs.lock().unwrap().read_bytes(&req.path, &req.caps);
                        match read_res {
                            Ok((bytes, class, version)) => {
                                use base64::Engine;
                                let _ = ctx
                                    .respond(
                                        aos_ipc::msg::Status::Ok,
                                        &FsReadBytesResponse {
                                            path: req.path,
                                            content_b64: base64::engine::general_purpose::STANDARD
                                                .encode(&bytes),
                                            class,
                                            version,
                                            size_bytes: bytes.len() as u64,
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

    // --- module.* ---
    {
        let s = sub.clone();
        svc.on("module.install", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<ModuleInstallRequest>() {
                    Ok(req) => {
                        let actor = if req.actor.is_empty() {
                            "human:ui".into()
                        } else {
                            req.actor.clone()
                        };
                        // Gate : humains OK ; agents doivent détenir `module.install`.
                        let allowed = actor.starts_with("human:")
                            || req.actor_caps.iter().any(|c| c == "module.install")
                            || s.granted_caps
                                .lock()
                                .unwrap()
                                .get(actor.strip_prefix("agent:").unwrap_or(&actor))
                                .map(|caps| caps.iter().any(|c| c == "module.install"))
                                .unwrap_or(false);
                        if !allowed {
                            let _ = ctx
                                .respond_error(
                                    aos_ipc::msg::Status::PermissionDenied,
                                    "module.install : capacité requise (cap.request)",
                                )
                                .await;
                            return;
                        }
                        let s2 = s.clone();
                        let source = req.source_dir.clone();
                        let approved = req.approved_caps.clone();
                        let r = tokio::task::spawn_blocking(move || {
                            s2.modules
                                .lock()
                                .unwrap()
                                .install(std::path::Path::new(&source), approved)
                        })
                        .await
                        .unwrap_or_else(|e| {
                            Err(aos_platform::module_rt::ModuleError::Io(e.to_string()))
                        });
                        let r = match r {
                            Err(aos_platform::module_rt::ModuleError::CapReviewRequired(caps_csv)) => {
                                let required: Vec<String> = caps_csv
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect();
                                let reason = format!(
                                    "Revue des caps requise pour installer ce module.\nCaps demandées:\n- {}",
                                    required.join("\n- ")
                                );
                                let (_id, rx) = s
                                    .confirm
                                    .ask(
                                        actor.clone(),
                                        "module.install".into(),
                                        req.source_dir.clone(),
                                        reason,
                                        Some(120),
                                    )
                                    .await;
                                let approved = rx.await.unwrap_or(false);
                                let caps = if approved {
                                    Some(required)
                                } else {
                                    // Refus → install quarantined (aucune cap).
                                    Some(Vec::new())
                                };
                                let s3 = s.clone();
                                let source = req.source_dir.clone();
                                tokio::task::spawn_blocking(move || {
                                    s3.modules
                                        .lock()
                                        .unwrap()
                                        .install(std::path::Path::new(&source), caps)
                                })
                                .await
                                .unwrap_or_else(|e| {
                                    Err(aos_platform::module_rt::ModuleError::Io(e.to_string()))
                                })
                            }
                            other => other,
                        };
                        match r {
                            Ok(info) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor,
                                    action: "module.install".into(),
                                    target: info.name.clone(),
                                    detail: serde_json::json!({
                                        "caps": info.granted_caps,
                                        "quarantined": info.quarantined,
                                    }),
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

    // --- module.scaffold / package / compile (F-EXT) ---
    {
        let s = sub.clone();
        svc.on("module.scaffold", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<ModuleScaffoldRequest>() {
                    Ok(req) => {
                        let r = s.author.lock().unwrap().scaffold(&req);
                        match r {
                            Ok(resp) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor: if req.actor.is_empty() {
                                        "human:ui".into()
                                    } else {
                                        req.actor
                                    },
                                    action: "module.scaffold".into(),
                                    target: resp.path.clone(),
                                    detail: serde_json::json!({"kind": resp.kind}),
                                });
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::BadRequest,
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
        svc.on("module.package", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<ModulePackageRequest>() {
                    Ok(req) => {
                        let r = s.author.lock().unwrap().package_script(&req.name);
                        match r {
                            Ok(resp) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor: if req.actor.is_empty() {
                                        "human:ui".into()
                                    } else {
                                        req.actor
                                    },
                                    action: "module.package".into(),
                                    target: req.name,
                                    detail: serde_json::json!({"hash": resp.hash}),
                                });
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
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
        svc.on("module.compile", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<ModuleCompileRequest>() {
                    Ok(req) => {
                        let actor = if req.actor.is_empty() {
                            "human:ui".into()
                        } else {
                            req.actor.clone()
                        };
                        let agent_id = actor.strip_prefix("agent:").unwrap_or("human:ui");
                        // Cap critique : High → confirm, Medium → confirm, Low → deny
                        if !actor.starts_with("human:") {
                            let decision = s.decide_cap_request(agent_id, "module.compile");
                            match decision {
                                aos_platform::subsystem::CapDecision::Deny => {
                                    let _ = ctx
                                        .respond_error(
                                            aos_ipc::msg::Status::PermissionDenied,
                                            "module.compile refusé (trust insuffisant)",
                                        )
                                        .await;
                                    return;
                                }
                                aos_platform::subsystem::CapDecision::Confirm => {
                                    let ok = s
                                        .policy_gate(
                                            std::collections::HashMap::from([(
                                                "action.kind".to_string(),
                                                "module.compile".to_string(),
                                            )]),
                                            agent_id,
                                            "module.compile",
                                            &req.name,
                                            &format!("compile-{}", req.name),
                                        )
                                        .await;
                                    if !ok {
                                        let _ = ctx
                                            .respond_error(
                                                aos_ipc::msg::Status::PermissionDenied,
                                                "module.compile : confirmation refusée",
                                            )
                                            .await;
                                        return;
                                    }
                                    s.grant_cap(agent_id, "module.compile");
                                }
                                aos_platform::subsystem::CapDecision::Grant => {
                                    s.grant_cap(agent_id, "module.compile");
                                }
                            }
                            if !req.actor_caps.iter().any(|c| c == "module.compile")
                                && !s
                                    .granted_caps
                                    .lock()
                                    .unwrap()
                                    .get(agent_id)
                                    .map(|c| c.iter().any(|x| x == "module.compile"))
                                    .unwrap_or(false)
                            {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::PermissionDenied,
                                        "module.compile : capacité manquante",
                                    )
                                    .await;
                                return;
                            }
                        }
                        let name = req.name.clone();
                        let s2 = s.clone();
                        let r = tokio::task::spawn_blocking(move || {
                            s2.author.lock().unwrap().compile_rust(&name)
                        })
                        .await
                        .unwrap_or_else(|e| {
                            Err(aos_platform::module_compile::CompileError::Other(e.to_string()))
                        });
                        match r {
                            Ok(resp) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor,
                                    action: "module.compile".into(),
                                    target: req.name,
                                    detail: serde_json::json!({"hash": resp.hash}),
                                });
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
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

    // --- skill.* (F-EXT) ---
    {
        let s = sub.clone();
        svc.on("skill.create", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<SkillCreateRequest>() {
                    Ok(req) => {
                        let actor = if req.actor.is_empty() {
                            "human:ui".into()
                        } else {
                            req.actor.clone()
                        };
                        let agent_id = actor.strip_prefix("agent:").unwrap_or("human:ui");
                        if !actor.starts_with("human:") {
                            let decision = s.decide_cap_request(agent_id, "skill.create");
                            match decision {
                                aos_platform::subsystem::CapDecision::Deny => {
                                    let _ = ctx
                                        .respond_error(
                                            aos_ipc::msg::Status::PermissionDenied,
                                            "skill.create refusé (trust low)",
                                        )
                                        .await;
                                    return;
                                }
                                aos_platform::subsystem::CapDecision::Confirm => {
                                    let ok = s
                                        .policy_gate(
                                            std::collections::HashMap::from([(
                                                "action.kind".to_string(),
                                                "skill.create".to_string(),
                                            )]),
                                            agent_id,
                                            "skill.create",
                                            &req.name,
                                            &format!("skill-create-{}", req.name),
                                        )
                                        .await;
                                    if !ok {
                                        let _ = ctx
                                            .respond_error(
                                                aos_ipc::msg::Status::PermissionDenied,
                                                "skill.create : confirmation refusée",
                                            )
                                            .await;
                                        return;
                                    }
                                }
                                aos_platform::subsystem::CapDecision::Grant => {}
                            }
                        }
                        let r = s.skills.lock().unwrap().create(&req);
                        match r {
                            Ok(info) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor,
                                    action: "skill.create".into(),
                                    target: info.name.clone(),
                                    detail: serde_json::json!({
                                        "tools": info.tools,
                                        "required_caps": info.required_caps,
                                    }),
                                });
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &info).await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::BadRequest,
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
        svc.on("skill.activate", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<SkillNameRequest>() {
                    Ok(req) => {
                        let info = s
                            .skills
                            .lock()
                            .unwrap()
                            .describe(&req.name)
                            .ok()
                            .or_else(|| aos_agent::skills::get_skill(&req.name));
                        match info {
                            Some(info) => {
                                // Activation = retourner le corps + caps à demander
                                s.audit(AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor: if req.actor.is_empty() {
                                        "human:ui".into()
                                    } else {
                                        req.actor
                                    },
                                    action: "skill.activate".into(),
                                    target: info.name.clone(),
                                    detail: serde_json::json!({"required_caps": info.required_caps}),
                                });
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &info).await;
                            }
                            None => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::NotFound,
                                        "skill inconnue",
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
        svc.on("skill.uninstall", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<SkillNameRequest>() {
                    Ok(req) => {
                        let result = s.skills.lock().unwrap().uninstall(&req.name);
                        match result {
                            Ok(()) => {
                                s.audit(AuditAppendRequest {
                                    trace_id: String::new(),
                                    actor: if req.actor.is_empty() {
                                        "human:ui".into()
                                    } else {
                                        req.actor
                                    },
                                    action: "skill.uninstall".into(),
                                    target: req.name,
                                    detail: serde_json::json!({}),
                                });
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &true).await;
                            }
                            Err(e) => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::NotFound,
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
                                if let Some(bus) = s.bus() {
                                    let _ = bus
                                        .call::<AgentGrantRequest, bool>(
                                            "agent.grant",
                                            &AgentGrantRequest {
                                                agent_id: req.agent_id.clone(),
                                                cap: req.cap.clone(),
                                            },
                                            vec![],
                                        )
                                        .await;
                                }
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
                                    if let Some(bus) = s.bus() {
                                        let _ = bus
                                            .call::<AgentGrantRequest, bool>(
                                                "agent.grant",
                                                &AgentGrantRequest {
                                                    agent_id: req.agent_id.clone(),
                                                    cap: req.cap.clone(),
                                                },
                                                vec![],
                                            )
                                            .await;
                                    }
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
                        {
                            let mut net = s.net.lock().unwrap();
                            net.set_mode(mode);
                            // Preview : en online, autoriser fetch/search génériques
                            // (toujours journalisé + confirm policy pour agents).
                            if matches!(mode, aos_platform::net::NetMode::Online) {
                                net.grant("net.connect:*:*".into());
                            }
                        }
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

    // --- secrets.get / set / list (E7 / Preview 0.4) ---
    {
        let s = sub.clone();
        svc.on("secrets.get", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<SecretGetRequest>() {
                    Ok(req) => {
                        let actor = ctx.intent.from.clone();
                        let r = {
                            let store = s.secrets.lock().unwrap();
                            store.get(&req.name, &actor)
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
    {
        let s = sub.clone();
        svc.on("secrets.set", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<SecretSetRequest>() {
                    Ok(req) => {
                        let actor = ctx.intent.from.clone();
                        let r = {
                            let mut store = s.secrets.lock().unwrap();
                            store.set(&req.name, &req.value, &actor)
                        };
                        match r {
                            Ok(()) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &true).await;
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
        svc.on("secrets.list", move |ctx| {
            let s = s.clone();
            async move {
                let actor = ctx.intent.from.clone();
                let r = {
                    let store = s.secrets.lock().unwrap();
                    store.list_names(&actor).map(|names| SecretListResponse {
                        names,
                        encrypted: store.is_encrypted(),
                    })
                };
                match r {
                    Ok(resp) => {
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
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

    // --- feedback.submit (local + issue GitHub optionnelle) ---
    {
        let s = sub.clone();
        svc.on("feedback.submit", move |ctx| {
            let s = s.clone();
            async move {
                match ctx.payload::<FeedbackSubmitRequest>() {
                    Ok(req) => {
                        let publish = req.publish_github;
                        let req_gh = req.clone();
                        match aos_platform::feedback::submit(
                            aos_platform::feedback::default_dir(),
                            req,
                        ) {
                            Ok(mut resp) => {
                                if aos_platform::feedback::is_security_category(&req_gh.category)
                                {
                                    resp.github_status = "skipped_security".into();
                                } else if publish {
                                    let token = s
                                        .secrets
                                        .lock()
                                        .unwrap()
                                        .get("github_token", "service:platformd")
                                        .ok()
                                        .or_else(|| std::env::var("AOS_GITHUB_TOKEN").ok())
                                        .or_else(|| std::env::var("GITHUB_TOKEN").ok());
                                    let gh = {
                                        let mut net = s.net.lock().unwrap();
                                        aos_platform::feedback::publish_to_github(
                                            &mut net,
                                            token.as_deref(),
                                            &req_gh,
                                            &resp.id,
                                        )
                                    };
                                    match gh {
                                        Ok(p) => {
                                            resp.github_issue_url = Some(p.issue_url.clone());
                                            resp.github_issue_number = p.issue_number;
                                            resp.github_status = p.via.into();
                                        }
                                        Err(e) => {
                                            resp.github_issue_url = Some(
                                                aos_platform::feedback::new_issue_form_url(
                                                    &req_gh, &resp.id,
                                                ),
                                            );
                                            resp.github_status = format!("form ({e})");
                                        }
                                    }
                                }
                                s.audit(AuditAppendRequest {
                                    trace_id: format!("feedback-{}", resp.id),
                                    actor: "human:ui".into(),
                                    action: "feedback.submit".into(),
                                    target: resp.id.clone(),
                                    detail: serde_json::json!({
                                        "path": resp.path,
                                        "github_status": resp.github_status,
                                        "github_issue": resp.github_issue_number,
                                    }),
                                });
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

    eprintln!("[aos-platformd] prêt");
    let _ = svc.serve(&config.bus).await;
}
