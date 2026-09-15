//! Event handlers for the direct chat lifecycle.

use crate::chat_error_copy;
use crate::cmd::{ChatLine, Cmd};
use crate::ui_format::now_ms;
use crate::{session_chat, UiApp};
use aos_proto::ChatAttachment;

fn persist_system_line(app: &mut UiApp, content: String) {
    if let Some(session_id) = app.chat_state.active_session.clone() {
        let _ = app.cmd_tx.send(Cmd::SessionAppend {
            session_id,
            role: "system".into(),
            content,
            attachments: vec![],
        });
    }
}

fn record_classified_chat_error(
    app: &mut UiApp,
    session_id: Option<String>,
    classified: &chat_error_copy::ChatErrorClassified,
    visible: &str,
) {
    let session_for_audit = session_id.clone();
    app.security_ui.record_chat_error(
        session_id,
        classified.code,
        visible.to_string(),
        now_ms(),
    );
    let _ = app.cmd_tx.send(Cmd::AuditAppend {
        action: "chat.error".into(),
        target: classified.code.to_string(),
        detail: serde_json::json!({
            "session_id": session_for_audit,
            "code": classified.code,
            "message": visible,
        }),
    });
}

fn push_system_chrome(app: &mut UiApp, content: String, persist: bool) {
    app.chat.push(ChatLine::plain("système", content.clone()));
    if persist {
        persist_system_line(app, content);
    }
}

/// Push system chrome to the transcript and persist it for export / reload.
pub(crate) fn push_persisted_system_chrome(app: &mut UiApp, content: String) {
    push_system_chrome(app, content, true);
}

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
    let model_id = app
        .chat_state
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .and_then(|session| session.model_id.clone())
        .filter(|id| !id.trim().is_empty())
        .or_else(|| {
            app.prefs
                .default_agent_model
                .clone()
                .filter(|id| !id.trim().is_empty())
        });
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
        model_id,
    );
    app.chat_state.runtime.load_fail_retry = None;
    if app.status.starts_with("assistant :") {
        app.status.clear();
    }
    // S2 : clôture le turn facturable (prompt enregistré au begin_turn).
    app.note_provider_completion(&session_id, &text);
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
    let classified = chat_error_copy::classify_chat_error(&t, &message);
    let visible = chat_error_copy::format_chat_error(&classified);
    let session_id = app.chat_state.active_session.clone();
    record_classified_chat_error(app, session_id.clone(), &classified, &visible);
    app.push_status(visible.clone());
    app.toasts.push_error(visible.clone());
    push_system_chrome(app, visible, true);
    false
}

/// Clear the chat turn that emitted the failure. Generic runtime failures
/// (downloads, settings, etc.) must not unlock an unrelated chat.
pub(crate) fn on_chat_error(app: &mut UiApp, session_id: String, message: String) {
    let t = crate::i18n::strings(&app.prefs.language);
    let load_fail = chat_error_copy::is_model_load_fail_error(&message);
    let classified = chat_error_copy::classify_chat_error(&t, &message);
    let visible = chat_error_copy::format_chat_error(&classified);
    let partial = app.chat_state.runtime.streaming.clone();

    app.chat_state.session_chat.finish_turn(&session_id);
    if app.chat_state.active_session.as_deref() == Some(session_id.as_str()) {
        let retry_turn = app.chat_state.runtime.outgoing_turn.take();
        app.chat_state.runtime.finish_turn();
        if !partial.trim().is_empty() {
            app.chat_state.runtime.load_fail_retry = None;
            if let Some(retry) = retry_turn {
                app.offer_partial_continuation(retry, partial);
            }
        } else if load_fail {
            app.chat_state.runtime.load_fail_retry = retry_turn;
        } else {
            app.chat_state.runtime.load_fail_retry = None;
        }
        if app.status.starts_with("assistant :") {
            app.status.clear();
        }
        app.status = visible.clone();
        app.toasts.push_error(visible.clone());
        record_classified_chat_error(
            app,
            Some(session_id.clone()),
            &classified,
            &visible,
        );
        // Log historique même quand le transcript porte déjà le message.
        app.status_history.push_back(visible.clone());
        while app.status_history.len() > 20 {
            app.status_history.pop_front();
        }
        if load_fail {
            // Chrome card carries the user-visible copy + Retry.
        } else {
            push_system_chrome(app, visible, true);
        }
    } else {
        app.chat_state.runtime.outgoing_turn = None;
        if !load_fail {
            app.chat_state.runtime.load_fail_retry = None;
        }
        app.chat_state.session_chat.mark_unread(&session_id);
        record_classified_chat_error(
            app,
            Some(session_id.clone()),
            &classified,
            &visible,
        );
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
