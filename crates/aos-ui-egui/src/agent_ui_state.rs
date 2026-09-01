//! Mutable state owned by the Agents workspace.

use crate::cmd::AgentNotice;
use aos_proto::{AgentSpec, AgentState, AgentTrace, McpServerInfo, SkillInfo};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

fn default_tool_selected() -> Vec<String> {
    vec![
        "notes.create".into(),
        "notes.list".into(),
        "notes.read".into(),
        "notes.search".into(),
        "notes.update".into(),
        "notes.links".into(),
        "notes.related".into(),
        "tasks.create".into(),
        "tasks.list".into(),
        "tasks.update".into(),
        "tasks.complete".into(),
        "module.scaffold".into(),
        "module.package".into(),
        "module.install".into(),
        "module.list".into(),
    ]
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RosterEditDraft {
    pub(crate) display_name: String,
    pub(crate) role: String,
    pub(crate) system_prompt: String,
    pub(crate) skills: Vec<String>,
    pub(crate) tools: Vec<String>,
    pub(crate) mcp_servers: Vec<String>,
    pub(crate) model_id: String,
}

#[derive(Debug)]
pub(crate) struct AgentUiState {
    // creation form
    pub(crate) display_name: String,
    pub(crate) task: String,
    pub(crate) system_prompt: String,
    pub(crate) docs: String,
    pub(crate) max_steps: u32,
    pub(crate) timeout_secs: u64,
    pub(crate) model_id: String,
    pub(crate) join_room_on_create: bool,
    // skills / tools / MCP
    pub(crate) skill_catalog: Vec<SkillInfo>,
    pub(crate) skill_selected: Vec<String>,
    pub(crate) mcp_catalog: Vec<McpServerInfo>,
    pub(crate) mcp_selected: Vec<String>,
    pub(crate) tool_selected: Vec<String>,
    // tabs and active agent
    pub(crate) open_tabs: Vec<String>,
    pub(crate) active_tab: Option<String>,
    // history, traces, steer
    pub(crate) show_history: bool,
    pub(crate) traces: HashMap<String, AgentTrace>,
    pub(crate) trace_fetched_at: Option<Instant>,
    pub(crate) steer_id: String,
    pub(crate) steer_txt: String,
    // live-transition tracking / notices
    pub(crate) prev_states: HashMap<String, AgentState>,
    pub(crate) notices: Vec<AgentNotice>,
    pub(crate) notified: HashSet<String>,
    /// Agent targeted for the next `user.ask` reply (multiple may be blocked).
    pub(crate) ask_reply_target: Option<String>,
    pub(crate) roster_edit_drafts: HashMap<String, RosterEditDraft>,
    /// Document-prep agent_id → original question (result card title).
    pub(crate) document_prep_agents: HashMap<String, String>,
    /// Suppress agent.kill ok status banners after document prep stop.
    pub(crate) document_prep_kill_pending: u32,
}

impl Default for AgentUiState {
    fn default() -> Self {
        Self {
            display_name: String::new(),
            task: String::new(),
            system_prompt: String::new(),
            docs: String::new(),
            max_steps: 0,
            timeout_secs: 0,
            model_id: String::new(),
            join_room_on_create: false,
            skill_catalog: Vec::new(),
            skill_selected: Vec::new(),
            mcp_catalog: Vec::new(),
            mcp_selected: Vec::new(),
            tool_selected: default_tool_selected(),
            open_tabs: Vec::new(),
            active_tab: None,
            show_history: false,
            traces: HashMap::new(),
            trace_fetched_at: None,
            steer_id: String::new(),
            steer_txt: String::new(),
            prev_states: HashMap::new(),
            notices: Vec::new(),
            notified: HashSet::new(),
            ask_reply_target: None,
            roster_edit_drafts: HashMap::new(),
            document_prep_agents: HashMap::new(),
            document_prep_kill_pending: 0,
        }
    }
}

impl AgentUiState {
    pub(crate) fn with_create_defaults(
        max_steps: u32,
        timeout_secs: u64,
        model_id: String,
    ) -> Self {
        Self {
            max_steps,
            timeout_secs,
            model_id,
            ..Self::default()
        }
    }

    pub(crate) fn create_model_id(&self) -> Option<String> {
        if self.model_id.is_empty() {
            None
        } else {
            Some(self.model_id.clone())
        }
    }

    pub(crate) fn open_tab(&mut self, id: &str) {
        if !self.open_tabs.iter().any(|t| t == id) {
            self.open_tabs.push(id.to_string());
        }
        self.active_tab = Some(id.to_string());
        self.steer_id = id.to_string();
        self.trace_fetched_at = None;
    }

    pub(crate) fn select_tab(&mut self, id: &str) {
        self.active_tab = Some(id.to_string());
        self.steer_id = id.to_string();
    }

    pub(crate) fn close_tab(&mut self, id: &str) {
        self.open_tabs.retain(|t| t != id);
        self.traces.remove(id);
        if self.active_tab.as_deref() == Some(id) {
            self.active_tab = self.open_tabs.last().cloned();
        }
    }

    pub(crate) fn close_all_tabs(&mut self) {
        self.open_tabs.clear();
        self.active_tab = None;
        self.traces.clear();
    }

    pub(crate) fn attach_document(&mut self, path: String) {
        if path.is_empty() {
            return;
        }
        if self.docs.is_empty() {
            self.docs = path;
        } else if !self.docs.split(',').any(|p| p.trim() == path) {
            self.docs = format!("{},{}", self.docs, path);
        }
    }

    pub(crate) fn upsert_trace(&mut self, trace: AgentTrace) {
        self.traces.insert(trace.agent_id.clone(), trace);
    }

    pub(crate) fn traces_poll_due(&self, interval: Duration) -> bool {
        self.trace_fetched_at
            .map(|t| t.elapsed() >= interval)
            .unwrap_or(true)
    }

    pub(crate) fn mark_traces_fetched(&mut self) {
        self.trace_fetched_at = Some(Instant::now());
    }

    pub(crate) fn prev_states_seeding(&self) -> bool {
        self.prev_states.is_empty()
    }

    pub(crate) fn record_prev_state(&mut self, agent_id: String, state: AgentState) {
        self.prev_states.insert(agent_id, state);
    }

    pub(crate) fn mark_notified(&mut self, agent_id: &str) {
        self.notified.insert(agent_id.to_string());
    }

    /// Insert a notice once per agent_id (deduped via `notified`).
    pub(crate) fn push_notice_once(&mut self, notice: AgentNotice) -> bool {
        if self.notified.contains(&notice.agent_id) {
            return false;
        }
        self.notified.insert(notice.agent_id.clone());
        self.notices.push(notice);
        true
    }

    pub(crate) fn dismiss_notices(&mut self, agent_ids: &[String]) {
        self.notices
            .retain(|x| !agent_ids.contains(&x.agent_id));
    }

    pub(crate) fn set_ask_reply_target(&mut self, agent_id: String) {
        self.ask_reply_target = Some(agent_id);
    }

    pub(crate) fn clear_ask_reply_if(&mut self, agent_id: &str) {
        if self.ask_reply_target.as_deref() == Some(agent_id) {
            self.ask_reply_target = None;
        }
    }

    pub(crate) fn clear_ask_reply_if_any(&mut self, ids: &[String]) {
        if self
            .ask_reply_target
            .as_ref()
            .is_some_and(|t| ids.iter().any(|id| id == t))
        {
            self.ask_reply_target = None;
        }
    }

    pub(crate) fn upsert_roster_draft_from_spec(&mut self, spec: &AgentSpec) {
        self.roster_edit_drafts.insert(
            spec.agent_id.clone(),
            RosterEditDraft {
                display_name: spec
                    .display_name
                    .clone()
                    .unwrap_or_else(|| spec.roster_display_name().to_string()),
                role: spec.goal.statement.clone(),
                system_prompt: spec.system_prompt.clone().unwrap_or_default(),
                skills: spec.skills.clone(),
                tools: spec.tools.clone(),
                mcp_servers: spec.mcp_servers.clone(),
                model_id: spec.model_id.clone().unwrap_or_default(),
            },
        );
    }

    pub(crate) fn register_document_prep(&mut self, agent_id: String, question: String) {
        self.document_prep_agents.insert(agent_id, question);
    }

    pub(crate) fn take_document_prep(&mut self, agent_id: &str) -> Option<String> {
        self.document_prep_agents.remove(agent_id)
    }

    pub(crate) fn bump_document_prep_kill_pending(&mut self) {
        self.document_prep_kill_pending = self.document_prep_kill_pending.saturating_add(1);
    }

    /// Returns true when a pending document-prep kill consumed the status banner.
    pub(crate) fn consume_document_prep_kill_ok(&mut self) -> bool {
        if self.document_prep_kill_pending > 0 {
            self.document_prep_kill_pending -= 1;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_tab_dedupes_and_sets_active_steer() {
        let mut state = AgentUiState::default();
        state.open_tab("a1");
        state.open_tab("a1");
        assert_eq!(state.open_tabs, vec!["a1".to_string()]);
        assert_eq!(state.active_tab.as_deref(), Some("a1"));
        assert_eq!(state.steer_id, "a1");
        assert!(state.trace_fetched_at.is_none());

        state.mark_traces_fetched();
        assert!(state.trace_fetched_at.is_some());
        state.open_tab("a2");
        assert_eq!(state.open_tabs, vec!["a1".to_string(), "a2".to_string()]);
        assert_eq!(state.active_tab.as_deref(), Some("a2"));
        assert_eq!(state.steer_id, "a2");
        assert!(state.trace_fetched_at.is_none());
    }

    #[test]
    fn close_tab_removes_trace_and_falls_back_active() {
        let mut state = AgentUiState::default();
        state.open_tab("a1");
        state.open_tab("a2");
        state.upsert_trace(AgentTrace {
            agent_id: "a1".into(),
            ..Default::default()
        });
        state.upsert_trace(AgentTrace {
            agent_id: "a2".into(),
            ..Default::default()
        });

        state.close_tab("a2");
        assert_eq!(state.open_tabs, vec!["a1".to_string()]);
        assert_eq!(state.active_tab.as_deref(), Some("a1"));
        assert!(!state.traces.contains_key("a2"));
        assert!(state.traces.contains_key("a1"));

        state.close_tab("a1");
        assert!(state.open_tabs.is_empty());
        assert_eq!(state.active_tab, None);
        assert!(state.traces.is_empty());
    }

    #[test]
    fn close_all_tabs_clears_detail_state() {
        let mut state = AgentUiState::default();
        state.open_tab("a1");
        state.upsert_trace(AgentTrace {
            agent_id: "a1".into(),
            ..Default::default()
        });
        state.close_all_tabs();
        assert!(state.open_tabs.is_empty());
        assert_eq!(state.active_tab, None);
        assert!(state.traces.is_empty());
    }

    #[test]
    fn select_tab_updates_steer_without_adding() {
        let mut state = AgentUiState::default();
        state.open_tab("a1");
        state.open_tab("a2");
        state.select_tab("a1");
        assert_eq!(state.open_tabs.len(), 2);
        assert_eq!(state.active_tab.as_deref(), Some("a1"));
        assert_eq!(state.steer_id, "a1");
    }

    #[test]
    fn attach_document_appends_and_dedupes() {
        let mut state = AgentUiState::default();
        state.attach_document("/notes/a.md".into());
        assert_eq!(state.docs, "/notes/a.md");
        state.attach_document("/notes/b.md".into());
        assert_eq!(state.docs, "/notes/a.md,/notes/b.md");
        state.attach_document("/notes/a.md".into());
        assert_eq!(state.docs, "/notes/a.md,/notes/b.md");
        state.attach_document(String::new());
        assert_eq!(state.docs, "/notes/a.md,/notes/b.md");
    }

    #[test]
    fn traces_poll_due_before_first_fetch() {
        let state = AgentUiState::default();
        assert!(state.traces_poll_due(Duration::from_secs(1)));
    }

    #[test]
    fn create_defaults_and_model_id_opt() {
        let state = AgentUiState::with_create_defaults(12, 300, "local-gguf".into());
        assert_eq!(state.max_steps, 12);
        assert_eq!(state.timeout_secs, 300);
        assert_eq!(state.create_model_id().as_deref(), Some("local-gguf"));

        let empty = AgentUiState::with_create_defaults(1, 60, String::new());
        assert!(empty.create_model_id().is_none());
    }

    #[test]
    fn push_notice_once_dedupes() {
        let mut state = AgentUiState::default();
        assert!(state.push_notice_once(AgentNotice {
            agent_id: "a1".into(),
            session_id: "s1".into(),
            summary: "done".into(),
        }));
        assert!(!state.push_notice_once(AgentNotice {
            agent_id: "a1".into(),
            session_id: "s1".into(),
            summary: "again".into(),
        }));
        assert_eq!(state.notices.len(), 1);
        assert!(state.notified.contains("a1"));
    }

    #[test]
    fn ask_reply_target_clear_helpers() {
        let mut state = AgentUiState::default();
        state.set_ask_reply_target("a1".into());
        state.clear_ask_reply_if("a2");
        assert_eq!(state.ask_reply_target.as_deref(), Some("a1"));
        state.clear_ask_reply_if("a1");
        assert!(state.ask_reply_target.is_none());

        state.set_ask_reply_target("a3".into());
        state.clear_ask_reply_if_any(&["a1".into(), "a3".into()]);
        assert!(state.ask_reply_target.is_none());
    }

    #[test]
    fn document_prep_register_take_and_kill_pending() {
        let mut state = AgentUiState::default();
        state.register_document_prep("a1".into(), "Write report".into());
        assert!(state.document_prep_agents.contains_key("a1"));
        assert_eq!(
            state.take_document_prep("a1").as_deref(),
            Some("Write report")
        );
        assert!(!state.document_prep_agents.contains_key("a1"));

        assert!(!state.consume_document_prep_kill_ok());
        state.bump_document_prep_kill_pending();
        state.bump_document_prep_kill_pending();
        assert!(state.consume_document_prep_kill_ok());
        assert_eq!(state.document_prep_kill_pending, 1);
        assert!(state.consume_document_prep_kill_ok());
        assert!(!state.consume_document_prep_kill_ok());
    }

    #[test]
    fn upsert_roster_draft_from_spec() {
        let mut state = AgentUiState::default();
        let spec = AgentSpec {
            agent_id: "roster-1".into(),
            goal: aos_proto::AgentGoal {
                statement: "assist".into(),
                ..Default::default()
            },
            kind: Default::default(),
            display_name: Some("Helper".into()),
            persona_id: None,
            system_prompt: Some("be helpful".into()),
            skills: vec!["notes-writer".into()],
            tools: vec!["notes.list".into()],
            mcp_servers: vec!["local".into()],
            documents: Vec::new(),
            caps: Vec::new(),
            model_id: Some("m1".into()),
            parent_id: None,
            session_id: None,
            budget: Default::default(),
            optimize_prompt: false,
            gate_mode: "ask".into(),
            origin: None,
        };
        state.upsert_roster_draft_from_spec(&spec);
        let draft = state.roster_edit_drafts.get("roster-1").expect("draft");
        assert_eq!(draft.display_name, "Helper");
        assert_eq!(draft.role, "assist");
        assert_eq!(draft.system_prompt, "be helpful");
        assert_eq!(draft.skills, vec!["notes-writer".to_string()]);
        assert_eq!(draft.tools, vec!["notes.list".to_string()]);
        assert_eq!(draft.mcp_servers, vec!["local".to_string()]);
        assert_eq!(draft.model_id, "m1");
    }
}
