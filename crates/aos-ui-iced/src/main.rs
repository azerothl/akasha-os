//! Prototype GUI **iced** (ADR 0003) : chat streamé + dashboard + agents.
//!
//! Pont : runtime tokio en arrière-plan + canaux std, drainés par un tick
//! iced périodique (100 ms).

use aos_ipc::BusClient;
use aos_proto::{
    AgentInfo, ChatMessage, InferParams, InferRequest, ModelInfo, SystemMetrics, TokenEvent,
};
use iced::widget::{column, progress_bar, row, scrollable, text, text_input};
use iced::{Element, Length, Subscription, Task, Theme};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Commandes GUI → runtime.
enum Cmd {
    Chat(Vec<(String, String)>),
}

/// Événements runtime → GUI.
#[derive(Debug)]
enum Evt {
    Delta(String),
    Done(String),
    Error(String),
    Models(Vec<ModelInfo>),
    Metrics(SystemMetrics),
    Agents(Vec<AgentInfo>),
}

fn main() -> iced::Result {
    iced::application(App::default, update, view)
        .title(title)
        .theme(theme)
        .subscription(subscription)
        .run()
}

fn title(_app: &App) -> String {
    String::from("Akasha OS — prototype iced (ADR 0003)")
}

fn theme(_app: &App) -> Theme {
    Theme::Dark
}

struct App {
    to_runtime: Sender<Cmd>,
    from_runtime: Arc<Mutex<Receiver<Evt>>>,
    chat: Vec<(String, String)>,
    streaming: String,
    input: String,
    models: Vec<ModelInfo>,
    metrics: Option<SystemMetrics>,
    agents: Vec<AgentInfo>,
}

impl Default for App {
    fn default() -> Self {
        let (cmd_tx, cmd_rx) = channel::<Cmd>();
        let (evt_tx, evt_rx) = channel::<Evt>();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("tokio");
            rt.block_on(runtime_main(cmd_rx, evt_tx));
        });
        Self {
            to_runtime: cmd_tx,
            from_runtime: Arc::new(Mutex::new(evt_rx)),
            chat: vec![(
                "système".into(),
                "Prototype iced — chat + dashboard + agents (bus IPC).".into(),
            )],
            streaming: String::new(),
            input: String::new(),
            models: Vec::new(),
            metrics: None,
            agents: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    InputChanged(String),
    Send,
}

async fn runtime_main(cmd_rx: Receiver<Cmd>, evt_tx: Sender<Evt>) {
    let bus = BusClient::connect("127.0.0.1:24701", "ui-iced")
        .await
        .expect("bus requis (run-demo.ps1)");
    {
        let bus = bus.clone();
        let evt_tx = evt_tx.clone();
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
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }
    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            Cmd::Chat(history) => {
                let bus = bus.clone();
                let evt_tx = evt_tx.clone();
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
                            }
                            let _ = evt_tx.send(Evt::Done(full));
                        }
                        Err(e) => {
                            let _ = evt_tx.send(Evt::Error(e.to_string()));
                        }
                    }
                });
            }
        }
    }
}

fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Tick => {
            if let Ok(rx) = app.from_runtime.lock() {
                while let Ok(ev) = rx.try_recv() {
                    match ev {
                        Evt::Delta(t) => app.streaming.push_str(&t),
                        Evt::Done(full) => {
                            if !full.is_empty() {
                                app.chat.push(("assistant".into(), full));
                            }
                            app.streaming.clear();
                        }
                        Evt::Error(m) => {
                            app.chat.push(("système".into(), m));
                            app.streaming.clear();
                        }
                        Evt::Models(m) => app.models = m,
                        Evt::Metrics(m) => app.metrics = Some(m),
                        Evt::Agents(a) => app.agents = a,
                    }
                }
            }
            Task::none()
        }
        Message::InputChanged(v) => {
            app.input = v;
            Task::none()
        }
        Message::Send => {
            let text = app.input.trim().to_string();
            if !text.is_empty() {
                app.input.clear();
                app.chat.push(("vous".into(), text));
                let history: Vec<(String, String)> = app
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
                app.streaming.clear();
                let _ = app.to_runtime.send(Cmd::Chat(history));
            }
            Task::none()
        }
    }
}

fn subscription(_app: &App) -> Subscription<Message> {
    iced::time::every(Duration::from_millis(100)).map(|_| Message::Tick)
}

fn view(app: &App) -> Element<'_, Message> {
    // Panneau ressources.
    let mut dash = column![text("Ressources").size(18)].spacing(6);
    if let Some(m) = &app.metrics {
        let ratio = m.ram_used as f32 / m.ram_total.max(1) as f32;
        dash = dash.push(progress_bar(0.0..=1.0, ratio)).push(text(format!(
            "RAM {:.1}/{:.1} GiB — CPU {:.0}%",
            m.ram_used as f64 / (1 << 30) as f64,
            m.ram_total as f64 / (1 << 30) as f64,
            m.cpu_percent
        )));
        for mm in &m.models {
            dash = dash.push(text(format!(
                "{} [{:?}]{}{}",
                mm.model_id,
                mm.state,
                if mm.vram_bytes + mm.ram_bytes > 0 {
                    format!(
                        " {:.1} GiB V / {:.1} GiB R",
                        mm.vram_bytes as f64 / (1 << 30) as f64,
                        mm.ram_bytes as f64 / (1 << 30) as f64
                    )
                } else {
                    String::new()
                },
                mm.last_tok_s
                    .map(|t| format!(" — {t:.1} tok/s"))
                    .unwrap_or_default()
            )));
        }
    }
    dash = dash.push(text("Agents").size(18));
    if app.agents.is_empty() {
        dash = dash.push(text("aucun agent"));
    }
    for a in &app.agents {
        dash = dash.push(text(format!("{} [{:?}]", a.agent_id, a.state)));
    }

    // Conversation.
    let mut chat_col = column![].spacing(6);
    for (role, content) in &app.chat {
        chat_col = chat_col.push(text(format!("[{role}]"))).push(text(content));
    }
    if !app.streaming.is_empty() {
        chat_col = chat_col
            .push(text("[assistant]"))
            .push(text(&app.streaming));
    }

    let input = row![
        text_input("message…", &app.input)
            .on_input(Message::InputChanged)
            .on_submit(Message::Send)
            .width(Length::Fill),
        iced::widget::button("Envoyer").on_press(Message::Send),
    ]
    .spacing(8);

    let content = row![
        column![text("Conversation").size(20), scrollable(chat_col), input]
            .spacing(8)
            .width(Length::FillPortion(3)),
        dash.width(Length::FillPortion(1)).spacing(6),
    ]
    .spacing(16)
    .padding(16);

    content.into()
}
