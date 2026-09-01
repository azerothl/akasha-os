//! Guided tester scenarios panel.

use crate::cmd::Cmd;
use crate::{i18n, scenarios_panel, UiApp};
use eframe::egui;

impl UiApp {
pub(crate) fn ui_scenarios(&mut self, ui: &mut egui::Ui) {
        let t = i18n::strings(&self.prefs.language);
        let mut flags = scenarios_panel::ScenarioFlags {
            chat: self.scen_chat,
            note_human: self.scen_note_human,
            note_agent: self.scen_note_agent,
            confirm: self.scen_confirm,
            audit: self.scen_audit,
            module_agent: self.scen_module_agent,
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
        self.scen_chat = flags.chat;
        self.scen_note_human = flags.note_human;
        self.scen_note_agent = flags.note_agent;
        self.scen_confirm = flags.confirm;
        self.scen_audit = flags.audit;
        self.scen_module_agent = flags.module_agent;
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
