//! Small, persistent-state-aware scheduler for platform maintenance jobs.
//!
//! Jobs own their payload and persisted `last_local_day_key`; this module owns
//! the common timing policy so a job can be safely caught up after downtime.

/// A local-time window in which a daily job may run. `None` means no end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DailyWindow {
    pub start_hour: i32,
    pub end_hour_exclusive: Option<i32>,
}

pub const ANY_TIME: DailyWindow = DailyWindow {
    start_hour: 0,
    end_hour_exclusive: None,
};

pub fn local_hour(now_ms: u64, offset_minutes: i32) -> i32 {
    let local_ms = now_ms as i64 + i64::from(offset_minutes) * 60_000;
    (local_ms.rem_euclid(86_400_000) / 3_600_000) as i32
}

/// True exactly when a job has not run for `current_day_key` and its window is
/// currently open. This is safe for both periodic heartbeats and startup
/// catch-up checks.
pub fn daily_job_due(
    last_local_day_key: &str,
    current_day_key: &str,
    now_ms: u64,
    offset_minutes: i32,
    window: DailyWindow,
) -> bool {
    if last_local_day_key == current_day_key {
        return false;
    }
    let hour = local_hour(now_ms, offset_minutes);
    hour >= window.start_hour
        && window
            .end_hour_exclusive
            .is_none_or(|end_hour| hour < end_hour)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_once_inside_its_window_and_catches_up_after_downtime() {
        let day = "day-4";
        let three_am = 86_400_000u64 * 4 + 3 * 3_600_000;
        let night = DailyWindow {
            start_hour: 2,
            end_hour_exclusive: Some(4),
        };
        assert!(daily_job_due("day-3", day, three_am, 0, night));
        assert!(!daily_job_due(day, day, three_am, 0, night));
        assert!(!daily_job_due(
            "day-3",
            day,
            three_am + 3 * 3_600_000,
            0,
            night
        ));

        let catch_up = DailyWindow {
            start_hour: 5,
            end_hour_exclusive: None,
        };
        assert!(daily_job_due(
            "day-3",
            day,
            three_am + 3 * 3_600_000,
            0,
            catch_up
        ));
    }
}
