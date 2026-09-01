//! Settings panel controller — secrets, catalogue, and schedule-form commands.

use crate::cmd::Cmd;
use crate::settings_ui_state::SettingsUiState;
use crate::UiApp;
use aos_proto::{ModuleCatalogue, ModuleInfo};

impl UiApp {
    pub(crate) fn on_secret_list(&mut self, names: Vec<String>, encrypted: bool) {
        self.settings_ui.apply_secret_list(names, encrypted);
    }

    pub(crate) fn on_catalogue(&mut self, catalogue: ModuleCatalogue) {
        self.settings_ui.set_catalogue(catalogue);
    }

    pub(crate) fn on_installed_skills(&mut self, names: Vec<String>) {
        self.settings_ui.set_installed_skills(names);
    }

    pub(crate) fn on_installed_modules(&mut self, list: Vec<ModuleInfo>) {
        if self.scenario_ui.pending_module_agent
            && SettingsUiState::has_new_decl_module(
                &list,
                &self.scenario_ui.pending_module_baseline,
            )
        {
            self.scenario_ui.mark_module_agent();
        }
        self.settings_ui.set_installed_modules(list);
    }

    pub(crate) fn send_settings_schedule_create(&mut self) {
        let Some((goal, interval_secs)) = self.settings_ui.take_schedule_create() else {
            return;
        };
        let _ = self.cmd_tx.send(Cmd::ScheduleCreate {
            goal,
            interval_secs,
            next_fire_ms: None,
            display_title: None,
        });
    }
}
