//! Traces légères et résumés pour l'UI chat.

use aos_proto::{DeepPlan, PlanStep, PlanStepStatus};

pub fn count_in_progress(steps: &[PlanStep]) -> usize {
    let mut n = 0;
    for s in steps {
        if matches!(
            s.status,
            PlanStepStatus::InProgress | PlanStepStatus::Delegated
        ) {
            n += 1;
        }
        n += count_in_progress(&s.children);
    }
    n
}

pub fn format_plan_updated_trace(plan: &DeepPlan) -> String {
    let n = count_in_progress(&plan.steps);
    format!(
        "Deep Thinking : plan mis à jour (v{}, {n} étape(s) en cours).",
        plan.version
    )
}

pub fn format_spawn_trace(step_id: &str, child_id: &str) -> String {
    format!("Sous-agent lancé pour l'étape {step_id} ({child_id}).")
}

/// Résumé indenté (texte) pour commandes « montre le plan ».
pub fn light_plan_summary(plan: &DeepPlan) -> String {
    let mut out = format!("Plan Deep Thinking (v{}) — {}\n", plan.version, plan.title);
    for step in &plan.steps {
        append_step(&mut out, step, 0);
    }
    out
}

fn append_step(out: &mut String, step: &PlanStep, depth: usize) {
    let indent = "  ".repeat(depth);
    let status = match step.status {
        PlanStepStatus::Pending => "pending",
        PlanStepStatus::InProgress => "in_progress",
        PlanStepStatus::Done => "done",
        PlanStepStatus::Delegated => "delegated",
        PlanStepStatus::Blocked => "blocked",
    };
    out.push_str(&format!(
        "{indent}> Étape {} — {} [{status}]\n",
        step.id, step.label
    ));
    for child in &step.children {
        append_step(out, child, depth + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::DeepPlanStatus;

    #[test]
    fn counts_nested_in_progress() {
        let plan = DeepPlan {
            id: "p".into(),
            agent_id: "a".into(),
            title: "t".into(),
            status: DeepPlanStatus::InProgress,
            steps: vec![PlanStep {
                id: "1".into(),
                label: "A".into(),
                description: None,
                status: PlanStepStatus::Done,
                agent_id: None,
                children: vec![],
                logs: vec![],
            }, PlanStep {
                id: "2".into(),
                label: "B".into(),
                description: None,
                status: PlanStepStatus::InProgress,
                agent_id: None,
                children: vec![PlanStep {
                    id: "2.1".into(),
                    label: "B1".into(),
                    description: None,
                    status: PlanStepStatus::Delegated,
                    agent_id: Some("c1".into()),
                    children: vec![],
                    logs: vec![],
                }],
                logs: vec![],
            }],
            version: 3,
            created_at_ms: 0,
            updated_at_ms: 0,
        };
        assert_eq!(count_in_progress(&plan.steps), 2);
        let msg = format_plan_updated_trace(&plan);
        assert!(msg.contains("v3"));
        assert!(msg.contains("2 étape"));
    }
}
