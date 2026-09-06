//! Brouillons du composer persistés par session (S7.1).
//!
//! Le composer est global alors que les sessions sont multiples : sans ça,
//! changer de session (ou relancer l'app) perd le texte en cours de frappe.
//! Stockage : `var/run/composer_drafts.json` (`{session_id: texte}`),
//! écriture debouncée (3 s) depuis `UiApp::autosave_composer_draft`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Au-delà, le brouillon est tronqué (pas de blob de 10 Mo dans var/run).
pub const DRAFT_MAX_CHARS: usize = 20_000;
/// Garde-fou si des sessions supprimées laissent des orphelins.
pub const DRAFT_MAX_SESSIONS: usize = 100;

pub(crate) fn drafts_path() -> PathBuf {
    crate::os_open::aos_home().join("var/run/composer_drafts.json")
}

/// Insère/tronque, ou retire l'entrée si le texte est vide.
pub fn stash_draft(map: &mut HashMap<String, String>, session_id: &str, text: &str) {
    if session_id.is_empty() {
        return;
    }
    if text.is_empty() {
        map.remove(session_id);
        return;
    }
    let kept: String = text.chars().take(DRAFT_MAX_CHARS).collect();
    map.insert(session_id.to_string(), kept);
}

/// Lecture défensive : JSON invalide → vide, entrées surdimensionnées
/// ignorées, cap du nombre de sessions.
pub fn load_drafts_from(path: &Path) -> HashMap<String, String> {
    let raw = std::fs::read_to_string(path).unwrap_or_default();
    if raw.trim().is_empty() {
        return HashMap::new();
    }
    let parsed: HashMap<String, String> = serde_json::from_str(&raw).unwrap_or_default();
    parsed
        .into_iter()
        .filter(|(k, v)| !k.is_empty() && !v.is_empty() && v.chars().count() <= DRAFT_MAX_CHARS)
        .take(DRAFT_MAX_SESSIONS)
        .collect()
}

pub fn save_drafts_to(path: &Path, map: &HashMap<String, String>) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(raw) = serde_json::to_string_pretty(map) {
        let _ = std::fs::write(path, raw);
    }
}

pub(crate) fn load_drafts() -> HashMap<String, String> {
    load_drafts_from(&drafts_path())
}

pub(crate) fn save_drafts(map: &HashMap<String, String>) {
    save_drafts_to(&drafts_path(), map);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stash_truncates_and_removes_empty() {
        let mut map = HashMap::new();
        stash_draft(&mut map, "s1", "");
        assert!(map.is_empty());
        stash_draft(&mut map, "s1", "hello");
        assert_eq!(map["s1"], "hello");
        let big = "x".repeat(DRAFT_MAX_CHARS + 10);
        stash_draft(&mut map, "s1", &big);
        assert_eq!(map["s1"].chars().count(), DRAFT_MAX_CHARS);
        stash_draft(&mut map, "", "kept?");
        assert!(!map.contains_key(""));
    }

    #[test]
    fn load_rejects_invalid_and_oversized() {
        let dir = std::env::temp_dir().join(format!(
            "aos-drafts-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let path = dir.join("composer_drafts.json");
        assert!(load_drafts_from(&path).is_empty());
        std::fs::create_dir_all(&dir).expect("tmp dir");
        std::fs::write(&path, "{not json").expect("write");
        assert!(load_drafts_from(&path).is_empty());
        let mut map = HashMap::new();
        map.insert("ok".into(), "draft".into());
        map.insert("big".into(), "y".repeat(DRAFT_MAX_CHARS + 1));
        save_drafts_to(&path, &map);
        let loaded = load_drafts_from(&path);
        assert_eq!(loaded.get("ok").map(String::as_str), Some("draft"));
        assert!(!loaded.contains_key("big"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
