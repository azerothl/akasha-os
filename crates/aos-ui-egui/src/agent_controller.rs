//! Agent workspace controller — create/tab/trace commands and catalog events.

use crate::cmd::Cmd;
use crate::{agent_cap_holder, UiApp};
use aos_proto::{AgentSpec, AgentTrace, DocumentRef, McpServerInfo, SkillInfo};
use eframe::egui;
use std::time::Duration;

impl UiApp {
    pub(crate) fn send_agents_page_create(&mut self, room_active: bool, library: bool) {
        let join_active_room = room_active && self.agent_ui.join_room_on_create;
        let documents: Vec<DocumentRef> = self
            .agent_ui
            .docs
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|p| DocumentRef {
                path: p.to_string(),
                label: p.to_string(),
            })
            .collect();
        let session_id = if library || join_active_room {
            self.chat_state.active_session.clone()
        } else {
            None
        };
        let _ = self.cmd_tx.send(Cmd::AgentCreate {
            display_name: self.agent_ui.display_name.clone(),
            task: self.agent_ui.task.clone(),
            system_prompt: if self.agent_ui.system_prompt.is_empty() {
                None
            } else {
                Some(self.agent_ui.system_prompt.clone())
            },
            skills: self.agent_ui.skill_selected.clone(),
            tools: self.agent_ui.tool_selected.clone(),
            mcp_servers: self.agent_ui.mcp_selected.clone(),
            documents,
            optimize_prompt: false,
            max_steps: self.agent_ui.max_steps,
            timeout_secs: self.agent_ui.timeout_secs,
            model_id: self.agent_ui.create_model_id(),
            session_id,
            origin: "library".into(),
            join_active_room,
            library,
        });
    }

    pub(crate) fn open_agent_tab(&mut self, id: &str) {
        self.prefs.ui_layout.activity_panel_open = true;
        crate::prefs::save_preferences(&self.prefs);
        self.agent_ui.open_tab(id);
        let _ = self.cmd_tx.send(Cmd::AgentTrace { id: id.to_string() });
        let holder = agent_cap_holder(id);
        self.security_ui.select_holder(holder.clone());
        let _ = self.cmd_tx.send(Cmd::CapList { holder });
        if self
            .agents
            .iter()
            .find(|a| a.agent_id == id)
            .is_some_and(|a| a.is_roster())
        {
            let _ = self.cmd_tx.send(Cmd::AgentSpecGet { id: id.to_string() });
        }
    }

    pub(crate) fn close_agent_tab(&mut self, id: &str) {
        self.agent_ui.close_tab(id);
        if self.agent_ui.open_tabs.is_empty() {
            self.prefs.ui_layout.activity_panel_open = false;
            crate::prefs::save_preferences(&self.prefs);
        }
    }

    pub(crate) fn poll_agent_trace(&mut self, ctx: &egui::Context) {
        if self.agent_ui.open_tabs.is_empty() {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(400));
        if !self.agent_ui.traces_poll_due(Duration::from_secs(1)) {
            return;
        }
        self.agent_ui.mark_traces_fetched();
        for id in self.agent_ui.open_tabs.clone() {
            let _ = self.cmd_tx.send(Cmd::AgentTrace { id });
        }
    }

    pub(crate) fn on_agent_skills(&mut self, list: Vec<SkillInfo>) {
        self.agent_ui.skill_catalog = list;
    }

    pub(crate) fn on_agent_mcp_servers(&mut self, list: Vec<McpServerInfo>) {
        self.agent_ui.mcp_catalog = list;
    }

    pub(crate) fn on_agent_prompt_optimized(&mut self, prompt: String) {
        self.agent_ui.system_prompt = prompt;
        self.status = "prompt système optimisé".into();
    }

    pub(crate) fn on_agent_trace(&mut self, t: AgentTrace) {
        if let Some(question) = self.agent_ui.take_document_prep(&t.agent_id) {
            if let Some(path) = aos_agent::document_prep::path_from_trace(&t) {
                self.attach_document_result_card(&question, &path);
            }
        }
        self.agent_ui.upsert_trace(t);
    }

    pub(crate) fn on_agent_spec_loaded(&mut self, spec: AgentSpec) {
        self.agent_ui.upsert_roster_draft_from_spec(&spec);
    }

    pub(crate) fn on_document_prep_spawned(&mut self, agent_id: String, title: String) {
        self.agent_ui.register_document_prep(agent_id, title);
    }
}
