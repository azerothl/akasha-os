//! Chat-to-agent delegation, capability selection, and agent launch orchestration.

use crate::cmd::Evt;
use crate::{agent_panel, chat_canvas, CHAT_AGENT_MAX_SUBAGENTS};
use aos_ipc::BusClient;
use aos_proto::{
    chat_tts_request, chat_user_wants_advisory, chat_user_wants_module_authoring,
    AgentCreateRequest, AgentGoal, AgentIdRequest, AgentInfo, AgentKind, AgentSpecResponse,
    ChatAttachment, ChatSessionAppendRequest, CognitiveMode, ModelInfo, ModelState,
};
use std::sync::mpsc::Sender;
use std::sync::Arc;

fn chat_action_is_self_tool(action: &str) -> bool {
    matches!(
        action,
        "module.scaffold"
            | "module.package"
            | "module.install"
            | "module.uninstall"
            | "skill.create"
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
            if mode.eq_ignore_ascii_case("deep_thinking")
                || mode.eq_ignore_ascii_case("deep-thinking")
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
                        if v.get("mode").and_then(|m| m.as_str()).is_some_and(|m| {
                            m.eq_ignore_ascii_case("deep_thinking")
                                || m.eq_ignore_ascii_case("deep-thinking")
                        }) {
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

/// Applique le mode Deep Thinking sur une requête de création d'agent.
pub(crate) fn apply_deep_thinking_mode(req: &mut AgentCreateRequest, goal: &str) {
    req.cognitive_mode = CognitiveMode::DeepThinking;
    req.skills.retain(|s| s != "planner");
    if !req.skills.iter().any(|s| s == "deep-thinking") {
        req.skills.push("deep-thinking".into());
    }
    let cleaned = strip_deep_thinking_activation(goal);
    if !cleaned.is_empty() && cleaned != goal {
        req.directive = cleaned.clone();
        if let Some(g) = req.goal.as_mut() {
            g.statement = cleaned;
        }
    }
}

/// Reprise après timeout / Stop : consigne injectée par `set_partial_continuation`.
///
/// Ces tours doivent rester en chat Direct (streamer la suite) — jamais forcer
/// un agent Deep Thinking dont le goal serait uniquement « Continue la réponse… ».
pub(crate) fn chat_is_partial_continuation(text: &str) -> bool {
    let t = text.trim();
    t.starts_with("Continue la réponse exactement là où elle")
        || t.starts_with("Continue the answer exactly where it stopped")
}

/// Spec de délégation forcée (chip Deep ou phrase) quand le superviseur n'a pas spawn.
pub(crate) fn deep_thinking_force_delegate(
    user_text: &str,
    canvas_open: bool,
    illustration_open: bool,
    canvas_exported: &[String],
) -> ChatDelegateSpec {
    let use_illust = chat_canvas::chat_wants_illust_agent(user_text, illustration_open);
    let (mut skills, mut tools) = chat_delegate_kit(
        user_text,
        canvas_open,
        illustration_open,
        false,
        use_illust,
        canvas_exported,
    );
    skills.retain(|s| s != "planner");
    if !skills.iter().any(|s| s == "deep-thinking") {
        skills.push("deep-thinking".into());
    }
    if chat_user_wants_advisory(user_text) {
        strip_module_authoring_tools(&mut tools);
        strip_advisory_notes_tools(&mut skills, &mut tools);
    }
    ChatDelegateSpec {
        brief: user_text.to_string(),
        skills,
        tools,
        prose: "Je lance un agent Deep Thinking.".into(),
        roster_id: None,
    }
}

/// Retire scaffold/package/install (garde list/describe pour l'analyse).
fn strip_module_authoring_tools(tools: &mut Vec<String>) {
    tools.retain(|t| {
        !matches!(
            t.as_str(),
            "module.scaffold" | "module.package" | "module.install" | "module.compile"
        )
    });
}

/// Advisory Deep Thinking: no note authoring — analysis/recommendation only.
fn strip_advisory_notes_tools(skills: &mut Vec<String>, tools: &mut Vec<String>) {
    skills.retain(|s| s != "notes-writer");
    tools.retain(|t| !t.starts_with("notes."));
}

/// Host-side decision to spawn a background agent from Direct chat.
#[derive(Debug, Clone)]
pub(crate) struct ChatDelegateSpec {
    pub brief: String,
    pub skills: Vec<String>,
    pub tools: Vec<String>,
    pub prose: String,
    /// Explicit library id from supervisor `agent.spawn` args (`roster_id` / `agent_id`).
    pub roster_id: Option<String>,
}

/// Profile cloned from a library roster entry into a new Task worker.
#[derive(Debug, Clone)]
pub(crate) struct RosterBinding {
    pub source_roster_id: String,
    pub display_name: Option<String>,
    pub persona_id: Option<String>,
    pub system_prompt: Option<String>,
    pub skills: Vec<String>,
    pub tools: Vec<String>,
    pub mcp_servers: Vec<String>,
    pub caps: Vec<String>,
    pub avatar: Option<String>,
    pub color: Option<String>,
    pub model_id: Option<String>,
}

fn is_delegate_roster_candidate(agent: &AgentInfo) -> bool {
    if agent.is_ephemeral_chat_spawn() {
        return false;
    }
    if agent.is_roster() {
        return true;
    }
    agent.kind == AgentKind::Task
        && matches!(agent.origin.as_deref(), Some("library") | Some("form"))
}

fn binding_from_agent(agent: &AgentInfo) -> RosterBinding {
    let display_name = {
        let t = agent.display_title();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    };
    RosterBinding {
        source_roster_id: agent.agent_id.clone(),
        display_name,
        persona_id: agent.persona_id.clone(),
        system_prompt: None,
        skills: agent.skills.clone(),
        tools: agent.tools.clone(),
        mcp_servers: agent.mcp_servers.clone(),
        caps: agent.caps.clone(),
        avatar: agent.avatar.clone(),
        color: agent.color.clone(),
        model_id: agent.model_id.clone(),
    }
}

fn tools_fully_covered(have: &[String], need: &[String]) -> bool {
    !need.is_empty() && need.iter().all(|t| have.iter().any(|h| h == t))
}

fn skill_overlap(have: &[String], need: &[String]) -> usize {
    need.iter().filter(|s| have.iter().any(|h| h == *s)).count()
}

/// Pick a library roster (or library Task) profile to clone into a new worker.
///
/// Explicit id always wins when the agent is an eligible library candidate.
/// Auto-match requires full coverage of `needed_tools` and skips empty-tool personas.
pub(crate) fn match_roster_for_delegate(
    agents: &[AgentInfo],
    needed_skills: &[String],
    needed_tools: &[String],
    explicit_id: Option<&str>,
) -> Option<RosterBinding> {
    let candidates: Vec<&AgentInfo> = agents
        .iter()
        .filter(|a| is_delegate_roster_candidate(a))
        .collect();

    if let Some(id) = explicit_id.map(str::trim).filter(|s| !s.is_empty()) {
        return candidates
            .iter()
            .find(|a| a.agent_id == id)
            .map(|a| binding_from_agent(a));
    }

    if needed_tools.is_empty() {
        return None;
    }

    let mut best: Option<(&AgentInfo, i32, usize)> = None;
    for agent in candidates {
        if agent.tools.is_empty() {
            continue;
        }
        if !tools_fully_covered(&agent.tools, needed_tools) {
            continue;
        }
        let skills = skill_overlap(&agent.skills, needed_skills) as i32;
        let score = (needed_tools.len() as i32) * 10 + skills;
        let extras = agent.tools.len();
        match best {
            None => best = Some((agent, score, extras)),
            Some((_, best_score, best_extras)) => {
                if score > best_score || (score == best_score && extras < best_extras) {
                    best = Some((agent, score, extras));
                }
            }
        }
    }
    best.map(|(a, _, _)| binding_from_agent(a))
}

fn merge_unique(dst: &mut Vec<String>, src: &[String]) {
    for item in src {
        if !dst.iter().any(|x| x == item) {
            dst.push(item.clone());
        }
    }
}

/// Union host kit with roster profile; records `source_roster_id` for future personality/STM.
pub(crate) fn apply_roster_binding(req: &mut AgentCreateRequest, binding: &RosterBinding) {
    req.source_roster_id = Some(binding.source_roster_id.clone());
    if let Some(name) = binding
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        req.display_name = Some(name.to_string());
    }
    if binding.persona_id.is_some() {
        req.persona_id = binding.persona_id.clone();
    }
    if let Some(prompt) = binding
        .system_prompt
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        match req
            .system_prompt
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(host) => {
                req.system_prompt = Some(format!("{prompt}\n\n{host}"));
            }
            None => {
                req.system_prompt = Some(prompt.to_string());
            }
        }
    }
    merge_unique(&mut req.skills, &binding.skills);
    merge_unique(&mut req.tools, &binding.tools);
    merge_unique(&mut req.mcp_servers, &binding.mcp_servers);
    merge_unique(&mut req.caps, &binding.caps);
    if req.avatar.is_none() {
        req.avatar = binding.avatar.clone();
    }
    if req.color.is_none() {
        req.color = binding.color.clone();
    }
    if req
        .model_id
        .as_ref()
        .map(|s| s.trim().is_empty())
        .unwrap_or(true)
    {
        req.model_id = binding.model_id.clone();
    }
}

fn roster_ack(display_name: &str) -> String {
    format!("Je confie ça à {display_name}.")
}

/// Short library list for the Direct supervisor prompt.
pub(crate) fn format_roster_for_delegation_prompt(agents: &[AgentInfo]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for agent in agents.iter().filter(|a| is_delegate_roster_candidate(a)) {
        let name = agent.display_title();
        let mut caps = Vec::new();
        if !agent.tools.is_empty() {
            let preview: Vec<&str> = agent.tools.iter().take(6).map(String::as_str).collect();
            caps.push(format!("tools={}", preview.join(",")));
        }
        if !agent.skills.is_empty() {
            let preview: Vec<&str> = agent.skills.iter().take(4).map(String::as_str).collect();
            caps.push(format!("skills={}", preview.join(",")));
        }
        let detail = if caps.is_empty() {
            "tools=(none)".to_string()
        } else {
            caps.join("; ")
        };
        lines.push(format!("- {} [{}]: {detail}", agent.agent_id, name));
    }
    if lines.is_empty() {
        return String::new();
    }
    format!(
        "\n\nAgents bibliothèque (préférer un match via roster_id si adapté) :\n{}",
        lines.join("\n")
    )
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

/// Retire les outils incompatibles avec le kit canvas (vectoriel) / Illustration / pixel.
fn strip_delegate_kit_tools(
    tools: &mut Vec<String>,
    skills: &mut Vec<String>,
    use_canvas: bool,
    use_illust: bool,
) {
    if use_canvas {
        tools.retain(|t| t.starts_with("canvas.") || t == "plan.update");
        // A canvas author needs a compact geometric context. Notes/tasks and
        // their long skill instructions caused the model to archive the
        // drawing mid-run instead of continuing the composition.
        skills.clear();
    } else if use_illust {
        tools.retain(|t| t.starts_with("illust.") || t == "plan.update");
        skills.clear();
    } else {
        tools.retain(|t| !t.starts_with("canvas.") && !t.starts_with("illust."));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeviceCaptureIntent {
    Camera,
    Microphone,
    Both,
}

pub(crate) fn chat_device_capture_intent(text: &str) -> Option<DeviceCaptureIntent> {
    let l = text.to_ascii_lowercase();
    let camera = l.contains("webcam")
        || l.contains("web cam")
        || l.contains("la caméra")
        || l.contains("ma caméra")
        || l.contains("la camera")
        || l.contains("ma camera")
        || l.contains("my camera")
        || l.contains("the camera")
        || l.contains("use the camera")
        || l.contains("utilise la cam")
        || l.contains("utiliser la cam")
        || l.contains("accéder à la cam")
        || l.contains("acceder a la cam")
        || l.contains("prends une photo")
        || l.contains("prend une photo")
        || l.contains("take a photo")
        || l.contains("take a picture")
        || l.contains("regarde-moi")
        || l.contains("regarde moi")
        || l.contains("look at me")
        || l.contains("vois ce que je")
        || l.contains("what do you see");
    let mic = l.contains("microphone")
        || l.contains("le micro")
        || l.contains("au micro")
        || l.contains("du micro")
        || l.contains("écoute-moi")
        || l.contains("ecoute-moi")
        || l.contains("listen to me")
        || l.contains("enregistre ma voix")
        || l.contains("record my voice");
    match (camera, mic) {
        (true, true) => Some(DeviceCaptureIntent::Both),
        (true, false) => Some(DeviceCaptureIntent::Camera),
        (false, true) => Some(DeviceCaptureIntent::Microphone),
        (false, false) => None,
    }
}

fn push_device_capture_tools(tools: &mut Vec<String>, intent: DeviceCaptureIntent) {
    for t in ["device.enumerate", "device.capture.stop"] {
        if !tools.iter().any(|x| x == t) {
            tools.push(t.into());
        }
    }
    if matches!(
        intent,
        DeviceCaptureIntent::Camera | DeviceCaptureIntent::Both
    ) && !tools.iter().any(|x| x == "device.camera.capture")
    {
        tools.push("device.camera.capture".into());
    }
    if matches!(
        intent,
        DeviceCaptureIntent::Microphone | DeviceCaptureIntent::Both
    ) && !tools.iter().any(|x| x == "device.mic.capture")
    {
        tools.push("device.mic.capture".into());
    }
}

fn device_capture_ack(intent: DeviceCaptureIntent) -> String {
    match intent {
        DeviceCaptureIntent::Microphone => "Je lance un agent pour le microphone.".into(),
        DeviceCaptureIntent::Camera | DeviceCaptureIntent::Both => {
            "Je lance un agent pour la webcam.".into()
        }
    }
}

fn text_has_usb_word(text: &str) -> bool {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '.')
        .any(|w| w == "usb" || w.starts_with("usb."))
}

/// Dans un long texte, exige « usb » proche d'un verbe/nom d'action (fenêtre ±4 mots).
fn usb_with_action_context(text: &str) -> bool {
    let words: Vec<String> = text
        .to_ascii_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    const ACTION: &[&str] = &[
        "list",
        "lister",
        "liste",
        "enumerate",
        "enumerer",
        "enumere",
        "port",
        "ports",
        "device",
        "devices",
        "serial",
        "com",
        "connect",
        "connecter",
        "brancher",
        "branch",
    ];
    for (i, w) in words.iter().enumerate() {
        if w != "usb" {
            continue;
        }
        let start = i.saturating_sub(4);
        let end = (i + 5).min(words.len());
        for neighbor in &words[start..end] {
            if ACTION.contains(&neighbor.as_str())
                || neighbor.starts_with("peripher")
                || neighbor.starts_with("périph")
            {
                return true;
            }
        }
    }
    false
}

fn text_has_com_port_ref(text: &str) -> bool {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .any(|w| w.len() >= 4 && w.starts_with("com") && w[3..].chars().all(|c| c.is_ascii_digit()))
}

fn usb_has_connect_verb(text: &str) -> bool {
    let l = text.to_ascii_lowercase();
    l.contains("se connecter")
        || l.contains("connecter")
        || l.contains("connect to")
        || l.contains(" connect ")
        || l.ends_with(" connect")
        || (l.contains("connect") && !l.contains("connected") && !l.contains("connectés"))
}

fn usb_has_open_verb(text: &str) -> bool {
    let l = text.to_ascii_lowercase();
    l.contains("ouvrir") || l.contains(" open ") || l.starts_with("open ")
}

/// Ouvrir / connecter un port (COM, série) — pas une simple liste USB.
pub(crate) fn chat_device_usb_connect_intent(text: &str) -> bool {
    if text.trim().is_empty() {
        return false;
    }
    let l = text.to_ascii_lowercase();
    if text_has_com_port_ref(text) {
        return true;
    }
    let portish = l.contains("com")
        || l.contains("série")
        || l.contains("serie")
        || l.contains("serial")
        || l.contains("port");
    let usb_serial = l.contains("usb serial")
        || l.contains("serial usb")
        || l.contains("série usb")
        || l.contains("serie usb");
    if l.contains("se connecter")
        && (portish || usb_serial || l.contains("usb") || text_has_com_port_ref(text))
    {
        return true;
    }
    if usb_has_open_verb(text) && (portish || text_has_com_port_ref(text)) {
        return true;
    }
    if usb_has_connect_verb(text) && (portish || usb_serial || l.contains("usb")) {
        return true;
    }
    false
}

const USB_OPEN_BRIEF_DIRECTIVE: &str = "[USB] Procédure obligatoire : \
1) device.usb.enumerate pour obtenir les device_id \
2) device.usb.open avec ce device_id exact. \
Dans goal.complete : cite uniquement le champ name (ex. CP2102, USB Serial), \
jamais /dev/*, path_hint ni device_id. \
Interdit : device.usb.io (capacité, pas un outil), device.enumerate, shell.run.";

fn enrich_usb_connect_brief(brief: String, user_text: &str) -> String {
    if !chat_device_usb_connect_intent(user_text) && !chat_device_usb_connect_intent(&brief) {
        return brief;
    }
    let trimmed = brief.trim();
    if trimmed.is_empty() {
        USB_OPEN_BRIEF_DIRECTIVE.into()
    } else if trimmed.contains(USB_OPEN_BRIEF_DIRECTIVE) {
        brief
    } else {
        format!("{trimmed}\n\n{USB_OPEN_BRIEF_DIRECTIVE}")
    }
}

fn push_device_usb_tools_and_brief(brief: &mut String, user_text: &str, tools: &mut Vec<String>) {
    push_device_usb_tools(tools);
    *brief = enrich_usb_connect_brief(brief.clone(), user_text);
}

/// Détecte une demande de liste/accès USB (FR/EN). Les longs textes exigent un signal
/// explicite pour éviter un « usb » isolé dans une doc technique.
pub(crate) fn chat_device_usb_intent(text: &str) -> bool {
    let l = text.to_ascii_lowercase();
    if l.contains("device.usb")
        || l.contains("périphérique usb")
        || l.contains("périphériques usb")
        || l.contains("peripherique usb")
        || l.contains("peripheriques usb")
        || l.contains("lister usb")
        || l.contains("liste usb")
        || l.contains("list usb")
        || l.contains("list the usb")
        || l.contains("ports série")
        || l.contains("port série")
        || l.contains("ports serie")
        || l.contains("port serie")
        || l.contains("serial port")
        || l.contains("serial ports")
        || l.contains("usb serial")
        || l.contains("serial usb")
        || l.contains("série usb")
        || l.contains("serie usb")
        || l.contains("com port")
        || l.contains("ports com")
        || l.contains("port com")
        || l.contains("usb device")
        || l.contains("usb devices")
        || l.contains("enumerate usb")
        || l.contains("énumérer usb")
        || l.contains("enumerer usb")
        || l.contains("brancher un usb")
        || l.contains("connecter un usb")
        || (l.contains("se connecter")
            && (l.contains("usb")
                || l.contains("série")
                || l.contains("serie")
                || l.contains("serial")
                || l.contains("com")
                || text_has_com_port_ref(text)))
        || l.contains("lister les périphériques usb")
        || l.contains("list usb devices")
        || text_has_com_port_ref(text)
    {
        return true;
    }
    if !text_has_usb_word(text) {
        return false;
    }
    if text.len() > 280 {
        return usb_with_action_context(text);
    }
    true
}

const DEVICE_USB_TOOLS: [&str; 5] = [
    "device.usb.enumerate",
    "device.usb.open",
    "device.usb.read",
    "device.usb.write",
    "device.usb.close",
];

fn push_device_usb_tools(tools: &mut Vec<String>) {
    for t in DEVICE_USB_TOOLS {
        if !tools.iter().any(|x| x == t) {
            tools.push(t.into());
        }
    }
}

fn device_usb_ack() -> String {
    "Je lance un agent pour les périphériques USB.".into()
}

/// Prefer a loaded vision model when the chat model cannot see the captured PNG.
pub(crate) fn device_vision_model_id(
    selected: Option<String>,
    available: &[ModelInfo],
) -> Option<String> {
    let selected_vision = selected.as_ref().and_then(|id| {
        available
            .iter()
            .find(|m| &m.id == id && m.has_vision)
            .map(|m| m.id.clone())
    });
    selected_vision
        .or_else(|| canvas_model_id(None, available))
        .or(selected)
}

/// A canvas critic needs pixels, not merely a capable model installed on disk.
/// Prefer a resident vision model even when the chat session selected a
/// text-only model; retain the selected model only when no vision model is
/// available on the machine.
pub(crate) fn canvas_model_id(selected: Option<String>, available: &[ModelInfo]) -> Option<String> {
    let resident_vision = || {
        available
            .iter()
            .find(|model| {
                model.has_vision
                    && matches!(
                        model.state,
                        ModelState::Loaded | ModelState::PartiallyOffloaded
                    )
            })
            .map(|model| model.id.clone())
    };

    match selected {
        Some(id)
            if available.iter().any(|model| {
                model.id == id
                    && model.has_vision
                    && matches!(
                        model.state,
                        ModelState::Loaded | ModelState::PartiallyOffloaded
                    )
            }) =>
        {
            Some(id)
        }
        // A text-only chat model cannot critique pixels. Prefer a resident
        // vision model for canvas delegation while retaining the selected
        // model as a last resort when the machine has no vision model.
        Some(id) => resident_vision().or(Some(id)),
        None => resident_vision(),
    }
}

pub(crate) fn chat_delegate_kit(
    brief: &str,
    canvas_open: bool,
    illustration_open: bool,
    use_canvas: bool,
    use_illust: bool,
    canvas_exported: &[String],
) -> (Vec<String>, Vec<String>) {
    let (mut skills, mut tools) = chat_agent_kit_ex(
        brief,
        canvas_open || use_canvas,
        illustration_open || use_illust,
        canvas_exported,
    );
    strip_delegate_kit_tools(&mut tools, &mut skills, use_canvas, use_illust);
    (skills, tools)
}

/// Si le chat doit déléguer : brief, kit, phrase d'accusé, roster_id optionnel.
pub(crate) fn chat_delegate_agent_spec(
    user_text: &str,
    model_output: &str,
    canvas_open: bool,
    illustration_open: bool,
    _canvas_aspect: aos_proto::CanvasAspect,
    canvas_exported: &[String],
) -> Option<ChatDelegateSpec> {
    if chat_tts_request(user_text).is_some() {
        return None;
    }
    let canvas_intent = chat_canvas::chat_wants_canvas_agent(user_text, canvas_open);
    let illust_intent = chat_canvas::chat_wants_illust_agent(user_text, illustration_open);
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
            let roster_id = action
                .args
                .get("roster_id")
                .or_else(|| action.args.get("agent_id"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let advisory = chat_user_wants_advisory(user_text);
            // Ne laisse pas le superviseur réécrire un conseil en « Créer un module… ».
            let brief = if brief.is_empty() || self_tool || advisory {
                user_text.to_string()
            } else {
                brief
            };
            let use_canvas = chat_canvas::chat_wants_canvas_agent(user_text, canvas_open)
                || chat_canvas::chat_wants_canvas_agent(&brief, canvas_open);
            let use_illust = !use_canvas
                && (chat_canvas::chat_wants_illust_agent(user_text, illustration_open)
                    || chat_canvas::chat_wants_illust_agent(&brief, illustration_open));
            let mut brief = if (use_canvas || use_illust) && !self_tool {
                user_text.to_string()
            } else {
                brief
            };
            let (mut skills, mut tools) = chat_delegate_kit(
                &brief,
                canvas_open,
                illustration_open,
                use_canvas,
                use_illust,
                canvas_exported,
            );
            merge_named_args(&mut skills, &action.args, "skills");
            merge_named_args(&mut tools, &action.args, "tools");
            if use_canvas {
                // Ensure canvas tools even if model passed a non-canvas tools list.
                let (_, canvas_tools) =
                    chat_agent_kit_ex(&brief, true, false, canvas_exported);
                for t in canvas_tools {
                    if !tools.iter().any(|x| x == &t) {
                        tools.push(t);
                    }
                }
                strip_delegate_kit_tools(&mut tools, &mut skills, true, false);
            } else if use_illust {
                let (_, illust_tools) =
                    chat_agent_kit_ex(&brief, false, true, canvas_exported);
                for t in illust_tools {
                    if !tools.iter().any(|x| x == &t) {
                        tools.push(t);
                    }
                }
                strip_delegate_kit_tools(&mut tools, &mut skills, false, true);
            } else {
                strip_delegate_kit_tools(&mut tools, &mut skills, false, false);
            }
            if let Some(intent) =
                chat_device_capture_intent(user_text).or_else(|| chat_device_capture_intent(&brief))
            {
                push_device_capture_tools(&mut tools, intent);
            }
            if chat_device_usb_intent(user_text) || chat_device_usb_intent(&brief) {
                push_device_usb_tools_and_brief(&mut brief, user_text, &mut tools);
            }
            if self_tool && !advisory {
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
            if advisory {
                strip_module_authoring_tools(&mut tools);
                if skills.iter().any(|s| s == "deep-thinking") {
                    strip_advisory_notes_tools(&mut skills, &mut tools);
                }
            }
            let mut prose = agent_panel::prose_without_json(model_output);
            if prose.is_empty() || self_tool {
                prose = if chat_user_wants_module_authoring(user_text) || self_tool {
                    "Je lance un agent pour créer le module.".into()
                } else if use_canvas {
                    "Je lance un agent pour dessiner sur le canvas.".into()
                } else if use_illust {
                    "Je lance un agent pour illustrer la scène.".into()
                } else if let Some(intent) = chat_device_capture_intent(user_text) {
                    device_capture_ack(intent)
                } else if chat_device_usb_intent(user_text) || chat_device_usb_intent(&brief) {
                    device_usb_ack()
                } else {
                    "Je lance un agent pour cette tâche.".into()
                };
            }
            return Some(ChatDelegateSpec {
                brief,
                skills,
                tools,
                prose,
                roster_id,
            });
        }
    }
    // JSON agent.spawn tronqué / illisible : si intent canvas, déléguer quand même.
    if canvas_intent
        && (model_output.contains("agent.spawn")
            || model_output.contains("\"action\"")
            || model_output.to_lowercase().contains("relance")
            || model_output.to_lowercase().contains("agent"))
    {
        let (skills, tools) = chat_delegate_kit(
            user_text,
            canvas_open,
            illustration_open,
            true,
            false,
            canvas_exported,
        );
        return Some(ChatDelegateSpec {
            brief: user_text.to_string(),
            skills,
            tools,
            prose: "Je lance un agent pour dessiner sur le canvas.".into(),
            roster_id: None,
        });
    }
    if chat_user_wants_module_authoring(user_text) {
        let (skills, tools) = chat_agent_kit(user_text);
        return Some(ChatDelegateSpec {
            brief: user_text.to_string(),
            skills,
            tools,
            prose: "Je lance un agent pour créer le module.".into(),
            roster_id: None,
        });
    }
    if canvas_intent {
        let (skills, tools) = chat_delegate_kit(
            user_text,
            canvas_open,
            illustration_open,
            true,
            false,
            canvas_exported,
        );
        return Some(ChatDelegateSpec {
            brief: user_text.to_string(),
            skills,
            tools,
            prose: "Je lance un agent pour dessiner sur le canvas.".into(),
            roster_id: None,
        });
    }
    if illust_intent {
        let (skills, tools) = chat_delegate_kit(
            user_text,
            canvas_open,
            illustration_open,
            false,
            true,
            canvas_exported,
        );
        return Some(ChatDelegateSpec {
            brief: user_text.to_string(),
            skills,
            tools,
            prose: "Je lance un agent pour illustrer la scène.".into(),
            roster_id: None,
        });
    }
    if chat_canvas::chat_user_wants_pixel_draw(user_text, canvas_open, illustration_open) {
        let (skills, tools) = chat_delegate_kit(
            user_text,
            canvas_open,
            illustration_open,
            false,
            false,
            canvas_exported,
        );
        return Some(ChatDelegateSpec {
            brief: user_text.to_string(),
            skills,
            tools,
            prose: "Je lance un agent pour générer l'image.".into(),
            roster_id: None,
        });
    }
    if let Some(intent) = chat_device_capture_intent(user_text) {
        let (skills, mut tools) = chat_delegate_kit(
            user_text,
            canvas_open,
            illustration_open,
            false,
            false,
            canvas_exported,
        );
        push_device_capture_tools(&mut tools, intent);
        return Some(ChatDelegateSpec {
            brief: user_text.to_string(),
            skills,
            tools,
            prose: device_capture_ack(intent),
            roster_id: None,
        });
    }
    if chat_device_usb_intent(user_text) {
        let (skills, mut tools) = chat_delegate_kit(
            user_text,
            canvas_open,
            illustration_open,
            false,
            false,
            canvas_exported,
        );
        let mut brief = user_text.to_string();
        push_device_usb_tools_and_brief(&mut brief, user_text, &mut tools);
        return Some(ChatDelegateSpec {
            brief,
            skills,
            tools,
            prose: device_usb_ack(),
            roster_id: None,
        });
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
    roster_id: Option<String>,
    auto_remember: bool,
    instincts_in_session: bool,
    model_id: Option<String>,
    max_steps: u32,
    canvas_aspect: aos_proto::CanvasAspect,
    deep_thinking: bool,
) {
    let canvas_delegate = tools.iter().any(|t| t.starts_with("canvas."));
    let illust_delegate = tools.iter().any(|t| t.starts_with("illust."));
    let device_camera_delegate = tools.iter().any(|t| t == "device.camera.capture");
    let advisory = chat_user_wants_advisory(&user_text);
    let mut skills = skills;
    let mut tools = tools;
    if advisory {
        strip_module_authoring_tools(&mut tools);
        if deep_thinking {
            strip_advisory_notes_tools(&mut skills, &mut tools);
        }
    }
    let goal_statement = if canvas_delegate || illust_delegate || advisory {
        // Advisory: garder la question utilisateur (pas un brief « Créer… »).
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
    } else if device_camera_delegate {
        let available: Vec<ModelInfo> = bus
            .call("model.list", &(), vec![])
            .await
            .unwrap_or_default();
        device_vision_model_id(model_id.clone(), &available)
    } else {
        model_id.clone()
    };
    if canvas_delegate {
        let exported: Vec<String> = bus
            .call::<(), Vec<aos_proto::ModuleInfo>>("module.list", &(), vec![])
            .await
            .map(|list| aos_agent::tools::canvas_tools_from_module_list(&list))
            .unwrap_or_default();
        req.system_prompt = Some(chat_canvas::canvas_agent_system_prompt(
            canvas_aspect,
            &exported,
        ));
    } else if illust_delegate {
        req.system_prompt = Some(aos_agent::tools::illust_draw_strategy_hint());
    }
    req.goal = Some(AgentGoal {
        statement: goal_statement.clone(),
        success_criteria: vec![],
        max_steps,
        max_subagents: if canvas_delegate || illust_delegate {
            0
        } else {
            CHAT_AGENT_MAX_SUBAGENTS
        },
        timeout_secs: 3600,
    });
    if !canvas_delegate && !illust_delegate {
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
    if req.tools.iter().any(|t| t == "files.generate") {
        req.caps.push("fs.write:/downloads/**".into());
        req.caps.push("fs.read:/downloads/**".into());
    }
    if req.tools.iter().any(|t| t.starts_with("canvas.")) {
        req.caps.push("tool.invoke:canvas".into());
        req.caps.push("fs.write:/downloads/**".into());
    }
    if req.tools.iter().any(|t| t.starts_with("illust.")) {
        req.caps.push("tool.invoke:illust".into());
        req.caps.push("fs.write:/downloads/**".into());
    }
    if req.tools.iter().any(|t| t == "device.camera.capture") {
        req.caps.push("device.camera.capture".into());
    }
    if req.tools.iter().any(|t| t == "device.mic.capture") {
        req.caps.push("device.mic.capture".into());
    }

    let agents: Vec<AgentInfo> = bus
        .call(aos_agent::intents::LIST, &(), vec![])
        .await
        .unwrap_or_default();
    let mut prose = prose;
    if let Some(mut binding) =
        match_roster_for_delegate(&agents, &req.skills, &req.tools, roster_id.as_deref())
    {
        if let Ok(spec_resp) = bus
            .call::<AgentIdRequest, AgentSpecResponse>(
                aos_agent::intents::SPEC_GET,
                &AgentIdRequest {
                    agent_id: binding.source_roster_id.clone(),
                },
                vec![],
            )
            .await
        {
            let spec = spec_resp.spec;
            if binding.system_prompt.is_none() {
                binding.system_prompt = spec.system_prompt;
            }
            merge_unique(&mut binding.skills, &spec.skills);
            merge_unique(&mut binding.tools, &spec.tools);
            merge_unique(&mut binding.mcp_servers, &spec.mcp_servers);
            merge_unique(&mut binding.caps, &spec.caps);
            if binding.avatar.is_none() {
                binding.avatar = spec.avatar;
            }
            if binding.color.is_none() {
                binding.color = spec.color;
            }
            if binding.persona_id.is_none() {
                binding.persona_id = spec.persona_id;
            }
            if binding
                .display_name
                .as_ref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
            {
                binding.display_name = spec.display_name;
            }
            if binding
                .model_id
                .as_ref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
            {
                binding.model_id = spec.model_id;
            }
        }
        apply_roster_binding(&mut req, &binding);
        if let Some(name) = binding
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            prose = roster_ack(name);
        }
    }

    req.gate_mode = crate::prefs::load_preferences().agent_gate_mode.clone();
    let wants_deep =
        deep_thinking || user_wants_deep_thinking(&user_text) || user_wants_deep_thinking(&brief);
    if wants_deep {
        apply_deep_thinking_mode(&mut req, &goal_statement);
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
                user_text.clone(),
                prose.clone(),
                model_id,
            );
            crate::runtime::maybe_spawn_skill_consider(
                bus.clone(),
                evt_tx.clone(),
                instincts_in_session,
                sid.clone(),
                &[],
                &user_text,
                &prose,
                "pressure",
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
            let _ = evt_tx.send(Evt::ChatError {
                session_id: sid,
                message: e.to_string(),
            });
        }
    }
}

pub(crate) async fn spawn_document_prep_agent(
    bus: Arc<BusClient>,
    evt_tx: Sender<Evt>,
    sid: String,
    question: String,
    history: Vec<(String, String)>,
    language: String,
    _model_id: Option<String>,
    max_steps: u32,
) {
    let title = question.trim().to_string();
    let goal = aos_agent::research_detect::document_prep_goal_from_thread(&history, &title);
    let mut req = AgentCreateRequest::simple(goal.clone());
    req.display_name = Some(aos_agent::persist::agent_title(&title));
    req.origin = Some("document".into());
    req.skills = vec!["research".into(), "file-author".into()];
    req.tools = vec![
        "memory.recall".into(),
        "web.search".into(),
        "web.browse".into(),
        "files.generate".into(),
        "fs.read".into(),
        "fs.list".into(),
        "agent.spawn".into(),
        "agent.await".into(),
        "goal.complete".into(),
    ];
    req.session_id = Some(sid.clone());
    req.system_prompt = Some(aos_agent::research_detect::document_prep_system_prompt(
        &language,
    ));
    req.goal = Some(AgentGoal {
        statement: goal.clone(),
        success_criteria: vec![
            "Structured markdown under /downloads/ with footnoted sources".into(),
        ],
        max_steps,
        max_subagents: CHAT_AGENT_MAX_SUBAGENTS,
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
                title,
                origin: "document".into(),
                ack: String::new(),
            });
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::ChatError {
                session_id: sid,
                message: e.to_string(),
            });
        }
    }
}

pub(crate) fn chat_agent_kit(task: &str) -> (Vec<String>, Vec<String>) {
    chat_agent_kit_ex(task, false, false, &[])
}

fn chat_agent_kit_ex(
    task: &str,
    canvas_open: bool,
    illustration_open: bool,
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
        "notes.delete".into(),
        "tasks.create".into(),
        "tasks.list".into(),
        "tasks.update".into(),
        "tasks.complete".into(),
        "plan.update".into(),
        "agent.spawn".into(),
        "agent.await".into(),
        "user.ask".into(),
    ];
    if aos_agent::research_detect::user_requested_document(task) {
        aos_agent::research_detect::ensure_document_file_tools(&mut skills, &mut tools);
    }
    if !chat_user_wants_advisory(task)
        && (lower.contains("module")
            || lower.contains("scaffold")
            || lower.contains("aospkg")
            || lower.contains("ext-rt"))
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
    } else if chat_user_wants_advisory(task)
        && (lower.contains("module") || lower.contains("aospkg") || lower.contains("ext-rt"))
    {
        // Analyse / limitations : lecture seule, pas de scaffold.
        for t in ["module.list", "module.describe"] {
            if !tools.iter().any(|x| x == t) {
                tools.push(t.into());
            }
        }
    }
    if (lower.contains("task") || lower.contains("tâche") || lower.contains("todo"))
        && !skills.iter().any(|s| s == "tasks")
    {
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
        && !tools.iter().any(|x| x == "media.audio.generate")
    {
        tools.push("media.audio.generate".into());
    }
    let wants_illust =
        illustration_open || aos_agent::tools::explicit_illust_intent(task);
    if !chat_canvas::chat_user_wants_explicit_canvas(task)
        && !canvas_open
        && !wants_illust
        && chat_device_capture_intent(task).is_none()
        && !chat_device_usb_intent(task)
        && (lower.contains("image")
            || lower.contains("png")
            || lower.contains("illustration")
            || lower.contains("diffusion")
            || chat_canvas::chat_user_has_draw_wording(task))
        && !tools.iter().any(|x| x == "media.image.generate")
    {
        tools.push("media.image.generate".into());
    }
    if canvas_open || chat_canvas::chat_user_wants_explicit_canvas(task) {
        for t in aos_agent::tools::filter_canvas_tool_ids(canvas_exported) {
            if !tools.iter().any(|x| x == &t) {
                tools.push(t);
            }
        }
    }
    if wants_illust {
        aos_agent::tools::merge_illust_tools(&mut tools, true);
        // Prefer Illustration surface over diffusion when the panel is open
        // or the intent is explicit.
        tools.retain(|t| t != "media.image.generate");
    }
    if let Some(intent) = chat_device_capture_intent(task) {
        push_device_capture_tools(&mut tools, intent);
    }
    if chat_device_usb_intent(task) {
        push_device_usb_tools(&mut tools);
    }
    if aos_agent::research_detect::user_requested_document(task) {
        aos_agent::research_detect::ensure_document_file_tools(&mut skills, &mut tools);
    }
    (skills, tools)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::{AgentKind, AgentState, CognitiveMode};

    fn sample_agent(
        id: &str,
        kind: AgentKind,
        origin: Option<&str>,
        tools: &[&str],
        skills: &[&str],
    ) -> AgentInfo {
        AgentInfo {
            agent_id: id.into(),
            state: if kind == AgentKind::Roster {
                AgentState::Roster
            } else {
                AgentState::Created
            },
            directive: String::new(),
            pid: None,
            caps: vec![],
            last_output: String::new(),
            step: 0,
            max_steps: 0,
            current_task: None,
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: skills.iter().map(|s| (*s).to_string()).collect(),
            tools: tools.iter().map(|s| (*s).to_string()).collect(),
            mcp_servers: vec![],
            fail_reason: None,
            session_id: None,
            model_id: None,
            title: id.into(),
            kind,
            display_name: Some(id.into()),
            persona_id: None,
            source_roster_id: None,
            origin: origin.map(str::to_string),
            avatar: None,
            color: None,
            deep_plan: None,
            cognitive_mode: CognitiveMode::Normal,
            execution_backend: Default::default(),
        }
    }

    #[test]
    fn match_explicit_roster_id() {
        let agents = vec![
            sample_agent(
                "agent-notes",
                AgentKind::Roster,
                Some("library"),
                &["notes.create"],
                &["notes"],
            ),
            sample_agent("persona-coder", AgentKind::Roster, None, &[], &[]),
        ];
        let hit = match_roster_for_delegate(
            &agents,
            &[],
            &["module.scaffold".into()],
            Some("persona-coder"),
        )
        .expect("explicit id");
        assert_eq!(hit.source_roster_id, "persona-coder");
    }

    #[test]
    fn match_by_tool_overlap() {
        let agents = vec![
            sample_agent("persona-coder", AgentKind::Roster, None, &[], &[]),
            sample_agent(
                "agent-mod",
                AgentKind::Roster,
                Some("library"),
                &["module.scaffold", "module.package", "module.install"],
                &[],
            ),
            sample_agent(
                "agent-notes",
                AgentKind::Roster,
                Some("library"),
                &["notes.create", "notes.list"],
                &["notes"],
            ),
        ];
        let need = vec![
            "module.scaffold".into(),
            "module.package".into(),
            "module.install".into(),
        ];
        let hit = match_roster_for_delegate(&agents, &[], &need, None).expect("tool match");
        assert_eq!(hit.source_roster_id, "agent-mod");
    }

    #[test]
    fn skip_empty_tools_and_ephemeral() {
        let agents = vec![
            sample_agent("persona-coder", AgentKind::Roster, None, &[], &[]),
            sample_agent(
                "agent-ephemeral",
                AgentKind::Task,
                Some("assistant"),
                &["module.scaffold", "module.package", "module.install"],
                &[],
            ),
        ];
        let need = vec![
            "module.scaffold".into(),
            "module.package".into(),
            "module.install".into(),
        ];
        assert!(match_roster_for_delegate(&agents, &[], &need, None).is_none());
    }

    #[test]
    fn apply_binding_unions_host_canvas_kit() {
        let mut req = AgentCreateRequest::simple("dessine un cercle");
        req.tools = vec!["canvas.stroke".into(), "canvas.rect".into()];
        req.skills = vec!["canvas".into()];
        req.system_prompt = Some("HOST_CANVAS".into());
        let binding = RosterBinding {
            source_roster_id: "agent-artist".into(),
            display_name: Some("Artist".into()),
            persona_id: None,
            system_prompt: Some("ARTIST_PERSONA".into()),
            skills: vec!["style".into()],
            tools: vec!["canvas.ellipse".into()],
            mcp_servers: vec![],
            caps: vec!["tool.invoke:canvas".into()],
            avatar: Some("spark".into()),
            color: None,
            model_id: None,
        };
        apply_roster_binding(&mut req, &binding);
        assert_eq!(req.source_roster_id.as_deref(), Some("agent-artist"));
        assert_eq!(req.display_name.as_deref(), Some("Artist"));
        assert!(req.tools.iter().any(|t| t == "canvas.stroke"));
        assert!(req.tools.iter().any(|t| t == "canvas.ellipse"));
        assert!(req.skills.iter().any(|s| s == "canvas"));
        assert!(req.skills.iter().any(|s| s == "style"));
        let prompt = req.system_prompt.as_deref().unwrap_or("");
        assert!(prompt.contains("ARTIST_PERSONA"));
        assert!(prompt.contains("HOST_CANVAS"));
    }

    #[test]
    fn parse_spawn_roster_id_arg() {
        let out = r#"Je m'en occupe.
{"action":"agent.spawn","args":{"brief":"créer une note","roster_id":"agent-notes"}}"#;
        let spec = chat_delegate_agent_spec("crée une note", out, false, false, aos_proto::CanvasAspect::Square,
            &[],
        )
        .expect("delegate");
        assert_eq!(spec.roster_id.as_deref(), Some("agent-notes"));
    }
}
