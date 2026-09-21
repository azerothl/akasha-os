//! Illustration surface panel (skill-inspired still + animate).

use crate::canvas_paint::parse_hex_color;
use crate::cmd::Cmd;
use crate::{chat_room, i18n, UiApp};
use aos_proto::{
    IllustrationDoc, IllustrationLook, IllustrationPaletteId, IllustrationPose,
};
use eframe::egui;

#[derive(Debug, PartialEq)]
enum PreviewChoice { Retained, Rejected, Pending, Historical }

fn preview_choice(doc: &IllustrationDoc, path: &str) -> PreviewChoice {
    if let Some(run) = &doc.image_run {
        if run.candidate_png.as_deref() == Some(path) {
            match run.candidate_selected {
                Some(false) => return PreviewChoice::Rejected,
                None => return PreviewChoice::Pending,
                Some(true) => {},
            }
        }
    }
    if doc.last_png.as_deref() == Some(path) { PreviewChoice::Retained }
    else { PreviewChoice::Historical }
}

fn history_label(doc: &IllustrationDoc, path: &str, label: &str, revision: u64, t: &i18n::UiStrings) -> String {
    let status = match preview_choice(doc, path) {
        PreviewChoice::Retained => t.illust_history_retained,
        PreviewChoice::Rejected => t.illust_history_rejected,
        PreviewChoice::Pending => t.illust_history_pending,
        PreviewChoice::Historical => "",
    };
    if status.is_empty() { format!("{label} · {revision}") }
    else { format!("{label} · {revision} · {status}") }
}

fn spec_is_drawable(spec: &aos_proto::IllustrationSpec) -> bool {
    !spec.skeleton.is_empty()
        || !spec.volumes.is_empty()
        || !spec.contours.is_empty()
        || !spec.details.is_empty()
        || !spec.parts.is_empty()
        || !spec.construction.action_line_points.is_empty()
        || spec.construction.chest_oval.is_some()
        || spec.construction.pelvis_oval.is_some()
}

#[derive(Clone, Copy)]
struct DrawIn {
    t0: f64,
    duration: f32,
    last_step: i32,
}

#[derive(Clone, Default)]
pub struct IllustrationUiState {
    pub doc: IllustrationDoc,
    pub last_error: String,
    pub pending_preview: Option<egui::ColorImage>,
    pub texture: Option<egui::TextureHandle>,
    pub animate_enabled: bool,
    /// Host path of the PNG currently loaded into `texture`.
    pub texture_path: String,
    /// None follows live generation; Some selects a published historical pass.
    pub selected_pass_path: Option<String>,
    pub poll_due: f64,
    /// Replay strokes when the spec changes (compose / enrich), not only the final PNG.
    pending_draw: bool,
    draw: Option<DrawIn>,
    /// In-panel playback started by the Animate button.
    pending_motion: bool,
    motion: Option<DrawIn>,
    /// Duration prompt before background frame render.
    pub animate_prompt: bool,
    pub animate_seconds: f32,
    pub animate_session: String,
    pub animate_busy: bool,
    pub animate_notice: String,
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
        let pending_candidate = self.illust_ui.doc.image_run.as_ref()
            .is_some_and(|r| r.candidate_png.is_some() && r.candidate_selected.is_none());
        let export_ready = !pending_candidate && self.illust_ui.doc.image_run.as_ref()
            .is_none_or(|r| r.status == aos_proto::IllustrationImageStatus::NeedsReview);

        ui.horizontal(|ui| {
            ui.heading(t.illust_title);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(export_ready, egui::Button::new(if self.illust_ui.doc.image_run.is_some() { t.illust_export_selected } else { t.illust_export }))
                    .on_hover_text(t.illust_export_tip)
                    .clicked()
                {
                    let _ = self.cmd_tx.send(Cmd::IllustExport {
                        session_id: session_id.to_string(),
                    });
                }
                let anim = ui.add_enabled(
                    self.illust_ui.doc.spec.is_some() && !self.illust_ui.animate_busy,
                    egui::Button::new(t.illust_animate),
                );
                if anim.clicked() {
                    if self.illust_ui.animate_seconds < 0.5 {
                        self.illust_ui.animate_seconds = 4.0;
                    }
                    self.illust_ui.animate_session = session_id.to_string();
                    self.illust_ui.animate_prompt = true;
                    self.illust_ui.animate_notice.clear();
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
                    IllustrationLook::Doodle,
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

        if let Some(run) = &self.illust_ui.doc.image_run {
            match run.status {
                aos_proto::IllustrationImageStatus::Running => {
                    ui.horizontal(|ui| { ui.spinner(); ui.label(format!("Génération en cours · {:?}", run.phase)); });
                }
                aos_proto::IllustrationImageStatus::NeedsReview => { ui.label("Rendu généré · qualité visuelle à vérifier"); }
                aos_proto::IllustrationImageStatus::Failed => {
                    ui.colored_label(egui::Color32::RED, run.error.as_deref().unwrap_or("Échec de génération"));
                }
            }
        } else if let Some(spec) = self.illust_ui.doc.spec.as_ref() {
            use aos_proto::IllustrationConstructionPhase as Phase;
            let label = match spec.construction_phase {
                Phase::Skeleton => "1/5 · Pose et construction",
                Phase::Volumes => "2/5 · Volumes",
                Phase::Contours => "3/5 · Contours",
                Phase::Details => "4/5 · Détails et habillage",
                Phase::Final => "5/5 · Rendu final",
            };
            ui.label(label);
        }

        let previous_selection = self.illust_ui.selected_pass_path.clone();
        if pending_candidate {
            let run = self.illust_ui.doc.image_run.as_ref().unwrap().clone();
            ui.label("Comparez les deux versions avant de poursuivre. L’original reste conservé.");
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.illust_ui.selected_pass_path, run.source_png.clone(), "Voir l’original");
                ui.selectable_value(&mut self.illust_ui.selected_pass_path, run.candidate_png.clone(), "Voir la retouche");
            });
            ui.add_enabled_ui(!agent_locked, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (label, keep_candidate) in [("Conserver l’original", false), ("Utiliser la retouche", true)] {
                        if ui.button(label).clicked() {
                            let _ = self.cmd_tx.send(Cmd::IllustResolveImage {
                                session_id: session_id.to_string(), run_id: run.id.clone(), keep_candidate,
                            });
                        }
                    }
                });
            });
        }
        egui::ComboBox::from_id_salt("illustration_pass_history")
            .selected_text(if previous_selection.is_some() { "Historique des passes" }
                else if self.illust_ui.doc.image_run.as_ref().is_some_and(|r| r.status == aos_proto::IllustrationImageStatus::Running) { "Suivre la génération" }
                else { t.illust_show_selected })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.illust_ui.selected_pass_path, None,
                    if self.illust_ui.doc.image_run.as_ref().is_some_and(|r| r.status == aos_proto::IllustrationImageStatus::Running) { "Suivre la génération" } else { t.illust_show_selected });
                for (phase, revision, path) in &self.illust_ui.doc.pass_previews {
                    use aos_proto::IllustrationConstructionPhase as Phase;
                    let label = match phase {
                        Phase::Skeleton => "Pose", Phase::Volumes => "Volumes",
                        Phase::Contours => "Contours", Phase::Details => "Détails",
                        Phase::Final => "Final",
                    };
                    ui.selectable_value(&mut self.illust_ui.selected_pass_path, Some(path.clone()),
                        history_label(&self.illust_ui.doc, path, label, *revision, t));
                }
            });
        if let Some(path) = self.illust_ui.selected_pass_path.as_deref() {
            if !pending_candidate && self.illust_ui.doc.last_png.as_deref() != Some(path) {
                if preview_choice(&self.illust_ui.doc, path) == PreviewChoice::Rejected {
                    ui.label(t.illust_history_rejected);
                }
                ui.label(t.illust_history_export_notice);
                if ui.button(t.illust_show_selected).clicked() {
                    self.illust_ui.selected_pass_path = None;
                }
            }
        }
        if previous_selection != self.illust_ui.selected_pass_path {
            self.illust_ui.pending_draw = false;
            self.illust_ui.pending_motion = false;
            self.illust_ui.draw = None;
            self.illust_ui.motion = None;
            self.illust_ui.pending_preview = None;
            self.illust_ui.texture = None;
            self.illust_ui.texture_path.clear();
        }

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

        if self.illust_ui.selected_pass_path.is_none() {
            self.tick_illust_draw(ui.ctx());
        }
        let playing = self.illust_ui.draw.is_some() || self.illust_ui.motion.is_some();
        if !playing {
            self.refresh_illust_texture(ui.ctx());
            if let Some(img) = self.illust_ui.pending_preview.take() {
                self.illust_ui.texture =
                    Some(ui.ctx().load_texture("illust_preview", img, Default::default()));
            }
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

        if playing {
            ui.small(t.illust_animating);
        }
        if self.illust_ui.animate_busy {
            ui.colored_label(egui::Color32::from_rgb(40, 90, 140), t.illust_animate_busy);
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

    fn tick_illust_draw(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if self.illust_ui.pending_motion {
            self.illust_ui.pending_motion = false;
            self.illust_ui.pending_draw = false;
            self.illust_ui.draw = None;
            self.illust_ui.motion = Some(DrawIn {
                t0: now,
                duration: 2.6,
                last_step: -1,
            });
        } else if self.illust_ui.pending_draw {
            self.illust_ui.pending_draw = false;
            let duration = self
                .illust_ui
                .doc
                .spec
                .as_ref()
                .map(aos_platform::illustration_raster::draw_in_seconds)
                .unwrap_or(2.4);
            self.illust_ui.draw = Some(DrawIn {
                t0: now,
                duration,
                last_step: -1,
            });
        }
        if self.illust_ui.motion.is_some() {
            self.tick_illust_motion(ctx, now);
            return;
        }
        let Some(draw) = self.illust_ui.draw else {
            return;
        };
        let u = ((now - draw.t0) / draw.duration as f64) as f32;
        if u >= 1.0 {
            self.illust_ui.draw = None;
            self.illust_ui.texture_path.clear();
            ctx.request_repaint();
            return;
        }
        let step = (u * 28.0) as i32;
        if step != draw.last_step || self.illust_ui.texture.is_none() {
            if let Some(draw) = self.illust_ui.draw.as_mut() {
                draw.last_step = step;
            }
            self.paint_illust_progress(ctx, u.clamp(0.0, 0.999));
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(32));
    }

    fn tick_illust_motion(&mut self, ctx: &egui::Context, now: f64) {
        let Some(motion) = self.illust_ui.motion else {
            return;
        };
        let u = ((now - motion.t0) / motion.duration as f64) as f32;
        if u >= 1.0 {
            self.illust_ui.motion = None;
            self.illust_ui.texture_path.clear();
            ctx.request_repaint();
            return;
        }
        let step = (u * 16.0) as i32;
        if step != motion.last_step || self.illust_ui.texture.is_none() {
            if let Some(motion) = self.illust_ui.motion.as_mut() {
                motion.last_step = step;
            }
            self.paint_illust_motion(ctx, u.clamp(0.0, 0.999));
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(40));
    }

    fn paint_illust_motion(&mut self, ctx: &egui::Context, progress: f32) {
        let doc = self.illust_ui.doc.clone();
        let Some(spec) = doc.spec.as_ref() else {
            return;
        };
        let camera = spec.camera.clone();
        let mode = spec.mode;
        let pose = pose_at(progress);
        let Ok(bytes) = aos_platform::illustration_raster::export_frame_png(
            &doc, &pose, &camera, mode, 420, 420,
        ) else {
            return;
        };
        let Some(img) = png_bytes_to_color_image(&bytes) else {
            return;
        };
        self.illust_ui.texture = Some(ctx.load_texture("illust_motion", img, Default::default()));
        self.illust_ui.texture_path.clear();
    }

    fn paint_illust_progress(&mut self, ctx: &egui::Context, progress: f32) {
        let doc = self.illust_ui.doc.clone();
        let Ok(bytes) = aos_platform::illustration_raster::export_png_progress(&doc, 480, 480, progress)
        else {
            return;
        };
        let Some(img) = png_bytes_to_color_image(&bytes) else {
            return;
        };
        self.illust_ui.texture = Some(ctx.load_texture("illust_draw", img, Default::default()));
        self.illust_ui.texture_path.clear();
    }

    fn refresh_illust_texture(&mut self, ctx: &egui::Context) {
        let logical = self.illust_ui.selected_pass_path.as_deref()
            .or_else(|| self.illust_ui.doc.image_run.as_ref()
                .filter(|r| r.candidate_selected.is_none())
                .and_then(|r| r.candidate_png.as_deref()))
            .or(self.illust_ui.doc.last_png.as_deref())
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
        if let (Some(old), Some(new)) = (&self.illust_ui.doc.image_run, &doc.image_run) {
            if old.id == new.id && old.candidate_selected != new.candidate_selected {
                self.illust_ui.selected_pass_path = None;
            }
        }
        if doc.session_id != self.illust_ui.doc.session_id {
            self.illust_ui.selected_pass_path = None;
        }
        let spec_changed = doc.spec != self.illust_ui.doc.spec;
        let path_changed = doc.last_png != self.illust_ui.doc.last_png
            || doc.last_sheet_png != self.illust_ui.doc.last_sheet_png;
        let pass_count_grew =
            doc.pass_previews.len() > self.illust_ui.doc.pass_previews.len();
        let image_pass_advanced = match (&self.illust_ui.doc.image_run, &doc.image_run) {
            (Some(old), Some(new)) if old.id == new.id => {
                old.phase != new.phase || path_changed || pass_count_grew
            }
            (None, Some(_)) => path_changed || pass_count_grew,
            _ => false,
        };
        let drawable = doc.spec.as_ref().is_some_and(spec_is_drawable);
        self.illust_ui.doc = doc;
        if self.illust_ui.doc.image_run.is_some() {
            // Image passes are full PNGs — no vector stroke replay.
            self.illust_ui.pending_draw = false;
            self.illust_ui.pending_motion = false;
            self.illust_ui.draw = None;
            self.illust_ui.motion = None;
            self.illust_ui.pending_preview = None;
            if image_pass_advanced || path_changed {
                // Show each published pass as soon as it lands.
                self.illust_ui.texture = None;
                self.illust_ui.texture_path.clear();
            }
        }
        self.illust_ui.last_error.clear();
        if self.illust_ui.selected_pass_path.is_some() {
            return;
        }
        if self.illust_ui.doc.image_run.is_none() && spec_changed && drawable {
            self.illust_ui.pending_draw = true;
            self.illust_ui.pending_motion = false;
            self.illust_ui.draw = None;
            self.illust_ui.motion = None;
            self.illust_ui.texture = None;
            self.illust_ui.texture_path.clear();
        } else if path_changed
            && self.illust_ui.draw.is_none()
            && self.illust_ui.motion.is_none()
            && !self.illust_ui.pending_draw
            && !self.illust_ui.pending_motion
        {
            self.illust_ui.texture = None;
            self.illust_ui.texture_path.clear();
        }
    }

    pub(crate) fn ui_illust_animate_popups(&mut self, ctx: &egui::Context, t: &i18n::UiStrings) {
        if self.illust_ui.animate_prompt {
            let mut close = false;
            let mut start = false;
            egui::Window::new(t.illust_animate_title)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label(t.illust_animate_seconds);
                    ui.add(
                        egui::Slider::new(&mut self.illust_ui.animate_seconds, 1.0..=8.0)
                            .fixed_decimals(1)
                            .suffix(" s"),
                    );
                    let frames = (self.illust_ui.animate_seconds * 24.0).round() as i32;
                    ui.small(
                        t.illust_animate_frames
                            .replace("{n}", &frames.to_string()),
                    );
                    ui.horizontal(|ui| {
                        if ui.button(t.memory_btn_cancel).clicked() {
                            close = true;
                        }
                        if ui.button(t.illust_animate_start).clicked() {
                            start = true;
                        }
                    });
                });
            if close {
                self.illust_ui.animate_prompt = false;
            }
            if start {
                let duration_s = self.illust_ui.animate_seconds.clamp(0.5, 12.0);
                let session_id = self.illust_ui.animate_session.clone();
                self.illust_ui.animate_prompt = false;
                self.illust_ui.animate_busy = true;
                self.illust_ui.animate_notice.clear();
                self.illust_ui.last_error.clear();
                let _ = self.cmd_tx.send(Cmd::IllustAnimate {
                    session_id,
                    duration_s,
                });
            }
        }
        if !self.illust_ui.animate_notice.is_empty() {
            let mut close = false;
            let notice = self.illust_ui.animate_notice.clone();
            egui::Window::new(t.illust_animate_done)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 28.0))
                .show(ctx, |ui| {
                    ui.label(&notice);
                    if ui.button("OK").clicked() {
                        close = true;
                    }
                });
            if close {
                self.illust_ui.animate_notice.clear();
            }
        }
    }
}

fn pose_at(u: f32) -> IllustrationPose {
    let phase = u.clamp(0.0, 1.0) * std::f32::consts::TAU * 1.5;
    IllustrationPose {
        twitch: phase.sin().abs(),
        tilt: phase.sin() * 0.9,
        flap: (phase * 2.0).sin().abs() * 0.7,
        walk: (phase * 0.5).sin() * 0.35,
        ..Default::default()
    }
}

fn png_bytes_to_color_image(bytes: &[u8]) -> Option<egui::ColorImage> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
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

#[cfg(test)]
mod image_choice_tests {
    use super::*;

    #[test]
    fn image_history_distinguishes_rejected_pending_and_selected() {
        let mut doc = IllustrationDoc::default();
        doc.last_png = Some("source.png".into());
        doc.image_run = Some(serde_json::from_value(serde_json::json!({
            "id":"edit", "source_revision":2, "phase":"final", "status":"needs_review",
            "source_png":"source.png", "candidate_png":"candidate.png"
        })).unwrap());
        assert_eq!(preview_choice(&doc, "candidate.png"), PreviewChoice::Pending);
        assert_eq!(preview_choice(&doc, "source.png"), PreviewChoice::Retained);
        assert_eq!(preview_choice(&doc, "old.png"), PreviewChoice::Historical);
        doc.image_run.as_mut().unwrap().candidate_selected = Some(false);
        assert_eq!(preview_choice(&doc, "candidate.png"), PreviewChoice::Rejected);
        doc.image_run.as_mut().unwrap().candidate_selected = Some(true);
        doc.last_png = Some("candidate.png".into());
        assert_eq!(preview_choice(&doc, "candidate.png"), PreviewChoice::Retained);
        assert_eq!(preview_choice(&doc, "source.png"), PreviewChoice::Historical);
    }
}
