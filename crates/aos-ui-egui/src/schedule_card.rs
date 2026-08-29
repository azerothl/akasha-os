//! Schedule cards in the chat thread (live / paused / stopped).

use aos_agent::schedule::ScheduleEntry;
use aos_proto::ChatAttachment;
use eframe::egui;

use crate::cmd::Cmd;
use crate::i18n::UiStrings;
use crate::format_schedule_next_label;

#[derive(Clone)]
pub enum ScheduleCardAction {
    None,
    Pause(String),
    Resume(String),
    Stop(String),
}

pub fn card_state_from_entry(entry: &ScheduleEntry) -> &'static str {
    if !entry.enabled {
        "stopped"
    } else if entry.paused {
        "paused"
    } else {
        "live"
    }
}

pub fn next_fire_ms_for_entry(entry: &ScheduleEntry, now_ms: u64) -> u64 {
    if entry.last_fired_ms == 0 {
        entry.next_fire_ms.unwrap_or(now_ms)
    } else {
        entry
            .last_fired_ms
            .saturating_add(entry.interval_secs.saturating_mul(1000))
    }
}

pub fn status_line(
    t: &UiStrings,
    state: &str,
    next_fire_ms: u64,
    now_ms: u64,
    tz_offset_min: i32,
) -> String {
    match state {
        "paused" => t.schedule_card_paused.into(),
        "stopped" => t.schedule_card_stopped.into(),
        _ => format_schedule_next_label(t, next_fire_ms, now_ms, tz_offset_min),
    }
}

pub fn render_schedule_card(
    ui: &mut egui::Ui,
    t: &UiStrings,
    title: &str,
    state: &str,
    next_fire_ms: u64,
    schedule_id: &str,
    now_ms: u64,
    tz_offset_min: i32,
) -> ScheduleCardAction {
    let status = status_line(t, state, next_fire_ms, now_ms, tz_offset_min);
    ui.group(|ui| {
        ui.label(egui::RichText::new(title.trim()).strong());
        ui.weak(&status);
        match state {
            "live" => ui.horizontal(|ui| {
                if ui.button(t.schedule_pause).clicked() {
                    ScheduleCardAction::Pause(schedule_id.to_string())
                } else if ui.button(t.schedule_stop).clicked() {
                    ScheduleCardAction::Stop(schedule_id.to_string())
                } else {
                    ScheduleCardAction::None
                }
            }),
            "paused" => ui.horizontal(|ui| {
                if ui.button(t.schedule_resume).clicked() {
                    ScheduleCardAction::Resume(schedule_id.to_string())
                } else if ui.button(t.schedule_stop).clicked() {
                    ScheduleCardAction::Stop(schedule_id.to_string())
                } else {
                    ScheduleCardAction::None
                }
            }),
            _ => ui.horizontal(|ui| {
                ui.add_enabled(false, egui::Label::new(""));
                ScheduleCardAction::None
            }),
        }
        .inner
    })
    .inner
}

pub fn sync_card_attachment(att: &mut ChatAttachment, entry: &ScheduleEntry, now_ms: u64) {
    if let ChatAttachment::ScheduleCard {
        state,
        next_fire_ms,
        ..
    } = att
    {
        *state = card_state_from_entry(entry).to_string();
        *next_fire_ms = next_fire_ms_for_entry(entry, now_ms);
    }
}

/// Prefer persisted schedule state over a stale attachment snapshot.
pub fn resolved_card_state<'a>(
    entry: Option<&'a ScheduleEntry>,
    attachment_state: &'a str,
) -> &'a str {
    entry
        .map(card_state_from_entry)
        .unwrap_or(attachment_state)
}

pub fn upsert_schedule_entry(schedules: &mut Vec<ScheduleEntry>, entry: ScheduleEntry) {
    if let Some(slot) = schedules.iter_mut().find(|s| s.id == entry.id) {
        *slot = entry;
    } else {
        schedules.push(entry);
    }
}

pub fn apply_local_pause(schedules: &mut Vec<ScheduleEntry>, id: &str) {
    if let Some(entry) = schedules.iter_mut().find(|s| s.id == id) {
        entry.paused = true;
    }
}

pub fn apply_local_resume(schedules: &mut Vec<ScheduleEntry>, id: &str) {
    if let Some(entry) = schedules.iter_mut().find(|s| s.id == id) {
        entry.paused = false;
    }
}

pub fn apply_local_stop(schedules: &mut Vec<ScheduleEntry>, id: &str) {
    if let Some(entry) = schedules.iter_mut().find(|s| s.id == id) {
        entry.enabled = false;
        entry.paused = false;
    }
}

pub fn apply_local_action_to_attachment(
    att: &mut ChatAttachment,
    schedules: &[ScheduleEntry],
    id: &str,
    now_ms: u64,
) {
    if let Some(entry) = schedules.iter().find(|s| s.id == id) {
        sync_card_attachment(att, entry, now_ms);
    }
}

pub fn send_schedule_action(cmd_tx: &std::sync::mpsc::Sender<Cmd>, action: ScheduleCardAction) {
    match action {
        ScheduleCardAction::Pause(id) => {
            let _ = cmd_tx.send(Cmd::SchedulePause { id });
        }
        ScheduleCardAction::Resume(id) => {
            let _ = cmd_tx.send(Cmd::ScheduleResume { id });
        }
        ScheduleCardAction::Stop(id) => {
            let _ = cmd_tx.send(Cmd::ScheduleCancel { id });
        }
        ScheduleCardAction::None => {}
    }
}

/// Primary/secondary action labels per card state (for UI + tests).
pub fn action_labels_for_state<'a>(t: &'a UiStrings, state: &str) -> Option<(&'a str, &'a str)> {
    match state {
        "live" => Some((t.schedule_pause, t.schedule_stop)),
        "paused" => Some((t.schedule_resume, t.schedule_stop)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_states_live_paused_stopped() {
        let entry = ScheduleEntry {
            id: "sch-1".into(),
            goal: "g".into(),
            interval_secs: 3600,
            enabled: true,
            paused: false,
            next_fire_ms: Some(1000),
            display_title: Some("title".into()),
            last_fired_ms: 0,
            fire_count: 0,
            active_agent_id: None,
            model_id: None,
            created_ms: 0,
        };
        assert_eq!(card_state_from_entry(&entry), "live");
        let mut paused = entry.clone();
        paused.paused = true;
        assert_eq!(card_state_from_entry(&paused), "paused");
        let mut stopped = entry.clone();
        stopped.enabled = false;
        assert_eq!(card_state_from_entry(&stopped), "stopped");
    }

    #[test]
    fn status_line_paused_stopped_i18n() {
        let t_en = crate::i18n::strings("en");
        let t_fr = crate::i18n::strings("fr");
        assert_eq!(status_line(&t_en, "paused", 0, 0, 0), "Paused");
        assert_eq!(status_line(&t_fr, "paused", 0, 0, 0), "En pause");
        assert_eq!(status_line(&t_en, "stopped", 0, 0, 0), "Stopped");
        assert_eq!(status_line(&t_fr, "stopped", 0, 0, 0), "Arrêté");
    }

    #[test]
    fn settings_expert_keys_unchanged() {
        let t_en = crate::i18n::strings("en");
        let t_fr = crate::i18n::strings("fr");
        assert_eq!(t_en.schedule_heading, "Schedules");
        assert_eq!(t_fr.schedule_heading, "Planifications");
        assert_eq!(t_en.schedule_interval, "Interval (seconds)");
        assert_eq!(t_fr.schedule_interval, "Intervalle (secondes)");
    }

    #[test]
    fn locked_button_labels() {
        let t_en = crate::i18n::strings("en");
        let t_fr = crate::i18n::strings("fr");
        assert_eq!(t_en.schedule_pause, "Pause");
        assert_eq!(t_fr.schedule_pause, "Pause");
        assert_eq!(t_en.schedule_stop, "Stop");
        assert_eq!(t_fr.schedule_stop, "Arrêter");
        assert_eq!(t_en.schedule_resume, "Resume");
        assert_eq!(t_fr.schedule_resume, "Reprendre");
    }

    #[test]
    fn paused_card_exposes_resume_and_stop_not_pause() {
        let t_en = crate::i18n::strings("en");
        let t_fr = crate::i18n::strings("fr");
        let live = action_labels_for_state(&t_en, "live").unwrap();
        let paused = action_labels_for_state(&t_en, "paused").unwrap();
        assert_eq!(live.0, "Pause");
        assert_eq!(paused.0, "Resume");
        assert_ne!(paused.0, live.0);
        assert_eq!(paused.1, "Stop");
        let paused_fr = action_labels_for_state(&t_fr, "paused").unwrap();
        assert_eq!(paused_fr.0, "Reprendre");
        assert_eq!(paused_fr.1, "Arrêter");
        assert!(action_labels_for_state(&t_en, "stopped").is_none());
    }

    #[test]
    fn date_fallback_contains_time_once() {
        let t_en = crate::i18n::strings("en");
        let tz = 0;
        let now_ms = 1_705_315_200_000u64; // 2024-01-15 10:00 UTC
        let next_ms = now_ms + 3 * 86_400_000; // three local days later
        let label = crate::format_schedule_next_label(&t_en, next_ms, now_ms, tz);
        let time = crate::format_local_time_hm(next_ms, tz);
        assert!(
            label.contains(&time),
            "expected time {time} in {label}"
        );
        assert_eq!(
            label.matches(&time).count(),
            1,
            "time must appear once in {label}"
        );
        assert!(
            !label.starts_with("Next: today"),
            "three-day offset must not use today template"
        );
    }

    #[test]
    fn sync_after_pause_keeps_paused_without_stale_attachment() {
        let entry = ScheduleEntry {
            id: "sch-1".into(),
            goal: "g".into(),
            interval_secs: 86_400,
            enabled: true,
            paused: true,
            next_fire_ms: Some(1_000),
            display_title: Some("every morning, g".into()),
            last_fired_ms: 0,
            fire_count: 0,
            active_agent_id: None,
            model_id: None,
            created_ms: 0,
        };
        let mut att = ChatAttachment::ScheduleCard {
            schedule_id: "sch-1".into(),
            title: "every morning, g".into(),
            goal: "g".into(),
            interval_secs: 86_400,
            next_fire_ms: 1_000,
            state: "live".into(),
        };
        sync_card_attachment(&mut att, &entry, 500);
        let ChatAttachment::ScheduleCard { state, .. } = att else {
            panic!("expected schedule card");
        };
        assert_eq!(state, "paused");
        assert_eq!(card_state_from_entry(&entry), "paused");
        assert_eq!(resolved_card_state(Some(&entry), "live"), "paused");
    }

    #[test]
    fn apply_local_pause_updates_entry_and_attachment() {
        let mut schedules = vec![ScheduleEntry {
            id: "sch-9".into(),
            goal: "g".into(),
            interval_secs: 3600,
            enabled: true,
            paused: false,
            next_fire_ms: Some(9_000),
            display_title: None,
            last_fired_ms: 0,
            fire_count: 0,
            active_agent_id: None,
            model_id: None,
            created_ms: 0,
        }];
        let mut att = ChatAttachment::ScheduleCard {
            schedule_id: "sch-9".into(),
            title: "hourly, g".into(),
            goal: "g".into(),
            interval_secs: 3600,
            next_fire_ms: 9_000,
            state: "live".into(),
        };
        apply_local_pause(&mut schedules, "sch-9");
        apply_local_action_to_attachment(&mut att, &schedules, "sch-9", 100);
        assert_eq!(card_state_from_entry(&schedules[0]), "paused");
        let ChatAttachment::ScheduleCard { state, .. } = att else {
            panic!("expected card");
        };
        assert_eq!(state, "paused");
    }
}
