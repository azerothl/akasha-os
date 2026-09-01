//! Event handling for schedule synchronization and chat-card lifecycle.

use crate::UiApp;
use aos_agent::schedule::ScheduleEntry;

pub(crate) fn on_schedules(app: &mut UiApp, entries: Vec<ScheduleEntry>) {
    app.schedule_ui.merge_entries(entries);
    app.sync_schedule_cards();
}

pub(crate) fn on_schedule_created(app: &mut UiApp, entry: ScheduleEntry) {
    app.upsert_schedule_entry(entry.clone());
    app.attach_schedule_card(&entry);
    app.sync_schedule_cards();
}

pub(crate) fn on_schedule_updated(app: &mut UiApp, entry: ScheduleEntry) {
    app.upsert_schedule_entry(entry);
    app.sync_schedule_cards();
}
