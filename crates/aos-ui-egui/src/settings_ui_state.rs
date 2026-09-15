//! Mutable state owned by the Settings panel (secrets, catalogue, schedule form).

use crate::secret_keygen::{KeygenAlphabet, KeygenTarget, LAN_SESSION_KEY_HEX_LEN};
use aos_proto::{ModuleCatalogue, ModuleInfo};
use std::collections::HashMap;

#[derive(Debug)]
pub(crate) struct SettingsUiState {
    pub(crate) search: String,
    /// P1 pagination lite : section sélectionnée quand la recherche est vide.
    /// `me` = panneau léger au premier open ; `all` reste disponible via la pill.
    pub(crate) section: String,
    pub(crate) secret_brave: String,
    pub(crate) secret_github: String,
    pub(crate) secret_openai: String,
    pub(crate) secret_lan_session: String,
    pub(crate) secret_custom_name: String,
    pub(crate) secret_custom_value: String,
    /// Last generated value shown in clear (password fields stay masked).
    pub(crate) keygen_preview: String,
    pub(crate) keygen_alphabet: KeygenAlphabet,
    pub(crate) keygen_length: usize,
    pub(crate) keygen_target: KeygenTarget,
    pub(crate) lan_node_id: String,
    pub(crate) lan_node_name: String,
    pub(crate) lan_node_address: String,
    pub(crate) lan_node_fingerprint: String,
    pub(crate) lan_pipeline_model: String,
    pub(crate) secret_names: Vec<String>,
    pub(crate) secret_vault_encrypted: bool,
    /// PIN entry for create / unlock (never persisted in clear).
    pub(crate) secrets_pin_entry: String,
    pub(crate) secrets_pin_confirm: String,
    /// Wall-clock seconds until which reveal stays unlocked (egui input time).
    pub(crate) secrets_unlocked_until: f64,
    pub(crate) secrets_revealed: HashMap<String, String>,
    pub(crate) secrets_reveal_pending: Option<String>,
    pub(crate) catalogue: Option<ModuleCatalogue>,
    pub(crate) installed_skills: Vec<String>,
    pub(crate) installed_modules: Vec<ModuleInfo>,
    pub(crate) schedule_goal: String,
    pub(crate) schedule_interval_secs: u64,
}

impl Default for SettingsUiState {
    fn default() -> Self {
        Self {
            search: String::new(),
            section: "me".into(),
            secret_brave: String::new(),
            secret_github: String::new(),
            secret_openai: String::new(),
            secret_lan_session: String::new(),
            secret_custom_name: String::new(),
            secret_custom_value: String::new(),
            keygen_preview: String::new(),
            keygen_alphabet: KeygenAlphabet::Hex,
            keygen_length: LAN_SESSION_KEY_HEX_LEN,
            keygen_target: KeygenTarget::LanSession,
            lan_node_id: String::new(),
            lan_node_name: String::new(),
            lan_node_address: String::new(),
            lan_node_fingerprint: String::new(),
            lan_pipeline_model: String::new(),
            secret_names: Vec::new(),
            secret_vault_encrypted: false,
            secrets_pin_entry: String::new(),
            secrets_pin_confirm: String::new(),
            secrets_unlocked_until: 0.0,
            secrets_revealed: HashMap::new(),
            secrets_reveal_pending: None,
            catalogue: None,
            installed_skills: Vec::new(),
            installed_modules: Vec::new(),
            schedule_goal: String::new(),
            schedule_interval_secs: 60,
        }
    }
}

impl SettingsUiState {
    pub(crate) fn apply_secret_list(&mut self, names: Vec<String>, encrypted: bool) {
        self.secret_names = names;
        self.secret_vault_encrypted = encrypted;
        self.secrets_revealed
            .retain(|name, _| self.secret_names.iter().any(|n| n == name));
    }

    pub(crate) fn secrets_unlocked(&self, now: f64) -> bool {
        now < self.secrets_unlocked_until
    }

    pub(crate) fn lock_secrets_reveal(&mut self) {
        self.secrets_unlocked_until = 0.0;
        self.secrets_revealed.clear();
        self.secrets_reveal_pending = None;
        self.secrets_pin_entry.clear();
        self.secrets_pin_confirm.clear();
    }

    pub(crate) fn unlock_secrets_reveal(&mut self, now: f64, ttl_secs: f64) {
        self.secrets_unlocked_until = now + ttl_secs;
        self.secrets_pin_entry.clear();
        self.secrets_pin_confirm.clear();
    }

    pub(crate) fn set_catalogue(&mut self, catalogue: ModuleCatalogue) {
        self.catalogue = Some(catalogue);
    }

    pub(crate) fn set_installed_skills(&mut self, names: Vec<String>) {
        self.installed_skills = names;
    }

    pub(crate) fn set_installed_modules(&mut self, list: Vec<ModuleInfo>) {
        self.installed_modules = list;
    }

    /// Installed module names used as a baseline when arming module-author scenarios.
    pub(crate) fn installed_module_names(&self) -> Vec<String> {
        self.installed_modules
            .iter()
            .map(|m| m.name.clone())
            .collect()
    }

    /// True when `list` contains a new sidebar decl-ui module vs `baseline`.
    pub(crate) fn has_new_decl_module(list: &[ModuleInfo], baseline: &[String]) -> bool {
        list.iter().any(|m| {
            aos_proto::decl_ui::sidebar_decl_ui_module(&m.name, m.ui_mode.as_deref())
                && !baseline.iter().any(|n| n == &m.name)
        })
    }

    /// Field that receives the next generated secret.
    pub(crate) fn keygen_target_field_mut(&mut self) -> &mut String {
        match self.keygen_target {
            KeygenTarget::Brave => &mut self.secret_brave,
            KeygenTarget::Github => &mut self.secret_github,
            KeygenTarget::Openai => &mut self.secret_openai,
            KeygenTarget::LanSession => &mut self.secret_lan_session,
        }
    }

    /// Take a non-empty schedule create request from the form, clearing the goal.
    pub(crate) fn take_schedule_create(&mut self) -> Option<(String, u64)> {
        let goal = self.schedule_goal.trim().to_string();
        if goal.is_empty() {
            return None;
        }
        let interval_secs = self.schedule_interval_secs.max(30);
        self.schedule_goal.clear();
        Some((goal, interval_secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_secret_list_stores_names_and_flag() {
        let mut state = SettingsUiState::default();
        state.apply_secret_list(vec!["brave_search_api_key".into()], true);
        assert_eq!(state.secret_names, vec!["brave_search_api_key".to_string()]);
        assert!(state.secret_vault_encrypted);
    }

    #[test]
    fn take_schedule_create_requires_goal_and_clears() {
        let mut state = SettingsUiState::default();
        assert!(state.take_schedule_create().is_none());

        state.schedule_goal = "  ".into();
        assert!(state.take_schedule_create().is_none());

        state.schedule_goal = " check inbox ".into();
        state.schedule_interval_secs = 10;
        let (goal, interval) = state.take_schedule_create().expect("goal present");
        assert_eq!(goal, "check inbox");
        assert_eq!(interval, 30);
        assert!(state.schedule_goal.is_empty());
    }

    #[test]
    fn has_new_decl_module_ignores_baseline() {
        let listed = vec![ModuleInfo {
            name: "cohortmod".into(),
            version: "0.1.0".into(),
            granted_caps: Vec::new(),
            tools: Vec::new(),
            quarantined: false,
            ui_mode: Some("declarative_ui".into()),
            ui_title: None,
        }];
        assert!(SettingsUiState::has_new_decl_module(&listed, &[]));
        assert!(!SettingsUiState::has_new_decl_module(
            &listed,
            &["cohortmod".into()]
        ));
    }

    #[test]
    fn keygen_target_field_routes_lan_by_default() {
        let mut state = SettingsUiState::default();
        assert_eq!(state.keygen_target, KeygenTarget::LanSession);
        assert_eq!(state.keygen_length, LAN_SESSION_KEY_HEX_LEN);
        *state.keygen_target_field_mut() = "abc".into();
        assert_eq!(state.secret_lan_session, "abc");
    }
}
