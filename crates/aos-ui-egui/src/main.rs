//! Agent OS Preview — UI egui (ADR 0003).
//!
//! Surface testeur : chat, dashboard, onboarding, notes, confirm, agents,
//! audit, scénarios guidés, retours (`feedback.submit`).

use aos_ipc::BusClient;
use aos_proto::{
    AgentCreateRequest, AgentGoal, AgentIdRequest, AgentInfo, AgentPromptOptimizeRequest,
    AgentPromptOptimizeResponse, AgentState, AgentSteerRequest, AuditEvent, AuditQueryRequest,
    ChatMessage, ChatSessionAppendRequest, ChatSessionCreateRequest, ChatSessionGetResponse,
    ChatSessionIdRequest, ChatSessionMeta, ChatSessionRenameRequest, ConfirmResponseRequest,
    DocumentRef, FeedbackSubmitRequest, FeedbackSubmitResponse, FilesGenerateRequest,
    FilesGenerateResponse, InferParams, InferRequest, McpServerInfo, MemContextRequest,
    MemContextResponse, MemHit, MemUserRecallRequest, MemUserRememberRequest, ModelInfo, ModelState,
    ModuleInvokeRequest, ModuleInvokeResponse, NetFetchRequest, NetFetchResponse, NetModeRequest,
    PendingConfirmation, SkillInfo, SystemMetrics, TokenEvent, WebSearchHit, WebSearchRequest,
    WebSearchResponse, SYSTEM_ASSISTANT_PROMPT,
};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

const PREVIEW_BANNER: &str =
    "Agent OS Preview 0.1 — exécuté sur Windows/Linux (échafaudage). Ce n'est pas encore l'OS bootable seL4.";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Chat,
    Memory,
    Notes,
    Agents,
    Audit,
    Scenarios,
    Feedback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OnboardingState {
    completed: bool,
    language: String,
    routing: String,
    trust_default: String,
}

impl Default for OnboardingState {
    fn default() -> Self {
        Self {
            completed: false,
            language: "fr".into(),
            routing: "local_only".into(),
            trust_default: "low".into(),
        }
    }
}

enum Cmd {
    /// Chat session active : historique + texte user (persisté côté platformd).
    Chat {
        session_id: String,
        history: Vec<(String, String)>,
        user_text: String,
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
    WebSearch { query: String },
    NetFetch { url: String },
    FilesGenerate {
        format: String,
        path: String,
        content: String,
        title: Option<String>,
    },
    Help,
    NotesList,
    NotesCreate { title: String, content: String },
    NotesSearch { query: String },
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
    },
    AgentKill { id: String },
    AgentPause { id: String },
    AgentSteer { id: String, text: String },
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
}

enum Evt {
    Delta(String),
    Done {
        text: String,
        session_id: String,
    },
    Error(String),
    Status(String),
    ChatSystem(String),
    Metrics(SystemMetrics),
    Agents(Vec<AgentInfo>),
    Notes(String),
    Audit(Vec<AuditEvent>),
    Confirms(Vec<PendingConfirmation>),
    FeedbackOk(FeedbackSubmitResponse),
    Sessions(Vec<ChatSessionMeta>),
    SessionLoaded {
        id: String,
        messages: Vec<(String, String)>,
    },
    MemHits(Vec<MemHit>),
    WebResults(Vec<WebSearchHit>),
    NetMode(bool),
    FileOk(String),
    Skills(Vec<SkillInfo>),
    McpServers(Vec<McpServerInfo>),
    PromptOptimized(String),
}

const SLASH_COMMANDS: &[(&str, &str)] = &[
    ("<texte>", "discuter avec l'assistant (modèle local)"),
    ("/commands", "cette liste"),
    ("/help", "état du système (services, agents, modèles)"),
    ("/agent <tâche>", "créer un agent (caps notes incluses)"),
    ("/notes", "lister les notes"),
    ("/notenew <titre> | <contenu>", "créer une note"),
    ("/notesearch <requête>", "recherche sémantique dans les notes"),
    ("/audit [n]", "n derniers événements d'audit"),
    ("/kill <id>", "tuer un agent"),
    ("/pause <id>", "suspendre un agent"),
];

fn main() -> eframe::Result<()> {
    let (cmd_tx, cmd_rx) = channel::<Cmd>();
    let (evt_tx, evt_rx) = channel::<Evt>();
    let version = std::env::var("AOS_PREVIEW_VERSION").unwrap_or_else(|_| "0.1.0".into());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title(format!("Agent OS Preview {version}")),
        ..Default::default()
    };
    eframe::run_native(
        &format!("Agent OS Preview {version}"),
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
                    &ChatSessionCreateRequest { title },
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
                model_id: None,
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
                        if !full.is_empty() {
                            let _ = bus
                                .call::<ChatSessionAppendRequest, aos_proto::ChatSessionMessage>(
                                    "chat.session.append",
                                    &ChatSessionAppendRequest {
                                        session_id: sid.clone(),
                                        role: "assistant".into(),
                                        content: full.clone(),
                                    },
                                    vec![],
                                )
                                .await;
                        }
                        let _ = evt_tx.send(Evt::Done {
                            text: full,
                            session_id: sid,
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
        Cmd::WebSearch { query } => {
            let req = WebSearchRequest {
                query,
                max_results: 5,
                caps: vec![
                    "net.connect:html.duckduckgo.com:443".into(),
                    "net.connect:api.search.brave.com:443".into(),
                    "net.connect:*:*".into(),
                ],
                actor: "human:ui".into(),
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
        Cmd::NetFetch { url } => {
            let req = NetFetchRequest {
                url,
                dest_path: None,
                max_bytes: 50 * 1024 * 1024,
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
            let mut out = String::from("Agent OS Preview — état\n");
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
        Cmd::NotesSearch { query } => {
            invoke_notes(
                &bus,
                &evt_tx,
                "notes.search",
                serde_json::json!({ "query": query }),
            )
            .await;
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
        } => {
            let mut req = AgentCreateRequest::simple(task.clone());
            req.system_prompt = system_prompt;
            req.skills = skills;
            req.tools = tools;
            req.mcp_servers = mcp_servers;
            req.documents = documents;
            req.optimize_prompt = optimize_prompt;
            req.goal = Some(AgentGoal {
                statement: task,
                success_criteria: vec![],
                max_steps,
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
                    let _ = evt_tx.send(Evt::Status(format!("agent créé : {}", r.agent_id)));
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
            let messages: Vec<(String, String)> = resp
                .messages
                .into_iter()
                .map(|m| {
                    let role = match m.role.as_str() {
                        "user" => "vous".into(),
                        "assistant" => "assistant".into(),
                        other => other.to_string(),
                    };
                    (role, m.content)
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
        ],
        trace_id: format!("ui-notes-{}", tool),
    };
    match bus
        .call::<ModuleInvokeRequest, ModuleInvokeResponse>("module.invoke", &req, vec![])
        .await
    {
        Ok(r) if r.ok => {
            let _ = evt_tx.send(Evt::Notes(
                serde_json::to_string_pretty(&r.result).unwrap_or_default(),
            ));
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
    chat: Vec<(String, String)>,
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
    gen_format: String,
    gen_content: String,
    gen_path: String,
    mem_query: String,
    mem_note: String,
    mem_hits: Vec<MemHit>,
    metrics: Option<SystemMetrics>,
    agents: Vec<AgentInfo>,
    confirms: Vec<PendingConfirmation>,
    notes_out: String,
    note_title: String,
    note_content: String,
    note_search: String,
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
    agent_detail_id: Option<String>,
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
}

impl UiApp {
    fn new(cmd_tx: Sender<Cmd>, evt_rx: Receiver<Evt>, version: String) -> Self {
        let onboarding = load_onboarding();
        let show_onboarding = !onboarding.completed;
        let _ = cmd_tx.send(Cmd::SessionBootstrap);
        Self {
            cmd_tx,
            evt_rx,
            version,
            tab: Tab::Chat,
            chat: vec![(
                "système".into(),
                format!(
                    "{PREVIEW_BANNER}\n\
                     Sessions persistées, mémoire, réseau opt-in.\n\
                     Tapez /commands — onglets Sessions / Mémoire / Notes…"
                ),
            )],
            streaming: String::new(),
            input: String::new(),
            chat_pending: false,
            sessions: Vec::new(),
            active_session: None,
            rename_buf: String::new(),
            network_online: false,
            web_query: String::new(),
            web_results: Vec::new(),
            fetch_url: String::new(),
            gen_format: "md".into(),
            gen_content: String::new(),
            gen_path: "/downloads/note.md".into(),
            mem_query: String::new(),
            mem_note: String::new(),
            mem_hits: Vec::new(),
            metrics: None,
            agents: Vec::new(),
            confirms: Vec::new(),
            notes_out: String::new(),
            note_title: String::new(),
            note_content: String::new(),
            note_search: String::new(),
            agent_task: String::new(),
            agent_system_prompt: String::new(),
            agent_docs: String::new(),
            agent_max_steps: 32,
            agent_optimize: false,
            skill_catalog: Vec::new(),
            skill_selected: Vec::new(),
            mcp_catalog: Vec::new(),
            mcp_selected: Vec::new(),
            tool_selected: vec![
                "notes.create".into(),
                "notes.list".into(),
                "notes.read".into(),
            ],
            agent_detail_id: None,
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
            self.chat.push((
                "système".into(),
                "aucune session — créez-en une dans le panneau Sessions".into(),
            ));
            return;
        };
        if self.chat_pending {
            self.chat.push(("vous".into(), text));
            self.chat.push((
                "système".into(),
                "réponse précédente encore en cours — patientez.".into(),
            ));
            return;
        }
        self.chat.push(("vous".into(), text.clone()));
        let history: Vec<(String, String)> = self
            .chat
            .iter()
            .filter(|(r, _)| r == "vous" || r == "assistant")
            .map(|(r, c)| {
                (
                    if r == "vous" {
                        "user".into()
                    } else {
                        "assistant".into()
                    },
                    c.clone(),
                )
            })
            .collect();
        self.streaming.clear();
        self.chat_pending = true;
        self.status = "assistant : génération…".into();
        let _ = self.cmd_tx.send(Cmd::Chat {
            session_id,
            history,
            user_text: text,
        });
        self.scen_chat = true;
    }

    fn handle_slash(&mut self, text: &str) {
        self.chat.push(("vous".into(), text.to_string()));
        let mut parts = text.splitn(2, char::is_whitespace);
        let cmd = parts.next().unwrap_or(text);
        let rest = parts.next().unwrap_or("").trim();
        match cmd {
            "/commands" => {
                let mut out = String::from("Commandes chat :\n");
                for (c, d) in SLASH_COMMANDS {
                    out.push_str(&format!("  {c} — {d}\n"));
                }
                self.chat.push(("système".into(), out));
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
                        self.chat.push((
                            "système".into(),
                            "usage : /notenew <titre> | <contenu>".into(),
                        ));
                        return;
                    }
                };
                if title.is_empty() || content.is_empty() {
                    self.chat.push((
                        "système".into(),
                        "usage : /notenew <titre> | <contenu>".into(),
                    ));
                    return;
                }
                let _ = self.cmd_tx.send(Cmd::NotesCreate { title, content });
                self.tab = Tab::Notes;
            }
            "/notesearch" => {
                if rest.is_empty() {
                    self.chat.push((
                        "système".into(),
                        "usage : /notesearch <requête>".into(),
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
                    self.chat.push((
                        "système".into(),
                        "usage : /agent <tâche>".into(),
                    ));
                    return;
                }
                self.pending_note_agent = rest.to_lowercase().contains("note");
                let mut skills = Vec::new();
                let mut tools = vec![
                    "notes.create".into(),
                    "notes.list".into(),
                    "notes.read".into(),
                ];
                if rest.to_lowercase().contains("note") {
                    skills.push("notes-writer".into());
                }
                if rest.to_lowercase().contains("plan") || rest.to_lowercase().contains("délégu") {
                    skills.push("planner".into());
                    tools.push("agent.spawn".into());
                    tools.push("agent.await".into());
                }
                let _ = self.cmd_tx.send(Cmd::AgentCreate {
                    task: rest.to_string(),
                    system_prompt: None,
                    skills,
                    tools,
                    mcp_servers: vec![],
                    documents: vec![],
                    optimize_prompt: false,
                    max_steps: 16,
                });
                self.tab = Tab::Agents;
            }
            "/audit" => {
                let n = rest.parse().unwrap_or(20);
                let _ = self.cmd_tx.send(Cmd::Audit { last: n });
                self.tab = Tab::Audit;
            }
            "/kill" => {
                if rest.is_empty() {
                    self.chat.push(("système".into(), "usage : /kill <id>".into()));
                    return;
                }
                let _ = self.cmd_tx.send(Cmd::AgentKill {
                    id: rest.to_string(),
                });
            }
            "/pause" => {
                if rest.is_empty() {
                    self.chat.push(("système".into(), "usage : /pause <id>".into()));
                    return;
                }
                let _ = self.cmd_tx.send(Cmd::AgentPause {
                    id: rest.to_string(),
                });
            }
            _ => {
                self.chat.push((
                    "système".into(),
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
                Evt::Done { text, session_id } => {
                    if self.active_session.as_deref() == Some(session_id.as_str()) && !text.is_empty()
                    {
                        self.chat.push(("assistant".into(), text));
                    }
                    self.streaming.clear();
                    self.chat_pending = false;
                    if self.status.starts_with("assistant :") {
                        self.status.clear();
                    }
                }
                Evt::Error(m) => {
                    self.status = m.clone();
                    self.chat.push(("système".into(), m));
                    self.streaming.clear();
                    self.chat_pending = false;
                }
                Evt::Status(m) => self.status = m,
                Evt::ChatSystem(m) => self.chat.push(("système".into(), m)),
                Evt::Metrics(m) => self.metrics = Some(m),
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
                Evt::Audit(a) => {
                    self.audit = a;
                    self.scen_audit = true;
                }
                Evt::Confirms(c) => self.confirms = c,
                Evt::FeedbackOk(r) => {
                    self.fb_result = format!(
                        "Enregistré : {}\nDossier : {}\nOuvrez ce dossier et joignez-le à une issue / canal cohorte.",
                        r.path, r.export_dir
                    );
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
                    let mut chat = vec![(
                        "système".into(),
                        format!("Session {id} — historique rechargé."),
                    )];
                    chat.extend(messages);
                    self.chat = chat;
                    self.streaming.clear();
                    self.chat_pending = false;
                }
                Evt::MemHits(h) => self.mem_hits = h,
                Evt::WebResults(r) => self.web_results = r,
                Evt::NetMode(online) => self.network_online = online,
                Evt::FileOk(msg) => {
                    self.status = msg.clone();
                    self.chat.push(("système".into(), msg));
                }
                Evt::Skills(list) => self.skill_catalog = list,
                Evt::McpServers(list) => self.mcp_catalog = list,
                Evt::PromptOptimized(p) => {
                    self.agent_system_prompt = p;
                    self.status = "prompt système optimisé".into();
                }
            }
        }

        if self.show_onboarding {
            egui::Window::new("Onboarding — Agent OS Preview")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(PREVIEW_BANNER);
                    ui.separator();
                    ui.label("Langue");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.onboarding.language, "fr".into(), "Français");
                        ui.radio_value(&mut self.onboarding.language, "en".into(), "English");
                    });
                    ui.label("Routage modèles");
                    ui.horizontal(|ui| {
                        ui.radio_value(
                            &mut self.onboarding.routing,
                            "local_only".into(),
                            "local_only (recommandé)",
                        );
                        ui.radio_value(&mut self.onboarding.routing, "balanced".into(), "balanced");
                    });
                    ui.label("Confiance agents par défaut");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.onboarding.trust_default, "low".into(), "basse");
                        ui.radio_value(
                            &mut self.onboarding.trust_default,
                            "medium".into(),
                            "moyenne",
                        );
                    });
                    ui.separator();
                    if ui.button("Terminer l'onboarding").clicked() {
                        self.onboarding.completed = true;
                        save_onboarding(&self.onboarding);
                        self.show_onboarding = false;
                        self.status = "onboarding terminé".into();
                    }
                });
        }

        egui::TopBottomPanel::top("banner").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(egui::Color32::from_rgb(220, 160, 40), PREVIEW_BANNER);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Signaler").clicked() {
                        self.tab = Tab::Feedback;
                    }
                    ui.label(format!("v{}", self.version));
                });
            });
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
                (Tab::Chat, "Chat"),
                (Tab::Memory, "Mémoire"),
                (Tab::Notes, "Notes"),
                (Tab::Agents, "Agents"),
                (Tab::Audit, "Audit"),
                (Tab::Scenarios, "Scénarios"),
                (Tab::Feedback, "Retour"),
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
            ui.heading("Réseau");
            let mut online = self.network_online;
            if ui
                .checkbox(&mut online, "Autoriser le réseau")
                .changed()
            {
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

        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Chat => self.ui_chat(ui),
            Tab::Memory => self.ui_memory(ui),
            Tab::Notes => self.ui_notes(ui),
            Tab::Agents => self.ui_agents(ui),
            Tab::Audit => self.ui_audit(ui),
            Tab::Scenarios => self.ui_scenarios(ui),
            Tab::Feedback => self.ui_feedback(ui),
        });
    }
}

impl UiApp {
    fn ui_chat(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(200.0);
                ui.heading("Sessions");
                if ui.button("+ Nouvelle").clicked() {
                    let n = self.sessions.len() + 1;
                    let _ = self.cmd_tx.send(Cmd::SessionCreate {
                        title: Some(format!("Session {n}")),
                    });
                }
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        for s in self.sessions.clone() {
                            let selected = self.active_session.as_deref() == Some(s.id.as_str());
                            if ui
                                .selectable_label(selected, format!("{} ({})", s.title, s.message_count))
                                .clicked()
                            {
                                let _ = self.cmd_tx.send(Cmd::SessionSelect { id: s.id.clone() });
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
                ui.add(
                    egui::TextEdit::singleline(&mut self.web_query)
                        .desired_width(180.0)
                        .hint_text("recherche web"),
                );
                if ui.button("Rechercher").clicked() && !self.web_query.is_empty() {
                    let _ = self.cmd_tx.send(Cmd::WebSearch {
                        query: self.web_query.clone(),
                    });
                }
                for hit in &self.web_results {
                    ui.small(format!("• {} — {}", hit.title, hit.url));
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.fetch_url)
                        .desired_width(180.0)
                        .hint_text("https://…"),
                );
                if ui.button("Télécharger URL").clicked() && !self.fetch_url.is_empty() {
                    let _ = self.cmd_tx.send(Cmd::NetFetch {
                        url: self.fetch_url.clone(),
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
                        .desired_width(180.0)
                        .hint_text("/downloads/…"),
                );
                ui.add(
                    egui::TextEdit::multiline(&mut self.gen_content)
                        .desired_width(180.0)
                        .desired_rows(3)
                        .hint_text("contenu"),
                );
                if ui.button("Générer fichier").clicked() && !self.gen_path.is_empty() {
                    let _ = self.cmd_tx.send(Cmd::FilesGenerate {
                        format: self.gen_format.clone(),
                        path: self.gen_path.clone(),
                        content: self.gen_content.clone(),
                        title: Some("Agent OS".into()),
                    });
                }
                if ui.button("Ouvrir downloads").clicked() {
                    let dir = aos_home().join("var/storage/data/downloads");
                    let _ = std::fs::create_dir_all(&dir);
                    #[cfg(windows)]
                    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                    #[cfg(target_os = "linux")]
                    let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
                }
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.heading("Conversation");
                if let Some(id) = &self.active_session {
                    ui.weak(format!("session {id}"));
                }
                egui::ScrollArea::vertical()
                    .max_height(ui.available_height() - 48.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for (role, text) in &self.chat {
                            ui.label(format!("[{role}]"));
                            ui.add(egui::Label::new(text).wrap());
                            ui.separator();
                        }
                        if !self.streaming.is_empty() {
                            ui.label("[assistant]");
                            ui.add(egui::Label::new(&self.streaming).wrap());
                        } else if self.chat_pending {
                            ui.label("[assistant]");
                            ui.weak("… en file / génération");
                        }
                    });
                ui.horizontal(|ui| {
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.input)
                            .desired_width(f32::INFINITY)
                            .hint_text("message ou /commands …"),
                    );
                    let send = ui.button("Envoyer").clicked()
                        || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                    if send {
                        self.send_chat();
                    }
                });
            });
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
        ui.heading("Notes");
        ui.horizontal(|ui| {
            if ui.button("Lister").clicked() {
                let _ = self.cmd_tx.send(Cmd::NotesList);
            }
            ui.text_edit_singleline(&mut self.note_search);
            if ui.button("Rechercher").clicked() && !self.note_search.is_empty() {
                let _ = self.cmd_tx.send(Cmd::NotesSearch {
                    query: self.note_search.clone(),
                });
            }
        });
        ui.separator();
        ui.label("Nouvelle note");
        ui.horizontal(|ui| {
            ui.label("Titre");
            ui.text_edit_singleline(&mut self.note_title);
        });
        ui.text_edit_multiline(&mut self.note_content);
        if ui.button("Créer").clicked()
            && !self.note_title.is_empty()
            && !self.note_content.is_empty()
        {
            let _ = self.cmd_tx.send(Cmd::NotesCreate {
                title: self.note_title.clone(),
                content: self.note_content.clone(),
            });
            self.note_title.clear();
            self.note_content.clear();
        }
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.monospace(&self.notes_out);
        });
    }

    fn ui_agents(&mut self, ui: &mut egui::Ui) {
        ui.heading("Agents — compose agentic");
        if ui.button("Rafraîchir catalogues (skills / MCP)").clicked() {
            let _ = self.cmd_tx.send(Cmd::AgentCatalogRefresh);
        }
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
                "fs.read",
                "fs.write",
                "fs.list",
                "web.search",
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
            });
        }

        ui.separator();
        ui.heading("Agents actifs");
        for a in self.agents.clone() {
            ui.horizontal(|ui| {
                let selected = self.agent_detail_id.as_deref() == Some(a.agent_id.as_str());
                if ui.selectable_label(selected, &a.agent_id).clicked() {
                    self.agent_detail_id = Some(a.agent_id.clone());
                    self.agent_steer_id = a.agent_id.clone();
                }
                ui.label(format!(
                    "[{:?}] step {}/{} {}",
                    a.state,
                    a.step,
                    a.max_steps,
                    a.current_task.clone().unwrap_or_default()
                ));
                if ui.button("Pause").clicked() {
                    let _ = self.cmd_tx.send(Cmd::AgentPause {
                        id: a.agent_id.clone(),
                    });
                }
                if ui.button("Kill").clicked() {
                    let _ = self.cmd_tx.send(Cmd::AgentKill {
                        id: a.agent_id.clone(),
                    });
                }
            });
            if !a.children.is_empty() {
                ui.small(format!("sous-agents: {}", a.children.join(", ")));
            }
            if !a.last_output.is_empty() {
                ui.label(egui::RichText::new(&a.last_output).small());
            }
        }

        if let Some(id) = self.agent_detail_id.clone() {
            if let Some(a) = self.agents.iter().find(|x| x.agent_id == id) {
                ui.separator();
                ui.heading(format!("Détail {id}"));
                ui.label(format!("Goal / directive : {}", a.directive));
                ui.label(format!("Parent : {:?}", a.parent_id));
                ui.label(format!("Enfants : {:?}", a.children));
                ui.label(format!("Caps : {}", a.caps.join(", ")));
            }
        }

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
        ui.label("Aucun envoi réseau automatique — fichier local dans var/feedback/.");
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
        if ui.button("Enregistrer le retour").clicked() && !self.fb_title.is_empty() {
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
