//! Declarative module panel integration.

use crate::*;

impl UiApp {
    pub(crate) fn ui_decl_module(&mut self, ui: &mut egui::Ui, module: &str) {
        if !self.decl_panels.contains_key(module) {
            self.decl_panels
                .insert(module.to_string(), decl_ui::DeclUiPanelState::new(module));
            let _ = self.cmd_tx.send(Cmd::ModuleUiLoad {
                module: module.to_string(),
            });
        }
        let t = i18n::strings(&self.prefs.language);
        let mut actions = decl_ui::DeclUiActions::default();
        if let Some(panel) = self.decl_panels.get_mut(module) {
            actions = panel.ui(ui, &mut self.decl_md_cache, t.decl_ui_refresh);
        }
        if actions.refresh {
            let _ = self.cmd_tx.send(Cmd::ModuleUiRefresh {
                module: module.to_string(),
            });
        }
        if let Some((tool, args)) = actions.invoke {
            let _ = self.cmd_tx.send(Cmd::ModuleUiInvoke {
                module: module.to_string(),
                tool,
                args,
            });
        }
    }
}
