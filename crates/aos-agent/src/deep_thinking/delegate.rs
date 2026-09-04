//! Liaison step ↔ sous-agent.

use aos_proto::{PlanStep, PlanStepStatus};

/// Trouve un step (récursif) par id.
pub fn find_step_mut<'a>(steps: &'a mut [PlanStep], step_id: &str) -> Option<&'a mut PlanStep> {
    for step in steps.iter_mut() {
        if step.id == step_id {
            return Some(step);
        }
        if let Some(found) = find_step_mut(&mut step.children, step_id) {
            return Some(found);
        }
    }
    None
}

pub fn find_step<'a>(steps: &'a [PlanStep], step_id: &str) -> Option<&'a PlanStep> {
    for step in steps {
        if step.id == step_id {
            return Some(step);
        }
        if let Some(found) = find_step(&step.children, step_id) {
            return Some(found);
        }
    }
    None
}

/// Lie un child agent à un step et passe en Delegated.
pub fn bind_child_to_step(steps: &mut [PlanStep], step_id: &str, child_id: &str) -> bool {
    let Some(step) = find_step_mut(steps, step_id) else {
        return false;
    };
    step.agent_id = Some(child_id.to_string());
    step.status = PlanStepStatus::Delegated;
    true
}

/// Retrouve le step_id lié à un child_id.
pub fn find_step_for_child(steps: &[PlanStep], child_id: &str) -> Option<String> {
    for step in steps {
        if step.agent_id.as_deref() == Some(child_id) {
            return Some(step.id.clone());
        }
        if let Some(id) = find_step_for_child(&step.children, child_id) {
            return Some(id);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Vec<PlanStep> {
        vec![PlanStep {
            id: "1".into(),
            label: "A".into(),
            description: None,
            status: PlanStepStatus::Pending,
            agent_id: None,
            children: vec![PlanStep {
                id: "1.1".into(),
                label: "A1".into(),
                description: None,
                status: PlanStepStatus::Pending,
                agent_id: None,
                children: vec![],
                logs: vec![],
            }],
            logs: vec![],
        }]
    }

    #[test]
    fn bind_and_find_child() {
        let mut steps = tree();
        assert!(bind_child_to_step(&mut steps, "1.1", "child-9"));
        assert_eq!(
            find_step(&steps, "1.1").unwrap().status,
            PlanStepStatus::Delegated
        );
        assert_eq!(
            find_step_for_child(&steps, "child-9").as_deref(),
            Some("1.1")
        );
    }
}
