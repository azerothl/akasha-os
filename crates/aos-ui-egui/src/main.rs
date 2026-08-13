//! Prototype GUI **egui** (ADR 0003) : chat streamé + dashboard + agents.
//!
//! Pont : runtime tokio en thread d'arrière-plan, événements vers le GUI via
//! canal std + `Context::request_repaint`. Commandes GUI → runtime via canal.

use aos_ipc::BusClient;
use aos_proto::{
    AgentInfo, ChatMessage, InferParams, InferRequest, ModelInfo, SystemMetrics, TokenEvent,
};
use eframe::egui;
use std::sync::mpsc::{channel, Receiver, Sender};

/// Commandes GUI → runtime.
enum Cmd {
    Chat(Vec<(String, String)>),
}

/// Événements runtime → GUI.
enum Evt {
    Delta(String),
    Done(String),
    Error(String),
    Models(Vec<ModelInfo>),
    Metrics(SystemMetrics),
    Agents(Vec<AgentInfo>),
}

fn main() -> eframe::Result<()> {
    let (cmd_tx, cmd_rx) = channel::<Cmd>();
    let (evt_tx, evt_rx) = channel::<Evt>();

    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Agent OS — prototype egui (ADR 0003)",
        options,
        Box::new(move |cc| {
            // Le contexte egui permet de réveiller le rendu depuis le thread tokio.
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().expect("tokio");
                rt.block_on(runtime_main(cmd_rx, evt_tx, ctx));
            });
            Ok(Box::new(UiApp::new(cmd_tx, evt_rx)))
        }),
    )
}

async fn runtime_main(cmd_rx: Receiver<Cmd>, evt_tx: Sender<Evt>, egui_ctx: egui::Context) {
    let bus = BusClient::connect("127.0.0.1:47001", "ui-egui")
        .await
        .expect("bus requis (run-demo.ps1)");
    // Rafraîchissement périodique des métriques.
    {
        let bus = bus.clone();
        let evt_tx = evt_tx.clone();
        let egui_ctx = egui_ctx.clone();
        tokio::spawn(async move {
            loop {
                if let Ok(m) = bus
                    .call::<(), Vec<ModelInfo>>("model.list", &(), vec![])
                    .await
                {
                    let _ = evt_tx.send(Evt::Models(m));
                }
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
                egui_ctx.request_repaint();
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
    }
    // Commandes du GUI (canal std → pont bloquant → tâches tokio).
    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            Cmd::Chat(history) => {
                let bus = bus.clone();
                let evt_tx = evt_tx.clone();
                let egui_ctx = egui_ctx.clone();
                tokio::spawn(async move {
                    let mut messages = vec![ChatMessage {
                        role: "system".into(),
                        content: aos_proto::SYSTEM_ASSISTANT_PROMPT.into(),
                    }];
                    messages.extend(history.into_iter().map(|(r, c)| ChatMessage {
                        role: r,
                        content: c,
                    }));
                    let req = InferRequest {
                        model_id: None,
                        messages,
                        params: InferParams {
                            max_tokens: 256,
                            ..Default::default()
                        },
                        priority: 3,
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
                                        let _ = evt_tx.send(Evt::Delta(text));
                                    }
                                    Ok(TokenEvent::Done { .. }) => break,
                                    Ok(TokenEvent::Error { message }) => {
                                        let _ = evt_tx.send(Evt::Error(message));
                                        break;
                                    }
                                    _ => {}
                                }
                                egui_ctx.request_repaint();
                            }
                            let _ = evt_tx.send(Evt::Done(full));
                        }
                        Err(e) => {
                            let _ = evt_tx.send(Evt::Error(e.to_string()));
                        }
                    }
                    egui_ctx.request_repaint();
                });
            }
        }
    }
}

struct UiApp {
    cmd_tx: Sender<Cmd>,
    evt_rx: Receiver<Evt>,
    chat: Vec<(String, String)>,
    streaming: String,
    input: String,
    models: Vec<ModelInfo>,
    metrics: Option<SystemMetrics>,
    agents: Vec<AgentInfo>,
}

impl UiApp {
    fn new(cmd_tx: Sender<Cmd>, evt_rx: Receiver<Evt>) -> Self {
        Self {
            cmd_tx,
            evt_rx,
            chat: vec![(
                "système".into(),
                "Prototype egui — chat + dashboard + agents (bus IPC).".into(),
            )],
            streaming: String::new(),
            input: String::new(),
            models: Vec::new(),
            metrics: None,
            agents: Vec::new(),
        }
    }
}

impl eframe::App for UiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Draine les événements du runtime.
        while let Ok(ev) = self.evt_rx.try_recv() {
            match ev {
                Evt::Delta(t) => self.streaming.push_str(&t),
                Evt::Done(full) => {
                    if !full.is_empty() {
                        self.chat.push(("assistant".into(), full));
                    }
                    self.streaming.clear();
                }
                Evt::Error(m) => {
                    self.chat.push(("système".into(), m));
                    self.streaming.clear();
                }
                Evt::Models(m) => self.models = m,
                Evt::Metrics(m) => self.metrics = Some(m),
                Evt::Agents(a) => self.agents = a,
            }
        }

        egui::SidePanel::right("dash")
            .min_width(320.0)
            .show(ctx, |ui| {
                ui.heading("Ressources");
                if let Some(m) = &self.metrics {
                    let ratio = m.ram_used as f32 / m.ram_total.max(1) as f32;
                    ui.add(egui::ProgressBar::new(ratio).text(format!(
                        "RAM {:.1}/{:.1} GiB — CPU {:.0}%",
                        m.ram_used as f64 / (1 << 30) as f64,
                        m.ram_total as f64 / (1 << 30) as f64,
                        m.cpu_percent
                    )));
                    for mm in &m.models {
                        ui.label(format!(
                            "{} [{:?}] {:.2}/{:.2}/{:.2} GiB (V/R/D){}",
                            mm.model_id,
                            mm.state,
                            mm.vram_bytes as f64 / (1 << 30) as f64,
                            mm.ram_bytes as f64 / (1 << 30) as f64,
                            mm.disk_bytes as f64 / (1 << 30) as f64,
                            mm.last_tok_s
                                .map(|t| format!(" — {t:.1} tok/s"))
                                .unwrap_or_default()
                        ));
                    }
                }
                ui.separator();
                ui.heading("Agents");
                for a in &self.agents {
                    ui.label(format!(
                        "{} [{:?}] pid={}",
                        a.agent_id,
                        a.state,
                        a.pid.map(|p| p.to_string()).unwrap_or("-".into())
                    ));
                }
                if self.agents.is_empty() {
                    ui.label("aucun agent");
                }
            });

        egui::TopBottomPanel::bottom("input").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.input)
                        .desired_width(f32::INFINITY)
                        .hint_text("message…"),
                );
                let send = ui.button("Envoyer").clicked()
                    || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if send && !self.input.trim().is_empty() {
                    let text = self.input.trim().to_string();
                    self.input.clear();
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
                    let _ = self.cmd_tx.send(Cmd::Chat(history));
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Conversation");
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
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
                    }
                });
        });
    }
}
