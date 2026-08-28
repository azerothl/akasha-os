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
    /// When true, schedule is paused (no fires until resumed).
    #[serde(default)]
    pub paused: bool,
    /// First-fire alignment (epoch ms). Used when `last_fired_ms == 0`.
    #[serde(default)]
    pub next_fire_ms: Option<u64>,
    /// Human title from chat (not the technical id).
    #[serde(default)]
    pub display_title: Option<String>,
    #[serde(default)]
    pub last_fired_ms: u64,
    #[serde(default)]
    pub fire_count: u64,
    /// Worker spawned by the last successful fire, if still claimed.
    #[serde(default)]
    pub active_agent_id: Option<String>,
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
    #[serde(default)]
    pub next_fire_ms: Option<u64>,
    #[serde(default)]
    pub display_title: Option<String>,
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
        paused: false,
        next_fire_ms: req.next_fire_ms,
        display_title: req
            .display_title
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        last_fired_ms: 0,
        fire_count: 0,
        active_agent_id: None,
        model_id: req.model_id.clone(),
        created_ms: now_ms(),
    };
    save(&entry)?;
    Ok(entry)
}

pub fn cancel(id: &str) -> Result<ScheduleEntry, String> {
    let mut e = load(id)?;
    e.enabled = false;
    e.paused = false;
    save(&e)?;
    Ok(e)
}

pub fn pause(id: &str) -> Result<ScheduleEntry, String> {
    let mut e = load(id)?;
    if !e.enabled {
        return Err("schedule arrêté".into());
    }
    e.paused = true;
    save(&e)?;
    Ok(e)
}

pub fn resume(id: &str) -> Result<ScheduleEntry, String> {
    let mut e = load(id)?;
    if !e.enabled {
        return Err("schedule arrêté".into());
    }
    e.paused = false;
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

/// Interval-due enabled schedules that are not already running.
///
/// Does **not** persist `last_fired_ms` — call [`mark_fired`] after a
/// successful spawn so a failed spawn can retry on the next tick.
/// `running_agent_ids` are workers still live in the runtime; a claimed
/// `active_agent_id` in that set blocks a new fire (overlap).
pub fn due(now: u64, running_agent_ids: &[String]) -> Result<Vec<ScheduleEntry>, String> {
    let mut out = Vec::new();
    for e in list()? {
        if !e.enabled || e.paused {
            continue;
        }
        if let Some(aid) = e.active_agent_id.as_deref() {
            if running_agent_ids.iter().any(|id| id == aid) {
                continue;
            }
        }
        let interval_ms = e.interval_secs.saturating_mul(1000);
        if e.last_fired_ms == 0 {
            if let Some(nf) = e.next_fire_ms {
                if now < nf {
                    continue;
                }
            }
            out.push(e);
        } else if now.saturating_sub(e.last_fired_ms) >= interval_ms {
            out.push(e);
        }
    }
    Ok(out)
}

/// Record a successful fire (interval consumed, overlap claim set).
pub fn mark_fired(id: &str, now: u64, agent_id: &str) -> Result<ScheduleEntry, String> {
    let mut e = load(id)?;
    e.last_fired_ms = now;
    e.fire_count = e.fire_count.saturating_add(1);
    e.active_agent_id = Some(agent_id.to_string());
    save(&e)?;
    Ok(e)
}

/// Drop the overlap claim when `agent_id` was the active fire.
pub fn release_agent(agent_id: &str) -> Result<(), String> {
    for mut e in list()? {
        if e.active_agent_id.as_deref() == Some(agent_id) {
            e.active_agent_id = None;
            save(&e)?;
        }
    }
    Ok(())
}

pub fn mark_path(home: &Path) -> PathBuf {
    home.join("var/schedules")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static HOME_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_home<T>(f: impl FnOnce() -> T) -> T {
        let _g = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!(
            "aos-sched-test-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp AOS_HOME");
        let prev = std::env::var_os("AOS_HOME");
        std::env::set_var("AOS_HOME", &dir);
        let out = f();
        match prev {
            Some(v) => std::env::set_var("AOS_HOME", v),
            None => std::env::remove_var("AOS_HOME"),
        }
        let _ = fs::remove_dir_all(&dir);
        out
    }

    fn sample(goal: &str, interval_secs: u64) -> ScheduleEntry {
        create(&ScheduleCreateRequest {
            goal: goal.into(),
            interval_secs,
            model_id: None,
            next_fire_ms: None,
            display_title: None,
        })
        .expect("create")
    }

    #[test]
    fn pause_blocks_due() {
        with_temp_home(|| {
            let e = sample("ping", 30);
            pause(&e.id).expect("pause");
            let now = e.created_ms.saturating_add(60_000);
            let due_list = due(now, &[]).expect("due");
            assert!(due_list.is_empty());
            resume(&e.id).expect("resume");
            let due_list = due(now, &[]).expect("due after resume");
            assert_eq!(due_list.len(), 1);
        });
    }

    #[test]
    fn next_fire_ms_delays_first_fire() {
        with_temp_home(|| {
            let now = now_ms().max(1);
            let future = now.saturating_add(3600_000);
            let e = create(&ScheduleCreateRequest {
                goal: "later".into(),
                interval_secs: 3600,
                model_id: None,
                next_fire_ms: Some(future),
                display_title: Some("every hour, later".into()),
            })
            .expect("create");
            let early = due(now, &[]).expect("due early");
            assert!(early.is_empty());
            let late = due(future, &[]).expect("due at next_fire");
            assert_eq!(late.len(), 1);
            assert_eq!(late[0].id, e.id);
        });
    }

    #[test]
    fn cancel_sets_stopped() {
        with_temp_home(|| {
            let e = sample("stop", 30);
            cancel(&e.id).expect("cancel");
            let disk = load(&e.id).expect("load");
            assert!(!disk.enabled);
            assert!(!disk.paused);
        });
    }

    #[test]
    fn due_does_not_consume_interval_until_mark_fired() {
        with_temp_home(|| {
            let e = sample("ping", 30);
            let now = e.created_ms.max(1);
            let first = due(now, &[]).expect("due");
            assert_eq!(first.len(), 1);
            assert_eq!(first[0].id, e.id);
            let disk = load(&e.id).expect("load");
            assert_eq!(disk.last_fired_ms, 0);
            assert_eq!(disk.fire_count, 0);
            let again = due(now, &[]).expect("due retry");
            assert_eq!(again.len(), 1, "failed spawn must remain due");
            mark_fired(&e.id, now, "agent-1").expect("mark");
            let after = due(now, &[]).expect("due after mark");
            assert!(after.is_empty());
            let disk = load(&e.id).expect("load after mark");
            assert_eq!(disk.last_fired_ms, now);
            assert_eq!(disk.fire_count, 1);
            assert_eq!(disk.active_agent_id.as_deref(), Some("agent-1"));
        });
    }

    #[test]
    fn due_skips_overlap_while_prior_agent_is_running() {
        with_temp_home(|| {
            let e = sample("long goal", 30);
            let t0 = e.created_ms.max(1);
            mark_fired(&e.id, t0, "agent-9").expect("mark");
            let later = t0.saturating_add(e.interval_secs.saturating_mul(1000));
            let blocked = due(later, &["agent-9".into()]).expect("due running");
            assert!(blocked.is_empty());
            let free = due(later, &[]).expect("due after exit");
            assert_eq!(free.len(), 1);
            release_agent("agent-9").expect("release");
            let disk = load(&e.id).expect("load");
            assert!(disk.active_agent_id.is_none());
        });
    }
}
