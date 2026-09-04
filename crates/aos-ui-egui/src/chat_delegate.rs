//! Chat-to-agent delegation, capability selection, and agent launch orchestration.

use crate::cmd::Evt;
use crate::{agent_panel, chat_canvas, CHAT_AGENT_MAX_SUBAGENTS};
use aos_ipc::BusClient;
use aos_proto::{
    chat_tts_request, chat_user_wants_module_authoring, AgentCreateRequest, AgentGoal,
    ChatAttachment, ChatSessionAppendRequest, CognitiveMode, ModelInfo, ModelState,
};
use std::sync::mpsc::Sender;
use std::sync::Arc;

fn chat_action_is_self_tool(action: &str) -> bool {
    matches!(
        action,
        "module.scaffold" | "module.package" | "module.install" | "module.uninstall" | "skill.create"
    )
}

/// Détecte l'activation Deep Thinking (JSON `mode` ou phrase FR/EN).
pub(crate) fn user_wants_deep_thinking(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(t) {
        if let Some(mode) = v.get("mode").and_then(|m| m.as_str()) {
            if mode.eq_ignore_ascii_case("deep_thinking") || mode.eq_ignore_ascii_case("deep-thinking")
            {
                return true;
            }
        }
    }
    // Embedded JSON fragment
    if t.contains("\"mode\"") {
        if let Some(start) = t.find('{') {
            if let Some(end) = t.rfind('}') {
                if end > start {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t[start..=end]) {
                        if v.get("mode")
                            .and_then(|m| m.as_str())
                            .is_some_and(|m| {
                                m.eq_ignore_ascii_case("deep_thinking")
                                    || m.eq_ignore_ascii_case("deep-thinking")
                            })
                        {
                            return true;
                        }
                    }
                }
            }
        }
    }
    let lower = t.to_ascii_lowercase();
    lower.contains("deep thinking")
        || lower.contains("deep_thinking")
        || lower.contains("deep-thinking")
        || lower.contains("active le deep thinking")
        || lower.contains("active deep thinking")
        || lower.contains("activer le deep thinking")
        || lower.contains("enable deep thinking")
        || lower.contains("mode deep thinking")
}

/// Retire la consigne d'activation du brief (garde le reste de la tâche).
pub(crate) fn strip_deep_thinking_activation(text: &str) -> String {
    let mut out = text.to_string();
    for phrase in [
        "Active le deep thinking pour cette tâche.",
        "Active le deep thinking pour cette tâche",
        "Activer le deep thinking pour cette tâche.",
        "Enable deep thinking for this task.",
        "Enable deep thinking for this task",
        "Active deep thinking.",
        "Active le deep thinking.",
    ] {
        out = out.replace(phrase, "");
    }
    out.trim().to_string()
}

fn merge_named_args(dst: &mut Vec<String>, args: &serde_json::Value, key: &str) {
    let Some(arr) = args.get(key).and_then(|v| v.as_array()) else {
        return;
    };
    for item in arr {
        if let Some(name) = item.as_str() {
            if !dst.iter().any(|x| x == name) {
                dst.push(name.to_string());
            }
        }
    }
}

/// Retire les outils incompatibles avec le kit canvas (vectoriel) vs pixel (diffusion).
fn strip_delegate_kit_tools(tools: &mut Vec<String>, skills: &mut Vec<String>, use_canvas: bool) {
    if use_canvas {
        tools.retain(|t| {
            t.starts_with("canvas.") || t == "plan.update"
        });
        // A canvas author needs a compact geometric context. Notes/tasks and
        // their long skill instructions caused the model to archive the
        // drawing mid-run instead of continuing the composition.
        skills.clear();
    } else {
        tools.retain(|t| !t.starts_with("canvas."));
    }
}

/// A canvas critic needs pixels, not merely a capable model installed on disk.
/// Keep an explicitly selected chat model untouched; only fill an absent model
/// with a vision-capable model that is already resident.
pub(crate) fn canvas_model_id(
    selected: Option<String>,
    available: &[ModelInfo],
) -> Option<String> {
    selected.or_else(|| {
        available
            .iter()
            .find(|model| {
                model.has_vision
                    && matches!(model.state, ModelState::Loaded | ModelState::PartiallyOffloaded)
            })
            .map(|model| model.id.clone())
    })
}

pub(crate) fn chat_delegate_kit(
    brief: &str,
    canvas_open: bool,
    use_canvas: bool,
    canvas_exported: &[String],
) -> (Vec<String>, Vec<String>) {
    let (mut skills, mut tools) =
        chat_agent_kit_ex(brief, canvas_open || use_canvas, canvas_exported);
    strip_delegate_kit_tools(&mut tools, &mut skills, use_canvas);
    (skills, tools)
}

/// Si le chat doit déléguer : (brief, skills, tools, phrase d'accusé).
pub(crate) fn chat_delegate_agent_spec(
    user_text: &str,
    model_output: &str,
    canvas_open: bool,
    _canvas_aspect: aos_proto::CanvasAspect,
    canvas_exported: &[String],
) -> Option<(String, Vec<String>, Vec<String>, String)> {
    if chat_tts_request(user_text).is_some() {
        return None;
    }
    let canvas_intent = chat_canvas::chat_wants_canvas_agent(user_text, canvas_open);
    if let Some(action) = aos_agent::actions::parse_action(model_output) {
        let spawn = action.action == "agent.spawn" || action.action == "agent.create";
        let self_tool = chat_action_is_self_tool(&action.action);
        if spawn || self_tool {
            let brief = action
                .args
                .get("brief")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let brief = if brief.is_empty() || self_tool {
                user_text.to_string()
            } else {
                brief
            };
            let use_canvas = chat_canvas::chat_wants_canvas_agent(user_text, canvas_open)
                || chat_canvas::chat_wants_canvas_agent(&brief, canvas_open);
            let brief = if use_canvas && !self_tool {
                user_text.to_string()
            } else {
                brief
            };
            let (mut skills, mut tools) =
                chat_delegate_kit(&brief, canvas_open, use_canvas, canvas_exported);
            merge_named_args(&mut skills, &action.args, "skills");
            merge_named_args(&mut tools, &action.args, "tools");
            if use_canvas {
                // Ensure canvas tools even if model passed a non-canvas tools list.
                let (_, canvas_tools) = chat_agent_kit_ex(&brief, true, canvas_exported);
                for t in canvas_tools {
                    if !tools.iter().any(|x| x == &t) {
                        tools.push(t);
                    }
                }
                strip_delegate_kit_tools(&mut tools, &mut skills, true);
            } else {
                strip_delegate_kit_tools(&mut tools, &mut skills, false);
            }
            if self_tool {
                for t in [
                    "module.scaffold",
                    "module.package",
                    "module.install",
                    "module.list",
                    "module.describe",
                ] {
                    if !tools.iter().any(|x| x == t) {
                        tools.push(t.into());
                    }
                }
                if action.action == "skill.create" && !tools.iter().any(|x| x == "skill.create") {
                    tools.push("skill.create".into());
                }
            }
            let mut prose = agent_panel::prose_without_json(model_output);
            if prose.is_empty() || self_tool {
                prose = if chat_user_wants_module_authoring(user_text) || self_tool {
                    "Je lance un agent pour créer le module.".into()
                } else if use_canvas {
                    "Je lance un agent pour dessiner sur le canvas.".into()
                } else {
                    "Je lance un agent pour cette tâche.".into()
                };
            }
            return Some((brief, skills, tools, prose));
        }
    }
    // JSON agent.spawn tronqué / illisible : si intent canvas, déléguer quand même.
    if canvas_intent
        && (model_output.contains("agent.spawn")
            || model_output.contains("\"action\"")
            || model_output.to_lowercase().contains("relance")
            || model_output.to_lowercase().contains("agent"))
    {
        let (skills, tools) = chat_delegate_kit(user_text, canvas_open, true, canvas_exported);
        return Some((
            user_text.to_string(),
            skills,
            tools,
            "Je lance un agent pour dessiner sur le canvas.".into(),
        ));
    }
    if chat_user_wants_module_authoring(user_text) {
        let (skills, tools) = chat_agent_kit(user_text);
        return Some((
            user_text.to_string(),
            skills,
            tools,
            "Je lance un agent pour créer le module.".into(),
        ));
    }
    if canvas_intent {
        let (skills, tools) = chat_delegate_kit(user_text, canvas_open, true, canvas_exported);
        return Some((
            user_text.to_string(),
            skills,
            tools,
            "Je lance un agent pour dessiner sur le canvas.".into(),
        ));
    }
    if chat_canvas::chat_user_wants_pixel_draw(user_text, canvas_open) {
        let (skills, tools) = chat_delegate_kit(user_text, canvas_open, false, canvas_exported);
        return Some((
            user_text.to_string(),
            skills,
            tools,
            "Je lance un agent pour générer l'image.".into(),
        ));
    }
    None
}

pub(crate) async fn session_has_running_canvas_agent(bus: &BusClient, session_id: &str) -> bool {
    let agents: Vec<aos_proto::AgentInfo> = bus
        .call(aos_agent::intents::LIST, &(), vec![])
        .await
        .unwrap_or_default();
    agents.iter().any(|a| {
        matches!(
            a.state,
            aos_proto::AgentState::Running
                | aos_proto::AgentState::Blocked
                | aos_proto::AgentState::Paused
        ) && a.session_id.as_deref() == Some(session_id)
            && a.tools.iter().any(|t| t.starts_with("canvas."))
    })
}

#[allow(clippy::too_many_arguments)] // Agent launch inputs remain explicit at the UI/runtime boundary.
pub(crate) async fn spawn_chat_delegate_agent(
    bus: Arc<BusClient>,
    evt_tx: Sender<Evt>,
    sid: String,
    user_text: String,
    brief: String,
    skills: Vec<String>,
    tools: Vec<String>,
    prose: String,
    auto_remember: bool,
    model_id: Option<String>,
    max_steps: u32,
    canvas_aspect: aos_proto::CanvasAspect,
) {
    let canvas_delegate = tools.iter().any(|t| t.starts_with("canvas."));
    let goal_statement = if canvas_delegate {
        user_text.trim().to_string()
    } else {
        brief.clone()
    };
    let mut req = AgentCreateRequest::simple(goal_statement.clone());
    req.display_name = Some(aos_agent::persist::agent_title(&goal_statement));
    req.origin = Some("assistant".into());
    req.skills = skills;
    req.tools = tools;
    req.session_id = Some(sid.clone());
    // Bind the chat session model. A canvas delegate without one must not silently
    // fall back to the default text model when a loaded visual model can critique it.
    req.model_id = if canvas_delegate {
        let available: Vec<ModelInfo> = bus
            .call("model.list", &(), vec![])
            .await
            .unwrap_or_default();
        canvas_model_id(model_id.clone(), &available)
    } else {
        model_id.clone()
    };
    if canvas_delegate {
        let exported: Vec<String> = bus
            .call::<(), Vec<aos_proto::ModuleInfo>>("module.list", &(), vec![])
            .await
            .map(|list| aos_agent::tools::canvas_tools_from_module_list(&list))
            .unwrap_or_default();
        req.system_prompt = Some(chat_canvas::canvas_agent_system_prompt(canvas_aspect, &exported));
    }
    req.goal = Some(AgentGoal {
        statement: goal_statement.clone(),
        success_criteria: vec![],
        max_steps,
        max_subagents: if canvas_delegate {
            0
        } else {
            CHAT_AGENT_MAX_SUBAGENTS
        },
        timeout_secs: 3600,
    });
    if !canvas_delegate {
        req.caps.push("tool.invoke:notes".into());
    }
    if req.skills.iter().any(|s| s.contains("task"))
        || req.tools.iter().any(|t| t.starts_with("tasks."))
    {
        req.caps.push("tool.invoke:tasks".into());
    }
    if req.tools.iter().any(|t| t.starts_with("module.")) {
        req.caps.push("module.install".into());
    }
    if req.tools.iter().any(|t| t.starts_with("media.")) {
        req.caps.push("media.generate".into());
        req.caps.push("fs.write:/downloads/**".into());
    }
    if req.tools.iter().any(|t| t.starts_with("canvas.")) {
        req.caps.push("tool.invoke:canvas".into());
        req.caps.push("fs.write:/downloads/**".into());
    }
    req.gate_mode = crate::prefs::load_preferences().agent_gate_mode.clone();
    if user_wants_deep_thinking(&user_text) || user_wants_deep_thinking(&brief) {
        req.cognitive_mode = CognitiveMode::DeepThinking;
        if !req.skills.iter().any(|s| s == "deep-thinking" || s == "planner") {
            req.skills.push("deep-thinking".into());
        }
        let cleaned = strip_deep_thinking_activation(&goal_statement);
        if !cleaned.is_empty() {
            req.directive = cleaned.clone();
            if let Some(g) = req.goal.as_mut() {
                g.statement = cleaned;
            }
        }
    }
    match bus
        .call::<AgentCreateRequest, aos_proto::AgentCreateResponse>(
            aos_agent::intents::CREATE,
            &req,
            vec![],
        )
        .await
    {
        Ok(r) => {
            let att = ChatAttachment::AgentRef {
                agent_id: r.agent_id.clone(),
                title: goal_statement.clone(),
                origin: "assistant".into(),
            };
            let _ = bus
                .call::<ChatSessionAppendRequest, aos_proto::ChatSessionMessage>(
                    "chat.session.append",
                    &ChatSessionAppendRequest {
                        session_id: sid.clone(),
                        role: "assistant".into(),
                        content: prose.clone(),
                        attachments: vec![att.clone()],
                        speaker_id: None,
                        speaker_name: None,
                        thinking: None,
                    },
                    vec![],
                )
                .await;
            crate::runtime::maybe_spawn_mem_extract(
                bus.clone(),
                evt_tx.clone(),
                auto_remember,
                sid.clone(),
                user_text,
                prose.clone(),
                model_id,
            );
            let _ = evt_tx.send(Evt::AgentSpawned {
                session_id: sid.clone(),
                agent_id: r.agent_id,
                title: goal_statement,
                origin: "assistant".into(),
                ack: prose,
            });
            let _ = evt_tx.send(Evt::Done {
                text: String::new(),
                session_id: sid,
                attachments: vec![],
            });
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::Error(e.to_string()));
        }
    }
}

pub(crate) async fn spawn_document_prep_agent(
    bus: Arc<BusClient>,
    evt_tx: Sender<Evt>,
    sid: String,
    question: String,
    language: String,
    _model_id: Option<String>,
    max_steps: u32,
) {
    let goal = question.trim().to_string();
    let mut req = AgentCreateRequest::simple(goal.clone());
    req.display_name = Some(aos_agent::persist::agent_title(&goal));
    req.origin = Some("document".into());
    req.skills = vec!["research".into(), "file-author".into()];
    req.tools = vec![
        "memory.recall".into(),
        "web.search".into(),
        "web.browse".into(),
        "files.generate".into(),
        "fs.read".into(),
        "fs.list".into(),
        "goal.complete".into(),
    ];
    req.session_id = Some(sid.clone());
    req.system_prompt = Some(aos_agent::research_detect::document_prep_system_prompt(
        &language,
    ));
    req.goal = Some(AgentGoal {
        statement: goal.clone(),
        success_criteria: vec!["Structured markdown under /downloads/ with footnoted sources"
            .into()],
        max_steps,
        max_subagents: 0,
        timeout_secs: 3600,
    });
    req.caps.push("tool.invoke:research".into());
    req.caps.push("net.connect:*".into());
    req.caps.push("fs.write:/downloads/**".into());
    req.caps.push("fs.read:/downloads/**".into());
    req.model_id = _model_id;
    req.gate_mode = crate::prefs::load_preferences().agent_gate_mode.clone();
    match bus
        .call::<AgentCreateRequest, aos_proto::AgentCreateResponse>(
            aos_agent::intents::CREATE,
            &req,
            vec![],
        )
        .await
    {
        Ok(r) => {
            let _ = evt_tx.send(Evt::AgentSpawned {
                session_id: sid,
                agent_id: r.agent_id,
                title: goal,
                origin: "document".into(),
                ack: String::new(),
            });
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::Error(e.to_string()));
        }
    }
}

pub(crate) fn chat_agent_kit(task: &str) -> (Vec<String>, Vec<String>) {
    chat_agent_kit_ex(task, false, &[])
}

fn chat_agent_kit_ex(
    task: &str,
    canvas_open: bool,
    canvas_exported: &[String],
) -> (Vec<String>, Vec<String>) {
    let lower = task.to_lowercase();
    let mut skills = vec!["planner".into(), "notes-writer".into()];
    let mut tools = vec![
        "notes.create".into(),
        "notes.list".into(),
        "notes.read".into(),
        "notes.search".into(),
        "notes.update".into(),
        "notes.links".into(),
        "notes.related".into(),
        "tasks.create".into(),
        "tasks.list".into(),
        "tasks.update".into(),
        "tasks.complete".into(),
        "plan.update".into(),
        "agent.spawn".into(),
        "agent.await".into(),
        "user.ask".into(),
    ];
    if lower.contains("module")
        || lower.contains("scaffold")
        || lower.contains("aospkg")
        || lower.contains("ext-rt")
    {
        for t in [
            "module.scaffold",
            "module.package",
            "module.install",
            "module.list",
            "module.describe",
        ] {
            if !tools.iter().any(|x| x == t) {
                tools.push(t.into());
            }
        }
    }
    if (lower.contains("task") || lower.contains("tâche") || lower.contains("todo"))
        && !skills.iter().any(|s| s == "tasks") {
            skills.push("tasks".into());
        }
    if lower.contains("recherch")
        || lower.contains("web")
        || lower.contains("search")
        || lower.contains("http")
    {
        if !skills.iter().any(|s| s == "research") {
            skills.push("research".into());
        }
        for t in ["web.search", "web.browse"] {
            if !tools.iter().any(|x| x == t) {
                tools.push(t.into());
            }
        }
    }
    if (lower.contains("audio")
        || lower.contains("tts")
        || lower.contains("voix")
        || lower.contains("speech")
        || lower.contains("speak")
        || lower.contains("wav")
        || lower.contains("vocal"))
        && !tools.iter().any(|x| x == "media.audio.generate") {
            tools.push("media.audio.generate".into());
        }
    if !chat_canvas::chat_user_wants_explicit_canvas(task)
        && !canvas_open
        && (lower.contains("image")
            || lower.contains("png")
            || lower.contains("illustration")
            || lower.contains("diffusion")
            || chat_canvas::chat_user_has_draw_wording(task))
        && !tools.iter().any(|x| x == "media.image.generate") {
            tools.push("media.image.generate".into());
        }
    if canvas_open || chat_canvas::chat_user_wants_explicit_canvas(task) {
        for t in aos_agent::tools::filter_canvas_tool_ids(canvas_exported) {
            if !tools.iter().any(|x| x == &t) {
                tools.push(t);
            }
        }
    }
    (skills, tools)
}
