//! Actual worker/broker/platform loop with a deterministic model fixture.
//! Optional agentd creation/restart; no weights, human UI or quality claim.
#[path = "support/agent_lifecycle.rs"]
mod agent_lifecycle;
use aos_ipc::{msg::Status, BusClient, BusService};
use aos_platform::{intents::workspace_execution, subsystem::PlatformConfig, PlatformSubsystem};
use aos_proto::{AgentOutputEvent, AgentSpec, InferRequest, TokenEvent};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let worker = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("usage: workspace_agent_context_probe ABSOLUTE_WORKER_EXE")?,
    );
    if !worker.is_absolute() || !worker.is_file() {
        return Err("worker must be an existing absolute executable".into());
    }
    let agentd = std::env::args().nth(2).map(PathBuf::from);
    if let Some(exe) = &agentd {
        if !exe.is_absolute() || !exe.is_file() || exe.parent() != worker.parent() {
            return Err("optional agentd must be absolute and beside worker".into());
        }
    }
    let home =
        std::env::temp_dir().join(format!("aos-agent-context-{:032x}", rand::random::<u128>()));
    std::fs::create_dir(&home)?;
    let project = home.join("project");
    std::fs::create_dir(&project)?;
    std::fs::write(
        project.join("first.txt"),
        "fixture source from the real platform\n",
    )?;
    std::fs::write(project.join("second.txt"), "another fixture source\n")?;
    std::fs::write(
        project.join("hidden.txt"),
        "password = 'SyntheticOnlyValue438927'",
    )?;
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
        net_mode: "offline_strict".into(),
        memory_v2: false,
        memory_v2_shadow: false,
    };
    let sub = PlatformSubsystem::open(&config)?;
    sub.workspaces
        .lock()
        .unwrap()
        .bind(&dir("project"), Some("demo"), "human:ui")?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?.to_string();
    let broker = tokio::spawn(aos_ipc::broker::serve(listener));
    let reports = Arc::new(Mutex::new(Vec::new()));
    let turns = Arc::new(AtomicUsize::new(0));
    let mut svc = BusService::new("worker-context-fixture");
    workspace_execution::register(&mut svc, sub.clone());
    svc.on("module.list", |ctx| async move {
        let _ = ctx
            .respond(Status::Ok, &Vec::<aos_proto::ModuleInfo>::new())
            .await;
    });
    let report_sink = reports.clone();
    if agentd.is_none() {
        svc.on("agent.report", move |ctx| {
            let sink = report_sink.clone();
            async move {
                if let Ok(report) = ctx.payload::<aos_agent::ReportPayload>() {
                    sink.lock().unwrap().push(report.event);
                }
                let _ = ctx.respond(Status::Ok, &true).await;
            }
        });
    }
    let model_turns = turns.clone();
    svc.on("model.infer",move |ctx| {
        let turns=model_turns.clone();
        async move {
            let req=ctx.payload::<InferRequest>().unwrap();
            let raw=if req.tools.is_empty() {
                json!({"complexity":"simple","reason":"deterministic worker fixture","needs_plan":false}).to_string()
            } else {
                assert!(req.tools.iter().any(|t|t["function"]["name"]=="workspace.inspect"));
                let turn=turns.fetch_add(1,Ordering::SeqCst);
                let (action,args)=match turn {
                    0 => ("workspace.inspect",json!({"root":"demo","page_entries":1})),
                    1..=3 => ("workspace.inspect",json!({"root":"demo","resume":true})),
                    4 => ("workspace.read",json!({"root":"demo","path":"first.txt"})),
                    5 => ("goal.complete",json!({"summary":"Deterministic worker fixture completed."})),
                    _ => ("goal.fail",json!({"reason":"unexpected extra fixture turn"})),
                };
                json!({"thought":"deterministic acceptance fixture","action":action,"args":args}).to_string()
            };
            let stream=ctx.open_stream();
            let _=stream.send(&TokenEvent::Started{inference_id:1}).await;
            let _=stream.send(&TokenEvent::Delta{text:raw}).await;
            let _=stream.send(&TokenEvent::Done{prompt_tokens:100,generated_tokens:30,ttft_ms:0.0,tok_s:0.0}).await;
            let _=stream.finish(Status::Ok).await;
        }
    });
    let service_addr = addr.clone();
    let service = tokio::spawn(async move { svc.serve(&service_addr).await });
    let control = BusClient::connect(&addr, "fixture-control").await?;
    let mut ready = false;
    for _ in 0..100 {
        if control
            .call::<(), bool>("bus.lookup:model.infer", &(), vec![])
            .await
            .unwrap_or(false)
        {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !ready {
        return Err("fixture service registration failed".into());
    }
    let spec: AgentSpec = serde_json::from_value(json!({
        "agent_id":"context-fixture","goal":{"statement":"Read the explicitly bound demo project using workspace context tools.","max_steps":8,"timeout_secs":90},
        "tools":["workspace.inspect","workspace.inventory","workspace.read"],
        "caps":["fs.read:/host/demo/**"],"gate_mode":"autonomous","model_id":"deterministic-fixture",
        "policy":{"tool_allowlist":["workspace.inspect","workspace.inventory","workspace.read","goal.complete","goal.fail"]}
    }))?;
    let spec_path = home.join("spec.json");
    std::fs::write(&spec_path, serde_json::to_vec_pretty(&spec)?)?;
    let log = std::fs::File::create(home.join("worker.log"))?;
    let mut lifecycle = Value::Null;
    let mut actor_id = spec.agent_id.clone();
    let worker_exit = if let Some(exe) = &agentd {
        let (events, report, id) = agent_lifecycle::run(exe, &home, &addr, &spec, "off").await?;
        *reports.lock().unwrap() = events;
        assert_eq!(report["ok"], true, "lifecycle acceptance failed: {report}");
        lifecycle = report;
        actor_id = id;
        None
    } else {
        let mut child = tokio::process::Command::new(&worker)
            .args([
                "--agent-id",
                "context-fixture",
                "--bus",
                &addr,
                "--spec-path",
            ])
            .arg(&spec_path)
            .current_dir(&home)
            .env("AOS_MODEL_GATE", "off") // Fixture-only: explicit legacy marker remains in trace.
            .stdout(log.try_clone()?)
            .stderr(log)
            .kill_on_drop(true)
            .spawn()?;
        let status = tokio::time::timeout(Duration::from_secs(120), child.wait()).await;
        let status = match status {
            Ok(status) => status?,
            Err(_) => {
                child.kill().await?;
                return Err(
                    format!("worker timeout; fixture retained at {}", home.display()).into(),
                );
            }
        };
        assert!(status.success(), "worker failed; see {}", home.display());
        status.code()
    };
    let state: Value = serde_json::from_slice(&std::fs::read(
        home.join("var/agents").join(&actor_id).join("state.json"),
    )?)?;
    let steps = reports
        .lock()
        .unwrap()
        .iter()
        .filter_map(|event| {
            if let AgentOutputEvent::Step(step) = event {
                Some(step.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let joined = serde_json::to_string(&steps)?;
    let final_state = reports.lock().unwrap().iter().rev().find_map(|event| {
        if let AgentOutputEvent::StateChanged { state } = event {
            Some(state.clone())
        } else {
            None
        }
    });
    assert_eq!(final_state, Some(aos_proto::AgentState::Done));
    assert_eq!(state["trace"].as_array().unwrap().len(), 8);
    for step in steps
        .iter()
        .filter(|s| aos_agent::workspace_context::is_context_tool(&s.action))
    {
        let start = step
            .tool_result
            .find('{')
            .ok_or("missing project context reply")?;
        let reply: Value = serde_json::from_str(&step.tool_result[start..])?;
        assert_eq!(reply["ok"], true);
    }
    assert_eq!(turns.load(Ordering::SeqCst), 6);
    assert!(joined.contains("fixture source from the real platform"));
    assert!(joined.contains("LEGACY_UNGATED_TOOL_PATH"));
    assert!(!joined.contains("SyntheticOnlyValue"));
    assert!(!joined.contains("invalid_project_context"));
    let cache: Value = serde_json::from_slice(&std::fs::read(
        home.join("sessions/workspace-inventory-demo.json"),
    )?)?;
    assert_eq!(cache["scan"]["complete"], true);
    assert_eq!(cache["scan"]["eligible_files"], 2);
    assert_eq!(cache["scan"]["finding_files"], 1);
    sub.audit.lock().unwrap().verify()?;
    let audit = std::fs::read_to_string(home.join("audit/audit.jsonl"))?;
    assert!(audit.contains(&format!("agent:{actor_id}")));
    assert!(!audit.contains("SyntheticOnlyValue"));
    assert_eq!(
        std::fs::read_to_string(project.join("first.txt"))?,
        "fixture source from the real platform\n"
    );
    assert_eq!(
        std::fs::read_to_string(project.join("second.txt"))?,
        "another fixture source\n"
    );
    assert_eq!(
        std::fs::read_to_string(project.join("hidden.txt"))?,
        "password = 'SyntheticOnlyValue438927'"
    );
    assert_eq!(std::fs::read_dir(&project)?.count(), 3);
    let report = json!({"ok":true,"fixture":home,"planner":"deterministic scripted model.infer service; no model weights",
        "worker_exit":worker_exit,"lifecycle":lifecycle,"planner_turns":turns.load(Ordering::SeqCst),"reported_steps":steps.len(),
        "worker_state":final_state,"complete_scan":cache["scan"]["complete"],"scan_pages":cache["continuation"]["pages"],
        "eligible_files":2,"withheld_fixture_files":1,"audit_verified":true,"source_unchanged":true,
        "model_gate":"off in fixture only; explicit LEGACY_UNGATED_TOOL_PATH verified",
        "scope":"Actual worker process, IPC broker and production platform; optional agentd lifecycle; not real-model autonomy, native UI or representative refactor evidence"});
    std::fs::write(
        home.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{report}");
    service.abort();
    broker.abort();
    Ok(())
}
