//! `aos-agent-worker` — boucle agentic Observe / Think / Act / Reflect / Checkpoint.
//!
//! Usage : `aos-agent-worker --agent-id <id> --bus <addr> --spec-path <path>
//!          [--restore]`

use aos_agent::actions::{parse_action, AgentAction};
use aos_agent::mcp::{open_mcp_tools, McpSession};
use aos_agent::persist::{self, compact_working_memory};
use aos_agent::prompt::{compile_system_prompt, optimize_prompt_request, PromptCompileInput};
use aos_agent::skills::{load_skills, merge_skill_tools};
use aos_agent::tools::{
    caps_for_tools, caps_subset, select_tools, ToolBackend, ToolDesc,
};
use aos_agent::{intents, CognitiveState, ControlCmd, ControlResp, ReportPayload};
use aos_ipc::{BusClient, BusService};
use aos_proto::{
    AgentCreateRequest, AgentCreateResponse, AgentGoal, AgentInfo, AgentOutputEvent, AgentSpec,
    AgentState, CancelRequest, ChatMessage, DocumentRef, FilesGenerateRequest, FsListRequest,
    FsReadRequest, FsReadResponse, FsWriteRequest, InferParams, InferRequest, MemContextRequest,
    MemContextResponse, MemEpisodicQueryRequest, MemEpisodicWriteRequest, MemHit,
    MemSharedReadRequest, MemSharedWriteRequest, ModuleInvokeRequest, ModuleInvokeResponse,
    NetFetchRequest, TaskNode, TaskNodeStatus, TokenEvent, WebSearchRequest, WebSearchResponse,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex};

enum WorkerCmd {
    Resume,
    Steer(String),
}

struct Shared {
    state: Mutex<CognitiveState>,
    paused: AtomicBool,
    current_inference: Mutex<Option<u64>>,
    cmd_tx: mpsc::Sender<WorkerCmd>,
}

fn parse_args() -> (String, String, PathBuf, bool) {
    let mut agent_id = None;
    let mut bus = None;
    let mut spec_path = None;
    let mut restore = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--agent-id" => agent_id = args.next(),
            "--bus" => bus = args.next(),
            "--spec-path" => spec_path = args.next().map(PathBuf::from),
            "--restore" => restore = true,
            // Compat legacy CLI
            "--directive" | "--caps" | "--model" => {
                let _ = args.next();
            }
            _ => {}
        }
    }
    (
        agent_id.expect("--agent-id requis"),
        bus.expect("--bus requis"),
        spec_path.expect("--spec-path requis"),
        restore,
    )
}

async fn report(bus: &BusClient, agent_id: &str, event: AgentOutputEvent) {
    let _ = bus
        .call::<ReportPayload, bool>(
            intents::REPORT,
            &ReportPayload {
                agent_id: agent_id.to_string(),
                event,
            },
            vec![],
        )
        .await;
}

#[tokio::main]
async fn main() {
    let (agent_id, bus_addr, spec_path, restore) = parse_args();
    let mut spec: AgentSpec = serde_json::from_str(
        &std::fs::read_to_string(&spec_path).expect("lecture spec.json"),
    )
    .expect("parse spec.json");
    spec.agent_id = agent_id.clone();

    let bus = BusClient::connect(&bus_addr, format!("agent:{agent_id}"))
        .await
        .expect("connexion au bus");

    let (cmd_tx, mut cmd_rx) = mpsc::channel::<WorkerCmd>(16);

    let mut state = if restore {
        persist::read_state(&agent_id).unwrap_or_else(|| {
            CognitiveState::new(agent_id.clone(), spec.caps.clone())
        })
    } else {
        CognitiveState::new(agent_id.clone(), spec.caps.clone())
    };
    state.goal = Some(spec.goal.clone());
    state.parent_id = spec.parent_id.clone();
    state.cap_set_snapshot = spec.caps.clone();

    let shared = Arc::new(Shared {
        state: Mutex::new(state),
        paused: AtomicBool::new(false),
        current_inference: Mutex::new(None),
        cmd_tx: cmd_tx.clone(),
    });

    // Skills + tools + MCP + modules installés (catalogue dynamique)
    let skill_docs = load_skills(&spec.skills);
    let tool_ids = merge_skill_tools(&spec.tools, &skill_docs);
    let (mut mcp_sessions, mcp_tools) = open_mcp_tools(&spec.mcp_servers).await;
    let mut module_tools = discover_module_tools(&bus).await;
    module_tools.extend(mcp_tools);
    let tools = select_tools(&tool_ids, &module_tools);
    // Enrich caps from tools if create didn't set them all
    let derived = caps_for_tools(&tools, &spec.mcp_servers);
    for c in derived {
        if !spec.caps.contains(&c) {
            spec.caps.push(c);
        }
    }

    // Optional prompt optimize
    if spec.optimize_prompt && spec.system_prompt.is_none() {
        if let Ok(opt) = optimize_prompt_now(&bus, &spec).await {
            spec.system_prompt = Some(opt);
            let _ = persist::write_spec(&spec);
        }
    }

    let system = compile_system_prompt(&PromptCompileInput {
        spec: &spec,
        skills: &skill_docs,
        tools: &tools,
        doc_index: &spec.documents,
    });
    {
        let mut st = shared.state.lock().await;
        if st.working_memory.is_empty()
            || st.working_memory[0].0 != "system"
        {
            st.working_memory.insert(0, ("system".into(), system));
        } else {
            st.working_memory[0] = ("system".into(), system);
        }
    }

    // Index documents
    index_documents(&bus, &agent_id, &spec.caps, &spec.documents).await;

    // Control service
    {
        let mut svc = BusService::new(format!("agent-{agent_id}"));
        let control_intent = format!("agent.{agent_id}.control");
        let shared_c = shared.clone();
        let bus_c = bus.clone();
        svc.on(&control_intent, move |ctx| {
            let shared = shared_c.clone();
            let bus = bus_c.clone();
            async move {
                let cmd: ControlCmd = match ctx.payload() {
                    Ok(c) => c,
                    Err(_) => {
                        let _ = ctx
                            .respond(
                                aos_ipc::msg::Status::BadRequest,
                                &ControlResp::Error("payload invalide".into()),
                            )
                            .await;
                        return;
                    }
                };
                let resp = match cmd {
                    ControlCmd::Pause => {
                        shared.paused.store(true, Ordering::SeqCst);
                        if let Some(id) = shared.current_inference.lock().await.take() {
                            let _ = bus
                                .call::<CancelRequest, bool>(
                                    "model.cancel",
                                    &CancelRequest { inference_id: id },
                                    vec![],
                                )
                                .await;
                        }
                        ControlResp::Ack
                    }
                    ControlCmd::Resume => {
                        shared.paused.store(false, Ordering::SeqCst);
                        let _ = shared.cmd_tx.send(WorkerCmd::Resume).await;
                        ControlResp::Ack
                    }
                    ControlCmd::Steer { directive } => {
                        let _ = shared.cmd_tx.send(WorkerCmd::Steer(directive)).await;
                        ControlResp::Ack
                    }
                    ControlCmd::Snapshot => ControlResp::State(shared.state.lock().await.clone()),
                    ControlCmd::GrantCap { cap } => {
                        let mut st = shared.state.lock().await;
                        if !st.cap_set_snapshot.contains(&cap) {
                            st.cap_set_snapshot.push(cap.clone());
                        }
                        ControlResp::Ack
                    }
                };
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
            }
        });
        let svc_bus = bus_addr.clone();
        tokio::spawn(async move {
            let _ = svc.serve(&svc_bus).await;
        });
    }

    report(
        &bus,
        &agent_id,
        AgentOutputEvent::StateChanged {
            state: AgentState::Running,
        },
    )
    .await;

    let started = Instant::now();
    let max_steps = spec.goal.max_steps;
    let timeout = Duration::from_secs(spec.goal.timeout_secs);
    let data_refs: Vec<String> = spec.documents.iter().map(|d| d.path.clone()).collect();

    // Seed first user turn with goal
    {
        let mut st = shared.state.lock().await;
        if !restore || st.step == 0 {
            st.push_user(&format!(
                "Goal à accomplir : {}\nCritères : {:?}\nCommence par plan.update si la tâche est complexe, sinon agis.",
                spec.goal.statement, spec.goal.success_criteria
            ));
        }
    }

    let mut pending_steer: Option<String> = None;
    let mut terminal: Option<AgentState> = None;

    while terminal.is_none() {
        // Pause / steer handling between steps
        while shared.paused.load(Ordering::SeqCst) {
            match cmd_rx.recv().await {
                Some(WorkerCmd::Resume) => {
                    shared.paused.store(false, Ordering::SeqCst);
                    report(
                        &bus,
                        &agent_id,
                        AgentOutputEvent::StateChanged {
                            state: AgentState::Running,
                        },
                    )
                    .await;
                }
                Some(WorkerCmd::Steer(d)) => {
                    pending_steer = Some(d);
                    shared.paused.store(false, Ordering::SeqCst);
                }
                None => {
                    terminal = Some(AgentState::Killed);
                    break;
                }
            }
        }
        if terminal.is_some() {
            break;
        }

        if let Some(d) = pending_steer.take() {
            shared.state.lock().await.push_user(&format!("[steer] {d}"));
            report(
                &bus,
                &agent_id,
                AgentOutputEvent::Log {
                    line: format!("steer : {d}"),
                },
            )
            .await;
        }

        // Non-blocking drain of steer while running
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                WorkerCmd::Steer(d) => {
                    shared.state.lock().await.push_user(&format!("[steer] {d}"));
                }
                WorkerCmd::Resume => {}
            }
        }

        let step = {
            let mut st = shared.state.lock().await;
            st.step += 1;
            st.step
        };

        if step > max_steps {
            report(
                &bus,
                &agent_id,
                AgentOutputEvent::Log {
                    line: format!("max_steps ({max_steps}) atteint"),
                },
            )
            .await;
            terminal = Some(AgentState::Failed);
            break;
        }
        if started.elapsed() > timeout {
            report(
                &bus,
                &agent_id,
                AgentOutputEvent::Log {
                    line: "timeout goal atteint".into(),
                },
            )
            .await;
            terminal = Some(AgentState::Failed);
            break;
        }
        if let Some(max_tok) = spec.budget.max_tokens {
            let used = shared.state.lock().await.tokens_used;
            if used >= max_tok {
                terminal = Some(AgentState::Failed);
                break;
            }
        }

        // Observe: inject mem.context periodically
        if step == 1 || step % 4 == 0 {
            inject_mem_context(&bus, &shared, &spec.goal.statement).await;
        }

        // Compact if needed
        {
            let mut st = shared.state.lock().await;
            if let Some(sum) = compact_working_memory(&mut st.working_memory, 12) {
                let _ = bus
                    .call::<MemEpisodicWriteRequest, u64>(
                        "mem.episodic_write",
                        &MemEpisodicWriteRequest {
                            namespace: format!("agent:{agent_id}"),
                            text: sum,
                            metadata: serde_json::json!({"kind":"compaction"}),
                            pinned: false,
                        },
                        vec![],
                    )
                    .await;
            }
        }

        let current_task = shared.state.lock().await.current_task_title();
        report(
            &bus,
            &agent_id,
            AgentOutputEvent::Progress {
                step,
                max_steps,
                current_task: current_task.clone(),
            },
        )
        .await;

        // Think
        let full_text = match infer_turn(
            &bus,
            &shared,
            &spec,
            &data_refs,
            &mut cmd_rx,
        )
        .await
        {
            InferOutcome::Text(t) => t,
            InferOutcome::Aborted => continue,
            InferOutcome::Fatal(e) => {
                report(
                    &bus,
                    &agent_id,
                    AgentOutputEvent::Error { message: e },
                )
                .await;
                terminal = Some(AgentState::Failed);
                break;
            }
            InferOutcome::Steer(d) => {
                pending_steer = Some(d);
                continue;
            }
        };

        shared.state.lock().await.push_assistant(&full_text);

        // Act
        let action = parse_action(&full_text).unwrap_or(AgentAction {
            thought: String::new(),
            action: "noop".into(),
            args: serde_json::json!({}),
        });

        report(
            &bus,
            &agent_id,
            AgentOutputEvent::Log {
                line: format!(
                    "step {step} action={} thought={}",
                    action.action,
                    truncate(&action.thought, 80)
                ),
            },
        )
        .await;

        let act_result = execute_action(
            &bus,
            &shared,
            &mut spec,
            &tools,
            &mut mcp_sessions,
            &action,
        )
        .await;

        match act_result {
            ActResult::Continue(outcome) => {
                let mut outcome = outcome;
                if outcome.contains("permission")
                    || outcome.contains("PermissionDenied")
                    || outcome.contains("ActorDenied")
                    || outcome.contains("capacité requise")
                    || outcome.contains("capacité manquante")
                {
                    let hint = if action.action.starts_with("module.install") {
                        "module.install".to_string()
                    } else if action.action.starts_with("module.compile") {
                        "module.compile".to_string()
                    } else if action.action.starts_with("web.") || action.action.starts_with("net.")
                    {
                        "net.connect:*".to_string()
                    } else if action.action.contains('.') {
                        let mod_name = action.action.split('.').next().unwrap_or("?");
                        format!("tool.invoke:{mod_name}")
                    } else {
                        "tool.invoke:*".to_string()
                    };
                    outcome.push_str(&format!(
                        "\n[hint] Essayez : TOOL: cap.request {{\"cap\":\"{hint}\",\"reason\":\"besoin pour {}\"}}",
                        action.action
                    ));
                }
                if !outcome.is_empty() {
                    shared
                        .state
                        .lock()
                        .await
                        .push_tool(&action.action, &outcome);
                }
            }
            ActResult::Complete(summary) => {
                // Verifier pass
                let ok = verify_goal(&bus, &shared, &spec, &summary).await;
                if ok {
                    shared
                        .state
                        .lock()
                        .await
                        .artifacts
                        .push(summary.clone());
                    report(
                        &bus,
                        &agent_id,
                        AgentOutputEvent::Log {
                            line: format!("goal.complete : {summary}"),
                        },
                    )
                    .await;
                    // Notify parent via mem.shared
                    if let Some(parent) = &spec.parent_id {
                        let key = format!("agent:{parent}/child:{agent_id}");
                        let _ = bus
                            .call::<MemSharedWriteRequest, bool>(
                                "mem.shared_write",
                                &MemSharedWriteRequest {
                                    name: key,
                                    value: serde_json::json!({"result": summary, "ok": true}),
                                },
                                vec![],
                            )
                            .await;
                    }
                    terminal = Some(AgentState::Done);
                } else {
                    shared.state.lock().await.push_user(
                        "Le vérificateur estime que les critères ne sont pas remplis. Continue.",
                    );
                }
            }
            ActResult::Fail(reason) => {
                report(
                    &bus,
                    &agent_id,
                    AgentOutputEvent::Log {
                        line: format!("goal.fail : {reason}"),
                    },
                )
                .await;
                if let Some(parent) = &spec.parent_id {
                    let key = format!("agent:{parent}/child:{agent_id}");
                    let _ = bus
                        .call::<MemSharedWriteRequest, bool>(
                            "mem.shared_write",
                            &MemSharedWriteRequest {
                                name: key,
                                value: serde_json::json!({"result": reason, "ok": false}),
                            },
                            vec![],
                        )
                        .await;
                }
                terminal = Some(AgentState::Failed);
            }
            ActResult::Blocked => {
                report(
                    &bus,
                    &agent_id,
                    AgentOutputEvent::StateChanged {
                        state: AgentState::Blocked,
                    },
                )
                .await;
            }
        }

        // Reflect every 3 steps or after error-looking outcomes
        if step % 3 == 0 {
            reflect(&bus, &shared, &spec).await;
        }

        // Checkpoint
        {
            let st = shared.state.lock().await;
            let _ = persist::write_state(&st);
            let _ = persist::write_spec(&spec);
        }
    }

    let final_state = terminal.unwrap_or(AgentState::Done);
    report(
        &bus,
        &agent_id,
        AgentOutputEvent::StateChanged {
            state: final_state.clone(),
        },
    )
    .await;
    {
        let st = shared.state.lock().await;
        let _ = persist::write_state(&st);
    }
    // Exit process → agentd marks Done if not already
}

enum InferOutcome {
    Text(String),
    Aborted,
    Fatal(String),
    Steer(String),
}

async fn infer_turn(
    bus: &BusClient,
    shared: &Shared,
    spec: &AgentSpec,
    data_refs: &[String],
    cmd_rx: &mut mpsc::Receiver<WorkerCmd>,
) -> InferOutcome {
    let messages: Vec<ChatMessage> = shared
        .state
        .lock()
        .await
        .working_memory
        .iter()
        .map(|(r, c)| ChatMessage {
            role: r.clone(),
            content: c.clone(),
        })
        .collect();
    let req = InferRequest {
        model_id: spec.model_id.clone(),
        messages,
        params: InferParams {
            temperature: 0.2,
            ..InferParams::default()
        },
        priority: 1,
        data_refs: data_refs.to_vec(),
        routing: None,
    };
    let mut rx = match bus
        .call_stream::<InferRequest, TokenEvent>("model.infer", &req, spec.caps.clone())
        .await
    {
        Ok(rx) => rx,
        Err(e) => return InferOutcome::Fatal(e.to_string()),
    };

    let mut full_text = String::new();
    while let Some(ev) = rx.recv().await {
        match ev {
            Ok(TokenEvent::Started { inference_id }) => {
                *shared.current_inference.lock().await = Some(inference_id);
            }
            Ok(TokenEvent::Delta { text }) => {
                if shared.paused.load(Ordering::SeqCst) {
                    *shared.current_inference.lock().await = None;
                    report(
                        bus,
                        &spec.agent_id,
                        AgentOutputEvent::StateChanged {
                            state: AgentState::Paused,
                        },
                    )
                    .await;
                    match cmd_rx.recv().await {
                        Some(WorkerCmd::Resume) => return InferOutcome::Aborted,
                        Some(WorkerCmd::Steer(d)) => return InferOutcome::Steer(d),
                        None => return InferOutcome::Fatal("control fermé".into()),
                    }
                }
                full_text.push_str(&text);
                report(
                    bus,
                    &spec.agent_id,
                    AgentOutputEvent::Token { text },
                )
                .await;
                shared.state.lock().await.tokens_used += 1;
            }
            Ok(TokenEvent::Done { .. }) => {}
            Ok(TokenEvent::Error { message }) => {
                return InferOutcome::Fatal(message);
            }
            Err(e) => return InferOutcome::Fatal(e.to_string()),
            Ok(TokenEvent::Queued { position }) => {
                report(
                    bus,
                    &spec.agent_id,
                    AgentOutputEvent::Log {
                        line: format!("en file (position {position})"),
                    },
                )
                .await;
            }
        }
    }
    *shared.current_inference.lock().await = None;
    InferOutcome::Text(full_text)
}

enum ActResult {
    Continue(String),
    Complete(String),
    Fail(String),
    Blocked,
}

async fn execute_action(
    bus: &BusClient,
    shared: &Shared,
    spec: &mut AgentSpec,
    tools: &[ToolDesc],
    mcp_sessions: &mut HashMap<String, McpSession>,
    action: &AgentAction,
) -> ActResult {
    let name = action.action.as_str();
    let args = &action.args;
    let agent_id = spec.agent_id.clone();
    let caps = {
        let st = shared.state.lock().await;
        if st.cap_set_snapshot.is_empty() {
            spec.caps.clone()
        } else {
            st.cap_set_snapshot.clone()
        }
    };
    // Keep spec.caps aligned for persistence
    for c in &caps {
        if !spec.caps.contains(c) {
            spec.caps.push(c.clone());
        }
    }
    let trace_id = format!(
        "trace-{}-{}",
        agent_id,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );

    match name {
        "noop" => ActResult::Continue(
            "aucune action structurée détectée — réponds en JSON action".into(),
        ),
        "goal.complete" => {
            let summary = args
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or("terminé")
                .to_string();
            ActResult::Complete(summary)
        }
        "goal.fail" => {
            let reason = args
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("échec")
                .to_string();
            ActResult::Fail(reason)
        }
        "plan.update" => {
            let nodes = parse_plan_nodes(args);
            {
                let mut st = shared.state.lock().await;
                st.set_plan(nodes.clone());
            }
            report(
                bus,
                &agent_id,
                AgentOutputEvent::PlanUpdated {
                    nodes: nodes.clone(),
                },
            )
            .await;
            ActResult::Continue(format!("plan mis à jour ({} nœuds)", nodes.len()))
        }
        "docs.read" => {
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !spec.documents.iter().any(|d| d.path == path) {
                return ActResult::Continue(format!(
                    "document non attaché: {path}"
                ));
            }
            let content = read_fs(bus, &path, &agent_id, &caps).await;
            ActResult::Continue(truncate(&content, 4000))
        }
        "memory.remember" => {
            let text = args
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let _ = bus
                .call::<MemEpisodicWriteRequest, u64>(
                    "mem.episodic_write",
                    &MemEpisodicWriteRequest {
                        namespace: format!("agent:{agent_id}"),
                        text: text.clone(),
                        metadata: serde_json::json!({}),
                        pinned: false,
                    },
                    vec![],
                )
                .await;
            ActResult::Continue(format!("mémorisé: {}", truncate(&text, 120)))
        }
        "memory.recall" => {
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            match bus
                .call::<MemContextRequest, MemContextResponse>(
                    "mem.context",
                    &MemContextRequest {
                        session_id: None,
                        query,
                        k: 5,
                    },
                    vec![],
                )
                .await
            {
                Ok(r) => ActResult::Continue(r.prompt_block),
                Err(e) => ActResult::Continue(format!("recall err: {e}")),
            }
        }
        "agent.spawn" => {
            // Profondeur max 2 : un sous-agent ne spawn pas.
            if spec.parent_id.is_some() {
                return ActResult::Continue(
                    "profondeur max: un sous-agent ne peut pas spawn".into(),
                );
            }
            let children_count = shared.state.lock().await.children.len() as u32;
            if children_count >= spec.goal.max_subagents {
                return ActResult::Continue("max_subagents atteint".into());
            }
            let brief = args
                .get("brief")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let child_skills: Vec<String> = args
                .get("skills")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let child_tools: Vec<String> = args
                .get("tools")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_else(|| spec.tools.clone());
            let child_docs: Vec<DocumentRef> = args
                .get("documents")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let mut child_caps = caps_for_tools(&select_tools(&child_tools, &[]), &[]);
            child_caps.retain(|c| caps_subset(&spec.caps, std::slice::from_ref(c)));
            if child_caps.is_empty() {
                child_caps = spec.caps.clone();
            }
            spawn_child(
                bus,
                shared,
                spec,
                &brief,
                &child_skills,
                &child_tools,
                &child_docs,
                &child_caps,
            )
            .await
        }
        "agent.await" => {
            let child_id = args
                .get("child_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let key = format!("agent:{}/child:{}", agent_id, child_id);
            // Poll shared mem / agent state
            for _ in 0..60 {
                if let Ok(Some(val)) = bus
                    .call::<MemSharedReadRequest, Option<serde_json::Value>>(
                        "mem.shared_read",
                        &MemSharedReadRequest { name: key.clone() },
                        vec![],
                    )
                    .await
                {
                    let result = val.to_string();
                    report(
                        bus,
                        &agent_id,
                        AgentOutputEvent::ChildDone {
                            child_id: child_id.clone(),
                            result: result.clone(),
                        },
                    )
                    .await;
                    return ActResult::Continue(result);
                }
                // Also check agent.list state
                if let Ok(list) = bus
                    .call::<(), Vec<AgentInfo>>("agent.list", &(), vec![])
                    .await
                {
                    if let Some(info) = list.iter().find(|a| a.agent_id == child_id) {
                        if matches!(info.state, AgentState::Done | AgentState::Failed | AgentState::Killed)
                        {
                            let result = info.last_output.clone();
                            report(
                                bus,
                                &agent_id,
                                AgentOutputEvent::ChildDone {
                                    child_id: child_id.clone(),
                                    result: result.clone(),
                                },
                            )
                            .await;
                            return ActResult::Continue(result);
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            ActResult::Blocked
        }
        other => {
            // Look up tool
            let desc = tools.iter().find(|t| t.name == other);
            let backend = desc.map(|d| d.backend.clone());
            match backend {
                Some(ToolBackend::Module) => {
                    let outcome =
                        invoke_module(bus, &agent_id, &caps, other, args, &trace_id).await;
                    ActResult::Continue(outcome)
                }
                None if other.contains('.') && !other.starts_with("mcp.") => {
                    let outcome =
                        invoke_module(bus, &agent_id, &caps, other, args, &trace_id).await;
                    ActResult::Continue(outcome)
                }
                Some(ToolBackend::Native) => {
                    let outcome = invoke_native(bus, &agent_id, &caps, other, args).await;
                    ActResult::Continue(outcome)
                }
                Some(ToolBackend::Mcp { server }) => {
                    if let Some(session) = mcp_sessions.get_mut(&server) {
                        match session.call_tool(other, args.clone()).await {
                            Ok(r) => ActResult::Continue(r),
                            Err(e) => ActResult::Continue(format!("mcp err: {e}")),
                        }
                    } else {
                        ActResult::Continue(format!("session mcp {server} absente"))
                    }
                }
                Some(ToolBackend::Runtime) => {
                    ActResult::Continue(format!("action runtime inconnue: {other}"))
                }
                None if other.starts_with("mcp.") => {
                    if let Some((server, _)) = parse_mcp_name(other) {
                        if let Some(session) = mcp_sessions.get_mut(&server) {
                            match session.call_tool(other, args.clone()).await {
                                Ok(r) => ActResult::Continue(r),
                                Err(e) => ActResult::Continue(format!("mcp err: {e}")),
                            }
                        } else {
                            ActResult::Continue(format!("mcp server {server} non ouvert"))
                        }
                    } else {
                        ActResult::Continue("nom mcp invalide".into())
                    }
                }
                None => ActResult::Continue(format!("outil inconnu: {other}")),
            }
        }
    }
}

async fn spawn_child(
    bus: &BusClient,
    shared: &Shared,
    parent: &AgentSpec,
    brief: &str,
    skills: &[String],
    tools: &[String],
    documents: &[DocumentRef],
    caps: &[String],
) -> ActResult {
    let req = AgentCreateRequest {
        directive: brief.to_string(),
        caps: caps.to_vec(),
        model_id: parent.model_id.clone(),
        goal: Some(AgentGoal {
            statement: brief.to_string(),
            success_criteria: vec!["Produire un résultat clair et concis".into()],
            max_steps: parent.goal.max_steps.min(16),
            max_subagents: 0,
            timeout_secs: parent.goal.timeout_secs.min(1800),
        }),
        system_prompt: None,
        skills: skills.to_vec(),
        tools: tools.to_vec(),
        mcp_servers: vec![],
        documents: documents.to_vec(),
        parent_id: Some(parent.agent_id.clone()),
        budget: parent.budget.clone(),
        optimize_prompt: false,
    };
    match bus
        .call::<AgentCreateRequest, AgentCreateResponse>("agent.create", &req, vec![])
        .await
    {
        Ok(resp) => {
            shared.state.lock().await.children.push(resp.agent_id.clone());
            report(
                bus,
                &parent.agent_id,
                AgentOutputEvent::ChildSpawned {
                    child_id: resp.agent_id.clone(),
                    brief: brief.to_string(),
                },
            )
            .await;
            ActResult::Continue(format!("sous-agent créé: {}", resp.agent_id))
        }
        Err(e) => ActResult::Continue(format!("spawn err: {e}")),
    }
}

fn parse_mcp_name(name: &str) -> Option<(String, String)> {
    // mcp.<server>:<tool>
    let rest = name.strip_prefix("mcp.")?;
    let (server, tool) = rest.split_once(':')?;
    Some((server.to_string(), tool.to_string()))
}

fn parse_plan_nodes(args: &serde_json::Value) -> Vec<TaskNode> {
    if let Some(arr) = args.get("nodes").and_then(|n| n.as_array()) {
        return arr
            .iter()
            .enumerate()
            .map(|(i, n)| TaskNode {
                id: n
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&format!("{i}"))
                    .to_string(),
                title: n
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("tâche")
                    .to_string(),
                status: match n.get("status").and_then(|v| v.as_str()).unwrap_or("Pending") {
                    "Running" => TaskNodeStatus::Running,
                    "Blocked" => TaskNodeStatus::Blocked,
                    "Done" => TaskNodeStatus::Done,
                    "Failed" => TaskNodeStatus::Failed,
                    _ => TaskNodeStatus::Pending,
                },
                notes: n
                    .get("notes")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
            .collect();
    }
    Vec::new()
}

async fn invoke_module(
    bus: &BusClient,
    agent_id: &str,
    caps: &[String],
    tool: &str,
    args: &serde_json::Value,
    trace_id: &str,
) -> String {
    let module = tool.split('.').next().unwrap_or("").to_string();
    let req = ModuleInvokeRequest {
        module,
        tool: tool.to_string(),
        args: args.clone(),
        actor: format!("agent:{agent_id}"),
        actor_caps: caps.to_vec(),
        trace_id: trace_id.to_string(),
    };
    match bus
        .call::<ModuleInvokeRequest, ModuleInvokeResponse>("module.invoke", &req, vec![])
        .await
    {
        Ok(resp) if resp.ok => resp.result.to_string(),
        Ok(resp) => format!("ERREUR outil: {}", resp.error.unwrap_or_default()),
        Err(e) => format!("ERREUR bus: {e}"),
    }
}

async fn invoke_native(
    bus: &BusClient,
    agent_id: &str,
    caps: &[String],
    tool: &str,
    args: &serde_json::Value,
) -> String {
    let actor = format!("agent:{agent_id}");
    match tool {
        "fs.read" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            read_fs(bus, path, agent_id, caps).await
        }
        "fs.write" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            match bus
                .call::<FsWriteRequest, serde_json::Value>(
                    "fs.write",
                    &FsWriteRequest {
                        path: path.clone(),
                        content,
                        tx_id: None,
                        actor,
                        caps: caps.to_vec(),
                        trace_id: String::new(),
                    },
                    vec![],
                )
                .await
            {
                Ok(_) => format!("écrit {path}"),
                Err(e) => format!("fs.write err: {e}"),
            }
        }
        "fs.list" => {
            let prefix = args
                .get("prefix")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            match bus
                .call::<FsListRequest, Vec<aos_proto::FsEntry>>(
                    "fs.list",
                    &FsListRequest {
                        prefix,
                        caps: caps.to_vec(),
                    },
                    vec![],
                )
                .await
            {
                Ok(entries) => serde_json::to_string(&entries).unwrap_or_default(),
                Err(e) => format!("fs.list err: {e}"),
            }
        }
        "mem.episodic_write" => {
            let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let ns = args
                .get("namespace")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("agent:{agent_id}"));
            match bus
                .call::<MemEpisodicWriteRequest, u64>(
                    "mem.episodic_write",
                    &MemEpisodicWriteRequest {
                        namespace: ns,
                        text,
                        metadata: serde_json::json!({}),
                        pinned: false,
                    },
                    vec![],
                )
                .await
            {
                Ok(id) => format!("episodic id={id}"),
                Err(e) => format!("err: {e}"),
            }
        }
        "mem.episodic_query" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("").to_string();
            match bus
                .call::<MemEpisodicQueryRequest, Vec<MemHit>>(
                    "mem.episodic_query",
                    &MemEpisodicQueryRequest {
                        query,
                        k: args.get("k").and_then(|v| v.as_u64()).unwrap_or(5) as usize,
                        namespace: Some(format!("agent:{agent_id}")),
                    },
                    vec![],
                )
                .await
            {
                Ok(hits) => serde_json::to_string(&hits).unwrap_or_default(),
                Err(e) => format!("err: {e}"),
            }
        }
        "mem.context" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("").to_string();
            match bus
                .call::<MemContextRequest, MemContextResponse>(
                    "mem.context",
                    &MemContextRequest {
                        session_id: None,
                        query,
                        k: 5,
                    },
                    vec![],
                )
                .await
            {
                Ok(r) => r.prompt_block,
                Err(e) => format!("err: {e}"),
            }
        }
        "web.search" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("").to_string();
            match bus
                .call::<WebSearchRequest, WebSearchResponse>(
                    "web.search",
                    &WebSearchRequest {
                        query,
                        max_results: args
                            .get("max_results")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(5) as usize,
                        caps: caps.to_vec(),
                        actor,
                    },
                    vec![],
                )
                .await
            {
                Ok(r) => serde_json::to_string(&r.results).unwrap_or_default(),
                Err(e) => format!("web.search err: {e}"),
            }
        }
        "net.fetch" => {
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
            match bus
                .call::<NetFetchRequest, serde_json::Value>(
                    "net.fetch",
                    &NetFetchRequest {
                        url,
                        dest_path: None,
                        max_bytes: 5_000_000,
                        caps: caps.to_vec(),
                        actor,
                    },
                    vec![],
                )
                .await
            {
                Ok(v) => v.to_string(),
                Err(e) => format!("net.fetch err: {e}"),
            }
        }
        "files.generate" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let format = args
                .get("format")
                .and_then(|v| v.as_str())
                .unwrap_or("md")
                .to_string();
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            match bus
                .call::<FilesGenerateRequest, serde_json::Value>(
                    "files.generate",
                    &FilesGenerateRequest {
                        format,
                        path,
                        content,
                        title: None,
                        caps: caps.to_vec(),
                        actor,
                    },
                    vec![],
                )
                .await
            {
                Ok(v) => v.to_string(),
                Err(e) => format!("files.generate err: {e}"),
            }
        }
        "cap.request" => {
            let cap = args
                .get("cap")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let reason = args
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            match bus
                .call::<aos_proto::CapRequestRequest, aos_proto::CapRequestOutcome>(
                    "cap.request",
                    &aos_proto::CapRequestRequest {
                        agent_id: agent_id.to_string(),
                        cap: cap.clone(),
                        reason,
                    },
                    vec![],
                )
                .await
            {
                Ok(aos_proto::CapRequestOutcome::Granted) => {
                    format!("cap accordée: {cap} (hot-grant en cours)")
                }
                Ok(aos_proto::CapRequestOutcome::Denied { reason }) => {
                    format!("cap refusée: {cap} — {reason}")
                }
                Ok(aos_proto::CapRequestOutcome::ConfirmationRequired { confirmation_id }) => {
                    format!("confirmation requise: {confirmation_id}")
                }
                Err(e) => format!("cap.request err: {e}"),
            }
        }
        "skill.create" => {
            let req = aos_proto::SkillCreateRequest {
                name: args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                description: args
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                when_to_use: args
                    .get("when_to_use")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                tools: args
                    .get("tools")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default(),
                required_caps: args
                    .get("required_caps")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default(),
                body: args
                    .get("body")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                actor: actor.clone(),
                actor_caps: caps.to_vec(),
            };
            match bus
                .call::<aos_proto::SkillCreateRequest, aos_proto::SkillInfo>(
                    "skill.create",
                    &req,
                    vec![],
                )
                .await
            {
                Ok(info) => format!("skill créée: {}", info.name),
                Err(e) => format!("skill.create err: {e}"),
            }
        }
        "skill.activate" => {
            let name = args
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            match bus
                .call::<aos_proto::SkillNameRequest, aos_proto::SkillInfo>(
                    "skill.activate",
                    &aos_proto::SkillNameRequest {
                        name: name.clone(),
                        actor: actor.clone(),
                        actor_caps: caps.to_vec(),
                    },
                    vec![],
                )
                .await
            {
                Ok(info) => format!(
                    "skill activée: {}\n{}\ncaps: {:?}\ntools: {:?}",
                    info.name, info.body, info.required_caps, info.tools
                ),
                Err(e) => format!("skill.activate err: {e}"),
            }
        }
        "skill.list" => match bus
            .call::<(), Vec<aos_proto::SkillInfo>>("skill.list", &(), vec![])
            .await
        {
            Ok(list) => serde_json::to_string(
                &list
                    .iter()
                    .map(|s| {
                        serde_json::json!({"name": s.name, "description": s.description})
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap_or_default(),
            Err(e) => format!("skill.list err: {e}"),
        },
        "module.scaffold" => {
            let req = aos_proto::ModuleScaffoldRequest {
                name: args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                kind: args
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("script")
                    .to_string(),
                description: args
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                tools: args
                    .get("tools")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default(),
                required_caps: args
                    .get("required_caps")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default(),
                source: args
                    .get("source")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                actor: actor.clone(),
                actor_caps: caps.to_vec(),
            };
            match bus
                .call::<aos_proto::ModuleScaffoldRequest, aos_proto::ModuleScaffoldResponse>(
                    "module.scaffold",
                    &req,
                    vec![],
                )
                .await
            {
                Ok(r) => format!("scaffold {}: {}", r.kind, r.path),
                Err(e) => format!("module.scaffold err: {e}"),
            }
        }
        "module.package" => {
            let name = args
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            match bus
                .call::<aos_proto::ModulePackageRequest, aos_proto::ModulePackageResponse>(
                    "module.package",
                    &aos_proto::ModulePackageRequest {
                        name,
                        actor: actor.clone(),
                        actor_caps: caps.to_vec(),
                    },
                    vec![],
                )
                .await
            {
                Ok(r) => format!("package: {} hash={}", r.package_dir, r.hash),
                Err(e) => format!("module.package err: {e}"),
            }
        }
        "module.compile" => {
            let name = args
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            match bus
                .call::<aos_proto::ModuleCompileRequest, aos_proto::ModuleCompileResponse>(
                    "module.compile",
                    &aos_proto::ModuleCompileRequest {
                        name,
                        actor: actor.clone(),
                        actor_caps: caps.to_vec(),
                    },
                    vec![],
                )
                .await
            {
                Ok(r) => format!("compilé: {} hash={}", r.package_dir, r.hash),
                Err(e) => format!("module.compile err: {e}"),
            }
        }
        "module.install" => {
            let source_dir = args
                .get("source_dir")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let approved_caps = args
                .get("approved_caps")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            match bus
                .call::<aos_proto::ModuleInstallRequest, aos_proto::ModuleInfo>(
                    "module.install",
                    &aos_proto::ModuleInstallRequest {
                        source_dir,
                        approved_caps,
                        actor: actor.clone(),
                        actor_caps: caps.to_vec(),
                    },
                    vec![],
                )
                .await
            {
                Ok(info) => format!(
                    "module installé: {} v{} tools={:?} — demandez cap.request tool.invoke:{}",
                    info.name, info.version, info.tools, info.name
                ),
                Err(e) => format!("module.install err: {e}"),
            }
        }
        "module.list" => match bus
            .call::<(), Vec<aos_proto::ModuleInfo>>("module.list", &(), vec![])
            .await
        {
            Ok(list) => serde_json::to_string(&list).unwrap_or_default(),
            Err(e) => format!("module.list err: {e}"),
        },
        "module.describe" => {
            let module = args
                .get("module")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            match bus
                .call::<aos_proto::ModuleIdRequest, serde_json::Value>(
                    "module.describe",
                    &aos_proto::ModuleIdRequest { module },
                    vec![],
                )
                .await
            {
                Ok(v) => v.to_string(),
                Err(e) => format!("module.describe err: {e}"),
            }
        }
        other => format!("natif non implémenté: {other}"),
    }
}

async fn discover_module_tools(bus: &BusClient) -> Vec<ToolDesc> {
    let mut out = Vec::new();
    let Ok(list) = bus
        .call::<(), Vec<aos_proto::ModuleInfo>>("module.list", &(), vec![])
        .await
    else {
        return out;
    };
    for info in list {
        for tool in &info.tools {
            out.push(ToolDesc {
                name: tool.clone(),
                description: format!("outil module {}", info.name),
                input_schema: serde_json::json!({"type":"object"}),
                backend: ToolBackend::Module,
                required_caps: vec![format!("tool.invoke:{}", info.name)],
            });
        }
        // Enrichir via describe si possible
        if let Ok(desc) = bus
            .call::<aos_proto::ModuleIdRequest, serde_json::Value>(
                "module.describe",
                &aos_proto::ModuleIdRequest {
                    module: info.name.clone(),
                },
                vec![],
            )
            .await
        {
            if let Some(tools) = desc
                .get("manifest")
                .and_then(|m| m.get("tools"))
                .and_then(|t| t.as_array())
            {
                for t in tools {
                    let name = t
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if name.is_empty() {
                        continue;
                    }
                    if let Some(existing) = out.iter_mut().find(|x| x.name == name) {
                        if let Some(d) = t.get("description").and_then(|v| v.as_str()) {
                            existing.description = d.to_string();
                        }
                        if let Some(schema) = t.get("input_schema") {
                            existing.input_schema = schema.clone();
                        }
                    }
                }
            }
        }
    }
    out
}

async fn read_fs(bus: &BusClient, path: &str, agent_id: &str, caps: &[String]) -> String {
    match bus
        .call::<FsReadRequest, FsReadResponse>(
            "fs.read",
            &FsReadRequest {
                path: path.to_string(),
                actor: format!("agent:{agent_id}"),
                caps: caps.to_vec(),
            },
            vec![],
        )
        .await
    {
        Ok(r) => r.content,
        Err(e) => format!("fs.read err: {e}"),
    }
}

async fn index_documents(
    bus: &BusClient,
    agent_id: &str,
    caps: &[String],
    docs: &[DocumentRef],
) {
    for d in docs {
        let content = read_fs(bus, &d.path, agent_id, caps).await;
        let excerpt = truncate(&content, 500);
        let _ = bus
            .call::<MemEpisodicWriteRequest, u64>(
                "mem.episodic_write",
                &MemEpisodicWriteRequest {
                    namespace: format!("agent:{agent_id}:docs"),
                    text: format!("{} ({}) : {excerpt}", d.label, d.path),
                    metadata: serde_json::json!({"path": d.path}),
                    pinned: true,
                },
                vec![],
            )
            .await;
    }
}

async fn inject_mem_context(bus: &BusClient, shared: &Shared, query: &str) {
    if let Ok(r) = bus
        .call::<MemContextRequest, MemContextResponse>(
            "mem.context",
            &MemContextRequest {
                session_id: None,
                query: query.to_string(),
                k: 3,
            },
            vec![],
        )
        .await
    {
        if !r.prompt_block.is_empty() {
            shared.state.lock().await.working_memory.push((
                "system".into(),
                format!("[mem.context]\n{}", r.prompt_block),
            ));
        }
    }
}

async fn reflect(bus: &BusClient, shared: &Shared, spec: &AgentSpec) {
    let st = shared.state.lock().await;
    let progress = format!(
        "step {}/{} goal={} tasks={:?}",
        st.step,
        spec.goal.max_steps,
        spec.goal.statement,
        st.plan_stack
    );
    drop(st);
    let req = InferRequest {
        model_id: spec.model_id.clone(),
        messages: vec![
            ChatMessage {
                role: "system".into(),
                content: "Tu es un critique. En 2 phrases: est-ce que l'agent avance vers le goal ? Que faire ensuite ?".into(),
            },
            ChatMessage {
                role: "user".into(),
                content: progress,
            },
        ],
        params: InferParams {
            max_tokens: 120,
            temperature: 0.1,
            ..InferParams::default()
        },
        priority: 1,
        data_refs: vec![],
        routing: None,
    };
    if let Ok(mut rx) = bus
        .call_stream::<InferRequest, TokenEvent>("model.infer", &req, vec![])
        .await
    {
        let mut text = String::new();
        while let Some(ev) = rx.recv().await {
            if let Ok(TokenEvent::Delta { text: t }) = ev {
                text.push_str(&t);
            }
        }
        if !text.is_empty() {
            shared.state.lock().await.reflections.push(text.clone());
            report(
                bus,
                &spec.agent_id,
                AgentOutputEvent::Reflection { text: text.clone() },
            )
            .await;
            shared
                .state
                .lock()
                .await
                .working_memory
                .push(("system".into(), format!("[reflect] {text}")));
        }
    }
}

async fn verify_goal(
    bus: &BusClient,
    shared: &Shared,
    spec: &AgentSpec,
    summary: &str,
) -> bool {
    if spec.goal.success_criteria.is_empty() {
        return true;
    }
    let criteria = spec.goal.success_criteria.join("; ");
    let req = InferRequest {
        model_id: spec.model_id.clone(),
        messages: vec![
            ChatMessage {
                role: "system".into(),
                content: "Réponds uniquement YES ou NO. Les critères de succès sont-ils remplis ?".into(),
            },
            ChatMessage {
                role: "user".into(),
                content: format!(
                    "Goal: {}\nCritères: {criteria}\nRésumé agent: {summary}",
                    spec.goal.statement
                ),
            },
        ],
        params: InferParams {
            max_tokens: 8,
            temperature: 0.0,
            ..InferParams::default()
        },
        priority: 2,
        data_refs: vec![],
        routing: None,
    };
    let mut text = String::new();
    if let Ok(mut rx) = bus
        .call_stream::<InferRequest, TokenEvent>("model.infer", &req, vec![])
        .await
    {
        while let Some(ev) = rx.recv().await {
            if let Ok(TokenEvent::Delta { text: t }) = ev {
                text.push_str(&t);
            }
        }
    }
    let upper = text.to_uppercase();
    let ok = upper.contains("YES") || upper.contains("OUI");
    shared.state.lock().await.push_tool(
        "verify",
        &format!("verdict={} raw={}", ok, truncate(&text, 40)),
    );
    ok
}

async fn optimize_prompt_now(bus: &BusClient, spec: &AgentSpec) -> Result<String, String> {
    let prompt = optimize_prompt_request(
        &spec.goal.statement,
        &spec.skills,
        &spec.tools,
        spec.system_prompt.as_deref(),
    );
    let req = InferRequest {
        model_id: spec.model_id.clone(),
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
    let mut rx = bus
        .call_stream::<InferRequest, TokenEvent>("model.infer", &req, vec![])
        .await
        .map_err(|e| e.to_string())?;
    let mut text = String::new();
    while let Some(ev) = rx.recv().await {
        if let Ok(TokenEvent::Delta { text: t }) = ev {
            text.push_str(&t);
        }
    }
    if text.trim().is_empty() {
        Err("prompt vide".into())
    } else {
        Ok(text.trim().to_string())
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        format!("{}…", s.chars().take(n).collect::<String>())
    } else {
        s.to_string()
    }
}
