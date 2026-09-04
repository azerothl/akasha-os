//! Conversation workspace, transcript, canvas, and composer.

use crate::composer_layout::{
    bounded_chat_workspace_width, chat_sessions_split, ChatSessionsSplit,
};
use crate::{chat_room, i18n, UiApp};
use eframe::egui;

impl UiApp {
    pub(crate) fn ui_chat(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        let full = ui.available_size();
        let gap = 8.0_f32;
        let canvas_open = chat_room::active_session_meta(
            &self.chat_state.sessions,
            self.chat_state.active_session.as_deref(),
        )
        .map(|m| m.canvas_open)
        .unwrap_or(false);
        let ChatSessionsSplit { side_w, chat_w } = chat_sessions_split(full.x, gap, canvas_open);

        ui.horizontal(|ui| {
            ui.set_min_height(full.y);
            if !self.prefs.ui_layout.canvas_focus {
                self.ui_chat_sidebar(ui, side_w, full.y, &t);
            }

            if !self.prefs.ui_layout.canvas_focus {
                ui.add_space(gap);
            }

            // Widgets in the session rail may need more than their planned
            // width (notably an unbroken model id). Use the remaining rect,
            // not the old split estimate, so the canvas and composer stay
            // inside the application viewport.
            let workspace_w = bounded_chat_workspace_width(
                if self.prefs.ui_layout.canvas_focus {
                    full.x
                } else {
                    chat_w
                },
                ui.available_width(),
                8.0,
            );
            self.ui_chat_workspace(ui, workspace_w, full.y, &t);
        });
    }
}
