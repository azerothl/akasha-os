//! Runtime bus pour tours de salon (`agent.room_turn` / `agent.room_conduct`).

use crate::persist;
use crate::room_conductor::{
    build_initial_queue, detect_peer_address, effective_max_turns, format_roster_for_prompt,
    sanitize_member_queue,
};
use aos_ipc::BusClient;
use aos_proto::{
    AgentRoomConductRequest, AgentRoomConductResponse, AgentRoomTurnRequest,
    AgentRoomTurnResponse, AgentSpec, CancelRequest, ChatAttachment, ChatMessage,
    ChatRoomMember, ChatSessionAppendRequest, ChatSessionGetResponse, ChatSessionIdRequest, ChatSessionMessage,
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
/// Le transcript inclut déjà le dernier message utilisateur (append platform avant conduct).
pub fn format_transcript_messages(
    session: &ChatSessionGetResponse,
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
    messages
}

fn member_display_name<'a>(
    session: &'a ChatSessionGetResponse,
    agent_id: &str,
) -> Result<&'a str, String> {
    session
        .meta
        .members
        .iter()
        .find(|m| m.agent_id == agent_id)
        .map(|m| m.display_name.as_str())
        .ok_or_else(|| format!("membre {agent_id} absent du salon"))
}

pub fn build_room_system_prompt(
    spec: &AgentSpec,
    display_name: &str,
    members: &[ChatRoomMember],
    canvas_open: bool,
) -> String {
    let roster = format_roster_for_prompt(members);
    let mut out = format!(
        "Tu es {display_name}, membre d'un salon multi-agent in-app. \
         Réponds en une seule prise, de façon concise.\n\
         Membres du salon (tu ne peux @ que ces noms ou ids roster) : {roster}.\n\
         Ne jamais inventer des collègues fictifs (pas de Dessinateur, Moteur de rendu, \
         @agent_id_123, etc.). Ne propose pas agent.spawn pour ajouter des membres.\n\
         Si tu es seul membre, agis toi-même — ne @ personne d'absent.\n\
         Pour interpeller un autre membre présent, utilise @Nom ou @agent_id du roster.\n"
    );
    if canvas_open {
        out.push_str(
            "Le canvas de session est ouvert : si tu as les outils canvas.*, \
             tu peux dessiner ou modifier le dessin toi-même (coords 0..1, commence par canvas.get). \
             Sinon, indique clairement que tu ne peux pas dessiner sans ces outils — \
             ne délègue pas à un agent inventé.\n",
        );
    }
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
                *round.current_inference.lock().await = None;
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
    if session.meta.mode != ChatSessionMode::Room {
        return Err("session n'est pas en mode salon".into());
    }
    let display_name = member_display_name(&session, &req.agent_id)?;
    let spec = persist::read_spec(&req.agent_id).ok_or_else(|| {
        format!("spec introuvable pour le membre {}", req.agent_id)
    })?;

    let system = build_room_system_prompt(
        &spec,
        display_name,
        &session.meta.members,
        session.meta.canvas_open,
    );
    let messages = format_transcript_messages(&session, &system);

    let model_id = spec.model_id.clone().or(session.meta.model_id.clone());
    let content = run_infer(bus, round, model_id, messages).await?;
    if content.is_empty() {
        return Err("réponse vide".into());
    }

    append_room_reply(
        bus,
        &req.session_id,
        &req.agent_id,
        display_name,
        &content,
    )
    .await?;

    Ok(AgentRoomTurnResponse {
        content,
        speaker_id: req.agent_id.clone(),
        speaker_name: display_name.to_string(),
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
    let mut queue =
        sanitize_member_queue(build_initial_queue(&req.content, &session.meta.members), &session.meta.members);
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
            display_name: String::new(),
            user_message: String::new(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::{
        AgentGoal, AgentSpec, ChatRoomConductorPolicy, ChatRoomMember, ChatSessionMessage,
        ChatSessionMeta, ChatSessionMode,
    };

    fn room_session_with_user(content: &str) -> ChatSessionGetResponse {
        ChatSessionGetResponse {
            meta: ChatSessionMeta {
                id: "sess-1".into(),
                title: "Salon".into(),
                created_ms: 1,
                updated_ms: 2,
                archived: false,
                message_count: 1,
                model_id: None,
                mode: ChatSessionMode::Room,
                members: vec![ChatRoomMember {
                    agent_id: "agent-a".into(),
                    display_name: "Alpha".into(),
                    persona_id: None,
                    joined_ms: 1,
                }],
                conductor_policy: ChatRoomConductorPolicy::default(),
                canvas_open: false,
            },
            messages: vec![ChatSessionMessage {
                role: "user".into(),
                content: content.into(),
                ts_ms: 3,
                attachments: vec![],
                speaker_id: None,
                speaker_name: None,
            }],
        }
    }

    #[test]
    fn infer_messages_do_not_duplicate_user_line() {
        let session = room_session_with_user("What do you think?");
        let msgs = format_transcript_messages(&session, "system");
        assert_eq!(msgs.len(), 2, "system + one user line from transcript");
        assert_eq!(
            msgs.iter().filter(|m| m.role == "user").count(),
            1,
            "must not append user_message again"
        );
        assert_eq!(msgs[1].content, "What do you think?");
    }

    #[test]
    fn member_display_name_from_session_not_request() {
        let session = room_session_with_user("hi");
        let name = member_display_name(&session, "agent-a").unwrap();
        assert_eq!(name, "Alpha");
        assert!(member_display_name(&session, "agent-unknown").is_err());
    }

    #[test]
    fn room_system_prompt_lists_roster_and_canvas_hint() {
        let members = vec![ChatRoomMember {
            agent_id: "persona-critic".into(),
            display_name: "Critic".into(),
            persona_id: Some("critic".into()),
            joined_ms: 1,
        }];
        let spec = AgentSpec {
            agent_id: "persona-critic".into(),
            goal: AgentGoal::default(),
            kind: Default::default(),
            display_name: Some("Critic".into()),
            persona_id: Some("critic".into()),
            system_prompt: None,
            skills: vec![],
            tools: vec![],
            mcp_servers: vec![],
            documents: vec![],
            caps: vec![],
            model_id: None,
            parent_id: None,
            session_id: None,
            budget: Default::default(),
            optimize_prompt: false,
        };
        let prompt = build_room_system_prompt(&spec, "Critic", &members, true);
        assert!(prompt.contains("Critic (@persona-critic"));
        assert!(prompt.contains("canvas.*"));
        assert!(prompt.contains("Dessinateur"));
    }
}
