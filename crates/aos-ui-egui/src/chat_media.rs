//! Chat PNG studio control + inline TTS card (P09.8).

use crate::cmd::Cmd;
use crate::decl_ui;
use crate::i18n::UiStrings;
use aos_proto::{ChatAttachment, MediaAudioOptions};
use eframe::egui;
use std::sync::mpsc::Sender;

pub fn render_image(
    ui: &mut egui::Ui,
    t: &UiStrings,
    path: &str,
    prompt: &str,
    on_studio: impl FnOnce(),
) {
    ui.label(format!("image: {path}"));
    if !prompt.is_empty() {
        ui.weak(prompt);
    }
    if let Some(tex) = decl_ui::try_load_png(ui.ctx(), path) {
        let [tw, th] = tex.size();
        let max_w = ui.available_width().min(512.0);
        let scale = if tw.max(th) < 48 {
            256.0 / tw.max(1) as f32
        } else {
            (max_w / tw.max(1) as f32).min(1.0)
        };
        ui.add(egui::Image::new(&tex).fit_to_original_size(scale));
    } else {
        ui.weak("PNG unreadable (path /downloads → var/storage/data)");
    }
    if ui.button(t.studio_open).clicked() {
        on_studio();
    }
}

pub fn render_pending_image_chips(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    paths: &mut Vec<String>,
) {
    let mut remove_idx = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
        for (i, path) in paths.iter().enumerate() {
            egui::Frame::new()
                .fill(ui.visuals().widgets.inactive.bg_fill)
                .corner_radius(0.0)
                .inner_margin(egui::Margin::symmetric(4, 2))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if let Some(tex) = try_load_chat_image(ctx, path) {
                            ui.add(
                                egui::Image::new(&tex)
                                    .max_size(egui::vec2(28.0, 28.0))
                                    .corner_radius(0.0),
                            );
                        } else {
                            ui.allocate_space(egui::vec2(28.0, 28.0));
                        }
                        if ui
                            .add(
                                egui::Label::new(egui::RichText::new("×").weak())
                                    .sense(egui::Sense::click()),
                            )
                            .clicked()
                        {
                            remove_idx = Some(i);
                        }
                    });
                });
        }
    });
    if let Some(i) = remove_idx {
        paths.remove(i);
    }
}

/// Pending image + document chips in one wrapped row (composer chrome).
pub fn render_pending_attachment_chips(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    image_paths: &mut Vec<String>,
    documents: &mut Vec<aos_proto::DocumentRef>,
) {
    if image_paths.is_empty() && documents.is_empty() {
        return;
    }
    let mut remove_image = None;
    let mut remove_doc = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
        for (i, path) in image_paths.iter().enumerate() {
            egui::Frame::new()
                .fill(ui.visuals().widgets.inactive.bg_fill)
                .corner_radius(0.0)
                .inner_margin(egui::Margin::symmetric(4, 2))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if let Some(tex) = try_load_chat_image(ctx, path) {
                            ui.add(
                                egui::Image::new(&tex)
                                    .max_size(egui::vec2(28.0, 28.0))
                                    .corner_radius(0.0),
                            );
                        } else {
                            ui.allocate_space(egui::vec2(28.0, 28.0));
                        }
                        if ui
                            .add(
                                egui::Label::new(egui::RichText::new("×").weak())
                                    .sense(egui::Sense::click()),
                            )
                            .clicked()
                        {
                            remove_image = Some(i);
                        }
                    });
                });
        }
        for (i, doc) in documents.iter().enumerate() {
            egui::Frame::new()
                .fill(ui.visuals().widgets.inactive.bg_fill)
                .corner_radius(0.0)
                .inner_margin(egui::Margin::symmetric(4, 2))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.label(egui::RichText::new(doc.label.as_str()).size(12.0));
                        if ui
                            .add(
                                egui::Label::new(egui::RichText::new("×").weak())
                                    .sense(egui::Sense::click()),
                            )
                            .clicked()
                        {
                            remove_doc = Some(i);
                        }
                    });
                });
        }
    });
    if let Some(i) = remove_image {
        image_paths.remove(i);
    }
    if let Some(i) = remove_doc {
        documents.remove(i);
    }
}

pub fn render_document(ui: &mut egui::Ui, t: &UiStrings, label: &str, path: &str) {
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.button(t.studio_open_file).clicked() {
            let _ = decl_ui::open_host_path(path);
        }
    });
}

fn try_load_chat_image(ctx: &egui::Context, logical: &str) -> Option<egui::TextureHandle> {
    let path = decl_ui::host_file_from_logical(logical);
    let bytes = std::fs::read(&path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    Some(ctx.load_texture(
        format!("chat-pending:{logical}"),
        color,
        egui::TextureOptions::LINEAR,
    ))
}

pub fn render_audio(ui: &mut egui::Ui, path: &str) {
    ui.horizontal(|ui| {
        ui.label(format!("audio: {path}"));
        if ui.button("Play").clicked() {
            let _ = decl_ui::open_host_path(path);
        }
    });
}

/// Returns true when Generate was clicked (caller should persist replacements).
pub fn render_tts_card(
    ui: &mut egui::Ui,
    t: &UiStrings,
    cmd: &Sender<Cmd>,
    att: &mut ChatAttachment,
    piper_ids: &[String],
) -> bool {
    let ChatAttachment::TtsDraft {
        text,
        model_id,
        options,
    } = att
    else {
        return false;
    };
    let mut generate = false;
    ui.group(|ui| {
        ui.strong("TTS");
        ui.label(t.tts_card_blurb);
        ui.label(text.as_str());
        ui.horizontal(|ui| {
            ui.label(t.settings_piper_voice);
            let shown = model_id.as_deref().unwrap_or("default");
            egui::ComboBox::from_id_salt(format!("tts_voice_{text}"))
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(model_id.is_none(), "default").clicked() {
                        *model_id = None;
                    }
                    for id in piper_ids {
                        if ui
                            .selectable_label(model_id.as_deref() == Some(id.as_str()), id)
                            .clicked()
                        {
                            *model_id = Some(id.clone());
                        }
                    }
                });
        });
        knobs(ui, options);
        if ui.button(t.tts_generate).clicked() {
            let _ = cmd.send(Cmd::MediaAudio {
                text: text.clone(),
                model_id: model_id.clone(),
                options: options.clone(),
            });
            generate = true;
        }
    });
    generate
}

fn knobs(ui: &mut egui::Ui, o: &mut MediaAudioOptions) {
    ui.horizontal(|ui| {
        let mut len = o.length_scale.unwrap_or(1.0);
        ui.label("length");
        if ui
            .add(egui::DragValue::new(&mut len).range(0.5..=2.0).speed(0.05))
            .changed()
        {
            o.length_scale = Some(len);
        }
        let mut noise = o.noise_scale.unwrap_or(0.667);
        ui.label("noise");
        if ui
            .add(egui::DragValue::new(&mut noise).range(0.0..=1.5).speed(0.01))
            .changed()
        {
            o.noise_scale = Some(noise);
        }
        let mut w = o.noise_w.unwrap_or(0.8);
        ui.label("noise_w");
        if ui
            .add(egui::DragValue::new(&mut w).range(0.0..=1.5).speed(0.01))
            .changed()
        {
            o.noise_w = Some(w);
        }
        let mut sil = o.sentence_silence.unwrap_or(0.2);
        ui.label("silence");
        if ui
            .add(egui::DragValue::new(&mut sil).range(0.0..=2.0).speed(0.05))
            .changed()
        {
            o.sentence_silence = Some(sil);
        }
        let mut spk = o.speaker.unwrap_or(0);
        ui.label("speaker");
        if ui.add(egui::DragValue::new(&mut spk).range(0..=16)).changed() {
            o.speaker = Some(spk);
        }
    });
}
