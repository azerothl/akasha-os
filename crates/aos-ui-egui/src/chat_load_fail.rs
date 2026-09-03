//! Load-fail chrome in the chat transcript (message + Retry).

use eframe::egui;

use crate::i18n::UiStrings;

/// Renders the load-fail card. Returns true when Retry is clicked.
#[allow(dead_code)]
pub(crate) fn render_load_fail_retry(ui: &mut egui::Ui, t: &UiStrings) -> bool {
    let mut retry = false;
    ui.group(|ui| {
        ui.label(t.chat_load_fail_message);
        if ui.button(t.chat_load_fail_retry).clicked() {
            retry = true;
        }
    });
    retry
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryAction {
    None,
    Retry,
    Unload,
    Reload,
}

/// Inline recovery keeps a failed chat turn actionable without sending the
/// user to Models. The caller supplies the model id for the bus command.
pub(crate) fn render_load_fail_recovery(ui: &mut egui::Ui, t: &UiStrings) -> RecoveryAction {
    let mut action = RecoveryAction::None;
    ui.group(|ui| {
        ui.label(t.chat_load_fail_message);
        ui.horizontal(|ui| {
            if ui.button(t.chat_load_fail_retry).clicked() { action = RecoveryAction::Retry; }
            if ui.button(t.models_unload).clicked() { action = RecoveryAction::Unload; }
            if ui.button(t.models_reload_clean).clicked() { action = RecoveryAction::Reload; }
        });
    });
    action
}
