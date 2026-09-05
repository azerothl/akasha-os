//! Pure date, metric, byte-size, and memory-relation formatting helpers.

use crate::i18n::UiStrings;
use aos_proto::{MemHit, MemRelationKind};
use std::collections::HashMap;

pub(crate) fn chrono_like_stamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

pub(crate) fn format_local_time_hm(ts_ms: u64, offset_minutes: i32) -> String {
    let (hours, mins, _) = local_hms(ts_ms, offset_minutes);
    format!("{hours:02}:{mins:02}")
}

pub(crate) fn format_local_time_hms(ts_ms: u64, offset_minutes: i32) -> String {
    let (hours, mins, secs) = local_hms(ts_ms, offset_minutes);
    format!("{hours:02}:{mins:02}:{secs:02}")
}

fn local_hms(ts_ms: u64, offset_minutes: i32) -> (i64, i64, i64) {
    let local_ms = ts_ms as i64 + (offset_minutes as i64) * 60_000;
    let secs = local_ms.div_euclid(1000);
    let hours = (secs / 3600) % 24;
    let mins = (secs / 60) % 60;
    let sec = secs % 60;
    (hours, mins, sec)
}

/// Clock for a chat bubble: `HH:MM:SS`, or `dd/mm/yyyy HH:MM:SS` when not today.
pub(crate) fn format_chat_stamp(ts_ms: u64, now_ms: u64, offset_minutes: i32) -> String {
    if ts_ms == 0 {
        return String::new();
    }
    let time = format_local_time_hms(ts_ms, offset_minutes);
    if local_day_index(ts_ms, offset_minutes) == local_day_index(now_ms, offset_minutes) {
        time
    } else {
        format!("{} {time}", format_local_date_short(ts_ms, offset_minutes))
    }
}

pub(crate) fn local_tz_offset_minutes() -> i32 {
    if let Ok(out) = std::process::Command::new("date").args(["+%z"]).output() {
        if out.status.success() {
            if let Ok(s) = String::from_utf8(out.stdout) {
                return parse_tz_offset_minutes(s.trim()).unwrap_or(0);
            }
        }
    }
    0
}

pub(crate) fn parse_tz_offset_minutes(raw: &str) -> Option<i32> {
    let s = raw.trim();
    if s.len() < 3 {
        return None;
    }
    let sign = match s.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let digits: String = s.chars().skip(1).filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 3 {
        return None;
    }
    let hours: i32 = digits[..digits.len().saturating_sub(2)].parse().ok()?;
    let mins: i32 = digits[digits.len().saturating_sub(2)..].parse().ok()?;
    Some(sign * (hours * 60 + mins))
}

pub(crate) fn local_day_index(ts_ms: u64, offset_minutes: i32) -> i64 {
    let local_ms = ts_ms as i64 + (offset_minutes as i64) * 60_000;
    local_ms.div_euclid(86_400_000)
}

pub(crate) fn civil_from_day_index(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 {
        z / 146097
    } else {
        (z - 146096) / 146097
    };
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if mp < 10 { y } else { y + 1 };
    (y, m, d)
}

pub(crate) fn format_local_date_short(ts_ms: u64, tz_offset_min: i32) -> String {
    let days = local_day_index(ts_ms, tz_offset_min);
    let (y, m, d) = civil_from_day_index(days);
    format!("{d:02}/{m:02}/{y}")
}

pub(crate) fn format_local_datetime(ts_ms: u64, offset_minutes: i32) -> String {
    if ts_ms == 0 {
        return String::new();
    }
    format!(
        "{} {}",
        format_local_date_short(ts_ms, offset_minutes),
        format_local_time_hms(ts_ms, offset_minutes)
    )
}

pub(crate) fn format_schedule_next_label(
    t: &UiStrings,
    next_fire_ms: u64,
    now_ms: u64,
    tz_offset_min: i32,
) -> String {
    let time = format_local_time_hm(next_fire_ms, tz_offset_min);
    let day_now = local_day_index(now_ms, tz_offset_min);
    let day_next = local_day_index(next_fire_ms, tz_offset_min);
    if day_next == day_now {
        t.schedule_card_next_today.replace("{time}", &time)
    } else if day_next == day_now + 1 {
        t.schedule_card_next_tomorrow.replace("{time}", &time)
    } else {
        let date = format_local_date_short(next_fire_ms, tz_offset_min);
        t.schedule_card_next_date
            .replace("{date}", &date)
            .replace("{time}", &time)
    }
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(crate) fn memory_relation_snippet(text: &str) -> String {
    let t = text.trim();
    if t.chars().count() <= 80 {
        t.to_string()
    } else {
        let end = t.char_indices().nth(80).map(|(i, _)| i).unwrap_or(t.len());
        format!("{}…", &t[..end])
    }
}

pub(crate) fn memory_relation_lines(
    hit: &MemHit,
    texts: &HashMap<u64, String>,
    t: &UiStrings,
) -> Vec<String> {
    let mut lines = Vec::new();
    for rel in &hit.relations {
        let Some(target) = texts.get(&rel.to) else {
            continue;
        };
        if !aos_proto::mem_extract::is_human_memory_fact(target) {
            continue;
        }
        let snippet = memory_relation_snippet(target);
        let line = match rel.rel {
            MemRelationKind::Supersedes => t.memory_rel_replaces.replace("{}", &snippet),
            MemRelationKind::Similar | MemRelationKind::Updates => {
                t.memory_rel_related_to.replace("{}", &snippet)
            }
        };
        lines.push(line);
    }
    lines
}

pub(crate) fn human_bytes(v: u64) -> String {
    const GIB: f64 = (1u64 << 30) as f64;
    const MIB: f64 = (1u64 << 20) as f64;
    if v >= (1u64 << 30) {
        format!("{:.2} GiB", v as f64 / GIB)
    } else if v >= (1u64 << 20) {
        format!("{:.1} MiB", v as f64 / MIB)
    } else {
        format!("{v} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UTC_2024_01_01: u64 = 1_704_067_200_000;

    #[test]
    fn chat_stamp_same_day_is_hms_only() {
        let now = UTC_2024_01_01 + 3_600_000;
        assert_eq!(format_chat_stamp(UTC_2024_01_01, now, 0), "00:00:00");
    }

    #[test]
    fn chat_stamp_other_day_includes_date() {
        let now = UTC_2024_01_01 + 86_400_000;
        assert_eq!(
            format_chat_stamp(UTC_2024_01_01, now, 0),
            "01/01/2024 00:00:00"
        );
    }

    #[test]
    fn local_datetime_joins_date_and_clock() {
        assert_eq!(
            format_local_datetime(UTC_2024_01_01, 0),
            "01/01/2024 00:00:00"
        );
        assert_eq!(
            format_local_datetime(UTC_2024_01_01, 60),
            "01/01/2024 01:00:00"
        );
        assert!(format_local_datetime(0, 0).is_empty());
    }
}
