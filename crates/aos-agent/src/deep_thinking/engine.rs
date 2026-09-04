//! API DeepThinkingEngine : create / update / replace / delegate / logs.

use super::delegate::{bind_child_to_step, find_step_mut};
use super::store::{PlanStore, StoreError};
use super::summary::{format_plan_updated_trace, format_spawn_trace};
use aos_proto::{
    DeepPlan, DeepPlanStatus, DeepPlanStepPatch, PlanAppendLogRequest, PlanCreateRequest,
    PlanDelegateStepRequest, PlanReplaceTreeRequest, PlanStep, PlanUpdateStepRequest,
};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("{0}")]
    Store(#[from] StoreError),
    #[error("cap refusée: {0}")]
    CapDenied(String),
    #[error("étape introuvable: {0}")]
    StepNotFound(String),
    #[error("requête invalide: {0}")]
    Invalid(String),
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

static PLAN_SEQ: AtomicU64 = AtomicU64::new(1);

fn new_plan_id(agent_id: &str) -> String {
    let n = PLAN_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("dplan-{agent_id}-{n}-{}", now_ms() % 100_000)
}

/// Vérifie qu'un holder possède `plan.read:agent:<id>` ou `plan.write:…`.
pub fn check_plan_cap(caps: &[String], agent_id: &str, write: bool) -> Result<(), EngineError> {
    let need = if write {
        format!("plan.write:agent:{agent_id}")
    } else {
        format!("plan.read:agent:{agent_id}")
    };
    let alt_write = format!("plan.write:agent:{agent_id}");
    if caps.iter().any(|c| c == &need || (!write && c == &alt_write) || c == "plan.write:*" || c == "plan.read:*")
    {
        return Ok(());
    }
    // Holder du plan (agent lui-même) : accepter aussi l'absence explicite si caps
    // contiennent déjà le pattern minté à la création — sinon refuse.
    Err(EngineError::CapDenied(need))
}

/// Cap mintées pour un agent deep thinking.
pub fn deep_thinking_caps(agent_id: &str) -> Vec<String> {
    vec![
        format!("plan.read:agent:{agent_id}"),
        format!("plan.write:agent:{agent_id}"),
    ]
}

pub struct DeepThinkingEngine {
    store: PlanStore,
}

impl DeepThinkingEngine {
    pub fn open(agents_root: impl AsRef<Path>) -> Self {
        Self {
            store: PlanStore::open(agents_root),
        }
    }

    pub fn store(&self) -> &PlanStore {
        &self.store
    }

    pub fn create(
        &self,
        req: PlanCreateRequest,
        caller_caps: &[String],
    ) -> Result<(DeepPlan, String), EngineError> {
        check_plan_cap(caller_caps, &req.agent_id, true)?;
        if req.task.trim().is_empty() && req.steps.is_empty() {
            return Err(EngineError::Invalid(
                "task ou steps requis pour plan.create".into(),
            ));
        }
        let ts = now_ms();
        let title = req
            .title
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| {
                let t = req.task.trim();
                if t.chars().count() > 72 {
                    format!("{}…", t.chars().take(71).collect::<String>())
                } else if t.is_empty() {
                    "Deep Thinking".into()
                } else {
                    t.to_string()
                }
            });
        let steps = if req.steps.is_empty() {
            // Plan minimal seed — le modèle enrichira via replace_tree.
            vec![PlanStep {
                id: "1".into(),
                label: "Analyser la tâche".into(),
                description: Some(req.task.clone()),
                status: Default::default(),
                agent_id: None,
                children: vec![],
                logs: vec![],
            }]
        } else {
            req.steps
        };
        let plan = DeepPlan {
            id: new_plan_id(&req.agent_id),
            agent_id: req.agent_id,
            title,
            status: DeepPlanStatus::InProgress,
            steps,
            version: 1,
            created_at_ms: ts,
            updated_at_ms: ts,
        };
        self.store.put(plan.clone())?;
        let trace = format_plan_updated_trace(&plan);
        Ok((plan, trace))
    }

    pub fn get(
        &self,
        plan_id: Option<&str>,
        agent_id: Option<&str>,
        caller_caps: &[String],
    ) -> Result<DeepPlan, EngineError> {
        let plan = if let Some(pid) = plan_id.filter(|s| !s.is_empty()) {
            self.store.get(pid)?
        } else if let Some(aid) = agent_id.filter(|s| !s.is_empty()) {
            self.store.get_by_agent(aid)?
        } else {
            return Err(EngineError::Invalid(
                "plan_id ou agent_id requis".into(),
            ));
        };
        check_plan_cap(caller_caps, &plan.agent_id, false)?;
        Ok(plan)
    }

    pub fn update_step(
        &self,
        req: PlanUpdateStepRequest,
        caller_caps: &[String],
    ) -> Result<(DeepPlan, String), EngineError> {
        let mut plan = self.store.get(&req.plan_id)?;
        check_plan_cap(caller_caps, &plan.agent_id, true)?;
        apply_patch(&mut plan.steps, &req.step_id, &req.patch)
            .ok_or_else(|| EngineError::StepNotFound(req.step_id.clone()))?;
        bump_version(&mut plan);
        self.store.put(plan.clone())?;
        let trace = format_plan_updated_trace(&plan);
        Ok((plan, trace))
    }

    pub fn replace_tree(
        &self,
        req: PlanReplaceTreeRequest,
        caller_caps: &[String],
    ) -> Result<(DeepPlan, String), EngineError> {
        let mut plan = self.store.get(&req.plan_id)?;
        check_plan_cap(caller_caps, &plan.agent_id, true)?;
        plan.steps = req.steps;
        if let Some(title) = req.title.filter(|t| !t.trim().is_empty()) {
            plan.title = title;
        }
        if let Some(status) = req.status {
            plan.status = status;
        }
        bump_version(&mut plan);
        self.store.put(plan.clone())?;
        let trace = format_plan_updated_trace(&plan);
        Ok((plan, trace))
    }

    pub fn delegate_step(
        &self,
        req: PlanDelegateStepRequest,
        caller_caps: &[String],
    ) -> Result<(DeepPlan, String), EngineError> {
        let mut plan = self.store.get(&req.plan_id)?;
        check_plan_cap(caller_caps, &plan.agent_id, true)?;
        if !bind_child_to_step(&mut plan.steps, &req.step_id, &req.child_id) {
            return Err(EngineError::StepNotFound(req.step_id.clone()));
        }
        if let Some(brief) = req.brief.as_ref().filter(|b| !b.trim().is_empty()) {
            if let Some(step) = find_step_mut(&mut plan.steps, &req.step_id) {
                step.logs
                    .push(format!("delegated: {}", brief.trim()));
            }
        }
        bump_version(&mut plan);
        self.store.put(plan.clone())?;
        let trace = format_spawn_trace(&req.step_id, &req.child_id);
        Ok((plan, trace))
    }

    pub fn append_log(
        &self,
        req: PlanAppendLogRequest,
        caller_caps: &[String],
    ) -> Result<(DeepPlan, String), EngineError> {
        let mut plan = self.store.get(&req.plan_id)?;
        check_plan_cap(caller_caps, &plan.agent_id, true)?;
        let step = find_step_mut(&mut plan.steps, &req.step_id)
            .ok_or_else(|| EngineError::StepNotFound(req.step_id.clone()))?;
        step.logs.push(req.line);
        bump_version(&mut plan);
        self.store.put(plan.clone())?;
        let trace = format_plan_updated_trace(&plan);
        Ok((plan, trace))
    }

    /// Marque un step délégué Done après ChildDone (pas de check cap externe — parent only).
    pub fn complete_delegated_child(
        &self,
        agent_id: &str,
        child_id: &str,
        result: &str,
    ) -> Result<Option<(DeepPlan, String)>, EngineError> {
        let Ok(mut plan) = self.store.get_by_agent(agent_id) else {
            return Ok(None);
        };
        let Some(step_id) = super::delegate::find_step_for_child(&plan.steps, child_id) else {
            return Ok(None);
        };
        if let Some(step) = find_step_mut(&mut plan.steps, &step_id) {
            step.status = aos_proto::PlanStepStatus::Done;
            let clipped: String = result.chars().take(400).collect();
            step.logs.push(format!("child_done: {clipped}"));
        }
        bump_version(&mut plan);
        self.store.put(plan.clone())?;
        let trace = format_plan_updated_trace(&plan);
        Ok(Some((plan, trace)))
    }
}

fn bump_version(plan: &mut DeepPlan) {
    plan.version = plan.version.saturating_add(1);
    plan.updated_at_ms = now_ms();
}

fn apply_patch(steps: &mut [PlanStep], step_id: &str, patch: &DeepPlanStepPatch) -> Option<()> {
    let step = find_step_mut(steps, step_id)?;
    if let Some(status) = patch.status {
        step.status = status;
    }
    if let Some(label) = patch.label.as_ref() {
        step.label = label.clone();
    }
    if let Some(desc) = patch.description.as_ref() {
        step.description = Some(desc.clone());
    }
    if let Some(aid) = patch.agent_id.as_ref() {
        step.agent_id = Some(aid.clone());
    }
    for line in &patch.logs {
        step.logs.push(line.clone());
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::{PlanStepStatus, PlanGetRequest};
    use std::fs;

    fn caps(agent: &str) -> Vec<String> {
        deep_thinking_caps(agent)
    }

    #[test]
    fn create_update_delegate_versioning() {
        let dir = std::env::temp_dir().join(format!("aos-dte-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let eng = DeepThinkingEngine::open(&dir);
        let c = caps("agent-x");

        let (plan, _) = eng
            .create(
                PlanCreateRequest {
                    agent_id: "agent-x".into(),
                    task: "Résumer le dépôt".into(),
                    title: None,
                    steps: vec![
                        PlanStep {
                            id: "1".into(),
                            label: "Analyse".into(),
                            description: None,
                            status: PlanStepStatus::Pending,
                            agent_id: None,
                            children: vec![],
                            logs: vec![],
                        },
                        PlanStep {
                            id: "2".into(),
                            label: "Extraction".into(),
                            description: None,
                            status: PlanStepStatus::Pending,
                            agent_id: None,
                            children: vec![PlanStep {
                                id: "2.1".into(),
                                label: "Fichiers clés".into(),
                                description: None,
                                status: PlanStepStatus::Pending,
                                agent_id: None,
                                children: vec![],
                                logs: vec![],
                            }],
                            logs: vec![],
                        },
                    ],
                },
                &c,
            )
            .unwrap();
        assert_eq!(plan.version, 1);

        let (plan, _) = eng
            .update_step(
                PlanUpdateStepRequest {
                    plan_id: plan.id.clone(),
                    step_id: "1".into(),
                    patch: DeepPlanStepPatch {
                        status: Some(PlanStepStatus::Done),
                        ..Default::default()
                    },
                },
                &c,
            )
            .unwrap();
        assert_eq!(plan.version, 2);

        let (plan, trace) = eng
            .delegate_step(
                PlanDelegateStepRequest {
                    plan_id: plan.id.clone(),
                    step_id: "2.1".into(),
                    child_id: "child-1".into(),
                    brief: Some("extraire README".into()),
                },
                &c,
            )
            .unwrap();
        assert_eq!(plan.version, 3);
        assert!(trace.contains("2.1"));
        assert_eq!(
            super::super::delegate::find_step(&plan.steps, "2.1")
                .unwrap()
                .status,
            PlanStepStatus::Delegated
        );

        // Child sans caps plan → refuse get
        let err = eng.get(Some(&plan.id), None, &[]);
        assert!(matches!(err, Err(EngineError::CapDenied(_))));

        let got = eng
            .get(
                None,
                Some("agent-x"),
                &c,
            )
            .unwrap();
        assert_eq!(got.id, plan.id);
        assert_eq!(eng.store.version_count("agent-x", &plan.id).unwrap(), 3);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn child_caps_cannot_read_parent_plan() {
        let dir = std::env::temp_dir().join(format!("aos-dte-deny-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let eng = DeepThinkingEngine::open(&dir);
        let (plan, _) = eng
            .create(
                PlanCreateRequest {
                    agent_id: "parent".into(),
                    task: "t".into(),
                    title: None,
                    steps: vec![],
                },
                &caps("parent"),
            )
            .unwrap();
        let child_caps = vec!["tool.invoke:notes".into()];
        assert!(eng
            .get(Some(&plan.id), None, &child_caps)
            .unwrap_err()
            .to_string()
            .contains("cap refusée"));
        let _ = PlanGetRequest::default();
        let _ = fs::remove_dir_all(&dir);
    }
}
