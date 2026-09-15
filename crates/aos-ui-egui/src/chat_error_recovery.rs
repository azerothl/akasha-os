//! Chat turn failure chrome (headline + cause + Retry / Activity).

use crate::cmd::ChatRetryTurn;
use crate::i18n::UiStrings;
use eframe::egui;

#[derive(Debug, Clone)]
pub(crate) struct ChatErrorRecovery {
    pub code: String,
    pub cause: String,
    pub retry_turn: Option<ChatRetryTurn>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChatErrorRecoveryAction {
    None,
    Retry,
    Activity,
}

/// Inline recovery for a classified chat failure (Designer chrome #245).
pub(crate) fn render_chat_error_recovery(
    ui: &mut egui::Ui,
    t: &UiStrings,
    recovery: &ChatErrorRecovery,
) -> ChatErrorRecoveryAction {
    let mut action = ChatErrorRecoveryAction::None;
    ui.group(|ui| {
        ui.label(t.chat_error_generic);
        if recovery.cause != t.chat_error_generic {
            ui.label(&recovery.cause);
        }
        ui.horizontal(|ui| {
            if recovery.retry_turn.is_some() {
                if ui.button(t.chat_error_retry).clicked() {
                    action = ChatErrorRecoveryAction::Retry;
                }
            }
            if ui.button(t.chat_error_activity).clicked() {
                action = ChatErrorRecoveryAction::Activity;
            }
        });
    });
    action
}
