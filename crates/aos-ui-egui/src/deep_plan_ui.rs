//! Deep Thinking plan collapsible UI + chat command parsing.

use aos_proto::{ChatAttachment, PlanStep, PlanStepStatus};
use eframe::egui;

/// Parse user requests about the deep plan. Returns a UI action if matched.
#[derive(Debug, Clone)]
pub(crate) enum DeepPlanCommand {
    ShowFull,
    ExpandStep { step_id: String },
    ShowLogs { step_id: String },
}

pub(crate) fn parse_deep_plan_command(text: &str) -> Option<DeepPlanCommand> {
    let lower = text.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return None;
    }
    if lower.contains("plan complet")
        || lower.contains("montre le plan")
        || lower.contains("montre-moi le plan")
        || lower.contains("show the plan")
        || lower.contains("show full plan")
        || lower.contains("full deep plan")
    {
        return Some(DeepPlanCommand::ShowFull);
    }
    // "déplie seulement l'étape 2" / "expand step 2.1"
    for prefix in [
        "déplie seulement l'étape ",
        "déplie l'étape ",
        "deplie l'etape ",
        "expand step ",
        "expand only step ",
        "ouvre l'étape ",
    ] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let id = rest
                .trim()
                .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.')
                .to_string();
            if !id.is_empty() {
                return Some(DeepPlanCommand::ExpandStep { step_id: id });
            }
        }
    }
    // "affiche les logs internes de l'étape 3.1"
    for prefix in [
        "affiche les logs internes de l'étape ",
        "affiche les logs de l'étape ",
        "logs de l'étape ",
        "show logs for step ",
        "show internal logs for step ",
        "logs step ",
    ] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let id = rest
                .trim()
                .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.')
                .to_string();
            if !id.is_empty() {
                return Some(DeepPlanCommand::ShowLogs { step_id: id });
            }
        }
    }
    None
}

pub(crate) fn apply_command_to_attachment(
    att: &mut ChatAttachment,
    cmd: &DeepPlanCommand,
) -> bool {
    let ChatAttachment::DeepPlan {
        expand_step_ids,
        show_logs_step_id,
        ..
    } = att
    else {
        return false;
    };
    match cmd {
        DeepPlanCommand::ShowFull => {
            expand_step_ids.clear();
            *show_logs_step_id = None;
            true
        }
        DeepPlanCommand::ExpandStep { step_id } => {
            if !expand_step_ids.iter().any(|s| s == step_id) {
                expand_step_ids.push(step_id.clone());
            }
            true
        }
        DeepPlanCommand::ShowLogs { step_id } => {
            if !expand_step_ids.iter().any(|s| s == step_id) {
                expand_step_ids.push(step_id.clone());
            }
            *show_logs_step_id = Some(step_id.clone());
            true
        }
    }
}

fn status_label(s: PlanStepStatus) -> &'static str {
    match s {
        PlanStepStatus::Pending => "pending",
        PlanStepStatus::InProgress => "in_progress",
        PlanStepStatus::Done => "done",
        PlanStepStatus::Delegated => "delegated",
        PlanStepStatus::Blocked => "blocked",
    }
}

/// Collapsible Deep Thinking plan tree (mirrors salon thinking toggle).
pub(crate) fn deep_plan_toggle(
    ui: &mut egui::Ui,
    line_index: usize,
    title: &str,
    version: u32,
    steps: &[PlanStep],
    expand_step_ids: &[String],
    show_logs_step_id: Option<&str>,
    open: &mut std::collections::HashSet<usize>,
) {
    let expanded = open.contains(&line_index);
    let header = format!("📋 Plan Deep Thinking (v{version}) — {title}");
    let response = ui.add(
        egui::Label::new(
            egui::RichText::new(header)
                .small()
                .color(ui.visuals().weak_text_color()),
        )
        .sense(egui::Sense::click()),
    );
    if response.clicked() {
        if expanded {
            open.remove(&line_index);
        } else {
            open.insert(line_index);
        }
    }
    if expanded || !expand_step_ids.is_empty() {
        ui.add_space(2.0);
        for step in steps {
            draw_step(ui, step, 0, expand_step_ids, show_logs_step_id, expanded);
        }
    }
}

fn draw_step(
    ui: &mut egui::Ui,
    step: &PlanStep,
    depth: usize,
    expand_step_ids: &[String],
    show_logs_step_id: Option<&str>,
    parent_expanded: bool,
) {
    let force = expand_step_ids.iter().any(|id| id == &step.id || step.id.starts_with(&format!("{id}.")));
    if !parent_expanded && !force && depth > 0 {
        // When only a subtree is requested, still show ancestors of forced ids
        let ancestor = expand_step_ids.iter().any(|id| id.starts_with(&format!("{}.", step.id)) || id == &step.id);
        if !ancestor {
            return;
        }
    }
    let indent = "  ".repeat(depth);
    let line = format!(
        "{indent}> Étape {} — {} [{}]",
        step.id,
        step.label,
        status_label(step.status)
    );
    ui.label(
        egui::RichText::new(line)
            .small()
            .monospace()
            .color(ui.visuals().text_color()),
    );
    if show_logs_step_id == Some(step.id.as_str()) && !step.logs.is_empty() {
        for log in &step.logs {
            ui.label(
                egui::RichText::new(format!("{indent}    · {log}"))
                    .italics()
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
        }
    }
    let child_parent = parent_expanded || force || expand_step_ids.iter().any(|id| id == &step.id);
    for child in &step.children {
        draw_step(
            ui,
            child,
            depth + 1,
            expand_step_ids,
            show_logs_step_id,
            child_parent,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_show_and_expand() {
        assert!(matches!(
            parse_deep_plan_command("Montre le plan complet"),
            Some(DeepPlanCommand::ShowFull)
        ));
        assert!(matches!(
            parse_deep_plan_command("Déplie seulement l'étape 2.1"),
            Some(DeepPlanCommand::ExpandStep { step_id }) if step_id == "2.1"
        ));
        assert!(matches!(
            parse_deep_plan_command("Affiche les logs internes de l'étape 3.1"),
            Some(DeepPlanCommand::ShowLogs { step_id }) if step_id == "3.1"
        ));
    }
}
