//! Schedule cards in the chat thread (live / paused / stopped).

use aos_agent::schedule::ScheduleEntry;
use aos_proto::ChatAttachment;
use eframe::egui;

use crate::cmd::Cmd;
use crate::i18n::UiStrings;
use crate::format_schedule_next_label;

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
                if ui.button(t.schedule_stop).clicked() {
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
    }
}
