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
    AgentSteerRequest, ChatMessage, InferParams, InferRequest, LoadRequest, ModelInfo, ModelState,
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
    Confirm(aos_proto::PendingConfirmation),
}

struct App {
    input: String,
    /// Historique conversationnel (rôle, texte).
    chat: Vec<(String, String)>,
    /// Réponse en cours de streaming.
    streaming: String,
    /// Décalage de scroll du panneau conversation (lignes).
    chat_scroll: i32,
    /// Suivi automatique en bas de conversation (réactivé à PageDown en bas).
    follow: bool,
    models: Vec<ModelInfo>,
    agents: Vec<AgentInfo>,
    metrics: Option<SystemMetrics>,
    agent_log: Vec<String>,
    pending_confirms: Vec<aos_proto::PendingConfirmation>,
    status: String,
}

impl App {
    fn new() -> Self {
        Self {
            input: String::new(),
            chat: vec![(
                "système".into(),
                "Assistant Akasha OS prêt. /commands pour les commandes, /help pour l'état du système.".into(),
            )],
            streaming: String::new(),
            chat_scroll: 0,
            follow: true,
            models: Vec::new(),
            agents: Vec::new(),
            metrics: None,
            agent_log: Vec::new(),
            pending_confirms: Vec::new(),
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
    // Scroll : suivi auto en bas, ou décalage manuel (PageUp/PageDown).
    let visible = main[0].height.saturating_sub(2) as i32;
    let max_scroll = (lines.len() as i32 - visible).max(0);
    let scroll = if app.follow {
        max_scroll
    } else {
        app.chat_scroll.clamp(0, max_scroll)
    };
    let title = if app.follow {
        "Conversation".to_string()
    } else {
        format!("Conversation (scroll {scroll}/{max_scroll} — PageDown pour suivre)")
    };
    let chat = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .scroll((scroll.max(0) as u16, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(chat, main[0]);

    // --- Dashboard ---
    let dash = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(5),
            Constraint::Length(7),
            Constraint::Length(6),
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

    // Confirmations bloquantes en attente (§9.4).
    let confirm_items: Vec<ListItem> = if app.pending_confirms.is_empty() {
        vec![ListItem::new("aucune confirmation en attente")]
    } else {
        app.pending_confirms
            .iter()
            .map(|p| {
                ListItem::new(format!(
                    "{} : {} {} → /confirm {} | /deny {}",
                    p.id, p.actor, p.action, p.id, p.id
                ))
            })
            .collect()
    };
    frame.render_widget(
        List::new(confirm_items).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Confirmations"),
        ),
        dash[3],
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
    let mut messages: Vec<ChatMessage> = vec![ChatMessage {
        role: "system".into(),
        content: aos_proto::SYSTEM_ASSISTANT_PROMPT.into(),
    }];
    messages.extend(history.iter().map(|(r, c)| ChatMessage {
        role: r.clone(),
        content: c.clone(),
    }));
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
        data_refs: vec![],
        routing: None,
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
            &{
                let mut r = AgentCreateRequest::simple(directive.clone());
                r.caps = vec!["tool.invoke:notes".into()];
                r
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
                            Ok(AgentOutputEvent::Progress {
                                step,
                                max_steps,
                                current_task,
                            }) => format!(
                                "[progress] {step}/{max_steps} {}",
                                current_task.unwrap_or_default()
                            ),
                            Ok(other) => format!("[{other:?}]"),
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

/// Table des commandes (affichée par /commands).
const COMMANDS: &[(&str, &str)] = &[
    (
        "<texte>",
        "discuter avec l'assistant système (modèle local)",
    ),
    (
        "/agent <tâche>",
        "déléguer une tâche à un agent (processus isolé)",
    ),
    ("/pause <id>", "suspendre un agent"),
    ("/resume <id>", "reprendre un agent"),
    ("/steer <id> <txt>", "rediriger un agent en cours"),
    ("/kill <id>", "tuer un agent (sans impact système)"),
    ("/models", "lister les modèles et leur état"),
    (
        "/load <modèle> [profil]",
        "charger un modèle (latency/balanced/memory-saver/cpu-only)",
    ),
    ("/modules", "lister les modules installés et leurs outils"),
    ("/install <dir>", "installer un module (.aospkg)"),
    ("/notes", "lister les notes (module notes)"),
    ("/note <titre>", "lire une note"),
    ("/notenew <titre> | <contenu>", "créer une note"),
    (
        "/notesearch <requête>",
        "recherche sémantique dans les notes",
    ),
    ("/audit [n]", "n derniers événements du journal signé"),
    (
        "/undo <chemin>",
        "annuler la dernière écriture sur un fichier",
    ),
    ("/commands", "cette liste"),
    (
        "/help",
        "état du système (services, agents, mémoire, modèles)",
    ),
    ("/quit ou Échap", "quitter"),
];

/// Affiche un résultat notes lisible (liste structurée ou JSON).
fn format_notes_tool_result(tool: &str, result: &serde_json::Value) -> String {
    if tool == "notes.list" {
        if let Some(arr) = result.get("notes").and_then(|n| n.as_array()) {
            let mut out = String::from("Notes :\n");
            for item in arr {
                if let Some(path) = item.as_str() {
                    out.push_str(&format!("  - {path}\n"));
                    continue;
                }
                let title = item
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("?");
                let path = item.get("path").and_then(|p| p.as_str()).unwrap_or("");
                let excerpt = item
                    .get("excerpt")
                    .and_then(|e| e.as_str())
                    .unwrap_or("");
                if excerpt.is_empty() {
                    out.push_str(&format!("  - {title} ({path})\n"));
                } else {
                    out.push_str(&format!("  - {title} ({path})\n    {excerpt}\n"));
                }
            }
            return out;
        }
    }
    result.to_string()
}

async fn show_help(bus: &Arc<BusClient>, tx: &mpsc::Sender<UiEvent>) {
    // Services vivants (découverte bus).
    let mut services = Vec::new();
    for (name, probe) in [
        ("modeld", "model.list"),
        ("agentd", "agent.list"),
        ("platformd", "module.list"),
    ] {
        let up = bus.lookup(probe).await.unwrap_or(false);
        services.push(format!("{name}: {}", if up { "up" } else { "DOWN" }));
    }
    // Modèles.
    let models: Vec<ModelInfo> = bus
        .call("model.list", &(), vec![])
        .await
        .unwrap_or_default();
    let loaded: Vec<&ModelInfo> = models
        .iter()
        .filter(|m| matches!(m.state, ModelState::Loaded | ModelState::PartiallyOffloaded))
        .collect();
    // Agents.
    let agents: Vec<AgentInfo> = bus
        .call(agent_intents::LIST, &(), vec![])
        .await
        .unwrap_or_default();
    let running = agents
        .iter()
        .filter(|a| matches!(a.state, aos_proto::AgentState::Running))
        .count();
    // Métriques hôte.
    let metrics: Option<SystemMetrics> = bus.call("model.metrics", &(), vec![]).await.ok();
    // Mémoire.
    let mem: Option<aos_proto::MemStats> = bus.call("mem.stats", &(), vec![]).await.ok();
    // Modules.
    let modules: Vec<aos_proto::ModuleInfo> = bus
        .call("module.list", &(), vec![])
        .await
        .unwrap_or_default();
    // Audit (comptage via requête large).
    let audit_count = bus
        .call::<aos_proto::AuditQueryRequest, Vec<aos_proto::AuditEvent>>(
            "audit.query",
            &aos_proto::AuditQueryRequest {
                trace_id: None,
                actor: None,
                action: None,
                last: 10_000,
            },
            vec![],
        )
        .await
        .map(|v| v.len())
        .unwrap_or(0);

    let mut out = String::new();
    out.push_str("Akasha OS — état du système (phases P0–P2 validées)\n");
    out.push_str(&format!("services : {}\n", services.join(", ")));
    out.push_str(&format!(
        "modèles : {} chargés / {} au registry{}\n",
        loaded.len(),
        models.len(),
        loaded
            .first()
            .map(|m| format!(" (ex: {})", m.id))
            .unwrap_or_default()
    ));
    out.push_str(&format!(
        "agents : {} actifs / {} total\n",
        running,
        agents.len()
    ));
    if let Some(m) = &metrics {
        out.push_str(&format!(
            "hôte : RAM {:.1}/{:.1} GiB, CPU {:.0}%\n",
            m.ram_used as f64 / (1 << 30) as f64,
            m.ram_total as f64 / (1 << 30) as f64,
            m.cpu_percent
        ));
    }
    if let Some(ms) = &mem {
        out.push_str(&format!(
            "mémoire : {} souvenirs épisodiques ({}), {} working\n",
            ms.episodic_total,
            ms.namespaces
                .iter()
                .map(|(n, c)| format!("{n}:{c}"))
                .collect::<Vec<_>>()
                .join(", "),
            ms.working_agents
        ));
    }
    out.push_str(&format!("modules : {} installés\n", modules.len()));
    out.push_str(&format!("audit : {} événements signés\n", audit_count));
    out.push_str("→ /commands pour la liste des commandes");
    let _ = tx.send(UiEvent::ChatError(out)).await;
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
        "/commands" => {
            let out = COMMANDS
                .iter()
                .map(|(c, d)| format!("{c:<32} {d}"))
                .collect::<Vec<_>>()
                .join("\n");
            app.chat
                .push(("système".into(), format!("Commandes :\n{out}")));
        }
        "/help" => {
            let bus = bus.clone();
            let tx = tx.clone();
            tokio::spawn(async move { show_help(&bus, &tx).await });
        }
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
        // --- P2 : modules, notes, audit, undo ---
        "/modules" => {
            let bus = bus.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let r = bus
                    .call::<(), Vec<aos_proto::ModuleInfo>>("module.list", &(), vec![])
                    .await;
                let msg = match r {
                    Ok(mods) if mods.is_empty() => "aucun module installé".to_string(),
                    Ok(mods) => mods
                        .iter()
                        .map(|m| format!("{} v{} [{}]", m.name, m.version, m.tools.join(", ")))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    Err(e) => format!("module.list: {e}"),
                };
                let _ = tx.send(UiEvent::ChatError(msg)).await;
            });
        }
        "/install" => {
            let dir = parts.next().unwrap_or("").to_string();
            let bus = bus.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let r = bus
                    .call::<aos_proto::ModuleInstallRequest, aos_proto::ModuleInfo>(
                        "module.install",
                        &aos_proto::ModuleInstallRequest {
                            source_dir: dir.clone(),
                            approved_caps: None,
                            actor: "human:ui".into(),
                            actor_caps: vec![],
                        },
                        vec![],
                    )
                    .await;
                let msg = match r {
                    Ok(m) => format!(
                        "module {} v{} installé (caps: {})",
                        m.name,
                        m.version,
                        m.granted_caps.join(", ")
                    ),
                    Err(e) => format!("install: {e}"),
                };
                let _ = tx.send(UiEvent::ChatError(msg)).await;
            });
        }
        "/notes" | "/note" | "/notenew" | "/notesearch" => {
            // Surface humaine du module « notes » (module.invoke en human:ui).
            let (tool, args) = match cmd {
                "/notes" => ("notes.list", serde_json::json!({})),
                "/note" => (
                    "notes.read",
                    serde_json::json!({"title": parts.collect::<Vec<_>>().join(" ")}),
                ),
                "/notenew" => {
                    let rest = input.trim_start_matches("/notenew").trim();
                    let mut split = rest.splitn(2, '|');
                    let title = split.next().unwrap_or("").trim().to_string();
                    let content = split.next().unwrap_or("").trim().to_string();
                    (
                        "notes.create",
                        serde_json::json!({"title": title, "content": content}),
                    )
                }
                _ => (
                    "notes.search",
                    serde_json::json!({"query": parts.collect::<Vec<_>>().join(" ")}),
                ),
            };
            let bus = bus.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let r = bus
                    .call::<aos_proto::ModuleInvokeRequest, aos_proto::ModuleInvokeResponse>(
                        "module.invoke",
                        &aos_proto::ModuleInvokeRequest {
                            module: "notes".into(),
                            tool: tool.to_string(),
                            args,
                            actor: "human:ui".into(),
                            actor_caps: vec![],
                            trace_id: String::new(),
                        },
                        vec![],
                    )
                    .await;
                let msg = match r {
                    Ok(resp) if resp.ok => format_notes_tool_result(tool, &resp.result),
                    Ok(resp) => format!("outil refusé: {}", resp.error.unwrap_or_default()),
                    Err(e) => format!("invoke: {e}"),
                };
                let _ = tx.send(UiEvent::ChatError(msg)).await;
            });
        }
        "/audit" => {
            let n: usize = parts.next().and_then(|s| s.parse().ok()).unwrap_or(10);
            let bus = bus.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let r = bus
                    .call::<aos_proto::AuditQueryRequest, Vec<aos_proto::AuditEvent>>(
                        "audit.query",
                        &aos_proto::AuditQueryRequest {
                            trace_id: None,
                            actor: None,
                            action: None,
                            last: n,
                        },
                        vec![],
                    )
                    .await;
                match r {
                    Ok(events) => {
                        for e in events {
                            let _ = tx
                                .send(UiEvent::AgentEvent(format!(
                                    "#{} {} {} → {} ({})",
                                    e.seq, e.actor, e.action, e.target, e.trace_id
                                )))
                                .await;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(UiEvent::AgentEvent(format!("audit: {e}"))).await;
                    }
                }
            });
        }
        "/undo" => {
            let path = parts.collect::<Vec<_>>().join(" ");
            let bus = bus.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let r = bus
                    .call::<aos_proto::FsUndoRequest, aos_proto::FsUndoResponse>(
                        "fs.undo",
                        &aos_proto::FsUndoRequest {
                            path: path.clone(),
                            actor: "human:ui".into(),
                            trace_id: String::new(),
                        },
                        vec![],
                    )
                    .await;
                let msg = match r {
                    Ok(resp) => format!("undo {}: {}", resp.path, resp.description),
                    Err(e) => format!("undo: {e}"),
                };
                let _ = tx.send(UiEvent::ChatError(msg)).await;
            });
        }
        // --- P3 : confirmations, trust, routage ---
        "/confirm" | "/deny" => {
            let approved = cmd == "/confirm";
            let id = parts.collect::<Vec<_>>().join(" ");
            if id.is_empty() {
                app.status = "usage: /confirm <id> ou /deny <id>".into();
            } else {
                let bus = bus.clone();
                let tx = tx.clone();
                tokio::spawn(async move {
                    let r = bus
                        .call::<aos_proto::ConfirmResponseRequest, bool>(
                            "confirm.respond",
                            &aos_proto::ConfirmResponseRequest {
                                id: id.clone(),
                                approved,
                            },
                            vec![],
                        )
                        .await;
                    let msg = match r {
                        Ok(true) => format!(
                            "confirmation {id} : {}",
                            if approved { "approuvée" } else { "refusée" }
                        ),
                        Ok(false) => format!("confirmation {id} inconnue/expirée"),
                        Err(e) => format!("confirm.respond: {e}"),
                    };
                    let _ = tx.send(UiEvent::ChatError(msg)).await;
                });
            }
        }
        "/trust" => {
            let args: Vec<String> = parts.map(|s| s.to_string()).collect();
            let bus = bus.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                match args.len() {
                    1 => {
                        let r = bus
                            .call::<aos_proto::TrustGetRequest, aos_proto::TrustProfile>(
                                "trust.get",
                                &aos_proto::TrustGetRequest {
                                    agent_id: args[0].clone(),
                                },
                                vec![],
                            )
                            .await;
                        let msg = match r {
                            Ok(p) => format!(
                                "{} : score {:.2} (palier {}), {} succès / {} échecs",
                                p.agent_id, p.score, p.tier, p.success_count, p.failure_count
                            ),
                            Err(e) => format!("trust.get: {e}"),
                        };
                        let _ = tx.send(UiEvent::ChatError(msg)).await;
                    }
                    2 => {
                        let score: f32 = args[1].parse().unwrap_or(0.5);
                        let r = bus
                            .call::<aos_proto::TrustSetRequest, bool>(
                                "trust.set",
                                &aos_proto::TrustSetRequest {
                                    agent_id: args[0].clone(),
                                    score,
                                },
                                vec![],
                            )
                            .await;
                        let msg = match r {
                            Ok(true) => format!("{} : score fixé à {:.2}", args[0], score),
                            _ => "trust.set: échec".to_string(),
                        };
                        let _ = tx.send(UiEvent::ChatError(msg)).await;
                    }
                    _ => {
                        let _ = tx
                            .send(UiEvent::ChatError(
                                "usage: /trust <agent_id> [score 0..1]".into(),
                            ))
                            .await;
                    }
                }
            });
        }
        "/routing" => {
            let mode = parts.collect::<Vec<_>>().join(" ");
            let bus = bus.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                if mode.is_empty() {
                    let _ = tx
                        .send(UiEvent::ChatError(
                            "usage: /routing balanced|local_only|remote_only".into(),
                        ))
                        .await;
                    return;
                }
                let r = bus
                    .call::<aos_proto::SetRoutingRequest, Result<(), String>>(
                        "model.set_routing",
                        &aos_proto::SetRoutingRequest { mode: mode.clone() },
                        vec![],
                    )
                    .await;
                let msg = match r {
                    Ok(Ok(())) => format!("routage : {mode}"),
                    Ok(Err(e)) => format!("routage refusé: {e}"),
                    Err(e) => format!("set_routing: {e}"),
                };
                let _ = tx.send(UiEvent::ChatError(msg)).await;
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

    // Abonnement aux confirmations bloquantes (Control bar, §9.4).
    {
        let bus = bus.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            if let Ok(mut rx) = bus
                .call_stream::<serde_json::Value, aos_proto::PendingConfirmation>(
                    "confirm.subscribe",
                    &serde_json::json!({}),
                    vec![],
                )
                .await
            {
                while let Some(ev) = rx.recv().await {
                    if let Ok(p) = ev {
                        let _ = tx.send(UiEvent::Confirm(p)).await;
                    }
                }
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
                            KeyCode::Backspace => {
                                app.input.pop();
                            }
                            KeyCode::PageUp => {
                                app.follow = false;
                                app.chat_scroll -= 10;
                            }
                            KeyCode::PageDown => {
                                app.chat_scroll += 10;
                                // Réactive le suivi quand on revient en bas.
                                app.follow = true;
                            }
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
                    UiEvent::Confirm(p) => {
                        app.pending_confirms.push(p);
                        app.follow = true;
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
