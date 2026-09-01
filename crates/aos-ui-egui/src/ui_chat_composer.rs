//! Chat composer, attachments, mentions, and slash-command completion.

use crate::chat_ask::agent_display_title;
use crate::cmd::Cmd;
use crate::composer_layout::{
    composer_field_width, send_button_reserved_width, stop_button_reserved_width,
};
use crate::slash::{slash_completions, slash_insert_text};
use crate::{chat_media, chat_room, i18n, icons, os_open, UiApp};
use aos_proto::ChatRoomMember;
use eframe::egui;

impl UiApp {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn ui_chat_composer(
        &mut self,
        ui: &mut egui::Ui,
        t: &i18n::UiStrings,
        room_mode: bool,
        room_members: &[ChatRoomMember],
        ask_queue: &[String],
        composer_h: f32,
        show_vision_banner: bool,
        chat_w: f32,
    ) {
        let completions = slash_completions(&self.input);
        let mention_hits = if room_mode {
            chat_room::mention_completions(&self.input, room_members, t)
        } else {
            Vec::new()
        };
        let mut chat_sent_this_frame = false;
        let input_row = ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), composer_h),
            egui::Layout::bottom_up(egui::Align::Min),
            |ui| {
                let t = i18n::strings(&self.prefs.language);
                let hint = match ask_queue.len() {
                    0 => t.chat_hint.to_string(),
                    1 => t.chat_hint_agent_ask.to_string(),
                    n => {
                        let title = self
                            .blocked_ask_agent()
                            .map(agent_display_title)
                            .unwrap_or_default();
                        t.chat_hint_agent_ask_many
                            .replace("{agent}", &title)
                            .replace("{n}", &n.to_string())
                    }
                };
                let show_stop =
                    self.chat_pending && (room_mode || self.chat_inference_id.is_some());
                let item_gap = ui.spacing().item_spacing.x;
                let send_w = send_button_reserved_width(ui, &t);
                let stop_w = if show_stop {
                    stop_button_reserved_width(ui, &t)
                } else {
                    0.0
                };

                let mut attach_from_menu = false;
                let mut attach_document_from_menu = false;
                let mut reuse_last_image = false;
                let mut send_clicked = false;
                let mut input_response: Option<egui::Response> = None;

                let mut run_attach_menu = |ui: &mut egui::Ui| {
                    icons::attach_menu(ui, "chat_attach", t.chat_attach_image, |ui| {
                        if self.last_session_image.is_some()
                            && ui.button(t.chat_last_session_image).clicked()
                        {
                            reuse_last_image = true;
                        }
                        if ui.button(t.chat_attach_image).clicked() {
                            attach_from_menu = true;
                        }
                        if ui.button(t.chat_attach_document).clicked() {
                            attach_document_from_menu = true;
                        }
                    });
                };

                let row_w = ui.available_width();
                let input_h = ui.spacing().interact_size.y;
                let field_w = composer_field_width(
                    row_w,
                    send_w,
                    icons::ATTACH_BTN_W,
                    stop_w,
                    item_gap,
                    show_stop,
                );

                ui.allocate_ui_with_layout(
                    egui::vec2(row_w, input_h),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        if show_stop {
                            if room_mode {
                                if ui
                                    .add_sized(
                                        egui::vec2(stop_w, input_h),
                                        egui::Button::new(t.chat_stop),
                                    )
                                    .clicked()
                                {
                                    if let Some(sid) = self.active_session.clone() {
                                        let _ = self
                                            .cmd_tx
                                            .send(Cmd::RoomTurnCancel { session_id: sid });
                                    }
                                }
                            } else if let Some(id) = self.chat_inference_id {
                                if ui
                                    .add_sized(
                                        egui::vec2(stop_w, input_h),
                                        egui::Button::new(t.chat_stop),
                                    )
                                    .clicked()
                                {
                                    if let Some(sid) = self.active_session.clone() {
                                        let _ = self.cmd_tx.send(Cmd::ChatCancel {
                                            inference_id: id,
                                            session_id: sid,
                                        });
                                    }
                                }
                            }
                        }
                        let send_btn = ui
                            .add_sized(egui::vec2(send_w, input_h), egui::Button::new(t.agent_send))
                            .on_hover_text(t.tip_send);
                        send_clicked |= send_btn.clicked();

                        ui.allocate_ui_with_layout(
                            egui::vec2(icons::ATTACH_BTN_W, input_h),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                run_attach_menu(ui);
                            },
                        );

                        ui.set_width(field_w);
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut self.input)
                                .id_salt("chat_input")
                                .desired_width(field_w)
                                .hint_text(&hint),
                        );
                        input_response = Some(r);
                    },
                );

                if attach_from_menu {
                    if let Some(path) = os_open::pick_os_file(
                        t.chat_attach_image,
                        &[("Images", &["png", "jpg", "jpeg", "webp"])],
                        os_open::user_downloads_dir().as_deref(),
                    ) {
                        self.queue_chat_image(path.to_string_lossy().into_owned());
                    }
                } else if attach_document_from_menu {
                    if let Some(path) = os_open::pick_os_file(
                        t.chat_attach_document,
                        &[(
                            "Documents",
                            aos_proto::chat_document::CHAT_DOCUMENT_EXTENSIONS,
                        )],
                        os_open::user_downloads_dir().as_deref(),
                    ) {
                        self.queue_chat_document(path.to_string_lossy().into_owned());
                    }
                } else if reuse_last_image {
                    if let Some(last) = self.last_session_image.clone() {
                        self.queue_chat_image(last);
                    }
                }

                if let Some(r) = input_response {
                    if self.chat_refocus {
                        r.request_focus();
                        self.chat_refocus = false;
                    }
                    let send = send_clicked
                        || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                    if send {
                        self.send_chat();
                        chat_sent_this_frame = true;
                        self.chat_refocus = true;
                    }
                }

                let composer_input_rect = ui.min_rect();
                if !self.chat_pending_images.is_empty() || !self.chat_pending_documents.is_empty() {
                    let ctx = ui.ctx().clone();
                    chat_media::render_pending_attachment_chips(
                        ui,
                        &ctx,
                        &mut self.chat_pending_images,
                        &mut self.chat_pending_documents,
                    );
                }
                if show_vision_banner {
                    let t = i18n::strings(&self.prefs.language);
                    ui.horizontal(|ui| {
                        ui.weak(t.chat_vision_banner);
                        if ui.small_button(t.chat_load_vision_model).clicked() {
                            self.load_preferred_vision_model();
                        }
                    });
                }
                if ask_queue.len() > 1 {
                    let t = i18n::strings(&self.prefs.language);
                    let title = self
                        .blocked_ask_agent()
                        .map(agent_display_title)
                        .unwrap_or_default();
                    ui.colored_label(
                        egui::Color32::from_rgb(240, 190, 100),
                        t.chat_ask_queue
                            .replace("{n}", &ask_queue.len().to_string())
                            .replace("{agent}", &title),
                    );
                }
                composer_input_rect
            },
        );
        let input_rect = input_row.inner;

        // Popup au-dessus de l'input, en overlay sur le chat (pas sous le cadre)
        if !mention_hits.is_empty() {
            let popup_w = input_rect.width().clamp(240.0, chat_w);
            let max_h = 180.0_f32;
            let mut picked: Option<String> = None;
            egui::Area::new(egui::Id::new("mention_completions_popup"))
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(input_rect.left(), input_rect.top() - 6.0))
                .pivot(egui::Align2::LEFT_BOTTOM)
                .interactable(true)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style())
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.set_min_width(popup_w * 0.85);
                            ui.set_max_width(popup_w);
                            ui.label(egui::RichText::new(t.room_mention_pick).small().strong());
                            egui::ScrollArea::vertical()
                                .max_height(max_h)
                                .show(ui, |ui| {
                                    for (text, name) in &mention_hits {
                                        if ui.selectable_label(false, name.as_str()).clicked() {
                                            picked = Some(text.clone());
                                        }
                                    }
                                });
                        });
                });
            if let Some(text) = picked {
                if !chat_sent_this_frame {
                    self.input = text;
                    self.chat_refocus = true;
                }
            }
        } else if !completions.is_empty() {
            let t = i18n::strings(&self.prefs.language);
            let popup_w = input_rect.width().clamp(240.0, chat_w);
            let max_h = 220.0_f32;
            let mut picked: Option<String> = None;
            egui::Area::new(egui::Id::new("slash_completions_popup"))
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(input_rect.left(), input_rect.top() - 6.0))
                .pivot(egui::Align2::LEFT_BOTTOM)
                .interactable(true)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style())
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.set_min_width(popup_w * 0.85);
                            ui.set_max_width(popup_w);
                            ui.label(egui::RichText::new(t.slash_pick).small().strong());
                            egui::ScrollArea::vertical()
                                .max_height(max_h)
                                .show(ui, |ui| {
                                    for (cmd, desc) in &completions {
                                        if ui
                                            .selectable_label(false, format!("{cmd} — {desc}"))
                                            .clicked()
                                        {
                                            picked = Some(slash_insert_text(cmd));
                                        }
                                    }
                                });
                        });
                });
            if let Some(text) = picked {
                if !chat_sent_this_frame {
                    self.input = text;
                    self.chat_refocus = true;
                }
            }
        }
    }
}
