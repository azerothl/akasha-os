//! Akasha OS Preview — UI egui (ADR 0003).
//!
//! Surface testeur : chat, dashboard, onboarding, notes, confirm, agents,
//! audit, scénarios guidés, retours (`feedback.submit`).

mod agent_act_phrase;
mod agent_controller;
mod agent_event_controller;
mod agent_panel;
mod agent_ui_state;
mod canvas_event_controller;
mod canvas_paint;
mod chat_ask;
mod chat_bubble;
mod chat_canvas;
mod chat_composer_state;
mod chat_controller;
mod chat_delegate;
mod chat_error_copy;
mod chat_event_controller;
mod chat_load_fail;
mod chat_media;
mod chat_room;
mod chat_runtime_state;
mod chat_sidebar_state;
mod chat_state;
mod chat_view_state;
mod cmd;
mod composer_layout;
mod confirmation_ui_state;
mod decl_ui;
mod deep_plan_ui;
mod feedback_event_controller;
mod feedback_ui_state;
mod guide;
mod i18n;
mod icons;
mod image_composition;
mod image_history;
mod image_prompt;
mod image_studio;
mod library_panel;
mod media_event_controller;
mod memory_controller;
mod memory_ui_state;
mod model_setup;
mod models_controller;
mod models_page;
mod models_ui_state;
mod module_actions;
mod module_event_controller;
mod nav;
mod notes_panel;
mod onboarding;
mod os_open;
mod prefs;
mod product_context;
mod research_choice;
mod research_controller;
mod research_document;
mod research_ui_state;
mod runtime;
mod scenario_ui_state;
mod scenarios_panel;
mod schedule_act_phrase;
mod schedule_card;
mod schedule_event_controller;
mod schedule_ui_state;
mod security_ui_state;
mod session_chat;
mod session_event_controller;
mod session_nav;
mod settings_controller;
mod settings_ui_state;
mod skill_offer;
mod slash;
mod tasks_panel;
mod theme;
mod troubleshoot;
mod ui_agents;
mod ui_chat;
mod ui_chat_composer;
mod ui_chat_session;
mod ui_chat_sidebar;
mod ui_chat_transcript;
mod ui_chat_workspace;
mod ui_decl_module;
mod ui_feedback;
mod ui_format;
mod ui_memory;
mod ui_models;
mod ui_providers;
mod ui_scenarios;
mod ui_security;
mod ui_settings;
mod ui_workspace;
mod workspace_controller;
mod workspace_ui_state;

use aos_agent::schedule::ScheduleEntry;
use aos_agent::schedule_parse::ParsedSchedule;
use aos_ipc::BusClient;
use aos_proto::{
    AgentInfo, AgentState, ChatAttachment, ChatSessionGetResponse, ChatSessionIdRequest,
    SystemMetrics,
};
use chat_ask::pending_ask_ids;
#[cfg(test)]
use chat_bubble::chat_bubble_max_width;
#[cfg(test)]
use chat_delegate::chat_delegate_kit;
use chat_delegate::{
    chat_agent_kit, chat_delegate_agent_spec, session_has_running_canvas_agent,
    spawn_chat_delegate_agent, spawn_document_prep_agent,
};
use cmd::{ChatLine, Cmd, Evt};
#[cfg(test)]
use composer_layout::{chat_composer_wraps, COMPOSER_INPUT_ROW_H};
use composer_layout::{estimate_composer_buttons_w, COMPOSER_MIN_INPUT_W};
use eframe::egui;
use egui_commonmark::CommonMarkCache;
use module_actions::{
    agent_id_cmd, invoke_module_bind, invoke_module_tool, invoke_notes, invoke_tasks,
    load_module_ui,
};
use onboarding::{load_onboarding, save_onboarding, OnboardingState};
use os_open::{aos_home, app_icon, bin_aos_session, open_in_browser};
use prefs::{load_preferences, save_preferences, Preferences};
use runtime::runtime_main;
use serde::{Deserialize, Serialize};
use slash::SLASH_COMMANDS;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use troubleshoot::run_troubleshoot;
use ui_format::{
    chrono_like_stamp, format_local_time_hm, format_schedule_next_label, local_tz_offset_minutes,
    memory_relation_lines, now_ms,
};
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
    Library,
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

/// Vertical scroll that fills the remaining panel height.
fn overflow_scroll(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let h = ui.available_height().max(1.0);
    egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .max_height(h)
        .show(ui, add_contents);
}

/// Vertical scroll with an explicit height budget (nested regions).
fn overflow_scroll_h(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    height: f32,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .max_height(height.max(1.0))
        .show(ui, add_contents);
}

fn designer_shot_mode() -> bool {
    matches!(
        std::env::var("AOS_DESIGNER_SHOT").ok().as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

fn main() -> eframe::Result<()> {
    if std::env::var_os("AOS_MODEL_SETUP").is_some() {
        return model_setup::run();
    }

    let (cmd_tx, cmd_rx) = channel::<Cmd>();
    let (evt_tx, evt_rx) = channel::<Evt>();
    let version =
        std::env::var("AOS_PREVIEW_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size(preview_min_inner_size())
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

fn agent_canvas_session_ops<'a>(
    ag: &AgentInfo,
    active_session: Option<&str>,
    canvas_ops: &'a [aos_proto::CanvasOp],
) -> Option<&'a [aos_proto::CanvasOp]> {
    let sid = ag.session_id.as_deref()?;
    if active_session == Some(sid) && !canvas_ops.is_empty() {
        Some(canvas_ops)
    } else {
        None
    }
}

fn agent_completion_chat_text(
    ag: &AgentInfo,
    t: &i18n::UiStrings,
    session_ops: Option<&[aos_proto::CanvasOp]>,
    trace: Option<&aos_proto::AgentTrace>,
    pending_note_agent: bool,
    notes_count: usize,
) -> String {
    let title = ag.display_title();
    if agent_panel::notes_create_fail_chrome(Some(ag), pending_note_agent, notes_count) {
        return t.notes_create_failed.to_string();
    }
    if agent_panel::canvas_draw_failure_muted(Some(ag), session_ops, trace) {
        return String::new();
    }
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
        AgentState::Failed => {
            if agent_panel::canvas_draw_fail_chrome(Some(ag), session_ops, trace) {
                return t.canvas_draw_failed.to_string();
            }
            if ag.fail_reason.as_deref() == Some(aos_agent::actions::THREAD_FAIL_COULD_NOT_ACT) {
                return i18n::agent_could_not_act_message(t);
            }
            if ag.fail_reason.as_deref() == Some(aos_agent::actions::THREAD_FAIL_COULD_NOT_CONTINUE)
                || ag
                    .fail_reason
                    .as_deref()
                    .is_some_and(aos_agent::context_budget::is_overflow_fail_reason)
            {
                return i18n::agent_could_not_continue_message(t);
            }
            format!(
                "Agent « {title} » a échoué : {}",
                i18n::resolve_agent_fail_reason(t, ag.fail_reason.as_deref())
            )
        }
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
                        speaker_name: m.speaker_name,
                        thinking: m.thinking,
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

pub(crate) async fn announce_and_load_session(
    bus: &Arc<BusClient>,
    evt_tx: &Sender<Evt>,
    id: &str,
) {
    let _ = evt_tx.send(Evt::SessionLoadIntent { id: id.to_string() });
    load_session(bus, evt_tx, id).await;
}

/// Kit des agents lancés depuis le chat : plan, notes, sous-agents — toujours.
const CHAT_AGENT_MIN_STEPS: u32 = 64;
pub(crate) const CHAT_AGENT_MAX_SUBAGENTS: u32 = 8;

fn chat_agent_max_steps(prefs_max: u32) -> u32 {
    prefs_max.clamp(CHAT_AGENT_MIN_STEPS, 128)
}

fn session_model_supports_vision(model_id: Option<&str>) -> bool {
    let Some(id) = model_id else {
        return false;
    };
    models_page::load_catalog_models()
        .iter()
        .any(|m| m.id == id && m.profiles.iter().any(|p| p == "vision"))
}

/// Legacy constant — canvas toolbar height is now dynamic via `chat_canvas::toolbar_max_height`.
/// Minimum inner window size (width × height) for a usable Preview layout.
fn preview_min_inner_size() -> [f32; 2] {
    let fr = i18n::strings("fr");
    [preview_min_inner_width(&fr), 600.0]
}

const LEFT_NAV_W: f32 = 88.0;
const CHAT_SIDE_MIN_W: f32 = 120.0;
const CHAT_SPLIT_GAP: f32 = 8.0;
const CHAT_MAIN_MARGIN: f32 = 16.0;

fn estimate_label_chip_w(label: &str) -> f32 {
    const CHAR_W: f32 = 8.5;
    const PAD: f32 = 22.0;
    label.len() as f32 * CHAR_W + PAD
}

fn session_toggle_reserve_width(t: &i18n::UiStrings, canvas_open: bool) -> f32 {
    let mut w = estimate_label_chip_w(t.session_toggle_salon)
        + 6.0
        + estimate_label_chip_w(t.session_toggle_canvas)
        + 6.0
        + estimate_label_chip_w(t.session_toggle_deep)
        + 6.0
        + icons::SESSION_ICON_SZ;
    if canvas_open {
        w += 6.0 + estimate_label_chip_w(t.session_focus);
    }
    w
}

fn session_toggle_chip(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    let w = estimate_label_chip_w(label);
    ui.add_sized(
        egui::vec2(w, ui.spacing().interact_size.y),
        egui::SelectableLabel::new(selected, egui::RichText::new(label)),
    )
}

fn composer_row_reserved_width(t: &i18n::UiStrings, show_stop: bool) -> f32 {
    icons::ATTACH_BTN_W + 4.0 + estimate_composer_buttons_w(t.agent_send, show_stop, t.chat_stop)
}

/// Minimum central chat pane width so composer + session toggles fit (FR labels).
fn preview_min_inner_width(t: &i18n::UiStrings) -> f32 {
    let composer = composer_row_reserved_width(t, true) + COMPOSER_MIN_INPUT_W;
    let session_bar = session_toggle_reserve_width(t, true) + 160.0;
    let canvas_split_min = 180.0 + CHAT_SPLIT_GAP + 200.0;
    let main_min = composer.max(session_bar).max(canvas_split_min);
    LEFT_NAV_W + CHAT_SIDE_MIN_W + CHAT_SPLIT_GAP + main_min + CHAT_MAIN_MARGIN
}

struct UiApp {
    cmd_tx: Sender<Cmd>,
    evt_rx: Receiver<Evt>,
    version: String,
    tab: Tab,
    chat: Vec<ChatLine>,
    chat_state: chat_state::ChatState,
    network_online: bool,
    prefs: Preferences,
    memory_ui: memory_ui_state::MemoryUiState,
    settings_ui: settings_ui_state::SettingsUiState,
    metrics: Option<SystemMetrics>,
    /// Live agent roster (shared with chat / ask / room membership).
    agents: Vec<AgentInfo>,
    confirmations_ui: confirmation_ui_state::ConfirmationUiState,
    workspace_ui: workspace_ui_state::WorkspaceUiState,
    /// User-initiated session navigation intent for the next cross-session load.
    pending_session_nav: session_nav::PendingSessionNav,
    schedule_ui: schedule_ui_state::ScheduleUiState,
    agent_ui: agent_ui_state::AgentUiState,
    security_ui: security_ui_state::SecurityUiState,
    status: String,
    onboarding: OnboardingState,
    show_onboarding: bool,
    scenario_ui: scenario_ui_state::ScenarioUiState,
    feedback_ui: feedback_ui_state::FeedbackUiState,
    chat_md_cache: CommonMarkCache,
    update_download_child: Option<std::process::Child>,
    update_status: String,
    models_ui: models_ui_state::ModelsUiState,
    decl_panels: HashMap<String, decl_ui::DeclUiPanelState>,
    decl_md_cache: CommonMarkCache,
    image_studio: image_studio::ImageStudioState,
    image_generating: Option<image_studio::ImageGenUiState>,
    show_go_to_palette: bool,
    guide: guide::GuideState,
    research_ui: research_ui_state::ResearchUiState,
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
    ("web", &["web.search", "web.browse", "net.fetch"]),
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
    ("agents", &["agent.spawn", "agent.await", "plan.update"]),
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
                if ui.checkbox(&mut on, label).on_hover_text(*name).changed() {
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
        let _ = cmd_tx.send(Cmd::SkillPassPending);
        let _ = cmd_tx.send(Cmd::CatalogueRefresh);
        let _ = cmd_tx.send(Cmd::ModuleList);
        if prefs.community_catalogue_enabled {
            let _ = cmd_tx.send(Cmd::CatalogueSetSource { enabled: true });
        }
        let _ = cmd_tx.send(Cmd::SetRouting {
            mode: prefs.routing.clone(),
        });
        let _ = cmd_tx.send(Cmd::ProviderList);
        if prefs.network_online {
            let _ = cmd_tx.send(Cmd::NetSetMode { online: true });
        }
        let model_updates_msg =
            std::fs::read_to_string(aos_home().join("var/run/model_updates.json"))
                .ok()
                .and_then(|s| {
                    serde_json::from_str::<serde_json::Value>(&s)
                        .ok()
                        .and_then(|v| {
                            v.get("reason")
                                .and_then(|r| r.as_str())
                                .map(|s| s.to_string())
                        })
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
            t.chat_thread_intro.replace("{}", &version)
        );
        Self {
            cmd_tx,
            evt_rx,
            version,
            tab: Tab::Chat,
            chat: vec![ChatLine::plain("système", intro)],
            chat_state: chat_state::ChatState::default(),
            network_online,
            prefs,
            memory_ui: memory_ui_state::MemoryUiState::default(),
            settings_ui: settings_ui_state::SettingsUiState::default(),
            metrics: None,
            agents: Vec::new(),
            confirmations_ui: confirmation_ui_state::ConfirmationUiState::default(),
            workspace_ui: workspace_ui_state::WorkspaceUiState::default(),
            pending_session_nav: session_nav::PendingSessionNav::None,
            schedule_ui: schedule_ui_state::ScheduleUiState::default(),
            agent_ui: agent_ui_state::AgentUiState::with_create_defaults(
                agent_max_steps,
                agent_timeout_secs,
                default_model,
            ),
            security_ui: security_ui_state::SecurityUiState::default(),
            status: String::new(),
            onboarding,
            show_onboarding,
            scenario_ui: scenario_ui_state::ScenarioUiState::default(),
            feedback_ui: feedback_ui_state::FeedbackUiState::default(),
            chat_md_cache: CommonMarkCache::default(),
            update_download_child: None,
            update_status: String::new(),
            models_ui: models_ui_state::ModelsUiState::with_updates_msg(model_updates_msg),
            decl_panels: HashMap::new(),
            decl_md_cache: CommonMarkCache::default(),
            image_studio: image_studio::ImageStudioState::default(),
            image_generating: None,
            show_go_to_palette: false,
            guide: guide::GuideState::default(),
            research_ui: research_ui_state::ResearchUiState::default(),
        }
    }

    fn blocked_ask_ids(&self) -> Vec<String> {
        let Some(sid) = self.chat_state.active_session.as_deref() else {
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
            .agent_ui
            .ask_reply_target
            .as_ref()
            .filter(|t| queue.iter().any(|x| x == *t))
            .cloned()
            .or_else(|| queue.first().cloned())?;
        self.agents.iter().find(|a| a.agent_id == chosen)
    }

    fn set_canvas_open_local(&mut self, session_id: &str, open: bool) {
        if let Some(s) = self
            .chat_state
            .sessions
            .iter_mut()
            .find(|s| s.id == session_id)
        {
            s.canvas_open = open;
        }
    }

    /// Ouvre le panneau canvas (optimiste côté UI, puis bus).
    fn open_canvas_face(&mut self, session_id: &str) {
        let already = self
            .chat_state
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
                speaker_name: None,
                thinking: None,
            });
            let _ = self.cmd_tx.send(Cmd::AgentKill { id: agent_id });
        }
        self.agent_ui.clear_ask_reply_if_any(&blocked_ids);
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
        self.scenario_ui
            .arm_module_agent(self.settings_ui.installed_module_names());
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
            model_id: self.agent_ui.create_model_id(),
            session_id: self.chat_state.active_session.clone(),
            origin: "form".into(),
            join_active_room: false,
            library: false,
        });
        self.tab = Tab::Agents;
        self.status = t.scen_module_agent_launched.into();
    }

    fn send_ask_reply(
        &mut self,
        session_id: String,
        agent_id: String,
        title: String,
        text: String,
    ) {
        self.chat.push(ChatLine {
            role: "user".into(),
            text: text.clone(),
            attachments: vec![ChatAttachment::AgentRef {
                agent_id: agent_id.clone(),
                title: title.clone(),
                origin: "ask-reply".into(),
            }],
            speaker_id: None,
            speaker_name: None,
            thinking: None,
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
        self.agent_ui.clear_ask_reply_if(&agent_id);
        self.status = "réponse envoyée à l'agent".into();
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
        if !self.show_onboarding || self.onboarding.tutorial_step != 1 || !self.onboarding.chat_sent
        {
            return;
        }
        self.onboarding.first_chat_done = true;
        save_onboarding(&self.onboarding);
        if onboarding::chat_step_can_advance(
            self.onboarding.chat_sent,
            self.onboarding.first_chat_done,
        ) {
            self.onboarding.tutorial_step = 2;
            save_onboarding(&self.onboarding);
            if let Some(id) = self.chat_state.active_session.as_deref() {
                let holder = format!("session:{id}");
                self.security_ui.select_holder(holder.clone());
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
                .replace("{n}", &self.security_ui.caps.len().to_string()),
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
                    self.chat
                        .push(ChatLine::plain("système", "usage : /notesearch <requête>"));
                    return;
                }
                let _ = self.cmd_tx.send(Cmd::NotesSearch {
                    query: rest.to_string(),
                });
                self.tab = Tab::Notes;
            }
            "/agent" => {
                if rest.is_empty() {
                    self.chat
                        .push(ChatLine::plain("système", "usage : /agent <tâche>"));
                    return;
                }
                let Some(session_id) = self.chat_state.active_session.clone() else {
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
                if rest.to_lowercase().contains("note") {
                    self.scenario_ui.mark_note_agent_pending();
                }
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
                let t = i18n::strings(&self.prefs.language);
                self.status = t.status_image_generating.into();
                if let Some(sid) = self.chat_state.active_session.clone() {
                    let _ = self.cmd_tx.send(Cmd::SessionAppend {
                        session_id: sid,
                        role: "user".into(),
                        content: text.to_string(),
                        attachments: vec![],
                    });
                }
                let enrich =
                    image_prompt::default_enrich_prompt(self.prefs.default_image_model.as_deref());
                let _ = self.cmd_tx.send(Cmd::MediaImage {
                    prompt: rest.to_string(),
                    model_id: self.prefs.default_image_model.clone(),
                    options: image_studio::image_options_for_model(
                        self.prefs.default_image_model.as_deref(),
                        Some("balanced"),
                    ),
                    output_path: None,
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
                let t = i18n::strings(&self.prefs.language);
                let Some(sid) = self.chat_state.active_session.clone() else {
                    self.chat
                        .push(ChatLine::plain("système", t.canvas_no_active_session));
                    return;
                };
                let open =
                    chat_room::active_session_meta(&self.chat_state.sessions, Some(sid.as_str()))
                        .map(|m| m.canvas_open)
                        .unwrap_or(false);
                let new_open = !open;
                self.set_canvas_open_local(&sid, new_open);
                let _ = self.cmd_tx.send(Cmd::CanvasSetOpen {
                    session_id: sid.clone(),
                    open: new_open,
                });
                if new_open {
                    let _ = self.cmd_tx.send(Cmd::CanvasPoll {
                        session_id: sid,
                        after_seq: None,
                    });
                }
            }
            _ => {
                self.chat.push(ChatLine::plain(
                    "système",
                    format!("commande inconnue : {cmd} — tapez /commands"),
                ));
            }
        }
    }

    fn chat_has_skill_offer(&self, pattern_id: &str) -> bool {
        self.chat.iter().any(|line| {
            line.attachments.iter().any(|att| {
                matches!(
                    att,
                    ChatAttachment::SkillOffer { pattern_id: pid, .. } if pid == pattern_id
                )
            })
        })
    }

    fn offer_skill_card(&mut self, offer: &aos_proto::SkillPassPendingOffer) {
        if self.chat_has_skill_offer(&offer.pattern_id) {
            return;
        }
        let att = ChatAttachment::SkillOffer {
            pattern_id: offer.pattern_id.clone(),
            label_en: offer.label_en.clone(),
            label_fr: offer.label_fr.clone(),
            state: "pending".into(),
        };
        self.chat.push(ChatLine {
            role: "assistant".into(),
            text: String::new(),
            attachments: vec![att.clone()],
            speaker_id: None,
            speaker_name: None,
            thinking: None,
        });
        if let Some(sid) = self.chat_state.active_session.clone() {
            let _ = self.cmd_tx.send(Cmd::SessionAppend {
                session_id: sid,
                role: "assistant".into(),
                content: String::new(),
                attachments: vec![att],
            });
        }
    }

    fn update_skill_offer_state(&mut self, pattern_id: &str, state: &str) {
        for line in &mut self.chat {
            for att in &mut line.attachments {
                if let ChatAttachment::SkillOffer {
                    pattern_id: pid,
                    state: st,
                    ..
                } = att
                {
                    if pid == pattern_id {
                        *st = state.to_string();
                    }
                }
            }
        }
    }

    fn handle_schedule_phrase(
        &mut self,
        session_id: &str,
        user_phrase: &str,
        parsed: ParsedSchedule,
    ) {
        let t = i18n::strings(&self.prefs.language);
        let act_text =
            schedule_act_phrase::act_phrase_from_parsed(&t, &parsed, &self.prefs.language);
        self.chat.push(ChatLine {
            role: "user".into(),
            text: user_phrase.to_string(),
            attachments: vec![],
            speaker_id: None,
            speaker_name: None,
            thinking: None,
        });
        let _ = self.cmd_tx.send(Cmd::SessionAppend {
            session_id: session_id.to_string(),
            role: "user".into(),
            content: user_phrase.to_string(),
            attachments: vec![],
        });
        let gate_ask = !self
            .prefs
            .agent_gate_mode
            .eq_ignore_ascii_case("autonomous");
        if gate_ask {
            let act_id = format!("sched-act-{}", chrono_like_stamp());
            let att = ChatAttachment::ScheduleAct {
                act_id: act_id.clone(),
                display_phrase: user_phrase.to_string(),
                goal: parsed.goal.clone(),
                when_label: parsed.when_label.clone(),
                interval_secs: parsed.interval_secs,
                next_fire_ms: parsed.next_fire_ms,
                state: "pending".into(),
                schedule_id: String::new(),
            };
            self.chat.push(ChatLine {
                role: "assistant".into(),
                text: act_text.clone(),
                attachments: vec![att.clone()],
                speaker_id: None,
                speaker_name: None,
                thinking: None,
            });
            let _ = self.cmd_tx.send(Cmd::SessionAppend {
                session_id: session_id.to_string(),
                role: "assistant".into(),
                content: act_text,
                attachments: vec![att],
            });
        } else {
            self.schedule_ui
                .set_pending_card_act(format!("sched-auto-{}", chrono_like_stamp()));
            self.chat.push(ChatLine {
                role: "assistant".into(),
                text: schedule_act_phrase::format_resolved_act(
                    &t,
                    &parsed.goal,
                    &parsed.when_label,
                    true,
                    &self.prefs.language,
                ),
                attachments: vec![],
                speaker_id: None,
                speaker_name: None,
                thinking: None,
            });
            let _ = self.cmd_tx.send(Cmd::ScheduleCreate {
                goal: parsed.goal,
                interval_secs: parsed.interval_secs,
                next_fire_ms: Some(parsed.next_fire_ms),
                display_title: Some(user_phrase.to_string()),
            });
        }
        self.mark_onboarding_chat_sent();
        self.scenario_ui.chat = true;
    }

    fn approve_schedule_act(&mut self, act_id: &str, msg_idx: usize) {
        let Some(att_idx) = self.chat[msg_idx].attachments.iter().position(|a| {
            matches!(
                a,
                ChatAttachment::ScheduleAct { act_id: id, .. } if id == act_id
            )
        }) else {
            return;
        };
        let ChatAttachment::ScheduleAct {
            goal,
            when_label,
            interval_secs,
            next_fire_ms,
            display_phrase,
            ..
        } = self.chat[msg_idx].attachments[att_idx].clone()
        else {
            return;
        };
        let t = i18n::strings(&self.prefs.language);
        self.chat[msg_idx].text = schedule_act_phrase::format_resolved_act(
            &t,
            &goal,
            &when_label,
            true,
            &self.prefs.language,
        );
        if let ChatAttachment::ScheduleAct { state, .. } =
            &mut self.chat[msg_idx].attachments[att_idx]
        {
            *state = "approved".into();
        }
        self.schedule_ui.mark_transcript_dirty();
        if let Some(session_id) = self.chat_state.active_session.clone() {
            let att = self.chat[msg_idx].attachments[att_idx].clone();
            let _ = self.cmd_tx.send(Cmd::SessionAppend {
                session_id,
                role: "assistant".into(),
                content: self.chat[msg_idx].text.clone(),
                attachments: vec![att],
            });
        }
        self.schedule_ui.set_pending_card_act(act_id.to_string());
        let _ = self.cmd_tx.send(Cmd::ScheduleCreate {
            goal,
            interval_secs,
            next_fire_ms: Some(next_fire_ms),
            display_title: Some(display_phrase),
        });
    }

    fn deny_schedule_act(&mut self, act_id: &str, msg_idx: usize) {
        let Some(att_idx) = self.chat[msg_idx].attachments.iter().position(|a| {
            matches!(
                a,
                ChatAttachment::ScheduleAct { act_id: id, .. } if id == act_id
            )
        }) else {
            return;
        };
        let ChatAttachment::ScheduleAct {
            goal, when_label, ..
        } = self.chat[msg_idx].attachments[att_idx].clone()
        else {
            return;
        };
        let t = i18n::strings(&self.prefs.language);
        self.chat[msg_idx].text = schedule_act_phrase::format_resolved_act(
            &t,
            &goal,
            &when_label,
            false,
            &self.prefs.language,
        );
        if let ChatAttachment::ScheduleAct { state, .. } =
            &mut self.chat[msg_idx].attachments[att_idx]
        {
            *state = "denied".into();
        }
        self.schedule_ui.mark_transcript_dirty();
        if let Some(session_id) = self.chat_state.active_session.clone() {
            let att = self.chat[msg_idx].attachments[att_idx].clone();
            let _ = self.cmd_tx.send(Cmd::SessionAppend {
                session_id,
                role: "assistant".into(),
                content: self.chat[msg_idx].text.clone(),
                attachments: vec![att],
            });
        }
    }

    fn attach_schedule_card(&mut self, entry: &ScheduleEntry) {
        let act_id = match self.schedule_ui.take_pending_card_act() {
            Some(id) => id,
            None => return,
        };
        let title = entry
            .display_title
            .clone()
            .unwrap_or_else(|| entry.goal.clone());
        let next_fire = schedule_card::next_fire_ms_for_entry(entry, now_ms());
        let state = schedule_card::card_state_from_entry(entry).to_string();
        let card = ChatAttachment::ScheduleCard {
            schedule_id: entry.id.clone(),
            title,
            goal: entry.goal.clone(),
            interval_secs: entry.interval_secs,
            next_fire_ms: next_fire,
            state,
        };
        for line in &mut self.chat {
            for att in &mut line.attachments {
                if let ChatAttachment::ScheduleAct {
                    act_id: id,
                    state,
                    schedule_id,
                    ..
                } = att
                {
                    if *id == act_id && state == "approved" {
                        *schedule_id = entry.id.clone();
                        line.attachments.push(card.clone());
                        self.schedule_ui.mark_transcript_dirty();
                        if let Some(session_id) = self.chat_state.active_session.clone() {
                            let _ = self.cmd_tx.send(Cmd::SessionAppend {
                                session_id,
                                role: "assistant".into(),
                                content: String::new(),
                                attachments: vec![card],
                            });
                        }
                        return;
                    }
                }
            }
        }
        self.schedule_ui.mark_transcript_dirty();
        self.chat.push(ChatLine {
            role: "assistant".into(),
            text: String::new(),
            attachments: vec![card.clone()],
            speaker_id: None,
            speaker_name: None,
            thinking: None,
        });
        if let Some(session_id) = self.chat_state.active_session.clone() {
            let _ = self.cmd_tx.send(Cmd::SessionAppend {
                session_id,
                role: "assistant".into(),
                content: String::new(),
                attachments: vec![card],
            });
        }
    }

    fn request_session_select(&mut self, id: String) {
        self.pending_session_nav = session_nav::PendingSessionNav::Explicit(id.clone());
        self.schedule_ui.clear_transcript_dirty();
        let _ = self.cmd_tx.send(Cmd::SessionSelect { id });
    }

    fn request_session_create(&mut self, title: Option<String>) {
        self.pending_session_nav = session_nav::PendingSessionNav::AwaitingCreate;
        self.schedule_ui.clear_transcript_dirty();
        let _ = self.cmd_tx.send(Cmd::SessionCreate { title });
    }

    fn sync_schedule_cards(&mut self) {
        let now = now_ms();
        for line in &mut self.chat {
            for att in &mut line.attachments {
                if let ChatAttachment::ScheduleCard { schedule_id, .. } = att {
                    if let Some(entry) = self
                        .schedule_ui
                        .entries
                        .iter()
                        .find(|s| s.id == *schedule_id)
                    {
                        schedule_card::sync_card_attachment(att, entry, now);
                    }
                }
            }
        }
    }

    fn upsert_schedule_entry(&mut self, entry: ScheduleEntry) {
        self.schedule_ui.upsert_entry(entry);
    }

    fn apply_schedule_card_action_local(&mut self, action: schedule_card::ScheduleCardAction) {
        let now = now_ms();
        match &action {
            schedule_card::ScheduleCardAction::Pause(id) => {
                schedule_card::apply_local_pause(&mut self.schedule_ui.entries, id);
            }
            schedule_card::ScheduleCardAction::Resume(id) => {
                schedule_card::apply_local_resume(&mut self.schedule_ui.entries, id);
            }
            schedule_card::ScheduleCardAction::Stop(id) => {
                schedule_card::apply_local_stop(&mut self.schedule_ui.entries, id);
            }
            schedule_card::ScheduleCardAction::None => return,
        }
        self.schedule_ui.mark_transcript_dirty();
        let id = match action {
            schedule_card::ScheduleCardAction::Pause(id)
            | schedule_card::ScheduleCardAction::Resume(id)
            | schedule_card::ScheduleCardAction::Stop(id) => id,
            schedule_card::ScheduleCardAction::None => return,
        };
        for line in &mut self.chat {
            for att in &mut line.attachments {
                if let ChatAttachment::ScheduleCard { schedule_id, .. } = att {
                    if schedule_id == &id {
                        schedule_card::apply_local_action_to_attachment(
                            att,
                            &self.schedule_ui.entries,
                            &id,
                            now,
                        );
                    }
                }
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
            speaker_name: None,
            thinking: None,
        });
        if let Some(sid) = self.chat_state.active_session.clone() {
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
            self.feedback_ui.result.clear();
        }
        self.tab = tab.clone();
        match tab {
            Tab::Chat => {
                let _ = self.cmd_tx.send(Cmd::SkillPassPending);
            }
            Tab::Providers => {
                let _ = self.cmd_tx.send(Cmd::ProviderList);
            }
            Tab::Audit => {
                let _ = self.cmd_tx.send(Cmd::Audit { last: 40 });
            }
            Tab::Caps if !self.security_ui.caps_holder.is_empty() => {
                let _ = self.cmd_tx.send(Cmd::CapList {
                    holder: self.security_ui.caps_holder.clone(),
                });
            }
            Tab::Notes => {
                let _ = self.cmd_tx.send(Cmd::NotesList);
            }
            Tab::Memory => {
                self.send_mem_list();
                let _ = self.cmd_tx.send(Cmd::MemSweepStatus);
            }
            Tab::Tasks => {
                let _ = self.cmd_tx.send(Cmd::TasksList);
            }
            Tab::Library => {
                let _ = self.cmd_tx.send(Cmd::UserLibraryList);
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
        for (idx, (tab, label, hint)) in primary.into_iter().enumerate() {
            // These glyphs are covered by egui's bundled emoji font (monochrome
            // in egui), unlike arbitrary geometric Unicode symbols.
            let icon = ['🔘', '⛃', '🖼', '🗀'][idx];
            let label_text = if self.prefs.ui_density == prefs::UiDensity::Compact {
                icon.to_string()
            } else {
                format!("{icon} {label}")
            };
            if ui
                .add_sized(
                    egui::vec2(ui.available_width().max(1.0), theme::CONTROL_MIN_H_COMFORTABLE),
                    egui::SelectableLabel::new(self.tab == tab, label_text),
                )
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
                    (Tab::Library, t.tab_library, t.tab_hint_library),
                    (Tab::Tasks, t.tab_tasks, t.tab_hint_tasks),
                    (Tab::Models, t.tab_models, t.tab_hint_models),
                    (Tab::Settings, t.tab_settings, t.tab_hint_settings),
                ] {
                    if ui
                        .selectable_label(self.tab == tab, label)
                        .on_hover_text(hint)
                        .clicked()
                    {
                        self.on_tab_open(tab);
                    }
                }
                if ui
                    .selectable_label(self.research_ui.documents_list.open, t.nav_documents)
                    .on_hover_text(t.documents_list_title)
                    .clicked()
                {
                    self.research_ui.open_documents_list();
                }
                for (tab, label, hint) in [
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
                ui.weak(t.nav_tester);
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
                    .settings_ui
                    .installed_modules
                    .iter()
                    .filter(|m| {
                        aos_proto::decl_ui::sidebar_decl_ui_module(&m.name, m.ui_mode.as_deref())
                    })
                    .map(|m| {
                        (
                            m.name.clone(),
                            m.ui_title.clone().unwrap_or_else(|| m.name.clone()),
                        )
                    })
                    .collect();
                if !decl_mods.is_empty() {
                    ui.separator();
                    ui.weak(t.nav_modules);
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

    fn start_update_download(&mut self) {
        if self.update_download_child.is_some() {
            return;
        }
        let session = bin_aos_session();
        match std::process::Command::new(&session)
            .arg("--download-update")
            .env("AOS_HOME", aos_home())
            .spawn()
        {
            Ok(child) => {
                self.update_download_child = Some(child);
                self.update_status.clear();
            }
            Err(_) => {
                let t = i18n::strings(&self.prefs.language);
                self.update_status = t.status_update_download_failed.into();
            }
        }
    }

    fn poll_update_download(&mut self) {
        let Some(ref mut child) = self.update_download_child else {
            return;
        };
        let t = i18n::strings(&self.prefs.language);
        match child.try_wait() {
            Ok(Some(status)) => {
                self.update_download_child = None;
                if status.success() && load_pending_update_version().is_some() {
                    self.update_status.clear();
                } else if !status.success() {
                    self.update_status = t.status_update_download_failed.into();
                }
            }
            Ok(None) => {}
            Err(_) => {
                self.update_download_child = None;
                self.update_status = t.status_update_download_failed.into();
            }
        }
    }

    fn ui_status_bar(&mut self, ui: &mut egui::Ui, t: &i18n::UiStrings) {
        ui.horizontal(|ui| {
            let net_label = if self.network_online {
                t.status_network_on
            } else {
                t.status_network_off
            };
            let net_response = ui
                .small_button(net_label)
                .on_hover_text("Ouvrir les détails du réseau");
            net_response.context_menu(|ui| {
                ui.label(if self.network_online {
                    "Le réseau est autorisé pour les outils configurés."
                } else {
                    "Le réseau est bloqué par défaut."
                });
                if ui
                    .button(if self.network_online {
                        "Désactiver le réseau"
                    } else {
                        "Autoriser le réseau"
                    })
                    .clicked()
                {
                    self.network_online = !self.network_online;
                    self.prefs.network_online = self.network_online;
                    save_preferences(&self.prefs);
                    let _ = self.cmd_tx.send(Cmd::NetSetMode {
                        online: self.network_online,
                    });
                    ui.close_menu();
                }
            });
            ui.separator();
            if ui
                .small_button(format!(
                    "{}: {}",
                    t.status_model_label,
                    self.status_model_name()
                ))
                .clicked()
            {
                self.on_tab_open(Tab::Models);
            }
            ui.separator();
            let caps_text = if self.security_ui.caps.is_empty() {
                format!("{}: —", t.status_caps_label)
            } else {
                format!(
                    "{}: {}",
                    t.status_caps_label,
                    t.status_caps_count
                        .replace("{n}", &self.security_ui.caps.len().to_string())
                )
            };
            if ui.small_button(caps_text).clicked() {
                self.on_tab_open(Tab::Caps);
            }
            ui.separator();
            let gate_ask = !self
                .prefs
                .agent_gate_mode
                .eq_ignore_ascii_case("autonomous");
            let gate_label = if gate_ask {
                t.status_gate_ask
            } else {
                t.status_gate_autonomous
            };
            let gate_response = ui.small_button(format!("{}: {}", t.status_gate_label, gate_label));
            gate_response.context_menu(|ui| {
                ui.label("Les demandes d’autorisation restent explicites dans la conversation.");
                if ui
                    .button(if gate_ask {
                        "Toujours autoriser"
                    } else {
                        "Demander à chaque fois"
                    })
                    .clicked()
                {
                    self.prefs.agent_gate_mode = if gate_ask {
                        "autonomous".into()
                    } else {
                        "ask".into()
                    };
                    save_preferences(&self.prefs);
                    ui.close_menu();
                }
            });
            ui.separator();
            if let Some(pending_ver) = load_pending_update_version() {
                ui.label(t.status_update_pending.replace("{version}", &pending_ver));
                if let Some(offer) = load_update_offer() {
                    if ui.small_button(t.update_notes).clicked() {
                        open_in_browser(&offer.html_url);
                    }
                }
            } else if self.update_download_child.is_some() {
                let ver = load_update_offer()
                    .map(|o| o.version)
                    .unwrap_or_else(|| "…".into());
                ui.label(t.status_update_downloading.replace("{version}", &ver));
            } else if let Some(offer) = load_update_offer() {
                ui.label(t.update_available.replace("{}", &offer.version));
                if ui.small_button(t.status_update_download).clicked() {
                    self.start_update_download();
                }
                if ui.small_button(t.update_notes).clicked() {
                    open_in_browser(&offer.html_url);
                }
            } else if !self.update_status.is_empty() {
                ui.label(&self.update_status);
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
                let destinations: [(&str, Tab); 14] = [
                    (t.tab_chat, Tab::Chat),
                    (t.tab_agents, Tab::Agents),
                    (t.tab_create, Tab::Image),
                    (t.tab_memory, Tab::Memory),
                    (t.tab_notes, Tab::Notes),
                    (t.tab_library, Tab::Library),
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
        theme::apply_theme(ctx, &self.prefs.theme, &self.prefs.custom_theme);
        theme::apply_ui_scale(ctx, self.prefs.ui_scale_percent);
        theme::apply_ui_density(ctx, self.prefs.ui_density);
        self.handle_keyboard_shortcuts(ctx);
        self.poll_update_download();
        while let Ok(ev) = self.evt_rx.try_recv() {
            match ev {
                Evt::Delta { session_id, text } => {
                    chat_event_controller::on_delta(self, session_id, text);
                }
                Evt::Done {
                    text,
                    session_id,
                    attachments,
                } => {
                    chat_event_controller::on_done(self, text, session_id, attachments);
                }
                Evt::ChatError {
                    session_id,
                    message,
                } => {
                    chat_event_controller::on_chat_error(self, session_id, message);
                }
                Evt::Error(m) => {
                    if chat_event_controller::on_error(self, m) {
                        break;
                    }
                }
                Evt::Status(m) => {
                    chat_event_controller::on_status(self, m);
                }
                Evt::ModelDownloadStarted { model_id } => {
                    self.on_model_download_started(model_id);
                }
                Evt::ModelDownloadProgress {
                    model_id,
                    done_bytes,
                    total_bytes,
                    percent,
                } => {
                    self.on_model_download_progress(model_id, done_bytes, total_bytes, percent);
                }
                Evt::ModelDownloadFinished { model_id } => {
                    self.on_model_download_finished(model_id);
                }
                Evt::ModelDownloadFailed { model_id, error } => {
                    self.on_model_download_failed(model_id, error);
                }
                Evt::MemExtracted { n } => self.on_mem_extracted(n),
                Evt::MemSweepStatus {
                    last_pass_ms,
                    last_pass_label,
                } => self.on_mem_sweep_status(last_pass_ms, last_pass_label),
                Evt::SkillPassPending(offer) => {
                    skill_offer::reconcile_pending_cards(
                        &mut self.chat,
                        offer.as_ref().map(|item| item.pattern_id.as_str()),
                    );
                    if let Some(o) = offer {
                        self.offer_skill_card(&o);
                    }
                }
                Evt::SkillPassCreated { pattern_id, .. } => {
                    self.update_skill_offer_state(&pattern_id, "created");
                }
                Evt::ChatSystem(m) => self.chat.push(ChatLine::plain("système", m)),
                Evt::Metrics(m) => self.metrics = Some(m),
                Evt::AgentSpawned {
                    session_id,
                    agent_id,
                    title,
                    origin,
                    ack,
                } => agent_event_controller::on_spawned(
                    self, session_id, agent_id, title, origin, ack,
                ),
                Evt::Agents(a) => agent_event_controller::on_agents(self, a),
                Evt::Notes(s) => self.on_notes_raw(s),
                Evt::NotesListed(notes) => self.on_notes_listed(notes),
                Evt::NoteLoaded(detail) => self.on_note_loaded(detail),
                Evt::NotesSearchHits(hits) => self.on_notes_search_hits(hits),
                Evt::NotesRelated(hits) => self.on_notes_related(hits),
                Evt::UserLibraryListed(docs) => self.on_user_library_listed(docs),
                Evt::NotesSaved { path, slug, title } => {
                    self.on_notes_saved(path, slug, title);
                }
                Evt::NotesSaveFailed {
                    title,
                    content,
                    path,
                } => {
                    self.on_notes_save_failed(title, content, path);
                }
                Evt::Audit(a) => {
                    self.security_ui.set_audit(a);
                    self.scenario_ui.audit = true;
                }
                Evt::Caps { holder, caps } => {
                    self.security_ui.set_caps(holder, caps);
                }
                Evt::Schedules(entries) => schedule_event_controller::on_schedules(self, entries),
                Evt::ScheduleCreated(entry) => {
                    schedule_event_controller::on_schedule_created(self, entry);
                }
                Evt::ScheduleUpdated(entry) => {
                    schedule_event_controller::on_schedule_updated(self, entry);
                }
                Evt::TasksListed(tasks) => self.on_tasks_listed(tasks),
                Evt::Confirms(c) => self.confirmations_ui.replace(c),
                Evt::FeedbackOk(r) => {
                    feedback_event_controller::on_feedback_ok(self, r);
                }
                Evt::FeedbackDraft(req) => feedback_event_controller::on_feedback_draft(self, req),
                Evt::Sessions(list) => session_event_controller::on_sessions(self, list),
                Evt::SessionMetaUpdated(meta) => {
                    if let Some(existing) = self
                        .chat_state
                        .sessions
                        .iter_mut()
                        .find(|s| s.id == meta.id)
                    {
                        *existing = meta;
                    }
                }
                Evt::SessionLoadIntent { id } => {
                    session_event_controller::on_load_intent(self, id);
                }
                Evt::SessionLoaded { id, messages, meta } => {
                    session_event_controller::on_loaded(self, id, messages, meta);
                }
                Evt::RoomTurnDone {
                    session_id,
                    agent_turns,
                    cancelled,
                } => {
                    session_event_controller::on_room_turn_done(
                        self,
                        session_id,
                        agent_turns,
                        cancelled,
                    );
                }
                Evt::CanvasMeta(meta) => {
                    canvas_event_controller::on_canvas_meta(self, meta);
                }
                Evt::CanvasSnapshot {
                    session_id,
                    canvas_open,
                    next_seq,
                    ops,
                    pen,
                    delta,
                    canvas_seeing,
                    layers,
                    active_layer_id,
                } => {
                    canvas_event_controller::on_canvas_snapshot(
                        self,
                        ctx,
                        canvas_event_controller::CanvasSnapshotEvent {
                            session_id,
                            canvas_open,
                            next_seq,
                            ops,
                            pen,
                            delta,
                            canvas_seeing,
                            layers,
                            active_layer_id,
                        },
                    );
                }
                Evt::CanvasExported { path, session_id } => {
                    canvas_event_controller::on_canvas_exported(self, path, session_id);
                }
                Evt::SessionExported { path, session_id } => {
                    if self.chat_state.active_session.as_deref() == Some(session_id.as_str()) {
                        let t = i18n::strings(&self.prefs.language);
                        self.status = t.session_export_toast.replace("{path}", &path);
                    }
                }
                Evt::AgentExported { path, agent_id } => {
                    if self.agent_ui.active_tab.as_deref() == Some(agent_id.as_str()) {
                        let t = i18n::strings(&self.prefs.language);
                        self.status = t.agent_export_toast.replace("{path}", &path);
                    }
                }
                Evt::MemHits(h) => self.on_mem_hits(h),
                Evt::SecretList { names, encrypted } => {
                    self.on_secret_list(names, encrypted);
                }
                Evt::WebResults(r) => self.chat_state.sidebar.web_results = r,
                Evt::BrowsePreview(t) => self.chat_state.sidebar.browse_preview = t,
                Evt::NetMode(online) => {
                    self.network_online = online;
                    self.prefs.network_online = online;
                    save_preferences(&self.prefs);
                }
                Evt::FileOk(msg) => {
                    self.status = msg.clone();
                    self.chat.push(ChatLine::plain("système", msg));
                }
                Evt::MediaImageEnriched { enriched } => {
                    media_event_controller::on_image_enriched(self, enriched);
                }
                Evt::MediaImageStarted {
                    enriching,
                    upscaling,
                    total_steps,
                } => {
                    media_event_controller::on_image_started(
                        self,
                        enriching,
                        upscaling,
                        total_steps,
                    );
                }
                Evt::MediaImageProgress {
                    enriching,
                    upscaling,
                    step,
                    total_steps,
                    elapsed_secs,
                } => {
                    media_event_controller::on_image_progress(
                        self,
                        enriching,
                        upscaling,
                        step,
                        total_steps,
                        elapsed_secs,
                    );
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
                    media_event_controller::on_media_ok(
                        self,
                        media_event_controller::MediaOkEvent {
                            kind,
                            path,
                            bytes,
                            engine,
                            prompt,
                            generation_prompt,
                            composition_blocks,
                        },
                    );
                }
                Evt::Skills(list) => self.on_agent_skills(list),
                Evt::McpServers(list) => self.on_agent_mcp_servers(list),
                Evt::PromptOptimized(p) => self.on_agent_prompt_optimized(p),
                Evt::Models(list) => self.on_models(list),
                Evt::ModelOperationFailed { model_id, error } => {
                    self.on_model_operation_failed(model_id, error);
                }
                Evt::Providers(list) => self.on_providers(list),
                Evt::ProviderTested {
                    ok,
                    message,
                    models,
                } => self.on_provider_tested(ok, message, models),
                Evt::AgentSpecLoaded { spec } => self.on_agent_spec_loaded(spec),
                Evt::AgentRosterSaved => {
                    let t = i18n::strings(&self.prefs.language);
                    self.status = t.agents_edit_saved.into();
                }
                Evt::AgentTrace(t) => self.on_agent_trace(t),
                Evt::InferStarted {
                    session_id,
                    inference_id,
                } => {
                    session_chat::on_infer_started(
                        &mut self.chat_state.session_chat,
                        self.chat_state.active_session.as_deref(),
                        &session_id,
                        inference_id,
                        &mut self.chat_state.runtime.inference_id,
                    );
                }
                Evt::ChatCancelled { session_id } => {
                    let on_active = session_chat::on_chat_cancelled(
                        &mut self.chat_state.session_chat,
                        self.chat_state.active_session.as_deref(),
                        &session_id,
                        &mut self.chat_state.runtime.streaming,
                        &mut self.chat_state.runtime.pending,
                        &mut self.chat_state.runtime.inference_id,
                        &mut self.chat,
                    );
                    if on_active {
                        self.chat_state.runtime.room_turn_text = None;
                        let t = i18n::strings(&self.prefs.language);
                        self.status = t.chat_stopped.into();
                    }
                }
                Evt::Catalogue(c) => module_event_controller::on_catalogue(self, c),
                Evt::InstalledSkills(list) => {
                    module_event_controller::on_installed_skills(self, list)
                }
                Evt::InstalledModules(list) => {
                    module_event_controller::on_installed_modules(self, list)
                }
                Evt::ModuleInstalled(msg) => module_event_controller::on_installed(self, msg),
                Evt::ModuleUninstalled(name) => module_event_controller::on_uninstalled(self, name),
                Evt::ModuleUiLoaded(resp) => module_event_controller::on_ui_loaded(self, resp),
                Evt::ModuleUiFailed { module, error } => {
                    module_event_controller::on_ui_failed(self, module, error);
                }
                Evt::ModuleUiBind {
                    module,
                    tool,
                    result,
                    error,
                } => {
                    module_event_controller::on_ui_bind(self, module, tool, result, error);
                }
                Evt::ModuleUiInvokeDone {
                    module,
                    tool,
                    ok,
                    result,
                    error,
                } => {
                    module_event_controller::on_ui_invoke_done(
                        self, module, tool, ok, result, error,
                    );
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
                                ui.radio_value(
                                    &mut self.onboarding.language,
                                    "fr".into(),
                                    "Français",
                                );
                                ui.radio_value(
                                    &mut self.onboarding.language,
                                    "en".into(),
                                    "English",
                                );
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
            ui.horizontal(|ui| {
                let count = self.agent_ui.notices.len();
                if ui
                    .button(format!("🔔 {count}"))
                    .on_hover_text("Centre de notifications")
                    .clicked()
                {
                    self.prefs.ui_layout.notifications_open =
                        !self.prefs.ui_layout.notifications_open;
                    save_preferences(&self.prefs);
                }
                ui.weak(format!("Preview {} — {}", self.version, t.preview_tagline));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(t.report).clicked() {
                        self.on_tab_open(Tab::Feedback);
                    }
                    if ui.small_button(t.tutorial).clicked() {
                        self.guide.open_topic(guide::GuideTopic::Overview);
                    }
                    if ui.small_button(t.troubleshooting).clicked() {
                        let _ = self.cmd_tx.send(Cmd::Troubleshoot);
                        self.on_tab_open(Tab::Feedback);
                        self.status = t.troubleshooting_status.into();
                    }
                });
            });
            if self.prefs.ui_layout.notifications_open && !self.agent_ui.notices.is_empty() {
                let notices = self.agent_ui.notices.clone();
                let mut dismiss: Vec<String> = Vec::new();
                let mut open_sess: Option<String> = None;
                for n in &notices {
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::from_rgb(120, 180, 230), &n.summary);
                        let sess_title = self
                            .chat_state
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
                        if icons::close_button(ui).clicked() {
                            dismiss.push(n.agent_id.clone());
                        }
                    });
                }
                self.agent_ui.dismiss_notices(&dismiss);
                if let Some(id) = open_sess {
                    self.tab = Tab::Chat;
                    self.request_session_select(id);
                }
            }
            if self.models_ui.model_download_restart.is_some() {
                self.ui_model_download_restart(ui, ctx);
            }
            // Confirmations en attente
            if !self.confirmations_ui.is_empty() {
                ui.separator();
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    t.confirm_pending
                        .replace("{n}", &self.confirmations_ui.len().to_string()),
                );
                overflow_scroll_h(ui, "pending_confirms", 180.0, |ui| {
                    for c in self.confirmations_ui.pending.clone() {
                        ui.group(|ui| {
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
                            ui.label(t.confirm_wants_action.replace("{action}", &c.action));
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
                                    self.scenario_ui.confirm = true;
                                }
                                if ui.button(t.confirm_deny).clicked() {
                                    let _ = self.cmd_tx.send(Cmd::Confirm {
                                        id: c.id.clone(),
                                        approved: false,
                                    });
                                    self.scenario_ui.confirm = true;
                                }
                            });
                        });
                    }
                });
            }
        });

        let sidebar_panel = egui::SidePanel::left("tabs")
            .default_width(
                self.prefs
                    .ui_layout
                    .chat_sidebar_width
                    .clamp(self.prefs.ui_density.rail_width().max(88.0), 220.0),
            )
            .min_width(self.prefs.ui_density.rail_width().max(88.0))
            .max_width(220.0)
            .resizable(true)
            .show(ctx, |ui| {
                overflow_scroll(ui, "nav_sidebar", |ui| {
                ui.heading(if self.prefs.ui_density == prefs::UiDensity::Compact { "A" } else { "Akasha" });
                    self.ui_nav_rail(ui, &t);
                    ui.separator();
                    ui.heading(if self.prefs.ui_density == prefs::UiDensity::Compact {
                        "RAM / CPU"
                    } else {
                        t.resources_heading
                    });
                    if let Some(m) = &self.metrics {
                        if self.prefs.ui_density != prefs::UiDensity::Compact {
                            let ratio = m.ram_used as f32 / m.ram_total.max(1) as f32;
                            ui.add(egui::ProgressBar::new(ratio).text(format!(
                                "{} {:.1}/{:.1} GiB",
                                t.metrics_ram,
                                m.ram_used as f64 / (1 << 30) as f64,
                                m.ram_total as f64 / (1 << 30) as f64
                            )));
                        } else {
                            ui.label(format!("RAM {:.0}%", 100.0 * m.ram_used as f32 / m.ram_total.max(1) as f32));
                        }
                        ui.label(format!("CPU {:.0}%", m.cpu_percent));
                        if self.prefs.ui_density != prefs::UiDensity::Compact {
                            ui.label(format!("{}: {}", t.metrics_live, m.live_inferences()));
                        }
                        // Keep technical model details progressive: the rail shows
                        // only a compact resource summary and Models owns the list.
                        ui.label(format!("{}: {}", t.tab_models, m.models.len()));
                        if ui
                            .button(t.tab_models)
                            .on_hover_text(t.tab_hint_models)
                            .clicked()
                        {
                            self.on_tab_open(Tab::Models);
                        }
                    } else {
                        ui.label("…");
                    }
                });
            });
        let sidebar_width = sidebar_panel.response.rect.width();
        if (sidebar_width - self.prefs.ui_layout.chat_sidebar_width).abs() > 1.0 {
            self.prefs.ui_layout.chat_sidebar_width = sidebar_width;
            save_preferences(&self.prefs);
        }

        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(28.0)
            .show(ctx, |ui| {
                let r = ui.max_rect();
                ui.painter().line_segment(
                    [r.left_top(), r.right_top()],
                    egui::Stroke::new(1.0_f32, theme::ICE_TRACK),
                );
                self.ui_status_bar(ui, &t);
                if !self.status.is_empty() {
                    ui.separator();
                    ui.weak(&self.status);
                }
            });

        self.ui_go_to_palette(ctx, &t);

        let mut restart_onboarding = false;
        guide::show_window(
            ctx,
            &mut self.guide,
            &self.prefs.language,
            &mut restart_onboarding,
        );
        research_document::show_documents_list(
            ctx,
            &mut self.research_ui.documents_list,
            &self.research_ui.documents,
            &mut self.research_ui.overlay,
            &t,
        );
        research_document::show_document_overlay(
            ctx,
            &mut self.research_ui.overlay,
            &mut self.chat_md_cache,
            &t,
        );
        if restart_onboarding {
            self.onboarding.tutorial_step = 0;
            self.onboarding.chat_sent = false;
            self.onboarding.first_chat_done = false;
            self.onboarding.completed = false;
            self.show_onboarding = true;
            save_onboarding(&self.onboarding);
        }

        self.poll_agent_trace(ctx);
        if !self.agent_ui.open_tabs.is_empty() || self.prefs.ui_layout.activity_panel_open {
            self.ui_agent_detail_panel(ctx);
        }

        let current_tab = self.tab.clone();
        egui::CentralPanel::default().show(ctx, |ui| match current_tab {
            Tab::Chat => self.ui_chat(ui),
            Tab::Memory => overflow_scroll(ui, "memory", |ui| self.ui_memory(ui)),
            Tab::Notes => overflow_scroll(ui, "notes", |ui| self.ui_notes(ui)),
            Tab::Library => overflow_scroll(ui, "library", |ui| self.ui_library(ui)),
            Tab::Tasks => overflow_scroll(ui, "tasks", |ui| self.ui_tasks(ui)),
            Tab::Agents => overflow_scroll(ui, "agents", |ui| self.ui_agents(ui)),
            Tab::Models => overflow_scroll(ui, "models", |ui| self.ui_models(ui, ctx)),
            Tab::Image => overflow_scroll(ui, "image", |ui| {
                let t = i18n::strings(&self.prefs.language);
                let g = guide::strings(&self.prefs.language);
                let mut open_create_guide = false;
                let gen = self.image_generating.as_ref();
                let dl_busy = self.models_ui.download_busy();
                let last_session = &mut self.chat_state.composer.last_session_image;
                self.image_studio.ui(
                    ui,
                    &t,
                    &self.cmd_tx,
                    gen,
                    dl_busy,
                    last_session,
                    Some(g.help_tooltip),
                    &mut open_create_guide,
                );
                if open_create_guide {
                    self.guide.open_topic(guide::GuideTopic::Create);
                }
            }),
            Tab::Providers => overflow_scroll(ui, "providers", |ui| self.ui_providers(ui)),
            Tab::Audit => overflow_scroll(ui, "audit", |ui| self.ui_audit(ui)),
            Tab::Caps => overflow_scroll(ui, "caps", |ui| self.ui_caps(ui)),
            Tab::Scenarios => overflow_scroll(ui, "scenarios", |ui| self.ui_scenarios(ui)),
            Tab::Feedback => overflow_scroll(ui, "feedback", |ui| self.ui_feedback(ui)),
            Tab::Settings => overflow_scroll(ui, "settings", |ui| self.ui_settings(ui)),
            Tab::Module(name) => overflow_scroll(ui, ("decl-mod", name.as_str()), |ui| {
                self.ui_decl_module(ui, &name)
            }),
        });
    }
}

#[cfg(test)]
mod ui_tests;
