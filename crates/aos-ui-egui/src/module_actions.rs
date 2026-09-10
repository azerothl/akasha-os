//! Bus actions for declarative modules and bundled Notes/Tasks modules.

use crate::cmd::Evt;
use crate::decl_media_job::{
    media_engine_is_real, media_job_cancelled, media_job_failed, media_job_queued,
    media_job_running, media_job_succeeded, media_jobs, new_media_job_id,
    parse_media_generate_request, read_image_gen_progress_file,
};
use crate::rich_decl::{demo_job_tick, demo_jobs};
use crate::notes_panel;
use aos_ipc::BusClient;
use aos_proto::decl_ui::ModuleUiResponse;
use aos_proto::{
    AgentIdRequest, MediaGenerateResponse, ModuleIdRequest, ModuleInvokeRequest,
    ModuleInvokeResponse,
};
use serde_json::Value;
use std::path::PathBuf;
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

pub(crate) async fn invoke_module_tool_quiet(
    bus: &Arc<BusClient>,
    module: &str,
    tool: &str,
    args: Value,
) -> Result<Value, String> {
    let req = ModuleInvokeRequest {
        module: module.to_string(),
        tool: tool.to_string(),
        args,
        actor: "human:ui".into(),
        actor_caps: vec![format!("tool.invoke:{module}")],
        trace_id: format!("ui-mod-quiet-{module}-{tool}"),
    };
    match bus
        .call::<ModuleInvokeRequest, ModuleInvokeResponse>("module.invoke", &req, vec![])
        .await
    {
        Ok(r) if r.ok => Ok(r.result),
        Ok(r) => Err(r.error.unwrap_or_else(|| "module tool failed".into())),
        Err(e) => Err(e.to_string()),
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
                let _ = evt_tx.send(Evt::Error(r.error.unwrap_or_else(|| "notes: échec".into())));
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

pub(crate) async fn agent_id_cmd(
    bus: &Arc<BusClient>,
    evt_tx: &Sender<Evt>,
    intent: &str,
    id: String,
) {
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

pub(crate) async fn run_decl_service_action(
    bus: &Arc<BusClient>,
    evt_tx: &Sender<Evt>,
    module: &str,
    action_id: &str,
    service: Option<&str>,
    tool: Option<&str>,
    input: serde_json::Value,
    refresh_binds: Vec<String>,
    subscription_id: Option<String>,
) {
    let demo_jobs = demo_jobs();
    if let Some(tool) = tool {
        invoke_module_tool(bus, evt_tx, module, tool, input).await;
        return;
    }
    let service = service.unwrap_or(action_id);
    match service {
        "jobs.demo.start" => {
            let steps = input.get("steps").and_then(|v| v.as_u64()).unwrap_or(5) as u32;
            let initial = {
                let mut reg = demo_jobs.lock().unwrap();
                reg.start(steps)
            };
            let job_id = initial.job_id.clone().unwrap_or_default();
            let job_id_bg = job_id.clone();
            let sub = subscription_id.clone().unwrap_or_else(|| "demo_job".into());
            let _ = evt_tx.send(Evt::ModuleUiJobUpdate {
                module: module.to_string(),
                subscription_id: sub.clone(),
                job: initial,
            });
            let evt_tx_bg = evt_tx.clone();
            let module_bg = module.to_string();
            tokio::spawn(async move {
                for step in 1..=steps {
                    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                    let cancelled = demo_jobs
                        .lock()
                        .unwrap()
                        .cancel_flag(&job_id_bg)
                        .map(|f| *f.lock().unwrap())
                        .unwrap_or(false);
                    let job = demo_job_tick(&job_id_bg, step, steps, cancelled);
                    let _ = evt_tx_bg.send(Evt::ModuleUiJobUpdate {
                        module: module_bg.clone(),
                        subscription_id: sub.clone(),
                        job: job.clone(),
                    });
                    if cancelled || job.state.as_deref() == Some("succeeded") {
                        break;
                    }
                }
                demo_jobs.lock().unwrap().remove(&job_id_bg);
            });
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: true,
                result: serde_json::json!({"job_id": job_id}),
                error: None,
                refresh_binds,
            });
        }
        "jobs.demo.cancel" | "job.cancel" => {
            let job_id = input
                .get("job_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let ok = demo_jobs.lock().unwrap().cancel(&job_id);
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok,
                result: serde_json::Value::Null,
                error: if ok {
                    None
                } else {
                    Some("job not found".into())
                },
                refresh_binds,
            });
        }
        "media.image.generate" => {
            run_media_image_generate(
                bus,
                evt_tx,
                module,
                action_id,
                input,
                refresh_binds,
                subscription_id,
            )
            .await;
        }
        "media.image.upscale" => {
            let mut req: aos_proto::MediaImageUpscaleRequest = match serde_json::from_value(input) {
                Ok(req) => req,
                Err(e) => {
                    let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                        module: module.to_string(),
                        action_id: action_id.to_string(),
                        ok: false,
                        result: Value::Null,
                        error: Some(format!("invalid media.image.upscale input: {e}")),
                        refresh_binds,
                    });
                    return;
                }
            };
            req.actor = "human:ui".into();
            req.caps = vec!["media.generate".into(), "fs.write:/downloads/**".into()];
            req.trace_id = format!("decl-ui-{module}-upscale");
            let result = bus
                .call::<aos_proto::MediaImageUpscaleRequest, MediaGenerateResponse>(
                    "media.image.upscale",
                    &req,
                    vec![],
                )
                .await;
            let result_value = result
                .as_ref()
                .ok()
                .and_then(|response| serde_json::to_value(response).ok())
                .unwrap_or(Value::Null);
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: result.is_ok(),
                result: result_value,
                error: result.as_ref().err().map(ToString::to_string),
                refresh_binds,
            });
        }
        "media.image.cancel" => {
            let job_id = input
                .get("job_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let ok = media_jobs().lock().unwrap().contains(&job_id);
            if ok {
                if let Err(e) = bus.call::<(), bool>("media.image.cancel", &(), vec![]).await {
                    let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                        module: module.to_string(),
                        action_id: action_id.to_string(),
                        ok: false,
                        result: serde_json::Value::Null,
                        error: Some(e.to_string()),
                        refresh_binds,
                    });
                    return;
                }
            }
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok,
                result: serde_json::Value::Null,
                error: if ok {
                    None
                } else {
                    Some("job not found".into())
                },
                refresh_binds,
            });
        }
        "files.save_as" => {
            let source = input
                .get("source_path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if source.is_empty() {
                let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                    module: module.to_string(),
                    action_id: action_id.to_string(),
                    ok: false,
                    result: serde_json::Value::Null,
                    error: Some("missing source_path".into()),
                    refresh_binds,
                });
                return;
            }
            let host_src = logical_downloads_path(source);
            if !host_src.is_file() {
                let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                    module: module.to_string(),
                    action_id: action_id.to_string(),
                    ok: false,
                    result: serde_json::Value::Null,
                    error: Some("source image not found".into()),
                    refresh_binds,
                });
                return;
            }
            let default_name = host_src
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("image.png");
            let dest = crate::os_open::save_os_file(
                "Save image",
                default_name,
                &[("PNG image", &["png"])],
                crate::os_open::user_downloads_dir().as_deref(),
            );
            let outcome = match dest {
                Some(dest_path) => std::fs::copy(&host_src, &dest_path)
                    .map(|_| serde_json::json!({"path": dest_path.display().to_string()}))
                    .map_err(|e| e.to_string()),
                None => Err("cancelled".into()),
            };
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: outcome.is_ok(),
                result: outcome.clone().unwrap_or(Value::Null),
                error: outcome.err(),
                refresh_binds,
            });
        }
        other => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: serde_json::Value::Null,
                error: Some(format!("unsupported declarative service action: {other}")),
                refresh_binds,
            });
        }
    }
}

async fn run_media_image_generate(
    bus: &Arc<BusClient>,
    evt_tx: &Sender<Evt>,
    module: &str,
    action_id: &str,
    input: Value,
    refresh_binds: Vec<String>,
    subscription_id: Option<String>,
) {
    let request = match parse_media_generate_request(&input) {
        Ok(r) => r,
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(e),
                refresh_binds,
            });
            return;
        }
    };
    let prompt = request.prompt.clone();
    let steps = request.options.steps.unwrap_or(20);
    let job_id = new_media_job_id();
    let sub = subscription_id.clone().unwrap_or_else(|| "generate_job".into());
    media_jobs()
        .lock()
        .unwrap()
        .register(&job_id, &sub, steps);
    let _ = evt_tx.send(Evt::ModuleUiJobUpdate {
        module: module.to_string(),
        subscription_id: sub.clone(),
        job: media_job_queued(&job_id, steps),
    });
    let _ = evt_tx.send(Evt::ModuleUiServiceDone {
        module: module.to_string(),
        action_id: action_id.to_string(),
        ok: true,
        result: serde_json::json!({"job_id": job_id}),
        error: None,
        refresh_binds: Vec::new(),
    });

    let bus_bg = bus.clone();
    let evt_tx_bg = evt_tx.clone();
    let module_bg = module.to_string();
    let action_id_bg = action_id.to_string();
    let refresh = refresh_binds.clone();
    tokio::spawn(async move {
        let _ = evt_tx_bg.send(Evt::ModuleUiJobUpdate {
            module: module_bg.clone(),
            subscription_id: sub.clone(),
            job: media_job_running(&job_id, 0, steps),
        });
        let ticker = tokio::spawn({
            let evt_tx_t = evt_tx_bg.clone();
            let module_t = module_bg.clone();
            let sub_t = sub.clone();
            let job_id_t = job_id.clone();
            async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    let (step, total) = read_image_gen_progress_file().unwrap_or((0, steps));
                    let total = if total > 0 { total } else { steps };
                    let allow = media_jobs()
                        .lock()
                        .unwrap()
                        .allow_progress_emit(&job_id_t);
                    if allow {
                        let _ = evt_tx_t.send(Evt::ModuleUiJobUpdate {
                            module: module_t.clone(),
                            subscription_id: sub_t.clone(),
                            job: media_job_running(&job_id_t, step, total),
                        });
                    }
                }
            }
        });
        let mut req = request;
        req.actor = "human:ui".into();
        req.caps = vec![
            "media.generate".into(),
            "fs.write:/downloads/**".into(),
        ];
        req.trace_id = format!("decl-ui-{module_bg}-{job_id}");
        let result = bus_bg
            .call::<aos_proto::MediaImageGenerateRequest, MediaGenerateResponse>(
                "media.image.generate",
                &req,
                vec![],
            )
            .await;
        ticker.abort();
        let _ = std::fs::remove_file(crate::decl_media_job::image_gen_progress_path());
        media_jobs().lock().unwrap().remove(&job_id);

        match result {
            Ok(response) => {
                if module_bg == "create" && !media_engine_is_real(&response.engine) {
                    let message =
                        "Create requires a real image engine; the Preview stub was rejected";
                    if response.path.starts_with("/downloads/") {
                        let _ = std::fs::remove_file(logical_downloads_path(&response.path));
                    }
                    let _ = evt_tx_bg.send(Evt::ModuleUiJobUpdate {
                        module: module_bg.clone(),
                        subscription_id: sub.clone(),
                        job: media_job_failed(&job_id, message),
                    });
                    let _ = evt_tx_bg.send(Evt::ModuleUiServiceDone {
                        module: module_bg.clone(),
                        action_id: action_id_bg.clone(),
                        ok: false,
                        result: Value::Null,
                        error: Some(message.into()),
                        refresh_binds: Vec::new(),
                    });
                    return;
                }
                let job = media_job_succeeded(&job_id, &response, &prompt);
                let _ = evt_tx_bg.send(Evt::ModuleUiJobUpdate {
                    module: module_bg.clone(),
                    subscription_id: sub.clone(),
                    job: job.clone(),
                });
                if module_bg == "create" {
                    let record_args = serde_json::json!({
                        "path": response.path,
                        "prompt": prompt,
                        "model_id": response.model_id,
                        "engine": response.engine,
                        "width": req.options.width,
                        "height": req.options.height,
                        "steps": req.options.steps,
                    });
                    let _ = invoke_module_tool_quiet(
                        &bus_bg,
                        "create",
                        "create.history.record",
                        record_args,
                    )
                    .await;
                }
                let _ = evt_tx_bg.send(Evt::ModuleUiServiceDone {
                    module: module_bg.clone(),
                    action_id: action_id_bg.clone(),
                    ok: true,
                    result: job.result.clone().unwrap_or(Value::Null),
                    error: None,
                    refresh_binds: refresh,
                });
            }
            Err(e) => {
                let message = e.to_string();
                let cancelled = message.to_ascii_lowercase().contains("annul")
                    || message.to_ascii_lowercase().contains("cancel");
                let (step, total) = read_image_gen_progress_file().unwrap_or((0, steps));
                let job = if cancelled {
                    media_job_cancelled(&job_id, step, total.max(steps))
                } else {
                    media_job_failed(&job_id, &message)
                };
                let _ = evt_tx_bg.send(Evt::ModuleUiJobUpdate {
                    module: module_bg.clone(),
                    subscription_id: sub.clone(),
                    job,
                });
                let _ = evt_tx_bg.send(Evt::ModuleUiServiceDone {
                    module: module_bg.clone(),
                    action_id: action_id_bg.clone(),
                    ok: false,
                    result: Value::Null,
                    error: Some(message),
                    refresh_binds: Vec::new(),
                });
            }
        }
    });
}

fn logical_downloads_path(logical: &str) -> PathBuf {
    let rel = logical.trim_start_matches('/');
    crate::os_open::aos_home()
        .join("var/storage/data")
        .join(rel)
}

pub(crate) async fn cancel_decl_job(
    bus: &Arc<BusClient>,
    evt_tx: &Sender<Evt>,
    module: &str,
    job_id: &str,
    subscription_id: &str,
) {
    let demo_jobs = demo_jobs();
    if demo_jobs.lock().unwrap().cancel(job_id) {
        let _ = evt_tx.send(Evt::ModuleUiJobUpdate {
            module: module.to_string(),
            subscription_id: subscription_id.to_string(),
            job: demo_job_tick(job_id, 0, 1, true),
        });
        return;
    }
    if media_jobs().lock().unwrap().contains(job_id) {
        let _ = bus.call::<(), bool>("media.image.cancel", &(), vec![]).await;
        let (step, total) = read_image_gen_progress_file().unwrap_or((0, 1));
        let _ = evt_tx.send(Evt::ModuleUiJobUpdate {
            module: module.to_string(),
            subscription_id: subscription_id.to_string(),
            job: media_job_cancelled(job_id, step, total.max(1)),
        });
    }
}
