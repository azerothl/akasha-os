//! `aos-agent-worker` — boucle agentic Observe / Think / Act / Reflect / Checkpoint.
//!
//! Usage : `aos-agent-worker --agent-id <id> --bus <addr> --spec-path <path>
//!          [--restore]`

use aos_agent::actions::{
    parse_actions, parse_embedded_action_question, strip_reasoning, strip_tool_markup,
    AgentAction, THREAD_FAIL_COULD_NOT_ACT, THREAD_FAIL_COULD_NOT_CONTINUE,
};
use aos_agent::assess::{parse_assess_response, AssessResult};
use aos_agent::mcp::{open_mcp_tools_with_secrets, McpSession};
use aos_agent::context_budget::{
    choose_agent_max_tokens, clamp_spawn_brief, compact_after_prompt_overflow,
    enforce_prompt_budget, is_prompt_too_long_error, is_technical_vision_infer_error,
    prompt_budget, sanitize_assistant_for_memory, LoopGuard, LoopVerdict, DEFAULT_N_CTX_HINT,
    MAX_OVERFLOW_INFER_RETRIES,
};
use aos_agent::persist;
use aos_agent::canvas_scene::{
    agent_has_canvas_tools, begin_canvas_vision, canvas_action_near_duplicate_reason, canvas_critic_system_prompt,
    canvas_op_succeeded,
    canvas_visual_fingerprint, canvas_visual_progress, canvas_visual_progress_note,
    canvas_reflect_user_content, canvas_repeat_stroke_verdict, canvas_scene_prompt_block,
    canvas_tool_mutates_scene, end_canvas_vision, fetch_canvas_aspect,
    fetch_canvas_global_validation, fetch_canvas_scene_digest, global_canvas_validation_due,
    merge_canvas_vision_refs, refresh_canvas_scene_after_op,
    session_model_has_vision, strip_vision_image_paths, should_run_canvas_critic,
    canvas_text_only_critic_system_prompt,
    CanvasRepeatVerdict,
};
use aos_agent::prompt::{compile_system_prompt, optimize_prompt_request, PromptCompileInput};
use aos_agent::skills::{load_skills, match_skill_by_action, merge_skill_tools, skill_misuse_hint, SkillDoc};
use aos_agent::tool_exec::format_module_invoke_result;
use aos_agent::tools::{
    canonicalize_tool_name, canvas_tool_denied_by_allowlist, canvas_tools_from_module_list, caps_for_tools, caps_subset,
    classify_action, canvas_draw_strategy_hint, is_module_fallback_candidate,
    normalize_tool_args, resolve_tool_backend, restrict_canvas_tools, select_tools,
    select_tools_mode, strip_canvas_blocked_runtime_tools, ToolBackend,
    ToolDesc,
};
use aos_agent::{intents, CognitiveState, ControlCmd, ControlResp, ReportPayload};
use aos_ipc::{BusClient, BusService};
use aos_proto::{
    AgentCreateRequest, AgentCreateResponse, AgentGoal, AgentInfo, AgentOutputEvent, AgentSpec,
    AgentSource, AgentState, AgentStepRecord, CancelRequest, ChatAttachment, ChatMessage,
    ChatSessionAppendRequest, ChatSessionGetResponse, ChatSessionIdRequest, DeepPlanStepPatch,
    DocumentRef, FilesGenerateRequest, FsListRequest, FsReadRequest, FsReadResponse, FsWriteRequest,
    InferParams, InferRequest, MemContextRequest, MemContextResponse, MemEpisodicQueryRequest,
    MemEpisodicWriteRequest, MemHit, MemRememberResponse, MemSharedReadRequest,
    MemSharedWriteRequest, ModuleInfo, ModuleInvokeRequest, ModuleInvokeResponse, NetFetchRequest,
    PlanAppendLogRequest, PlanCreateRequest, PlanDelegateStepRequest, PlanGetRequest,
    PlanReplaceTreeRequest, PlanResponse, PlanStep, PlanStepStatus, PlanUpdateStepRequest, TaskNode,
    TaskNodeStatus, TokenEvent, WebBrowseRequest, WebBrowseResponse, WebSearchHit, WebSearchRequest,
    WebSearchResponse,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, Mutex};

enum WorkerCmd {
    Resume,
    Steer(String),
    ActDecision { act_id: String, approved: bool },
    ChildFinished {
        child_id: String,
        result: String,
        ok: bool,
    },
}

/// Attente max d'une réponse `user.ask` (bornée aussi par le timeout du goal).
const USER_ASK_TIMEOUT: Duration = Duration::from_secs(10 * 60);

enum AskWait {
    Answer(String),
    Timeout { waited_secs: u64 },
    Killed,
}

struct Shared {
    state: Mutex<CognitiveState>,
    paused: AtomicBool,
    current_inference: Mutex<Option<u64>>,
    cmd_tx: mpsc::Sender<WorkerCmd>,
    /// Résultats poussés par agentd quand un sous-agent atteint un état terminal.
    child_results: Mutex<HashMap<String, (String, bool)>>,
    /// Sous-agents déjà intégrés (évite un double inject après `agent.await`).
    consumed_child_results: Mutex<HashSet<String>>,
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

fn load_mcp_secrets(agent_id: &str) -> HashMap<String, String> {
    let path = PathBuf::from("var/agents")
        .join(agent_id)
        .join("mcp_secrets.json");
    let map = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let _ = std::fs::remove_file(&path);
    map
}

/// When the spec has no model_id, inherit the bound chat session's model so
/// model.infer does not fall back to installed default_chat.
async fn inherit_session_model(bus: &BusClient, spec: &mut AgentSpec) {
    if spec
        .model_id
        .as_ref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
    {
        return;
    }
    let Some(sid) = spec
        .session_id
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    let Ok(resp) = bus
        .call::<ChatSessionIdRequest, ChatSessionGetResponse>(
            "chat.session.get",
            &ChatSessionIdRequest {
                session_id: sid.to_string(),
            },
            vec![],
        )
        .await
    else {
        return;
    };
    if let Some(mid) = resp.meta.model_id.filter(|s| !s.trim().is_empty()) {
        spec.model_id = Some(mid);
        let _ = persist::write_spec(spec);
    }
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

    inherit_session_model(&bus, &mut spec).await;

    let canvas_exported: Vec<String> = bus
        .call::<(), Vec<ModuleInfo>>("module.list", &(), vec![])
        .await
        .map(|list| canvas_tools_from_module_list(&list))
        .unwrap_or_default();
    restrict_canvas_tools(&mut spec.tools, &canvas_exported);
    let _ = persist::write_spec(&spec);

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
    let deep = spec.cognitive_mode.is_deep_thinking();
    state.deep_thinking = deep;
    if deep {
        state.needs_plan = true;
    }

    let shared = Arc::new(Shared {
        state: Mutex::new(state),
        paused: AtomicBool::new(false),
        current_inference: Mutex::new(None),
        cmd_tx: cmd_tx.clone(),
        child_results: Mutex::new(HashMap::new()),
        consumed_child_results: Mutex::new(HashSet::new()),
    });

    // Skills + tools + MCP + modules installés (catalogue dynamique)
    let mut skill_docs = load_skills(&spec.skills);
    let tool_ids = merge_skill_tools(&spec.tools, &skill_docs);
    let mcp_secrets = load_mcp_secrets(&agent_id);
    let (mut mcp_sessions, mcp_tools) =
        open_mcp_tools_with_secrets(&spec.mcp_servers, &mcp_secrets).await;
    let mut module_tools = discover_module_tools(&bus).await;
    module_tools.extend(mcp_tools);
    let mut tools = select_tools_mode(&tool_ids, &module_tools, deep);
    strip_canvas_blocked_runtime_tools(&mut tools, &spec.tools);
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

    install_system_prompt(&bus, &shared, &spec, &skill_docs, &tools).await;

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
                    ControlCmd::ChildFinished {
                        child_id,
                        result,
                        ok,
                    } => {
                        record_child_finished(&shared, child_id.clone(), result.clone(), ok).await;
                        let _ = shared
                            .cmd_tx
                            .send(WorkerCmd::ChildFinished {
                                child_id,
                                result,
                                ok,
                            })
                            .await;
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
                    ControlCmd::ActDecision { act_id, approved } => {
                        let _ = shared
                            .cmd_tx
                            .send(WorkerCmd::ActDecision { act_id, approved })
                            .await;
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

    // Seed + task.assess + mémoire (ordre : assess → plan si besoin → mémoire ciblée)
    let is_child = spec.parent_id.is_some();
    let fresh_start = !restore || shared.state.lock().await.step == 0;
    if fresh_start {
        if is_child {
            {
                let mut st = shared.state.lock().await;
                st.complexity = Some("simple".into());
                st.needs_plan = false;
                st.push_user(&format!(
                    "Brief sous-agent : {}\nCritères : {:?}\n\
                     Consulte la mémoire ([mem.bootstrap]) pour accélérer, puis agis. \
                     Pas de plan.update (tu es un sous-agent).",
                    spec.goal.statement, spec.goal.success_criteria
                ));
            }
            bootstrap_memory_recall(
                &bus,
                &shared,
                &agent_id,
                &spec.goal.statement,
                "brief sous-agent",
            )
            .await;
        } else {
            let assess = require_canvas_plan(run_task_assess(
                &bus,
                &shared,
                &spec,
                &spec.goal.statement,
                "démarrage",
            )
            .await, &spec);
            apply_assess_to_runtime(
                &bus,
                &shared,
                &mut spec,
                &mut skill_docs,
                &mut tools,
                &module_tools,
                &assess,
            )
            .await;
            {
                let mut st = shared.state.lock().await;
                if assess.is_complex() {
                    let canvas_draw = agent_has_canvas_tools(&spec.tools);
                    let msg = if canvas_draw {
                        format!(
                            "Goal à accomplir : {}\nCritères : {:?}\n\
                             Classification : complex — {}. \
                             Dessin canvas : un seul auteur (toi). Pas de agent.spawn — \
                             {} \
                             media.image.generate interdit.",
                            spec.goal.statement,
                            spec.goal.success_criteria,
                            assess.reason,
                            canvas_draw_strategy_hint(&canvas_exported)
                        )
                    } else {
                        format!(
                            "Goal à accomplir : {}\nCritères : {:?}\n\
                             Classification : complex — {}. \
                             Première action obligatoire : plan.update avec des nœuds atomiques. \
                             Si des nœuds sont indépendants (pas de dépendance de données), \
                             délègue-les en parallèle via agent.spawn (briefs COURTS auto-suffisants, \
                             tools/docs minimaux — pas de dump parent) puis agent.await ; \
                             n'exécute en solo que les nœuds séquentiels ou légers. \
                             memory.recall sur le nœud / brief courant (pas sur le goal entier).",
                            spec.goal.statement, spec.goal.success_criteria, assess.reason
                        )
                    };
                    st.push_user(&msg);
                } else {
                    st.push_user(&format!(
                        "Goal à accomplir : {}\nCritères : {:?}\n\
                         Classification : simple — {}. \
                         La mémoire vient d'être consultée ([mem.bootstrap]) : réutilise-la ; \
                         affiner avec memory.recall si besoin avant une recherche externe, puis agis.",
                        spec.goal.statement, spec.goal.success_criteria, assess.reason
                    ));
                }
            }
            if !assess.is_complex() {
                bootstrap_memory_recall(
                    &bus,
                    &shared,
                    &agent_id,
                    &spec.goal.statement,
                    "démarrage",
                )
                .await;
            }
        }
    }

    let mut pending_steer: Option<String> = None;
    let mut terminal: Option<AgentState> = None;
    let mut loop_guard = LoopGuard::default();
    let mut last_canvas_scene_png: Option<String> = None;
    let mut last_canvas_visual = None;
    let mut n_ctx_hint = DEFAULT_N_CTX_HINT;

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
                Some(WorkerCmd::ActDecision { .. }) => {}
                Some(WorkerCmd::ChildFinished {
                    child_id,
                    result,
                    ok,
                }) => {
                    record_child_finished(&shared, child_id, result, ok).await;
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
            if is_child {
                bootstrap_memory_recall(&bus, &shared, &agent_id, &d, "steer").await;
            } else {
                let assess = require_canvas_plan(
                    run_task_assess(&bus, &shared, &spec, &d, "steer").await,
                    &spec,
                );
                apply_assess_to_runtime(
                    &bus,
                    &shared,
                    &mut spec,
                    &mut skill_docs,
                    &mut tools,
                    &module_tools,
                    &assess,
                )
                .await;
                if assess.is_complex() {
                    let needs_new_plan = shared.state.lock().await.task_graph.is_empty();
                    if needs_new_plan {
                        shared.state.lock().await.push_user(
                            "[steer] Tâche devenue complexe — appelle plan.update avant d'agir.",
                        );
                    } else {
                        let q = shared
                            .state
                            .lock()
                            .await
                            .current_task_title()
                            .unwrap_or_else(|| d.clone());
                        bootstrap_memory_recall(&bus, &shared, &agent_id, &q, "steer").await;
                    }
                } else {
                    bootstrap_memory_recall(&bus, &shared, &agent_id, &d, "steer").await;
                }
            }
        }

        // Non-blocking drain of steer / child-done while running
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                WorkerCmd::Steer(d) => {
                    shared.state.lock().await.push_user(&format!("[steer] {d}"));
                }
                WorkerCmd::ChildFinished {
                    child_id,
                    result,
                    ok,
                } => {
                    record_child_finished(&shared, child_id, result, ok).await;
                }
                WorkerCmd::Resume | WorkerCmd::ActDecision { .. } => {}
            }
        }
        drain_child_finished_into_memory(&bus, &agent_id, &shared).await;

        let step = {
            let mut st = shared.state.lock().await;
            st.step += 1;
            st.step
        };

        if step > max_steps {
            let reason = format!("max_steps ({max_steps}) atteint");
            shared.state.lock().await.artifacts.push(reason.clone());
            report(
                &bus,
                &agent_id,
                AgentOutputEvent::Error {
                    message: reason.clone(),
                },
            )
            .await;
            report(
                &bus,
                &agent_id,
                AgentOutputEvent::Log {
                    line: reason,
                },
            )
            .await;
            terminal = Some(AgentState::Failed);
            break;
        }
        if started.elapsed() > timeout {
            let reason = "timeout goal atteint".to_string();
            shared.state.lock().await.artifacts.push(reason.clone());
            report(
                &bus,
                &agent_id,
                AgentOutputEvent::Error {
                    message: reason.clone(),
                },
            )
            .await;
            report(
                &bus,
                &agent_id,
                AgentOutputEvent::Log {
                    line: reason,
                },
            )
            .await;
            terminal = Some(AgentState::Failed);
            break;
        }
        if let Some(max_tok) = spec.budget.max_tokens {
            let used = shared.state.lock().await.tokens_used;
            if used >= max_tok {
                let reason = format!("budget tokens atteint ({used}/{max_tok})");
                shared.state.lock().await.artifacts.push(reason.clone());
                report(
                    &bus,
                    &agent_id,
                    AgentOutputEvent::Error {
                        message: reason,
                    },
                )
                .await;
                terminal = Some(AgentState::Failed);
                break;
            }
        }

        // Observe: rappel mémoire périodique sur le nœud courant (sinon le goal)
        if step > 1 && step % 4 == 0 {
            let query = {
                let st = shared.state.lock().await;
                st.current_task_title()
                    .unwrap_or_else(|| spec.goal.statement.clone())
            };
            inject_mem_context(&bus, &shared, &agent_id, &query).await;
        }

        // Budget prompt avant infer (aligné n_ctx − max_gen)
        let gen_tokens = {
            let st = shared.state.lock().await;
            choose_agent_max_tokens(&st.working_memory, &spec.goal.statement)
        };
        let budget = prompt_budget(n_ctx_hint, gen_tokens);
        {
            let mut st = shared.state.lock().await;
            if let Some(sum) = enforce_prompt_budget(&mut st.working_memory, budget, 6) {
                let _ = bus
                    .call::<MemEpisodicWriteRequest, MemRememberResponse>(
                        "mem.episodic_write",
                        &MemEpisodicWriteRequest {
                            namespace: format!("agent:{agent_id}"),
                            text: sum,
                            metadata: serde_json::json!({"kind":"compaction"}),
                            pinned: false,
                            ..Default::default()
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

        let step_t0 = Instant::now();
        let infer_t0 = Instant::now();
        // Think (+ retry PromptTooLong : trim + réduction max_tokens)
        let mut prompt_retries = 0u32;
        let mut gen_tokens = gen_tokens;
        let canvas_agent = agent_has_canvas_tools(&spec.tools);
        let mut step_refs = data_refs.clone();
        let canvas_sid = spec.session_id.clone();
        if let Some(ref png) = last_canvas_scene_png.take() {
            if session_model_has_vision(&bus, spec.model_id.as_deref()).await {
                step_refs = merge_canvas_vision_refs(&step_refs, png);
            }
        } else if canvas_agent {
            if let Some(sid) = canvas_sid.as_deref() {
                let aspect = fetch_canvas_aspect(&bus, sid).await;
                if let Some(png) = begin_canvas_vision(
                    &bus,
                    sid,
                    aspect,
                    spec.model_id.as_deref(),
                )
                .await
                {
                    step_refs = merge_canvas_vision_refs(&step_refs, &png);
                }
            }
        }
        let canvas_active = canvas_agent
            && step_refs.iter().any(|p| {
                let lower = p.to_ascii_lowercase();
                lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg")
            });
        let infer = loop {
            match infer_turn(
                &bus,
                &shared,
                &spec,
                &step_refs,
                &mut cmd_rx,
                gen_tokens,
            )
            .await
            {
                InferOutcome::Text(t) => break Ok(t),
                InferOutcome::Aborted => break Err(InferControl::Continue),
                InferOutcome::Fatal(e)
                    if is_prompt_too_long_error(&e) && prompt_retries < MAX_OVERFLOW_INFER_RETRIES =>
                {
                    prompt_retries += 1;
                    eprintln!(
                        "prompt overflow — compaction silencieuse max_tokens réduit \
                         (retry {prompt_retries}/{MAX_OVERFLOW_INFER_RETRIES} ; {e})"
                    );
                    {
                        let mut st = shared.state.lock().await;
                        if let Some(sum) = compact_after_prompt_overflow(
                            &mut st.working_memory,
                            &mut n_ctx_hint,
                            &mut gen_tokens,
                            &e,
                        ) {
                            let _ = bus
                                .call::<MemEpisodicWriteRequest, MemRememberResponse>(
                                    "mem.episodic_write",
                                    &MemEpisodicWriteRequest {
                                        namespace: format!("agent:{agent_id}"),
                                        text: sum,
                                        metadata: serde_json::json!({
                                            "kind": "compaction",
                                            "reason": "prompt_too_long"
                                        }),
                                        pinned: false,
                                        ..Default::default()
                                    },
                                    vec![],
                                )
                                .await;
                        }
                    }
                }
                InferOutcome::Fatal(e) if is_technical_vision_infer_error(&e) => {
                    eprintln!("vision refs ignorées (pas de mmproj) : {e}");
                    step_refs = strip_vision_image_paths(&step_refs);
                    if canvas_active {
                        if let Some(sid) = canvas_sid.as_deref() {
                            end_canvas_vision(&bus, sid).await;
                        }
                    }
                    continue;
                }
                InferOutcome::Fatal(e) => {
                    if is_prompt_too_long_error(&e) {
                        eprintln!(
                            "prompt overflow après {prompt_retries} retries : {e}"
                        );
                        report(
                            &bus,
                            &agent_id,
                            AgentOutputEvent::Error {
                                message: THREAD_FAIL_COULD_NOT_CONTINUE.into(),
                            },
                        )
                        .await;
                    } else {
                        report(
                            &bus,
                            &agent_id,
                            AgentOutputEvent::Error { message: e },
                        )
                        .await;
                    }
                    terminal = Some(AgentState::Failed);
                    break Err(InferControl::Fail);
                }
                InferOutcome::Steer(d) => {
                    pending_steer = Some(d);
                    break Err(InferControl::Continue);
                }
            }
        };
        let infer = match infer {
            Ok(t) => t,
            Err(InferControl::Continue) => {
                if canvas_active {
                    if let Some(sid) = canvas_sid.as_deref() {
                        end_canvas_vision(&bus, sid).await;
                    }
                }
                continue;
            }
            Err(InferControl::Fail) => {
                if canvas_active {
                    if let Some(sid) = canvas_sid.as_deref() {
                        end_canvas_vision(&bus, sid).await;
                    }
                }
                break;
            }
        };
        if canvas_active {
            if let Some(sid) = canvas_sid.as_deref() {
                end_canvas_vision(&bus, sid).await;
            }
        }
        let infer_ms = infer_t0.elapsed().as_millis() as u64;
        let (reasoning, clean_text) = aos_agent::actions::split_reasoning(&infer.text);
        let full_text = if clean_text.is_empty() && !reasoning.is_empty() {
            String::new()
        } else {
            clean_text
        };

        let mut batch_actions = parse_actions(&infer.text);
        let parsed_ok = !batch_actions.is_empty();
        if batch_actions.is_empty() {
            batch_actions.push(AgentAction {
                thought: if reasoning.is_empty() {
                    String::new()
                } else {
                    reasoning.chars().take(400).collect()
                },
                action: "noop".into(),
                args: serde_json::json!({}),
            });
        }
        let memory_text = if parsed_ok {
            strip_tool_markup(&full_text)
        } else {
            full_text.clone()
        };
        shared
            .state
            .lock()
            .await
            .push_assistant(&sanitize_assistant_for_memory(&memory_text, parsed_ok));

        let action_log = if batch_actions.len() == 1 {
            batch_actions[0].action.clone()
        } else {
            batch_actions
                .iter()
                .map(|a| a.action.as_str())
                .collect::<Vec<_>>()
                .join("+")
        };
        report(
            &bus,
            &agent_id,
            AgentOutputEvent::Log {
                line: format!(
                    "step {step} action={action_log} thought={}",
                    truncate(&batch_actions[0].thought, 80)
                ),
            },
        )
        .await;

        let tool_t0 = Instant::now();
        let mut action = batch_actions[0].clone();
        let mut act_result = ActResult::Continue(String::new());
        let mut tool_result = String::new();
        let mut step_fail_reason: Option<String> = None;
        let mut step_child_id: Option<String> = None;
        let mut step_sources: Vec<AgentSource> = Vec::new();
        let mut multi_continue = false;
        let mut canvas_scene_changed = false;

        'run_actions: for (action_idx, step_action) in batch_actions.iter().enumerate() {
            if terminal.is_some() {
                break;
            }
            action = step_action.clone();
            // Reject exact replays before issuing them. A post-write warning is
            // too late for a vector canvas because the duplicate is visible.
            let canonical_action = canonicalize_tool_name(&action.action);
            let canvas_stage_block = {
                let st = shared.state.lock().await;
                st.canvas_preparation_action_block_reason(&canonical_action)
                    .map(str::to_string)
            };
            let duplicate_canvas_op = {
                let st = shared.state.lock().await;
                canvas_action_near_duplicate_reason(&st.trace, &canonical_action, &action.args)
            };
            let one = if let Some(reason) = canvas_stage_block {
                ActResult::Continue(reason)
            } else if let Some(reason) = duplicate_canvas_op {
                ActResult::Continue(reason)
            } else if should_gate_action(&spec, &action.action) {
                match gate_action(
                    &bus, &shared, &mut cmd_rx, &spec, &action, &agent_id, timeout, started,
                )
                .await
                {
                    GateWait::Proceed => {
                        execute_action(
                            &bus,
                            &shared,
                            &mut spec,
                            &tools,
                            &skill_docs,
                            &mut mcp_sessions,
                            &action,
                        )
                        .await
                    }
                    GateWait::Denied => {
                        ActResult::Continue("action refusée par l'utilisateur".into())
                    }
                    GateWait::Killed => {
                        terminal = Some(AgentState::Killed);
                        ActResult::Continue("interrompu pendant l'attente d'autorisation".into())
                    }
                }
            } else {
                execute_action(
                    &bus,
                    &shared,
                    &mut spec,
                    &tools,
                    &skill_docs,
                    &mut mcp_sessions,
                    &action,
                )
                .await
            };

            match one {
                ActResult::Continue(outcome) => {
                    let mut outcome = outcome;
                    if outcome.contains("permission")
                        || outcome.contains("PermissionDenied")
                        || outcome.contains("ActorDenied")
                        || outcome.contains("capacité requise")
                        || outcome.contains("capacité manquante")
                    {
                        let canonical = canonicalize_tool_name(&action.action);
                        let hint = if canonical.starts_with("module.install") {
                            "module.install".to_string()
                        } else if canonical.starts_with("module.compile") {
                            "module.compile".to_string()
                        } else if canonical.starts_with("web.") || canonical.starts_with("net.")
                        {
                            "net.connect:*".to_string()
                        } else if canonical.starts_with("media.") {
                            "media.generate".to_string()
                        } else if canonical.starts_with("fs.") {
                            "fs.write:**".to_string()
                        } else if canonical.contains('.') {
                            let mod_name = canonical.split('.').next().unwrap_or("?");
                            format!("tool.invoke:{mod_name}")
                        } else {
                            "tool.invoke:*".to_string()
                        };
                        outcome.push_str(&format!(
                            "\n[hint] Essayez : TOOL: cap.request {{\"cap\":\"{hint}\",\"reason\":\"besoin pour {}\"}}",
                            action.action
                        ));
                    }
                    if canvas_tool_mutates_scene(&action.action) && canvas_op_succeeded(&outcome) {
                        if let Some(sid) = spec.session_id.as_deref().filter(|s| !s.is_empty()) {
                            let scene = refresh_canvas_scene_after_op(
                                &bus,
                                sid,
                                &outcome,
                            )
                            .await;
                            outcome = scene.text;
                            if let Some(png) = scene.png_path {
                                if let Some(current) = canvas_visual_fingerprint(&png) {
                                    if let Some(previous) = last_canvas_visual {
                                        let progress = canvas_visual_progress(previous, current);
                                        outcome.push_str(&format!("\n\n{}", canvas_visual_progress_note(progress)));
                                        if !progress.meaningful_change {
                                            shared.state.lock().await.push_user(
                                                "[runtime] Le vérificateur visuel ne voit presque aucun changement. Ne répète pas cette forme : choisis une pièce distincte ou modifie une séquence existante.",
                                            );
                                        }
                                    }
                                    last_canvas_visual = Some(current);
                                } else {
                                    outcome.push_str(
                                        "\n\n[canvas visual verifier] indisponible : PNG exporté inaccessible au worker.",
                                    );
                                }
                                last_canvas_scene_png = Some(png);
                                canvas_scene_changed = true;
                            }
                        }
                    }
                    let mut one_tool_result = outcome.clone();
                    if let Some(child) =
                        extract_child_id(&action.action, &outcome, &action.args)
                    {
                        step_child_id = Some(child);
                    }
                    let sources = collect_sources(&action.action, &action.args, &outcome);
                    if !sources.is_empty() {
                        step_sources.extend(sources);
                    }
                    if action.action == "web.search" && !step_sources.is_empty() {
                        one_tool_result = format!(
                            "{} résultat(s) : {}",
                            step_sources.len(),
                            step_sources
                                .iter()
                                .take(3)
                                .map(|s| s.title.as_str())
                                .collect::<Vec<_>>()
                                .join(" · ")
                        );
                    }
                    if !outcome.is_empty() {
                        let mut st = shared.state.lock().await;
                        if canvas_tool_mutates_scene(&action.action) {
                            st.push_canvas_tool(&action.action, &outcome);
                        } else {
                            st.push_tool(&action.action, &outcome);
                        }
                        maybe_report_plan_advance_after_canvas_draw(
                            &bus,
                            &agent_id,
                            &mut st,
                            &action.action,
                            &outcome,
                        )
                        .await;
                    }
                    if !tool_result.is_empty() && !one_tool_result.is_empty() {
                        tool_result.push('\n');
                    }
                    tool_result.push_str(&one_tool_result);
                    act_result = ActResult::Continue(tool_result.clone());
                    multi_continue = true;
                    if action_idx + 1 < batch_actions.len() && terminal.is_none() {
                        continue 'run_actions;
                    }
                }
                other => {
                    act_result = other;
                    break 'run_actions;
                }
            }
        }
        let tool_ms = tool_t0.elapsed().as_millis() as u64;

        match act_result {
            ActResult::AskUser { question, choices } => {
                let body = format_user_question(&question, &choices);
                report(
                    &bus,
                    &agent_id,
                    AgentOutputEvent::Log {
                        line: format!("user.ask : {body}"),
                    },
                )
                .await;
                post_user_question(&bus, &spec, &body).await;
                shared.paused.store(true, Ordering::SeqCst);
                report(
                    &bus,
                    &agent_id,
                    AgentOutputEvent::StateChanged {
                        state: AgentState::Blocked,
                    },
                )
                .await;
                let remaining = timeout.saturating_sub(started.elapsed());
                let ask_wait = USER_ASK_TIMEOUT.min(remaining.max(Duration::from_secs(15)));
                match wait_user_answer(&shared, &mut cmd_rx, ask_wait).await {
                    AskWait::Killed => {
                        terminal = Some(AgentState::Killed);
                        tool_result = "interrompu en attendant l'utilisateur".into();
                    }
                    AskWait::Timeout { waited_secs } => {
                        let mins = (waited_secs / 60).max(1);
                        report(
                            &bus,
                            &agent_id,
                            AgentOutputEvent::Log {
                                line: format!("user.ask.timeout : {mins} min"),
                            },
                        )
                        .await;
                        post_ask_timeout(&bus, &spec, mins).await;
                        report(
                            &bus,
                            &agent_id,
                            AgentOutputEvent::StateChanged {
                                state: AgentState::Running,
                            },
                        )
                        .await;
                        tool_result = format!(
                            "(aucune réponse après {mins} min — continue avec les infos disponibles ; ne repose pas la même question tout de suite)"
                        );
                        shared
                            .state
                            .lock()
                            .await
                            .push_tool("user.ask", &tool_result);
                    }
                    AskWait::Answer(answer) => {
                        report(
                            &bus,
                            &agent_id,
                            AgentOutputEvent::StateChanged {
                                state: AgentState::Running,
                            },
                        )
                        .await;
                        tool_result = format!("réponse utilisateur : {answer}");
                        shared
                            .state
                            .lock()
                            .await
                            .push_tool("user.ask", &tool_result);
                    }
                }
            }
            ActResult::Continue(outcome) if !multi_continue => {
                let mut outcome = outcome;
                if outcome.contains("permission")
                    || outcome.contains("PermissionDenied")
                    || outcome.contains("ActorDenied")
                    || outcome.contains("capacité requise")
                    || outcome.contains("capacité manquante")
                {
                    let canonical = canonicalize_tool_name(&action.action);
                    let hint = if canonical.starts_with("module.install") {
                        "module.install".to_string()
                    } else if canonical.starts_with("module.compile") {
                        "module.compile".to_string()
                    } else if canonical.starts_with("web.") || canonical.starts_with("net.")
                    {
                        "net.connect:*".to_string()
                    } else if canonical.starts_with("media.") {
                        "media.generate".to_string()
                    } else if canonical.starts_with("fs.") {
                        "fs.write:**".to_string()
                    } else if canonical.contains('.') {
                        let mod_name = canonical.split('.').next().unwrap_or("?");
                        format!("tool.invoke:{mod_name}")
                    } else {
                        "tool.invoke:*".to_string()
                    };
                    outcome.push_str(&format!(
                        "\n[hint] Essayez : TOOL: cap.request {{\"cap\":\"{hint}\",\"reason\":\"besoin pour {}\"}}",
                        action.action
                    ));
                }
                if canvas_tool_mutates_scene(&action.action) && canvas_op_succeeded(&outcome) {
                    if let Some(sid) = spec.session_id.as_deref().filter(|s| !s.is_empty()) {
                        let scene = refresh_canvas_scene_after_op(
                            &bus,
                            sid,
                            &outcome,
                        )
                        .await;
                        outcome = scene.text;
                        if let Some(png) = scene.png_path {
                            if let Some(current) = canvas_visual_fingerprint(&png) {
                                if let Some(previous) = last_canvas_visual {
                                    let progress = canvas_visual_progress(previous, current);
                                    outcome.push_str(&format!("\n\n{}", canvas_visual_progress_note(progress)));
                                    if !progress.meaningful_change {
                                        shared.state.lock().await.push_user(
                                            "[runtime] Le vérificateur visuel ne voit presque aucun changement. Ne répète pas cette forme : choisis une pièce distincte ou modifie une séquence existante.",
                                        );
                                    }
                                }
                                last_canvas_visual = Some(current);
                            } else {
                                outcome.push_str(
                                    "\n\n[canvas visual verifier] indisponible : PNG exporté inaccessible au worker.",
                                );
                            }
                            last_canvas_scene_png = Some(png);
                            canvas_scene_changed = true;
                        }
                    }
                }
                tool_result = outcome.clone();
                step_child_id = extract_child_id(&action.action, &outcome, &action.args);
                step_sources = collect_sources(&action.action, &action.args, &outcome);
                if action.action == "web.search" && !step_sources.is_empty() {
                    tool_result = format!(
                        "{} résultat(s) : {}",
                        step_sources.len(),
                        step_sources
                            .iter()
                            .take(3)
                            .map(|s| s.title.as_str())
                            .collect::<Vec<_>>()
                            .join(" · ")
                    );
                }
                if !outcome.is_empty() {
                    let mut st = shared.state.lock().await;
                    if canvas_tool_mutates_scene(&action.action) {
                        st.push_canvas_tool(&action.action, &outcome);
                    } else {
                        st.push_tool(&action.action, &outcome);
                    }
                    maybe_report_plan_advance_after_canvas_draw(
                        &bus,
                        &agent_id,
                        &mut st,
                        &action.action,
                        &outcome,
                    )
                    .await;
                }
            }
            ActResult::Continue(_) => {}
            ActResult::Complete(summary) => {
                tool_result = summary.clone();
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
                    terminal = Some(AgentState::Done);
                } else {
                    shared.state.lock().await.push_user(
                        "Le vérificateur estime que les critères ne sont pas remplis. Continue.",
                    );
                }
            }
            ActResult::Fail(reason) => {
                tool_result = reason.clone();
                step_fail_reason = Some(reason.clone());
                report(
                    &bus,
                    &agent_id,
                    AgentOutputEvent::Error {
                        message: reason.clone(),
                    },
                )
                .await;
                report(
                    &bus,
                    &agent_id,
                    AgentOutputEvent::Log {
                        line: format!("goal.fail : {reason}"),
                    },
                )
                .await;
                terminal = Some(AgentState::Failed);
            }
        }

        match loop_guard.observe(&action.action, &tool_result) {
            LoopVerdict::Ok => {}
            LoopVerdict::Warn(msg) => {
                report(
                    &bus,
                    &agent_id,
                    AgentOutputEvent::Log {
                        line: msg.clone(),
                    },
                )
                .await;
                shared
                    .state
                    .lock()
                    .await
                    .push_user(&format!("[runtime] {msg}"));
            }
            LoopVerdict::Abort(reason) => {
                step_fail_reason = Some(THREAD_FAIL_COULD_NOT_ACT.into());
                shared
                    .state
                    .lock()
                    .await
                    .artifacts
                    .push(THREAD_FAIL_COULD_NOT_ACT.into());
                report(
                    &bus,
                    &agent_id,
                    AgentOutputEvent::Error {
                        message: THREAD_FAIL_COULD_NOT_ACT.into(),
                    },
                )
                .await;
                report(
                    &bus,
                    &agent_id,
                    AgentOutputEvent::Log {
                        line: reason.clone(),
                    },
                )
                .await;
                shared
                    .state
                    .lock()
                    .await
                    .push_user(&format!("[runtime] {reason}"));
                terminal = Some(AgentState::Failed);
            }
        }

        // Independent whole-scene check every three canvas mutations. This is
        // deterministic and remains available when the selected model is text-only.
        let global_feedback = if terminal.is_none()
            && canvas_agent
            && global_canvas_validation_due(canvas_scene_changed, step)
        {
            if let Some(sid) = spec.session_id.as_deref().filter(|sid| !sid.is_empty()) {
                fetch_canvas_global_validation(&bus, sid, &spec.goal.statement, false)
                    .await
                    .filter(|report| !report.issues.is_empty())
                    .map(|report| report.prompt_block())
            } else {
                None
            }
        } else {
            None
        };
        if let Some(feedback) = &global_feedback {
            shared.state.lock().await.working_memory.push((
                "system".into(),
                format!("{feedback}\nCorrige les séquences signalées avant de poursuivre."),
            ));
            report(
                &bus,
                &agent_id,
                AgentOutputEvent::Reflection {
                    text: feedback.clone(),
                },
            )
            .await;
        }

        // Critic: after every canvas stroke (scene PNG) or every 3 steps otherwise.
        let model_reflection = if terminal.is_none()
            && should_run_canvas_critic(canvas_agent, canvas_scene_changed, step)
        {
            reflect(&bus, &shared, &spec).await
        } else {
            None
        };
        let reflection = match (global_feedback, model_reflection) {
            (Some(global), Some(model)) => Some(format!("{global}\n\n{model}")),
            (Some(global), None) => Some(global),
            (None, model) => model,
        };

        let skill_pairs: Vec<(String, Vec<String>)> = skill_docs
            .iter()
            .map(|s| (s.name.clone(), s.tools.clone()))
            .collect();
        let (tool_kind, mcp_server, skill) =
            classify_action(&action.action, &tools, &skill_pairs);
        let record = AgentStepRecord {
            step,
            thought: action.thought.clone(),
            response: full_text,
            action: action_log,
            args: action.args.clone(),
            tool_kind,
            mcp_server,
            skill,
            tool_result,
            reflection,
            duration_ms: step_t0.elapsed().as_millis() as u64,
            infer_ms,
            tool_ms,
            prompt_tokens: infer.prompt_tokens,
            generated_tokens: infer.generated_tokens,
            ttft_ms: infer.ttft_ms,
            tok_s: infer.tok_s,
            current_task: current_task.clone(),
            ts_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            fail_reason: step_fail_reason,
            child_id: step_child_id,
            sources: step_sources,
        };
        {
            let mut st = shared.state.lock().await;
            st.tokens_used += record.generated_tokens as u64;
            st.trace.push(record.clone());
            if let Some(verdict) = canvas_repeat_stroke_verdict(&st.trace, &action.action) {
                match verdict {
                    CanvasRepeatVerdict::Warn(msg) => {
                        st.push_user(&format!("[runtime] {msg}"));
                    }
                    CanvasRepeatVerdict::Abort(msg) => {
                        st.push_user(&format!("[runtime] {msg}"));
                        st.artifacts.push(msg.to_string());
                        drop(st);
                        report(
                            &bus,
                            &agent_id,
                            AgentOutputEvent::Log {
                                line: msg.to_string(),
                            },
                        )
                        .await;
                        terminal = Some(AgentState::Failed);
                    }
                }
            }
        }
        report(&bus, &agent_id, AgentOutputEvent::Step(record)).await;

        // A canvas plan is a bounded composition contract. Once its final
        // drawing stage is complete, stop before a text-only model starts
        // adding speculative layers over the finished composition.
        if terminal.is_none()
            && canvas_agent
            && (canvas_scene_changed || action.action == "canvas.export")
        {
            let plan_complete = shared.state.lock().await.canvas_plan_is_complete();
            if plan_complete {
                let final_validation = if let Some(sid) =
                    spec.session_id.as_deref().filter(|sid| !sid.is_empty())
                {
                    fetch_canvas_global_validation(
                        &bus,
                        sid,
                        &spec.goal.statement,
                        true,
                    )
                    .await
                } else {
                    None
                };
                if let Some(validation) =
                    final_validation.filter(|report| report.requires_modification())
                {
                    let feedback = format!(
                        "{}\nLe plan est terminé mais la cohérence globale exige une correction ciblée.",
                        validation.prompt_block()
                    );
                    shared
                        .state
                        .lock()
                        .await
                        .working_memory
                        .push(("system".into(), feedback.clone()));
                    report(
                        &bus,
                        &agent_id,
                        AgentOutputEvent::Reflection { text: feedback },
                    )
                    .await;
                } else {
                    report(
                        &bus,
                        &agent_id,
                        AgentOutputEvent::Log {
                            line: "plan canvas terminé et validation globale acceptée".into(),
                        },
                    )
                    .await;
                    shared
                        .state
                        .lock()
                        .await
                        .artifacts
                        .push("plan canvas terminé et validation globale acceptée".into());
                    terminal = Some(AgentState::Done);
                }
            }
        }

        // Checkpoint
        {
            let st = shared.state.lock().await;
            let _ = persist::write_state(&st);
            let _ = persist::write_spec(&spec);
        }
        if terminal.is_some() {
            break;
        }
    }

    let final_state = terminal.unwrap_or(AgentState::Done);
    notify_parent_if_child(&bus, &spec, &shared, &final_state).await;
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
    Text(InferTurn),
    Aborted,
    Fatal(String),
    Steer(String),
}

enum InferControl {
    Continue,
    Fail,
}

struct InferTurn {
    text: String,
    prompt_tokens: u32,
    generated_tokens: u32,
    ttft_ms: f64,
    tok_s: f64,
}

async fn infer_turn(
    bus: &BusClient,
    shared: &Shared,
    spec: &AgentSpec,
    data_refs: &[String],
    cmd_rx: &mut mpsc::Receiver<WorkerCmd>,
    max_tokens: u32,
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
    let images: Vec<String> = data_refs
        .iter()
        .filter(|p| {
            let lower = p.to_ascii_lowercase();
            lower.ends_with(".png")
                || lower.ends_with(".jpg")
                || lower.ends_with(".jpeg")
                || lower.ends_with(".webp")
        })
        .take(4).cloned()
        .collect();
    let req = InferRequest {
        model_id: spec.model_id.clone(),
        messages,
        params: InferParams {
            temperature: 0.2,
            max_tokens,
            ..InferParams::default()
        },
        priority: 1,
        data_refs: data_refs.to_vec(),
        images,
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
    let mut generated_fallback = 0u32;
    let mut done_stats: Option<(u32, u32, f64, f64)> = None;
    let mut token_buf = String::new();
    let mut last_token_flush = Instant::now();
    let infer_deadline = Duration::from_secs(180);
    let started_infer = Instant::now();

    while let Some(ev) = rx.recv().await {
        if started_infer.elapsed() > infer_deadline {
            if !token_buf.is_empty() {
                report(
                    bus,
                    &spec.agent_id,
                    AgentOutputEvent::Token {
                        text: std::mem::take(&mut token_buf),
                    },
                )
                .await;
            }
            return InferOutcome::Fatal(format!(
                "timeout inférence ({} s) — le modèle ou le bus ne répond plus",
                infer_deadline.as_secs()
            ));
        }
        match ev {
            Ok(TokenEvent::Started { inference_id }) => {
                *shared.current_inference.lock().await = Some(inference_id);
            }
            Ok(TokenEvent::Delta { text }) => {
                if shared.paused.load(Ordering::SeqCst) {
                    if !token_buf.is_empty() {
                        report(
                            bus,
                            &spec.agent_id,
                            AgentOutputEvent::Token {
                                text: std::mem::take(&mut token_buf),
                            },
                        )
                        .await;
                    }
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
                        Some(WorkerCmd::ActDecision { .. }) => return InferOutcome::Aborted,
                        Some(WorkerCmd::ChildFinished {
                            child_id,
                            result,
                            ok,
                        }) => {
                            record_child_finished(shared, child_id, result, ok).await;
                            loop {
                                match cmd_rx.recv().await {
                                    Some(WorkerCmd::Resume) => return InferOutcome::Aborted,
                                    Some(WorkerCmd::Steer(d)) => return InferOutcome::Steer(d),
                                    Some(WorkerCmd::ActDecision { .. }) => {
                                        return InferOutcome::Aborted
                                    }
                                    Some(WorkerCmd::ChildFinished {
                                        child_id,
                                        result,
                                        ok,
                                    }) => {
                                        record_child_finished(shared, child_id, result, ok).await;
                                    }
                                    None => return InferOutcome::Fatal("control fermé".into()),
                                }
                            }
                        }
                        None => return InferOutcome::Fatal("control fermé".into()),
                    }
                }
                full_text.push_str(&text);
                generated_fallback += 1;
                token_buf.push_str(&text);
                if token_buf.len() >= 96 || last_token_flush.elapsed() >= Duration::from_millis(80)
                {
                    report(
                        bus,
                        &spec.agent_id,
                        AgentOutputEvent::Token {
                            text: std::mem::take(&mut token_buf),
                        },
                    )
                    .await;
                    last_token_flush = Instant::now();
                }
            }
            Ok(TokenEvent::Done {
                prompt_tokens,
                generated_tokens,
                ttft_ms,
                tok_s,
            }) => {
                done_stats = Some((prompt_tokens, generated_tokens, ttft_ms, tok_s));
            }
            Ok(TokenEvent::Error { message }) => {
                if !token_buf.is_empty() {
                    report(
                        bus,
                        &spec.agent_id,
                        AgentOutputEvent::Token {
                            text: std::mem::take(&mut token_buf),
                        },
                    )
                    .await;
                }
                return InferOutcome::Fatal(message);
            }
            Err(e) => {
                if !token_buf.is_empty() {
                    report(
                        bus,
                        &spec.agent_id,
                        AgentOutputEvent::Token {
                            text: std::mem::take(&mut token_buf),
                        },
                    )
                    .await;
                }
                return InferOutcome::Fatal(e.to_string());
            }
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
    if !token_buf.is_empty() {
        report(
            bus,
            &spec.agent_id,
            AgentOutputEvent::Token {
                text: std::mem::take(&mut token_buf),
            },
        )
        .await;
    }
    *shared.current_inference.lock().await = None;
    let (prompt_tokens, generated_tokens, ttft_ms, tok_s) =
        done_stats.unwrap_or((0, generated_fallback, 0.0, 0.0));
    InferOutcome::Text(InferTurn {
        text: full_text,
        prompt_tokens,
        generated_tokens,
        ttft_ms,
        tok_s,
    })
}

enum ActResult {
    Continue(String),
    Complete(String),
    Fail(String),
    /// Pause jusqu'à une réponse utilisateur (`user.ask`).
    AskUser {
        question: String,
        choices: Vec<String>,
    },
}

async fn execute_action(
    bus: &BusClient,
    shared: &Shared,
    spec: &mut AgentSpec,
    tools: &[ToolDesc],
    skills: &[SkillDoc],
    mcp_sessions: &mut HashMap<String, McpSession>,
    action: &AgentAction,
) -> ActResult {
    let canonical = canonicalize_tool_name(&action.action);
    let name = canonical.as_str();
    let args_owned = normalize_tool_args(name, &action.args);
    let args = &args_owned;
    // Skill name used as tool (research, file.author, …) → correction claire
    if tools.iter().all(|t| t.name != name) {
        if let Some(skill) = match_skill_by_action(name, skills) {
            return ActResult::Continue(skill_misuse_hint(name, skill));
        }
    }

    // Gate : plan obligatoire tant que task_graph / deep plan vide
    {
        let st = shared.state.lock().await;
        if st.blocks_action(name) {
            let msg = if st.deep_thinking {
                "plan requis: appelle plan.create avec un arbre d'étapes avant toute autre action \
                 (mode Deep Thinking). goal.fail reste autorisé. \
                 Exemple: {\"thought\":\"découper\",\"action\":\"plan.create\",\"args\":{\"steps\":[{\"id\":\"1\",\"label\":\"…\"}]}}"
            } else {
                "plan requis: appelle plan.update avec des nœuds atomiques avant toute autre action \
                 (task.assess = complex). goal.fail reste autorisé. \
                 Exemple: {\"thought\":\"découper\",\"action\":\"plan.update\",\"args\":{\"nodes\":[{\"id\":\"1\",\"title\":\"…\",\"status\":\"Pending\"}]}}"
            };
            return ActResult::Continue(msg.into());
        }
    }

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
            "aucune action JSON détectée (souvent tronquée). \
             Réponds uniquement par {\"thought\":\"…\",\"action\":\"<outil>\",\"args\":{…}}. \
             Note longue : notes.create (titre + outline court) puis notes.update par sections \
             (≤ ~1200 car. de content)."
                .into(),
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
        "user.ask" => {
            let question = args
                .get("question")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if let Some(embedded) = parse_embedded_action_question(&question) {
                return ActResult::Continue(format!(
                    "user.ask incorrect : exécute directement \
                     {{\"action\":\"{}\",\"args\":{}}} — pas de question à l'humain.",
                    embedded.action, embedded.args
                ));
            }
            if agent_has_canvas_tools(&spec.tools) {
                return ActResult::Continue(
                    "canvas : n'utilise pas user.ask — exécute l'outil directement \
                     (ex. {\"action\":\"canvas.set_style\",\"args\":{\"color\":\"#8D6E63\"}}). \
                     Couleur, style et coords ne demandent jamais l'humain."
                        .into(),
                );
            }
            if question.is_empty() {
                ActResult::Continue("user.ask : question vide".into())
            } else {
                let choices: Vec<String> = args
                    .get("choices")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                            .filter(|s| !s.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();
                ActResult::AskUser { question, choices }
            }
        }
        "plan.update" => {
            let requested_nodes = parse_plan_nodes(args);
            let (nodes, need_mem) = {
                let mut st = shared.state.lock().await;
                // The author supplies geometry; the runtime supplies a stable
                // composition budget. Do this only for the first canvas plan so
                // an intentional later re-plan is still possible.
                let nodes = if agent_has_canvas_tools(&spec.tools) && st.task_graph.is_empty() {
                    CognitiveState::canonical_canvas_composition_plan()
                } else {
                    requested_nodes
                };
                st.set_plan(nodes.clone());
                let need = st.needs_plan && !st.plan_memory_recalled;
                if need {
                    st.plan_memory_recalled = true;
                }
                (nodes, need)
            };
            report(
                bus,
                &agent_id,
                AgentOutputEvent::PlanUpdated {
                    nodes: nodes.clone(),
                },
            )
            .await;
            if need_mem {
                let query = nodes
                    .iter()
                    .find(|n| n.status == TaskNodeStatus::Pending || n.status == TaskNodeStatus::Running)
                    .map(|n| n.title.clone())
                    .or_else(|| nodes.first().map(|n| n.title.clone()))
                    .unwrap_or_else(|| spec.goal.statement.clone());
                bootstrap_memory_recall(bus, shared, &agent_id, &query, "après plan.update").await;
            }
            ActResult::Continue(format!("plan mis à jour ({} nœuds)", nodes.len()))
        }
        "plan.create" => {
            handle_deep_plan_create(bus, shared, spec, args).await
        }
        "plan.update_step" => {
            handle_deep_plan_update_step(bus, shared, spec, args).await
        }
        "plan.replace_tree" => {
            handle_deep_plan_replace_tree(bus, shared, spec, args).await
        }
        "plan.delegate_step" => {
            handle_deep_plan_delegate(bus, shared, spec, args).await
        }
        "plan.get" => handle_deep_plan_get(bus, shared, spec, args).await,
        "plan.append_log" => {
            handle_deep_plan_append_log(bus, shared, spec, args).await
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
                .call::<MemEpisodicWriteRequest, MemRememberResponse>(
                    "mem.episodic_write",
                    &MemEpisodicWriteRequest {
                        namespace: format!("agent:{agent_id}"),
                        text: text.clone(),
                        metadata: serde_json::json!({}),
                        pinned: false,
                        auto_link: true,
                        ..Default::default()
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
            let block = recall_memory_bundle(bus, &agent_id, &query, 5).await;
            ActResult::Continue(block)
        }
        "agent.spawn" => {
            if agent_has_canvas_tools(&spec.tools) {
                let hint = if aos_agent::canvas_scene::agent_has_canvas_path(&spec.tools) {
                    "canvas.path/stroke/rect/ellipse/…"
                } else {
                    "canvas.stroke/spline/rect/ellipse/…"
                };
                return ActResult::Continue(format!(
                    "canvas : dessine toi-même avec {hint} — \
                     ne spawn pas des sous-agents pour le même dessin (un seul auteur, traits séquentiels)."
                ));
            }
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
            let brief = clamp_spawn_brief(
                args.get("brief").and_then(|v| v.as_str()).unwrap_or(""),
            );
            if brief.is_empty() {
                return ActResult::Continue("brief sous-agent vide".into());
            }
            let child_skills: Vec<String> = args
                .get("skills")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let child_tools: Vec<String> = args
                .get("tools")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_else(|| spec.tools.clone());
            // Docs explicites uniquement (pas d'héritage parent) — max 3 pour limiter le contexte enfant
            let child_docs: Vec<DocumentRef> = args
                .get("documents")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let child_docs: Vec<DocumentRef> = child_docs.into_iter().take(3).collect();
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
                .trim()
                .to_string();
            let my_children = shared.state.lock().await.children.clone();
            if let Some(msg) = await_child_reject_reason(&child_id, &my_children) {
                return ActResult::Continue(msg);
            }
            let key = CognitiveState::child_shared_mem_key(&agent_id, &child_id);
            if let Some(result) = take_awaited_child_result(shared, bus, &agent_id, &child_id).await
            {
                return ActResult::Continue(result);
            }
            let mut seen_in_list = false;
            // Poll shared mem / agent state (~30s). Never Block: a missing or
            // crashed child must not freeze the parent for a human Resume.
            for i in 0..60 {
                if let Some(result) =
                    take_awaited_child_result(shared, bus, &agent_id, &child_id).await
                {
                    return ActResult::Continue(result);
                }
                if let Ok(Some(val)) = bus
                    .call::<MemSharedReadRequest, Option<serde_json::Value>>(
                        "mem.shared_read",
                        &MemSharedReadRequest { name: key.clone() },
                        vec![],
                    )
                    .await
                {
                    return ActResult::Continue(
                        finish_agent_await(bus, shared, &agent_id, child_id, val.to_string())
                            .await,
                    );
                }
                if let Ok(list) = bus
                    .call::<(), Vec<AgentInfo>>("agent.list", &(), vec![])
                    .await
                {
                    if let Some(info) = list.iter().find(|a| a.agent_id == child_id) {
                        seen_in_list = true;
                        if matches!(
                            info.state,
                            AgentState::Done | AgentState::Failed | AgentState::Killed
                        ) {
                            let result = if info.last_output.is_empty() {
                                format!("{} ({:?})", child_id, info.state)
                            } else {
                                info.last_output.clone()
                            };
                            return ActResult::Continue(
                                finish_agent_await(bus, shared, &agent_id, child_id, result).await,
                            );
                        }
                    } else if i >= 3 && !seen_in_list {
                        return ActResult::Continue(format!(
                            "agent.await: {child_id} introuvable dans agent.list \
                             (lancement probablement échoué) — spawn à nouveau ou continue sans lui"
                        ));
                    }
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            ActResult::Continue(format!(
                "agent.await: {child_id} toujours en cours après 30s — \
                 réessaie agent.await ou poursuis sans lui"
            ))
        }
        other => {
            let backend = resolve_tool_backend(other, tools);
            match backend {
                Some(ToolBackend::Module) => {
                    let outcome = invoke_module(
                        bus,
                        &agent_id,
                        &caps,
                        other,
                        args,
                        &trace_id,
                        spec.session_id.as_deref(),
                    )
                    .await;
                    ActResult::Continue(outcome)
                }
                None if canvas_tool_denied_by_allowlist(other, tools) => {
                    ActResult::Continue(format!(
                        "outil canvas non autorisé: {other}. Utilise uniquement les outils fournis ; \
                         pour remplir une silhouette, passe `fill:true` à canvas.path/rect/ellipse."
                    ))
                }
                None if is_module_fallback_candidate(other) => {
                    let outcome = invoke_module(
                        bus,
                        &agent_id,
                        &caps,
                        other,
                        args,
                        &trace_id,
                        spec.session_id.as_deref(),
                    )
                    .await;
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
                None => ActResult::Continue(format!(
                    "outil inconnu: {other} — ce n'est pas un module WASM. \
                     TTS : media.audio.generate {{\"text\":\"...\"}} ; \
                     image : media.image.generate {{\"prompt\":\"...\"}}."
                )),
            }
        }
    }
}

fn format_user_question(question: &str, choices: &[String]) -> String {
    if choices.is_empty() {
        question.to_string()
    } else {
        let opts: String = choices.iter().map(|c| format!("\n- {c}")).collect();
        format!("{question}\n\nChoix possibles :{opts}")
    }
}

fn ask_heading(spec: &AgentSpec) -> String {
    let title = spec.goal.statement.trim();
    let title: String = if title.is_empty() {
        spec.agent_id.clone()
    } else {
        title.chars().take(80).collect()
    };
    format!("**Question — {title}**")
}

async fn post_user_question(bus: &BusClient, spec: &AgentSpec, body: &str) {
    let Some(session_id) = spec.session_id.clone() else {
        return;
    };
    let content = format!("{}\n\n{body}", ask_heading(spec));
    let _ = bus
        .call::<ChatSessionAppendRequest, aos_proto::ChatSessionMessage>(
            "chat.session.append",
            &ChatSessionAppendRequest {
                session_id,
                role: "assistant".into(),
                content,
                attachments: vec![ChatAttachment::AgentRef {
                    agent_id: spec.agent_id.clone(),
                    title: spec.goal.statement.clone(),
                    origin: "ask".into(),
                }],
                speaker_id: None,
                speaker_name: None,
                thinking: None,
            },
            vec![],
        )
        .await;
}

async fn post_ask_timeout(bus: &BusClient, spec: &AgentSpec, mins: u64) {
    let Some(session_id) = spec.session_id.clone() else {
        return;
    };
    let content = format!(
        "**Question expirée** ({mins} min) — l'agent continue sans réponse."
    );
    let _ = bus
        .call::<ChatSessionAppendRequest, aos_proto::ChatSessionMessage>(
            "chat.session.append",
            &ChatSessionAppendRequest {
                session_id,
                role: "assistant".into(),
                content,
                attachments: vec![ChatAttachment::AgentRef {
                    agent_id: spec.agent_id.clone(),
                    title: spec.goal.statement.clone(),
                    origin: "ask-timeout".into(),
                }],
                speaker_id: None,
                speaker_name: None,
                thinking: None,
            },
            vec![],
        )
        .await;
}

async fn wait_user_answer(
    shared: &Shared,
    cmd_rx: &mut mpsc::Receiver<WorkerCmd>,
    limit: Duration,
) -> AskWait {
    let deadline = tokio::time::Instant::now() + limit;
    let waited_secs = limit.as_secs();
    while shared.paused.load(Ordering::SeqCst) {
        match tokio::time::timeout_at(deadline, cmd_rx.recv()).await {
            Ok(Some(WorkerCmd::Steer(d))) => {
                shared.paused.store(false, Ordering::SeqCst);
                return AskWait::Answer(d.trim().to_string());
            }
            Ok(Some(WorkerCmd::Resume)) => {
                shared.paused.store(false, Ordering::SeqCst);
                return AskWait::Answer(
                    "(l'utilisateur a repris sans répondre — continue avec les infos disponibles)"
                        .into(),
                );
            }
            Ok(Some(WorkerCmd::ActDecision { .. })) => {}
            Ok(Some(WorkerCmd::ChildFinished {
                child_id,
                result,
                ok,
            })) => {
                record_child_finished(shared, child_id, result, ok).await;
            }
            Ok(None) => return AskWait::Killed,
            Err(_) => {
                shared.paused.store(false, Ordering::SeqCst);
                return AskWait::Timeout { waited_secs };
            }
        }
    }
    AskWait::Answer(String::new())
}

fn should_gate_action(spec: &AgentSpec, action: &str) -> bool {
    spec.session_id.is_some()
        && aos_agent::agent_act::AgentGateMode::parse(&spec.gate_mode)
            == aos_agent::agent_act::AgentGateMode::Ask
        && aos_agent::agent_act::requires_act_gate(action)
}

enum GateWait {
    Proceed,
    Denied,
    Killed,
}

fn next_act_id(agent_id: &str) -> String {
    format!(
        "act-{}-{}",
        agent_id,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    )
}

async fn post_agent_act(bus: &BusClient, spec: &AgentSpec, act_id: &str, action: &AgentAction) {
    let Some(session_id) = spec.session_id.clone() else {
        return;
    };
    let _ = bus
        .call::<ChatSessionAppendRequest, aos_proto::ChatSessionMessage>(
            "chat.session.append",
            &ChatSessionAppendRequest {
                session_id,
                role: "assistant".into(),
                content: String::new(),
                attachments: vec![ChatAttachment::AgentAct {
                    agent_id: spec.agent_id.clone(),
                    act_id: act_id.to_string(),
                    phrase: String::new(),
                    action: action.action.clone(),
                    args: action.args.clone(),
                    state: "pending".into(),
                }],
                speaker_id: None,
                speaker_name: None,
                thinking: None,
            },
            vec![],
        )
        .await;
}

async fn post_act_resolved(
    bus: &BusClient,
    spec: &AgentSpec,
    act_id: &str,
    action: &AgentAction,
    approved: bool,
) {
    let Some(session_id) = spec.session_id.clone() else {
        return;
    };
    let state = if approved { "approved" } else { "denied" };
    let _ = bus
        .call::<ChatSessionAppendRequest, aos_proto::ChatSessionMessage>(
            "chat.session.append",
            &ChatSessionAppendRequest {
                session_id,
                role: "assistant".into(),
                content: String::new(),
                attachments: vec![ChatAttachment::AgentAct {
                    agent_id: spec.agent_id.clone(),
                    act_id: act_id.to_string(),
                    phrase: String::new(),
                    action: action.action.clone(),
                    args: action.args.clone(),
                    state: state.into(),
                }],
                speaker_id: None,
                speaker_name: None,
                thinking: None,
            },
            vec![],
        )
        .await;
}

async fn wait_act_decision(
    shared: &Shared,
    cmd_rx: &mut mpsc::Receiver<WorkerCmd>,
    act_id: &str,
    limit: Duration,
) -> GateWait {
    let deadline = tokio::time::Instant::now() + limit;
    loop {
        match tokio::time::timeout_at(deadline, cmd_rx.recv()).await {
            Ok(Some(WorkerCmd::ActDecision {
                act_id: id,
                approved,
            })) if id == act_id => {
                shared.paused.store(false, Ordering::SeqCst);
                return if approved {
                    GateWait::Proceed
                } else {
                    GateWait::Denied
                };
            }
            Ok(Some(WorkerCmd::ActDecision { .. })) => {}
            Ok(Some(WorkerCmd::Resume)) => {
                shared.paused.store(false, Ordering::SeqCst);
                return GateWait::Denied;
            }
            Ok(Some(WorkerCmd::Steer(_))) => {}
            Ok(Some(WorkerCmd::ChildFinished {
                child_id,
                result,
                ok,
            })) => {
                record_child_finished(shared, child_id, result, ok).await;
            }
            Ok(None) => return GateWait::Killed,
            Err(_) => {
                shared.paused.store(false, Ordering::SeqCst);
                return GateWait::Denied;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)] // Gate state is explicit to keep authorization fail-closed.
async fn gate_action(
    bus: &BusClient,
    shared: &Shared,
    cmd_rx: &mut mpsc::Receiver<WorkerCmd>,
    spec: &AgentSpec,
    action: &AgentAction,
    agent_id: &str,
    timeout: Duration,
    started: Instant,
) -> GateWait {
    let act_id = next_act_id(agent_id);
    post_agent_act(bus, spec, &act_id, action).await;
    shared.paused.store(true, Ordering::SeqCst);
    report(
        bus,
        agent_id,
        AgentOutputEvent::StateChanged {
            state: AgentState::Blocked,
        },
    )
    .await;
    let remaining = timeout.saturating_sub(started.elapsed());
    let wait_limit = Duration::from_secs(300).min(remaining.max(Duration::from_secs(30)));
    let outcome = wait_act_decision(shared, cmd_rx, &act_id, wait_limit).await;
    post_act_resolved(
        bus,
        spec,
        &act_id,
        action,
        matches!(outcome, GateWait::Proceed),
    )
    .await;
    if matches!(outcome, GateWait::Proceed | GateWait::Denied) {
        report(
            bus,
            agent_id,
            AgentOutputEvent::StateChanged {
                state: AgentState::Running,
            },
        )
        .await;
    }
    outcome
}

/// Canvas delegates keep the user's subject as the child goal, not designer examples.
fn canvas_child_goal_statement(parent: &AgentSpec, brief: &str) -> String {
    let is_canvas_parent = parent.tools.iter().any(|t| t.starts_with("canvas."));
    if !is_canvas_parent {
        return brief.to_string();
    }
    let user_goal = parent.goal.statement.trim();
    if user_goal.is_empty() {
        brief.to_string()
    } else {
        user_goal.to_string()
    }
}

#[allow(clippy::too_many_arguments)] // Child inheritance inputs stay explicit at the spawn boundary.
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
    let brief = clamp_spawn_brief(brief);
    let child_goal_statement = canvas_child_goal_statement(parent, &brief);
    let req = AgentCreateRequest {
        directive: brief.clone(),
        kind: Default::default(),
        display_name: None,
        persona_id: None,
        caps: caps.to_vec(),
        model_id: parent.model_id.clone(),
        goal: Some(AgentGoal {
            statement: child_goal_statement,
            success_criteria: vec![
                "Résultat clair et concis (≤ ~800 caractères utiles)".into(),
            ],
            max_steps: (parent.goal.max_steps / 2).clamp(24, 64),
            max_subagents: 0,
            timeout_secs: parent.goal.timeout_secs.min(1800),
        }),
        system_prompt: Some(
            "Sous-agent : exécute UNIQUEMENT ce brief. Ne redis pas le contexte parent. \
             Réponds de façon concise ; pas de dump d'historique."
                .into(),
        ),
        skills: skills.to_vec(),
        tools: tools.to_vec(),
        mcp_servers: vec![],
        documents: documents.to_vec(),
        parent_id: Some(parent.agent_id.clone()),
        session_id: parent.session_id.clone(),
        budget: parent.budget.clone(),
        optimize_prompt: false,
        gate_mode: parent.gate_mode.clone(),
        origin: None,
    cognitive_mode: aos_proto::CognitiveMode::Normal,
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
                    brief: brief.clone(),
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

async fn maybe_report_plan_advance_after_canvas_draw(
    bus: &BusClient,
    agent_id: &str,
    st: &mut aos_agent::CognitiveState,
    action: &str,
    outcome: &str,
) {
    if !st.maybe_advance_plan_after_canvas_draw(action, outcome) {
        return;
    }
    let nodes = st.task_graph.clone();
    report(
        bus,
        agent_id,
        AgentOutputEvent::PlanUpdated { nodes },
    )
    .await;
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

fn parse_plan_step_status(raw: &str) -> PlanStepStatus {
    match raw.trim().to_ascii_lowercase().as_str() {
        "in_progress" | "inprogress" | "running" => PlanStepStatus::InProgress,
        "done" | "complete" | "completed" => PlanStepStatus::Done,
        "delegated" => PlanStepStatus::Delegated,
        "blocked" => PlanStepStatus::Blocked,
        _ => PlanStepStatus::Pending,
    }
}

fn parse_deep_steps(args: &serde_json::Value) -> Vec<PlanStep> {
    let Some(arr) = args.get("steps").and_then(|n| n.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .enumerate()
        .map(|(i, n)| parse_deep_step(n, &format!("{}", i + 1)))
        .collect()
}

fn parse_deep_step(n: &serde_json::Value, fallback_id: &str) -> PlanStep {
    let id = n
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or(fallback_id)
        .to_string();
    let label = n
        .get("label")
        .or_else(|| n.get("title"))
        .and_then(|v| v.as_str())
        .unwrap_or("étape")
        .to_string();
    let children = n
        .get("children")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .enumerate()
                .map(|(j, c)| parse_deep_step(c, &format!("{id}.{}", j + 1)))
                .collect()
        })
        .unwrap_or_default();
    PlanStep {
        id,
        label,
        description: n
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        status: n
            .get("status")
            .and_then(|v| v.as_str())
            .map(parse_plan_step_status)
            .unwrap_or_default(),
        agent_id: n
            .get("agent_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        children,
        logs: n
            .get("logs")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

async fn report_deep_plan(
    bus: &BusClient,
    agent_id: &str,
    plan: aos_proto::DeepPlan,
    trace: &str,
) {
    report(
        bus,
        agent_id,
        AgentOutputEvent::DeepPlanUpdated {
            plan: plan.clone(),
        },
    )
    .await;
    if !trace.trim().is_empty() {
        report(
            bus,
            agent_id,
            AgentOutputEvent::DeepTrace {
                message: trace.to_string(),
            },
        )
        .await;
    }
}

async fn resolve_plan_id(shared: &Shared, args: &serde_json::Value) -> Option<String> {
    if let Some(pid) = args.get("plan_id").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        return Some(pid.to_string());
    }
    let st = shared.state.lock().await;
    st.deep_plan_id.clone()
}

async fn handle_deep_plan_create(
    bus: &BusClient,
    shared: &Shared,
    spec: &AgentSpec,
    args: &serde_json::Value,
) -> ActResult {
    let steps = parse_deep_steps(args);
    let task = args
        .get("task")
        .and_then(|v| v.as_str())
        .unwrap_or(spec.goal.statement.as_str())
        .to_string();
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let req = PlanCreateRequest {
        agent_id: spec.agent_id.clone(),
        task,
        title,
        steps,
    };
    match bus
        .call::<PlanCreateRequest, PlanResponse>("plan.create", &req, vec![])
        .await
    {
        Ok(resp) => {
            {
                let mut st = shared.state.lock().await;
                st.deep_plan_id = Some(resp.plan.id.clone());
                st.deep_thinking = true;
                if !st.plan_memory_recalled {
                    st.plan_memory_recalled = true;
                }
            }
            let trace = format!(
                "Deep Thinking : plan créé (v{}, {} étapes racines).",
                resp.plan.version,
                resp.plan.steps.len()
            );
            report_deep_plan(bus, &spec.agent_id, resp.plan.clone(), &trace).await;
            bootstrap_memory_recall(
                bus,
                shared,
                &spec.agent_id,
                &spec.goal.statement,
                "après plan.create",
            )
            .await;
            ActResult::Continue(format!(
                "plan créé id={} v{} ({} racines)",
                resp.plan.id,
                resp.plan.version,
                resp.plan.steps.len()
            ))
        }
        Err(e) => ActResult::Continue(format!("plan.create err: {e}")),
    }
}

async fn handle_deep_plan_update_step(
    bus: &BusClient,
    shared: &Shared,
    spec: &AgentSpec,
    args: &serde_json::Value,
) -> ActResult {
    let Some(plan_id) = resolve_plan_id(shared, args).await else {
        return ActResult::Continue("plan.update_step : plan_id manquant (appelle plan.create d'abord)".into());
    };
    let step_id = args
        .get("step_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if step_id.is_empty() {
        return ActResult::Continue("plan.update_step : step_id requis".into());
    }
    let patch = DeepPlanStepPatch {
        status: args
            .get("status")
            .and_then(|v| v.as_str())
            .map(parse_plan_step_status),
        label: args
            .get("label")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        description: args
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        logs: args
            .get("logs")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        agent_id: None,
    };
    let req = PlanUpdateStepRequest {
        plan_id,
        step_id,
        patch,
    };
    match bus
        .call::<PlanUpdateStepRequest, PlanResponse>("plan.update_step", &req, vec![])
        .await
    {
        Ok(resp) => {
            let trace = format!(
                "Deep Thinking : plan mis à jour (v{}).",
                resp.plan.version
            );
            report_deep_plan(bus, &spec.agent_id, resp.plan, &trace).await;
            ActResult::Continue(trace)
        }
        Err(e) => ActResult::Continue(format!("plan.update_step err: {e}")),
    }
}

async fn handle_deep_plan_replace_tree(
    bus: &BusClient,
    shared: &Shared,
    spec: &AgentSpec,
    args: &serde_json::Value,
) -> ActResult {
    let Some(plan_id) = resolve_plan_id(shared, args).await else {
        return ActResult::Continue("plan.replace_tree : plan_id manquant".into());
    };
    let steps = parse_deep_steps(args);
    if steps.is_empty() {
        return ActResult::Continue("plan.replace_tree : steps requis".into());
    }
    let req = PlanReplaceTreeRequest {
        plan_id,
        steps,
        title: args
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        status: None,
    };
    match bus
        .call::<PlanReplaceTreeRequest, PlanResponse>("plan.replace_tree", &req, vec![])
        .await
    {
        Ok(resp) => {
            let trace = format!(
                "Deep Thinking : plan révisé (v{}, {} racines).",
                resp.plan.version,
                resp.plan.steps.len()
            );
            report_deep_plan(bus, &spec.agent_id, resp.plan, &trace).await;
            ActResult::Continue(trace)
        }
        Err(e) => ActResult::Continue(format!("plan.replace_tree err: {e}")),
    }
}

async fn handle_deep_plan_delegate(
    bus: &BusClient,
    shared: &Shared,
    spec: &mut AgentSpec,
    args: &serde_json::Value,
) -> ActResult {
    let Some(plan_id) = resolve_plan_id(shared, args).await else {
        return ActResult::Continue("plan.delegate_step : plan_id manquant".into());
    };
    let step_id = args
        .get("step_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let brief = args
        .get("brief")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if step_id.is_empty() || brief.is_empty() {
        return ActResult::Continue("plan.delegate_step : step_id et brief requis".into());
    }
    let skills: Vec<String> = args
        .get("skills")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let tools: Vec<String> = args
        .get("tools")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let documents: Vec<DocumentRef> = args
        .get("documents")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let spawn = spawn_child(bus, shared, spec, &brief, &skills, &tools, &documents, &[]).await;
    let ActResult::Continue(msg) = &spawn else {
        return spawn;
    };
    let Some(child_id) = msg.strip_prefix("sous-agent créé: ").map(|s| s.trim().to_string()) else {
        return ActResult::Continue(format!("délégation : spawn a échoué ({msg})"));
    };
    let req = PlanDelegateStepRequest {
        plan_id,
        step_id: step_id.clone(),
        child_id: child_id.clone(),
        brief: Some(brief),
    };
    match bus
        .call::<PlanDelegateStepRequest, PlanResponse>("plan.delegate_step", &req, vec![])
        .await
    {
        Ok(resp) => {
            let trace = format!("Sous-agent lancé pour l'étape {step_id} ({child_id}).");
            report_deep_plan(bus, &spec.agent_id, resp.plan, &trace).await;
            ActResult::Continue(format!("{msg} ; étape {step_id} déléguée"))
        }
        Err(e) => ActResult::Continue(format!("{msg} ; plan.delegate_step err: {e}")),
    }
}

async fn handle_deep_plan_get(
    bus: &BusClient,
    shared: &Shared,
    spec: &AgentSpec,
    args: &serde_json::Value,
) -> ActResult {
    let plan_id = resolve_plan_id(shared, args).await;
    let req = PlanGetRequest {
        plan_id,
        agent_id: Some(spec.agent_id.clone()),
    };
    match bus
        .call::<PlanGetRequest, PlanResponse>("plan.get", &req, vec![])
        .await
    {
        Ok(resp) => {
            let summary = aos_agent::deep_thinking::light_plan_summary(&resp.plan);
            ActResult::Continue(truncate(&summary, 4000))
        }
        Err(e) => ActResult::Continue(format!("plan.get err: {e}")),
    }
}

async fn handle_deep_plan_append_log(
    bus: &BusClient,
    shared: &Shared,
    spec: &AgentSpec,
    args: &serde_json::Value,
) -> ActResult {
    let Some(plan_id) = resolve_plan_id(shared, args).await else {
        return ActResult::Continue("plan.append_log : plan_id manquant".into());
    };
    let step_id = args
        .get("step_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let line = args
        .get("line")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if step_id.is_empty() || line.is_empty() {
        return ActResult::Continue("plan.append_log : step_id et line requis".into());
    }
    let req = PlanAppendLogRequest {
        plan_id,
        step_id,
        line,
    };
    match bus
        .call::<PlanAppendLogRequest, PlanResponse>("plan.append_log", &req, vec![])
        .await
    {
        Ok(resp) => {
            report(
                bus,
                &spec.agent_id,
                AgentOutputEvent::DeepPlanUpdated { plan: resp.plan },
            )
            .await;
            ActResult::Continue("log interne ajouté".into())
        }
        Err(e) => ActResult::Continue(format!("plan.append_log err: {e}")),
    }
}

async fn invoke_module(
    bus: &BusClient,
    agent_id: &str,
    caps: &[String],
    tool: &str,
    args: &serde_json::Value,
    trace_id: &str,
    session_id: Option<&str>,
) -> String {
    let module = tool.split('.').next().unwrap_or("").to_string();
    let mut args = args.clone();
    if module == "canvas" {
        // Fail closed: canvas.* requires a bound session_id; reject calls when none is available.
        let sid = match session_id.filter(|s| !s.is_empty()) {
            Some(s) => s,
            None => return "ERREUR outil: canvas.* requiert un session_id lié".to_string(),
        };
        // Always overwrite with the bound session — never trust model-supplied ids.
        let orig = args.clone();
        args = serde_json::json!({});
        if let Some(obj) = args.as_object_mut() {
            // Merge original args first so tool params are preserved.
            if let Some(orig_obj) = orig.as_object() {
                for (k, v) in orig_obj {
                    obj.insert(k.clone(), v.clone());
                }
            }
            // Then overwrite session_id and author_id unconditionally.
            obj.insert("session_id".into(), serde_json::json!(sid));
            obj.insert("author_id".into(), serde_json::json!(agent_id));
        }
    }
    let req = ModuleInvokeRequest {
        module,
        tool: tool.to_string(),
        args,
        actor: format!("agent:{agent_id}"),
        actor_caps: caps.to_vec(),
        trace_id: trace_id.to_string(),
    };
    match bus
        .call::<ModuleInvokeRequest, ModuleInvokeResponse>("module.invoke", &req, vec![])
        .await
    {
        Ok(resp) if resp.ok => format_module_invoke_result(&resp.result),
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
                .call::<MemEpisodicWriteRequest, MemRememberResponse>(
                    "mem.episodic_write",
                    &MemEpisodicWriteRequest {
                        namespace: ns,
                        text,
                        metadata: serde_json::json!({}),
                        pinned: false,
                        auto_link: true,
                        ..Default::default()
                    },
                    vec![],
                )
                .await
            {
                Ok(r) => format!("episodic id={}", r.id),
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
                        product_k: 4,
                        user_doc_k: 0,
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
            let engine = args
                .get("engine")
                .and_then(|v| v.as_str())
                .unwrap_or("auto")
                .to_string();
            match bus
                .call::<WebSearchRequest, WebSearchResponse>(
                    "web.search",
                    &WebSearchRequest {
                        query: query.clone(),
                        max_results: args
                            .get("max_results")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(5) as usize,
                        caps: caps.to_vec(),
                        actor,
                        engine,
                    },
                    vec![],
                )
                .await
            {
                Ok(r) if r.results.is_empty() => {
                    format!(
                        "web.search: 0 résultat pour « {query} ». \
                         Réessaie avec args.engine=\"bing\" ou \"duckduckgo\", \
                         ou utilise web.browse sur une URL connue."
                    )
                }
                Ok(r) => serde_json::to_string(&r.results).unwrap_or_default(),
                Err(e) => format!(
                    "web.search err: {e}. Si le réseau est online, réessaie avec \
                     engine=\"bing\" ou utilise web.browse sur une URL."
                ),
            }
        }
        "web.browse" => {
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let max_chars = args
                .get("max_chars")
                .and_then(|v| v.as_u64())
                .unwrap_or(12_000) as usize;
            match bus
                .call::<WebBrowseRequest, WebBrowseResponse>(
                    "web.browse",
                    &WebBrowseRequest {
                        url,
                        max_chars,
                        caps: caps.to_vec(),
                        actor,
                    },
                    vec![],
                )
                .await
            {
                Ok(r) => serde_json::to_string(&r).unwrap_or_default(),
                Err(e) => format!("web.browse err: {e}"),
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
                Ok(v) => format!(
                    "{v}\n[hint] net.fetch enregistre un fichier ; pour lire le contenu d'une page HTML utilise web.browse."
                ),
                Err(e) => format!(
                    "net.fetch err: {e}. Pour extraire le texte d'une page, utilise web.browse."
                ),
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
        "media.image.generate" => {
            let prompt = args
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let model_id = args
                .get("model_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let options = match args.get("options") {
                None => aos_proto::MediaImageOptions::default(),
                Some(v) => match serde_json::from_value::<aos_proto::MediaImageOptions>(v.clone())
                {
                    Ok(o) => o,
                    Err(e) => {
                        return format!("media.image.generate err: options refused ({e})");
                    }
                },
            };
            match bus
                .call::<aos_proto::MediaImageGenerateRequest, aos_proto::MediaGenerateResponse>(
                    "media.image.generate",
                    &aos_proto::MediaImageGenerateRequest {
                        prompt,
                        path,
                        model_id,
                        options,
                        actor: actor.clone(),
                        caps: caps.to_vec(),
                        trace_id: String::new(),
                    },
                    vec![],
                )
                .await
            {
                Ok(r) => format!("image {} ({} octets, moteur {})", r.path, r.bytes, r.engine),
                Err(e) => format!("media.image.generate err: {e}"),
            }
        }
        "media.audio.generate" => {
            let text = args
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let model_id = args
                .get("model_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let options = match args.get("options") {
                None => aos_proto::MediaAudioOptions::default(),
                Some(v) => match serde_json::from_value::<aos_proto::MediaAudioOptions>(v.clone())
                {
                    Ok(o) => o,
                    Err(e) => {
                        return format!("media.audio.generate err: options refused ({e})");
                    }
                },
            };
            match bus
                .call::<aos_proto::MediaAudioGenerateRequest, aos_proto::MediaGenerateResponse>(
                    "media.audio.generate",
                    &aos_proto::MediaAudioGenerateRequest {
                        text,
                        path,
                        model_id,
                        options,
                        actor: actor.clone(),
                        caps: caps.to_vec(),
                        trace_id: String::new(),
                    },
                    vec![],
                )
                .await
            {
                Ok(r) => format!("audio {} ({} octets, moteur {})", r.path, r.bytes, r.engine),
                Err(e) => format!("media.audio.generate err: {e}"),
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
                ui: args
                    .get("ui")
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
        "module.uninstall" => {
            let module = args
                .get("module")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            match bus
                .call::<aos_proto::ModuleUninstallRequest, Result<(), String>>(
                    "module.uninstall",
                    &aos_proto::ModuleUninstallRequest {
                        module: module.clone(),
                        actor: actor.clone(),
                        actor_caps: caps.to_vec(),
                    },
                    vec![],
                )
                .await
            {
                Ok(Ok(())) => format!("module désinstallé: {module}"),
                Ok(Err(e)) => format!("module.uninstall err: {e}"),
                Err(e) => format!("module.uninstall err: {e}"),
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
            .call::<MemEpisodicWriteRequest, MemRememberResponse>(
                "mem.episodic_write",
                &MemEpisodicWriteRequest {
                    namespace: format!("agent:{agent_id}:docs"),
                    text: format!("{} ({}) : {excerpt}", d.label, d.path),
                    metadata: serde_json::json!({"path": d.path}),
                    pinned: true,
                    ..Default::default()
                },
                vec![],
            )
            .await;
    }
}

async fn install_system_prompt(
    bus: &BusClient,
    shared: &Shared,
    spec: &AgentSpec,
    skills: &[SkillDoc],
    tools: &[ToolDesc],
) {
    let mut system = compile_system_prompt(&PromptCompileInput {
        spec,
        skills,
        tools,
        doc_index: &spec.documents,
    });
    let has_canvas = tools.iter().any(|t| t.name.starts_with("canvas."));
    if has_canvas {
        if let Some(sid) = spec.session_id.as_deref().filter(|s| !s.is_empty()) {
            if let Some(digest) = fetch_canvas_scene_digest(bus, sid).await {
                system.push_str("\n\n");
                system.push_str(&canvas_scene_prompt_block(&digest));
            }
        }
    }
    let mut st = shared.state.lock().await;
    if st.working_memory.is_empty() || st.working_memory[0].0 != "system" {
        st.working_memory.insert(0, ("system".into(), system));
    } else {
        st.working_memory[0] = ("system".into(), system);
    }
}

/// Applique le résultat de `task.assess` : state, skill planner, recompile prompt.
async fn apply_assess_to_runtime(
    bus: &BusClient,
    shared: &Shared,
    spec: &mut AgentSpec,
    skill_docs: &mut Vec<SkillDoc>,
    tools: &mut Vec<ToolDesc>,
    module_tools: &[ToolDesc],
    assess: &AssessResult,
) {
    {
        let mut st = shared.state.lock().await;
        st.complexity = Some(assess.complexity.clone());
        if st.deep_thinking {
            st.needs_plan = true;
        } else {
            st.needs_plan = assess.needs_plan;
        }
        if !st.needs_plan {
            // Steer vers simple : lever le gate même si un ancien plan existe
            // (needs_plan=false suffit via plan_gate_active).
        } else {
            // Nouveau besoin de plan : reset le flag mémoire-après-plan
            if st.task_graph.is_empty() && st.deep_plan_id.is_none() {
                st.plan_memory_recalled = false;
            }
        }
    }

    if assess.is_complex() && !spec.skills.iter().any(|s| s == "planner" || s == "deep-thinking") {
        if agent_has_canvas_tools(&spec.tools) {
            install_system_prompt(bus, shared, spec, skill_docs, tools).await;
            return;
        }
        if spec.cognitive_mode.is_deep_thinking() {
            if !spec.skills.iter().any(|s| s == "deep-thinking") {
                spec.skills.push("deep-thinking".into());
            }
        } else {
            spec.skills.push("planner".into());
        }
        *skill_docs = load_skills(&spec.skills);
        let tool_ids = merge_skill_tools(&spec.tools, skill_docs);
        *tools = select_tools_mode(
            &tool_ids,
            module_tools,
            spec.cognitive_mode.is_deep_thinking(),
        );
        strip_canvas_blocked_runtime_tools(tools, &spec.tools);
        let derived = caps_for_tools(tools, &spec.mcp_servers);
        for c in derived {
            if !spec.caps.contains(&c) {
                spec.caps.push(c);
            }
        }
        install_system_prompt(bus, shared, spec, skill_docs, tools).await;
        let _ = persist::write_spec(spec);
        report(
            bus,
            &spec.agent_id,
            AgentOutputEvent::Log {
                line: "skill planner activée (task.assess = complex)".into(),
            },
        )
        .await;
    } else if assess.is_complex() {
        // Planner déjà présent : recompile quand même pour coller au protocole à jour
        install_system_prompt(bus, shared, spec, skill_docs, tools).await;
    }
}

async fn run_task_assess(
    bus: &BusClient,
    shared: &Shared,
    spec: &AgentSpec,
    statement: &str,
    reason: &str,
) -> AssessResult {
    let t0 = Instant::now();
    let req = InferRequest {
        model_id: spec.model_id.clone(),
        messages: vec![
            ChatMessage {
                role: "system".into(),
                content: "Tu classifies des tâches pour un agent OS. Réponds UNIQUEMENT par un JSON \
                     {\"complexity\":\"simple\"|\"complex\",\"reason\":\"…\",\"needs_plan\":bool}. \
                     complex/needs_plan=true si plusieurs étapes, livrables distincts, recherche+rédaction, \
                     délégation, critères multiples, OU si plusieurs sous-travaux indépendants \
                     peuvent avancer en parallèle (ex. chercher A et rédiger B, deux notes/fichiers distincts, \
                     plusieurs recherches sans dépendance). simple seulement si une seule chaîne séquentielle courte. \
                     Pas de balises <think>."
                    .into(),
            },
            ChatMessage {
                role: "user".into(),
                content: format!("Tâche ({reason}) : {statement}"),
            },
        ],
        params: InferParams {
            max_tokens: 80,
            temperature: 0.0,
            ..InferParams::default()
        },
        priority: 2,
        data_refs: vec![],
        images: vec![],
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
    let assess = if text.trim().is_empty() {
        AssessResult::complex("task.assess: pas de réponse modèle — plan par défaut")
    } else {
        parse_assess_response(&text)
    };
    let tool_ms = t0.elapsed().as_millis() as u64;

    let record = AgentStepRecord {
        step: 0,
        thought: format!("Estimer la complexité ({reason})"),
        response: truncate(&text, 500),
        action: "task.assess".into(),
        args: serde_json::json!({
            "query": statement,
            "reason": reason,
            "complexity": assess.complexity,
            "needs_plan": assess.needs_plan,
        }),
        tool_kind: "runtime".into(),
        mcp_server: None,
        skill: None,
        tool_result: format!(
            "{} — {} (needs_plan={})",
            assess.complexity, assess.reason, assess.needs_plan
        ),
        reflection: None,
        duration_ms: tool_ms,
        infer_ms: tool_ms,
        tool_ms: 0,
        prompt_tokens: 0,
        generated_tokens: 0,
        ttft_ms: 0.0,
        tok_s: 0.0,
        current_task: None,
        ts_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        fail_reason: None,
        child_id: None,
        sources: vec![],
    };
    {
        let mut st = shared.state.lock().await;
        st.trace.push(record.clone());
    }
    report(bus, &spec.agent_id, AgentOutputEvent::Step(record)).await;
    report(
        bus,
        &spec.agent_id,
        AgentOutputEvent::Log {
            line: format!(
                "task.assess ({reason}): {} — {}",
                assess.complexity,
                truncate(&assess.reason, 80)
            ),
        },
    )
    .await;
    assess
}

/// Canvas drawing is spatially complex even when its text goal is short. A
/// bounded plan prevents a local chat model from treating every new stroke as
/// an unstructured continuation.
fn require_canvas_plan(assess: AssessResult, spec: &AgentSpec) -> AssessResult {
    if assess.is_complex() || !agent_has_canvas_tools(&spec.tools) {
        return assess;
    }
    AssessResult::complex(
        "dessin canvas : plan de composition requis pour séparer analyse, silhouette, détails et finitions",
    )
}

async fn recall_memory_bundle(
    bus: &BusClient,
    agent_id: &str,
    query: &str,
    k: usize,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    for (label, ns) in [
        ("Mémoire agent", format!("agent:{agent_id}")),
        ("Documents indexés", format!("agent:{agent_id}:docs")),
    ] {
        match bus
            .call::<MemEpisodicQueryRequest, Vec<MemHit>>(
                "mem.episodic_query",
                &MemEpisodicQueryRequest {
                    query: query.to_string(),
                    k,
                    namespace: Some(ns),
                },
                vec![],
            )
            .await
        {
            Ok(hits) if !hits.is_empty() => {
                parts.push(format!("{label}:"));
                for h in hits {
                    if h.superseded {
                        continue;
                    }
                    let pin = if h.pinned { " ★" } else { "" };
                    let mut line = format!("- [{}] {}{}", h.id, truncate(&h.text, 400), pin);
                    let similar: Vec<_> = h
                        .relations
                        .iter()
                        .filter(|r| r.rel == aos_proto::MemRelationKind::Similar)
                        .map(|r| r.to.to_string())
                        .collect();
                    if !similar.is_empty() {
                        line.push_str(&format!(" ~similar:{}", similar.join(",")));
                    }
                    parts.push(line);
                }
            }
            Ok(_) => {}
            Err(e) => parts.push(format!("({label}: err {e})")),
        }
    }

    match bus
        .call::<MemContextRequest, MemContextResponse>(
            "mem.context",
            &MemContextRequest {
                session_id: None,
                query: query.to_string(),
                k,
                product_k: 4,
                user_doc_k: 0,
            },
            vec![],
        )
        .await
    {
        Ok(r) if !r.prompt_block.trim().is_empty() => {
            parts.push(r.prompt_block.trim().to_string());
        }
        Ok(_) => {}
        Err(e) => parts.push(format!("(mémoire utilisateur: err {e})")),
    }

    if parts.is_empty() {
        format!(
            "(aucune information mémorisée trouvée pour « {} »)",
            truncate(query, 120)
        )
    } else {
        parts.join("\n")
    }
}

async fn inject_mem_context(bus: &BusClient, shared: &Shared, agent_id: &str, query: &str) {
    let block = recall_memory_bundle(bus, agent_id, query, 3).await;
    if block.starts_with("(aucune information") {
        return;
    }
    shared.state.lock().await.working_memory.push((
        "system".into(),
        format!("[mem.context]\n{block}"),
    ));
}

/// Consulte la mémoire dès le début (ou après un steer) et l'enregistre dans la timeline.
async fn bootstrap_memory_recall(
    bus: &BusClient,
    shared: &Shared,
    agent_id: &str,
    query: &str,
    reason: &str,
) {
    let t0 = Instant::now();
    let block = recall_memory_bundle(bus, agent_id, query, 5).await;
    let tool_ms = t0.elapsed().as_millis() as u64;

    shared.state.lock().await.working_memory.push((
        "system".into(),
        format!(
            "[mem.bootstrap]\nConsultation mémoire ({reason}) pour : {}\n{block}\n\
             Consigne: réutilise ces informations avant web.search / net.fetch. \
             Affiner avec memory.recall si besoin.",
            truncate(query, 200)
        ),
    ));

    let record = AgentStepRecord {
        step: 0,
        thought: format!("Interroger la mémoire ({reason}) avant d'agir"),
        response: String::new(),
        action: "memory.recall".into(),
        args: serde_json::json!({ "query": query, "reason": reason }),
        tool_kind: "runtime".into(),
        mcp_server: None,
        skill: None,
        tool_result: truncate(&block, 2000),
        reflection: None,
        duration_ms: tool_ms,
        infer_ms: 0,
        tool_ms,
        prompt_tokens: 0,
        generated_tokens: 0,
        ttft_ms: 0.0,
        tok_s: 0.0,
        current_task: None,
        ts_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        fail_reason: None,
        child_id: None,
        sources: vec![],
    };
    {
        let mut st = shared.state.lock().await;
        st.trace.push(record.clone());
    }
    report(bus, agent_id, AgentOutputEvent::Step(record)).await;
    report(
        bus,
        agent_id,
        AgentOutputEvent::Log {
            line: format!("mémoire consultée ({reason}): {}", truncate(query, 80)),
        },
    )
    .await;
}

async fn reflect(bus: &BusClient, shared: &Shared, spec: &AgentSpec) -> Option<String> {
    let (progress, canvas_sid, canvas_draw) = {
        let st = shared.state.lock().await;
        let canvas_draw = agent_has_canvas_tools(&spec.tools);
        let progress = if canvas_draw {
            canvas_reflect_user_content(
                st.step,
                spec.goal.max_steps,
                &spec.goal.statement,
                &st.plan_stack,
                &st.trace,
                &spec.tools,
            )
        } else {
            format!(
                "step {}/{} goal={} tasks={:?}",
                st.step,
                spec.goal.max_steps,
                spec.goal.statement,
                st.plan_stack
            )
        };
        let canvas_sid = if canvas_draw {
            spec.session_id.clone()
        } else {
            None
        };
        (progress, canvas_sid, canvas_draw)
    };
    let mut images: Vec<String> = Vec::new();
    let mut data_refs: Vec<String> = Vec::new();
    let canvas_active = if let Some(sid) = canvas_sid.as_deref().filter(|s| !s.is_empty()) {
        let aspect = fetch_canvas_aspect(bus, sid).await;
        if let Some(png) = begin_canvas_vision(
            bus,
            sid,
            aspect,
            spec.model_id.as_deref(),
        )
        .await
        {
            data_refs = merge_canvas_vision_refs(&[], &png);
            images = data_refs.clone();
            true
        } else {
            false
        }
    } else {
        false
    };
    let critic_system = if canvas_draw {
        if canvas_active {
            canvas_critic_system_prompt()
        } else {
            canvas_text_only_critic_system_prompt()
        }
    } else {
        "Tu es un critique. En 2 phrases en français: est-ce que l'agent avance vers le goal ? Que faire ensuite ? \
         Réponds directement, sans balises <think> ni monologue Thinking Process."
    };

    let req = InferRequest {
        model_id: spec.model_id.clone(),
        messages: vec![
            ChatMessage {
                role: "system".into(),
                content: critic_system.into(),
            },
            ChatMessage {
                role: "user".into(),
                content: progress,
            },
        ],
        params: InferParams {
            max_tokens: 220,
            temperature: 0.1,
            ..InferParams::default()
        },
        priority: 1,
        data_refs,
        images,
        routing: None,
    };
    let result = if let Ok(mut rx) = bus
        .call_stream::<InferRequest, TokenEvent>("model.infer", &req, vec![])
        .await
    {
        let mut text = String::new();
        while let Some(ev) = rx.recv().await {
            if let Ok(TokenEvent::Delta { text: t }) = ev {
                text.push_str(&t);
            }
        }
        let text = strip_reasoning(&text);
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
                .push((
                    "system".into(),
                    format!("[canvas critic — consigne prioritaire au prochain tour]\n{text}"),
                ));
            Some(text)
        } else {
            None
        }
    } else {
        None
    };

    if canvas_active {
        if let Some(sid) = canvas_sid.as_deref() {
            end_canvas_vision(bus, sid).await;
        }
    }
    result
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
        images: vec![],
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
        images: vec![],
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

async fn record_child_finished(shared: &Shared, child_id: String, result: String, ok: bool) {
    shared
        .child_results
        .lock()
        .await
        .insert(child_id, (result, ok));
}

async fn drain_child_finished_into_memory(bus: &BusClient, agent_id: &str, shared: &Shared) {
    let pending: Vec<(String, (String, bool))> = {
        let mut map = shared.child_results.lock().await;
        map.drain().collect()
    };
    for (child_id, (result, ok)) in pending {
        if !shared
            .consumed_child_results
            .lock()
            .await
            .insert(child_id.clone())
        {
            continue;
        }
        let injected = {
            let mut st = shared.state.lock().await;
            st.inject_child_done_memory(&child_id, &result, ok)
        };
        if injected {
            report(
                bus,
                agent_id,
                AgentOutputEvent::Log {
                    line: format!("sous-agent {child_id} terminé"),
                },
            )
            .await;
        }
    }
}

fn child_terminal_result(st: &CognitiveState, state: &AgentState) -> String {
    if let Some(artifact) = st.artifacts.last().filter(|s| !s.trim().is_empty()) {
        return artifact.clone();
    }
    if let Some(rec) = st
        .trace
        .last()
        .filter(|r| !r.tool_result.trim().is_empty())
    {
        return rec.tool_result.clone();
    }
    format!("{state:?}")
}

async fn notify_parent_if_child(
    bus: &BusClient,
    spec: &AgentSpec,
    shared: &Shared,
    state: &AgentState,
) {
    let Some(parent) = spec.parent_id.as_ref() else {
        return;
    };
    let result = {
        let st = shared.state.lock().await;
        child_terminal_result(&st, state)
    };
    let ok = matches!(state, AgentState::Done);
    let key = CognitiveState::child_shared_mem_key(parent, &spec.agent_id);
    let _ = bus
        .call::<MemSharedWriteRequest, bool>(
            "mem.shared_write",
            &MemSharedWriteRequest {
                name: key,
                value: serde_json::json!({"result": result, "ok": ok}),
            },
            vec![],
        )
        .await;
}

async fn take_awaited_child_result(
    shared: &Shared,
    bus: &BusClient,
    agent_id: &str,
    child_id: &str,
) -> Option<String> {
    let (result, ok) = shared.child_results.lock().await.remove(child_id)?;
    let payload = serde_json::json!({"result": result, "ok": ok}).to_string();
    Some(finish_agent_await(bus, shared, agent_id, child_id.to_string(), payload).await)
}

async fn finish_agent_await(
    bus: &BusClient,
    shared: &Shared,
    agent_id: &str,
    child_id: String,
    result: String,
) -> String {
    shared
        .consumed_child_results
        .lock()
        .await
        .insert(child_id.clone());
    report(
        bus,
        agent_id,
        AgentOutputEvent::ChildDone {
            child_id,
            result: result.clone(),
        },
    )
    .await;
    result
}

/// Refuse d'attendre un id vide ou un agent que ce parent n'a pas spawn.
fn await_child_reject_reason(child_id: &str, my_children: &[String]) -> Option<String> {
    if child_id.is_empty() {
        return Some(
            "agent.await: child_id manquant — spawn d'abord un sous-agent".into(),
        );
    }
    if !my_children.iter().any(|c| c == child_id) {
        return Some(format!(
            "agent.await: {child_id} n'est pas un sous-agent que tu as créé. \
             Utilise l'id renvoyé par agent.spawn, ou spawn d'abord."
        ));
    }
    None
}

fn extract_child_id(action: &str, outcome: &str, args: &serde_json::Value) -> Option<String> {
    if action == "agent.spawn" {
        if let Some(rest) = outcome.strip_prefix("sous-agent créé: ") {
            return Some(rest.trim().to_string());
        }
    }
    if action == "agent.await" {
        return args
            .get("child_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
    }
    None
}

fn collect_sources(
    action: &str,
    args: &serde_json::Value,
    outcome: &str,
) -> Vec<AgentSource> {
    match action {
        "web.search" => {
            if let Ok(hits) = serde_json::from_str::<Vec<WebSearchHit>>(outcome) {
                return hits
                    .into_iter()
                    .map(|h| AgentSource {
                        kind: "web".into(),
                        title: h.title,
                        locator: h.url,
                        snippet: h.snippet,
                    })
                    .collect();
            }
            Vec::new()
        }
        "web.browse" => {
            let url = args
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if let Ok(v) = serde_json::from_str::<WebBrowseResponse>(outcome) {
                return vec![AgentSource {
                    kind: "web".into(),
                    title: if v.title.is_empty() {
                        url.clone()
                    } else {
                        v.title
                    },
                    locator: if v.final_url.is_empty() {
                        v.url
                    } else {
                        v.final_url
                    },
                    snippet: truncate(&v.text, 200),
                }];
            }
            if !url.is_empty() {
                return vec![AgentSource {
                    kind: "web".into(),
                    title: url.clone(),
                    locator: url,
                    snippet: truncate(outcome, 200),
                }];
            }
            Vec::new()
        }
        "docs.read" | "fs.read" => {
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if path.is_empty() {
                return Vec::new();
            }
            vec![AgentSource {
                kind: "document".into(),
                title: path.clone(),
                locator: path,
                snippet: truncate(outcome, 200),
            }]
        }
        "net.fetch" => {
            let url = args
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let path = serde_json::from_str::<serde_json::Value>(outcome)
                .ok()
                .and_then(|v| {
                    v.get("path")
                        .and_then(|p| p.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_default();
            if url.is_empty() && path.is_empty() {
                return Vec::new();
            }
            vec![AgentSource {
                kind: "fetch".into(),
                title: if path.is_empty() {
                    url.clone()
                } else {
                    path.clone()
                },
                locator: if url.is_empty() { path } else { url },
                snippet: truncate(outcome, 120),
            }]
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        await_child_reject_reason, canvas_child_goal_statement, child_terminal_result,
        require_canvas_plan,
    };
    use aos_agent::assess::AssessResult;
    use aos_agent::CognitiveState;
    use aos_proto::{AgentGoal, AgentSpec, AgentState};

    #[test]
    fn canvas_goal_requires_a_bounded_composition_plan() {
        let spec = AgentSpec {
            agent_id: "canvas-agent".into(),
            goal: AgentGoal::default(),
            tools: vec!["canvas.path".into()],
            kind: Default::default(),
            display_name: None,
            persona_id: None,
            system_prompt: None,
            skills: vec![],
            mcp_servers: vec![],
            documents: vec![],
            caps: vec![],
            model_id: None,
            parent_id: None,
            session_id: None,
            budget: Default::default(),
            optimize_prompt: false,
            gate_mode: "autonomous".into(),
            origin: None,
        cognitive_mode: aos_proto::CognitiveMode::Normal,
        };
        assert!(require_canvas_plan(AssessResult::simple("short goal"), &spec).is_complex());
        let non_canvas = AgentSpec { tools: vec![], ..spec };
        assert!(!require_canvas_plan(AssessResult::simple("short goal"), &non_canvas).is_complex());
    }

    #[test]
    fn await_rejects_empty_child_id() {
        assert!(await_child_reject_reason("", &["agent-2".into()]).is_some());
    }

    #[test]
    fn await_rejects_child_not_spawned_by_parent() {
        let msg = await_child_reject_reason("agent-9", &["agent-2".into()]).unwrap();
        assert!(msg.contains("agent-9"));
        assert!(msg.contains("pas un sous-agent"));
    }

    #[test]
    fn await_accepts_own_child() {
        assert!(await_child_reject_reason("agent-2", &["agent-2".into()]).is_none());
    }

    #[test]
    fn child_terminal_result_prefers_artifacts() {
        let mut st = CognitiveState::new("child", vec![]);
        st.artifacts.push("résumé du sous-agent".into());
        assert_eq!(
            child_terminal_result(&st, &AgentState::Done),
            "résumé du sous-agent"
        );
        let empty = CognitiveState::new("child", vec![]);
        assert_eq!(child_terminal_result(&empty, &AgentState::Failed), "Failed");
    }

    #[test]
    fn canvas_child_inherits_parent_user_goal_not_house_example() {
        let parent = AgentSpec {
            agent_id: "parent".into(),
            goal: AgentGoal {
                statement: "dessine une canette Coca-Cola".into(),
                ..Default::default()
            },
            tools: vec!["canvas.stroke".into()],
            kind: Default::default(),
            display_name: None,
            persona_id: None,
            system_prompt: None,
            skills: vec![],
            mcp_servers: vec![],
            documents: vec![],
            caps: vec![],
            model_id: None,
            parent_id: None,
            session_id: None,
            budget: Default::default(),
            optimize_prompt: false,
            gate_mode: "ask".into(),
            origin: None,
        cognitive_mode: aos_proto::CognitiveMode::Normal,
        };
        let child_goal = canvas_child_goal_statement(
            &parent,
            "Une maison = toit + murs + porte + fenêtre",
        );
        assert_eq!(child_goal, "dessine une canette Coca-Cola");
    }

    #[test]
    fn non_canvas_child_keeps_spawn_brief_as_goal() {
        let parent = AgentSpec {
            agent_id: "parent".into(),
            goal: AgentGoal {
                statement: "recherche sur le climat".into(),
                ..Default::default()
            },
            tools: vec!["web.search".into()],
            kind: Default::default(),
            display_name: None,
            persona_id: None,
            system_prompt: None,
            skills: vec![],
            mcp_servers: vec![],
            documents: vec![],
            caps: vec![],
            model_id: None,
            parent_id: None,
            session_id: None,
            budget: Default::default(),
            optimize_prompt: false,
            gate_mode: "ask".into(),
            origin: None,
        cognitive_mode: aos_proto::CognitiveMode::Normal,
        };
        let child_goal = canvas_child_goal_statement(&parent, "résumer les sources A et B");
        assert_eq!(child_goal, "résumer les sources A et B");
    }
}
