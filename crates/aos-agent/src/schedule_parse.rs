//! Parse human schedule phrases (FR/EN) into interval + next-fire alignment.
//!
//! No cron language — only a small set of morning/hourly patterns for chat.

use std::time::{SystemTime, UNIX_EPOCH};

/// Parsed schedule intent from a chat phrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSchedule {
    /// Agent goal (what to do).
    pub goal: String,
    /// Human when fragment for act copy (e.g. "every morning", "chaque matin").
    pub when_label: String,
    /// Interval between fires in seconds (minimum 30).
    pub interval_secs: u64,
    /// First fire instant (epoch ms, local-aligned where applicable).
    pub next_fire_ms: u64,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn local_day_start_ms(now_ms: u64, tz_offset_min: i32) -> u64 {
    let local_ms = now_ms as i64 + (tz_offset_min as i64) * 60_000;
    let local_secs = local_ms.div_euclid(1000);
    let day_secs = local_secs - (local_secs % 86_400);
    (day_secs * 1000 - (tz_offset_min as i64) * 60_000).max(0) as u64
}

fn local_hm_ms(day_start_ms: u64, hour: u64, minute: u64, _tz_offset_min: i32) -> u64 {
    day_start_ms.saturating_add((hour * 3600 + minute * 60) * 1000)
}

/// Align next daily fire at `hour`:`minute` local time.
pub fn next_daily_fire_ms(now_ms: u64, tz_offset_min: i32, hour: u64, minute: u64) -> u64 {
    let day_start = local_day_start_ms(now_ms, tz_offset_min);
    let target = local_hm_ms(day_start, hour, minute, tz_offset_min);
    if now_ms < target {
        target
    } else {
        local_hm_ms(day_start.saturating_add(86_400_000), hour, minute, tz_offset_min)
    }
}

/// Align next hourly fire at the next hour boundary (local).
pub fn next_hourly_fire_ms(now_ms: u64, tz_offset_min: i32) -> u64 {
    let local_ms = now_ms as i64 + (tz_offset_min as i64) * 60_000;
    let local_secs = local_ms.div_euclid(1000);
    let next_hour_secs = local_secs - (local_secs % 3600) + 3600;
    let utc_ms = next_hour_secs * 1000 - (tz_offset_min as i64) * 60_000;
    utc_ms.max(now_ms as i64) as u64
}

fn normalize(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Try to parse a chat phrase into a schedule. Returns `None` if not a schedule phrase.
pub fn try_parse_phrase(phrase: &str, now_ms: u64, tz_offset_min: i32) -> Option<ParsedSchedule> {
    let raw = phrase.trim();
    if raw.is_empty() {
        return None;
    }
    let lower = normalize(raw);

    // Morning patterns — default 08:00 local.
    const MORNING_PREFIXES: &[(&str, &str, &str)] = &[
        ("every morning", "every morning", "every morning"),
        ("each morning", "each morning", "each morning"),
        ("chaque matin", "chaque matin", "chaque matin"),
        ("tous les matins", "tous les matins", "tous les matins"),
    ];
    for (prefix, when_en, when_fr) in MORNING_PREFIXES {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let goal = rest.trim_start_matches([',', ' ', ':', '-']).trim();
            if goal.is_empty() {
                return None;
            }
            let when_label = if prefix.contains("matin") {
                when_fr.to_string()
            } else {
                when_en.to_string()
            };
            return Some(ParsedSchedule {
                goal: goal.to_string(),
                when_label,
                interval_secs: 86_400,
                next_fire_ms: next_daily_fire_ms(now_ms, tz_offset_min, 8, 0),
            });
        }
    }

    // Hourly patterns.
    const HOURLY_PREFIXES: &[(&str, &str, &str)] = &[
        ("every hour", "every hour", "every hour"),
        ("each hour", "each hour", "each hour"),
        ("hourly", "hourly", "hourly"),
        ("toutes les heures", "toutes les heures", "toutes les heures"),
        ("chaque heure", "chaque heure", "chaque heure"),
    ];
    for (prefix, when_en, when_fr) in HOURLY_PREFIXES {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let goal = rest.trim_start_matches([',', ' ', ':', '-']).trim();
            if goal.is_empty() {
                return None;
            }
            let when_label = if prefix.contains("heure") {
                when_fr.to_string()
            } else {
                when_en.to_string()
            };
            return Some(ParsedSchedule {
                goal: goal.to_string(),
                when_label,
                interval_secs: 3600,
                next_fire_ms: next_hourly_fire_ms(now_ms, tz_offset_min),
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn en_morning_phrase() {
        // 2024-01-15 10:00 UTC, offset 0 → next fire tomorrow 08:00 UTC
        let now = 1_705_315_200_000u64; // Mon 2024-01-15 10:00:00 UTC
        let parsed = try_parse_phrase("every morning, summarize my notes", now, 0).unwrap();
        assert_eq!(parsed.goal, "summarize my notes");
        assert_eq!(parsed.when_label, "every morning");
        assert_eq!(parsed.interval_secs, 86_400);
        assert!(parsed.next_fire_ms > now);
        let hm = format_local_hm(parsed.next_fire_ms, 0);
        assert_eq!(hm, "08:00");
    }

    #[test]
    fn fr_morning_phrase() {
        let now = 1_705_315_200_000u64;
        let parsed =
            try_parse_phrase("chaque matin, résume mes notes", now, 60).unwrap();
        assert_eq!(parsed.goal, "résume mes notes");
        assert_eq!(parsed.when_label, "chaque matin");
        assert_eq!(parsed.interval_secs, 86_400);
    }

    #[test]
    fn en_hourly_phrase() {
        let now = 1_705_315_200_000u64; // :00:00
        let parsed = try_parse_phrase("every hour, check inbox", now, 0).unwrap();
        assert_eq!(parsed.goal, "check inbox");
        assert_eq!(parsed.when_label, "every hour");
        assert_eq!(parsed.interval_secs, 3600);
        assert!(parsed.next_fire_ms >= now);
    }

    #[test]
    fn fr_hourly_phrase() {
        let now = 1_705_315_200_000u64;
        let parsed = try_parse_phrase("toutes les heures, vérifie la boîte", now, 0).unwrap();
        assert_eq!(parsed.goal, "vérifie la boîte");
        assert_eq!(parsed.when_label, "toutes les heures");
        assert_eq!(parsed.interval_secs, 3600);
    }

    #[test]
    fn non_schedule_returns_none() {
        assert!(try_parse_phrase("hello world", now_ms(), 0).is_none());
        assert!(try_parse_phrase("every morning", now_ms(), 0).is_none());
    }

    fn format_local_hm(ts_ms: u64, offset_min: i32) -> String {
        let local_ms = ts_ms as i64 + (offset_min as i64) * 60_000;
        let secs = local_ms.div_euclid(1000);
        let mins = (secs / 60) % 60;
        let hours = (secs / 3600) % 24;
        format!("{hours:02}:{mins:02}")
    }
}
