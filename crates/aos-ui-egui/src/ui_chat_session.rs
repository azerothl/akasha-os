//! Conversation session bar, room members, and canvas session actions.

use crate::cmd::Cmd;
use crate::{
    chat_canvas, chat_room, guide, i18n, icons, session_toggle_chip, session_toggle_reserve_width,
    UiApp, CANVAS_TOOLBAR_ROW_H,
};
use aos_proto::{AgentInfo, ChatRoomMember, ChatSessionMode};
use eframe::egui;

impl UiApp {
    pub(crate) fn ui_room_member_chip(
        &mut self,
        ui: &mut egui::Ui,
        t: &i18n::UiStrings,
        session_id: &str,
        mem: &ChatRoomMember,
    ) {
        let name = chat_room::member_display_label(t, mem);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.strong(&name);
            if icons::close_button(ui)
                .on_hover_text(t.room_member_remove)
                .clicked()
            {
                let _ = self.cmd_tx.send(Cmd::SessionMembersRemove {
                    session_id: session_id.to_string(),
                    agent_id: mem.agent_id.clone(),
                });
            }
        });
    }

    pub(crate) fn ui_room_add_library_chips(
        &mut self,
        ui: &mut egui::Ui,
        t: &i18n::UiStrings,
        session_id: &str,
        model_id: Option<String>,
        candidates: &[AgentInfo],
    ) {
        if candidates.is_empty() {
            return;
        }
        ui.weak(t.room_add_from_library);
        ui.horizontal_wrapped(|ui| {
            for agent in candidates {
                let label = chat_room::roster_agent_label(t, agent);
                if ui.small_button(&label).clicked() {
                    if let Some(persona_id) = agent.persona_id.clone() {
                        let _ = self.cmd_tx.send(Cmd::RoomAddPersona {
                            session_id: session_id.to_string(),
                            persona_id,
                            model_id: model_id.clone(),
                        });
                    } else {
                        let stored_name = agent
                            .display_name
                            .clone()
                            .filter(|n| !n.trim().is_empty())
                            .unwrap_or(label.clone());
                        let _ = self.cmd_tx.send(Cmd::SessionMembersAdd {
                            session_id: session_id.to_string(),
                            member: ChatRoomMember {
                                agent_id: agent.agent_id.clone(),
                                display_name: stored_name,
                                persona_id: None,
                                joined_ms: chat_room::joined_ms_now(),
                            },
                        });
                    }
                }
            }
        });
    }

    pub(crate) fn dispatch_canvas_ui_action(
        &mut self,
        action: Option<chat_canvas::CanvasUiAction>,
        session_id: &str,
    ) {
        match action {
            Some(chat_canvas::CanvasUiAction::Apply(op)) => {
                match &op {
                    aos_proto::CanvasOpBody::Clear => self.chat_state.view.canvas.ops.clear(),
                    aos_proto::CanvasOpBody::Undo => {
                        if let Some(pos) = self
                            .chat_state
                            .view
                            .canvas
                            .ops
                            .iter()
                            .rposition(|o| o.author_id == "human")
                        {
                            self.chat_state.view.canvas.ops.remove(pos);
                        }
                    }
                    // Apply optimistically so the stroke/shape is visible immediately without
                    // waiting for the server roundtrip snapshot.
                    _ => {
                        self.chat_state.view.canvas.ops.push(aos_proto::CanvasOp {
                            seq: 0,
                            author_id: "human".into(),
                            ts_ms: 0,
                            body: op.clone(),
                        });
                    }
                }
                let _ = self.cmd_tx.send(Cmd::CanvasApply {
                    session_id: session_id.to_string(),
                    author_id: "human".into(),
                    op,
                });
            }
            Some(chat_canvas::CanvasUiAction::SetStyle { color, width }) => {
                let _ = self.cmd_tx.send(Cmd::CanvasSetStyle {
                    session_id: session_id.to_string(),
                    color,
                    width,
                });
            }
            Some(chat_canvas::CanvasUiAction::Export) => {
                let aspect = self
                    .chat_state
                    .sessions
                    .iter()
                    .find(|s| s.id == session_id)
                    .map(|s| s.canvas_aspect)
                    .unwrap_or_default();
                let _ = self.cmd_tx.send(Cmd::CanvasExport {
                    session_id: session_id.to_string(),
                    aspect,
                });
            }
            Some(chat_canvas::CanvasUiAction::SetAspect(aspect)) => {
                if let Some(s) = self
                    .chat_state
                    .sessions
                    .iter_mut()
                    .find(|s| s.id == session_id)
                {
                    s.canvas_aspect = aspect;
                }
                let _ = self.cmd_tx.send(Cmd::CanvasSetAspect {
                    session_id: session_id.to_string(),
                    aspect,
                });
            }
            None => {}
        }
    }

    pub(crate) fn canvas_poll_if_due(&mut self, ui: &egui::Ui, session_id: &str) {
        let now = ui.ctx().input(|i| i.time);
        if now >= self.chat_state.view.canvas.poll_due {
            self.chat_state.view.canvas.poll_due = now + 0.20;
            let after = self.chat_state.view.canvas.poll_after_seq();
            let _ = self.cmd_tx.send(Cmd::CanvasPoll {
                session_id: session_id.to_string(),
                after_seq: after,
            });
        }
    }

    pub(crate) fn ui_session_bar(&mut self, ui: &mut egui::Ui, t: &i18n::UiStrings) {
        let Some(sid) = self.chat_state.active_session.clone() else {
            return;
        };
        let meta = chat_room::active_session_meta(&self.chat_state.sessions, Some(sid.as_str()));
        let room = chat_room::session_is_room(meta);
        let canvas_open = meta.map(|m| m.canvas_open).unwrap_or(false);
        let members_vec = meta.map(|m| m.members.clone()).unwrap_or_default();
        let members = members_vec.as_slice();
        let model_id = meta.and_then(|m| m.model_id.clone());
        let session_title = self
            .chat_state
            .sessions
            .iter()
            .find(|s| s.id == sid)
            .map(|s| s.title.clone())
            .unwrap_or_else(|| "Session".to_string());
        let count_line = t
            .room_header_member_count
            .replace("{n}", &members.len().to_string());

        let g = guide::strings(&self.prefs.language);

        ui.horizontal(|ui| {
            let full_w = ui.available_width();
            let toggle_w = session_toggle_reserve_width(t);
            let left_w = (full_w - toggle_w).max(0.0);

            ui.allocate_ui_with_layout(
                egui::vec2(left_w, ui.available_height()),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    let header = egui::RichText::new(&session_title).strong();
                    let title_resp = ui.add(egui::Label::new(header).sense(if room {
                        egui::Sense::click()
                    } else {
                        egui::Sense::hover()
                    }));
                    if room && title_resp.clicked() {
                        self.chat_state.view.room_members_open =
                            !self.chat_state.view.room_members_open;
                    }
                    if room {
                        title_resp.on_hover_text(t.room_header_open_members);
                        if !members.is_empty() {
                            ui.weak(format!("· {count_line}"));
                        }
                        icons::caret(ui, self.chat_state.view.room_members_open);
                    }

                    if guide::tab_help_button(ui, g.help_tooltip) {
                        self.guide.open_topic(guide::GuideTopic::Chat);
                    }
                },
            );

            ui.allocate_ui_with_layout(
                egui::vec2(toggle_w.min(full_w), ui.available_height()),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    if session_toggle_chip(ui, canvas_open, t.session_toggle_canvas) {
                        let new_open = !canvas_open;
                        self.set_canvas_open_local(&sid, new_open);
                        let _ = self.cmd_tx.send(Cmd::CanvasSetOpen {
                            session_id: sid.clone(),
                            open: new_open,
                        });
                    }
                    if session_toggle_chip(ui, room, t.session_toggle_salon) {
                        let mode = if room {
                            ChatSessionMode::Direct
                        } else {
                            ChatSessionMode::Room
                        };
                        let _ = self.cmd_tx.send(Cmd::SessionSetMode {
                            session_id: sid.clone(),
                            mode,
                        });
                    }
                },
            );
        });

        if canvas_open {
            let mut toolbar_action: Option<chat_canvas::CanvasUiAction> = None;
            let mut open_canvas_guide = false;
            let toolbar_min_w = chat_canvas::toolbar_content_min_width(
                t,
                self.chat_state.view.canvas.seeing,
                self.chat_state.view.canvas.clear_confirm_open,
            );
            let track_w = ui.available_width();
            ui.allocate_ui_with_layout(
                egui::vec2(track_w, CANVAS_TOOLBAR_ROW_H),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(track_w);
                    egui::ScrollArea::horizontal()
                        .id_salt("canvas_toolbar_scroll")
                        .auto_shrink([false, false])
                        .max_height(CANVAS_TOOLBAR_ROW_H)
                        .show(ui, |ui| {
                            ui.set_min_width(toolbar_min_w);
                            ui.horizontal(|ui| {
                                ui.set_min_height(CANVAS_TOOLBAR_ROW_H - 4.0);
                                toolbar_action = chat_canvas::ui_canvas_toolbar(
                                    ui,
                                    t,
                                    &mut self.chat_state.view.canvas,
                                    Some(g.help_tooltip),
                                    &mut open_canvas_guide,
                                );
                            });
                        });
                },
            );
            if open_canvas_guide {
                self.guide.open_topic(guide::GuideTopic::Canvas);
            }
            if let Some(action) = toolbar_action {
                self.dispatch_canvas_ui_action(Some(action), &sid);
            }
        }

        if room && self.chat_state.view.room_members_open {
            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong(t.room_members_heading);
                        if guide::tab_help_button(ui, g.help_tooltip) {
                            self.guide.open_topic(guide::GuideTopic::Salon);
                        }
                    });
                    if members.is_empty() {
                        ui.weak(t.room_members_empty);
                    } else {
                        for mem in members {
                            self.ui_room_member_chip(ui, t, &sid, mem);
                        }
                    }
                    let candidates = chat_room::library_add_candidates(&self.agents, members, t);
                    self.ui_room_add_library_chips(ui, t, &sid, model_id.clone(), &candidates);
                });
        }

        if room && !members.is_empty() && !self.chat_state.view.room_members_open {
            ui.horizontal_wrapped(|ui| {
                for mem in members {
                    self.ui_room_member_chip(ui, t, &sid, mem);
                }
            });
        }

        ui.add_space(4.0);
    }
}
