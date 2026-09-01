//! Mutable runtime state for schedules and their chat cards.

use aos_agent::schedule::ScheduleEntry;

#[derive(Default)]
pub(crate) struct ScheduleUiState {
    pub(crate) entries: Vec<ScheduleEntry>,
    /// Act id waiting for `Evt::ScheduleCreated` to attach a thread card.
    pub(crate) pending_card_act: Option<String>,
    /// Protects locally edited schedule cards from being clobbered by a reload.
    pub(crate) transcript_dirty: bool,
}

impl ScheduleUiState {
    pub(crate) fn mark_transcript_dirty(&mut self) {
        self.transcript_dirty = true;
    }

    pub(crate) fn clear_transcript_dirty(&mut self) {
        self.transcript_dirty = false;
    }

    pub(crate) fn set_pending_card_act(&mut self, act_id: String) {
        self.pending_card_act = Some(act_id);
    }

    pub(crate) fn take_pending_card_act(&mut self) -> Option<String> {
        self.pending_card_act.take()
    }

    pub(crate) fn upsert_entry(&mut self, entry: ScheduleEntry) {
        crate::schedule_card::upsert_schedule_entry(&mut self.entries, entry);
    }

    pub(crate) fn merge_entries(&mut self, entries: Vec<ScheduleEntry>) {
        crate::schedule_card::merge_schedule_list(&mut self.entries, entries);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_card_act_is_consumed_once() {
        let mut state = ScheduleUiState::default();
        assert!(state.take_pending_card_act().is_none());
        state.set_pending_card_act("act-1".into());
        assert_eq!(state.take_pending_card_act().as_deref(), Some("act-1"));
        assert!(state.take_pending_card_act().is_none());
    }

    #[test]
    fn transcript_dirty_can_be_reset_after_navigation() {
        let mut state = ScheduleUiState::default();
        assert!(!state.transcript_dirty);
        state.mark_transcript_dirty();
        assert!(state.transcript_dirty);
        state.clear_transcript_dirty();
        assert!(!state.transcript_dirty);
    }
}
