//! Live status line for the pending assistant bubble (no tokens yet).

use std::collections::HashMap;

use aos_proto::{AgentInfo, AgentState, AgentTrace};

use crate::i18n::{self, UiStrings};

/// Inference phase while a chat turn is pending (before first token).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ChatInferPhase {
    /// Building prompts / memory / vision before `model.infer` yields tokens.
    #[default]
    Preparing,
    /// Waiting in the model scheduler queue.
    Queued { position: usize },
    /// Inference accepted; waiting for the first delta.
    WaitingFirstToken,
}

/// Format the weak status line under the pending assistant header.
pub(crate) fn format_pending_assistant_status(
    t: &UiStrings,
    phase: ChatInferPhase,
    session_id: Option<&str>,
    agents: &[AgentInfo],
    traces: &HashMap<String, AgentTrace>,
) -> String {
    if let Some(status) = format_active_agent_status(t, session_id, agents, traces) {
        return status;
    }
    format_phase_status(t, phase)
}

fn format_phase_status(t: &UiStrings, phase: ChatInferPhase) -> String {
    match phase {
        ChatInferPhase::Preparing => t.chat_pending_preparing.to_string(),
        ChatInferPhase::Queued { position } => t
            .chat_pending_queued
            .replace("{n}", &position.to_string()),
        ChatInferPhase::WaitingFirstToken => t.chat_pending_waiting_tokens.to_string(),
    }
}

fn format_active_agent_status(
    t: &UiStrings,
    session_id: Option<&str>,
    agents: &[AgentInfo],
    traces: &HashMap<String, AgentTrace>,
) -> Option<String> {
    let sid = session_id?.trim();
    if sid.is_empty() {
        return None;
    }
    let agent = agents
        .iter()
        .filter(|a| {
            a.session_id.as_deref() == Some(sid)
                && matches!(a.state, AgentState::Running | AgentState::Blocked)
                && !a.is_roster()
        })
        .max_by_key(|a| a.step)?;

    if let Some(task) = agent
        .current_task
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(truncate_status(task, 96));
    }

    if agent.max_steps > 0 && agent.step > 0 {
        return Some(
            t.chat_pending_agent_step
                .replace("{step}", &agent.step.to_string())
                .replace("{max}", &agent.max_steps.to_string()),
        );
    }

    if let Some(trace) = traces.get(&agent.agent_id) {
        if let Some(action) = last_tool_action(trace) {
            if let Some(label) = i18n::tool_human_label(t, action) {
                return Some(label.to_string());
            }
            let trimmed = action.trim();
            if !trimmed.is_empty() {
                return Some(truncate_status(trimmed, 48));
            }
        }
    }

    Some(t.chat_pending_agent_working.to_string())
}

fn last_tool_action(trace: &AgentTrace) -> Option<&str> {
    trace
        .steps
        .iter()
        .rev()
        .map(|s| s.action.as_str())
        .find(|a| !a.trim().is_empty())
}

fn truncate_status(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    format!("{}…", s.chars().take(max).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::{AgentKind, AgentStepRecord};

    fn en() -> UiStrings {
        i18n::strings("en")
    }

    fn fr() -> UiStrings {
        i18n::strings("fr")
    }

    fn agent(
        id: &str,
        session: &str,
        state: AgentState,
        step: u32,
        max_steps: u32,
        task: Option<&str>,
    ) -> AgentInfo {
        AgentInfo {
            agent_id: id.into(),
            state,
            directive: "do things".into(),
            pid: None,
            caps: vec![],
            last_output: String::new(),
            step,
            max_steps,
            current_task: task.map(str::to_string),
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: vec![],
            tools: vec![],
            mcp_servers: vec![],
            fail_reason: None,
            session_id: Some(session.into()),
            model_id: None,
            title: "helper".into(),
            kind: AgentKind::Task,
            display_name: None,
            persona_id: None,
            origin: Some("assistant".into()),
            avatar: None,
            color: None,
            deep_plan: None,
            cognitive_mode: Default::default(),
        }
    }

    #[test]
    fn phase_labels_en_and_fr() {
        let t = en();
        assert_eq!(
            format_phase_status(&t, ChatInferPhase::Preparing),
            t.chat_pending_preparing
        );
        assert!(format_phase_status(&t, ChatInferPhase::Queued { position: 3 }).contains('3'));
        assert_eq!(
            format_phase_status(&t, ChatInferPhase::WaitingFirstToken),
            t.chat_pending_waiting_tokens
        );

        let t = fr();
        assert!(!format_phase_status(&t, ChatInferPhase::Preparing).is_empty());
        assert!(format_phase_status(&t, ChatInferPhase::Queued { position: 2 }).contains('2'));
    }

    #[test]
    fn prefers_agent_current_task_over_phase() {
        let t = en();
        let agents = vec![agent(
            "a1",
            "s1",
            AgentState::Running,
            2,
            10,
            Some("Search the repo"),
        )];
        let status = format_pending_assistant_status(
            &t,
            ChatInferPhase::Queued { position: 9 },
            Some("s1"),
            &agents,
            &HashMap::new(),
        );
        assert_eq!(status, "Search the repo");
    }

    #[test]
    fn agent_step_when_no_task() {
        let t = en();
        let agents = vec![agent("a1", "s1", AgentState::Running, 3, 12, None)];
        let status = format_pending_assistant_status(
            &t,
            ChatInferPhase::Preparing,
            Some("s1"),
            &agents,
            &HashMap::new(),
        );
        assert_eq!(
            status,
            t.chat_pending_agent_step
                .replace("{step}", "3")
                .replace("{max}", "12")
        );
    }

    #[test]
    fn agent_last_tool_label_when_no_step_progress() {
        let t = en();
        let agents = vec![agent("a1", "s1", AgentState::Blocked, 0, 0, None)];
        let mut traces = HashMap::new();
        traces.insert(
            "a1".into(),
            AgentTrace {
                agent_id: "a1".into(),
                steps: vec![AgentStepRecord {
                    step: 1,
                    action: "web.search".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
        );
        let status = format_pending_assistant_status(
            &t,
            ChatInferPhase::Preparing,
            Some("s1"),
            &agents,
            &traces,
        );
        assert_eq!(
            status,
            i18n::tool_human_label(&t, "web.search").unwrap_or("web.search")
        );
    }

    #[test]
    fn falls_back_to_phase_without_session_agents() {
        let t = en();
        let agents = vec![agent("a1", "other", AgentState::Running, 1, 5, Some("Nope"))];
        let status = format_pending_assistant_status(
            &t,
            ChatInferPhase::WaitingFirstToken,
            Some("s1"),
            &agents,
            &HashMap::new(),
        );
        assert_eq!(status, t.chat_pending_waiting_tokens);
    }

    #[test]
    fn ignores_roster_and_done_agents() {
        let t = en();
        let mut roster = agent("r1", "s1", AgentState::Roster, 1, 1, Some("Roster task"));
        roster.kind = AgentKind::Roster;
        let agents = vec![
            roster,
            agent("d1", "s1", AgentState::Done, 5, 5, Some("Done task")),
        ];
        let status = format_pending_assistant_status(
            &t,
            ChatInferPhase::Preparing,
            Some("s1"),
            &agents,
            &HashMap::new(),
        );
        assert_eq!(status, t.chat_pending_preparing);
    }
}
