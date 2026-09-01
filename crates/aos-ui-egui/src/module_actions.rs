//! Bus actions for declarative modules and bundled Notes/Tasks modules.

use crate::cmd::Evt;
use crate::{notes_panel, tasks_panel};
use aos_ipc::BusClient;
use aos_proto::decl_ui::ModuleUiResponse;
use aos_proto::{
    AgentIdRequest, ModuleIdRequest, ModuleInvokeRequest, ModuleInvokeResponse,
};
use std::sync::mpsc::Sender;
use std::sync::Arc;

pub(crate) async fn load_module_ui(bus: &Arc<BusClient>, evt_tx: &Sender<Evt>, module: &str) {
    match bus
        .call::<ModuleIdRequest, ModuleUiResponse>(
            "module.ui",
            &ModuleIdRequest {
                module: module.to_string(),
            },
            vec![],
        )
        .await
    {
        Ok(resp) => {
            let _ = evt_tx.send(Evt::ModuleUiLoaded(resp));
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiFailed {
                module: module.to_string(),
                error: e.to_string(),
            });
        }
    }
}

pub(crate) async fn invoke_module_bind(
    bus: &Arc<BusClient>,
    evt_tx: &Sender<Evt>,
    module: &str,
    tool: &str,
) {
    let req = ModuleInvokeRequest {
        module: module.to_string(),
        tool: tool.to_string(),
        args: serde_json::json!({}),
        actor: "human:ui".into(),
        actor_caps: vec![format!("tool.invoke:{module}")],
        trace_id: format!("ui-mod-bind-{module}-{tool}"),
    };
    match bus
        .call::<ModuleInvokeRequest, ModuleInvokeResponse>("module.invoke", &req, vec![])
        .await
    {
        Ok(r) if r.ok => {
            let _ = evt_tx.send(Evt::ModuleUiBind {
                module: module.to_string(),
                tool: tool.to_string(),
                result: r.result,
                error: None,
            });
        }
        Ok(r) => {
            let _ = evt_tx.send(Evt::ModuleUiBind {
                module: module.to_string(),
                tool: tool.to_string(),
                result: serde_json::Value::Null,
                error: r.error,
            });
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiBind {
                module: module.to_string(),
                tool: tool.to_string(),
                result: serde_json::Value::Null,
                error: Some(e.to_string()),
            });
        }
    }
}

pub(crate) async fn invoke_module_tool(
    bus: &Arc<BusClient>,
    evt_tx: &Sender<Evt>,
    module: &str,
    tool: &str,
    args: serde_json::Value,
) {
    let req = ModuleInvokeRequest {
        module: module.to_string(),
        tool: tool.to_string(),
        args,
        actor: "human:ui".into(),
        actor_caps: vec![format!("tool.invoke:{module}")],
        trace_id: format!("ui-mod-{module}-{tool}"),
    };
    match bus
        .call::<ModuleInvokeRequest, ModuleInvokeResponse>("module.invoke", &req, vec![])
        .await
    {
        Ok(r) if r.ok => {
            let _ = evt_tx.send(Evt::ModuleUiInvokeDone {
                module: module.to_string(),
                tool: tool.to_string(),
                ok: true,
                result: r.result.clone(),
                error: None,
            });
            let _ = evt_tx.send(Evt::ModuleUiBind {
                module: module.to_string(),
                tool: tool.to_string(),
                result: r.result,
                error: None,
            });
        }
        Ok(r) => {
            let _ = evt_tx.send(Evt::ModuleUiInvokeDone {
                module: module.to_string(),
                tool: tool.to_string(),
                ok: false,
                result: serde_json::Value::Null,
                error: r.error.clone(),
            });
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiInvokeDone {
                module: module.to_string(),
                tool: tool.to_string(),
                ok: false,
                result: serde_json::Value::Null,
                error: Some(e.to_string()),
            });
        }
    }
}

pub(crate) async fn invoke_notes(
    bus: &Arc<BusClient>,
    evt_tx: &Sender<Evt>,
    tool: &str,
    args: serde_json::Value,
) {
    let save_payload = if matches!(tool, "notes.create" | "notes.update") {
        Some(notes_save_payload_from_args(&args))
    } else {
        None
    };
    let req = ModuleInvokeRequest {
        module: "notes".into(),
        tool: tool.into(),
        args,
        actor: "human:ui".into(),
        actor_caps: vec![
            "fs.read:/documents/notes/**".into(),
            "fs.write:/documents/notes/**".into(),
            "mem.write:module:notes".into(),
            "mem.query:module:notes".into(),
            "tool.invoke:notes".into(),
        ],
        trace_id: format!("ui-notes-{}", tool),
    };
    match bus
        .call::<ModuleInvokeRequest, ModuleInvokeResponse>("module.invoke", &req, vec![])
        .await
    {
        Ok(r) if r.ok => {
            let pretty = serde_json::to_string_pretty(&r.result).unwrap_or_default();
            let _ = evt_tx.send(Evt::Notes(pretty));
            match tool {
                "notes.list" => {
                    let notes = notes_panel::parse_list_result(&r.result);
                    let _ = evt_tx.send(Evt::NotesListed(notes));
                }
                "notes.read" => {
                    if let Some(d) = notes_panel::parse_detail(&r.result) {
                        let _ = evt_tx.send(Evt::NoteLoaded(d));
                    }
                }
                "notes.search" => {
                    let hits = notes_panel::parse_search_hits(&r.result);
                    let _ = evt_tx.send(Evt::NotesSearchHits(hits));
                }
                "notes.related" => {
                    let hits = notes_panel::parse_related(&r.result);
                    let _ = evt_tx.send(Evt::NotesRelated(hits));
                }
                "notes.create" | "notes.update" => {
                    let path = r
                        .result
                        .get("path")
                        .and_then(|p| p.as_str())
                        .unwrap_or("")
                        .to_string();
                    let slug = r
                        .result
                        .get("slug")
                        .and_then(|p| p.as_str())
                        .unwrap_or("")
                        .to_string();
                    let title = r
                        .result
                        .get("title")
                        .and_then(|p| p.as_str())
                        .unwrap_or("")
                        .to_string();
                    let _ = evt_tx.send(Evt::NotesSaved { path, slug, title });
                    // Rafraîchir la liste après écriture.
                    let list_req = ModuleInvokeRequest {
                        module: "notes".into(),
                        tool: "notes.list".into(),
                        args: serde_json::json!({}),
                        actor: "human:ui".into(),
                        actor_caps: vec![
                            "fs.read:/documents/notes/**".into(),
                            "tool.invoke:notes".into(),
                        ],
                        trace_id: "ui-notes-list-after-save".into(),
                    };
                    if let Ok(lr) = bus
                        .call::<ModuleInvokeRequest, ModuleInvokeResponse>(
                            "module.invoke",
                            &list_req,
                            vec![],
                        )
                        .await
                    {
                        if lr.ok {
                            let notes = notes_panel::parse_list_result(&lr.result);
                            let _ = evt_tx.send(Evt::NotesListed(notes));
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(r) => {
            if let Some((title, content, path)) = save_payload {
                let _ = evt_tx.send(Evt::NotesSaveFailed {
                    title,
                    content,
                    path,
                });
            } else {
                let _ = evt_tx.send(Evt::Error(
                    r.error.unwrap_or_else(|| "notes: échec".into()),
                ));
            }
        }
        Err(_) => {
            if let Some((title, content, path)) = save_payload {
                let _ = evt_tx.send(Evt::NotesSaveFailed {
                    title,
                    content,
                    path,
                });
            } else {
                let _ = evt_tx.send(Evt::Error("notes: échec".into()));
            }
        }
    }
}

fn notes_save_payload_from_args(args: &serde_json::Value) -> (String, String, Option<String>) {
    let title = args
        .get("title")
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .to_string();
    let content = args
        .get("content")
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .to_string();
    let path = args
        .get("path")
        .and_then(|p| p.as_str())
        .map(|s| s.to_string());
    (title, content, path)
}

pub(crate) async fn invoke_tasks(
    bus: &Arc<BusClient>,
    evt_tx: &Sender<Evt>,
    tool: &str,
    args: serde_json::Value,
) {
    let req = ModuleInvokeRequest {
        module: "tasks".into(),
        tool: tool.into(),
        args,
        actor: "human:ui".into(),
        actor_caps: vec![
            "fs.read:/documents/tasks/**".into(),
            "fs.write:/documents/tasks/**".into(),
            "tool.invoke:tasks".into(),
        ],
        trace_id: format!("ui-tasks-{tool}"),
    };
    match bus
        .call::<ModuleInvokeRequest, ModuleInvokeResponse>("module.invoke", &req, vec![])
        .await
    {
        Ok(r) if r.ok => {
            match tool {
                "tasks.list" => {
                    let tasks = tasks_panel::parse_list_result(&r.result);
                    let _ = evt_tx.send(Evt::TasksListed(tasks));
                }
                "tasks.create" | "tasks.update" | "tasks.complete" => {
                    let _ = evt_tx.send(Evt::Status(format!("{tool} OK")));
                    let list_req = ModuleInvokeRequest {
                        module: "tasks".into(),
                        tool: "tasks.list".into(),
                        args: serde_json::json!({}),
                        actor: "human:ui".into(),
                        actor_caps: vec![
                            "fs.read:/documents/tasks/**".into(),
                            "tool.invoke:tasks".into(),
                        ],
                        trace_id: "ui-tasks-list-after".into(),
                    };
                    if let Ok(lr) = bus
                        .call::<ModuleInvokeRequest, ModuleInvokeResponse>(
                            "module.invoke",
                            &list_req,
                            vec![],
                        )
                        .await
                    {
                        if lr.ok {
                            let tasks = tasks_panel::parse_list_result(&lr.result);
                            let _ = evt_tx.send(Evt::TasksListed(tasks));
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(r) => {
            let _ = evt_tx.send(Evt::Error(
                r.error.unwrap_or_else(|| "tasks: échec".into()),
            ));
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::Error(e.to_string()));
        }
    }
}

pub(crate) async fn agent_id_cmd(bus: &Arc<BusClient>, evt_tx: &Sender<Evt>, intent: &str, id: String) {
    match bus
        .call::<AgentIdRequest, bool>(intent, &AgentIdRequest { agent_id: id }, vec![])
        .await
    {
        Ok(_) => {
            let _ = evt_tx.send(Evt::Status(format!("{intent} ok")));
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::Error(e.to_string()));
        }
    }
}
