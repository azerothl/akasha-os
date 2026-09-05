//! Event handling for agent roster transitions and notices.

use crate::cmd::{AgentNotice, ChatLine, Cmd};
use crate::chat_ask::{agent_display_title, chat_has_open_ask};
use crate::{
    agent_canvas_session_ops, agent_completion_chat_text, agent_panel, i18n, UiApp,
};
use aos_proto::{AgentInfo, AgentState, ChatAttachment};

pub(crate) fn on_spawned(
    app: &mut UiApp,
    session_id: String,
    agent_id: String,
    title: String,
    origin: String,
    ack: String,
) {
    if origin == "document" {
        app.on_document_prep_spawned(agent_id.clone(), title.clone());
        if app.chat_state.active_session.as_deref() == Some(session_id.as_str()) {
            app.attach_document_progress_agent(&agent_id, &title);
        }
    } else {
        app.arm_pending_module_agent(&title);
        if app.chat_state.active_session.as_deref() == Some(session_id.as_str()) {
            app.chat.push(ChatLine {
                role: "assistant".into(),
                text: ack,
                attachments: vec![ChatAttachment::AgentRef {
                    agent_id,
                    title,
                    origin,
                }],
                speaker_id: None,
                speaker_name: None,
                thinking: None,
            });
        } else {
            app.status = format!("agent lancé : {agent_id}");
        }
    }
}

pub(crate) fn on_agents(app: &mut UiApp, agents: Vec<AgentInfo>) {
    let t = i18n::strings(&app.prefs.language);
    if app.scenario_ui.pending_note_agent
        && agents.iter().any(|ag| {
            matches!(ag.state, AgentState::Done | AgentState::Failed | AgentState::Killed)
        })
    {
        let _ = app.cmd_tx.send(Cmd::NotesList);
    }
    if app.scenario_ui.pending_module_agent
        && agents.iter().any(|ag| {
            matches!(ag.state, AgentState::Done | AgentState::Failed | AgentState::Killed)
        })
    {
        let _ = app.cmd_tx.send(Cmd::ModuleList);
    }
    let seeding = app.agent_ui.prev_states_seeding();
    for ag in &agents {
        if let Some(plan) = ag.deep_plan.as_ref() {
            if ag
                .session_id
                .as_deref()
                .is_some_and(|sid| app.chat_state.active_session.as_deref() == Some(sid))
            {
                let had = app.chat.iter().any(|l| {
                    l.attachments.iter().any(|a| {
                        matches!(
                            a,
                            ChatAttachment::DeepPlan { plan_id, .. } if plan_id == &plan.id
                        ) || matches!(
                            a,
                            ChatAttachment::DeepPlan { agent_id: aid, .. }
                                if aid == &ag.agent_id
                        )
                    })
                });
                let idx = crate::deep_plan_ui::sync_deep_plan_in_chat(
                    &mut app.chat,
                    &ag.agent_id,
                    plan,
                );
                if !had {
                    app.chat_state.view.deep_plan_open.insert(idx);
                }
            }
        }
        let prev = app.agent_ui.prev_states.get(&ag.agent_id).cloned();
        let terminal = matches!(ag.state, AgentState::Done | AgentState::Failed | AgentState::Killed);
        let was_active = prev
            .as_ref()
            .map(|p| !matches!(p, AgentState::Done | AgentState::Failed | AgentState::Killed))
            .unwrap_or(false);
        if terminal {
            if app.agent_ui.document_prep_agents.contains_key(&ag.agent_id)
                && was_active
                && !seeding
            {
                if ag.state == AgentState::Done {
                    let _ = app.cmd_tx.send(Cmd::AgentTrace {
                        id: ag.agent_id.clone(),
                    });
                } else {
                    app.agent_ui.take_document_prep(&ag.agent_id);
                    app.agent_ui.mark_notified(&ag.agent_id);
                }
            } else if let Some(sid) = &ag.session_id {
                let on_this_session =
                    app.chat_state.active_session.as_deref() == Some(sid.as_str());
                let already = app.chat.iter().any(|l| {
                    l.attachments.iter().any(|a| {
                        matches!(
                            a,
                            ChatAttachment::AgentRef { agent_id, origin, .. }
                                if agent_id == &ag.agent_id && origin == "completion"
                        )
                    })
                });
                if on_this_session {
                    let session_ops = agent_canvas_session_ops(
                        ag,
                        app.chat_state.active_session.as_deref(),
                        &app.chat_state.view.canvas.ops,
                    );
                    let trace = app.agent_ui.traces.get(&ag.agent_id);
                    let content = agent_completion_chat_text(
                        ag,
                        &t,
                        session_ops,
                        trace,
                        app.scenario_ui.pending_note_agent,
                        app.workspace_ui.notes.notes.len(),
                    );
                    if already {
                        if !ag.last_output.trim().is_empty() {
                            if let Some(line) = app.chat.iter_mut().find(|l| {
                                l.attachments.iter().any(|a| {
                                    matches!(
                                        a,
                                        ChatAttachment::AgentRef { agent_id, origin, .. }
                                            if agent_id == &ag.agent_id && origin == "completion"
                                    )
                                })
                            }) {
                                if line.text != content {
                                    line.text = content;
                                }
                            }
                        }
                    } else if !seeding
                        && !app.agent_ui.document_prep_agents.contains_key(&ag.agent_id)
                    {
                        if content.is_empty()
                            && !agent_panel::canvas_draw_step_cap_continue(
                                Some(ag),
                                session_ops,
                                trace,
                            )
                        {
                            app.agent_ui.mark_notified(&ag.agent_id);
                            continue;
                        }
                        app.chat.push(ChatLine {
                            role: "assistant".into(),
                            text: content,
                            attachments: vec![ChatAttachment::AgentRef {
                                agent_id: ag.agent_id.clone(),
                                title: ag.directive.clone(),
                                origin: "completion".into(),
                            }],
                            speaker_id: None,
                            speaker_name: None,
                            thinking: None,
                        });
                    }
                } else if !seeding
                    && !on_this_session
                    && !app.agent_ui.notified.contains(&ag.agent_id)
                    && was_active
                {
                    let session_ops = agent_canvas_session_ops(
                        ag,
                        app.chat_state.active_session.as_deref(),
                        &app.chat_state.view.canvas.ops,
                    );
                    let trace = app.agent_ui.traces.get(&ag.agent_id);
                    let summary = if agent_panel::notes_create_fail_chrome(
                        Some(ag),
                        app.scenario_ui.pending_note_agent,
                        app.workspace_ui.notes.notes.len(),
                    ) {
                        t.notes_create_failed.to_string()
                    } else if agent_panel::canvas_draw_failure_muted(
                        Some(ag),
                        session_ops,
                        trace,
                    ) {
                        String::new()
                    } else {
                        match ag.state {
                            AgentState::Done => format!("{} terminé", ag.display_title()),
                            AgentState::Failed => {
                                if agent_panel::canvas_draw_fail_chrome(
                                    Some(ag),
                                    session_ops,
                                    trace,
                                ) {
                                    t.canvas_draw_failed.to_string()
                                } else if ag.fail_reason.as_deref()
                                    == Some(aos_agent::actions::THREAD_FAIL_COULD_NOT_ACT)
                                {
                                    i18n::agent_could_not_act_message(&t)
                                } else if ag.fail_reason.as_deref()
                                    == Some(aos_agent::actions::THREAD_FAIL_COULD_NOT_CONTINUE)
                                    || ag.fail_reason.as_deref().is_some_and(
                                        aos_agent::context_budget::is_overflow_fail_reason,
                                    )
                                {
                                    i18n::agent_could_not_continue_message(&t)
                                } else {
                                    format!(
                                        "{} échoué — {}",
                                        ag.display_title(),
                                        i18n::resolve_agent_fail_reason(
                                            &t,
                                            ag.fail_reason.as_deref(),
                                        )
                                    )
                                }
                            }
                            AgentState::Killed => format!("{} arrêté", ag.display_title()),
                            _ => format!("{} terminé", ag.display_title()),
                        }
                    };
                    if summary.is_empty() {
                        app.agent_ui.mark_notified(&ag.agent_id);
                        continue;
                    }
                    app.agent_ui.push_notice_once(AgentNotice {
                        agent_id: ag.agent_id.clone(),
                        session_id: sid.clone(),
                        summary,
                    });
                }
            }
        }
        if prev == Some(AgentState::Blocked) && ag.state != AgentState::Blocked {
            app.agent_ui.clear_ask_reply_if(&ag.agent_id);
            if let Some(sid) = &ag.session_id {
                let on_this_session =
                    app.chat_state.active_session.as_deref() == Some(sid.as_str());
                if on_this_session && chat_has_open_ask(&app.chat, &ag.agent_id) {
                    let expired = ag.last_output.starts_with("Question expirée");
                    let text = if expired {
                        "**Question expirée** — l'agent continue sans réponse.".into()
                    } else {
                        "**Question close** — l'agent a repris.".into()
                    };
                    app.chat.push(ChatLine {
                        role: "assistant".into(),
                        text,
                        attachments: vec![ChatAttachment::AgentRef {
                            agent_id: ag.agent_id.clone(),
                            title: ag.directive.clone(),
                            origin: "ask-timeout".into(),
                        }],
                        speaker_id: None,
                        speaker_name: None,
                        thinking: None,
                    });
                }
            }
        }
        if ag.state == AgentState::Blocked {
            if let Some(sid) = &ag.session_id {
                let on_this_session =
                    app.chat_state.active_session.as_deref() == Some(sid.as_str());
                let already = chat_has_open_ask(&app.chat, &ag.agent_id);
                if on_this_session
                    && !already
                    && !ag.last_output.trim().is_empty()
                    && !ag.last_output.starts_with("Question expirée")
                {
                    let title = agent_display_title(ag);
                    let body = format!("**Question — {title}**\n\n{}", ag.last_output.trim());
                    app.chat.push(ChatLine {
                        role: "assistant".into(),
                        text: body,
                        attachments: vec![ChatAttachment::AgentRef {
                            agent_id: ag.agent_id.clone(),
                            title: ag.directive.clone(),
                            origin: "ask".into(),
                        }],
                        speaker_id: None,
                        speaker_name: None,
                        thinking: None,
                    });
                } else if !on_this_session && !app.agent_ui.notified.contains(&ag.agent_id) {
                    app.agent_ui.push_notice_once(AgentNotice {
                        agent_id: ag.agent_id.clone(),
                        session_id: sid.clone(),
                        summary: format!("{} pose une question", agent_display_title(ag)),
                    });
                }
            }
        }
        app.agent_ui
            .record_prev_state(ag.agent_id.clone(), ag.state.clone());
    }
    app.agents = agents;
}
