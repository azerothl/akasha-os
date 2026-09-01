//! Memory panel controller — recall/list/sweep event handlers.

use crate::cmd::Cmd;
use crate::{i18n, UiApp};
use aos_proto::MemHit;

impl UiApp {
    pub(crate) fn on_mem_hits(&mut self, hits: Vec<MemHit>) {
        self.memory_ui.set_hits(hits);
    }

    pub(crate) fn on_mem_sweep_status(&mut self, last_pass_ms: u64, last_pass_label: String) {
        self.memory_ui
            .apply_sweep_status(last_pass_ms, last_pass_label);
    }

    pub(crate) fn on_mem_extracted(&mut self, n: usize) {
        let t = i18n::strings(&self.prefs.language);
        self.status = t.memory_extracted_toast.replace("{}", &n.to_string());
        let _ = self.cmd_tx.send(Cmd::MemList {
            include_superseded: self.memory_ui.show_superseded,
        });
    }

    pub(crate) fn send_mem_list(&self) {
        let _ = self.cmd_tx.send(Cmd::MemList {
            include_superseded: self.memory_ui.show_superseded,
        });
    }

    pub(crate) fn send_mem_remember(&mut self) {
        let Some(text) = self.memory_ui.take_remember_note() else {
            return;
        };
        let _ = self.cmd_tx.send(Cmd::MemRemember {
            text,
            pinned: true,
        });
    }
}
