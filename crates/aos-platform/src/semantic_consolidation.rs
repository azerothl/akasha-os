//! Local-only model orchestration and the shared bus/WASM permission boundary.
use crate::{memory::semantic, subsystem::PlatformSubsystem};
use aos_proto::semantic_memory::*;
use aos_proto::{AuditAppendRequest, ChatMessage, InferParams, InferRequest, TokenEvent};
use serde_json::{json, Value};

static JOB: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub async fn run(
    sub: &PlatformSubsystem,
    req: MemSemanticRequest,
) -> Result<MemSemanticResponse, String> {
    run_with_mode(sub, req, false).await
}

pub async fn run_background(
    sub: &PlatformSubsystem,
    req: MemSemanticRequest,
) -> Result<MemSemanticResponse, String> {
    run_with_mode(sub, req, true).await
}

async fn run_with_mode(
    sub: &PlatformSubsystem,
    req: MemSemanticRequest,
    background: bool,
) -> Result<MemSemanticResponse, String> {
    let _job = JOB
        .try_lock()
        .map_err(|_| "semantic consolidation already running")?;
    let batch = {
        let mem = sub.mem.lock().unwrap();
        let status = mem.semantic_list(&req.namespace)?;
        if status.pending_sources == 0 || (background && !status.enabled) {
            return Ok(status);
        }
        mem.semantic_prepare(&req.namespace)?
    };
    if batch.evidence.is_empty()
        || sub
            .mem
            .lock()
            .unwrap()
            .semantic_list(&req.namespace)?
            .pending_sources
            == 0
    {
        return sub.mem.lock().unwrap().semantic_list(&req.namespace);
    }
    let bus = sub
        .bus()
        .ok_or("semantic consolidation requires a local model bus")?;
    let request = InferRequest {
        model_id: req.model_id,
        messages: vec![
            ChatMessage {
                role: "system".into(),
                content: semantic::SYSTEM_PROMPT.into(),
            },
            ChatMessage {
                role: "user".into(),
                content: semantic::batch_prompt(&batch),
            },
        ],
        tools: vec![],
        params: InferParams {
            max_tokens: 2048,
            temperature: 0.1,
            top_p: 0.9,
            seed: Some(42),
        },
        priority: 1,
        data_refs: vec![],
        images: vec![],
        routing: Some("local_only".into()),
    };
    let mut inference_id = 0;
    let result = tokio::time::timeout(std::time::Duration::from_secs(180), async {
        let mut stream = bus
            .call_stream::<InferRequest, TokenEvent>("model.infer", &request, vec![])
            .await
            .map_err(|e| e.to_string())?;
        let mut output = String::new();
        while let Some(event) = stream.recv().await {
            match event.map_err(|e| e.to_string())? {
                TokenEvent::Started { inference_id: id } => inference_id = id,
                TokenEvent::Delta { text } => {
                    if output.len() + text.len() > 32_768 {
                        return Err("semantic output exceeds size limit".into());
                    }
                    output.push_str(&text);
                }
                TokenEvent::Error { message } => return Err(message),
                TokenEvent::Done { .. } => return Ok(output),
                _ => {}
            }
        }
        Err("semantic model stream ended without completion".to_string())
    })
    .await
    .map_err(|_| "semantic inference timeout".to_string())
    .and_then(|r| r);
    if result.is_err() && inference_id != 0 {
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            bus.call::<_, Value>(
                "model.cancel",
                &aos_proto::CancelRequest { inference_id },
                vec![],
            ),
        )
        .await;
    }
    let output = result?;
    let proposals = semantic::parse_proposals(&output)?;
    sub.mem.lock().unwrap().semantic_apply(&batch, proposals)
}

pub fn authorize(
    actor: &str,
    caps: &[String],
    namespace: &str,
    write: bool,
    human_only: bool,
) -> Result<(), String> {
    semantic::validate_namespace(namespace)?;
    if matches!(actor, "human:ui" | "ui-egui") {
        return Ok(());
    }
    if human_only {
        return Err("semantic settings and confirmation require the human UI".into());
    }
    for kind in if write {
        vec!["mem.read", "mem.write"]
    } else {
        vec!["mem.read"]
    } {
        if !caps.iter().any(|cap| {
            cap.strip_prefix(&format!("{kind}:"))
                .or_else(|| {
                    if kind == "mem.read" {
                        cap.strip_prefix("mem.query:")
                    } else {
                        None
                    }
                })
                .is_some_and(|p| crate::storage::glob_match(p, namespace))
        }) {
            return Err(format!("permission refused: {kind}:{namespace}"));
        }
    }
    Ok(())
}

pub async fn check_access(
    sub: &PlatformSubsystem,
    actor: &str,
    caps: &[String],
    namespace: &str,
    write: bool,
    human_only: bool,
) -> Result<(), String> {
    if !matches!(actor, "human:ui" | "ui-egui")
        && caps.iter().any(|c| c.starts_with("cap://kernel/"))
    {
        semantic::validate_namespace(namespace)?;
        if human_only {
            return Err("semantic settings and confirmation require the human UI".into());
        }
        let rights = if write {
            vec!["read".into(), "write".into()]
        } else {
            vec!["read".into()]
        };
        sub.authorize_kernel(caps, actor, &format!("mem:{namespace}"), &rights)
            .await
            .ok_or("kernel capability resolution unavailable")??;
    } else {
        authorize(actor, caps, namespace, write, human_only)?;
    }
    Ok(())
}

pub async fn context(
    sub: &PlatformSubsystem,
    namespace: &str,
    query: &str,
    actor: &str,
    caps: &[String],
) -> Result<String, String> {
    check_access(sub, actor, caps, namespace, false, false).await?;
    Ok(sub.mem.lock().unwrap().semantic_prompt(namespace, query))
}

pub async fn dispatch(
    sub: &PlatformSubsystem,
    intent: &str,
    args: Value,
    actor: &str,
    caps: &[String],
    trace: &str,
) -> Result<Value, String> {
    let namespace = args
        .get("namespace")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let result = async {
        let write = intent != "mem.semantic.list";
        check_access(
            sub,
            actor,
            caps,
            &namespace,
            write,
            matches!(intent, "mem.semantic.settings" | "mem.semantic.review"),
        )
        .await?;
        let response = match intent {
            "mem.semantic.list" => sub.mem.lock().unwrap().semantic_list(&namespace)?,
            "mem.semantic.settings" => {
                let req: MemSemanticSettingsRequest =
                    serde_json::from_value(args).map_err(|e| e.to_string())?;
                sub.mem
                    .lock()
                    .unwrap()
                    .semantic_enable(&req.namespace, req.enabled)?
            }
            "mem.semantic.review" => {
                let req: MemSemanticReviewRequest =
                    serde_json::from_value(args).map_err(|e| e.to_string())?;
                sub.mem.lock().unwrap().semantic_review(&req)?
            }
            "mem.semantic.consolidate" => {
                run(
                    sub,
                    serde_json::from_value(args).map_err(|e| e.to_string())?,
                )
                .await?
            }
            _ => return Err("unknown semantic intent".into()),
        };
        serde_json::to_value(response).map_err(|e| e.to_string())
    }
    .await;
    sub.audit(AuditAppendRequest {
        trace_id: trace.into(), actor: actor.into(), action: intent.into(), target: namespace,
        detail: json!({"ok":result.is_ok(),"concepts":result.as_ref().ok().and_then(|r|r["concepts"].as_array().map(Vec::len))}),
    });
    result
}

pub fn register(svc: &mut aos_ipc::BusService, sub: std::sync::Arc<PlatformSubsystem>) {
    for intent in [
        "mem.semantic.list",
        "mem.semantic.consolidate",
        "mem.semantic.settings",
        "mem.semantic.review",
    ] {
        let sub = sub.clone();
        svc.on(intent, move |ctx| {
            let sub = sub.clone();
            async move {
                let result = match ctx.payload::<Value>() {
                    Ok(args) => {
                        dispatch(
                            &sub,
                            intent,
                            args,
                            &ctx.intent.from,
                            &ctx.intent.caps,
                            &format!("semantic-{}", ctx.intent.correlation_id),
                        )
                        .await
                    }
                    Err(e) => Err(e.to_string()),
                };
                match result {
                    Ok(value) => {
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &value).await;
                    }
                    Err(e) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, &e)
                            .await;
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subsystem::PlatformConfig;
    use aos_ipc::{broker, msg::Status, BusClient, BusService};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[test]
    fn semantic_permissions_are_scoped_and_review_is_human_only() {
        let caps = vec!["mem.read:user:alice".into(), "mem.write:user:alice".into()];
        assert!(authorize("agent:a", &caps, "user:alice", true, false).is_ok());
        assert!(authorize("agent:a", &caps, "user:bob", false, false).is_err());
        assert!(authorize("agent:a", &caps[..1], "user:alice", true, false).is_err());
        assert!(authorize("agent:a", &caps, "user:alice", true, true).is_err());
        assert!(authorize("ui-egui", &[], "user:alice", true, true).is_ok());
        assert!(authorize("ui-egui", &[], "product:docs", true, true).is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn semantic_bus_consolidates_locally_and_retries_failure_without_consuming_sources() {
        let dir = std::env::temp_dir().join(format!("aos-semantic-bus-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = |name: &str| dir.join(name).to_string_lossy().into_owned();
        let config: PlatformConfig = serde_json::from_value(json!({
            "bus":"127.0.0.1:0", "audit_dir":path("audit"),"storage_dir":path("storage"),
            "memory_dir":path("memory"),"modules_dir":path("modules"),"skills_dir":path("skills"),
            "sessions_dir":path("sessions"),"catalogue_file":path("catalogue.yaml"),
            "community_catalogue_dir":path("community"),"secrets_file":path("secrets/keys.yaml"),
            "memory_v2":true
        }))
        .unwrap();
        let sub = PlatformSubsystem::open(&config).unwrap();
        let fact = |text: &str, session: &str| {
            let req = serde_json::from_value(json!({"namespace":"user:default","kind":"claim","content":text,"status":"accepted",
                "source_refs":[{"source_type":"chat_session","source_id":session}],"metadata":{"session_id":session}})).unwrap();
            sub.mem
                .lock()
                .unwrap()
                .object_create(req, vec![])
                .unwrap()
                .id
        };
        assert_eq!(fact("User runs models offline.", "s1"), 1);
        assert_eq!(fact("User self-hosts inference tools.", "s2"), 2);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let broker_task = tokio::spawn(async move { broker::serve(listener).await.unwrap() });
        sub.set_bus(BusClient::connect(&addr, "service:semantic").await.unwrap());
        let mode = Arc::new(AtomicUsize::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let cancellations = Arc::new(AtomicUsize::new(0));
        let mut svc = BusService::new("semantic-test");
        let cancelled = cancellations.clone();
        svc.on("model.cancel", move |ctx| {
            let cancelled = cancelled.clone();
            async move {
                let req: aos_proto::CancelRequest = ctx.payload().unwrap();
                assert_eq!(req.inference_id, 42);
                cancelled.fetch_add(1, Ordering::SeqCst);
                ctx.respond(Status::Ok, &true).await.unwrap();
            }
        });
        let (mode2, calls2) = (mode.clone(), calls.clone());
        svc.on("model.infer", move |ctx| {
            let (mode,calls)=(mode2.clone(),calls2.clone());
            async move {
                calls.fetch_add(1,Ordering::SeqCst);
                let req: InferRequest=ctx.payload().unwrap();
                assert_eq!(req.routing.as_deref(),Some("local_only"));
                assert_eq!(req.priority,1); assert!(req.tools.is_empty());
                assert!(req.messages[0].content.contains("untrusted data"));
                let prompt: Value=serde_json::from_str(&req.messages[1].content).unwrap();
                assert_eq!(prompt["namespace"],"user:default");
                let stream=ctx.open_stream();
                stream.send(&TokenEvent::Started { inference_id:42 }).await.unwrap();
                if mode.load(Ordering::SeqCst)==1 {
                    stream.send(&TokenEvent::Error{message:"fixture model unavailable".into()}).await.unwrap();
                } else {
                    let contrad = if mode.load(Ordering::SeqCst)==2 {vec![3]} else {vec![]};
                    let text=json!({"concepts":[{"key":"local_first","kind":"preference","summary":"The user tends to prefer local-first tools.","supporting_ids":[1,2],"contradicting_ids":contrad}]}).to_string();
                    stream.send(&TokenEvent::Delta{text}).await.unwrap();
                    stream.send(&TokenEvent::Done{prompt_tokens:1,generated_tokens:1,ttft_ms:0.0,tok_s:0.0}).await.unwrap();
                }
                stream.finish(Status::Ok).await.unwrap();
            }
        });
        register(&mut svc, sub.clone());
        let service_addr = addr.clone();
        let service_task = tokio::spawn(async move { svc.serve(&service_addr).await.unwrap() });
        let ui = BusClient::connect(&addr, "ui-egui").await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while !ui.lookup("mem.semantic.list").await.unwrap() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let request = json!({"namespace":"user:default"});
        let agent = BusClient::connect(&addr, "agent:test").await.unwrap();
        assert!(agent
            .call::<_, MemSemanticResponse>("mem.semantic.list", &request, vec![])
            .await
            .is_err());
        assert!(agent
            .call::<_, MemSemanticResponse>(
                "mem.semantic.list",
                &request,
                vec!["mem.read:user:default".into()]
            )
            .await
            .is_ok());
        let result = ui
            .call::<_, MemSemanticResponse>("mem.semantic.consolidate", &request, vec![])
            .await
            .unwrap();
        assert_eq!(result.concepts.len(), 1);
        assert_eq!(result.pending_sources, 0);
        ui.call::<_, MemSemanticResponse>(
            "mem.semantic.review",
            &json!({"namespace":"user:default","id":1,"revision":1,"accepted":true}),
            vec![],
        )
        .await
        .unwrap();
        assert!(sub
            .mem
            .lock()
            .unwrap()
            .semantic_prompt("user:default", "tools")
            .contains("local-first"));
        assert!(context(&sub, "user:default", "tools", "agent:test", &[])
            .await
            .is_err());
        assert!(context(
            &sub,
            "user:default",
            "tools",
            "agent:test",
            &["mem.query:user:default".into()]
        )
        .await
        .unwrap()
        .contains("local-first"));
        assert!(context(
            &sub,
            "user:default",
            "tools",
            "agent:test",
            &["mem.read:user:other".into()]
        )
        .await
        .is_err());
        assert!(context(
            &sub,
            "user:default",
            "tools",
            "agent:test",
            &[
                "mem.read:user:default".into(),
                "cap://kernel/missing".into()
            ]
        )
        .await
        .is_err());
        // Unchanged memory does not trigger another model invocation.
        ui.call::<_, MemSemanticResponse>("mem.semantic.consolidate", &request, vec![])
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        fact("User now chooses managed cloud tools.", "s3");
        mode.store(1, Ordering::SeqCst);
        assert!(ui
            .call::<_, MemSemanticResponse>("mem.semantic.consolidate", &request, vec![])
            .await
            .is_err());
        assert_eq!(
            sub.mem
                .lock()
                .unwrap()
                .semantic_list("user:default")
                .unwrap()
                .pending_sources,
            1
        );
        assert_eq!(cancellations.load(Ordering::SeqCst), 1);
        mode.store(2, Ordering::SeqCst);
        let revised = ui
            .call::<_, MemSemanticResponse>("mem.semantic.consolidate", &request, vec![])
            .await
            .unwrap();
        assert!(revised.concepts[0].disputed);
        assert!(sub
            .mem
            .lock()
            .unwrap()
            .semantic_prompt("user:default", "tools")
            .is_empty());
        assert_eq!(revised.concepts[0].revision, 2);
        service_task.abort();
        broker_task.abort();
        drop((agent, ui, sub));
        let _ = std::fs::remove_dir_all(dir);
    }
}
