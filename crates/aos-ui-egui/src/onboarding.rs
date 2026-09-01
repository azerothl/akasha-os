//! First-run onboarding helpers (see docs/UI.md).

use crate::os_open::aos_home;
use crate::prefs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OnboardingState {
    pub(crate) completed: bool,
    pub(crate) language: String,
    pub(crate) routing: String,
    pub(crate) trust_default: String,
    #[serde(default)]
    pub(crate) tutorial_step: u32,
    /// User sent a chat message during the first-run chat step.
    #[serde(default)]
    pub(crate) chat_sent: bool,
    /// Assistant replied to the first-run chat message.
    #[serde(default)]
    pub(crate) first_chat_done: bool,
}

impl Default for OnboardingState {
    fn default() -> Self {
        let language = prefs::detect_os_language();
        Self {
            completed: false,
            language,
            routing: "local_only".into(),
            trust_default: "medium".into(),
            tutorial_step: 0,
            chat_sent: false,
            first_chat_done: false,
        }
    }
}

fn onboarding_path() -> std::path::PathBuf {
    aos_home().join("var/run/onboarding.json")
}

pub(crate) fn load_onboarding() -> OnboardingState {
    let path = onboarding_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub(crate) fn save_onboarding(state: &OnboardingState) {
    let path = onboarding_path();
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    if let Ok(serialized) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(path, serialized);
    }
}

pub const TUTORIAL_STEP_COUNT: u32 = 3;

/// Last tutorial step index (0-based).
pub const TUTORIAL_LAST_STEP: u32 = TUTORIAL_STEP_COUNT - 1;

/// Whether the user may advance past the chat step without a completed turn.
pub fn chat_step_can_advance(chat_sent: bool, first_chat_done: bool) -> bool {
    chat_sent && first_chat_done
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tutorial_has_three_steps() {
        assert_eq!(TUTORIAL_STEP_COUNT, 3);
        assert_eq!(TUTORIAL_LAST_STEP, 2);
    }

    #[test]
    fn chat_step_requires_reply() {
        assert!(!chat_step_can_advance(true, false));
        assert!(!chat_step_can_advance(false, false));
        assert!(chat_step_can_advance(true, true));
    }
}
