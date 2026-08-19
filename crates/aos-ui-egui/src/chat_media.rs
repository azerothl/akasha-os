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
