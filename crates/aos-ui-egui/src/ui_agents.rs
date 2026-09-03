//! Agent roster, creation, and detail panels.

use crate::cmd::Cmd;
use crate::{
    agent_canvas_session_ops, agent_cap_holder, agent_panel, agent_shown_in_tab, chat_room, guide,
    i18n, icons, open_in_browser, overflow_scroll, overflow_scroll_h, ui_roster_tool_checkboxes,
    ChatLine, UiApp,
};
use aos_proto::{AgentInfo, AgentState, ChatAttachment};
use eframe::egui;

impl UiApp {
    pub(crate) fn ui_agents(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        let g = guide::strings(&self.prefs.language);
        ui.horizontal(|ui| {
            ui.heading(t.tab_agents);
            if guide::tab_help_button(ui, g.help_tooltip) {
                self.guide.open_topic(guide::GuideTopic::Agents);
            }
        });
        ui.weak(t.agents_blurb);
        ui.separator();
        if ui.button(t.agents_refresh_catalogs).clicked() {
            let _ = self.cmd_tx.send(Cmd::AgentCatalogRefresh);
        }
        ui.label(t.agents_label);
        ui.text_edit_singleline(&mut self.agent_ui.display_name);
        ui.label(t.agents_role);
        ui.weak(t.agents_role_optional);
        ui.add(
            egui::TextEdit::multiline(&mut self.agent_ui.task)
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        ui.collapsing(t.agents_advanced, |ui| {
            ui.label(t.agents_model);
            egui::ComboBox::from_id_salt("agent_model")
                .selected_text(if self.agent_ui.model_id.is_empty() {
                    "default".to_string()
                } else {
                    self.agent_ui.model_id.clone()
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.agent_ui.model_id, String::new(), "default");
                    for m in &self.models_ui.model_infos {
                        ui.selectable_value(
                            &mut self.agent_ui.model_id,
                            m.id.clone(),
                            format!("{} [{:?}]", m.id, m.state),
                        );
                    }
                });
            ui.label(t.agents_system_prompt);
            ui.add(
                egui::TextEdit::multiline(&mut self.agent_ui.system_prompt)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            ui.collapsing("Skills", |ui| {
                if self.agent_ui.skill_catalog.is_empty() {
                    ui.weak(t.agents_catalog_empty);
                    for name in ["notes-writer", "research", "file-author", "planner"] {
                        let mut on = self.agent_ui.skill_selected.iter().any(|s| s == name);
                        if ui.checkbox(&mut on, name).changed() {
                            if on {
                                self.agent_ui.skill_selected.push(name.into());
                            } else {
                                self.agent_ui.skill_selected.retain(|s| s != name);
                            }
                        }
                    }
                } else {
                    for s in self.agent_ui.skill_catalog.clone() {
                        let mut on = self.agent_ui.skill_selected.contains(&s.name);
                        if ui
                            .checkbox(&mut on, format!("{} — {}", s.name, s.description))
                            .changed()
                        {
                            if on {
                                self.agent_ui.skill_selected.push(s.name.clone());
                                for t in &s.tools {
                                    if !self.agent_ui.tool_selected.contains(t) {
                                        self.agent_ui.tool_selected.push(t.clone());
                                    }
                                }
                            } else {
                                self.agent_ui.skill_selected.retain(|x| x != &s.name);
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
                    let mut on = self.agent_ui.tool_selected.iter().any(|t| t == name);
                    if ui.checkbox(&mut on, name).changed() {
                        if on {
                            self.agent_ui.tool_selected.push(name.into());
                        } else {
                            self.agent_ui.tool_selected.retain(|t| t != name);
                        }
                    }
                }
            });

            ui.collapsing(t.agents_mcp, |ui| {
                if self.agent_ui.mcp_catalog.is_empty() {
                    ui.weak(t.agents_mcp_empty);
                }
                for s in self.agent_ui.mcp_catalog.clone() {
                    let mut on = self.agent_ui.mcp_selected.contains(&s.name);
                    if ui
                        .checkbox(&mut on, format!("{} ({})", s.name, s.command))
                        .changed()
                    {
                        if on {
                            self.agent_ui.mcp_selected.push(s.name.clone());
                        } else {
                            self.agent_ui.mcp_selected.retain(|x| x != &s.name);
                        }
                    }
                }
            });

            ui.label(t.agents_docs);
            ui.text_edit_singleline(&mut self.agent_ui.docs);
        });

        let room_active = chat_room::session_is_room(chat_room::active_session_meta(
            &self.chat_state.sessions,
            self.chat_state.active_session.as_deref(),
        ));
        if room_active {
            ui.checkbox(
                &mut self.agent_ui.join_room_on_create,
                t.agents_join_room_on_create,
            );
        }

        ui.horizontal(|ui| {
            let can_create = !self.agent_ui.display_name.trim().is_empty();
            if ui
                .add_enabled(can_create, egui::Button::new(t.agents_create))
                .clicked()
            {
                self.send_agents_page_create(room_active, true);
            }
            if ui.button(t.agents_create_task).clicked() {
                if !can_create {
                    self.status = t.agents_label_required.into();
                } else if self.agent_ui.task.trim().is_empty() {
                    self.status = t.agents_task_goal_required.into();
                } else {
                    self.send_agents_page_create(room_active, false);
                }
            }
        });

        ui.separator();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.agent_ui.show_history, false, t.agents_tab_active);
            ui.selectable_value(&mut self.agent_ui.show_history, true, t.agents_tab_history);
        });
        let history = self.agent_ui.show_history;
        let library_agents = chat_room::agents_with_library_placeholders(&self.agents, &t);
        let visible: Vec<AgentInfo> = library_agents
            .iter()
            .filter(|a| agent_shown_in_tab(a, history))
            .cloned()
            .collect();
        let list_h = ui.available_height().max(120.0);
        overflow_scroll_h(ui, "agents_list", list_h, |ui| {
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
                    a.parent_id
                        .as_ref()
                        .is_some_and(|p| !visible.iter().any(|x| x.agent_id == *p))
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
            ui.text_edit_singleline(&mut self.agent_ui.steer_id);
            ui.text_edit_singleline(&mut self.agent_ui.steer_txt);
            if ui.button(t.agent_send).clicked()
                && !self.agent_ui.steer_id.is_empty()
                && !self.agent_ui.steer_txt.is_empty()
            {
                let _ = self.cmd_tx.send(Cmd::AgentSteer {
                    id: self.agent_ui.steer_id.clone(),
                    text: self.agent_ui.steer_txt.clone(),
                });
            }
        });
    }

    pub(crate) fn draw_agent_row(
        &mut self,
        ui: &mut egui::Ui,
        a: &AgentInfo,
        indent: usize,
        t: i18n::UiStrings,
    ) {
        ui.horizontal(|ui| {
            if indent > 0 {
                ui.add_space(16.0 * indent as f32);
                icons::child_branch(ui);
            }
            let selected = self.agent_ui.active_tab.as_deref() == Some(a.agent_id.as_str());
            let label = chat_room::roster_agent_label(&t, a);
            let label = agent_panel::truncate(&label, 48);
            if ui
                .selectable_label(selected, &label)
                .on_hover_text(&a.agent_id)
                .clicked()
            {
                self.open_agent_tab(&a.agent_id);
            }
            ui.weak(&a.agent_id);
            ui.colored_label(
                agent_panel::state_color(&a.state),
                if a.is_roster() {
                    "Roster".to_string()
                } else {
                    format!("{:?}", a.state)
                },
            );
            if !a.is_roster() {
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
            }
            if let Some(task) = &a.current_task {
                ui.small(task);
            }
            if !a.children.is_empty() && indent == 0 {
                ui.small(
                    t.agents_subagents
                        .replace("{n}", &a.children.len().to_string()),
                );
            }
            if let Some(reason) = &a.fail_reason {
                let session_ops = agent_canvas_session_ops(
                    a,
                    self.chat_state.active_session.as_deref(),
                    &self.chat_state.view.canvas.ops,
                );
                let trace = self.agent_ui.traces.get(&a.agent_id);
                let visible = agent_panel::resolve_visible_fail_reason(
                    &t,
                    Some(a),
                    reason.as_str(),
                    session_ops,
                    trace,
                );
                if !visible.is_empty() {
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 120, 100),
                        agent_panel::truncate(&visible, 40),
                    );
                }
            }
            if !a.is_roster() {
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
            }
        });
    }

    pub(crate) fn ui_roster_detail_edits(
        &mut self,
        ui: &mut egui::Ui,
        agent_id: &str,
        t: i18n::UiStrings,
    ) {
        let Some(draft) = self.agent_ui.roster_edit_drafts.get_mut(agent_id) else {
            ui.weak("…");
            return;
        };
        ui.separator();
        ui.collapsing(t.agents_tools, |ui| {
            ui_roster_tool_checkboxes(ui, &t, &mut draft.tools);
        });
        ui.collapsing(t.agents_skills, |ui| {
            if self.agent_ui.skill_catalog.is_empty() {
                ui.weak(t.agents_catalog_empty);
                for name in ["notes-writer", "research", "file-author", "planner"] {
                    let mut on = draft.skills.iter().any(|s| s == name);
                    if ui.checkbox(&mut on, name).changed() {
                        if on {
                            draft.skills.push(name.into());
                        } else {
                            draft.skills.retain(|s| s != name);
                        }
                    }
                }
            } else {
                for s in self.agent_ui.skill_catalog.clone() {
                    let mut on = draft.skills.contains(&s.name);
                    if ui
                        .checkbox(&mut on, format!("{} — {}", s.name, s.description))
                        .changed()
                    {
                        if on {
                            draft.skills.push(s.name.clone());
                            for tool in &s.tools {
                                if !draft.tools.contains(tool) {
                                    draft.tools.push(tool.clone());
                                }
                            }
                        } else {
                            draft.skills.retain(|x| x != &s.name);
                        }
                    }
                }
            }
        });
        ui.collapsing(t.agents_mcp, |ui| {
            if self.agent_ui.mcp_catalog.is_empty() {
                ui.weak(t.agents_mcp_empty);
            }
            for s in self.agent_ui.mcp_catalog.clone() {
                let mut on = draft.mcp_servers.contains(&s.name);
                if ui
                    .checkbox(&mut on, &s.name)
                    .on_hover_text(&s.command)
                    .changed()
                {
                    if on {
                        draft.mcp_servers.push(s.name.clone());
                    } else {
                        draft.mcp_servers.retain(|x| x != &s.name);
                    }
                }
            }
        });
        if ui.button(t.agents_edit_save).clicked() {
            let draft = draft.clone();
            let _ = self.cmd_tx.send(Cmd::AgentRosterUpdate {
                agent_id: agent_id.to_string(),
                display_name: draft.display_name,
                role: draft.role,
                system_prompt: if draft.system_prompt.is_empty() {
                    None
                } else {
                    Some(draft.system_prompt)
                },
                skills: draft.skills,
                tools: draft.tools,
                mcp_servers: draft.mcp_servers,
                model_id: if draft.model_id.is_empty() {
                    None
                } else {
                    Some(draft.model_id)
                },
            });
        }
    }

    pub(crate) fn ui_agent_detail_panel(&mut self, ctx: &egui::Context) {
        if self.agent_ui.open_tabs.is_empty() && !self.prefs.ui_layout.activity_panel_open {
            return;
        }
        egui::SidePanel::right("agent_detail_tabs")
            .default_width(self.prefs.ui_layout.context_panel_width.max(280.0))
            .min_width(420.0)
            .resizable(true)
            .show(ctx, |ui| {
                let t = i18n::strings(&self.prefs.language);
                ui.horizontal(|ui| {
                    ui.heading(t.agent_detail);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("×").on_hover_text("Fermer Activité").clicked() {
                            self.prefs.ui_layout.activity_panel_open = false;
                            self.agent_ui.close_all_tabs();
                            crate::prefs::save_preferences(&self.prefs);
                        }
                        if ui.small_button(t.agent_close_all).clicked() {
                            self.agent_ui.close_all_tabs();
                        }
                    });
                });
                ui.horizontal_wrapped(|ui| {
                    let tabs = self.agent_ui.open_tabs.clone();
                    for id in tabs {
                        let selected = self.agent_ui.active_tab.as_deref() == Some(id.as_str());
                        let label = if let Some(a) = self.agents.iter().find(|x| x.agent_id == id) {
                            format!(
                                "{} [{:?}]",
                                agent_panel::truncate(a.display_title(), 28),
                                a.state
                            )
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
                                    if ui.selectable_label(selected, label).clicked() {
                                        self.agent_ui.select_tab(&id);
                                        let holder = agent_cap_holder(&id);
                                        self.security_ui.select_holder(holder.clone());
                                        let _ = self.cmd_tx.send(Cmd::CapList { holder });
                                        if self
                                            .agents
                                            .iter()
                                            .find(|a| a.agent_id == id)
                                            .is_some_and(|a| a.is_roster())
                                        {
                                            let _ = self
                                                .cmd_tx
                                                .send(Cmd::AgentSpecGet { id: id.clone() });
                                        }
                                    }
                                    if icons::close_button(ui).clicked() {
                                        self.close_agent_tab(&id);
                                    }
                                });
                            });
                    }
                });
                ui.separator();

                if self.agent_ui.open_tabs.is_empty() {
                    let active = self.chat_state.active_session.as_deref();
                    let mut shown = 0usize;
                    let activity_agents: Vec<_> = self
                        .agents
                        .iter()
                        .filter(|a| a.session_id.as_deref() == active)
                        .cloned()
                        .collect();
                    for agent in activity_agents {
                        shown += 1;
                        let icon = match agent.state {
                            AgentState::Done => "✓",
                            AgentState::Failed => "!",
                            AgentState::Blocked => "?",
                            AgentState::Running => "…",
                            _ => "·",
                        };
                        ui.horizontal(|ui| {
                            ui.label(format!("{icon} {}", agent.display_title()));
                            ui.weak(format!("{:?}", agent.state));
                            if ui.small_button("Détails").clicked() {
                                self.agent_ui.open_tab(&agent.agent_id);
                                self.agent_ui.select_tab(&agent.agent_id);
                            }
                        });
                    }
                    if shown == 0 {
                        ui.weak("Aucune activité d’agent dans cette conversation.");
                    }
                }

                overflow_scroll(ui, "agent_detail_body", |ui| {
                    let active = self.agent_ui.active_tab.clone();
                    if let Some(id) = active {
                        let holder = agent_cap_holder(&id);
                        let t = i18n::strings(&self.prefs.language);
                        ui.collapsing(format!("{} ({holder})", t.caps_heading), |ui| {
                            ui.horizontal(|ui| {
                                if ui.small_button(t.caps_refresh).clicked() {
                                    self.security_ui.select_holder(holder.clone());
                                    let _ = self.cmd_tx.send(Cmd::CapList {
                                        holder: holder.clone(),
                                    });
                                }
                            });
                            self.draw_caps_list(ui, &holder);
                        });
                        ui.separator();

                        let info = self.agents.iter().find(|a| a.agent_id == id).cloned();
                        let trace = self.agent_ui.traces.get(&id).cloned();
                        if info.as_ref().is_some_and(|a| a.is_roster()) {
                            self.ui_roster_detail_edits(ui, &id, t);
                        }
                        let session_ops = info.as_ref().and_then(|a| {
                            agent_canvas_session_ops(
                                a,
                                self.chat_state.active_session.as_deref(),
                                &self.chat_state.view.canvas.ops,
                            )
                        });
                        let actions = agent_panel::draw_agent_detail(
                            ui,
                            info.as_ref(),
                            trace.as_ref(),
                            session_ops,
                            &mut self.agent_ui.steer_txt,
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
                                    .or_else(|| self.chat_state.active_session.clone())
                                {
                                    if self.chat_state.active_session.as_deref()
                                        == Some(sid.as_str())
                                    {
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
                                            speaker_id: None,
                                            speaker_name: None,
                                            thinking: None,
                                        });
                                    }
                                }
                            }
                            let _ = self.cmd_tx.send(Cmd::AgentResume { id: id.clone() });
                        }
                        if actions.retry {
                            let _ = self.cmd_tx.send(Cmd::AgentRetry { id: id.clone() });
                        }
                        if actions.continue_canvas {
                            let _ = self.cmd_tx.send(Cmd::AgentRetry { id: id.clone() });
                        }
                        if actions.export {
                            let _ = self.cmd_tx.send(Cmd::AgentExport { id: id.clone() });
                        }
                        if let Some(text) = actions.steer {
                            let blocked = info
                                .as_ref()
                                .is_some_and(|a| a.state == AgentState::Blocked);
                            if blocked {
                                if let Some(sid) = info
                                    .as_ref()
                                    .and_then(|a| a.session_id.clone())
                                    .or_else(|| self.chat_state.active_session.clone())
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
                            self.agent_ui.steer_txt.clear();
                        }
                        if let Some(child) = actions.open_child {
                            self.open_agent_tab(&child);
                        }
                    } else {
                        ui.weak(t.agents_select_tab);
                    }
                });
            });
    }
}
