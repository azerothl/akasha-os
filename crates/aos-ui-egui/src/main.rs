//! Akasha OS Preview — UI egui (ADR 0003).
//!
//! Surface testeur : chat, dashboard, onboarding, notes, confirm, agents,
//! audit, scénarios guidés, retours (`feedback.submit`).

mod agent_panel;
mod decl_ui;
mod i18n;
mod model_setup;
mod notes_panel;
mod prefs;
mod tasks_panel;
mod chat_ask;
mod chat_media;
mod cmd;
mod image_studio;
mod os_open;
mod runtime;
mod scenarios_panel;
mod slash;

use chat_ask::{agent_display_title, chat_has_open_ask, pending_ask_ids};
use cmd::{AgentNotice, ChatLine, Cmd, Evt};
use os_open::{aos_home, app_icon, bin_aos_session, native_path, open_in_browser, open_os_folder};
use runtime::runtime_main;
use slash::{slash_completions, slash_insert_text, SLASH_COMMANDS};
use aos_agent::schedule::ScheduleEntry;
use aos_ipc::BusClient;
use aos_proto::{
    AgentCreateRequest, AgentGoal, AgentIdRequest, AgentInfo, AgentState, AgentTrace, AuditEvent,
    CapInfo, ChatAttachment, ChatSessionAppendRequest, ChatSessionGetResponse, ChatSessionIdRequest, ChatSessionMeta, DocumentRef,
    FeedbackSubmitRequest, FeedbackSubmitResponse, McpServerInfo, MemHit, ModelInfo,
    ModuleCatalogue, ModuleIdRequest, ModuleInfo, ModuleInvokeRequest, ModuleInvokeResponse,
    PendingConfirmation, ProviderRecord,
    SkillInfo, SystemMetrics, WebSearchHit,
    chat_user_wants_module_authoring,
};
use aos_proto::decl_ui::ModuleUiResponse;
use prefs::{load_preferences, save_preferences, Preferences};
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

#[derive(Clone, PartialEq, Eq)]
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
}

impl Default for OnboardingState {
    fn default() -> Self {
        Self {
            completed: false,
            language: "en".into(),
            routing: "local_only".into(),
            trust_default: "medium".into(),
            tutorial_step: 0,
        }
    }
}

fn apply_theme(ctx: &egui::Context, theme: &str) {
    match theme {
        "light" => ctx.set_visuals(egui::Visuals::light()),
        "soft" => {
            let mut v = egui::Visuals::light();
            v.override_text_color = Some(egui::Color32::from_rgb(40, 44, 52));
            v.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(245, 246, 248);
            v.widgets.inactive.bg_fill = egui::Color32::from_rgb(232, 236, 240);
            v.panel_fill = egui::Color32::from_rgb(250, 250, 252);
            v.window_fill = egui::Color32::from_rgb(255, 255, 255);
            ctx.set_visuals(v);
        }
        "high_contrast" => {
            let mut v = egui::Visuals::dark();
            v.override_text_color = Some(egui::Color32::WHITE);
            v.widgets.noninteractive.fg_stroke =
                egui::Stroke::new(1.5_f32, egui::Color32::WHITE);
            v.widgets.inactive.fg_stroke = egui::Stroke::new(1.5_f32, egui::Color32::WHITE);
            v.widgets.hovered.fg_stroke = egui::Stroke::new(2.0_f32, egui::Color32::YELLOW);
            v.widgets.active.fg_stroke = egui::Stroke::new(2.0_f32, egui::Color32::YELLOW);
            v.selection.bg_fill = egui::Color32::from_rgb(0, 90, 200);
            v.extreme_bg_color = egui::Color32::BLACK;
            v.panel_fill = egui::Color32::BLACK;
            v.window_fill = egui::Color32::from_rgb(10, 10, 10);
            ctx.set_visuals(v);
        }
        _ => ctx.set_visuals(egui::Visuals::dark()),
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

fn agent_is_live(state: &AgentState) -> bool {
    matches!(
        state,
        AgentState::Created | AgentState::Running | AgentState::Paused | AgentState::Blocked
    )
}

fn agent_completion_chat_text(ag: &AgentInfo) -> String {
    let title = {
        let t = ag.directive.trim();
        if t.is_empty() {
            ag.agent_id.clone()
        } else {
            t.chars().take(80).collect()
        }
    };
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
                    }
                })
                .collect();
            let _ = evt_tx.send(Evt::SessionLoaded {
                id: resp.meta.id,
                messages,
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

/// Si le chat doit déléguer : (brief, skills, tools, phrase d'accusé).
pub(crate) fn chat_delegate_agent_spec(
    user_text: &str,
    model_output: &str,
) -> Option<(String, Vec<String>, Vec<String>, String)> {
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
            let (mut skills, mut tools) = chat_agent_kit(&brief);
            merge_named_args(&mut skills, &action.args, "skills");
            merge_named_args(&mut tools, &action.args, "tools");
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
                } else {
                    "Je lance un agent pour cette tâche.".into()
                };
            }
            return Some((brief, skills, tools, prose));
        }
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
    None
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
) {
    let mut req = AgentCreateRequest::simple(brief.clone());
    req.skills = skills;
    req.tools = tools;
    req.session_id = Some(sid.clone());
    req.goal = Some(AgentGoal {
        statement: brief.clone(),
        success_criteria: vec![],
        max_steps,
        max_subagents: CHAT_AGENT_MAX_SUBAGENTS,
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
                title: brief.clone(),
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
                title: brief,
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
    /// Agent visé pour la prochaine réponse `user.ask` (plusieurs bloqués).
    ask_reply_target: Option<String>,
    /// Re-focus chat TextEdit after send (Enter clears focus).
    chat_refocus: bool,
    decl_panels: HashMap<String, decl_ui::DeclUiPanelState>,
    decl_md_cache: CommonMarkCache,
    image_studio: image_studio::ImageStudioState,
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
            ask_reply_target: None,
            chat_refocus: false,
            decl_panels: HashMap::new(),
            decl_md_cache: CommonMarkCache::default(),
            image_studio: image_studio::ImageStudioState::default(),
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
        if let Some((agent_id, title)) = self
            .blocked_ask_agent()
            .map(|ag| (ag.agent_id.clone(), ag.directive.clone()))
        {
            self.send_ask_reply(session_id, agent_id, title, text);
            return;
        }
        if self.chat_pending {
            self.chat.push(ChatLine::plain("user", text));
            self.chat.push(ChatLine::plain(
                "système",
                "réponse précédente encore en cours — patientez.",
            ));
            return;
        }
        self.chat.push(ChatLine::plain("user", text.clone()));
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
        let _ = self.cmd_tx.send(Cmd::Chat {
            session_id,
            history,
            user_text: text,
            model_id,
            auto_remember: self.prefs.auto_remember_chat,
            max_steps: chat_agent_max_steps(self.prefs.default_max_steps),
            routing: self.prefs.routing.clone(),
        });
        self.scen_chat = true;
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
                let _ = self.cmd_tx.send(Cmd::MediaImage {
                    prompt: rest.to_string(),
                    model_id: self.prefs.default_image_model.clone(),
                    options: self.prefs.image_options(),
                });
            }
            "/speak" => {
                if rest.is_empty() {
                    self.chat
                        .push(ChatLine::plain("système", "usage : /speak <texte>"));
                    return;
                }
                let att = ChatAttachment::TtsDraft {
                    text: rest.to_string(),
                    model_id: self.prefs.default_audio_model.clone(),
                    options: aos_proto::MediaAudioOptions::default(),
                };
                self.chat.push(ChatLine {
                    role: "assistant".into(),
                    text: rest.to_string(),
                    attachments: vec![att.clone()],
                });
                if let Some(sid) = self.active_session.clone() {
                    let _ = self.cmd_tx.send(Cmd::SessionAppend {
                        session_id: sid,
                        role: "assistant".into(),
                        content: rest.to_string(),
                        attachments: vec![att],
                    });
                }
                self.status = i18n::strings(&self.prefs.language).tts_card_blurb.into();
            }
            _ => {
                self.chat.push(ChatLine::plain(
                    "système",
                    format!("commande inconnue : {cmd} — tapez /commands"),
                ));
            }
        }
    }
}

impl eframe::App for UiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_theme(ctx, &self.prefs.theme);
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
                        });
                    }
                    self.streaming.clear();
                    self.chat_pending = false;
                    self.chat_inference_id = None;
                    if self.status.starts_with("assistant :") {
                        self.status.clear();
                    }
                }
                Evt::Error(m) => {
                    self.status = m.clone();
                    self.chat.push(ChatLine::plain("système", m));
                    self.streaming.clear();
                    self.chat_pending = false;
                    self.chat_inference_id = None;
                }
                Evt::Status(m) => self.status = m,
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
                                        });
                                    }
                                } else if !seeding
                                    && !on_this_session
                                    && !self.agent_notified.contains(&ag.agent_id)
                                    && was_active
                                {
                                    let summary = match ag.state {
                                        AgentState::Done => format!("{} terminé", ag.agent_id),
                                        AgentState::Failed => format!(
                                            "{} échoué — {}",
                                            ag.agent_id,
                                            ag.fail_reason.as_deref().unwrap_or("échec")
                                        ),
                                        AgentState::Killed => format!("{} arrêté", ag.agent_id),
                                        _ => format!("{} terminé", ag.agent_id),
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
                Evt::SessionLoaded { id, messages } => {
                    self.active_session = Some(id.clone());
                    self.rename_buf = self
                        .sessions
                        .iter()
                        .find(|s| s.id == id)
                        .map(|s| s.title.clone())
                        .unwrap_or_default();
                    let mut chat = vec![ChatLine::plain(
                        "système",
                        format!("Session {id} — historique rechargé."),
                    )];
                    chat.extend(messages);
                    self.chat = chat;
                    self.streaming.clear();
                    self.chat_pending = false;
                    self.chat_inference_id = None;
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
                Evt::MediaOk {
                    kind,
                    path,
                    bytes,
                    engine,
                    prompt,
                } => {
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
                        self.image_studio.open_from_chat(&prompt, &path);
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
                Evt::AgentTrace(t) => {
                    self.agent_traces.insert(t.agent_id.clone(), t);
                }
                Evt::InferStarted { inference_id } => {
                    self.chat_inference_id = Some(inference_id);
                }
                Evt::ChatCancelled => {
                    self.chat_pending = false;
                    self.chat_inference_id = None;
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

        if self.show_onboarding {
            egui::Window::new(t.tutorial_title)
                .collapsible(false)
                .resizable(true)
                .default_width(520.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    let step = self.onboarding.tutorial_step;
                    ui.label(t.step_of.replace("{}", &(step + 1).to_string()));
                    ui.separator();
                    match step {
                        0 => {
                            ui.heading(t.welcome);
                            ui.label(t.preview_banner.replace("{}", &self.version));
                            ui.label(t.welcome_body1);
                            ui.label(t.welcome_body2);
                        }
                        1 => {
                            ui.heading(t.preferences);
                            ui.label(t.language);
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut self.onboarding.language, "fr".into(), "Français");
                                ui.radio_value(&mut self.onboarding.language, "en".into(), "English");
                            });
                            ui.label(t.routing);
                            ui.horizontal(|ui| {
                                ui.radio_value(
                                    &mut self.onboarding.routing,
                                    "local_only".into(),
                                    t.routing_local,
                                );
                                ui.radio_value(
                                    &mut self.onboarding.routing,
                                    "balanced".into(),
                                    "balanced",
                                );
                            });
                            ui.label(t.trust_default);
                            ui.horizontal(|ui| {
                                ui.radio_value(
                                    &mut self.onboarding.trust_default,
                                    "low".into(),
                                    t.trust_low,
                                );
                                ui.radio_value(
                                    &mut self.onboarding.trust_default,
                                    "medium".into(),
                                    t.trust_medium,
                                );
                            });
                        }
                        2 => {
                            ui.heading(t.product_tour);
                            ui.label(t.tour_chat);
                            ui.label(t.tour_memory);
                            ui.label(t.tour_notes);
                            ui.label(t.tour_agents);
                            ui.label(t.tour_network);
                            ui.label(t.tour_feedback);
                        }
                        _ => {
                            ui.heading(t.test_path);
                            ui.label(t.test_path_body1);
                            ui.label(t.test_path_body2);
                            ui.label(t.test_path_body3);
                        }
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        if step > 0 && ui.button(t.prev).clicked() {
                            self.onboarding.tutorial_step = step - 1;
                            save_onboarding(&self.onboarding);
                        }
                        if step < 3 {
                            if ui.button(t.next).clicked() {
                                if step == 1 {
                                    self.prefs.language = self.onboarding.language.clone();
                                    self.prefs.routing = self.onboarding.routing.clone();
                                    self.prefs.trust_default = self.onboarding.trust_default.clone();
                                    save_preferences(&self.prefs);
                                    let _ = self.cmd_tx.send(Cmd::SetRouting {
                                        mode: self.prefs.routing.clone(),
                                    });
                                }
                                self.onboarding.tutorial_step = step + 1;
                                save_onboarding(&self.onboarding);
                            }
                        } else if ui.button(t.finish_tutorial).clicked() {
                            self.prefs.language = self.onboarding.language.clone();
                            self.prefs.routing = self.onboarding.routing.clone();
                            self.prefs.trust_default = self.onboarding.trust_default.clone();
                            save_preferences(&self.prefs);
                            let _ = self.cmd_tx.send(Cmd::SetRouting {
                                mode: self.prefs.routing.clone(),
                            });
                            self.onboarding.completed = true;
                            self.onboarding.tutorial_step = 3;
                            save_onboarding(&self.onboarding);
                            self.show_onboarding = false;
                            self.tab = Tab::Scenarios;
                            self.status = t.tutorial_done_status.into();
                        }
                        if ui.button(t.skip).clicked() {
                            self.prefs.language = self.onboarding.language.clone();
                            self.prefs.routing = self.onboarding.routing.clone();
                            self.prefs.trust_default = self.onboarding.trust_default.clone();
                            save_preferences(&self.prefs);
                            self.onboarding.completed = true;
                            save_onboarding(&self.onboarding);
                            self.show_onboarding = false;
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
                ui.colored_label(egui::Color32::from_rgb(220, 160, 40), t.preview_banner.replace("{}", &self.version));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t.report).clicked() {
                        self.tab = Tab::Feedback;
                    }
                    if ui.button(t.tutorial).clicked() {
                        self.onboarding.tutorial_step = 0;
                        self.onboarding.completed = false;
                        self.show_onboarding = true;
                        save_onboarding(&self.onboarding);
                    }
                    if ui.button(t.troubleshooting).clicked() {
                        let _ = self.cmd_tx.send(Cmd::Troubleshoot);
                        self.tab = Tab::Feedback;
                        self.status = t.troubleshooting_status.into();
                    }
                    ui.label(format!("v{}", self.version));
                });
            });
            if let Some(offer) = self.update_offer.clone() {
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
                    format!("{} confirmation(s) en attente", self.confirms.len()),
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
                        ui.label(format!(
                            "{} — {} sur {}\n{}",
                            c.id, c.action, c.target, c.reason
                        ));
                        if rich {
                            ui.colored_label(
                                egui::Color32::from_rgb(220, 180, 80),
                                "Extension OS : revue des caps / manifeste requise",
                            );
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Accepter").clicked() {
                                let _ = self.cmd_tx.send(Cmd::Confirm {
                                    id: c.id.clone(),
                                    approved: true,
                                });
                                self.scen_confirm = true;
                            }
                            if ui.button("Refuser").clicked() {
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

        egui::SidePanel::left("tabs").exact_width(140.0).show(ctx, |ui| {
            overflow_scroll(ui, "nav_sidebar", |ui| {
            ui.heading("Preview");
            for (tab, label, hint) in [
                (Tab::Chat, t.tab_chat, t.tab_hint_chat),
                (Tab::Memory, t.tab_memory, t.tab_hint_memory),
                (Tab::Notes, t.tab_notes, t.tab_hint_notes),
                (Tab::Tasks, t.tab_tasks, t.tab_hint_tasks),
                (Tab::Agents, t.tab_agents, t.tab_hint_agents),
                (Tab::Models, t.tab_models, t.tab_hint_models),
                (Tab::Image, t.tab_image, t.tab_hint_image),
                (Tab::Providers, t.tab_providers, t.tab_hint_providers),
                (Tab::Audit, t.tab_audit, t.tab_hint_audit),
                (Tab::Caps, t.tab_caps, t.tab_hint_caps),
                (Tab::Scenarios, t.tab_scenarios, t.tab_hint_scenarios),
                (Tab::Feedback, t.tab_feedback, t.tab_hint_feedback),
                (Tab::Settings, t.tab_settings, t.tab_hint_settings),
            ] {
                if ui
                    .selectable_label(self.tab == tab, label)
                    .on_hover_text(hint)
                    .clicked()
                {
                    if tab == Tab::Feedback && self.tab != Tab::Feedback {
                        self.fb_result.clear();
                    }
                    self.tab = tab.clone();
                    if tab == Tab::Providers {
                        let _ = self.cmd_tx.send(Cmd::ProviderList);
                    }
                    if tab == Tab::Audit {
                        let _ = self.cmd_tx.send(Cmd::Audit { last: 40 });
                    }
                    if tab == Tab::Caps && !self.caps_holder.is_empty() {
                        let _ = self.cmd_tx.send(Cmd::CapList {
                            holder: self.caps_holder.clone(),
                        });
                    }
                    if tab == Tab::Notes {
                        let _ = self.cmd_tx.send(Cmd::NotesList);
                    }
                    if tab == Tab::Memory {
                        let _ = self.cmd_tx.send(Cmd::MemList {
                            include_superseded: self.mem_show_superseded,
                        });
                    }
                    if tab == Tab::Tasks {
                        let _ = self.cmd_tx.send(Cmd::TasksList);
                    }
                    if tab == Tab::Settings {
                        let _ = self.cmd_tx.send(Cmd::ScheduleList);
                        let _ = self.cmd_tx.send(Cmd::CatalogueRefresh);
                        let _ = self.cmd_tx.send(Cmd::ModuleList);
                    }
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
                        self.tab = tab;
                        let _ = self.cmd_tx.send(Cmd::ModuleUiLoad { module: name });
                    }
                }
            }
            ui.separator();
            ui.heading(t.network_heading);
            let mut online = self.network_online;
            if ui
                .checkbox(&mut online, t.allow_network)
                .changed()
            {
                self.network_online = online;
                self.prefs.network_online = online;
                save_preferences(&self.prefs);
                let _ = self.cmd_tx.send(Cmd::NetSetMode { online });
            }
            ui.separator();
            ui.heading(t.resources_heading);
            if let Some(m) = &self.metrics {
                let ratio = m.ram_used as f32 / m.ram_total.max(1) as f32;
                ui.add(egui::ProgressBar::new(ratio).text(format!(
                    "RAM {:.1}/{:.1} GiB",
                    m.ram_used as f64 / (1 << 30) as f64,
                    m.ram_total as f64 / (1 << 30) as f64
                )));
                ui.label(format!("CPU {:.0}%", m.cpu_percent));
                ui.label(format!("{}: {}", t.metrics_live, m.live_inferences()));
                for mm in &m.models {
                    ui.group(|ui| {
                        ui.label(format!("{} [{:?}]", mm.model_id, mm.state));
                        let ttft = mm
                            .last_ttft_ms
                            .map(|v| format!("{v:.0} ms"))
                            .unwrap_or_else(|| "—".into());
                        let toks = mm
                            .last_tok_s
                            .map(|v| format!("{v:.1}"))
                            .unwrap_or_else(|| "—".into());
                        ui.monospace(format!(
                            "{} {} · {} {} · {} {:.0} MiB",
                            t.metrics_ttft,
                            ttft,
                            t.metrics_tok_s,
                            toks,
                            t.metrics_vram,
                            mm.vram_bytes as f64 / (1 << 20) as f64
                        ));
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
            Tab::Models => overflow_scroll(ui, "models", |ui| self.ui_models(ui)),
            Tab::Image => overflow_scroll(ui, "image", |ui| {
                self.image_studio
                    .ui(ui, &i18n::strings(&self.prefs.language), &self.cmd_tx);
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
                    ui.heading("Conversation");
                    if let Some(id) = &self.active_session {
                        ui.weak(format!("session {id}"));
                    }

                    let input_reserve = 44.0_f32;
                    let scroll_h = (ui.available_height() - input_reserve).max(120.0);
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
                                let is_completion = attachments.iter().any(|a| {
                                    matches!(
                                        a,
                                        ChatAttachment::AgentRef { origin, .. }
                                            if origin == "completion"
                                    )
                                });
                                let text = if role == "assistant" && !is_completion {
                                    agent_panel::format_assistant_display(&text)
                                } else {
                                    text
                                };
                                let shown_role = if role == "user" || role == "vous" {
                                    t.chat_you.to_string()
                                } else {
                                    role.clone()
                                };
                            ui.horizontal(|ui| {
                                ui.label(format!("[{shown_role}]"));
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
                                    let info =
                                        self.agents.iter().find(|a| a.agent_id == *agent_id);
                                    let selected = reply_id.as_deref() == Some(agent_id.as_str());
                                    let action = ui
                                        .push_id(("chat_agent_card", i, j, agent_id.as_str()), |ui| {
                                            agent_panel::chat_agent_card(
                                                ui,
                                                info,
                                                agent_id.as_str(),
                                                title.as_str(),
                                                origin.as_str(),
                                                selected && origin == "ask",
                                                &t,
                                            )
                                        })
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
                                                &t,
                                                path.as_str(),
                                                prompt.as_str(),
                                                || {
                                                    open_studio =
                                                        Some((prompt.clone(), path.clone()));
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
                                                &t,
                                                &self.cmd_tx,
                                                &mut self.chat[i].attachments[j],
                                                &piper,
                                            ) {
                                                self.status = "audio : génération…".into();
                                            }
                                        }
                                    }
                                }
                                ui.separator();
                            }
                            if let Some(id) = open_agent {
                                self.open_agent_tab(&id);
                            }
                            if let Some((prompt, path)) = open_studio {
                                self.image_studio.open_from_chat(&prompt, &path);
                                self.tab = Tab::Image;
                            }
                            if let Some(id) = target_reply {
                                self.ask_reply_target = Some(id);
                                self.chat_refocus = true;
                                self.status = "réponse destinée à cet agent".into();
                            }
                            if !self.streaming.is_empty() {
                                ui.label("[assistant]");
                                let streaming =
                                    agent_panel::format_streaming_preview(&self.streaming);
                                ui.push_id("chat_md_stream", |ui| {
                                    CommonMarkViewer::new().show(
                                        ui,
                                        &mut self.chat_md_cache,
                                        &streaming,
                                    );
                                });
                            } else if self.chat_pending {
                                ui.label("[assistant]");
                                ui.weak("… en file / génération");
                            }
                        });

                    let completions = slash_completions(&self.input);
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
                                if ui.button(t.chat_stop).clicked() {
                                    if let Some(id) = self.chat_inference_id {
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
                    if !completions.is_empty() {
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
        ui.label(t.agents_goal);
        ui.add(
            egui::TextEdit::multiline(&mut self.agent_task)
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        ui.label(t.agents_system_prompt);
        ui.add(
            egui::TextEdit::multiline(&mut self.agent_system_prompt)
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.agent_optimize, t.agents_optimize);
            if ui.button(t.agents_optimize_now).clicked() && !self.agent_task.is_empty() {
                let _ = self.cmd_tx.send(Cmd::AgentPromptOptimize {
                    goal: self.agent_task.clone(),
                    skills: self.skill_selected.clone(),
                    tools: self.tool_selected.clone(),
                    current: if self.agent_system_prompt.is_empty() {
                        None
                    } else {
                        Some(self.agent_system_prompt.clone())
                    },
                });
            }
            ui.label("max_steps");
            ui.add(egui::DragValue::new(&mut self.agent_max_steps).range(1..=128));
            ui.label("timeout_s");
            ui.add(egui::DragValue::new(&mut self.agent_timeout_secs).range(60..=86_400));
        });

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

        if ui.button(t.agents_create).clicked() && !self.agent_task.is_empty() {
            self.pending_note_agent = self.agent_task.to_lowercase().contains("note");
            let task = self.agent_task.clone();
            self.arm_pending_module_agent(&task);
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
                optimize_prompt: self.agent_optimize,
                max_steps: self.agent_max_steps,
                timeout_secs: self.agent_timeout_secs,
                model_id: if self.agent_model_id.is_empty() {
                    None
                } else {
                    Some(self.agent_model_id.clone())
                },
                session_id: self.active_session.clone(),
                origin: "form".into(),
            });
        }

        ui.separator();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.agent_show_history, false, t.agents_tab_active);
            ui.selectable_value(&mut self.agent_show_history, true, t.agents_tab_history);
        });
        let history = self.agent_show_history;
        let visible: Vec<AgentInfo> = self
            .agents
            .iter()
            .filter(|a| agent_is_live(&a.state) != history)
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
            let label = agent_panel::truncate(a.display_title(), 48);
            if ui.selectable_label(selected, &label).on_hover_text(&a.agent_id).clicked()
            {
                self.open_agent_tab(&a.agent_id);
            }
            ui.weak(&a.agent_id);
            ui.colored_label(
                agent_panel::state_color(&a.state),
                format!("{:?}", a.state),
            );
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

        ui.heading(t.settings_general);
        egui::Grid::new("settings_general")
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
            });

        ui.add_space(8.0);
        ui.heading(t.settings_models);
        egui::Grid::new("settings_models")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .min_col_width(label_w)
            .show(ui, |ui| {
                ui.label(t.routing);
                ui.horizontal(|ui| {
                    for (code, label) in [
                        ("local_only", t.routing_local),
                        ("balanced", t.settings_routing_balanced),
                        ("remote_only", t.settings_routing_remote),
                    ] {
                        if ui
                            .selectable_label(self.prefs.routing == code, label)
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

                ui.label(t.tab_models);
                if ui.button(t.tab_models).clicked() {
                    self.tab = Tab::Models;
                }
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

                ui.label("W / H / steps");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.prefs.image_width).range(64..=2048));
                    ui.add(egui::DragValue::new(&mut self.prefs.image_height).range(64..=2048));
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

        ui.add_space(8.0);
        ui.heading(t.settings_network);
        egui::Grid::new("settings_network")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .min_col_width(label_w)
            .show(ui, |ui| {
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

        ui.add_space(8.0);
        ui.heading(t.settings_agents);
        egui::Grid::new("settings_agents")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .min_col_width(label_w)
            .show(ui, |ui| {
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

        ui.add_space(8.0);
        ui.heading(t.settings_web);
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
                                .selectable_label(self.prefs.web_search_engine == eng, eng)
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
        ui.heading(t.settings_secrets);
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
                ui.weak(format!("{}: {}", t.settings_secret_configured, self.secret_names.join(", ")));
            }
        });
        ui.weak(t.settings_brave_hint);

        ui.add_space(12.0);
        ui.heading(t.settings_catalogue);
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
                        let mut label = format!("{} {} ({})", e.name, e.version, e.kind);
                        if let Some(m) = &installed {
                            label.push_str(&format!(" [{}]", t.settings_catalogue_installed));
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
                            } else if ui.button(t.settings_catalogue_install).clicked() {
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

        ui.add_space(12.0);
        ui.heading(t.settings_installed_modules);
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

        ui.add_space(12.0);
        ui.heading(t.schedule_heading);
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

    fn ui_models(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        ui.heading(t.tab_models);
        ui.weak(t.models_media_packs);
        if ui.button("Refresh list").clicked() {
            let _ = self.cmd_tx.send(Cmd::ModelsRefresh);
        }
        if !self.model_updates_msg.is_empty() {
            ui.colored_label(
                egui::Color32::from_rgb(180, 220, 120),
                &self.model_updates_msg,
            );
        }
        if !self.download_status.is_empty() {
            ui.label(&self.download_status);
        }
        ui.separator();
        ui.label(t.metrics_live);
        if let Some(m) = &self.metrics {
            for mm in &m.models {
                ui.group(|ui| {
                    ui.strong(format!("{} [{:?}]", mm.model_id, mm.state));
                    let ttft = mm
                        .last_ttft_ms
                        .map(|v| format!("{v:.1} ms"))
                        .unwrap_or_else(|| "—".into());
                    let toks = mm
                        .last_tok_s
                        .map(|v| format!("{v:.2}"))
                        .unwrap_or_else(|| "—".into());
                    ui.label(format!("{}: {}", t.metrics_ttft, ttft));
                    ui.label(format!("{}: {}", t.metrics_tok_s, toks));
                    ui.label(format!(
                        "{}: {:.2} GiB · {}: {:.2} GiB · {}: {:.2} GiB",
                        t.metrics_vram,
                        mm.vram_bytes as f64 / (1 << 30) as f64,
                        t.metrics_ram,
                        mm.ram_bytes as f64 / (1 << 30) as f64,
                        t.metrics_disk,
                        mm.disk_bytes as f64 / (1 << 30) as f64
                    ));
                    ui.weak(format!(
                        "active={} {}={}",
                        mm.active_inferences, t.metrics_queued, mm.queued
                    ));
                });
            }
        } else {
            ui.label("…");
        }
        ui.separator();
        ui.label("Installed / registered (model.list)");
        for m in self.model_infos.clone() {
            ui.horizontal(|ui| {
                ui.label(format!("{} — {} [{:?}]", m.id, m.name, m.state));
                if ui.button("Load").clicked() {
                    let _ = self.cmd_tx.send(Cmd::ModelLoad {
                        model_id: m.id.clone(),
                    });
                }
                if ui.button("Set session default").clicked() {
                    if let Some(sid) = self.active_session.clone() {
                        let _ = self.cmd_tx.send(Cmd::SessionSetModel {
                            session_id: sid,
                            model_id: Some(m.id.clone()),
                        });
                    }
                }
            });
        }
        ui.separator();
        ui.label("Offerings (download via aos-session)");
        let offerings_path = aos_home().join("share/models/catalog-offerings.json");
        if let Ok(raw) = std::fs::read_to_string(offerings_path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(arr) = v.get("models").and_then(|m| m.as_array()) {
                    for m in arr {
                        let id = m.get("id").and_then(|x| x.as_str()).unwrap_or("");
                        let name = m.get("name").and_then(|x| x.as_str()).unwrap_or(id);
                        let bytes = m.get("bytes").and_then(|x| x.as_u64()).unwrap_or(0);
                        let modality = m
                            .get("modality")
                            .and_then(|x| x.as_str())
                            .unwrap_or("");
                        let installed = self.model_infos.iter().any(|x| x.id == id);
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "{}{}{} ({:.1} GiB)",
                                if installed { "[ok] " } else { "" },
                                if modality.is_empty() {
                                    String::new()
                                } else {
                                    format!("[{modality}] ")
                                },
                                name,
                                bytes as f64 / (1 << 30) as f64
                            ));
                            if !installed && ui.button("Download").clicked() {
                                let session = bin_aos_session();
                                let id_owned = id.to_string();
                                match std::process::Command::new(&session)
                                    .arg("--download-models")
                                    .arg(&id_owned)
                                    .env("AOS_HOME", aos_home())
                                    .status()
                                {
                                    Ok(st) if st.success() => {
                                        self.download_status = format!(
                                            "Downloaded {id_owned} — restart Preview to load"
                                        );
                                        self.model_updates_msg.clear();
                                    }
                                    Ok(st) => {
                                        self.download_status =
                                            format!("Download failed (exit {st})");
                                    }
                                    Err(e) => {
                                        self.download_status = format!("Download error: {e}");
                                    }
                                }
                            }
                        });
                    }
                }
            }
        } else {
            ui.label("catalog-offerings.json missing");
        }
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
            ui.label(t.caps_holder);
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

    #[test]
    fn create_module_dump_delegates_instead_of_display() {
        let dumped = r#"{"kind":"column","children":[{"kind":"heading","text":"Ping"}]}"#;
        let spec = chat_delegate_agent_spec("crée un module ping", dumped);
        let (brief, _skills, tools, prose) = spec.expect("doit déléguer");
        assert_eq!(brief, "crée un module ping");
        assert!(tools.iter().any(|x| x == "module.scaffold"));
        assert!(prose.contains("agent"));
    }

    #[test]
    fn explain_module_does_not_delegate() {
        assert!(chat_delegate_agent_spec(
            "c'est quoi un module",
            "Un module est un package."
        )
        .is_none());
    }

    #[test]
    fn model_scaffold_action_delegates() {
        let out = r#"{"action":"module.scaffold","args":{"name":"ping"}}"#;
        let spec = chat_delegate_agent_spec("fais un ping", out);
        let (_brief, _skills, tools, _) = spec.expect("doit déléguer");
        assert!(tools.iter().any(|x| x == "module.scaffold"));
    }
}
