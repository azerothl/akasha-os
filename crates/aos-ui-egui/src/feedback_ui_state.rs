//! Mutable state owned by the feedback form.

use std::path::PathBuf;

#[derive(Debug)]
pub(crate) struct FeedbackUiState {
    pub(crate) title: String,
    pub(crate) category: String,
    pub(crate) severity: String,
    pub(crate) body: String,
    pub(crate) template_category: String,
    pub(crate) template_required: bool,
    pub(crate) scenario: String,
    pub(crate) attachments: Vec<PathBuf>,
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
            body: template_for("ux").into(),
            template_category: "ux".into(),
            template_required: true,
            scenario: String::new(),
            attachments: Vec::new(),
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
        self.body = template_for("ux").into();
        self.template_category = "ux".into();
        self.template_required = true;
        self.scenario.clear();
        self.attachments.clear();
        self.category = "ux".into();
        self.severity = "medium".into();
        self.publish_github = true;
        self.diag_meta = None;
    }

    pub(crate) fn select_category(&mut self, category: &str) {
        let old = template_for(&self.template_category);
        if self.body.trim().is_empty() || self.body == old {
            self.body = template_for(category).into();
        }
        self.template_category = category.into();
    }

    pub(crate) fn template_complete(&self) -> bool {
        if !self.template_required {
            return true;
        }
        required_sections(&self.category).iter().all(|heading| {
            let Some(start) = self.body.find(heading) else {
                return false;
            };
            let rest = &self.body[start + heading.len()..];
            let content = rest.split("\n## ").next().unwrap_or("").trim();
            !content.is_empty()
                && !content.contains("[à compléter]")
                && !content.contains("[à completer]")
        })
    }
}

pub(crate) fn template_for(category: &str) -> &'static str {
    match category {
        "bug" | "security" => "## Ce qui s'est passé\n[à compléter]\n\n## Étapes pour reproduire\n[à compléter]\n\n## Comportement attendu\n[à compléter]\n\n## Comportement observé\n[à compléter]\n\n## Contexte complémentaire\n[à compléter]",
        "perf" => "## Scénario\n[à compléter]\n\n## Étapes pour reproduire\n[à compléter]\n\n## Performance attendue\n[à compléter]\n\n## Performance observée\n[à compléter]",
        "other" => "## Demande\n[à compléter]\n\n## Contexte\n[à compléter]\n\n## Détails complémentaires\n[à compléter]",
        _ => "## Objectif\n[à compléter]\n\n## Ce qui était déroutant\n[à compléter]\n\n## Amélioration suggérée\n[à compléter]\n\n## Contexte complémentaire\n[à compléter]",
    }
}

fn required_sections(category: &str) -> &'static [&'static str] {
    match category {
        "bug" | "security" => &[
            "## Ce qui s'est passé",
            "## Étapes pour reproduire",
            "## Comportement attendu",
            "## Comportement observé",
            "## Contexte complémentaire",
        ],
        "perf" => &[
            "## Scénario",
            "## Étapes pour reproduire",
            "## Performance attendue",
            "## Performance observée",
        ],
        "other" => &["## Demande", "## Contexte", "## Détails complémentaires"],
        _ => &[
            "## Objectif",
            "## Ce qui était déroutant",
            "## Amélioration suggérée",
            "## Contexte complémentaire",
        ],
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
        assert!(state.body.contains("## Objectif"));
        assert!(state.scenario.is_empty());
        assert!(state.publish_github);
        assert!(state.diag_meta.is_none());
        assert_eq!(state.result, "sent");
    }
}
