//! Models / Providers controller — catalog, download, and provider form events.

use crate::cmd::Cmd;
use crate::{i18n, UiApp};
use aos_proto::{ModelInfo, ProviderRecord};

impl UiApp {
    pub(crate) fn on_models(&mut self, list: Vec<ModelInfo>) {
        self.models_ui.set_model_infos(list);
    }

    pub(crate) fn on_model_operation_failed(&mut self, model_id: String, error: String) {
        let t = i18n::strings(&self.prefs.language);
        self.models_ui.record_error(model_id.clone(), error.clone());
        self.models_ui.transitions.remove(&model_id);
        self.status = format!(
            "{}: {error}",
            t.models_download_failed.replace("{}", &model_id)
        );
        self.toasts.push_error(self.status.clone());
    }

    pub(crate) fn on_providers(&mut self, list: Vec<ProviderRecord>) {
        self.models_ui.set_providers(list);
    }

    pub(crate) fn on_provider_tested(&mut self, ok: bool, message: String, models: Vec<String>) {
        self.models_ui.apply_provider_tested(ok, message, models);
    }

    pub(crate) fn on_model_download_started(&mut self, model_id: String) {
        let t = i18n::strings(&self.prefs.language);
        let status = t.models_downloading.replace("{}", &model_id);
        self.models_ui.start_download(model_id, status);
    }

    pub(crate) fn on_model_download_progress(
        &mut self,
        model_id: String,
        done_bytes: u64,
        total_bytes: u64,
        percent: u8,
    ) {
        let t = i18n::strings(&self.prefs.language);
        let status = format!(
            "{} {percent}%",
            t.models_downloading.replace("{}", &model_id)
        );
        self.models_ui
            .set_download_progress(model_id, done_bytes, total_bytes, percent, status);
    }

    pub(crate) fn on_model_download_finished(&mut self, model_id: String) {
        let t = i18n::strings(&self.prefs.language);
        let status = t.models_download_done.replace("{}", &model_id);
        self.models_ui.finish_download(model_id.clone(), status);
        self.image_studio.on_download_finished(&model_id);
    }

    pub(crate) fn on_model_download_failed(&mut self, model_id: String, error: String) {
        let t = i18n::strings(&self.prefs.language);
        let status = format!(
            "{}: {error}",
            t.models_download_failed.replace("{}", &model_id)
        );
        self.models_ui.fail_download(status);
    }

    pub(crate) fn on_model_removed(&mut self, model_id: String) {
        let t = i18n::strings(&self.prefs.language);
        self.models_ui
            .mark_model_removed(model_id.clone(), t.models_removed.to_string());
        self.toasts
            .push_success(format!("{} : {model_id}", t.models_removed));
    }

    pub(crate) fn send_provider_upsert(&mut self) {
        let Some((provider, secret_value)) = self.models_ui.take_provider_upsert() else {
            return;
        };
        let _ = self.cmd_tx.send(Cmd::ProviderUpsert {
            provider,
            secret_value,
        });
    }
}
