//! Conversation workspace, transcript, canvas, and composer.

use crate::composer_layout::{chat_sessions_split, ChatSessionsSplit};
use crate::{chat_room, i18n, UiApp};
use eframe::egui;

impl UiApp {
    pub(crate) fn ui_chat(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        let full = ui.available_size();
        let gap = 8.0_f32;
        let canvas_open =
            chat_room::active_session_meta(&self.sessions, self.active_session.as_deref())
                .map(|m| m.canvas_open)
                .unwrap_or(false);
        let ChatSessionsSplit { side_w, chat_w } = chat_sessions_split(full.x, gap, canvas_open);

        ui.horizontal(|ui| {
            ui.set_min_height(full.y);
            self.ui_chat_sidebar(ui, side_w, full.y, &t);

            ui.add_space(gap);

            self.ui_chat_workspace(ui, chat_w, full.y, &t);
        });
    }
}
