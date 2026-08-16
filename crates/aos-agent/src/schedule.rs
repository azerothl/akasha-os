//! OS agent scheduler — cap-gated interval schedules (Preview 0.3 / E2).
//!
//! Persists under `$AOS_HOME/var/schedules/*.json`. Fires spawn agents via
//! callbacks provided by `aos-agentd` (not chat channels).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleEntry {
    pub id: String,
    pub goal: String,
    /// Interval between fires in seconds (minimum 30).
    pub interval_secs: u64,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub last_fired_ms: u64,
    #[serde(default)]
    pub fire_count: u64,
    #[serde(default)]
    pub model_id: Option<String>,
    pub created_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleCreateRequest {
    pub goal: String,
    pub interval_secs: u64,
    #[serde(default)]
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleIdRequest {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleListResponse {
    pub schedules: Vec<ScheduleEntry>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn schedules_dir() -> PathBuf {
    let home = std::env::var("AOS_HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join("var/schedules")
}

pub fn ensure_dir() -> Result<(), String> {
    fs::create_dir_all(schedules_dir()).map_err(|e| e.to_string())
}

fn path_for(id: &str) -> PathBuf {
    schedules_dir().join(format!("{id}.json"))
}

pub fn list() -> Result<Vec<ScheduleEntry>, String> {
    ensure_dir()?;
    let mut out = Vec::new();
    let dir = schedules_dir();
    let rd = fs::read_dir(&dir).map_err(|e| e.to_string())?;
    for ent in rd.flatten() {
        let p = ent.path();
        if p.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(raw) = fs::read_to_string(&p) {
            if let Ok(s) = serde_json::from_str::<ScheduleEntry>(&raw) {
                out.push(s);
            }
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

pub fn create(req: &ScheduleCreateRequest) -> Result<ScheduleEntry, String> {
    if req.goal.trim().is_empty() {
        return Err("goal vide".into());
    }
    let interval = req.interval_secs.max(30);
    ensure_dir()?;
    let id = format!("sch-{}", now_ms());
    let entry = ScheduleEntry {
        id: id.clone(),
        goal: req.goal.trim().to_string(),
        interval_secs: interval,
        enabled: true,
        last_fired_ms: 0,
        fire_count: 0,
        model_id: req.model_id.clone(),
        created_ms: now_ms(),
    };
    save(&entry)?;
    Ok(entry)
}

pub fn cancel(id: &str) -> Result<ScheduleEntry, String> {
    let mut e = load(id)?;
    e.enabled = false;
    save(&e)?;
    Ok(e)
}

pub fn load(id: &str) -> Result<ScheduleEntry, String> {
    let p = path_for(id);
    let raw = fs::read_to_string(&p).map_err(|e| format!("schedule {id}: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

pub fn save(entry: &ScheduleEntry) -> Result<(), String> {
    ensure_dir()?;
    let p = path_for(&entry.id);
    let raw = serde_json::to_string_pretty(entry).map_err(|e| e.to_string())?;
    fs::write(p, raw).map_err(|e| e.to_string())
}

/// Returns schedules that are due to fire now.
pub fn due(now: u64) -> Result<Vec<ScheduleEntry>, String> {
    let mut out = Vec::new();
    for mut e in list()? {
        if !e.enabled {
            continue;
        }
        let interval_ms = e.interval_secs.saturating_mul(1000);
        if e.last_fired_ms == 0 || now.saturating_sub(e.last_fired_ms) >= interval_ms {
            e.last_fired_ms = now;
            e.fire_count = e.fire_count.saturating_add(1);
            save(&e)?;
            out.push(e);
        }
    }
    Ok(out)
}

pub fn mark_path(home: &Path) -> PathBuf {
    home.join("var/schedules")
}
