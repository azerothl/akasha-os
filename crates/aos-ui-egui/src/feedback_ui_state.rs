//! Mutable state owned by the feedback form.

use std::path::PathBuf;

#[derive(Debug)]
pub(crate) struct FeedbackUiState {
    pub(crate) title: String,
    pub(crate) category: String,
    pub(crate) severity: String,
    pub(crate) body: String,
    pub(crate) scenario: String,
    pub(crate) result: String,
    pub(crate) publish_github: bool,
    pub(crate) export_dir: Option<PathBuf>,
    pub(crate) diag_meta: Option<serde_json::Value>,
}

impl Default for FeedbackUiState {
    fn default() -> Self {
        Self {
            title: String::new(),
            category: "ux".into(),
            severity: "medium".into(),
            body: String::new(),
            scenario: String::new(),
            result: String::new(),
            publish_github: true,
            export_dir: None,
            diag_meta: None,
        }
    }
}

impl FeedbackUiState {
    pub(crate) fn reset_form(&mut self) {
        self.title.clear();
        self.body.clear();
        self.scenario.clear();
        self.category = "ux".into();
        self.severity = "medium".into();
        self.publish_github = true;
        self.diag_meta = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_form_restores_defaults_without_clearing_result() {
        let mut state = FeedbackUiState {
            title: "Bug".into(),
            category: "security".into(),
            severity: "high".into(),
            body: "details".into(),
            scenario: "scenario".into(),
            result: "sent".into(),
            publish_github: false,
            diag_meta: Some(serde_json::json!({"healthy": false})),
            ..Default::default()
        };

        state.reset_form();

        assert!(state.title.is_empty());
        assert_eq!(state.category, "ux");
        assert_eq!(state.severity, "medium");
        assert!(state.body.is_empty());
        assert!(state.scenario.is_empty());
        assert!(state.publish_github);
        assert!(state.diag_meta.is_none());
        assert_eq!(state.result, "sent");
    }
}
