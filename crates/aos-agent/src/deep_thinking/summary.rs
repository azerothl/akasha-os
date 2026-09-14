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

pub fn count_delegated(steps: &[PlanStep]) -> usize {
    let mut n = 0;
    for s in steps {
        if matches!(s.status, PlanStepStatus::Delegated) {
            n += 1;
        }
        n += count_delegated(&s.children);
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

/// Contexte critic : le Deep Plan (pas le `plan_stack` legacy vide).
pub fn deep_thinking_critic_progress(
    step: u32,
    max_steps: u32,
    goal: &str,
    plan: Option<&DeepPlan>,
) -> String {
    match plan {
        Some(plan) => {
            let summary = light_plan_summary(plan);
            let delegated = count_delegated(&plan.steps);
            let mut out = format!(
                "step {step}/{max_steps} goal={goal}\n\
                 Deep Thinking : un plan EXISTE (la tâche n'est PAS vide).\n{summary}"
            );
            if delegated > 0 {
                out.push_str(
                    "\nÉtape(s) delegated : le sous-agent travaille encore — \
                     conseiller de réessayer agent.await ; \
                     interdire de recréer notes/scaffold en parallèle.",
                );
            }
            out
        }
        None => format!(
            "step {step}/{max_steps} goal={goal}\n\
             Deep Thinking : aucun plan encore — prochaine action = plan.create \
             (ce n'est PAS une « tâche vide »)."
        ),
    }
}

pub fn deep_thinking_critic_system_prompt() -> &'static str {
    "Tu es un critique Deep Thinking. En 2 phrases en français : est-ce que l'agent avance ? Que faire ensuite ?\n\
     Si un plan Deep Thinking est listé, la tâche N'EST PAS vide.\n\
     Si une étape est delegated : conseille de réessayer agent.await — l'enfant n'est pas bloqué ; \
     interdis de refaire notes.create / module.scaffold en parallèle.\n\
     Si le goal est une évaluation / conseil : le prochain pas est goal.complete avec la réponse, \
     pas de construire un module.\n\
     Réponds directement, sans balises <think> ni monologue Thinking Process."
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

    fn sample_plan() -> DeepPlan {
        DeepPlan {
            id: "p".into(),
            agent_id: "a".into(),
            title: "t".into(),
            status: DeepPlanStatus::InProgress,
            steps: vec![
                PlanStep {
                    id: "1".into(),
                    label: "A".into(),
                    description: None,
                    status: PlanStepStatus::Done,
                    agent_id: None,
                    children: vec![],
                    logs: vec![],
                },
                PlanStep {
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
                },
            ],
            version: 3,
            created_at_ms: 0,
            updated_at_ms: 0,
        }
    }

    #[test]
    fn counts_nested_in_progress() {
        let plan = sample_plan();
        assert_eq!(count_in_progress(&plan.steps), 2);
        assert_eq!(count_delegated(&plan.steps), 1);
        let msg = format_plan_updated_trace(&plan);
        assert!(msg.contains("v3"));
        assert!(msg.contains("2 étape"));
    }

    #[test]
    fn critic_progress_mentions_existing_plan() {
        let plan = sample_plan();
        let text = deep_thinking_critic_progress(3, 40, "évaluer un module", Some(&plan));
        assert!(text.contains("n'est PAS vide") || text.contains("EXISTE"));
        assert!(text.contains("delegated"));
        assert!(text.contains("agent.await"));
        let empty = deep_thinking_critic_progress(1, 40, "évaluer", None);
        assert!(empty.contains("plan.create"));
        assert!(empty.contains("PAS une"));
    }
}
