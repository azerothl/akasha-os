//! In-app chat room helpers (slice 3): personas, roster labels, speaker colors, @ mentions.

use crate::i18n::{self, UiStrings};
use aos_agent::room_conductor::build_initial_queue;
use aos_proto::{AgentInfo, AgentKind, AgentState, ChatRoomMember, ChatSessionMode, ChatSessionMeta};

pub use aos_agent::room_personas::{persona_agent_id, persona_by_id, RoomPersona, ROOM_PERSONAS};

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
        .map(|id| roster_member_label(t, members, id))
        .collect();
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
pub fn agents_with_library_placeholders(agents: &[AgentInfo], t: &UiStrings) -> Vec<AgentInfo> {
    let mut out = agents.to_vec();
    for persona in ROOM_PERSONAS {
        let id = persona_agent_id(persona.id);
        if out.iter().any(|a| a.agent_id == id) {
            continue;
        }
        let label = persona_label(t, persona.id);
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

/// Display name from roster (`speaker_id`); never trust a free-text spoof field on the message.
pub fn roster_display_name(members: &[ChatRoomMember], speaker_id: &str) -> String {
    members
        .iter()
        .find(|m| m.agent_id == speaker_id)
        .map(|m| m.display_name.clone())
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
        let name_match = !needle.is_empty()
            && m.display_name.to_ascii_lowercase().starts_with(&needle);
        let id_match = !needle.is_empty() && m.agent_id.to_ascii_lowercase().starts_with(&needle);
        if needle.is_empty() || name_match || id_match {
            let label = member_display_label(t, m);
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
    fn roster_lookup_beats_spoof_name() {
        let members = vec![member("agent-a", "Researcher")];
        assert_eq!(
            roster_display_name(&members, "agent-a"),
            "Researcher"
        );
        assert_eq!(roster_display_name(&members, "agent-b"), "agent-b");
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
            Some("Codeur")
        );
    }

    #[test]
    fn all_personas_defined() {
        for id in ["researcher", "critic", "coder", "planner"] {
            assert!(persona_by_id(id).is_some());
        }
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
}
