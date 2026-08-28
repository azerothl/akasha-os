//! In-app chat room helpers (slice 3): personas, roster labels, speaker colors, @ mentions.

use crate::i18n::{self, UiStrings};
use aos_agent::room_conductor::build_initial_queue;
use aos_proto::{AgentInfo, AgentKind, AgentState, ChatRoomMember, ChatSessionMode, ChatSessionMeta};

pub use aos_agent::room_personas::{persona_agent_id, persona_by_id, ROOM_PERSONAS};

/// Localized persona chip / roster label (EN + FR via i18n).
pub fn persona_label(t: &UiStrings, persona_id: &str) -> &'static str {
    i18n::persona_label(t, persona_id)
}

/// Roster name for UI: localized persona label when `persona_id` is set.
pub fn member_display_label(t: &UiStrings, member: &ChatRoomMember) -> String {
    if let Some(pid) = member.persona_id.as_deref() {
        persona_label(t, pid).to_string()
    } else {
        member.display_name.clone()
    }
}

/// Speaker queue for an in-flight room turn (`Researcher puis Critic` / `Researcher then Critic`).
pub fn format_turn_speaker_queue(
    t: &UiStrings,
    user_message: &str,
    members: &[ChatRoomMember],
) -> Option<String> {
    if user_message.trim().is_empty() || members.is_empty() {
        return None;
    }
    let queue = build_initial_queue(user_message, members);
    if queue.is_empty() {
        return None;
    }
    let names: Vec<String> = queue
        .iter()
        .filter_map(|id| {
            members
                .iter()
                .find(|m| m.agent_id == *id)
                .map(|m| member_display_label(t, m))
        })
        .collect();
    if names.is_empty() {
        return None;
    }
    Some(names.join(t.room_queue_joiner))
}

fn roster_member_label(t: &UiStrings, members: &[ChatRoomMember], agent_id: &str) -> String {
    members
        .iter()
        .find(|m| m.agent_id == agent_id)
        .map(|m| member_display_label(t, m))
        .unwrap_or_else(|| agent_id.to_string())
}

pub fn active_session_meta<'a>(
    sessions: &'a [ChatSessionMeta],
    active_id: Option<&str>,
) -> Option<&'a ChatSessionMeta> {
    let id = active_id?;
    sessions.iter().find(|s| s.id == id)
}

pub fn session_is_room(meta: Option<&ChatSessionMeta>) -> bool {
    meta.is_some_and(|m| m.mode == ChatSessionMode::Room)
}

/// Merge built-in persona placeholders so the Agents library always lists all four.
pub fn agents_with_library_placeholders(agents: &[AgentInfo], _t: &UiStrings) -> Vec<AgentInfo> {
    let mut out = agents.to_vec();
    for persona in ROOM_PERSONAS {
        let id = persona_agent_id(persona.id);
        if out.iter().any(|a| a.agent_id == id) {
            continue;
        }
        let label = persona.display_name;
        out.push(AgentInfo {
            agent_id: id,
            state: AgentState::Roster,
            directive: String::new(),
            pid: None,
            caps: vec![],
            last_output: String::new(),
            step: 0,
            max_steps: 0,
            current_task: None,
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: vec![],
            tools: vec![],
            mcp_servers: vec![],
            fail_reason: None,
            session_id: None,
            title: label.to_string(),
            kind: AgentKind::Roster,
            display_name: Some(label.to_string()),
            persona_id: Some(persona.id.to_string()),
        });
    }
    out
}

/// Label for a library roster agent (localized persona or display name).
pub fn roster_agent_label(t: &UiStrings, agent: &AgentInfo) -> String {
    if let Some(pid) = agent.persona_id.as_deref() {
        persona_label(t, pid).to_string()
    } else {
        agent.display_title().to_string()
    }
}

/// User-created library entries and built-in personas; excludes active task workers.
pub fn is_salon_library_candidate(agent: &AgentInfo) -> bool {
    if agent.persona_id.is_some() {
        return agent.is_roster();
    }
    if agent.is_roster() {
        return true;
    }
    let Some(name) = agent.display_name.as_deref().filter(|n| !n.trim().is_empty()) else {
        return false;
    };
    let _ = name;
    agent.max_steps == 0
        && !matches!(
            agent.state,
            AgentState::Running | AgentState::Created | AgentState::Paused | AgentState::Blocked
        )
}

/// Roster library entries not yet in this session.
pub fn library_add_candidates(
    agents: &[AgentInfo],
    members: &[ChatRoomMember],
    t: &UiStrings,
) -> Vec<AgentInfo> {
    let present = member_ids(members);
    agents_with_library_placeholders(agents, t)
        .into_iter()
        .filter(|a| is_salon_library_candidate(a))
        .filter(|a| !present.contains(a.agent_id.as_str()))
        .collect()
}

pub fn member_ids(members: &[ChatRoomMember]) -> std::collections::HashSet<&str> {
    members.iter().map(|m| m.agent_id.as_str()).collect()
}

/// Display name from roster (`speaker_id`); never trust a free-text spoof field on the message.
pub fn roster_display_name(t: &UiStrings, members: &[ChatRoomMember], speaker_id: &str) -> String {
    members
        .iter()
        .find(|m| m.agent_id == speaker_id)
        .map(|m| member_display_label(t, m))
        .unwrap_or_else(|| speaker_id.to_string())
}

/// Stable per-speaker RGB derived from `speaker_id` (orrery hues, no purple glow).
pub fn speaker_color_rgb(speaker_id: &str, dark: bool) -> (u8, u8, u8) {
    let h = stable_hash(speaker_id);
    if dark {
        (
            40 + (h % 80) as u8,
            90 + ((h >> 8) % 70) as u8,
            100 + ((h >> 16) % 60) as u8,
        )
    } else {
        (
            180 + (h % 60) as u8,
            200 + ((h >> 8) % 40) as u8,
            210 + ((h >> 16) % 30) as u8,
        )
    }
}

fn stable_hash(s: &str) -> u32 {
    let mut h: u32 = 2_166_136_261;
    for b in s.bytes() {
        h = h.wrapping_mul(16_777_619).wrapping_add(u32::from(b));
    }
    h
}

pub fn joined_ms_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// `@` mention completions against the salon roster (display name or agent id prefix).
pub fn mention_completions(
    input: &str,
    members: &[ChatRoomMember],
    t: &UiStrings,
) -> Vec<(String, String)> {
    let Some(at) = input.rfind('@') else {
        return Vec::new();
    };
    let tail = &input[at + 1..];
    if tail.contains(' ') {
        return Vec::new();
    }
    let needle = tail.to_ascii_lowercase();
    let mut out = Vec::new();
    for m in members {
        let label = member_display_label(t, m);
        let name_match =
            !needle.is_empty() && label.to_ascii_lowercase().starts_with(&needle);
        let stored_name_match = !needle.is_empty()
            && m.display_name.to_ascii_lowercase().starts_with(&needle);
        let id_match = !needle.is_empty() && m.agent_id.to_ascii_lowercase().starts_with(&needle);
        let persona_match = m
            .persona_id
            .as_deref()
            .is_some_and(|p| !needle.is_empty() && p.to_ascii_lowercase().starts_with(&needle));
        if needle.is_empty() || name_match || stored_name_match || id_match || persona_match {
            out.push((insert_mention(input, at, &label), label));
        }
    }
    out
}

fn insert_mention(input: &str, at: usize, display_name: &str) -> String {
    let mut out = String::new();
    out.push_str(&input[..at]);
    out.push('@');
    out.push_str(display_name);
    out.push(' ');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::ChatSessionMode;

    fn member(id: &str, name: &str) -> ChatRoomMember {
        ChatRoomMember {
            agent_id: id.into(),
            display_name: name.into(),
            persona_id: None,
            joined_ms: 0,
        }
    }

    #[test]
    fn library_add_candidates_skips_current_members() {
        let t = i18n::strings("en");
        let mut m1 = member("persona-coder", "Coder");
        m1.persona_id = Some("coder".into());
        let members = vec![m1];
        let candidates = library_add_candidates(&[], &members, &t);
        assert!(!candidates.iter().any(|a| a.persona_id.as_deref() == Some("coder")));
        assert!(candidates.iter().any(|a| a.persona_id.as_deref() == Some("researcher")));
    }

    #[test]
    fn roster_lookup_beats_spoof_name() {
        let t = i18n::strings("en");
        let members = vec![member("agent-a", "Researcher")];
        assert_eq!(
            roster_display_name(&t, &members, "agent-a"),
            "Researcher"
        );
        assert_eq!(roster_display_name(&t, &members, "agent-b"), "agent-b");
    }

    #[test]
    fn speaker_color_stable() {
        let a = speaker_color_rgb("agent-alpha", true);
        let b = speaker_color_rgb("agent-alpha", true);
        let c = speaker_color_rgb("agent-beta", true);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn mention_completions_prefix() {
        let members = vec![
            member("a1", "Researcher"),
            member("a2", "Coder"),
        ];
        let hits = mention_completions("hello @Res", &members, &i18n::strings("en"));
        assert_eq!(hits.len(), 1);
        assert!(hits[0].0.contains("@Researcher"));
    }

    #[test]
    fn session_is_room_flag() {
        let meta = ChatSessionMeta {
            id: "s".into(),
            title: "t".into(),
            created_ms: 0,
            updated_ms: 0,
            archived: false,
            message_count: 0,
            model_id: None,
            mode: ChatSessionMode::Room,
            members: vec![],
            conductor_policy: Default::default(),
            canvas_open: false,
            canvas_aspect: aos_proto::CanvasAspect::Square,
        };
        assert!(session_is_room(Some(&meta)));
    }

    #[test]
    fn library_placeholders_include_four_personas() {
        let t = i18n::strings("fr");
        let list = agents_with_library_placeholders(&[], &t);
        assert_eq!(list.len(), 4);
        assert!(list.iter().any(|a| a.persona_id.as_deref() == Some("coder")));
        assert_eq!(
            list.iter()
                .find(|a| a.persona_id.as_deref() == Some("coder"))
                .and_then(|a| a.display_name.as_deref()),
            Some("Coder")
        );
        assert_eq!(
            roster_agent_label(
                &t,
                list.iter()
                    .find(|a| a.persona_id.as_deref() == Some("coder"))
                    .unwrap(),
            ),
            "Codeur"
        );
    }

    #[test]
    fn all_personas_defined() {
        for id in ["researcher", "critic", "coder", "planner"] {
            assert!(persona_by_id(id).is_some());
        }
    }

    #[test]
    fn ghost_mention_no_speaker_queue_label() {
        let t = i18n::strings("fr");
        let mut m1 = member("persona-critic", "Critic");
        m1.persona_id = Some("critic".into());
        let members = vec![m1];
        assert!(format_turn_speaker_queue(&t, "@Dessinateur", &members).is_none());
        assert!(format_turn_speaker_queue(&t, "@agent_id_123", &members).is_none());
    }

    #[test]
    fn canvas_update_queues_strip_member_without_at() {
        let t = i18n::strings("fr");
        let mut m1 = member("persona-critic", "Critic");
        m1.persona_id = Some("critic".into());
        let members = vec![m1];
        let q = format_turn_speaker_queue(&t, "Mets à jour le dessin", &members).expect("queue");
        assert!(q.contains(t.persona_critic));
    }

    #[test]
    fn turn_queue_joins_speakers() {
        let t = i18n::strings("fr");
        let mut m1 = member("a1", "Researcher");
        m1.persona_id = Some("researcher".into());
        let mut m2 = member("a2", "Coder");
        m2.persona_id = Some("coder".into());
        let members = vec![m1, m2];
        let q = format_turn_speaker_queue(&t, "@Researcher @Coder", &members).expect("queue");
        assert!(q.contains(t.room_queue_joiner));
        assert!(q.contains(t.persona_researcher));
        assert!(q.contains(t.persona_coder));
    }

    fn custom_library_agent(id: &str, label: &str) -> AgentInfo {
        AgentInfo {
            agent_id: id.into(),
            state: AgentState::Roster,
            directive: String::new(),
            pid: None,
            caps: vec![],
            last_output: String::new(),
            step: 0,
            max_steps: 0,
            current_task: None,
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: vec![],
            tools: vec![],
            mcp_servers: vec![],
            fail_reason: None,
            session_id: None,
            title: label.into(),
            kind: AgentKind::Roster,
            display_name: Some(label.into()),
            persona_id: None,
        }
    }

    fn task_worker_agent(id: &str, label: &str) -> AgentInfo {
        AgentInfo {
            agent_id: id.into(),
            state: AgentState::Running,
            directive: "Analyze the skills registry".into(),
            pid: Some(42),
            caps: vec![],
            last_output: String::new(),
            step: 1,
            max_steps: 32,
            current_task: None,
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: vec![],
            tools: vec![],
            mcp_servers: vec![],
            fail_reason: None,
            session_id: Some("session-1".into()),
            title: label.into(),
            kind: AgentKind::Task,
            display_name: Some(label.into()),
            persona_id: None,
        }
    }

    #[test]
    fn typed_display_name_round_trips_to_labels() {
        let t = i18n::strings("en");
        let agent = custom_library_agent("agent-7", "Skills Auditor");
        assert_eq!(agent.display_title(), "Skills Auditor");
        assert_eq!(roster_agent_label(&t, &agent), "Skills Auditor");
        let member = ChatRoomMember {
            agent_id: agent.agent_id.clone(),
            display_name: "Skills Auditor".into(),
            persona_id: None,
            joined_ms: 0,
        };
        assert_eq!(member_display_label(&t, &member), "Skills Auditor");
    }

    #[test]
    fn page_created_roster_agent_in_library_add_candidates() {
        let t = i18n::strings("en");
        let agents = vec![custom_library_agent("agent-7", "Skills Auditor")];
        let candidates = library_add_candidates(&agents, &[], &t);
        assert!(candidates.iter().any(|a| a.agent_id == "agent-7"));
        assert_eq!(
            roster_agent_label(&t, candidates.iter().find(|a| a.agent_id == "agent-7").unwrap()),
            "Skills Auditor"
        );
    }

    #[test]
    fn task_worker_excluded_from_library_add_candidates() {
        let t = i18n::strings("en");
        let agents = vec![task_worker_agent("agent-8", "Analyze skills")];
        let candidates = library_add_candidates(&agents, &[], &t);
        assert!(!candidates.iter().any(|a| a.agent_id == "agent-8"));
        assert!(!is_salon_library_candidate(&agents[0]));
    }

    #[test]
    fn library_candidate_with_idle_task_kind_and_label() {
        let mut agent = custom_library_agent("agent-9", "Reviewer");
        agent.kind = AgentKind::Task;
        agent.state = AgentState::Done;
        assert!(is_salon_library_candidate(&agent));
        let t = i18n::strings("en");
        let candidates = library_add_candidates(&[agent], &[], &t);
        assert!(candidates.iter().any(|a| a.agent_id == "agent-9"));
    }
}
