//! Chat PNG studio control + inline TTS card (P09.8).

use crate::cmd::Cmd;
use crate::decl_ui;
use crate::i18n::UiStrings;
use crate::icons;
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

pub fn render_pending_image_chips(ui: &mut egui::Ui, ctx: &egui::Context, paths: &mut Vec<String>) {
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
                        if icons::close_button(ui).clicked() {
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
                        if icons::close_button(ui).clicked() {
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
                        if icons::close_button(ui).clicked() {
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

pub fn render_audio(ui: &mut egui::Ui, t: &UiStrings, path: &str) {
    ui.horizontal(|ui| {
        ui.label(format!("{} : {path}", t.models_tab_audio));
        let play_label = if t.studio_open_file.starts_with("Ouvrir") {
            "Lire"
        } else {
            "Play"
        };
        if ui
            .button(play_label)
            .on_hover_text(if play_label == "Lire" {
                "Lit le WAV avec le lecteur système"
            } else {
                "Play WAV with the system player"
            })
            .clicked()
        {
            let _ = play_audio(path);
        }
        if ui
            .button(t.studio_open_file)
            .on_hover_text("Ouvre le fichier avec le lecteur audio du système")
            .clicked()
        {
            let _ = decl_ui::open_host_path(path);
        }
    });
    if let Some(info) = inspect_media(path) {
        ui.weak(info);
    }
}

/// Render a generated video as a first-class attachment. Preview's bundled
/// writer emits WebM/AVI/WebP; the card exposes codec/container/size metadata
/// and keeps the OS opener as a fallback when no embedded decoder is present.
pub fn render_video(ui: &mut egui::Ui, t: &UiStrings, path: &str, prompt: &str) {
    ui.label(format!("{} : {path}", t.studio_create_mode_video));
    if !prompt.is_empty() {
        ui.weak(prompt);
    }
    if let Some(info) = inspect_media(path) {
        ui.weak(info);
    }
    ui.horizontal(|ui| {
        if ui
            .button(t.studio_open_file)
            .on_hover_text("Ouvre le WebM avec le lecteur vidéo du système")
            .clicked()
        {
            let _ = decl_ui::open_host_path(path);
        }
    });
}

fn play_audio(logical: &str) -> std::io::Result<()> {
    let path = decl_ui::host_file_from_logical(logical);
    #[cfg(target_os = "windows")]
    {
        // SoundPlayer is part of Windows and handles the PCM WAV produced by
        // Piper without adding a native audio dependency to the UI binary.
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-WindowStyle",
                "Hidden",
                "-Command",
                &format!(
                    "(New-Object Media.SoundPlayer '{}').PlaySync()",
                    path.to_string_lossy().replace('\'', "''")
                ),
            ])
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("afplay").arg(path).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        let player = if std::process::Command::new("aplay")
            .arg("--version")
            .output()
            .is_ok()
        {
            "aplay"
        } else {
            "paplay"
        };
        std::process::Command::new(player).arg(path).spawn()?;
    }
    Ok(())
}

pub(crate) fn inspect_media(logical: &str) -> Option<String> {
    let path = decl_ui::host_file_from_logical(logical);
    let bytes = std::fs::read(&path).ok()?;
    let size = bytes.len();
    if bytes.len() >= 44 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        let channels = u16::from_le_bytes([bytes[22], bytes[23]]);
        let rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
        let bits = u16::from_le_bytes([bytes[34], bytes[35]]);
        let data_bytes = find_wav_data_bytes(&bytes).unwrap_or(0);
        let duration = if channels > 0 && rate > 0 && bits > 0 {
            data_bytes as f64 / (rate as f64 * channels as f64 * bits as f64 / 8.0)
        } else {
            0.0
        };
        return Some(format!(
            "WAV · {channels} ch · {rate} Hz · {bits} bit · {:.2} s · {size} octets",
            duration
        ));
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"AVI " {
        return Some(format!("AVI · durée à lire · {size} octets"));
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some(format!("WebP animé · durée à lire · {size} octets"));
    }
    if bytes.len() >= 4 && &bytes[..4] == b"\x1a\x45\xdf\xa3" {
        let duration = webm_duration_seconds(&bytes);
        return Some(match duration {
            Some(seconds) => format!("WebM/EBML · {:.2} s · {size} octets", seconds),
            None => format!("WebM/EBML · durée à lire · {size} octets"),
        });
    }
    None
}

fn find_wav_data_bytes(bytes: &[u8]) -> Option<usize> {
    let mut i = 12;
    while i + 8 <= bytes.len() {
        let len = u32::from_le_bytes(bytes.get(i + 4..i + 8)?.try_into().ok()?) as usize;
        if &bytes[i..i + 4] == b"data" {
            return Some(len.min(bytes.len().saturating_sub(i + 8)));
        }
        i = i.saturating_add(8 + len + (len & 1));
    }
    None
}

fn webm_duration_seconds(bytes: &[u8]) -> Option<f64> {
    // Matroska stores Duration as a float and TimecodeScale as an integer.
    // A bounded scan is sufficient for the small metadata headers emitted by
    // Preview and avoids pulling a full container parser into the UI binary.
    let mut scale = 1_000_000f64;
    if let Some((offset, len)) = find_ebml_element(bytes, &[0x2a, 0xd7, 0xb1]) {
        if (1..=8).contains(&len) && offset + len <= bytes.len() {
            let mut value = 0u64;
            for byte in &bytes[offset..offset + len] {
                value = (value << 8) | u64::from(*byte);
            }
            if value > 0 {
                scale = value as f64;
            }
        }
    }
    let (offset, len) = find_ebml_element(bytes, &[0x44, 0x89])?;
    let value = match len {
        4 => f32::from_be_bytes(bytes.get(offset..offset + 4)?.try_into().ok()?) as f64,
        8 => f64::from_be_bytes(bytes.get(offset..offset + 8)?.try_into().ok()?),
        _ => return None,
    };
    Some(value * scale / 1_000_000_000.0)
}

fn find_ebml_element(bytes: &[u8], id: &[u8]) -> Option<(usize, usize)> {
    let start = bytes.windows(id.len()).position(|window| window == id)? + id.len();
    let (size, width) = read_ebml_vint(bytes.get(start..)?);
    let offset = start + width;
    Some((offset, size? as usize))
}

fn read_ebml_vint(bytes: &[u8]) -> (Option<u64>, usize) {
    let Some(&first) = bytes.first() else {
        return (None, 0);
    };
    let Some(width) = (1..=8).find(|w| first & (0x80 >> (w - 1)) != 0) else {
        return (None, 0);
    };
    if bytes.len() < width {
        return (None, width);
    }
    let mut value = u64::from(first & (0x7f >> (width - 1)));
    for byte in &bytes[1..width] {
        value = (value << 8) | u64::from(*byte);
    }
    (Some(value), width)
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
            .add(
                egui::DragValue::new(&mut noise)
                    .range(0.0..=1.5)
                    .speed(0.01),
            )
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
        if ui
            .add(egui::DragValue::new(&mut spk).range(0..=16))
            .changed()
        {
            o.speaker = Some(spk);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::inspect_media;

    #[test]
    fn wav_metadata_reports_duration_and_format() {
        let path = std::env::temp_dir().join(format!("aos-ui-media-{}.wav", std::process::id()));
        let mut bytes = vec![0u8; 44 + 2205 * 2];
        bytes[..4].copy_from_slice(b"RIFF");
        bytes[8..12].copy_from_slice(b"WAVE");
        bytes[12..16].copy_from_slice(b"fmt ");
        bytes[16..20].copy_from_slice(&16u32.to_le_bytes());
        bytes[20..22].copy_from_slice(&1u16.to_le_bytes());
        bytes[22..24].copy_from_slice(&1u16.to_le_bytes());
        bytes[24..28].copy_from_slice(&22050u32.to_le_bytes());
        bytes[28..32].copy_from_slice(&44100u32.to_le_bytes());
        bytes[32..34].copy_from_slice(&2u16.to_le_bytes());
        bytes[34..36].copy_from_slice(&16u16.to_le_bytes());
        bytes[36..40].copy_from_slice(b"data");
        bytes[40..44].copy_from_slice(&(2205u32 * 2).to_le_bytes());
        std::fs::write(&path, bytes).unwrap();
        let info = inspect_media(&path.to_string_lossy()).unwrap();
        assert!(info.contains("WAV"));
        assert!(info.contains("22050 Hz"));
        assert!(info.contains("0.10 s"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn webm_metadata_reports_container_without_decoding_as_png() {
        let path = std::env::temp_dir().join(format!("aos-ui-media-{}.webm", std::process::id()));
        std::fs::write(&path, b"\x1a\x45\xdf\xa3\x00").unwrap();
        let info = inspect_media(&path.to_string_lossy()).unwrap();
        assert!(info.contains("WebM/EBML"));
        let _ = std::fs::remove_file(path);
    }
}
