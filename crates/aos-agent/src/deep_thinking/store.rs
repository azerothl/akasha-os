//! Persistance versionnée des plans Deep Thinking.
//!
//! `var/agents/<agent_id>/plans/<plan_id>.json` + `…/<plan_id>.versions.jsonl`.

use aos_proto::DeepPlan;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("plan introuvable: {0}")]
    NotFound(String),
}

/// Store en mémoire + disque pour les plans d'un répertoire agents.
pub struct PlanStore {
    root: PathBuf,
    /// plan_id → plan courant
    plans: Mutex<HashMap<String, DeepPlan>>,
    /// agent_id → plan_id actif
    by_agent: Mutex<HashMap<String, String>>,
}

impl PlanStore {
    pub fn open(agents_root: impl AsRef<Path>) -> Self {
        Self {
            root: agents_root.as_ref().to_path_buf(),
            plans: Mutex::new(HashMap::new()),
            by_agent: Mutex::new(HashMap::new()),
        }
    }

    fn plan_dir(&self, agent_id: &str) -> PathBuf {
        self.root.join(agent_id).join("plans")
    }

    fn plan_path(&self, agent_id: &str, plan_id: &str) -> PathBuf {
        self.plan_dir(agent_id).join(format!("{plan_id}.json"))
    }

    fn versions_path(&self, agent_id: &str, plan_id: &str) -> PathBuf {
        self.plan_dir(agent_id)
            .join(format!("{plan_id}.versions.jsonl"))
    }

    pub fn put(&self, plan: DeepPlan) -> Result<(), StoreError> {
        let dir = self.plan_dir(&plan.agent_id);
        fs::create_dir_all(&dir)?;
        let path = self.plan_path(&plan.agent_id, &plan.id);
        let json = serde_json::to_string_pretty(&plan)?;
        fs::write(&path, json)?;

        let versions = self.versions_path(&plan.agent_id, &plan.id);
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(versions)?;
        let line = serde_json::to_string(&plan)?;
        writeln!(f, "{line}")?;

        let mut plans = self.plans.lock().expect("plan store lock");
        let mut by_agent = self.by_agent.lock().expect("by_agent lock");
        by_agent.insert(plan.agent_id.clone(), plan.id.clone());
        plans.insert(plan.id.clone(), plan);
        Ok(())
    }

    pub fn get(&self, plan_id: &str) -> Result<DeepPlan, StoreError> {
        {
            let plans = self.plans.lock().expect("plan store lock");
            if let Some(p) = plans.get(plan_id) {
                return Ok(p.clone());
            }
        }
        self.load_from_disk_by_id(plan_id)
    }

    pub fn get_by_agent(&self, agent_id: &str) -> Result<DeepPlan, StoreError> {
        let cached_id = {
            let by_agent = self.by_agent.lock().expect("by_agent lock");
            by_agent.get(agent_id).cloned()
        };
        if let Some(pid) = cached_id {
            return self.get(&pid);
        }
        let dir = self.plan_dir(agent_id);
        if !dir.is_dir() {
            return Err(StoreError::NotFound(agent_id.into()));
        }
        let mut latest: Option<DeepPlan> = None;
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.ends_with(".json") || name.contains(".versions.") {
                continue;
            }
            let raw = fs::read_to_string(entry.path())?;
            let plan: DeepPlan = serde_json::from_str(&raw)?;
            let replace = match &latest {
                None => true,
                Some(cur) => {
                    plan.updated_at_ms > cur.updated_at_ms
                        || (plan.updated_at_ms == cur.updated_at_ms && plan.version > cur.version)
                }
            };
            if replace {
                latest = Some(plan);
            }
        }
        let plan = latest.ok_or_else(|| StoreError::NotFound(agent_id.into()))?;
        self.cache_plan(&plan);
        Ok(plan)
    }

    fn cache_plan(&self, plan: &DeepPlan) {
        let mut plans = self.plans.lock().expect("plan store lock");
        let mut by_agent = self.by_agent.lock().expect("by_agent lock");
        by_agent.insert(plan.agent_id.clone(), plan.id.clone());
        plans.insert(plan.id.clone(), plan.clone());
    }

    fn load_from_disk_by_id(&self, plan_id: &str) -> Result<DeepPlan, StoreError> {
        if !self.root.is_dir() {
            return Err(StoreError::NotFound(plan_id.into()));
        }
        for agent_entry in fs::read_dir(&self.root)? {
            let agent_entry = agent_entry?;
            if !agent_entry.file_type()?.is_dir() {
                continue;
            }
            let path = agent_entry.path().join("plans").join(format!("{plan_id}.json"));
            if path.is_file() {
                let raw = fs::read_to_string(&path)?;
                let plan: DeepPlan = serde_json::from_str(&raw)?;
                self.cache_plan(&plan);
                return Ok(plan);
            }
        }
        Err(StoreError::NotFound(plan_id.into()))
    }

    /// Nombre de versions enregistrées (lignes jsonl).
    pub fn version_count(&self, agent_id: &str, plan_id: &str) -> Result<usize, StoreError> {
        let path = self.versions_path(agent_id, plan_id);
        if !path.is_file() {
            return Ok(0);
        }
        let f = File::open(path)?;
        Ok(BufReader::new(f).lines().count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::{DeepPlanStatus, PlanStep, PlanStepStatus};

    fn sample_plan(agent_id: &str, id: &str, version: u32) -> DeepPlan {
        DeepPlan {
            id: id.into(),
            agent_id: agent_id.into(),
            title: "Test".into(),
            status: DeepPlanStatus::Pending,
            steps: vec![PlanStep {
                id: "1".into(),
                label: "Analyse".into(),
                description: None,
                status: PlanStepStatus::Pending,
                agent_id: None,
                children: vec![],
                logs: vec![],
            }],
            version,
            created_at_ms: 1,
            updated_at_ms: version as u64,
        }
    }

    #[test]
    fn put_get_and_versioning() {
        let dir = std::env::temp_dir().join(format!("aos-plan-store-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let store = PlanStore::open(&dir);

        let p1 = sample_plan("a1", "plan-1", 1);
        store.put(p1.clone()).unwrap();
        let got = store.get("plan-1").unwrap();
        assert_eq!(got.version, 1);
        assert_eq!(store.get_by_agent("a1").unwrap().id, "plan-1");

        let mut p2 = p1;
        p2.version = 2;
        p2.updated_at_ms = 2;
        p2.title = "Rev".into();
        store.put(p2).unwrap();
        assert_eq!(store.get("plan-1").unwrap().title, "Rev");
        assert_eq!(store.version_count("a1", "plan-1").unwrap(), 2);

        let _ = fs::remove_dir_all(&dir);
    }
}
