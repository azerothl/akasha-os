//! Salon `user.ask` — pause mid-turn, wait for human reply, resume tool loop.

use crate::actions::parse_embedded_action_question;
use crate::room_runtime::RoomRoundState;
use crate::storage_path::{text_contains_disallowed_storage_path, ROOM_HOST_PATH_DISALLOWED};
use aos_ipc::BusClient;
use aos_proto::{ChatAttachment, ChatSessionAppendRequest, ChatSessionMessage};
use std::time::Duration;
use tokio::sync::oneshot;

/// Attente max d'une réponse `user.ask` en salon (aligné sur le worker solo).
pub const ROOM_ASK_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomAskWait {
    Answer(String),
    Timeout { waited_secs: u64 },
    Cancelled,
}

/// Formate une question `user.ask` (corps visible, sans id d'outil).
pub fn format_user_question(question: &str, choices: &[String]) -> String {
    if choices.is_empty() {
        question.to_string()
    } else {
        let opts: String = choices.iter().map(|c| format!("\n- {c}")).collect();
        format!("{question}\n\nChoix possibles :{opts}")
    }
}

fn ask_heading(display_name: &str) -> String {
    format!("**Question — {display_name}**")
}

/// Publie la question dans le fil salon (bulle speaker + carte ask).
pub async fn post_room_ask(
    bus: &BusClient,
    session_id: &str,
    agent_id: &str,
    display_name: &str,
    body: &str,
) -> Result<(), String> {
    let content = format!("{}\n\n{body}", ask_heading(display_name));
    bus.call::<ChatSessionAppendRequest, ChatSessionMessage>(
        "chat.session.append",
        &ChatSessionAppendRequest {
            session_id: session_id.to_string(),
            role: "assistant".into(),
            content,
            attachments: vec![ChatAttachment::AgentRef {
                agent_id: agent_id.to_string(),
                title: display_name.to_string(),
                origin: "ask".into(),
            }],
            speaker_id: None,
            speaker_name: None,
            thinking: None,
        },
        vec![],
    )
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Publie l'expiration ask (même surface que le worker solo).
pub async fn post_room_ask_timeout(
    bus: &BusClient,
    session_id: &str,
    agent_id: &str,
    display_name: &str,
    mins: u64,
) -> Result<(), String> {
    let content = format!("**Question expirée** ({mins} min) — l'agent continue sans réponse.");
    bus.call::<ChatSessionAppendRequest, ChatSessionMessage>(
        "chat.session.append",
        &ChatSessionAppendRequest {
            session_id: session_id.to_string(),
            role: "assistant".into(),
            content,
            attachments: vec![ChatAttachment::AgentRef {
                agent_id: agent_id.to_string(),
                title: display_name.to_string(),
                origin: "ask-timeout".into(),
            }],
            speaker_id: None,
            speaker_name: None,
            thinking: None,
        },
        vec![],
    )
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Débloque le tour salon en attente d'une réponse humaine.
pub async fn deliver_room_ask_reply(round: &RoomRoundState, answer: String) -> bool {
    let mut slot = round.ask_reply_tx.lock().await;
    if let Some(tx) = slot.take() {
        let _ = tx.send(answer);
        true
    } else {
        false
    }
}

/// Attend la réponse utilisateur ou timeout / annulation du tour.
pub async fn wait_room_ask_reply(round: &RoomRoundState, timeout: Duration) -> RoomAskWait {
    let (tx, rx) = oneshot::channel();
    {
        let mut slot = round.ask_reply_tx.lock().await;
        *slot = Some(tx);
    }

    let deadline = tokio::time::Instant::now() + timeout;
    let waited_secs = timeout.as_secs();
    let mut rx = rx;
    loop {
        if round.is_cancelled() {
            round.ask_reply_tx.lock().await.take();
            return RoomAskWait::Cancelled;
        }
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            round.ask_reply_tx.lock().await.take();
            return RoomAskWait::Timeout { waited_secs };
        }
        let tick = remaining.min(Duration::from_millis(200));
        tokio::select! {
            result = &mut rx => {
                return match result {
                    Ok(answer) => RoomAskWait::Answer(answer),
                    Err(_) => RoomAskWait::Cancelled,
                };
            }
            _ = tokio::time::sleep(tick) => {}
        }
    }
}

/// Exécute `user.ask` dans la boucle outils salon : question visible, attente, reprise.
pub async fn handle_room_user_ask(
    bus: &BusClient,
    round: &RoomRoundState,
    session_id: &str,
    agent_id: &str,
    display_name: &str,
    args: &serde_json::Value,
) -> Result<String, String> {
    let question = args
        .get("question")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if let Some(embedded) = parse_embedded_action_question(&question) {
        return Ok(format!(
            "user.ask incorrect : exécute directement \
             {{\"action\":\"{}\",\"args\":{}}} — pas de question à l'humain.",
            embedded.action, embedded.args
        ));
    }
    if question.is_empty() {
        return Ok("user.ask : question vide".into());
    }
    if text_contains_disallowed_storage_path(&question) {
        return Ok(ROOM_HOST_PATH_DISALLOWED.into());
    }
    let choices: Vec<String> = args
        .get("choices")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let body = format_user_question(&question, &choices);
    post_room_ask(bus, session_id, agent_id, display_name, &body).await?;
    match wait_room_ask_reply(round, ROOM_ASK_TIMEOUT).await {
        RoomAskWait::Answer(answer) if !answer.trim().is_empty() => {
            Ok(format!("réponse utilisateur : {answer}"))
        }
        RoomAskWait::Answer(_) => Ok(
            "(l'utilisateur a repris sans répondre — continue avec les infos disponibles)".into(),
        ),
        RoomAskWait::Timeout { waited_secs } => {
            let mins = (waited_secs / 60).max(1);
            post_room_ask_timeout(bus, session_id, agent_id, display_name, mins).await?;
            Ok(format!(
                "(aucune réponse après {mins} min — continue avec les infos disponibles ; ne repose pas la même question tout de suite)"
            ))
        }
        RoomAskWait::Cancelled => Err("tour annulé".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_user_question_with_choices() {
        let q = format_user_question("Quelle option ?", &["A".into(), "B".into()]);
        assert!(q.contains("Quelle option ?"));
        assert!(q.contains("Choix possibles"));
        assert!(q.contains("- A"));
    }

    #[test]
    fn user_ask_rejects_host_path_in_question() {
        let question = "peux-tu ouvrir e:/test/test ?";
        assert!(text_contains_disallowed_storage_path(question));
    }

    #[tokio::test]
    async fn deliver_room_ask_reply_sends_answer() {
        let round = RoomRoundState::new();
        let (tx, rx) = oneshot::channel();
        *round.ask_reply_tx.lock().await = Some(tx);
        assert!(deliver_room_ask_reply(&round, "ok".into()).await);
        assert_eq!(rx.await.unwrap(), "ok");
    }
}
