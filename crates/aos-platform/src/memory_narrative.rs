//! Scheduling helpers for deterministic Memory V2 period narrations.
//!
//! Narrations are generated for completed local periods.  Keeping the calendar
//! arithmetic here avoids adding a date/time dependency to the platform store
//! and makes the scheduler easy to test with fixed timestamps.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const DAY_MS: i64 = 86_400_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NarrativeWindow {
    pub cadence: &'static str,
    pub key: String,
    pub from_ms: u64,
    pub to_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NarrativeScheduleState {
    #[serde(default)]
    pub weekly: HashMap<String, String>,
    #[serde(default)]
    pub monthly: HashMap<String, String>,
    #[serde(default)]
    pub annual: HashMap<String, String>,
}

impl NarrativeScheduleState {
    pub fn path_for(memory_dir: &Path) -> PathBuf {
        memory_dir.join("narrative_state.json")
    }

    pub fn load(memory_dir: &Path) -> Self {
        let path = Self::path_for(memory_dir);
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, memory_dir: &Path) -> Result<(), String> {
        let path = Self::path_for(memory_dir);
        let tmp = path.with_extension("json.tmp");
        let raw = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, raw).map_err(|e| e.to_string())?;
        std::fs::rename(tmp, path).map_err(|e| e.to_string())
    }

    pub fn last_key(&self, cadence: &str, namespace: &str) -> Option<&str> {
        let map = match cadence {
            "weekly" => &self.weekly,
            "monthly" => &self.monthly,
            "annual" => &self.annual,
            _ => return None,
        };
        map.get(namespace).map(String::as_str)
    }

    pub fn set_key(&mut self, cadence: &str, namespace: &str, key: String) {
        let map = match cadence {
            "weekly" => &mut self.weekly,
            "monthly" => &mut self.monthly,
            "annual" => &mut self.annual,
            _ => return,
        };
        map.insert(namespace.to_string(), key);
    }
}

/// Return the last completed local week, month and year.
pub fn completed_windows(now_ms: u64, offset_minutes: i32) -> Vec<NarrativeWindow> {
    let offset_ms = i64::from(offset_minutes) * 60_000;
    let local_day = (now_ms as i64 + offset_ms).div_euclid(DAY_MS);
    let week_start = local_day - local_day.rem_euclid(7) - 7;
    let (year, month, _) = civil_from_days(local_day);

    let (previous_month_year, previous_month) = if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    };
    let previous_year = year - 1;

    vec![
        window("weekly", format!("week-{week_start}"), week_start, week_start + 7, offset_ms),
        {
            let start = days_from_civil(previous_month_year, previous_month, 1);
            let (next_year, next_month) = if previous_month == 12 {
                (previous_month_year + 1, 1)
            } else {
                (previous_month_year, previous_month + 1)
            };
            let end = days_from_civil(next_year, next_month, 1);
            window(
                "monthly",
                format!("month-{previous_month_year:04}-{previous_month:02}"),
                start,
                end,
                offset_ms,
            )
        },
        {
            let start = days_from_civil(previous_year, 1, 1);
            let end = days_from_civil(previous_year + 1, 1, 1);
            window("annual", format!("year-{previous_year:04}"), start, end, offset_ms)
        },
    ]
}

fn window(
    cadence: &'static str,
    key: String,
    start_day: i64,
    end_day: i64,
    offset_ms: i64,
) -> NarrativeWindow {
    let from = (start_day * DAY_MS - offset_ms).max(0) as u64;
    let to = (end_day * DAY_MS - offset_ms).max(0) as u64;
    NarrativeWindow { cadence, key, from_ms: from, to_ms: to }
}

// Howard Hinnant's civil calendar conversion, using the Unix epoch as day 0.
fn civil_from_days(days: i64) -> (i32, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year as i32, m, d)
}

fn days_from_civil(year: i32, month: i64, day: i64) -> i64 {
    let year = i64::from(year) - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let mp = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_periods_are_before_current_local_date() {
        let windows = completed_windows(1_700_000_000_000, 60);
        assert_eq!(windows.len(), 3);
        assert!(windows.iter().all(|window| window.from_ms < window.to_ms));
        assert!(windows.iter().all(|window| window.to_ms <= 1_700_000_000_000));
        assert_eq!(windows[0].cadence, "weekly");
        assert_eq!(windows[1].cadence, "monthly");
        assert_eq!(windows[2].cadence, "annual");
    }

    #[test]
    fn state_is_keyed_by_namespace_and_cadence() {
        let mut state = NarrativeScheduleState::default();
        state.set_key("monthly", "project:akasha", "month-2026-08".into());
        assert_eq!(state.last_key("monthly", "project:akasha"), Some("month-2026-08"));
        assert!(state.last_key("weekly", "project:akasha").is_none());
    }

    #[test]
    fn leap_year_month_bounds_are_correct() {
        let start = days_from_civil(2024, 2, 1);
        let end = days_from_civil(2024, 3, 1);
        assert_eq!(end - start, 29);
    }
}
