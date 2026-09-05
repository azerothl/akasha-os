//! Health-check for live agents: detect stalls and choose a recovery action.
//!
//! Explicit user pause is never auto-resumed. `user.ask` and act-gate waits
//! get a longer window than a Running worker that has gone silent.

use crate::agent_act::requires_act_gate;
use crate::context_budget::is_infer_stall_error;
use aos_proto::{AgentKind, AgentOutputEvent, AgentState};
use std::time::Duration;

/// No reports while Running / Created before the first nudge.
pub const RUNNING_STALL: Duration = Duration::from_secs(4 * 60);
/// New worker that never reached Running.
pub const CREATED_STALL: Duration = Duration::from_secs(90);
/// `user.ask` wait (worker timeout is 10 min) plus a short grace.
pub const BLOCKED_ASK: Duration = Duration::from_secs(11 * 60);
/// Inline act-gate wait (worker cap is 5 min) plus grace.
pub const BLOCKED_GATE: Duration = Duration::from_secs(6 * 60);
/// Blocked on something else (child wait, inconsistent state).
pub const BLOCKED_OTHER: Duration = Duration::from_secs(3 * 60);
/// Auto-recoveries before the agent is marked Failed.
pub const MAX_RECOVERIES: u32 = 2;

#[derive(Debug, Clone)]
pub struct HealthSample {
    pub state: AgentState,
    pub kind: AgentKind,
    pub pid: Option<u32>,
    pub last_action: String,
    pub fail_reason: Option<String>,
    pub idle: Duration,
    pub recoveries: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthAction {
    None,
    /// Steer (+ Resume) while the worker is still alive.
    Nudge,
    /// Steer + Resume a Blocked worker past its legitimate wait.
    Unblock,
    /// Kill the worker and restore it from disk.
    Restart,
    /// Give up: mark Failed.
    MarkFailed,
}

/// Reports that mean the worker is actually making progress (not a health log).
pub fn event_proves_progress(event: &AgentOutputEvent) -> bool {
    match event {
        AgentOutputEvent::Token { .. }
        | AgentOutputEvent::Progress { .. }
        | AgentOutputEvent::Step(_)
        | AgentOutputEvent::ChildDone { .. }
        | AgentOutputEvent::ChildSpawned { .. } => true,
        AgentOutputEvent::StateChanged { state } => {
            matches!(state, AgentState::Running)
        }
        _ => false,
    }
}

pub fn blocked_wait_limit(last_action: &str) -> Duration {
    if last_action == "user.ask" {
        BLOCKED_ASK
    } else if requires_act_gate(last_action) {
        BLOCKED_GATE
    } else {
        BLOCKED_OTHER
    }
}

pub fn evaluate(sample: &HealthSample) -> HealthAction {
    if sample.kind == AgentKind::Roster || sample.state == AgentState::Roster {
        return HealthAction::None;
    }
    if sample.state == AgentState::Failed {
        // Infer/bus stall is transient: restore the worker instead of leaving it dead.
        return if sample.fail_reason.as_deref().is_some_and(is_infer_stall_error)
            && sample.recoveries < MAX_RECOVERIES
        {
            HealthAction::Restart
        } else {
            HealthAction::None
        };
    }
    let live = matches!(
        sample.state,
        AgentState::Created | AgentState::Running | AgentState::Paused | AgentState::Blocked
    );
    if !live {
        return HealthAction::None;
    }

    if sample.pid.is_none() {
        return if sample.state == AgentState::Paused {
            // User pause with a dead worker: do not resurrect as Running.
            HealthAction::MarkFailed
        } else if sample.recoveries >= MAX_RECOVERIES {
            HealthAction::MarkFailed
        } else {
            HealthAction::Restart
        };
    }

    match sample.state {
        AgentState::Paused => HealthAction::None,
        AgentState::Blocked => {
            if sample.idle < blocked_wait_limit(&sample.last_action) {
                HealthAction::None
            } else {
                escalate(sample.recoveries, HealthAction::Unblock)
            }
        }
        AgentState::Created => {
            if sample.idle < CREATED_STALL {
                HealthAction::None
            } else {
                escalate(sample.recoveries, HealthAction::Nudge)
            }
        }
        AgentState::Running => {
            if sample.idle < RUNNING_STALL {
                HealthAction::None
            } else {
                escalate(sample.recoveries, HealthAction::Nudge)
            }
        }
        _ => HealthAction::None,
    }
}

fn escalate(recoveries: u32, first: HealthAction) -> HealthAction {
    if recoveries >= MAX_RECOVERIES {
        HealthAction::MarkFailed
    } else if recoveries >= 1 {
        HealthAction::Restart
    } else {
        first
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(
        state: AgentState,
        pid: Option<u32>,
        last_action: &str,
        idle: Duration,
        recoveries: u32,
    ) -> HealthSample {
        HealthSample {
            state,
            kind: AgentKind::Task,
            pid,
            last_action: last_action.into(),
            fail_reason: None,
            idle,
            recoveries,
        }
    }

    #[test]
    fn roster_and_terminal_are_ignored() {
        let mut s = sample(AgentState::Running, Some(1), "fs.read", RUNNING_STALL, 0);
        s.kind = AgentKind::Roster;
        assert_eq!(evaluate(&s), HealthAction::None);
        let done = sample(AgentState::Done, None, "goal.complete", RUNNING_STALL, 0);
        assert_eq!(evaluate(&done), HealthAction::None);
    }

    #[test]
    fn user_pause_is_never_auto_resumed() {
        let s = sample(
            AgentState::Paused,
            Some(1),
            "fs.read",
            Duration::from_secs(3600),
            0,
        );
        assert_eq!(evaluate(&s), HealthAction::None);
    }

    #[test]
    fn running_stall_nudges_then_restarts_then_fails() {
        let fresh = sample(AgentState::Running, Some(1), "web.search", Duration::from_secs(30), 0);
        assert_eq!(evaluate(&fresh), HealthAction::None);
        let stall = sample(AgentState::Running, Some(1), "web.search", RUNNING_STALL, 0);
        assert_eq!(evaluate(&stall), HealthAction::Nudge);
        let again = sample(AgentState::Running, Some(1), "web.search", RUNNING_STALL, 1);
        assert_eq!(evaluate(&again), HealthAction::Restart);
        let give_up = sample(AgentState::Running, Some(1), "web.search", RUNNING_STALL, 2);
        assert_eq!(evaluate(&give_up), HealthAction::MarkFailed);
    }

    #[test]
    fn user_ask_is_not_unblocked_before_its_window() {
        let s = sample(
            AgentState::Blocked,
            Some(1),
            "user.ask",
            Duration::from_secs(10 * 60),
            0,
        );
        assert_eq!(evaluate(&s), HealthAction::None);
        let late = sample(AgentState::Blocked, Some(1), "user.ask", BLOCKED_ASK, 0);
        assert_eq!(evaluate(&late), HealthAction::Unblock);
    }

    #[test]
    fn act_gate_blocked_uses_gate_window() {
        let s = sample(
            AgentState::Blocked,
            Some(1),
            "notes.create",
            Duration::from_secs(4 * 60),
            0,
        );
        assert_eq!(evaluate(&s), HealthAction::None);
        let late = sample(AgentState::Blocked, Some(1), "notes.create", BLOCKED_GATE, 0);
        assert_eq!(evaluate(&late), HealthAction::Unblock);
    }

    #[test]
    fn other_blocked_unblocks_after_three_minutes() {
        let s = sample(AgentState::Blocked, Some(1), "agent.await", BLOCKED_OTHER, 0);
        assert_eq!(evaluate(&s), HealthAction::Unblock);
    }

    #[test]
    fn dead_worker_restarts_unless_user_paused() {
        let running = sample(AgentState::Running, None, "fs.read", Duration::from_secs(1), 0);
        assert_eq!(evaluate(&running), HealthAction::Restart);
        let paused = sample(AgentState::Paused, None, "fs.read", Duration::from_secs(1), 0);
        assert_eq!(evaluate(&paused), HealthAction::MarkFailed);
    }

    #[test]
    fn infer_stall_failed_is_restarted() {
        let mut s = sample(
            AgentState::Failed,
            None,
            "fs.read",
            Duration::from_secs(1),
            0,
        );
        s.fail_reason = Some(
            "timeout inférence (180 s) — le modèle ou le bus ne répond plus".into(),
        );
        assert_eq!(evaluate(&s), HealthAction::Restart);
        s.recoveries = MAX_RECOVERIES;
        assert_eq!(evaluate(&s), HealthAction::None);
        s.recoveries = 0;
        s.fail_reason = Some("Impossible de continuer.".into());
        assert_eq!(evaluate(&s), HealthAction::None);
    }

    #[test]
    fn token_and_step_count_as_progress() {
        assert!(event_proves_progress(&AgentOutputEvent::Token {
            text: "a".into()
        }));
        assert!(event_proves_progress(&AgentOutputEvent::Progress {
            step: 1,
            max_steps: 8,
            current_task: None,
        }));
        assert!(!event_proves_progress(&AgentOutputEvent::Log {
            line: "health : reprise".into()
        }));
    }
}
