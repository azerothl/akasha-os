//! Event handlers for feedback submission results.

use crate::os_open::{aos_home, native_path, open_in_browser};
use crate::UiApp;
use aos_proto::{FeedbackSubmitRequest, FeedbackSubmitResponse};

pub(crate) fn on_feedback_draft(app: &mut UiApp, request: FeedbackSubmitRequest) {
    let category = request.category.clone();
    app.feedback_ui.title = request.title;
    app.feedback_ui.category = request.category;
    app.feedback_ui.severity = request.severity;
    app.feedback_ui.body = request.body;
    app.feedback_ui.attachments = request
        .attachments
        .iter()
        .map(|a| native_path(&a.path))
        .collect();
    app.feedback_ui.template_category = category;
    app.feedback_ui.template_required =
        request.meta.get("source").and_then(|v| v.as_str()) != Some("troubleshooting_button");
    app.feedback_ui.scenario = request.scenario.unwrap_or_default();
    app.feedback_ui.publish_github =
        request.publish_github && !app.feedback_ui.category.eq_ignore_ascii_case("security");
    app.feedback_ui.diag_meta = Some(request.meta);
    app.tab = crate::Tab::Feedback;
}

pub(crate) fn on_feedback_ok(app: &mut UiApp, response: FeedbackSubmitResponse) {
    let mut message = format!(
        "Enregistré localement : {}\nDossier : {}",
        response.path, response.export_dir
    );
    match response.github_status.as_str() {
        "created" | "api" | "gh" => {
            if let Some(url) = &response.github_issue_url {
                message.push_str(&format!(
                    "\nIssue GitHub #{} : {}",
                    response
                        .github_issue_number
                        .map(|number| number.to_string())
                        .unwrap_or_else(|| "?".into()),
                    url
                ));
                open_in_browser(url);
            }
        }
        "skipped_security" => {
            message.push_str(
                "\nCatégorie security : non publié (issue publique interdite). Conservez le dossier local.",
            );
        }
        status if status == "form" || status.starts_with("form ") => {
            if let Some(url) = &response.github_issue_url {
                message.push_str(
                    "\nFormulaire GitHub ouvert — cliquez « Submit new issue » pour publier.",
                );
                open_in_browser(url);
            }
        }
        "local_only" => {}
        other => {
            message.push_str(&format!("\nGitHub : {other}"));
            if let Some(url) = &response.github_issue_url {
                open_in_browser(url);
            }
        }
    }
    app.feedback_ui.result = message;
    app.status = format!("feedback {}", response.id);
    let export_raw = native_path(&response.export_dir);
    let export = if export_raw.is_absolute() {
        export_raw
    } else {
        aos_home().join(&export_raw)
    };
    app.feedback_ui.export_dir = export
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| path.to_path_buf())
        .or(Some(export));
    app.reset_feedback_form();
}
