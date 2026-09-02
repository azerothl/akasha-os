//! Load-fail chrome in the chat transcript (message + Retry).

use eframe::egui;

use crate::i18n::UiStrings;

/// Renders the load-fail card. Returns true when Retry is clicked.
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
