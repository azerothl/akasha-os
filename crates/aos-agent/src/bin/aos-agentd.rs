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
    AgentStartRequest, AgentState, AgentSteerRequest, AgentStepRecord, AgentTrace, ChatAttachment,
    ChatMessage, ChatSessionAppendRequest, InferParams, InferRequest, McpServerInfo, SkillInfo,
    TokenEvent,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::{mpsc, Mutex};

struct AgentEntry {
    info: AgentInfo,
    subscribers: Vec<mpsc::Sender<AgentOutputEvent>>,
    trace: Vec<AgentStepRecord>,
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
        // try_send : un abonné lent ne doit pas figer agent.report / le worker.
        let _ = tx.try_send(event.clone());
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
        session_id: req.session_id.clone(),
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

    let restored = if restore {
        persist::read_state(agent_id)
    } else {
        None
    };
    let restored_trace = restored.as_ref().map(|s| s.trace.clone()).unwrap_or_default();
    let restored_tokens = restored.as_ref().map(|s| s.tokens_used).unwrap_or(0);

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
                    step: restored.as_ref().map(|s| s.step).unwrap_or(0),
                    max_steps: spec.goal.max_steps,
                    current_task: None,
                    parent_id: spec.parent_id.clone(),
                    children: restored
                        .as_ref()
                        .map(|s| s.children.clone())
                        .unwrap_or_default(),
                    tokens_used: restored_tokens,
                    skills: spec.skills.clone(),
                    tools: spec.tools.clone(),
                    mcp_servers: spec.mcp_servers.clone(),
                    fail_reason: None,
                    session_id: spec.session_id.clone(),
                },
                subscribers: Vec::new(),
                trace: restored_trace,
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
    // --- agent.retry ---
    {
        let shared = shared.clone();
        let bus2 = bus.clone();
        let bus_addr2 = bus_addr.clone();
        svc.on(intents::RETRY, move |ctx| {
            let shared = shared.clone();
            let bus = bus2.clone();
            let bus_addr = bus_addr2.clone();
            async move {
                match ctx.payload::<AgentIdRequest>() {
                    Ok(req) => {
                        let (state, pid, fail_reason, last_action) = {
                            let rt = shared.lock().await;
                            match rt.agents.get(&req.agent_id) {
                                Some(e) => {
                                    let last_action = e
                                        .trace
                                        .last()
                                        .map(|s| s.action.clone())
                                        .unwrap_or_default();
                                    (
                                        e.info.state.clone(),
                                        e.info.pid,
                                        e.info.fail_reason.clone(),
                                        last_action,
                                    )
                                }
                                None => {
                                    let _ = ctx
                                        .respond_error(
                                            aos_ipc::msg::Status::NotFound,
                                            "agent inconnu",
                                        )
                                        .await;
                                    return;
                                }
                            }
                        };

                        match state {
                            AgentState::Paused | AgentState::Blocked if pid.is_some() => {
                                let steer = format!(
                                    "continue — lève le blocage ({})",
                                    fail_reason.unwrap_or_else(|| "attente".into())
                                );
                                let _ = send_control(
                                    &bus,
                                    &req.agent_id,
                                    &ControlCmd::Steer { directive: steer },
                                )
                                .await;
                                let r =
                                    send_control(&bus, &req.agent_id, &ControlCmd::Resume).await;
                                {
                                    let mut rt = shared.lock().await;
                                    if let Some(e) = rt.agents.get_mut(&req.agent_id) {
                                        e.info.fail_reason = None;
                                        e.info.state = AgentState::Running;
                                        persist::update_info_sidecar(&e.info);
                                    }
                                }
                                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &r).await;
                            }
                            AgentState::Failed | AgentState::Killed | AgentState::Done
                                if pid.is_none() =>
                            {
                                let Some(mut spec) = persist::read_spec(&req.agent_id) else {
                                    let _ = ctx
                                        .respond_error(
                                            aos_ipc::msg::Status::NotFound,
                                            "spec introuvable",
                                        )
                                        .await;
                                    return;
                                };
                                let reason = fail_reason
                                    .clone()
                                    .unwrap_or_else(|| "échec inconnu".into());
                                if reason.contains("max_steps") {
                                    spec.goal.max_steps = spec.goal.max_steps.saturating_add(8);
                                    if let Some(ms) = spec.budget.max_steps.as_mut() {
                                        *ms = ms.saturating_add(8);
                                    }
                                    let _ = persist::write_spec(&spec);
                                }
                                if let Some(mut st) = persist::read_state(&req.agent_id) {
                                    st.push_user(&format!(
                                        "[retry] dernière action `{last_action}` a échoué : {reason}. Réessaie autrement."
                                    ));
                                    let _ = persist::write_state(&st);
                                }
                                match spawn_worker(
                                    &shared,
                                    &bus_addr,
                                    &req.agent_id,
                                    &spec,
                                    true,
                                )
                                .await
                                {
                                    Ok(_) => {
                                        let mut rt = shared.lock().await;
                                        if let Some(e) = rt.agents.get_mut(&req.agent_id) {
                                            e.info.fail_reason = None;
                                            e.info.state = AgentState::Running;
                                            e.info.max_steps = spec.goal.max_steps;
                                            persist::update_info_sidecar(&e.info);
                                        }
                                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &true).await;
                                    }
                                    Err(e) => {
                                        let _ = ctx
                                            .respond_error(
                                                aos_ipc::msg::Status::InternalError,
                                                &e,
                                            )
                                            .await;
                                    }
                                }
                            }
                            other => {
                                let _ = ctx
                                    .respond_error(
                                        aos_ipc::msg::Status::BadRequest,
                                        &format!("retry impossible depuis l'état {other:?}"),
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

    // --- agent.trace ---
    {
        let shared = shared.clone();
        svc.on(intents::TRACE, move |ctx| {
            let shared = shared.clone();
            async move {
                match ctx.payload::<AgentIdRequest>() {
                    Ok(req) => {
                        let live: Option<(AgentTrace, Option<String>)> = {
                            let rt = shared.lock().await;
                            rt.agents.get(&req.agent_id).map(|e| {
                                let mut t = persist::assemble_trace(
                                    &req.agent_id,
                                    persist::read_spec(&req.agent_id).as_ref(),
                                    persist::read_state(&req.agent_id).as_ref(),
                                    Some(&e.trace),
                                );
                                if t.fail_reason.is_none() {
                                    t.fail_reason = e.info.fail_reason.clone();
                                }
                                (t, e.info.fail_reason.clone())
                            })
                        };
                        let mut trace = live
                            .map(|(t, _)| t)
                            .unwrap_or_else(|| persist::load_trace(&req.agent_id));
                        if trace.fail_reason.is_none() {
                            if let Ok(info) = std::fs::read_to_string(
                                persist::agent_dir(&req.agent_id).join("info.json"),
                            ) {
                                if let Ok(i) = serde_json::from_str::<AgentInfo>(&info) {
                                    trace.fail_reason = i.fail_reason;
                                }
                            }
                        }
                        if trace.steps.is_empty()
                            && trace.working_memory.is_empty()
                            && persist::read_spec(&req.agent_id).is_none()
                        {
                            let _ = ctx
                                .respond_error(aos_ipc::msg::Status::NotFound, "agent inconnu")
                                .await;
                        } else {
                            let _ = ctx.respond(aos_ipc::msg::Status::Ok, &trace).await;
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
        let bus_report = bus.clone();
        svc.on(intents::REPORT, move |ctx| {
            let shared = shared.clone();
            let bus = bus_report.clone();
            async move {
                match ctx.payload::<ReportPayload>() {
                    Ok(rep) => {
                        let mut chat_summary: Option<(String, String, String, String, String)> =
                            None;
                        {
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
                                        let prev = entry.info.state.clone();
                                        entry.info.state = state.clone();
                                        if matches!(
                                            state,
                                            AgentState::Running | AgentState::Done
                                        ) {
                                            entry.info.fail_reason = None;
                                        }
                                        let terminal = matches!(
                                            state,
                                            AgentState::Done
                                                | AgentState::Failed
                                                | AgentState::Killed
                                        );
                                        let was_active = !matches!(
                                            prev,
                                            AgentState::Done
                                                | AgentState::Failed
                                                | AgentState::Killed
                                        );
                                        if terminal && was_active {
                                            if let Some(sid) = entry.info.session_id.clone() {
                                                let summary = match state {
                                                    AgentState::Done => {
                                                        let out = entry.info.last_output.trim();
                                                        if out.is_empty() {
                                                            format!(
                                                                "Agent {} terminé.",
                                                                entry.info.agent_id
                                                            )
                                                        } else {
                                                            let excerpt: String =
                                                                out.chars().take(500).collect();
                                                            format!(
                                                                "Agent {} terminé.\n{}",
                                                                entry.info.agent_id, excerpt
                                                            )
                                                        }
                                                    }
                                                    AgentState::Failed => {
                                                        let reason = entry
                                                            .info
                                                            .fail_reason
                                                            .clone()
                                                            .unwrap_or_else(|| {
                                                                "échec".into()
                                                            });
                                                        format!(
                                                            "Agent {} a échoué : {}",
                                                            entry.info.agent_id, reason
                                                        )
                                                    }
                                                    AgentState::Killed => format!(
                                                        "Agent {} arrêté.",
                                                        entry.info.agent_id
                                                    ),
                                                    _ => String::new(),
                                                };
                                                chat_summary = Some((
                                                    sid,
                                                    entry.info.agent_id.clone(),
                                                    entry.info.directive.clone(),
                                                    summary,
                                                    format!("{state:?}").to_lowercase(),
                                                ));
                                            }
                                        }
                                    }
                                    AgentOutputEvent::Error { message } => {
                                        entry.info.fail_reason = Some(message.clone());
                                    }
                                    AgentOutputEvent::Progress {
                                        step,
                                        max_steps,
                                        current_task,
                                    } => {
                                        entry.info.step = *step;
                                        entry.info.max_steps = *max_steps;
                                        entry.info.current_task = current_task.clone();
                                        entry.info.last_output.clear();
                                    }
                                    AgentOutputEvent::ChildSpawned { child_id, .. } => {
                                        if !entry.info.children.contains(child_id) {
                                            entry.info.children.push(child_id.clone());
                                        }
                                    }
                                    AgentOutputEvent::Step(rec) => {
                                        if let Some(existing) =
                                            entry.trace.iter_mut().find(|s| s.step == rec.step)
                                        {
                                            *existing = rec.clone();
                                        } else {
                                            entry.trace.push(rec.clone());
                                        }
                                        entry.info.tokens_used = entry
                                            .trace
                                            .iter()
                                            .map(|s| s.generated_tokens as u64)
                                            .sum();
                                        if let Some(reason) = &rec.fail_reason {
                                            entry.info.fail_reason = Some(reason.clone());
                                        }
                                        entry.info.last_output.clear();
                                    }
                                    _ => {}
                                }
                                persist::update_info_sidecar(&entry.info);
                                broadcast(entry, &rep.event).await;
                            }
                        }
                        if let Some((session_id, agent_id, title, content, _state)) = chat_summary {
                            let _ = bus
                                .call::<ChatSessionAppendRequest, aos_proto::ChatSessionMessage>(
                                    "chat.session.append",
                                    &ChatSessionAppendRequest {
                                        session_id,
                                        role: "assistant".into(),
                                        content,
                                        attachments: vec![ChatAttachment::AgentRef {
                                            agent_id,
                                            title,
                                            origin: "completion".into(),
                                        }],
                                    },
                                    vec![],
                                )
                                .await;
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
