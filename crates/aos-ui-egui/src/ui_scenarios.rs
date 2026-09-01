//! Guided tester scenarios panel.

use crate::cmd::Cmd;
use crate::{i18n, scenarios_panel, UiApp};
use eframe::egui;

impl UiApp {
pub(crate) fn ui_scenarios(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        let mut flags = scenarios_panel::ScenarioFlags {
            chat: self.scenario_ui.chat,
            note_human: self.scenario_ui.note_human,
            note_agent: self.scenario_ui.note_agent,
            confirm: self.scenario_ui.confirm,
            audit: self.scenario_ui.audit,
            module_agent: self.scenario_ui.module_agent,
        };
        let mut launch = false;
        let mut test_confirm = false;
        scenarios_panel::ui(
            ui,
            &t,
            &mut flags,
            || launch = true,
            || test_confirm = true,
        );
        self.scenario_ui.chat = flags.chat;
        self.scenario_ui.note_human = flags.note_human;
        self.scenario_ui.note_agent = flags.note_agent;
        self.scenario_ui.confirm = flags.confirm;
        self.scenario_ui.audit = flags.audit;
        self.scenario_ui.module_agent = flags.module_agent;
        if launch {
            self.launch_module_author_agent();
        }
        if test_confirm {
            self.status =
                "Créez puis tentez de supprimer une note sensible, ou utilisez le gate P3 en lab."
                    .into();
            let _ = self.cmd_tx.send(Cmd::RefreshConfirms);
        }
    }

}
