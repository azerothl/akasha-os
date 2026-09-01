//! Mutable state for guided cohort scenarios and their pending probes.

#[derive(Default)]
pub(crate) struct ScenarioUiState {
    pub(crate) chat: bool,
    pub(crate) note_human: bool,
    pub(crate) note_agent: bool,
    pub(crate) confirm: bool,
    pub(crate) audit: bool,
    pub(crate) module_agent: bool,
    pub(crate) pending_note_agent: bool,
    pub(crate) pending_module_agent: bool,
    pub(crate) pending_module_baseline: Vec<String>,
}

impl ScenarioUiState {
    pub(crate) fn arm_module_agent(&mut self, baseline: Vec<String>) {
        self.pending_module_agent = true;
        self.pending_module_baseline = baseline;
    }

    pub(crate) fn mark_module_agent(&mut self) {
        self.module_agent = true;
        self.pending_module_agent = false;
        self.pending_module_baseline.clear();
    }

    pub(crate) fn mark_note_agent_pending(&mut self) {
        self.pending_note_agent = true;
    }

    pub(crate) fn mark_note_agent(&mut self) {
        self.note_agent = true;
        self.pending_note_agent = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_probe_tracks_baseline_until_completion() {
        let mut state = ScenarioUiState::default();
        state.arm_module_agent(vec!["existing".into()]);
        assert!(state.pending_module_agent);
        assert_eq!(state.pending_module_baseline, vec!["existing"]);
        state.mark_module_agent();
        assert!(state.module_agent);
        assert!(!state.pending_module_agent);
        assert!(state.pending_module_baseline.is_empty());
    }

    #[test]
    fn note_probe_is_consumed_when_agent_writes() {
        let mut state = ScenarioUiState::default();
        state.mark_note_agent_pending();
        assert!(state.pending_note_agent);
        state.mark_note_agent();
        assert!(state.note_agent);
        assert!(!state.pending_note_agent);
    }
}
