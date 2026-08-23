//! Conducteur déterministe pour les salons multi-agent (`ChatSessionMode::Room`).

use aos_proto::{ChatRoomConductorPolicy, ChatRoomMember};

/// Plafond dur des tours agent par message utilisateur (indépendamment de la politique).
pub const HARD_MAX_AGENT_TURNS: u32 = 4;

/// Résout un token `@…` vers un `agent_id` membre.
pub fn resolve_mention_token(token: &str, members: &[ChatRoomMember]) -> Option<String> {
    let needle = token.trim_start_matches('@');
    if needle.is_empty() {
        return None;
    }
    members
        .iter()
        .find(|m| {
            m.agent_id == needle || m.display_name.eq_ignore_ascii_case(needle)
        })
        .map(|m| m.agent_id.clone())
}

/// Extrait les mentions `@Name` / `@agent_id` dans l'ordre d'apparition.
pub fn parse_mentions(content: &str, members: &[ChatRoomMember]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    let bytes = content.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'@' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() {
                let c = bytes[end];
                if c.is_ascii_alphanumeric() || c == b'-' || c == b'_' {
                    end += 1;
                } else {
                    break;
                }
            }
            if end > start {
                let token = &content[start..end];
                if let Some(id) = resolve_mention_token(token, members) {
                    if !out.iter().any(|x| x == &id) {
                        out.push(id);
                    }
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

/// Heuristique : premier membre ou `display_name` présent dans le texte.
pub fn pick_first_speaker(content: &str, members: &[ChatRoomMember]) -> Option<String> {
    if members.is_empty() {
        return None;
    }
    let lower = content.to_ascii_lowercase();
    for m in members {
        let name = m.display_name.trim();
        if !name.is_empty() && lower.contains(&name.to_ascii_lowercase()) {
            return Some(m.agent_id.clone());
        }
    }
    Some(members[0].agent_id.clone())
}

/// File initiale : mentions ordonnées, sinon un seul locuteur heuristique.
pub fn build_initial_queue(content: &str, members: &[ChatRoomMember]) -> Vec<String> {
    let mentions = parse_mentions(content, members);
    if !mentions.is_empty() {
        return mentions;
    }
    pick_first_speaker(content, members)
        .into_iter()
        .collect()
}

/// Détecte une adresse explicite (`@` ou nom affiché) vers un autre membre.
pub fn detect_peer_address(
    reply: &str,
    members: &[ChatRoomMember],
    exclude_agent_id: &str,
) -> Option<String> {
    let others: Vec<&ChatRoomMember> = members
        .iter()
        .filter(|m| m.agent_id != exclude_agent_id)
        .collect();
    if others.is_empty() {
        return None;
    }
    let mentions = parse_mentions(reply, members);
    for id in mentions {
        if id != exclude_agent_id {
            return Some(id);
        }
    }
    let lower = reply.to_ascii_lowercase();
    for m in others {
        let name = m.display_name.trim();
        if !name.is_empty() && lower.contains(&name.to_ascii_lowercase()) {
            return Some(m.agent_id.clone());
        }
    }
    None
}

/// Nombre effectif de tours agent autorisés (politique + plafond dur).
pub fn effective_max_turns(policy: &ChatRoomConductorPolicy) -> u32 {
    policy
        .max_agent_turns_per_user
        .min(HARD_MAX_AGENT_TURNS)
        .max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::ChatRoomConductorPolicy;

    fn members() -> Vec<ChatRoomMember> {
        vec![
            ChatRoomMember {
                agent_id: "agent-alpha".into(),
                display_name: "Alpha".into(),
                persona_id: None,
                joined_ms: 1,
            },
            ChatRoomMember {
                agent_id: "agent-beta".into(),
                display_name: "Beta".into(),
                persona_id: None,
                joined_ms: 2,
            },
            ChatRoomMember {
                agent_id: "agent-gamma".into(),
                display_name: "Gamma".into(),
                persona_id: None,
                joined_ms: 3,
            },
        ]
    }

    #[test]
    fn parse_mentions_by_display_name_and_agent_id() {
        let m = members();
        let ids = parse_mentions("Hey @Alpha and @agent-gamma please weigh in", &m);
        assert_eq!(
            ids,
            vec![
                String::from("agent-alpha"),
                String::from("agent-gamma"),
            ]
        );
    }

    #[test]
    fn parse_mentions_preserves_order_without_duplicates() {
        let m = members();
        let ids = parse_mentions("@Beta then @Beta again @Alpha", &m);
        assert_eq!(
            ids,
            vec![String::from("agent-beta"), String::from("agent-alpha")]
        );
    }

    #[test]
    fn cap_of_four_agent_turns() {
        let policy = ChatRoomConductorPolicy {
            max_agent_turns_per_user: 99,
            allow_peer_debate: true,
        };
        assert_eq!(effective_max_turns(&policy), 4);
        let policy_default = ChatRoomConductorPolicy::default();
        assert_eq!(effective_max_turns(&policy_default), 4);

        let m = members();
        let content = "@Alpha @Beta @Gamma @Alpha @Beta extra";
        let queue = build_initial_queue(content, &m);
        let max = effective_max_turns(&policy) as usize;
        let capped: Vec<_> = queue.into_iter().take(max).collect();
        assert_eq!(capped.len(), 3);
        assert!(capped.len() <= max);
    }

    #[test]
    fn stop_without_peer_when_no_address_in_reply() {
        let m = members();
        assert!(detect_peer_address(
            "I agree with the plan, no need to tag anyone.",
            &m,
            "agent-alpha"
        )
        .is_none());
        let queue = build_initial_queue("Hello everyone", &m);
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn peer_followup_when_reply_mentions_member() {
        let m = members();
        let peer = detect_peer_address("@Beta can you confirm?", &m, "agent-alpha");
        assert_eq!(peer, Some("agent-beta".into()));
    }

    #[test]
    fn heuristic_picks_display_name_in_text() {
        let m = members();
        let speaker = pick_first_speaker("What does Beta think?", &m).unwrap();
        assert_eq!(speaker, "agent-beta");
    }
}
