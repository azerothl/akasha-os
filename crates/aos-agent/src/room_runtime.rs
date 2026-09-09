//! Runtime bus pour tours de salon (`agent.room_turn` / `agent.room_conduct`).

use crate::actions::{
    parse_actions, strip_tool_markup, AgentAction, THREAD_FAIL_COULD_NOT_CONTINUE,
};
use crate::artifact_card::{
    attachments_from_artifacts, detect_from_tool, strip_paths_from_prose, ProducedArtifact,
};
use crate::canvas_scene::{
    begin_canvas_vision, canvas_scene_prompt_block, canvas_tool_mutates_scene, end_canvas_vision,
    fetch_canvas_aspect, fetch_canvas_scene_digest, merge_canvas_vision_refs,
    refresh_canvas_scene_after_op, resolve_resident_vision_model, session_model_has_vision,
};
use crate::context_budget::{
    compact_after_prompt_overflow, enforce_prompt_budget, is_prompt_too_long_error, prompt_budget,
    DEFAULT_N_CTX_HINT, MAX_OVERFLOW_INFER_RETRIES,
};
use crate::device_tools::capture_png_path_from_tool_result;
use crate::mcp::open_mcp_tools_with_secrets;
use crate::module_discovery::{
    active_module_names, discover_module_tools, merge_skill_tools_for_modules, tool_in_catalog,
    tool_unavailable_message,
};
use crate::persist;
use crate::room_ask::handle_room_user_ask;
use crate::room_conductor::{
    apply_peer_followups, build_initial_queue, effective_max_turns, effective_peer_followup_budget,
    format_roster_for_prompt, initial_schedule, peers_requesting_response, pop_next_scheduled_turn,
    sanitize_member_queue,
};
use crate::room_reply::split_room_reply;
use crate::skills::load_skills;
use crate::storage_path::{post_room_host_path_notice, ROOM_HOST_PATH_DISALLOWED};
use crate::tool_exec::execute_room_tool;
use crate::tools::{
    canonicalize_tool_name, canvas_tools_from_module_list, caps_for_tools, default_agent_tools,
    merge_canvas_tools, select_tools, ToolDesc,
};
use aos_ipc::BusClient;
use aos_proto::{
    AgentRoomConductRequest, AgentRoomConductResponse, AgentRoomTurnRequest, AgentRoomTurnResponse,
    AgentSpec, CancelRequest, ChatAttachment, ChatMessage, ChatRoomMember,
    ChatSessionAppendRequest, ChatSessionGetResponse, ChatSessionIdRequest, ChatSessionMessage,
    ChatSessionMode, InferParams, InferRequest, ModuleInfo, TokenEvent,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

const TRANSCRIPT_LIMIT: usize = 40;
const MAX_ROOM_TOOL_STEPS: usize = 20;
const ROOM_INFER_MAX_TOKENS: u32 = 768;
/// Below `PREFIX_SPEC_PRIORITY` (2) in `aos-model` so each member turn clears KV and
/// skips prompt-lookup drafting — otherwise prior assistant bubbles in the prompt
/// get replayed verbatim across rebound speakers in the same round.
const ROOM_INFER_PRIORITY: u8 = 1;

/// Sentinel returned to the UI when a salon turn cannot invoke the tools an ask requires.
pub const ROOM_ACTION_UNAVAILABLE: &str = "room_action_unavailable";

const ROOM_ACTION_PROTOCOL: &str = r#"## Protocole d'actions (salon)

Quand tu dois utiliser un outil, réponds par un objet JSON unique :
{"thought":"raisonnement court","action":"<outil>","args":{...}}

- `action` = nom exact du catalogue (`canvas.stroke`, `canvas.get`, …).
- Pour le canvas : coords 0..1, commence par `canvas.get`, omets `session_id` (le runtime le force).
- Couleur : `canvas.set_style` avec `color` #RRGGBB ou `color` sur chaque op — le teal par défaut n'est pas la seule teinte.
- Document / présentation / rapport demandé par l'utilisateur : `files.generate` sous `/downloads/` (md), **pas** `notes.create`.
- Notes du carnet interne uniquement si l'utilisateur demande une *note* — sinon livrable fichier.
- Quand tu as fini (y compris après des outils), réponds en texte libre SANS JSON — c'est ta réplique visible dans le salon.
- `user.ask` : {"question":"...","choices":["option A","option B"]} — pause le tour jusqu'à la réponse humaine dans le fil.
- Pas de `agent.spawn` ni collègues inventés."#;

/// État d'un tour de salon en cours (annulation cooperative).
#[derive(Debug)]
pub struct RoomRoundState {
    pub cancelled: AtomicBool,
    pub cancel_notify: Notify,
    pub current_inference: Mutex<Option<u64>>,
    pub ask_reply_tx: Mutex<Option<tokio::sync::oneshot::Sender<String>>>,
}

impl Default for RoomRoundState {
    fn default() -> Self {
        Self::new()
    }
}

impl RoomRoundState {
    pub fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            cancel_notify: Notify::new(),
            current_inference: Mutex::new(None),
            ask_reply_tx: Mutex::new(None),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.cancel_notify.notify_waiters();
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
        } else if msg.speaker_id.is_some()
            || msg.speaker_name.as_deref().is_some_and(|n| !n.is_empty())
        {
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

fn room_kit_lacks_required_tools(tool_ids: &[String], user_message: &str) -> bool {
    if crate::research_detect::user_requested_document(user_message) {
        return !tool_ids.iter().any(|t| t == "files.generate");
    }
    if crate::research_detect::user_requested_note(user_message) {
        return !tool_ids.iter().any(|t| t == "notes.create");
    }
    false
}

/// Assemble tool ids + caps for a roster member salon turn.
///
/// Built-in personas often ship with empty `tools`/`skills`; merge the same baseline
/// notes/tasks/fs/web kit as solo chat. Custom agents keep the tools and caps granted
/// at creation — baseline and derived caps apply only when the spec ships empty.
pub fn assemble_room_member_tools(
    spec: &AgentSpec,
    canvas_open: bool,
    canvas_exported: &[String],
    user_message: &str,
    installed_modules: &std::collections::HashSet<String>,
    module_tools: &[ToolDesc],
) -> (Vec<String>, Vec<String>) {
    let spec_tools_empty = spec.tools.is_empty();
    let mut skills = spec.skills.clone();
    let mut base_tools = spec.tools.clone();
    if spec_tools_empty {
        for t in default_agent_tools() {
            if !base_tools.iter().any(|x| x == &t) {
                base_tools.push(t);
            }
        }
        if !skills.iter().any(|s| s == "notes-writer") {
            skills.push("notes-writer".into());
        }
    }
    let had_files_generate = base_tools.iter().any(|t| t == "files.generate");
    if crate::research_detect::user_requested_document(user_message) {
        crate::research_detect::ensure_document_file_tools(&mut skills, &mut base_tools);
    }
    let injected_document_tools =
        !had_files_generate && base_tools.iter().any(|t| t == "files.generate");
    let skill_docs = load_skills(&skills);
    let mut tool_ids = merge_skill_tools_for_modules(&base_tools, &skill_docs, installed_modules);
    let canvas_granted = base_tools.iter().any(|t| t.starts_with("canvas."));
    if canvas_open && (canvas_granted || spec_tools_empty) {
        merge_canvas_tools(&mut tool_ids, true, canvas_exported);
    }
    let tools = select_tools(&tool_ids, module_tools);
    let mut caps = spec.caps.clone();
    if spec_tools_empty {
        for c in caps_for_tools(&tools, &spec.mcp_servers) {
            if !caps.contains(&c) {
                caps.push(c);
            }
        }
    } else if injected_document_tools {
        let fg_tools = select_tools(&["files.generate".into()], module_tools);
        for c in caps_for_tools(&fg_tools, &spec.mcp_servers) {
            if !caps.contains(&c) {
                caps.push(c);
            }
        }
    }
    (tool_ids, caps)
}

/// Back-compat alias for [`assemble_room_member_tools`].
pub fn room_member_kit(
    spec: &AgentSpec,
    canvas_open: bool,
    canvas_exported: &[String],
    document_ask: bool,
    installed_modules: &std::collections::HashSet<String>,
    module_tools: &[ToolDesc],
) -> (Vec<String>, Vec<String>) {
    let user_message = if document_ask {
        "prepare a document"
    } else {
        ""
    };
    assemble_room_member_tools(
        spec,
        canvas_open,
        canvas_exported,
        user_message,
        installed_modules,
        module_tools,
    )
}

fn last_user_message_text(session: &ChatSessionGetResponse) -> &str {
    session
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_str())
        .unwrap_or("")
}

pub async fn fetch_canvas_exported_tools(bus: &BusClient) -> Vec<String> {
    bus.call::<(), Vec<ModuleInfo>>("module.list", &(), vec![])
        .await
        .map(|list| canvas_tools_from_module_list(&list))
        .unwrap_or_default()
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

async fn fetch_session(
    bus: &BusClient,
    session_id: &str,
) -> Result<ChatSessionGetResponse, String> {
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
    artifacts: &[ProducedArtifact],
) -> Result<(), String> {
    let visible = strip_paths_from_prose(content, artifacts);
    let mut attachments = vec![ChatAttachment::AgentRef {
        agent_id: speaker_id.to_string(),
        title: speaker_name.to_string(),
        origin: "room".into(),
    }];
    attachments.extend(attachments_from_artifacts(artifacts));
    bus.call::<ChatSessionAppendRequest, ChatSessionMessage>(
        "chat.session.append",
        &ChatSessionAppendRequest {
            session_id: session_id.to_string(),
            role: "assistant".into(),
            content: visible,
            attachments,
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

fn enforce_room_prompt_budget(
    messages: &mut Vec<ChatMessage>,
    n_ctx: usize,
    max_gen: u32,
) -> Option<String> {
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
    loop {
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
        // A model stream can be silent while loading or decoding. Await the
        // notification so the Cancel button remains responsive without polling.
        let ev = tokio::select! {
            ev = rx.recv() => ev,
            _ = round.cancel_notify.notified() => {
                if let Some(id) = *round.current_inference.lock().await {
                    let _ = bus.call::<CancelRequest, bool>(
                        "model.cancel", &CancelRequest { inference_id: id }, vec![]
                    ).await;
                    *round.current_inference.lock().await = None;
                }
                return Err("tour annulé".into());
            }
        };
        let Some(ev) = ev else { break };
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
            Err(e)
                if is_prompt_too_long_error(&e) && prompt_retries < MAX_OVERFLOW_INFER_RETRIES =>
            {
                prompt_retries += 1;
                let mut pairs = chat_messages_as_pairs(messages);
                let _ =
                    compact_after_prompt_overflow(&mut pairs, &mut n_ctx_hint, &mut gen_tokens, &e);
                sync_pairs_to_chat_messages(&pairs, messages);
            }
            Err(e) if is_prompt_too_long_error(&e) => {
                eprintln!("room infer prompt overflow après {prompt_retries} retries : {e}");
                return Err(THREAD_FAIL_COULD_NOT_CONTINUE.into());
            }
            Err(e) => return Err(e),
        }
    }
}

fn room_reply_from_model(
    text: &str,
    parsed: Option<&AgentAction>,
) -> Option<(String, Option<String>)> {
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

#[allow(clippy::too_many_arguments)] // Runtime context is explicit at this orchestration boundary.
async fn run_room_tool_loop(
    bus: &BusClient,
    round: &RoomRoundState,
    agent_id: &str,
    display_name: &str,
    session_id: &str,
    model_id: Option<String>,
    mut messages: Vec<ChatMessage>,
    tool_ids: &[String],
    caps: &[String],
    mcp_servers: &[String],
    images: &[String],
) -> Result<(String, Vec<ProducedArtifact>), String> {
    let (mut mcp_sessions, _) = open_mcp_tools_with_secrets(mcp_servers, &HashMap::new()).await;
    let trace_base = format!(
        "room-{agent_id}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let mut pending_canvas_png: Option<String> = None;
    let mut pending_device_png: Option<String> = None;
    let mut infer_model = model_id.clone();
    let mut produced_artifacts: Vec<ProducedArtifact> = Vec::new();

    for step in 0..MAX_ROOM_TOOL_STEPS {
        let module_tools = discover_module_tools(bus).await;
        let tool_descs = select_tools(tool_ids, &module_tools);
        let has_canvas = tool_descs.iter().any(|t| t.name.starts_with("canvas."));
        let mut step_refs: Vec<String> = if step == 0 { images.to_vec() } else { vec![] };
        if let Some(ref png) = pending_canvas_png.take() {
            if session_model_has_vision(bus, infer_model.as_deref()).await {
                step_refs = merge_canvas_vision_refs(&step_refs, png);
            }
        } else if has_canvas {
            let aspect = fetch_canvas_aspect(bus, session_id).await;
            if let Some(png) =
                begin_canvas_vision(bus, session_id, aspect, infer_model.as_deref()).await
            {
                step_refs = merge_canvas_vision_refs(&step_refs, &png);
            }
        }
        if let Some(ref png) = pending_device_png {
            if let Some(vid) = resolve_resident_vision_model(bus, infer_model.as_deref()).await {
                infer_model = Some(vid);
                if !step_refs.iter().any(|p| p == png) {
                    step_refs.push(png.clone());
                }
            }
        }
        let canvas_active = has_canvas
            && step_refs.iter().any(|p| {
                let lower = p.to_ascii_lowercase();
                lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg")
            });
        let raw_result = run_infer(
            bus,
            round,
            infer_model.clone(),
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
            return Ok((reply, produced_artifacts));
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
                content:
                    "Réponds par un outil JSON valide ou un message texte final pour le salon."
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
            let outcome = if canonicalize_tool_name(&action.action) == "user.ask" {
                handle_room_user_ask(bus, round, session_id, agent_id, display_name, &action.args)
                    .await?
            } else if !tool_in_catalog(&canonicalize_tool_name(&action.action), &tool_descs) {
                tool_unavailable_message(&action.action, "absent du catalogue modules actif")
            } else {
                let mut outcome = execute_room_tool(
                    bus,
                    agent_id,
                    caps,
                    &tool_descs,
                    &action.action,
                    &action.args,
                    &trace_id,
                    Some(session_id),
                    &mut mcp_sessions,
                )
                .await;

                if canvas_tool_mutates_scene(&action.action) {
                    let scene = refresh_canvas_scene_after_op(bus, session_id, &outcome).await;
                    outcome = scene.text;
                    if let Some(png) = scene.png_path {
                        pending_canvas_png = Some(png);
                    }
                }
                if canonicalize_tool_name(&action.action).starts_with("device.camera") {
                    if let Some(path) = capture_png_path_from_tool_result(&outcome) {
                        pending_device_png = Some(path);
                    }
                }
                outcome
            };
            if let Some(art) = detect_from_tool(&action.action, &action.args, &outcome) {
                if let Some(idx) = produced_artifacts.iter().position(|a| a.path == art.path) {
                    produced_artifacts[idx] = art;
                } else {
                    produced_artifacts.push(art);
                }
            }
            if outcome == ROOM_HOST_PATH_DISALLOWED {
                let _ = post_room_host_path_notice(bus, session_id).await;
            }

            messages.push(ChatMessage {
                role: "user".into(),
                content: format!("[outil] {outcome}"),
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
    let mut spec = persist::read_spec(&req.agent_id)
        .ok_or_else(|| format!("spec introuvable pour le membre {}", req.agent_id))?;
    spec.session_id = Some(req.session_id.clone());

    let canvas_exported = if session.meta.canvas_open {
        fetch_canvas_exported_tools(bus).await
    } else {
        Vec::new()
    };
    let user_message = last_user_message_text(&session);
    let module_list = bus
        .call::<(), Vec<ModuleInfo>>("module.list", &(), vec![])
        .await
        .unwrap_or_default();
    let installed_modules = active_module_names(&module_list);
    let module_tools = discover_module_tools(bus).await;
    let (tool_ids, caps) = assemble_room_member_tools(
        &spec,
        session.meta.canvas_open,
        &canvas_exported,
        user_message,
        &installed_modules,
        &module_tools,
    );
    let tool_descs = if tool_ids.is_empty() {
        Vec::new()
    } else {
        select_tools(&tool_ids, &module_tools)
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
    if room_kit_lacks_required_tools(&tool_ids, user_message) {
        return Err(ROOM_ACTION_UNAVAILABLE.into());
    }

    let (content, thinking, artifacts) = if tool_descs.is_empty() {
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
        let (content, thinking) = split_room_reply(&raw);
        (content, thinking, Vec::new())
    } else {
        let (reply, artifacts) = run_room_tool_loop(
            bus,
            round,
            &req.agent_id,
            display_name,
            &req.session_id,
            model_id,
            messages,
            &tool_ids,
            &caps,
            &spec.mcp_servers,
            &images,
        )
        .await?;
        (reply, None, artifacts)
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
        &artifacts,
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
    let peer_budget =
        effective_peer_followup_budget(session.meta.conductor_policy.max_agent_turns_per_user);
    let mut queue = initial_schedule(sanitize_member_queue(
        build_initial_queue(&req.content, &session.meta.members),
        &session.meta.members,
    ));
    queue.truncate(max);
    if queue.is_empty() {
        return Ok(AgentRoomConductResponse {
            agent_turns: 0,
            cancelled: false,
        });
    }

    let mut agent_turns = 0u32;
    let mut initial_done = std::collections::HashSet::<String>::new();
    let mut peer_followups_run = 0u32;

    while (agent_turns as usize) < max {
        if round.is_cancelled() {
            return Ok(AgentRoomConductResponse {
                agent_turns,
                cancelled: true,
            });
        }

        let Some(turn) = pop_next_scheduled_turn(&mut queue, &initial_done) else {
            break;
        };
        let agent_id = turn.agent_id;
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
        if turn.peer_followup {
            peer_followups_run += 1;
        } else {
            initial_done.insert(agent_id);
        }
        agent_turns += 1;

        // Slice C: optional supervisor-directed speaker selection could replace or
        // augment this peer rebound queue without changing mention parsing.
        if session.meta.conductor_policy.allow_peer_debate {
            let peers =
                peers_requesting_response(&reply.content, &session.meta.members, &member.agent_id);
            apply_peer_followups(
                &mut queue,
                &peers,
                &initial_done,
                peer_followups_run,
                peer_budget,
            );
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
    use std::collections::HashSet;

    fn no_modules() -> HashSet<String> {
        HashSet::new()
    }

    fn empty_discovered() -> Vec<ToolDesc> {
        Vec::new()
    }

    fn room_session_with_user(content: &str) -> ChatSessionGetResponse {
        ChatSessionGetResponse {
            meta: ChatSessionMeta {
                id: "sess-1".into(),
                title: "Salon".into(),
                created_ms: 1,
                updated_ms: 2,
                archived: false,
                pinned: false,
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
        const { assert!(ROOM_INFER_PRIORITY < 2) };
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
            policy: None,
            parent_id: None,
            session_id: None,
            budget: Default::default(),
            optimize_prompt: false,
            gate_mode: "ask".into(),
            origin: None,
            cognitive_mode: aos_proto::CognitiveMode::Normal,
        };
        let prompt =
            build_room_system_prompt(&spec, "Critic", &members, false, &[], "sess-1", None);
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
            policy: None,
            parent_id: None,
            session_id: None,
            budget: Default::default(),
            optimize_prompt: false,
            gate_mode: "ask".into(),
            origin: None,
            cognitive_mode: aos_proto::CognitiveMode::Normal,
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
            policy: None,
            parent_id: None,
            session_id: None,
            budget: Default::default(),
            optimize_prompt: false,
            gate_mode: "ask".into(),
            origin: None,
            cognitive_mode: aos_proto::CognitiveMode::Normal,
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
    fn room_member_kit_adds_baseline_tools_when_spec_empty() {
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
            policy: None,
            parent_id: None,
            session_id: None,
            budget: Default::default(),
            optimize_prompt: false,
            gate_mode: "ask".into(),
            origin: None,
            cognitive_mode: aos_proto::CognitiveMode::Normal,
        };
        let (ids, caps) =
            room_member_kit(&spec, false, &[], false, &no_modules(), &empty_discovered());
        assert!(ids.iter().any(|x| x == "notes.create"));
        assert!(caps.iter().any(|c| c == "tool.invoke:notes"));
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
            policy: None,
            parent_id: None,
            session_id: None,
            budget: Default::default(),
            optimize_prompt: false,
            gate_mode: "ask".into(),
            origin: None,
            cognitive_mode: aos_proto::CognitiveMode::Normal,
        };
        use crate::tools::CANVAS_TOOL_IDS;
        let exported: Vec<String> = CANVAS_TOOL_IDS.iter().map(|s| (*s).to_string()).collect();
        let (ids, caps) = room_member_kit(
            &spec,
            true,
            &exported,
            false,
            &no_modules(),
            &empty_discovered(),
        );
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
            policy: None,
            parent_id: None,
            session_id: None,
            budget: Default::default(),
            optimize_prompt: false,
            gate_mode: "ask".into(),
            origin: None,
            cognitive_mode: aos_proto::CognitiveMode::Normal,
        };
        let (ids, caps) =
            room_member_kit(&spec, false, &[], false, &no_modules(), &empty_discovered());
        assert!(!ids.iter().any(|x| x.starts_with("canvas.")));
        assert!(!caps.iter().any(|c| c == "tool.invoke:canvas"));
    }

    #[test]
    fn assemble_room_member_tools_injects_baseline_when_spec_empty() {
        let spec = AgentSpec {
            agent_id: "persona-researcher".into(),
            goal: AgentGoal::default(),
            kind: Default::default(),
            display_name: Some("Researcher".into()),
            persona_id: Some("researcher".into()),
            system_prompt: None,
            skills: vec![],
            tools: vec![],
            mcp_servers: vec![],
            documents: vec![],
            caps: vec![],
            model_id: None,
            policy: None,
            parent_id: None,
            session_id: None,
            budget: Default::default(),
            optimize_prompt: false,
            gate_mode: "ask".into(),
            origin: None,
            cognitive_mode: aos_proto::CognitiveMode::Normal,
        };
        let (ids, caps) = assemble_room_member_tools(
            &spec,
            false,
            &[],
            "écris une note rapide",
            &no_modules(),
            &empty_discovered(),
        );
        assert!(ids.iter().any(|x| x == "notes.create"));
        assert!(caps.iter().any(|c| c == "tool.invoke:notes"));
        assert!(!ids.iter().any(|x| x == "files.generate"));
    }

    #[test]
    fn assemble_room_member_tools_injects_files_generate_on_document_ask() {
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
            policy: None,
            parent_id: None,
            session_id: None,
            budget: Default::default(),
            optimize_prompt: false,
            gate_mode: "ask".into(),
            origin: None,
            cognitive_mode: aos_proto::CognitiveMode::Normal,
        };
        let (ids, caps) = assemble_room_member_tools(
            &spec,
            false,
            &[],
            "prepare a document about rust",
            &no_modules(),
            &empty_discovered(),
        );
        assert!(ids.iter().any(|x| x == "files.generate"));
        assert!(caps.iter().any(|c| c == "fs.write:/downloads/**"));
    }

    #[test]
    fn room_member_kit_injects_files_generate_on_document_ask() {
        let spec = AgentSpec {
            agent_id: "agent-x".into(),
            goal: AgentGoal::default(),
            kind: Default::default(),
            display_name: None,
            persona_id: None,
            system_prompt: None,
            skills: vec!["notes-writer".into()],
            tools: vec!["notes.create".into()],
            mcp_servers: vec![],
            documents: vec![],
            caps: vec![],
            model_id: None,
            policy: None,
            parent_id: None,
            session_id: None,
            budget: Default::default(),
            optimize_prompt: false,
            gate_mode: "ask".into(),
            origin: None,
            cognitive_mode: aos_proto::CognitiveMode::Normal,
        };
        let (ids, caps) =
            room_member_kit(&spec, false, &[], true, &no_modules(), &empty_discovered());
        assert!(ids.iter().any(|x| x == "files.generate"));
        assert!(caps.iter().any(|c| c == "fs.write:/downloads/**"));
        let (ids_off, _) =
            room_member_kit(&spec, false, &[], false, &no_modules(), &empty_discovered());
        assert!(!ids_off.iter().any(|x| x == "files.generate"));
    }

    #[test]
    fn room_kit_lacks_required_tools_detects_missing_note_tool() {
        assert!(room_kit_lacks_required_tools(
            &["canvas.stroke".into()],
            "écris une note"
        ));
        assert!(!room_kit_lacks_required_tools(
            &["notes.create".into()],
            "écris une note"
        ));
        assert!(room_kit_lacks_required_tools(
            &["notes.create".into()],
            "prepare a document about rust"
        ));
        assert!(!room_kit_lacks_required_tools(
            &["files.generate".into()],
            "prepare a document about rust"
        ));
    }

    #[test]
    fn execute_room_turn_rejects_missing_note_tool_kit() {
        let session = room_session_with_user("écris une note rapide");
        let spec = AgentSpec {
            agent_id: "agent-a".into(),
            goal: AgentGoal::default(),
            kind: Default::default(),
            display_name: Some("Alpha".into()),
            persona_id: None,
            system_prompt: None,
            skills: vec![],
            tools: vec!["canvas.stroke".into()],
            mcp_servers: vec![],
            documents: vec![],
            caps: vec![],
            model_id: None,
            policy: None,
            parent_id: None,
            session_id: None,
            budget: Default::default(),
            optimize_prompt: false,
            gate_mode: "ask".into(),
            origin: None,
            cognitive_mode: aos_proto::CognitiveMode::Normal,
        };
        let user_message = last_user_message_text(&session);
        let (tool_ids, _) = assemble_room_member_tools(
            &spec,
            false,
            &[],
            user_message,
            &no_modules(),
            &empty_discovered(),
        );
        assert!(room_kit_lacks_required_tools(&tool_ids, user_message));
        assert_eq!(ROOM_ACTION_UNAVAILABLE, "room_action_unavailable");
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

    #[test]
    fn room_action_protocol_allows_user_ask_not_spawn() {
        assert!(ROOM_ACTION_PROTOCOL.contains("user.ask"));
        assert!(!ROOM_ACTION_PROTOCOL.contains("agent.spawn :"));
        assert!(ROOM_ACTION_PROTOCOL.contains("Pas de `agent.spawn`"));
    }

    #[test]
    fn custom_agent_keeps_granted_tools_and_caps_without_canvas_floor() {
        use crate::tools::CANVAS_TOOL_IDS;
        let spec = AgentSpec {
            agent_id: "agent-custom".into(),
            goal: AgentGoal::default(),
            kind: Default::default(),
            display_name: Some("Custom".into()),
            persona_id: None,
            system_prompt: None,
            skills: vec![],
            tools: vec!["web.search".into()],
            mcp_servers: vec![],
            documents: vec![],
            caps: vec!["net.connect:*:*".into()],
            model_id: None,
            policy: None,
            parent_id: None,
            session_id: None,
            budget: Default::default(),
            optimize_prompt: false,
            gate_mode: "ask".into(),
            origin: None,
            cognitive_mode: aos_proto::CognitiveMode::Normal,
        };
        let exported: Vec<String> = CANVAS_TOOL_IDS.iter().map(|s| (*s).to_string()).collect();
        let (ids, caps) = assemble_room_member_tools(
            &spec,
            true,
            &exported,
            "",
            &no_modules(),
            &empty_discovered(),
        );
        assert!(ids.iter().any(|x| x == "web.search"));
        assert!(!ids.iter().any(|x| x.starts_with("canvas.")));
        assert!(!ids.iter().any(|x| x == "notes.create"));
        assert_eq!(caps, vec!["net.connect:*:*".to_string()]);
    }
}
