//! Conducteur déterministe pour les salons multi-agent (`ChatSessionMode::Room`).

use aos_proto::{ChatRoomConductorPolicy, ChatRoomMember};

/// Plafond dur des tours agent par message utilisateur (indépendamment de la politique).
pub const HARD_MAX_AGENT_TURNS: u32 = 4;

/// `agent_id` présent dans le roster de session.
pub fn is_roster_member(agent_id: &str, members: &[ChatRoomMember]) -> bool {
    members.iter().any(|m| m.agent_id == agent_id)
}

/// Résout un token `@…` vers un `agent_id` membre du roster (display_name, persona_id, agent_id).
/// Les `@agent_id_123` inventés hors roster sont ignorés.
pub fn resolve_mention_token(token: &str, members: &[ChatRoomMember]) -> Option<String> {
    let needle = token.trim_start_matches('@');
    if needle.is_empty() {
        return None;
    }
    members
        .iter()
        .find(|m| {
            m.agent_id == needle
                || m.display_name.eq_ignore_ascii_case(needle)
                || m
                    .persona_id
                    .as_deref()
                    .is_some_and(|p| p.eq_ignore_ascii_case(needle))
        })
        .map(|m| m.agent_id.clone())
}

/// Filtre une file : uniquement des `agent_id` roster, sans doublons.
pub fn sanitize_member_queue(queue: Vec<String>, members: &[ChatRoomMember]) -> Vec<String> {
    let mut out = Vec::new();
    for id in queue {
        if is_roster_member(&id, members) && !out.iter().any(|x| x == &id) {
            out.push(id);
        }
    }
    out
}

/// Liste lisible des membres pour le prompt système du salon.
pub fn format_roster_for_prompt(members: &[ChatRoomMember]) -> String {
    if members.is_empty() {
        return "aucun".into();
    }
    members
        .iter()
        .map(|m| {
            let mut line = format!("{} (@{})", m.display_name, m.agent_id);
            if let Some(p) = m.persona_id.as_deref() {
                line.push_str(&format!(" persona={p}"));
            }
            line
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Indique si le texte contient au moins un token `@mention` (même non roster).
pub fn content_has_mention_tokens(content: &str) -> bool {
    let bytes = content.as_bytes();
    let mut i = 0;
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
                return true;
            }
            i = end;
        } else {
            i += 1;
        }
    }
    false
}

/// Extrait les mentions `@Name` / `@agent_id` roster dans l'ordre d'apparition.
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

/// File initiale : `@` roster uniquement ; `@` invalide → rien ; sans `@` → tous les membres (ordre strip).
pub fn build_initial_queue(content: &str, members: &[ChatRoomMember]) -> Vec<String> {
    if content_has_mention_tokens(content) {
        return sanitize_member_queue(parse_mentions(content, members), members);
    }
    sanitize_member_queue(
        members.iter().map(|m| m.agent_id.clone()).collect(),
        members,
    )
}

/// Détecte une `@mention` roster vers un autre membre (pas de prose / ids inventés).
pub fn detect_peer_address(
    reply: &str,
    members: &[ChatRoomMember],
    exclude_agent_id: &str,
) -> Option<String> {
    if members.len() <= 1 {
        return None;
    }
    for id in parse_mentions(reply, members) {
        if id != exclude_agent_id && is_roster_member(&id, members) {
            return Some(id);
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
        assert_eq!(
            queue,
            vec![
                String::from("agent-alpha"),
                String::from("agent-beta"),
                String::from("agent-gamma"),
            ]
        );
    }

    #[test]
    fn no_mention_queues_all_members_in_strip_order() {
        let m = members();
        let queue = build_initial_queue("Review this sketch", &m);
        assert_eq!(
            queue,
            vec![
                String::from("agent-alpha"),
                String::from("agent-beta"),
                String::from("agent-gamma"),
            ]
        );
    }

    #[test]
    fn no_mention_initial_queue_capped_by_effective_max_turns() {
        let policy = ChatRoomConductorPolicy {
            max_agent_turns_per_user: 1,
            allow_peer_debate: false,
        };
        let max = effective_max_turns(&policy) as usize;
        let m = members();
        let queue = build_initial_queue("Hello everyone", &m);
        let capped: Vec<_> = queue.into_iter().take(max).collect();
        assert_eq!(capped.len(), 1);
        assert_eq!(capped[0], "agent-alpha");
    }

    #[test]
    fn peer_followup_when_reply_mentions_member() {
        let m = members();
        let peer = detect_peer_address("@Beta can you confirm?", &m, "agent-alpha");
        assert_eq!(peer, Some("agent-beta".into()));
    }

    #[test]
    fn invented_agent_id_not_in_speaker_queue() {
        let m = vec![ChatRoomMember {
            agent_id: "persona-critic".into(),
            display_name: "Critic".into(),
            persona_id: Some("critic".into()),
            joined_ms: 1,
        }];
        let ids = parse_mentions("@agent_id_123 @Critic", &m);
        assert_eq!(ids, vec![String::from("persona-critic")]);
        let ghost_only = build_initial_queue("@agent_id_123", &m);
        assert!(ghost_only.is_empty());
        let ghost_with_text = build_initial_queue("@agent_id_123 update the drawing", &m);
        assert!(ghost_with_text.is_empty());
        assert!(
            detect_peer_address(
                "@agent_id_456 (Dessinateur) please render",
                &m,
                "persona-critic"
            )
            .is_none()
        );
    }

    #[test]
    fn canvas_update_without_mention_queues_strip_member() {
        let m = vec![ChatRoomMember {
            agent_id: "persona-critic".into(),
            display_name: "Critic".into(),
            persona_id: Some("critic".into()),
            joined_ms: 1,
        }];
        let queue = build_initial_queue("Mets à jour le dessin", &m);
        assert_eq!(queue, vec![String::from("persona-critic")]);
    }

    #[test]
    fn invented_display_name_mention_queues_nothing() {
        let m = vec![ChatRoomMember {
            agent_id: "persona-critic".into(),
            display_name: "Critic".into(),
            persona_id: Some("critic".into()),
            joined_ms: 1,
        }];
        assert!(build_initial_queue("@Dessinateur", &m).is_empty());
    }

    #[test]
    fn critic_mention_by_display_name_and_persona_id() {
        let m = vec![ChatRoomMember {
            agent_id: "persona-critic".into(),
            display_name: "Critic".into(),
            persona_id: Some("critic".into()),
            joined_ms: 1,
        }];
        assert_eq!(
            resolve_mention_token("Critic", &m),
            Some("persona-critic".into())
        );
        assert_eq!(
            resolve_mention_token("critic", &m),
            Some("persona-critic".into())
        );
        assert_eq!(
            resolve_mention_token("persona-critic", &m),
            Some("persona-critic".into())
        );
    }

    #[test]
    fn single_member_room_no_peer_from_invented_mentions() {
        let m = vec![ChatRoomMember {
            agent_id: "persona-critic".into(),
            display_name: "Critic".into(),
            persona_id: Some("critic".into()),
            joined_ms: 1,
        }];
        let queue = build_initial_queue("please update the house drawing", &m);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0], "persona-critic");
    }

    #[test]
    fn format_roster_for_prompt_lists_members() {
        let m = vec![ChatRoomMember {
            agent_id: "persona-critic".into(),
            display_name: "Critic".into(),
            persona_id: Some("critic".into()),
            joined_ms: 1,
        }];
        let roster = format_roster_for_prompt(&m);
        assert!(roster.contains("Critic"));
        assert!(roster.contains("persona-critic"));
        assert!(roster.contains("persona=critic"));
    }

    #[test]
    fn heuristic_picks_display_name_in_text() {
        let m = members();
        let speaker = pick_first_speaker("What does Beta think?", &m).unwrap();
        assert_eq!(speaker, "agent-beta");
    }
}
