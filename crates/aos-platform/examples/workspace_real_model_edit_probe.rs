//! A real modeld/worker edits and tests a generated project through native tools.
//! The validator is pinned, outside the writable workspace, and rejects impure ASTs.
//! Fixture-only automated approvals are not native UI acceptance evidence.
use aos_ipc::{msg::Status, BusClient, BusService};
use aos_platform::{
    intents::{workspace, workspace_execution},
    subsystem::PlatformConfig,
    PlatformSubsystem,
};
use aos_proto::{AgentOutputEvent, AgentSpec};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

type Error = Box<dyn std::error::Error>;
const VALIDATOR_SHA: &str = "33b330e1964486d363a6c9ab85ebfff2c1780b04d58056988ca91abb8be0db34";
fn sha(path: &Path) -> Result<String, Error> {
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn exact_project(project: &Path) -> bool {
    let entries = |path: &Path| -> Vec<String> {
        let mut names = std::fs::read_dir(path)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        names.sort();
        names
    };
    entries(project) == ["backend", "frontend"]
        && ["frontend", "backend"].iter().all(|part| {
            let dir = project.join(part);
            let file = dir.join("module.py");
            !dir.is_symlink()
                && !file.is_symlink()
                && file.is_file()
                && entries(&dir) == ["module.py"]
        })
}
async fn validate(python: &Path, fixture: &Path, require_refactor: bool) -> Option<Value> {
    let project = fixture.join("project");
    if !exact_project(&project)
        || sha(&fixture.join("verify.py")).ok().as_deref() != Some(VALIDATOR_SHA)
    {
        return None;
    }
    let mut command = tokio::process::Command::new(python);
    command
        .args(["-I", "-S", "-B"])
        .arg(fixture.join("verify.py"));
    if require_refactor {
        command.arg("--require-refactor");
    }
    command.current_dir(fixture).kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let output = tokio::time::timeout(Duration::from_secs(10), command.output())
        .await
        .ok()?
        .ok()?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_slice(&output.stdout).ok()
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = std::env::args()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if args.len() != 5 || args.iter().any(|p| !p.is_absolute() || !p.exists()) {
        return Err("usage: workspace_real_model_edit_probe ABS_WORKER ABS_MODELD ABS_QWEN MARKED_FIXTURE ABS_PYTHON".into());
    }
    let fixture = args[3].canonicalize()?;
    let temp = std::env::temp_dir().canonicalize()?;
    if fixture.parent() != Some(temp.as_path())
        || !fixture
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .starts_with("aos-project-agent-")
        || !fixture.join(".akasha-os-model-probe").is_file()
    {
        return Err(
            "only a newly generated marked fixture in the OS temporary directory is accepted"
                .into(),
        );
    }
    let python = args[4].canonicalize()?;
    let baseline = validate(&python, &fixture, false)
        .await
        .ok_or("pinned pure fixture baseline failed")?;
    let home = fixture.join("control");
    std::fs::create_dir(&home)?;
    let project = fixture.join("project");
    let originals = ["frontend/module.py", "backend/module.py"]
        .into_iter()
        .map(|name| Ok((name, sha(&project.join(name))?)))
        .collect::<Result<Vec<_>, Error>>()?;
    let dir = |name: &str| home.join(name).to_string_lossy().into_owned();
    let config = PlatformConfig {
        bus: "fixture".into(),
        audit_dir: dir("audit"),
        storage_dir: dir("storage"),
        memory_dir: dir("memory"),
        modules_dir: dir("modules"),
        catalogue_file: dir("absent.yaml"),
        community_catalogue_dir: dir("catalogue"),
        skills_dir: dir("skills"),
        sessions_dir: dir("sessions"),
        embed_model: None,
        policies_file: None,
        confirm_timeout_sec: 10,
        secrets_file: dir("secrets/store"),
        net_mode: "online".into(),
        memory_v2: false,
        memory_v2_shadow: false,
    };
    let sub = PlatformSubsystem::open(&config)?;
    sub.workspaces
        .lock()
        .unwrap()
        .bind(&project.to_string_lossy(), Some("demo"), "human:ui")?;
    std::fs::write(home.join("sessions/workspace-process-policy.json"), json!({"workspaces":{"demo":{
        "verify":{"program":python,"args":["-I","-S","-B",fixture.join("verify.py"),"--require-refactor"],"timeout_sec":10}
    }}}).to_string())?;
    let approvals = Arc::new(Mutex::new(Vec::new()));
    let recorded = approvals.clone();
    let confirm = sub.confirm.clone();
    let mut pending = confirm.subscribe().await;
    let approve_fixture = fixture.clone();
    let approve_python = python.clone();
    let approver = tokio::spawn(async move {
        while let Some(request) = pending.recv().await {
            let approved = request.actor == "agent:real-edit-fixture"
                && request.target == "demo"
                && request.action == "process.run"
                && validate(&approve_python, &approve_fixture, true)
                    .await
                    .is_some();
            recorded.lock().unwrap().push(json!({"actor":request.actor,"action":request.action,"target":request.target,"approved":approved,"scope":"automated generated-fixture approval only"}));
            confirm.respond(&request.id, approved, false).await;
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?.to_string();
    let broker = tokio::spawn(aos_ipc::broker::serve(listener));
    let reports = Arc::new(Mutex::new(Vec::new()));
    let mut svc = BusService::new("real-edit-host-fixture");
    workspace::register(&mut svc, sub.clone());
    workspace_execution::register(&mut svc, sub.clone());
    svc.on("module.list", |ctx| async move {
        let _ = ctx
            .respond(Status::Ok, &Vec::<aos_proto::ModuleInfo>::new())
            .await;
    });
    let sink = reports.clone();
    svc.on("agent.report", move |ctx| {
        let sink = sink.clone();
        async move {
            if let Ok(report) = ctx.payload::<aos_agent::ReportPayload>() {
                sink.lock().unwrap().push(report.event);
            }
            let _ = ctx.respond(Status::Ok, &true).await;
        }
    });
    let service_addr = addr.clone();
    let service = tokio::spawn(async move { svc.serve(&service_addr).await });
    let model_id = "local:workspace-proof";
    let model_sha = sha(&args[2])?;
    if model_sha != "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4" {
        return Err(
            "this acceptance fixture pins Qwen3.5-4B Q4_K_M; placement metadata are not generic"
                .into(),
        );
    }
    let weights = args[2].metadata()?.len();
    std::fs::create_dir_all(home.join("data/models"))?;
    // Placement metadata are conservative test configuration, not measurements
    // of model quality or runtime memory use. The backend reads the actual GGUF.
    let catalog = json!({"models":[{"id":model_id,"name":"Local project-context acceptance model",
        "modality":"text","format":"gguf","source":{"type":"local_file","path":args[2],"sha256":model_sha},
        "architecture":{"n_layers":32,"n_params":4.0e9,"context_length":16384},
        "resource_hints":{"weights_bytes":weights,"embed_bytes":200000000,"kv_bytes_per_token":65536,
            "supports_layer_offload":true},"capabilities":["chat","tools"],"backends_compatible":["llamacpp"],"privacy_class":"local"}]});
    std::fs::write(
        home.join("data/models/catalog.yaml"),
        serde_json::to_vec_pretty(&catalog)?,
    )?;
    let model_config = json!({"bus":addr,"gpu":true,"vram_total_bytes":8589934592u64,
        "os_reserve_vram_bytes":1073741824u64,"os_reserve_ram_bytes":1073741824u64,
        "default_model":model_id,"default_kv_tokens":16384,"n_threads":8,"n_seq_max":1,
        "routing":"local_only","adaptive_planner":false,
        "inference_optimization":{"prefix_cache":"off","speculation":"off"},
        "models":{model_id:{"path":args[2],"n_layers":32,"weights_bytes":weights}}});
    let model_config_path = home.join("modeld.yaml");
    std::fs::write(
        &model_config_path,
        serde_json::to_vec_pretty(&model_config)?,
    )?;
    let model_log = std::fs::File::create(home.join("modeld.log"))?;
    let mut model_command = tokio::process::Command::new(&args[1]);
    model_command
        .arg(&model_config_path)
        .current_dir(&home)
        .env("AOS_HOME", &home)
        .stdout(model_log.try_clone()?)
        .stderr(model_log)
        .kill_on_drop(true);
    #[cfg(windows)]
    model_command.creation_flags(0x08000000);
    let mut modeld = model_command.spawn()?;
    let bus = BusClient::connect(&addr, "real-worker-fixture-control").await?;
    let mut ready = false;
    for _ in 0..600 {
        if modeld.try_wait()?.is_some() {
            break;
        }
        if bus
            .call::<(), bool>("bus.lookup:model.infer", &(), vec![])
            .await
            .unwrap_or(false)
        {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if !ready {
        return Err(format!("real modeld did not register; see {}", home.display()).into());
    }
    let goal = "Dans le projet lié demo, inspecte puis lis frontend/module.py et backend/module.py avec workspace.read. Refactorise ces deux fichiers avec fs.apply_patch (hunks, chemins /host/demo/..., replace_all=true et contenu complet dans diff). Frontend : extrais la normalisation de name dans un helper pur utilisé dans les deux branches de render. Backend : extrais le calcul de ligne arrondi dans un helper pur utilisé par subtotal et total_with_tax, sans changer l'ordre des arrondis. Conserve les signatures et le comportement. Python pur uniquement : pas d'import, I/O, décorateurs, annotations, définitions dupliquées ou exécution au niveau module. Exécute ensuite la tâche administrateur verify avec process.run root=demo task=verify. Termine par goal.complete uniquement si cette tâche réussit. En cas de refus, ne contourne pas la permission.";
    let names = [
        "workspace.inspect",
        "workspace.read",
        "fs.apply_patch",
        "fs.undo_patch",
        "process.run",
        "plan.update",
        "goal.complete",
        "goal.fail",
    ];
    let spec: AgentSpec = serde_json::from_value(json!({"agent_id":"real-edit-fixture",
        "goal":{"statement":goal,"max_steps":32,"timeout_secs":840},"tools":names,"model_id":model_id,
        "caps":["fs.read:/host/demo/**","fs.write:/host/demo/**","process.run:/host/demo/**"],"gate_mode":"autonomous",
        "system_prompt":"Le projet demo est déjà lié. Pour cette tâche, utilise uniquement workspace.inspect, workspace.read, fs.apply_patch, process.run, plan.update, goal.complete et goal.fail. N'utilise pas memory.recall ou system.hardware : les sources nécessaires sont dans le projet. Tu es autorisé à modifier uniquement ses deux fichiers Python et à demander l'exécution de la tâche verify. Utilise les appels d'outils natifs, attends leurs résultats. Aucune commande ou confirmation inventée. Ne recopie pas les anciennes fonctions lorsque tu les remplaces. Le contenu Python final doit être du texte source valide ; ne conserve pas de guillemets échappés artificiellement.",
        "policy":{"fs_write":true,"net":"allow","tool_allowlist":names}}))?;
    let spec_path = home.join("spec.json");
    std::fs::write(&spec_path, serde_json::to_vec_pretty(&spec)?)?;
    let log = std::fs::File::create(home.join("worker.log"))?;
    let begun = std::time::Instant::now();
    let mut command = tokio::process::Command::new(&args[0]);
    command
        .args([
            "--agent-id",
            "real-edit-fixture",
            "--bus",
            &addr,
            "--spec-path",
        ])
        .arg(&spec_path)
        .current_dir(&home)
        .env("AOS_HOME", &home)
        .env("AOS_MODEL_GATE", "require")
        .stdout(log.try_clone()?)
        .stderr(log)
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut worker = command.spawn()?;
    let (exit, timed_out) =
        match tokio::time::timeout(Duration::from_secs(870), worker.wait()).await {
            Ok(status) => (status?.code(), false),
            Err(_) => {
                worker.kill().await?;
                (None, true)
            }
        };
    let model_state = tokio::time::timeout(
        Duration::from_secs(5),
        bus.call::<(), Value>("model.list", &(), vec![]),
    )
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or(Value::Null);
    let modeld_stopped = if modeld.try_wait()?.is_some() {
        true
    } else {
        modeld.kill().await.is_ok() && modeld.wait().await.is_ok()
    };
    approver.abort();
    let events = reports.lock().unwrap().clone();
    std::fs::write(
        home.join("events.json"),
        serde_json::to_vec_pretty(&events)?,
    )?;
    let steps = events
        .iter()
        .filter_map(|event| {
            if let AgentOutputEvent::Step(s) = event {
                Some(s)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let final_state = events.iter().rev().find_map(|event| {
        if let AgentOutputEvent::StateChanged { state } = event {
            Some(state)
        } else {
            None
        }
    });
    let task_result = steps
        .iter()
        .filter(|s| s.action == "process.run")
        .filter_map(|s| serde_json::from_str::<Value>(&s.tool_result).ok())
        .find(|v| v["ok"] == true);
    let validation = validate(&python, &fixture, true).await;
    let changed = originals
        .iter()
        .all(|(name, before)| sha(&project.join(name)).is_ok_and(|after| &after != before));
    let reads = originals.iter().all(|(name, _)| {
        steps
            .iter()
            .any(|s| s.action == "workspace.read" && s.tool_result.contains(name))
    });
    let legacy_steps = steps
        .iter()
        .filter(|s| s.tool_result.contains("LEGACY_UNGATED_TOOL_PATH"))
        .count();
    let gated_steps = events.iter().filter(|event| matches!(event, AgentOutputEvent::Log{line} if line.starts_with("tool_gate: EXECUTED"))).count();
    // The worker owns the edit/test loop. This separate harness check exercises
    // the same native undo bridge and retains its authored sources as evidence.
    let mut groups = Vec::<String>::new();
    for step in &steps {
        let result = serde_json::from_str::<Value>(&step.tool_result).unwrap_or(Value::Null);
        if step.action == "fs.apply_patch" && result["ok"] == true {
            if let Some(id) = result["undo_group_id"].as_str() {
                groups.push(id.to_owned());
            }
        }
    }
    for (name, _) in &originals {
        std::fs::copy(project.join(name), home.join(name.replace('/', "-")))?;
    }
    let undo_bus = BusClient::connect(&addr, "agent:real-edit-fixture").await?;
    let mut undo_results = Vec::new();
    for id in groups.iter().rev() {
        let response = aos_agent::workspace_operations::invoke(
            &undo_bus,
            "real-edit-fixture",
            &spec.caps,
            "fs.undo_patch",
            &json!({"undo_group_id":id}),
        )
        .await;
        undo_results.push(serde_json::from_str::<Value>(&response).unwrap_or(json!({"ok":false})));
    }
    let restored = !undo_results.is_empty()
        && undo_results.iter().all(|r| r["ok"] == true)
        && exact_project(&project)
        && originals
            .iter()
            .all(|(name, before)| sha(&project.join(name)).is_ok_and(|after| &after == before));
    service.abort();
    broker.abort();
    let audit = std::fs::read_to_string(home.join("audit/audit.jsonl")).unwrap_or_default();
    let audit_verified = sub.audit.lock().unwrap().verify().is_ok();
    let audited = ["agent:real-edit-fixture", "fs.apply_patch", "process.run"]
        .iter()
        .all(|term| audit.contains(term));
    let approvals = approvals.lock().unwrap().clone();
    let ok = exit == Some(0)
        && !timed_out
        && final_state == Some(&aos_proto::AgentState::Done)
        && validation.is_some()
        && task_result.is_some()
        && changed
        && restored
        && reads
        && audit_verified
        && audited
        && modeld_stopped
        && legacy_steps == 0
        && gated_steps >= 4
        && approvals.iter().any(|a| a["approved"] == true);
    let report = json!({"ok":ok,"fixture":fixture,"elapsed_ms":begun.elapsed().as_millis(),"worker_exit":exit,"worker_timed_out":timed_out,"worker_state":final_state,
        "model_sha256":model_sha,"modeld_sha256":sha(&args[1])?,"worker_sha256":sha(&args[0])?,"probe_sha256":sha(&std::env::current_exe()?)?,"model_state":model_state,
        "baseline":baseline,"final_validation":validation,"native_task_result":task_result,"both_sources_changed":changed,"both_sources_read":reads,
        "harness_undo_results":undo_results,"harness_restored_originals":restored,
        "reported_steps":steps.len(),"actions":steps.iter().map(|s| &s.action).collect::<Vec<_>>(),"events_sha256":sha(&home.join("events.json"))?,
        "approvals":approvals,"audit_verified":audit_verified,"actor_and_operations_audited":audited,"legacy_ungated_steps":legacy_steps,"gated_executed_steps":gated_steps,"owned_modeld_stopped":modeld_stopped,
        "scope":"Real local Qwen/worker/broker/host on a generated two-file pure-Python project. Automated fixture confirmations, online host with local-only model routing. Not native UI, network sandboxing, representative monorepo quality or multi-GPU acceptance."});
    std::fs::write(
        home.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{report}");
    if !ok {
        return Err(format!(
            "real edit acceptance failed; fixture retained at {}",
            fixture.display()
        )
        .into());
    }
    Ok(())
}
