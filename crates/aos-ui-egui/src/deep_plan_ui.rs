//! Deep Thinking plan collapsible UI + chat command parsing.

use crate::cmd::ChatLine;
use aos_proto::{ChatAttachment, DeepPlan, PlanStep, PlanStepStatus};
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

/// Collapse historical duplicates then upsert the live plan card for `agent_id`.
pub(crate) fn sync_deep_plan_in_chat(
    chat: &mut Vec<ChatLine>,
    agent_id: &str,
    plan: &DeepPlan,
) -> usize {
    collapse_duplicate_deep_plans(chat);
    upsert_deep_plan_line(chat, agent_id, plan)
}

/// Keep the latest DeepPlan card per plan_id (drop older system-only duplicates).
pub(crate) fn collapse_duplicate_deep_plans(chat: &mut Vec<ChatLine>) {
    let mut seen_plan_ids = std::collections::HashSet::new();
    let mut drop_idx = Vec::new();
    for (i, line) in chat.iter().enumerate().rev() {
        let plan_ids: Vec<String> = line
            .attachments
            .iter()
            .filter_map(|a| match a {
                ChatAttachment::DeepPlan { plan_id, .. } => Some(plan_id.clone()),
                _ => None,
            })
            .collect();
        if plan_ids.is_empty() {
            continue;
        }
        let mut remove_atts = false;
        for pid in &plan_ids {
            if !seen_plan_ids.insert(pid.clone()) {
                remove_atts = true;
            }
        }
        if remove_atts {
            let only_deep = line.attachments.iter().all(|a| {
                matches!(a, ChatAttachment::DeepPlan { .. })
            });
            if only_deep {
                drop_idx.push(i);
            }
        }
    }
    for i in drop_idx {
        chat.remove(i);
    }
}

/// Insert or update a single DeepPlan attachment line for this agent/plan.
pub(crate) fn upsert_deep_plan_line(
    chat: &mut Vec<ChatLine>,
    agent_id: &str,
    plan: &DeepPlan,
) -> usize {
    let title = if plan.title.trim().is_empty() {
        "plan"
    } else {
        plan.title.as_str()
    };
    let content = format!("📋 Plan Deep Thinking (v{}) — {title}", plan.version);
    let target = chat.iter().enumerate().rev().find(|(_, line)| {
        line.attachments.iter().any(|a| {
            matches!(
                a,
                ChatAttachment::DeepPlan { plan_id, .. } if plan_id == &plan.id
            ) || matches!(
                a,
                ChatAttachment::DeepPlan { agent_id: aid, .. } if aid == agent_id
            )
        })
    }).map(|(i, _)| i);

    if let Some(idx) = target {
        let line = &mut chat[idx];
        let (expand, logs) = line
            .attachments
            .iter()
            .find_map(|a| match a {
                ChatAttachment::DeepPlan {
                    expand_step_ids,
                    show_logs_step_id,
                    ..
                } => Some((expand_step_ids.clone(), show_logs_step_id.clone())),
                _ => None,
            })
            .unwrap_or_default();
        line.role = "system".into();
        line.text = content;
        line.attachments = vec![ChatAttachment::DeepPlan {
            agent_id: agent_id.to_string(),
            plan_id: plan.id.clone(),
            title: plan.title.clone(),
            version: plan.version,
            steps: plan.steps.clone(),
            expand_step_ids: expand,
            show_logs_step_id: logs,
        }];
        idx
    } else {
        chat.push(ChatLine {
            role: "system".into(),
            text: content,
            attachments: vec![ChatAttachment::DeepPlan {
                agent_id: agent_id.to_string(),
                plan_id: plan.id.clone(),
                title: plan.title.clone(),
                version: plan.version,
                steps: plan.steps.clone(),
                expand_step_ids: vec![],
                show_logs_step_id: None,
            }],
            speaker_id: None,
            speaker_name: None,
            thinking: None,
        });
        chat.len() - 1
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
    use aos_proto::{DeepPlanStatus, PlanStep};

    fn sample_plan(version: u32, status: PlanStepStatus) -> DeepPlan {
        DeepPlan {
            id: "p1".into(),
            agent_id: "a1".into(),
            title: "Mission".into(),
            status: DeepPlanStatus::InProgress,
            steps: vec![PlanStep {
                id: "1".into(),
                label: "Étape".into(),
                description: None,
                status,
                agent_id: None,
                children: vec![],
                logs: vec![],
            }],
            version,
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

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

    #[test]
    fn upsert_updates_same_card_not_duplicates() {
        let mut chat = Vec::new();
        let p1 = sample_plan(1, PlanStepStatus::Pending);
        upsert_deep_plan_line(&mut chat, "a1", &p1);
        let p2 = sample_plan(2, PlanStepStatus::Done);
        upsert_deep_plan_line(&mut chat, "a1", &p2);
        assert_eq!(chat.len(), 1);
        let ChatAttachment::DeepPlan { version, steps, .. } = &chat[0].attachments[0] else {
            panic!("expected DeepPlan");
        };
        assert_eq!(*version, 2);
        assert!(matches!(steps[0].status, PlanStepStatus::Done));
    }

    #[test]
    fn collapse_keeps_latest_plan_card() {
        let mut chat = vec![
            ChatLine {
                role: "system".into(),
                text: "old".into(),
                attachments: vec![ChatAttachment::DeepPlan {
                    agent_id: "a1".into(),
                    plan_id: "p1".into(),
                    title: "t".into(),
                    version: 1,
                    steps: vec![],
                    expand_step_ids: vec![],
                    show_logs_step_id: None,
                }],
                speaker_id: None,
                speaker_name: None,
                thinking: None,
            },
            ChatLine {
                role: "system".into(),
                text: "new".into(),
                attachments: vec![ChatAttachment::DeepPlan {
                    agent_id: "a1".into(),
                    plan_id: "p1".into(),
                    title: "t".into(),
                    version: 3,
                    steps: vec![],
                    expand_step_ids: vec![],
                    show_logs_step_id: None,
                }],
                speaker_id: None,
                speaker_name: None,
                thinking: None,
            },
        ];
        collapse_duplicate_deep_plans(&mut chat);
        assert_eq!(chat.len(), 1);
        assert_eq!(chat[0].text, "new");
    }
}
