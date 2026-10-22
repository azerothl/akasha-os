//! Real local-model smoke in a new isolated home, using synthetic observations.
//! Usage: semantic_memory_probe <absolute aos-modeld> <absolute Qwen GGUF>
use aos_ipc::{BusClient, BusService};
use aos_platform::{
    memory::MemoryStore,
    semantic_consolidation,
    subsystem::{PlatformConfig, PlatformSubsystem},
};
use aos_proto::semantic_memory::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::Path;

fn hash(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os()
        .skip(1)
        .map(std::path::PathBuf::from)
        .collect();
    if args.len() != 2 || args.iter().any(|p| !p.is_absolute() || !p.is_file()) {
        return Err("expected absolute modeld and Qwen GGUF paths".into());
    }
    let model_sha = hash(&args[1])?;
    if model_sha != "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4" {
        return Err("probe pins Qwen3.5-4B Q4_K_M".into());
    }
    let home =
        std::env::temp_dir().join(format!("aos-semantic-real-{:032x}", rand::random::<u128>()));
    std::fs::create_dir_all(home.join("data/models"))?;
    let path = |name: &str| home.join(name).to_string_lossy().into_owned();
    let config: PlatformConfig = serde_json::from_value(json!({
        "audit_dir":path("audit"),"storage_dir":path("storage"),"memory_dir":path("memory"),
        "modules_dir":path("modules"),"skills_dir":path("skills"),"sessions_dir":path("sessions"),
        "catalogue_file":path("absent.yaml"),"community_catalogue_dir":path("community"),
        "secrets_file":path("secrets/keys.yaml"),"memory_v2":true,"net_mode":"offline_strict"
    }))?;
    let sub = PlatformSubsystem::open(&config)?;
    let subjects=[("user:default",vec![
        "The user prefers Rust because they value native execution speed.",
        "The user chose Zig to avoid runtime overhead.",
        "The user dislikes Electron because of memory consumption.",
        "The user prefers self-hosted models that run offline.",
    ]),("agent:builder",vec![
        "In task A, the builder agent fixed the Rust compilation error successfully after reading the compiler diagnostic.",
        "In task B, the builder agent succeeded by reading compiler diagnostics before changing the code.",
        "In task C, the builder agent failed when it ignored the compiler diagnostics and guessed a fix.",
        "In task D, the builder agent repeated the failure after guessing a fix without reading compiler diagnostics.",
    ])];
    for (namespace, texts) in &subjects {
        for (i, text) in texts.iter().enumerate() {
            let req = serde_json::from_value(
                json!({"namespace":namespace,"kind":"event","content":text,"status":"accepted",
            "source_refs":[{"source_type":"fixture_observation","source_id":format!("{namespace}-{i}")}]}),
            )?;
            sub.mem.lock().unwrap().object_create(req, vec![])?;
        }
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?.to_string();
    let broker = tokio::spawn(aos_ipc::broker::serve(listener));
    let mut svc = BusService::new("semantic-real-host");
    semantic_consolidation::register(&mut svc, sub.clone());
    let service_addr = addr.clone();
    let service = tokio::spawn(async move { svc.serve(&service_addr).await });
    let bus = BusClient::connect(&addr, "ui-egui").await?;
    sub.set_bus(bus.clone());
    let id = "local:semantic-proof";
    let bytes = args[1].metadata()?.len();
    let catalog = json!({"models":[{"id":id,"name":"Semantic smoke model","modality":"text","format":"gguf",
        "source":{"type":"local_file","path":args[1],"sha256":model_sha},
        "architecture":{"n_layers":32,"n_params":4.0e9,"context_length":16384},
        "resource_hints":{"weights_bytes":bytes,"embed_bytes":200000000,"kv_bytes_per_token":65536,"supports_layer_offload":true},
        "capabilities":["chat"],"backends_compatible":["llamacpp"],"privacy_class":"local"}]});
    std::fs::write(
        home.join("data/models/catalog.yaml"),
        serde_json::to_vec_pretty(&catalog)?,
    )?;
    let model_config = json!({"bus":addr,"gpu":true,"vram_total_bytes":8589934592u64,
        "os_reserve_vram_bytes":1073741824u64,"os_reserve_ram_bytes":1073741824u64,
        "default_model":id,"default_kv_tokens":16384,"n_threads":8,"n_seq_max":1,"routing":"local_only","adaptive_planner":false,
        "inference_optimization":{"prefix_cache":"off","speculation":"off"},
        "models":{id:{"path":args[1],"n_layers":32,"weights_bytes":bytes}}});
    std::fs::write(
        home.join("modeld.yaml"),
        serde_json::to_vec_pretty(&model_config)?,
    )?;
    let log = std::fs::File::create(home.join("modeld.log"))?;
    let mut command = tokio::process::Command::new(&args[0]);
    command
        .arg(home.join("modeld.yaml"))
        .current_dir(&home)
        .env("AOS_HOME", &home)
        .stdout(log.try_clone()?)
        .stderr(log)
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut modeld = command.spawn()?;
    let ready = tokio::time::timeout(std::time::Duration::from_secs(90), async {
        loop {
            if modeld.try_wait().map_err(|e| e.to_string())?.is_some() {
                return Err("owned modeld exited".to_string());
            }
            if bus.lookup("model.infer").await.map_err(|e| e.to_string())?
                && bus
                    .lookup("mem.semantic.consolidate")
                    .await
                    .map_err(|e| e.to_string())?
            {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await;
    let mut results = Vec::new();
    if matches!(ready, Ok(Ok(()))) {
        for (namespace, _) in &subjects {
            let request = MemSemanticRequest {
                namespace: (*namespace).into(),
                model_id: Some(id.into()),
            };
            let result = bus
                .call::<_, MemSemanticResponse>("mem.semantic.consolidate", &request, vec![])
                .await;
            match result {
                Ok(response) => {
                    let no_unconfirmed_context = sub
                        .mem
                        .lock()
                        .unwrap()
                        .semantic_prompt(namespace, "compiler and native tools")
                        .is_empty();
                    let mut confirmed = false;
                    if let Some(c) = response.concepts.iter().find(|c| !c.disputed) {
                        bus.call::<_, MemSemanticResponse>(
                            "mem.semantic.review",
                            &MemSemanticReviewRequest {
                                namespace: (*namespace).into(),
                                id: c.id,
                                revision: c.revision,
                                accepted: true,
                            },
                            vec![],
                        )
                        .await?;
                        confirmed = !sub
                            .mem
                            .lock()
                            .unwrap()
                            .semantic_prompt(namespace, "compiler and native tools")
                            .is_empty();
                    }
                    let mut disputed_confirmation_blocked = false;
                    if !response.concepts.is_empty() && response.concepts.iter().all(|c| c.disputed)
                    {
                        let c = &response.concepts[0];
                        disputed_confirmation_blocked = bus
                            .call::<_, MemSemanticResponse>(
                                "mem.semantic.review",
                                &MemSemanticReviewRequest {
                                    namespace: (*namespace).into(),
                                    id: c.id,
                                    revision: c.revision,
                                    accepted: true,
                                },
                                vec![],
                            )
                            .await
                            .is_err()
                            && sub
                                .mem
                                .lock()
                                .unwrap()
                                .semantic_prompt(namespace, "compiler")
                                .is_empty();
                    }
                    let reopened = MemoryStore::open_with_v2(home.join("memory"), true)?;
                    let persisted = reopened.semantic_list(namespace)?.concepts.len()
                        == response.concepts.len();
                    results.push(json!({"namespace":namespace,"response":response,"no_unconfirmed_context":no_unconfirmed_context,"confirmed_context":confirmed,"disputed_confirmation_blocked":disputed_confirmation_blocked,"survived_restart":persisted}));
                }
                Err(error) => {
                    results.push(json!({"namespace":namespace,"error":error.to_string()}))
                }
            }
        }
    }
    let stopped = if modeld.try_wait()?.is_some() {
        true
    } else {
        modeld.kill().await.is_ok() && modeld.wait().await.is_ok()
    };
    broker.abort();
    service.abort();
    let ok = results.len() == subjects.len()
        && results.iter().all(|r| {
            r["no_unconfirmed_context"] == true
                && (r["confirmed_context"] == true || r["disputed_confirmation_blocked"] == true)
                && r["survived_restart"] == true
        })
        && stopped;
    let report = json!({"ok":ok,"fixture":home,"model_sha256":model_sha,"modeld_sha256":hash(&args[0])?,"owned_modeld_stopped":stopped,"results":results,
        "scope":"real Qwen through modeld and IPC over eight synthetic observations; automated confirmation; no installed UI or representative semantic-quality benchmark"});
    std::fs::write(
        home.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !ok {
        return Err("semantic local-model smoke did not pass; fixture report retained".into());
    }
    Ok(())
}
