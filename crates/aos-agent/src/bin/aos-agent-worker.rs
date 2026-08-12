//! `aos-agent-worker` — processus isolé d'un agent (P1.4).
//!
//! Usage : `aos-agent-worker --agent-id <id> --bus <addr> --directive <texte>
//!          [--caps cap://a,cap://b] [--model <id>]`
//!
//! Boucle cognitive v1 : directive → `model.infer` en flux → rapporte les
//! tokens à `aos-agentd` (intent `agent.report`). Contrôlable en direct via
//! l'intent `agent.<id>.control` (pause / resume / steer / snapshot).
//!
//! Sémantique v1 du pause : abandon de la génération partielle (cancel côté
//! Model Subsystem) et gel de l'état ; `resume` régénère le tour depuis la
//! mémoire de travail. Honnête et documenté — la reprise au token près est
//! une affaire de scheduler (P5).

use aos_agent::{intents, CognitiveState, ControlCmd, ControlResp, ReportPayload};
use aos_ipc::{BusClient, BusService};
use aos_proto::{
    AgentOutputEvent, AgentState, CancelRequest, ChatMessage, InferParams, InferRequest, TokenEvent,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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

fn parse_args() -> (String, String, String, Vec<String>, Option<String>) {
    let mut agent_id = None;
    let mut bus = None;
    let mut directive = None;
    let mut caps = Vec::new();
    let mut model = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--agent-id" => agent_id = args.next(),
            "--bus" => bus = args.next(),
            "--directive" => directive = args.next(),
            "--caps" => {
                caps = args
                    .next()
                    .map(|s| {
                        s.split(',')
                            .map(|c| c.trim().to_string())
                            .filter(|c| !c.is_empty())
                            .collect()
                    })
                    .unwrap_or_default()
            }
            "--model" => model = args.next(),
            _ => {}
        }
    }
    (
        agent_id.expect("--agent-id requis"),
        bus.expect("--bus requis"),
        directive.expect("--directive requis"),
        caps,
        model,
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
    let (agent_id, bus_addr, directive, caps, model) = parse_args();
    let bus = BusClient::connect(&bus_addr, format!("agent:{agent_id}"))
        .await
        .expect("connexion au bus");

    let (cmd_tx, mut cmd_rx) = mpsc::channel::<WorkerCmd>(16);
    let shared = Arc::new(Shared {
        state: Mutex::new(CognitiveState::new(agent_id.clone(), caps.clone())),
        paused: AtomicBool::new(false),
        current_inference: Mutex::new(None),
        cmd_tx: cmd_tx.clone(),
    });

    // Service de contrôle `agent.<id>.control`.
    let mut svc = BusService::new(format!("agent-{agent_id}"));
    let control_intent = format!("agent.{agent_id}.control");
    {
        let shared = shared.clone();
        let bus = bus.clone();
        svc.on(&control_intent, move |ctx| {
            let shared = shared.clone();
            let bus = bus.clone();
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
                };
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
            }
        });
    }
    let svc_bus = bus_addr.clone();
    tokio::spawn(async move { svc.serve(&svc_bus).await });

    report(
        &bus,
        &agent_id,
        AgentOutputEvent::StateChanged {
            state: AgentState::Running,
        },
    )
    .await;

    // Boucle cognitive.
    let mut pending_directive = Some(directive);
    'outer: loop {
        let current = match pending_directive.take() {
            Some(d) => d,
            None => match cmd_rx.recv().await {
                Some(WorkerCmd::Steer(d)) => d,
                Some(WorkerCmd::Resume) => continue,
                None => break,
            },
        };

        let mut completed = false;
        while !completed {
            // Enregistre la directive dans la mémoire de travail.
            {
                let mut st = shared.state.lock().await;
                st.push_user(&current);
            }
            report(
                &bus,
                &agent_id,
                AgentOutputEvent::Log {
                    line: format!("directive : {current}"),
                },
            )
            .await;

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
                model_id: model.clone(),
                messages,
                params: InferParams::default(),
                priority: 1,
            };
            let rx = bus
                .call_stream::<InferRequest, TokenEvent>("model.infer", &req, caps.clone())
                .await;
            let mut rx = match rx {
                Ok(rx) => rx,
                Err(e) => {
                    report(
                        &bus,
                        &agent_id,
                        AgentOutputEvent::Error {
                            message: e.to_string(),
                        },
                    )
                    .await;
                    break 'outer;
                }
            };

            let mut full_text = String::new();
            let mut interrupted = false;
            while let Some(ev) = rx.recv().await {
                match ev {
                    Ok(TokenEvent::Started { inference_id }) => {
                        *shared.current_inference.lock().await = Some(inference_id);
                    }
                    Ok(TokenEvent::Delta { text }) => {
                        if shared.paused.load(Ordering::SeqCst) {
                            interrupted = true;
                            break;
                        }
                        full_text.push_str(&text);
                        report(&bus, &agent_id, AgentOutputEvent::Token { text }).await;
                    }
                    Ok(TokenEvent::Done { tok_s, ttft_ms, .. }) => {
                        report(
                            &bus,
                            &agent_id,
                            AgentOutputEvent::Log {
                                line: format!("fin (TTFT {ttft_ms:.0} ms, {tok_s:.1} tok/s)"),
                            },
                        )
                        .await;
                    }
                    Ok(TokenEvent::Error { message }) => {
                        report(&bus, &agent_id, AgentOutputEvent::Error { message }).await;
                    }
                    Err(e) => {
                        report(
                            &bus,
                            &agent_id,
                            AgentOutputEvent::Error {
                                message: e.to_string(),
                            },
                        )
                        .await;
                    }
                    Ok(TokenEvent::Queued { position }) => {
                        report(
                            &bus,
                            &agent_id,
                            AgentOutputEvent::Log {
                                line: format!("en file (position {position})"),
                            },
                        )
                        .await;
                    }
                }
            }
            *shared.current_inference.lock().await = None;

            if interrupted {
                report(
                    &bus,
                    &agent_id,
                    AgentOutputEvent::StateChanged {
                        state: AgentState::Paused,
                    },
                )
                .await;
                // Attendre Resume / Steer.
                match cmd_rx.recv().await {
                    Some(WorkerCmd::Resume) => continue, // régénère ce tour
                    Some(WorkerCmd::Steer(d)) => {
                        pending_directive = Some(d);
                        continue 'outer;
                    }
                    None => break 'outer,
                }
            } else {
                shared.state.lock().await.push_assistant(&full_text);
                completed = true;
            }
        }
        report(
            &bus,
            &agent_id,
            AgentOutputEvent::StateChanged {
                state: AgentState::Running,
            },
        )
        .await;
    }
}
