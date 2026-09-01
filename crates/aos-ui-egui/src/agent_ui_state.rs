//! Mutable state owned by the Agents workspace.

use aos_proto::{AgentTrace, McpServerInfo, SkillInfo};
use std::collections::HashMap;
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

#[derive(Debug)]
pub(crate) struct AgentUiState {
    // creation form
    pub(crate) display_name: String,
    pub(crate) task: String,
    pub(crate) system_prompt: String,
    pub(crate) docs: String,
    pub(crate) max_steps: u32,
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
}

impl Default for AgentUiState {
    fn default() -> Self {
        Self {
            display_name: String::new(),
            task: String::new(),
            system_prompt: String::new(),
            docs: String::new(),
            max_steps: 0,
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
        }
    }
}

impl AgentUiState {
    pub(crate) fn with_max_steps(max_steps: u32) -> Self {
        Self {
            max_steps,
            ..Self::default()
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
}
