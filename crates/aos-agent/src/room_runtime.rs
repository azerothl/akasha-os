//! Runtime bus pour tours de salon (`agent.room_turn` / `agent.room_conduct`).

use crate::persist;
use crate::room_conductor::{
    build_initial_queue, detect_peer_address, effective_max_turns,
};
use aos_ipc::BusClient;
use aos_proto::{
    AgentRoomConductRequest, AgentRoomConductResponse, AgentRoomTurnRequest,
    AgentRoomTurnResponse, AgentSpec, CancelRequest, ChatAttachment, ChatMessage,
    ChatSessionAppendRequest, ChatSessionGetResponse, ChatSessionIdRequest, ChatSessionMessage,
    ChatSessionMode, InferParams, InferRequest, TokenEvent,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

const TRANSCRIPT_LIMIT: usize = 40;

/// État d'un tour de salon en cours (annulation cooperative).
#[derive(Debug)]
pub struct RoomRoundState {
    pub cancelled: AtomicBool,
    pub current_inference: Mutex<Option<u64>>,
}

impl RoomRoundState {
    pub fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            current_inference: Mutex::new(None),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// Caps strictement limitées : inférence seule, jamais d'élargissement destructif.
pub fn room_turn_infer_caps() -> Vec<String> {
    vec![]
}

/// Formate le transcript session pour l'inférence (labels user / assistant (Name)).
pub fn format_transcript_messages(
    session: &ChatSessionGetResponse,
    member_name: &str,
    system_prompt: &str,
) -> Vec<ChatMessage> {
    let mut messages = vec![ChatMessage {
        role: "system".into(),
        content: system_prompt.to_string(),
    }];
    let start = session.messages.len().saturating_sub(TRANSCRIPT_LIMIT);
    for msg in session.messages.iter().skip(start) {
        if msg.role == "user" {
            messages.push(ChatMessage {
                role: "user".into(),
                content: msg.content.clone(),
            });
        } else {
            let label = msg
                .speaker_name
                .as_deref()
                .filter(|n| !n.is_empty())
                .unwrap_or("assistant");
            messages.push(ChatMessage {
                role: "assistant".into(),
                content: format!("({label}) {}", msg.content),
            });
        }
    }
    let _ = member_name;
    messages
}

pub fn build_room_system_prompt(spec: &AgentSpec, display_name: &str) -> String {
    let mut out = format!(
        "Tu es {display_name}, membre d'un salon multi-agent in-app. \
         Réponds en une seule prise, de façon concise. \
         Pour interpeller un autre membre, utilise @Nom ou @agent_id.\n"
    );
    if let Some(p) = spec.system_prompt.as_deref() {
        let p = p.trim();
        if !p.is_empty() {
            out.push('\n');
            out.push_str(p);
            out.push('\n');
        }
    }
    if !spec.goal.statement.trim().is_empty() {
        out.push_str("\nObjectif / persona : ");
        out.push_str(spec.goal.statement.trim());
        out.push('\n');
    }
    out
}

async fn fetch_session(bus: &BusClient, session_id: &str) -> Result<ChatSessionGetResponse, String> {
    bus.call::<ChatSessionIdRequest, ChatSessionGetResponse>(
        "chat.session.get",
        &ChatSessionIdRequest {
            session_id: session_id.to_string(),
        },
        vec![],
    )
    .await
    .map_err(|e| e.to_string())
}

async fn append_room_reply(
    bus: &BusClient,
    session_id: &str,
    speaker_id: &str,
    speaker_name: &str,
    content: &str,
) -> Result<(), String> {
    bus.call::<ChatSessionAppendRequest, ChatSessionMessage>(
        "chat.session.append",
        &ChatSessionAppendRequest {
            session_id: session_id.to_string(),
            role: "assistant".into(),
            content: content.to_string(),
            attachments: vec![ChatAttachment::AgentRef {
                agent_id: speaker_id.to_string(),
                title: speaker_name.to_string(),
                origin: "room".into(),
            }],
            speaker_id: Some(speaker_id.to_string()),
            speaker_name: Some(speaker_name.to_string()),
        },
        vec![],
    )
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

async fn run_infer(
    bus: &BusClient,
    round: &RoomRoundState,
    model_id: Option<String>,
    messages: Vec<ChatMessage>,
) -> Result<String, String> {
    let req = InferRequest {
        model_id,
        messages,
        params: InferParams {
            max_tokens: 768,
            temperature: 0.3,
            ..InferParams::default()
        },
        priority: 6,
        data_refs: vec![],
        routing: None,
    };
    let mut rx = bus
        .call_stream::<InferRequest, TokenEvent>("model.infer", &req, room_turn_infer_caps())
        .await
        .map_err(|e| e.to_string())?;

    let mut full = String::new();
    while let Some(ev) = rx.recv().await {
        if round.is_cancelled() {
            if let Some(id) = *round.current_inference.lock().await {
                let _ = bus
                    .call::<CancelRequest, bool>(
                        "model.cancel",
                        &CancelRequest { inference_id: id },
                        vec![],
                    )
                    .await;
            }
            return Err("tour annulé".into());
        }
        match ev {
            Ok(TokenEvent::Started { inference_id }) => {
                *round.current_inference.lock().await = Some(inference_id);
            }
            Ok(TokenEvent::Delta { text }) => full.push_str(&text),
            Ok(TokenEvent::Done { .. }) => break,
            Ok(TokenEvent::Error { message }) => return Err(message),
            Ok(TokenEvent::Queued { .. }) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    *round.current_inference.lock().await = None;
    Ok(full.trim().to_string())
}

/// Exécute un tour agent unique (`agent.room_turn`).
pub async fn execute_room_turn(
    bus: &BusClient,
    round: &RoomRoundState,
    req: &AgentRoomTurnRequest,
) -> Result<AgentRoomTurnResponse, String> {
    let session = fetch_session(bus, &req.session_id).await?;
    let spec = persist::read_spec(&req.agent_id).ok_or_else(|| {
        format!("spec introuvable pour le membre {}", req.agent_id)
    })?;
    if !session
        .meta
        .members
        .iter()
        .any(|m| m.agent_id == req.agent_id)
    {
        return Err(format!("membre {} absent du salon", req.agent_id));
    }

    let system = build_room_system_prompt(&spec, &req.display_name);
    let mut messages = format_transcript_messages(&session, &req.display_name, &system);
    messages.push(ChatMessage {
        role: "user".into(),
        content: req.user_message.clone(),
    });

    let model_id = spec.model_id.clone().or(session.meta.model_id.clone());
    let content = run_infer(bus, round, model_id, messages).await?;
    if content.is_empty() {
        return Err("réponse vide".into());
    }

    append_room_reply(
        bus,
        &req.session_id,
        &req.agent_id,
        &req.display_name,
        &content,
    )
    .await?;

    Ok(AgentRoomTurnResponse {
        content,
        speaker_id: req.agent_id.clone(),
        speaker_name: req.display_name.clone(),
    })
}

/// Orchestre un message utilisateur complet (conducteur déterministe).
pub async fn execute_room_conduct(
    bus: &BusClient,
    round: Arc<RoomRoundState>,
    req: &AgentRoomConductRequest,
) -> Result<AgentRoomConductResponse, String> {
    let session = fetch_session(bus, &req.session_id).await?;
    if session.meta.mode != ChatSessionMode::Room {
        return Err("session n'est pas en mode salon".into());
    }
    if session.meta.members.is_empty() {
        return Err("salon sans membres".into());
    }

    let max = effective_max_turns(&session.meta.conductor_policy) as usize;
    let mut queue = build_initial_queue(&req.content, &session.meta.members);
    if queue.is_empty() {
        return Err("aucun locuteur déterminé".into());
    }

    let mut agent_turns = 0u32;
    let mut peer_followup_used = false;

    while (agent_turns as usize) < max {
        if round.is_cancelled() {
            return Ok(AgentRoomConductResponse {
                agent_turns,
                cancelled: true,
            });
        }

        if queue.is_empty() {
            break;
        }

        let agent_id = queue.remove(0);
        let member = session
            .meta
            .members
            .iter()
            .find(|m| m.agent_id == agent_id)
            .ok_or_else(|| format!("membre {agent_id} introuvable"))?;

        let turn_req = AgentRoomTurnRequest {
            session_id: req.session_id.clone(),
            agent_id: member.agent_id.clone(),
            display_name: member.display_name.clone(),
            user_message: req.content.clone(),
        };

        let reply = match execute_room_turn(bus, round.as_ref(), &turn_req).await {
            Ok(r) => r,
            Err(e) if e == "tour annulé" => {
                return Ok(AgentRoomConductResponse {
                    agent_turns,
                    cancelled: true,
                });
            }
            Err(e) => return Err(e),
        };
        agent_turns += 1;

        if (agent_turns as usize) >= max {
            break;
        }

        if session.meta.conductor_policy.allow_peer_debate && !peer_followup_used {
            if let Some(peer_id) =
                detect_peer_address(&reply.content, &session.meta.members, &member.agent_id)
            {
                if !queue.iter().any(|id| id == &peer_id) {
                    queue.push(peer_id);
                    peer_followup_used = true;
                }
            }
        }
    }

    Ok(AgentRoomConductResponse {
        agent_turns,
        cancelled: false,
    })
}
