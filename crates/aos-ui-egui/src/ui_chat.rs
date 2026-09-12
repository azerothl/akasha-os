//! Conversation workspace, transcript, canvas, and composer.
//!
//! THESIS: Daily Preview is an agentic host shell, not a chat-wrapper column.
//! OWN-WORLD: Tokenized chamber hues (user-recolorable); 8px radius; product type
//! scale; no brand fill on the window. Status clock lives at the bottom.
//! STORY: Send a turn, watch the agent work (card, tool traces, Grant/Deny,
//! artifact), switch among 50 sessions without a permanent list.
//! FIRST VIEWPORT: Minimal left rail; session switcher in the header; overlay
//! list on demand; transcript owns the window; Network · Model · Caps on a
//! 28px bottom bar.
//! FORM: Operate shell, code-led; Station Board layout distilled after the
//! cobalt/ticker miss. Seed key 7b57e595, steered.
//! FINISH: unreviewed and undocumented is unfinished; this build ends with the
//! finish review, the verdict, and DESIGN.md.

use crate::composer_layout::bounded_chat_workspace_width;
use crate::prefs::UiPresentationMode;
use crate::{i18n, UiApp};
use eframe::egui;

/// Session picker stays a floating list, never a second full-height column.
pub(crate) fn session_picker_max_size(screen_w: f32, screen_h: f32) -> egui::Vec2 {
    let max_w = 280.0_f32.min((screen_w * 0.34).max(220.0));
    let max_h = (screen_h * 0.58).clamp(280.0, 460.0);
    egui::vec2(max_w, max_h)
}

impl UiApp {
    pub(crate) fn ui_chat(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        let full = ui.available_size();
        let hide_session_picker = matches!(
            self.prefs.ui_presentation,
            UiPresentationMode::Focus | UiPresentationMode::Zen
        ) || self.prefs.ui_layout.canvas_focus;
        if hide_session_picker {
            self.chat_state.sidebar.picker_open = false;
        }

        if self.chat_state.sidebar.picker_open && !hide_session_picker {
            let mut open = true;
            let ctx = ui.ctx().clone();
            let screen = ctx.screen_rect();
            let max = session_picker_max_size(screen.width(), screen.height());
            let pos = egui::pos2(screen.left() + 56.0 + 10.0, screen.top() + 44.0);
            egui::Window::new(t.session_picker)
                .id(egui::Id::new("aos_session_picker_v2"))
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .constrain(true)
                .default_pos(pos)
                .default_size([260.0, 400.0_f32.min(max.y)])
                .min_size([220.0, 240.0])
                .max_size(max)
                .show(&ctx, |ui| {
                    let inner_h = (max.y - 40.0).min(ui.available_height()).max(200.0);
                    let inner_w = ui.available_width().clamp(200.0, max.x);
                    ui.set_max_size(egui::vec2(inner_w, inner_h));
                    self.ui_chat_sidebar(ui, inner_w, inner_h, &t);
                });
            if !open {
                self.chat_state.sidebar.picker_open = false;
            }
        }

        ui.horizontal(|ui| {
            ui.set_min_height(full.y);
            let workspace_w = bounded_chat_workspace_width(full.x, ui.available_width(), 0.0);
            self.ui_chat_workspace(ui, workspace_w, full.y, &t);
        });
    }
}
