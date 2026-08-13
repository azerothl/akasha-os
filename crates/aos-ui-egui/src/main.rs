//! Agent OS Preview — UI egui (ADR 0003).
//!
//! Surface testeur : chat, dashboard, onboarding, notes, confirm, agents,
//! audit, scénarios guidés, retours (`feedback.submit`).

use aos_ipc::BusClient;
use aos_proto::{
    AgentCreateRequest, AgentIdRequest, AgentInfo, AgentState, AgentSteerRequest, AuditEvent,
    AuditQueryRequest, ChatMessage, ConfirmResponseRequest, FeedbackSubmitRequest,
    FeedbackSubmitResponse, InferParams, InferRequest, ModelInfo, ModelState, ModuleInvokeRequest,
    ModuleInvokeResponse, PendingConfirmation, SystemMetrics, TokenEvent, SYSTEM_ASSISTANT_PROMPT,
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
    Chat(Vec<(String, String)>),
    Help,
    NotesList,
    NotesCreate { title: String, content: String },
    NotesSearch { query: String },
    Confirm { id: String, approved: bool },
    AgentCreate { task: String },
    AgentKill { id: String },
    AgentPause { id: String },
    AgentSteer { id: String, text: String },
    Audit { last: usize },
    Feedback(FeedbackSubmitRequest),
    KillAuditd,
    RefreshConfirms,
}

enum Evt {
    Delta(String),
    Done(String),
    Error(String),
    Status(String),
    ChatSystem(String),
    Metrics(SystemMetrics),
    Agents(Vec<AgentInfo>),
    Notes(String),
    Audit(Vec<AuditEvent>),
    Confirms(Vec<PendingConfirmation>),
    FeedbackOk(FeedbackSubmitResponse),
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
        Cmd::Chat(history) => {
            let _ = evt_tx.send(Evt::Status(
                "assistant : génération en cours…".into(),
            ));
            let mut messages = vec![ChatMessage {
                role: "system".into(),
                content: SYSTEM_ASSISTANT_PROMPT.into(),
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
                        let _ = evt_tx.send(Evt::Done(full));
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
        Cmd::AgentCreate { task } => {
            match bus
                .call::<AgentCreateRequest, aos_proto::AgentCreateResponse>(
                    aos_agent::intents::CREATE,
                    &AgentCreateRequest {
                        directive: task,
                        // Preview : cap notes pour le scénario « note via agent »
                        // (sinon module_rt refuse tool.invoke:notes).
                        caps: vec!["tool.invoke:notes".into()],
                        model_id: None,
                    },
                    vec![],
                )
                .await
            {
                Ok(r) => {
                    let _ = evt_tx.send(Evt::Status(format!(
                        "agent créé : {} (cap tool.invoke:notes)",
                        r.agent_id
                    )));
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
    metrics: Option<SystemMetrics>,
    agents: Vec<AgentInfo>,
    confirms: Vec<PendingConfirmation>,
    notes_out: String,
    note_title: String,
    note_content: String,
    note_search: String,
    agent_task: String,
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
        Self {
            cmd_tx,
            evt_rx,
            version,
            tab: Tab::Chat,
            chat: vec![(
                "système".into(),
                format!(
                    "{PREVIEW_BANNER}\n\
                     Tapez /commands pour la liste des commandes, /help pour l'état du système.\n\
                     Onglets : notes, agents, audit, scénarios, retours."
                ),
            )],
            streaming: String::new(),
            input: String::new(),
            chat_pending: false,
            metrics: None,
            agents: Vec::new(),
            confirms: Vec::new(),
            notes_out: String::new(),
            note_title: String::new(),
            note_content: String::new(),
            note_search: String::new(),
            agent_task: String::new(),
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
        if self.chat_pending {
            self.chat.push(("vous".into(), text));
            self.chat.push((
                "système".into(),
                "réponse précédente encore en cours — patientez (indicateur « … en file / génération »)."
                    .into(),
            ));
            return;
        }
        self.chat.push(("vous".into(), text));
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
        self.status = "assistant : génération… (prioritaire, mais attend la fin d'un infer agent en cours)"
            .into();
        let _ = self.cmd_tx.send(Cmd::Chat(history));
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
                let _ = self.cmd_tx.send(Cmd::AgentCreate {
                    task: rest.to_string(),
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
                Evt::Done(full) => {
                    if !full.is_empty() {
                        self.chat.push(("assistant".into(), full));
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
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{} — {} sur {} ({})",
                            c.id, c.action, c.target, c.reason
                        ));
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
                }
            }
        });

        egui::SidePanel::left("tabs").exact_width(140.0).show(ctx, |ui| {
            ui.heading("Preview");
            for (tab, label) in [
                (Tab::Chat, "Chat"),
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
        ui.heading("Conversation");
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
                    ui.weak("… en file / génération (les agents running utilisent aussi le GPU)");
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
        ui.heading("Agents");
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.agent_task);
            if ui.button("Créer").clicked() && !self.agent_task.is_empty() {
                self.pending_note_agent = self.agent_task.to_lowercase().contains("note");
                let _ = self.cmd_tx.send(Cmd::AgentCreate {
                    task: self.agent_task.clone(),
                });
                self.agent_task.clear();
            }
        });
        ui.separator();
        for a in self.agents.clone() {
            ui.horizontal(|ui| {
                ui.label(format!("{} [{:?}]", a.agent_id, a.state));
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
            if !a.last_output.is_empty() {
                ui.label(egui::RichText::new(&a.last_output).small());
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
                "Protocole terminé — envoyez un retour (onglet Retour).",
            );
        }
        if ui.button("Demander une confirmation test (fs.delete)").clicked() {
            // Déclenche via policy : on demande au bus une delete qui exige confirm.
            // Simplifié : message d'aide.
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
