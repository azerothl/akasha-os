//! Akasha OS Preview — UI egui (ADR 0003).
//!
//! Surface testeur : chat, dashboard, onboarding, notes, confirm, agents,
//! audit, scénarios guidés, retours (`feedback.submit`).

mod agent_panel;
mod decl_ui;
mod i18n;
mod model_setup;
mod models_page;
mod notes_panel;
mod prefs;
mod tasks_panel;
mod chat_ask;
mod chat_canvas;
mod chat_media;
mod chat_room;
mod cmd;
mod image_composition;
mod image_history;
mod image_prompt;
mod image_studio;
mod nav;
mod onboarding;
mod os_open;
mod product_context;
mod runtime;
mod scenarios_panel;
mod slash;
mod theme;

use chat_ask::{agent_display_title, chat_has_open_ask, pending_ask_ids};
use cmd::{AgentNotice, ChatLine, Cmd, Evt};
use os_open::{aos_home, app_icon, bin_aos_session, native_path, open_in_browser, open_os_folder, open_url, request_preview_restart};
use runtime::runtime_main;
use slash::{slash_completions, slash_insert_text, SLASH_COMMANDS};
use aos_agent::schedule::ScheduleEntry;
use aos_ipc::BusClient;
use aos_proto::{
    AgentCreateRequest, AgentGoal, AgentIdRequest, AgentInfo, AgentState, AgentTrace, AuditEvent,
    CapInfo, ChatAttachment, ChatRoomMember, ChatSessionAppendRequest, ChatSessionGetResponse,
    ChatSessionIdRequest, ChatSessionMeta, ChatSessionMode, DocumentRef,
    FeedbackSubmitRequest, FeedbackSubmitResponse, McpServerInfo, MemHit, ModelInfo,
    ModuleCatalogue, ModuleIdRequest, ModuleInfo, ModuleInvokeRequest, ModuleInvokeResponse,
    PendingConfirmation, ProviderRecord,
    SkillInfo, SystemMetrics, WebSearchHit,
    chat_tts_request, chat_user_wants_module_authoring, ModelMetrics,
};
use aos_proto::decl_ui::ModuleUiResponse;
use prefs::{load_preferences, save_preferences, Preferences, UI_SCALE_PRESETS};
use eframe::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateOffer {
    version: String,
    tag: String,
    html_url: String,
    asset_name: String,
    download_url: String,
    size: u64,
}

fn load_update_offer() -> Option<UpdateOffer> {
    let p = aos_home().join("var/run/update_available.json");
    let raw = std::fs::read_to_string(p).ok()?;
    serde_json::from_str(&raw).ok()
}

fn load_pending_update_version() -> Option<String> {
    let p = aos_home().join("var/updates/pending.json");
    let raw = std::fs::read_to_string(p).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    v.get("version")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tab {
    Chat,
    Memory,
    Notes,
    Tasks,
    Agents,
    Models,
    Image,
    Providers,
    Audit,
    Caps,
    Scenarios,
    Feedback,
    Settings,
    Module(String),
}

fn agent_cap_holder(agent_id: &str) -> String {
    format!("agent:{agent_id}")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OnboardingState {
    completed: bool,
    language: String,
    routing: String,
    trust_default: String,
    #[serde(default)]
    tutorial_step: u32,
    /// User sent a chat message during the first-run chat step.
    #[serde(default)]
    chat_sent: bool,
    /// Assistant replied to the first-run chat message.
    #[serde(default)]
    first_chat_done: bool,
}

impl Default for OnboardingState {
    fn default() -> Self {
        let language = prefs::detect_os_language();
        Self {
            completed: false,
            language: language.clone(),
            routing: "local_only".into(),
            trust_default: "medium".into(),
            tutorial_step: 0,
            chat_sent: false,
            first_chat_done: false,
        }
    }
}

/// Vertical scroll that takes the remaining panel and appears only on overflow.
fn overflow_scroll(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .show(ui, add_contents);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChatBubbleKind {
    User,
    Assistant,
    RoomSpeaker,
    System,
}

fn chat_bubble_kind(role: &str, speaker_id: Option<&str>, room_mode: bool) -> ChatBubbleKind {
    match role {
        "user" | "vous" => ChatBubbleKind::User,
        "assistant" if room_mode && speaker_id.is_some() => ChatBubbleKind::RoomSpeaker,
        "assistant" => ChatBubbleKind::Assistant,
        _ => ChatBubbleKind::System,
    }
}

fn chat_role_label(kind: ChatBubbleKind, t: &i18n::UiStrings, raw_role: &str) -> String {
    match kind {
        ChatBubbleKind::User => t.chat_you.to_string(),
        ChatBubbleKind::Assistant => t.chat_assistant.to_string(),
        ChatBubbleKind::RoomSpeaker => String::new(), // filled from roster
        ChatBubbleKind::System => {
            if raw_role == "système" || raw_role == "system" {
                t.chat_system.to_string()
            } else {
                raw_role.to_string()
            }
        }
    }
}

fn chat_bubble_colors(kind: ChatBubbleKind, dark: bool) -> (egui::Color32, egui::Color32, egui::Color32) {
    // fill, stroke, role label — orrery-ish cyan / mute / paper without purple glow
    match (kind, dark) {
        (ChatBubbleKind::User, true) => (
            egui::Color32::from_rgb(18, 42, 48),
            egui::Color32::from_rgb(62, 224, 196),
            egui::Color32::from_rgb(120, 230, 210),
        ),
        (ChatBubbleKind::User, false) => (
            egui::Color32::from_rgb(220, 242, 238),
            egui::Color32::from_rgb(20, 140, 120),
            egui::Color32::from_rgb(10, 100, 90),
        ),
        (ChatBubbleKind::Assistant, true) => (
            egui::Color32::from_rgb(28, 32, 40),
            egui::Color32::from_rgb(90, 100, 120),
            egui::Color32::from_rgb(180, 190, 210),
        ),
        (ChatBubbleKind::Assistant, false) => (
            egui::Color32::from_rgb(236, 238, 244),
            egui::Color32::from_rgb(120, 128, 148),
            egui::Color32::from_rgb(50, 56, 72),
        ),
        (ChatBubbleKind::RoomSpeaker, true) => (
            egui::Color32::from_rgb(28, 32, 40),
            egui::Color32::from_rgb(90, 100, 120),
            egui::Color32::from_rgb(180, 190, 210),
        ),
        (ChatBubbleKind::RoomSpeaker, false) => (
            egui::Color32::from_rgb(236, 238, 244),
            egui::Color32::from_rgb(120, 128, 148),
            egui::Color32::from_rgb(50, 56, 72),
        ),
        (ChatBubbleKind::System, true) => (
            egui::Color32::from_rgb(22, 22, 26),
            egui::Color32::from_rgb(70, 70, 78),
            egui::Color32::from_rgb(150, 150, 160),
        ),
        (ChatBubbleKind::System, false) => (
            egui::Color32::from_rgb(242, 242, 244),
            egui::Color32::from_rgb(170, 170, 178),
            egui::Color32::from_rgb(100, 100, 110),
        ),
    }
}

/// Role-colored message frame. User sits on the right; assistant/system on the left.
fn chat_message_frame(
    ui: &mut egui::Ui,
    kind: ChatBubbleKind,
    color_override: Option<(egui::Color32, egui::Color32)>,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let dark = ui.visuals().dark_mode;
    let (fill, stroke) = color_override.unwrap_or_else(|| {
        let (f, s, _) = chat_bubble_colors(kind, dark);
        (f, s)
    });
    let max_w = (ui.available_width() * match kind {
        ChatBubbleKind::User => 0.88,
        ChatBubbleKind::Assistant | ChatBubbleKind::RoomSpeaker => 0.96,
        ChatBubbleKind::System => 0.92,
    })
    .clamp(200.0, ui.available_width());

    let layout = match kind {
        ChatBubbleKind::User => egui::Layout::right_to_left(egui::Align::Min),
        _ => egui::Layout::left_to_right(egui::Align::Min),
    };

    ui.with_layout(layout, |ui| {
        ui.set_max_width(max_w);
        egui::Frame::NONE
            .fill(fill)
            .stroke(egui::Stroke::new(1.0_f32, stroke))
            .corner_radius(6.0)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_max_width(max_w - 8.0);
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    add_contents(ui);
                });
            });
    });
    ui.add_space(6.0);
}

fn main() -> eframe::Result<()> {
    if std::env::var_os("AOS_MODEL_SETUP").is_some() {
        return model_setup::run();
    }

    let (cmd_tx, cmd_rx) = channel::<Cmd>();
    let (evt_tx, evt_rx) = channel::<Evt>();
    let version = std::env::var("AOS_PREVIEW_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title(format!("Akasha OS Preview {version}"))
            .with_icon(app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        &format!("Akasha OS Preview {version}"),
        options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().expect("tokio");
                rt.block_on(runtime_main(cmd_rx, evt_tx, ctx));
            });
            Ok(Box::new(UiApp::new(cmd_tx, evt_rx, version)))
        }),
    )
}

fn onboarding_path() -> PathBuf {
    aos_home().join("var/run/onboarding.json")
}

fn load_onboarding() -> OnboardingState {
    let p = onboarding_path();
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_onboarding(state: &OnboardingState) {
    let p = onboarding_path();
    let _ = std::fs::create_dir_all(p.parent().unwrap());
    if let Ok(s) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(p, s);
    }
}

pub(crate) fn chrono_like_stamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

fn human_bytes(v: u64) -> String {
    const GIB: f64 = (1u64 << 30) as f64;
    const MIB: f64 = (1u64 << 20) as f64;
    if v >= (1u64 << 30) {
        format!("{:.2} GiB", v as f64 / GIB)
    } else if v >= (1u64 << 20) {
        format!("{:.1} MiB", v as f64 / MIB)
    } else {
        format!("{v} B")
    }
}

fn format_model_infer_line(mm: &ModelMetrics, t: &i18n::UiStrings) -> String {
    let vram = format!("{:.0} MiB", mm.vram_bytes as f64 / (1 << 20) as f64);
    if mm.media_total_steps.is_some() || mm.media_step.is_some() {
        let step = mm
            .media_step
            .map(|v| v.to_string())
            .unwrap_or_else(|| "—".into());
        let total = mm
            .media_total_steps
            .map(|v| v.to_string())
            .unwrap_or_else(|| "—".into());
        let step_s = mm
            .last_step_s
            .map(|v| format!("{v:.2}"))
            .unwrap_or_else(|| "—".into());
        format!(
            "{} {}/{} · {} {} · {} {}",
            t.metrics_step, step, total, t.metrics_step_s, step_s, t.metrics_vram, vram
        )
    } else {
        let ttft = mm
            .last_ttft_ms
            .map(|v| format!("{v:.0} ms"))
            .unwrap_or_else(|| "—".into());
        let toks = mm
            .last_tok_s
            .map(|v| format!("{v:.1}"))
            .unwrap_or_else(|| "—".into());
        let mut line = format!(
            "{} {} · {} {} · {} {}",
            t.metrics_ttft, ttft, t.metrics_tok_s, toks, t.metrics_vram, vram
        );
        if let Some(d) = mm.draft_accept {
            line.push_str(&format!(" · {} {d:.1}", t.metrics_draft));
        }
        if let Some(p) = mm.prefix_hit {
            if p > 0 {
                line.push_str(&format!(" · {} {p}", t.metrics_prefix));
            }
        }
        line
    }
}

fn agent_is_live(state: &AgentState) -> bool {
    matches!(
        state,
        AgentState::Created | AgentState::Running | AgentState::Paused | AgentState::Blocked
    )
}

fn agent_shown_in_tab(a: &AgentInfo, history: bool) -> bool {
    if a.is_roster() {
        return !history;
    }
    agent_is_live(&a.state) != history
}

fn agent_completion_chat_text(ag: &AgentInfo) -> String {
    let title = ag.display_title();
    match ag.state {
        AgentState::Done => {
            let out = ag.last_output.trim();
            if out.is_empty() {
                format!("Agent « {title} » terminé.")
            } else {
                let body: String = out.chars().take(8000).collect();
                format!("**Résultat — {title}**\n\n{body}")
            }
        }
        AgentState::Failed => format!(
            "Agent « {title} » a échoué : {}",
            ag.fail_reason.as_deref().unwrap_or("échec")
        ),
        AgentState::Killed => format!("Agent « {title} » arrêté."),
        _ => format!("Agent « {title} » terminé."),
    }
}

pub(crate) async fn load_session(bus: &Arc<BusClient>, evt_tx: &Sender<Evt>, id: &str) {
    match bus
        .call::<ChatSessionIdRequest, ChatSessionGetResponse>(
            "chat.session.get",
            &ChatSessionIdRequest {
                session_id: id.to_string(),
            },
            vec![],
        )
        .await
    {
        Ok(resp) => {
            let messages: Vec<ChatLine> = resp
                .messages
                .into_iter()
                .map(|m| {
                    let role = match m.role.as_str() {
                        "vous" => "user".into(),
                        other => other.to_string(),
                    };
                    ChatLine {
                        role,
                        text: m.content,
                        attachments: m.attachments,
                        speaker_id: m.speaker_id,
                    }
                })
                .collect();
            let meta = resp.meta;
            let _ = evt_tx.send(Evt::SessionLoaded {
                id: meta.id.clone(),
                messages,
                meta,
            });
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::Error(e.to_string()));
        }
    }
}

/// Kit des agents lancés depuis le chat : plan, notes, sous-agents — toujours.
const CHAT_AGENT_MIN_STEPS: u32 = 64;
pub(crate) const CHAT_AGENT_MAX_SUBAGENTS: u32 = 8;

fn chat_agent_max_steps(prefs_max: u32) -> u32 {
    prefs_max.max(CHAT_AGENT_MIN_STEPS).min(128)
}

fn chat_action_is_self_tool(action: &str) -> bool {
    matches!(
        action,
        "module.scaffold" | "module.package" | "module.install" | "module.uninstall" | "skill.create"
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

/// Retire les outils incompatibles avec le kit canvas (vectoriel) vs pixel (diffusion).
fn strip_delegate_kit_tools(tools: &mut Vec<String>, skills: &mut Vec<String>, use_canvas: bool) {
    if use_canvas {
        tools.retain(|t| {
            !t.starts_with("media.image")
                && t != "user.ask"
                && t != "agent.spawn"
                && t != "agent.await"
        });
        skills.retain(|s| s != "planner");
    } else {
        tools.retain(|t| !t.starts_with("canvas."));
    }
}

fn chat_delegate_kit(
    brief: &str,
    canvas_open: bool,
    use_canvas: bool,
) -> (Vec<String>, Vec<String>) {
    let (mut skills, mut tools) = chat_agent_kit_ex(brief, canvas_open || use_canvas);
    strip_delegate_kit_tools(&mut tools, &mut skills, use_canvas);
    (skills, tools)
}

/// Si le chat doit déléguer : (brief, skills, tools, phrase d'accusé).
pub(crate) fn chat_delegate_agent_spec(
    user_text: &str,
    model_output: &str,
    canvas_open: bool,
    _canvas_aspect: aos_proto::CanvasAspect,
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
            let (mut skills, mut tools) = chat_delegate_kit(&brief, canvas_open, use_canvas);
            merge_named_args(&mut skills, &action.args, "skills");
            merge_named_args(&mut tools, &action.args, "tools");
            if use_canvas {
                // Ensure canvas tools even if model passed a non-canvas tools list.
                let (_, canvas_tools) = chat_agent_kit_ex(&brief, true);
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
        let (skills, tools) = chat_delegate_kit(user_text, canvas_open, true);
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
        let (skills, tools) = chat_delegate_kit(user_text, canvas_open, true);
        return Some((
            user_text.to_string(),
            skills,
            tools,
            "Je lance un agent pour dessiner sur le canvas.".into(),
        ));
    }
    if chat_canvas::chat_user_wants_pixel_draw(user_text, canvas_open) {
        let (skills, tools) = chat_delegate_kit(user_text, canvas_open, false);
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
    req.skills = skills;
    req.tools = tools;
    req.session_id = Some(sid.clone());
    if canvas_delegate {
        req.system_prompt = Some(chat_canvas::canvas_agent_system_prompt(canvas_aspect));
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
    req.caps.push("tool.invoke:notes".into());
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

fn chat_agent_kit(task: &str) -> (Vec<String>, Vec<String>) {
    chat_agent_kit_ex(task, false)
}

fn chat_agent_kit_ex(task: &str, canvas_open: bool) -> (Vec<String>, Vec<String>) {
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
    if lower.contains("task") || lower.contains("tâche") || lower.contains("todo") {
        if !skills.iter().any(|s| s == "tasks") {
            skills.push("tasks".into());
        }
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
    if lower.contains("audio")
        || lower.contains("tts")
        || lower.contains("voix")
        || lower.contains("speech")
        || lower.contains("speak")
        || lower.contains("wav")
        || lower.contains("vocal")
    {
        if !tools.iter().any(|x| x == "media.audio.generate") {
            tools.push("media.audio.generate".into());
        }
    }
    if !chat_canvas::chat_user_wants_explicit_canvas(task)
        && !canvas_open
        && (lower.contains("image")
            || lower.contains("png")
            || lower.contains("illustration")
            || lower.contains("diffusion")
            || chat_canvas::chat_user_has_draw_wording(task))
    {
        if !tools.iter().any(|x| x == "media.image.generate") {
            tools.push("media.image.generate".into());
        }
    }
    if canvas_open || chat_canvas::chat_user_wants_explicit_canvas(task) {
        for t in [
            "canvas.set_style",
            "canvas.stroke",
            "canvas.line",
            "canvas.spline",
            "canvas.rect",
            "canvas.ellipse",
            "canvas.erase",
            "canvas.clear",
            "canvas.undo",
            "canvas.get",
            "canvas.export",
        ] {
            if !tools.iter().any(|x| x == t) {
                tools.push(t.into());
            }
        }
    }
    (skills, tools)
}

/// Collecte un diagnostic Preview, l'archive localement et préremplit l'onglet Retour.
pub(crate) async fn run_troubleshoot(bus: &Arc<BusClient>, evt_tx: &Sender<Evt>) {
    let _ = evt_tx.send(Evt::Status("Dépannage : collecte des diagnostics…".into()));
    let home = aos_home();
    let version = std::fs::read_to_string(home.join("VERSION"))
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into())
        .trim()
        .to_string();

    let mut findings: Vec<String> = Vec::new();
    let mut sections: Vec<String> = Vec::new();

    sections.push(format!("## Environnement\n- version: {version}\n- AOS_HOME: {}\n- os: {}", home.display(), std::env::consts::OS));

    // NVIDIA
    let nvidia = std::process::Command::new("nvidia-smi")
        .args(["-L"])
        .output();
    match nvidia {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout).trim().to_string();
            sections.push(format!("## NVIDIA\n```\n{text}\n```"));
            if text.is_empty() {
                findings.push("nvidia-smi -L OK mais sortie vide".into());
            }
        }
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            findings.push(format!("nvidia-smi a échoué : {err}"));
            sections.push(format!("## NVIDIA\nERREUR:\n```\n{err}\n```"));
        }
        Err(e) => {
            findings.push(format!("nvidia-smi introuvable : {e}"));
            sections.push(format!("## NVIDIA\nintrouvable: {e}"));
        }
    }

    // Logs daemons (dernières lignes)
    let run_dir = home.join("var/run");
    let mut log_block = String::from("## Logs daemons (var/run)\n");
    let mut log_errors = 0usize;
    if let Ok(rd) = std::fs::read_dir(&run_dir) {
        let mut files: Vec<_> = rd.filter_map(|e| e.ok()).collect();
        files.sort_by_key(|e| e.file_name());
        for ent in files {
            let name = ent.file_name().to_string_lossy().to_string();
            if !(name.ends_with(".stderr.log") || name.ends_with(".stdout.log") || name.ends_with(".log"))
            {
                continue;
            }
            let path = ent.path();
            let raw = std::fs::read_to_string(&path).unwrap_or_default();
            let lines: Vec<&str> = raw.lines().collect();
            let start = lines.len().saturating_sub(40);
            let tail = lines[start..].join("\n");
            for line in &lines[start..] {
                let lower = line.to_lowercase();
                if lower.contains("error")
                    || lower.contains("panic")
                    || lower.contains("fatal")
                    || lower.contains("échec")
                    || lower.contains("failed")
                {
                    log_errors += 1;
                }
            }
            if !tail.trim().is_empty() {
                log_block.push_str(&format!("### {name}\n```\n{tail}\n```\n"));
            }
        }
    } else {
        log_block.push_str("(dossier var/run absent)\n");
        findings.push("var/run inaccessible".into());
    }
    if log_errors > 0 {
        findings.push(format!("{log_errors} ligne(s) d'erreur détectée(s) dans les logs récents"));
    }
    sections.push(log_block);

    // Services via bus
    let mut svc = String::from("## Services (bus)\n");
    match bus
        .call::<(), Vec<AgentInfo>>(aos_agent::intents::LIST, &(), vec![])
        .await
    {
        Ok(agents) => {
            svc.push_str(&format!("- agents actifs : {}\n", agents.len()));
            for a in agents.iter().take(12) {
                svc.push_str(&format!(
                    "  - {} [{:?}] step {}/{}\n",
                    a.agent_id, a.state, a.step, a.max_steps
                ));
                if matches!(a.state, AgentState::Failed) {
                    findings.push(format!("agent {} en Failed", a.agent_id));
                }
            }
        }
        Err(e) => {
            findings.push(format!("agent.list inaccessible : {e}"));
            svc.push_str(&format!("- agent.list ERREUR : {e}\n"));
        }
    }
    match bus.call::<(), Vec<ModelInfo>>("model.list", &(), vec![]).await {
        Ok(models) => {
            svc.push_str(&format!("- modèles : {}\n", models.len()));
            for m in models.iter().take(8) {
                svc.push_str(&format!("  - {} [{:?}]\n", m.id, m.state));
            }
        }
        Err(e) => {
            findings.push(format!("model.list inaccessible : {e}"));
            svc.push_str(&format!("- model.list ERREUR : {e}\n"));
        }
    }
    match bus
        .call::<(), serde_json::Value>("module.list", &(), vec![])
        .await
    {
        Ok(v) => svc.push_str(&format!("- modules : {v}\n")),
        Err(e) => {
            findings.push(format!("module.list inaccessible : {e}"));
            svc.push_str(&format!("- module.list ERREUR : {e}\n"));
        }
    }
    sections.push(svc);

    let healthy = findings.is_empty();
    let summary = if healthy {
        "Aucune anomalie évidente détectée. Les logs daemons restent disponibles sous var/run/."
            .to_string()
    } else {
        format!(
            "{} anomalie(s) :\n{}",
            findings.len(),
            findings
                .iter()
                .map(|f| format!("- {f}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };

    let body = format!(
        "## Résumé dépannage automatique\n\n{summary}\n\n{}\n",
        sections.join("\n")
    );

    let _ = evt_tx.send(Evt::Status(if healthy {
        "Dépannage : OK — aucune anomalie majeure".into()
    } else {
        format!("Dépannage : {} anomalie(s) — rapport en cours…", findings.len())
    }));

    // Archive locale + brouillon dans l'onglet Retour (l'utilisateur publie l'issue).
    let req = FeedbackSubmitRequest {
        title: if healthy {
            format!("[Preview][diag] OK — v{version}")
        } else {
            format!("[Preview][bug] Dépannage auto — {} anomalie(s)", findings.len())
        },
        category: if healthy { "other".into() } else { "bug".into() },
        severity: if healthy {
            "low".into()
        } else if findings.len() >= 3 {
            "high".into()
        } else {
            "medium".into()
        },
        body,
        scenario: Some("troubleshooting".into()),
        meta: serde_json::json!({
            "preview_version": version,
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "source": "troubleshooting_button",
            "findings": findings,
            "healthy": healthy,
        }),
        // Copie locale seulement : l'issue GitHub est créée quand l'utilisateur
        // envoie le formulaire prérempli, pour que le rapport y figure.
        publish_github: false,
    };

    match bus
        .call::<FeedbackSubmitRequest, FeedbackSubmitResponse>(
            "feedback.submit",
            &req,
            vec![],
        )
        .await
    {
        Ok(r) => {
            let _ = evt_tx.send(Evt::FeedbackOk(r));
            let mut draft = req;
            draft.publish_github = !healthy;
            let _ = evt_tx.send(Evt::FeedbackDraft(draft));
            let _ = evt_tx.send(Evt::Status(if healthy {
                "Dépannage OK — rapport local prêt dans l'onglet Retour".into()
            } else {
                format!(
                    "Dépannage : {} anomalie(s) — rapport prêt, envoyez-le depuis Retour",
                    findings.len()
                )
            }));
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::Error(format!("Dépannage : échec feedback.submit : {e}")));
            // Même en cas d'échec de la sauvegarde locale, on pré-remplit le formulaire
            // Retour pour que l'utilisateur puisse quand même remonter l'issue avec le rapport.
            let mut draft = req;
            draft.publish_github = !healthy;
            let _ = evt_tx.send(Evt::FeedbackDraft(draft));
        }
    }
}

pub(crate) async fn load_module_ui(bus: &Arc<BusClient>, evt_tx: &Sender<Evt>, module: &str) {
    match bus
        .call::<ModuleIdRequest, ModuleUiResponse>(
            "module.ui",
            &ModuleIdRequest {
                module: module.to_string(),
            },
            vec![],
        )
        .await
    {
        Ok(resp) => {
            let _ = evt_tx.send(Evt::ModuleUiLoaded(resp));
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiFailed {
                module: module.to_string(),
                error: e.to_string(),
            });
        }
    }
}

pub(crate) async fn invoke_module_bind(
    bus: &Arc<BusClient>,
    evt_tx: &Sender<Evt>,
    module: &str,
    tool: &str,
) {
    let req = ModuleInvokeRequest {
        module: module.to_string(),
        tool: tool.to_string(),
        args: serde_json::json!({}),
        actor: "human:ui".into(),
        actor_caps: vec![format!("tool.invoke:{module}")],
        trace_id: format!("ui-mod-bind-{module}-{tool}"),
    };
    match bus
        .call::<ModuleInvokeRequest, ModuleInvokeResponse>("module.invoke", &req, vec![])
        .await
    {
        Ok(r) if r.ok => {
            let _ = evt_tx.send(Evt::ModuleUiBind {
                module: module.to_string(),
                tool: tool.to_string(),
                result: r.result,
                error: None,
            });
        }
        Ok(r) => {
            let _ = evt_tx.send(Evt::ModuleUiBind {
                module: module.to_string(),
                tool: tool.to_string(),
                result: serde_json::Value::Null,
                error: r.error,
            });
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiBind {
                module: module.to_string(),
                tool: tool.to_string(),
                result: serde_json::Value::Null,
                error: Some(e.to_string()),
            });
        }
    }
}

pub(crate) async fn invoke_module_tool(
    bus: &Arc<BusClient>,
    evt_tx: &Sender<Evt>,
    module: &str,
    tool: &str,
    args: serde_json::Value,
) {
    let req = ModuleInvokeRequest {
        module: module.to_string(),
        tool: tool.to_string(),
        args,
        actor: "human:ui".into(),
        actor_caps: vec![format!("tool.invoke:{module}")],
        trace_id: format!("ui-mod-{module}-{tool}"),
    };
    match bus
        .call::<ModuleInvokeRequest, ModuleInvokeResponse>("module.invoke", &req, vec![])
        .await
    {
        Ok(r) if r.ok => {
            let _ = evt_tx.send(Evt::ModuleUiInvokeDone {
                module: module.to_string(),
                tool: tool.to_string(),
                ok: true,
                result: r.result.clone(),
                error: None,
            });
            let _ = evt_tx.send(Evt::ModuleUiBind {
                module: module.to_string(),
                tool: tool.to_string(),
                result: r.result,
                error: None,
            });
        }
        Ok(r) => {
            let _ = evt_tx.send(Evt::ModuleUiInvokeDone {
                module: module.to_string(),
                tool: tool.to_string(),
                ok: false,
                result: serde_json::Value::Null,
                error: r.error.clone(),
            });
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiInvokeDone {
                module: module.to_string(),
                tool: tool.to_string(),
                ok: false,
                result: serde_json::Value::Null,
                error: Some(e.to_string()),
            });
        }
    }
}

pub(crate) async fn invoke_notes(
    bus: &Arc<BusClient>,
    evt_tx: &Sender<Evt>,
    tool: &str,
    args: serde_json::Value,
) {
    let req = ModuleInvokeRequest {
        module: "notes".into(),
        tool: tool.into(),
        args,
        actor: "human:ui".into(),
        actor_caps: vec![
            "fs.read:/documents/notes/**".into(),
            "fs.write:/documents/notes/**".into(),
            "mem.write:module:notes".into(),
            "mem.query:module:notes".into(),
            "tool.invoke:notes".into(),
        ],
        trace_id: format!("ui-notes-{}", tool),
    };
    match bus
        .call::<ModuleInvokeRequest, ModuleInvokeResponse>("module.invoke", &req, vec![])
        .await
    {
        Ok(r) if r.ok => {
            let pretty = serde_json::to_string_pretty(&r.result).unwrap_or_default();
            let _ = evt_tx.send(Evt::Notes(pretty));
            match tool {
                "notes.list" => {
                    let notes = notes_panel::parse_list_result(&r.result);
                    let _ = evt_tx.send(Evt::NotesListed(notes));
                }
                "notes.read" => {
                    if let Some(d) = notes_panel::parse_detail(&r.result) {
                        let _ = evt_tx.send(Evt::NoteLoaded(d));
                    }
                }
                "notes.search" => {
                    let hits = notes_panel::parse_search_hits(&r.result);
                    let _ = evt_tx.send(Evt::NotesSearchHits(hits));
                }
                "notes.related" => {
                    let hits = notes_panel::parse_related(&r.result);
                    let _ = evt_tx.send(Evt::NotesRelated(hits));
                }
                "notes.create" | "notes.update" => {
                    let path = r
                        .result
                        .get("path")
                        .and_then(|p| p.as_str())
                        .unwrap_or("")
                        .to_string();
                    let slug = r
                        .result
                        .get("slug")
                        .and_then(|p| p.as_str())
                        .unwrap_or("")
                        .to_string();
                    let title = r
                        .result
                        .get("title")
                        .and_then(|p| p.as_str())
                        .unwrap_or("")
                        .to_string();
                    let _ = evt_tx.send(Evt::NotesSaved { path, slug, title });
                    // Rafraîchir la liste après écriture.
                    let list_req = ModuleInvokeRequest {
                        module: "notes".into(),
                        tool: "notes.list".into(),
                        args: serde_json::json!({}),
                        actor: "human:ui".into(),
                        actor_caps: vec![
                            "fs.read:/documents/notes/**".into(),
                            "tool.invoke:notes".into(),
                        ],
                        trace_id: "ui-notes-list-after-save".into(),
                    };
                    if let Ok(lr) = bus
                        .call::<ModuleInvokeRequest, ModuleInvokeResponse>(
                            "module.invoke",
                            &list_req,
                            vec![],
                        )
                        .await
                    {
                        if lr.ok {
                            let notes = notes_panel::parse_list_result(&lr.result);
                            let _ = evt_tx.send(Evt::NotesListed(notes));
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(r) => {
            let _ = evt_tx.send(Evt::Error(
                r.error.unwrap_or_else(|| "notes: échec".into()),
            ));
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::Error(e.to_string()));
        }
    }
}

pub(crate) async fn invoke_tasks(
    bus: &Arc<BusClient>,
    evt_tx: &Sender<Evt>,
    tool: &str,
    args: serde_json::Value,
) {
    let req = ModuleInvokeRequest {
        module: "tasks".into(),
        tool: tool.into(),
        args,
        actor: "human:ui".into(),
        actor_caps: vec![
            "fs.read:/documents/tasks/**".into(),
            "fs.write:/documents/tasks/**".into(),
            "tool.invoke:tasks".into(),
        ],
        trace_id: format!("ui-tasks-{tool}"),
    };
    match bus
        .call::<ModuleInvokeRequest, ModuleInvokeResponse>("module.invoke", &req, vec![])
        .await
    {
        Ok(r) if r.ok => {
            match tool {
                "tasks.list" => {
                    let tasks = tasks_panel::parse_list_result(&r.result);
                    let _ = evt_tx.send(Evt::TasksListed(tasks));
                }
                "tasks.create" | "tasks.update" | "tasks.complete" => {
                    let _ = evt_tx.send(Evt::Status(format!("{tool} OK")));
                    let list_req = ModuleInvokeRequest {
                        module: "tasks".into(),
                        tool: "tasks.list".into(),
                        args: serde_json::json!({}),
                        actor: "human:ui".into(),
                        actor_caps: vec![
                            "fs.read:/documents/tasks/**".into(),
                            "tool.invoke:tasks".into(),
                        ],
                        trace_id: "ui-tasks-list-after".into(),
                    };
                    if let Ok(lr) = bus
                        .call::<ModuleInvokeRequest, ModuleInvokeResponse>(
                            "module.invoke",
                            &list_req,
                            vec![],
                        )
                        .await
                    {
                        if lr.ok {
                            let tasks = tasks_panel::parse_list_result(&lr.result);
                            let _ = evt_tx.send(Evt::TasksListed(tasks));
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(r) => {
            let _ = evt_tx.send(Evt::Error(
                r.error.unwrap_or_else(|| "tasks: échec".into()),
            ));
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::Error(e.to_string()));
        }
    }
}

pub(crate) async fn agent_id_cmd(bus: &Arc<BusClient>, evt_tx: &Sender<Evt>, intent: &str, id: String) {
    match bus
        .call::<AgentIdRequest, bool>(intent, &AgentIdRequest { agent_id: id }, vec![])
        .await
    {
        Ok(_) => {
            let _ = evt_tx.send(Evt::Status(format!("{intent} ok")));
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::Error(e.to_string()));
        }
    }
}

struct UiApp {
    cmd_tx: Sender<Cmd>,
    evt_rx: Receiver<Evt>,
    version: String,
    tab: Tab,
    chat: Vec<ChatLine>,
    streaming: String,
    input: String,
    chat_pending: bool,
    chat_inference_id: Option<u64>,
    catalogue: Option<ModuleCatalogue>,
    installed_modules: Vec<ModuleInfo>,
    sessions: Vec<ChatSessionMeta>,
    active_session: Option<String>,
    rename_buf: String,
    network_online: bool,
    web_query: String,
    web_results: Vec<WebSearchHit>,
    fetch_url: String,
    browse_preview: String,
    prefs: Preferences,
    agent_timeout_secs: u64,
    gen_format: String,
    gen_content: String,
    gen_path: String,
    mem_query: String,
    mem_note: String,
    mem_hits: Vec<MemHit>,
    mem_show_superseded: bool,
    mem_edit_id: Option<u64>,
    mem_edit_text: String,
    secret_brave: String,
    secret_github: String,
    secret_openai: String,
    secret_names: Vec<String>,
    secret_vault_encrypted: bool,
    metrics: Option<SystemMetrics>,
    agents: Vec<AgentInfo>,
    /// États précédents pour détecter Done/Failed/Killed (notifications).
    agent_prev_states: HashMap<String, AgentState>,
    /// Notices terminales hors session active.
    agent_notices: Vec<AgentNotice>,
    /// agent_id déjà notifiés (dédup).
    agent_notified: std::collections::HashSet<String>,
    confirms: Vec<PendingConfirmation>,
    notes: notes_panel::NotesPanelState,
    /// Dernier payload notes brut (scénarios / debug).
    notes_out: String,
    tasks: tasks_panel::TasksPanelState,
    schedules: Vec<ScheduleEntry>,
    schedule_goal: String,
    schedule_interval_secs: u64,
    agent_display_name: String,
    agent_task: String,
    agent_system_prompt: String,
    agent_docs: String,
    agent_max_steps: u32,
    agent_optimize: bool,
    skill_catalog: Vec<SkillInfo>,
    skill_selected: Vec<String>,
    mcp_catalog: Vec<McpServerInfo>,
    mcp_selected: Vec<String>,
    tool_selected: Vec<String>,
    agent_open_tabs: Vec<String>,
    agent_active_tab: Option<String>,
    agent_show_history: bool,
    agent_traces: HashMap<String, AgentTrace>,
    trace_fetched_at: Option<Instant>,
    agent_steer_id: String,
    agent_steer_txt: String,
    audit: Vec<AuditEvent>,
    caps: Vec<CapInfo>,
    caps_holder: String,
    status: String,
    onboarding: OnboardingState,
    show_onboarding: bool,
    pending_note_agent: bool,
    pending_module_agent: bool,
    pending_module_baseline: Vec<String>,
    // scenarios
    scen_chat: bool,
    scen_note_human: bool,
    scen_note_agent: bool,
    scen_confirm: bool,
    scen_audit: bool,
    scen_module_agent: bool,
    // feedback
    fb_title: String,
    fb_category: String,
    fb_severity: String,
    fb_body: String,
    fb_scenario: String,
    fb_result: String,
    fb_github: bool,
    fb_dir: Option<PathBuf>,
    /// Méta du rapport de dépannage (préservée pour la remontée d'issue).
    fb_diag_meta: Option<serde_json::Value>,
    chat_md_cache: CommonMarkCache,
    update_offer: Option<UpdateOffer>,
    update_status: String,
    model_infos: Vec<ModelInfo>,
    providers: Vec<ProviderRecord>,
    provider_id: String,
    provider_preset: String,
    provider_endpoint: String,
    provider_secret_name: String,
    provider_secret_value: String,
    provider_enabled: bool,
    provider_test_msg: String,
    agent_model_id: String,
    model_updates_msg: String,
    download_status: String,
    model_download: Option<ModelDownloadUiState>,
    model_download_restart: Option<String>,
    /// Agent visé pour la prochaine réponse `user.ask` (plusieurs bloqués).
    ask_reply_target: Option<String>,
    /// Re-focus chat TextEdit after send (Enter clears focus).
    chat_refocus: bool,
    decl_panels: HashMap<String, decl_ui::DeclUiPanelState>,
    decl_md_cache: CommonMarkCache,
    image_studio: image_studio::ImageStudioState,
    image_generating: Option<image_studio::ImageGenUiState>,
    models_catalog_tab: models_page::ModelCatalogTab,
    hf_download_url: String,
    hf_download_name: String,
    hf_download_status: String,
    show_go_to_palette: bool,
    agent_join_room_on_create: bool,
    /// Last user message for an in-flight room turn (speaker queue in thinking UI).
    room_turn_pending_text: Option<String>,
    /// Room: Members pane toggled from clickable session header.
    room_members_pane_open: bool,
    canvas_panel: chat_canvas::CanvasPanelState,
    roster_edit_drafts: HashMap<String, RosterEditDraft>,
}

#[derive(Clone, Default)]
struct RosterEditDraft {
    display_name: String,
    role: String,
    system_prompt: String,
    skills: Vec<String>,
    tools: Vec<String>,
    mcp_servers: Vec<String>,
    model_id: String,
}

const ROSTER_TOOL_GROUPS: &[(&str, &[&str])] = &[
    (
        "notes",
        &[
            "notes.create",
            "notes.list",
            "notes.read",
            "notes.search",
            "notes.update",
            "notes.links",
            "notes.related",
        ],
    ),
    (
        "tasks",
        &[
            "tasks.create",
            "tasks.list",
            "tasks.update",
            "tasks.complete",
        ],
    ),
    (
        "files",
        &["fs.read", "fs.write", "fs.list", "files.generate"],
    ),
    (
        "web",
        &["web.search", "web.browse", "net.fetch"],
    ),
    (
        "canvas",
        &[
            "canvas.stroke",
            "canvas.rect",
            "canvas.ellipse",
            "canvas.erase",
            "canvas.clear",
            "canvas.undo",
            "canvas.get",
            "canvas.export",
        ],
    ),
    (
        "agents",
        &["agent.spawn", "agent.await", "plan.update"],
    ),
];

fn roster_tool_family_label(t: &i18n::UiStrings, family: &str) -> &'static str {
    match family {
        "notes" => t.agents_tool_family_notes,
        "tasks" => t.agents_tool_family_tasks,
        "files" => t.agents_tool_family_files,
        "web" => t.agents_tool_family_web,
        "canvas" => t.agents_tool_family_canvas,
        "agents" => t.agents_tool_family_agents,
        _ => "?",
    }
}

fn ui_roster_tool_checkboxes(ui: &mut egui::Ui, t: &i18n::UiStrings, selected: &mut Vec<String>) {
    for (family, tools) in ROSTER_TOOL_GROUPS {
        ui.label(roster_tool_family_label(t, family));
        ui.indent(family, |ui| {
            for name in *tools {
                let label = i18n::roster_tool_label(t, name);
                let mut on = selected.iter().any(|t| t == name);
                if ui
                    .checkbox(&mut on, label)
                    .on_hover_text(*name)
                    .changed()
                {
                    if on {
                        selected.push((*name).into());
                    } else {
                        selected.retain(|t| t != name);
                    }
                }
            }
        });
        ui.add_space(4.0);
    }
}

#[derive(Debug, Clone)]
struct ModelDownloadUiState {
    model_id: String,
    percent: u8,
    done_bytes: u64,
    total_bytes: u64,
}

impl UiApp {
    fn new(cmd_tx: Sender<Cmd>, evt_rx: Receiver<Evt>, version: String) -> Self {
        let onboarding = load_onboarding();
        let mut prefs = load_preferences();
        if prefs.language.is_empty() {
            prefs.language = onboarding.language.clone();
        }
        let show_onboarding = !onboarding.completed;
        let t = i18n::strings(&prefs.language);
        let _ = cmd_tx.send(Cmd::SessionBootstrap);
        let _ = cmd_tx.send(Cmd::CatalogueRefresh);
        let _ = cmd_tx.send(Cmd::ModuleList);
        let _ = cmd_tx.send(Cmd::SetRouting {
            mode: prefs.routing.clone(),
        });
        let _ = cmd_tx.send(Cmd::ProviderList);
        if prefs.network_online {
            let _ = cmd_tx.send(Cmd::NetSetMode { online: true });
        }
        let model_updates_msg = std::fs::read_to_string(aos_home().join("var/run/model_updates.json"))
            .ok()
            .and_then(|s| {
                serde_json::from_str::<serde_json::Value>(&s)
                    .ok()
                    .and_then(|v| v.get("reason").and_then(|r| r.as_str()).map(|s| s.to_string()))
            })
            .unwrap_or_default();
        let default_model = prefs.default_agent_model.clone().unwrap_or_default();
        let agent_max_steps = prefs.default_max_steps;
        let agent_timeout_secs = prefs.default_timeout_secs;
        let network_online = prefs.network_online;
        let intro = format!(
            "{}\n\
             Sessions / Memory / Network opt-in.\n\
             Type /commands — use the side tabs.",
            t.preview_banner.replace("{}", &version)
        );
        Self {
            cmd_tx,
            evt_rx,
            version,
            tab: Tab::Chat,
            chat: vec![ChatLine::plain("système", intro)],
            streaming: String::new(),
            input: String::new(),
            chat_pending: false,
            chat_inference_id: None,
            catalogue: None,
            installed_modules: Vec::new(),
            sessions: Vec::new(),
            active_session: None,
            rename_buf: String::new(),
            network_online,
            web_query: String::new(),
            web_results: Vec::new(),
            fetch_url: String::new(),
            browse_preview: String::new(),
            prefs,
            agent_timeout_secs,
            gen_format: "md".into(),
            gen_content: String::new(),
            gen_path: "/downloads/note.md".into(),
            mem_query: String::new(),
            mem_note: String::new(),
            mem_hits: Vec::new(),
            mem_show_superseded: true,
            mem_edit_id: None,
            mem_edit_text: String::new(),
            secret_brave: String::new(),
            secret_github: String::new(),
            secret_openai: String::new(),
            secret_names: Vec::new(),
            secret_vault_encrypted: false,
            metrics: None,
            agents: Vec::new(),
            agent_prev_states: HashMap::new(),
            agent_notices: Vec::new(),
            agent_notified: std::collections::HashSet::new(),
            confirms: Vec::new(),
            notes: notes_panel::NotesPanelState::default(),
            notes_out: String::new(),
            tasks: tasks_panel::TasksPanelState::default(),
            schedules: Vec::new(),
            schedule_goal: String::new(),
            schedule_interval_secs: 60,
            agent_display_name: String::new(),
            agent_task: String::new(),
            agent_system_prompt: String::new(),
            agent_docs: String::new(),
            agent_max_steps,
            agent_optimize: false,
            skill_catalog: Vec::new(),
            skill_selected: Vec::new(),
            mcp_catalog: Vec::new(),
            mcp_selected: Vec::new(),
            tool_selected: vec![
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
                "module.scaffold".into(),
                "module.package".into(),
                "module.install".into(),
                "module.list".into(),
            ],
            agent_open_tabs: Vec::new(),
            agent_active_tab: None,
            agent_show_history: false,
            agent_traces: HashMap::new(),
            trace_fetched_at: None,
            agent_steer_id: String::new(),
            agent_steer_txt: String::new(),
            audit: Vec::new(),
            caps: Vec::new(),
            caps_holder: String::new(),
            status: String::new(),
            onboarding,
            show_onboarding,
            pending_note_agent: false,
            pending_module_agent: false,
            pending_module_baseline: Vec::new(),
            scen_chat: false,
            scen_note_human: false,
            scen_note_agent: false,
            scen_confirm: false,
            scen_audit: false,
            scen_module_agent: false,
            fb_title: String::new(),
            fb_category: "ux".into(),
            fb_severity: "medium".into(),
            fb_body: String::new(),
            fb_scenario: String::new(),
            fb_result: String::new(),
            fb_github: true,
            fb_dir: None,
            fb_diag_meta: None,
            chat_md_cache: CommonMarkCache::default(),
            update_offer: load_update_offer(),
            update_status: String::new(),
            model_infos: Vec::new(),
            providers: Vec::new(),
            provider_id: String::new(),
            provider_preset: "openai".into(),
            provider_endpoint: "https://api.openai.com/v1".into(),
            provider_secret_name: "openai_api_key".into(),
            provider_secret_value: String::new(),
            provider_enabled: true,
            provider_test_msg: String::new(),
            agent_model_id: default_model,
            model_updates_msg,
            download_status: String::new(),
            model_download: None,
            model_download_restart: None,
            ask_reply_target: None,
            chat_refocus: false,
            decl_panels: HashMap::new(),
            decl_md_cache: CommonMarkCache::default(),
            image_studio: image_studio::ImageStudioState::default(),
            image_generating: None,
            models_catalog_tab: models_page::ModelCatalogTab::Llm,
            hf_download_url: String::new(),
            hf_download_name: String::new(),
            hf_download_status: String::new(),
            show_go_to_palette: false,
            agent_join_room_on_create: false,
            room_turn_pending_text: None,
            room_members_pane_open: false,
            canvas_panel: chat_canvas::CanvasPanelState::default(),
            roster_edit_drafts: HashMap::new(),
        }
    }

    fn blocked_ask_ids(&self) -> Vec<String> {
        let Some(sid) = self.active_session.as_deref() else {
            return Vec::new();
        };
        self.agents
            .iter()
            .filter(|a| a.session_id.as_deref() == Some(sid) && a.state == AgentState::Blocked)
            .map(|a| a.agent_id.clone())
            .collect()
    }

    fn pending_ask_queue(&self) -> Vec<String> {
        pending_ask_ids(&self.chat, &self.blocked_ask_ids())
    }

    fn blocked_ask_agent(&self) -> Option<&AgentInfo> {
        let queue = self.pending_ask_queue();
        let chosen = self
            .ask_reply_target
            .as_ref()
            .filter(|t| queue.iter().any(|x| x == *t))
            .cloned()
            .or_else(|| queue.first().cloned())?;
        self.agents.iter().find(|a| a.agent_id == chosen)
    }

    fn set_canvas_open_local(&mut self, session_id: &str, open: bool) {
        if let Some(s) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            s.canvas_open = open;
        }
    }

    /// Ouvre le panneau canvas (optimiste côté UI, puis bus).
    fn open_canvas_face(&mut self, session_id: &str) {
        let already = self
            .sessions
            .iter()
            .find(|s| s.id == session_id)
            .map(|s| s.canvas_open)
            .unwrap_or(false);
        if already {
            return;
        }
        self.set_canvas_open_local(session_id, true);
        let _ = self.cmd_tx.send(Cmd::CanvasSetOpen {
            session_id: session_id.to_string(),
            open: true,
        });
    }

    /// Tue les agents bloqués sur user.ask pour libérer la session (ex. image kit coincé).
    fn break_stuck_session_agents(&mut self, session_id: &str) {
        let t = i18n::strings(&self.prefs.language);
        let blocked: Vec<(String, String)> = self
            .agents
            .iter()
            .filter(|a| {
                a.session_id.as_deref() == Some(session_id) && a.state == AgentState::Blocked
            })
            .map(|a| (a.agent_id.clone(), a.directive.clone()))
            .collect();
        let blocked_ids: Vec<String> = blocked.iter().map(|(id, _)| id.clone()).collect();
        for (agent_id, title) in blocked {
            self.chat.push(ChatLine {
                role: "user".into(),
                text: t.agent_unblocked.into(),
                attachments: vec![ChatAttachment::AgentRef {
                    agent_id: agent_id.clone(),
                    title,
                    origin: "ask-reply".into(),
                }],
                speaker_id: None,
            });
            let _ = self.cmd_tx.send(Cmd::AgentKill { id: agent_id });
        }
        if self
            .ask_reply_target
            .as_ref()
            .is_some_and(|t| blocked_ids.iter().any(|id| id == t))
        {
            self.ask_reply_target = None;
        }
    }

    fn task_looks_like_module_authoring(task: &str) -> bool {
        let lower = task.to_lowercase();
        lower.contains("module")
            || lower.contains("scaffold")
            || lower.contains("aospkg")
            || lower.contains("cohortmod")
            || lower.contains("ext-rt")
    }

    fn arm_pending_module_agent(&mut self, task: &str) {
        if !Self::task_looks_like_module_authoring(task) {
            return;
        }
        self.pending_module_agent = true;
        self.pending_module_baseline = self
            .installed_modules
            .iter()
            .map(|m| m.name.clone())
            .collect();
    }

    fn launch_module_author_agent(&mut self) {
        const TASK: &str = "Crée un module script nommé cohortmod. Étapes obligatoires dans cet ordre : \
1) module.scaffold avec name=cohortmod, kind=script, description='module cohorte ping' (pas de rust, pas de compile). \
2) module.package avec name=cohortmod. \
3) module.install avec source_dir égal au package_dir renvoyé par package. \
Puis module.list pour confirmer que cohortmod est installé. Termine avec goal.complete.";
        self.arm_pending_module_agent(TASK);
        let t = i18n::strings(&self.prefs.language);
        let tools = vec![
            "module.scaffold".into(),
            "module.package".into(),
            "module.install".into(),
            "module.list".into(),
            "module.describe".into(),
            "user.ask".into(),
            "plan.update".into(),
        ];
        let _ = self.cmd_tx.send(Cmd::AgentCreate {
            display_name: aos_agent::persist::agent_title(TASK),
            task: TASK.to_string(),
            system_prompt: None,
            skills: vec!["planner".into()],
            tools,
            mcp_servers: vec![],
            documents: vec![],
            optimize_prompt: false,
            max_steps: self.prefs.default_max_steps.max(20),
            timeout_secs: self.prefs.default_timeout_secs.max(180),
            model_id: if self.agent_model_id.is_empty() {
                None
            } else {
                Some(self.agent_model_id.clone())
            },
            session_id: self.active_session.clone(),
            origin: "form".into(),
            join_active_room: false,
            library: false,
        });
        self.tab = Tab::Agents;
        self.status = t.scen_module_agent_launched.into();
    }

    fn send_ask_reply(&mut self, session_id: String, agent_id: String, title: String, text: String) {
        self.chat.push(ChatLine {
            role: "user".into(),
            text: text.clone(),
            attachments: vec![ChatAttachment::AgentRef {
                agent_id: agent_id.clone(),
                title: title.clone(),
                origin: "ask-reply".into(),
            }],
            speaker_id: None,
        });
        let _ = self.cmd_tx.send(Cmd::SessionAppend {
            session_id,
            role: "user".into(),
            content: text.clone(),
            attachments: vec![ChatAttachment::AgentRef {
                agent_id: agent_id.clone(),
                title,
                origin: "ask-reply".into(),
            }],
        });
        let _ = self.cmd_tx.send(Cmd::AgentSteer {
            id: agent_id.clone(),
            text,
        });
        if self.ask_reply_target.as_deref() == Some(agent_id.as_str()) {
            self.ask_reply_target = None;
        }
        self.status = "réponse envoyée à l'agent".into();
    }

    fn send_chat(&mut self) {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return;
        }
        self.input.clear();
        self.chat_refocus = true;
        if text.starts_with('/') {
            self.handle_slash(&text);
            return;
        }
        let Some(session_id) = self.active_session.clone() else {
            self.chat.push(ChatLine::plain(
                "système",
                "aucune session — créez-en une dans le panneau Sessions",
            ));
            return;
        };
        let explicit_canvas = chat_canvas::chat_should_open_canvas_face(&text);
        if explicit_canvas {
            self.break_stuck_session_agents(&session_id);
            self.open_canvas_face(&session_id);
        }
        if !explicit_canvas {
            if let Some((agent_id, title)) = self
                .blocked_ask_agent()
                .map(|ag| (ag.agent_id.clone(), ag.directive.clone()))
            {
                self.send_ask_reply(session_id, agent_id, title, text);
                return;
            }
        }
        if self.chat_pending {
            self.chat.push(ChatLine::plain("user", text));
            self.chat.push(ChatLine::plain(
                "système",
                "réponse précédente encore en cours — patientez.",
            ));
            return;
        }
        if let Some(spoken) = chat_tts_request(&text) {
            self.chat.push(ChatLine::plain("user", text.clone()));
            if let Some(sid) = self.active_session.clone() {
                let _ = self.cmd_tx.send(Cmd::SessionAppend {
                    session_id: sid,
                    role: "user".into(),
                    content: text,
                    attachments: vec![],
                });
            }
            if spoken.trim().is_empty() {
                self.chat.push(ChatLine::plain(
                    "système",
                    "usage : /speak <texte> — indiquez le texte à lire.",
                ));
                return;
            }
            self.open_tts_card(&spoken);
            return;
        }
        self.chat.push(ChatLine::plain("user", text.clone()));
        if chat_room::session_is_room(chat_room::active_session_meta(
            &self.sessions,
            self.active_session.as_deref(),
        )) {
            let Some(session_id) = self.active_session.clone() else {
                return;
            };
            self.streaming.clear();
            self.chat_pending = true;
            self.chat_inference_id = None;
            self.room_turn_pending_text = Some(text.clone());
            let _ = self.cmd_tx.send(Cmd::RoomTurn {
                session_id,
                content: text,
            });
            self.mark_onboarding_chat_sent();
            self.scen_chat = true;
            return;
        }
        let history: Vec<(String, String)> = self
            .chat
            .iter()
            .filter(|l| l.role == "user" || l.role == "vous" || l.role == "assistant")
            .map(|l| {
                (
                    if l.role == "vous" || l.role == "user" {
                        "user".into()
                    } else {
                        "assistant".into()
                    },
                    l.text.clone(),
                )
            })
            .collect();
        self.streaming.clear();
        self.chat_pending = true;
        self.chat_inference_id = None;
        self.status = "assistant : génération…".into();
        let model_id = self
            .sessions
            .iter()
            .find(|s| s.id == session_id)
            .and_then(|s| s.model_id.clone());
        let canvas_open = self
            .sessions
            .iter()
            .find(|s| s.id == session_id)
            .map(|s| s.canvas_open)
            .unwrap_or(false)
            || (self.active_session.as_deref() == Some(session_id.as_str())
                && (!self.canvas_panel.ops.is_empty() || self.canvas_panel.next_seq > 1));
        let canvas_aspect = self
            .sessions
            .iter()
            .find(|s| s.id == session_id)
            .map(|s| s.canvas_aspect)
            .unwrap_or_default();
        let _ = self.cmd_tx.send(Cmd::Chat {
            session_id,
            history,
            user_text: text,
            model_id,
            auto_remember: self.prefs.auto_remember_chat,
            max_steps: chat_agent_max_steps(self.prefs.default_max_steps),
            routing: self.prefs.routing.clone(),
            language: self.prefs.language.clone(),
            canvas_open,
            canvas_aspect,
        });
        self.mark_onboarding_chat_sent();
        self.scen_chat = true;
    }

    fn apply_onboarding_prefs(&mut self) {
        self.prefs.language = self.onboarding.language.clone();
        self.prefs.routing = self.onboarding.routing.clone();
        self.prefs.trust_default = self.onboarding.trust_default.clone();
        save_preferences(&self.prefs);
        let _ = self.cmd_tx.send(Cmd::SetRouting {
            mode: self.prefs.routing.clone(),
        });
    }

    fn complete_onboarding(&mut self, status: String) {
        self.apply_onboarding_prefs();
        self.onboarding.completed = true;
        self.onboarding.tutorial_step = onboarding::TUTORIAL_LAST_STEP;
        save_onboarding(&self.onboarding);
        self.show_onboarding = false;
        self.tab = Tab::Chat;
        self.status = status;
    }

    fn mark_onboarding_chat_sent(&mut self) {
        if self.show_onboarding && self.onboarding.tutorial_step == 1 {
            self.onboarding.chat_sent = true;
            save_onboarding(&self.onboarding);
        }
    }

    fn mark_onboarding_chat_done(&mut self) {
        if !self.show_onboarding || self.onboarding.tutorial_step != 1 || !self.onboarding.chat_sent {
            return;
        }
        self.onboarding.first_chat_done = true;
        save_onboarding(&self.onboarding);
        if onboarding::chat_step_can_advance(self.onboarding.chat_sent, self.onboarding.first_chat_done)
        {
            self.onboarding.tutorial_step = 2;
            save_onboarding(&self.onboarding);
            if let Some(id) = self.active_session.as_deref() {
                let holder = format!("session:{id}");
                self.caps_holder = holder.clone();
                let _ = self.cmd_tx.send(Cmd::CapList { holder });
            }
        }
    }

    fn routing_human_label<'a>(&self, t: &'a i18n::UiStrings) -> &'a str {
        i18n::routing_label(t, &self.prefs.routing)
    }

    fn ui_onboarding_allowance_recap(&self, ui: &mut egui::Ui, t: &i18n::UiStrings) {
        ui.label(t.onboard_allowance_intro);
        if self.prefs.network_online {
            ui.label(t.onboard_allowance_network_on);
        } else {
            ui.label(t.onboard_allowance_network_off);
        }
        ui.label(
            t.onboard_allowance_routing
                .replace("{routing}", self.routing_human_label(t)),
        );
        ui.label(
            t.onboard_allowance_trust
                .replace("{trust}", &self.prefs.trust_default),
        );
        if self.prefs.auto_remember_chat {
            ui.label(t.onboard_allowance_memory_on);
        } else {
            ui.label(t.onboard_allowance_memory_off);
        }
        ui.label(
            t.onboard_allowance_caps
                .replace("{n}", &self.caps.len().to_string()),
        );
        ui.label(t.onboard_allowance_no_agent_tools);
        ui.add_space(8.0);
        ui.weak(t.onboard_allowance_scenarios);
    }

    fn handle_slash(&mut self, text: &str) {
        self.chat.push(ChatLine::plain("user", text));
        let mut parts = text.splitn(2, char::is_whitespace);
        let cmd = parts.next().unwrap_or(text);
        let rest = parts.next().unwrap_or("").trim();
        match cmd {
            "/commands" => {
                let mut out = String::from("Commandes chat :\n");
                for (c, d) in SLASH_COMMANDS {
                    out.push_str(&format!("  {c} — {d}\n"));
                }
                self.chat.push(ChatLine::plain("système", out));
            }
            "/help" => {
                self.status = "interrogation des services…".into();
                let _ = self.cmd_tx.send(Cmd::Help);
            }
            "/notes" => {
                let _ = self.cmd_tx.send(Cmd::NotesList);
                self.tab = Tab::Notes;
            }
            "/notenew" => {
                let (title, content) = match rest.split_once('|') {
                    Some((t, c)) => (t.trim().to_string(), c.trim().to_string()),
                    None => {
                        self.chat.push(ChatLine::plain(
                            "système",
                            "usage : /notenew <titre> | <contenu>",
                        ));
                        return;
                    }
                };
                if title.is_empty() || content.is_empty() {
                    self.chat.push(ChatLine::plain(
                        "système",
                        "usage : /notenew <titre> | <contenu>",
                    ));
                    return;
                }
                let _ = self.cmd_tx.send(Cmd::NotesCreate { title, content });
                self.tab = Tab::Notes;
            }
            "/notesearch" => {
                if rest.is_empty() {
                    self.chat.push(ChatLine::plain(
                        "système",
                        "usage : /notesearch <requête>",
                    ));
                    return;
                }
                let _ = self.cmd_tx.send(Cmd::NotesSearch {
                    query: rest.to_string(),
                });
                self.tab = Tab::Notes;
            }
            "/agent" => {
                if rest.is_empty() {
                    self.chat.push(ChatLine::plain("système", "usage : /agent <tâche>"));
                    return;
                }
                let Some(session_id) = self.active_session.clone() else {
                    self.chat.push(ChatLine::plain(
                        "système",
                        "aucune session — créez-en une avant /agent",
                    ));
                    return;
                };
                // Persister la ligne slash (sinon perdue au reload).
                let _ = self.cmd_tx.send(Cmd::SessionAppend {
                    session_id: session_id.clone(),
                    role: "user".into(),
                    content: text.to_string(),
                    attachments: vec![],
                });
                self.pending_note_agent = rest.to_lowercase().contains("note");
                self.arm_pending_module_agent(rest);
                let (skills, tools) = chat_agent_kit(rest);
                let _ = self.cmd_tx.send(Cmd::AgentCreate {
                    display_name: aos_agent::persist::agent_title(rest),
                    task: rest.to_string(),
                    system_prompt: None,
                    skills,
                    tools,
                    mcp_servers: vec![],
                    documents: vec![],
                    optimize_prompt: false,
                    max_steps: chat_agent_max_steps(self.prefs.default_max_steps),
                    timeout_secs: self.prefs.default_timeout_secs,
                    model_id: None,
                    session_id: Some(session_id),
                    origin: "slash".into(),
                    join_active_room: false,
                    library: false,
                });
                // Rester dans le chat — carte via Evt::AgentSpawned
            }
            "/audit" => {
                let n = rest.parse().unwrap_or(20);
                let _ = self.cmd_tx.send(Cmd::Audit { last: n });
                self.tab = Tab::Audit;
            }
            "/kill" => {
                if rest.is_empty() {
                    self.chat
                        .push(ChatLine::plain("système", "usage : /kill <id>"));
                    return;
                }
                let _ = self.cmd_tx.send(Cmd::AgentKill {
                    id: rest.to_string(),
                });
            }
            "/pause" => {
                if rest.is_empty() {
                    self.chat
                        .push(ChatLine::plain("système", "usage : /pause <id>"));
                    return;
                }
                let _ = self.cmd_tx.send(Cmd::AgentPause {
                    id: rest.to_string(),
                });
            }
            "/image" => {
                if rest.is_empty() {
                    self.chat
                        .push(ChatLine::plain("système", "usage : /image <prompt>"));
                    return;
                }
                self.status = "image : génération…".into();
                if let Some(sid) = self.active_session.clone() {
                    let _ = self.cmd_tx.send(Cmd::SessionAppend {
                        session_id: sid,
                        role: "user".into(),
                        content: text.to_string(),
                        attachments: vec![],
                    });
                }
                let enrich = image_prompt::default_enrich_prompt(
                    self.prefs.default_image_model.as_deref(),
                );
                let _ = self.cmd_tx.send(Cmd::MediaImage {
                    prompt: rest.to_string(),
                    model_id: self.prefs.default_image_model.clone(),
                    options: image_studio::image_options_for_model(
                        self.prefs.default_image_model.as_deref(),
                        Some("balanced"),
                    ),
                    enrich_prompt: enrich,
                    enhance_prompt_chat: false,
                    generation_prompt: None,
                    composition_blocks: Vec::new(),
                });
            }
            "/speak" => {
                if rest.is_empty() {
                    self.chat
                        .push(ChatLine::plain("système", "usage : /speak <texte>"));
                    return;
                }
                self.open_tts_card(rest);
            }
            "/canvas" => {
                let Some(sid) = self.active_session.clone() else {
                    self.chat.push(ChatLine::plain(
                        "système",
                        "aucune session — créez-en une d'abord",
                    ));
                    return;
                };
                let open = chat_room::active_session_meta(&self.sessions, Some(sid.as_str()))
                    .map(|m| m.canvas_open)
                    .unwrap_or(false);
                let _ = self.cmd_tx.send(Cmd::CanvasSetOpen {
                    session_id: sid,
                    open: !open,
                });
            }
            _ => {
                self.chat.push(ChatLine::plain(
                    "système",
                    format!("commande inconnue : {cmd} — tapez /commands"),
                ));
            }
        }
    }

    fn open_tts_card(&mut self, spoken: &str) {
        let att = ChatAttachment::TtsDraft {
            text: spoken.to_string(),
            model_id: self.prefs.default_audio_model.clone(),
            options: aos_proto::MediaAudioOptions::default(),
        };
        self.chat.push(ChatLine {
            role: "assistant".into(),
            text: spoken.to_string(),
            attachments: vec![att.clone()],
            speaker_id: None,
        });
        if let Some(sid) = self.active_session.clone() {
            let _ = self.cmd_tx.send(Cmd::SessionAppend {
                session_id: sid,
                role: "assistant".into(),
                content: spoken.to_string(),
                attachments: vec![att],
            });
        }
        self.status = i18n::strings(&self.prefs.language).tts_card_blurb.into();
    }

    fn on_tab_open(&mut self, tab: Tab) {
        if tab == Tab::Feedback && self.tab != Tab::Feedback {
            self.fb_result.clear();
        }
        self.tab = tab.clone();
        match tab {
            Tab::Providers => {
                let _ = self.cmd_tx.send(Cmd::ProviderList);
            }
            Tab::Audit => {
                let _ = self.cmd_tx.send(Cmd::Audit { last: 40 });
            }
            Tab::Caps if !self.caps_holder.is_empty() => {
                let _ = self.cmd_tx.send(Cmd::CapList {
                    holder: self.caps_holder.clone(),
                });
            }
            Tab::Notes => {
                let _ = self.cmd_tx.send(Cmd::NotesList);
            }
            Tab::Memory => {
                let _ = self.cmd_tx.send(Cmd::MemList {
                    include_superseded: self.mem_show_superseded,
                });
            }
            Tab::Tasks => {
                let _ = self.cmd_tx.send(Cmd::TasksList);
            }
            Tab::Settings => {
                let _ = self.cmd_tx.send(Cmd::ScheduleList);
                let _ = self.cmd_tx.send(Cmd::CatalogueRefresh);
                let _ = self.cmd_tx.send(Cmd::ModuleList);
            }
            _ => {}
        }
    }

    fn status_model_name(&self) -> String {
        if let Some(metrics) = &self.metrics {
            if let Some(m) = metrics.models.first() {
                return m.model_id.clone();
            }
        }
        self.prefs
            .default_agent_model
            .clone()
            .unwrap_or_else(|| "default".into())
    }

    fn ui_nav_rail(&mut self, ui: &mut egui::Ui, t: &i18n::UiStrings) {
        let primary = [
            (Tab::Chat, t.tab_chat, t.tab_hint_chat),
            (Tab::Agents, t.tab_agents, t.tab_hint_agents),
            (Tab::Image, t.tab_create, t.tab_hint_image),
            (Tab::Memory, t.tab_memory, t.tab_hint_memory),
        ];
        for (tab, label, hint) in primary {
            if ui
                .selectable_label(self.tab == tab, label)
                .on_hover_text(hint)
                .clicked()
            {
                self.on_tab_open(tab);
            }
        }

        ui.separator();
        let more_open = nav::is_overflow_tab(&self.tab);
        egui::CollapsingHeader::new(t.nav_more)
            .default_open(more_open)
            .show(ui, |ui| {
                for (tab, label, hint) in [
                    (Tab::Notes, t.tab_notes, t.tab_hint_notes),
                    (Tab::Tasks, t.tab_tasks, t.tab_hint_tasks),
                    (Tab::Models, t.tab_models, t.tab_hint_models),
                    (Tab::Settings, t.tab_settings, t.tab_hint_settings),
                    (Tab::Caps, t.tab_caps, t.tab_hint_caps),
                    (Tab::Audit, t.tab_audit, t.tab_hint_audit),
                    (Tab::Providers, t.tab_providers, t.tab_hint_providers),
                ] {
                    if ui
                        .selectable_label(self.tab == tab, label)
                        .on_hover_text(hint)
                        .clicked()
                    {
                        self.on_tab_open(tab);
                    }
                }
                ui.weak("— tester —");
                for (tab, label, hint) in [
                    (Tab::Scenarios, t.tab_scenarios, t.tab_hint_scenarios),
                    (Tab::Feedback, t.tab_feedback, t.tab_hint_feedback),
                ] {
                    if ui
                        .selectable_label(self.tab == tab, label)
                        .on_hover_text(hint)
                        .clicked()
                    {
                        self.on_tab_open(tab);
                    }
                }
                let decl_mods: Vec<(String, String)> = self
                    .installed_modules
                    .iter()
                    .filter(|m| {
                        aos_proto::decl_ui::sidebar_decl_ui_module(
                            &m.name,
                            m.ui_mode.as_deref(),
                        )
                    })
                    .map(|m| {
                        (
                            m.name.clone(),
                            m.ui_title
                                .clone()
                                .unwrap_or_else(|| m.name.clone()),
                        )
                    })
                    .collect();
                if !decl_mods.is_empty() {
                    ui.separator();
                    ui.weak("Modules");
                    for (name, label) in decl_mods {
                        let tab = Tab::Module(name.clone());
                        if ui.selectable_label(self.tab == tab, &label).clicked() {
                            self.on_tab_open(tab);
                            let _ = self.cmd_tx.send(Cmd::ModuleUiLoad { module: name });
                        }
                    }
                }
            });
    }

    fn ui_status_bar(&mut self, ui: &mut egui::Ui, t: &i18n::UiStrings) {
        ui.horizontal(|ui| {
            let net_label = if self.network_online {
                t.status_network_on
            } else {
                t.status_network_off
            };
            if ui.small_button(net_label).clicked() {
                self.network_online = !self.network_online;
                self.prefs.network_online = self.network_online;
                save_preferences(&self.prefs);
                let _ = self.cmd_tx.send(Cmd::NetSetMode {
                    online: self.network_online,
                });
            }
            ui.separator();
            if ui
                .small_button(format!("{}: {}", t.status_model_label, self.status_model_name()))
                .clicked()
            {
                self.on_tab_open(Tab::Models);
            }
            ui.separator();
            let caps_text = if self.caps.is_empty() {
                format!("{}: —", t.status_caps_label)
            } else {
                format!(
                    "{}: {}",
                    t.status_caps_label,
                    t.status_caps_count.replace("{n}", &self.caps.len().to_string())
                )
            };
            if ui.small_button(caps_text).clicked() {
                self.on_tab_open(Tab::Caps);
            }
            ui.separator();
            if let Some(pending_ver) = load_pending_update_version() {
                ui.label(t.status_update_pending.replace("{version}", &pending_ver));
            } else if let Some(offer) = &self.update_offer {
                ui.label(t.update_available.replace("{}", &offer.version));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let lang_btn = if self.prefs.language.eq_ignore_ascii_case("en") {
                    t.status_lang_en
                } else {
                    t.status_lang_fr
                };
                if ui.small_button(lang_btn).clicked() {
                    self.prefs.language = if self.prefs.language.eq_ignore_ascii_case("en") {
                        "fr".into()
                    } else {
                        "en".into()
                    };
                    save_preferences(&self.prefs);
                }
                ui.weak(format!("v{}", self.version));
            });
        });
    }

    fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            if i.modifiers.command || i.modifiers.ctrl {
                if i.key_pressed(egui::Key::K) {
                    self.show_go_to_palette = true;
                }
                for (idx, key) in [
                    egui::Key::Num1,
                    egui::Key::Num2,
                    egui::Key::Num3,
                    egui::Key::Num4,
                ]
                .iter()
                .enumerate()
                {
                    if i.key_pressed(*key) {
                        if let Some(tab) = nav::tab_from_primary_index(idx) {
                            self.on_tab_open(tab);
                        }
                    }
                }
            }
        });
    }

    fn ui_go_to_palette(&mut self, ctx: &egui::Context, t: &i18n::UiStrings) {
        if !self.show_go_to_palette {
            return;
        }
        egui::Window::new(t.go_to_title)
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 48.0])
            .show(ctx, |ui| {
                ui.weak(t.go_to_hint);
                ui.separator();
                let destinations: [(&str, Tab); 13] = [
                    (t.tab_chat, Tab::Chat),
                    (t.tab_agents, Tab::Agents),
                    (t.tab_create, Tab::Image),
                    (t.tab_memory, Tab::Memory),
                    (t.tab_notes, Tab::Notes),
                    (t.tab_tasks, Tab::Tasks),
                    (t.tab_models, Tab::Models),
                    (t.tab_settings, Tab::Settings),
                    (t.tab_caps, Tab::Caps),
                    (t.tab_audit, Tab::Audit),
                    (t.tab_providers, Tab::Providers),
                    (t.tab_scenarios, Tab::Scenarios),
                    (t.tab_feedback, Tab::Feedback),
                ];
                for (label, tab) in destinations {
                    if ui.button(label).clicked() {
                        self.on_tab_open(tab);
                        self.show_go_to_palette = false;
                    }
                }
                if ui.button(t.skip).clicked() {
                    self.show_go_to_palette = false;
                }
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.show_go_to_palette = false;
        }
    }
}

impl eframe::App for UiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::apply_theme(ctx, &self.prefs.theme);
        theme::apply_ui_scale(ctx, self.prefs.ui_scale_percent);
        self.handle_keyboard_shortcuts(ctx);
        while let Ok(ev) = self.evt_rx.try_recv() {
            match ev {
                Evt::Delta(t) => self.streaming.push_str(&t),
                Evt::Done {
                    text,
                    session_id,
                    attachments,
                } => {
                    if self.active_session.as_deref() == Some(session_id.as_str()) && !text.is_empty()
                    {
                        self.chat.push(ChatLine {
                            role: "assistant".into(),
                            text,
                            attachments,
                            speaker_id: None,
                        });
                    }
                    self.streaming.clear();
                    self.chat_pending = false;
                    self.chat_inference_id = None;
                    if self.status.starts_with("assistant :") {
                        self.status.clear();
                    }
                    self.mark_onboarding_chat_done();
                }
                Evt::Error(m) => {
                    if m.contains("media.image") || m.starts_with("Image:") {
                        self.image_generating = None;
                    }
                    self.status = m.clone();
                    self.chat.push(ChatLine::plain("système", m));
                    self.streaming.clear();
                    self.chat_pending = false;
                    self.chat_inference_id = None;
                    self.room_turn_pending_text = None;
                }
                Evt::Status(m) => {
                    if let Some(id) = m.strip_prefix("model removed:") {
                        let id = id.trim().to_string();
                        self.model_download_restart = Some(id.clone());
                        let t = i18n::strings(&self.prefs.language);
                        self.download_status = t.models_removed.to_string();
                    }
                    self.status = m;
                }
                Evt::ModelDownloadStarted { model_id } => {
                    self.model_download_restart = None;
                    self.model_download = Some(ModelDownloadUiState {
                        model_id: model_id.clone(),
                        percent: 0,
                        done_bytes: 0,
                        total_bytes: 0,
                    });
                    let t = i18n::strings(&self.prefs.language);
                    self.download_status =
                        t.models_downloading.replace("{}", &model_id);
                }
                Evt::ModelDownloadProgress {
                    model_id,
                    done_bytes,
                    total_bytes,
                    percent,
                } => {
                    self.model_download = Some(ModelDownloadUiState {
                        model_id: model_id.clone(),
                        percent,
                        done_bytes,
                        total_bytes,
                    });
                    let t = i18n::strings(&self.prefs.language);
                    self.download_status = format!(
                        "{} {percent}%",
                        t.models_downloading.replace("{}", &model_id)
                    );
                }
                Evt::ModelDownloadFinished { model_id } => {
                    self.model_download = None;
                    self.model_download_restart = Some(model_id.clone());
                    let t = i18n::strings(&self.prefs.language);
                    self.download_status =
                        t.models_download_done.replace("{}", &model_id);
                    self.model_updates_msg.clear();
                    self.image_studio.on_download_finished(&model_id);
                }
                Evt::ModelDownloadFailed { model_id, error } => {
                    self.model_download = None;
                    self.model_download_restart = None;
                    let t = i18n::strings(&self.prefs.language);
                    self.download_status = format!(
                        "{}: {error}",
                        t.models_download_failed.replace("{}", &model_id)
                    );
                }
                Evt::MemExtracted { n } => {
                    let t = i18n::strings(&self.prefs.language);
                    self.status = t.memory_extracted_toast.replace("{}", &n.to_string());
                    // Refresh memory list so the chat badge appears.
                    let _ = self.cmd_tx.send(Cmd::MemList {
                        include_superseded: self.mem_show_superseded,
                    });
                }
                Evt::ChatSystem(m) => self.chat.push(ChatLine::plain("système", m)),
                Evt::Metrics(m) => self.metrics = Some(m),
                Evt::AgentSpawned {
                    session_id,
                    agent_id,
                    title,
                    origin,
                    ack,
                } => {
                    self.arm_pending_module_agent(&title);
                    if self.active_session.as_deref() == Some(session_id.as_str()) {
                        self.chat.push(ChatLine {
                            role: "assistant".into(),
                            text: ack,
                            attachments: vec![ChatAttachment::AgentRef {
                                agent_id,
                                title,
                                origin,
                            }],
                            speaker_id: None,
                        });
                    } else {
                        self.status = format!("agent lancé : {agent_id}");
                    }
                }
                Evt::Agents(a) => {
                    if self.pending_note_agent
                        && a.iter().any(|ag| {
                            matches!(
                                ag.state,
                                AgentState::Done | AgentState::Failed | AgentState::Killed
                            )
                        })
                    {
                        let _ = self.cmd_tx.send(Cmd::NotesList);
                    }
                    if self.pending_module_agent
                        && a.iter().any(|ag| {
                            matches!(
                                ag.state,
                                AgentState::Done | AgentState::Failed | AgentState::Killed
                            )
                        })
                    {
                        let _ = self.cmd_tx.send(Cmd::ModuleList);
                    }
                    let seeding = self.agent_prev_states.is_empty();
                    for ag in &a {
                        let prev = self.agent_prev_states.get(&ag.agent_id).cloned();
                        let terminal = matches!(
                            ag.state,
                            AgentState::Done | AgentState::Failed | AgentState::Killed
                        );
                        let was_active = prev
                            .as_ref()
                            .map(|p| {
                                !matches!(
                                    p,
                                    AgentState::Done | AgentState::Failed | AgentState::Killed
                                )
                            })
                            .unwrap_or(false);
                        if terminal {
                            if let Some(sid) = &ag.session_id {
                                let on_this_session =
                                    self.active_session.as_deref() == Some(sid.as_str());
                                let already = self.chat.iter().any(|l| {
                                    l.attachments.iter().any(|a| {
                                        matches!(
                                            a,
                                            ChatAttachment::AgentRef {
                                                agent_id,
                                                origin,
                                                ..
                                            } if agent_id == &ag.agent_id
                                                && origin == "completion"
                                        )
                                    })
                                });
                                if on_this_session {
                                    let content = agent_completion_chat_text(ag);
                                    if already {
                                        if !ag.last_output.trim().is_empty() {
                                            if let Some(line) =
                                                self.chat.iter_mut().find(|l| {
                                                    l.attachments.iter().any(|a| {
                                                        matches!(
                                                            a,
                                                            ChatAttachment::AgentRef {
                                                                agent_id,
                                                                origin,
                                                                ..
                                                            } if agent_id == &ag.agent_id
                                                                && origin == "completion"
                                                        )
                                                    })
                                                })
                                            {
                                                if line.text != content {
                                                    line.text = content;
                                                }
                                            }
                                        }
                                    } else if !seeding {
                                        self.chat.push(ChatLine {
                                            role: "assistant".into(),
                                            text: content,
                                            attachments: vec![ChatAttachment::AgentRef {
                                                agent_id: ag.agent_id.clone(),
                                                title: ag.directive.clone(),
                                                origin: "completion".into(),
                                            }],
                                            speaker_id: None,
                                        });
                                    }
                                } else if !seeding
                                    && !on_this_session
                                    && !self.agent_notified.contains(&ag.agent_id)
                                    && was_active
                                {
                                    let summary = match ag.state {
                                        AgentState::Done => format!("{} terminé", ag.display_title()),
                                        AgentState::Failed => format!(
                                            "{} échoué — {}",
                                            ag.display_title(),
                                            ag.fail_reason.as_deref().unwrap_or("échec")
                                        ),
                                        AgentState::Killed => format!("{} arrêté", ag.display_title()),
                                        _ => format!("{} terminé", ag.display_title()),
                                    };
                                    self.agent_notified.insert(ag.agent_id.clone());
                                    self.agent_notices.push(AgentNotice {
                                        agent_id: ag.agent_id.clone(),
                                        session_id: sid.clone(),
                                        summary,
                                    });
                                }
                            }
                        }
                        if prev == Some(AgentState::Blocked) && ag.state != AgentState::Blocked {
                            if self.ask_reply_target.as_deref() == Some(ag.agent_id.as_str()) {
                                self.ask_reply_target = None;
                            }
                            if let Some(sid) = &ag.session_id {
                                let on_this_session =
                                    self.active_session.as_deref() == Some(sid.as_str());
                                if on_this_session
                                    && chat_has_open_ask(&self.chat, &ag.agent_id)
                                {
                                    let expired =
                                        ag.last_output.starts_with("Question expirée");
                                    let text = if expired {
                                        "**Question expirée** — l'agent continue sans réponse."
                                            .into()
                                    } else {
                                        "**Question close** — l'agent a repris.".into()
                                    };
                                    self.chat.push(ChatLine {
                                        role: "assistant".into(),
                                        text,
                                        attachments: vec![ChatAttachment::AgentRef {
                                            agent_id: ag.agent_id.clone(),
                                            title: ag.directive.clone(),
                                            origin: "ask-timeout".into(),
                                        }],
                                        speaker_id: None,
                                    });
                                }
                            }
                        }
                        if ag.state == AgentState::Blocked {
                            if let Some(sid) = &ag.session_id {
                                let on_this_session =
                                    self.active_session.as_deref() == Some(sid.as_str());
                                let already =
                                    chat_has_open_ask(&self.chat, &ag.agent_id);
                                if on_this_session
                                    && !already
                                    && !ag.last_output.trim().is_empty()
                                    && !ag.last_output.starts_with("Question expirée")
                                {
                                    let title = agent_display_title(ag);
                                    let body = format!(
                                        "**Question — {title}**\n\n{}",
                                        ag.last_output.trim()
                                    );
                                    self.chat.push(ChatLine {
                                        role: "assistant".into(),
                                        text: body,
                                        attachments: vec![ChatAttachment::AgentRef {
                                            agent_id: ag.agent_id.clone(),
                                            title: ag.directive.clone(),
                                            origin: "ask".into(),
                                        }],
                                        speaker_id: None,
                                    });
                                } else if !on_this_session
                                    && !self.agent_notified.contains(&ag.agent_id)
                                {
                                    self.agent_notified.insert(ag.agent_id.clone());
                                    self.agent_notices.push(AgentNotice {
                                        agent_id: ag.agent_id.clone(),
                                        session_id: sid.clone(),
                                        summary: format!(
                                            "{} pose une question",
                                            agent_display_title(ag)
                                        ),
                                    });
                                }
                            }
                        }
                        self.agent_prev_states
                            .insert(ag.agent_id.clone(), ag.state.clone());
                    }
                    self.agents = a;
                }
                Evt::Notes(s) => {
                    if self.pending_note_agent
                        && !s.is_empty()
                        && !s.contains("aucune note")
                        && s != self.notes_out
                    {
                        self.scen_note_agent = true;
                        self.pending_note_agent = false;
                    }
                    self.notes_out = s;
                    self.scen_note_human = true;
                }
                Evt::NotesListed(notes) => {
                    self.notes.apply_listed(notes);
                    self.scen_note_human = true;
                }
                Evt::NoteLoaded(detail) => {
                    self.notes.apply_loaded(detail);
                }
                Evt::NotesSearchHits(hits) => {
                    self.notes.apply_search_hits(hits);
                }
                Evt::NotesRelated(hits) => {
                    self.notes.apply_related(hits);
                }
                Evt::NotesSaved { path, slug, title } => {
                    self.notes.mark_saved(path, slug, title);
                    self.scen_note_human = true;
                }
                Evt::Audit(a) => {
                    self.audit = a;
                    self.scen_audit = true;
                }
                Evt::Caps { holder, caps } => {
                    self.caps_holder = holder;
                    self.caps = caps;
                }
                Evt::Schedules(s) => self.schedules = s,
                Evt::TasksListed(tasks) => {
                    let t = i18n::strings(&self.prefs.language);
                    self.tasks.apply_listed(tasks, t.tasks_count);
                }
                Evt::Confirms(c) => self.confirms = c,
                Evt::FeedbackOk(r) => {
                    let mut msg = format!(
                        "Enregistré localement : {}\nDossier : {}",
                        r.path, r.export_dir
                    );
                    match r.github_status.as_str() {
                        "created" | "api" | "gh" => {
                            if let Some(url) = &r.github_issue_url {
                                msg.push_str(&format!(
                                    "\nIssue GitHub #{} : {url}",
                                    r.github_issue_number
                                        .map(|n| n.to_string())
                                        .unwrap_or_else(|| "?".into())
                                ));
                                open_in_browser(url);
                            }
                        }
                        "skipped_security" => {
                            msg.push_str(
                                "\nCatégorie security : non publié (issue publique interdite). Conservez le dossier local.",
                            );
                        }
                        s if s == "form" || s.starts_with("form ") => {
                            if let Some(url) = &r.github_issue_url {
                                msg.push_str(
                                    "\nFormulaire GitHub ouvert — cliquez « Submit new issue » pour publier.",
                                );
                                open_in_browser(url);
                            }
                        }
                        "local_only" => {}
                        other => {
                            msg.push_str(&format!("\nGitHub : {other}"));
                            if let Some(url) = &r.github_issue_url {
                                open_in_browser(url);
                            }
                        }
                    }
                    self.fb_result = msg;
                    self.status = format!("feedback {}", r.id);
                    let export_raw = native_path(&r.export_dir);
                    let export = if export_raw.is_absolute() {
                        export_raw
                    } else {
                        aos_home().join(&export_raw)
                    };
                    self.fb_dir = export
                        .parent()
                        .filter(|p| !p.as_os_str().is_empty())
                        .map(|p| p.to_path_buf())
                        .or(Some(export));
                    self.reset_feedback_form();
                }
                Evt::FeedbackDraft(req) => {
                    self.fb_title = req.title;
                    self.fb_category = req.category;
                    self.fb_severity = req.severity;
                    self.fb_body = req.body;
                    self.fb_scenario = req.scenario.unwrap_or_default();
                    self.fb_github = req.publish_github
                        && !self.fb_category.eq_ignore_ascii_case("security");
                    self.fb_diag_meta = Some(req.meta);
                    self.tab = Tab::Feedback;
                }
                Evt::Sessions(list) => self.sessions = list,
                Evt::SessionLoaded { id, messages, meta } => {
                    let session_changed = self.active_session.as_deref() != Some(id.as_str());
                    self.active_session = Some(id.clone());
                    self.rename_buf = meta.title.clone();
                    if let Some(s) = self.sessions.iter_mut().find(|s| s.id == meta.id) {
                        *s = meta.clone();
                    }
                    if session_changed {
                        self.room_members_pane_open = false;
                        let mut chat = vec![ChatLine::plain(
                            "système",
                            format!("Session {id} — historique rechargé."),
                        )];
                        chat.extend(messages);
                        self.chat = chat;
                    } else {
                        self.chat = messages;
                    }
                    self.streaming.clear();
                    self.chat_pending = false;
                    self.chat_inference_id = None;
                    self.room_turn_pending_text = None;
                    if meta.canvas_open {
                        let _ = self.cmd_tx.send(Cmd::CanvasPoll {
                            session_id: id.clone(),
                            after_seq: None,
                        });
                    } else {
                        self.canvas_panel = chat_canvas::CanvasPanelState::default();
                    }
                }
                Evt::RoomTurnDone {
                    session_id,
                    agent_turns,
                    cancelled,
                } => {
                    if self.active_session.as_deref() == Some(session_id.as_str()) {
                        self.chat_pending = false;
                        self.chat_inference_id = None;
                        self.room_turn_pending_text = None;
                        let t = i18n::strings(&self.prefs.language);
                        self.status = if cancelled {
                            t.room_turn_cancelled.into()
                        } else {
                            t.room_turn_done
                                .replace("{n}", &agent_turns.to_string())
                        };
                    }
                }
                Evt::CanvasMeta(meta) => {
                    if let Some(s) = self.sessions.iter_mut().find(|s| s.id == meta.id) {
                        *s = meta.clone();
                    }
                }
                Evt::CanvasSnapshot {
                    session_id,
                    canvas_open,
                    next_seq,
                    ops,
                    pen,
                    delta,
                } => {
                    if self.active_session.as_deref() != Some(session_id.as_str()) {
                        // still update meta open flag
                    }
                    if let Some(s) = self.sessions.iter_mut().find(|s| s.id == session_id) {
                        s.canvas_open = canvas_open;
                    }
                    if self.active_session.as_deref() == Some(session_id.as_str()) {
                        let now = ctx.input(|i| i.time);
                        if delta {
                            self.canvas_panel.merge_delta(ops, next_seq, now);
                        } else {
                            self.canvas_panel.apply_snapshot(ops, next_seq, now);
                        }
                        self.canvas_panel.sync_pen(&pen);
                    }
                }
                Evt::CanvasExported { path, session_id } => {
                    if self.active_session.as_deref() == Some(session_id.as_str()) {
                        self.status = format!("Canvas → {path}");
                        self.chat.push(ChatLine {
                            role: "assistant".into(),
                            text: format!("Canvas exporté : {path}"),
                            attachments: vec![ChatAttachment::Image {
                                path,
                                prompt: "canvas export".into(),
                            }],
                            speaker_id: None,
                        });
                    }
                }
                Evt::MemHits(h) => self.mem_hits = h,
                Evt::SecretList { names, encrypted } => {
                    self.secret_names = names;
                    self.secret_vault_encrypted = encrypted;
                }
                Evt::WebResults(r) => self.web_results = r,
                Evt::BrowsePreview(t) => self.browse_preview = t,
                Evt::NetMode(online) => {
                    self.network_online = online;
                    self.prefs.network_online = online;
                    save_preferences(&self.prefs);
                },
                Evt::FileOk(msg) => {
                    self.status = msg.clone();
                    self.chat.push(ChatLine::plain("système", msg));
                }
                Evt::MediaImageEnriched { enriched } => {
                    self.image_studio.set_enriched_prompt(&enriched);
                    self.status = "Image: enhanced prompt ready, generating…".into();
                }
                Evt::MediaImageStarted {
                    enriching,
                    upscaling,
                    total_steps,
                } => {
                    self.image_generating = Some(image_studio::ImageGenUiState {
                        enriching,
                        upscaling,
                        step: 0,
                        total_steps,
                        elapsed_secs: 0,
                    });
                    if enriching {
                        self.status = "Image: rewriting prompt…".into();
                    } else {
                        self.status = format!("Image: generating ({total_steps} steps)…");
                    }
                }
                Evt::MediaImageProgress {
                    enriching,
                    upscaling,
                    step,
                    total_steps,
                    elapsed_secs,
                } => {
                    self.image_generating = Some(image_studio::ImageGenUiState {
                        enriching,
                        upscaling,
                        step,
                        total_steps,
                        elapsed_secs,
                    });
                    if enriching {
                        self.status =
                            format!("Image: rewriting prompt… ({elapsed_secs}s)");
                    } else if upscaling {
                        self.status = format!("Image: upscaling… ({elapsed_secs}s)");
                    } else if step > 0 && total_steps > 0 {
                        self.status = format!(
                            "Image: step {step}/{total_steps} ({elapsed_secs}s)"
                        );
                    } else {
                        self.status = format!(
                            "Image: generating ({total_steps} steps, {elapsed_secs}s)…"
                        );
                    }
                }
                Evt::MediaOk {
                    kind,
                    path,
                    bytes,
                    engine,
                    prompt,
                    generation_prompt,
                    composition_blocks,
                    model_id: _,
                } => {
                    self.image_generating = None;
                    self.status = format!("{kind} → {path} ({bytes} bytes, {engine})");
                    let att = if kind == "audio" {
                        ChatAttachment::Audio { path: path.clone() }
                    } else {
                        ChatAttachment::Image {
                            path: path.clone(),
                            prompt: prompt.clone(),
                        }
                    };
                    if kind != "audio" {
                        if prompt.is_empty() {
                            self.image_studio.preview = Some(path.clone());
                            // Upscale: try restore prompts/composition from sidecar.
                            self.image_studio.apply_history_for_path(&path);
                        } else {
                            self.image_studio.open_from_chat(
                                &prompt,
                                &path,
                                generation_prompt.as_deref(),
                            );
                            if !composition_blocks.is_empty() {
                                self.image_studio
                                    .set_composition_blocks(composition_blocks);
                            } else {
                                self.image_studio.apply_history_for_path(&path);
                            }
                        }
                        self.tab = Tab::Image;
                    }
                    let note = if engine == "stub" {
                        format!(
                            "{kind}: {path}\n(stub — pack média ou moteur sd.cpp/piper absent)"
                        )
                    } else {
                        format!("{kind}: {path} ({engine})")
                    };
                    self.chat.push(ChatLine {
                        role: "assistant".into(),
                        text: note.clone(),
                        attachments: vec![att.clone()],
                        speaker_id: None,
                    });
                    if let Some(sid) = self.active_session.clone() {
                        let _ = self.cmd_tx.send(Cmd::SessionAppend {
                            session_id: sid,
                            role: "assistant".into(),
                            content: note,
                            attachments: vec![att],
                        });
                    }
                }
                Evt::Skills(list) => self.skill_catalog = list,
                Evt::McpServers(list) => self.mcp_catalog = list,
                Evt::PromptOptimized(p) => {
                    self.agent_system_prompt = p;
                    self.status = "prompt système optimisé".into();
                }
                Evt::Models(list) => self.model_infos = list,
                Evt::Providers(list) => self.providers = list,
                Evt::ProviderTested {
                    ok,
                    message,
                    models,
                } => {
                    self.provider_test_msg = if ok {
                        format!("ok — {message}")
                    } else {
                        format!("fail — {message}")
                    };
                    if !models.is_empty() {
                        self.provider_test_msg
                            .push_str(&format!(" ({})", models.join(", ")));
                    }
                }
                Evt::AgentSpecLoaded { spec } => {
                    self.roster_edit_drafts.insert(
                        spec.agent_id.clone(),
                        RosterEditDraft {
                            display_name: spec
                                .display_name
                                .clone()
                                .unwrap_or_else(|| spec.roster_display_name().to_string()),
                            role: spec.goal.statement.clone(),
                            system_prompt: spec.system_prompt.clone().unwrap_or_default(),
                            skills: spec.skills.clone(),
                            tools: spec.tools.clone(),
                            mcp_servers: spec.mcp_servers.clone(),
                            model_id: spec.model_id.clone().unwrap_or_default(),
                        },
                    );
                }
                Evt::AgentRosterSaved => {
                    let t = i18n::strings(&self.prefs.language);
                    self.status = t.agents_edit_saved.into();
                }
                Evt::AgentTrace(t) => {
                    self.agent_traces.insert(t.agent_id.clone(), t);
                }
                Evt::InferStarted { inference_id } => {
                    self.chat_inference_id = Some(inference_id);
                }
                Evt::ChatCancelled => {
                    self.chat_pending = false;
                    self.chat_inference_id = None;
                    self.room_turn_pending_text = None;
                    if !self.streaming.is_empty() {
                        let partial = std::mem::take(&mut self.streaming);
                        self.chat.push(ChatLine::plain("assistant", partial));
                    }
                    let t = i18n::strings(&self.prefs.language);
                    self.status = t.chat_stopped.into();
                }
                Evt::Catalogue(c) => self.catalogue = Some(c),
                Evt::InstalledModules(list) => {
                    if self.pending_module_agent {
                        let new_mod = list.iter().any(|m| {
                            aos_proto::decl_ui::sidebar_decl_ui_module(
                                &m.name,
                                m.ui_mode.as_deref(),
                            ) && !self.pending_module_baseline.iter().any(|n| n == &m.name)
                        });
                        if new_mod {
                            self.scen_module_agent = true;
                            self.pending_module_agent = false;
                            self.pending_module_baseline.clear();
                        }
                    }
                    self.installed_modules = list;
                }
                Evt::ModuleInstalled(msg) => {
                    self.status = msg;
                    let _ = self.cmd_tx.send(Cmd::CatalogueRefresh);
                    let _ = self.cmd_tx.send(Cmd::ModuleList);
                }
                Evt::ModuleUninstalled(name) => {
                    self.status = format!("uninstalled {name}");
                    self.decl_panels.remove(&name);
                    if matches!(&self.tab, Tab::Module(m) if m == &name) {
                        self.tab = Tab::Settings;
                    }
                    let _ = self.cmd_tx.send(Cmd::ModuleList);
                }
                Evt::ModuleUiLoaded(resp) => {
                    let module = resp.module.clone();
                    let title = resp.document.title.clone();
                    let binds = {
                        let panel = self
                            .decl_panels
                            .entry(module.clone())
                            .or_insert_with(|| decl_ui::DeclUiPanelState::new(&module));
                        panel.set_document(resp.document);
                        decl_ui::ingest_tool_schemas(&resp.tools, &mut panel.tool_schemas);
                        panel.status = format!("loaded {title}");
                        panel.tools_to_bind()
                    };
                    for tool in binds {
                        let _ = self.cmd_tx.send(Cmd::ModuleUiBind {
                            module: module.clone(),
                            tool,
                        });
                    }
                }
                Evt::ModuleUiFailed { module, error } => {
                    let panel = self
                        .decl_panels
                        .entry(module.clone())
                        .or_insert_with(|| decl_ui::DeclUiPanelState::new(&module));
                    panel.set_error(error);
                }
                Evt::ModuleUiBind {
                    module,
                    tool,
                    result,
                    error,
                } => {
                    if let Some(panel) = self.decl_panels.get_mut(&module) {
                        panel.set_bind_result(&tool, result);
                        if let Some(e) = error {
                            panel.status = format!("{tool}: {e}");
                        }
                    }
                }
                Evt::ModuleUiInvokeDone {
                    module,
                    tool,
                    ok,
                    result,
                    error,
                } => {
                    if let Some(panel) = self.decl_panels.get_mut(&module) {
                        if ok {
                            // Keep invoke results in the bind cache so widgets bound
                            // to this tool can update immediately without full reload.
                            panel.set_bind_result(&tool, result);
                        }
                        panel.status = if ok {
                            format!("{tool} ok")
                        } else {
                            error.unwrap_or_else(|| format!("{tool} failed"))
                        };
                    }
                }
            }
        }

        let t = i18n::strings(&self.prefs.language);
        let onboard_t = if self.show_onboarding {
            i18n::strings(&self.onboarding.language)
        } else {
            t
        };

        if self.show_onboarding {
            let step = self.onboarding.tutorial_step;
            let anchor = if step == 1 {
                egui::Align2::CENTER_TOP
            } else {
                egui::Align2::CENTER_CENTER
            };
            egui::Window::new(onboard_t.tutorial_title)
                .collapsible(false)
                .resizable(true)
                .default_width(520.0)
                .anchor(anchor, if step == 1 { [0.0, 12.0] } else { [0.0, 0.0] })
                .show(ctx, |ui| {
                    ui.label(onboard_t.step_of.replace("{}", &(step + 1).to_string()));
                    ui.separator();
                    match step {
                        0 => {
                            ui.heading(onboard_t.welcome);
                            ui.label(onboard_t.preview_tagline);
                            ui.label(onboard_t.welcome_body1);
                            ui.add_space(8.0);
                            ui.label(onboard_t.language);
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut self.onboarding.language, "fr".into(), "Français");
                                ui.radio_value(&mut self.onboarding.language, "en".into(), "English");
                            });
                        }
                        1 => {
                            ui.heading(onboard_t.onboard_chat_heading);
                            ui.label(onboard_t.onboard_chat_body);
                            if self.onboarding.chat_sent && !self.onboarding.first_chat_done {
                                ui.weak(onboard_t.onboard_chat_waiting);
                            }
                        }
                        _ => {
                            ui.heading(onboard_t.onboard_allowance_heading);
                            self.ui_onboarding_allowance_recap(ui, &onboard_t);
                        }
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        if step > 0 && ui.button(onboard_t.prev).clicked() {
                            self.onboarding.tutorial_step = step - 1;
                            save_onboarding(&self.onboarding);
                            if step == 1 {
                                self.tab = Tab::Chat;
                            }
                        }
                        if step == 0 {
                            if ui.button(onboard_t.next).clicked() {
                                self.apply_onboarding_prefs();
                                self.onboarding.tutorial_step = 1;
                                save_onboarding(&self.onboarding);
                                self.tab = Tab::Chat;
                            }
                        } else if step == 1 {
                            let ready = onboarding::chat_step_can_advance(
                                self.onboarding.chat_sent,
                                self.onboarding.first_chat_done,
                            );
                            ui.add_enabled_ui(ready, |ui| {
                                if ui.button(onboard_t.next).clicked() {
                                    self.onboarding.tutorial_step = 2;
                                    save_onboarding(&self.onboarding);
                                }
                            });
                        } else if ui.button(onboard_t.finish_tutorial).clicked() {
                            self.complete_onboarding(onboard_t.tutorial_done_status.into());
                        }
                        if ui.button(onboard_t.skip).clicked() {
                            self.complete_onboarding(String::new());
                        }
                    });
                });
        }

        egui::TopBottomPanel::top("banner").show(ctx, |ui| {
            if !self.agent_notices.is_empty() {
                let notices = self.agent_notices.clone();
                let mut dismiss: Vec<String> = Vec::new();
                let mut open_sess: Option<String> = None;
                for n in &notices {
                    ui.horizontal(|ui| {
                        ui.colored_label(
                            egui::Color32::from_rgb(120, 180, 230),
                            &n.summary,
                        );
                        let sess_title = self
                            .sessions
                            .iter()
                            .find(|s| s.id == n.session_id)
                            .map(|s| s.title.clone())
                            .unwrap_or_else(|| n.session_id.clone());
                        ui.label(format!("— {sess_title}"));
                        if ui.button("Ouvrir").clicked() {
                            open_sess = Some(n.session_id.clone());
                            dismiss.push(n.agent_id.clone());
                        }
                        if ui.small_button("×").clicked() {
                            dismiss.push(n.agent_id.clone());
                        }
                    });
                }
                self.agent_notices
                    .retain(|x| !dismiss.contains(&x.agent_id));
                if let Some(id) = open_sess {
                    self.tab = Tab::Chat;
                    let _ = self.cmd_tx.send(Cmd::SessionSelect { id });
                }
            }
            ui.horizontal(|ui| {
                ui.weak(format!(
                    "Preview {} — {}",
                    self.version, t.preview_tagline
                ));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(t.report).clicked() {
                        self.on_tab_open(Tab::Feedback);
                    }
                    if ui.small_button(t.tutorial).clicked() {
                        self.onboarding.tutorial_step = 0;
                        self.onboarding.chat_sent = false;
                        self.onboarding.first_chat_done = false;
                        self.onboarding.completed = false;
                        self.show_onboarding = true;
                        save_onboarding(&self.onboarding);
                    }
                    if ui.small_button(t.troubleshooting).clicked() {
                        let _ = self.cmd_tx.send(Cmd::Troubleshoot);
                        self.on_tab_open(Tab::Feedback);
                        self.status = t.troubleshooting_status.into();
                    }
                });
            });
            if let Some(pending_ver) = load_pending_update_version() {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(120, 200, 140),
                        t.update_pending_restart.replace("{}", &pending_ver),
                    );
                    if let Some(offer) = self.update_offer.clone() {
                        if ui.button(t.update_notes).clicked() {
                            open_in_browser(&offer.html_url);
                        }
                    }
                });
            } else if let Some(offer) = self.update_offer.clone() {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(100, 180, 255),
                        t.update_available.replace("{}", &offer.version),
                    );
                    if ui.button(t.update_notes).clicked() {
                        open_in_browser(&offer.html_url);
                    }
                    if ui.button(t.update_download).clicked() {
                        let session = bin_aos_session();
                        match std::process::Command::new(&session)
                            .arg("--download-update")
                            .env("AOS_HOME", aos_home())
                            .status()
                        {
                            Ok(st) if st.success() => {
                                self.update_status =
                                    t.update_downloaded.replace("{}", &offer.version);
                            }
                            Ok(st) => {
                                self.update_status =
                                    t.update_fail_exit.replace("{}", &st.to_string());
                            }
                            Err(e) => {
                                self.update_status =
                                    t.update_fail.replace("{}", &e.to_string());
                            }
                        }
                    }
                });
            }
            if !self.model_updates_msg.is_empty() {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(180, 220, 120),
                        format!("Models: {}", self.model_updates_msg),
                    );
                    if ui.button("Open Models").clicked() {
                        self.tab = Tab::Models;
                    }
                });
            }
            if self.model_download_restart.is_some() {
                self.ui_model_download_restart(ui, ctx);
            }
            if !self.update_status.is_empty() {
                ui.label(&self.update_status);
            }
            if !self.status.is_empty() {
                ui.label(&self.status);
            }
            // Confirmations en attente
            if !self.confirms.is_empty() {
                ui.separator();
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    t.confirm_pending.replace("{n}", &self.confirms.len().to_string()),
                );
                for c in self.confirms.clone() {
                    ui.vertical(|ui| {
                        let rich = matches!(
                            c.action.as_str(),
                            "module.install"
                                | "module.uninstall"
                                | "module.compile"
                                | "skill.create"
                                | "cap.request"
                                | "media.generate"
                                | "media.image.generate"
                                | "media.audio.generate"
                        );
                        ui.label(
                            t.confirm_wants_action.replace("{action}", &c.action),
                        );
                        ui.monospace(format!("{} → {}", c.target, c.reason));
                        if rich {
                            ui.colored_label(
                                egui::Color32::from_rgb(220, 180, 80),
                                "Extension OS : revue des caps / manifeste requise",
                            );
                        }
                        ui.horizontal(|ui| {
                            if ui.button(t.confirm_grant).clicked() {
                                let _ = self.cmd_tx.send(Cmd::Confirm {
                                    id: c.id.clone(),
                                    approved: true,
                                });
                                self.scen_confirm = true;
                            }
                            if ui.button(t.confirm_deny).clicked() {
                                let _ = self.cmd_tx.send(Cmd::Confirm {
                                    id: c.id.clone(),
                                    approved: false,
                                });
                                self.scen_confirm = true;
                            }
                        });
                    });
                }
            }
        });

        egui::SidePanel::left("tabs").exact_width(148.0).show(ctx, |ui| {
            overflow_scroll(ui, "nav_sidebar", |ui| {
                ui.heading("Akasha");
                self.ui_nav_rail(ui, &t);
                ui.separator();
                ui.heading(t.resources_heading);
                if let Some(m) = &self.metrics {
                    let ratio = m.ram_used as f32 / m.ram_total.max(1) as f32;
                    ui.add(egui::ProgressBar::new(ratio).text(format!(
                        "{} {:.1}/{:.1} GiB",
                        t.metrics_ram,
                        m.ram_used as f64 / (1 << 30) as f64,
                        m.ram_total as f64 / (1 << 30) as f64
                    )));
                    ui.label(format!("CPU {:.0}%", m.cpu_percent));
                    ui.label(format!("{}: {}", t.metrics_live, m.live_inferences()));
                    for mm in &m.models {
                        ui.group(|ui| {
                            ui.label(format!("{} [{:?}]", mm.model_id, mm.state));
                            ui.monospace(format_model_infer_line(mm, &t));
                            if mm.disk_bytes > 0 {
                                ui.weak(format!(
                                    "{} {}",
                                    t.metrics_disk,
                                    human_bytes(mm.disk_bytes)
                                ));
                            }
                            if mm.queued > 0 || mm.active_inferences > 0 {
                                ui.weak(format!(
                                    "inf={} {}={}",
                                    mm.active_inferences, t.metrics_queued, mm.queued
                                ));
                            }
                        });
                    }
                } else {
                    ui.label("…");
                }
            });
        });

        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(28.0)
            .show(ctx, |ui| {
                self.ui_status_bar(ui, &t);
            });

        self.ui_go_to_palette(ctx, &t);

        self.poll_agent_trace(ctx);
        if !self.agent_open_tabs.is_empty() {
            self.ui_agent_detail_panel(ctx);
        }

        let current_tab = self.tab.clone();
        egui::CentralPanel::default().show(ctx, |ui| match current_tab {
            Tab::Chat => self.ui_chat(ui),
            Tab::Memory => self.ui_memory(ui),
            Tab::Notes => self.ui_notes(ui),
            Tab::Tasks => overflow_scroll(ui, "tasks", |ui| self.ui_tasks(ui)),
            Tab::Agents => overflow_scroll(ui, "agents", |ui| self.ui_agents(ui)),
            Tab::Models => overflow_scroll(ui, "models", |ui| self.ui_models(ui, ctx)),
            Tab::Image => overflow_scroll(ui, "image", |ui| {
                let gen = self.image_generating.as_ref();
                let dl_busy = self.model_download.is_some();
                self.image_studio
                    .ui(ui, &i18n::strings(&self.prefs.language), &self.cmd_tx, gen, dl_busy);
            }),
            Tab::Providers => overflow_scroll(ui, "providers", |ui| self.ui_providers(ui)),
            Tab::Audit => self.ui_audit(ui),
            Tab::Caps => overflow_scroll(ui, "caps", |ui| self.ui_caps(ui)),
            Tab::Scenarios => overflow_scroll(ui, "scenarios", |ui| self.ui_scenarios(ui)),
            Tab::Feedback => overflow_scroll(ui, "feedback", |ui| self.ui_feedback(ui)),
            Tab::Settings => overflow_scroll(ui, "settings", |ui| self.ui_settings(ui)),
            Tab::Module(name) => {
                overflow_scroll(ui, ("decl-mod", name.as_str()), |ui| self.ui_decl_module(ui, &name))
            }
        });
    }
}

impl UiApp {
    fn ui_decl_module(&mut self, ui: &mut egui::Ui, module: &str) {
        if !self.decl_panels.contains_key(module) {
            self.decl_panels
                .insert(module.to_string(), decl_ui::DeclUiPanelState::new(module));
            let _ = self.cmd_tx.send(Cmd::ModuleUiLoad {
                module: module.to_string(),
            });
        }
        let t = i18n::strings(&self.prefs.language);
        let mut actions = decl_ui::DeclUiActions::default();
        if let Some(panel) = self.decl_panels.get_mut(module) {
            actions = panel.ui(ui, &mut self.decl_md_cache, t.decl_ui_refresh);
        }
        if actions.refresh {
            let _ = self.cmd_tx.send(Cmd::ModuleUiRefresh {
                module: module.to_string(),
            });
        }
        if let Some((tool, args)) = actions.invoke {
            let _ = self.cmd_tx.send(Cmd::ModuleUiInvoke {
                module: module.to_string(),
                tool,
                args,
            });
        }
    }

    fn ui_room_member_chip(
        &mut self,
        ui: &mut egui::Ui,
        t: &i18n::UiStrings,
        session_id: &str,
        mem: &ChatRoomMember,
    ) {
        let name = chat_room::member_display_label(t, mem);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.strong(&name);
            if ui
                .add(
                    egui::Label::new(egui::RichText::new("×").weak())
                        .sense(egui::Sense::click()),
                )
                .on_hover_text(t.room_member_remove)
                .clicked()
            {
                let _ = self.cmd_tx.send(Cmd::SessionMembersRemove {
                    session_id: session_id.to_string(),
                    agent_id: mem.agent_id.clone(),
                });
            }
        });
    }

    fn ui_room_add_library_chips(
        &mut self,
        ui: &mut egui::Ui,
        t: &i18n::UiStrings,
        session_id: &str,
        model_id: Option<String>,
        candidates: &[AgentInfo],
    ) {
        if candidates.is_empty() {
            return;
        }
        ui.weak(t.room_add_from_library);
        ui.horizontal_wrapped(|ui| {
            for agent in candidates {
                let label = chat_room::roster_agent_label(t, agent);
                if ui.small_button(&label).clicked() {
                    if let Some(persona_id) = agent.persona_id.clone() {
                        let _ = self.cmd_tx.send(Cmd::RoomAddPersona {
                            session_id: session_id.to_string(),
                            persona_id,
                            model_id: model_id.clone(),
                        });
                    } else {
                        let _ = self.cmd_tx.send(Cmd::SessionMembersAdd {
                            session_id: session_id.to_string(),
                            member: ChatRoomMember {
                                agent_id: agent.agent_id.clone(),
                                display_name: label,
                                persona_id: None,
                                joined_ms: chat_room::joined_ms_now(),
                            },
                        });
                    }
                }
            }
        });
    }

    fn ui_room_persona_shortcuts(
        &mut self,
        ui: &mut egui::Ui,
        t: &i18n::UiStrings,
        session_id: &str,
        members: &[ChatRoomMember],
        model_id: Option<String>,
    ) {
        ui.horizontal_wrapped(|ui| {
            for persona in chat_room::ROOM_PERSONAS {
                let agent_id = chat_room::persona_agent_id(persona.id);
                if members.iter().any(|m| m.agent_id == agent_id) {
                    continue;
                }
                let label = chat_room::persona_label(t, persona.id);
                if ui.small_button(label).clicked() {
                    let _ = self.cmd_tx.send(Cmd::RoomAddPersona {
                        session_id: session_id.to_string(),
                        persona_id: persona.id.to_string(),
                        model_id: model_id.clone(),
                    });
                }
            }
        });
    }

    fn dispatch_canvas_ui_action(
        &mut self,
        action: Option<chat_canvas::CanvasUiAction>,
        session_id: &str,
    ) {
        match action {
            Some(chat_canvas::CanvasUiAction::Apply(op)) => {
                match &op {
                    aos_proto::CanvasOpBody::Clear => self.canvas_panel.ops.clear(),
                    aos_proto::CanvasOpBody::Undo => {
                        if let Some(pos) = self
                            .canvas_panel
                            .ops
                            .iter()
                            .rposition(|o| o.author_id == "human")
                        {
                            self.canvas_panel.ops.remove(pos);
                        }
                    }
                    // Apply optimistically so the stroke/shape is visible immediately without
                    // waiting for the server roundtrip snapshot.
                    _ => {
                        self.canvas_panel.ops.push(aos_proto::CanvasOp {
                            seq: 0,
                            author_id: "human".into(),
                            ts_ms: 0,
                            body: op.clone(),
                        });
                    }
                }
                let _ = self.cmd_tx.send(Cmd::CanvasApply {
                    session_id: session_id.to_string(),
                    author_id: "human".into(),
                    op,
                });
            }
            Some(chat_canvas::CanvasUiAction::SetStyle { color, width }) => {
                let _ = self.cmd_tx.send(Cmd::CanvasSetStyle {
                    session_id: session_id.to_string(),
                    color,
                    width,
                });
            }
            Some(chat_canvas::CanvasUiAction::Export) => {
                let aspect = self
                    .sessions
                    .iter()
                    .find(|s| s.id == session_id)
                    .map(|s| s.canvas_aspect)
                    .unwrap_or_default();
                let _ = self.cmd_tx.send(Cmd::CanvasExport {
                    session_id: session_id.to_string(),
                    aspect,
                });
            }
            Some(chat_canvas::CanvasUiAction::SetAspect(aspect)) => {
                if let Some(s) = self.sessions.iter_mut().find(|s| s.id == session_id) {
                    s.canvas_aspect = aspect;
                }
                let _ = self.cmd_tx.send(Cmd::CanvasSetAspect {
                    session_id: session_id.to_string(),
                    aspect,
                });
            }
            None => {}
        }
    }

    fn canvas_poll_if_due(&mut self, ui: &egui::Ui, session_id: &str) {
        let now = ui.ctx().input(|i| i.time);
        if now >= self.canvas_panel.poll_due {
            self.canvas_panel.poll_due = now + 0.20;
            let after = if self.canvas_panel.last_seen_seq > 0 {
                Some(self.canvas_panel.last_seen_seq)
            } else {
                None
            };
            let _ = self.cmd_tx.send(Cmd::CanvasPoll {
                session_id: session_id.to_string(),
                after_seq: after,
            });
        }
    }

    fn ui_session_bar(&mut self, ui: &mut egui::Ui, t: &i18n::UiStrings) {
        let Some(sid) = self.active_session.clone() else {
            return;
        };
        let meta = chat_room::active_session_meta(&self.sessions, Some(sid.as_str()));
        let room = chat_room::session_is_room(meta);
        let canvas_open = meta.map(|m| m.canvas_open).unwrap_or(false);
        let members_vec = meta.map(|m| m.members.clone()).unwrap_or_default();
        let members = members_vec.as_slice();
        let model_id = meta.and_then(|m| m.model_id.clone());
        let session_title = self
            .sessions
            .iter()
            .find(|s| s.id == sid)
            .map(|s| s.title.clone())
            .unwrap_or_else(|| "Session".to_string());
        let count_line = t
            .room_header_member_count
            .replace("{n}", &members.len().to_string());

        ui.horizontal(|ui| {
            let header = egui::RichText::new(&session_title).strong();
            let title_resp = ui.add(
                egui::Label::new(header).sense(if room {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                }),
            );
            if room && title_resp.clicked() {
                self.room_members_pane_open = !self.room_members_pane_open;
            }
            if room {
                title_resp.on_hover_text(t.room_header_open_members);
                if !members.is_empty() {
                    ui.weak(format!("· {count_line}"));
                }
                if self.room_members_pane_open {
                    ui.weak("▾");
                } else {
                    ui.weak("▸");
                }
            }

            let mut toolbar_action: Option<chat_canvas::CanvasUiAction> = None;
            if canvas_open {
                ui.separator();
                toolbar_action = chat_canvas::ui_canvas_toolbar(ui, t, &mut self.canvas_panel);
            }

            let toggle_reserve = 150.0_f32;
            let spare = ui.available_width() - toggle_reserve;
            if spare > 0.0 {
                ui.add_space(spare);
            }
            if ui.selectable_label(room, t.session_toggle_salon).clicked() {
                let mode = if room {
                    ChatSessionMode::Direct
                } else {
                    ChatSessionMode::Room
                };
                let _ = self.cmd_tx.send(Cmd::SessionSetMode {
                    session_id: sid.clone(),
                    mode,
                });
            }
            if ui
                .selectable_label(canvas_open, t.session_toggle_canvas)
                .clicked()
            {
                let new_open = !canvas_open;
                self.set_canvas_open_local(&sid, new_open);
                let _ = self.cmd_tx.send(Cmd::CanvasSetOpen {
                    session_id: sid.clone(),
                    open: new_open,
                });
            }
            if let Some(action) = toolbar_action {
                self.dispatch_canvas_ui_action(Some(action), &sid);
            }
        });

        if room && self.room_members_pane_open {
            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.strong(t.room_members_heading);
                    if members.is_empty() {
                        ui.weak(t.room_members_empty);
                    } else {
                        for mem in members {
                            self.ui_room_member_chip(ui, t, &sid, mem);
                        }
                    }
                    let candidates =
                        chat_room::library_add_candidates(&self.agents, members, t);
                    self.ui_room_add_library_chips(
                        ui,
                        t,
                        &sid,
                        model_id.clone(),
                        &candidates,
                    );
                });
        }

        if room && !members.is_empty() && !self.room_members_pane_open {
            ui.horizontal_wrapped(|ui| {
                for mem in members {
                    self.ui_room_member_chip(ui, t, &sid, mem);
                }
            });
        }

        if room {
            self.ui_room_persona_shortcuts(ui, t, &sid, members, model_id);
        }

        ui.add_space(4.0);
    }

    fn ui_chat_transcript(
        &mut self,
        ui: &mut egui::Ui,
        t: &i18n::UiStrings,
        room_mode: bool,
        room_members: &[ChatRoomMember],
        scroll_h: f32,
    ) {
        egui::ScrollArea::vertical()
            .id_salt("conversation_scroll")
            .auto_shrink([false, false])
            .max_height(scroll_h)
            .min_scrolled_height(scroll_h)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.set_min_height(scroll_h);
                let mut open_agent: Option<String> = None;
                let mut target_reply: Option<String> = None;
                let mut open_studio: Option<(String, String)> = None;
                let reply_id = self.blocked_ask_agent().map(|a| a.agent_id.clone());
                let n = self.chat.len();
                for i in 0..n {
                    let role = self.chat[i].role.clone();
                    let text = self.chat[i].text.clone();
                    let attachments = self.chat[i].attachments.clone();
                    let speaker_id = self.chat[i].speaker_id.clone();
                    let is_completion = attachments.iter().any(|a| {
                        matches!(
                            a,
                            ChatAttachment::AgentRef { origin, .. } if origin == "completion"
                        )
                    });
                    let text = if role == "assistant"
                        && !is_completion
                        && speaker_id.is_none()
                    {
                        agent_panel::format_assistant_display(&text)
                    } else {
                        text
                    };
                    let kind = chat_bubble_kind(&role, speaker_id.as_deref(), room_mode);
                    let mut shown_role = chat_role_label(kind, t, &role);
                    if kind == ChatBubbleKind::RoomSpeaker {
                        if let Some(sid) = speaker_id.as_deref() {
                            shown_role = room_members
                                .iter()
                                .find(|m| m.agent_id == sid)
                                .map(|m| chat_room::member_display_label(t, m))
                                .unwrap_or_else(|| sid.to_string());
                        }
                    }
                    let (fill, stroke, role_color) = if kind == ChatBubbleKind::RoomSpeaker {
                        if let Some(sid) = speaker_id.as_deref() {
                            let (r, g, b) =
                                chat_room::speaker_color_rgb(sid, ui.visuals().dark_mode);
                            let c = egui::Color32::from_rgb(r, g, b);
                            (c.gamma_multiply(0.25), c, c)
                        } else {
                            chat_bubble_colors(kind, ui.visuals().dark_mode)
                        }
                    } else {
                        chat_bubble_colors(kind, ui.visuals().dark_mode)
                    };
                    let frame_colors = if kind == ChatBubbleKind::RoomSpeaker {
                        Some((fill, stroke))
                    } else {
                        None
                    };
                    chat_message_frame(ui, kind, frame_colors, |ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(
                                role_color,
                                egui::RichText::new(&shown_role).strong().small(),
                            );
                            if ui.small_button(t.btn_copy).clicked() {
                                ui.ctx().copy_text(text.clone());
                                self.status = t.copied.into();
                            }
                        });
                        if !text.is_empty() {
                            if role == "assistant" {
                                ui.push_id(("chat_md", i), |ui| {
                                    CommonMarkViewer::new().show(
                                        ui,
                                        &mut self.chat_md_cache,
                                        &text,
                                    );
                                });
                            } else {
                                ui.add(egui::Label::new(&text).wrap());
                            }
                        }
                        for (j, att) in attachments.iter().enumerate() {
                            match att {
                                ChatAttachment::AgentRef {
                                    agent_id,
                                    title,
                                    origin,
                                } => {
                                    if origin == "room" {
                                        continue;
                                    }
                                    let info = self
                                        .agents
                                        .iter()
                                        .find(|a| a.agent_id == *agent_id);
                                    let selected =
                                        reply_id.as_deref() == Some(agent_id.as_str());
                                    let action = ui
                                        .push_id(
                                            ("chat_agent_card", i, j, agent_id.as_str()),
                                            |ui| {
                                                agent_panel::chat_agent_card(
                                                    ui,
                                                    info,
                                                    agent_id.as_str(),
                                                    title.as_str(),
                                                    origin.as_str(),
                                                    selected && origin == "ask",
                                                    t,
                                                )
                                            },
                                        )
                                        .inner;
                                    match action {
                                        agent_panel::ChatCardAction::OpenDetail => {
                                            open_agent = Some(agent_id.clone());
                                        }
                                        agent_panel::ChatCardAction::TargetReply => {
                                            target_reply = Some(agent_id.clone());
                                        }
                                        agent_panel::ChatCardAction::None => {}
                                    }
                                }
                                ChatAttachment::Image { path, prompt } => {
                                    chat_media::render_image(
                                        ui,
                                        t,
                                        path.as_str(),
                                        prompt.as_str(),
                                        || {
                                            open_studio = Some((prompt.clone(), path.clone()));
                                        },
                                    );
                                }
                                ChatAttachment::Audio { path } => {
                                    chat_media::render_audio(ui, path.as_str());
                                }
                                ChatAttachment::TtsDraft { .. } => {
                                    let piper: Vec<String> = self
                                        .model_infos
                                        .iter()
                                        .filter(|m| m.id.contains("piper"))
                                        .map(|m| m.id.clone())
                                        .collect();
                                    if chat_media::render_tts_card(
                                        ui,
                                        t,
                                        &self.cmd_tx,
                                        &mut self.chat[i].attachments[j],
                                        &piper,
                                    ) {
                                        self.status = "audio : génération…".into();
                                    }
                                }
                            }
                        }
                    });
                }
                if let Some(id) = open_agent {
                    self.open_agent_tab(&id);
                }
                if let Some((prompt, path)) = open_studio {
                    self.image_studio.open_from_chat(&prompt, &path, None);
                    self.image_studio.apply_history_for_path(&path);
                    self.tab = Tab::Image;
                }
                if let Some(id) = target_reply {
                    self.ask_reply_target = Some(id);
                    self.chat_refocus = true;
                    self.status = "réponse destinée à cet agent".into();
                }
                if !self.streaming.is_empty() {
                    let (_, _, role_color) =
                        chat_bubble_colors(ChatBubbleKind::Assistant, ui.visuals().dark_mode);
                    chat_message_frame(ui, ChatBubbleKind::Assistant, None, |ui| {
                        ui.colored_label(
                            role_color,
                            egui::RichText::new(t.chat_assistant).strong().small(),
                        );
                        let streaming =
                            agent_panel::format_streaming_preview(&self.streaming);
                        ui.push_id("chat_md_stream", |ui| {
                            CommonMarkViewer::new().show(
                                ui,
                                &mut self.chat_md_cache,
                                &streaming,
                            );
                        });
                    });
                } else if self.chat_pending {
                    let (_, _, role_color) =
                        chat_bubble_colors(ChatBubbleKind::Assistant, ui.visuals().dark_mode);
                    let thinking = if room_mode {
                        self.room_turn_pending_text
                            .as_deref()
                            .and_then(|msg| {
                                chat_room::format_turn_speaker_queue(t, msg, room_members)
                            })
                            .unwrap_or_else(|| t.chat_assistant.to_string())
                    } else {
                        t.chat_assistant.to_string()
                    };
                    chat_message_frame(ui, ChatBubbleKind::Assistant, None, |ui| {
                        ui.colored_label(
                            role_color,
                            egui::RichText::new(&thinking).strong().small(),
                        );
                        ui.weak("…");
                    });
                }
            });
    }

    fn ui_chat(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        let full = ui.available_size();
        let side_w = 220.0_f32;
        let gap = 8.0_f32;
        let chat_w = (full.x - side_w - gap).max(320.0);

        ui.horizontal(|ui| {
            ui.set_min_height(full.y);
            ui.allocate_ui_with_layout(
                egui::vec2(side_w, full.y),
                egui::Layout::top_down(egui::Align::Min).with_cross_justify(true),
                |ui| {
                    ui.set_width(side_w);
                    overflow_scroll(ui, "chat_side", |ui| {
                    ui.set_width(side_w);
                    ui.heading("Sessions");
                    ui.label("Model");
                    {
                        let sid = self.active_session.clone();
                        let mut current = self
                            .sessions
                            .iter()
                            .find(|s| Some(s.id.as_str()) == sid.as_deref())
                            .and_then(|s| s.model_id.clone())
                            .unwrap_or_default();
                        egui::ComboBox::from_id_salt("session_model")
                            .selected_text(if current.is_empty() {
                                "default".to_string()
                            } else {
                                current.clone()
                            })
                            .width(side_w - 12.0)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_value(&mut current, String::new(), "default")
                                    .changed()
                                {
                                    if let Some(id) = sid.clone() {
                                        let _ = self.cmd_tx.send(Cmd::SessionSetModel {
                                            session_id: id,
                                            model_id: None,
                                        });
                                    }
                                }
                                let local_only = self.prefs.routing == "local_only";
                                ui.weak("Local");
                                for m in &self.model_infos {
                                    if m.id.starts_with("provider:") {
                                        continue;
                                    }
                                    if ui
                                        .selectable_value(
                                            &mut current,
                                            m.id.clone(),
                                            format!("{} [{:?}]", m.id, m.state),
                                        )
                                        .changed()
                                    {
                                        if let Some(id) = sid.clone() {
                                            let _ = self.cmd_tx.send(Cmd::SessionSetModel {
                                                session_id: id,
                                                model_id: Some(m.id.clone()),
                                            });
                                        }
                                    }
                                }
                                ui.weak("Providers");
                                for m in &self.model_infos {
                                    if !m.id.starts_with("provider:") {
                                        continue;
                                    }
                                    let pid = m.id.split(':').nth(1).unwrap_or("");
                                    let loopback = self
                                        .providers
                                        .iter()
                                        .find(|p| p.id == pid)
                                        .map(|p| {
                                            let h = p
                                                .endpoint
                                                .trim_start_matches("https://")
                                                .trim_start_matches("http://")
                                                .split(['/', ':'])
                                                .next()
                                                .unwrap_or("");
                                            matches!(
                                                h,
                                                "127.0.0.1" | "localhost" | "::1" | "[::1]"
                                            )
                                        })
                                        .unwrap_or(false);
                                    if local_only && !loopback {
                                        continue;
                                    }
                                    if ui
                                        .selectable_value(
                                            &mut current,
                                            m.id.clone(),
                                            format!("{} [{:?}]", m.id, m.state),
                                        )
                                        .changed()
                                    {
                                        if let Some(id) = sid.clone() {
                                            let _ = self.cmd_tx.send(Cmd::SessionSetModel {
                                                session_id: id,
                                                model_id: Some(m.id.clone()),
                                            });
                                        }
                                    }
                                }
                            });
                    }
                    if ui.button("+ Nouvelle").clicked() {
                        let n = self.sessions.len() + 1;
                        let _ = self.cmd_tx.send(Cmd::SessionCreate {
                            title: Some(format!("Session {n}")),
                        });
                    }
                    egui::ScrollArea::vertical()
                        .id_salt("sessions_list")
                        .max_height(160.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.set_min_width(side_w - 16.0);
                            for s in self.sessions.clone() {
                                let selected =
                                    self.active_session.as_deref() == Some(s.id.as_str());
                                if ui
                                    .selectable_label(
                                        selected,
                                        format!("{} ({})", s.title, s.message_count),
                                    )
                                    .clicked()
                                {
                                    let _ =
                                        self.cmd_tx.send(Cmd::SessionSelect { id: s.id.clone() });
                                }
                            }
                        });
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.rename_buf)
                                .desired_width(120.0)
                                .hint_text("titre"),
                        );
                        if ui.button("Renommer").clicked() {
                            if let Some(id) = self.active_session.clone() {
                                let _ = self.cmd_tx.send(Cmd::SessionRename {
                                    id,
                                    title: self.rename_buf.clone(),
                                });
                            }
                        }
                    });
                    if ui.button("Exporter MD").clicked() {
                        if let Some(id) = self.active_session.clone() {
                            let _ = self.cmd_tx.send(Cmd::SessionExport { id });
                        }
                    }
                    if ui.button("Supprimer").clicked() {
                        if let Some(id) = self.active_session.clone() {
                            let _ = self.cmd_tx.send(Cmd::SessionDelete { id });
                        }
                    }
                    ui.separator();
                    ui.heading("Web / fichiers");
                    ui.set_min_width(side_w - 16.0);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.web_query)
                                    .desired_width(side_w - 20.0)
                                    .hint_text("recherche web"),
                            );
                            if ui.button("Rechercher").clicked() && !self.web_query.is_empty() {
                                let _ = self.cmd_tx.send(Cmd::WebSearch {
                                    query: self.web_query.clone(),
                                    engine: self.prefs.web_search_engine.clone(),
                                });
                            }
                            for hit in &self.web_results {
                                ui.small(format!("• {} — {}", hit.title, hit.url));
                            }
                            ui.add(
                                egui::TextEdit::singleline(&mut self.fetch_url)
                                    .desired_width(side_w - 20.0)
                                    .hint_text("https://…"),
                            );
                            ui.horizontal(|ui| {
                                if ui.button("Télécharger URL").clicked()
                                    && !self.fetch_url.is_empty()
                                {
                                    let _ = self.cmd_tx.send(Cmd::NetFetch {
                                        url: self.fetch_url.clone(),
                                        max_bytes: self.prefs.web_fetch_max_bytes,
                                    });
                                }
                                let t = i18n::strings(&self.prefs.language);
                                if ui.button(t.web_browse_btn).clicked()
                                    && !self.fetch_url.is_empty()
                                {
                                    let _ = self.cmd_tx.send(Cmd::WebBrowse {
                                        url: self.fetch_url.clone(),
                                        max_chars: self.prefs.web_browse_max_chars,
                                    });
                                }
                            });
                            if !self.browse_preview.is_empty() {
                                ui.collapsing("Aperçu page", |ui| {
                                    ui.small(&self.browse_preview);
                                });
                            }
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt("gen_fmt")
                                    .selected_text(&self.gen_format)
                                    .show_ui(ui, |ui| {
                                        for f in ["md", "txt", "json", "csv", "png", "pdf"] {
                                            ui.selectable_value(&mut self.gen_format, f.into(), f);
                                        }
                                    });
                            });
                            ui.add(
                                egui::TextEdit::singleline(&mut self.gen_path)
                                    .desired_width(side_w - 20.0)
                                    .hint_text("/downloads/…"),
                            );
                            ui.add(
                                egui::TextEdit::multiline(&mut self.gen_content)
                                    .desired_width(side_w - 20.0)
                                    .desired_rows(3)
                                    .hint_text("contenu"),
                            );
                            if ui.button("Générer fichier").clicked() && !self.gen_path.is_empty() {
                                let _ = self.cmd_tx.send(Cmd::FilesGenerate {
                                    format: self.gen_format.clone(),
                                    path: self.gen_path.clone(),
                                    content: self.gen_content.clone(),
                                    title: Some("Akasha OS".into()),
                                });
                            }
                            if ui.button("Ouvrir downloads").clicked() {
                                let dir = aos_home().join("var/storage/data/downloads");
                                open_os_folder(&dir);
                            }
                    });
                },
            );

            ui.add_space(gap);

            ui.allocate_ui_with_layout(
                egui::vec2(chat_w, full.y),
                egui::Layout::top_down(egui::Align::Min).with_cross_justify(true),
                |ui| {
                    ui.set_min_width(chat_w);
                    ui.set_min_height(full.y);
                    let room_mode = chat_room::session_is_room(chat_room::active_session_meta(
                        &self.sessions,
                        self.active_session.as_deref(),
                    ));
                    self.ui_session_bar(ui, &t);

                    let room_members: Vec<ChatRoomMember> = chat_room::active_session_meta(
                        &self.sessions,
                        self.active_session.as_deref(),
                    )
                    .map(|m| m.members.clone())
                    .unwrap_or_default();
                    let canvas_open = chat_room::active_session_meta(
                        &self.sessions,
                        self.active_session.as_deref(),
                    )
                    .map(|m| m.canvas_open)
                    .unwrap_or(false);
                    let canvas_aspect = chat_room::active_session_meta(
                        &self.sessions,
                        self.active_session.as_deref(),
                    )
                    .map(|m| m.canvas_aspect)
                    .unwrap_or_default();
                    let active_sid = self.active_session.clone();

                    let input_reserve = 44.0_f32;
                    let content_h = (ui.available_height() - input_reserve).max(120.0);

                    if canvas_open {
                        let split_gap = 8.0_f32;
                        let canvas_min = 180.0_f32;
                        ui.horizontal(|ui| {
                            ui.set_min_height(content_h);
                            let total_w = ui.available_width();
                            let canvas_w = ((total_w - split_gap) * 0.42)
                                .max(canvas_min)
                                .min(total_w - 220.0);
                            let transcript_w = (total_w - canvas_w - split_gap).max(200.0);

                            ui.allocate_ui_with_layout(
                                egui::vec2(transcript_w, content_h),
                                egui::Layout::top_down(egui::Align::Min).with_cross_justify(true),
                                |ui| {
                                    self.ui_chat_transcript(
                                        ui,
                                        &t,
                                        room_mode,
                                        &room_members,
                                        content_h,
                                    );
                                },
                            );
                            ui.add_space(split_gap);
                            ui.allocate_ui_with_layout(
                                egui::vec2(canvas_w, content_h),
                                egui::Layout::top_down(egui::Align::Min).with_cross_justify(true),
                                |ui| {
                                    if let Some(ref sid) = active_sid {
                                        let aspect_action =
                                            chat_canvas::ui_canvas_aspect_row(ui, &t, canvas_aspect);
                                        self.dispatch_canvas_ui_action(aspect_action, sid);
                                        let action = chat_canvas::ui_canvas_surface(
                                            ui,
                                            &mut self.canvas_panel,
                                            canvas_aspect,
                                            t.canvas_empty_hint,
                                        );
                                        self.dispatch_canvas_ui_action(action, sid);
                                        self.canvas_poll_if_due(ui, sid);
                                    }
                                },
                            );
                        });
                    } else {
                        self.ui_chat_transcript(
                            ui,
                            &t,
                            room_mode,
                            &room_members,
                            content_h,
                        );
                    }

                    let completions = slash_completions(&self.input);
                    let mention_hits = if room_mode {
                        chat_room::mention_completions(&self.input, &room_members, &t)
                    } else {
                        Vec::new()
                    };
                    let ask_queue = self.pending_ask_queue();
                    if ask_queue.len() > 1 {
                        let t = i18n::strings(&self.prefs.language);
                        let title = self
                            .blocked_ask_agent()
                            .map(agent_display_title)
                            .unwrap_or_default();
                        ui.colored_label(
                            egui::Color32::from_rgb(240, 190, 100),
                            t.chat_ask_queue
                                .replace("{n}", &ask_queue.len().to_string())
                                .replace("{agent}", &title),
                        );
                    }
                    let input_row = ui.with_layout(
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            let t = i18n::strings(&self.prefs.language);
                            let hint = match ask_queue.len() {
                                0 => t.chat_hint.to_string(),
                                1 => t.chat_hint_agent_ask.to_string(),
                                n => {
                                    let title = self
                                        .blocked_ask_agent()
                                        .map(agent_display_title)
                                        .unwrap_or_default();
                                    t.chat_hint_agent_ask_many
                                        .replace("{agent}", &title)
                                        .replace("{n}", &n.to_string())
                                }
                            };
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut self.input)
                                    .id_salt("chat_input")
                                    .desired_width(ui.available_width() - 90.0)
                                    .hint_text(hint),
                            );
                            if self.chat_refocus {
                                r.request_focus();
                                self.chat_refocus = false;
                            }
                            let send_btn = ui.button("Envoyer").on_hover_text(t.tip_send);
                            if self.chat_pending {
                                if room_mode {
                                    if ui.button(t.chat_stop).clicked() {
                                        if let Some(sid) = self.active_session.clone() {
                                            let _ = self.cmd_tx.send(Cmd::RoomTurnCancel {
                                                session_id: sid,
                                            });
                                        }
                                    }
                                } else if let Some(id) = self.chat_inference_id {
                                    if ui.button(t.chat_stop).clicked() {
                                        let _ = self.cmd_tx.send(Cmd::ChatCancel {
                                            inference_id: id,
                                        });
                                    }
                                }
                            }
                            let send = send_btn.clicked()
                                || (r.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                            if send {
                                self.send_chat();
                                self.chat_refocus = true;
                            }
                            r
                        },
                    );
                    let input_rect = input_row.inner.rect;

                    // Popup au-dessus de l'input, en overlay sur le chat (pas sous le cadre)
                    if !mention_hits.is_empty() {
                        let popup_w = input_rect.width().clamp(240.0, chat_w);
                        let max_h = 180.0_f32;
                        let mut picked: Option<String> = None;
                        egui::Area::new(egui::Id::new("mention_completions_popup"))
                            .order(egui::Order::Foreground)
                            .fixed_pos(egui::pos2(input_rect.left(), input_rect.top() - 6.0))
                            .pivot(egui::Align2::LEFT_BOTTOM)
                            .interactable(true)
                            .show(ui.ctx(), |ui| {
                                egui::Frame::popup(ui.style())
                                    .inner_margin(egui::Margin::same(8))
                                    .show(ui, |ui| {
                                        ui.set_min_width(popup_w * 0.85);
                                        ui.set_max_width(popup_w);
                                        ui.label(
                                            egui::RichText::new(t.room_mention_pick)
                                                .small()
                                                .strong(),
                                        );
                                        egui::ScrollArea::vertical()
                                            .max_height(max_h)
                                            .show(ui, |ui| {
                                                for (text, name) in &mention_hits {
                                                    if ui
                                                        .selectable_label(false, name.as_str())
                                                        .clicked()
                                                    {
                                                        picked = Some(text.clone());
                                                    }
                                                }
                                            });
                                    });
                            });
                        if let Some(text) = picked {
                            self.input = text;
                            self.chat_refocus = true;
                        }
                    } else if !completions.is_empty() {
                        let t = i18n::strings(&self.prefs.language);
                        let popup_w = input_rect.width().clamp(240.0, chat_w);
                        let max_h = 220.0_f32;
                        let mut picked: Option<String> = None;
                        egui::Area::new(egui::Id::new("slash_completions_popup"))
                            .order(egui::Order::Foreground)
                            .fixed_pos(egui::pos2(input_rect.left(), input_rect.top() - 6.0))
                            .pivot(egui::Align2::LEFT_BOTTOM)
                            .interactable(true)
                            .show(ui.ctx(), |ui| {
                                egui::Frame::popup(ui.style())
                                    .inner_margin(egui::Margin::same(8))
                                    .show(ui, |ui| {
                                        ui.set_min_width(popup_w * 0.85);
                                        ui.set_max_width(popup_w);
                                        ui.label(
                                            egui::RichText::new(t.slash_pick)
                                                .small()
                                                .strong(),
                                        );
                                        egui::ScrollArea::vertical()
                                            .max_height(max_h)
                                            .show(ui, |ui| {
                                                for (cmd, desc) in &completions {
                                                    if ui
                                                        .selectable_label(
                                                            false,
                                                            format!("{cmd} — {desc}"),
                                                        )
                                                        .clicked()
                                                    {
                                                        picked = Some(slash_insert_text(cmd));
                                                    }
                                                }
                                            });
                                    });
                            });
                        if let Some(text) = picked {
                            self.input = text;
                            self.chat_refocus = true;
                        }
                    }
                },
            );
        });
    }

    fn ui_memory(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        ui.heading(t.tab_memory);
        ui.weak(t.memory_blurb);
        ui.separator();
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.mem_note)
                    .desired_width(400.0)
                    .hint_text(t.memory_hint_remember),
            );
            if ui.button(t.memory_btn_remember).clicked() && !self.mem_note.is_empty() {
                let _ = self.cmd_tx.send(Cmd::MemRemember {
                    text: self.mem_note.clone(),
                    pinned: true,
                });
                self.mem_note.clear();
            }
            if ui.button(t.memory_btn_list).clicked() {
                let _ = self.cmd_tx.send(Cmd::MemList {
                    include_superseded: self.mem_show_superseded,
                });
            }
            if ui.button(t.memory_btn_wipe).clicked() {
                let _ = self.cmd_tx.send(Cmd::MemWipeUser);
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.mem_query)
                    .desired_width(400.0)
                    .hint_text(t.memory_hint_recall),
            );
            if ui.button(t.memory_btn_recall).clicked() && !self.mem_query.is_empty() {
                let _ = self.cmd_tx.send(Cmd::MemRecall {
                    query: self.mem_query.clone(),
                });
            }
            ui.checkbox(&mut self.mem_show_superseded, t.memory_show_superseded);
        });
        if let Some(edit_id) = self.mem_edit_id {
            ui.horizontal(|ui| {
                ui.label(format!("{} #{edit_id}", t.memory_editing));
                ui.add(
                    egui::TextEdit::singleline(&mut self.mem_edit_text).desired_width(360.0),
                );
                if ui.button(t.memory_btn_save).clicked() && !self.mem_edit_text.is_empty() {
                    let _ = self.cmd_tx.send(Cmd::MemEdit {
                        id: edit_id,
                        text: self.mem_edit_text.clone(),
                    });
                    self.mem_edit_id = None;
                    self.mem_edit_text.clear();
                }
                if ui.button(t.memory_btn_supersede).clicked() && !self.mem_edit_text.is_empty() {
                    let _ = self.cmd_tx.send(Cmd::MemSupersede {
                        id: edit_id,
                        text: self.mem_edit_text.clone(),
                    });
                    self.mem_edit_id = None;
                    self.mem_edit_text.clear();
                }
                if ui.button(t.memory_btn_cancel).clicked() {
                    self.mem_edit_id = None;
                    self.mem_edit_text.clear();
                }
            });
        }
        ui.separator();
        let mut edit_req: Option<(u64, String)> = None;
        let mut delete_id: Option<u64> = None;
        let mut supersede_req: Option<(u64, String)> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            if self.mem_hits.is_empty() {
                ui.weak(t.memory_empty);
            }
            for h in &self.mem_hits {
                if h.superseded && !self.mem_show_superseded {
                    continue;
                }
                let star = if h.pinned { "★" } else { "·" };
                let status = if h.superseded {
                    " [superseded]"
                } else {
                    ""
                };
                let chat_badge = h
                    .metadata
                    .get("source")
                    .and_then(|v| v.as_str())
                    .filter(|s| *s == "chat")
                    .map(|_| format!(" [{}]", t.memory_badge_chat))
                    .unwrap_or_default();
                let rels: String = h
                    .relations
                    .iter()
                    .map(|r| format!("{}→{}", r.rel.as_str(), r.to))
                    .collect::<Vec<_>>()
                    .join(", ");
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "[{star}] #{} {}{status}{chat_badge} (score {:.2})",
                        h.id, h.text, h.score
                    ));
                });
                if !rels.is_empty() {
                    ui.weak(format!("  {}", rels));
                }
                ui.horizontal(|ui| {
                    if ui.small_button(t.memory_btn_edit).clicked() {
                        edit_req = Some((h.id, h.text.clone()));
                    }
                    if ui.small_button(t.memory_btn_replace).clicked() {
                        supersede_req = Some((h.id, h.text.clone()));
                    }
                    if ui.small_button(t.memory_btn_delete).clicked() {
                        delete_id = Some(h.id);
                    }
                });
                ui.separator();
            }
        });
        if let Some((id, text)) = edit_req {
            self.mem_edit_id = Some(id);
            self.mem_edit_text = text;
        }
        if let Some((id, text)) = supersede_req {
            self.mem_edit_id = Some(id);
            self.mem_edit_text = text;
        }
        if let Some(id) = delete_id {
            let _ = self.cmd_tx.send(Cmd::MemDelete { id });
        }
    }

    fn ui_tasks(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        ui.weak(t.tasks_blurb);
        ui.separator();
        let actions = self.tasks.ui(ui, &t);
        if actions.list {
            let _ = self.cmd_tx.send(Cmd::TasksList);
        }
        if let Some((title, notes)) = actions.create {
            let _ = self.cmd_tx.send(Cmd::TasksCreate { title, notes });
        }
        if let Some((id, done)) = actions.complete {
            let _ = self.cmd_tx.send(Cmd::TasksComplete { id, done });
        }
    }

    fn ui_notes(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        ui.weak(t.notes_blurb);
        ui.separator();
        let actions = notes_panel::show_notes_panel(ui, &mut self.notes, &t);
        if actions.list {
            let _ = self.cmd_tx.send(Cmd::NotesList);
        }
        if let Some(query) = actions.search {
            let _ = self.cmd_tx.send(Cmd::NotesSearch { query });
        }
        if let Some(path) = actions.read_path {
            let title = actions.read_title.clone().or_else(|| {
                self.notes
                    .notes
                    .iter()
                    .find(|n| n.path == path)
                    .map(|n| n.title.clone())
            });
            let slug = self
                .notes
                .notes
                .iter()
                .find(|n| n.path == path)
                .map(|n| n.slug.clone());
            let _ = self.cmd_tx.send(Cmd::NotesRead {
                title,
                path: Some(path),
                slug,
            });
        } else if let Some(title) = actions.read_title {
            let _ = self.cmd_tx.send(Cmd::NotesRead {
                title: Some(title),
                path: None,
                slug: None,
            });
        }
        if let Some((title, content)) = actions.save_create {
            let _ = self.cmd_tx.send(Cmd::NotesCreate { title, content });
        }
        if let Some((title, path, content)) = actions.save_update {
            let _ = self.cmd_tx.send(Cmd::NotesUpdate {
                title,
                path,
                content,
            });
        }
        if let Some(path) = actions.attach_path {
            if self.agent_docs.is_empty() {
                self.agent_docs = path;
            } else if !self.agent_docs.split(',').any(|p| p.trim() == path) {
                self.agent_docs = format!("{},{}", self.agent_docs, path);
            }
            self.tab = Tab::Agents;
            self.status = "Note jointe — créez un agent avec ce document.".into();
        }
        if let Some((path, topic)) = actions.related {
            let _ = self.cmd_tx.send(Cmd::NotesRelated { path, topic });
        }
    }

    fn ui_agents(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        ui.heading(t.tab_agents);
        ui.weak(t.agents_blurb);
        ui.separator();
        if ui.button(t.agents_refresh_catalogs).clicked() {
            let _ = self.cmd_tx.send(Cmd::AgentCatalogRefresh);
        }
        ui.label(t.agents_display_name);
        ui.text_edit_singleline(&mut self.agent_display_name);
        ui.label(t.agents_role);
        ui.weak(t.agents_role_optional);
        ui.add(
            egui::TextEdit::multiline(&mut self.agent_task)
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        ui.collapsing(t.agents_advanced, |ui| {
            ui.label(t.agents_model);
            egui::ComboBox::from_id_salt("agent_model")
                .selected_text(if self.agent_model_id.is_empty() {
                    "default".to_string()
                } else {
                    self.agent_model_id.clone()
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.agent_model_id, String::new(), "default");
                    for m in &self.model_infos {
                        ui.selectable_value(
                            &mut self.agent_model_id,
                            m.id.clone(),
                            format!("{} [{:?}]", m.id, m.state),
                        );
                    }
                });
            ui.label(t.agents_system_prompt);
            ui.add(
                egui::TextEdit::multiline(&mut self.agent_system_prompt)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            ui.collapsing("Skills", |ui| {
            if self.skill_catalog.is_empty() {
                ui.weak(t.agents_catalog_empty);
                for name in ["notes-writer", "research", "file-author", "planner"] {
                    let mut on = self.skill_selected.iter().any(|s| s == name);
                    if ui.checkbox(&mut on, name).changed() {
                        if on {
                            self.skill_selected.push(name.into());
                        } else {
                            self.skill_selected.retain(|s| s != name);
                        }
                    }
                }
            } else {
                for s in self.skill_catalog.clone() {
                    let mut on = self.skill_selected.contains(&s.name);
                    if ui
                        .checkbox(&mut on, format!("{} — {}", s.name, s.description))
                        .changed()
                    {
                        if on {
                            self.skill_selected.push(s.name.clone());
                            for t in &s.tools {
                                if !self.tool_selected.contains(t) {
                                    self.tool_selected.push(t.clone());
                                }
                            }
                        } else {
                            self.skill_selected.retain(|x| x != &s.name);
                        }
                    }
                }
            }
            });

        ui.collapsing(t.agents_tools, |ui| {
            for name in [
                "notes.create",
                "notes.list",
                "notes.read",
                "notes.search",
                "notes.update",
                "notes.links",
                "notes.related",
                "tasks.create",
                "tasks.list",
                "tasks.update",
                "tasks.complete",
                "fs.read",
                "fs.write",
                "fs.list",
                "web.search",
                "web.browse",
                "net.fetch",
                "files.generate",
                "agent.spawn",
                "agent.await",
                "plan.update",
            ] {
                let mut on = self.tool_selected.iter().any(|t| t == name);
                if ui.checkbox(&mut on, name).changed() {
                    if on {
                        self.tool_selected.push(name.into());
                    } else {
                        self.tool_selected.retain(|t| t != name);
                    }
                }
            }
        });

        ui.collapsing(t.agents_mcp, |ui| {
            if self.mcp_catalog.is_empty() {
                ui.weak(t.agents_mcp_empty);
            }
            for s in self.mcp_catalog.clone() {
                let mut on = self.mcp_selected.contains(&s.name);
                if ui
                    .checkbox(&mut on, format!("{} ({})", s.name, s.command))
                    .changed()
                {
                    if on {
                        self.mcp_selected.push(s.name.clone());
                    } else {
                        self.mcp_selected.retain(|x| x != &s.name);
                    }
                }
            }
        });

        ui.label(t.agents_docs);
        ui.text_edit_singleline(&mut self.agent_docs);
        });

        let room_active = chat_room::session_is_room(chat_room::active_session_meta(
            &self.sessions,
            self.active_session.as_deref(),
        ));
        if room_active {
            ui.checkbox(
                &mut self.agent_join_room_on_create,
                t.agents_join_room_on_create,
            );
        }

        if ui.button(t.agents_create).clicked() && !self.agent_display_name.trim().is_empty() {
            let documents: Vec<DocumentRef> = self
                .agent_docs
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|p| DocumentRef {
                    path: p.to_string(),
                    label: p.to_string(),
                })
                .collect();
            let _ = self.cmd_tx.send(Cmd::AgentCreate {
                display_name: self.agent_display_name.clone(),
                task: self.agent_task.clone(),
                system_prompt: if self.agent_system_prompt.is_empty() {
                    None
                } else {
                    Some(self.agent_system_prompt.clone())
                },
                skills: self.skill_selected.clone(),
                tools: self.tool_selected.clone(),
                mcp_servers: self.mcp_selected.clone(),
                documents,
                optimize_prompt: false,
                max_steps: self.agent_max_steps,
                timeout_secs: self.agent_timeout_secs,
                model_id: if self.agent_model_id.is_empty() {
                    None
                } else {
                    Some(self.agent_model_id.clone())
                },
                session_id: self.active_session.clone(),
                origin: "library".into(),
                join_active_room: room_active && self.agent_join_room_on_create,
                library: true,
            });
        }

        ui.separator();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.agent_show_history, false, t.agents_tab_active);
            ui.selectable_value(&mut self.agent_show_history, true, t.agents_tab_history);
        });
        let history = self.agent_show_history;
        let library_agents = chat_room::agents_with_library_placeholders(&self.agents, &t);
        let visible: Vec<AgentInfo> = library_agents
            .iter()
            .filter(|a| agent_shown_in_tab(a, history))
            .cloned()
            .collect();
        egui::ScrollArea::vertical()
            .id_salt("agents_list")
            .max_height(280.0)
            .show(ui, |ui| {
                if visible.is_empty() {
                    ui.weak(if history {
                        t.agents_history_empty
                    } else {
                        t.agents_active_empty
                    });
                    return;
                }
                let roots: Vec<_> = visible
                    .iter()
                    .filter(|a| a.parent_id.is_none())
                    .cloned()
                    .collect();
                let orphans: Vec<_> = visible
                    .iter()
                    .filter(|a| {
                        a.parent_id.as_ref().is_some_and(|p| {
                            !visible.iter().any(|x| x.agent_id == *p)
                        })
                    })
                    .cloned()
                    .collect();

                for a in roots.into_iter().chain(orphans) {
                    self.draw_agent_row(ui, &a, 0, t);
                    let children: Vec<_> = visible
                        .iter()
                        .filter(|c| c.parent_id.as_deref() == Some(a.agent_id.as_str()))
                        .cloned()
                        .collect();
                    for child in children {
                        self.draw_agent_row(ui, &child, 1, t);
                    }
                }
            });
        ui.weak(t.agent_click_for_detail);

        ui.separator();
        ui.label(t.agent_steer);
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.agent_steer_id);
            ui.text_edit_singleline(&mut self.agent_steer_txt);
            if ui.button(t.agent_send).clicked()
                && !self.agent_steer_id.is_empty()
                && !self.agent_steer_txt.is_empty()
            {
                let _ = self.cmd_tx.send(Cmd::AgentSteer {
                    id: self.agent_steer_id.clone(),
                    text: self.agent_steer_txt.clone(),
                });
            }
        });
    }

    fn draw_agent_row(
        &mut self,
        ui: &mut egui::Ui,
        a: &AgentInfo,
        indent: usize,
        t: i18n::UiStrings,
    ) {
        ui.horizontal(|ui| {
            if indent > 0 {
                ui.add_space(16.0 * indent as f32);
                ui.small("↳");
            }
            let selected = self.agent_active_tab.as_deref() == Some(a.agent_id.as_str());
            let label = if let Some(pid) = a.persona_id.as_deref() {
                chat_room::persona_label(&t, pid).to_string()
            } else {
                agent_panel::truncate(a.display_title(), 48)
            };
            if ui.selectable_label(selected, &label).on_hover_text(&a.agent_id).clicked()
            {
                self.open_agent_tab(&a.agent_id);
            }
            ui.weak(&a.agent_id);
            ui.colored_label(
                agent_panel::state_color(&a.state),
                if a.is_roster() {
                    "Roster".to_string()
                } else {
                    format!("{:?}", a.state)
                },
            );
            if !a.is_roster() {
                ui.label(format!(
                    "step {}/{}{}",
                    a.step,
                    a.max_steps,
                    if a.tokens_used > 0 {
                        format!(" · {} tok", a.tokens_used)
                    } else {
                        String::new()
                    }
                ));
            }
            if let Some(task) = &a.current_task {
                ui.small(task);
            }
            if !a.children.is_empty() && indent == 0 {
                ui.small(t.agents_subagents.replace("{n}", &a.children.len().to_string()));
            }
            if let Some(reason) = &a.fail_reason {
                ui.colored_label(
                    egui::Color32::from_rgb(220, 120, 100),
                    agent_panel::truncate(reason, 40),
                );
            }
            if !a.is_roster() {
                if ui.small_button(t.agent_pause).clicked() {
                    let _ = self.cmd_tx.send(Cmd::AgentPause {
                        id: a.agent_id.clone(),
                    });
                }
                if ui.small_button(t.agent_kill).clicked() {
                    let _ = self.cmd_tx.send(Cmd::AgentKill {
                        id: a.agent_id.clone(),
                    });
                }
            }
        });
    }

    fn open_agent_tab(&mut self, id: &str) {
        if !self.agent_open_tabs.iter().any(|t| t == id) {
            self.agent_open_tabs.push(id.to_string());
        }
        self.agent_active_tab = Some(id.to_string());
        self.agent_steer_id = id.to_string();
        self.trace_fetched_at = None;
        let _ = self.cmd_tx.send(Cmd::AgentTrace {
            id: id.to_string(),
        });
        let holder = agent_cap_holder(id);
        self.caps_holder = holder.clone();
        let _ = self.cmd_tx.send(Cmd::CapList { holder });
        if self
            .agents
            .iter()
            .find(|a| a.agent_id == id)
            .is_some_and(|a| a.is_roster())
        {
            let _ = self.cmd_tx.send(Cmd::AgentSpecGet { id: id.to_string() });
        }
    }

    fn ui_roster_detail_edits(&mut self, ui: &mut egui::Ui, agent_id: &str, t: i18n::UiStrings) {
        let Some(draft) = self.roster_edit_drafts.get_mut(agent_id) else {
            ui.weak("…");
            return;
        };
        ui.separator();
        ui.collapsing(t.agents_tools, |ui| {
            ui_roster_tool_checkboxes(ui, &t, &mut draft.tools);
        });
        ui.collapsing(t.agents_skills, |ui| {
            if self.skill_catalog.is_empty() {
                ui.weak(t.agents_catalog_empty);
                for name in ["notes-writer", "research", "file-author", "planner"] {
                    let mut on = draft.skills.iter().any(|s| s == name);
                    if ui.checkbox(&mut on, name).changed() {
                        if on {
                            draft.skills.push(name.into());
                        } else {
                            draft.skills.retain(|s| s != name);
                        }
                    }
                }
            } else {
                for s in self.skill_catalog.clone() {
                    let mut on = draft.skills.contains(&s.name);
                    if ui
                        .checkbox(&mut on, format!("{} — {}", s.name, s.description))
                        .changed()
                    {
                        if on {
                            draft.skills.push(s.name.clone());
                            for tool in &s.tools {
                                if !draft.tools.contains(tool) {
                                    draft.tools.push(tool.clone());
                                }
                            }
                        } else {
                            draft.skills.retain(|x| x != &s.name);
                        }
                    }
                }
            }
        });
        ui.collapsing(t.agents_mcp, |ui| {
            if self.mcp_catalog.is_empty() {
                ui.weak(t.agents_mcp_empty);
            }
            for s in self.mcp_catalog.clone() {
                let mut on = draft.mcp_servers.contains(&s.name);
                if ui
                    .checkbox(&mut on, &s.name)
                    .on_hover_text(&s.command)
                    .changed()
                {
                    if on {
                        draft.mcp_servers.push(s.name.clone());
                    } else {
                        draft.mcp_servers.retain(|x| x != &s.name);
                    }
                }
            }
        });
        if ui.button(t.agents_edit_save).clicked() {
            let draft = draft.clone();
            let _ = self.cmd_tx.send(Cmd::AgentRosterUpdate {
                agent_id: agent_id.to_string(),
                display_name: draft.display_name,
                role: draft.role,
                system_prompt: if draft.system_prompt.is_empty() {
                    None
                } else {
                    Some(draft.system_prompt)
                },
                skills: draft.skills,
                tools: draft.tools,
                mcp_servers: draft.mcp_servers,
                model_id: if draft.model_id.is_empty() {
                    None
                } else {
                    Some(draft.model_id)
                },
            });
        }
    }

    fn close_agent_tab(&mut self, id: &str) {
        self.agent_open_tabs.retain(|t| t != id);
        self.agent_traces.remove(id);
        if self.agent_active_tab.as_deref() == Some(id) {
            self.agent_active_tab = self.agent_open_tabs.last().cloned();
        }
    }

    fn poll_agent_trace(&mut self, ctx: &egui::Context) {
        if self.agent_open_tabs.is_empty() {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(400));
        let due = self
            .trace_fetched_at
            .map(|t| t.elapsed() >= Duration::from_secs(1))
            .unwrap_or(true);
        if due {
            self.trace_fetched_at = Some(Instant::now());
            for id in self.agent_open_tabs.clone() {
                let _ = self.cmd_tx.send(Cmd::AgentTrace { id });
            }
        }
    }

    fn ui_agent_detail_panel(&mut self, ctx: &egui::Context) {
        if self.agent_open_tabs.is_empty() {
            return;
        }
        egui::SidePanel::right("agent_detail_tabs")
            .default_width(520.0)
            .min_width(420.0)
            .resizable(true)
            .show(ctx, |ui| {
                let t = i18n::strings(&self.prefs.language);
                ui.horizontal(|ui| {
                    ui.heading(t.agent_detail);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(t.agent_close_all).clicked() {
                            self.agent_open_tabs.clear();
                            self.agent_active_tab = None;
                            self.agent_traces.clear();
                        }
                    });
                });
                ui.horizontal_wrapped(|ui| {
                    let tabs = self.agent_open_tabs.clone();
                    for id in tabs {
                        let selected = self.agent_active_tab.as_deref() == Some(id.as_str());
                        let label = if let Some(a) = self.agents.iter().find(|x| x.agent_id == id)
                        {
                            format!("{} [{:?}]", agent_panel::truncate(a.display_title(), 28), a.state)
                        } else {
                            id.clone()
                        };
                        egui::Frame::NONE
                            .fill(if selected {
                                egui::Color32::from_rgb(45, 55, 70)
                            } else {
                                egui::Color32::from_rgb(30, 32, 38)
                            })
                            .corner_radius(3.0)
                            .inner_margin(egui::Margin::symmetric(6, 3))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if ui
                                        .selectable_label(selected, label)
                                        .clicked()
                                    {
                                        self.agent_active_tab = Some(id.clone());
                                        self.agent_steer_id = id.clone();
                                        let holder = agent_cap_holder(&id);
                                        self.caps_holder = holder.clone();
                                        let _ = self.cmd_tx.send(Cmd::CapList { holder });
                                        if self
                                            .agents
                                            .iter()
                                            .find(|a| a.agent_id == id)
                                            .is_some_and(|a| a.is_roster())
                                        {
                                            let _ = self.cmd_tx.send(Cmd::AgentSpecGet {
                                                id: id.clone(),
                                            });
                                        }
                                    }
                                    if ui.small_button("×").clicked() {
                                        self.close_agent_tab(&id);
                                    }
                                });
                            });
                    }
                });
                ui.separator();

                let active = self.agent_active_tab.clone();
                if let Some(id) = active {
                    let holder = agent_cap_holder(&id);
                    let t = i18n::strings(&self.prefs.language);
                    ui.collapsing(format!("{} ({holder})", t.caps_heading), |ui| {
                        ui.horizontal(|ui| {
                            if ui.small_button(t.caps_refresh).clicked() {
                                self.caps_holder = holder.clone();
                                let _ = self.cmd_tx.send(Cmd::CapList {
                                    holder: holder.clone(),
                                });
                            }
                        });
                        self.draw_caps_list(ui, &holder);
                    });
                    ui.separator();

                    let info = self.agents.iter().find(|a| a.agent_id == id).cloned();
                    let trace = self.agent_traces.get(&id).cloned();
                    if info.as_ref().is_some_and(|a| a.is_roster()) {
                        self.ui_roster_detail_edits(ui, &id, t);
                    }
                    let actions = agent_panel::draw_agent_detail(
                        ui,
                        info.as_ref(),
                        trace.as_ref(),
                        &mut self.agent_steer_txt,
                        &mut self.chat_md_cache,
                        &open_in_browser,
                        &t,
                    );
                    if actions.pause {
                        let _ = self.cmd_tx.send(Cmd::AgentPause { id: id.clone() });
                    }
                    if actions.kill {
                        let _ = self.cmd_tx.send(Cmd::AgentKill { id: id.clone() });
                    }
                    if actions.resume {
                        if info
                            .as_ref()
                            .is_some_and(|a| a.state == AgentState::Blocked)
                        {
                            if let Some(sid) = info
                                .as_ref()
                                .and_then(|a| a.session_id.clone())
                                .or_else(|| self.active_session.clone())
                            {
                                if self.active_session.as_deref() == Some(sid.as_str()) {
                                    let title = info
                                        .as_ref()
                                        .map(|a| a.directive.clone())
                                        .unwrap_or_default();
                                    self.chat.push(ChatLine {
                                        role: "user".into(),
                                        text: t.agent_unblocked.into(),
                                        attachments: vec![ChatAttachment::AgentRef {
                                            agent_id: id.clone(),
                                            title,
                                            origin: "ask-reply".into(),
                                        }],
                                        speaker_id: None,
                                    });
                                }
                            }
                        }
                        let _ = self.cmd_tx.send(Cmd::AgentResume { id: id.clone() });
                    }
                    if actions.retry {
                        let _ = self.cmd_tx.send(Cmd::AgentRetry { id: id.clone() });
                    }
                    if let Some(text) = actions.steer {
                        let blocked = info
                            .as_ref()
                            .is_some_and(|a| a.state == AgentState::Blocked);
                        if blocked {
                            if let Some(sid) = info
                                .as_ref()
                                .and_then(|a| a.session_id.clone())
                                .or_else(|| self.active_session.clone())
                            {
                                let title = info
                                    .as_ref()
                                    .map(|a| a.directive.clone())
                                    .unwrap_or_default();
                                self.send_ask_reply(sid, id.clone(), title, text);
                            } else {
                                let _ = self.cmd_tx.send(Cmd::AgentSteer {
                                    id: id.clone(),
                                    text,
                                });
                            }
                        } else {
                            let _ = self.cmd_tx.send(Cmd::AgentSteer {
                                id: id.clone(),
                                text,
                            });
                        }
                        self.agent_steer_txt.clear();
                    }
                    if let Some(child) = actions.open_child {
                        self.open_agent_tab(&child);
                    }
                } else {
                    ui.weak(t.agents_select_tab);
                }
            });
    }

    fn ui_settings(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        ui.heading(t.settings_title);
        ui.separator();

        let label_w = 160.0_f32;

        ui.heading(t.settings_me);
        egui::Grid::new("settings_me")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .min_col_width(label_w)
            .show(ui, |ui| {
                ui.label(t.language);
                ui.horizontal(|ui| {
                    for (code, label) in [("en", "English"), ("fr", "Français")] {
                        if ui
                            .selectable_label(self.prefs.language == code, label)
                            .clicked()
                        {
                            self.prefs.language = code.into();
                            self.onboarding.language = code.into();
                            save_preferences(&self.prefs);
                            save_onboarding(&self.onboarding);
                            self.status = t.settings_saved.into();
                        }
                    }
                });
                ui.end_row();

                ui.label(t.theme);
                let theme_label = match self.prefs.theme.as_str() {
                    "light" => t.theme_light,
                    "soft" => t.theme_soft,
                    "high_contrast" => t.theme_high_contrast,
                    _ => t.theme_dark,
                };
                egui::ComboBox::from_id_salt("prefs_theme")
                    .selected_text(theme_label)
                    .show_ui(ui, |ui| {
                        for (code, label) in [
                            ("dark", t.theme_dark),
                            ("light", t.theme_light),
                            ("soft", t.theme_soft),
                            ("high_contrast", t.theme_high_contrast),
                        ] {
                            if ui
                                .selectable_label(self.prefs.theme == code, label)
                                .clicked()
                            {
                                self.prefs.theme = code.into();
                                save_preferences(&self.prefs);
                                self.status = t.settings_saved.into();
                            }
                        }
                    });
                ui.end_row();

                ui.label(t.settings_ui_scale);
                let scale_label = format!("{}%", self.prefs.ui_scale_percent);
                egui::ComboBox::from_id_salt("prefs_ui_scale")
                    .selected_text(scale_label)
                    .show_ui(ui, |ui| {
                        for percent in UI_SCALE_PRESETS {
                            let label = format!("{percent}%");
                            if ui
                                .selectable_label(self.prefs.ui_scale_percent == percent, label)
                                .on_hover_text(t.settings_ui_scale_hint)
                                .clicked()
                            {
                                self.prefs.ui_scale_percent = percent;
                                save_preferences(&self.prefs);
                                self.status = t.settings_saved.into();
                            }
                        }
                    });
                ui.end_row();

                ui.label(t.settings_auto_download_updates);
                let mut auto_upd = self.prefs.auto_download_updates;
                if ui
                    .checkbox(&mut auto_upd, t.settings_auto_download_updates)
                    .on_hover_text(t.settings_auto_download_updates_hint)
                    .changed()
                {
                    self.prefs.auto_download_updates = auto_upd;
                    save_preferences(&self.prefs);
                    self.status = t.settings_saved.into();
                }
                ui.end_row();
            });

        ui.add_space(12.0);
        ui.heading(t.settings_models);
        egui::Grid::new("settings_models")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .min_col_width(label_w)
            .show(ui, |ui| {
                ui.label(t.inference_mode);
                ui.horizontal(|ui| {
                    for (code, label) in [
                        ("auto", "Auto"),
                        ("gpu", t.inference_gpu),
                        ("cpu", t.inference_cpu),
                    ] {
                        if ui
                            .selectable_label(self.prefs.inference_mode == code, label)
                            .clicked()
                        {
                            self.prefs.inference_mode = code.into();
                            save_preferences(&self.prefs);
                            let _ = self.cmd_tx.send(Cmd::MigrateModeld {
                                target: code.to_string(),
                            });
                            self.status = format!("{} — migrate ({code})", t.settings_saved);
                        }
                    }
                });
                ui.end_row();

                ui.label(t.routing);
                ui.horizontal(|ui| {
                    for code in ["local_only", "balanced", "remote_only"] {
                        let label = i18n::routing_label(&t, code);
                        let tech = i18n::routing_technical(&t, code);
                        if ui
                            .selectable_label(self.prefs.routing == code, label)
                            .on_hover_text(tech)
                            .clicked()
                        {
                            self.prefs.routing = code.into();
                            self.onboarding.routing = code.into();
                            save_preferences(&self.prefs);
                            save_onboarding(&self.onboarding);
                            let _ = self.cmd_tx.send(Cmd::SetRouting {
                                mode: code.to_string(),
                            });
                        }
                    }
                });
                ui.end_row();

                ui.label(t.settings_default_model);
                egui::ComboBox::from_id_salt("prefs_agent_model")
                    .selected_text(
                        self.prefs
                            .default_agent_model
                            .clone()
                            .unwrap_or_else(|| "default".into()),
                    )
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(self.prefs.default_agent_model.is_none(), "default")
                            .clicked()
                        {
                            self.prefs.default_agent_model = None;
                            self.agent_model_id.clear();
                            save_preferences(&self.prefs);
                        }
                        for m in self.model_infos.clone() {
                            let selected =
                                self.prefs.default_agent_model.as_deref() == Some(m.id.as_str());
                            if ui.selectable_label(selected, &m.id).clicked() {
                                self.prefs.default_agent_model = Some(m.id.clone());
                                self.agent_model_id = m.id;
                                save_preferences(&self.prefs);
                            }
                        }
                    });
                ui.end_row();

                ui.label(t.settings_image_pack);
                egui::ComboBox::from_id_salt("prefs_image_pack")
                    .selected_text(
                        self.prefs
                            .default_image_model
                            .clone()
                            .unwrap_or_else(|| "default".into()),
                    )
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(self.prefs.default_image_model.is_none(), "default")
                            .clicked()
                        {
                            self.prefs.default_image_model = None;
                            save_preferences(&self.prefs);
                        }
                        for m in self.model_infos.clone() {
                            if !(m.id.contains("sd-")
                                || m.id.contains("flux")
                                || m.id.contains("ideogram")
                                || m.name.to_ascii_lowercase().contains("image"))
                            {
                                continue;
                            }
                            let selected =
                                self.prefs.default_image_model.as_deref() == Some(m.id.as_str());
                            if ui.selectable_label(selected, &m.id).clicked() {
                                self.prefs.default_image_model = Some(m.id.clone());
                                save_preferences(&self.prefs);
                            }
                        }
                    });
                ui.end_row();

                ui.label(t.settings_piper_voice);
                egui::ComboBox::from_id_salt("prefs_piper")
                    .selected_text(
                        self.prefs
                            .default_audio_model
                            .clone()
                            .unwrap_or_else(|| "default".into()),
                    )
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(self.prefs.default_audio_model.is_none(), "default")
                            .clicked()
                        {
                            self.prefs.default_audio_model = None;
                            save_preferences(&self.prefs);
                        }
                        for m in self.model_infos.clone() {
                            if !m.id.contains("piper") {
                                continue;
                            }
                            let selected =
                                self.prefs.default_audio_model.as_deref() == Some(m.id.as_str());
                            if ui.selectable_label(selected, &m.id).clicked() {
                                self.prefs.default_audio_model = Some(m.id.clone());
                                save_preferences(&self.prefs);
                            }
                        }
                    });
                ui.end_row();

                ui.horizontal(|ui| {
                    if ui.button(t.tab_models).clicked() {
                        self.tab = Tab::Models;
                    }
                    if ui
                        .button(t.tab_providers)
                        .on_hover_text(t.tab_hint_providers)
                        .clicked()
                    {
                        self.tab = Tab::Providers;
                    }
                });
                ui.end_row();
            });

        egui::CollapsingHeader::new(t.settings_expert_image_defaults)
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("settings_image_defaults")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .min_col_width(label_w)
                    .show(ui, |ui| {
                        ui.label("W / H / steps");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.prefs.image_width).range(64..=2048),
                            );
                            ui.add(
                                egui::DragValue::new(&mut self.prefs.image_height)
                                    .range(64..=2048),
                            );
                            if ui
                                .add(egui::DragValue::new(&mut self.prefs.image_steps).range(1..=150))
                                .changed()
                            {
                                save_preferences(&self.prefs);
                            }
                            if ui.button(t.settings_saved).clicked() {
                                save_preferences(&self.prefs);
                            }
                        });
                        ui.end_row();
                    });
            });

        ui.add_space(12.0);
        ui.heading(t.settings_trust);
        egui::Grid::new("settings_trust")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .min_col_width(label_w)
            .show(ui, |ui| {
                ui.label(t.trust_default);
                ui.horizontal(|ui| {
                    for (code, label) in [("low", t.trust_low), ("medium", t.trust_medium)] {
                        if ui
                            .selectable_label(self.prefs.trust_default == code, label)
                            .clicked()
                        {
                            self.prefs.trust_default = code.into();
                            self.onboarding.trust_default = code.into();
                            save_preferences(&self.prefs);
                            save_onboarding(&self.onboarding);
                        }
                    }
                });
                ui.end_row();

                ui.label(t.network_heading);
                let mut online = self.prefs.network_online;
                if ui.checkbox(&mut online, t.allow_network).changed() {
                    self.prefs.network_online = online;
                    self.network_online = online;
                    save_preferences(&self.prefs);
                    let _ = self.cmd_tx.send(Cmd::NetSetMode { online });
                }
                ui.end_row();

                ui.label(t.settings_auto_remember);
                let mut auto = self.prefs.auto_remember_chat;
                if ui
                    .checkbox(&mut auto, t.settings_auto_remember)
                    .on_hover_text(t.settings_auto_remember_hint)
                    .changed()
                {
                    self.prefs.auto_remember_chat = auto;
                    save_preferences(&self.prefs);
                    self.status = t.settings_saved.into();
                }
                ui.end_row();
            });

        egui::CollapsingHeader::new(t.settings_expert_agent)
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("settings_agents")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .min_col_width(label_w)
                    .show(ui, |ui| {
                        ui.label(t.settings_max_steps);
                        if ui
                            .add(egui::DragValue::new(&mut self.prefs.default_max_steps).range(1..=128))
                            .changed()
                        {
                            self.agent_max_steps = self.prefs.default_max_steps;
                            save_preferences(&self.prefs);
                        }
                        ui.end_row();

                        ui.label(t.settings_timeout);
                        if ui
                            .add(
                                egui::DragValue::new(&mut self.prefs.default_timeout_secs)
                                    .range(60..=86_400),
                            )
                            .changed()
                        {
                            self.agent_timeout_secs = self.prefs.default_timeout_secs;
                            save_preferences(&self.prefs);
                        }
                        ui.end_row();
                    });
            });

        egui::CollapsingHeader::new(t.settings_expert_web)
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("settings_web")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .min_col_width(label_w)
                    .show(ui, |ui| {
                        ui.label(t.settings_search_engine);
                        egui::ComboBox::from_id_salt("prefs_search_engine")
                            .selected_text(&self.prefs.web_search_engine)
                            .show_ui(ui, |ui| {
                                for eng in ["auto", "brave", "duckduckgo", "bing"] {
                                    if ui
                                        .selectable_label(
                                            self.prefs.web_search_engine == eng,
                                            eng,
                                        )
                                        .clicked()
                                    {
                                        self.prefs.web_search_engine = eng.into();
                                        save_preferences(&self.prefs);
                                    }
                                }
                            });
                        ui.end_row();

                        ui.label(t.settings_browse_chars);
                        if ui
                            .add(
                                egui::DragValue::new(&mut self.prefs.web_browse_max_chars)
                                    .range(1000..=100_000),
                            )
                            .changed()
                        {
                            save_preferences(&self.prefs);
                        }
                        ui.end_row();

                        ui.label(t.settings_fetch_max);
                        if ui
                            .add(
                                egui::DragValue::new(&mut self.prefs.web_fetch_max_bytes)
                                    .range(1024..=200_000_000),
                            )
                            .changed()
                        {
                            save_preferences(&self.prefs);
                        }
                        ui.end_row();
                    });
            });

        egui::CollapsingHeader::new(t.settings_secrets)
            .default_open(false)
            .show(ui, |ui| {
                ui.weak(t.settings_secrets_blurb);
                egui::Grid::new("settings_secrets")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .min_col_width(label_w)
                    .show(ui, |ui| {
                        ui.label("Brave Search");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.secret_brave)
                                    .password(true)
                                    .desired_width(220.0)
                                    .hint_text("BSA…"),
                            );
                            if ui.button(t.settings_secret_save).clicked() {
                                let _ = self.cmd_tx.send(Cmd::SecretSet {
                                    name: "brave_search_api_key".into(),
                                    value: self.secret_brave.clone(),
                                });
                                self.secret_brave.clear();
                            }
                        });
                        ui.end_row();

                        ui.label("GitHub token");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.secret_github)
                                    .password(true)
                                    .desired_width(220.0)
                                    .hint_text("ghp_…"),
                            );
                            if ui.button(t.settings_secret_save).clicked() {
                                let _ = self.cmd_tx.send(Cmd::SecretSet {
                                    name: "github_token".into(),
                                    value: self.secret_github.clone(),
                                });
                                self.secret_github.clear();
                            }
                        });
                        ui.end_row();

                        ui.label(t.settings_secret_openai);
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.secret_openai)
                                    .password(true)
                                    .desired_width(220.0)
                                    .hint_text("sk-…"),
                            );
                            if ui.button(t.settings_secret_save).clicked() {
                                let _ = self.cmd_tx.send(Cmd::SecretSet {
                                    name: "openai_api_key".into(),
                                    value: self.secret_openai.clone(),
                                });
                                self.secret_openai.clear();
                            }
                        });
                        ui.end_row();
                    });
                ui.horizontal(|ui| {
                    if ui.button(t.settings_secret_list).clicked() {
                        let _ = self.cmd_tx.send(Cmd::SecretList);
                    }
                    if self.secret_vault_encrypted {
                        ui.weak(t.settings_secret_encrypted);
                    }
                    if !self.secret_names.is_empty() {
                        ui.weak(format!(
                            "{}: {}",
                            t.settings_secret_configured,
                            self.secret_names.join(", ")
                        ));
                    }
                });
                ui.weak(t.settings_brave_hint);
            });

        egui::CollapsingHeader::new(t.settings_catalogue)
            .default_open(false)
            .show(ui, |ui| {
                ui.weak(t.settings_catalogue_blurb);
                if ui.button(t.settings_secret_list).clicked() {
                    let _ = self.cmd_tx.send(Cmd::CatalogueRefresh);
                    let _ = self.cmd_tx.send(Cmd::ModuleList);
                }
                match self.catalogue.clone() {
                    Some(cat) if cat.signature_ok => {
                        for e in cat.entries {
                            let installed = self
                                .installed_modules
                                .iter()
                                .find(|m| m.name == e.name)
                                .cloned();
                            ui.horizontal(|ui| {
                                let mut label =
                                    format!("{} {} ({})", e.name, e.version, e.kind);
                                if let Some(m) = &installed {
                                    label.push_str(&format!(
                                        " [{}]",
                                        t.settings_catalogue_installed
                                    ));
                                    if m.quarantined {
                                        label.push_str(" [quarantine]");
                                    }
                                }
                                ui.label(label);
                                if e.kind == "module" {
                                    if aos_proto::decl_ui::is_bundled_module(&e.name) {
                                        ui.weak(t.settings_bundled_locked);
                                    } else if installed.is_some() {
                                        if ui.button(t.settings_catalogue_uninstall).clicked() {
                                            let _ = self.cmd_tx.send(Cmd::ModuleUninstall {
                                                name: e.name.clone(),
                                            });
                                        }
                                    } else if ui.button(t.settings_catalogue_install).clicked()
                                    {
                                        let src = aos_home().join(&e.path);
                                        let _ = self.cmd_tx.send(Cmd::ModuleInstall {
                                            source_dir: src.to_string_lossy().into_owned(),
                                            approved_caps: None,
                                        });
                                    }
                                }
                            });
                            if !e.attested_caps.is_empty() {
                                ui.weak(format!(
                                    "{}: {}",
                                    t.settings_catalogue_caps,
                                    e.attested_caps.join(", ")
                                ));
                            }
                        }
                    }
                    Some(_) => {
                        ui.weak(t.settings_catalogue_unsigned);
                    }
                    None => {
                        ui.weak(t.settings_catalogue_unsigned);
                    }
                }

                ui.add_space(8.0);
                ui.weak(t.settings_installed_modules);
                for m in self.installed_modules.clone() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{} v{}", m.name, m.version));
                        if aos_proto::decl_ui::is_bundled_module(&m.name) {
                            ui.weak(t.settings_bundled_locked);
                        } else if ui.button(t.settings_catalogue_uninstall).clicked() {
                            let _ = self.cmd_tx.send(Cmd::ModuleUninstall {
                                name: m.name.clone(),
                            });
                        }
                    });
                }
            });

        egui::CollapsingHeader::new(t.schedule_heading)
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("settings_schedules")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .min_col_width(label_w)
                    .show(ui, |ui| {
                        ui.label(t.schedule_goal);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.schedule_goal)
                                .desired_width(280.0)
                                .hint_text("agent goal"),
                        );
                        ui.end_row();

                        ui.label(t.schedule_interval);
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.schedule_interval_secs)
                                    .range(30..=86_400)
                                    .suffix(" s"),
                            );
                            if ui
                                .button(t.schedule_create)
                                .on_hover_text(t.tip_schedule_create)
                                .clicked()
                                && !self.schedule_goal.trim().is_empty()
                            {
                                let _ = self.cmd_tx.send(Cmd::ScheduleCreate {
                                    goal: self.schedule_goal.trim().to_string(),
                                    interval_secs: self.schedule_interval_secs.max(30),
                                });
                                self.schedule_goal.clear();
                            }
                            if ui.button(t.caps_refresh).clicked() {
                                let _ = self.cmd_tx.send(Cmd::ScheduleList);
                            }
                        });
                        ui.end_row();
                    });
                if self.schedules.is_empty() {
                    ui.weak("Aucun schedule");
                } else {
                    for s in self.schedules.clone() {
                        ui.horizontal(|ui| {
                            let flag = if s.enabled { "ON" } else { "OFF" };
                            ui.monospace(&s.id);
                            ui.label(format!(
                                "[{flag}] every {}s · fires={} · {}",
                                s.interval_secs, s.fire_count, s.goal
                            ));
                            if s.enabled
                                && ui
                                    .small_button(t.schedule_cancel)
                                    .on_hover_text(t.tip_schedule_cancel)
                                    .clicked()
                            {
                                let _ = self.cmd_tx.send(Cmd::ScheduleCancel { id: s.id });
                            }
                        });
                    }
                }
            });
    }

    fn ui_providers(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        ui.heading(t.tab_providers);
        ui.weak(t.providers_blurb);
        ui.separator();
        if ui.button(t.providers_refresh).clicked() {
            let _ = self.cmd_tx.send(Cmd::ProviderList);
        }
        ui.add_space(6.0);
        for p in self.providers.clone() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.strong(&p.id);
                    ui.weak(&p.preset);
                    ui.label(&p.endpoint);
                    if p.enabled {
                        ui.weak(t.providers_on);
                    } else {
                        ui.weak(t.providers_off);
                    }
                    if ui.button(t.providers_test).clicked() {
                        let _ = self.cmd_tx.send(Cmd::ProviderTest { id: p.id.clone() });
                    }
                    if ui.button(t.providers_remove).clicked() {
                        let _ = self.cmd_tx.send(Cmd::ProviderRemove { id: p.id.clone() });
                    }
                    if ui.button(t.providers_edit).clicked() {
                        self.provider_id = p.id.clone();
                        self.provider_preset = p.preset.clone();
                        self.provider_endpoint = p.endpoint.clone();
                        self.provider_secret_name =
                            p.secret_name.clone().unwrap_or_default();
                        self.provider_enabled = p.enabled;
                    }
                });
                if !p.discovered_models.is_empty() {
                    ui.weak(p.discovered_models.join(", "));
                }
            });
        }
        ui.separator();
        ui.label(t.providers_add_edit);
        ui.horizontal(|ui| {
            ui.label("id");
            ui.text_edit_singleline(&mut self.provider_id);
        });
        ui.horizontal(|ui| {
            ui.label(t.providers_preset);
            egui::ComboBox::from_id_salt("provider_preset")
                .selected_text(&self.provider_preset)
                .show_ui(ui, |ui| {
                    for &(name, endpoint, secret) in aos_proto::PROVIDER_PRESETS {
                        if ui
                            .selectable_label(self.provider_preset == name, name)
                            .clicked()
                        {
                            self.provider_preset = name.into();
                            if self.provider_id.is_empty() {
                                self.provider_id = name.into();
                            }
                            if !endpoint.is_empty() {
                                self.provider_endpoint = endpoint.into();
                            }
                            if let Some(s) = secret {
                                self.provider_secret_name = s.into();
                            }
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label(t.providers_endpoint);
            ui.add(
                egui::TextEdit::singleline(&mut self.provider_endpoint).desired_width(420.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label(t.providers_secret);
            ui.text_edit_singleline(&mut self.provider_secret_name);
        });
        ui.horizontal(|ui| {
            ui.label("API key (vault)");
            ui.add(
                egui::TextEdit::singleline(&mut self.provider_secret_value)
                    .password(true)
                    .desired_width(280.0),
            );
        });
        ui.checkbox(&mut self.provider_enabled, t.providers_enabled);
        ui.horizontal(|ui| {
            if ui.button(t.providers_save).clicked() && !self.provider_id.trim().is_empty() {
                let rec = ProviderRecord {
                    id: self.provider_id.trim().to_string(),
                    preset: self.provider_preset.clone(),
                    endpoint: self.provider_endpoint.trim().to_string(),
                    secret_name: if self.provider_secret_name.trim().is_empty() {
                        None
                    } else {
                        Some(self.provider_secret_name.trim().to_string())
                    },
                    enabled: self.provider_enabled,
                    discovered_models: Vec::new(),
                };
                let secret = if self.provider_secret_value.is_empty() {
                    None
                } else {
                    Some(std::mem::take(&mut self.provider_secret_value))
                };
                let _ = self.cmd_tx.send(Cmd::ProviderUpsert {
                    provider: rec,
                    secret_value: secret,
                });
            }
            if ui.button(t.providers_test).clicked() && !self.provider_id.trim().is_empty() {
                let _ = self.cmd_tx.send(Cmd::ProviderTest {
                    id: self.provider_id.trim().to_string(),
                });
            }
        });
        if !self.provider_test_msg.is_empty() {
            ui.label(&self.provider_test_msg);
        }
    }

    fn ui_model_download_restart(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.model_download_restart.is_none() {
            return;
        }
        let t = i18n::strings(&self.prefs.language);
        ui.horizontal(|ui| {
            ui.colored_label(
                egui::Color32::from_rgb(120, 200, 140),
                self.download_status.as_str(),
            );
            if ui.button(t.models_restart_preview).clicked() {
                request_preview_restart(ctx);
                self.model_download_restart = None;
            }
            if ui.small_button("×").clicked() {
                self.model_download_restart = None;
                self.download_status.clear();
            }
        });
    }

    fn ui_models(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let t = i18n::strings(&self.prefs.language);
        ui.heading(t.tab_models);
        ui.weak(t.tab_hint_models);
        ui.horizontal(|ui| {
            if ui.button("Refresh list").clicked() {
                let _ = self.cmd_tx.send(Cmd::ModelsRefresh);
            }
        });
        if !self.model_updates_msg.is_empty() {
            ui.colored_label(
                egui::Color32::from_rgb(180, 220, 120),
                &self.model_updates_msg,
            );
        }
        if !self.download_status.is_empty() && self.model_download_restart.is_none() {
            ui.label(&self.download_status);
        }
        if let Some(dl) = &self.model_download {
            let frac = (dl.percent as f32 / 100.0).clamp(0.0, 1.0);
            let txt = if dl.total_bytes > 0 {
                format!(
                    "{} · {} / {}",
                    dl.model_id,
                    human_bytes(dl.done_bytes),
                    human_bytes(dl.total_bytes)
                )
            } else {
                format!("{} · {}%", dl.model_id, dl.percent)
            };
            ui.add(egui::ProgressBar::new(frac).text(txt));
        }
        self.ui_model_download_restart(ui, ctx);

        models_page::ui_hf_import(
            ui,
            &mut self.hf_download_url,
            &mut self.hf_download_name,
            &mut self.hf_download_status,
            self.model_download.is_some(),
            &self.cmd_tx,
            &t,
        );

        ui.separator();
        models_page::ui_catalog_tab_bar(ui, &mut self.models_catalog_tab, &t);
        if matches!(
            self.models_catalog_tab,
            models_page::ModelCatalogTab::Image | models_page::ModelCatalogTab::Audio
        ) {
            ui.weak(t.models_media_packs);
        }

        let catalog = models_page::load_catalog_models();
        let installed_rows = models_page::load_installed_rows(&self.model_infos);
        let busy = self.model_download.is_some();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                match self.models_catalog_tab {
                    models_page::ModelCatalogTab::Installed => {
                        if installed_rows.is_empty() {
                            ui.weak(t.models_catalog_empty);
                        }
                        for m in installed_rows {
                            let id = m.id.clone();
                            let mut load = false;
                            let mut set_default = false;
                            let mut redownload = false;
                            let mut remove = false;
                            models_page::ui_installed_card(
                                ui,
                                &m,
                                busy,
                                &t,
                                &mut || load = true,
                                &mut || set_default = true,
                                &mut || redownload = true,
                                &mut || remove = true,
                            );
                            if load {
                                let _ = self.cmd_tx.send(Cmd::ModelLoad {
                                    model_id: id.clone(),
                                });
                            }
                            if set_default {
                                if let Some(sid) = self.active_session.clone() {
                                    let _ = self.cmd_tx.send(Cmd::SessionSetModel {
                                        session_id: sid,
                                        model_id: Some(id.clone()),
                                    });
                                }
                            }
                            if redownload {
                                let _ = self.cmd_tx.send(Cmd::ModelRedownload {
                                    model_id: id.clone(),
                                });
                            }
                            if remove {
                                let _ = self.cmd_tx.send(Cmd::ModelRemove {
                                    model_id: id,
                                });
                            }
                            ui.add_space(6.0);
                        }
                        ui.separator();
                        ui.label(t.metrics_live);
                        if let Some(m) = &self.metrics {
                            for mm in &m.models {
                                ui.group(|ui| {
                                    ui.strong(format!("{} [{:?}]", mm.model_id, mm.state));
                                    ui.label(format_model_infer_line(mm, &t));
                                });
                            }
                        }
                    }
                    tab => {
                        let filtered: Vec<_> = catalog
                            .iter()
                            .filter(|m| models_page::category_of(m) == tab)
                            .collect();
                        if filtered.is_empty() {
                            ui.weak(t.models_catalog_empty);
                        }
                        for m in filtered {
                            let installed = installed_rows.iter().any(|x| x.id == m.id);
                            let id = m.id.clone();
                            let mut download = false;
                            let mut redownload = false;
                            let mut remove = false;
                            let mut open_hf = None;
                            models_page::ui_model_card(
                                ui,
                                m,
                                installed,
                                busy,
                                &t,
                                &mut || download = true,
                                &mut || redownload = true,
                                &mut || remove = true,
                                &mut |url| open_hf = Some(url.to_string()),
                            );
                            if download {
                                let _ = self.cmd_tx.send(Cmd::ModelDownload {
                                    model_id: id.clone(),
                                });
                            }
                            if redownload {
                                let _ = self.cmd_tx.send(Cmd::ModelRedownload {
                                    model_id: id.clone(),
                                });
                            }
                            if remove {
                                let _ = self.cmd_tx.send(Cmd::ModelRemove {
                                    model_id: id.clone(),
                                });
                            }
                            if let Some(url) = open_hf {
                                open_url(&url);
                            }
                            ui.add_space(6.0);
                        }
                    }
                }
            });
    }

    fn ui_audit(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        ui.heading(t.audit_heading);
        ui.horizontal(|ui| {
            if ui.button(t.decl_ui_refresh).clicked() {
                let _ = self.cmd_tx.send(Cmd::Audit { last: 50 });
            }
            if ui.button(t.audit_kill_p4).clicked() {
                let _ = self.cmd_tx.send(Cmd::KillAuditd);
            }
        });
        egui::ScrollArea::vertical().show(ui, |ui| {
            for e in &self.audit {
                ui.monospace(format!(
                    "#{} {} {} {} → {}",
                    e.seq, e.actor, e.action, e.target, e.hash
                ));
            }
        });
    }

    fn ui_caps(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        ui.heading(t.caps_heading);
        ui.weak(t.caps_blurb);
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(t.caps_subject);
            ui.add(
                egui::TextEdit::singleline(&mut self.caps_holder)
                    .desired_width(280.0)
                    .hint_text("agent:<id>"),
            );
            if ui
                .button(t.caps_refresh)
                .on_hover_text(t.tip_caps_refresh)
                .clicked()
                && !self.caps_holder.trim().is_empty()
            {
                let holder = self.caps_holder.trim().to_string();
                self.caps_holder = holder.clone();
                let _ = self.cmd_tx.send(Cmd::CapList { holder });
            }
        });
        if let Some(id) = self.agent_active_tab.clone() {
            ui.horizontal(|ui| {
                let holder = agent_cap_holder(&id);
                ui.weak(format!("Agent actif → {holder}"));
                if ui.small_button("Charger").clicked() {
                    self.caps_holder = holder.clone();
                    let _ = self.cmd_tx.send(Cmd::CapList { holder });
                }
            });
        }
        ui.separator();
        let holder = self.caps_holder.clone();
        self.draw_caps_list(ui, &holder);
    }

    fn draw_caps_list(&mut self, ui: &mut egui::Ui, holder: &str) {
        let t = i18n::strings(&self.prefs.language);
        let matching: Vec<CapInfo> = if holder.is_empty() {
            self.caps.clone()
        } else {
            self.caps
                .iter()
                .filter(|c| c.holder == holder)
                .cloned()
                .collect()
        };
        if matching.is_empty() {
            ui.weak(t.caps_empty);
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt(format!("caps_list_{holder}"))
            .max_height(360.0)
            .show(ui, |ui| {
                for c in matching {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.monospace(format!("#{}", c.cap_id));
                            ui.label(&c.object);
                            ui.weak(c.rights.join(", "));
                            if ui
                                .small_button(t.caps_revoke)
                                .on_hover_text(t.tip_caps_revoke)
                                .clicked()
                            {
                                let _ = self.cmd_tx.send(Cmd::CapRevoke {
                                    holder: c.holder.clone(),
                                    cap_id: c.cap_id,
                                    tree: false,
                                });
                            }
                        });
                    });
                }
            });
    }

    fn ui_scenarios(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        let mut flags = scenarios_panel::ScenarioFlags {
            chat: self.scen_chat,
            note_human: self.scen_note_human,
            note_agent: self.scen_note_agent,
            confirm: self.scen_confirm,
            audit: self.scen_audit,
            module_agent: self.scen_module_agent,
        };
        let mut launch = false;
        let mut test_confirm = false;
        scenarios_panel::ui(
            ui,
            &t,
            &mut flags,
            || launch = true,
            || test_confirm = true,
        );
        self.scen_chat = flags.chat;
        self.scen_note_human = flags.note_human;
        self.scen_note_agent = flags.note_agent;
        self.scen_confirm = flags.confirm;
        self.scen_audit = flags.audit;
        self.scen_module_agent = flags.module_agent;
        if launch {
            self.launch_module_author_agent();
        }
        if test_confirm {
            self.status =
                "Créez puis tentez de supprimer une note sensible, ou utilisez le gate P3 en lab."
                    .into();
            let _ = self.cmd_tx.send(Cmd::RefreshConfirms);
        }
    }

    fn reset_feedback_form(&mut self) {
        self.fb_title.clear();
        self.fb_body.clear();
        self.fb_scenario.clear();
        self.fb_category = "ux".into();
        self.fb_severity = "medium".into();
        self.fb_github = true;
        self.fb_diag_meta = None;
    }

    fn ui_feedback(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        ui.heading(t.feedback_heading);
        ui.label(t.feedback_blurb);
        ui.horizontal(|ui| {
            ui.label(t.feedback_title);
            ui.text_edit_singleline(&mut self.fb_title);
        });
        ui.horizontal(|ui| {
            ui.label(t.feedback_category);
            egui::ComboBox::from_id_salt("fb_cat")
                .selected_text(&self.fb_category)
                .show_ui(ui, |ui| {
                    for c in ["bug", "ux", "perf", "security", "other"] {
                        ui.selectable_value(&mut self.fb_category, c.into(), c);
                    }
                });
            ui.label(t.feedback_severity);
            egui::ComboBox::from_id_salt("fb_sev")
                .selected_text(&self.fb_severity)
                .show_ui(ui, |ui| {
                    for s in ["low", "medium", "high"] {
                        ui.selectable_value(&mut self.fb_severity, s.into(), s);
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label(t.feedback_scenario);
            ui.text_edit_singleline(&mut self.fb_scenario);
        });
        ui.text_edit_multiline(&mut self.fb_body);
        let t = i18n::strings(&self.prefs.language);
        if ui.button(t.btn_copy).clicked() {
            ui.ctx().copy_text(self.fb_body.clone());
            self.status = t.copied.into();
        }
        let security = self.fb_category.eq_ignore_ascii_case("security");
        if security {
            self.fb_github = false;
            ui.weak(
                "Les rapports security restent locaux (pas d'issue publique). Utilisez GitHub Security Advisories.",
            );
        } else {
            ui.checkbox(&mut self.fb_github, "Créer une issue GitHub");
            if self.fb_github && !self.network_online {
                ui.weak(
                    "Réseau in-app coupé : le navigateur ouvrira le formulaire GitHub (compte GitHub requis).",
                );
            }
        }
        if ui.button("Envoyer le retour").clicked() && !self.fb_title.is_empty() {
            let mut meta = serde_json::json!({
                "preview_version": self.version,
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "scenarios": {
                    "chat_offline": self.scen_chat,
                    "note_human": self.scen_note_human,
                    "note_agent": self.scen_note_agent,
                    "confirm": self.scen_confirm,
                    "audit": self.scen_audit,
                    "module_agent": self.scen_module_agent,
                },
                "onboarding": self.onboarding,
            });
            // Fusionner les champs du rapport de dépannage (source, findings, healthy)
            // pour qu'ils figurent dans l'issue GitHub remontée.
            if let Some(diag) = &self.fb_diag_meta {
                if let (Some(m), Some(d)) = (meta.as_object_mut(), diag.as_object()) {
                    for (k, v) in d {
                        m.entry(k).or_insert_with(|| v.clone());
                    }
                }
            }
            let _ = self.cmd_tx.send(Cmd::Feedback(FeedbackSubmitRequest {
                title: self.fb_title.clone(),
                category: self.fb_category.clone(),
                severity: self.fb_severity.clone(),
                body: self.fb_body.clone(),
                scenario: if self.fb_scenario.is_empty() {
                    None
                } else {
                    Some(self.fb_scenario.clone())
                },
                meta,
                publish_github: self.fb_github && !security,
            }));
        }
        if !self.fb_result.is_empty() {
            ui.separator();
            ui.label(&self.fb_result);
            if ui.button(t.feedback_open_folder).clicked() {
                let dir = self
                    .fb_dir
                    .clone()
                    .unwrap_or_else(|| aos_home().join("var/feedback"));
                open_os_folder(&dir);
            }
        }
    }
}

#[cfg(test)]
mod delegate_tests {
    use super::*;
    use aos_proto::CanvasAspect;

    const ASPECT: CanvasAspect = CanvasAspect::Square;

    #[test]
    fn create_module_dump_delegates_instead_of_display() {
        let dumped = r#"{"kind":"column","children":[{"kind":"heading","text":"Ping"}]}"#;
        let spec = chat_delegate_agent_spec("crée un module ping", dumped, false, ASPECT);
        let (brief, _skills, tools, prose) = spec.expect("doit déléguer");
        assert_eq!(brief, "crée un module ping");
        assert!(tools.iter().any(|x| x == "module.scaffold"));
        assert!(prose.contains("agent"));
    }

    #[test]
    fn explain_module_does_not_delegate() {
        assert!(chat_delegate_agent_spec(
            "c'est quoi un module",
            "Un module est un package.",
            false,
            ASPECT,
        )
        .is_none());
    }

    #[test]
    fn model_scaffold_action_delegates() {
        let out = r#"{"action":"module.scaffold","args":{"name":"ping"}}"#;
        let spec = chat_delegate_agent_spec("fais un ping", out, false, ASPECT);
        let (_brief, _skills, tools, _) = spec.expect("doit déléguer");
        assert!(tools.iter().any(|x| x == "module.scaffold"));
    }

    #[test]
    fn tts_ask_does_not_delegate_agent() {
        let out = r#"{"action":"agent.spawn","args":{"brief":"tts"}}"#;
        assert!(chat_delegate_agent_spec("génère un audio qui dit bonjour", out, false, ASPECT).is_none());
        let (_skills, tools) = chat_agent_kit("génère un audio de bonjour");
        assert!(tools.iter().any(|t| t == "media.audio.generate"));
    }

    #[test]
    fn draw_request_delegates_with_image_tools_when_canvas_closed() {
        let spec = chat_delegate_agent_spec("dessine une maison", "Ok.", false, ASPECT);
        let (_brief, _skills, tools, _prose) = spec.expect("doit déléguer image");
        assert!(tools.iter().any(|x| x == "media.image.generate"));
        assert!(!tools.iter().any(|x| x == "canvas.stroke"));
    }

    #[test]
    fn draw_request_delegates_with_canvas_tools_when_canvas_open() {
        let spec = chat_delegate_agent_spec("dessine une maison", "Ok.", true, ASPECT)
            .expect("canvas ouvert + dessine doit déléguer canvas");
        let (_brief, _skills, tools, _prose) = spec;
        assert!(tools.iter().any(|x| x == "canvas.stroke"));
        assert!(!tools.iter().any(|x| x == "media.image.generate"));
        assert!(!tools.iter().any(|x| x == "user.ask"));
        assert!(!tools.iter().any(|x| x == "agent.spawn"));
        assert!(!tools.iter().any(|x| x == "agent.await"));
        assert!(!tools.iter().any(|x| x == "canvas.fill"));
    }

    #[test]
    fn explicit_canvas_delegates_with_canvas_tools() {
        let spec = chat_delegate_agent_spec("dessine sur le canvas une maison", "Ok.", false, ASPECT);
        let (brief, skills, tools, prose) = spec.expect("doit déléguer canvas");
        assert_eq!(brief, "dessine sur le canvas une maison");
        assert!(!brief.contains("toit + murs"));
        assert!(tools.iter().any(|x| x == "canvas.stroke"));
        assert!(!tools.iter().any(|x| x == "media.image.generate"));
        assert!(!tools.iter().any(|x| x == "user.ask"));
        assert!(!tools.iter().any(|x| x == "agent.spawn"));
        assert!(!skills.iter().any(|s| s == "planner"));
        assert!(prose.to_lowercase().contains("canvas") || prose.contains("dessin"));
    }

    #[test]
    fn canvas_delegate_brief_is_user_goal_not_designer_guide() {
        let spec = chat_delegate_agent_spec(
            "dessine une canette Coca-Cola sur le canvas",
            "Ok.",
            false,
            ASPECT,
        )
        .expect("canvas delegate");
        let (brief, _skills, tools, _) = spec;
        assert!(tools.iter().any(|x| x.starts_with("canvas.")));
        assert_eq!(brief, "dessine une canette Coca-Cola sur le canvas");
        assert!(!brief.contains("Exemple si le sujet est une maison"));
        assert!(!brief.contains("canvas.set_style"));
    }

    #[test]
    fn dans_le_canvas_delegates_with_canvas_tools() {
        let spec = chat_delegate_agent_spec("dessine dans le canvas", "Ok.", false, ASPECT)
            .expect("dessine dans le canvas doit déléguer canvas");
        let (_brief, _skills, tools, _prose) = spec;
        assert!(tools.iter().any(|x| x == "canvas.stroke"));
        assert!(!tools.iter().any(|x| x == "media.image.generate"));
        assert!(!tools.iter().any(|x| x == "user.ask"));
    }

    #[test]
    fn bare_dessine_delegates_with_image_tools() {
        let spec = chat_delegate_agent_spec("dessine une maison", "Ok.", false, ASPECT)
            .expect("dessine une maison doit déléguer image");
        let (_brief, _skills, tools, _prose) = spec;
        assert!(tools.iter().any(|x| x == "media.image.generate"));
        assert!(!tools.iter().any(|x| x == "canvas.stroke"));
    }

    #[test]
    fn canvas_followup_when_open_does_not_delegate() {
        assert!(chat_delegate_agent_spec(
            "essai encore en ajoutant plus de détails",
            "D'accord.",
            true,
            ASPECT,
        )
        .is_none());
        assert!(chat_delegate_agent_spec("vas y", "Ok.", true, ASPECT).is_none());
    }

    #[test]
    fn canvas_truncated_spawn_explicit_canvas_delegates() {
        let out = r#"{"action":"agent.spawn","args":{"brief":"Génération d'une maison médiévale avec plus de détails en cours..."#;
        let spec = chat_delegate_agent_spec("dessine sur le canvas", out, false, ASPECT);
        let (_brief, _skills, tools, _) = spec.expect("JSON tronqué + explicit canvas");
        assert!(tools.iter().any(|x| x == "canvas.stroke"));
    }

    #[test]
    fn canvas_truncated_spawn_followup_does_not_delegate() {
        let out = r#"{"action":"agent.spawn","args":{"brief":"Génération..."#;
        assert!(chat_delegate_agent_spec("vas y", out, true, ASPECT).is_none());
    }

    #[test]
    fn explicit_canvas_after_image_delegate_gets_canvas_tools() {
        let image = chat_delegate_agent_spec("dessine une maison", "Ok.", false, ASPECT)
            .expect("image delegate");
        let image_tools = image.2;
        assert!(image_tools.iter().any(|x| x == "media.image.generate"));
        assert!(!image_tools.iter().any(|x| x == "canvas.stroke"));

        let canvas = chat_delegate_agent_spec("dessine sur le canvas", "Ok.", false, ASPECT)
            .expect("canvas delegate after image");
        let canvas_tools = canvas.2;
        assert!(canvas_tools.iter().any(|x| x == "canvas.stroke"));
        assert!(!canvas_tools.iter().any(|x| x == "media.image.generate"));
    }

    #[test]
    fn prompts_never_mention_canvas_draw() {
        let brief = chat_canvas::canvas_agent_brief("dessine sur le canvas", ASPECT);
        assert!(!brief.contains("canvas.draw"));
        assert!(brief.contains("canvas.stroke"));
        assert!(brief.contains("carré 1:1"));
        assert!(brief.contains("jamais canvas.clear"));
        assert!(!brief.contains("canvas.*"));
        assert!(!aos_proto::CHAT_DELEGATION_PROMPT.contains("canvas.draw"));
    }

    #[test]
    fn canvas_followup_without_open_does_not_delegate() {
        assert!(chat_delegate_agent_spec(
            "essai encore en ajoutant plus de détails",
            "D'accord.",
            false,
            ASPECT,
        )
        .is_none());
    }
}
