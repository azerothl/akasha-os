//! Central conversation workspace: transcript, canvas, and composer sizing.

use crate::composer_layout::{chat_canvas_layout, chat_composer_reserve_height, ChatCanvasLayout};
use crate::ui_chat_composer::ChatComposerContext;
use crate::{chat_canvas, chat_room, i18n, session_model_supports_vision, UiApp};
use aos_proto::ChatRoomMember;
use eframe::egui;

impl UiApp {
    pub(crate) fn ui_chat_workspace(
        &mut self,
        ui: &mut egui::Ui,
        width: f32,
        height: f32,
        t: &i18n::UiStrings,
    ) {
        let chat_w = width;
        let full_y = height;
        ui.allocate_ui_with_layout(
            egui::vec2(chat_w, full_y),
            egui::Layout::top_down(egui::Align::Min).with_cross_justify(true),
            |ui| {
                ui.set_min_width(chat_w);
                ui.set_min_height(full_y);
                let room_mode = chat_room::session_is_room(chat_room::active_session_meta(
                    &self.sessions,
                    self.active_session.as_deref(),
                ));
                let room_session_meta =
                    chat_room::active_session_meta(&self.sessions, self.active_session.as_deref());
                let room_members: Vec<ChatRoomMember> = room_session_meta
                    .map(|m| m.members.clone())
                    .unwrap_or_default();
                let room_conductor_policy = room_session_meta.map(|m| m.conductor_policy.clone());
                let canvas_open =
                    chat_room::active_session_meta(&self.sessions, self.active_session.as_deref())
                        .map(|m| m.canvas_open)
                        .unwrap_or(false);
                let canvas_aspect =
                    chat_room::active_session_meta(&self.sessions, self.active_session.as_deref())
                        .map(|m| m.canvas_aspect)
                        .unwrap_or_default();
                let active_sid = self.active_session.clone();

                let ask_queue = self.pending_ask_queue();
                let session_model = self
                    .sessions
                    .iter()
                    .find(|s| self.active_session.as_deref() == Some(s.id.as_str()))
                    .and_then(|s| s.model_id.clone());
                let show_vision_banner = !self.chat_state.composer.pending_images.is_empty()
                    && !session_model_supports_vision(session_model.as_deref());
                let composer_h = chat_composer_reserve_height(
                    chat_w,
                    ask_queue.len(),
                    self.chat_state.composer.pending_images.len(),
                    self.chat_state.composer.pending_documents.len(),
                    show_vision_banner,
                );
                let pane_h = ui.available_height();
                let body_h = (pane_h - composer_h).max(120.0);

                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), body_h),
                    egui::Layout::top_down(egui::Align::Min).with_cross_justify(true),
                    |ui| {
                        self.ui_session_bar(ui, t);
                        let content_h = ui.available_height().max(80.0);

                        if canvas_open {
                            let split_gap = 8.0_f32;
                            let total_w = ui.available_width();
                            match chat_canvas_layout(total_w, content_h, split_gap) {
                                ChatCanvasLayout::SideBySide {
                                    transcript_w,
                                    canvas_w,
                                } => {
                                    ui.horizontal(|ui| {
                                        ui.set_min_height(content_h);
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(transcript_w, content_h),
                                            egui::Layout::top_down(egui::Align::Min)
                                                .with_cross_justify(true),
                                            |ui| {
                                                self.ui_chat_transcript(
                                                    ui,
                                                    t,
                                                    room_mode,
                                                    &room_members,
                                                    room_conductor_policy.as_ref(),
                                                    content_h,
                                                );
                                            },
                                        );
                                        ui.add_space(split_gap);
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(canvas_w, content_h),
                                            egui::Layout::top_down(egui::Align::Min)
                                                .with_cross_justify(true),
                                            |ui| {
                                                if let Some(ref sid) = active_sid {
                                                    let aspect_action =
                                                        chat_canvas::ui_canvas_aspect_row(
                                                            ui,
                                                            t,
                                                            canvas_aspect,
                                                        );
                                                    self.dispatch_canvas_ui_action(
                                                        aspect_action,
                                                        sid,
                                                    );
                                                    let action = chat_canvas::ui_canvas_surface(
                                                        ui,
                                                        &mut self.chat_state.view.canvas,
                                                        canvas_aspect,
                                                        t.canvas_empty_hint,
                                                    );
                                                    self.dispatch_canvas_ui_action(action, sid);
                                                    self.canvas_poll_if_due(ui, sid);
                                                }
                                            },
                                        );
                                    });
                                }
                                ChatCanvasLayout::Stacked {
                                    transcript_h,
                                    canvas_h,
                                } => {
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(total_w, transcript_h),
                                        egui::Layout::top_down(egui::Align::Min)
                                            .with_cross_justify(true),
                                        |ui| {
                                            self.ui_chat_transcript(
                                                ui,
                                                t,
                                                room_mode,
                                                &room_members,
                                                room_conductor_policy.as_ref(),
                                                transcript_h,
                                            );
                                        },
                                    );
                                    ui.add_space(split_gap);
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(total_w, canvas_h),
                                        egui::Layout::top_down(egui::Align::Min)
                                            .with_cross_justify(true),
                                        |ui| {
                                            if let Some(ref sid) = active_sid {
                                                let aspect_action =
                                                    chat_canvas::ui_canvas_aspect_row(
                                                        ui,
                                                        t,
                                                        canvas_aspect,
                                                    );
                                                self.dispatch_canvas_ui_action(aspect_action, sid);
                                                let action = chat_canvas::ui_canvas_surface(
                                                    ui,
                                                    &mut self.chat_state.view.canvas,
                                                    canvas_aspect,
                                                    t.canvas_empty_hint,
                                                );
                                                self.dispatch_canvas_ui_action(action, sid);
                                                self.canvas_poll_if_due(ui, sid);
                                            }
                                        },
                                    );
                                }
                            }
                        } else {
                            self.ui_chat_transcript(
                                ui,
                                t,
                                room_mode,
                                &room_members,
                                room_conductor_policy.as_ref(),
                                content_h,
                            );
                        }
                    },
                );

                self.ui_chat_composer(
                    ui,
                    ChatComposerContext {
                        strings: t,
                        room_mode,
                        room_members: &room_members,
                        ask_queue: &ask_queue,
                        height: composer_h,
                        show_vision_banner,
                        chat_width: chat_w,
                    },
                );
            },
        );
    }
}
