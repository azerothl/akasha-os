//! Live status line for the pending assistant bubble (no tokens yet).

use std::collections::HashMap;

use aos_proto::{AgentInfo, AgentRoomConductProgress, AgentState, AgentTrace};

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
///
/// In room (salon) mode there is no `model.infer` phase stream: the UI waits on a
/// single `chat.session.room.turn` that runs the conductor + each member’s reply.
/// Prefer live `AgentRoomConductProgress` (speaker + phase); otherwise a generic
/// room label. Chat phase labels apply only outside room mode.
pub(crate) fn format_pending_assistant_status(
    t: &UiStrings,
    phase: ChatInferPhase,
    session_id: Option<&str>,
    agents: &[AgentInfo],
    traces: &HashMap<String, AgentTrace>,
    room_mode: bool,
    room_progress: Option<&AgentRoomConductProgress>,
) -> String {
    if room_mode {
        if let Some(status) = format_room_progress(t, room_progress) {
            return status;
        }
        if let Some(status) = format_active_agent_status(t, session_id, agents, traces) {
            return status;
        }
        return t.chat_pending_room_turn.to_string();
    }
    if let Some(status) = format_active_agent_status(t, session_id, agents, traces) {
        return status;
    }
    format_phase_status(t, phase)
}

fn format_room_progress(
    t: &UiStrings,
    progress: Option<&AgentRoomConductProgress>,
) -> Option<String> {
    let progress = progress.filter(|p| p.active)?;
    let name = progress
        .speaker_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            progress
                .speaker_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
        })?;
    let phase_label = room_activity_label(t, progress);
    if progress.turn_index > 0 && progress.turn_total > 0 {
        Some(
            t.chat_pending_room_speaker_step
                .replace("{name}", name)
                .replace("{i}", &progress.turn_index.to_string())
                .replace("{n}", &progress.turn_total.to_string())
                .replace("{phase}", &phase_label),
        )
    } else {
        Some(
            t.chat_pending_room_speaker
                .replace("{name}", name)
                .replace("{phase}", &phase_label),
        )
    }
}

fn room_activity_label(t: &UiStrings, progress: &AgentRoomConductProgress) -> String {
    if let Some(detail) = progress
        .detail
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if let Some(label) = i18n::tool_human_label(t, detail) {
            return format!("{label}…");
        }
    }
    room_phase_label(t, progress.phase.as_str()).to_string()
}

fn room_phase_label<'a>(t: &'a UiStrings, phase: &str) -> &'a str {
    match phase {
        "thinking" => t.chat_pending_room_phase_thinking,
        "generating" => t.chat_pending_room_phase_thinking, // legacy: room infer is reflection
        "reading" => t.chat_pending_room_phase_reading,
        "searching" => t.chat_pending_room_phase_searching,
        "tools" => t.chat_pending_room_phase_tools,
        "waiting_user" => t.chat_pending_room_phase_waiting_user,
        "preparing" => t.chat_pending_room_phase_preparing,
        _ => t.chat_pending_room_phase_working,
    }
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
            false,
            None,
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
            false,
            None,
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
            false,
            None,
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
            false,
            None,
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
            false,
            None,
        );
        assert_eq!(status, t.chat_pending_preparing);
    }

    #[test]
    fn room_mode_uses_room_label_not_preparing() {
        let t = en();
        let status = format_pending_assistant_status(
            &t,
            ChatInferPhase::Preparing,
            Some("s1"),
            &[],
            &HashMap::new(),
            true,
            None,
        );
        assert_eq!(status, t.chat_pending_room_turn);

        let t = fr();
        let status = format_pending_assistant_status(
            &t,
            ChatInferPhase::Preparing,
            Some("s1"),
            &[],
            &HashMap::new(),
            true,
            None,
        );
        assert_eq!(status, t.chat_pending_room_turn);
        assert!(!status.contains("contexte"));
    }

    #[test]
    fn room_progress_shows_speaker_and_phase() {
        let t = fr();
        let progress = AgentRoomConductProgress {
            session_id: "s1".into(),
            active: true,
            speaker_id: Some("planner".into()),
            speaker_name: Some("Planificateur".into()),
            turn_index: 1,
            turn_total: 4,
            phase: "thinking".into(),
            detail: None,
        };
        let status = format_pending_assistant_status(
            &t,
            ChatInferPhase::Preparing,
            Some("s1"),
            &[],
            &HashMap::new(),
            true,
            Some(&progress),
        );
        assert!(status.contains("Planificateur"));
        assert!(status.contains("1/4"));
        assert!(status.contains("réflexion"));
    }

    #[test]
    fn room_progress_prefers_tool_detail_over_phase() {
        let t = fr();
        let progress = AgentRoomConductProgress {
            session_id: "s1".into(),
            active: true,
            speaker_id: Some("planner".into()),
            speaker_name: Some("Planificateur".into()),
            turn_index: 1,
            turn_total: 4,
            phase: "reading".into(),
            detail: Some("fs.read".into()),
        };
        let status = format_pending_assistant_status(
            &t,
            ChatInferPhase::Preparing,
            Some("s1"),
            &[],
            &HashMap::new(),
            true,
            Some(&progress),
        );
        assert!(status.contains("Lire un fichier"));
        assert!(!status.contains("génération"));
    }
}
