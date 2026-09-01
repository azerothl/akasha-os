//! Event handlers for session navigation and room-turn lifecycle.

use crate::chat_canvas;
use crate::chat_room;
use crate::cmd::{ChatLine, Cmd};
use crate::{designer_shot_mode, session_nav, UiApp};
use aos_proto::ChatSessionMeta;

pub(crate) fn on_sessions(app: &mut UiApp, sessions: Vec<ChatSessionMeta>) {
    app.chat_state.sessions = sessions;
}

pub(crate) fn on_load_intent(app: &mut UiApp, id: String) {
    session_nav::apply_session_load_intent(&mut app.pending_session_nav, &id);
}

pub(crate) fn on_loaded(app: &mut UiApp, id: String, messages: Vec<ChatLine>, meta: ChatSessionMeta) {
    let session_changed = app.chat_state.active_session.as_deref() != Some(id.as_str());
    if session_changed
        && !session_nav::should_switch_session_view(
            app.chat_state.active_session.as_deref(),
            &app.pending_session_nav,
            id.as_str(),
        )
    {
        if let Some(session) = app.chat_state.sessions.iter_mut().find(|s| s.id == meta.id) {
            *session = meta;
        }
        return;
    }

    if !session_changed
        && !session_nav::should_replace_chat_on_same_session_reload(
            app.schedule_ui.transcript_dirty,
        )
    {
        app.chat_state.sidebar.rename = meta.title.clone();
        if let Some(session) = app.chat_state.sessions.iter_mut().find(|s| s.id == meta.id) {
            *session = meta;
        }
        app.sync_schedule_cards();
        app.pending_session_nav = session_nav::PendingSessionNav::None;
        return;
    }

    app.pending_session_nav = session_nav::PendingSessionNav::None;
    app.chat_state.active_session = Some(id.clone());
    app.chat_state.sidebar.rename = meta.title.clone();
    if let Some(session) = app.chat_state.sessions.iter_mut().find(|s| s.id == meta.id) {
        *session = meta.clone();
    }
    if session_changed {
        app.chat_state.view.room_members_open = false;
        let mut chat = Vec::new();
        if !designer_shot_mode() {
            chat.push(ChatLine::plain(
                "système",
                format!("Session {id} — historique rechargé."),
            ));
        }
        chat.extend(messages);
        app.chat = chat;
    } else {
        app.chat = messages;
    }
    app.sync_schedule_cards();
    let _ = app.cmd_tx.send(Cmd::SkillPassPending);
    app.chat_state.session_chat.clear_unread(&id);
    app.chat_state.session_chat.sync_active_view(
        app.chat_state.active_session.as_deref(),
        &mut app.chat_state.runtime.streaming,
        &mut app.chat_state.runtime.pending,
        &mut app.chat_state.runtime.inference_id,
    );
    app.chat_state.runtime.room_turn_text = None;
    if meta.canvas_open {
        let _ = app.cmd_tx.send(Cmd::CanvasPoll {
            session_id: id,
            after_seq: None,
        });
    } else {
        app.chat_state.view.canvas = chat_canvas::CanvasPanelState::default();
    }
}

pub(crate) fn on_room_turn_done(
    app: &mut UiApp,
    session_id: String,
    agent_turns: u32,
    cancelled: bool,
) {
    app.chat_state.session_chat.finish_turn(&session_id);
    if app.chat_state.active_session.as_deref() == Some(session_id.as_str()) {
        app.chat_state.runtime.pending = false;
        app.chat_state.runtime.inference_id = None;
        app.chat_state.runtime.room_turn_text = None;
        if let Some(status) = chat_room::room_turn_done_status(agent_turns, cancelled) {
            app.status = status;
        } else if app.status.starts_with("salon :") {
            app.status.clear();
        }
    } else if !cancelled && agent_turns > 0 {
        app.chat_state.session_chat.mark_unread(&session_id);
    }
}
