//! Conducteur déterministe pour les salons multi-agent (`ChatSessionMode::Room`).

use crate::room_personas::persona_mention_labels;
use aos_proto::{ChatRoomConductorPolicy, ChatRoomMember};

/// Plafond dur des tours agent par message utilisateur (indépendamment de la politique).
pub const HARD_MAX_AGENT_TURNS: u32 = 4;

/// Plafond des tours relancés par un `@` pair après le passage initial du roster.
pub const HARD_MAX_PEER_FOLLOWUPS: u32 = 2;

/// Tour planifié : passage initial du strip ou relance `@` pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledTurn {
    pub agent_id: String,
    pub peer_followup: bool,
}

/// Convertit la file initiale (`build_initial_queue`) en tours planifiés.
pub fn initial_schedule(queue: Vec<String>) -> Vec<ScheduledTurn> {
    queue
        .into_iter()
        .map(|agent_id| ScheduledTurn {
            agent_id,
            peer_followup: false,
        })
        .collect()
}

/// Prochain tour exécutable : relances `@` pair ou premier passage initial non encore fait.
pub fn pop_next_scheduled_turn(
    queue: &mut Vec<ScheduledTurn>,
    initial_done: &std::collections::HashSet<String>,
) -> Option<ScheduledTurn> {
    let mut idx = 0;
    while idx < queue.len() {
        let turn = &queue[idx];
        if turn.peer_followup || !initial_done.contains(&turn.agent_id) {
            return Some(queue.remove(idx));
        }
        idx += 1;
    }
    None
}

fn scheduled_peer_followups(queue: &[ScheduledTurn]) -> usize {
    queue.iter().filter(|t| t.peer_followup).count()
}

/// Après une réplique, priorise ou relance les pairs `@` mentionnés.
///
/// - Membre pas encore passé dans le strip initial → remonte son tour initial (pas de doublon).
/// - Membre déjà passé → relance `@` pair (budget `peer_followup_budget`).
pub fn apply_peer_followups(
    queue: &mut Vec<ScheduledTurn>,
    peers: &[String],
    initial_done: &std::collections::HashSet<String>,
    peer_followups_run: u32,
    peer_followup_budget: u32,
) {
    let mut reserved = peer_followups_run + scheduled_peer_followups(queue) as u32;
    for peer_id in peers {
        if initial_done.contains(peer_id) {
            if reserved >= peer_followup_budget {
                continue;
            }
            queue.retain(|t| !(t.agent_id == *peer_id && t.peer_followup));
            queue.insert(
                0,
                ScheduledTurn {
                    agent_id: peer_id.clone(),
                    peer_followup: true,
                },
            );
            reserved += 1;
        } else if let Some(pos) = queue
            .iter()
            .position(|t| t.agent_id == *peer_id && !t.peer_followup)
        {
            let entry = queue.remove(pos);
            queue.insert(0, entry);
        }
    }
}

/// Budget effectif de relances `@` pair pour une ronde utilisateur.
pub fn effective_peer_followup_budget(max_agent_turns: u32) -> u32 {
    HARD_MAX_PEER_FOLLOWUPS.min(max_agent_turns.saturating_sub(1))
}

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
            let mut line = m.display_name.clone();
            if let Some(p) = m.persona_id.as_deref() {
                line.push_str(&format!(" (persona={p})"));
            }
            line
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Indique si le texte contient au moins un token `@mention` (même non roster).
pub fn content_has_mention_tokens(content: &str) -> bool {
    content.contains('@')
}

/// Mentions roster triées par longueur de label décroissante (noms multi-mots en premier).
fn mention_labels_longest_first(members: &[ChatRoomMember]) -> Vec<(String, String)> {
    let mut labels: Vec<(String, String)> = Vec::new();
    for m in members {
        if !m.display_name.trim().is_empty() {
            labels.push((m.display_name.clone(), m.agent_id.clone()));
        }
        if let Some(p) = m.persona_id.as_deref().filter(|p| !p.is_empty()) {
            labels.push((p.to_string(), m.agent_id.clone()));
            for alias in persona_mention_labels(p) {
                labels.push((alias.to_string(), m.agent_id.clone()));
            }
        }
        labels.push((m.agent_id.clone(), m.agent_id.clone()));
    }
    labels.sort_by_key(|(label, _)| std::cmp::Reverse(label.len()));
    labels.dedup_by(|a, b| a.0.eq_ignore_ascii_case(&b.0) && a.1 == b.1);
    labels
}

/// True when the character immediately after a matched prefix is a mention
/// boundary (end of string, whitespace, punctuation, …).
fn mention_boundary_ok(tail: &str, matched_bytes: usize) -> bool {
    match tail.get(matched_bytes..).and_then(|rest| rest.chars().next()) {
        None => true,
        Some(c) => !c.is_ascii_alphanumeric() && c != '-' && c != '_',
    }
}

/// UTF-8–safe ASCII-case prefix match. `str::get` returns `None` when
/// `needle.len()` is not a char boundary in `haystack` (e.g. longer ASCII
/// label overlapping a multi-byte `—`).
fn ascii_prefix_eq_ignore_case(haystack: &str, needle: &str) -> bool {
    haystack
        .get(..needle.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(needle))
}

/// Extrait les mentions `@Name` / `@agent_id` roster dans l'ordre d'apparition.
/// Supporte les `display_name` multi-mots (`@devil's advocate`).
pub fn parse_mentions(content: &str, members: &[ChatRoomMember]) -> Vec<String> {
    let labels = mention_labels_longest_first(members);
    let mut out = Vec::new();
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'@' {
            let tail = &content[i + 1..];
            let mut matched = false;
            for (label, agent_id) in &labels {
                if !ascii_prefix_eq_ignore_case(tail, label) {
                    continue;
                }
                if !mention_boundary_ok(tail, label.len()) {
                    continue;
                }
                if !out.iter().any(|x| x == agent_id) {
                    out.push(agent_id.clone());
                }
                i += 1 + label.len();
                matched = true;
                break;
            }
            if matched {
                continue;
            }
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
                if let Some(token) = content.get(start..end) {
                    if let Some(id) = resolve_mention_token(token, members) {
                        if !out.iter().any(|x| x == &id) {
                            out.push(id);
                        }
                    }
                }
            }
            i = end;
        } else {
            // Advance one Unicode scalar so `i` stays on a char boundary
            // (needed before the next `@` / `content[i+1..]` slice).
            i += content[i..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(1);
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

/// Détecte les `@mention` roster vers d'autres membres (pas de prose / ids inventés).
pub fn detect_peer_addresses(
    reply: &str,
    members: &[ChatRoomMember],
    exclude_agent_id: &str,
) -> Vec<String> {
    if members.len() <= 1 {
        return Vec::new();
    }
    parse_mentions(reply, members)
        .into_iter()
        .filter(|id| id != exclude_agent_id)
        .collect()
}

/// Pairs `@` mentionnés quand la réplique pose une question ou formule une demande.
pub fn peers_requesting_response(
    reply: &str,
    members: &[ChatRoomMember],
    exclude_agent_id: &str,
) -> Vec<String> {
    if !reply_invites_peer_response(reply) {
        return Vec::new();
    }
    detect_peer_addresses(reply, members, exclude_agent_id)
}

/// True when an agent reply expects a peer to answer (question or explicit request).
pub fn reply_invites_peer_response(reply: &str) -> bool {
    if !reply.contains('@') {
        return false;
    }
    if reply.contains('?') || reply.contains('？') {
        return true;
    }
    let lower = reply.to_ascii_lowercase();
    if lower.contains("merci")
        || lower.contains("thanks")
        || lower.contains("thank you")
        || lower.contains("got it")
        || lower.contains("bien noté")
    {
        return false;
    }
    const REQUEST_MARKERS: &[&str] = &[
        "peux-tu",
        "peux tu",
        "pourrais-tu",
        "pourriez-vous",
        "pouvez-vous",
        "can you",
        "could you",
        "would you",
        "please",
        "s'il te plaît",
        "s'il vous plaît",
        "confirmes",
        "confirme",
        "confirm",
        "acceptes",
        "acceptez",
        "accept",
        "valide",
        "dis-moi",
        "tell me",
        "your thoughts",
        "ton avis",
        "your view",
        "weigh in",
        "est-ce que",
        "peux-tu confirmer",
    ];
    REQUEST_MARKERS.iter().any(|m| lower.contains(m))
}

/// Premier membre mentionné (compat tests / appels simples).
pub fn detect_peer_address(
    reply: &str,
    members: &[ChatRoomMember],
    exclude_agent_id: &str,
) -> Option<String> {
    detect_peer_addresses(reply, members, exclude_agent_id).into_iter().next()
}

/// Nombre effectif de tours agent autorisés (politique + plafond dur).
pub fn effective_max_turns(policy: &ChatRoomConductorPolicy) -> u32 {
    policy.max_agent_turns_per_user.clamp(1, HARD_MAX_AGENT_TURNS)
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
    fn peer_followup_when_reply_mentions_localized_persona_label() {
        let m = vec![
            ChatRoomMember {
                agent_id: "persona-researcher".into(),
                display_name: "Researcher".into(),
                persona_id: Some("researcher".into()),
                joined_ms: 1,
            },
            ChatRoomMember {
                agent_id: "persona-critic".into(),
                display_name: "Critic".into(),
                persona_id: Some("critic".into()),
                joined_ms: 2,
            },
        ];
        let peer = detect_peer_address("@Critique peux-tu confirmer ?", &m, "persona-researcher");
        assert_eq!(peer, Some("persona-critic".into()));
    }

    #[test]
    fn peer_followup_when_reply_mentions_member() {
        let m = members();
        let peer = detect_peer_address("@Beta can you confirm?", &m, "agent-alpha");
        assert_eq!(peer, Some("agent-beta".into()));
    }

    #[test]
    fn parse_mentions_multi_word_display_name() {
        let m = vec![
            ChatRoomMember {
                agent_id: "agent-devil".into(),
                display_name: "devil's advocate".into(),
                persona_id: None,
                joined_ms: 1,
            },
            ChatRoomMember {
                agent_id: "agent-researcher".into(),
                display_name: "Researcher".into(),
                persona_id: Some("researcher".into()),
                joined_ms: 2,
            },
        ];
        let ids = parse_mentions(
            "(Critic) @supervisor @devil's advocate @Researcher",
            &m,
        );
        assert_eq!(
            ids,
            vec![
                String::from("agent-devil"),
                String::from("agent-researcher"),
            ]
        );
    }

    #[test]
    fn detect_peer_addresses_returns_all_mentioned_peers() {
        let m = members();
        let peers = detect_peer_addresses("@Beta and @Gamma?", &m, "agent-alpha");
        assert_eq!(
            peers,
            vec![String::from("agent-beta"), String::from("agent-gamma")]
        );
    }

    #[test]
    fn rebound_enqueue_prioritizes_mentioned_peer() {
        let mut queue = vec![
            String::from("agent-alpha"),
            String::from("agent-beta"),
            String::from("agent-gamma"),
        ];
        let spoken = std::collections::HashSet::<String>::new();
        let peers = vec![String::from("agent-gamma")];
        for peer_id in peers {
            if spoken.contains(&peer_id) {
                continue;
            }
            queue.retain(|id| id != &peer_id);
            queue.insert(0, peer_id);
        }
        assert_eq!(queue[0], "agent-gamma");
        assert_eq!(queue[1], "agent-alpha");
    }

    #[test]
    fn parse_mentions_skips_utf8_unsafe_longer_label_prefix() {
        // Longer ASCII label (8 bytes) must not slice into the multi-byte em dash
        // after a shorter name — that used to panic in aos-agentd.
        let m = vec![
            ChatRoomMember {
                agent_id: "persona-critique".into(),
                display_name: "Critique".into(),
                persona_id: None,
                joined_ms: 1,
            },
            ChatRoomMember {
                agent_id: "persona-critic".into(),
                display_name: "Critic".into(),
                persona_id: Some("critic".into()),
                joined_ms: 2,
            },
        ];
        let ids = parse_mentions(
            "@Critic — je vais jouer le rôle…\n**@Critic** : « Vends la souve",
            &m,
        );
        assert_eq!(ids, vec![String::from("persona-critic")]);
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
        assert!(!roster.contains("@persona-critic"));
        assert!(roster.contains("persona=critic"));
    }

    #[test]
    fn peer_followup_requeues_member_who_already_spoke() {
        use std::collections::HashSet;

        let mut queue = Vec::new();
        let initial_done = HashSet::from([String::from("agent-beta")]);
        apply_peer_followups(
            &mut queue,
            &[String::from("agent-beta")],
            &initial_done,
            0,
            HARD_MAX_PEER_FOLLOWUPS,
        );
        assert_eq!(
            queue,
            vec![ScheduledTurn {
                agent_id: "agent-beta".into(),
                peer_followup: true,
            }]
        );
    }

    #[test]
    fn peer_followup_bumps_unspoken_member_to_front_without_duplicate() {
        use std::collections::HashSet;

        let mut queue = initial_schedule(vec![
            String::from("agent-alpha"),
            String::from("agent-beta"),
            String::from("agent-gamma"),
        ]);
        let initial_done = HashSet::from([String::from("agent-alpha")]);
        apply_peer_followups(
            &mut queue,
            &[String::from("agent-gamma")],
            &initial_done,
            0,
            HARD_MAX_PEER_FOLLOWUPS,
        );
        assert_eq!(queue[0].agent_id, "agent-gamma");
        assert!(!queue[0].peer_followup);
        assert_eq!(queue.len(), 3);
    }

    #[test]
    fn peer_followup_budget_caps_requeues() {
        use std::collections::HashSet;

        let mut queue = Vec::new();
        let initial_done = HashSet::from([
            String::from("agent-alpha"),
            String::from("agent-beta"),
            String::from("agent-gamma"),
        ]);
        apply_peer_followups(
            &mut queue,
            &[
                String::from("agent-beta"),
                String::from("agent-gamma"),
                String::from("agent-alpha"),
            ],
            &initial_done,
            0,
            2,
        );
        assert_eq!(queue.len(), 2);
        assert!(queue.iter().all(|t| t.peer_followup));
    }

    #[test]
    fn conduct_queue_skips_duplicate_initial_slots() {
        use std::collections::HashSet;

        let mut queue = initial_schedule(vec![
            String::from("agent-alpha"),
            String::from("agent-beta"),
            String::from("agent-alpha"),
        ]);
        let mut initial_done = HashSet::<String>::new();
        let mut turns = Vec::new();
        while let Some(turn) = pop_next_scheduled_turn(&mut queue, &initial_done) {
            assert!(!turn.peer_followup);
            initial_done.insert(turn.agent_id.clone());
            turns.push(turn.agent_id);
        }
        assert_eq!(
            turns,
            vec![String::from("agent-alpha"), String::from("agent-beta")]
        );
    }

    #[test]
    fn debate_schedules_peer_followup_after_initial_strip() {
        use std::collections::HashSet;

        let m = members();
        let mut queue = initial_schedule(build_initial_queue("Quels risques ?", &m));
        let mut initial_done = HashSet::<String>::new();
        let mut peer_followups_run = 0u32;
        let peer_budget = effective_peer_followup_budget(effective_max_turns(
            &ChatRoomConductorPolicy::default(),
        ));
        let max = effective_max_turns(&ChatRoomConductorPolicy::default()) as usize;
        let mut spoken = Vec::new();
        let replies = [
            ("agent-alpha", "@Beta ton avis ?"),
            ("agent-beta", "@Alpha confirmes ?"),
            ("agent-alpha", "oui"),
            ("agent-gamma", "ok"),
        ];
        let mut step = 0usize;
        while spoken.len() < max {
            let Some(turn) = pop_next_scheduled_turn(&mut queue, &initial_done) else {
                break;
            };
            spoken.push(turn.agent_id.clone());
            if turn.peer_followup {
                peer_followups_run += 1;
            } else {
                initial_done.insert(turn.agent_id.clone());
            }
            if step < replies.len() {
                let (speaker, reply) = replies[step];
                assert_eq!(speaker, turn.agent_id);
                let peers = peers_requesting_response(reply, &m, speaker);
                apply_peer_followups(
                    &mut queue,
                    &peers,
                    &initial_done,
                    peer_followups_run,
                    peer_budget,
                );
            }
            step += 1;
        }
        assert_eq!(
            spoken,
            vec![
                "agent-alpha",
                "agent-beta",
                "agent-alpha",
                "agent-gamma",
            ]
        );
        assert_eq!(peer_followups_run, 1);
    }

    #[test]
    fn no_peer_followup_when_reply_has_no_address() {
        use std::collections::HashSet;

        let m = members();
        let mut queue = initial_schedule(build_initial_queue("Hello", &m));
        let mut initial_done = HashSet::new();
        let turn = pop_next_scheduled_turn(&mut queue, &initial_done).unwrap();
        let speaker = turn.agent_id.clone();
        initial_done.insert(turn.agent_id);
        let peers = peers_requesting_response("I agree, no need to tag anyone.", &m, &speaker);
        assert!(peers.is_empty());
        apply_peer_followups(
            &mut queue,
            &peers,
            &initial_done,
            0,
            HARD_MAX_PEER_FOLLOWUPS,
        );
        assert_eq!(queue.len(), 2);
        assert!(queue.iter().all(|t| !t.peer_followup));
    }

    #[test]
    fn thanks_only_mention_does_not_request_peer_response() {
        let m = members();
        let reply = "Merci @Beta pour la synthèse.";
        assert_eq!(
            detect_peer_addresses(reply, &m, "agent-alpha"),
            vec!["agent-beta".to_string()]
        );
        assert!(peers_requesting_response(reply, &m, "agent-alpha").is_empty());
    }

    #[test]
    fn question_mention_requests_peer_response() {
        let m = members();
        let reply = "@Beta, peux-tu détailler les sources ?";
        assert_eq!(
            peers_requesting_response(reply, &m, "agent-alpha"),
            vec!["agent-beta".to_string()]
        );
    }

    #[test]
    fn reply_invites_peer_response_for_questions_and_requests() {
        assert!(reply_invites_peer_response("@Beta, peux-tu détailler ?"));
        assert!(reply_invites_peer_response("@Beta, please share sources."));
        assert!(!reply_invites_peer_response("Merci @Beta pour la synthèse."));
        assert!(!reply_invites_peer_response("I agree with the direction."));
    }

    #[test]
    fn parse_mentions_localized_persona_label() {
        let m = vec![ChatRoomMember {
            agent_id: "persona-researcher".into(),
            display_name: "Researcher".into(),
            persona_id: Some("researcher".into()),
            joined_ms: 1,
        }];
        let ids = parse_mentions("@Chercheur peux-tu résumer ?", &m);
        assert_eq!(ids, vec![String::from("persona-researcher")]);
    }

    #[test]
    fn directed_mention_queues_only_target_member() {
        let m = members();
        let queue = build_initial_queue("@Beta what do you think?", &m);
        assert_eq!(queue, vec![String::from("agent-beta")]);
    }

    #[test]
    fn two_member_strip_turn_yields_two_distinct_speakers() {
        let m = vec![
            ChatRoomMember {
                agent_id: "agent-2".into(),
                display_name: "Maya".into(),
                persona_id: None,
                joined_ms: 1,
            },
            ChatRoomMember {
                agent_id: "agent-3".into(),
                display_name: "Leo".into(),
                persona_id: None,
                joined_ms: 2,
            },
        ];
        let queue = build_initial_queue("bonjour, qui est là", &m);
        assert_eq!(queue.len(), 2);
        assert_eq!(queue[0], "agent-2");
        assert_eq!(queue[1], "agent-3");
    }
}
