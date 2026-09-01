//! Event handlers for image, video, and audio generation updates.

use crate::{image_studio, ChatAttachment, ChatLine, UiApp};

pub(crate) struct MediaOkEvent {
    pub(crate) kind: String,
    pub(crate) path: String,
    pub(crate) bytes: u64,
    pub(crate) engine: String,
    pub(crate) prompt: String,
    pub(crate) generation_prompt: Option<String>,
    pub(crate) composition_blocks: Vec<crate::image_composition::CompositionBlock>,
}

pub(crate) fn on_image_enriched(app: &mut UiApp, enriched: String) {
    app.image_studio.set_enriched_prompt(&enriched);
    app.status = "Image: enhanced prompt ready, generating…".into();
}

pub(crate) fn on_image_started(
    app: &mut UiApp,
    enriching: bool,
    upscaling: bool,
    total_steps: u32,
) {
    app.image_generating = Some(image_studio::ImageGenUiState {
        enriching,
        upscaling,
        step: 0,
        total_steps,
        elapsed_secs: 0,
    });
    if enriching {
        app.status = "Image: rewriting prompt…".into();
    } else {
        app.status = format!("Image: generating ({total_steps} steps)…");
    }
}

pub(crate) fn on_image_progress(
    app: &mut UiApp,
    enriching: bool,
    upscaling: bool,
    step: u32,
    total_steps: u32,
    elapsed_secs: u64,
) {
    app.image_generating = Some(image_studio::ImageGenUiState {
        enriching,
        upscaling,
        step,
        total_steps,
        elapsed_secs,
    });
    if enriching {
        app.status = format!("Image: rewriting prompt… ({elapsed_secs}s)");
    } else if upscaling {
        app.status = format!("Image: upscaling… ({elapsed_secs}s)");
    } else if step > 0 && total_steps > 0 {
        app.status = format!("Image: step {step}/{total_steps} ({elapsed_secs}s)");
    } else {
        app.status = format!(
            "Image: generating ({total_steps} steps, {elapsed_secs}s)…"
        );
    }
}

pub(crate) fn on_media_ok(app: &mut UiApp, event: MediaOkEvent) {
    let MediaOkEvent {
        kind,
        path,
        bytes,
        engine,
        prompt,
        generation_prompt,
        composition_blocks,
    } = event;
    app.image_generating = None;
    app.status = format!("{kind} → {path} ({bytes} bytes, {engine})");
    let att = if kind == "audio" {
        ChatAttachment::Audio { path: path.clone() }
    } else {
        ChatAttachment::Image {
            path: path.clone(),
            prompt: prompt.clone(),
        }
    };
    if kind == "image" || kind == "video" {
        if kind == "image" {
            app.chat_state.composer.last_session_image = Some(path.clone());
            if prompt.is_empty() {
                app.image_studio.preview = Some(path.clone());
                app.image_studio.apply_history_for_path(&path);
            } else {
                app.image_studio.open_from_chat(
                    &prompt,
                    &path,
                    generation_prompt.as_deref(),
                );
                if !composition_blocks.is_empty() {
                    app.image_studio.set_composition_blocks(composition_blocks);
                } else {
                    app.image_studio.apply_history_for_path(&path);
                }
            }
        } else {
            app.image_studio.on_video_generated(path.clone());
        }
        app.tab = crate::Tab::Image;
    }
    let note = if engine == "stub" {
        format!("{kind}: {path}\n(stub — pack média ou moteur sd.cpp/piper absent)")
    } else {
        format!("{kind}: {path} ({engine})")
    };
    app.chat.push(ChatLine {
        role: "assistant".into(),
        text: note.clone(),
        attachments: vec![att.clone()],
        speaker_id: None,
        speaker_name: None,
        thinking: None,
    });
    if let Some(sid) = app.chat_state.active_session.clone() {
        let _ = app.cmd_tx.send(crate::cmd::Cmd::SessionAppend {
            session_id: sid,
            role: "assistant".into(),
            content: note,
            attachments: vec![att],
        });
    }
}
