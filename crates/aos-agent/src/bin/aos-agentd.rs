//! `aos-agentd` — Agent Runtime daemon (P1.4).
//!
//! Lifecycle des agents (§4.3) : chaque agent est un processus
//! `aos-agent-worker` isolé. agentd détient le `CapStore` logique (aos-caps),
//! mint les capacités demandées à la création, et sert les intents `agent.*`.

use aos_agent::{intents, ControlCmd, ControlResp, ReportPayload, SubscribeRequest};
use aos_caps::{CapStore, HolderId};
use aos_ipc::{BusClient, BusService};
use aos_proto::{
    AgentCreateRequest, AgentCreateResponse, AgentIdRequest, AgentInfo, AgentOutputEvent,
    AgentState, AgentSteerRequest,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::{mpsc, Mutex};

struct AgentEntry {
    info: AgentInfo,
    subscribers: Vec<mpsc::Sender<AgentOutputEvent>>,
}

struct Runtime {
    agents: HashMap<String, AgentEntry>,
    caps: CapStore,
    next_id: AtomicU64,
}

type Shared = Arc<Mutex<Runtime>>;

async fn broadcast(entry: &mut AgentEntry, event: &AgentOutputEvent) {
    entry.subscribers.retain(|tx| !tx.is_closed());
    for tx in &entry.subscribers {
        let _ = tx.send(event.clone()).await;
    }
}

fn worker_exe_path() -> std::path::PathBuf {
    let exe = std::env::current_exe().expect("current_exe");
    let dir = exe.parent().expect("dir du binaire");
    dir.join(if cfg!(windows) {
        "aos-agent-worker.exe"
    } else {
        "aos-agent-worker"
    })
}

/// Appelle l'intent `agent.<id>.control` du worker.
async fn send_control(bus: &BusClient, agent_id: &str, cmd: &ControlCmd) -> ControlResp {
    let intent = format!("agent.{agent_id}.control");
    match bus
        .call::<ControlCmd, ControlResp>(&intent, cmd, vec![])
        .await
    {
        Ok(r) => r,
        Err(e) => ControlResp::Error(e.to_string()),
    }
}

#[tokio::main]
async fn main() {
    let bus_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("127.0.0.1:{}", aos_ipc::DEFAULT_BUS_PORT));
    let bus = BusClient::connect(&bus_addr, "agentd")
        .await
        .expect("connexion au bus — lancer aos-busd d'abord");
    eprintln!("[aos-agentd] connecté au bus {bus_addr}");

    let shared: Shared = Arc::new(Mutex::new(Runtime {
        agents: HashMap::new(),
        caps: CapStore::new(),
        next_id: AtomicU64::new(1),
    }));

    let mut svc = BusService::new("agentd");

    // --- agent.create : mint caps + spawn worker isolé ---
    {
        let shared = shared.clone();
        let bus_addr2 = bus_addr.clone();
        svc.on(intents::CREATE, move |ctx| {
            let shared = shared.clone();
            let bus_addr = bus_addr2.clone();
            async move {
                let req: AgentCreateRequest = match ctx.payload() {
                    Ok(r) => r,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                let n = {
                    let rt = shared.lock().await;
                    rt.next_id.fetch_add(1, Ordering::Relaxed)
                };
                let agent_id = format!("agent-{n}");
                // Mint logique (holder = agent) — P1 : caps logiques.
                {
                    let mut rt = shared.lock().await;
                    let holder = HolderId(n);
                    for uri in &req.caps {
                        rt.caps
                            .mint(holder, uri.clone(), aos_caps::Rights::all(), None, None, 0);
                    }
                }
                let child = Command::new(worker_exe_path())
                    .arg("--agent-id")
                    .arg(&agent_id)
                    .arg("--bus")
                    .arg(&bus_addr)
                    .arg("--directive")
                    .arg(&req.directive)
                    .arg("--caps")
                    .arg(req.caps.join(","))
                    .args(match &req.model_id {
                        Some(m) => vec!["--model".to_string(), m.clone()],
                        None => vec![],
                    })
                    .spawn();
                let mut child = match child {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = ctx
                            .respond_error(
                                aos_ipc::msg::Status::InternalError,
                                &format!("spawn worker: {e}"),
                            )
                            .await;
                        return;
                    }
                };
                let pid = child.id();
                {
                    let mut rt = shared.lock().await;
                    rt.agents.insert(
                        agent_id.clone(),
                        AgentEntry {
                            info: AgentInfo {
                                agent_id: agent_id.clone(),
                                state: AgentState::Created,
                                directive: req.directive.clone(),
                                pid,
                                caps: req.caps.clone(),
                                last_output: String::new(),
                            },
                            subscribers: Vec::new(),
                        },
                    );
                }
                eprintln!("[aos-agentd] {agent_id} créé (pid {pid:?})");
                let _ = ctx
                    .respond(
                        aos_ipc::msg::Status::Ok,
                        &AgentCreateResponse {
                            agent_id: agent_id.clone(),
                        },
                    )
                    .await;

                // Surveillance asynchrone du processus fils.
                let shared2 = shared.clone();
                let agent_id2 = agent_id.clone();
                tokio::spawn(async move {
                    let _ = child.wait().await;
                    let mut rt = shared2.lock().await;
                    if let Some(entry) = rt.agents.get_mut(&agent_id2) {
                        entry.info.pid = None;
                        if !matches!(entry.info.state, AgentState::Killed) {
                            entry.info.state = AgentState::Done;
                            let ev = AgentOutputEvent::StateChanged {
                                state: AgentState::Done,
                            };
                            broadcast(entry, &ev).await;
                        }
                    }
                });
            }
        });
    }

    // --- agent.pause ---
    {
        let bus2 = bus.clone();
        svc.on(intents::PAUSE, move |ctx| {
            let bus = bus2.clone();
            async move {
                match ctx.payload::<AgentIdRequest>() {
                    Ok(req) => {
                        let r = send_control(&bus, &req.agent_id, &ControlCmd::Pause).await;
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

    // --- agent.resume ---
    {
        let bus2 = bus.clone();
        svc.on(intents::RESUME, move |ctx| {
            let bus = bus2.clone();
            async move {
                match ctx.payload::<AgentIdRequest>() {
                    Ok(req) => {
                        let r = send_control(&bus, &req.agent_id, &ControlCmd::Resume).await;
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

    // --- agent.steer ---
    {
        let bus2 = bus.clone();
        svc.on(intents::STEER, move |ctx| {
            let bus = bus2.clone();
            async move {
                match ctx.payload::<AgentSteerRequest>() {
                    Ok(req) => {
                        let r = send_control(
                            &bus,
                            &req.agent_id,
                            &ControlCmd::Steer {
                                directive: req.directive.clone(),
                            },
                        )
                        .await;
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

    // --- agent.kill : terminaison du processus, sans impact ailleurs ---
    {
        let shared = shared.clone();
        svc.on(intents::KILL, move |ctx| {
            let shared = shared.clone();
            async move {
                match ctx.payload::<AgentIdRequest>() {
                    Ok(req) => {
                        let mut rt = shared.lock().await;
                        match rt.agents.get_mut(&req.agent_id) {
                            Some(entry) => {
                                entry.info.state = AgentState::Killed;
                                let ev = AgentOutputEvent::StateChanged {
                                    state: AgentState::Killed,
                                };
                                broadcast(entry, &ev).await;
                                // Le processus est tué via son handle détenu
                                // par la tâche de surveillance (voir create) —
                                // ici on force via le pid si encore vivant.
                                if let Some(pid) = entry.info.pid.take() {
                                    kill_pid(pid);
                                }
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &true).await;
                            }
                            None => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::NotFound, "agent inconnu")
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

    // --- agent.state / agent.list ---
    {
        let shared = shared.clone();
        svc.on(intents::STATE, move |ctx| {
            let shared = shared.clone();
            async move {
                match ctx.payload::<AgentIdRequest>() {
                    Ok(req) => {
                        let rt = shared.lock().await;
                        match rt.agents.get(&req.agent_id) {
                            Some(entry) => {
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &entry.info).await;
                            }
                            None => {
                                let _ = ctx
                                    .respond_error(aos_ipc::msg::Status::NotFound, "agent inconnu")
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
        let shared = shared.clone();
        svc.on(intents::LIST, move |ctx| {
            let shared = shared.clone();
            async move {
                let rt = shared.lock().await;
                let list: Vec<AgentInfo> = rt.agents.values().map(|e| e.info.clone()).collect();
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &list).await;
            }
        });
    }

    // --- agent.subscribe : flux de sortie temps réel ---
    {
        let shared = shared.clone();
        svc.on(intents::SUBSCRIBE, move |ctx| {
            let shared = shared.clone();
            async move {
                match ctx.payload::<SubscribeRequest>() {
                    Ok(req) => {
                        let (tx, mut rx) = mpsc::channel::<AgentOutputEvent>(128);
                        {
                            let mut rt = shared.lock().await;
                            if let Some(entry) = rt.agents.get_mut(&req.agent_id) {
                                entry.subscribers.push(tx);
                            }
                        }
                        let stream = ctx.open_stream();
                        tokio::spawn(async move {
                            while let Some(ev) = rx.recv().await {
                                if stream.send(&ev).await.is_err() {
                                    return;
                                }
                            }
                            let _ = stream.finish(aos_ipc::msg::Status::Ok).await;
                        });
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

    // --- agent.report (worker → agentd) ---
    {
        let shared = shared.clone();
        svc.on(intents::REPORT, move |ctx| {
            let shared = shared.clone();
            async move {
                match ctx.payload::<ReportPayload>() {
                    Ok(rep) => {
                        let mut rt = shared.lock().await;
                        if let Some(entry) = rt.agents.get_mut(&rep.agent_id) {
                            match &rep.event {
                                AgentOutputEvent::Token { text } => {
                                    entry.info.last_output.push_str(text);
                                    if entry.info.last_output.len() > 4000 {
                                        let cut = entry.info.last_output.len() - 4000;
                                        entry.info.last_output.drain(..cut);
                                    }
                                }
                                AgentOutputEvent::StateChanged { state } => {
                                    entry.info.state = state.clone();
                                }
                                _ => {}
                            }
                            broadcast(entry, &rep.event).await;
                        }
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

    // --- agent.snapshot : état cognitif → var/agents/<id>.json ---
    {
        let bus2 = bus.clone();
        svc.on(intents::SNAPSHOT, move |ctx| {
            let bus = bus2.clone();
            async move {
                match ctx.payload::<AgentIdRequest>() {
                    Ok(req) => {
                        let r = send_control(&bus, &req.agent_id, &ControlCmd::Snapshot).await;
                        match r {
                            ControlResp::State(state) => {
                                let dir = std::path::Path::new("var/agents");
                                let _ = std::fs::create_dir_all(dir);
                                let path = dir.join(format!("{}.json", req.agent_id));
                                let ok = state
                                    .to_json()
                                    .map(|j| std::fs::write(&path, j).is_ok())
                                    .unwrap_or(false);
                                let _ = ctx
                                    .respond(
                                        aos_ipc::msg::Status::Ok,
                                        &path.to_string_lossy().to_string(),
                                    )
                                    .await;
                                let _ = ok;
                            }
                            other => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::InternalError,
                                        &format!("snapshot impossible: {other:?}"),
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

    eprintln!("[aos-agentd] prêt");
    let _ = svc.serve(&bus_addr).await;
}

/// Tue un processus par pid (cross-platform minimal).
fn kill_pid(pid: u32) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F", "/T"])
            .output();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output();
    }
}
