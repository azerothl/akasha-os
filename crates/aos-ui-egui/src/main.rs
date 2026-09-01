//! Akasha OS Preview — UI egui (ADR 0003).
//!
//! Surface testeur : chat, dashboard, onboarding, notes, confirm, agents,
//! audit, scénarios guidés, retours (`feedback.submit`).

mod agent_act_phrase;
mod agent_panel;
mod decl_ui;
mod guide;
mod i18n;
mod model_setup;
mod models_page;
mod module_actions;
mod notes_panel;
mod prefs;
mod schedule_act_phrase;
mod schedule_card;
mod session_nav;
mod tasks_panel;
mod chat_ask;
mod chat_bubble;
mod chat_canvas;
mod chat_delegate;
mod chat_media;
mod chat_room;
mod cmd;
mod composer_layout;
mod image_composition;
mod image_history;
mod image_prompt;
mod icons;
mod image_studio;
mod library_panel;
mod nav;
mod onboarding;
mod os_open;
mod product_context;
mod research_choice;
mod research_document;
mod runtime;
mod scenarios_panel;
mod session_chat;
mod skill_offer;
mod slash;
mod theme;
mod troubleshoot;
mod ui_format;
mod ui_feedback;
mod ui_agents;
mod ui_models;
mod ui_memory;
mod ui_providers;
mod ui_security;
mod ui_scenarios;
mod ui_settings;
mod ui_workspace;

use chat_ask::{agent_display_title, chat_has_open_ask, pending_ask_ids};
use chat_bubble::{
    chat_bubble_colors, chat_bubble_kind, chat_markdown_viewer, chat_message_frame,
    chat_role_label, ChatBubbleKind,
};
use chat_delegate::{
    chat_agent_kit, chat_delegate_agent_spec, session_has_running_canvas_agent,
    spawn_chat_delegate_agent, spawn_document_prep_agent,
};
#[cfg(test)]
use chat_delegate::chat_delegate_kit;
#[cfg(test)]
use chat_bubble::chat_bubble_max_width;
use cmd::{AgentNotice, ChatLine, Cmd, Evt};
use composer_layout::{
    chat_canvas_layout, chat_composer_reserve_height, chat_sessions_split, composer_field_width,
    estimate_composer_buttons_w, send_button_reserved_width, stop_button_reserved_width,
    ChatCanvasLayout, ChatSessionsSplit, COMPOSER_MIN_INPUT_W,
};
#[cfg(test)]
use composer_layout::{chat_composer_wraps, COMPOSER_INPUT_ROW_H};
use os_open::{aos_home, app_icon, bin_aos_session, native_path, open_in_browser, open_os_folder};
use runtime::runtime_main;
use module_actions::{
    agent_id_cmd, invoke_module_bind, invoke_module_tool, invoke_notes, invoke_tasks,
    load_module_ui,
};
use troubleshoot::run_troubleshoot;
use ui_format::{
    chrono_like_stamp, format_local_time_hm, format_model_infer_line,
    format_schedule_next_label, human_bytes, local_tz_offset_minutes, memory_relation_lines,
    now_ms,
};
use slash::{slash_completions, slash_insert_text, SLASH_COMMANDS};
use aos_agent::schedule::ScheduleEntry;
use aos_agent::schedule_parse::{self, ParsedSchedule};
use aos_ipc::BusClient;
use aos_proto::{
    AgentInfo, AgentState, AgentTrace, AuditEvent,
    CapInfo, ChatAttachment, ChatRoomMember, ChatSessionGetResponse,
    ChatSessionIdRequest, ChatSessionMeta, ChatSessionMode, DocumentRef,
    McpServerInfo, MemHit, ModelInfo,
    ModuleCatalogue, ModuleInfo,
    PendingConfirmation, ProviderRecord,
    SkillInfo, SystemMetrics, WebSearchHit,
    chat_tts_request,
};
use prefs::{load_preferences, save_preferences, Preferences};
use eframe::egui;
use egui_commonmark::CommonMarkCache;
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
    let version = std::env::var("AOS_PREVIEW_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into());

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
) -> String {
    let title = ag.display_title();
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
            if ag.fail_reason.as_deref()
                == Some(aos_agent::actions::THREAD_FAIL_COULD_NOT_CONTINUE)
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
    let _ = evt_tx.send(Evt::SessionLoadIntent {
        id: id.to_string(),
    });
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

/// One-row height for the canvas tool strip (session bar row 2).
const CANVAS_TOOLBAR_ROW_H: f32 = 36.0;

/// Minimum inner window size (width × height) for a usable Preview layout.
fn preview_min_inner_size() -> [f32; 2] {
    let fr = i18n::strings("fr");
    [preview_min_inner_width(&fr), 600.0]
}

const LEFT_NAV_W: f32 = 148.0;
const CHAT_SIDE_MIN_W: f32 = 120.0;
const CHAT_SPLIT_GAP: f32 = 8.0;
const CHAT_MAIN_MARGIN: f32 = 16.0;

fn estimate_label_chip_w(label: &str) -> f32 {
    const CHAR_W: f32 = 8.5;
    const PAD: f32 = 22.0;
    label.len() as f32 * CHAR_W + PAD
}

fn session_toggle_reserve_width(t: &i18n::UiStrings) -> f32 {
    estimate_label_chip_w(t.session_toggle_salon) + 6.0 + estimate_label_chip_w(t.session_toggle_canvas)
}

fn session_toggle_chip(ui: &mut egui::Ui, selected: bool, label: &str) -> bool {
    let w = estimate_label_chip_w(label);
    ui.add_sized(
        egui::vec2(w, ui.spacing().interact_size.y),
        egui::SelectableLabel::new(selected, egui::RichText::new(label)),
    )
    .clicked()
}

fn composer_row_reserved_width(t: &i18n::UiStrings, show_stop: bool) -> f32 {
    icons::ATTACH_BTN_W
        + 4.0
        + estimate_composer_buttons_w(t.agent_send, show_stop, t.chat_stop)
}

/// Minimum central chat pane width so composer + session toggles fit (FR labels).
fn preview_min_inner_width(t: &i18n::UiStrings) -> f32 {
    let composer = composer_row_reserved_width(t, true) + COMPOSER_MIN_INPUT_W;
    let session_bar = session_toggle_reserve_width(t) + 160.0;
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
    streaming: String,
    input: String,
    chat_pending: bool,
    chat_inference_id: Option<u64>,
    /// PNG/JPEG paths queued for the next chat turn (vision).
    chat_pending_images: Vec<String>,
    /// PDF/txt/md paths queued for the next chat turn (text extraction at send).
    chat_pending_documents: Vec<DocumentRef>,
    /// Last Create / canvas export image path in this session.
    last_session_image: Option<String>,
    catalogue: Option<ModuleCatalogue>,
    installed_modules: Vec<ModuleInfo>,
    sessions: Vec<ChatSessionMeta>,
    active_session: Option<String>,
    session_chat: session_chat::SessionChatState,
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
    mem_sweep_last_pass_ms: u64,
    mem_sweep_last_pass_label: String,
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
    library: library_panel::LibraryPanelState,
    schedules: Vec<ScheduleEntry>,
    schedule_goal: String,
    schedule_interval_secs: u64,
    /// Act id waiting for `Evt::ScheduleCreated` to attach a thread card.
    schedule_pending_card_act: Option<String>,
    /// User-initiated session navigation intent for the next cross-session load.
    pending_session_nav: session_nav::PendingSessionNav,
    /// In-memory schedule act/card edits not yet safe to clobber from disk.
    schedule_transcript_dirty: bool,
    agent_display_name: String,
    agent_task: String,
    agent_system_prompt: String,
    agent_docs: String,
    agent_max_steps: u32,
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
    update_download_child: Option<std::process::Child>,
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
    /// Expanded salon thinking blocks keyed by chat line index.
    room_thinking_open: std::collections::HashSet<usize>,
    /// Last user message for an in-flight room turn (speaker queue in thinking UI).
    room_turn_pending_text: Option<String>,
    /// Room: Members pane toggled from clickable session header.
    room_members_pane_open: bool,
    canvas_panel: chat_canvas::CanvasPanelState,
    roster_edit_drafts: HashMap<String, RosterEditDraft>,
    guide: guide::GuideState,
    /// Deferred normal chat after user picks Answer on a research choice card.
    research_pending_chat: Option<ResearchPendingChat>,
    /// Document-prep agent_id → original question (result card title).
    document_prep_agents: HashMap<String, String>,
    /// Suppress agent.kill ok status banners after document prep stop.
    document_prep_kill_pending: u32,
    /// Recoverable prepared documents (var/documents index).
    research_documents: Vec<aos_agent::document_index::ResearchDocumentEntry>,
    document_overlay: research_document::DocumentOverlayState,
    documents_list: research_document::DocumentsListState,
}

#[derive(Clone)]
struct ResearchPendingChat {
    session_id: String,
    history: Vec<(String, String)>,
    user_text: String,
    model_id: Option<String>,
    images: Vec<String>,
    documents: Vec<DocumentRef>,
    auto_remember: bool,
    max_steps: u32,
    routing: String,
    language: String,
    canvas_open: bool,
    canvas_aspect: aos_proto::CanvasAspect,
    choice_id: String,
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
        let _ = cmd_tx.send(Cmd::SkillPassPending);
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
            t.chat_thread_intro.replace("{}", &version)
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
            chat_pending_images: Vec::new(),
            chat_pending_documents: Vec::new(),
            last_session_image: None,
            catalogue: None,
            installed_modules: Vec::new(),
            sessions: Vec::new(),
            active_session: None,
            session_chat: session_chat::SessionChatState::default(),
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
            mem_sweep_last_pass_ms: 0,
            mem_sweep_last_pass_label: String::new(),
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
            library: library_panel::LibraryPanelState::default(),
            schedules: Vec::new(),
            schedule_goal: String::new(),
            schedule_interval_secs: 60,
            schedule_pending_card_act: None,
            pending_session_nav: session_nav::PendingSessionNav::None,
            schedule_transcript_dirty: false,
            agent_display_name: String::new(),
            agent_task: String::new(),
            agent_system_prompt: String::new(),
            agent_docs: String::new(),
            agent_max_steps,
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
            update_download_child: None,
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
            room_thinking_open: std::collections::HashSet::new(),
            room_members_pane_open: false,
            canvas_panel: chat_canvas::CanvasPanelState::default(),
            roster_edit_drafts: HashMap::new(),
            guide: guide::GuideState::default(),
            research_pending_chat: None,
            document_prep_agents: HashMap::new(),
            document_prep_kill_pending: 0,
            research_documents: research_document::load_index_entries(),
            document_overlay: research_document::DocumentOverlayState::default(),
            documents_list: research_document::DocumentsListState::default(),
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
                speaker_name: None,
                thinking: None,
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

    fn dispatch_pending_chat(&mut self, pending: ResearchPendingChat) {
        self.streaming.clear();
        self.chat_pending = true;
        self.chat_inference_id = None;
        self.status = "assistant : génération…".into();
        let _ = self.cmd_tx.send(Cmd::Chat {
            session_id: pending.session_id,
            history: pending.history,
            user_text: pending.user_text,
            model_id: pending.model_id,
            images: pending.images,
            documents: pending.documents,
            auto_remember: pending.auto_remember,
            max_steps: pending.max_steps,
            routing: pending.routing,
            language: pending.language,
            canvas_open: pending.canvas_open,
            canvas_aspect: pending.canvas_aspect,
        });
        self.mark_onboarding_chat_sent();
        self.scen_chat = true;
    }

    fn offer_research_choice(
        &mut self,
        session_id: &str,
        user_text: &str,
        pending: ResearchPendingChat,
    ) {
        let att = research_choice::choice_attachment(user_text, &pending.choice_id);
        self.chat.push(ChatLine {
            role: "assistant".into(),
            text: String::new(),
            attachments: vec![att.clone()],
            speaker_id: None,
            speaker_name: None,
            thinking: None,
        });
        let _ = self.cmd_tx.send(Cmd::SessionAppend {
            session_id: session_id.to_string(),
            role: "assistant".into(),
            content: String::new(),
            attachments: vec![att],
        });
        self.research_pending_chat = Some(pending);
        self.mark_onboarding_chat_sent();
        self.scen_chat = true;
    }

    fn start_document_prep(&mut self, session_id: &str, pending: ResearchPendingChat) {
        let question = pending.user_text.clone();
        let t = i18n::strings(&pending.language);
        let ack = t.document_prep_ack;
        let att = research_document::progress_attachment(&question, "pending");
        self.chat.push(ChatLine {
            role: "assistant".into(),
            text: ack.into(),
            attachments: vec![att.clone()],
            speaker_id: None,
            speaker_name: None,
            thinking: None,
        });
        let _ = self.cmd_tx.send(Cmd::SessionAppend {
            session_id: session_id.to_string(),
            role: "assistant".into(),
            content: ack.into(),
            attachments: vec![att],
        });
        let _ = self.cmd_tx.send(Cmd::DocumentPrepSpawn {
            session_id: session_id.to_string(),
            question: pending.user_text,
            language: pending.language,
            model_id: pending.model_id,
            max_steps: pending.max_steps,
        });
        self.mark_onboarding_chat_sent();
        self.scen_chat = true;
    }

    fn attach_document_progress_agent(&mut self, agent_id: &str, question: &str) {
        let att = research_document::progress_attachment(question, agent_id);
        for line in &mut self.chat {
            let has_placeholder = line.attachments.iter().any(|a| {
                matches!(
                    a,
                    ChatAttachment::DocumentProgress {
                        agent_id: id,
                        ..
                    } if id == "pending"
                )
            });
            if has_placeholder {
                line.attachments.retain(|a| {
                    !matches!(
                        a,
                        ChatAttachment::DocumentProgress {
                            agent_id: id,
                            ..
                        } if id == "pending"
                    )
                });
                line.attachments.push(att.clone());
                return;
            }
        }
        self.chat.push(ChatLine {
            role: "assistant".into(),
            text: String::new(),
            attachments: vec![att],
            speaker_id: None,
            speaker_name: None,
            thinking: None,
        });
    }

    fn replace_progress_with_result(&mut self, question: &str, path: &str) {
        let label = research_choice::label_from_path(path);
        let result = research_choice::document_result_attachment(question, path, &label);
        let mut replaced = false;
        for line in &mut self.chat {
            if line.attachments.iter().any(|a| {
                matches!(a, ChatAttachment::DocumentProgress { .. })
            }) {
                line.attachments.retain(|a| !matches!(a, ChatAttachment::DocumentProgress { .. }));
                line.attachments.push(result.clone());
                replaced = true;
                break;
            }
        }
        if !replaced {
            self.chat.push(ChatLine {
                role: "assistant".into(),
                text: String::new(),
                attachments: vec![result.clone()],
                speaker_id: None,
                speaker_name: None,
                thinking: None,
            });
        }
        if let Some(sid) = self.active_session.clone() {
            let _ = self.cmd_tx.send(Cmd::SessionAppend {
                session_id: sid,
                role: "assistant".into(),
                content: String::new(),
                attachments: vec![result],
            });
        }
    }

    fn record_prepared_document(&mut self, question: &str, path: &str, label: &str) {
        let home = aos_home();
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let _ = aos_agent::document_index::record_research_document(
            &home,
            question,
            path,
            label,
            ms,
        );
        self.research_documents = research_document::load_index_entries();
    }

    fn resolve_research_choice_answer(&mut self, choice_id: &str, msg_idx: usize) {
        let Some(pending) = self.research_pending_chat.take() else {
            return;
        };
        if pending.choice_id != choice_id {
            self.research_pending_chat = Some(pending);
            return;
        }
        if let Some(att) = self.chat[msg_idx].attachments.iter_mut().find_map(|a| {
            if let ChatAttachment::ResearchChoice {
                choice_id: id,
                state,
                ..
            } = a
            {
                if id == choice_id {
                    Some(state)
                } else {
                    None
                }
            } else {
                None
            }
        }) {
            *att = "answer".into();
        }
        self.dispatch_pending_chat(pending);
    }

    fn resolve_research_choice_document(
        &mut self,
        choice_id: &str,
        msg_idx: usize,
        session_id: &str,
    ) {
        let pending = match self.research_pending_chat.take() {
            Some(p) if p.choice_id == choice_id => p,
            other => {
                self.research_pending_chat = other;
                return;
            }
        };
        if let Some(att) = self.chat[msg_idx].attachments.iter_mut().find_map(|a| {
            if let ChatAttachment::ResearchChoice {
                choice_id: id,
                state,
                ..
            } = a
            {
                if id == choice_id {
                    Some(state)
                } else {
                    None
                }
            } else {
                None
            }
        }) {
            *att = "document".into();
        }
        let _ = self.cmd_tx.send(Cmd::SessionAppend {
            session_id: session_id.to_string(),
            role: "user".into(),
            content: pending.user_text.clone(),
            attachments: vec![],
        });
        self.start_document_prep(session_id, pending);
    }

    fn attach_document_result_card(&mut self, question: &str, path: &str) {
        let label = research_choice::label_from_path(path);
        self.replace_progress_with_result(question, path);
        self.record_prepared_document(question, path, &label);
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
        if self.ask_reply_target.as_deref() == Some(agent_id.as_str()) {
            self.ask_reply_target = None;
        }
        self.status = "réponse envoyée à l'agent".into();
    }

    fn send_chat(&mut self) {
        let text = self.input.trim().to_string();
        let pending_images = self.chat_pending_images.clone();
        let pending_documents = self.chat_pending_documents.clone();
        if text.is_empty() && pending_images.is_empty() && pending_documents.is_empty() {
            return;
        }
        let t = i18n::strings(&self.prefs.language);
        let text = if text.is_empty() && !pending_images.is_empty() {
            t.chat_empty_image_prompt.to_string()
        } else {
            text
        };
        self.input.clear();
        self.chat_refocus = true;
        if text.starts_with('/')
            && pending_images.is_empty()
            && pending_documents.is_empty()
        {
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
        if pending_images.is_empty() && pending_documents.is_empty() {
            let tz = local_tz_offset_minutes();
            if let Some(parsed) = schedule_parse::try_parse_phrase(&text, now_ms(), tz) {
                self.handle_schedule_phrase(&session_id, &text, parsed);
                return;
            }
        }
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
        if self
            .active_session
            .as_deref()
            .is_some_and(|sid| self.session_chat.is_pending(sid))
        {
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
        if !pending_images.is_empty() {
            let model_id = self
                .sessions
                .iter()
                .find(|s| s.id == session_id)
                .and_then(|s| s.model_id.clone());
            if !session_model_supports_vision(model_id.as_deref()) {
                self.chat.push(ChatLine::plain(
                    "système",
                    i18n::strings(&self.prefs.language).chat_vision_banner,
                ));
                return;
            }
        }
        let image_atts: Vec<ChatAttachment> = pending_images
            .iter()
            .map(|path| ChatAttachment::Image {
                path: path.clone(),
                prompt: String::new(),
            })
            .collect();
        let doc_atts: Vec<ChatAttachment> = pending_documents
            .iter()
            .map(|doc| ChatAttachment::Document {
                path: doc.path.clone(),
                label: doc.label.clone(),
            })
            .collect();
        let mut attachments = image_atts;
        attachments.extend(doc_atts);
        self.chat.push(ChatLine {
            role: "user".into(),
            text: text.clone(),
            attachments,
            speaker_id: None,
            speaker_name: None,
            thinking: None,
        });
        self.chat_pending_images.clear();
        self.chat_pending_documents.clear();
        let room_content = aos_proto::chat_document::merge_documents_into_user_content(
            &text,
            &pending_documents,
        );
        if chat_room::session_is_room(chat_room::active_session_meta(
            &self.sessions,
            self.active_session.as_deref(),
        )) {
            let Some(session_id) = self.active_session.clone() else {
                return;
            };
            self.session_chat.begin_turn(&session_id);
            self.streaming.clear();
            self.chat_pending = true;
            self.chat_inference_id = None;
            self.room_turn_pending_text = Some(text.clone());
            let _ = self.cmd_tx.send(Cmd::RoomTurn {
                session_id,
                content: room_content,
                images: pending_images,
            });
            self.mark_onboarding_chat_sent();
            self.scen_chat = true;
            return;
        }
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
        if pending_images.is_empty()
            && pending_documents.is_empty()
            && aos_agent::research_detect::is_research_shaped_ask(&text)
        {
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
            if aos_agent::research_detect::user_requested_document(&text) {
                let choice_id = format!("research-choice-{}", chrono_like_stamp());
                let pending = ResearchPendingChat {
                    session_id: session_id.clone(),
                    history,
                    user_text: text.clone(),
                    model_id: model_id.clone(),
                    images: pending_images,
                    documents: pending_documents,
                    auto_remember: self.prefs.auto_remember_chat,
                    max_steps: chat_agent_max_steps(self.prefs.default_max_steps),
                    routing: self.prefs.routing.clone(),
                    language: self.prefs.language.clone(),
                    canvas_open,
                    canvas_aspect,
                    choice_id,
                };
                let _ = self.cmd_tx.send(Cmd::SessionAppend {
                    session_id: session_id.clone(),
                    role: "user".into(),
                    content: text.clone(),
                    attachments: self.chat.last().map(|l| l.attachments.clone()).unwrap_or_default(),
                });
                self.start_document_prep(session_id.as_str(), pending);
                return;
            }
            let choice_id = format!("research-choice-{}", chrono_like_stamp());
            let pending = ResearchPendingChat {
                session_id: session_id.clone(),
                history,
                user_text: text.clone(),
                model_id: model_id.clone(),
                images: pending_images,
                documents: pending_documents,
                auto_remember: self.prefs.auto_remember_chat,
                max_steps: chat_agent_max_steps(self.prefs.default_max_steps),
                routing: self.prefs.routing.clone(),
                language: self.prefs.language.clone(),
                canvas_open,
                canvas_aspect,
                choice_id,
            };
            self.offer_research_choice(&session_id, &text, pending);
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
        self.session_chat.begin_turn(&session_id);
        self.streaming.clear();
        self.chat_pending = true;
        self.chat_inference_id = None;
        self.status = "assistant : génération…".into();
        let _ = self.cmd_tx.send(Cmd::Chat {
            session_id,
            history,
            user_text: text,
            model_id,
            images: pending_images,
            documents: pending_documents,
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

    fn queue_chat_document(&mut self, path: String) {
        if path.is_empty() || !aos_proto::chat_document::is_chat_document_path(&path) {
            return;
        }
        let label = aos_proto::chat_document::document_label_from_path(&path);
        let doc = DocumentRef { path, label };
        if !self
            .chat_pending_documents
            .iter()
            .any(|d| d.path == doc.path)
        {
            if self.chat_pending_documents.len()
                >= aos_proto::chat_document::CHAT_MAX_PENDING_DOCUMENTS
            {
                self.chat_pending_documents.remove(0);
            }
            self.chat_pending_documents.push(doc);
        }
        self.status = i18n::strings(&self.prefs.language)
            .chat_attach_document
            .to_string();
    }

    fn queue_chat_image(&mut self, path: String) {
        if path.is_empty() {
            return;
        }
        if !self.chat_pending_images.iter().any(|p| p == &path) {
            if self.chat_pending_images.len() >= 4 {
                self.chat_pending_images.remove(0);
            }
            self.chat_pending_images.push(path.clone());
        }
        self.last_session_image = Some(path);
        self.status = i18n::strings(&self.prefs.language)
            .chat_attach_image
            .to_string();
    }

    fn load_preferred_vision_model(&mut self) {
        let Some(model_id) = models_page::first_catalog_vision_model_id() else {
            self.status = i18n::strings(&self.prefs.language)
                .chat_vision_banner
                .to_string();
            return;
        };
        if let Some(sid) = self.active_session.clone() {
            let _ = self.cmd_tx.send(Cmd::SessionSetModel {
                session_id: sid,
                model_id: Some(model_id.clone()),
            });
        }
        let _ = self.cmd_tx.send(Cmd::ModelLoad {
            model_id: model_id.clone(),
        });
        self.status = format!("vision: {model_id}");
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
        if let Some(sid) = self.active_session.clone() {
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

    fn handle_schedule_phrase(&mut self, session_id: &str, user_phrase: &str, parsed: ParsedSchedule) {
        let t = i18n::strings(&self.prefs.language);
        let act_text = schedule_act_phrase::act_phrase_from_parsed(&t, &parsed, &self.prefs.language);
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
        let gate_ask = !self.prefs.agent_gate_mode.eq_ignore_ascii_case("autonomous");
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
            self.schedule_pending_card_act = Some(format!("sched-auto-{}", chrono_like_stamp()));
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
        self.scen_chat = true;
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
        self.schedule_transcript_dirty = true;
        if let Some(session_id) = self.active_session.clone() {
            let att = self.chat[msg_idx].attachments[att_idx].clone();
            let _ = self.cmd_tx.send(Cmd::SessionAppend {
                session_id,
                role: "assistant".into(),
                content: self.chat[msg_idx].text.clone(),
                attachments: vec![att],
            });
        }
        self.schedule_pending_card_act = Some(act_id.to_string());
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
            goal,
            when_label,
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
            false,
            &self.prefs.language,
        );
        if let ChatAttachment::ScheduleAct { state, .. } =
            &mut self.chat[msg_idx].attachments[att_idx]
        {
            *state = "denied".into();
        }
        self.schedule_transcript_dirty = true;
        if let Some(session_id) = self.active_session.clone() {
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
        let act_id = match self.schedule_pending_card_act.take() {
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
                        self.schedule_transcript_dirty = true;
                        if let Some(session_id) = self.active_session.clone() {
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
        self.schedule_transcript_dirty = true;
        self.chat.push(ChatLine {
            role: "assistant".into(),
            text: String::new(),
            attachments: vec![card.clone()],
            speaker_id: None,
            speaker_name: None,
            thinking: None,
        });
        if let Some(session_id) = self.active_session.clone() {
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
        self.schedule_transcript_dirty = false;
        let _ = self.cmd_tx.send(Cmd::SessionSelect { id });
    }

    fn request_session_create(&mut self, title: Option<String>) {
        self.pending_session_nav = session_nav::PendingSessionNav::AwaitingCreate;
        self.schedule_transcript_dirty = false;
        let _ = self.cmd_tx.send(Cmd::SessionCreate { title });
    }

    fn request_session_delete(&mut self, id: String) {
        self.pending_session_nav = session_nav::PendingSessionNav::AwaitingDelete;
        self.schedule_transcript_dirty = false;
        let _ = self.cmd_tx.send(Cmd::SessionDelete { id });
    }

    fn sync_schedule_cards(&mut self) {
        let now = now_ms();
        for line in &mut self.chat {
            for att in &mut line.attachments {
                if let ChatAttachment::ScheduleCard { schedule_id, .. } = att {
                    if let Some(entry) = self.schedules.iter().find(|s| s.id == *schedule_id) {
                        schedule_card::sync_card_attachment(att, entry, now);
                    }
                }
            }
        }
    }

    fn upsert_schedule_entry(&mut self, entry: ScheduleEntry) {
        schedule_card::upsert_schedule_entry(&mut self.schedules, entry);
    }

    fn apply_schedule_card_action_local(&mut self, action: schedule_card::ScheduleCardAction) {
        let now = now_ms();
        match &action {
            schedule_card::ScheduleCardAction::Pause(id) => {
                schedule_card::apply_local_pause(&mut self.schedules, id);
            }
            schedule_card::ScheduleCardAction::Resume(id) => {
                schedule_card::apply_local_resume(&mut self.schedules, id);
            }
            schedule_card::ScheduleCardAction::Stop(id) => {
                schedule_card::apply_local_stop(&mut self.schedules, id);
            }
            schedule_card::ScheduleCardAction::None => return,
        }
        self.schedule_transcript_dirty = true;
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
                            &self.schedules,
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
            Tab::Chat => {
                let _ = self.cmd_tx.send(Cmd::SkillPassPending);
            }
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
                    .selectable_label(self.documents_list.open, t.nav_documents)
                    .on_hover_text(t.documents_list_title)
                    .clicked()
                {
                    self.documents_list.open = true;
                    self.research_documents = research_document::load_index_entries();
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
            let gate_ask = !self.prefs.agent_gate_mode.eq_ignore_ascii_case("autonomous");
            let gate_label = if gate_ask {
                t.status_gate_ask
            } else {
                t.status_gate_autonomous
            };
            if ui
                .small_button(format!("{}: {}", t.status_gate_label, gate_label))
                .clicked()
            {
                self.prefs.agent_gate_mode = if gate_ask {
                    "autonomous".into()
                } else {
                    "ask".into()
                };
                save_preferences(&self.prefs);
            }
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
        theme::apply_theme(ctx, &self.prefs.theme);
        theme::apply_ui_scale(ctx, self.prefs.ui_scale_percent);
        self.handle_keyboard_shortcuts(ctx);
        self.poll_update_download();
        while let Ok(ev) = self.evt_rx.try_recv() {
            match ev {
                Evt::Delta { session_id, text } => {
                    session_chat::on_delta(
                        &mut self.session_chat,
                        self.active_session.as_deref(),
                        &session_id,
                        &text,
                        &mut self.streaming,
                    );
                }
                Evt::Done {
                    text,
                    session_id,
                    attachments,
                } => {
                    session_chat::on_done(
                        &mut self.session_chat,
                        self.active_session.as_deref(),
                        &session_id,
                        &text,
                        attachments,
                        &mut self.chat,
                        &mut self.streaming,
                        &mut self.chat_pending,
                        &mut self.chat_inference_id,
                    );
                    if self.status.starts_with("assistant :") {
                        self.status.clear();
                    }
                    self.mark_onboarding_chat_done();
                }
                Evt::Error(m) => {
                    if aos_agent::context_budget::is_technical_vision_infer_error(&m) {
                        self.chat_pending = false;
                        self.streaming.clear();
                        self.chat_inference_id = None;
                        self.room_turn_pending_text = None;
                        if self.status.starts_with("assistant :") {
                            self.status.clear();
                        }
                        break;
                    }
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
                    if m == format!("{} ok", aos_agent::intents::KILL)
                        && self.document_prep_kill_pending > 0
                    {
                        self.document_prep_kill_pending -= 1;
                    } else {
                        self.status = m;
                    }
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
                Evt::MemSweepStatus {
                    last_pass_ms,
                    last_pass_label,
                } => {
                    self.mem_sweep_last_pass_ms = last_pass_ms;
                    self.mem_sweep_last_pass_label = last_pass_label;
                }
                Evt::SkillPassPending(offer) => {
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
                } => {
                    if origin == "document" {
                        self.document_prep_agents
                            .insert(agent_id.clone(), title.clone());
                        if self.active_session.as_deref() == Some(session_id.as_str()) {
                            self.attach_document_progress_agent(&agent_id, &title);
                        }
                    } else {
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
                                speaker_name: None,
                                thinking: None,
                            });
                        } else {
                            self.status = format!("agent lancé : {agent_id}");
                        }
                    }
                }
                Evt::Agents(a) => {
                    let t = i18n::strings(&self.prefs.language);
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
                            if self.document_prep_agents.contains_key(&ag.agent_id) && was_active && !seeding
                            {
                                if ag.state == AgentState::Done {
                                    let _ = self.cmd_tx.send(Cmd::AgentTrace {
                                        id: ag.agent_id.clone(),
                                    });
                                } else {
                                    self.document_prep_agents.remove(&ag.agent_id);
                                    self.agent_notified.insert(ag.agent_id.clone());
                                }
                            } else if let Some(sid) = &ag.session_id {
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
                                    let session_ops = agent_canvas_session_ops(
                                        ag,
                                        self.active_session.as_deref(),
                                        &self.canvas_panel.ops,
                                    );
                                    let trace = self.agent_traces.get(&ag.agent_id);
                                    let content = agent_completion_chat_text(
                                        ag,
                                        &t,
                                        session_ops,
                                        trace,
                                    );
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
                                    } else if !seeding
                                        && !self.document_prep_agents.contains_key(&ag.agent_id)
                                    {
                                        if content.is_empty()
                                            && !agent_panel::canvas_draw_step_cap_continue(
                                                Some(ag),
                                                session_ops,
                                                trace,
                                            )
                                        {
                                            self.agent_notified.insert(ag.agent_id.clone());
                                            continue;
                                        }
                                        self.chat.push(ChatLine {
                                            role: "assistant".into(),
                                            text: content,
                                            attachments: vec![ChatAttachment::AgentRef {
                                                agent_id: ag.agent_id.clone(),
                                                title: ag.directive.clone(),
                                                origin: "completion".into(),
                                            }],
                                            speaker_id: None,
                                            speaker_name: None,
                                            thinking: None,
                                        });
                                    }
                                } else if !seeding
                                    && !on_this_session
                                    && !self.agent_notified.contains(&ag.agent_id)
                                    && was_active
                                {
                                    let session_ops = agent_canvas_session_ops(
                                        ag,
                                        self.active_session.as_deref(),
                                        &self.canvas_panel.ops,
                                    );
                                    let trace = self.agent_traces.get(&ag.agent_id);
                                    let summary = if agent_panel::canvas_draw_failure_muted(
                                        Some(ag),
                                        session_ops,
                                        trace,
                                    ) {
                                        String::new()
                                    } else {
                                        match ag.state {
                                        AgentState::Done => format!("{} terminé", ag.display_title()),
                                        AgentState::Failed => {
                                            if agent_panel::canvas_draw_fail_chrome(
                                                Some(ag),
                                                session_ops,
                                                trace,
                                            ) {
                                                t.canvas_draw_failed.to_string()
                                            } else if ag.fail_reason.as_deref()
                                                == Some(aos_agent::actions::THREAD_FAIL_COULD_NOT_ACT)
                                            {
                                                i18n::agent_could_not_act_message(&t)
                                            } else if ag.fail_reason.as_deref()
                                                == Some(
                                                    aos_agent::actions::THREAD_FAIL_COULD_NOT_CONTINUE,
                                                )
                                                || ag.fail_reason.as_deref().is_some_and(
                                                    aos_agent::context_budget::is_overflow_fail_reason,
                                                )
                                            {
                                                i18n::agent_could_not_continue_message(&t)
                                            } else {
                                                format!(
                                                    "{} échoué — {}",
                                                    ag.display_title(),
                                                    i18n::resolve_agent_fail_reason(
                                                        &t,
                                                        ag.fail_reason.as_deref(),
                                                    )
                                                )
                                            }
                                        }
                                        AgentState::Killed => format!("{} arrêté", ag.display_title()),
                                        _ => format!("{} terminé", ag.display_title()),
                                    }
                                    };
                                    if summary.is_empty() {
                                        self.agent_notified.insert(ag.agent_id.clone());
                                        continue;
                                    }
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
                                        speaker_name: None,
                                        thinking: None,
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
                                        speaker_name: None,
                                        thinking: None,
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
                Evt::UserLibraryListed(docs) => {
                    self.library.docs = docs;
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
                Evt::Schedules(s) => {
                    schedule_card::merge_schedule_list(&mut self.schedules, s);
                    self.sync_schedule_cards();
                }
                Evt::ScheduleCreated(entry) => {
                    self.upsert_schedule_entry(entry.clone());
                    self.attach_schedule_card(&entry);
                    self.sync_schedule_cards();
                }
                Evt::ScheduleUpdated(entry) => {
                    self.upsert_schedule_entry(entry);
                    self.sync_schedule_cards();
                }
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
                Evt::SessionLoadIntent { id } => {
                    session_nav::apply_session_load_intent(&mut self.pending_session_nav, &id);
                }
                Evt::SessionLoaded { id, messages, meta } => {
                    let session_changed = self.active_session.as_deref() != Some(id.as_str());
                    if session_changed
                        && !session_nav::should_switch_session_view(
                            self.active_session.as_deref(),
                            &self.pending_session_nav,
                            id.as_str(),
                        )
                    {
                        if let Some(s) = self.sessions.iter_mut().find(|s| s.id == meta.id) {
                            *s = meta.clone();
                        }
                    } else if !session_changed
                        && !session_nav::should_replace_chat_on_same_session_reload(
                            self.schedule_transcript_dirty,
                        )
                    {
                        self.rename_buf = meta.title.clone();
                        if let Some(s) = self.sessions.iter_mut().find(|s| s.id == meta.id) {
                            *s = meta.clone();
                        }
                        self.sync_schedule_cards();
                        self.pending_session_nav = session_nav::PendingSessionNav::None;
                    } else {
                        self.pending_session_nav = session_nav::PendingSessionNav::None;
                        self.active_session = Some(id.clone());
                        self.rename_buf = meta.title.clone();
                        if let Some(s) = self.sessions.iter_mut().find(|s| s.id == meta.id) {
                            *s = meta.clone();
                        }
                        if session_changed {
                            self.room_members_pane_open = false;
                            let mut chat = Vec::new();
                            if !designer_shot_mode() {
                                chat.push(ChatLine::plain(
                                    "système",
                                    format!("Session {id} — historique rechargé."),
                                ));
                            }
                            chat.extend(messages);
                            self.chat = chat;
                        } else {
                            self.chat = messages;
                        }
                        self.sync_schedule_cards();
                        let _ = self.cmd_tx.send(Cmd::SkillPassPending);
                        self.session_chat.clear_unread(&id);
                        self.session_chat.sync_active_view(
                            self.active_session.as_deref(),
                            &mut self.streaming,
                            &mut self.chat_pending,
                            &mut self.chat_inference_id,
                        );
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
                }
                Evt::RoomTurnDone {
                    session_id,
                    agent_turns,
                    cancelled,
                } => {
                    self.session_chat.finish_turn(&session_id);
                    if self.active_session.as_deref() == Some(session_id.as_str()) {
                        self.chat_pending = false;
                        self.chat_inference_id = None;
                        self.room_turn_pending_text = None;
                        if let Some(status) =
                            chat_room::room_turn_done_status(agent_turns, cancelled)
                        {
                            self.status = status;
                        } else if self.status.starts_with("salon :") {
                            self.status.clear();
                        }
                    } else if !cancelled && agent_turns > 0 {
                        self.session_chat.mark_unread(&session_id);
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
                    canvas_seeing,
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
                        if let Some(seeing) = canvas_seeing {
                            self.canvas_panel.seeing = seeing;
                        }
                    }
                }
                Evt::CanvasExported { path, session_id } => {
                    if self.active_session.as_deref() == Some(session_id.as_str()) {
                        self.status = format!("Canvas → {path}");
                        self.last_session_image = Some(path.clone());
                        self.chat.push(ChatLine {
                            role: "assistant".into(),
                            text: format!("Canvas exporté : {path}"),
                            attachments: vec![ChatAttachment::Image {
                                path,
                                prompt: "canvas export".into(),
                            }],
                            speaker_id: None,
                            speaker_name: None,
                            thinking: None,
                        });
                    }
                }
                Evt::SessionExported { path, session_id } => {
                    if self.active_session.as_deref() == Some(session_id.as_str()) {
                        let t = i18n::strings(&self.prefs.language);
                        self.status = t.session_export_toast.replace("{path}", &path);
                    }
                }
                Evt::AgentExported { path, agent_id } => {
                    if self.agent_active_tab.as_deref() == Some(agent_id.as_str()) {
                        let t = i18n::strings(&self.prefs.language);
                        self.status = t.agent_export_toast.replace("{path}", &path);
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
                    if kind == "image" || kind == "video" {
                        if kind == "image" {
                            self.last_session_image = Some(path.clone());
                            if prompt.is_empty() {
                                self.image_studio.preview = Some(path.clone());
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
                        } else {
                            self.image_studio.on_video_generated(path.clone());
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
                        speaker_name: None,
                        thinking: None,
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
                    if let Some(question) = self.document_prep_agents.remove(&t.agent_id) {
                        if let Some(path) = aos_agent::document_prep::path_from_trace(&t) {
                            self.attach_document_result_card(&question, &path);
                        }
                    }
                    self.agent_traces.insert(t.agent_id.clone(), t);
                }
                Evt::InferStarted {
                    session_id,
                    inference_id,
                } => {
                    session_chat::on_infer_started(
                        &mut self.session_chat,
                        self.active_session.as_deref(),
                        &session_id,
                        inference_id,
                        &mut self.chat_inference_id,
                    );
                }
                Evt::ChatCancelled { session_id } => {
                    let on_active = session_chat::on_chat_cancelled(
                        &mut self.session_chat,
                        self.active_session.as_deref(),
                        &session_id,
                        &mut self.streaming,
                        &mut self.chat_pending,
                        &mut self.chat_inference_id,
                        &mut self.chat,
                    );
                    if on_active {
                        self.room_turn_pending_text = None;
                        let t = i18n::strings(&self.prefs.language);
                        self.status = t.chat_stopped.into();
                    }
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
                        if icons::close_button(ui).clicked() {
                            dismiss.push(n.agent_id.clone());
                        }
                    });
                }
                self.agent_notices
                    .retain(|x| !dismiss.contains(&x.agent_id));
                if let Some(id) = open_sess {
                    self.tab = Tab::Chat;
                    self.request_session_select(id);
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
                        self.guide.open_topic(guide::GuideTopic::Overview);
                    }
                    if ui.small_button(t.troubleshooting).clicked() {
                        let _ = self.cmd_tx.send(Cmd::Troubleshoot);
                        self.on_tab_open(Tab::Feedback);
                        self.status = t.troubleshooting_status.into();
                    }
                });
            });
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
                overflow_scroll_h(ui, "pending_confirms", 180.0, |ui| {
                    for c in self.confirms.clone() {
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
                });
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
                let r = ui.max_rect();
                ui.painter().line_segment(
                    [r.left_top(), r.right_top()],
                    egui::Stroke::new(1.0, theme::ICE_TRACK),
                );
                self.ui_status_bar(ui, &t);
            });

        self.ui_go_to_palette(ctx, &t);

        let mut restart_onboarding = false;
        guide::show_window(ctx, &mut self.guide, &self.prefs.language, &mut restart_onboarding);
        research_document::show_documents_list(
            ctx,
            &mut self.documents_list,
            &self.research_documents,
            &mut self.document_overlay,
            &t,
        );
        research_document::show_document_overlay(
            ctx,
            &mut self.document_overlay,
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
        if !self.agent_open_tabs.is_empty() {
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
                let dl_busy = self.model_download.is_some();
                let last_session = &mut self.last_session_image;
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
            if icons::close_button(ui)
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
                        let stored_name = agent
                            .display_name
                            .clone()
                            .filter(|n| !n.trim().is_empty())
                            .unwrap_or(label.clone());
                        let _ = self.cmd_tx.send(Cmd::SessionMembersAdd {
                            session_id: session_id.to_string(),
                            member: ChatRoomMember {
                                agent_id: agent.agent_id.clone(),
                                display_name: stored_name,
                                persona_id: None,
                                joined_ms: chat_room::joined_ms_now(),
                            },
                        });
                    }
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
            let after = self.canvas_panel.poll_after_seq();
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

        let g = guide::strings(&self.prefs.language);

        ui.horizontal(|ui| {
            let full_w = ui.available_width();
            let toggle_w = session_toggle_reserve_width(t);
            let left_w = (full_w - toggle_w).max(0.0);

            ui.allocate_ui_with_layout(
                egui::vec2(left_w, ui.available_height()),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
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
                        icons::caret(ui, self.room_members_pane_open);
                    }

                    if guide::tab_help_button(ui, g.help_tooltip) {
                        self.guide.open_topic(guide::GuideTopic::Chat);
                    }
                },
            );

            ui.allocate_ui_with_layout(
                egui::vec2(toggle_w.min(full_w), ui.available_height()),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    if session_toggle_chip(ui, canvas_open, t.session_toggle_canvas) {
                        let new_open = !canvas_open;
                        self.set_canvas_open_local(&sid, new_open);
                        let _ = self.cmd_tx.send(Cmd::CanvasSetOpen {
                            session_id: sid.clone(),
                            open: new_open,
                        });
                    }
                    if session_toggle_chip(ui, room, t.session_toggle_salon) {
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
                },
            );
        });

        if canvas_open {
            let mut toolbar_action: Option<chat_canvas::CanvasUiAction> = None;
            let mut open_canvas_guide = false;
            let toolbar_min_w = chat_canvas::toolbar_content_min_width(
                t,
                self.canvas_panel.seeing,
                self.canvas_panel.clear_confirm_open,
            );
            let track_w = ui.available_width();
            ui.allocate_ui_with_layout(
                egui::vec2(track_w, CANVAS_TOOLBAR_ROW_H),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(track_w);
                    egui::ScrollArea::horizontal()
                        .id_salt("canvas_toolbar_scroll")
                        .auto_shrink([false, false])
                        .max_height(CANVAS_TOOLBAR_ROW_H)
                        .show(ui, |ui| {
                            ui.set_min_width(toolbar_min_w);
                            ui.horizontal(|ui| {
                                ui.set_min_height(CANVAS_TOOLBAR_ROW_H - 4.0);
                                toolbar_action = chat_canvas::ui_canvas_toolbar(
                                    ui,
                                    t,
                                    &mut self.canvas_panel,
                                    Some(g.help_tooltip),
                                    &mut open_canvas_guide,
                                );
                            });
                        });
                },
            );
            if open_canvas_guide {
                self.guide.open_topic(guide::GuideTopic::Canvas);
            }
            if let Some(action) = toolbar_action {
                self.dispatch_canvas_ui_action(Some(action), &sid);
            }
        }

        if room && self.room_members_pane_open {
            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong(t.room_members_heading);
                        if guide::tab_help_button(ui, g.help_tooltip) {
                            self.guide.open_topic(guide::GuideTopic::Salon);
                        }
                    });
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

        ui.add_space(4.0);
    }

    fn ui_chat_transcript(
        &mut self,
        ui: &mut egui::Ui,
        t: &i18n::UiStrings,
        room_mode: bool,
        room_members: &[ChatRoomMember],
        room_conductor_policy: Option<&aos_proto::ChatRoomConductorPolicy>,
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
                let mut act_decision: Option<(String, String, bool)> = None;
                let mut research_choice_pick: Option<(String, usize, research_choice::ResearchChoiceAction)> =
                    None;
                let mut document_progress_action: Option<(usize, research_choice::DocumentProgressAction)> =
                    None;
                let mut document_result_open: Option<(String, String)> = None;
                let mut schedule_act: Option<(String, usize, bool)> = None;
                let tz_offset = local_tz_offset_minutes();
                let chat_now = now_ms();
                let reply_id = self.blocked_ask_agent().map(|a| a.agent_id.clone());
                let n = self.chat.len();
                for i in 0..n {
                    let role = self.chat[i].role.clone();
                    let mut text = self.chat[i].text.clone();
                    let attachments = self.chat[i].attachments.clone();
                    let speaker_id = self.chat[i].speaker_id.clone();
                    let speaker_name = self.chat[i].speaker_name.clone();
                    let thinking = self.chat[i].thinking.clone();
                    let is_completion = attachments.iter().any(|a| {
                        matches!(
                            a,
                            ChatAttachment::AgentRef { origin, .. } if origin == "completion"
                        )
                    });
                    if let Some(act_text) = attachments.iter().find_map(|a| {
                        agent_act_phrase::thread_display_from_attachment(t, a)
                    }) {
                        text = act_text;
                    } else if role == "assistant"
                        && text.trim() == aos_agent::actions::THREAD_FAIL_COULD_NOT_ACT
                    {
                        text = i18n::agent_could_not_act_message(t);
                    } else if role == "assistant"
                        && (text.trim() == aos_agent::actions::THREAD_FAIL_COULD_NOT_CONTINUE
                            || aos_agent::context_budget::is_overflow_fail_reason(text.trim()))
                    {
                        text = i18n::agent_could_not_continue_message(t);
                    }
                    let kind = chat_bubble_kind(&role, speaker_id.as_deref(), room_mode);
                    let text = if kind == ChatBubbleKind::RoomSpeaker {
                        let visible = chat_room::format_room_visible_bubble(&text);
                        chat_room::strip_roster_agent_id_mentions(t, &visible, room_members)
                    } else if role == "assistant"
                        && !is_completion
                        && speaker_id.is_none()
                    {
                        agent_panel::format_chat_assistant_display(&text)
                    } else {
                        text
                    };
                    let mut shown_role = chat_role_label(kind, t, &role);
                    if kind == ChatBubbleKind::RoomSpeaker {
                        if let Some(sid) = speaker_id.as_deref() {
                            shown_role = chat_room::roster_display_name(
                                t,
                                room_members,
                                sid,
                                speaker_name.as_deref(),
                            );
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
                        if kind == ChatBubbleKind::RoomSpeaker {
                            if let Some(th) = thinking.as_deref().filter(|s| !s.trim().is_empty()) {
                                chat_room::room_thinking_toggle(
                                    ui,
                                    t,
                                    i,
                                    th,
                                    &mut self.room_thinking_open,
                                );
                            }
                        }
                        if !text.is_empty() {
                            if role == "assistant" {
                                ui.push_id(("chat_md", i), |ui| {
                                    chat_markdown_viewer(ui).show(
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
                                    if origin == "room" || origin == "document" {
                                        continue;
                                    }
                                    let info = self
                                        .agents
                                        .iter()
                                        .find(|a| a.agent_id == *agent_id);
                                    let selected =
                                        reply_id.as_deref() == Some(agent_id.as_str());
                                    let session_ops = info.and_then(|ag| {
                                        agent_canvas_session_ops(
                                            ag,
                                            self.active_session.as_deref(),
                                            &self.canvas_panel.ops,
                                        )
                                    });
                                    let trace = self.agent_traces.get(agent_id);
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
                                                    session_ops,
                                                    trace,
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
                                        agent_panel::ChatCardAction::Export => {
                                            let _ = self.cmd_tx.send(Cmd::AgentExport {
                                                id: agent_id.clone(),
                                            });
                                        }
                                        agent_panel::ChatCardAction::Retry => {
                                            let _ = self.cmd_tx.send(Cmd::AgentRetry {
                                                id: agent_id.clone(),
                                            });
                                        }
                                        agent_panel::ChatCardAction::Continue => {
                                            let _ = self.cmd_tx.send(Cmd::AgentRetry {
                                                id: agent_id.clone(),
                                            });
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
                                ChatAttachment::Document { path, label } => {
                                    chat_media::render_document(
                                        ui,
                                        t,
                                        label.as_str(),
                                        path.as_str(),
                                    );
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
                                ChatAttachment::AgentAct {
                                    agent_id,
                                    act_id,
                                    state,
                                    ..
                                } => {
                                    if state == "pending" {
                                        ui.horizontal(|ui| {
                                            if ui.button(t.agent_act_allow_once).clicked() {
                                                act_decision = Some((
                                                    agent_id.clone(),
                                                    act_id.clone(),
                                                    true,
                                                ));
                                            }
                                            if ui.button(t.agent_act_deny).clicked() {
                                                act_decision = Some((
                                                    agent_id.clone(),
                                                    act_id.clone(),
                                                    false,
                                                ));
                                            }
                                        });
                                    }
                                }
                                ChatAttachment::SkillOffer { .. } => {
                                    skill_offer::render_skill_offer_card(
                                        ui,
                                        t,
                                        &self.prefs.language,
                                        &self.cmd_tx,
                                        &mut self.chat[i].attachments[j],
                                    );
                                }
                                ChatAttachment::ResearchChoice {
                                    choice_id,
                                    state,
                                    ..
                                } => {
                                    let action = research_choice::render_research_choice(
                                        ui,
                                        t,
                                        state,
                                    );
                                    if action != research_choice::ResearchChoiceAction::None {
                                        research_choice_pick =
                                            Some((choice_id.clone(), i, action));
                                    }
                                }
                                ChatAttachment::DocumentProgress {
                                    question,
                                    agent_id,
                                    state,
                                } => {
                                    let action = research_choice::render_document_progress(
                                        ui,
                                        t,
                                        question,
                                        agent_id,
                                        state,
                                    );
                                    if action != research_choice::DocumentProgressAction::None {
                                        document_progress_action = Some((i, action));
                                    }
                                }
                                ChatAttachment::DocumentResult {
                                    question,
                                    path,
                                    ..
                                } => {
                                    if research_choice::render_document_result(ui, t, question)
                                        == research_choice::DocumentResultAction::Open
                                    {
                                        document_result_open =
                                            Some((question.clone(), path.clone()));
                                    }
                                }
                                ChatAttachment::ScheduleAct {
                                    act_id,
                                    state,
                                    ..
                                } => {
                                    if state == "pending" {
                                        ui.horizontal(|ui| {
                                            if ui.button(t.agent_act_allow_once).clicked() {
                                                schedule_act =
                                                    Some((act_id.clone(), i, true));
                                            }
                                            if ui.button(t.agent_act_deny).clicked() {
                                                schedule_act =
                                                    Some((act_id.clone(), i, false));
                                            }
                                        });
                                    }
                                }
                                ChatAttachment::ScheduleCard {
                                    schedule_id,
                                    title,
                                    state,
                                    next_fire_ms,
                                    ..
                                } => {
                                    let entry = self
                                        .schedules
                                        .iter()
                                        .find(|s| s.id == *schedule_id);
                                    let display_state = schedule_card::resolved_card_state(
                                        entry,
                                        state,
                                    );
                                    let display_next = entry
                                        .map(|e| {
                                            schedule_card::next_fire_ms_for_entry(e, chat_now)
                                        })
                                        .unwrap_or(*next_fire_ms);
                                    let action = ui
                                        .push_id(
                                            ("chat_schedule_card", i, j, schedule_id.as_str()),
                                            |ui| {
                                                schedule_card::render_schedule_card(
                                                    ui,
                                                    t,
                                                    title,
                                                    display_state,
                                                    display_next,
                                                    schedule_id,
                                                    chat_now,
                                                    tz_offset,
                                                )
                                            },
                                        )
                                        .inner;
                                    if !matches!(action, schedule_card::ScheduleCardAction::None)
                                    {
                                        self.apply_schedule_card_action_local(action.clone());
                                    }
                                    schedule_card::send_schedule_action(
                                        &self.cmd_tx,
                                        action,
                                    );
                                }
                            }
                        }
                    });
                }
                if let Some((agent_id, act_id, approved)) = act_decision {
                    let _ = self.cmd_tx.send(Cmd::AgentActDecision {
                        agent_id,
                        act_id,
                        approved,
                    });
                }
                if let Some((choice_id, msg_idx, action)) = research_choice_pick {
                    match action {
                        research_choice::ResearchChoiceAction::Answer => {
                            self.resolve_research_choice_answer(&choice_id, msg_idx);
                        }
                        research_choice::ResearchChoiceAction::Document => {
                            if let Some(sid) = self.active_session.clone() {
                                self.resolve_research_choice_document(
                                    &choice_id,
                                    msg_idx,
                                    &sid,
                                );
                            }
                        }
                        research_choice::ResearchChoiceAction::None => {}
                    }
                }
                if let Some((
                    msg_idx,
                    research_choice::DocumentProgressAction::Stop(agent_id),
                )) = document_progress_action
                {
                    for att in &mut self.chat[msg_idx].attachments {
                        if let ChatAttachment::DocumentProgress { state, .. } = att {
                            *state = "stopped".into();
                        }
                    }
                    self.document_prep_kill_pending += 1;
                    let _ = self.cmd_tx.send(Cmd::AgentKill { id: agent_id });
                }
                if let Some((question, path)) = document_result_open {
                    research_document::open_document(
                        &mut self.document_overlay,
                        &question,
                        &path,
                    );
                }
                if let Some((act_id, msg_idx, approved)) = schedule_act {
                    if approved {
                        self.approve_schedule_act(&act_id, msg_idx);
                    } else {
                        self.deny_schedule_act(&act_id, msg_idx);
                    }
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
                            agent_panel::format_chat_streaming_preview(&self.streaming);
                        ui.push_id("chat_md_stream", |ui| {
                            chat_markdown_viewer(ui).show(
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
                                chat_room::format_turn_speaker_queue(
                                    t,
                                    msg,
                                    room_members,
                                    room_conductor_policy,
                                )
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
        let gap = 8.0_f32;
        let canvas_open = chat_room::active_session_meta(
            &self.sessions,
            self.active_session.as_deref(),
        )
        .map(|m| m.canvas_open)
        .unwrap_or(false);
        let ChatSessionsSplit { side_w, chat_w } = chat_sessions_split(full.x, gap, canvas_open);

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
                        self.request_session_create(Some(format!("Session {n}")));
                    }
                    for s in self.sessions.clone() {
                        let selected =
                            self.active_session.as_deref() == Some(s.id.as_str());
                        let unread = self.session_chat.is_unread(&s.id);
                        let row = ui.horizontal(|ui| {
                            if unread {
                                let t = i18n::strings(&self.prefs.language);
                                icons::status_dot(ui, theme::SIGNAL)
                                    .on_hover_text(t.session_unread_reply);
                            }
                            let title = ui.selectable_label(selected, &s.title);
                            ui.label(
                                egui::RichText::new(format!("({})", s.message_count)).weak(),
                            );
                            title
                        });
                        if row.inner.clicked() || row.response.clicked() {
                            self.request_session_select(s.id.clone());
                        }
                    }
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
                    if ui.button(t.session_export).clicked() {
                        if let Some(id) = self.active_session.clone() {
                            let _ = self.cmd_tx.send(Cmd::SessionExport { id });
                        }
                    }
                    if ui.button("Supprimer").clicked() {
                        if let Some(id) = self.active_session.clone() {
                            self.request_session_delete(id);
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
                    let room_session_meta = chat_room::active_session_meta(
                        &self.sessions,
                        self.active_session.as_deref(),
                    );
                    let room_members: Vec<ChatRoomMember> = room_session_meta
                        .map(|m| m.members.clone())
                        .unwrap_or_default();
                    let room_conductor_policy = room_session_meta.map(|m| m.conductor_policy.clone());
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

                    let ask_queue = self.pending_ask_queue();
                    let session_model = self
                        .sessions
                        .iter()
                        .find(|s| self.active_session.as_deref() == Some(s.id.as_str()))
                        .and_then(|s| s.model_id.clone());
                    let show_vision_banner = !self.chat_pending_images.is_empty()
                        && !session_model_supports_vision(session_model.as_deref());
                    let composer_h = chat_composer_reserve_height(
                        chat_w,
                        ask_queue.len(),
                        self.chat_pending_images.len(),
                        self.chat_pending_documents.len(),
                        show_vision_banner,
                    );
                    let pane_h = ui.available_height();
                    let body_h = (pane_h - composer_h).max(120.0);

                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), body_h),
                        egui::Layout::top_down(egui::Align::Min).with_cross_justify(true),
                        |ui| {
                            self.ui_session_bar(ui, &t);
                            let content_h = ui.available_height().max(80.0);

                            if canvas_open {
                                let split_gap = 8.0_f32;
                                let total_w = ui.available_width();
                                match chat_canvas_layout(total_w, content_h, split_gap) {
                                    ChatCanvasLayout::SideBySide {
                                        transcript_w,
                                        canvas_w,
                                    } => {
                                        ui.horizontal(|ui| {
                                            ui.set_min_height(content_h);
                                            ui.allocate_ui_with_layout(
                                                egui::vec2(transcript_w, content_h),
                                                egui::Layout::top_down(egui::Align::Min)
                                                    .with_cross_justify(true),
                                                |ui| {
                                                    self.ui_chat_transcript(
                                                        ui,
                                                        &t,
                                                        room_mode,
                                                        &room_members,
                                                        room_conductor_policy.as_ref(),
                                                        content_h,
                                                    );
                                                },
                                            );
                                            ui.add_space(split_gap);
                                            ui.allocate_ui_with_layout(
                                                egui::vec2(canvas_w, content_h),
                                                egui::Layout::top_down(egui::Align::Min)
                                                    .with_cross_justify(true),
                                                |ui| {
                                                    if let Some(ref sid) = active_sid {
                                                        let aspect_action =
                                                            chat_canvas::ui_canvas_aspect_row(
                                                                ui, &t, canvas_aspect,
                                                            );
                                                        self.dispatch_canvas_ui_action(
                                                            aspect_action, sid,
                                                        );
                                                        let action =
                                                            chat_canvas::ui_canvas_surface(
                                                                ui,
                                                                &mut self.canvas_panel,
                                                                canvas_aspect,
                                                                t.canvas_empty_hint,
                                                            );
                                                        self.dispatch_canvas_ui_action(
                                                            action, sid,
                                                        );
                                                        self.canvas_poll_if_due(ui, sid);
                                                    }
                                                },
                                            );
                                        });
                                    }
                                    ChatCanvasLayout::Stacked {
                                        transcript_h,
                                        canvas_h,
                                    } => {
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(total_w, transcript_h),
                                            egui::Layout::top_down(egui::Align::Min)
                                                .with_cross_justify(true),
                                            |ui| {
                                                self.ui_chat_transcript(
                                                    ui,
                                                    &t,
                                                    room_mode,
                                                    &room_members,
                                                    room_conductor_policy.as_ref(),
                                                    transcript_h,
                                                );
                                            },
                                        );
                                        ui.add_space(split_gap);
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(total_w, canvas_h),
                                            egui::Layout::top_down(egui::Align::Min)
                                                .with_cross_justify(true),
                                            |ui| {
                                                if let Some(ref sid) = active_sid {
                                                    let aspect_action =
                                                        chat_canvas::ui_canvas_aspect_row(
                                                            ui, &t, canvas_aspect,
                                                        );
                                                    self.dispatch_canvas_ui_action(
                                                        aspect_action, sid,
                                                    );
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
                                    }
                                }
                            } else {
                                self.ui_chat_transcript(
                                    ui,
                                    &t,
                                    room_mode,
                                    &room_members,
                                    room_conductor_policy.as_ref(),
                                    content_h,
                                );
                            }
                        },
                    );

                    let completions = slash_completions(&self.input);
                    let mention_hits = if room_mode {
                        chat_room::mention_completions(&self.input, &room_members, &t)
                    } else {
                        Vec::new()
                    };
                    let mut chat_sent_this_frame = false;
                    let input_row = ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), composer_h),
                        egui::Layout::bottom_up(egui::Align::Min),
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
                            let show_stop = self.chat_pending
                                && (room_mode || self.chat_inference_id.is_some());
                            let item_gap = ui.spacing().item_spacing.x;
                            let send_w = send_button_reserved_width(ui, &t);
                            let stop_w = if show_stop {
                                stop_button_reserved_width(ui, &t)
                            } else {
                                0.0
                            };

                            let mut attach_from_menu = false;
                            let mut attach_document_from_menu = false;
                            let mut reuse_last_image = false;
                            let mut send_clicked = false;
                            let mut input_response: Option<egui::Response> = None;

                            let mut run_attach_menu = |ui: &mut egui::Ui| {
                                icons::attach_menu(ui, "chat_attach", t.chat_attach_image, |ui| {
                                    if self.last_session_image.is_some()
                                        && ui.button(t.chat_last_session_image).clicked()
                                    {
                                        reuse_last_image = true;
                                    }
                                    if ui.button(t.chat_attach_image).clicked() {
                                        attach_from_menu = true;
                                    }
                                    if ui.button(t.chat_attach_document).clicked() {
                                        attach_document_from_menu = true;
                                    }
                                });
                            };

                            let row_w = ui.available_width();
                            let input_h = ui.spacing().interact_size.y;
                            let field_w = composer_field_width(
                                row_w,
                                send_w,
                                icons::ATTACH_BTN_W,
                                stop_w,
                                item_gap,
                                show_stop,
                            );

                            ui.allocate_ui_with_layout(
                                egui::vec2(row_w, input_h),
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if show_stop {
                                        if room_mode {
                                            if ui
                                                .add_sized(
                                                    egui::vec2(stop_w, input_h),
                                                    egui::Button::new(t.chat_stop),
                                                )
                                                .clicked()
                                            {
                                                if let Some(sid) = self.active_session.clone() {
                                                    let _ = self.cmd_tx.send(
                                                        Cmd::RoomTurnCancel {
                                                            session_id: sid,
                                                        },
                                                    );
                                                }
                                            }
                                        } else if let Some(id) = self.chat_inference_id {
                                            if ui
                                                .add_sized(
                                                    egui::vec2(stop_w, input_h),
                                                    egui::Button::new(t.chat_stop),
                                                )
                                                .clicked()
                                            {
                                                if let Some(sid) = self.active_session.clone() {
                                                    let _ = self.cmd_tx.send(Cmd::ChatCancel {
                                                        inference_id: id,
                                                        session_id: sid,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                    let send_btn = ui
                                        .add_sized(
                                            egui::vec2(send_w, input_h),
                                            egui::Button::new(t.agent_send),
                                        )
                                        .on_hover_text(t.tip_send);
                                    send_clicked |= send_btn.clicked();

                                    ui.allocate_ui_with_layout(
                                        egui::vec2(icons::ATTACH_BTN_W, input_h),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            run_attach_menu(ui);
                                        },
                                    );

                                    ui.set_width(field_w);
                                    let r = ui.add(
                                        egui::TextEdit::singleline(&mut self.input)
                                            .id_salt("chat_input")
                                            .desired_width(field_w)
                                            .hint_text(&hint),
                                    );
                                    input_response = Some(r);
                                },
                            );

                            if attach_from_menu {
                                if let Some(path) = os_open::pick_os_file(
                                    t.chat_attach_image,
                                    &[("Images", &["png", "jpg", "jpeg", "webp"])],
                                    os_open::user_downloads_dir().as_deref(),
                                ) {
                                    self.queue_chat_image(path.to_string_lossy().into_owned());
                                }
                            } else if attach_document_from_menu {
                                if let Some(path) = os_open::pick_os_file(
                                    t.chat_attach_document,
                                    &[(
                                        "Documents",
                                        aos_proto::chat_document::CHAT_DOCUMENT_EXTENSIONS,
                                    )],
                                    os_open::user_downloads_dir().as_deref(),
                                ) {
                                    self.queue_chat_document(
                                        path.to_string_lossy().into_owned(),
                                    );
                                }
                            } else if reuse_last_image {
                                if let Some(last) = self.last_session_image.clone() {
                                    self.queue_chat_image(last);
                                }
                            }

                            if let Some(r) = input_response {
                                if self.chat_refocus {
                                    r.request_focus();
                                    self.chat_refocus = false;
                                }
                                let send = send_clicked
                                    || (r.lost_focus()
                                        && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                                if send {
                                    self.send_chat();
                                    chat_sent_this_frame = true;
                                    self.chat_refocus = true;
                                }
                            }

                            let composer_input_rect = ui.min_rect();
                            if !self.chat_pending_images.is_empty()
                                || !self.chat_pending_documents.is_empty()
                            {
                                let ctx = ui.ctx().clone();
                                chat_media::render_pending_attachment_chips(
                                    ui,
                                    &ctx,
                                    &mut self.chat_pending_images,
                                    &mut self.chat_pending_documents,
                                );
                            }
                            if show_vision_banner {
                                let t = i18n::strings(&self.prefs.language);
                                ui.horizontal(|ui| {
                                    ui.weak(t.chat_vision_banner);
                                    if ui.small_button(t.chat_load_vision_model).clicked() {
                                        self.load_preferred_vision_model();
                                    }
                                });
                            }
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
                            composer_input_rect
                        },
                    );
                    let input_rect = input_row.inner;

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
                            if !chat_sent_this_frame {
                                self.input = text;
                                self.chat_refocus = true;
                            }
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
                            if !chat_sent_this_frame {
                                self.input = text;
                                self.chat_refocus = true;
                            }
                        }
                    }
                },
            );
        });
    }



}

#[cfg(test)]
mod delegate_tests {
    use super::*;
    use aos_proto::CanvasAspect;

    const ASPECT: CanvasAspect = CanvasAspect::Square;

    fn full_canvas_exported() -> Vec<String> {
        aos_agent::tools::CANVAS_TOOL_IDS
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    }

    #[test]
    fn create_module_dump_delegates_instead_of_display() {
        let dumped = r#"{"kind":"column","children":[{"kind":"heading","text":"Ping"}]}"#;
        let spec = chat_delegate_agent_spec("crée un module ping", dumped, false, ASPECT, &full_canvas_exported());
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
            &full_canvas_exported(),
        )
        .is_none());
    }

    #[test]
    fn model_scaffold_action_delegates() {
        let out = r#"{"action":"module.scaffold","args":{"name":"ping"}}"#;
        let spec = chat_delegate_agent_spec("fais un ping", out, false, ASPECT, &full_canvas_exported());
        let (_brief, _skills, tools, _) = spec.expect("doit déléguer");
        assert!(tools.iter().any(|x| x == "module.scaffold"));
    }

    #[test]
    fn tts_ask_does_not_delegate_agent() {
        let out = r#"{"action":"agent.spawn","args":{"brief":"tts"}}"#;
        assert!(chat_delegate_agent_spec("génère un audio qui dit bonjour", out, false, ASPECT, &full_canvas_exported()).is_none());
        let (_skills, tools) = chat_agent_kit("génère un audio de bonjour");
        assert!(tools.iter().any(|t| t == "media.audio.generate"));
    }

    #[test]
    fn draw_request_delegates_with_image_tools_when_canvas_closed() {
        let spec = chat_delegate_agent_spec("dessine une maison", "Ok.", false, ASPECT, &full_canvas_exported());
        let (_brief, _skills, tools, _prose) = spec.expect("doit déléguer image");
        assert!(tools.iter().any(|x| x == "media.image.generate"));
        assert!(!tools.iter().any(|x| x == "canvas.stroke"));
    }

    #[test]
    fn draw_request_delegates_with_canvas_tools_when_canvas_open() {
        let spec = chat_delegate_agent_spec("dessine une maison", "Ok.", true, ASPECT, &full_canvas_exported())
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
        let spec = chat_delegate_agent_spec("dessine sur le canvas une maison", "Ok.", false, ASPECT, &full_canvas_exported());
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
            &full_canvas_exported(),
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
        let spec = chat_delegate_agent_spec("dessine dans le canvas", "Ok.", false, ASPECT, &full_canvas_exported())
            .expect("dessine dans le canvas doit déléguer canvas");
        let (_brief, _skills, tools, _prose) = spec;
        assert!(tools.iter().any(|x| x == "canvas.stroke"));
        assert!(!tools.iter().any(|x| x == "media.image.generate"));
        assert!(!tools.iter().any(|x| x == "user.ask"));
    }

    #[test]
    fn bare_dessine_delegates_with_image_tools() {
        let spec = chat_delegate_agent_spec("dessine une maison", "Ok.", false, ASPECT, &full_canvas_exported())
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
            &full_canvas_exported(),
        )
        .is_none());
        assert!(chat_delegate_agent_spec("vas y", "Ok.", true, ASPECT, &full_canvas_exported()).is_none());
    }

    #[test]
    fn canvas_truncated_spawn_explicit_canvas_delegates() {
        let out = r#"{"action":"agent.spawn","args":{"brief":"Génération d'une maison médiévale avec plus de détails en cours..."#;
        let spec = chat_delegate_agent_spec("dessine sur le canvas", out, false, ASPECT, &full_canvas_exported());
        let (_brief, _skills, tools, _) = spec.expect("JSON tronqué + explicit canvas");
        assert!(tools.iter().any(|x| x == "canvas.stroke"));
    }

    #[test]
    fn canvas_truncated_spawn_followup_does_not_delegate() {
        let out = r#"{"action":"agent.spawn","args":{"brief":"Génération..."#;
        assert!(chat_delegate_agent_spec("vas y", out, true, ASPECT, &full_canvas_exported()).is_none());
    }

    #[test]
    fn explicit_canvas_after_image_delegate_gets_canvas_tools() {
        let image = chat_delegate_agent_spec("dessine une maison", "Ok.", false, ASPECT, &full_canvas_exported())
            .expect("image delegate");
        let image_tools = image.2;
        assert!(image_tools.iter().any(|x| x == "media.image.generate"));
        assert!(!image_tools.iter().any(|x| x == "canvas.stroke"));

        let canvas = chat_delegate_agent_spec("dessine sur le canvas", "Ok.", false, ASPECT, &full_canvas_exported())
            .expect("canvas delegate after image");
        let canvas_tools = canvas.2;
        assert!(canvas_tools.iter().any(|x| x == "canvas.stroke"));
        assert!(!canvas_tools.iter().any(|x| x == "media.image.generate"));
    }

    #[test]
    fn prompts_never_mention_canvas_draw() {
        let brief = chat_canvas::canvas_agent_brief(
            "dessine sur le canvas",
            ASPECT,
            &full_canvas_exported(),
        );
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
            &full_canvas_exported(),
        )
        .is_none());
    }

    #[test]
    fn delegate_kit_includes_canvas_path_when_module_exports_it() {
        let exported = vec![
            "canvas.path".into(),
            "canvas.stroke".into(),
            "canvas.get".into(),
        ];
        let (_, tools) = chat_delegate_kit("dessine un moulin", true, true, &exported);
        assert!(tools.iter().any(|t| t == "canvas.path"));
        let brief = chat_canvas::canvas_agent_brief("dessine un moulin", ASPECT, &exported);
        assert!(brief.contains("canvas.path"));
        assert!(brief.contains("pièce manquante"));
    }

    #[test]
    fn delegate_kit_omits_canvas_path_when_module_does_not_export_it() {
        let exported = vec![
            "canvas.stroke".into(),
            "canvas.spline".into(),
            "canvas.rect".into(),
            "canvas.get".into(),
        ];
        let (_, tools) = chat_delegate_kit("dessine un moulin", true, true, &exported);
        assert!(!tools.iter().any(|t| t == "canvas.path"));
        let brief = chat_canvas::canvas_agent_brief("dessine un moulin", ASPECT, &exported);
        assert!(!brief.contains("canvas.path"));
    }
}

#[cfg(test)]
mod research_document_tests {
    use aos_agent::document_prep::{compose_document, BrowsePage};
    use aos_proto::WebSearchHit;

    #[test]
    fn user_requested_document_skips_choice_card() {
        assert!(aos_agent::research_detect::user_requested_document(
            "Please prepare a document about agentic apps"
        ));
    }

    #[test]
    fn research_shaped_ask_triggers_choice_path() {
        assert!(aos_agent::research_detect::is_research_shaped_ask(
            "what is the state of the art of agentic apps?"
        ));
    }

    #[test]
    fn answer_choice_does_not_require_document_footnotes() {
        assert!(!crate::research_choice::choice_actions_enabled("answer"));
    }

    #[test]
    fn prepare_document_mock_has_verified_footnote() {
        let hits = vec![WebSearchHit {
            title: "Agentic apps overview".into(),
            url: "https://example.org/agentic".into(),
            snippet: "Tool-using agents are maturing.".into(),
        }];
        let pages = vec![BrowsePage {
            url: "https://example.org/agentic".into(),
            title: "Agentic apps overview".into(),
            text: "Full page body.".into(),
            fetch_error: None,
        }];
        let md = compose_document("Agentic apps?", &hits, &pages);
        assert!(md.contains("[^1]: Agentic apps overview — https://example.org/agentic"));
        assert!(!md.contains("https://invented.example"));
    }
}

#[cfg(test)]
mod canvas_completion_tests {
    use super::*;
    use aos_proto::{AgentInfo, AgentKind, AgentState, AgentTrace};

    fn canvas_agent(agent_id: &str) -> AgentInfo {
        AgentInfo {
            agent_id: agent_id.into(),
            state: AgentState::Failed,
            directive: "dessine un moulin".into(),
            pid: None,
            caps: vec![],
            last_output: String::new(),
            step: 64,
            max_steps: 64,
            current_task: None,
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: vec![],
            tools: vec!["canvas.stroke".into()],
            mcp_servers: vec![],
            fail_reason: Some("max_steps (64) atteint".into()),
            session_id: Some("sess-1".into()),
            title: String::new(),
            kind: AgentKind::Task,
            display_name: None,
            persona_id: None,
            origin: None,
        }
    }

    #[test]
    fn completion_chat_muted_when_canvas_has_traits() {
        let t = i18n::strings("fr");
        let ag = canvas_agent("agent-102");
        let ops = vec![aos_proto::CanvasOp {
            seq: 1,
            author_id: "agent-102".into(),
            ts_ms: 0,
            body: aos_proto::CanvasOpBody::Stroke {
                points: vec![aos_proto::CanvasPoint { x: 0.1, y: 0.2 }],
                color: "#3ee0c4".into(),
                width: 0.01,
            },
        }];
        let text = agent_completion_chat_text(&ag, &t, Some(&ops), None);
        assert!(text.is_empty(), "expected mute, got: {text}");
        assert!(!text.contains("agent-102"));
        assert!(!text.contains("terminé"));
    }

    #[test]
    fn completion_chat_empty_canvas_shows_locked_fail_copy() {
        let t = i18n::strings("fr");
        let ag = canvas_agent("agent-90");
        let text = agent_completion_chat_text(&ag, &t, None, None);
        assert_eq!(text, t.canvas_draw_failed);
        assert!(!text.contains("max_steps"));
        assert!(!text.contains("agent-90"));
    }

    #[test]
    fn completion_chat_muted_from_trace_traits_without_session_ops() {
        let t = i18n::strings("en");
        let ag = canvas_agent("agent-99");
        let trace = AgentTrace {
            agent_id: "agent-99".into(),
            steps: vec![aos_proto::AgentStepRecord {
                step: 1,
                action: "canvas.spline".into(),
                tool_result: "ok seq=1".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let text = agent_completion_chat_text(&ag, &t, None, Some(&trace));
        assert!(text.is_empty());
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn chat_sessions_split_never_exceeds_total() {
        for full_w in [250.0, 400.0, 900.0, 1280.0] {
            let gap = 8.0;
            let split = chat_sessions_split(full_w, gap, false);
            assert!(
                split.side_w + gap + split.chat_w <= full_w + 0.01,
                "full_w={full_w} side={} chat={}",
                split.side_w,
                split.chat_w
            );
        }
    }

    #[test]
    fn chat_canvas_side_by_side_widths_fit() {
        let gap = 8.0;
        let layout = chat_canvas_layout(500.0, 400.0, gap);
        match layout {
            ChatCanvasLayout::SideBySide {
                transcript_w,
                canvas_w,
            } => {
                assert!(transcript_w + gap + canvas_w <= 500.0 + 0.01);
            }
            ChatCanvasLayout::Stacked { .. } => panic!("expected side-by-side at 500px"),
        }
    }

    #[test]
    fn session_toggle_reserve_fits_fr_canvas_label() {
        let fr = i18n::strings("fr");
        let w = session_toggle_reserve_width(&fr);
        assert!(
            w >= estimate_label_chip_w(fr.session_toggle_canvas) + 40.0,
            "reserve {w} should fit full Canvas label"
        );
    }

    #[test]
    fn chat_canvas_stacks_before_side_by_side_threshold() {
        let gap = 8.0;
        let layout = chat_canvas_layout(300.0, 400.0, gap);
        assert!(matches!(layout, ChatCanvasLayout::Stacked { .. }));
    }

    #[test]
    fn composer_field_width_reserves_fr_envoyer() {
        let fr = i18n::strings("fr");
        let send_w = estimate_composer_buttons_w(fr.agent_send, false, "");
        let field = composer_field_width(420.0, send_w, icons::ATTACH_BTN_W, 0.0, 4.0, false);
        assert!(field > 80.0);
        assert!(send_w >= estimate_composer_buttons_w("Envoyer", false, ""));
    }

    #[test]
    fn composer_field_width_at_900_central_pane() {
        let fr = i18n::strings("fr");
        let send_w = estimate_composer_buttons_w(fr.agent_send, false, "");
        let stop_w = estimate_composer_buttons_w("Stop", false, "");
        let field =
            composer_field_width(580.0, send_w, icons::ATTACH_BTN_W, stop_w, 4.0, true);
        assert!(field > 200.0);
        assert!(
            field + send_w + stop_w + icons::ATTACH_BTN_W + 12.0 <= 580.0 + 0.01
        );
    }

    #[test]
    fn composer_reserve_height_is_single_row() {
        let h = chat_composer_reserve_height(400.0, 0, 0, 0, false);
        assert!((h - COMPOSER_INPUT_ROW_H).abs() < 0.01);
    }

    #[test]
    fn composer_row_reserved_fits_fr_labels() {
        let fr = i18n::strings("fr");
        let reserved = composer_row_reserved_width(&fr, true);
        assert!(reserved > icons::ATTACH_BTN_W + 80.0);
    }

    #[test]
    fn composer_wraps_when_too_narrow() {
        let buttons = estimate_composer_buttons_w("Envoyer", true, "Stop");
        assert!(chat_composer_wraps(280.0, icons::ATTACH_BTN_W, buttons));
        assert!(!chat_composer_wraps(900.0, icons::ATTACH_BTN_W, buttons));
    }

    #[test]
    fn preview_min_width_fits_fr_composer_and_toggles() {
        let fr = i18n::strings("fr");
        let min_w = preview_min_inner_width(&fr);
        let composer = composer_row_reserved_width(&fr, true) + COMPOSER_MIN_INPUT_W;
        assert!(min_w >= LEFT_NAV_W + CHAT_SIDE_MIN_W + composer);
        assert!(min_w >= LEFT_NAV_W + session_toggle_reserve_width(&fr) + 200.0);
    }

    #[test]
    fn bubble_max_width_never_exceeds_available() {
        for w in [80.0, 150.0, 400.0] {
            for kind in [
                ChatBubbleKind::User,
                ChatBubbleKind::Assistant,
                ChatBubbleKind::System,
            ] {
                let max_w = chat_bubble_max_width(w, kind);
                assert!(max_w <= w + 0.01, "kind={kind:?} avail={w} max={max_w}");
            }
        }
    }
}
