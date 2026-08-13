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
    AgentOutputEvent, AgentState, CancelRequest, ChatMessage, InferParams, InferRequest,
    ModuleInvokeRequest, ModuleInvokeResponse, TokenEvent,
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
    // Connaissance système (§4.5) : l'agent sait où il vit et comment y
    // répondre — injecté en tête de mémoire de travail.
    {
        let mut st = shared.state.lock().await;
        st.working_memory.push((
            "system".to_string(),
            format!(
                "{}\n\nTu es l'agent {} d'Agent OS. Si l'utilisateur te demande d'utiliser un outil, réponds par la ligne `TOOL: <outil> <args json>`.",
                aos_proto::SYSTEM_ASSISTANT_PROMPT,
                agent_id
            ),
        ));
    }

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

        // Une seule entrée utilisateur par directive.
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
        let trace_id = format!(
            "trace-{}-{}",
            agent_id,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        );

        let mut tool_rounds = 0;
        let mut completed = false;
        while !completed {
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
                params: InferParams {
                    // Température basse : conformité du protocole TOOL: (P2).
                    temperature: 0.2,
                    ..InferParams::default()
                },
                priority: 1,
                data_refs: vec![],
                routing: None,
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
                // --- Convention d'appel d'outils (P2) : `TOOL: <outil> <json>` ---
                if let Some((tool, args)) = parse_tool_call(&full_text) {
                    if tool_rounds < 2 {
                        tool_rounds += 1;
                        shared.state.lock().await.push_assistant(&full_text);
                        let outcome =
                            invoke_tool(&bus, &agent_id, &caps, &tool, &args, &trace_id).await;
                        report(
                            &bus,
                            &agent_id,
                            AgentOutputEvent::Log {
                                line: format!("outil {tool} → {}", truncate(&outcome, 120)),
                            },
                        )
                        .await;
                        shared
                            .state
                            .lock()
                            .await
                            .working_memory
                            .push(("tool".to_string(), format!("[{tool}] {outcome}")));
                        continue; // tour final avec le résultat de l'outil
                    }
                }
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

/// Parse une ligne `TOOL: <outil> <args json>` dans la réponse du modèle.
fn parse_tool_call(text: &str) -> Option<(String, serde_json::Value)> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("TOOL:") {
            let rest = rest.trim();
            let (tool, args_str) = match rest.find(char::is_whitespace) {
                Some(i) => (rest[..i].to_string(), rest[i..].trim()),
                None => (rest.to_string(), "{}"),
            };
            let args = serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
            return Some((tool, args));
        }
    }
    None
}

/// Invoque un outil de module via le bus (caps de l'agent présentées).
async fn invoke_tool(
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

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        format!("{}…", s.chars().take(n).collect::<String>())
    } else {
        s.to_string()
    }
}
