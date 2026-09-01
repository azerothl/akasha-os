//! Mutable state owned by the Models / Providers panels (catalog, downloads, HF import).

use crate::models_page::ModelCatalogTab;
use aos_proto::{ModelInfo, ProviderRecord};

#[derive(Debug, Clone)]
pub(crate) struct ModelDownloadUiState {
    pub(crate) model_id: String,
    pub(crate) percent: u8,
    pub(crate) done_bytes: u64,
    pub(crate) total_bytes: u64,
}

#[derive(Debug)]
pub(crate) struct ModelsUiState {
    pub(crate) model_infos: Vec<ModelInfo>,
    pub(crate) providers: Vec<ProviderRecord>,
    pub(crate) provider_id: String,
    pub(crate) provider_preset: String,
    pub(crate) provider_endpoint: String,
    pub(crate) provider_secret_name: String,
    pub(crate) provider_secret_value: String,
    pub(crate) provider_enabled: bool,
    pub(crate) provider_test_msg: String,
    pub(crate) model_updates_msg: String,
    pub(crate) download_status: String,
    pub(crate) model_download: Option<ModelDownloadUiState>,
    pub(crate) model_download_restart: Option<String>,
    pub(crate) catalog_tab: ModelCatalogTab,
    pub(crate) hf_download_url: String,
    pub(crate) hf_download_name: String,
    pub(crate) hf_download_status: String,
}

impl Default for ModelsUiState {
    fn default() -> Self {
        Self {
            model_infos: Vec::new(),
            providers: Vec::new(),
            provider_id: String::new(),
            provider_preset: "openai".into(),
            provider_endpoint: "https://api.openai.com/v1".into(),
            provider_secret_name: "openai_api_key".into(),
            provider_secret_value: String::new(),
            provider_enabled: true,
            provider_test_msg: String::new(),
            model_updates_msg: String::new(),
            download_status: String::new(),
            model_download: None,
            model_download_restart: None,
            catalog_tab: ModelCatalogTab::Llm,
            hf_download_url: String::new(),
            hf_download_name: String::new(),
            hf_download_status: String::new(),
        }
    }
}

impl ModelsUiState {
    pub(crate) fn with_updates_msg(model_updates_msg: String) -> Self {
        Self {
            model_updates_msg,
            ..Self::default()
        }
    }

    pub(crate) fn set_model_infos(&mut self, list: Vec<ModelInfo>) {
        self.model_infos = list;
    }

    pub(crate) fn set_providers(&mut self, list: Vec<ProviderRecord>) {
        self.providers = list;
    }

    pub(crate) fn load_provider_for_edit(&mut self, p: &ProviderRecord) {
        self.provider_id = p.id.clone();
        self.provider_preset = p.preset.clone();
        self.provider_endpoint = p.endpoint.clone();
        self.provider_secret_name = p.secret_name.clone().unwrap_or_default();
        self.provider_enabled = p.enabled;
    }

    pub(crate) fn apply_provider_preset(
        &mut self,
        name: &str,
        endpoint: &str,
        secret: Option<&str>,
    ) {
        self.provider_preset = name.into();
        if self.provider_id.is_empty() {
            self.provider_id = name.into();
        }
        if !endpoint.is_empty() {
            self.provider_endpoint = endpoint.into();
        }
        if let Some(s) = secret {
            self.provider_secret_name = s.into();
        }
    }

    pub(crate) fn apply_provider_tested(
        &mut self,
        ok: bool,
        message: String,
        models: Vec<String>,
    ) {
        self.provider_test_msg = if ok {
            format!("ok — {message}")
        } else {
            format!("fail — {message}")
        };
        if !models.is_empty() {
            self.provider_test_msg
                .push_str(&format!(" ({})", models.join(", ")));
        }
    }

    /// Build upsert payload from the form; clears the vault secret field when present.
    pub(crate) fn take_provider_upsert(&mut self) -> Option<(ProviderRecord, Option<String>)> {
        let id = self.provider_id.trim().to_string();
        if id.is_empty() {
            return None;
        }
        let provider = ProviderRecord {
            id,
            preset: self.provider_preset.clone(),
            endpoint: self.provider_endpoint.trim().to_string(),
            secret_name: if self.provider_secret_name.trim().is_empty() {
                None
            } else {
                Some(self.provider_secret_name.trim().to_string())
            },
            enabled: self.provider_enabled,
            discovered_models: Vec::new(),
        };
        let secret = if self.provider_secret_value.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.provider_secret_value))
        };
        Some((provider, secret))
    }

    pub(crate) fn start_download(&mut self, model_id: String, status: String) {
        self.model_download_restart = None;
        self.model_download = Some(ModelDownloadUiState {
            model_id,
            percent: 0,
            done_bytes: 0,
            total_bytes: 0,
        });
        self.download_status = status;
    }

    pub(crate) fn set_download_progress(
        &mut self,
        model_id: String,
        done_bytes: u64,
        total_bytes: u64,
        percent: u8,
        status: String,
    ) {
        self.model_download = Some(ModelDownloadUiState {
            model_id,
            percent,
            done_bytes,
            total_bytes,
        });
        self.download_status = status;
    }

    pub(crate) fn finish_download(&mut self, model_id: String, status: String) {
        self.model_download = None;
        self.model_download_restart = Some(model_id);
        self.download_status = status;
        self.model_updates_msg.clear();
    }

    pub(crate) fn fail_download(&mut self, status: String) {
        self.model_download = None;
        self.model_download_restart = None;
        self.download_status = status;
    }

    pub(crate) fn mark_model_removed(&mut self, model_id: String, status: String) {
        self.model_download_restart = Some(model_id);
        self.download_status = status;
    }

    pub(crate) fn dismiss_download_restart(&mut self, clear_status: bool) {
        self.model_download_restart = None;
        if clear_status {
            self.download_status.clear();
        }
    }

    pub(crate) fn download_busy(&self) -> bool {
        self.model_download.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_provider_upsert_requires_id_and_clears_secret() {
        let mut state = ModelsUiState::default();
        assert!(state.take_provider_upsert().is_none());

        state.provider_id = "  openai  ".into();
        state.provider_secret_value = "sk-test".into();
        let (rec, secret) = state.take_provider_upsert().expect("id present");
        assert_eq!(rec.id, "openai");
        assert_eq!(secret.as_deref(), Some("sk-test"));
        assert!(state.provider_secret_value.is_empty());
    }

    #[test]
    fn download_lifecycle_clears_updates_on_finish() {
        let mut state = ModelsUiState::with_updates_msg("pending".into());
        state.start_download("m1".into(), "downloading".into());
        assert!(state.download_busy());
        assert!(state.model_download_restart.is_none());

        state.set_download_progress("m1".into(), 10, 100, 10, "10%".into());
        assert_eq!(state.model_download.as_ref().unwrap().percent, 10);

        state.finish_download("m1".into(), "done".into());
        assert!(!state.download_busy());
        assert_eq!(state.model_download_restart.as_deref(), Some("m1"));
        assert!(state.model_updates_msg.is_empty());
    }

    #[test]
    fn fail_and_dismiss_restart() {
        let mut state = ModelsUiState::default();
        state.start_download("m1".into(), "dl".into());
        state.fail_download("failed".into());
        assert!(!state.download_busy());
        assert!(state.model_download_restart.is_none());
        assert_eq!(state.download_status, "failed");

        state.mark_model_removed("m1".into(), "removed".into());
        state.dismiss_download_restart(true);
        assert!(state.model_download_restart.is_none());
        assert!(state.download_status.is_empty());
    }

    #[test]
    fn apply_provider_tested_formats_message() {
        let mut state = ModelsUiState::default();
        state.apply_provider_tested(true, "latency ok".into(), vec!["gpt".into()]);
        assert_eq!(state.provider_test_msg, "ok — latency ok (gpt)");
        state.apply_provider_tested(false, "timeout".into(), Vec::new());
        assert_eq!(state.provider_test_msg, "fail — timeout");
    }

    #[test]
    fn load_provider_for_edit_copies_fields() {
        let mut state = ModelsUiState::default();
        state.load_provider_for_edit(&ProviderRecord {
            id: "or".into(),
            preset: "openrouter".into(),
            endpoint: "https://openrouter.ai/api/v1".into(),
            secret_name: Some("openrouter_api_key".into()),
            enabled: false,
            discovered_models: Vec::new(),
        });
        assert_eq!(state.provider_id, "or");
        assert_eq!(state.provider_preset, "openrouter");
        assert!(!state.provider_enabled);
        assert_eq!(state.provider_secret_name, "openrouter_api_key");
    }
}
