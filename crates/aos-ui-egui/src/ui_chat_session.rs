//! Conversation session bar, room members, and canvas session actions.

use crate::cmd::Cmd;
use crate::{
    chat_canvas, chat_room, guide, i18n, icons, session_toggle_chip, session_toggle_reserve_width,
    UiApp, CANVAS_TOOLBAR_ROW_H,
};
use aos_proto::{
    align_canvas_op_body, canvas_op_bbox, normalize_canvas_color, set_canvas_op_body_dash,
    set_canvas_op_body_gradient, set_canvas_op_body_opacity, set_canvas_op_rotation,
    usable_canvas_bbox, AgentInfo, CanvasLayer, CanvasOpBody, ChatRoomMember, ChatSessionMode,
};
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
                    _ => {
                        let layer_id = self.chat_state.view.canvas.active_layer_id.clone();
                        self.chat_state.view.canvas.ops.push(aos_proto::CanvasOp {
                            seq: 0,
                            author_id: "human".into(),
                            ts_ms: 0,
                            layer_id,
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
            Some(chat_canvas::CanvasUiAction::Edit(edit)) => {
                match &edit {
                    aos_proto::CanvasEdit::LayerSet {
                        id,
                        visible,
                        locked,
                        opacity,
                    } => {
                        if let Some(layer) = self
                            .chat_state
                            .view
                            .canvas
                            .layers
                            .iter_mut()
                            .find(|l| l.id == *id)
                        {
                            if let Some(v) = visible {
                                layer.visible = *v;
                            }
                            if let Some(v) = locked {
                                layer.locked = *v;
                            }
                            if let Some(v) = opacity {
                                layer.opacity = *v;
                            }
                        }
                    }
                    aos_proto::CanvasEdit::LayerActivate { id } => {
                        self.chat_state.view.canvas.active_layer_id = id.clone();
                    }
                    aos_proto::CanvasEdit::Delete { seq } => {
                        self.chat_state.view.canvas.ops.retain(|o| o.seq != *seq);
                        if self.chat_state.view.canvas.selected_seq == Some(*seq) {
                            self.chat_state.view.canvas.selected_seq = None;
                        }
                    }
                    aos_proto::CanvasEdit::Reorder { seq, z } => {
                        if let Some(pos) = self
                            .chat_state
                            .view
                            .canvas
                            .ops
                            .iter()
                            .position(|o| o.seq == *seq)
                        {
                            let op = self.chat_state.view.canvas.ops.remove(pos);
                            let z = (*z).clamp(0, self.chat_state.view.canvas.ops.len() as i64)
                                as usize;
                            self.chat_state.view.canvas.ops.insert(z, op);
                        }
                    }
                    aos_proto::CanvasEdit::Restyle {
                        seq,
                        color,
                        width,
                        fill,
                        rotation,
                        opacity,
                        dash,
                        gradient,
                    } => {
                        if let Some(op) = self
                            .chat_state
                            .view
                            .canvas
                            .ops
                            .iter_mut()
                            .find(|o| o.seq == *seq)
                        {
                            if let Some(c) = color.as_deref() {
                                if let Some(normalized) = normalize_canvas_color(c) {
                                    match &mut op.body {
                                        CanvasOpBody::Stroke { color, .. }
                                        | CanvasOpBody::Rect { color, .. }
                                        | CanvasOpBody::Ellipse { color, .. }
                                        | CanvasOpBody::Line { color, .. }
                                        | CanvasOpBody::Spline { color, .. }
                                        | CanvasOpBody::Path { color, .. }
                                        | CanvasOpBody::Fill { color, .. } => {
                                            *color = normalized;
                                        }
                                        CanvasOpBody::Erase { .. }
                                        | CanvasOpBody::Clear
                                        | CanvasOpBody::Undo => {}
                                    }
                                }
                            }
                            if let Some(w) = width {
                                let w = w.clamp(0.001, 0.25);
                                match &mut op.body {
                                    CanvasOpBody::Stroke { width, .. }
                                    | CanvasOpBody::Rect { width, .. }
                                    | CanvasOpBody::Ellipse { width, .. }
                                    | CanvasOpBody::Line { width, .. }
                                    | CanvasOpBody::Spline { width, .. }
                                    | CanvasOpBody::Path { width, .. }
                                    | CanvasOpBody::Erase { width, .. } => *width = w,
                                    CanvasOpBody::Fill { .. }
                                    | CanvasOpBody::Clear
                                    | CanvasOpBody::Undo => {}
                                }
                            }
                            if let Some(fill) = fill {
                                match &mut op.body {
                                    CanvasOpBody::Rect { fill: slot, .. }
                                    | CanvasOpBody::Ellipse { fill: slot, .. }
                                    | CanvasOpBody::Path { fill: slot, .. } => *slot = *fill,
                                    _ => {}
                                }
                            }
                            if let Some(rotation) = rotation {
                                let _ = set_canvas_op_rotation(&mut op.body, *rotation);
                            }
                            if let Some(opacity) = opacity {
                                set_canvas_op_body_opacity(&mut op.body, *opacity);
                            }
                            if let Some(dash) = dash {
                                set_canvas_op_body_dash(&mut op.body, dash.clone());
                            }
                            if let Some(gradient) = gradient {
                                set_canvas_op_body_gradient(&mut op.body, gradient.clone());
                            }
                        }
                    }
                    aos_proto::CanvasEdit::Align { seq, to_seq, edges } => {
                        let canvas = &mut self.chat_state.view.canvas;
                        if let Some(src_idx) = canvas.ops.iter().position(|o| o.seq == *seq) {
                            if let Some(src_bbox) = canvas_op_bbox(&canvas.ops[src_idx].body) {
                                let target = if let Some(to) = to_seq {
                                    canvas
                                        .ops
                                        .iter()
                                        .find(|o| o.seq == *to)
                                        .and_then(|o| canvas_op_bbox(&o.body))
                                        .unwrap_or_else(usable_canvas_bbox)
                                } else {
                                    usable_canvas_bbox()
                                };
                                align_canvas_op_body(
                                    &mut canvas.ops[src_idx].body,
                                    src_bbox,
                                    target,
                                    edges,
                                );
                            }
                        }
                    }
                    aos_proto::CanvasEdit::LayerRename { id, name } => {
                        if let Some(layer) = self
                            .chat_state
                            .view
                            .canvas
                            .layers
                            .iter_mut()
                            .find(|l| l.id == *id)
                        {
                            layer.name = name.clone();
                        }
                    }
                    aos_proto::CanvasEdit::LayerReorder { id, parent_id, z } => {
                        let canvas = &mut self.chat_state.view.canvas;
                        if let Some(layer) = canvas.layers.iter_mut().find(|l| l.id == *id) {
                            layer.parent_id = parent_id.clone();
                        }
                        if let Some(pos) = canvas.layers.iter().position(|l| l.id == *id) {
                            let layer = canvas.layers.remove(pos);
                            let z = (*z).clamp(0, canvas.layers.len() as i64) as usize;
                            canvas.layers.insert(z, layer);
                        }
                    }
                    aos_proto::CanvasEdit::LayerCreate { name, parent_id } => {
                        let canvas = &mut self.chat_state.view.canvas;
                        let n = canvas
                            .layers
                            .iter()
                            .filter_map(|l| l.id.strip_prefix("lyr-"))
                            .filter_map(|s| s.parse::<u32>().ok())
                            .max()
                            .unwrap_or(1)
                            .saturating_add(1);
                        let layer_id = format!("lyr-{n}");
                        let label = name
                            .as_ref()
                            .filter(|s| !s.trim().is_empty())
                            .cloned()
                            .unwrap_or_else(|| format!("Layer {}", canvas.layers.len() + 1));
                        canvas.layers.push(CanvasLayer {
                            id: layer_id.clone(),
                            name: label,
                            parent_id: parent_id.clone(),
                            visible: true,
                            locked: false,
                            opacity: 1.0,
                        });
                        canvas.active_layer_id = layer_id;
                    }
                    aos_proto::CanvasEdit::LayerDelete { id } => {
                        let canvas = &mut self.chat_state.view.canvas;
                        if canvas.layers.len() > 1 {
                            if let Some(idx) = canvas.layers.iter().position(|l| l.id == *id) {
                                let removed = canvas.layers.remove(idx);
                                let fallback = removed
                                    .parent_id
                                    .clone()
                                    .filter(|p| canvas.layers.iter().any(|l| l.id == *p))
                                    .unwrap_or_else(|| canvas.layers[0].id.clone());
                                for child in canvas
                                    .layers
                                    .iter_mut()
                                    .filter(|l| l.parent_id.as_deref() == Some(id.as_str()))
                                {
                                    child.parent_id = removed.parent_id.clone();
                                }
                                for op in &mut canvas.ops {
                                    if op.layer_id == *id {
                                        op.layer_id = fallback.clone();
                                    }
                                }
                                if canvas.active_layer_id == *id {
                                    canvas.active_layer_id = fallback;
                                }
                            }
                        }
                    }
                    _ => {}
                }
                let _ = self.cmd_tx.send(Cmd::CanvasEdit {
                    session_id: session_id.to_string(),
                    author_id: "human".into(),
                    edit,
                });
            }
            Some(chat_canvas::CanvasUiAction::SetStyle {
                color,
                width,
                opacity,
                dash,
            }) => {
                let _ = self.cmd_tx.send(Cmd::CanvasSetStyle {
                    session_id: session_id.to_string(),
                    color,
                    width,
                    opacity,
                    dash,
                });
            }
            Some(chat_canvas::CanvasUiAction::ExportPng) => {
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
                    format: "png".into(),
                });
            }
            Some(chat_canvas::CanvasUiAction::ExportSvg) => {
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
                    format: "svg".into(),
                });
            }
            Some(chat_canvas::CanvasUiAction::ExportJson) => {
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
                    format: "json".into(),
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
            Some(chat_canvas::CanvasUiAction::ImportJson) => {
                let t = crate::i18n::strings(&self.prefs.language);
                if let Some(path) = crate::os_open::pick_os_file(
                    t.canvas_import,
                    &[("JSON", &["json"])],
                    crate::os_open::user_downloads_dir().as_deref(),
                ) {
                    if let Ok(raw) = std::fs::read_to_string(&path) {
                        match aos_proto::parse_canvas_sidecar_json(&raw) {
                            Ok((doc, aspect)) => {
                                let _ = self.cmd_tx.send(Cmd::CanvasImport {
                                    session_id: session_id.to_string(),
                                    doc,
                                    aspect: Some(aspect),
                                });
                            }
                            Err(e) => {
                                self.status = format!("{}: {e}", t.canvas_import);
                            }
                        }
                    }
                }
            }
            Some(chat_canvas::CanvasUiAction::ResetView) => {
                self.chat_state.view.canvas.view_pan = eframe::egui::Vec2::ZERO;
                self.chat_state.view.canvas.view_zoom = 1.0;
            }
            None => {}
        }
    }

    pub(crate) fn canvas_poll_if_due(&mut self, ui: &egui::Ui, session_id: &str) {
        if !ui.ctx().input(|i| i.focused) {
            return;
        }
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
                    let mut selected_model = model_id.clone().unwrap_or_default();
                    let model_label = if selected_model.is_empty() {
                        "default".to_string()
                    } else {
                        selected_model.clone()
                    };
                    egui::ComboBox::from_id_salt("chat_model_picker")
                        .selected_text(model_label)
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(&mut selected_model, String::new(), "default")
                                .changed()
                            {
                                let _ = self.cmd_tx.send(Cmd::SessionSetModel {
                                    session_id: sid.clone(),
                                    model_id: None,
                                });
                            }
                            for m in self
                                .models_ui
                                .model_infos
                                .iter()
                                .filter(|m| !m.id.starts_with("provider:"))
                            {
                                if ui
                                    .selectable_value(&mut selected_model, m.id.clone(), &m.name)
                                    .changed()
                                {
                                    let _ = self.cmd_tx.send(Cmd::SessionSetModel {
                                        session_id: sid.clone(),
                                        model_id: Some(m.id.clone()),
                                    });
                                }
                            }
                        });
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
                    let activity_label = if canvas_open {
                        if self.prefs.ui_layout.activity_panel_open {
                            "× Act.".to_string()
                        } else {
                            "Act.".to_string()
                        }
                    } else if self.prefs.ui_layout.activity_panel_open {
                        format!("× {}", t.activity_open)
                    } else {
                        t.activity_open.to_string()
                    };
                    if ui
                        .button(activity_label)
                        .on_hover_text(t.activity_open)
                        .clicked()
                    {
                        self.prefs.ui_layout.activity_panel_open =
                            !self.prefs.ui_layout.activity_panel_open;
                        crate::prefs::save_preferences(&self.prefs);
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
                    if canvas_open
                        && ui
                            .button(if self.prefs.ui_layout.canvas_focus {
                                "Focus ×"
                            } else {
                                "Focus"
                            })
                            .on_hover_text(if self.prefs.ui_layout.canvas_focus {
                                t.canvas_focus_exit
                            } else {
                                t.canvas_focus
                            })
                            .clicked()
                    {
                        self.prefs.ui_layout.canvas_focus = !self.prefs.ui_layout.canvas_focus;
                        crate::prefs::save_preferences(&self.prefs);
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
                                    chat_canvas::canvas_agent_drawing_on_session(
                                        &self.agents,
                                        &sid,
                                    ),
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
