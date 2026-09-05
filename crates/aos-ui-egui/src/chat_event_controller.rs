//! Event handlers for the direct chat lifecycle.

use crate::chat_error_copy;
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
    app.chat_state.runtime.load_fail_retry = None;
    if app.status.starts_with("assistant :") {
        app.status.clear();
    }
    app.mark_onboarding_chat_done();
}

/// Handle an error and return whether the event loop should stop processing.
pub(crate) fn on_error(app: &mut UiApp, message: String) -> bool {
    if aos_agent::context_budget::is_technical_vision_infer_error(&message) {
        return true;
    }
    if message.contains("media.image") || message.starts_with("Image:") {
        app.image_generating = None;
    }
    let t = crate::i18n::strings(&app.prefs.language);
    let visible = chat_error_copy::user_visible_chat_error(&t, &message);
    app.push_status(visible.clone());
    app.toasts.push_error(visible.clone());
    app.chat.push(ChatLine::plain("système", visible));
    false
}

/// Clear the chat turn that emitted the failure. Generic runtime failures
/// (downloads, settings, etc.) must not unlock an unrelated chat.
pub(crate) fn on_chat_error(app: &mut UiApp, session_id: String, message: String) {
    let t = crate::i18n::strings(&app.prefs.language);
    let load_fail = chat_error_copy::is_model_load_fail_error(&message);
    let visible = chat_error_copy::user_visible_chat_error(&t, &message);

    app.chat_state.session_chat.finish_turn(&session_id);
    if app.chat_state.active_session.as_deref() == Some(session_id.as_str()) {
        let retry_turn = app.chat_state.runtime.outgoing_turn.take();
        app.chat_state.runtime.finish_turn();
        if load_fail {
            app.chat_state.runtime.load_fail_retry = retry_turn;
        } else {
            app.chat_state.runtime.load_fail_retry = None;
        }
        if app.status.starts_with("assistant :") {
            app.status.clear();
        }
        app.status = visible.clone();
        app.toasts.push_error(visible.clone());
        // Log historique même quand le transcript porte déjà le message.
        app.status_history.push_back(visible.clone());
        while app.status_history.len() > 20 {
            app.status_history.pop_front();
        }
        if load_fail {
            // Chrome card carries the user-visible copy + Retry.
        } else {
            app.chat.push(ChatLine::plain("système", visible));
        }
    } else {
        app.chat_state.runtime.outgoing_turn = None;
        if !load_fail {
            app.chat_state.runtime.load_fail_retry = None;
        }
        app.chat_state.session_chat.mark_unread(&session_id);
    }
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
        app.push_status(message);
    }
}
