//! `aos-ui` — UI minimale P1.6 : shell conversationnel + dashboard ressources.
//!
//! TUI (ratatui) — le choix du framework GUI produit est tracé dans
//! `adr/0003-ui-framework.md`.
//!
//! - Panneau gauche : conversation avec l'assistant système (`model.infer`
//!   en flux via le bus) ;
//! - Panneau droit : dashboard ressources (VRAM/RAM/disque par modèle,
//!   agents actifs) rafraîchi toutes les 2 s ;
//! - Commandes : `/agent <tâche>` (délègue à un processus agent isolé),
//!   `/pause|/resume|/kill|/steer <id>`, `/load <modèle> [profil]`,
//!   `/models`, `/quit`.

use aos_agent::{intents as agent_intents, SubscribeRequest};
use aos_ipc::BusClient;
use aos_proto::{
    AgentCreateRequest, AgentCreateResponse, AgentIdRequest, AgentInfo, AgentOutputEvent,
    AgentSteerRequest, ChatMessage, InferParams, InferRequest, LoadRequest, ModelInfo,
    SystemMetrics, TokenEvent,
};
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap};
use ratatui::Terminal;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Événements internes de la boucle UI.
enum UiEvent {
    ChatDelta(String),
    ChatDone(String),
    ChatError(String),
    AgentEvent(String),
    Models(Vec<ModelInfo>),
    Agents(Vec<AgentInfo>),
    Metrics(SystemMetrics),
}

struct App {
    input: String,
    /// Historique conversationnel (rôle, texte).
    chat: Vec<(String, String)>,
    /// Réponse en cours de streaming.
    streaming: String,
    models: Vec<ModelInfo>,
    agents: Vec<AgentInfo>,
    metrics: Option<SystemMetrics>,
    agent_log: Vec<String>,
    status: String,
}

impl App {
    fn new() -> Self {
        Self {
            input: String::new(),
            chat: vec![(
                "système".into(),
                "Assistant Agent OS prêt. Tapez un message, ou /agent <tâche>, /load, /models, /kill <id>, /quit.".into(),
            )],
            streaming: String::new(),
            models: Vec::new(),
            agents: Vec::new(),
            metrics: None,
            agent_log: Vec::new(),
            status: "connexion…".into(),
        }
    }
}

fn draw(frame: &mut ratatui::Frame, app: &App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(3)])
        .split(frame.area());
    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(root[0]);

    // --- Chat (shell conversationnel) ---
    let mut lines: Vec<ratatui::text::Line> = Vec::new();
    for (role, text) in &app.chat {
        lines.push(ratatui::text::Line::from(format!("[{role}]")));
        for l in text.lines() {
            lines.push(ratatui::text::Line::from(l.to_string()));
        }
        lines.push(ratatui::text::Line::from(""));
    }
    if !app.streaming.is_empty() {
        lines.push(ratatui::text::Line::from("[assistant]"));
        for l in app.streaming.lines() {
            lines.push(ratatui::text::Line::from(l.to_string()));
        }
    }
    let chat = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Conversation"))
        .wrap(Wrap { trim: false });
    frame.render_widget(chat, main[0]);

    // --- Dashboard ---
    let dash = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(6),
            Constraint::Length(8),
        ])
        .split(main[1]);

    // Jauges RAM/VRAM (somme des plans).
    let (ram_ratio, ram_label) = match &app.metrics {
        Some(m) => {
            let ratio = if m.ram_total > 0 {
                m.ram_used as f64 / m.ram_total as f64
            } else {
                0.0
            };
            (
                ratio,
                format!(
                    "RAM {:.1}/{:.1} GiB (CPU {:.0}%)",
                    m.ram_used as f64 / (1 << 30) as f64,
                    m.ram_total as f64 / (1 << 30) as f64,
                    m.cpu_percent
                ),
            )
        }
        None => (0.0, "RAM ?".into()),
    };
    let vram_used: u64 = app
        .metrics
        .as_ref()
        .map(|m| m.models.iter().map(|m| m.vram_bytes).sum())
        .unwrap_or(0);
    let vram_ratio = vram_used as f64 / (8u64 << 30) as f64;
    let gauges = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(3),
        ])
        .split(dash[0]);
    frame.render_widget(
        Gauge::default()
            .block(Block::default().title("RAM hôte"))
            .ratio(ram_ratio.clamp(0.0, 1.0))
            .label(ram_label),
        gauges[0],
    );
    frame.render_widget(
        Gauge::default()
            .block(Block::default().title("VRAM (plans, budget 8 GiB)"))
            .ratio(vram_ratio.clamp(0.0, 1.0))
            .label(format!("{:.1} GiB", vram_used as f64 / (1 << 30) as f64)),
        gauges[1],
    );
    frame.render_widget(
        Paragraph::new(app.status.clone())
            .block(Block::default().borders(Borders::ALL).title("Statut")),
        gauges[2],
    );

    // Modèles.
    let model_items: Vec<ListItem> = app
        .models
        .iter()
        .map(|m| {
            let metrics = app
                .metrics
                .as_ref()
                .and_then(|mm| mm.models.iter().find(|x| x.model_id == m.id));
            let perf = metrics
                .and_then(|mm| mm.last_tok_s.map(|t| format!(" | {:.1} tok/s", t)))
                .unwrap_or_default();
            ListItem::new(format!(
                "{} [{:?}] {}{}",
                m.id,
                m.state,
                m.placement.clone().unwrap_or_default(),
                perf
            ))
        })
        .collect();
    frame.render_widget(
        List::new(model_items).block(Block::default().borders(Borders::ALL).title("Modèles")),
        dash[1],
    );

    // Agents + log.
    let mut agent_items: Vec<ListItem> = app
        .agents
        .iter()
        .map(|a| {
            ListItem::new(format!(
                "{} [{:?}] pid={} — {}",
                a.agent_id,
                a.state,
                a.pid.map(|p| p.to_string()).unwrap_or("-".into()),
                a.directive.chars().take(40).collect::<String>()
            ))
        })
        .collect();
    agent_items.push(ListItem::new("── log ──"));
    for l in app.agent_log.iter().rev().take(4) {
        agent_items.push(ListItem::new(l.clone()));
    }
    frame.render_widget(
        List::new(agent_items).block(Block::default().borders(Borders::ALL).title("Agents")),
        dash[2],
    );

    // Input.
    let input = Paragraph::new(app.input.as_str())
        .block(Block::default().borders(Borders::ALL).title("Entrée"));
    frame.render_widget(input, root[1]);
    frame.set_cursor_position((root[1].x + 1 + app.input.len() as u16, root[1].y + 1));
}

async fn refresh_models(bus: &BusClient, tx: &mpsc::Sender<UiEvent>) {
    if let Ok(models) = bus
        .call::<(), Vec<ModelInfo>>("model.list", &(), vec![])
        .await
    {
        let _ = tx.send(UiEvent::Models(models)).await;
    }
    if let Ok(metrics) = bus
        .call::<(), SystemMetrics>("model.metrics", &(), vec![])
        .await
    {
        let _ = tx.send(UiEvent::Metrics(metrics)).await;
    }
    if let Ok(agents) = bus
        .call::<(), Vec<AgentInfo>>(agent_intents::LIST, &(), vec![])
        .await
    {
        let _ = tx.send(UiEvent::Agents(agents)).await;
    }
}

async fn run_chat(bus: Arc<BusClient>, history: Vec<(String, String)>, tx: mpsc::Sender<UiEvent>) {
    let messages: Vec<ChatMessage> = history
        .iter()
        .map(|(r, c)| ChatMessage {
            role: r.clone(),
            content: c.clone(),
        })
        .collect();
    let req = InferRequest {
        model_id: None,
        messages,
        params: InferParams {
            max_tokens: 256,
            temperature: 0.7,
            top_p: 0.9,
            seed: None,
        },
        priority: 3, // interactive
    };
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
                        let _ = tx.send(UiEvent::ChatDelta(text)).await;
                    }
                    Ok(TokenEvent::Done { .. }) => {
                        let _ = tx.send(UiEvent::ChatDone(full.clone())).await;
                        return;
                    }
                    Ok(TokenEvent::Error { message }) => {
                        let _ = tx.send(UiEvent::ChatError(message)).await;
                        return;
                    }
                    _ => {}
                }
            }
            let _ = tx.send(UiEvent::ChatDone(full)).await;
        }
        Err(e) => {
            let _ = tx.send(UiEvent::ChatError(e.to_string())).await;
        }
    }
}

async fn run_agent_task(bus: Arc<BusClient>, directive: String, tx: mpsc::Sender<UiEvent>) {
    match bus
        .call::<AgentCreateRequest, AgentCreateResponse>(
            agent_intents::CREATE,
            &AgentCreateRequest {
                directive: directive.clone(),
                caps: vec![],
                model_id: None,
            },
            vec![],
        )
        .await
    {
        Ok(resp) => {
            let _ = tx
                .send(UiEvent::AgentEvent(format!(
                    "{} créé : {directive}",
                    resp.agent_id
                )))
                .await;
            // Souscription à sa sortie.
            match bus
                .call_stream::<SubscribeRequest, AgentOutputEvent>(
                    agent_intents::SUBSCRIBE,
                    &SubscribeRequest {
                        agent_id: resp.agent_id.clone(),
                    },
                    vec![],
                )
                .await
            {
                Ok(mut rx) => {
                    while let Some(ev) = rx.recv().await {
                        let line = match ev {
                            Ok(AgentOutputEvent::Token { text }) => text,
                            Ok(AgentOutputEvent::Log { line }) => format!("[log] {line}"),
                            Ok(AgentOutputEvent::StateChanged { state }) => {
                                format!("[état] {state:?}")
                            }
                            Ok(AgentOutputEvent::Error { message }) => format!("[err] {message}"),
                            Err(e) => format!("[bus] {e}"),
                        };
                        if tx.send(UiEvent::AgentEvent(line)).await.is_err() {
                            return;
                        }
                    }
                }
                Err(e) => {
                    let _ = tx
                        .send(UiEvent::AgentEvent(format!("subscribe: {e}")))
                        .await;
                }
            }
        }
        Err(e) => {
            let _ = tx.send(UiEvent::AgentEvent(format!("create: {e}"))).await;
        }
    }
}

async fn handle_command(
    bus: &Arc<BusClient>,
    input: &str,
    app: &mut App,
    tx: &mpsc::Sender<UiEvent>,
) {
    let mut parts = input.split_whitespace();
    let cmd = parts.next().unwrap_or("");
    match cmd {
        "/quit" => std::process::exit(0),
        "/models" => refresh_models(bus, tx).await,
        "/load" => {
            let id = parts.next().unwrap_or("").to_string();
            let profile = parts.next().unwrap_or("balanced").to_string();
            let bus = bus.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let r = bus
                    .call::<LoadRequest, aos_proto::LoadResponse>(
                        "model.load",
                        &LoadRequest {
                            model_id: id.clone(),
                            profile,
                            kv_tokens: 2048,
                        },
                        vec![],
                    )
                    .await;
                let msg = match r {
                    Ok(resp) => format!("chargé: {} ({})", resp.model_id, resp.placement),
                    Err(e) => format!("échec load {id}: {e}"),
                };
                let _ = tx.send(UiEvent::ChatError(msg)).await;
            });
        }
        "/agent" => {
            let directive = input.trim_start_matches("/agent").trim().to_string();
            if directive.is_empty() {
                app.status = "usage: /agent <tâche>".into();
                return;
            }
            let bus = bus.clone();
            let tx = tx.clone();
            tokio::spawn(run_agent_task(bus, directive, tx));
        }
        "/kill" | "/pause" | "/resume" => {
            let id = parts.next().unwrap_or("").to_string();
            let intent = match cmd {
                "/kill" => agent_intents::KILL,
                "/pause" => agent_intents::PAUSE,
                _ => agent_intents::RESUME,
            };
            let bus = bus.clone();
            let idc = id.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let r = bus
                    .call::<AgentIdRequest, bool>(intent, &AgentIdRequest { agent_id: id }, vec![])
                    .await;
                let _ = tx
                    .send(UiEvent::AgentEvent(format!(
                        "{intent} {idc}: {:?}",
                        r.is_ok()
                    )))
                    .await;
            });
        }
        "/steer" => {
            let id = parts.next().unwrap_or("").to_string();
            let rest = input
                .trim_start_matches("/steer")
                .trim_start_matches(&id)
                .trim()
                .to_string();
            let bus = bus.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let r = bus
                    .call::<AgentSteerRequest, bool>(
                        agent_intents::STEER,
                        &AgentSteerRequest {
                            agent_id: id,
                            directive: rest,
                        },
                        vec![],
                    )
                    .await;
                let _ = tx
                    .send(UiEvent::AgentEvent(format!("steer: {:?}", r.is_ok())))
                    .await;
            });
        }
        _ => {
            app.status = format!("commande inconnue: {cmd}");
        }
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let bus_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("127.0.0.1:{}", aos_ipc::DEFAULT_BUS_PORT));

    let bus = match BusClient::connect(&bus_addr, "ui").await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("impossible de joindre le bus {bus_addr}: {e}");
            eprintln!("lancer d'abord: aos-busd, aos-modeld, aos-agentd");
            return Ok(());
        }
    };

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    app.status = format!("bus {bus_addr} connecté");

    let (tx, mut rx) = mpsc::channel::<UiEvent>(256);
    let mut keys = EventStream::new();

    // Tick périodique : métriques + listes.
    {
        let bus = bus.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            loop {
                refresh_models(&bus, &tx).await;
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }

    loop {
        terminal.draw(|f| draw(f, &app))?;
        tokio::select! {
            maybe_key = keys.next() => {
                if let Some(Ok(Event::Key(k))) = maybe_key {
                    if k.kind == KeyEventKind::Press {
                        match k.code {
                            KeyCode::Enter => {
                                let input = app.input.trim().to_string();
                                app.input.clear();
                                if input.starts_with('/') {
                                    handle_command(&bus, &input, &mut app, &tx).await;
                                } else if !input.is_empty() {
                                    app.chat.push(("vous".into(), input));
                                    let history: Vec<(String, String)> = app.chat.iter()
                                        .filter(|(r, _)| r == "vous" || r == "assistant")
                                        .map(|(r, c)| (if r == "vous" { "user".into() } else { "assistant".into() }, c.clone()))
                                        .collect();
                                    app.streaming.clear();
                                    tokio::spawn(run_chat(bus.clone(), history, tx.clone()));
                                }
                            }
                            KeyCode::Char(c) => app.input.push(c),
                            KeyCode::Backspace => { app.input.pop(); }
                            KeyCode::Esc => break,
                            _ => {}
                        }
                    }
                }
            }
            Some(ev) = rx.recv() => {
                match ev {
                    UiEvent::ChatDelta(t) => app.streaming.push_str(&t),
                    UiEvent::ChatDone(full) => {
                        if !full.is_empty() {
                            app.chat.push(("assistant".into(), full));
                        }
                        app.streaming.clear();
                    }
                    UiEvent::ChatError(m) => {
                        app.chat.push(("système".into(), m));
                        app.streaming.clear();
                    }
                    UiEvent::AgentEvent(l) => {
                        app.agent_log.push(l.chars().take(200).collect());
                        if app.agent_log.len() > 200 { app.agent_log.remove(0); }
                    }
                    UiEvent::Models(m) => app.models = m,
                    UiEvent::Agents(a) => app.agents = a,
                    UiEvent::Metrics(m) => app.metrics = Some(m),
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
