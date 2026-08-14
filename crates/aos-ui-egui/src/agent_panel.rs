//! Panneau détail agent : timeline mise en forme, sources, sous-agents.

use aos_proto::{AgentInfo, AgentSource, AgentState, AgentStepRecord, AgentTrace};
use eframe::egui::{self, Color32, RichText, Ui};

pub struct PanelActions {
    pub pause: bool,
    pub kill: bool,
    pub resume: bool,
    pub retry: bool,
    pub steer: Option<String>,
    pub open_child: Option<String>,
}

impl Default for PanelActions {
    fn default() -> Self {
        Self {
            pause: false,
            kill: false,
            resume: false,
            retry: false,
            steer: None,
            open_child: None,
        }
    }
}

pub fn state_color(state: &AgentState) -> Color32 {
    match state {
        AgentState::Failed | AgentState::Killed => Color32::from_rgb(220, 90, 90),
        AgentState::Blocked => Color32::from_rgb(230, 160, 60),
        AgentState::Done => Color32::from_rgb(100, 190, 120),
        AgentState::Paused => Color32::from_rgb(160, 160, 180),
        AgentState::Running => Color32::from_rgb(120, 180, 230),
        AgentState::Created => Color32::GRAY,
    }
}

pub fn fmt_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else if ms < 60_000 {
        format!("{:.1} s", ms as f64 / 1000.0)
    } else {
        format!("{} min {:02} s", ms / 60_000, (ms % 60_000) / 1000)
    }
}

pub fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        format!("{}…", s.chars().take(n).collect::<String>())
    } else {
        s.to_string()
    }
}

/// Retire les blocs `<think>…</think>` (Qwen3/3.5) pour l'affichage.
fn strip_think_tags(text: &str) -> String {
    let mut rest = text.to_string();
    loop {
        let lower = rest.to_ascii_lowercase();
        let Some(start) = lower.find("<think>") else {
            break;
        };
        let after = start + "<think>".len();
        let lower_tail = rest[after..].to_ascii_lowercase();
        if let Some(rel) = lower_tail.find("</think>") {
            let end = after + rel + "</think>".len();
            rest = format!("{}{}", &rest[..start], &rest[end..]);
        } else {
            rest.truncate(start);
            break;
        }
    }
    let lower = rest.to_ascii_lowercase();
    if let Some(idx) = lower.find("</think>") {
        rest = format!("{}{}", &rest[..idx], &rest[idx + "</think>".len()..]);
    }
    rest.trim().to_string()
}

/// Extrait la prose hors blocs JSON / TOOL:.
pub fn prose_without_json(response: &str) -> String {
    let mut out = strip_think_tags(response);
    if let Some(start) = out.find("```json") {
        if let Some(end_rel) = out[start + 7..].find("```") {
            let end = start + 7 + end_rel + 3;
            out = format!("{}{}", &out[..start], &out[end..]);
        }
    } else if let Some(start) = out.find('{') {
        // Retire le premier objet JSON équilibré
        if let Some(end) = find_json_object_end(&out[start..]) {
            out = format!("{}{}", &out[..start], &out[start + end..]);
        }
    }
    out.lines()
        .filter(|l| !l.trim().starts_with("TOOL:"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn find_json_object_end(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    for (i, ch) in s.char_indices() {
        if in_str {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn aggregate_sources(trace: &AgentTrace) -> Vec<AgentSource> {
    let mut out = Vec::new();
    for step in &trace.steps {
        for s in &step.sources {
            if !out.iter().any(|x: &AgentSource| {
                x.locator == s.locator && x.kind == s.kind
            }) {
                out.push(s.clone());
            }
        }
    }
    out
}

fn tool_kind_color(kind: &str) -> Color32 {
    match kind {
        "mcp" => Color32::from_rgb(100, 200, 180),
        "module" => Color32::from_rgb(120, 190, 230),
        "native" => Color32::from_rgb(140, 170, 220),
        "runtime" => Color32::from_rgb(200, 160, 120),
        _ => Color32::LIGHT_GRAY,
    }
}

fn card_frame(fill: Color32) -> egui::Frame {
    egui::Frame::NONE
        .fill(fill)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(8, 6))
}

pub fn draw_agent_detail(
    ui: &mut Ui,
    info: Option<&AgentInfo>,
    trace: Option<&AgentTrace>,
    steer_buf: &mut String,
    open_in_browser: &dyn Fn(&str),
) -> PanelActions {
    let mut actions = PanelActions::default();
    let id = info
        .map(|a| a.agent_id.as_str())
        .or_else(|| trace.map(|t| t.agent_id.as_str()))
        .unwrap_or("?");

    if let Some(a) = info {
        ui.label(RichText::new(&a.directive).strong());
        ui.horizontal(|ui| {
            ui.colored_label(state_color(&a.state), format!("{:?}", a.state));
            ui.separator();
            ui.label(format!("tour {}/{}", a.step, a.max_steps));
            let tokens = trace.map(|t| t.tokens_used).unwrap_or(a.tokens_used);
            ui.separator();
            ui.label(format!("{tokens} tok"));
            if let Some(t) = trace {
                ui.separator();
                ui.label(fmt_ms(t.total_duration_ms));
            }
        });

        let fail = a
            .fail_reason
            .clone()
            .or_else(|| trace.and_then(|t| t.fail_reason.clone()));
        if matches!(a.state, AgentState::Failed | AgentState::Blocked | AgentState::Killed)
            || fail.is_some()
        {
            card_frame(Color32::from_rgb(60, 30, 30)).show(ui, |ui| {
                ui.label(
                    RichText::new(if matches!(a.state, AgentState::Blocked) {
                        "Bloqué"
                    } else {
                        "Échec"
                    })
                    .color(Color32::from_rgb(240, 140, 120))
                    .strong(),
                );
                ui.label(
                    fail.unwrap_or_else(|| "motif non renseigné".into()),
                );
            });
            ui.add_space(4.0);
        }

        ui.horizontal(|ui| {
            if !a.skills.is_empty() {
                ui.small(format!("skills: {}", a.skills.join(", ")));
            }
            if !a.mcp_servers.is_empty() {
                ui.small(format!("MCP: {}", a.mcp_servers.join(", ")));
            }
        });
        if let Some(parent) = &a.parent_id {
            ui.small(format!("parent: {parent}"));
        }

        ui.horizontal(|ui| {
            match a.state {
                AgentState::Running => {
                    if ui.button("Pause").clicked() {
                        actions.pause = true;
                    }
                }
                AgentState::Paused | AgentState::Blocked => {
                    if ui.button("Débloquer").clicked() {
                        actions.resume = true;
                    }
                }
                AgentState::Failed | AgentState::Killed => {
                    if ui.button("Relancer l'étape").clicked() {
                        actions.retry = true;
                    }
                }
                AgentState::Done => {
                    if ui.button("Relancer").clicked() {
                        actions.retry = true;
                    }
                }
                AgentState::Created => {}
            }
            if !matches!(a.state, AgentState::Killed | AgentState::Done) {
                if ui.button("Kill").clicked() {
                    actions.kill = true;
                }
            }
            ui.label("Steer");
            ui.add(
                egui::TextEdit::singleline(steer_buf)
                    .desired_width(160.0)
                    .hint_text("directive…"),
            );
            if ui.button("Envoyer").clicked() && !steer_buf.is_empty() {
                actions.steer = Some(steer_buf.clone());
            }
        });

        if !a.children.is_empty() {
            ui.separator();
            ui.label(RichText::new("Sous-agents").strong());
            ui.horizontal_wrapped(|ui| {
                for child in &a.children {
                    let child_info_label = child.clone();
                    if ui
                        .add(
                            egui::Button::new(RichText::new(format!("↗ {child_info_label}"))
                                .color(Color32::from_rgb(160, 200, 255)))
                            .frame(true),
                        )
                        .clicked()
                    {
                        actions.open_child = Some(child.clone());
                    }
                }
            });
        }
    } else {
        ui.weak(format!("Agent {id} — infos partielles (trace disque)"));
    }

    ui.separator();

    if let Some(t) = trace {
        let sources = aggregate_sources(t);
        if !sources.is_empty() {
            ui.collapsing(format!("Sources ({})", sources.len()), |ui| {
                for s in &sources {
                    ui.horizontal(|ui| {
                        ui.colored_label(
                            match s.kind.as_str() {
                                "web" => Color32::from_rgb(100, 180, 255),
                                "document" => Color32::from_rgb(180, 200, 120),
                                "fetch" => Color32::from_rgb(200, 160, 100),
                                _ => Color32::GRAY,
                            },
                            format!("[{}]", s.kind),
                        );
                        let label = if s.title.is_empty() {
                            s.locator.as_str()
                        } else {
                            s.title.as_str()
                        };
                        if s.kind == "web" || s.locator.starts_with("http") {
                            if ui.link(truncate(label, 60)).clicked() {
                                open_in_browser(&s.locator);
                            }
                        } else {
                            ui.label(truncate(label, 80));
                        }
                    });
                    if !s.snippet.is_empty() {
                        ui.small(truncate(&s.snippet, 160));
                    }
                }
            });
            ui.add_space(4.0);
        }
    }

    egui::ScrollArea::vertical()
        .id_salt(format!("agent_timeline_{id}"))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if let Some(t) = trace {
                if t.steps.is_empty() {
                    ui.weak("Aucun tour enregistré pour l'instant.");
                    if !t.working_memory.is_empty() {
                        for (role, content) in &t.working_memory {
                            if role == "system" && content.len() > 300 {
                                ui.collapsing(format!("{role}"), |ui| {
                                    ui.small(content);
                                });
                            } else {
                                ui.label(
                                    RichText::new(format!(
                                        "{role}: {}",
                                        truncate(content, 300)
                                    ))
                                    .small(),
                                );
                            }
                        }
                    }
                } else {
                    let last = t.steps.len().saturating_sub(1);
                    for (i, rec) in t.steps.iter().enumerate() {
                        draw_step(ui, id, rec, i == last, &mut actions);
                    }
                }
                if let Some(a) = info {
                    if matches!(a.state, AgentState::Running) && !a.last_output.is_empty() {
                        ui.separator();
                        ui.collapsing("Tour en cours (flux)", |ui| {
                            ui.label(RichText::new(truncate(&a.last_output, 800)).small());
                        });
                    }
                }
            } else {
                ui.weak("Chargement du journal…");
            }
        });

    actions
}

fn draw_step(
    ui: &mut Ui,
    agent_id: &str,
    rec: &AgentStepRecord,
    default_open: bool,
    actions: &mut PanelActions,
) {
    let header = format!(
        "Tour {} · {} · {} · {} tok",
        rec.step,
        fmt_ms(rec.duration_ms),
        rec.action,
        rec.prompt_tokens + rec.generated_tokens,
    );
    egui::CollapsingHeader::new(header)
        .id_salt(format!("step-{agent_id}-{}", rec.step))
        .default_open(default_open)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.small(format!("inférence {}", fmt_ms(rec.infer_ms)));
                ui.small(format!("outil {}", fmt_ms(rec.tool_ms)));
                if rec.tok_s > 0.0 {
                    ui.small(format!("{:.1} tok/s", rec.tok_s));
                }
                if let Some(task) = &rec.current_task {
                    ui.small(format!("tâche: {task}"));
                }
            });

            if let Some(reason) = &rec.fail_reason {
                card_frame(Color32::from_rgb(55, 28, 28)).show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("Échec / blocage : {reason}"))
                            .color(Color32::from_rgb(240, 140, 120)),
                    );
                });
            }

            if !rec.thought.is_empty() {
                card_frame(Color32::from_rgb(40, 36, 55)).show(ui, |ui| {
                    ui.label(
                        RichText::new("Réflexion")
                            .color(Color32::from_rgb(180, 170, 220))
                            .small()
                            .strong(),
                    );
                    ui.label(RichText::new(&rec.thought).italics());
                });
            }

            let prose = prose_without_json(&rec.response);
            if !prose.is_empty() {
                card_frame(Color32::from_rgb(32, 40, 36)).show(ui, |ui| {
                    ui.label(
                        RichText::new("Réponse")
                            .color(Color32::from_rgb(160, 210, 170))
                            .small()
                            .strong(),
                    );
                    ui.label(&prose);
                });
            }
            if !rec.response.is_empty()
                && (rec.response.contains('{') || rec.response.contains("```"))
            {
                ui.collapsing("JSON brut", |ui| {
                    ui.monospace(truncate(&rec.response, 3000));
                });
            }

            if rec.action == "agent.spawn" {
                let child = rec
                    .child_id
                    .clone()
                    .or_else(|| {
                        rec.tool_result
                            .strip_prefix("sous-agent créé: ")
                            .map(|s| s.trim().to_string())
                    })
                    .unwrap_or_else(|| "?".into());
                let brief = rec
                    .args
                    .get("brief")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                card_frame(Color32::from_rgb(30, 45, 60)).show(ui, |ui| {
                    ui.label(
                        RichText::new("Sous-agent spawn")
                            .color(Color32::from_rgb(140, 190, 255))
                            .strong(),
                    );
                    if !brief.is_empty() {
                        ui.small(&brief);
                    }
                    if child != "?"
                        && ui
                            .button(RichText::new(format!("Ouvrir {child}"))
                                .color(Color32::from_rgb(160, 200, 255)))
                            .clicked()
                    {
                        actions.open_child = Some(child);
                    }
                });
            } else if rec.action == "agent.await" {
                card_frame(Color32::from_rgb(50, 42, 30)).show(ui, |ui| {
                    ui.label(
                        RichText::new("Attente sous-agent")
                            .color(Color32::from_rgb(230, 180, 100))
                            .strong(),
                    );
                    if let Some(c) = &rec.child_id {
                        if ui.link(format!("attendre {c}")).clicked() {
                            actions.open_child = Some(c.clone());
                        }
                    }
                    if !rec.tool_result.is_empty() {
                        ui.small(truncate(&rec.tool_result, 200));
                    }
                });
            } else if rec.action != "noop" {
                card_frame(Color32::from_rgb(28, 36, 48)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(
                            tool_kind_color(&rec.tool_kind),
                            RichText::new(format!("{} · {}", rec.action, rec.tool_kind)).strong(),
                        );
                        if let Some(s) = &rec.skill {
                            ui.label(format!("skill: {s}"));
                        }
                        if let Some(m) = &rec.mcp_server {
                            ui.label(format!("MCP: {m}"));
                        }
                    });
                    if !rec.tool_result.is_empty() {
                        ui.label(RichText::new(truncate(&rec.tool_result, 400)).small());
                    }
                    let args = serde_json::to_string_pretty(&rec.args)
                        .unwrap_or_else(|_| rec.args.to_string());
                    if args != "null" && args != "{}" && args != "[]" {
                        ui.collapsing("Arguments", |ui| {
                            ui.monospace(&args);
                        });
                    }
                });
            }

            if !rec.sources.is_empty() {
                ui.small(format!(
                    "sources: {}",
                    rec.sources
                        .iter()
                        .map(|s| {
                            if s.title.is_empty() {
                                s.locator.as_str()
                            } else {
                                s.title.as_str()
                            }
                        })
                        .take(4)
                        .collect::<Vec<_>>()
                        .join(" · ")
                ));
            }

            if let Some(r) = &rec.reflection {
                let r = strip_think_tags(r);
                if !r.is_empty() {
                    card_frame(Color32::from_rgb(48, 40, 28)).show(ui, |ui| {
                        ui.label(
                            RichText::new("Critique / reflect")
                                .color(Color32::from_rgb(220, 180, 120))
                                .small()
                                .strong(),
                        );
                        ui.label(RichText::new(r).italics());
                    });
                }
            }
        });
}
