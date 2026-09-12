//! Conversation session bar, room members, and canvas session actions.

use crate::cmd::Cmd;
use crate::{chat_canvas, chat_room, guide, i18n, icons, session_toggle_reserve_width, UiApp};
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
        let border = ui.visuals().widgets.noninteractive.bg_stroke.color;
        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .stroke(egui::Stroke::new(1.0_f32, border))
            .corner_radius(crate::theme::RADIUS_SM)
            .inner_margin(egui::Margin::symmetric(5, 1))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ui.label(egui::RichText::new(&name).small());
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
            });
    }

    pub(crate) fn ui_room_add_library_menu(
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
        let popup_id = ui.id().with("room_add_library");
        let btn = icons::plus_button(ui).on_hover_text(t.room_add_from_library);
        if btn.clicked() {
            ui.memory_mut(|mem| mem.toggle_popup(popup_id));
        }
        egui::popup::popup_below_widget(
            ui,
            popup_id,
            &btn,
            egui::PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(180.0);
                ui.label(t.room_add_from_library);
                ui.separator();
                for agent in candidates {
                    let label = chat_room::roster_agent_label(t, agent);
                    if ui.button(&label).clicked() {
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
                                .unwrap_or_else(|| label.clone());
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
                        ui.close_menu();
                    }
                }
            },
        );
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
                                        | CanvasOpBody::Fill { color, .. }
                                        | CanvasOpBody::Text { color, .. } => {
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
                                    // Texte : la taille passe par `size`, pas `width`.
                                    CanvasOpBody::Text { .. }
                                    | CanvasOpBody::Fill { .. }
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
            Some(chat_canvas::CanvasUiAction::SetGuides {
                show_grid,
                snap,
                grid_size,
            }) => {
                let _ = self.cmd_tx.send(Cmd::CanvasSetGuides {
                    session_id: session_id.to_string(),
                    show_grid,
                    snap,
                    grid_size,
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
        let sid = self.chat_state.active_session.clone();
        if sid.is_none() {
            ui.horizontal(|ui| {
                self.ui_session_switcher(ui, t);
                if icons::plus_button(ui)
                    .on_hover_text(t.session_new)
                    .clicked()
                {
                    let n = self.chat_state.sessions.len() + 1;
                    self.request_session_create(Some(format!("Session {n}")));
                }
            });
            return;
        }
        let Some(sid) = sid else {
            return;
        };
        let meta = chat_room::active_session_meta(&self.chat_state.sessions, Some(sid.as_str()));
        let room = chat_room::session_is_room(meta);
        let canvas_open = meta.map(|m| m.canvas_open).unwrap_or(false);
        let members_vec = meta.map(|m| m.members.clone()).unwrap_or_default();
        let members = members_vec.as_slice();
        let model_id = meta.and_then(|m| m.model_id.clone());
        let count_line = t
            .room_header_member_count
            .replace("{n}", &members.len().to_string());

        let g = guide::strings(&self.prefs.language);

        // Barre responsive : une ligne si large, deux lignes si étroit.
        // Les chips à largeur forcée par estimation en octets peignaient
        // le texte sur les voisins (superposition) ; ils sont en taille
        // naturelle et la barre bascule sur deux lignes sous 200px restants.
        let bar_w = ui.available_width();
        let narrow = (bar_w - session_toggle_reserve_width(t, canvas_open, true)) < 200.0;
        if narrow {
            self.ui_session_bar_left(ui, t, &g, room, &count_line, !members.is_empty());
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.ui_session_bar_toggles(ui, t, &sid, room, canvas_open, true);
                });
            });
        } else {
            let tuck_secondary =
                canvas_open && (bar_w - session_toggle_reserve_width(t, canvas_open, true)) < 280.0;
            ui.horizontal(|ui| {
                let full_w = ui.available_width();
                let toggle_w = session_toggle_reserve_width(t, canvas_open, tuck_secondary);
                let left_w = (full_w - toggle_w).max(0.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(left_w, ui.available_height()),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_max_width(left_w);
                        self.ui_session_bar_left(ui, t, &g, room, &count_line, !members.is_empty());
                    },
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(toggle_w.min(full_w), ui.available_height()),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        self.ui_session_bar_toggles(ui, t, &sid, room, canvas_open, tuck_secondary);
                    },
                );
            });
        }

        if room {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if members.is_empty() {
                    ui.weak(t.room_members_empty);
                } else {
                    for mem in members {
                        self.ui_room_member_chip(ui, t, &sid, mem);
                    }
                }
                let candidates = chat_room::library_add_candidates(&self.agents, members, t);
                self.ui_room_add_library_menu(ui, t, &sid, model_id.clone(), &candidates);
                if guide::tab_help_button(ui, g.help_tooltip) {
                    self.guide.open_topic(guide::GuideTopic::Salon);
                }
            });
        }

        ui.add_space(4.0);
    }

    /// Tools / style / export strip that sits above the canvas surface only
    /// (keeps transcript height free when the detail panel is open).
    pub(crate) fn ui_canvas_tools_above_surface(
        &mut self,
        ui: &mut egui::Ui,
        t: &i18n::UiStrings,
        session_id: &str,
    ) {
        let mut toolbar_action: Option<chat_canvas::CanvasUiAction> = None;
        let mut open_canvas_guide = false;
        let select_active = self.chat_state.view.canvas.tool == chat_canvas::CanvasTool::Select
            && self.chat_state.view.canvas.selected_seq.is_some();
        let track_w = ui.available_width();
        let toolbar_rows = chat_canvas::toolbar_row_count(select_active, track_w);
        let toolbar_h = (toolbar_rows as f32 * chat_canvas::toolbar_row_height() + 4.0)
            .min(chat_canvas::toolbar_max_height());
        ui.allocate_ui_with_layout(
            egui::vec2(track_w, toolbar_h),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(track_w);
                ui.set_max_width(track_w);
                let g = guide::strings(&self.prefs.language);
                toolbar_action = chat_canvas::ui_canvas_toolbar(
                    ui,
                    t,
                    &mut self.chat_state.view.canvas,
                    chat_canvas::canvas_agent_drawing_on_session(&self.agents, session_id),
                    Some(g.help_tooltip),
                    &mut open_canvas_guide,
                );
            },
        );
        if open_canvas_guide {
            self.guide.open_topic(guide::GuideTopic::Canvas);
        }
        if let Some(action) = toolbar_action {
            self.dispatch_canvas_ui_action(Some(action), session_id);
        }
    }

    fn ui_session_switcher(&mut self, ui: &mut egui::Ui, t: &i18n::UiStrings) {
        let title = self
            .chat_state
            .active_session
            .as_deref()
            .and_then(|id| {
                self.chat_state
                    .sessions
                    .iter()
                    .find(|s| s.id == id)
                    .map(|s| s.title.as_str())
            })
            .filter(|s| !s.is_empty())
            .unwrap_or(t.session_picker);
        let short = crate::agent_panel::truncate(title, 32);
        let open = self.chat_state.sidebar.picker_open;
        let galley = ui.fonts(|f| {
            f.layout_no_wrap(
                short.to_string(),
                egui::FontId::proportional(14.0),
                egui::Color32::PLACEHOLDER,
            )
        });
        let size = egui::vec2((galley.size().x + 28.0).max(72.0), 28.0);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        if ui.is_rect_visible(rect) {
            let hovered = response.hovered() || open;
            if hovered {
                ui.painter().rect_filled(
                    rect,
                    crate::theme::RADIUS_SM as f32,
                    ui.visuals().widgets.hovered.bg_fill,
                );
            }
            let text_pos = egui::pos2(rect.left() + 8.0, rect.center().y);
            ui.painter().text(
                text_pos,
                egui::Align2::LEFT_CENTER,
                short,
                egui::FontId::proportional(14.0),
                ui.visuals().strong_text_color(),
            );
            let caret_rect = egui::Rect::from_center_size(
                egui::pos2(rect.right() - 10.0, rect.center().y),
                egui::vec2(10.0, 10.0),
            );
            let color = ui.visuals().weak_text_color();
            let stroke = egui::Stroke::new(1.4_f32, color);
            let c = caret_rect.center();
            let s = 3.2_f32;
            ui.painter().line_segment(
                [c + egui::vec2(-s, -s * 0.2), c + egui::vec2(0.0, s * 0.7)],
                stroke,
            );
            ui.painter().line_segment(
                [c + egui::vec2(0.0, s * 0.7), c + egui::vec2(s, -s * 0.2)],
                stroke,
            );
        }
        if response.on_hover_text(t.session_picker_hint).clicked() {
            self.chat_state.sidebar.picker_open = !open;
        }
    }

    /// Moitié gauche de la barre : switcher, compteur salon, aide.
    fn ui_session_bar_left(
        &mut self,
        ui: &mut egui::Ui,
        t: &i18n::UiStrings,
        g: &guide::GuideStrings,
        room: bool,
        count_line: &str,
        has_members: bool,
    ) {
        self.ui_session_switcher(ui, t);
        if room && has_members {
            ui.weak(egui::RichText::new(count_line).small());
        }
        if guide::tab_help_button(ui, g.help_tooltip) {
            self.guide.open_topic(guide::GuideTopic::Chat);
        }
    }

    /// Moitié droite : Activité / Salon / Deep / Canvas en icônes.
    fn ui_session_bar_toggles(
        &mut self,
        ui: &mut egui::Ui,
        t: &i18n::UiStrings,
        sid: &str,
        room: bool,
        canvas_open: bool,
        tuck_secondary: bool,
    ) {
        if icons::session_chrome_toggle(
            ui,
            canvas_open,
            icons::SessionChromeIcon::Canvas,
            t.session_toggle_canvas,
        )
        .clicked()
        {
            let new_open = !canvas_open;
            self.set_canvas_open_local(sid, new_open);
            let _ = self.cmd_tx.send(Cmd::CanvasSetOpen {
                session_id: sid.to_string(),
                open: new_open,
            });
        }
        if tuck_secondary {
            let _ = icons::overflow_menu(ui, "session_more_toggles", t.session_toggle_deep, |ui| {
                if canvas_open {
                    let focus_on = self.prefs.ui_layout.canvas_focus;
                    if ui
                        .selectable_label(
                            focus_on,
                            if focus_on {
                                t.session_focus_exit
                            } else {
                                t.session_focus
                            },
                        )
                        .clicked()
                    {
                        self.prefs.ui_layout.canvas_focus = !focus_on;
                        crate::prefs::save_preferences(&self.prefs);
                        ui.close_menu();
                    }
                }
                let deep_on = self.chat_state.composer.deep_thinking;
                if ui
                    .selectable_label(deep_on, t.session_toggle_deep)
                    .on_hover_text(t.tip_session_deep_thinking)
                    .clicked()
                {
                    self.chat_state.composer.deep_thinking = !deep_on;
                    ui.close_menu();
                }
            });
        } else {
            if canvas_open {
                let _ = icons::overflow_menu(ui, "session_canvas_focus", t.session_focus, |ui| {
                    let focus_on = self.prefs.ui_layout.canvas_focus;
                    if ui
                        .selectable_label(
                            focus_on,
                            if focus_on {
                                t.session_focus_exit
                            } else {
                                t.session_focus
                            },
                        )
                        .clicked()
                    {
                        self.prefs.ui_layout.canvas_focus = !focus_on;
                        crate::prefs::save_preferences(&self.prefs);
                        ui.close_menu();
                    }
                });
            }
            if icons::session_chrome_toggle(
                ui,
                self.chat_state.composer.deep_thinking,
                icons::SessionChromeIcon::Deep,
                t.tip_session_deep_thinking,
            )
            .clicked()
            {
                self.chat_state.composer.deep_thinking = !self.chat_state.composer.deep_thinking;
            }
        }
        if icons::session_chrome_toggle(
            ui,
            room,
            icons::SessionChromeIcon::Salon,
            t.session_toggle_salon,
        )
        .clicked()
        {
            let mode = if room {
                ChatSessionMode::Direct
            } else {
                ChatSessionMode::Room
            };
            let _ = self.cmd_tx.send(Cmd::SessionSetMode {
                session_id: sid.to_string(),
                mode,
            });
        }
        if icons::activity_toggle_button(ui, self.prefs.ui_layout.activity_panel_open)
            .on_hover_text(if self.prefs.ui_layout.activity_panel_open {
                t.activity_close
            } else {
                t.activity_open
            })
            .clicked()
        {
            self.prefs.ui_layout.activity_panel_open = !self.prefs.ui_layout.activity_panel_open;
            crate::prefs::save_preferences(&self.prefs);
        }
    }

    /// Session-model picker anchored on a clock/composer control.
    pub(crate) fn ui_session_model_picker(
        &mut self,
        ui: &mut egui::Ui,
        t: &i18n::UiStrings,
        anchor: &egui::Response,
        id_salt: &str,
        above: bool,
    ) {
        let popup_id = ui.id().with(id_salt);
        if anchor.clicked() {
            ui.memory_mut(|mem| mem.toggle_popup(popup_id));
        }
        let placement = if above {
            egui::AboveOrBelow::Above
        } else {
            egui::AboveOrBelow::Below
        };
        let current = self.status_model_name();
        let infos = self.models_ui.model_infos.clone();
        let sid = self.chat_state.active_session.clone();
        let mut picked: Option<Option<String>> = None;
        egui::popup::popup_above_or_below_widget(
            ui,
            popup_id,
            anchor,
            placement,
            egui::PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                // Keep the menu inside the window when the composer sits in a narrow pane.
                let max_w = (ui.ctx().screen_rect().width() * 0.42).clamp(160.0, 280.0);
                ui.set_min_width(max_w.min(200.0));
                ui.set_max_width(max_w);
                ui.label(egui::RichText::new(t.status_model_label).small().strong());
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .show(ui, |ui| {
                        let default_on = current.eq_ignore_ascii_case("default");
                        if ui
                            .selectable_label(default_on, t.status_model_default)
                            .clicked()
                        {
                            picked = Some(None);
                        }
                        for m in &infos {
                            let selected = current == m.id;
                            let label = crate::models_page::model_human_label(
                                &m.id,
                                &infos,
                                t.status_model_default,
                            );
                            let short = crate::agent_panel::truncate(&label, 36);
                            if ui
                                .selectable_label(selected, short)
                                .on_hover_text(&m.id)
                                .clicked()
                            {
                                picked = Some(Some(m.id.clone()));
                            }
                        }
                    });
            },
        );
        if let Some(model_id) = picked {
            if let Some(sid) = sid {
                let _ = self.cmd_tx.send(Cmd::SessionSetModel {
                    session_id: sid,
                    model_id,
                });
            } else if let Some(model_id) = model_id {
                self.prefs.default_agent_model = Some(model_id);
                crate::prefs::save_preferences(&self.prefs);
            }
            ui.memory_mut(|mem| mem.close_popup());
        }
    }
}
