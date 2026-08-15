//! Akasha OS Preview — UI egui (ADR 0003).
//!
//! Surface testeur : chat, dashboard, onboarding, notes, confirm, agents,
//! audit, scénarios guidés, retours (`feedback.submit`).

mod agent_panel;
mod i18n;
mod model_setup;
mod notes_panel;
mod prefs;

use aos_ipc::BusClient;
use aos_proto::{
    AgentCreateRequest, AgentGoal, AgentIdRequest, AgentInfo, AgentPromptOptimizeRequest,
    AgentPromptOptimizeResponse, AgentState, AgentSteerRequest, AgentTrace, AuditEvent, AuditQueryRequest,
    ChatAttachment, ChatMessage, ChatSessionAppendRequest, ChatSessionCreateRequest,
    ChatSessionGetResponse, ChatSessionIdRequest, ChatSessionMeta, ChatSessionRenameRequest,
    ChatSessionSetModelRequest, ConfirmResponseRequest, DocumentRef, FeedbackSubmitRequest,
    FeedbackSubmitResponse, FilesGenerateRequest, FilesGenerateResponse, InferParams, InferRequest,
    McpServerInfo, MemContextRequest, MemContextResponse, MemHit, MemUserRecallRequest,
    MemUserRememberRequest, LoadRequest, ModelInfo, ModelState, ModuleInvokeRequest,
    ModuleInvokeResponse, NetFetchRequest, NetFetchResponse, NetModeRequest, PendingConfirmation,
    SetRoutingRequest, SkillInfo, SystemMetrics, TokenEvent, WebBrowseRequest, WebBrowseResponse,
    WebSearchHit, WebSearchRequest, WebSearchResponse, CHAT_DELEGATION_PROMPT, SYSTEM_ASSISTANT_PROMPT,
};
use prefs::{load_preferences, save_preferences, Preferences};
use eframe::egui;
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

fn open_in_browser(url: &str) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Chat,
    Memory,
    Notes,
    Agents,
    Models,
    Audit,
    Scenarios,
    Feedback,
    Settings,
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

enum Cmd {
    /// Chat session active : historique + texte user (persisté côté platformd).
    Chat {
        session_id: String,
        history: Vec<(String, String)>,
        user_text: String,
        model_id: Option<String>,
    },
    SessionBootstrap,
    SessionCreate { title: Option<String> },
    SessionSelect { id: String },
    SessionRename { id: String, title: String },
    SessionDelete { id: String },
    SessionExport { id: String },
    MemRecall { query: String },
    MemRemember { text: String, pinned: bool },
    NetSetMode { online: bool },
    SetRouting { mode: String },
    WebSearch { query: String, engine: String },
    WebBrowse { url: String, max_chars: usize },
    NetFetch { url: String, max_bytes: u64 },
    FilesGenerate {
        format: String,
        path: String,
        content: String,
        title: Option<String>,
    },
    Help,
    NotesList,
    NotesCreate { title: String, content: String },
    NotesUpdate {
        title: String,
        path: String,
        content: String,
    },
    NotesRead {
        title: Option<String>,
        path: Option<String>,
        slug: Option<String>,
    },
    NotesSearch { query: String },
    NotesRelated {
        path: String,
        topic: String,
    },
    Confirm { id: String, approved: bool },
    AgentCreate {
        task: String,
        system_prompt: Option<String>,
        skills: Vec<String>,
        tools: Vec<String>,
        mcp_servers: Vec<String>,
        documents: Vec<DocumentRef>,
        optimize_prompt: bool,
        max_steps: u32,
        timeout_secs: u64,
        model_id: Option<String>,
        session_id: Option<String>,
        /// `slash` | `assistant` | `form`
        origin: String,
    },
    AgentKill { id: String },
    AgentPause { id: String },
    AgentResume { id: String },
    AgentRetry { id: String },
    AgentSteer { id: String, text: String },
    AgentTrace { id: String },
    AgentPromptOptimize {
        goal: String,
        skills: Vec<String>,
        tools: Vec<String>,
        current: Option<String>,
    },
    AgentCatalogRefresh,
    Audit { last: usize },
    Feedback(FeedbackSubmitRequest),
    KillAuditd,
    RefreshConfirms,
    ModelsRefresh,
    ModelLoad { model_id: String },
    SessionSetModel {
        session_id: String,
        model_id: Option<String>,
    },
    /// Append chat sans infer (slash /agent, etc.).
    SessionAppend {
        session_id: String,
        role: String,
        content: String,
        attachments: Vec<ChatAttachment>,
    },
}

enum Evt {
    Delta(String),
    Done {
        text: String,
        session_id: String,
        attachments: Vec<ChatAttachment>,
    },
    Error(String),
    Status(String),
    ChatSystem(String),
    Metrics(SystemMetrics),
    Agents(Vec<AgentInfo>),
    AgentSpawned {
        session_id: String,
        agent_id: String,
        title: String,
        origin: String,
        ack: String,
    },
    NotesListed(Vec<notes_panel::NoteListItem>),
    NoteLoaded(notes_panel::NoteDetail),
    NotesSearchHits(Vec<notes_panel::NoteSearchHit>),
    NotesRelated(Vec<notes_panel::NoteRelatedHit>),
    NotesSaved {
        path: String,
        slug: String,
        title: String,
    },
    /// Payload brut (compat scénarios / debug).
    Notes(String),
    Audit(Vec<AuditEvent>),
    Confirms(Vec<PendingConfirmation>),
    FeedbackOk(FeedbackSubmitResponse),
    Sessions(Vec<ChatSessionMeta>),
    SessionLoaded {
        id: String,
        messages: Vec<ChatLine>,
    },
    MemHits(Vec<MemHit>),
    WebResults(Vec<WebSearchHit>),
    BrowsePreview(String),
    NetMode(bool),
    FileOk(String),
    Skills(Vec<SkillInfo>),
    McpServers(Vec<McpServerInfo>),
    PromptOptimized(String),
    Models(Vec<ModelInfo>),
    AgentTrace(AgentTrace),
}

#[derive(Debug, Clone)]
struct ChatLine {
    role: String,
    text: String,
    attachments: Vec<ChatAttachment>,
}

impl ChatLine {
    fn plain(role: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            text: text.into(),
            attachments: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct AgentNotice {
    agent_id: String,
    session_id: String,
    summary: String,
}

const SLASH_COMMANDS: &[(&str, &str)] = &[
    ("<texte>", "discuter avec l'assistant (modèle local)"),
    ("/commands", "cette liste"),
    ("/help", "état du système (services, agents, modèles)"),
    ("/agent <tâche>", "lancer un agent en fond (carte dans le chat)"),
    ("/notes", "lister les notes"),
    ("/notenew <titre> | <contenu>", "créer une note"),
    ("/notesearch <requête>", "recherche sémantique dans les notes"),
    ("/audit [n]", "n derniers événements d'audit"),
    ("/kill <id>", "tuer un agent"),
    ("/pause <id>", "suspendre un agent"),
];

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
            .with_title(format!("Akasha OS Preview {version}")),
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

fn aos_home() -> PathBuf {
    std::env::var("AOS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn bin_aos_session() -> PathBuf {
    let exe = if cfg!(windows) {
        "aos-session.exe"
    } else {
        "aos-session"
    };
    let p = aos_home().join("bin").join(exe);
    if p.exists() {
        p
    } else {
        PathBuf::from(exe)
    }
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

async fn runtime_main(cmd_rx: Receiver<Cmd>, evt_tx: Sender<Evt>, egui_ctx: egui::Context) {
    let bus = match BusClient::connect("127.0.0.1:24701", "ui-egui").await {
        Ok(b) => b,
        Err(e) => {
            let _ = evt_tx.send(Evt::Error(format!(
                "bus injoignable ({e}). Lancez via aos-session."
            )));
            return;
        }
    };

    // Poll métriques / agents / confirms
    {
        let bus = bus.clone();
        let evt_tx = evt_tx.clone();
        let egui_ctx = egui_ctx.clone();
        tokio::spawn(async move {
            loop {
                if let Ok(m) = bus
                    .call::<(), SystemMetrics>("model.metrics", &(), vec![])
                    .await
                {
                    let _ = evt_tx.send(Evt::Metrics(m));
                }
                if let Ok(a) = bus
                    .call::<(), Vec<AgentInfo>>(aos_agent::intents::LIST, &(), vec![])
                    .await
                {
                    let _ = evt_tx.send(Evt::Agents(a));
                }
                if let Ok(models) = bus
                    .call::<(), Vec<ModelInfo>>("model.list", &(), vec![])
                    .await
                {
                    let _ = evt_tx.send(Evt::Models(models));
                }
                if let Ok(c) = bus
                    .call::<(), Vec<PendingConfirmation>>("confirm.list", &(), vec![])
                    .await
                {
                    let _ = evt_tx.send(Evt::Confirms(c));
                }
                egui_ctx.request_repaint();
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
    }

    while let Ok(cmd) = cmd_rx.recv() {
        let bus = bus.clone();
        let evt_tx = evt_tx.clone();
        let egui_ctx = egui_ctx.clone();
        tokio::spawn(async move {
            handle_cmd(bus, evt_tx, cmd).await;
            egui_ctx.request_repaint();
        });
    }
}

async fn handle_cmd(bus: Arc<BusClient>, evt_tx: Sender<Evt>, cmd: Cmd) {
    match cmd {
        Cmd::SessionBootstrap => {
            let list: Vec<ChatSessionMeta> = bus
                .call("chat.session.list", &(), vec![])
                .await
                .unwrap_or_default();
            if list.is_empty() {
                match bus
                    .call::<ChatSessionCreateRequest, ChatSessionMeta>(
                        "chat.session.create",
                        &ChatSessionCreateRequest {
                            title: Some("Session 1".into()),
                            model_id: None,
                        },
                        vec![],
                    )
                    .await
                {
                    Ok(m) => {
                        let _ = evt_tx.send(Evt::Sessions(vec![m.clone()]));
                        let _ = evt_tx.send(Evt::SessionLoaded {
                            id: m.id,
                            messages: vec![],
                        });
                    }
                    Err(e) => {
                        let _ = evt_tx.send(Evt::Error(format!("session create: {e}")));
                    }
                }
            } else {
                let id = list[0].id.clone();
                let _ = evt_tx.send(Evt::Sessions(list));
                load_session(&bus, &evt_tx, &id).await;
            }
        }
        Cmd::SessionCreate { title } => {
            match bus
                .call::<ChatSessionCreateRequest, ChatSessionMeta>(
                    "chat.session.create",
                    &ChatSessionCreateRequest {
                        title,
                        model_id: None,
                    },
                    vec![],
                )
                .await
            {
                Ok(m) => {
                    let list: Vec<ChatSessionMeta> = bus
                        .call("chat.session.list", &(), vec![])
                        .await
                        .unwrap_or_default();
                    let _ = evt_tx.send(Evt::Sessions(list));
                    let _ = evt_tx.send(Evt::SessionLoaded {
                        id: m.id,
                        messages: vec![],
                    });
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::SessionSelect { id } => {
            load_session(&bus, &evt_tx, &id).await;
        }
        Cmd::SessionRename { id, title } => {
            let _ = bus
                .call::<ChatSessionRenameRequest, ChatSessionMeta>(
                    "chat.session.rename",
                    &ChatSessionRenameRequest {
                        session_id: id.clone(),
                        title,
                    },
                    vec![],
                )
                .await;
            let list: Vec<ChatSessionMeta> = bus
                .call("chat.session.list", &(), vec![])
                .await
                .unwrap_or_default();
            let _ = evt_tx.send(Evt::Sessions(list));
        }
        Cmd::SessionDelete { id } => {
            let _ = bus
                .call::<ChatSessionIdRequest, bool>(
                    "chat.session.delete",
                    &ChatSessionIdRequest { session_id: id },
                    vec![],
                )
                .await;
            let _ = evt_tx.send(Evt::Status("session supprimée".into()));
            let list: Vec<ChatSessionMeta> = bus
                .call("chat.session.list", &(), vec![])
                .await
                .unwrap_or_default();
            if list.is_empty() {
                match bus
                    .call::<ChatSessionCreateRequest, ChatSessionMeta>(
                        "chat.session.create",
                        &ChatSessionCreateRequest {
                            title: Some("Session 1".into()),
                            model_id: None,
                        },
                        vec![],
                    )
                    .await
                {
                    Ok(m) => {
                        let list2: Vec<ChatSessionMeta> = bus
                            .call("chat.session.list", &(), vec![])
                            .await
                            .unwrap_or_default();
                        let _ = evt_tx.send(Evt::Sessions(list2));
                        let _ = evt_tx.send(Evt::SessionLoaded {
                            id: m.id,
                            messages: vec![],
                        });
                    }
                    Err(e) => {
                        let _ = evt_tx.send(Evt::Error(e.to_string()));
                    }
                }
            } else {
                let id = list[0].id.clone();
                let _ = evt_tx.send(Evt::Sessions(list));
                load_session(&bus, &evt_tx, &id).await;
            }
        }
        Cmd::SessionExport { id } => {
            match bus
                .call::<ChatSessionIdRequest, String>(
                    "chat.session.export",
                    &ChatSessionIdRequest { session_id: id },
                    vec![],
                )
                .await
            {
                Ok(md) => {
                    let path = aos_home().join("var/downloads").join(format!(
                        "session-export-{}.md",
                        chrono_like_stamp()
                    ));
                    let _ = std::fs::create_dir_all(path.parent().unwrap());
                    match std::fs::write(&path, md) {
                        Ok(()) => {
                            let _ = evt_tx.send(Evt::FileOk(path.display().to_string()));
                        }
                        Err(e) => {
                            let _ = evt_tx.send(Evt::Error(e.to_string()));
                        }
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::Chat {
            session_id,
            history,
            user_text,
            model_id,
        } => {
            let _ = evt_tx.send(Evt::Status(
                "assistant : génération en cours…".into(),
            ));
            let _ = bus
                .call::<ChatSessionAppendRequest, aos_proto::ChatSessionMessage>(
                    "chat.session.append",
                    &ChatSessionAppendRequest {
                        session_id: session_id.clone(),
                        role: "user".into(),
                        content: user_text.clone(),
                        attachments: vec![],
                    },
                    vec![],
                )
                .await;

            let mem_block = bus
                .call::<MemContextRequest, MemContextResponse>(
                    "mem.context",
                    &MemContextRequest {
                        session_id: Some(session_id.clone()),
                        query: user_text.clone(),
                        k: 5,
                    },
                    vec![],
                )
                .await
                .ok()
                .map(|r| r.prompt_block)
                .unwrap_or_default();

            let mut system = SYSTEM_ASSISTANT_PROMPT.to_string();
            system.push_str(CHAT_DELEGATION_PROMPT);
            if !mem_block.trim().is_empty() {
                system.push_str("\n\n");
                system.push_str(&mem_block);
            }
            let mut messages = vec![ChatMessage {
                role: "system".into(),
                content: system,
            }];
            messages.extend(history.into_iter().map(|(r, c)| ChatMessage {
                role: r,
                content: c,
            }));
            let req = InferRequest {
                model_id,
                messages,
                params: InferParams {
                    max_tokens: 512,
                    ..Default::default()
                },
                priority: 8,
                data_refs: vec![],
                routing: Some("local_only".into()),
            };
            let sid = session_id.clone();
            let infer = async {
                match bus
                    .call_stream::<InferRequest, TokenEvent>("model.infer", &req, vec![])
                    .await
                {
                    Ok(mut rx) => {
                        let mut full = String::new();
                        while let Some(ev) = rx.recv().await {
                            match ev {
                                Ok(TokenEvent::Delta { text }) => {
                                    full.push_str(&text);
                                    let _ = evt_tx.send(Evt::Delta(text));
                                }
                                Ok(TokenEvent::Done { .. }) => break,
                                Ok(TokenEvent::Error { message }) => {
                                    let _ = evt_tx.send(Evt::Error(message));
                                    return;
                                }
                                _ => {}
                            }
                        }
                        if full.is_empty() {
                            let _ = evt_tx.send(Evt::Done {
                                text: String::new(),
                                session_id: sid,
                                attachments: vec![],
                            });
                            return;
                        }

                        // Délégation : agent.spawn / agent.create → worker en fond
                        if let Some(action) = aos_agent::actions::parse_action(&full) {
                            if action.action == "agent.spawn" || action.action == "agent.create" {
                                let brief = action
                                    .args
                                    .get("brief")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .trim()
                                    .to_string();
                                let brief = if brief.is_empty() {
                                    user_text.clone()
                                } else {
                                    brief
                                };
                                let (mut skills, mut tools) = agent_heuristics_for_task(&brief);
                                if let Some(arr) = action.args.get("skills").and_then(|v| v.as_array())
                                {
                                    for s in arr {
                                        if let Some(name) = s.as_str() {
                                            if !skills.iter().any(|x| x == name) {
                                                skills.push(name.to_string());
                                            }
                                        }
                                    }
                                }
                                if let Some(arr) = action.args.get("tools").and_then(|v| v.as_array())
                                {
                                    for t in arr {
                                        if let Some(name) = t.as_str() {
                                            if !tools.iter().any(|x| x == name) {
                                                tools.push(name.to_string());
                                            }
                                        }
                                    }
                                }
                                let mut prose = agent_panel::prose_without_json(&full);
                                if prose.is_empty() {
                                    prose = "Je lance un agent pour cette tâche.".into();
                                }
                                let mut req = AgentCreateRequest::simple(brief.clone());
                                req.skills = skills;
                                req.tools = tools;
                                req.session_id = Some(sid.clone());
                                req.goal = Some(AgentGoal {
                                    statement: brief.clone(),
                                    success_criteria: vec![],
                                    max_steps: 16,
                                    max_subagents: 4,
                                    timeout_secs: 3600,
                                });
                                if req.skills.iter().any(|s| s.contains("notes"))
                                    || req.tools.iter().any(|t| t.starts_with("notes."))
                                {
                                    req.caps.push("tool.invoke:notes".into());
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
                                return;
                            }
                        }

                        let _ = bus
                            .call::<ChatSessionAppendRequest, aos_proto::ChatSessionMessage>(
                                "chat.session.append",
                                &ChatSessionAppendRequest {
                                    session_id: sid.clone(),
                                    role: "assistant".into(),
                                    content: full.clone(),
                                    attachments: vec![],
                                },
                                vec![],
                            )
                            .await;
                        let _ = evt_tx.send(Evt::Done {
                            text: full,
                            session_id: sid,
                            attachments: vec![],
                        });
                    }
                    Err(e) => {
                        let _ = evt_tx.send(Evt::Error(e.to_string()));
                    }
                }
            };
            match tokio::time::timeout(std::time::Duration::from_secs(180), infer).await {
                Ok(()) => {}
                Err(_) => {
                    let _ = evt_tx.send(Evt::Error(
                        "timeout chat (180 s) — modeld a peut-être planté (voir var/run/aos-modeld.stderr.log) ; relancez aos-session".into(),
                    ));
                }
            }
        }
        Cmd::MemRecall { query } => {
            match bus
                .call::<MemUserRecallRequest, Vec<MemHit>>(
                    "mem.user.recall",
                    &MemUserRecallRequest { query, k: 8 },
                    vec![],
                )
                .await
            {
                Ok(hits) => {
                    let _ = evt_tx.send(Evt::MemHits(hits));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::MemRemember { text, pinned } => {
            match bus
                .call::<MemUserRememberRequest, String>(
                    "mem.user.remember",
                    &MemUserRememberRequest {
                        text,
                        metadata: serde_json::json!({"source": "ui"}),
                        pinned,
                    },
                    vec![],
                )
                .await
            {
                Ok(id) => {
                    let _ = evt_tx.send(Evt::Status(format!("mémoire enregistrée ({id})")));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::NetSetMode { online } => {
            let mode = if online {
                "online".to_string()
            } else {
                "offline_strict".to_string()
            };
            match bus
                .call::<NetModeRequest, bool>("net.set_mode", &NetModeRequest { mode }, vec![])
                .await
            {
                Ok(_) => {
                    let _ = evt_tx.send(Evt::NetMode(online));
                    let _ = evt_tx.send(Evt::Status(if online {
                        "réseau autorisé (online)".into()
                    } else {
                        "réseau coupé (offline_strict)".into()
                    }));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::SetRouting { mode } => {
            match bus
                .call::<SetRoutingRequest, bool>(
                    "model.set_routing",
                    &SetRoutingRequest { mode: mode.clone() },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    let _ = evt_tx.send(Evt::Status(format!("routing → {mode}")));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("model.set_routing: {e}")));
                }
            }
        }
        Cmd::WebSearch { query, engine } => {
            let req = WebSearchRequest {
                query,
                max_results: 5,
                caps: vec![
                    "net.connect:html.duckduckgo.com:443".into(),
                    "net.connect:api.search.brave.com:443".into(),
                    "net.connect:www.bing.com:443".into(),
                    "net.connect:*:*".into(),
                ],
                actor: "human:ui".into(),
                engine,
            };
            match bus
                .call::<WebSearchRequest, WebSearchResponse>("web.search", &req, vec![])
                .await
            {
                Ok(r) => {
                    let _ = evt_tx.send(Evt::WebResults(r.results));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("web.search: {e}")));
                }
            }
        }
        Cmd::WebBrowse { url, max_chars } => {
            let req = WebBrowseRequest {
                url,
                max_chars,
                caps: vec!["net.connect:*:*".into()],
                actor: "human:ui".into(),
            };
            match bus
                .call::<WebBrowseRequest, WebBrowseResponse>("web.browse", &req, vec![])
                .await
            {
                Ok(r) => {
                    let body = if r.text.chars().count() > 2000 {
                        format!("{}…", r.text.chars().take(2000).collect::<String>())
                    } else {
                        r.text
                    };
                    let preview = format!("{}\n{}\n\n{}", r.title, r.final_url, body);
                    let _ = evt_tx.send(Evt::BrowsePreview(preview));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("web.browse: {e}")));
                }
            }
        }
        Cmd::NetFetch { url, max_bytes } => {
            let req = NetFetchRequest {
                url,
                dest_path: None,
                max_bytes,
                caps: vec![
                    "net.connect:*:*".into(),
                    "fs.write:/downloads/**".into(),
                ],
                actor: "human:ui".into(),
            };
            match bus
                .call::<NetFetchRequest, NetFetchResponse>("net.fetch", &req, vec![])
                .await
            {
                Ok(r) => {
                    let _ = evt_tx.send(Evt::FileOk(format!(
                        "téléchargé {} ({} octets, {})",
                        r.path, r.bytes, r.content_type
                    )));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("net.fetch: {e}")));
                }
            }
        }
        Cmd::FilesGenerate {
            format,
            path,
            content,
            title,
        } => {
            let req = FilesGenerateRequest {
                format,
                path,
                content,
                title,
                caps: vec!["fs.write:/downloads/**".into()],
                actor: "human:ui".into(),
            };
            match bus
                .call::<FilesGenerateRequest, FilesGenerateResponse>(
                    "files.generate",
                    &req,
                    vec![],
                )
                .await
            {
                Ok(r) => {
                    let _ = evt_tx.send(Evt::FileOk(format!(
                        "généré {} ({} octets)",
                        r.path, r.bytes
                    )));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("files.generate: {e}")));
                }
            }
        }
        Cmd::Help => {
            let mut services = Vec::new();
            for (name, probe) in [
                ("modeld", "model.list"),
                ("agentd", "agent.list"),
                ("platformd", "module.list"),
                ("capkd", "cap.check"),
            ] {
                let up = bus.lookup(probe).await.unwrap_or(false);
                services.push(format!("{name}: {}", if up { "up" } else { "DOWN" }));
            }
            let models: Vec<ModelInfo> = bus
                .call("model.list", &(), vec![])
                .await
                .unwrap_or_default();
            let loaded = models
                .iter()
                .filter(|m| matches!(m.state, ModelState::Loaded | ModelState::PartiallyOffloaded))
                .count();
            let agents: Vec<AgentInfo> = bus
                .call(aos_agent::intents::LIST, &(), vec![])
                .await
                .unwrap_or_default();
            let running = agents
                .iter()
                .filter(|a| matches!(a.state, AgentState::Running))
                .count();
            let metrics: Option<SystemMetrics> = bus.call("model.metrics", &(), vec![]).await.ok();
            let mut out = String::from("Akasha OS Preview — état\n");
            out.push_str(&format!("services : {}\n", services.join(", ")));
            out.push_str(&format!(
                "modèles : {loaded} chargés / {} au registry\n",
                models.len()
            ));
            out.push_str(&format!(
                "agents : {running} running / {} total\n",
                agents.len()
            ));
            if let Some(m) = metrics {
                out.push_str(&format!(
                    "hôte : RAM {:.1}/{:.1} GiB, CPU {:.0}%\n",
                    m.ram_used as f64 / (1 << 30) as f64,
                    m.ram_total as f64 / (1 << 30) as f64,
                    m.cpu_percent
                ));
            }
            out.push_str("→ /commands pour la liste des commandes");
            let _ = evt_tx.send(Evt::ChatSystem(out));
        }
        Cmd::NotesList => {
            invoke_notes(&bus, &evt_tx, "notes.list", serde_json::json!({})).await;
        }
        Cmd::NotesCreate { title, content } => {
            invoke_notes(
                &bus,
                &evt_tx,
                "notes.create",
                serde_json::json!({ "title": title, "content": content }),
            )
            .await;
        }
        Cmd::NotesUpdate {
            title,
            path,
            content,
        } => {
            invoke_notes(
                &bus,
                &evt_tx,
                "notes.update",
                serde_json::json!({ "title": title, "path": path, "content": content }),
            )
            .await;
        }
        Cmd::NotesRead { title, path, slug } => {
            let mut args = serde_json::json!({});
            if let Some(t) = title {
                args["title"] = serde_json::json!(t);
            }
            if let Some(p) = path {
                args["path"] = serde_json::json!(p);
            }
            if let Some(s) = slug {
                args["slug"] = serde_json::json!(s);
            }
            invoke_notes(&bus, &evt_tx, "notes.read", args).await;
        }
        Cmd::NotesSearch { query } => {
            invoke_notes(
                &bus,
                &evt_tx,
                "notes.search",
                serde_json::json!({ "query": query }),
            )
            .await;
        }
        Cmd::NotesRelated { path, topic } => {
            let mut args = serde_json::json!({ "path": path });
            if !topic.is_empty() {
                args["topic"] = serde_json::json!(topic);
            }
            invoke_notes(&bus, &evt_tx, "notes.related", args).await;
        }
        Cmd::Confirm { id, approved } => {
            match bus
                .call::<ConfirmResponseRequest, bool>(
                    "confirm.respond",
                    &ConfirmResponseRequest { id, approved },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    let _ = evt_tx.send(Evt::Status(if approved {
                        "confirmation acceptée".into()
                    } else {
                        "confirmation refusée".into()
                    }));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::AgentCreate {
            task,
            system_prompt,
            skills,
            tools,
            mcp_servers,
            documents,
            optimize_prompt,
            max_steps,
            timeout_secs,
            model_id,
            session_id,
            origin,
        } => {
            let mut req = AgentCreateRequest::simple(task.clone());
            req.system_prompt = system_prompt;
            req.skills = skills;
            req.tools = tools;
            req.mcp_servers = mcp_servers;
            req.documents = documents;
            req.optimize_prompt = optimize_prompt;
            req.session_id = session_id.clone();
            req.goal = Some(AgentGoal {
                statement: task.clone(),
                success_criteria: vec![],
                max_steps,
                max_subagents: 4,
                timeout_secs,
            });
            req.model_id = model_id;
            if req.skills.iter().any(|s| s.contains("notes"))
                || req.tools.iter().any(|t| t.starts_with("notes."))
            {
                req.caps.push("tool.invoke:notes".into());
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
                    if let Some(sid) = session_id {
                        let ack = format!("Agent {} lancé en fond.", r.agent_id);
                        let att = ChatAttachment::AgentRef {
                            agent_id: r.agent_id.clone(),
                            title: task.clone(),
                            origin: origin.clone(),
                        };
                        let _ = bus
                            .call::<ChatSessionAppendRequest, aos_proto::ChatSessionMessage>(
                                "chat.session.append",
                                &ChatSessionAppendRequest {
                                    session_id: sid.clone(),
                                    role: "assistant".into(),
                                    content: ack.clone(),
                                    attachments: vec![att],
                                },
                                vec![],
                            )
                            .await;
                        let _ = evt_tx.send(Evt::AgentSpawned {
                            session_id: sid,
                            agent_id: r.agent_id.clone(),
                            title: task,
                            origin,
                            ack,
                        });
                    } else {
                        let _ = evt_tx.send(Evt::Status(format!("agent créé : {}", r.agent_id)));
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::AgentCatalogRefresh => {
            if let Ok(list) = bus
                .call::<(), Vec<SkillInfo>>(aos_agent::intents::SKILL_LIST, &(), vec![])
                .await
            {
                let _ = evt_tx.send(Evt::Skills(list));
            }
            if let Ok(list) = bus
                .call::<(), Vec<McpServerInfo>>(aos_agent::intents::MCP_LIST, &(), vec![])
                .await
            {
                let _ = evt_tx.send(Evt::McpServers(list));
            }
        }
        Cmd::AgentPromptOptimize {
            goal,
            skills,
            tools,
            current,
        } => {
            match bus
                .call::<AgentPromptOptimizeRequest, AgentPromptOptimizeResponse>(
                    aos_agent::intents::PROMPT_OPTIMIZE,
                    &AgentPromptOptimizeRequest {
                        goal,
                        skills,
                        tools,
                        current_prompt: current,
                        model_id: None,
                    },
                    vec![],
                )
                .await
            {
                Ok(r) => {
                    let _ = evt_tx.send(Evt::PromptOptimized(r.optimized_prompt));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::AgentKill { id } => {
            agent_id_cmd(&bus, &evt_tx, aos_agent::intents::KILL, id).await;
        }
        Cmd::AgentPause { id } => {
            agent_id_cmd(&bus, &evt_tx, aos_agent::intents::PAUSE, id).await;
        }
        Cmd::AgentResume { id } => {
            agent_id_cmd(&bus, &evt_tx, aos_agent::intents::RESUME, id).await;
        }
        Cmd::AgentRetry { id } => {
            agent_id_cmd(&bus, &evt_tx, aos_agent::intents::RETRY, id).await;
        }
        Cmd::AgentSteer { id, text } => {
            match bus
                .call::<AgentSteerRequest, ()>(
                    aos_agent::intents::STEER,
                    &AgentSteerRequest {
                        agent_id: id,
                        directive: text,
                    },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    let _ = evt_tx.send(Evt::Status("steer envoyé".into()));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::AgentTrace { id } => {
            match bus
                .call::<AgentIdRequest, AgentTrace>(
                    aos_agent::intents::TRACE,
                    &AgentIdRequest { agent_id: id },
                    vec![],
                )
                .await
            {
                Ok(t) => {
                    let _ = evt_tx.send(Evt::AgentTrace(t));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("agent.trace: {e}")));
                }
            }
        }
        Cmd::Audit { last } => {
            match bus
                .call::<AuditQueryRequest, Vec<AuditEvent>>(
                    "audit.query",
                    &AuditQueryRequest {
                        trace_id: None,
                        actor: None,
                        action: None,
                        last,
                    },
                    vec![],
                )
                .await
            {
                Ok(ev) => {
                    let _ = evt_tx.send(Evt::Audit(ev));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::Feedback(req) => {
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
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::KillAuditd => {
            #[cfg(windows)]
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/IM", "aos-auditd.exe"])
                .status();
            #[cfg(not(windows))]
            let _ = std::process::Command::new("pkill")
                .args(["-x", "aos-auditd"])
                .status();
            let _ = evt_tx.send(Evt::Status(
                "aos-auditd tué — le superviseur de session doit le redémarrer".into(),
            ));
        }
        Cmd::RefreshConfirms => {
            if let Ok(c) = bus
                .call::<(), Vec<PendingConfirmation>>("confirm.list", &(), vec![])
                .await
            {
                let _ = evt_tx.send(Evt::Confirms(c));
            }
        }
        Cmd::ModelsRefresh => {
            if let Ok(models) = bus
                .call::<(), Vec<ModelInfo>>("model.list", &(), vec![])
                .await
            {
                let _ = evt_tx.send(Evt::Models(models));
            }
        }
        Cmd::ModelLoad { model_id } => {
            match bus
                .call::<LoadRequest, ()>(
                    "model.load",
                    &LoadRequest {
                        model_id: model_id.clone(),
                        profile: "balanced".into(),
                        kv_tokens: 8192,
                    },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    let _ = evt_tx.send(Evt::Status(format!("model load: {model_id}")));
                    if let Ok(models) = bus
                        .call::<(), Vec<ModelInfo>>("model.list", &(), vec![])
                        .await
                    {
                        let _ = evt_tx.send(Evt::Models(models));
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::SessionSetModel {
            session_id,
            model_id,
        } => {
            match bus
                .call::<ChatSessionSetModelRequest, ChatSessionMeta>(
                    "chat.session.set_model",
                    &ChatSessionSetModelRequest {
                        session_id,
                        model_id,
                    },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    if let Ok(list) = bus
                        .call::<(), Vec<ChatSessionMeta>>("chat.session.list", &(), vec![])
                        .await
                    {
                        let _ = evt_tx.send(Evt::Sessions(list));
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::SessionAppend {
            session_id,
            role,
            content,
            attachments,
        } => {
            let _ = bus
                .call::<ChatSessionAppendRequest, aos_proto::ChatSessionMessage>(
                    "chat.session.append",
                    &ChatSessionAppendRequest {
                        session_id,
                        role,
                        content,
                        attachments,
                    },
                    vec![],
                )
                .await;
        }
    }
}

fn chrono_like_stamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

async fn load_session(bus: &Arc<BusClient>, evt_tx: &Sender<Evt>, id: &str) {
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
                        "user" => "vous".into(),
                        "assistant" => "assistant".into(),
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

/// Heuristiques skills/tools partagées `/agent` et délégation chat.
fn agent_heuristics_for_task(task: &str) -> (Vec<String>, Vec<String>) {
    let lower = task.to_lowercase();
    let mut skills = Vec::new();
    let mut tools = vec![
        "notes.create".into(),
        "notes.list".into(),
        "notes.read".into(),
        "notes.search".into(),
        "notes.update".into(),
        "notes.links".into(),
        "notes.related".into(),
    ];
    if lower.contains("note") {
        skills.push("notes-writer".into());
    }
    if lower.contains("plan") || lower.contains("délégu") {
        skills.push("planner".into());
        tools.push("agent.spawn".into());
        tools.push("agent.await".into());
    }
    (skills, tools)
}

async fn invoke_notes(
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

async fn agent_id_cmd(bus: &Arc<BusClient>, evt_tx: &Sender<Evt>, intent: &str, id: String) {
    match bus
        .call::<AgentIdRequest, ()>(intent, &AgentIdRequest { agent_id: id }, vec![])
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
    agent_traces: HashMap<String, AgentTrace>,
    trace_fetched_at: Option<Instant>,
    agent_steer_id: String,
    agent_steer_txt: String,
    audit: Vec<AuditEvent>,
    status: String,
    onboarding: OnboardingState,
    show_onboarding: bool,
    pending_note_agent: bool,
    // scenarios
    scen_chat: bool,
    scen_note_human: bool,
    scen_note_agent: bool,
    scen_confirm: bool,
    scen_audit: bool,
    // feedback
    fb_title: String,
    fb_category: String,
    fb_severity: String,
    fb_body: String,
    fb_scenario: String,
    fb_result: String,
    fb_github: bool,
    update_offer: Option<UpdateOffer>,
    update_status: String,
    model_infos: Vec<ModelInfo>,
    agent_model_id: String,
    model_updates_msg: String,
    download_status: String,
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
        let _ = cmd_tx.send(Cmd::SetRouting {
            mode: prefs.routing.clone(),
        });
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
        Self {
            cmd_tx,
            evt_rx,
            version,
            tab: Tab::Chat,
            chat: vec![ChatLine::plain(
                "système",
                format!(
                    "{}\n\
                     Sessions / Memory / Network opt-in.\n\
                     Type /commands — use the side tabs.",
                    t.preview_banner
                ),
            )],
            streaming: String::new(),
            input: String::new(),
            chat_pending: false,
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
            metrics: None,
            agents: Vec::new(),
            agent_prev_states: HashMap::new(),
            agent_notices: Vec::new(),
            agent_notified: std::collections::HashSet::new(),
            confirms: Vec::new(),
            notes: notes_panel::NotesPanelState::default(),
            notes_out: String::new(),
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
            ],
            agent_open_tabs: Vec::new(),
            agent_active_tab: None,
            agent_traces: HashMap::new(),
            trace_fetched_at: None,
            agent_steer_id: String::new(),
            agent_steer_txt: String::new(),
            audit: Vec::new(),
            status: String::new(),
            onboarding,
            show_onboarding,
            pending_note_agent: false,
            scen_chat: false,
            scen_note_human: false,
            scen_note_agent: false,
            scen_confirm: false,
            scen_audit: false,
            fb_title: String::new(),
            fb_category: "ux".into(),
            fb_severity: "medium".into(),
            fb_body: String::new(),
            fb_scenario: String::new(),
            fb_result: String::new(),
            fb_github: true,
            update_offer: load_update_offer(),
            update_status: String::new(),
            model_infos: Vec::new(),
            agent_model_id: default_model,
            model_updates_msg,
            download_status: String::new(),
        }
    }

    fn send_chat(&mut self) {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return;
        }
        self.input.clear();
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
        if self.chat_pending {
            self.chat.push(ChatLine::plain("vous", text));
            self.chat.push(ChatLine::plain(
                "système",
                "réponse précédente encore en cours — patientez.",
            ));
            return;
        }
        self.chat.push(ChatLine::plain("vous", text.clone()));
        let history: Vec<(String, String)> = self
            .chat
            .iter()
            .filter(|l| l.role == "vous" || l.role == "assistant")
            .map(|l| {
                (
                    if l.role == "vous" {
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
        });
        self.scen_chat = true;
    }

    fn handle_slash(&mut self, text: &str) {
        self.chat.push(ChatLine::plain("vous", text));
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
                let (skills, tools) = agent_heuristics_for_task(rest);
                let _ = self.cmd_tx.send(Cmd::AgentCreate {
                    task: rest.to_string(),
                    system_prompt: None,
                    skills,
                    tools,
                    mcp_servers: vec![],
                    documents: vec![],
                    optimize_prompt: false,
                    max_steps: 16,
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
                    if self.status.starts_with("assistant :") {
                        self.status.clear();
                    }
                }
                Evt::Error(m) => {
                    self.status = m.clone();
                    self.chat.push(ChatLine::plain("système", m));
                    self.streaming.clear();
                    self.chat_pending = false;
                }
                Evt::Status(m) => self.status = m,
                Evt::ChatSystem(m) => self.chat.push(ChatLine::plain("système", m)),
                Evt::Metrics(m) => self.metrics = Some(m),
                Evt::AgentSpawned {
                    session_id,
                    agent_id,
                    title,
                    origin,
                    ack,
                } => {
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
                        if terminal && was_active {
                            if let Some(sid) = &ag.session_id {
                                let viewing = self.tab == Tab::Chat
                                    && self.active_session.as_deref() == Some(sid.as_str());
                                if !viewing && !self.agent_notified.contains(&ag.agent_id) {
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
                                if viewing {
                                    let content = match ag.state {
                                        AgentState::Done => {
                                            let out = ag.last_output.trim();
                                            if out.is_empty() {
                                                format!("Agent {} terminé.", ag.agent_id)
                                            } else {
                                                let excerpt: String =
                                                    out.chars().take(500).collect();
                                                format!(
                                                    "Agent {} terminé.\n{}",
                                                    ag.agent_id, excerpt
                                                )
                                            }
                                        }
                                        AgentState::Failed => format!(
                                            "Agent {} a échoué : {}",
                                            ag.agent_id,
                                            ag.fail_reason.as_deref().unwrap_or("échec")
                                        ),
                                        AgentState::Killed => {
                                            format!("Agent {} arrêté.", ag.agent_id)
                                        }
                                        _ => format!("Agent {} terminé.", ag.agent_id),
                                    };
                                    // Évite doublon si agentd a déjà écrit le même résumé
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
                                    if !already {
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
                }
                Evt::MemHits(h) => self.mem_hits = h,
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
                Evt::Skills(list) => self.skill_catalog = list,
                Evt::McpServers(list) => self.mcp_catalog = list,
                Evt::PromptOptimized(p) => {
                    self.agent_system_prompt = p;
                    self.status = "prompt système optimisé".into();
                }
                Evt::Models(list) => self.model_infos = list,
                Evt::AgentTrace(t) => {
                    self.agent_traces.insert(t.agent_id.clone(), t);
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
                            ui.label(t.preview_banner);
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
                ui.colored_label(egui::Color32::from_rgb(220, 160, 40), t.preview_banner);
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
                        let dir = aos_home().join("var/run");
                        let _ = std::fs::create_dir_all(&dir);
                        #[cfg(windows)]
                        let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                        #[cfg(target_os = "linux")]
                        let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
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
                                | "module.compile"
                                | "skill.create"
                                | "cap.request"
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
            ui.heading("Preview");
            for (tab, label) in [
                (Tab::Chat, t.tab_chat),
                (Tab::Memory, t.tab_memory),
                (Tab::Notes, t.tab_notes),
                (Tab::Agents, t.tab_agents),
                (Tab::Models, t.tab_models),
                (Tab::Audit, t.tab_audit),
                (Tab::Scenarios, t.tab_scenarios),
                (Tab::Feedback, t.tab_feedback),
                (Tab::Settings, t.tab_settings),
            ] {
                if ui
                    .selectable_label(self.tab == tab, label)
                    .clicked()
                {
                    self.tab = tab;
                    if tab == Tab::Audit {
                        let _ = self.cmd_tx.send(Cmd::Audit { last: 40 });
                    }
                    if tab == Tab::Notes {
                        let _ = self.cmd_tx.send(Cmd::NotesList);
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
            ui.heading("Ressources");
            if let Some(m) = &self.metrics {
                let ratio = m.ram_used as f32 / m.ram_total.max(1) as f32;
                ui.add(egui::ProgressBar::new(ratio).text(format!(
                    "RAM {:.1}/{:.1} GiB",
                    m.ram_used as f64 / (1 << 30) as f64,
                    m.ram_total as f64 / (1 << 30) as f64
                )));
                ui.label(format!("CPU {:.0}%", m.cpu_percent));
                for mm in &m.models {
                    ui.label(format!("{} [{:?}]", mm.model_id, mm.state));
                }
            } else {
                ui.label("…");
            }
        });

        self.poll_agent_trace(ctx);
        if self.tab == Tab::Agents {
            self.ui_agent_detail_panel(ctx);
        }

        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Chat => self.ui_chat(ui),
            Tab::Memory => self.ui_memory(ui),
            Tab::Notes => self.ui_notes(ui),
            Tab::Agents => self.ui_agents(ui),
            Tab::Models => self.ui_models(ui),
            Tab::Audit => self.ui_audit(ui),
            Tab::Scenarios => self.ui_scenarios(ui),
            Tab::Feedback => self.ui_feedback(ui),
            Tab::Settings => self.ui_settings(ui),
        });
    }
}

impl UiApp {
    fn ui_chat(&mut self, ui: &mut egui::Ui) {
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
                                for m in &self.model_infos {
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
                    egui::ScrollArea::vertical()
                        .id_salt("web_tools")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
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
                                let _ = std::fs::create_dir_all(&dir);
                                #[cfg(windows)]
                                let _ =
                                    std::process::Command::new("explorer").arg(&dir).spawn();
                                #[cfg(target_os = "linux")]
                                let _ =
                                    std::process::Command::new("xdg-open").arg(&dir).spawn();
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

                    let input_reserve = 40.0_f32;
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
                            for line in &self.chat {
                                ui.label(format!("[{}]", line.role));
                                if !line.text.is_empty() {
                                    ui.add(egui::Label::new(&line.text).wrap());
                                }
                                for att in &line.attachments {
                                    let ChatAttachment::AgentRef {
                                        agent_id,
                                        title,
                                        ..
                                    } = att;
                                    let info =
                                        self.agents.iter().find(|a| a.agent_id == *agent_id);
                                    if agent_panel::chat_agent_card(ui, info, agent_id, title) {
                                        open_agent = Some(agent_id.clone());
                                    }
                                }
                                ui.separator();
                            }
                            if let Some(id) = open_agent {
                                self.open_agent_tab(&id);
                            }
                            if !self.streaming.is_empty() {
                                ui.label("[assistant]");
                                ui.add(egui::Label::new(&self.streaming).wrap());
                            } else if self.chat_pending {
                                ui.label("[assistant]");
                                ui.weak("… en file / génération");
                            }
                        });

                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut self.input)
                                .desired_width(ui.available_width() - 90.0)
                                .hint_text("message ou /commands …"),
                        );
                        let send = ui.button("Envoyer").clicked()
                            || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                        if send {
                            self.send_chat();
                        }
                    });
                },
            );
        });
    }

    fn ui_memory(&mut self, ui: &mut egui::Ui) {
        ui.heading("Mémoire long terme (user:default)");
        ui.label("Épingler des faits stables ; ils sont injectés avant chaque infer.");
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.mem_note)
                    .desired_width(400.0)
                    .hint_text("fait à mémoriser"),
            );
            if ui.button("Mémoriser").clicked() && !self.mem_note.is_empty() {
                let _ = self.cmd_tx.send(Cmd::MemRemember {
                    text: self.mem_note.clone(),
                    pinned: true,
                });
                self.mem_note.clear();
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.mem_query)
                    .desired_width(400.0)
                    .hint_text("requête recall"),
            );
            if ui.button("Recall").clicked() && !self.mem_query.is_empty() {
                let _ = self.cmd_tx.send(Cmd::MemRecall {
                    query: self.mem_query.clone(),
                });
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            if self.mem_hits.is_empty() {
                ui.weak("Aucun hit — mémorisez un fait puis recall.");
            }
            for h in &self.mem_hits {
                let pinned = h
                    .metadata
                    .get("pinned")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                ui.label(format!(
                    "[{}] {} (score {:.2})",
                    if pinned { "*" } else { "·" },
                    h.text,
                    h.score
                ));
                ui.separator();
            }
        });
    }

    fn ui_notes(&mut self, ui: &mut egui::Ui) {
        let actions = notes_panel::show_notes_panel(ui, &mut self.notes);
        if actions.list {
            let _ = self.cmd_tx.send(Cmd::NotesList);
        }
        if let Some(query) = actions.search {
            let _ = self.cmd_tx.send(Cmd::NotesSearch { query });
        }
        if let Some(path) = actions.read_path {
            let _ = self.cmd_tx.send(Cmd::NotesRead {
                title: None,
                path: Some(path),
                slug: None,
            });
        }
        if let Some(title) = actions.read_title {
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
        ui.heading("Agents — compose agentic");
        if ui.button("Rafraîchir catalogues (skills / MCP)").clicked() {
            let _ = self.cmd_tx.send(Cmd::AgentCatalogRefresh);
        }
        ui.label("Model");
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
        ui.label("Goal");
        ui.add(
            egui::TextEdit::multiline(&mut self.agent_task)
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        ui.label("Prompt système (optionnel)");
        ui.add(
            egui::TextEdit::multiline(&mut self.agent_system_prompt)
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.agent_optimize, "Optimiser avant démarrage");
            if ui.button("Optimiser maintenant").clicked() && !self.agent_task.is_empty() {
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
                ui.weak("Aucun catalogue — cliquez Rafraîchir (skills/ livrés au démarrage)");
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

        ui.collapsing("Outils", |ui| {
            for name in [
                "notes.create",
                "notes.list",
                "notes.read",
                "notes.search",
                "notes.update",
                "notes.links",
                "notes.related",
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

        ui.collapsing("Serveurs MCP", |ui| {
            if self.mcp_catalog.is_empty() {
                ui.weak("Configurer var/mcp/servers.yaml puis Rafraîchir");
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

        ui.label("Documents (chemins séparés par virgule)");
        ui.text_edit_singleline(&mut self.agent_docs);

        if ui.button("Créer l'agent").clicked() && !self.agent_task.is_empty() {
            self.pending_note_agent = self.agent_task.to_lowercase().contains("note");
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
        ui.heading("Agents actifs");
        egui::ScrollArea::vertical()
            .id_salt("agents_list")
            .max_height(280.0)
            .show(ui, |ui| {
                let roots: Vec<_> = self
                    .agents
                    .iter()
                    .filter(|a| a.parent_id.is_none())
                    .cloned()
                    .collect();
                let orphans: Vec<_> = self
                    .agents
                    .iter()
                    .filter(|a| {
                        a.parent_id
                            .as_ref()
                            .is_some_and(|p| !self.agents.iter().any(|x| x.agent_id == *p))
                    })
                    .cloned()
                    .collect();

                for a in roots.into_iter().chain(orphans) {
                    self.draw_agent_row(ui, &a, 0);
                    let children: Vec<_> = self
                        .agents
                        .iter()
                        .filter(|c| c.parent_id.as_deref() == Some(a.agent_id.as_str()))
                        .cloned()
                        .collect();
                    for child in children {
                        self.draw_agent_row(ui, &child, 1);
                    }
                }
            });
        ui.weak("Cliquez un agent pour ouvrir le panneau détail (onglets).");

        ui.separator();
        ui.label("Steer");
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.agent_steer_id);
            ui.text_edit_singleline(&mut self.agent_steer_txt);
            if ui.button("Envoyer").clicked()
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

    fn draw_agent_row(&mut self, ui: &mut egui::Ui, a: &AgentInfo, indent: usize) {
        ui.horizontal(|ui| {
            if indent > 0 {
                ui.add_space(16.0 * indent as f32);
                ui.small("↳");
            }
            let selected = self.agent_active_tab.as_deref() == Some(a.agent_id.as_str());
            if ui
                .selectable_label(selected, &a.agent_id)
                .clicked()
            {
                self.open_agent_tab(&a.agent_id);
            }
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
                ui.small(format!("(+{} sous-agents)", a.children.len()));
            }
            if let Some(reason) = &a.fail_reason {
                ui.colored_label(
                    egui::Color32::from_rgb(220, 120, 100),
                    agent_panel::truncate(reason, 40),
                );
            }
            if ui.small_button("Pause").clicked() {
                let _ = self.cmd_tx.send(Cmd::AgentPause {
                    id: a.agent_id.clone(),
                });
            }
            if ui.small_button("Kill").clicked() {
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
    }

    fn close_agent_tab(&mut self, id: &str) {
        self.agent_open_tabs.retain(|t| t != id);
        self.agent_traces.remove(id);
        if self.agent_active_tab.as_deref() == Some(id) {
            self.agent_active_tab = self.agent_open_tabs.last().cloned();
        }
    }

    fn poll_agent_trace(&mut self, ctx: &egui::Context) {
        if self.tab != Tab::Agents || self.agent_open_tabs.is_empty() {
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
                ui.horizontal(|ui| {
                    ui.heading("Détail");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✕ tout").clicked() {
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
                            format!("{id} [{:?}]", a.state)
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
                    let info = self.agents.iter().find(|a| a.agent_id == id).cloned();
                    let trace = self.agent_traces.get(&id).cloned();
                    let actions = agent_panel::draw_agent_detail(
                        ui,
                        info.as_ref(),
                        trace.as_ref(),
                        &mut self.agent_steer_txt,
                        &open_in_browser,
                    );
                    if actions.pause {
                        let _ = self.cmd_tx.send(Cmd::AgentPause { id: id.clone() });
                    }
                    if actions.kill {
                        let _ = self.cmd_tx.send(Cmd::AgentKill { id: id.clone() });
                    }
                    if actions.resume {
                        let _ = self.cmd_tx.send(Cmd::AgentResume { id: id.clone() });
                    }
                    if actions.retry {
                        let _ = self.cmd_tx.send(Cmd::AgentRetry { id: id.clone() });
                    }
                    if let Some(text) = actions.steer {
                        let _ = self.cmd_tx.send(Cmd::AgentSteer {
                            id: id.clone(),
                            text,
                        });
                        self.agent_steer_txt.clear();
                    }
                    if let Some(child) = actions.open_child {
                        self.open_agent_tab(&child);
                    }
                } else {
                    ui.weak("Sélectionnez un onglet.");
                }
            });
    }

    fn ui_settings(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        ui.heading(t.settings_title);
        ui.separator();

        ui.heading(t.settings_general);
        ui.horizontal(|ui| {
            ui.label(t.language);
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
        ui.horizontal(|ui| {
            ui.label(t.trust_default);
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

        ui.add_space(8.0);
        ui.heading(t.settings_models);
        ui.horizontal(|ui| {
            ui.label(t.routing);
            for (code, label) in [
                ("local_only", t.routing_local),
                ("balanced", t.settings_routing_balanced),
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
        if ui.button(t.tab_models).clicked() {
            self.tab = Tab::Models;
        }

        ui.add_space(8.0);
        ui.heading(t.settings_network);
        let mut online = self.prefs.network_online;
        if ui.checkbox(&mut online, t.allow_network).changed() {
            self.prefs.network_online = online;
            self.network_online = online;
            save_preferences(&self.prefs);
            let _ = self.cmd_tx.send(Cmd::NetSetMode { online });
        }

        ui.add_space(8.0);
        ui.heading(t.settings_agents);
        ui.horizontal(|ui| {
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
                        let selected = self.prefs.default_agent_model.as_deref() == Some(m.id.as_str());
                        if ui.selectable_label(selected, &m.id).clicked() {
                            self.prefs.default_agent_model = Some(m.id.clone());
                            self.agent_model_id = m.id;
                            save_preferences(&self.prefs);
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label(t.settings_max_steps);
            if ui
                .add(egui::DragValue::new(&mut self.prefs.default_max_steps).range(1..=128))
                .changed()
            {
                self.agent_max_steps = self.prefs.default_max_steps;
                save_preferences(&self.prefs);
            }
        });
        ui.horizontal(|ui| {
            ui.label(t.settings_timeout);
            if ui
                .add(
                    egui::DragValue::new(&mut self.prefs.default_timeout_secs).range(60..=86_400),
                )
                .changed()
            {
                self.agent_timeout_secs = self.prefs.default_timeout_secs;
                save_preferences(&self.prefs);
            }
        });

        ui.add_space(8.0);
        ui.heading(t.settings_web);
        ui.horizontal(|ui| {
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
        });
        ui.horizontal(|ui| {
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
        });
        ui.horizontal(|ui| {
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
        });
        ui.weak(t.settings_brave_hint);
    }

    fn ui_models(&mut self, ui: &mut egui::Ui) {
        ui.heading("Models");
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
                        let installed = self.model_infos.iter().any(|x| x.id == id);
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "{}{} ({:.1} GiB)",
                                if installed { "[ok] " } else { "" },
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
        ui.heading("Journal d'audit");
        ui.horizontal(|ui| {
            if ui.button("Rafraîchir").clicked() {
                let _ = self.cmd_tx.send(Cmd::Audit { last: 50 });
            }
            if ui.button("Tuer aos-auditd (test P4)").clicked() {
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

    fn ui_scenarios(&mut self, ui: &mut egui::Ui) {
        ui.heading("Scénarios guidés (protocole cohorte)");
        ui.label("Cochez au fur et à mesure — voir aussi docs/TESTER.md");
        ui.checkbox(&mut self.scen_chat, "1. Chat offline (onglet Chat)");
        ui.checkbox(
            &mut self.scen_note_human,
            "2. Créer une note humaine (onglet Notes)",
        );
        ui.checkbox(
            &mut self.scen_note_agent,
            "3. Note via agent (créer un agent avec une tâche notes)",
        );
        ui.checkbox(
            &mut self.scen_confirm,
            "4. Accepter/refuser une confirmation sensible",
        );
        ui.checkbox(
            &mut self.scen_audit,
            "5. Vérifier l'audit ; tuer auditd et continuer à chatter",
        );
        ui.label("PC.6–PC.9 : sessions / mémoire / réseau / downloads — voir TESTER.md §6–9");
        ui.separator();
        let done = [
            self.scen_chat,
            self.scen_note_human,
            self.scen_note_agent,
            self.scen_confirm,
            self.scen_audit,
        ]
        .iter()
        .filter(|x| **x)
        .count();
        ui.label(format!("Progression : {done}/5"));
        if done == 5 {
            ui.colored_label(
                egui::Color32::LIGHT_GREEN,
                "Protocole de base terminé — envoyez un retour (onglet Retour).",
            );
        }
        if ui.button("Demander une confirmation test (fs.delete)").clicked() {
            self.status =
                "Créez puis tentez de supprimer une note sensible, ou utilisez le gate P3 en lab."
                    .into();
            let _ = self.cmd_tx.send(Cmd::RefreshConfirms);
        }
    }

    fn ui_feedback(&mut self, ui: &mut egui::Ui) {
        ui.heading("Retour testeur");
        ui.label(
            "Copie locale dans var/feedback/. Une issue GitHub est créée sur azerothl/akasha-os (formulaire navigateur, ou API si jeton / gh).",
        );
        ui.horizontal(|ui| {
            ui.label("Titre");
            ui.text_edit_singleline(&mut self.fb_title);
        });
        ui.horizontal(|ui| {
            ui.label("Catégorie");
            egui::ComboBox::from_id_salt("fb_cat")
                .selected_text(&self.fb_category)
                .show_ui(ui, |ui| {
                    for c in ["bug", "ux", "perf", "security", "other"] {
                        ui.selectable_value(&mut self.fb_category, c.into(), c);
                    }
                });
            ui.label("Sévérité");
            egui::ComboBox::from_id_salt("fb_sev")
                .selected_text(&self.fb_severity)
                .show_ui(ui, |ui| {
                    for s in ["low", "medium", "high"] {
                        ui.selectable_value(&mut self.fb_severity, s.into(), s);
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Scénario");
            ui.text_edit_singleline(&mut self.fb_scenario);
        });
        ui.text_edit_multiline(&mut self.fb_body);
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
            let meta = serde_json::json!({
                "preview_version": self.version,
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "scenarios": {
                    "chat_offline": self.scen_chat,
                    "note_human": self.scen_note_human,
                    "note_agent": self.scen_note_agent,
                    "confirm": self.scen_confirm,
                    "audit": self.scen_audit,
                },
                "onboarding": self.onboarding,
            });
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
            if ui.button("Ouvrir le dossier feedback").clicked() {
                let dir = aos_home().join("var/feedback");
                #[cfg(windows)]
                let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                #[cfg(target_os = "linux")]
                let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
            }
        }
    }
}
