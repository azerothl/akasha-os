//! Declarative module panel integration.

use crate::cmd::Cmd;
use crate::{decl_ui, i18n, UiApp};
use eframe::egui;

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
            actions = panel.ui(
                ui,
                &mut self.decl_md_cache,
                t.decl_ui_refresh,
                &self.prefs.language,
            );
            for (key, value) in actions.local_patch.drain() {
                panel.local_state.insert(key, value);
            }
        }
        if actions.refresh {
            let _ = self.cmd_tx.send(Cmd::ModuleUiRefresh {
                module: module.to_string(),
            });
        }
        if let Some(inv) = actions.invoke {
            if let Some(panel) = self.decl_panels.get_mut(module) {
                panel.set_pending_invoke(true);
                panel.pending_refresh_binds = inv.refresh_binds;
                panel.pending_clear_form_keys = inv.clear_form_keys;
            }
            let _ = self.cmd_tx.send(Cmd::ModuleUiInvoke {
                module: module.to_string(),
                tool: inv.tool,
                args: inv.args,
            });
        }
        if let Some(svc) = actions.service_action {
            if let Some(panel) = self.decl_panels.get_mut(module) {
                panel.set_pending_invoke(true);
                panel.pending_refresh_binds = svc.refresh_binds.clone();
            }
            let _ = self.cmd_tx.send(Cmd::ModuleUiServiceAction {
                module: module.to_string(),
                action_id: svc.action_id,
                service: svc.service,
                tool: svc.tool,
                input: svc.input,
                refresh_binds: svc.refresh_binds,
                subscription_id: svc.subscription_id,
            });
        }
        if let Some((job_id, subscription_id)) = actions.cancel_job {
            let _ = self.cmd_tx.send(Cmd::ModuleUiCancelJob {
                module: module.to_string(),
                job_id,
                subscription_id,
            });
        }
    }
}
