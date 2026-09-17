//! Illustration surface panel (skill-inspired still + animate).

use crate::canvas_paint::parse_hex_color;
use crate::cmd::Cmd;
use crate::{chat_room, i18n, UiApp};
use aos_proto::{IllustrationDoc, IllustrationLook, IllustrationPaletteId};
use eframe::egui;

#[derive(Clone, Default)]
pub struct IllustrationUiState {
    pub doc: IllustrationDoc,
    pub last_error: String,
    pub pending_preview: Option<egui::ColorImage>,
    pub texture: Option<egui::TextureHandle>,
    pub animate_enabled: bool,
    /// Host path of the PNG currently loaded into `texture`.
    pub texture_path: String,
    pub poll_due: f64,
}

impl UiApp {
    pub(crate) fn set_illustration_open_local(&mut self, session_id: &str, open: bool) {
        if let Some(s) = self
            .chat_state
            .sessions
            .iter_mut()
            .find(|s| s.id == session_id)
        {
            s.illustration_open = open;
        }
    }

    pub(crate) fn ui_illustration_panel(
        &mut self,
        ui: &mut egui::Ui,
        t: &i18n::UiStrings,
        session_id: &str,
    ) {
        let locked_by = self
            .illust_ui
            .doc
            .lock
            .as_ref()
            .map(|l| l.holder.clone())
            .unwrap_or_default();
        let agent_locked = !locked_by.is_empty() && !locked_by.starts_with("human");

        ui.horizontal(|ui| {
            ui.heading(t.illust_title);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(t.illust_export)
                    .on_hover_text(t.illust_export_tip)
                    .clicked()
                {
                    let _ = self.cmd_tx.send(Cmd::IllustExport {
                        session_id: session_id.to_string(),
                    });
                }
                let anim = ui.add_enabled(
                    self.illust_ui.doc.spec.is_some(),
                    egui::Button::new(t.illust_animate),
                );
                if anim.clicked() {
                    let _ = self.cmd_tx.send(Cmd::IllustAnimate {
                        session_id: session_id.to_string(),
                    });
                }
                if agent_locked {
                    if ui
                        .button(t.illust_takeover)
                        .on_hover_text(format!("{} ({locked_by})", t.illust_takeover_tip))
                        .clicked()
                    {
                        let _ = self.cmd_tx.send(Cmd::IllustTakeover {
                            session_id: session_id.to_string(),
                        });
                    }
                    ui.colored_label(egui::Color32::from_rgb(200, 120, 40), t.illust_locked);
                }
            });
        });

        if !self.illust_ui.last_error.is_empty() {
            ui.colored_label(egui::Color32::from_rgb(220, 80, 80), &self.illust_ui.last_error);
        }

        ui.add_enabled_ui(!agent_locked, |ui| {
            ui.horizontal(|ui| {
                ui.label(t.illust_look);
                for look in [
                    IllustrationLook::Ink,
                    IllustrationLook::Riso,
                    IllustrationLook::Screen,
                    IllustrationLook::Pencil,
                    IllustrationLook::Blueprint,
                ] {
                    let selected = self.illust_ui.doc.brief.look == look;
                    if ui.selectable_label(selected, look.as_str()).clicked() {
                        self.illust_ui.doc.brief.look = look;
                        self.illust_ui.doc.brief.palette = look.default_palette();
                        self.push_illust_brief(session_id);
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.label(t.illust_palette);
                for pal in [
                    IllustrationPaletteId::PaperInk,
                    IllustrationPaletteId::RisoPop,
                    IllustrationPaletteId::ScreenSea,
                    IllustrationPaletteId::PencilMinimal,
                    IllustrationPaletteId::BlueprintNight,
                ] {
                    let selected = self.illust_ui.doc.brief.palette == pal;
                    if ui.selectable_label(selected, pal.as_str()).clicked() {
                        self.illust_ui.doc.brief.palette = pal;
                        self.push_illust_brief(session_id);
                    }
                }
            });

            let mut subject = self.illust_ui.doc.brief.subject.clone();
            let mut push_brief = false;
            ui.horizontal(|ui| {
                ui.label(t.illust_subject);
                let response = ui.text_edit_singleline(&mut subject);
                if response.lost_focus() {
                    push_brief = subject != self.illust_ui.doc.brief.subject;
                }
            });
            self.illust_ui.doc.brief.subject = subject;
            if push_brief {
                self.push_illust_brief(session_id);
            }
        });

        // Preview
        let avail = ui.available_size();
        let side = avail.x.min(avail.y).max(120.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
        let paper = parse_hex_color(
            aos_proto::IllustrationPaletteColors::for_preset(self.illust_ui.doc.brief.palette)
                .paper
                .as_str(),
        );
        ui.painter().rect_filled(rect, 4.0, paper);

        self.refresh_illust_texture(ui.ctx());
        if let Some(img) = self.illust_ui.pending_preview.take() {
            self.illust_ui.texture =
                Some(ui.ctx().load_texture("illust_preview", img, Default::default()));
        }
        if let Some(tex) = &self.illust_ui.texture {
            ui.painter().image(
                tex.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        } else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                t.illust_empty,
                egui::FontId::proportional(14.0),
                egui::Color32::from_rgb(80, 70, 60),
            );
        }

        if let Some(png) = &self.illust_ui.doc.last_png {
            ui.small(format!("{}: {png}", t.illust_last_export));
        }
        if let Some(mp4) = &self.illust_ui.doc.last_mp4 {
            ui.small(format!("mp4: {mp4}"));
        }

        ui.separator();
        ui.small(t.illust_hint);
    }

    pub(crate) fn illust_poll_if_due(&mut self, ui: &egui::Ui, session_id: &str) {
        let now = ui.input(|i| i.time);
        if now >= self.illust_ui.poll_due {
            self.illust_ui.poll_due = now + 0.45;
            let _ = self.cmd_tx.send(Cmd::IllustGet {
                session_id: session_id.to_string(),
            });
        }
    }

    fn refresh_illust_texture(&mut self, ctx: &egui::Context) {
        let logical = self
            .illust_ui
            .doc
            .last_png
            .as_deref()
            .or(self.illust_ui.doc.last_sheet_png.as_deref());
        let Some(logical) = logical else {
            return;
        };
        if logical == self.illust_ui.texture_path && self.illust_ui.texture.is_some() {
            return;
        }
        if let Some(tex) = crate::decl_ui::try_load_png(ctx, logical) {
            self.illust_ui.texture = Some(tex);
            self.illust_ui.texture_path = logical.to_string();
        }
    }

    fn push_illust_brief(&mut self, session_id: &str) {
        let brief = self.illust_ui.doc.brief.clone();
        let _ = self.cmd_tx.send(Cmd::IllustSetBrief {
            session_id: session_id.to_string(),
            brief,
        });
    }

    pub(crate) fn apply_illust_doc(&mut self, doc: IllustrationDoc) {
        let path_changed = doc.last_png != self.illust_ui.doc.last_png
            || doc.last_sheet_png != self.illust_ui.doc.last_sheet_png;
        self.illust_ui.doc = doc;
        self.illust_ui.last_error.clear();
        if path_changed {
            self.illust_ui.texture = None;
            self.illust_ui.texture_path.clear();
        }
    }
}

/// Ensure session bar knows illustration_open from meta.
pub fn illustration_open_for(app: &UiApp) -> bool {
    chat_room::active_session_meta(
        &app.chat_state.sessions,
        app.chat_state.active_session.as_deref(),
    )
    .map(|m| m.illustration_open)
    .unwrap_or(false)
}
