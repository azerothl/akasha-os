//! `aos-agentd` — Agent Runtime daemon (agentic).
//!
//! Lifecycle : crée un `AgentSpec` persisté, spawn `aos-agent-worker` avec
//! `--spec-path`. Skills, MCP catalogue, prompt optimize.

use aos_agent::deep_thinking::{self, deep_thinking_caps, DeepThinkingEngine};
use aos_agent::health::{self, HealthAction, HealthSample};
use aos_agent::mcp::{list_mcp_servers, load_servers_config, resolve_secret_placeholder};
use aos_agent::room_personas::{self, persona_agent_id, ROOM_PERSONAS};
use aos_agent::persist::{self, registry_add};
use aos_agent::prompt::optimize_prompt_request;
use aos_agent::room_runtime::{self, RoomRoundState};
use aos_agent::schedule::{self, ScheduleCreateRequest, ScheduleIdRequest, ScheduleListResponse};
use aos_agent::skills::{get_skill, list_skills, load_skills, merge_skill_tools};
use aos_agent::tools::{caps_for_tools, default_agent_tools, select_tools};
use aos_agent::{intents, ControlCmd, ControlResp, ReportPayload, SubscribeRequest};
use aos_caps::{CapStore, HolderId};
use aos_ipc::{BusClient, BusService};
use aos_proto::{
    AgentCreateRequest, AgentCreateResponse, AgentIdRequest, AgentInfo, AgentKind,
    AgentOutputEvent, AgentPromptOptimizeRequest, AgentPromptOptimizeResponse,
    AgentRoomConductRequest, AgentRoomTurnRequest, AgentRosterUpdateRequest, AgentSpec,
    AgentSpecResponse, AgentStartRequest, AgentState, AgentSteerRequest, AgentStepRecord,
    AgentTrace, CapInfo, CapListRequest, CapMintRequest, CapMintResponse, ChatAttachment,
    ChatMessage, ChatSessionAppendRequest, ChatSessionGetResponse, ChatSessionIdRequest,
    ChatSessionRoomTurnCancelRequest, CancelRequest, CognitiveMode, InferParams, InferRequest,
    McpServerInfo, PlanAppendLogRequest, PlanCreateRequest, PlanDelegateStepRequest,
    PlanGetRequest, PlanReplaceTreeRequest, PlanResponse, PlanUpdateStepRequest, SecretGetRequest,
    SkillInfo, TokenEvent,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::process::Command;
use tokio::sync::{mpsc, Mutex};

struct AgentEntry {
    info: AgentInfo,
    subscribers: Vec<mpsc::Sender<AgentOutputEvent>>,
    trace: Vec<AgentStepRecord>,
    last_activity: Instant,
    health_recoveries: u32,
}

impl AgentEntry {
    fn new(info: AgentInfo, trace: Vec<AgentStepRecord>) -> Self {
        Self {
            info,
            subscribers: Vec::new(),
            trace,
            last_activity: Instant::now(),
            health_recoveries: 0,
        }
    }
}

struct Runtime {
    agents: HashMap<String, AgentEntry>,
    caps: CapStore,
    room_rounds: HashMap<String, Arc<RoomRoundState>>,
    deep_thinking: DeepThinkingEngine,
}

type Shared = Arc<Mutex<Runtime>>;

async fn broadcast(entry: &mut AgentEntry, event: &AgentOutputEvent) {
    entry.subscribers.retain(|tx| !tx.is_closed());
    for tx in &entry.subscribers {
        // try_send : un abonné lent ne doit pas figer agent.report / le worker.
        let _ = tx.try_send(event.clone());
    }
}

/// Pousse la fin d'un sous-agent vers le parent (plan deep, UI, worker).
async fn notify_parent_child_terminal(
    shared: &Shared,
    bus: &BusClient,
    parent_id: &str,
    child_id: &str,
    result: &str,
    ok: bool,
) {
    {
        let mut rt = shared.lock().await;
        if let Ok(Some((plan, _))) = rt
            .deep_thinking
            .complete_delegated_child(parent_id, child_id, result)
        {
            if let Some(entry) = rt.agents.get_mut(parent_id) {
                entry.info.deep_plan = Some(plan.clone());
                persist::update_info_sidecar(&entry.info);
                broadcast(
                    entry,
                    &AgentOutputEvent::DeepPlanUpdated { plan },
                )
                .await;
            }
        }
        if let Some(entry) = rt.agents.get_mut(parent_id) {
            broadcast(
                entry,
                &AgentOutputEvent::ChildDone {
                    child_id: child_id.to_string(),
                    result: result.to_string(),
                },
            )
            .await;
        }
    }
    let _ = send_control(
        bus,
        parent_id,
        &ControlCmd::ChildFinished {
            child_id: child_id.to_string(),
            result: result.to_string(),
            ok,
        },
    )
    .await;
}

/// Exit without a worker `StateChanged` report is a failure, never a successful Done.
fn unexpected_child_exit_terminal(last_output: &str) -> (AgentState, String, bool) {
    let result = if last_output.trim().is_empty() {
        "sous-agent arrêté sans rapport de fin".into()
    } else {
        last_output.to_string()
    };
    (AgentState::Failed, result, false)
}

fn note_agent_activity(entry: &mut AgentEntry, event: &AgentOutputEvent) {
    entry.last_activity = Instant::now();
    if health::event_proves_progress(event) {
        entry.health_recoveries = 0;
    }
}

struct HealthJob {
    agent_id: String,
    action: HealthAction,
    idle_secs: u64,
    parent_id: Option<String>,
}

async fn run_health_pass(shared: &Shared, bus: &Arc<BusClient>, bus_addr: &str) {
    let jobs: Vec<HealthJob> = {
        let rt = shared.lock().await;
        rt.agents
            .iter()
            .filter_map(|(id, entry)| {
                let action = health::evaluate(&HealthSample {
                    state: entry.info.state.clone(),
                    kind: entry.info.kind,
                    pid: entry.info.pid,
                    last_action: entry
                        .trace
                        .last()
                        .map(|s| s.action.clone())
                        .unwrap_or_default(),
                    idle: entry.last_activity.elapsed(),
                    recoveries: entry.health_recoveries,
                });
                if action == HealthAction::None {
                    return None;
                }
                Some(HealthJob {
                    agent_id: id.clone(),
                    action,
                    idle_secs: entry.last_activity.elapsed().as_secs(),
                    parent_id: entry.info.parent_id.clone(),
                })
            })
            .collect()
    };
    for job in jobs {
        apply_health_action(shared, bus, bus_addr, job).await;
    }
}

async fn apply_health_action(
    shared: &Shared,
    bus: &Arc<BusClient>,
    bus_addr: &str,
    job: HealthJob,
) {
    let HealthJob {
        agent_id,
        action,
        idle_secs,
        parent_id,
    } = job;
    eprintln!(
        "[aos-agentd] health {agent_id} {:?} after {idle_secs}s idle",
        action
    );
    match action {
        HealthAction::None => {}
        HealthAction::Nudge | HealthAction::Unblock => {
            let line = if action == HealthAction::Unblock {
                format!(
                    "health : déblocage automatique (aucune activité depuis {idle_secs}s)"
                )
            } else {
                format!(
                    "health : reprise automatique (aucune activité depuis {idle_secs}s)"
                )
            };
            let _ = send_control(
                bus,
                &agent_id,
                &ControlCmd::Steer {
                    directive: format!(
                        "continue — le runtime n'a vu aucune activité depuis {idle_secs}s. \
                         Poursuis la tâche ou goal.complete / goal.fail."
                    ),
                },
            )
            .await;
            let _ = send_control(bus, &agent_id, &ControlCmd::Resume).await;
            let mut rt = shared.lock().await;
            if let Some(entry) = rt.agents.get_mut(&agent_id) {
                entry.health_recoveries = entry.health_recoveries.saturating_add(1);
                entry.last_activity = Instant::now();
                if entry.info.state == AgentState::Blocked {
                    entry.info.state = AgentState::Running;
                    entry.info.fail_reason = None;
                }
                persist::update_info_sidecar(&entry.info);
                broadcast(
                    entry,
                    &AgentOutputEvent::Log { line: line.clone() },
                )
                .await;
                if action == HealthAction::Unblock {
                    broadcast(
                        entry,
                        &AgentOutputEvent::StateChanged {
                            state: AgentState::Running,
                        },
                    )
                    .await;
                }
            }
        }
        HealthAction::Restart => {
            let (recoveries, pid) = {
                let mut rt = shared.lock().await;
                let Some(entry) = rt.agents.get_mut(&agent_id) else {
                    return;
                };
                entry.health_recoveries = entry.health_recoveries.saturating_add(1);
                (entry.health_recoveries, entry.info.pid.take())
            };
            if let Some(pid) = pid {
                kill_pid(pid);
            }
            let Some(spec) = persist::read_spec(&agent_id) else {
                return;
            };
            match spawn_worker(shared, bus_addr, &agent_id, &spec, true, Some(bus.clone())).await
            {
                Ok(_) => eprintln!("[aos-agentd] health {agent_id} worker relancé"),
                Err(e) => {
                    eprintln!("[aos-agentd] health restart {agent_id}: {e}");
                    return;
                }
            }
            let mut rt = shared.lock().await;
            if let Some(entry) = rt.agents.get_mut(&agent_id) {
                entry.health_recoveries = recoveries;
                entry.last_activity = Instant::now();
                broadcast(
                    entry,
                    &AgentOutputEvent::Log {
                        line: format!(
                            "health : worker relancé (aucune activité depuis {idle_secs}s)"
                        ),
                    },
                )
                .await;
            }
        }
        HealthAction::MarkFailed => {
            let pid = {
                let mut rt = shared.lock().await;
                let pid = rt.agents.get_mut(&agent_id).and_then(|e| e.info.pid.take());
                if let Some(entry) = rt.agents.get_mut(&agent_id) {
                    entry.info.state = AgentState::Failed;
                    entry.info.fail_reason =
                        Some("bloqué sans activité (health check)".into());
                    entry.last_activity = Instant::now();
                    persist::update_info_sidecar(&entry.info);
                    broadcast(
                        entry,
                        &AgentOutputEvent::Log {
                            line: format!(
                                "health : agent marqué Failed (aucune activité depuis {idle_secs}s)"
                            ),
                        },
                    )
                    .await;
                    broadcast(
                        entry,
                        &AgentOutputEvent::StateChanged {
                            state: AgentState::Failed,
                        },
                    )
                    .await;
                }
                pid
            };
            if let Some(pid) = pid {
                kill_pid(pid);
            }
            if let Some(parent_id) = parent_id {
                notify_parent_child_terminal(
                    shared,
                    bus,
                    &parent_id,
                    &agent_id,
                    "bloqué sans activité (health check)",
                    false,
                )
                .await;
            }
        }
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

/// Public `agent.pause` / `resume` / `steer` / `retry` return `bool`, not the
/// internal worker [`ControlResp`] (which encodes `Ack` as a CBOR string).
async fn respond_control(ctx: aos_ipc::IntentCtx, r: ControlResp) {
    match r {
        ControlResp::Ack | ControlResp::State(_) => {
            let _ = ctx.respond(aos_ipc::msg::Status::Ok, &true).await;
        }
        ControlResp::Error(e) => {
            let _ = ctx
                .respond_error(aos_ipc::msg::Status::InternalError, &e)
                .await;
        }
    }
}

fn build_spec(agent_id: &str, req: &AgentCreateRequest) -> AgentSpec {
    let mut spec = room_personas::roster_spec_from_request(agent_id, req);
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
    if caps.is_empty() && req.spawns_worker() {
        caps.push("tool.invoke:notes".into());
    }
    if req.cognitive_mode.is_deep_thinking() {
        for c in deep_thinking_caps(&spec.agent_id) {
            if !caps.contains(&c) {
                caps.push(c);
            }
        }
        if !spec.skills.iter().any(|s| s == "deep-thinking" || s == "planner") {
            spec.skills.push("deep-thinking".into());
        }
    }
    spec.tools = tool_ids;
    spec.caps = caps;
    if spec.display_name.as_deref().is_none_or(|s| s.trim().is_empty()) {
        let title = persist::agent_title(&spec.goal.statement);
        if !title.is_empty() {
            spec.display_name = Some(title);
        }
    }
    spec
}

fn apply_tool_caps(spec: &mut AgentSpec) {
    let skill_docs = load_skills(&spec.skills);
    let tool_ids = merge_skill_tools(&spec.tools, &skill_docs);
    let tools = select_tools(&tool_ids, &[]);
    spec.tools = tool_ids;
    let mut caps = Vec::new();
    for c in caps_for_tools(&tools, &spec.mcp_servers) {
        if !caps.contains(&c) {
            caps.push(c);
        }
    }
    if spec.tools.iter().any(|t| t.starts_with("module."))
        && !caps.iter().any(|c| c == "module.install")
    {
        caps.push("module.install".into());
    }
    spec.caps = caps;
}

fn update_roster_from_request(spec: &mut AgentSpec, req: &AgentRosterUpdateRequest) {
    if let Some(name) = req.display_name.as_ref() {
        let t = name.trim();
        if !t.is_empty() {
            spec.display_name = Some(t.to_string());
        }
    }
    if let Some(prompt) = req.system_prompt.as_ref() {
        spec.system_prompt = if prompt.trim().is_empty() {
            None
        } else {
            Some(prompt.clone())
        };
    }
    if let Some(role) = req.role.as_ref() {
        spec.goal.statement = role.clone();
    }
    spec.skills = req.skills.clone();
    spec.tools = req.tools.clone();
    spec.mcp_servers = req.mcp_servers.clone();
    if let Some(model) = req.model_id.as_ref() {
        spec.model_id = if model.trim().is_empty() {
            None
        } else {
            Some(model.clone())
        };
    }
    apply_tool_caps(spec);
}

fn roster_info_from_spec(spec: &AgentSpec) -> AgentInfo {
    let title = spec.roster_display_name().to_string();
    AgentInfo {
        agent_id: spec.agent_id.clone(),
        state: AgentState::Roster,
        directive: spec.goal.statement.clone(),
        pid: None,
        caps: spec.caps.clone(),
        last_output: String::new(),
        step: 0,
        max_steps: 0,
        current_task: None,
        parent_id: spec.parent_id.clone(),
        children: Vec::new(),
        tokens_used: 0,
        skills: spec.skills.clone(),
        tools: spec.tools.clone(),
        mcp_servers: spec.mcp_servers.clone(),
        fail_reason: None,
        session_id: spec.session_id.clone(),
        title,
        kind: AgentKind::Roster,
        display_name: spec.display_name.clone(),
        persona_id: spec.persona_id.clone(),
        origin: spec.origin.clone(),
        deep_plan: None,
        cognitive_mode: spec.cognitive_mode,
    }
}

async fn register_roster_agent(
    shared: &Shared,
    agent_id: &str,
    spec: &AgentSpec,
) -> Result<(), String> {
    persist::write_spec(spec).map_err(|e| e.to_string())?;
    persist::registry_add(agent_id);
    let info = roster_info_from_spec(spec);
    {
        let mut rt = shared.lock().await;
        rt.agents.insert(
            agent_id.to_string(),
            AgentEntry::new(info, Vec::new()),
        );
        persist::update_info_sidecar(&rt.agents[agent_id].info);
    }
    Ok(())
}

async fn ensure_builtin_persona_agents(shared: &Shared) {
    for persona in ROOM_PERSONAS {
        let agent_id = persona_agent_id(persona.id);
        if persist::read_spec(&agent_id).is_some() {
            continue;
        }
        let req = room_personas::persona_create_request(persona, None);
        let spec = build_spec(&agent_id, &req);
        match register_roster_agent(shared, &agent_id, &spec).await {
            Ok(()) => eprintln!("[aos-agentd] persona roster {agent_id} enregistrée"),
            Err(e) => eprintln!("[aos-agentd] persona {agent_id}: {e}"),
        }
    }
}

fn mcp_secrets_path(agent_id: &str) -> PathBuf {
    PathBuf::from("var/agents").join(agent_id).join("mcp_secrets.json")
}

/// Résout `${secret:…}` pour les serveurs MCP de l'agent et écrit un fichier
/// éphémère lu puis effacé par le worker.
/// Droits émis pour chaque cap agent publiée dans `aos-capkd`.
fn agent_cap_rights() -> Vec<String> {
    vec![
        "read".into(),
        "write".into(),
        "execute".into(),
        "grant".into(),
        "revoke".into(),
    ]
}

/// Publie les caps logiques de l'agent dans le noyau (`holder = agent:<id>`),
/// sans dupliquer les objets déjà présents. L'UI Capacité interroge `cap.list`
/// sur `aos-capkd`, pas le CapStore local d'agentd.
async fn publish_caps_to_capkd(bus: &BusClient, agent_id: &str, caps: &[String]) {
    if caps.is_empty() {
        return;
    }
    let holder = format!("agent:{agent_id}");
    let existing = bus
        .call::<CapListRequest, Vec<CapInfo>>(
            "cap.list",
            &CapListRequest {
                holder: holder.clone(),
            },
            vec![],
        )
        .await
        .unwrap_or_default();
    let have: HashSet<&str> = existing.iter().map(|c| c.object.as_str()).collect();
    let rights = agent_cap_rights();
    for object in caps {
        if have.contains(object.as_str()) {
            continue;
        }
        if let Err(e) = bus
            .call::<CapMintRequest, CapMintResponse>(
                "cap.mint",
                &CapMintRequest {
                    holder: holder.clone(),
                    object: object.clone(),
                    rights: rights.clone(),
                },
                vec![],
            )
            .await
        {
            eprintln!("[aos-agentd] cap.mint {holder}/{object}: {e}");
        }
    }
}

async fn prepare_mcp_secrets(
    bus: &BusClient,
    agent_id: &str,
    mcp_servers: &[String],
) -> Result<(), String> {
    if mcp_servers.is_empty() {
        return Ok(());
    }
    let cfg = load_servers_config(std::path::Path::new("var/mcp/servers.yaml"));
    let mut needed: Vec<String> = Vec::new();
    for name in mcp_servers {
        if let Some(server) = cfg.servers.get(name) {
            for v in server.env.values() {
                if let Some(sec) = v
                    .trim()
                    .strip_prefix("${secret:")
                    .and_then(|s| s.strip_suffix('}'))
                {
                    if !needed.iter().any(|n| n == sec) {
                        needed.push(sec.to_string());
                    }
                }
            }
        }
    }
    let mut secrets = HashMap::new();
    for name in needed {
        match bus
            .call::<SecretGetRequest, String>(
                "secrets.get",
                &SecretGetRequest {
                    name: name.clone(),
                    actor: String::new(),
                },
                vec![],
            )
            .await
        {
            Ok(v) => {
                secrets.insert(name, v);
            }
            Err(e) => return Err(format!("{name}: {e}")),
        }
    }
    // Validate placeholders resolve
    for name in mcp_servers {
        if let Some(server) = cfg.servers.get(name) {
            for v in server.env.values() {
                let _ = resolve_secret_placeholder(v, &secrets)?;
            }
        }
    }
    let path = mcp_secrets_path(agent_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(&secrets).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

async fn spawn_worker(
    shared: &Shared,
    bus_addr: &str,
    agent_id: &str,
    spec: &AgentSpec,
    restore: bool,
    bus: Option<Arc<BusClient>>,
) -> Result<u32, String> {
    let spec_path = persist::write_spec(spec).map_err(|e| e.to_string())?;
    registry_add(agent_id);

    // Pré-résout les secrets MCP (agentd = service ; le worker est agent:*).
    if let Some(bus) = bus.as_ref() {
        if let Err(e) = prepare_mcp_secrets(bus, agent_id, &spec.mcp_servers).await {
            eprintln!("[aos-agentd] mcp secrets {agent_id}: {e}");
        }
    }

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
        let n = persist::seq_from_id(agent_id).max(1);
        let holder = HolderId(n);
        for uri in &spec.caps {
            rt.caps
                .mint(holder, uri.clone(), aos_caps::Rights::all(), None, None, 0);
        }
        let info = AgentInfo {
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
                    title: persist::read_info(agent_id)
                        .map(|i| i.title)
                        .filter(|t| !t.is_empty())
                        .unwrap_or_else(|| {
                            spec.display_name
                                .clone()
                                .filter(|t| !t.trim().is_empty())
                                .unwrap_or_else(|| persist::agent_title(&spec.goal.statement))
                        }),
                    kind: spec.kind,
                    display_name: spec.display_name.clone(),
                    persona_id: spec.persona_id.clone(),
                    origin: spec.origin.clone(),
                    deep_plan: None,
                    cognitive_mode: spec.cognitive_mode,
                };
                let mut entry = AgentEntry::new(info, restored_trace);
                if let Some(prev) = rt.agents.remove(agent_id) {
                    entry.subscribers = prev.subscribers;
                }
                rt.agents.insert(agent_id.to_string(), entry);
        persist::update_info_sidecar(&rt.agents[agent_id].info);
    }

    if let Some(bus) = bus.as_ref() {
        publish_caps_to_capkd(bus, agent_id, &spec.caps).await;
    }

    let bus_for_wait = bus.clone();
    let shared2 = shared.clone();
    let agent_id2 = agent_id.to_string();
    let waited_pid = pid;
    let mut child = child;
    tokio::spawn(async move {
        let _ = child.wait().await;
        let mut unexpected_parent: Option<(String, String, bool)> = None;
        let mut release_schedule = true;
        {
            let mut rt = shared2.lock().await;
            if let Some(entry) = rt.agents.get_mut(&agent_id2) {
                if entry.info.pid.is_some() && entry.info.pid != waited_pid {
                    // A newer worker owns this id (health restart / start).
                    release_schedule = false;
                } else {
                    let ours = entry.info.pid == waited_pid;
                    if ours {
                        entry.info.pid = None;
                    }
                    if ours
                        && !matches!(
                            entry.info.state,
                            AgentState::Killed | AgentState::Done | AgentState::Failed
                        )
                    {
                        let parent_id = entry.info.parent_id.clone();
                        let (state, result, ok) =
                            unexpected_child_exit_terminal(&entry.info.last_output);
                        entry.info.state = state.clone();
                        if !ok {
                            entry.info.fail_reason = Some(result.clone());
                        }
                        let ev = AgentOutputEvent::StateChanged { state };
                        broadcast(entry, &ev).await;
                        if let Some(parent_id) = parent_id {
                            unexpected_parent = Some((parent_id, result, ok));
                        }
                    }
                    if ours {
                        persist::update_info_sidecar(&entry.info);
                    }
                    if !ours
                        && matches!(
                            entry.info.state,
                            AgentState::Created
                                | AgentState::Running
                                | AgentState::Paused
                                | AgentState::Blocked
                        )
                    {
                        // Health restart already took the pid; do not free the schedule.
                        release_schedule = false;
                    }
                }
            }
        }
        if let (Some(bus), Some((parent_id, result, ok))) = (bus_for_wait, unexpected_parent) {
            notify_parent_child_terminal(
                &shared2,
                &bus,
                &parent_id,
                &agent_id2,
                &result,
                ok,
            )
            .await;
        }
        if release_schedule {
            if let Err(e) = schedule::release_agent(&agent_id2) {
                eprintln!("[aos-agentd] schedule release {agent_id2}: {e}");
            }
        }
    });

    Ok(pid.unwrap_or(0))
}

async fn cancel_room_round(shared: &Shared, bus: &BusClient, session_id: &str) {
    let prev = {
        let mut rt = shared.lock().await;
        rt.room_rounds.remove(session_id)
    };
    if let Some(prev) = prev {
        prev.cancel();
        if let Some(id) = *prev.current_inference.lock().await {
            let _ = bus
                .call::<CancelRequest, bool>(
                    "model.cancel",
                    &CancelRequest { inference_id: id },
                    vec![],
                )
                .await;
        }
    }
}

async fn begin_room_round(shared: &Shared, session_id: &str) -> Arc<RoomRoundState> {
    let round = Arc::new(RoomRoundState::new());
    let mut rt = shared.lock().await;
    rt.room_rounds
        .insert(session_id.to_string(), round.clone());
    round
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
        room_rounds: HashMap::new(),
        deep_thinking: DeepThinkingEngine::open(persist::agents_root()),
    }));
    {
        let mut rt = shared.lock().await;
        hydrate_persisted_agents(&mut rt);
    }
    ensure_builtin_persona_agents(&shared).await;

    let mut svc = BusService::new("agentd");

    // --- agent.create ---
    {
        let shared = shared.clone();
        let bus_addr2 = bus_addr.clone();
        let bus2 = bus.clone();
        svc.on(intents::CREATE, move |ctx| {
            let shared = shared.clone();
            let bus_addr = bus_addr2.clone();
            let bus = bus2.clone();
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
                let agent_id = if let Some(pid) = req.persona_id.as_deref() {
                    persona_agent_id(pid)
                } else {
                    persist::alloc_agent_id()
                };
                let mut spec = build_spec(&agent_id, &req);
                // Inherit chat session model when caller left model_id empty so we do
                // not fall through to modeld's default_chat (may differ from session).
                if spec
                    .model_id
                    .as_ref()
                    .map(|s| s.trim().is_empty())
                    .unwrap_or(true)
                {
                    if let Some(sid) = spec
                        .session_id
                        .as_ref()
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                    {
                        if let Ok(resp) = bus
                            .call::<ChatSessionIdRequest, ChatSessionGetResponse>(
                                "chat.session.get",
                                &ChatSessionIdRequest {
                                    session_id: sid.to_string(),
                                },
                                vec![],
                            )
                            .await
                        {
                            if let Some(mid) = resp
                                .meta
                                .model_id
                                .filter(|s| !s.trim().is_empty())
                            {
                                spec.model_id = Some(mid);
                            }
                        }
                    }
                }
                if !req.spawns_worker() {
                    if persist::read_spec(&agent_id).is_some() {
                        let mut rt = shared.lock().await;
                        if !rt.agents.contains_key(&agent_id) {
                            let info = roster_info_from_spec(&spec);
                            rt.agents.insert(
                                agent_id.clone(),
                                AgentEntry::new(info, Vec::new()),
                            );
                        }
                        let _ = ctx
                            .respond(
                                aos_ipc::msg::Status::Ok,
                                &AgentCreateResponse { agent_id },
                            )
                            .await;
                        return;
                    }
                    match register_roster_agent(&shared, &agent_id, &spec).await {
                        Ok(()) => {
                            eprintln!("[aos-agentd] {agent_id} roster enregistré");
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
                    return;
                }
                match spawn_worker(&shared, &bus_addr, &agent_id, &spec, false, Some(bus.clone())).await {
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

    // --- agent.spec.get ---
    {
        svc.on(intents::SPEC_GET, move |ctx| async move {
            let req: AgentIdRequest = match ctx.payload() {
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
            let _ = ctx
                .respond(aos_ipc::msg::Status::Ok, &AgentSpecResponse { spec })
                .await;
        });
    }

    // --- agent.roster.update ---
    {
        let shared = shared.clone();
        svc.on(intents::ROSTER_UPDATE, move |ctx| {
            let shared = shared.clone();
            async move {
                let req: AgentRosterUpdateRequest = match ctx.payload() {
                    Ok(r) => r,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                let Some(mut spec) = persist::read_spec(&req.agent_id) else {
                    let _ = ctx
                        .respond_error(aos_ipc::msg::Status::NotFound, "spec introuvable")
                        .await;
                    return;
                };
                if spec.kind != AgentKind::Roster {
                    let _ = ctx
                        .respond_error(
                            aos_ipc::msg::Status::BadRequest,
                            "seules les entrées roster sont modifiables ici",
                        )
                        .await;
                    return;
                }
                update_roster_from_request(&mut spec, &req);
                if let Err(e) = persist::write_spec(&spec) {
                    let _ = ctx
                        .respond_error(aos_ipc::msg::Status::InternalError, &e.to_string())
                        .await;
                    return;
                }
                let info = roster_info_from_spec(&spec);
                {
                    let mut rt = shared.lock().await;
                    if let Some(entry) = rt.agents.get_mut(&req.agent_id) {
                        entry.info = info.clone();
                    } else {
                        rt.agents.insert(
                            req.agent_id.clone(),
                            AgentEntry::new(info.clone(), Vec::new()),
                        );
                    }
                    persist::update_info_sidecar(&info);
                }
                let _ = ctx
                    .respond(aos_ipc::msg::Status::Ok, &AgentSpecResponse { spec })
                    .await;
            }
        });
    }

    // --- agent.start (restore) ---
    {
        let shared = shared.clone();
        let bus2 = bus.clone();
        let bus_addr2 = bus_addr.clone();
        svc.on(intents::START, move |ctx| {
            let shared = shared.clone();
            let bus = bus2.clone();
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
                match spawn_worker(&shared, &bus_addr, &req.agent_id, &spec, true, Some(bus.clone())).await {
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
                        respond_control(ctx, r).await;
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
                        respond_control(ctx, r).await;
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
                                respond_control(ctx, r).await;
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
                                if aos_agent::context_budget::is_overflow_fail_reason(&reason) {
                                    if let Some(mut st) = persist::read_state(&req.agent_id) {
                                        let _ = aos_agent::context_budget::aggressive_trim_for_overflow(
                                            &mut st.working_memory,
                                            aos_agent::context_budget::DEFAULT_N_CTX_HINT,
                                            aos_agent::context_budget::AGENT_GEN_TOKENS,
                                        );
                                        let _ = persist::write_state(&st);
                                    }
                                } else if reason.contains("max_steps") {
                                    spec.goal.max_steps = spec.goal.max_steps.saturating_add(8);
                                    if let Some(ms) = spec.budget.max_steps.as_mut() {
                                        *ms = ms.saturating_add(8);
                                    }
                                    let _ = persist::write_spec(&spec);
                                }
                                if let Some(mut st) = persist::read_state(&req.agent_id) {
                                    if aos_agent::context_budget::is_overflow_fail_reason(&reason) {
                                        let _ = persist::write_state(&st);
                                    } else {
                                        st.push_user(&format!(
                                            "[retry] dernière action `{last_action}` a échoué : {reason}. Réessaie autrement."
                                        ));
                                        let _ = persist::write_state(&st);
                                    }
                                }
                                match spawn_worker(
                                    &shared,
                                    &bus_addr,
                                    &req.agent_id,
                                    &spec,
                                    true,
                                    Some(bus.clone()),
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
                        respond_control(ctx, r).await;
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
                        let mut deep_trace_chat: Option<(String, String)> = None;
                        let mut parent_notify: Option<(String, String, String, bool)> = None;
                        {
                            let mut rt = shared.lock().await;
                            if let AgentOutputEvent::ChildDone { child_id, result } = &rep.event {
                                if let Ok(Some((plan, _))) = rt
                                    .deep_thinking
                                    .complete_delegated_child(&rep.agent_id, child_id, result)
                                {
                                    if let Some(entry) = rt.agents.get_mut(&rep.agent_id) {
                                        entry.info.deep_plan = Some(plan);
                                    }
                                }
                            }
                            if let Some(entry) = rt.agents.get_mut(&rep.agent_id) {
                                note_agent_activity(entry, &rep.event);
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
                                        let parent_id = entry.info.parent_id.clone();
                                        let result_text = if !entry.info.last_output.trim().is_empty()
                                        {
                                            entry.info.last_output.clone()
                                        } else {
                                            entry.info.fail_reason.clone().unwrap_or_else(|| {
                                                format!("{state:?}")
                                            })
                                        };
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
                                            if let Some(parent_id) = parent_id {
                                                parent_notify = Some((
                                                    parent_id,
                                                    entry.info.agent_id.clone(),
                                                    result_text.chars().take(4000).collect(),
                                                    matches!(state, AgentState::Done),
                                                ));
                                            }
                                            if let Some(sid) = entry.info.session_id.clone() {
                                                let summary = match state {
                                                    AgentState::Done => agent_done_chat_message(
                                                        &entry.info.agent_id,
                                                        &entry.info,
                                                        &entry.info.last_output,
                                                    ),
                                                    AgentState::Failed => {
                                                        if entry.info.fail_reason.as_deref()
                                                            == Some(
                                                                aos_agent::actions::THREAD_FAIL_COULD_NOT_ACT,
                                                            )
                                                        {
                                                            aos_agent::actions::THREAD_FAIL_COULD_NOT_ACT
                                                                .to_string()
                                                        } else if entry.info.fail_reason.as_deref()
                                                            == Some(
                                                                aos_agent::actions::THREAD_FAIL_COULD_NOT_CONTINUE,
                                                            )
                                                        {
                                                            aos_agent::actions::THREAD_FAIL_COULD_NOT_CONTINUE
                                                                .to_string()
                                                        } else {
                                                            let reason = entry
                                                                .info
                                                                .fail_reason
                                                                .clone()
                                                                .unwrap_or_else(|| "échec".into());
                                                            format!(
                                                                "Agent « {} » a échoué : {}",
                                                                entry.info.display_title(),
                                                                reason
                                                            )
                                                        }
                                                    }
                                                    AgentState::Killed => format!(
                                                        "Agent « {} » arrêté.",
                                                        entry.info.display_title()
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
                                        if aos_agent::context_budget::is_technical_vision_infer_error(
                                            message,
                                        ) {
                                            eprintln!(
                                                "agent vision refs ignorées (pas de mmproj) : {message}"
                                            );
                                        } else {
                                            let reason =
                                                if aos_agent::context_budget::is_technical_prompt_overflow_message(
                                                    message,
                                                ) {
                                                    eprintln!(
                                                        "agent prompt overflow (journal) : {message}"
                                                    );
                                                    aos_agent::actions::THREAD_FAIL_COULD_NOT_CONTINUE
                                                        .to_string()
                                                } else {
                                                    message.clone()
                                                };
                                            entry.info.fail_reason = Some(reason);
                                        }
                                    }
                                    AgentOutputEvent::Log { line } => {
                                        if let Some(rest) = line.strip_prefix("goal.complete : ")
                                        {
                                            let rest = rest.trim();
                                            if !rest.is_empty() {
                                                entry.info.last_output = rest.to_string();
                                            }
                                        } else if let Some(rest) =
                                            line.strip_prefix("user.ask : ")
                                        {
                                            let rest = rest.trim();
                                            if !rest.is_empty() {
                                                entry.info.last_output = rest.to_string();
                                                entry.info.fail_reason = None;
                                            }
                                        } else if let Some(rest) =
                                            line.strip_prefix("user.ask.timeout : ")
                                        {
                                            let rest = rest.trim();
                                            entry.info.last_output = format!(
                                                "Question expirée ({rest})"
                                            );
                                            entry.info.fail_reason = None;
                                        }
                                    }
                                    AgentOutputEvent::Progress {
                                        step,
                                        max_steps,
                                        current_task,
                                    } => {
                                        entry.info.step = *step;
                                        entry.info.max_steps = *max_steps;
                                        entry.info.current_task = current_task.clone();
                                        if entry.info.state != AgentState::Blocked {
                                            entry.info.last_output.clear();
                                        }
                                    }
                                    AgentOutputEvent::ChildSpawned { child_id, .. } => {
                                        if !entry.info.children.contains(child_id) {
                                            entry.info.children.push(child_id.clone());
                                        }
                                    }
                                    AgentOutputEvent::DeepPlanUpdated { plan } => {
                                        entry.info.deep_plan = Some(plan.clone());
                                        if let Some(cur) = plan
                                            .steps
                                            .iter()
                                            .find(|s| {
                                                matches!(
                                                    s.status,
                                                    aos_proto::PlanStepStatus::InProgress
                                                        | aos_proto::PlanStepStatus::Delegated
                                                )
                                            })
                                            .map(|s| s.label.clone())
                                        {
                                            entry.info.current_task = Some(cur);
                                        }
                                    }
                                    AgentOutputEvent::DeepTrace { message } => {
                                        if entry.info.last_output.is_empty() {
                                            entry.info.last_output = message.clone();
                                        }
                                        if let Some(sid) = entry.info.session_id.clone() {
                                            deep_trace_chat = Some((sid, message.clone()));
                                        }
                                    }
                                    AgentOutputEvent::Step(rec) => {
                                        if rec.action == "goal.complete"
                                            && !rec.tool_result.trim().is_empty()
                                        {
                                            entry.info.last_output = rec.tool_result.clone();
                                        } else if rec.action == "goal.fail"
                                            && !rec.tool_result.trim().is_empty()
                                        {
                                            entry.info.last_output = rec.tool_result.clone();
                                            if entry.info.fail_reason.is_none() {
                                                entry.info.fail_reason =
                                                    Some(rec.tool_result.clone());
                                            }
                                        } else if rec.action == "user.ask"
                                            && !rec.tool_result.trim().is_empty()
                                        {
                                            // garder la question visible pendant l'attente
                                            if entry.info.last_output.is_empty() {
                                                entry.info.last_output = rec.tool_result.clone();
                                            }
                                        } else {
                                            entry.info.last_output.clear();
                                        }
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
                                    }
                                    _ => {}
                                }
                                persist::update_info_sidecar(&entry.info);
                                broadcast(entry, &rep.event).await;
                            }
                        }
                        if let Some((parent_id, child_id, result, ok)) = parent_notify {
                            notify_parent_child_terminal(
                                &shared,
                                &bus,
                                &parent_id,
                                &child_id,
                                &result,
                                ok,
                            )
                            .await;
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
                                        speaker_id: None,
                                        speaker_name: None,
                                        thinking: None,
                                    },
                                    vec![],
                                )
                                .await;
                        }
                        if let Some((session_id, message)) = deep_trace_chat {
                            let mut attachments = Vec::new();
                            {
                                let rt = shared.lock().await;
                                if let Some(entry) = rt.agents.get(&rep.agent_id) {
                                    if let Some(plan) = entry.info.deep_plan.as_ref() {
                                        attachments.push(ChatAttachment::DeepPlan {
                                            agent_id: entry.info.agent_id.clone(),
                                            plan_id: plan.id.clone(),
                                            title: plan.title.clone(),
                                            version: plan.version,
                                            steps: plan.steps.clone(),
                                            expand_step_ids: vec![],
                                            show_logs_step_id: None,
                                        });
                                    }
                                }
                            }
                            let _ = bus
                                .call::<ChatSessionAppendRequest, aos_proto::ChatSessionMessage>(
                                    "chat.session.append",
                                    &ChatSessionAppendRequest {
                                        session_id,
                                        role: "system".into(),
                                        content: format!("🧠 {message}"),
                                        attachments,
                                        speaker_id: None,
                                        speaker_name: None,
                                        thinking: None,
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
                        publish_caps_to_capkd(&bus, &req.agent_id, std::slice::from_ref(&req.cap)).await;
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

    // --- agent.caps.sync — publie les caps logiques dans aos-capkd ---
    {
        let shared2 = shared.clone();
        let bus2 = bus.clone();
        svc.on(intents::CAPS_SYNC, move |ctx| {
            let shared = shared2.clone();
            let bus = bus2.clone();
            async move {
                match ctx.payload::<AgentIdRequest>() {
                    Ok(req) => {
                        let caps = {
                            let rt = shared.lock().await;
                            if let Some(entry) = rt.agents.get(&req.agent_id) {
                                entry.info.caps.clone()
                            } else {
                                drop(rt);
                                persist::read_spec(&req.agent_id)
                                    .map(|s| s.caps)
                                    .unwrap_or_default()
                            }
                        };
                        publish_caps_to_capkd(&bus, &req.agent_id, &caps).await;
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
                    images: vec![],
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

    // --- agent.room_turn (one-shot membre salon) ---
    {
        let shared = shared.clone();
        let bus2 = bus.clone();
        svc.on(intents::ROOM_TURN, move |ctx| {
            let shared = shared.clone();
            let bus = bus2.clone();
            async move {
                let req: AgentRoomTurnRequest = match ctx.payload() {
                    Ok(r) => r,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                let round = {
                    let rt = shared.lock().await;
                    rt.room_rounds
                        .get(&req.session_id)
                        .cloned()
                        .unwrap_or_else(|| Arc::new(RoomRoundState::new()))
                };
                match room_runtime::execute_room_turn(&bus, round.as_ref(), &req).await {
                    Ok(resp) => {
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
                    }
                    Err(e) => {
                        let status = if e.contains("introuvable") || e.contains("absent") {
                            aos_ipc::msg::Status::NotFound
                        } else if e.contains("mode salon") {
                            aos_ipc::msg::Status::BadRequest
                        } else if e == "tour annulé" {
                            aos_ipc::msg::Status::Cancelled
                        } else {
                            aos_ipc::msg::Status::InternalError
                        };
                        let _ = ctx.respond_error(status, &e).await;
                    }
                }
            }
        });
    }

    // --- agent.room_conduct (conducteur salon) ---
    {
        let shared = shared.clone();
        let bus2 = bus.clone();
        svc.on(intents::ROOM_CONDUCT, move |ctx| {
            let shared = shared.clone();
            let bus = bus2.clone();
            async move {
                let req: AgentRoomConductRequest = match ctx.payload() {
                    Ok(r) => r,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                cancel_room_round(&shared, &bus, &req.session_id).await;
                let round = begin_room_round(&shared, &req.session_id).await;
                match room_runtime::execute_room_conduct(&bus, round, &req).await {
                    Ok(resp) => {
                        let mut rt = shared.lock().await;
                        rt.room_rounds.remove(&req.session_id);
                        drop(rt);
                        let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
                    }
                    Err(e) => {
                        let mut rt = shared.lock().await;
                        rt.room_rounds.remove(&req.session_id);
                        drop(rt);
                        let status = if e.contains("mode salon") || e.contains("sans membres") {
                            aos_ipc::msg::Status::BadRequest
                        } else {
                            aos_ipc::msg::Status::InternalError
                        };
                        let _ = ctx.respond_error(status, &e).await;
                    }
                }
            }
        });
    }

    // --- agent.room_conduct.cancel ---
    {
        let shared = shared.clone();
        let bus2 = bus.clone();
        svc.on(intents::ROOM_CONDUCT_CANCEL, move |ctx| {
            let shared = shared.clone();
            let bus = bus2.clone();
            async move {
                let req: ChatSessionRoomTurnCancelRequest = match ctx.payload() {
                    Ok(r) => r,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                cancel_room_round(&shared, &bus, &req.session_id).await;
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &true).await;
            }
        });
    }

    eprintln!("[aos-agentd] prêt (agentic)");

    // Schedule ticker (E2 / P03.4).
    {
        let shared_tick = shared.clone();
        let bus_addr_tick = bus_addr.clone();
        let bus_tick = bus.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
            loop {
                interval.tick().await;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                let running: Vec<String> = {
                    let rt = shared_tick.lock().await;
                    rt.agents
                        .iter()
                        .filter(|(_, e)| {
                            e.info.pid.is_some()
                                && !matches!(
                                    e.info.state,
                                    AgentState::Done | AgentState::Failed | AgentState::Killed
                                )
                        })
                        .map(|(id, _)| id.clone())
                        .collect()
                };
                let due = match schedule::due(now, &running) {
                    Ok(d) => d,
                    Err(e) => {
                        eprintln!("[aos-agentd] schedule.due: {e}");
                        continue;
                    }
                };
                for entry in due {
                    eprintln!(
                        "[aos-agentd] schedule fire {} — {}",
                        entry.id, entry.goal
                    );
                    let mut req = AgentCreateRequest::simple(&entry.goal);
                    req.model_id = entry.model_id.clone();
                    req.tools = default_agent_tools();
                    let agent_id = persist::alloc_agent_id();
                    let spec = build_spec(&agent_id, &req);
                    match spawn_worker(
                        &shared_tick,
                        &bus_addr_tick,
                        &agent_id,
                        &spec,
                        false,
                        Some(bus_tick.clone()),
                    )
                    .await
                    {
                        Ok(pid) => {
                            if let Err(e) = schedule::mark_fired(&entry.id, now, &agent_id) {
                                eprintln!(
                                    "[aos-agentd] schedule mark {} → {agent_id}: {e}",
                                    entry.id
                                );
                            }
                            eprintln!(
                                "[aos-agentd] schedule {} → {agent_id} (pid {pid})",
                                entry.id
                            );
                        }
                        Err(e) => {
                            eprintln!("[aos-agentd] schedule spawn {agent_id}: {e}");
                        }
                    }
                }
            }
        });
    }

    // Health-check: stalled Running / Blocked-without-wait / dead workers.
    {
        let shared_h = shared.clone();
        let bus_h = bus.clone();
        let bus_addr_h = bus_addr.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(20));
            loop {
                interval.tick().await;
                run_health_pass(&shared_h, &bus_h, &bus_addr_h).await;
            }
        });
    }

    // --- Deep Thinking plan.* ---
    {
        let shared2 = shared.clone();
        svc.on(intents::PLAN_CREATE, move |ctx| {
            let shared = shared2.clone();
            async move {
                let req: PlanCreateRequest = match ctx.payload() {
                    Ok(r) => r,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                let caps = agent_caps_snapshot(&shared, &req.agent_id).await;
                let result = {
                    let rt = shared.lock().await;
                    rt.deep_thinking.create(req, &caps)
                };
                respond_plan(ctx, &shared, result).await;
            }
        });
    }
    {
        let shared2 = shared.clone();
        svc.on(intents::PLAN_GET, move |ctx| {
            let shared = shared2.clone();
            async move {
                let req: PlanGetRequest = match ctx.payload() {
                    Ok(r) => r,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                let caps = if let Some(aid) = req.agent_id.as_deref().filter(|s| !s.is_empty()) {
                    agent_caps_snapshot(&shared, aid).await
                } else if let Some(pid) = req.plan_id.as_deref() {
                    caps_for_plan_id(&shared, pid).await
                } else {
                    Vec::new()
                };
                let result = {
                    let rt = shared.lock().await;
                    rt.deep_thinking
                        .get(req.plan_id.as_deref(), req.agent_id.as_deref(), &caps)
                        .map(|plan| (plan, String::new()))
                };
                respond_plan(ctx, &shared, result).await;
            }
        });
    }
    {
        let shared2 = shared.clone();
        svc.on(intents::PLAN_UPDATE_STEP, move |ctx| {
            let shared = shared2.clone();
            async move {
                let req: PlanUpdateStepRequest = match ctx.payload() {
                    Ok(r) => r,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                let caps = caps_for_plan_id(&shared, &req.plan_id).await;
                let result = {
                    let rt = shared.lock().await;
                    rt.deep_thinking.update_step(req, &caps)
                };
                respond_plan(ctx, &shared, result).await;
            }
        });
    }
    {
        let shared2 = shared.clone();
        svc.on(intents::PLAN_REPLACE_TREE, move |ctx| {
            let shared = shared2.clone();
            async move {
                let req: PlanReplaceTreeRequest = match ctx.payload() {
                    Ok(r) => r,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                let caps = caps_for_plan_id(&shared, &req.plan_id).await;
                let result = {
                    let rt = shared.lock().await;
                    rt.deep_thinking.replace_tree(req, &caps)
                };
                respond_plan(ctx, &shared, result).await;
            }
        });
    }
    {
        let shared2 = shared.clone();
        svc.on(intents::PLAN_DELEGATE_STEP, move |ctx| {
            let shared = shared2.clone();
            async move {
                let req: PlanDelegateStepRequest = match ctx.payload() {
                    Ok(r) => r,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                let caps = caps_for_plan_id(&shared, &req.plan_id).await;
                let result = {
                    let rt = shared.lock().await;
                    rt.deep_thinking.delegate_step(req, &caps)
                };
                respond_plan(ctx, &shared, result).await;
            }
        });
    }
    {
        let shared2 = shared.clone();
        svc.on(intents::PLAN_APPEND_LOG, move |ctx| {
            let shared = shared2.clone();
            async move {
                let req: PlanAppendLogRequest = match ctx.payload() {
                    Ok(r) => r,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                let caps = caps_for_plan_id(&shared, &req.plan_id).await;
                let result = {
                    let rt = shared.lock().await;
                    rt.deep_thinking.append_log(req, &caps)
                };
                respond_plan(ctx, &shared, result).await;
            }
        });
    }

    // --- schedule.create / list / cancel ---
    {
        svc.on(intents::SCHEDULE_CREATE, move |ctx| async move {
            let req: ScheduleCreateRequest = match ctx.payload() {
                Ok(r) => r,
                Err(_) => {
                    let _ = ctx
                        .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                        .await;
                    return;
                }
            };
            match schedule::create(&req) {
                Ok(e) => {
                    let _ = ctx.respond(aos_ipc::msg::Status::Ok, &e).await;
                }
                Err(err) => {
                    let _ = ctx
                        .respond_error(aos_ipc::msg::Status::InternalError, &err)
                        .await;
                }
            }
        });
    }
    {
        svc.on(intents::SCHEDULE_LIST, move |ctx| async move {
            match schedule::list() {
                Ok(schedules) => {
                    let _ = ctx
                        .respond(
                            aos_ipc::msg::Status::Ok,
                            &ScheduleListResponse { schedules },
                        )
                        .await;
                }
                Err(err) => {
                    let _ = ctx
                        .respond_error(aos_ipc::msg::Status::InternalError, &err)
                        .await;
                }
            }
        });
    }
    {
        svc.on(intents::SCHEDULE_CANCEL, move |ctx| async move {
            let req: ScheduleIdRequest = match ctx.payload() {
                Ok(r) => r,
                Err(_) => {
                    let _ = ctx
                        .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                        .await;
                    return;
                }
            };
            match schedule::cancel(&req.id) {
                Ok(e) => {
                    let _ = ctx.respond(aos_ipc::msg::Status::Ok, &e).await;
                }
                Err(err) => {
                    let _ = ctx
                        .respond_error(aos_ipc::msg::Status::InternalError, &err)
                        .await;
                }
            }
        });
    }
    {
        svc.on(intents::SCHEDULE_PAUSE, move |ctx| async move {
            let req: ScheduleIdRequest = match ctx.payload() {
                Ok(r) => r,
                Err(_) => {
                    let _ = ctx
                        .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                        .await;
                    return;
                }
            };
            match schedule::pause(&req.id) {
                Ok(e) => {
                    let _ = ctx.respond(aos_ipc::msg::Status::Ok, &e).await;
                }
                Err(err) => {
                    let _ = ctx
                        .respond_error(aos_ipc::msg::Status::InternalError, &err)
                        .await;
                }
            }
        });
    }
    {
        svc.on(intents::SCHEDULE_RESUME, move |ctx| async move {
            let req: ScheduleIdRequest = match ctx.payload() {
                Ok(r) => r,
                Err(_) => {
                    let _ = ctx
                        .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                        .await;
                    return;
                }
            };
            match schedule::resume(&req.id) {
                Ok(e) => {
                    let _ = ctx.respond(aos_ipc::msg::Status::Ok, &e).await;
                }
                Err(err) => {
                    let _ = ctx
                        .respond_error(aos_ipc::msg::Status::InternalError, &err)
                        .await;
                }
            }
        });
    }

    let _ = svc.serve(&bus_addr).await;
}

fn agent_caps_blocking(rt: &Runtime, agent_id: &str) -> Vec<String> {
    rt.agents
        .get(agent_id)
        .map(|e| e.info.caps.clone())
        .or_else(|| persist::read_spec(agent_id).map(|s| s.caps))
        .unwrap_or_default()
}

async fn agent_caps_snapshot(shared: &Shared, agent_id: &str) -> Vec<String> {
    let rt = shared.lock().await;
    agent_caps_blocking(&rt, agent_id)
}

async fn caps_for_plan_id(shared: &Shared, plan_id: &str) -> Vec<String> {
    let rt = shared.lock().await;
    match rt.deep_thinking.get(Some(plan_id), None, &["plan.read:*".into()]) {
        Ok(plan) => agent_caps_blocking(&rt, &plan.agent_id),
        Err(_) => Vec::new(),
    }
}

async fn respond_plan(
    ctx: aos_ipc::IntentCtx,
    shared: &Shared,
    result: Result<(aos_proto::DeepPlan, String), deep_thinking::EngineError>,
) {
    match result {
        Ok((plan, trace)) => {
            {
                let mut rt = shared.lock().await;
                if let Some(entry) = rt.agents.get_mut(&plan.agent_id) {
                    entry.info.deep_plan = Some(plan.clone());
                    entry.info.cognitive_mode = CognitiveMode::DeepThinking;
                }
            }
            let _ = ctx
                .respond(aos_ipc::msg::Status::Ok, &PlanResponse { plan })
                .await;
            let _ = trace;
        }
        Err(e) => {
            let _ = ctx
                .respond_error(aos_ipc::msg::Status::InternalError, &e.to_string())
                .await;
        }
    }
}

fn hydrate_persisted_agents(rt: &mut Runtime) {
    for id in persist::list_agent_ids() {
        if rt.agents.contains_key(&id) {
            continue;
        }
        let Some(mut info) = persist::read_info(&id).or_else(|| persist::info_from_spec(&id))
        else {
            continue;
        };
        info.pid = None;
        let mut dirty = false;
        if info.title.is_empty() {
            info.title = persist::agent_title(&info.directive);
            dirty = true;
        }
        let was_live = matches!(
            info.state,
            AgentState::Running
                | AgentState::Created
                | AgentState::Paused
                | AgentState::Blocked
        );
        if info.state == AgentState::Roster || info.kind == AgentKind::Roster {
            // Roster specs stay idle across restarts.
        } else if was_live {
            info.state = AgentState::Killed;
            if info.fail_reason.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
                info.fail_reason = Some("arrêté au redémarrage".into());
            }
            dirty = true;
        }
        if dirty {
            persist::update_info_sidecar(&info);
        }
        let trace = persist::read_state(&id)
            .map(|s| s.trace)
            .unwrap_or_default();
        rt.agents.insert(id, AgentEntry::new(info, trace));
    }
}

fn agent_done_chat_message(_agent_id: &str, info: &AgentInfo, last_output: &str) -> String {
    let out = last_output.trim();
    let title = info.display_title();
    if out.is_empty() {
        format!("Agent « {title} » terminé.")
    } else {
        let body: String = out.chars().take(8000).collect();
        format!("**Résultat — {title}**\n\n{body}")
    }
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

#[cfg(test)]
mod tests {
    use super::unexpected_child_exit_terminal;
    use aos_proto::AgentState;

    #[test]
    fn unexpected_child_exit_is_a_failure_not_success() {
        let (state, result, ok) = unexpected_child_exit_terminal("");
        assert_eq!(state, AgentState::Failed);
        assert!(!ok);
        assert!(result.contains("sans rapport de fin"));

        let (state, result, ok) = unexpected_child_exit_terminal("partial output");
        assert_eq!(state, AgentState::Failed);
        assert!(!ok);
        assert_eq!(result, "partial output");
    }
}
