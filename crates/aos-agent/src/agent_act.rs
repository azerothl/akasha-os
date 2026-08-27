//! Agent act gate policy (tester-cohort slice 1). Human phrases live in UI i18n.

/// Gate mode for chat-delegated agents (`ask` prompts before each mutating act).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentGateMode {
    Ask,
    Autonomous,
}

impl AgentGateMode {
    pub fn parse(s: &str) -> Self {
        if s.eq_ignore_ascii_case("autonomous") || s.eq_ignore_ascii_case("auto") {
            Self::Autonomous
        } else {
            Self::Ask
        }
    }

    pub fn as_pref_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Autonomous => "autonomous",
        }
    }
}

/// Whether this tool action should pause for inline Allow Once / Deny in chat.
pub fn requires_act_gate(action: &str) -> bool {
    let name = action.trim();
    if name.is_empty()
        || matches!(
            name,
            "noop"
                | "goal.complete"
                | "goal.fail"
                | "task.assess"
                | "plan.update"
                | "user.ask"
                | "agent.spawn"
                | "agent.await"
                | "agent.create"
                | "cap.request"
        )
    {
        return false;
    }
    if name.starts_with("notes.") {
        return !matches!(name, "notes.list" | "notes.read" | "notes.search" | "notes.related" | "notes.links");
    }
    if name.starts_with("tasks.") {
        return !matches!(name, "tasks.list");
    }
    if name.starts_with("canvas.") {
        return !matches!(name, "canvas.get" | "canvas.export");
    }
    if name.starts_with("fs.") {
        return matches!(name, "fs.write" | "fs.delete" | "fs.mkdir");
    }
    if name.starts_with("media.") {
        return true;
    }
    if name.starts_with("module.") || name == "skill.create" {
        return false;
    }
    if name.starts_with("web.") || name == "net.fetch" {
        return true;
    }
    if name == "mem.episodic_write" {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_mutating_notes_not_list() {
        assert!(requires_act_gate("notes.create"));
        assert!(!requires_act_gate("notes.list"));
    }

    #[test]
    fn gate_canvas_mutate_not_get() {
        assert!(requires_act_gate("canvas.stroke"));
        assert!(!requires_act_gate("canvas.get"));
    }

    #[test]
    fn gate_mode_parse() {
        assert_eq!(AgentGateMode::parse("ask"), AgentGateMode::Ask);
        assert_eq!(AgentGateMode::parse("autonomous"), AgentGateMode::Autonomous);
    }

    #[test]
    fn act_sentences_not_in_agent_crate() {
        let src = include_str!("agent_act.rs");
        let marker = ["pub", " fn ", "phrase", "_fr"].concat();
        assert!(
            !src.contains(&marker),
            "agent_act must not format user-visible phrases; use UI i18n"
        );
    }
}
