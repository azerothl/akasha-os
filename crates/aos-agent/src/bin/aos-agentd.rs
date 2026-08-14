//! `aos-agentd` — Agent Runtime daemon (agentic).
//!
//! Lifecycle : crée un `AgentSpec` persisté, spawn `aos-agent-worker` avec
//! `--spec-path`. Skills, MCP catalogue, prompt optimize.

use aos_agent::mcp::list_mcp_servers;
use aos_agent::persist::{self, registry_add};
use aos_agent::prompt::optimize_prompt_request;
use aos_agent::skills::{get_skill, list_skills, load_skills, merge_skill_tools};
use aos_agent::tools::{caps_for_tools, select_tools};
use aos_agent::{intents, ControlCmd, ControlResp, ReportPayload, SubscribeRequest};
use aos_caps::{CapStore, HolderId};
use aos_ipc::{BusClient, BusService};
use aos_proto::{
    AgentCreateRequest, AgentCreateResponse, AgentIdRequest, AgentInfo,
    AgentOutputEvent, AgentPromptOptimizeRequest, AgentPromptOptimizeResponse, AgentSpec,
    AgentStartRequest, AgentState, AgentSteerRequest, ChatMessage, InferParams, InferRequest,
    McpServerInfo, SkillInfo, TokenEvent,
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

fn build_spec(agent_id: &str, req: &AgentCreateRequest) -> AgentSpec {
    let goal = req.resolved_goal();
    let skill_docs = load_skills(&req.skills);
    let tool_ids = merge_skill_tools(&req.tools, &skill_docs);
    let tools = select_tools(&tool_ids, &[]);
    let mut caps = req.caps.clone();
    for c in caps_for_tools(&tools, &req.mcp_servers) {
        if !caps.contains(&c) {
            caps.push(c);
        }
    }
    // Default notes cap for simple /agent flows with empty tools
    if caps.is_empty() {
        caps.push("tool.invoke:notes".into());
    }
    AgentSpec {
        agent_id: agent_id.to_string(),
        goal,
        system_prompt: req.system_prompt.clone(),
        skills: req.skills.clone(),
        tools: tool_ids,
        mcp_servers: req.mcp_servers.clone(),
        documents: req.documents.clone(),
        caps,
        model_id: req.model_id.clone(),
        parent_id: req.parent_id.clone(),
        budget: req.budget.clone(),
        optimize_prompt: req.optimize_prompt,
    }
}

async fn spawn_worker(
    shared: &Shared,
    bus_addr: &str,
    agent_id: &str,
    spec: &AgentSpec,
    restore: bool,
) -> Result<u32, String> {
    let spec_path = persist::write_spec(spec).map_err(|e| e.to_string())?;
    registry_add(agent_id);

    let mut cmd = Command::new(worker_exe_path());
    cmd.arg("--agent-id")
        .arg(agent_id)
        .arg("--bus")
        .arg(bus_addr)
        .arg("--spec-path")
        .arg(&spec_path);
    if restore {
        cmd.arg("--restore");
    }
    let child = cmd.spawn().map_err(|e| format!("spawn worker: {e}"))?;
    let pid = child.id();

    {
        let mut rt = shared.lock().await;
        // Mint caps
        let n = agent_id
            .strip_prefix("agent-")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(1);
        let holder = HolderId(n);
        for uri in &spec.caps {
            rt.caps
                .mint(holder, uri.clone(), aos_caps::Rights::all(), None, None, 0);
        }
        rt.agents.insert(
            agent_id.to_string(),
            AgentEntry {
                info: AgentInfo {
                    agent_id: agent_id.to_string(),
                    state: AgentState::Created,
                    directive: spec.goal.statement.clone(),
                    pid,
                    caps: spec.caps.clone(),
                    last_output: String::new(),
                    step: 0,
                    max_steps: spec.goal.max_steps,
                    current_task: None,
                    parent_id: spec.parent_id.clone(),
                    children: Vec::new(),
                },
                subscribers: Vec::new(),
            },
        );
        persist::update_info_sidecar(&rt.agents[agent_id].info);
    }

    let shared2 = shared.clone();
    let agent_id2 = agent_id.to_string();
    let mut child = child;
    tokio::spawn(async move {
        let _ = child.wait().await;
        let mut rt = shared2.lock().await;
        if let Some(entry) = rt.agents.get_mut(&agent_id2) {
            entry.info.pid = None;
            if !matches!(
                entry.info.state,
                AgentState::Killed | AgentState::Done | AgentState::Failed
            ) {
                entry.info.state = AgentState::Done;
                let ev = AgentOutputEvent::StateChanged {
                    state: AgentState::Done,
                };
                broadcast(entry, &ev).await;
            }
            persist::update_info_sidecar(&entry.info);
        }
    });

    Ok(pid.unwrap_or(0))
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

    // --- agent.create ---
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
                let spec = build_spec(&agent_id, &req);
                match spawn_worker(&shared, &bus_addr, &agent_id, &spec, false).await {
                    Ok(pid) => {
                        eprintln!("[aos-agentd] {agent_id} créé (pid {pid})");
                        let _ = ctx
                            .respond(
                                aos_ipc::msg::Status::Ok,
                                &AgentCreateResponse { agent_id },
                            )
                            .await;
                    }
                    Err(e) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::InternalError, &e)
                            .await;
                    }
                }
            }
        });
    }

    // --- agent.start (restore) ---
    {
        let shared = shared.clone();
        let bus_addr2 = bus_addr.clone();
        svc.on(intents::START, move |ctx| {
            let shared = shared.clone();
            let bus_addr = bus_addr2.clone();
            async move {
                let req: AgentStartRequest = match ctx.payload() {
                    Ok(r) => r,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                let Some(spec) = persist::read_spec(&req.agent_id) else {
                    let _ = ctx
                        .respond_error(aos_ipc::msg::Status::NotFound, "spec introuvable")
                        .await;
                    return;
                };
                // Already running?
                {
                    let rt = shared.lock().await;
                    if let Some(e) = rt.agents.get(&req.agent_id) {
                        if e.info.pid.is_some() {
                            let _ = ctx
                                .respond_error(
                                    aos_ipc::msg::Status::BadRequest,
                                    "agent déjà en cours",
                                )
                                .await;
                            return;
                        }
                    }
                }
                match spawn_worker(&shared, &bus_addr, &req.agent_id, &spec, true).await {
                    Ok(_) => {
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &true).await;
                    }
                    Err(e) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::InternalError, &e)
                            .await;
                    }
                }
            }
        });
    }

    // --- pause / resume / steer / kill (inchangés dans l'esprit) ---
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
                                if let Some(pid) = entry.info.pid.take() {
                                    kill_pid(pid);
                                }
                                persist::update_info_sidecar(&entry.info);
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

    // --- state / list ---
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

    // --- subscribe ---
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

    // --- report ---
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
                                AgentOutputEvent::Progress {
                                    step,
                                    max_steps,
                                    current_task,
                                } => {
                                    entry.info.step = *step;
                                    entry.info.max_steps = *max_steps;
                                    entry.info.current_task = current_task.clone();
                                }
                                AgentOutputEvent::ChildSpawned { child_id, .. } => {
                                    if !entry.info.children.contains(child_id) {
                                        entry.info.children.push(child_id.clone());
                                    }
                                }
                                _ => {}
                            }
                            persist::update_info_sidecar(&entry.info);
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

    // --- snapshot ---
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
                                let _ = persist::write_state(&state);
                                let path = persist::agent_dir(&req.agent_id).join("state.json");
                                let _ = ctx
                                    .respond(
                                        aos_ipc::msg::Status::Ok,
                                        &path.to_string_lossy().to_string(),
                                    )
                                    .await;
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

    // --- agent.grant (hot-grant) ---
    {
        let shared2 = shared.clone();
        let bus2 = bus.clone();
        svc.on(intents::GRANT, move |ctx| {
            let shared = shared2.clone();
            let bus = bus2.clone();
            async move {
                match ctx.payload::<aos_proto::AgentGrantRequest>() {
                    Ok(req) => {
                        {
                            let mut rt = shared.lock().await;
                            let n = req
                                .agent_id
                                .strip_prefix("agent-")
                                .and_then(|s| s.parse::<u64>().ok())
                                .unwrap_or(1);
                            rt.caps.mint(
                                HolderId(n),
                                req.cap.clone(),
                                aos_caps::Rights::all(),
                                None,
                                None,
                                0,
                            );
                            if let Some(entry) = rt.agents.get_mut(&req.agent_id) {
                                if !entry.info.caps.contains(&req.cap) {
                                    entry.info.caps.push(req.cap.clone());
                                }
                                persist::update_info_sidecar(&entry.info);
                            }
                        }
                        let _ = send_control(
                            &bus,
                            &req.agent_id,
                            &ControlCmd::GrantCap {
                                cap: req.cap.clone(),
                            },
                        )
                        .await;
                        // Persist into spec.json
                        if let Some(mut spec) = persist::read_spec(&req.agent_id) {
                            if !spec.caps.contains(&req.cap) {
                                spec.caps.push(req.cap.clone());
                                let _ = persist::write_spec(&spec);
                            }
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

    // --- skill.list / skill.get ---
    {
        svc.on(intents::SKILL_LIST, move |ctx| {
            async move {
                let list: Vec<SkillInfo> = list_skills();
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &list).await;
            }
        });
    }
    {
        svc.on(intents::SKILL_GET, move |ctx| {
            async move {
                let name: String = match ctx.payload() {
                    Ok(n) => n,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                match get_skill(&name) {
                    Some(s) => {
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &s).await;
                    }
                    None => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::NotFound, "skill inconnue")
                            .await;
                    }
                }
            }
        });
    }

    // --- mcp.list ---
    {
        svc.on(intents::MCP_LIST, move |ctx| {
            async move {
                let list: Vec<McpServerInfo> = list_mcp_servers();
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &list).await;
            }
        });
    }

    // --- agent.prompt.optimize ---
    {
        let bus2 = bus.clone();
        svc.on(intents::PROMPT_OPTIMIZE, move |ctx| {
            let bus = bus2.clone();
            async move {
                let req: AgentPromptOptimizeRequest = match ctx.payload() {
                    Ok(r) => r,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                let prompt = optimize_prompt_request(
                    &req.goal,
                    &req.skills,
                    &req.tools,
                    req.current_prompt.as_deref(),
                );
                let infer = InferRequest {
                    model_id: req.model_id.clone(),
                    messages: vec![ChatMessage {
                        role: "user".into(),
                        content: prompt,
                    }],
                    params: InferParams {
                        max_tokens: 512,
                        temperature: 0.3,
                        ..InferParams::default()
                    },
                    priority: 2,
                    data_refs: vec![],
                    routing: None,
                };
                match bus
                    .call_stream::<InferRequest, TokenEvent>("model.infer", &infer, vec![])
                    .await
                {
                    Ok(mut rx) => {
                        let mut text = String::new();
                        while let Some(ev) = rx.recv().await {
                            if let Ok(TokenEvent::Delta { text: t }) = ev {
                                text.push_str(&t);
                            }
                        }
                        let _ = ctx
                            .respond(
                                aos_ipc::msg::Status::Ok,
                                &AgentPromptOptimizeResponse {
                                    optimized_prompt: text.trim().to_string(),
                                },
                            )
                            .await;
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
        });
    }

    eprintln!("[aos-agentd] prêt (agentic)");
    let _ = svc.serve(&bus_addr).await;
}

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
