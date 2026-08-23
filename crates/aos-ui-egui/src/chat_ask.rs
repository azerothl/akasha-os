//! FIFO `user.ask` queue helpers.

use crate::cmd::ChatLine;
use aos_proto::AgentInfo;

pub(crate) fn ask_origin_closes(origin: &str) -> bool {
    origin == "ask-reply" || origin == "ask-timeout"
}

/// File FIFO des `user.ask` encore ouverts pour des agents actuellement bloqués.
pub(crate) fn pending_ask_ids(chat: &[ChatLine], blocked_ids: &[String]) -> Vec<String> {
    let blocked: std::collections::HashSet<&str> =
        blocked_ids.iter().map(String::as_str).collect();
    let mut order: Vec<String> = Vec::new();
    for line in chat {
        for att in &line.attachments {
            let Some((agent_id, _, origin)) = att.as_agent_ref() else {
                continue;
            };
            if origin == "ask" && blocked.contains(agent_id) {
                if !order.iter().any(|id| id == agent_id) {
                    order.push(agent_id.to_string());
                }
            } else if ask_origin_closes(origin) {
                order.retain(|id| id != agent_id);
            }
        }
    }
    for id in blocked_ids {
        if !order.iter().any(|x| x == id) {
            order.push(id.clone());
        }
    }
    order
}

pub(crate) fn chat_has_open_ask(chat: &[ChatLine], agent_id: &str) -> bool {
    let mut open = false;
    for line in chat {
        for att in &line.attachments {
            let Some((id, _, origin)) = att.as_agent_ref() else {
                continue;
            };
            if id != agent_id {
                continue;
            }
            if origin == "ask" {
                open = true;
            } else if ask_origin_closes(origin) {
                open = false;
            }
        }
    }
    open
}

pub(crate) fn agent_display_title(ag: &AgentInfo) -> String {
    let t = ag.directive.trim();
    if t.is_empty() {
        ag.agent_id.clone()
    } else {
        t.chars().take(48).collect()
    }
}

#[cfg(test)]
mod ask_queue_tests {
    use super::*;
    use crate::cmd::ChatLine;
    use aos_proto::ChatAttachment;

    fn ask_line(id: &str, origin: &str) -> ChatLine {
        ChatLine {
            role: "assistant".into(),
            text: origin.into(),
            attachments: vec![ChatAttachment::AgentRef {
                agent_id: id.into(),
                title: id.into(),
                origin: origin.into(),
            }],
            speaker_id: None,
        }
    }

    #[test]
    fn fifo_then_close() {
        let chat = vec![ask_line("a", "ask"), ask_line("b", "ask")];
        let q = pending_ask_ids(&chat, &["a".into(), "b".into()]);
        assert_eq!(q, vec!["a", "b"]);
        let chat = vec![
            ask_line("a", "ask"),
            ask_line("b", "ask"),
            ask_line("a", "ask-reply"),
        ];
        let q = pending_ask_ids(&chat, &["b".into()]);
        assert_eq!(q, vec!["b"]);
    }

    #[test]
    fn timeout_closes_and_blocked_without_card_appends() {
        let chat = vec![ask_line("a", "ask"), ask_line("a", "ask-timeout")];
        let q = pending_ask_ids(&chat, &["c".into()]);
        assert_eq!(q, vec!["c"]);
        assert!(!chat_has_open_ask(&chat, "a"));
    }
}
