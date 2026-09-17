//! Phase 2: external harness execution backend for task workers.
//!
//! Runs under `aos-agent-worker` when `AgentSpec.execution_backend` is
//! `ExternalHarness`. Maps:
//! - first turn → fixed start argv(goal)
//! - steer → continue/resume argv(directive)
//! - pause → cancel in-flight CLI + wait for resume/steer
//! - kill → agentd kills the worker; child uses kill_on_drop

use crate::agent_act::requires_act_gate;
use crate::harness::{
    default_turn_timeout_secs, has_harness_cap, run_turn, HarnessKind, HarnessTurnResult,
};
use crate::persist;
use crate::{intents, CognitiveState, ControlCmd, ControlResp, ReportPayload};
use aos_ipc::{BusClient, BusService};
use aos_proto::{
    AgentExecutionBackend, AgentOutputEvent, AgentSpec, AgentState, AgentStepRecord,
    ChatAttachment, ChatSessionAppendRequest,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, Mutex};

enum BackendCmd {
    Resume,
    Steer(String),
    ActDecision { approved: bool },
}

struct Shared {
    paused: AtomicBool,
    cancel_turn: AtomicBool,
    cmd_tx: mpsc::Sender<BackendCmd>,
    pending_act: Mutex<Option<String>>,
    state: Mutex<CognitiveState>,
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn parse_kind(backend: &AgentExecutionBackend) -> Result<(HarnessKind, Option<String>), String> {
    match backend {
        AgentExecutionBackend::Native => Err("backend native".into()),
        AgentExecutionBackend::ExternalHarness { harness, cwd } => {
            let kind = HarnessKind::parse(harness)
                .ok_or_else(|| format!("harness backend inconnu: {harness} (codex|claude|grok)"))?;
            Ok((kind, cwd.clone()))
        }
    }
}

/// Ensure the worker owns `harness.run` before spawning any CLI.
pub fn ensure_harness_caps(spec: &mut AgentSpec) {
    if !spec.execution_backend.is_external_harness() {
        return;
    }
    if !spec.tools.iter().any(|t| t == "harness.run") {
        spec.tools.push("harness.run".into());
    }
    if !has_harness_cap(&spec.caps) {
        spec.caps.push("harness.run".into());
    }
    spec.cognitive_mode = aos_proto::CognitiveMode::Normal;
}

fn should_gate_first_spawn(spec: &AgentSpec) -> bool {
    spec.session_id.is_some() && requires_act_gate("harness.run")
}

async fn post_agent_act(bus: &BusClient, spec: &AgentSpec, act_id: &str, prompt: &str) {
    let Some(session_id) = spec.session_id.clone() else {
        return;
    };
    let harness = spec
        .execution_backend
        .harness_id()
        .unwrap_or("harness")
        .to_string();
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
                    action: "harness.run".into(),
                    args: serde_json::json!({
                        "harness": harness,
                        "prompt": prompt,
                    }),
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
    prompt: &str,
    approved: bool,
) {
    let Some(session_id) = spec.session_id.clone() else {
        return;
    };
    let harness = spec
        .execution_backend
        .harness_id()
        .unwrap_or("harness")
        .to_string();
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
                    action: "harness.run".into(),
                    args: serde_json::json!({
                        "harness": harness,
                        "prompt": prompt,
                    }),
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

async fn wait_act_gate(
    bus: &BusClient,
    shared: &Shared,
    cmd_rx: &mut mpsc::Receiver<BackendCmd>,
    spec: &AgentSpec,
    prompt: &str,
) -> bool {
    if !should_gate_first_spawn(spec) {
        return true;
    }
    let act_id = format!("act-{}-{}", spec.agent_id, now_ms());
    *shared.pending_act.lock().await = Some(act_id.clone());
    report(
        bus,
        &spec.agent_id,
        AgentOutputEvent::StateChanged {
            state: AgentState::Blocked,
        },
    )
    .await;
    post_agent_act(bus, spec, &act_id, prompt).await;

    let deadline = Instant::now() + Duration::from_secs(10 * 60);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            post_act_resolved(bus, spec, &act_id, prompt, false).await;
            *shared.pending_act.lock().await = None;
            return false;
        }
        match tokio::time::timeout(left, cmd_rx.recv()).await {
            Ok(Some(BackendCmd::ActDecision { approved })) => {
                post_act_resolved(bus, spec, &act_id, prompt, approved).await;
                *shared.pending_act.lock().await = None;
                return approved;
            }
            Ok(Some(BackendCmd::Steer(_))) | Ok(Some(BackendCmd::Resume)) => {}
            Ok(None) => return false,
            Err(_) => {
                post_act_resolved(bus, spec, &act_id, prompt, false).await;
                *shared.pending_act.lock().await = None;
                return false;
            }
        }
    }
}

async fn wait_while_paused(
    bus: &BusClient,
    shared: &Shared,
    cmd_rx: &mut mpsc::Receiver<BackendCmd>,
    agent_id: &str,
) -> Option<String> {
    if !shared.paused.load(Ordering::SeqCst) {
        return None;
    }
    report(
        bus,
        agent_id,
        AgentOutputEvent::StateChanged {
            state: AgentState::Paused,
        },
    )
    .await;
    loop {
        match cmd_rx.recv().await {
            Some(BackendCmd::Resume) => {
                shared.paused.store(false, Ordering::SeqCst);
                shared.cancel_turn.store(false, Ordering::SeqCst);
                report(
                    bus,
                    agent_id,
                    AgentOutputEvent::StateChanged {
                        state: AgentState::Running,
                    },
                )
                .await;
                return None;
            }
            Some(BackendCmd::Steer(d)) => {
                shared.paused.store(false, Ordering::SeqCst);
                shared.cancel_turn.store(false, Ordering::SeqCst);
                report(
                    bus,
                    agent_id,
                    AgentOutputEvent::StateChanged {
                        state: AgentState::Running,
                    },
                )
                .await;
                return Some(d);
            }
            Some(BackendCmd::ActDecision { .. }) => {}
            None => return None,
        }
    }
}

struct EmitStep<'a> {
    bus: &'a BusClient,
    shared: &'a Shared,
    spec: &'a AgentSpec,
    step: u32,
    prompt: &'a str,
    result: &'a HarnessTurnResult,
    kind: HarnessKind,
    duration_ms: u64,
}

async fn emit_step(ctx: EmitStep<'_>) {
    let EmitStep {
        bus,
        shared,
        spec,
        step,
        prompt,
        result,
        kind,
        duration_ms,
    } = ctx;
    let tool_result = result.format_tool_result(kind);
    let short: String = prompt.chars().take(80).collect();
    let record = AgentStepRecord {
        step,
        thought: format!("external harness {}", kind.as_str()),
        response: String::new(),
        action: "harness.run".into(),
        args: serde_json::json!({
            "harness": kind.as_str(),
            "prompt": prompt,
            "resume": step > 1,
        }),
        tool_kind: "harness".into(),
        mcp_server: None,
        skill: None,
        tool_result: tool_result.clone(),
        reflection: None,
        duration_ms,
        infer_ms: 0,
        tool_ms: duration_ms,
        prompt_tokens: 0,
        generated_tokens: 0,
        ttft_ms: 0.0,
        tok_s: 0.0,
        current_task: Some(short.clone()),
        ts_ms: now_ms(),
        fail_reason: if result.ok() {
            None
        } else if result.cancelled {
            Some("cancelled".into())
        } else if result.timed_out {
            Some("timeout".into())
        } else {
            Some(format!("exit {:?}", result.exit_code))
        },
        child_id: None,
        sources: vec![],
    };
    {
        let mut st = shared.state.lock().await;
        st.step = step;
        st.trace.push(record.clone());
        let _ = persist::write_state(&st);
    }
    report(
        bus,
        &spec.agent_id,
        AgentOutputEvent::Progress {
            step,
            max_steps: spec.goal.max_steps,
            current_task: Some(format!(
                "{} · {}",
                kind.as_str(),
                short.chars().take(60).collect::<String>()
            )),
        },
    )
    .await;
    report(
        bus,
        &spec.agent_id,
        AgentOutputEvent::Log { line: tool_result },
    )
    .await;
    report(bus, &spec.agent_id, AgentOutputEvent::Step(record)).await;
}

async fn fail_fast(bus: &BusClient, agent_id: &str, message: String) {
    report(bus, agent_id, AgentOutputEvent::Error { message }).await;
    report(
        bus,
        agent_id,
        AgentOutputEvent::StateChanged {
            state: AgentState::Failed,
        },
    )
    .await;
}

/// Full worker lifecycle for an external harness backend.
pub async fn run(bus: Arc<BusClient>, bus_addr: String, mut spec: AgentSpec, restore: bool) {
    let agent_id = spec.agent_id.clone();
    ensure_harness_caps(&mut spec);
    let _ = persist::write_spec(&spec);

    let (kind, cwd) = match parse_kind(&spec.execution_backend) {
        Ok(v) => v,
        Err(e) => {
            fail_fast(bus.as_ref(), &agent_id, e).await;
            return;
        }
    };

    if !has_harness_cap(&spec.caps) {
        fail_fast(
            bus.as_ref(),
            &agent_id,
            "harness backend : capacité harness.run manquante".into(),
        )
        .await;
        return;
    }

    let (cmd_tx, mut cmd_rx) = mpsc::channel::<BackendCmd>(16);
    let mut state = if restore {
        persist::read_state(&agent_id)
            .unwrap_or_else(|| CognitiveState::new(agent_id.clone(), spec.caps.clone()))
    } else {
        CognitiveState::new(agent_id.clone(), spec.caps.clone())
    };
    state.goal = Some(spec.goal.clone());
    state.cap_set_snapshot = spec.caps.clone();

    let shared = Arc::new(Shared {
        paused: AtomicBool::new(false),
        cancel_turn: AtomicBool::new(false),
        cmd_tx: cmd_tx.clone(),
        pending_act: Mutex::new(None),
        state: Mutex::new(state),
    });

    {
        let mut svc = BusService::new(format!("agent-{agent_id}"));
        let control_intent = format!("agent.{agent_id}.control");
        let shared_c = shared.clone();
        svc.on(&control_intent, move |ctx| {
            let shared = shared_c.clone();
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
                        shared.cancel_turn.store(true, Ordering::SeqCst);
                        ControlResp::Ack
                    }
                    ControlCmd::Resume => {
                        shared.paused.store(false, Ordering::SeqCst);
                        shared.cancel_turn.store(false, Ordering::SeqCst);
                        let _ = shared.cmd_tx.send(BackendCmd::Resume).await;
                        ControlResp::Ack
                    }
                    ControlCmd::Steer { directive } => {
                        shared.paused.store(false, Ordering::SeqCst);
                        shared.cancel_turn.store(false, Ordering::SeqCst);
                        let _ = shared.cmd_tx.send(BackendCmd::Steer(directive)).await;
                        ControlResp::Ack
                    }
                    ControlCmd::ActDecision { approved, .. } => {
                        let _ = shared
                            .cmd_tx
                            .send(BackendCmd::ActDecision { approved })
                            .await;
                        ControlResp::Ack
                    }
                    ControlCmd::Snapshot => ControlResp::State(shared.state.lock().await.clone()),
                    ControlCmd::GrantCap { cap } => {
                        let mut st = shared.state.lock().await;
                        if !st.cap_set_snapshot.contains(&cap) {
                            st.cap_set_snapshot.push(cap);
                        }
                        ControlResp::Ack
                    }
                    ControlCmd::SetPolicy { .. } | ControlCmd::ChildFinished { .. } => {
                        ControlResp::Ack
                    }
                };
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
            }
        });
        let svc_bus = bus_addr;
        tokio::spawn(async move {
            let _ = svc.serve(&svc_bus).await;
        });
    }

    report(
        bus.as_ref(),
        &agent_id,
        AgentOutputEvent::StateChanged {
            state: AgentState::Running,
        },
    )
    .await;
    report(
        bus.as_ref(),
        &agent_id,
        AgentOutputEvent::Log {
            line: format!(
                "backend externe {} (cwd={})",
                kind.as_str(),
                cwd.as_deref().unwrap_or("AOS_HOME")
            ),
        },
    )
    .await;

    let started = Instant::now();
    let timeout = Duration::from_secs(spec.goal.timeout_secs.max(60));
    let max_steps = spec.goal.max_steps.max(1);
    let turn_timeout = default_turn_timeout_secs().max(60);

    let mut step = shared.state.lock().await.step;
    let mut next_prompt = if step == 0 {
        Some(spec.goal.statement.clone())
    } else {
        None
    };
    let mut resume = step > 0;
    let mut first_spawn_gated = step == 0;

    loop {
        if started.elapsed() > timeout {
            fail_fast(
                bus.as_ref(),
                &agent_id,
                "timeout goal (backend harness)".into(),
            )
            .await;
            return;
        }

        if let Some(steer) = wait_while_paused(bus.as_ref(), &shared, &mut cmd_rx, &agent_id).await
        {
            next_prompt = Some(steer);
            resume = true;
        }

        let prompt = match next_prompt.take() {
            Some(p) if !p.trim().is_empty() => p,
            _ => {
                report(
                    bus.as_ref(),
                    &agent_id,
                    AgentOutputEvent::Log {
                        line: "en attente d'un steer…".into(),
                    },
                )
                .await;
                match cmd_rx.recv().await {
                    Some(BackendCmd::Steer(d)) => {
                        resume = true;
                        d
                    }
                    Some(BackendCmd::Resume) => continue,
                    Some(BackendCmd::ActDecision { .. }) => continue,
                    None => {
                        report(
                            bus.as_ref(),
                            &agent_id,
                            AgentOutputEvent::StateChanged {
                                state: AgentState::Killed,
                            },
                        )
                        .await;
                        return;
                    }
                }
            }
        };

        if first_spawn_gated {
            let approved = wait_act_gate(bus.as_ref(), &shared, &mut cmd_rx, &spec, &prompt).await;
            first_spawn_gated = false;
            if !approved {
                fail_fast(bus.as_ref(), &agent_id, "lancement harness refusé".into()).await;
                return;
            }
            report(
                bus.as_ref(),
                &agent_id,
                AgentOutputEvent::StateChanged {
                    state: AgentState::Running,
                },
            )
            .await;
        }

        if step >= max_steps {
            report(
                bus.as_ref(),
                &agent_id,
                AgentOutputEvent::StateChanged {
                    state: AgentState::Done,
                },
            )
            .await;
            return;
        }

        step += 1;
        shared.cancel_turn.store(false, Ordering::SeqCst);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_flag = cancel.clone();
        let shared_c = shared.clone();
        let watcher = tokio::spawn(async move {
            while !cancel_flag.load(Ordering::SeqCst) {
                if shared_c.cancel_turn.load(Ordering::SeqCst)
                    || shared_c.paused.load(Ordering::SeqCst)
                {
                    cancel_flag.store(true, Ordering::SeqCst);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });

        let turn_started = Instant::now();
        let secs = turn_timeout.min(timeout.saturating_sub(started.elapsed()).as_secs().max(15));
        let result = run_turn(kind, &prompt, cwd.as_deref(), secs, resume, Some(cancel)).await;
        watcher.abort();

        match result {
            Ok(r) => {
                let dur = turn_started.elapsed().as_millis() as u64;
                emit_step(EmitStep {
                    bus: bus.as_ref(),
                    shared: &shared,
                    spec: &spec,
                    step,
                    prompt: &prompt,
                    result: &r,
                    kind,
                    duration_ms: dur,
                })
                .await;
                if r.cancelled {
                    resume = true;
                    continue;
                }
                if r.timed_out || !r.ok() {
                    report(
                        bus.as_ref(),
                        &agent_id,
                        AgentOutputEvent::StateChanged {
                            state: AgentState::Failed,
                        },
                    )
                    .await;
                    return;
                }
                resume = true;
                if step >= max_steps {
                    report(
                        bus.as_ref(),
                        &agent_id,
                        AgentOutputEvent::StateChanged {
                            state: AgentState::Done,
                        },
                    )
                    .await;
                    return;
                }
            }
            Err(e) => {
                fail_fast(bus.as_ref(), &agent_id, e).await;
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_caps_adds_harness_run() {
        let mut spec: AgentSpec = serde_json::from_value(serde_json::json!({
            "agent_id": "a",
            "goal": {"statement": "fix", "max_steps": 3, "timeout_secs": 60},
            "execution_backend": {"kind": "external_harness", "harness": "codex"}
        }))
        .unwrap();
        ensure_harness_caps(&mut spec);
        assert!(spec.tools.iter().any(|t| t == "harness.run"));
        assert!(has_harness_cap(&spec.caps));
        assert!(!spec.cognitive_mode.is_deep_thinking());
    }

    #[test]
    fn parse_kind_ok() {
        let b = AgentExecutionBackend::from_harness_id("claude", Some("/tmp".into()));
        let (k, cwd) = parse_kind(&b).unwrap();
        assert_eq!(k, HarnessKind::Claude);
        assert_eq!(cwd.as_deref(), Some("/tmp"));
    }
}
