//! First-run onboarding helpers (see docs/UI.md).

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
