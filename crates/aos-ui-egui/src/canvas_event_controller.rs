//! Event handlers for canvas session metadata, snapshots, and exports.

use crate::{ChatAttachment, ChatLine, UiApp};
use aos_proto::{CanvasLayer, CanvasOp, CanvasPenStyle, ChatSessionMeta};
use eframe::egui;

pub(crate) struct CanvasSnapshotEvent {
    pub(crate) session_id: String,
    pub(crate) canvas_open: bool,
    pub(crate) next_seq: u64,
    pub(crate) ops: Vec<CanvasOp>,
    pub(crate) pen: CanvasPenStyle,
    pub(crate) delta: bool,
    pub(crate) canvas_seeing: Option<bool>,
    pub(crate) layers: Vec<CanvasLayer>,
    pub(crate) active_layer_id: String,
}

pub(crate) fn on_canvas_meta(app: &mut UiApp, meta: ChatSessionMeta) {
    if let Some(session) = app.chat_state.sessions.iter_mut().find(|s| s.id == meta.id) {
        *session = meta;
    }
}

pub(crate) fn on_canvas_snapshot(app: &mut UiApp, ctx: &egui::Context, event: CanvasSnapshotEvent) {
    let CanvasSnapshotEvent {
        session_id,
        canvas_open,
        next_seq,
        ops,
        pen,
        delta,
        canvas_seeing,
        layers,
        active_layer_id,
    } = event;
    if let Some(session) = app
        .chat_state
        .sessions
        .iter_mut()
        .find(|s| s.id == session_id)
    {
        session.canvas_open = canvas_open;
    }
    if app.chat_state.active_session.as_deref() != Some(session_id.as_str()) {
        return;
    }
    let now = ctx.input(|i| i.time);
    if delta {
        if ops.is_empty() && next_seq > app.chat_state.view.canvas.last_seen_seq {
            // A clear advances the server cursor without producing a drawable
            // op. Treat that empty delta as a reset instead of retaining the
            // stale local scene until the canvas is reopened.
            app.chat_state.view.canvas.apply_snapshot(Vec::new(), next_seq, now);
        } else {
            app.chat_state.view.canvas.merge_delta(ops, next_seq, now);
        }
    } else {
        app.chat_state
            .view
            .canvas
            .apply_snapshot(ops, next_seq, now);
    }
    app.chat_state.view.canvas.sync_pen(&pen);
    if !layers.is_empty() {
        app.chat_state
            .view
            .canvas
            .sync_layers(layers, active_layer_id);
    }
    if let Some(seeing) = canvas_seeing {
        app.chat_state.view.canvas.seeing = seeing;
    }
}

pub(crate) fn on_canvas_exported(app: &mut UiApp, path: String, session_id: String) {
    if app.chat_state.active_session.as_deref() != Some(session_id.as_str()) {
        return;
    }
    let t = crate::i18n::strings(&app.prefs.language);
    app.status = t.canvas_export_status.replace("{path}", &path);
    app.chat_state.composer.last_session_image = Some(path.clone());
    app.chat.push(ChatLine {
        role: "assistant".into(),
        text: t.canvas_exported.replace("{path}", &path),
        attachments: vec![ChatAttachment::Image {
            path,
            prompt: "canvas export".into(),
        }],
        speaker_id: None,
        speaker_name: None,
        thinking: None,
    });
}
