//! Chat composer, attachments, mentions, and slash-command completion.

use crate::chat_ask::agent_display_title;
use crate::cmd::Cmd;
use crate::composer_layout::{
    chat_composer_input_height, composer_action_row_count, composer_enter_action,
    composer_field_width, send_button_reserved_width, stop_button_reserved_width,
    ComposerEnterAction, COMPOSER_ACTION_ROW_H, COMPOSER_FRAME_PAD_V, COMPOSER_ZONE_GAP,
};
use crate::slash::{slash_completions, slash_insert_text};
use crate::{chat_media, chat_room, i18n, icons, os_open, UiApp};
use aos_proto::ChatRoomMember;
use eframe::egui;

pub(crate) struct ChatComposerContext<'a> {
    pub(crate) strings: &'a i18n::UiStrings,
    pub(crate) room_mode: bool,
    pub(crate) room_members: &'a [ChatRoomMember],
    pub(crate) ask_queue: &'a [String],
    pub(crate) height: f32,
    pub(crate) show_vision_banner: bool,
    pub(crate) chat_width: f32,
}

const COMPOSER_MIN_FIELD: f32 = 80.0;

impl UiApp {
    pub(crate) fn ui_chat_composer(&mut self, ui: &mut egui::Ui, context: ChatComposerContext<'_>) {
        let ChatComposerContext {
            strings: t,
            room_mode,
            room_members,
            ask_queue,
            height: composer_h,
            show_vision_banner,
            chat_width: chat_w,
        } = context;
        let completions = slash_completions(&self.chat_state.composer.input);
        let mention_hits = if room_mode {
            chat_room::mention_completions(&self.chat_state.composer.input, room_members, t)
        } else {
            Vec::new()
        };
        let mut chat_sent_this_frame = false;
        let mut sel_changed = false;
        // S7.5 : reset sélection à chaque frappe (les listes changent).
        {
            let composer = &mut self.chat_state.composer;
            if composer.popup_input != composer.input {
                composer.popup_input = composer.input.clone();
                composer.popup_sel = 0;
                composer.popup_dismissed = false;
            }
        }
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
                    self.chat_state.runtime.pending && self.chat_state.active_session.is_some();
                let item_gap = ui.spacing().item_spacing.x.max(4.0);
                let send_w = send_button_reserved_width(ui, &t);
                let stop_w = if show_stop {
                    stop_button_reserved_width(ui, &t)
                } else {
                    0.0
                };
                let buttons_w = send_w + if show_stop { stop_w + item_gap } else { 0.0 };

                let mut attach_from_menu = false;
                let mut attach_document_from_menu = false;
                let mut reuse_last_image = false;
                let mut send_clicked = false;
                let mut stop_clicked = false;
                let mut input_response: Option<egui::Response> = None;
                let has_last_image = self.chat_state.composer.last_session_image.is_some();

                let row_w = ui.available_width();
                let input_h = chat_composer_input_height(&self.chat_state.composer.input);
                let action_rows = composer_action_row_count(row_w, show_stop, buttons_w) as f32;
                let action_h = COMPOSER_ACTION_ROW_H * action_rows;
                let frame_h = input_h + COMPOSER_ZONE_GAP + action_h + COMPOSER_FRAME_PAD_V;
                let inner_w = (row_w - 4.0).max(0.0);
                let field_w = composer_field_width(
                    inner_w - 16.0,
                    send_w,
                    icons::ATTACH_BTN_W,
                    stop_w,
                    item_gap,
                    show_stop,
                );
                let wrap_actions = action_rows > 1.0;
                let strip_h = COMPOSER_ACTION_ROW_H;
                let btn_h = 28.0_f32;
                let colors = crate::theme::button_colors(ui);
                let model_id = self.status_model_name();
                let model_human = crate::models_page::model_human_label(
                    &model_id,
                    &self.models_ui.model_infos,
                    t.status_model_default,
                );
                let model_short = crate::agent_panel::truncate(&model_human, 16);
                let model_hover = format!("{} — {}", t.status_model_label, model_id);

                // Prefer the measured frame; never clip the action strip under a short reserve.
                let frame_slot_h = frame_h.max(ui.available_height().min(frame_h + 8.0));
                ui.allocate_ui_with_layout(
                    egui::vec2(row_w, frame_slot_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::Frame::NONE
                            .fill(ui.visuals().faint_bg_color)
                            .stroke(egui::Stroke::new(
                                1.0_f32,
                                ui.visuals().widgets.noninteractive.bg_stroke.color,
                            ))
                            .corner_radius(crate::theme::RADIUS_MD)
                            .inner_margin(egui::Margin::symmetric(8, 6))
                            .show(ui, |ui| {
                                ui.set_max_width(inner_w);
                                ui.set_min_height(
                                    (frame_slot_h - COMPOSER_FRAME_PAD_V).max(input_h + strip_h),
                                );

                                // Zone 1 — attach + field (actions live on the strip below).
                                ui.horizontal(|ui| {
                                    ui.set_max_width(inner_w);
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(icons::ATTACH_BTN_W, input_h),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            icons::attach_menu(
                                                ui,
                                                "chat_attach",
                                                t.chat_attach_image,
                                                |ui| {
                                                    if has_last_image
                                                        && ui
                                                            .button(t.chat_last_session_image)
                                                            .clicked()
                                                    {
                                                        reuse_last_image = true;
                                                    }
                                                    if ui.button(t.chat_attach_image).clicked() {
                                                        attach_from_menu = true;
                                                    }
                                                    if ui.button(t.chat_attach_document).clicked() {
                                                        attach_document_from_menu = true;
                                                    }
                                                },
                                            );
                                        },
                                    );
                                    let field_avail =
                                        (ui.available_width() - item_gap).max(COMPOSER_MIN_FIELD);
                                    let r = ui.add_sized(
                                        egui::vec2(
                                            field_w.min(field_avail).max(COMPOSER_MIN_FIELD),
                                            input_h,
                                        ),
                                        egui::TextEdit::multiline(
                                            &mut self.chat_state.composer.input,
                                        )
                                        .id_salt("chat_input")
                                        .frame(false)
                                        .desired_rows(2)
                                        .hint_text(&hint),
                                    );
                                    input_response = Some(r);
                                });

                                ui.add_space(COMPOSER_ZONE_GAP);

                                // Zone 2 — model chip left, Envoyer/Stop right (fully inside frame).
                                let paint_model_chip =
                                    |ui: &mut egui::Ui, id_salt: &str, max_w: f32| -> egui::Response {
                                        let painted = egui::Frame::NONE
                                            .fill(ui.visuals().extreme_bg_color)
                                            .stroke(egui::Stroke::new(
                                                1.0_f32,
                                                ui.visuals()
                                                    .widgets
                                                    .noninteractive
                                                    .bg_stroke
                                                    .color,
                                            ))
                                            .corner_radius(crate::theme::RADIUS_SM)
                                            .inner_margin(egui::Margin::symmetric(8, 2))
                                            .show(ui, |ui| {
                                                ui.set_max_width(max_w.max(48.0));
                                                ui.set_height(btn_h - 2.0);
                                                ui.horizontal_centered(|ui| {
                                                    ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new(
                                                                model_short.as_str(),
                                                            )
                                                            .small()
                                                            .color(ui.visuals().text_color()),
                                                        )
                                                        .truncate(),
                                                    );
                                                    icons::dropdown_chevron(ui);
                                                });
                                            })
                                            .response
                                            .on_hover_text(&model_hover);
                                        ui.interact(
                                            painted.rect,
                                            ui.id().with(id_salt),
                                            egui::Sense::click(),
                                        )
                                        .union(painted)
                                    };

                                let strip_w = ui.available_width().min(inner_w);
                                if wrap_actions {
                                    let model_resp =
                                        paint_model_chip(ui, "composer_model_chip", strip_w);
                                    self.ui_session_model_picker(
                                        ui,
                                        &t,
                                        &model_resp,
                                        "composer_model_picker",
                                        true,
                                    );
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(strip_w, strip_h),
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let send_draw =
                                                send_w.min(ui.available_width().max(48.0));
                                            let send_btn = ui
                                                .add_sized(
                                                    egui::vec2(send_draw, btn_h),
                                                    egui::Button::new(t.agent_send)
                                                        .corner_radius(crate::theme::RADIUS_SM)
                                                        .fill(colors.accent),
                                                )
                                                .on_hover_text(format!("{} (Enter)", t.tip_send));
                                            send_clicked |= send_btn.clicked();
                                            if show_stop {
                                                let stop_draw = stop_w.min(
                                                    (ui.available_width() - item_gap).max(40.0),
                                                );
                                                let stop_btn = ui.add_sized(
                                                    egui::vec2(stop_draw, btn_h),
                                                    egui::Button::new(t.chat_stop)
                                                        .corner_radius(crate::theme::RADIUS_SM),
                                                );
                                                if stop_btn.clicked() {
                                                    stop_clicked = true;
                                                }
                                            }
                                        },
                                    );
                                } else {
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(strip_w, strip_h),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            let right_w = buttons_w
                                                .min((strip_w - 56.0 - item_gap).max(48.0));
                                            let left_w =
                                                (strip_w - right_w - item_gap).max(48.0);
                                            ui.allocate_ui_with_layout(
                                                egui::vec2(left_w, strip_h),
                                                egui::Layout::left_to_right(egui::Align::Center),
                                                |ui| {
                                                    let model_resp = paint_model_chip(
                                                        ui,
                                                        "composer_model_chip",
                                                        left_w,
                                                    );
                                                    self.ui_session_model_picker(
                                                        ui,
                                                        &t,
                                                        &model_resp,
                                                        "composer_model_picker",
                                                        true,
                                                    );
                                                },
                                            );
                                            ui.allocate_ui_with_layout(
                                                egui::vec2(right_w, strip_h),
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    let send_btn = ui
                                                        .add_sized(
                                                            egui::vec2(send_w.min(right_w), btn_h),
                                                            egui::Button::new(t.agent_send)
                                                                .corner_radius(
                                                                    crate::theme::RADIUS_SM,
                                                                )
                                                                .fill(colors.accent),
                                                        )
                                                        .on_hover_text(format!(
                                                            "{} (Enter)",
                                                            t.tip_send
                                                        ));
                                                    send_clicked |= send_btn.clicked();
                                                    if show_stop {
                                                        let stop_btn = ui.add_sized(
                                                            egui::vec2(
                                                                stop_w.min(
                                                                    (ui.available_width()
                                                                        - item_gap)
                                                                        .max(40.0),
                                                                ),
                                                                btn_h,
                                                            ),
                                                            egui::Button::new(t.chat_stop)
                                                                .corner_radius(
                                                                    crate::theme::RADIUS_SM,
                                                                ),
                                                        );
                                                        if stop_btn.clicked() {
                                                            stop_clicked = true;
                                                        }
                                                    }
                                                },
                                            );
                                        },
                                    );
                                }
                            });
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
                    if let Some(last) = self.chat_state.composer.last_session_image.clone() {
                        self.queue_chat_image(last);
                    }
                }

                if stop_clicked {
                    if room_mode {
                        if let Some(sid) = self.chat_state.active_session.clone() {
                            let _ = self.cmd_tx.send(Cmd::RoomTurnCancel { session_id: sid });
                        }
                    } else {
                        self.cancel_pending_turn();
                    }
                }

                if let Some(r) = input_response {
                    if self.chat_state.composer.refocus {
                        r.request_focus();
                        self.chat_state.composer.refocus = false;
                    }
                    // Enter sends; Shift+Enter remains a newline. L'éditeur
                    // garde tout le texte collé : la hauteur visuelle est
                    // plafonnée (132px) mais le contenu défile au lieu
                    // d'être tronqué silencieusement à 5 lignes.
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let shift_enter = ui.input(|i| i.modifiers.shift);
                    // S7.5 : popup ouverte + focus = les flèches naviguent,
                    // Entrée choisit, Échap ferme (pas d'envoi).
                    let focused = r.has_focus();
                    let popup_open = !self.chat_state.composer.popup_dismissed
                        && (!mention_hits.is_empty() || !completions.is_empty());
                    if focused && popup_open {
                        let n = if !mention_hits.is_empty() {
                            mention_hits.len()
                        } else {
                            completions.len()
                        }
                        .max(1);
                        let sel = &mut self.chat_state.composer.popup_sel;
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                            *sel = (*sel + 1) % n;
                            sel_changed = true;
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                            *sel = (*sel + n - 1) % n;
                            sel_changed = true;
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            self.chat_state.composer.popup_dismissed = true;
                        }
                    }
                    let mut keyboard_picked: Option<String> = None;
                    if focused && popup_open && enter && !shift_enter {
                        let sel = self.chat_state.composer.popup_sel;
                        if !mention_hits.is_empty() {
                            keyboard_picked = mention_hits.get(sel).map(|(text, _)| text.clone());
                        } else if !completions.is_empty() {
                            keyboard_picked =
                                completions.get(sel).map(|(cmd, _)| slash_insert_text(cmd));
                        }
                    }
                    let send = send_clicked
                        || (keyboard_picked.is_none()
                            && matches!(
                                composer_enter_action(enter, shift_enter),
                                ComposerEnterAction::Send
                            ));
                    if let Some(text) = keyboard_picked {
                        self.chat_state.composer.input = text;
                        self.chat_state.composer.refocus = true;
                    }
                    if send && self.chat_state.composer.input.ends_with('\n') {
                        self.chat_state.composer.input.pop();
                    }
                    if send {
                        self.send_chat();
                        chat_sent_this_frame = true;
                        self.chat_state.composer.refocus = true;
                    }
                }

                let composer_input_rect = ui.min_rect();
                if !self.chat_state.composer.pending_images.is_empty()
                    || !self.chat_state.composer.pending_documents.is_empty()
                {
                    let ctx = ui.ctx().clone();
                    chat_media::render_pending_attachment_chips(
                        ui,
                        &ctx,
                        &mut self.chat_state.composer.pending_images,
                        &mut self.chat_state.composer.pending_documents,
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
            let sel = self.chat_state.composer.popup_sel;
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
                                    for (idx, (text, name)) in mention_hits.iter().enumerate() {
                                        let selected = idx == sel;
                                        let resp = ui.selectable_label(selected, name.as_str());
                                        if selected && sel_changed {
                                            resp.scroll_to_me(None);
                                        }
                                        if resp.clicked() {
                                            picked = Some(text.clone());
                                        }
                                    }
                                });
                            ui.weak("↑↓ · Enter · Esc");
                        });
                });
            if let Some(text) = picked {
                if !chat_sent_this_frame {
                    self.chat_state.composer.input = text;
                    self.chat_state.composer.refocus = true;
                }
            }
        } else if !completions.is_empty() {
            let t = i18n::strings(&self.prefs.language);
            let popup_w = input_rect.width().clamp(240.0, chat_w);
            let max_h = 220.0_f32;
            let mut picked: Option<String> = None;
            let sel = self.chat_state.composer.popup_sel;
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
                                    for (idx, (cmd, desc)) in completions.iter().enumerate() {
                                        let selected = idx == sel;
                                        let resp = ui
                                            .selectable_label(selected, format!("{cmd} — {desc}"));
                                        if selected && sel_changed {
                                            resp.scroll_to_me(None);
                                        }
                                        if resp.clicked() {
                                            picked = Some(slash_insert_text(cmd));
                                        }
                                    }
                                });
                            ui.weak("↑↓ · Enter · Esc");
                        });
                });
            if let Some(text) = picked {
                if !chat_sent_this_frame {
                    self.chat_state.composer.input = text;
                    self.chat_state.composer.refocus = true;
                }
            }
        }
    }
}
