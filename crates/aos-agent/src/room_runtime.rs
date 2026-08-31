//! Runtime bus pour tours de salon (`agent.room_turn` / `agent.room_conduct`).

use crate::actions::{parse_actions, strip_tool_markup, AgentAction, THREAD_FAIL_COULD_NOT_CONTINUE};
use crate::canvas_scene::{
    begin_canvas_vision, canvas_scene_prompt_block, canvas_tool_mutates_scene,
    end_canvas_vision, fetch_canvas_aspect,
    fetch_canvas_scene_digest, merge_canvas_vision_refs, refresh_canvas_scene_after_op,
    session_model_has_vision,
};
use crate::context_budget::{
    compact_after_prompt_overflow, enforce_prompt_budget, is_prompt_too_long_error,
    prompt_budget, DEFAULT_N_CTX_HINT, MAX_OVERFLOW_INFER_RETRIES,
};
use crate::mcp::open_mcp_tools_with_secrets;
use crate::persist;
use crate::room_conductor::{
    build_initial_queue, detect_peer_addresses, effective_max_turns, format_roster_for_prompt,
    sanitize_member_queue,
};
use crate::room_reply::split_room_reply;
use crate::skills::{load_skills, merge_skill_tools};
use crate::tool_exec::execute_room_tool;
use crate::tools::{
    caps_for_tools, merge_canvas_tools, select_tools, ToolDesc,
};
use aos_ipc::BusClient;
use aos_proto::{
    AgentRoomConductRequest, AgentRoomConductResponse, AgentRoomTurnRequest,
    AgentRoomTurnResponse, AgentSpec, CancelRequest, ChatAttachment, ChatMessage,
    ChatRoomMember, ChatSessionAppendRequest, ChatSessionGetResponse, ChatSessionIdRequest,
    ChatSessionMessage, ChatSessionMode, InferParams, InferRequest, TokenEvent,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

const TRANSCRIPT_LIMIT: usize = 40;
const MAX_ROOM_TOOL_STEPS: usize = 20;
const ROOM_INFER_MAX_TOKENS: u32 = 768;
/// Below `PREFIX_SPEC_PRIORITY` (2) in `aos-model` so each member turn clears KV and
/// skips prompt-lookup drafting — otherwise prior assistant bubbles in the prompt
/// get replayed verbatim across rebound speakers in the same round.
const ROOM_INFER_PRIORITY: u8 = 1;

const ROOM_ACTION_PROTOCOL: &str = r#"## Protocole d'actions (salon)

Quand tu dois utiliser un outil, réponds par un objet JSON unique :
{"thought":"raisonnement court","action":"<outil>","args":{...}}

- `action` = nom exact du catalogue (`canvas.stroke`, `canvas.get`, …).
- Pour le canvas : coords 0..1, commence par `canvas.get`, omets `session_id` (le runtime le force).
- Couleur : `canvas.set_style` avec `color` #RRGGBB ou `color` sur chaque op — le teal par défaut n'est pas la seule teinte.
- Quand tu as fini (y compris après des outils), réponds en texte libre SANS JSON — c'est ta réplique visible dans le salon.
- Pas de `agent.spawn`, `user.ask`, ni collègues inventés."#;

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

/// Formate le transcript session pour l'inférence.
///
/// Messages humains : `role: user`. Interventions salon précédentes : aussi `role: user`
/// avec attribution `[Salon — Name] …` — pas `role: assistant` + `(Name)`, sinon le
/// template ChatML se termine sur un tour assistant et le modèle continue/copie la
/// dernière bulle. Le transcript inclut déjà le message utilisateur déclencheur
/// (append platform avant conduct) ; `AgentRoomTurnRequest.user_message` ne doit pas
/// être réinjecté (voir test `infer_messages_do_not_duplicate_user_line`).
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
        } else if msg.speaker_id.is_some() || msg.speaker_name.as_deref().is_some_and(|n| !n.is_empty()) {
            let label = msg
                .speaker_name
                .as_deref()
                .filter(|n| !n.is_empty())
                .unwrap_or("assistant");
            messages.push(ChatMessage {
                role: "user".into(),
                content: format!("[Salon — {label}] {}", msg.content),
            });
        } else {
            messages.push(ChatMessage {
                role: "assistant".into(),
                content: msg.content.clone(),
            });
        }
    }
    messages
}

/// Synthetic user line so the chat template ends on a user turn and the model is
/// nudged to answer as this roster member instead of continuing the last bubble.
pub fn append_room_turn_nudge(messages: &mut Vec<ChatMessage>, display_name: &str) {
    messages.push(ChatMessage {
        role: "user".into(),
        content: format!(
            "[Salon — tour de {display_name}] Réponds avec ta propre voix et ton rôle. \
             Ne recopie pas les messages précédents ; apporte une contribution distincte."
        ),
    });
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

/// Assemble tool ids + caps for a roster member turn.
pub fn room_member_kit(spec: &AgentSpec, canvas_open: bool) -> (Vec<String>, Vec<String>) {
    let skill_docs = load_skills(&spec.skills);
    let mut tool_ids = merge_skill_tools(&spec.tools, &skill_docs);
    merge_canvas_tools(&mut tool_ids, canvas_open);
    let tools = select_tools(&tool_ids, &[]);
    let mut caps = spec.caps.clone();
    for c in caps_for_tools(&tools, &spec.mcp_servers) {
        if !caps.contains(&c) {
            caps.push(c);
        }
    }
    (tool_ids, caps)
}

pub fn build_room_system_prompt(
    spec: &AgentSpec,
    display_name: &str,
    members: &[ChatRoomMember],
    canvas_open: bool,
    tools: &[ToolDesc],
    session_id: &str,
    canvas_scene_digest: Option<&str>,
) -> String {
    let roster = format_roster_for_prompt(members);
    let mut out = format!(
        "Tu es {display_name}, membre d'un salon multi-agent in-app. \
         Réponds en une seule prise, de façon concise.\n\
         Membres du salon (tu ne peux @ que ces noms) : {roster}.\n\
         Ne recopie pas les interventions précédentes du fil ; apporte ta propre perspective \
         en tant que {display_name} (pas de copier-coller, pas de préfixe `(Autre)`).\n\
         Ne jamais inventer des collègues fictifs (pas de Dessinateur, Moteur de rendu, \
         @agent_id_123, etc.). Ne propose pas agent.spawn pour ajouter des membres.\n\
         Si tu es seul membre, agis toi-même — ne @ personne d'absent.\n\
         Pour interpeller un autre membre présent, utilise @Nom du roster.\n"
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
    if !tools.is_empty() {
        out.push_str("\n## Outils disponibles\n");
        for t in tools {
            out.push_str(&format!(
                "- `{}` : {} | schema: {}\n",
                t.name, t.description, t.input_schema
            ));
        }
        if tools.iter().any(|t| t.name.starts_with("canvas.")) {
            out.push_str(&format!(
                "\nCanvas de session lié : `{session_id}`. \
                 Omets `session_id` dans les args canvas (le runtime le force).\n"
            ));
            if let Some(digest) = canvas_scene_digest {
                out.push_str(&canvas_scene_prompt_block(digest));
                out.push('\n');
            }
        }
        out.push('\n');
        out.push_str(ROOM_ACTION_PROTOCOL);
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
    thinking: Option<&str>,
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
            thinking: thinking.map(str::to_string),
        },
        vec![],
    )
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

fn room_images_from_session(session: &ChatSessionGetResponse) -> Vec<String> {
    session
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| {
            m.attachments
                .iter()
                .filter_map(|a| match a {
                    ChatAttachment::Image { path, .. } => Some(path.clone()),
                    _ => None,
                })
                .take(4)
                .collect()
        })
        .unwrap_or_default()
}

fn chat_messages_as_pairs(messages: &[ChatMessage]) -> Vec<(String, String)> {
    messages
        .iter()
        .map(|m| (m.role.clone(), m.content.clone()))
        .collect()
}

fn sync_pairs_to_chat_messages(pairs: &[(String, String)], messages: &mut Vec<ChatMessage>) {
    messages.clear();
    messages.extend(pairs.iter().map(|(role, content)| ChatMessage {
        role: role.clone(),
        content: content.clone(),
    }));
}

fn enforce_room_prompt_budget(messages: &mut Vec<ChatMessage>, n_ctx: usize, max_gen: u32) -> Option<String> {
    let mut pairs = chat_messages_as_pairs(messages);
    let budget = prompt_budget(n_ctx, max_gen);
    let note = enforce_prompt_budget(&mut pairs, budget, 6)?;
    sync_pairs_to_chat_messages(&pairs, messages);
    Some(note)
}

#[cfg(test)]
fn compact_room_messages_for_overflow(
    messages: &mut Vec<ChatMessage>,
    n_ctx: usize,
    max_gen: u32,
) -> Option<String> {
    let mut pairs = chat_messages_as_pairs(messages);
    let note = crate::context_budget::aggressive_trim_for_overflow(&mut pairs, n_ctx, max_gen)?;
    sync_pairs_to_chat_messages(&pairs, messages);
    Some(note)
}

async fn run_infer_once(
    bus: &BusClient,
    round: &RoomRoundState,
    model_id: Option<String>,
    messages: &[ChatMessage],
    infer_caps: &[String],
    images: &[String],
    max_tokens: u32,
) -> Result<String, String> {
    let req = InferRequest {
        model_id,
        messages: messages.to_vec(),
        params: InferParams {
            max_tokens,
            temperature: 0.3,
            ..InferParams::default()
        },
        priority: ROOM_INFER_PRIORITY,
        data_refs: images.to_vec(),
        images: images.to_vec(),
        routing: None,
    };
    let mut rx = bus
        .call_stream::<InferRequest, TokenEvent>("model.infer", &req, infer_caps.to_vec())
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

async fn run_infer(
    bus: &BusClient,
    round: &RoomRoundState,
    model_id: Option<String>,
    messages: &mut Vec<ChatMessage>,
    infer_caps: &[String],
    images: &[String],
) -> Result<String, String> {
    let mut n_ctx_hint = DEFAULT_N_CTX_HINT;
    let mut gen_tokens = ROOM_INFER_MAX_TOKENS;
    let mut prompt_retries = 0u32;

    if let Some(_note) = enforce_room_prompt_budget(messages, n_ctx_hint, gen_tokens) {
        // compaction pré-infer (aligné worker)
    }

    loop {
        match run_infer_once(
            bus,
            round,
            model_id.clone(),
            messages,
            infer_caps,
            images,
            gen_tokens,
        )
        .await
        {
            Ok(text) => return Ok(text),
            Err(e) if e == "tour annulé" => return Err(e),
            Err(e) if is_prompt_too_long_error(&e) && prompt_retries < MAX_OVERFLOW_INFER_RETRIES => {
                prompt_retries += 1;
                let mut pairs = chat_messages_as_pairs(messages);
                let _ = compact_after_prompt_overflow(&mut pairs, &mut n_ctx_hint, &mut gen_tokens, &e);
                sync_pairs_to_chat_messages(&pairs, messages);
            }
            Err(e) if is_prompt_too_long_error(&e) => {
                eprintln!(
                    "room infer prompt overflow après {prompt_retries} retries : {e}"
                );
                return Err(THREAD_FAIL_COULD_NOT_CONTINUE.into());
            }
            Err(e) => return Err(e),
        }
    }
}

fn room_reply_from_model(text: &str, parsed: Option<&AgentAction>) -> Option<(String, Option<String>)> {
    if parsed.is_some() {
        return None;
    }
    let (visible, thinking) = split_room_reply(text);
    if visible.trim().is_empty() {
        None
    } else {
        Some((visible, thinking))
    }
}

async fn run_room_tool_loop(
    bus: &BusClient,
    round: &RoomRoundState,
    agent_id: &str,
    session_id: &str,
    model_id: Option<String>,
    mut messages: Vec<ChatMessage>,
    tool_descs: &[ToolDesc],
    caps: &[String],
    mcp_servers: &[String],
    images: &[String],
) -> Result<String, String> {
    let (mut mcp_sessions, _) = open_mcp_tools_with_secrets(mcp_servers, &HashMap::new()).await;
    let trace_base = format!(
        "room-{agent_id}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let mut pending_canvas_png: Option<String> = None;

    for step in 0..MAX_ROOM_TOOL_STEPS {
        let has_canvas = tool_descs.iter().any(|t| t.name.starts_with("canvas."));
        let mut step_refs: Vec<String> = if step == 0 {
            images.to_vec()
        } else {
            vec![]
        };
        if let Some(ref png) = pending_canvas_png.take() {
            if session_model_has_vision(bus, model_id.as_deref()).await {
                step_refs = merge_canvas_vision_refs(&step_refs, png);
            }
        } else if has_canvas {
            let aspect = fetch_canvas_aspect(bus, session_id).await;
            if let Some(png) =
                begin_canvas_vision(bus, session_id, aspect, model_id.as_deref()).await
            {
                step_refs = merge_canvas_vision_refs(&step_refs, &png);
            }
        }
        let canvas_active = has_canvas && step_refs.iter().any(|p| {
            let lower = p.to_ascii_lowercase();
            lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg")
        });
        let raw_result = run_infer(
            bus,
            round,
            model_id.clone(),
            &mut messages,
            caps,
            &step_refs,
        )
        .await;
        if canvas_active {
            end_canvas_vision(bus, session_id).await;
        }
        let raw = raw_result?;
        if raw.is_empty() {
            return Err("réponse vide".into());
        }

        let parsed_actions = parse_actions(&raw);
        if let Some((reply, _thinking)) = room_reply_from_model(&raw, parsed_actions.first()) {
            return Ok(reply);
        }

        if parsed_actions.is_empty() {
            if step + 1 >= MAX_ROOM_TOOL_STEPS {
                return Err("trop d'étapes sans réponse texte".into());
            }
            messages.push(ChatMessage {
                role: "assistant".into(),
                content: raw,
            });
            messages.push(ChatMessage {
                role: "user".into(),
                content: "Réponds par un outil JSON valide ou un message texte final pour le salon."
                    .into(),
            });
            continue;
        }

        let assistant_content = strip_tool_markup(&raw);
        messages.push(ChatMessage {
            role: "assistant".into(),
            content: if assistant_content.is_empty() {
                "[outil]".into()
            } else {
                assistant_content
            },
        });

        for action in parsed_actions {
            let trace_id = format!("{trace_base}-{step}");
            let mut outcome = execute_room_tool(
                bus,
                agent_id,
                caps,
                tool_descs,
                &action.action,
                &action.args,
                &trace_id,
                Some(session_id),
                &mut mcp_sessions,
            )
            .await;

            if canvas_tool_mutates_scene(&action.action) {
                let scene =
                    refresh_canvas_scene_after_op(bus, session_id, &outcome).await;
                outcome = scene.text;
                if let Some(png) = scene.png_path {
                    pending_canvas_png = Some(png);
                }
            }

            messages.push(ChatMessage {
                role: "user".into(),
                content: format!("[outil {}] {outcome}", action.action),
            });
        }
    }

    Err("limite d'outils salon atteinte".into())
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
    let mut spec = persist::read_spec(&req.agent_id).ok_or_else(|| {
        format!("spec introuvable pour le membre {}", req.agent_id)
    })?;
    spec.session_id = Some(req.session_id.clone());

    let (tool_ids, caps) = room_member_kit(&spec, session.meta.canvas_open);
    let tool_descs = if tool_ids.is_empty() {
        Vec::new()
    } else {
        select_tools(&tool_ids, &[])
    };

    let canvas_digest = if session.meta.canvas_open {
        fetch_canvas_scene_digest(bus, &req.session_id).await
    } else {
        None
    };

    let system = build_room_system_prompt(
        &spec,
        display_name,
        &session.meta.members,
        session.meta.canvas_open,
        &tool_descs,
        &req.session_id,
        canvas_digest.as_deref(),
    );
    let mut messages = format_transcript_messages(&session, &system);
    append_room_turn_nudge(&mut messages, display_name);

    let model_id = spec.model_id.clone().or(session.meta.model_id.clone());
    let images = room_images_from_session(&session);
    let (content, thinking) = if tool_descs.is_empty() {
        let mut refs = images.clone();
        let canvas_png = if session.meta.canvas_open {
            begin_canvas_vision(
                bus,
                &req.session_id,
                session.meta.canvas_aspect,
                model_id.as_deref(),
            )
            .await
        } else {
            None
        };
        if let Some(ref png) = canvas_png {
            refs = merge_canvas_vision_refs(&refs, png);
        }
        let raw = run_infer(
            bus,
            round,
            model_id,
            &mut messages,
            &room_turn_infer_caps(),
            &refs,
        )
        .await;
        if canvas_png.is_some() {
            end_canvas_vision(bus, &req.session_id).await;
        }
        let raw = raw?;
        split_room_reply(&raw)
    } else {
        let reply = run_room_tool_loop(
            bus,
            round,
            &req.agent_id,
            &req.session_id,
            model_id,
            messages,
            &tool_descs,
            &caps,
            &spec.mcp_servers,
            &images,
        )
        .await?;
        (reply, None)
    };
    if content.is_empty() {
        return Err("réponse vide".into());
    }

    append_room_reply(
        bus,
        &req.session_id,
        &req.agent_id,
        display_name,
        &content,
        thinking.as_deref(),
    )
    .await?;

    Ok(AgentRoomTurnResponse {
        content,
        speaker_id: req.agent_id.clone(),
        speaker_name: display_name.to_string(),
        thinking,
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
    queue.truncate(max);
    if queue.is_empty() {
        return Ok(AgentRoomConductResponse {
            agent_turns: 0,
            cancelled: false,
        });
    }

    let mut agent_turns = 0u32;
    let mut spoken = std::collections::HashSet::<String>::new();

    while (agent_turns as usize) < max {
        if round.is_cancelled() {
            return Ok(AgentRoomConductResponse {
                agent_turns,
                cancelled: true,
            });
        }

        let agent_id = loop {
            if queue.is_empty() {
                break None;
            }
            let id = queue.remove(0);
            if spoken.contains(&id) {
                continue;
            }
            break Some(id);
        };
        let Some(agent_id) = agent_id else {
            break;
        };
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
        spoken.insert(agent_id);
        agent_turns += 1;

        if (agent_turns as usize) >= max {
            break;
        }

        if session.meta.conductor_policy.allow_peer_debate {
            for peer_id in detect_peer_addresses(
                &reply.content,
                &session.meta.members,
                &member.agent_id,
            ) {
                if spoken.contains(&peer_id) {
                    continue;
                }
                queue.retain(|id| id != &peer_id);
                queue.insert(0, peer_id);
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
                canvas_aspect: aos_proto::CanvasAspect::Square,
            },
            messages: vec![ChatSessionMessage {
                role: "user".into(),
                content: content.into(),
                ts_ms: 3,
                attachments: vec![],
                speaker_id: None,
                speaker_name: None,
                        thinking: None,
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
    fn turn_nudge_appended_after_transcript() {
        let session = room_session_with_user("Quels manques?");
        let mut msgs = format_transcript_messages(&session, "system");
        append_room_turn_nudge(&mut msgs, "Critic");
        assert_eq!(msgs.last().unwrap().role, "user");
        assert!(msgs.last().unwrap().content.contains("Critic"));
        assert!(msgs.last().unwrap().content.contains("Ne recopie pas"));
    }

    #[test]
    fn prior_room_replies_are_user_attribution_not_assistant() {
        let mut session = room_session_with_user("Quels manques?");
        session.messages.push(ChatSessionMessage {
            role: "assistant".into(),
            content: "Phase 1 : audit".into(),
            ts_ms: 4,
            attachments: vec![],
            speaker_id: Some("persona-planner".into()),
            speaker_name: Some("Planner".into()),
            thinking: None,
        });
        let msgs = format_transcript_messages(&session, "system");
        assert_eq!(
            msgs.iter().filter(|m| m.role == "assistant").count(),
            0,
            "room peers must not flatten to assistant turns"
        );
        let planner = msgs
            .iter()
            .find(|m| m.content.contains("Phase 1"))
            .expect("planner line");
        assert_eq!(planner.role, "user");
        assert!(planner.content.starts_with("[Salon — Planner]"));
    }

    #[test]
    fn infer_prompt_ends_on_user_turn_after_nudge() {
        let mut session = room_session_with_user("Quels manques?");
        session.messages.push(ChatSessionMessage {
            role: "assistant".into(),
            content: "Plan détaillé".into(),
            ts_ms: 4,
            attachments: vec![],
            speaker_id: Some("persona-planner".into()),
            speaker_name: Some("Planner".into()),
            thinking: None,
        });
        let mut msgs = format_transcript_messages(&session, "system");
        append_room_turn_nudge(&mut msgs, "Critic");
        assert_eq!(msgs.last().unwrap().role, "user");
        assert_eq!(
            msgs.iter().filter(|m| m.role == "assistant").count(),
            0,
            "no assistant turns before generation"
        );
    }

    #[test]
    fn room_infer_priority_disables_prefix_spec_path() {
        assert!(ROOM_INFER_PRIORITY < 2);
    }

    #[test]
    fn room_system_prompt_anti_echo() {
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
            gate_mode: "ask".into(),
            origin: None,
        };
        let prompt = build_room_system_prompt(&spec, "Critic", &members, false, &[], "sess-1", None);
        assert!(prompt.contains("Ne recopie pas"));
        assert!(prompt.contains("Critic"));
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
            gate_mode: "ask".into(),
            origin: None,
        };
        let prompt = build_room_system_prompt(&spec, "Critic", &members, true, &[], "sess-1", None);
        assert!(prompt.contains("Critic"));
        assert!(!prompt.contains("@persona-critic"));
        assert!(prompt.contains("canvas.*"));
        assert!(prompt.contains("Dessinateur"));
    }

    #[test]
    fn room_system_prompt_includes_scene_digest_when_provided() {
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
            tools: vec!["canvas.stroke".into()],
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
        };
        let tools = select_tools(&spec.tools, &[]);
        let digest = "next_seq=2 aspect=square 1:1 ops=1\ncounts: stroke=1\nseq=1 stroke (0.1,0.1)-(0.2,0.2)";
        let prompt = build_room_system_prompt(
            &spec,
            "Critic",
            &members,
            true,
            &tools,
            "sess-1",
            Some(digest),
        );
        assert!(prompt.contains("seq=1"));
        assert!(prompt.contains("canvas.get"));
    }

    #[test]
    fn room_member_kit_adds_canvas_when_open() {
        let spec = AgentSpec {
            agent_id: "persona-coder".into(),
            goal: AgentGoal::default(),
            kind: Default::default(),
            display_name: Some("Coder".into()),
            persona_id: Some("coder".into()),
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
            gate_mode: "ask".into(),
            origin: None,
        };
        let (ids, caps) = room_member_kit(&spec, true);
        assert!(ids.iter().any(|x| x == "canvas.set_style"));
        assert!(ids.iter().any(|x| x == "canvas.stroke"));
        assert!(ids.iter().any(|x| x == "canvas.line"));
        assert!(ids.iter().any(|x| x == "canvas.spline"));
        assert!(ids.iter().any(|x| x == "canvas.path"));
        assert!(!ids.iter().any(|x| x == "canvas.fill"));
        assert!(ids.iter().any(|x| x == "canvas.get"));
        assert!(caps.iter().any(|c| c == "tool.invoke:canvas"));
    }

    #[test]
    fn room_member_kit_no_canvas_when_closed() {
        let spec = AgentSpec {
            agent_id: "agent-x".into(),
            goal: AgentGoal::default(),
            kind: Default::default(),
            display_name: None,
            persona_id: None,
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
            gate_mode: "ask".into(),
            origin: None,
        };
        let (ids, caps) = room_member_kit(&spec, false);
        assert!(!ids.iter().any(|x| x.starts_with("canvas.")));
        assert!(!caps.iter().any(|c| c == "tool.invoke:canvas"));
    }

    #[test]
    fn room_messages_compact_on_overflow_signal() {
        let mut msgs = vec![
            ChatMessage {
                role: "system".into(),
                content: "system ".repeat(500),
            },
            ChatMessage {
                role: "user".into(),
                content: "draw a house".into(),
            },
        ];
        for i in 0..20 {
            msgs.push(ChatMessage {
                role: "assistant".into(),
                content: format!("(Alpha) step {i} {}", "y".repeat(800)),
            });
            msgs.push(ChatMessage {
                role: "user".into(),
                content: format!("tool {i} {}", "z".repeat(600)),
            });
        }
        let err = "le prompt ne tient pas dans le contexte (prompt=8749 + réserve_gen=520 = 9269 tokens > ctx=9216)";
        assert!(crate::context_budget::is_prompt_too_long_error(err));
        let note = compact_room_messages_for_overflow(&mut msgs, 9216, 512);
        assert!(note.is_some());
        let pairs = chat_messages_as_pairs(&msgs);
        let after = crate::context_budget::estimate_messages_tokens(&pairs);
        assert!(after < 8749);
    }
}
