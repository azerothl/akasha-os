//! Mutable state for pending capability/tool confirmations.

use aos_proto::PendingConfirmation;

#[derive(Default)]
pub(crate) struct ConfirmationUiState {
    pub(crate) pending: Vec<PendingConfirmation>,
}

impl ConfirmationUiState {
    pub(crate) fn replace(&mut self, confirmations: Vec<PendingConfirmation>) {
        self.pending = confirmations;
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_updates_pending_count() {
        let mut state = ConfirmationUiState::default();
        assert!(state.is_empty());
        state.replace(vec![PendingConfirmation {
            id: "confirm-1".into(),
            actor: "agent:a1".into(),
            action: "notes.write".into(),
            target: "a.md".into(),
            reason: "write note".into(),
            deadline_ts_ms: 0,
        }]);
        assert!(!state.is_empty());
        assert_eq!(state.len(), 1);
    }
}
