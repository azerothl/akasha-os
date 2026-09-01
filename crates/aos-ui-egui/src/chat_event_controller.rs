//! Event handlers for the direct chat lifecycle.

use crate::cmd::ChatLine;
use crate::{session_chat, UiApp};
use aos_proto::ChatAttachment;

pub(crate) fn on_delta(app: &mut UiApp, session_id: String, text: String) {
    session_chat::on_delta(
        &mut app.chat_state.session_chat,
        app.chat_state.active_session.as_deref(),
        &session_id,
        &text,
        &mut app.chat_state.runtime.streaming,
    );
}

pub(crate) fn on_done(
    app: &mut UiApp,
    text: String,
    session_id: String,
    attachments: Vec<ChatAttachment>,
) {
    session_chat::on_done(
        &mut app.chat_state.session_chat,
        app.chat_state.active_session.as_deref(),
        &session_id,
        &text,
        attachments,
        &mut app.chat,
        &mut app.chat_state.runtime.streaming,
        &mut app.chat_state.runtime.pending,
        &mut app.chat_state.runtime.inference_id,
    );
    if app.status.starts_with("assistant :") {
        app.status.clear();
    }
    app.mark_onboarding_chat_done();
}

/// Handle an error and return whether the event loop should stop processing.
pub(crate) fn on_error(app: &mut UiApp, message: String) -> bool {
    if aos_agent::context_budget::is_technical_vision_infer_error(&message) {
        app.chat_state.runtime.finish_turn();
        if app.status.starts_with("assistant :") {
            app.status.clear();
        }
        return true;
    }
    if message.contains("media.image") || message.starts_with("Image:") {
        app.image_generating = None;
    }
    app.status = message.clone();
    app.chat.push(ChatLine::plain("système", message));
    app.chat_state.runtime.finish_turn();
    false
}

pub(crate) fn on_status(app: &mut UiApp, message: String) {
    if let Some(id) = message.strip_prefix("model removed:") {
        app.on_model_removed(id.trim().to_string());
    }
    if message == format!("{} ok", aos_agent::intents::KILL)
        && app.agent_ui.consume_document_prep_kill_ok()
    {
        // swallow kill-ok banner for document-prep stop
    } else {
        app.status = message;
    }
}
