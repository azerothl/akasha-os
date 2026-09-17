//! Logical storage paths allowed in salon / agent tools (`/documents`, `/downloads`, notes).

use aos_ipc::BusClient;
use aos_proto::host_folder::folder_display_name;
use aos_proto::{ChatSessionAppendRequest, ChatSessionMessage};

/// Sentinel posted to the transcript — UI maps it to host-path toast copy (never a bubble).
pub const ROOM_HOST_PATH_DISALLOWED: &str = "room_host_path_disallowed";

/// True for the bare sentinel or `room_host_path_disallowed:{folder}`.
pub fn is_host_path_disallowed_outcome(msg: &str) -> bool {
    let trimmed = msg.trim();
    trimmed == ROOM_HOST_PATH_DISALLOWED
        || trimmed
            .strip_prefix(ROOM_HOST_PATH_DISALLOWED)
            .is_some_and(|rest| rest.starts_with(':'))
}

/// Sentinel plus a safe last-segment folder label (never a full host path).
pub fn host_path_disallowed_token(path: &str) -> String {
    match safe_folder_label(path) {
        Some(name) => format!("{ROOM_HOST_PATH_DISALLOWED}:{name}"),
        None => ROOM_HOST_PATH_DISALLOWED.to_string(),
    }
}

fn safe_folder_label(path: &str) -> Option<String> {
    let name = folder_display_name(path);
    if name == "?" || name.is_empty() {
        return None;
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return None;
    }
    Some(name)
}

/// True when a path names host storage or a logical prefix outside the sandbox.
pub fn is_disallowed_storage_path(path: &str) -> bool {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return false;
    }
    if looks_like_host_path(trimmed) {
        return true;
    }
    if !trimmed.starts_with('/') {
        return true;
    }
    if trimmed.starts_with("/documents/")
        || trimmed.starts_with("/downloads/")
        || trimmed == "/documents"
        || trimmed == "/downloads"
    {
        return false;
    }
    true
}

fn looks_like_host_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    if path.contains('\\') {
        return true;
    }
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return true;
    }
    false
}

/// Scan free text for a disallowed storage path token (user message or `user.ask` question).
pub fn text_contains_disallowed_storage_path(text: &str) -> bool {
    for token in text.split_whitespace() {
        let token = token.trim_matches(|c: char| {
            matches!(
                c,
                ',' | '.' | ';' | ':' | '!' | '?' | ')' | '(' | '"' | '\'' | '`'
            )
        });
        if token.is_empty() {
            continue;
        }
        if (token.starts_with('/') || looks_like_host_path(token))
            && is_disallowed_storage_path(token)
        {
            return true;
        }
    }
    false
}

/// Publie le sentinel dans le fil — l'UI toaste une fois à l'ingestion, sans bulle.
pub async fn post_room_host_path_notice(
    bus: &BusClient,
    session_id: &str,
    notice: &str,
) -> Result<(), String> {
    let content = if is_host_path_disallowed_outcome(notice) {
        notice.trim().to_string()
    } else {
        ROOM_HOST_PATH_DISALLOWED.to_string()
    };
    bus.call::<ChatSessionAppendRequest, ChatSessionMessage>(
        "chat.session.append",
        &ChatSessionAppendRequest {
            session_id: session_id.to_string(),
            role: "system".into(),
            content,
            attachments: vec![],
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_drive_paths_disallowed() {
        assert!(is_disallowed_storage_path("e:/test/test"));
        assert!(is_disallowed_storage_path(r"C:\Users\me\file.txt"));
    }

    #[test]
    fn logical_documents_and_downloads_allowed() {
        assert!(!is_disallowed_storage_path("/documents/notes/a.md"));
        assert!(!is_disallowed_storage_path("/downloads/report.md"));
    }

    #[test]
    fn other_logical_prefixes_disallowed() {
        assert!(is_disallowed_storage_path("/home/user/x"));
        assert!(is_disallowed_storage_path("/var/tmp/x"));
    }

    #[test]
    fn text_scan_finds_host_path() {
        assert!(text_contains_disallowed_storage_path(
            "écris dans e:/test/test"
        ));
        assert!(!text_contains_disallowed_storage_path(
            "sauve sous /downloads/rapport.md"
        ));
    }

    #[test]
    fn token_uses_safe_last_segment_only() {
        assert_eq!(
            host_path_disallowed_token("e:/akasha/agents"),
            "room_host_path_disallowed:agents"
        );
        assert_eq!(
            host_path_disallowed_token(r"C:\Users\me\file.txt"),
            "room_host_path_disallowed:file.txt"
        );
        assert!(is_host_path_disallowed_outcome(
            "room_host_path_disallowed:agents"
        ));
        assert!(is_host_path_disallowed_outcome(ROOM_HOST_PATH_DISALLOWED));
        assert!(!is_host_path_disallowed_outcome(
            "room_host_path_disallowed_other"
        ));
    }
}
