//! Agent job intake façade (P21.4) — enqueue → `aos-agentd` via the bus.
//!
//! Serverd does **not** run the agent runtime. Caps fail-closed: empty actor
//! is refused; enqueue is audited. Job records live under
//! `$AOS_HOME/var/run/serverd-jobs.json` for `job-list` / `job-status`.

use aos_ipc::BusClient;
use aos_proto::{AgentCreateRequest, AgentCreateResponse, AgentStartRequest};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::default_bus_addr;

/// Relative path for the intake job ledger.
pub fn jobs_store_relpath() -> &'static str {
    "var/run/serverd-jobs.json"
}

fn jobs_path(home: &Path) -> PathBuf {
    home.join(jobs_store_relpath())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// How intake maps onto agentd.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum IntakeMode {
    /// `agent.create` with a goal (spawns worker when statement + max_steps).
    #[default]
    Create,
    /// `agent.start` for an existing persisted agent id.
    Start,
    /// `schedule.create` (E2 recurring) — still no UI required.
    Schedule,
}

impl IntakeMode {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "create" | "agent.create" => Ok(Self::Create),
            "start" | "agent.start" => Ok(Self::Start),
            "schedule" | "schedule.create" => Ok(Self::Schedule),
            other => Err(format!(
                "unknown intake mode `{other}` (create|start|schedule)"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Start => "start",
            Self::Schedule => "schedule",
        }
    }
}

/// One intake job recorded by serverd (consultable without egui).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobRecord {
    pub id: String,
    pub actor: String,
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule_id: Option<String>,
    /// `submitted` | `failed`
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub created_ms: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct JobStore {
    jobs: Vec<JobRecord>,
}

fn load_store(home: &Path) -> JobStore {
    let path = jobs_path(home);
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => JobStore::default(),
    }
}

fn save_store(home: &Path, store: &JobStore) -> Result<(), String> {
    if let Some(parent) = jobs_path(home).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Cap ledger size so restart thrash cannot unbounded grow the file.
    let mut trimmed = store.jobs.clone();
    if trimmed.len() > 200 {
        let skip = trimmed.len() - 200;
        trimmed = trimmed.split_off(skip);
    }
    let out = JobStore { jobs: trimmed };
    let raw = serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?;
    fs::write(jobs_path(home), raw).map_err(|e| e.to_string())
}

/// Parameters for a single enqueue (control socket / CLI).
#[derive(Debug, Clone, Default)]
pub struct EnqueueParams {
    pub actor: String,
    pub goal: Option<String>,
    pub agent_id: Option<String>,
    pub model_id: Option<String>,
    pub caps: Vec<String>,
    pub mode: IntakeMode,
    pub interval_secs: Option<u64>,
}

/// Fail-closed gate before talking to agentd.
pub fn validate_enqueue(p: &EnqueueParams) -> Result<(), String> {
    let actor = p.actor.trim();
    if actor.is_empty() {
        return Err("actor required (caps fail-closed; serverd does not mint identity)".into());
    }
    match p.mode {
        IntakeMode::Create | IntakeMode::Schedule => {
            let goal = p.goal.as_deref().unwrap_or("").trim();
            if goal.is_empty() {
                return Err("goal required for create/schedule intake".into());
            }
        }
        IntakeMode::Start => {
            let id = p.agent_id.as_deref().unwrap_or("").trim();
            if id.is_empty() {
                return Err("agent_id required for start intake".into());
            }
        }
    }
    Ok(())
}

pub fn list_jobs(home: &Path) -> Vec<JobRecord> {
    load_store(home).jobs
}

pub fn get_job(home: &Path, id: &str) -> Option<JobRecord> {
    load_store(home)
        .jobs
        .into_iter()
        .find(|j| j.id == id)
}

/// Enqueue via bus → agentd; persist job record; audit via caller.
pub fn enqueue(home: &Path, params: EnqueueParams) -> Result<JobRecord, String> {
    validate_enqueue(&params)?;
    let job_id = format!("job-{}", now_ms());
    let result = submit_to_agentd(&params);
    let record = match result {
        Ok(submitted) => JobRecord {
            id: job_id,
            actor: params.actor.trim().to_string(),
            mode: params.mode.as_str().into(),
            goal: params.goal.clone(),
            agent_id: submitted.agent_id.or(params.agent_id.clone()),
            schedule_id: submitted.schedule_id,
            state: "submitted".into(),
            message: submitted.message,
            created_ms: now_ms(),
        },
        Err(e) => JobRecord {
            id: job_id,
            actor: params.actor.trim().to_string(),
            mode: params.mode.as_str().into(),
            goal: params.goal.clone(),
            agent_id: params.agent_id.clone(),
            schedule_id: None,
            state: "failed".into(),
            message: Some(e.clone()),
            created_ms: now_ms(),
        },
    };
    let mut store = load_store(home);
    store.jobs.push(record.clone());
    save_store(home, &store)?;
    if record.state == "failed" {
        return Err(record
            .message
            .clone()
            .unwrap_or_else(|| "enqueue failed".into()));
    }
    Ok(record)
}

struct Submitted {
    agent_id: Option<String>,
    schedule_id: Option<String>,
    message: Option<String>,
}

fn submit_to_agentd(params: &EnqueueParams) -> Result<Submitted, String> {
    let bus_addr = default_bus_addr();
    let caps = params.caps.clone();
    let from = format!("serverd:{}", params.actor.trim());
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(async {
        let bus = BusClient::connect(&bus_addr, "aos-serverd-intake")
            .await
            .map_err(|e| format!("bus connect: {e}"))?;
        match params.mode {
            IntakeMode::Create => {
                let goal = params.goal.as_deref().unwrap_or("").trim();
                let mut req = AgentCreateRequest::simple(goal);
                req.origin = Some("serverd-intake".into());
                req.model_id = params.model_id.clone();
                if !params.caps.is_empty() {
                    req.caps = params.caps.clone();
                }
                let resp: AgentCreateResponse = bus
                    .call_from(&from, "agent.create", &req, caps)
                    .await
                    .map_err(|e| format!("agent.create: {e}"))?;
                Ok(Submitted {
                    agent_id: Some(resp.agent_id),
                    schedule_id: None,
                    message: Some("agent.create submitted".into()),
                })
            }
            IntakeMode::Start => {
                let agent_id = params.agent_id.as_deref().unwrap_or("").trim().to_string();
                let req = AgentStartRequest {
                    agent_id: agent_id.clone(),
                };
                let _: bool = bus
                    .call_from(&from, "agent.start", &req, caps)
                    .await
                    .map_err(|e| format!("agent.start: {e}"))?;
                Ok(Submitted {
                    agent_id: Some(agent_id),
                    schedule_id: None,
                    message: Some("agent.start submitted".into()),
                })
            }
            IntakeMode::Schedule => {
                let goal = params.goal.as_deref().unwrap_or("").trim().to_string();
                let interval = params.interval_secs.unwrap_or(3600).max(30);
                let req = ScheduleCreatePayload {
                    goal,
                    interval_secs: interval,
                    model_id: params.model_id.clone(),
                    next_fire_ms: None,
                    display_title: Some("serverd-intake".into()),
                };
                let entry: ScheduleEntryPayload = bus
                    .call_from(&from, "schedule.create", &req, caps)
                    .await
                    .map_err(|e| format!("schedule.create: {e}"))?;
                Ok(Submitted {
                    agent_id: None,
                    schedule_id: Some(entry.id),
                    message: Some("schedule.create submitted".into()),
                })
            }
        }
    })
}

/// Minimal CBOR/JSON-compatible schedule.create request (matches aos-agent).
#[derive(Debug, Serialize)]
struct ScheduleCreatePayload {
    goal: String,
    interval_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_fire_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ScheduleEntryPayload {
    id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_requires_actor_and_goal() {
        let err = validate_enqueue(&EnqueueParams {
            actor: String::new(),
            goal: Some("x".into()),
            mode: IntakeMode::Create,
            ..Default::default()
        })
        .unwrap_err();
        assert!(err.contains("actor"));

        let err = validate_enqueue(&EnqueueParams {
            actor: "ops".into(),
            goal: None,
            mode: IntakeMode::Create,
            ..Default::default()
        })
        .unwrap_err();
        assert!(err.contains("goal"));

        let err = validate_enqueue(&EnqueueParams {
            actor: "ops".into(),
            mode: IntakeMode::Start,
            ..Default::default()
        })
        .unwrap_err();
        assert!(err.contains("agent_id"));
    }

    #[test]
    fn mode_parse_aliases() {
        assert_eq!(IntakeMode::parse("create").unwrap(), IntakeMode::Create);
        assert_eq!(IntakeMode::parse("agent.start").unwrap(), IntakeMode::Start);
        assert_eq!(
            IntakeMode::parse("schedule.create").unwrap(),
            IntakeMode::Schedule
        );
    }

    #[test]
    fn job_store_roundtrip() {
        let home = std::env::temp_dir().join(format!("aos-intake-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(home.join("var/run")).unwrap();
        let mut store = JobStore::default();
        store.jobs.push(JobRecord {
            id: "job-1".into(),
            actor: "cli".into(),
            mode: "create".into(),
            goal: Some("hi".into()),
            agent_id: Some("agent-1".into()),
            schedule_id: None,
            state: "submitted".into(),
            message: None,
            created_ms: 1,
        });
        save_store(&home, &store).unwrap();
        let loaded = list_jobs(&home);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].agent_id.as_deref(), Some("agent-1"));
        let _ = fs::remove_dir_all(&home);
    }
}
