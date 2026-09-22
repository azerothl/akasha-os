//! Bus actions for declarative modules and bundled Notes/Tasks modules.

use crate::cmd::Evt;
use crate::decl_media_job::{
    media_engine_is_real, media_job_cancelled, media_job_failed, media_job_queued,
    media_job_running, media_job_succeeded, media_jobs, new_media_job_id,
    parse_media_generate_request, read_image_gen_progress_file,
};
use crate::notes_panel;
use crate::rich_decl::{demo_job_tick, demo_jobs};
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
                "notes.delete" => {
                    let path = r
                        .result
                        .get("path")
                        .and_then(|p| p.as_str())
                        .unwrap_or("")
                        .to_string();
                    let _ = evt_tx.send(Evt::NotesDeleted { path });
                    let list_req = ModuleInvokeRequest {
                        module: "notes".into(),
                        tool: "notes.list".into(),
                        args: serde_json::json!({}),
                        actor: "human:ui".into(),
                        actor_caps: vec![
                            "fs.read:/documents/notes/**".into(),
                            "tool.invoke:notes".into(),
                        ],
                        trace_id: "ui-notes-list-after-delete".into(),
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
                if let Err(e) = bus
                    .call::<(), bool>("media.image.cancel", &(), vec![])
                    .await
                {
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
        aos_proto::RENDER_STUB_SERVICE => {
            run_render_stub_beauty(bus, evt_tx, module, action_id, input, refresh_binds).await;
        }
        aos_proto::RENDER_SUBMIT_SERVICE => {
            run_render_submit(bus, evt_tx, module, action_id, input, refresh_binds).await;
        }
        aos_proto::RENDER_STATUS_SERVICE => {
            run_render_status(evt_tx, module, action_id, input, refresh_binds);
        }
        aos_proto::RENDER_RESULT_SERVICE => {
            run_render_result(evt_tx, module, action_id, input, refresh_binds);
        }
        aos_proto::ASSET_INSTANTIATE_SERVICE => {
            run_asset_instantiate(evt_tx, module, action_id, input, refresh_binds);
        }
        aos_proto::SCENE_COMPOSE_SERVICE => {
            run_scene_compose(evt_tx, module, action_id, input, refresh_binds);
        }
        aos_proto::SCENE_POSE_SERVICE => {
            run_scene_pose(evt_tx, module, action_id, input, refresh_binds);
        }
        aos_proto::SCENE_GET_SERVICE
        | aos_proto::SCENE_SELECT_SERVICE
        | aos_proto::SCENE_TRS_SERVICE
        | aos_proto::SCENE_APPLY_SERVICE
        | aos_proto::SCENE_LOCK_SERVICE
        | aos_proto::SCENE_UNLOCK_SERVICE
        | aos_proto::SCENE_LOCKS_SERVICE => {
            run_scene_edit_service(
                evt_tx,
                module,
                action_id,
                service,
                input,
                refresh_binds,
            );
        }
        aos_proto::MESH_ASSIST_SERVICE => {
            run_mesh_assist(evt_tx, module, action_id, input, refresh_binds);
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

fn illustration_render_service() -> &'static aos_scene::RenderService {
    use std::sync::OnceLock;
    static SVC: OnceLock<aos_scene::RenderService> = OnceLock::new();
    SVC.get_or_init(aos_scene::RenderService::with_default_backends)
}

async fn run_render_stub_beauty(
    bus: &Arc<BusClient>,
    evt_tx: &Sender<Evt>,
    module: &str,
    action_id: &str,
    input: Value,
    refresh_binds: Vec<String>,
) {
    use aos_proto::FsWriteBytesRequest;
    use base64::Engine as _;

    let path = input
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("/documents/illustrations/beauty-stub.png");
    let r = input.get("r").and_then(|v| v.as_u64()).unwrap_or(48) as u8;
    let g = input.get("g").and_then(|v| v.as_u64()).unwrap_or(72) as u8;
    let b = input.get("b").and_then(|v| v.as_u64()).unwrap_or(96) as u8;
    let svc = illustration_render_service();
    let rendered = match svc.stub_beauty(path, (r, g, b)) {
        Ok(res) => res,
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(format!("render.stub.beauty: {e}")),
                refresh_binds,
            });
            return;
        }
    };
    let req = FsWriteBytesRequest {
        path: rendered.path.clone(),
        content_b64: base64::engine::general_purpose::STANDARD.encode(&rendered.png),
        actor: "human:ui".into(),
        caps: vec![
            aos_proto::ILLUSTRATION_FS_WRITE_CAP.into(),
            aos_proto::RENDER_STUB_CAP.into(),
        ],
        trace_id: format!("decl-ui-{module}-render-stub"),
    };
    let result = bus
        .call::<FsWriteBytesRequest, Value>("fs.write_bytes", &req, vec![])
        .await;
    match result {
        Ok(_) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: true,
                result: serde_json::json!({
                    "path": rendered.path,
                    "kind": "stub_beauty",
                    "backend": "stub",
                    "job_id": rendered.job_id,
                    "width": rendered.width,
                    "height": rendered.height,
                }),
                error: None,
                refresh_binds,
            });
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(format!("render.stub.beauty: {e}")),
                refresh_binds,
            });
        }
    }
}

async fn run_render_submit(
    bus: &Arc<BusClient>,
    evt_tx: &Sender<Evt>,
    module: &str,
    action_id: &str,
    input: Value,
    refresh_binds: Vec<String>,
) {
    use aos_proto::FsWriteBytesRequest;
    use aos_scene::{
        load_project_yaml, parse_backend, parse_pass, parse_style, RenderBackendId, RenderSubmit,
        SceneGraph,
    };
    use base64::Engine as _;

    let path = input
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("/documents/illustrations/beauty.png");
    let backend = match parse_backend(input.get("backend").and_then(|v| v.as_str())) {
        Ok(b) => b,
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(e.to_string()),
                refresh_binds,
            });
            return;
        }
    };
    let pass = match parse_pass(input.get("pass").and_then(|v| v.as_str())) {
        Ok(p) => p,
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(e.to_string()),
                refresh_binds,
            });
            return;
        }
    };
    let width = input.get("width").and_then(|v| v.as_u64()).unwrap_or(256) as u32;
    let height = input.get("height").and_then(|v| v.as_u64()).unwrap_or(256) as u32;
    let r = input.get("r").and_then(|v| v.as_u64()).unwrap_or(48) as u8;
    let g = input.get("g").and_then(|v| v.as_u64()).unwrap_or(72) as u8;
    let b = input.get("b").and_then(|v| v.as_u64()).unwrap_or(96) as u8;
    let style = match parse_style(
        input
            .get("style")
            .or_else(|| input.get("style_id"))
            .and_then(|v| v.as_str()),
    ) {
        Ok(s) => s,
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(e.to_string()),
                refresh_binds,
            });
            return;
        }
    };

    let scene = if let Some(yaml) = input.get("scene_yaml").and_then(|v| v.as_str()) {
        if yaml.trim().is_empty() {
            SceneGraph::demo_scene()
        } else {
            match load_project_yaml(yaml) {
                Ok(p) => p.scene,
                Err(e) => {
                    let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                        module: module.to_string(),
                        action_id: action_id.to_string(),
                        ok: false,
                        result: Value::Null,
                        error: Some(format!("scene_yaml: {e}")),
                        refresh_binds,
                    });
                    return;
                }
            }
        }
    } else {
        SceneGraph::demo_scene()
    };

    let render_cap = match backend {
        RenderBackendId::Stub => aos_proto::RENDER_STUB_CAP,
        RenderBackendId::Cpu => aos_proto::RENDER_CPU_CAP,
        RenderBackendId::Blender => aos_proto::RENDER_BLENDER_CAP,
    };

    let svc = illustration_render_service();
    let rendered = match svc.submit_and_result(RenderSubmit {
        scene,
        backend,
        pass,
        width,
        height,
        output_path: path.to_string(),
        stub_rgb: (r, g, b),
        style,
    }) {
        Ok(res) => res,
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(format!("render.submit: {e}")),
                refresh_binds,
            });
            return;
        }
    };

    let req = FsWriteBytesRequest {
        path: rendered.path.clone(),
        content_b64: base64::engine::general_purpose::STANDARD.encode(&rendered.png),
        actor: "human:ui".into(),
        caps: vec![
            aos_proto::ILLUSTRATION_FS_WRITE_CAP.into(),
            render_cap.into(),
        ],
        trace_id: format!("decl-ui-{module}-render-submit"),
    };
    let result = bus
        .call::<FsWriteBytesRequest, Value>("fs.write_bytes", &req, vec![])
        .await;
    match result {
        Ok(_) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: true,
                result: serde_json::json!({
                    "path": rendered.path,
                    "kind": "render_submit",
                    "backend": rendered.backend.as_str(),
                    "pass": rendered.pass.as_str(),
                    "job_id": rendered.job_id,
                    "width": rendered.width,
                    "height": rendered.height,
                    "status": "succeeded",
                    "style": input
                        .get("style")
                        .or_else(|| input.get("style_id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                }),
                error: None,
                refresh_binds,
            });
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(format!("render.submit write: {e}")),
                refresh_binds,
            });
        }
    }
}

fn run_render_status(
    evt_tx: &Sender<Evt>,
    module: &str,
    action_id: &str,
    input: Value,
    refresh_binds: Vec<String>,
) {
    let Some(job_id) = input.get("job_id").and_then(|v| v.as_str()) else {
        let _ = evt_tx.send(Evt::ModuleUiServiceDone {
            module: module.to_string(),
            action_id: action_id.to_string(),
            ok: false,
            result: Value::Null,
            error: Some("render.status: missing job_id".into()),
            refresh_binds,
        });
        return;
    };
    match illustration_render_service().status(job_id) {
        Ok(st) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: true,
                result: serde_json::json!({
                    "job_id": st.job_id,
                    "status": st.state.as_str(),
                    "backend": st.backend.as_str(),
                    "pass": st.pass.as_str(),
                    "error": st.error,
                }),
                error: None,
                refresh_binds,
            });
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(e.to_string()),
                refresh_binds,
            });
        }
    }
}

fn run_render_result(
    evt_tx: &Sender<Evt>,
    module: &str,
    action_id: &str,
    input: Value,
    refresh_binds: Vec<String>,
) {
    let Some(job_id) = input.get("job_id").and_then(|v| v.as_str()) else {
        let _ = evt_tx.send(Evt::ModuleUiServiceDone {
            module: module.to_string(),
            action_id: action_id.to_string(),
            ok: false,
            result: Value::Null,
            error: Some("render.result: missing job_id".into()),
            refresh_binds,
        });
        return;
    };
    match illustration_render_service().result(job_id) {
        Ok(res) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: true,
                result: serde_json::json!({
                    "job_id": res.job_id,
                    "path": res.path,
                    "width": res.width,
                    "height": res.height,
                    "backend": res.backend.as_str(),
                    "pass": res.pass.as_str(),
                    "status": "succeeded",
                }),
                error: None,
                refresh_binds,
            });
        }
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(e.to_string()),
                refresh_binds,
            });
        }
    }
}

fn run_asset_instantiate(
    evt_tx: &Sender<Evt>,
    module: &str,
    action_id: &str,
    input: Value,
    refresh_binds: Vec<String>,
) {
    use aos_scene::{
        embedded_primitives_pack, instantiate_asset, load_project_yaml, save_project_yaml,
        ProjectFile, SceneGraph,
    };

    let asset_id = input
        .get("asset_id")
        .and_then(|v| v.as_str())
        .unwrap_or("humanoid.placeholder");
    let parent_id = input.get("parent_id").and_then(|v| v.as_str());
    let prefix = input
        .get("prefix")
        .and_then(|v| v.as_str())
        .unwrap_or("inst_");

    let mut scene = if let Some(yaml) = input.get("scene_yaml").and_then(|v| v.as_str()) {
        if yaml.trim().is_empty() {
            SceneGraph::demo_scene()
        } else {
            match load_project_yaml(yaml) {
                Ok(p) => p.scene,
                Err(e) => {
                    let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                        module: module.to_string(),
                        action_id: action_id.to_string(),
                        ok: false,
                        result: Value::Null,
                        error: Some(format!("scene_yaml: {e}")),
                        refresh_binds,
                    });
                    return;
                }
            }
        }
    } else {
        SceneGraph::demo_scene()
    };

    let pack = match embedded_primitives_pack() {
        Ok(p) => p,
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(format!("asset pack: {e}")),
                refresh_binds,
            });
            return;
        }
    };

    let inst = match instantiate_asset(&mut scene, &pack, asset_id, parent_id, prefix) {
        Ok(i) => i,
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(format!("asset.instantiate: {e}")),
                refresh_binds,
            });
            return;
        }
    };

    let yaml = match save_project_yaml(&ProjectFile::new(scene)) {
        Ok(y) => y,
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(format!("save scene: {e}")),
                refresh_binds,
            });
            return;
        }
    };

    let _ = evt_tx.send(Evt::ModuleUiServiceDone {
        module: module.to_string(),
        action_id: action_id.to_string(),
        ok: true,
        result: serde_json::json!({
            "asset_id": asset_id,
            "root_id": inst.root_id,
            "created_ids": inst.created_ids,
            "scene_yaml": yaml,
            "pack_path": "/assets/illustration/primitives/pack.yaml",
        }),
        error: None,
        refresh_binds,
    });
}

fn run_scene_compose(
    evt_tx: &Sender<Evt>,
    module: &str,
    action_id: &str,
    input: Value,
    refresh_binds: Vec<String>,
) {
    use aos_scene::{compose_from_prompt, save_project_yaml, ProjectFile};

    let prompt = input
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if prompt.is_empty() {
        let _ = evt_tx.send(Evt::ModuleUiServiceDone {
            module: module.to_string(),
            action_id: action_id.to_string(),
            ok: false,
            result: Value::Null,
            error: Some("scene.compose: missing prompt".into()),
            refresh_binds,
        });
        return;
    }

    let composed = match compose_from_prompt(prompt) {
        Ok(c) => c,
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(format!("scene.compose: {e}")),
                refresh_binds,
            });
            return;
        }
    };

    let selected = composed
        .character_id
        .clone()
        .unwrap_or_else(|| composed.camera_id.clone());
    let yaml = match save_project_yaml(&ProjectFile::new(composed.scene)) {
        Ok(y) => y,
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(format!("scene.compose save: {e}")),
                refresh_binds,
            });
            return;
        }
    };

    let _ = evt_tx.send(Evt::ModuleUiServiceDone {
        module: module.to_string(),
        action_id: action_id.to_string(),
        ok: true,
        result: serde_json::json!({
            "prompt": prompt,
            "template_id": composed.template_id,
            "placed_assets": composed.placed_assets,
            "character_id": composed.character_id,
            "camera_id": composed.camera_id,
            "root_id": selected,
            "scene_yaml": yaml,
        }),
        error: None,
        refresh_binds,
    });
}

fn run_scene_pose(
    evt_tx: &Sender<Evt>,
    module: &str,
    action_id: &str,
    input: Value,
    refresh_binds: Vec<String>,
) {
    use aos_scene::{
        apply_ik_chain, apply_pose, apply_pose_preset, load_project_yaml, save_project_yaml,
        IkChain, JointId, PoseOp, ProjectFile, SceneGraph, UndoStack, Vec3,
    };
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    fn pose_undo_stacks() -> &'static Mutex<HashMap<String, UndoStack>> {
        static STACKS: OnceLock<Mutex<HashMap<String, UndoStack>>> = OnceLock::new();
        STACKS.get_or_init(|| Mutex::new(HashMap::new()))
    }

    let mut scene = if let Some(yaml) = input.get("scene_yaml").and_then(|v| v.as_str()) {
        if yaml.trim().is_empty() {
            SceneGraph::demo_scene()
        } else {
            match load_project_yaml(yaml) {
                Ok(p) => p.scene,
                Err(e) => {
                    let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                        module: module.to_string(),
                        action_id: action_id.to_string(),
                        ok: false,
                        result: Value::Null,
                        error: Some(format!("scene_yaml: {e}")),
                        refresh_binds,
                    });
                    return;
                }
            }
        }
    } else {
        SceneGraph::demo_scene()
    };

    let character_root = input
        .get("humanoid_root")
        .and_then(|v| v.as_str())
        .or_else(|| input.get("character_root").and_then(|v| v.as_str()))
        .or_else(|| input.get("root_id").and_then(|v| v.as_str()))
        .unwrap_or("humanoid");

    let want_undo = input
        .get("undo")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if want_undo {
        let undone = {
            let mut map = pose_undo_stacks().lock().unwrap_or_else(|e| e.into_inner());
            let stack = map.entry(module.to_string()).or_default();
            stack.undo(&mut scene)
        };
        match undone {
            Ok(true) => {}
            Ok(false) => {
                let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                    module: module.to_string(),
                    action_id: action_id.to_string(),
                    ok: false,
                    result: Value::Null,
                    error: Some("scene.pose: nothing to undo".into()),
                    refresh_binds,
                });
                return;
            }
            Err(e) => {
                let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                    module: module.to_string(),
                    action_id: action_id.to_string(),
                    ok: false,
                    result: Value::Null,
                    error: Some(format!("scene.pose undo: {e}")),
                    refresh_binds,
                });
                return;
            }
        }
        let yaml = match save_project_yaml(&ProjectFile::new(scene)) {
            Ok(y) => y,
            Err(e) => {
                let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                    module: module.to_string(),
                    action_id: action_id.to_string(),
                    ok: false,
                    result: Value::Null,
                    error: Some(format!("scene.pose save: {e}")),
                    refresh_binds,
                });
                return;
            }
        };
        let _ = evt_tx.send(Evt::ModuleUiServiceDone {
            module: module.to_string(),
            action_id: action_id.to_string(),
            ok: true,
            result: serde_json::json!({
                "humanoid_root": character_root,
                "character_root": character_root,
                "undone": true,
                "scene_yaml": yaml,
                "root_id": character_root,
            }),
            error: None,
            refresh_binds,
        });
        return;
    }

    let mut stacks = pose_undo_stacks()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let undo_stack = stacks.entry(module.to_string()).or_default();

    let preset = input.get("preset").and_then(|v| v.as_str());
    let look_at = input.get("look_at");
    let ik = input.get("ik").or_else(|| input.get("solve_ik"));

    let applied = if let Some(name) = preset {
        apply_pose_preset(&mut scene, character_root, name, Some(undo_stack))
    } else if let Some(target) = look_at {
        let tx = target.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let ty = target.get("y").and_then(|v| v.as_f64()).unwrap_or(1.5) as f32;
        let tz = target.get("z").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        apply_pose(
            &mut scene,
            &PoseOp::LookAt {
                character_root: character_root.into(),
                target_world: Vec3::new(tx, ty, tz),
            },
            Some(undo_stack),
        )
    } else if let Some(ik_val) = ik {
        let chain_name = ik_val
            .get("chain")
            .and_then(|v| v.as_str())
            .or_else(|| input.get("chain").and_then(|v| v.as_str()))
            .unwrap_or("arm_r");
        if IkChain::parse(chain_name).is_none() {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(format!("scene.pose: unknown IK chain `{chain_name}`")),
                refresh_binds,
            });
            return;
        }
        let target = ik_val.get("target").or(Some(ik_val));
        let tx = target
            .and_then(|t| t.get("x"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5) as f32;
        let ty = target
            .and_then(|t| t.get("y"))
            .and_then(|v| v.as_f64())
            .unwrap_or(1.2) as f32;
        let tz = target
            .and_then(|t| t.get("z"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.2) as f32;
        let pole = ik_val.get("pole").map(|p| {
            Vec3::new(
                p.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                p.get("y").and_then(|v| v.as_f64()).unwrap_or(1.5) as f32,
                p.get("z").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
            )
        });
        apply_ik_chain(
            &mut scene,
            character_root,
            chain_name,
            Vec3::new(tx, ty, tz),
            pole,
            Some(undo_stack),
        )
    } else if let Some(joint) = input.get("joint").and_then(|v| v.as_str()) {
        let Some(joint_id) = JointId::parse(joint) else {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(format!("scene.pose: unknown joint `{joint}`")),
                refresh_binds,
            });
            return;
        };
        let axis = input.get("axis").and_then(|v| v.as_array());
        let ax = axis
            .and_then(|a| a.first())
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32;
        let ay = axis
            .and_then(|a| a.get(1))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32;
        let az = axis
            .and_then(|a| a.get(2))
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0) as f32;
        let angle = input
            .get("angle_rad")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5) as f32;
        apply_pose(
            &mut scene,
            &PoseOp::RotateJoint {
                character_root: character_root.into(),
                joint: joint_id,
                axis: Vec3::new(ax, ay, az),
                angle_rad: angle,
            },
            Some(undo_stack),
        )
    } else {
        // Default DeclUI affordance: wave right.
        apply_pose_preset(
            &mut scene,
            character_root,
            "wave_right",
            Some(undo_stack),
        )
    };

    let ops = match applied {
        Ok(o) => o,
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(format!("scene.pose: {e}")),
                refresh_binds,
            });
            return;
        }
    };
    drop(stacks);

    let yaml = match save_project_yaml(&ProjectFile::new(scene)) {
        Ok(y) => y,
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(format!("scene.pose save: {e}")),
                refresh_binds,
            });
            return;
        }
    };

    let _ = evt_tx.send(Evt::ModuleUiServiceDone {
        module: module.to_string(),
        action_id: action_id.to_string(),
        ok: true,
        result: serde_json::json!({
            "humanoid_root": character_root,
            "character_root": character_root,
            "preset": preset,
            "ops_count": ops.len(),
            "scene_yaml": yaml,
            "root_id": character_root,
        }),
        error: None,
        refresh_binds,
    });
}

fn run_scene_edit_service(
    evt_tx: &Sender<Evt>,
    module: &str,
    action_id: &str,
    service: &str,
    input: Value,
    refresh_binds: Vec<String>,
) {
    use aos_scene::{
        apply_batch, apply_one, AgentEditOp, EditActorKind, EditSnapshot, LockKind, LockScope,
    };

    let yaml = input
        .get("scene_yaml")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let mut snap = match EditSnapshot::from_yaml(yaml) {
        Ok(s) => s,
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(format!("{service}: {e}")),
                refresh_binds,
            });
            return;
        }
    };

    // DeclUI is always human — locks do not block the user.
    let actor = "human:ui";
    let kind = EditActorKind::Human;

    let outcome = match service {
        aos_proto::SCENE_GET_SERVICE | aos_proto::SCENE_LOCKS_SERVICE => Ok(()),
        aos_proto::SCENE_SELECT_SERVICE => {
            let id = input
                .get("id")
                .or_else(|| input.get("selected_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            apply_one(
                &mut snap,
                &AgentEditOp::Select { id: id.into() },
                actor,
                kind,
            )
        }
        aos_proto::SCENE_TRS_SERVICE => {
            let id = input
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let translation = match input.get("translation").cloned() {
                Some(v) => match serde_json::from_value(v) {
                    Ok(t) => Some(t),
                    Err(e) => {
                        let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                            module: module.to_string(),
                            action_id: action_id.to_string(),
                            ok: false,
                            result: Value::Null,
                            error: Some(format!("translation: {e}")),
                            refresh_binds,
                        });
                        return;
                    }
                },
                None => None,
            };
            let rotation = match input.get("rotation").cloned() {
                Some(v) => match serde_json::from_value(v) {
                    Ok(t) => Some(t),
                    Err(e) => {
                        let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                            module: module.to_string(),
                            action_id: action_id.to_string(),
                            ok: false,
                            result: Value::Null,
                            error: Some(format!("rotation: {e}")),
                            refresh_binds,
                        });
                        return;
                    }
                },
                None => None,
            };
            let scale = match input.get("scale").cloned() {
                Some(v) => match serde_json::from_value(v) {
                    Ok(t) => Some(t),
                    Err(e) => {
                        let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                            module: module.to_string(),
                            action_id: action_id.to_string(),
                            ok: false,
                            result: Value::Null,
                            error: Some(format!("scale: {e}")),
                            refresh_binds,
                        });
                        return;
                    }
                },
                None => None,
            };
            apply_one(
                &mut snap,
                &AgentEditOp::Trs {
                    id,
                    translation,
                    rotation,
                    scale,
                },
                actor,
                kind,
            )
        }
        aos_proto::SCENE_APPLY_SERVICE => {
            let ops: Result<Vec<AgentEditOp>, _> = input
                .get("ops")
                .cloned()
                .ok_or_else(|| "ops required".to_string())
                .and_then(|v| serde_json::from_value(v).map_err(|e| e.to_string()));
            match ops {
                Ok(ops) => apply_batch(&mut snap, &ops, actor, kind),
                Err(e) => {
                    let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                        module: module.to_string(),
                        action_id: action_id.to_string(),
                        ok: false,
                        result: Value::Null,
                        error: Some(format!("scene.apply: {e}")),
                        refresh_binds,
                    });
                    return;
                }
            }
        }
        aos_proto::SCENE_LOCK_SERVICE => {
            let id = input
                .get("id")
                .or_else(|| input.get("node_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let scope = match input.get("scope").and_then(|v| v.as_str()).unwrap_or("node") {
                "subtree" => LockScope::Subtree,
                _ => LockScope::Node,
            };
            let kind_lock = if input.get("pose").and_then(|v| v.as_bool()).unwrap_or(false)
                || input.get("kind").and_then(|v| v.as_str()) == Some("pose")
            {
                LockKind::Pose
            } else {
                LockKind::Semantic
            };
            apply_one(
                &mut snap,
                &AgentEditOp::Lock {
                    id,
                    scope,
                    kind: kind_lock,
                },
                actor,
                kind,
            )
        }
        aos_proto::SCENE_UNLOCK_SERVICE => {
            let id = input
                .get("id")
                .or_else(|| input.get("node_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            apply_one(&mut snap, &AgentEditOp::Unlock { id }, actor, kind)
        }
        other => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(format!("unsupported scene edit service: {other}")),
                refresh_binds,
            });
            return;
        }
    };

    match outcome {
        Ok(()) => match snap.result_json() {
            Ok(result) => {
                let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                    module: module.to_string(),
                    action_id: action_id.to_string(),
                    ok: true,
                    result,
                    error: None,
                    refresh_binds,
                });
            }
            Err(e) => {
                let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                    module: module.to_string(),
                    action_id: action_id.to_string(),
                    ok: false,
                    result: Value::Null,
                    error: Some(format!("{service}: {e}")),
                    refresh_binds,
                });
            }
        },
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(format!("{service}: {e}")),
                refresh_binds,
            });
        }
    }
}

fn run_mesh_assist(
    evt_tx: &Sender<Evt>,
    module: &str,
    action_id: &str,
    input: Value,
    refresh_binds: Vec<String>,
) {
    use aos_scene::{
        load_project_yaml, mesh_assist, save_project_yaml, MeshAssistBackendId, MeshAssistRequest,
        ProjectFile, SceneGraph,
    };

    let prompt = input
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if prompt.is_empty() {
        let _ = evt_tx.send(Evt::ModuleUiServiceDone {
            module: module.to_string(),
            action_id: action_id.to_string(),
            ok: false,
            result: Value::Null,
            error: Some("mesh.assist: missing prompt".into()),
            refresh_binds,
        });
        return;
    }

    let backend_raw = input
        .get("backend")
        .and_then(|v| v.as_str())
        .unwrap_or("stub");
    let Some(backend) = MeshAssistBackendId::parse(backend_raw) else {
        let _ = evt_tx.send(Evt::ModuleUiServiceDone {
            module: module.to_string(),
            action_id: action_id.to_string(),
            ok: false,
            result: Value::Null,
            error: Some(format!("mesh.assist: unknown backend `{backend_raw}`")),
            refresh_binds,
        });
        return;
    };

    let mut scene = if let Some(yaml) = input.get("scene_yaml").and_then(|v| v.as_str()) {
        if yaml.trim().is_empty() {
            SceneGraph::demo_scene()
        } else {
            match load_project_yaml(yaml) {
                Ok(p) => p.scene,
                Err(e) => {
                    let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                        module: module.to_string(),
                        action_id: action_id.to_string(),
                        ok: false,
                        result: Value::Null,
                        error: Some(format!("scene_yaml: {e}")),
                        refresh_binds,
                    });
                    return;
                }
            }
        }
    } else {
        SceneGraph::demo_scene()
    };

    let parent_id = input
        .get("parent_id")
        .and_then(|v| v.as_str())
        .unwrap_or("root");
    let prefix = input
        .get("prefix")
        .and_then(|v| v.as_str())
        .unwrap_or("mesh_");

    let req = MeshAssistRequest {
        prompt: prompt.to_string(),
        parent_id: parent_id.to_string(),
        prefix: prefix.to_string(),
        backend,
    };

    let applied = match mesh_assist(&mut scene, &req) {
        Ok(r) => r,
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(format!("mesh.assist: {e}")),
                refresh_binds,
            });
            return;
        }
    };

    let yaml = match save_project_yaml(&ProjectFile::new(scene)) {
        Ok(y) => y,
        Err(e) => {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some(format!("mesh.assist save: {e}")),
                refresh_binds,
            });
            return;
        }
    };

    let _ = evt_tx.send(Evt::ModuleUiServiceDone {
        module: module.to_string(),
        action_id: action_id.to_string(),
        ok: true,
        result: serde_json::json!({
            "prompt": prompt,
            "backend": applied.backend.as_str(),
            "is_stub": applied.is_stub,
            "kind_id": applied.kind_id,
            "created_ids": applied.created_ids,
            "notes": applied.notes,
            "root_id": applied.root_id,
            "scene_yaml": yaml,
        }),
        error: None,
        refresh_binds,
    });
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
    if module == "create" {
        if request.prompt.trim().is_empty() {
            let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                module: module.to_string(),
                action_id: action_id.to_string(),
                ok: false,
                result: Value::Null,
                error: Some("Prompt vide : décrivez ce que vous voulez créer.".into()),
                refresh_binds: refresh_binds.clone(),
            });
            return;
        }
        if let Some(model_id) = request.model_id.as_deref().filter(|id| !id.is_empty()) {
            match crate::models_page::model_install_state(model_id) {
                crate::models_page::ModelInstallState::Complete => {}
                crate::models_page::ModelInstallState::Missing => {
                    let message = format!(
                        "Le modèle {model_id} n'est pas installé. Téléchargez-le depuis Modèles avant de générer."
                    );
                    let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                        module: module.to_string(),
                        action_id: action_id.to_string(),
                        ok: false,
                        result: Value::Null,
                        error: Some(message),
                        refresh_binds: refresh_binds.clone(),
                    });
                    return;
                }
                crate::models_page::ModelInstallState::Incomplete => {
                    let missing = crate::models_page::missing_model_annexes(model_id);
                    let message = format!(
                        "modèle incomplet : fichiers auxiliaires manquants ({})",
                        missing.join(", ")
                    );
                    let _ = evt_tx.send(Evt::ModuleUiServiceDone {
                        module: module.to_string(),
                        action_id: action_id.to_string(),
                        ok: false,
                        result: Value::Null,
                        error: Some(message),
                        refresh_binds: refresh_binds.clone(),
                    });
                    return;
                }
            }
        }
    }
    let prompt = request.prompt.clone();
    let steps = request.options.steps.unwrap_or(20);
    let job_id = new_media_job_id();
    let sub = subscription_id
        .clone()
        .unwrap_or_else(|| "generate_job".into());
    media_jobs().lock().unwrap().register(&job_id, &sub, steps);
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
                    let allow = media_jobs().lock().unwrap().allow_progress_emit(&job_id_t);
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
        // Keep the user's text for history, but honor the same prompt
        // assistant and composition pipeline that the former native Create
        // panel used before it became a module.
        let original_prompt = req.prompt.clone();
        apply_create_presets(&mut req);
        normalize_create_options(&mut req.options);
        if req.enhance_prompt_chat
            && !req.composition_blocks.is_empty()
            && enhance_create_layer_prompts(&bus_bg, &evt_tx_bg, &mut req.composition_blocks).await
        {
            let _ = evt_tx_bg.send(Evt::ModuleUiLayersGenerated {
                module: module_bg.clone(),
                layers: req.composition_blocks.clone(),
            });
        }
        let edited_prompt = req
            .generation_prompt
            .clone()
            .filter(|text| req.use_edited_enriched && !text.trim().is_empty());
        let mut generation_prompt = edited_prompt.clone();
        let json_enrichment_supported =
            crate::image_prompt::supports_json_prompt_enrichment(req.model_id.as_deref());
        if req.enrich_prompt && !json_enrichment_supported {
            let _ = evt_tx_bg.send(Evt::Status(
                "Structured prompt is unavailable for this model; using the original prompt".into(),
            ));
            req.enrich_prompt = false;
        }
        if edited_prompt.is_none() && (req.enrich_prompt || req.enhance_prompt_chat) {
            // Both assistants may be enabled. Run chat first and the
            // structured pass second so Ideogram receives JSON rather than
            // the intermediate chat prose. An edited prompt is authoritative
            // and deliberately bypasses this block.
            let mut assistant_source = original_prompt.clone();
            if req.enhance_prompt_chat {
                match crate::runtime::enrich_prompt_for_module(
                    &bus_bg,
                    &evt_tx_bg,
                    &assistant_source,
                    req.model_id.as_deref(),
                    true,
                )
                .await
                {
                    Ok(text) if !text.trim().is_empty() => {
                        assistant_source = text.clone();
                        generation_prompt = Some(text);
                    }
                    Ok(_) => {}
                    Err(err) => {
                        let _ = evt_tx_bg.send(Evt::Status(format!(
                            "Prompt improvement unavailable; continuing with the original prompt ({err})"
                        )));
                    }
                }
            }
            if req.enrich_prompt && json_enrichment_supported {
                match crate::runtime::enrich_prompt_for_module(
                    &bus_bg,
                    &evt_tx_bg,
                    &assistant_source,
                    req.model_id.as_deref(),
                    false,
                )
                .await
                {
                    Ok(text) if !text.trim().is_empty() => generation_prompt = Some(text),
                    Ok(_) => {}
                    Err(err) => {
                        let _ = evt_tx_bg.send(Evt::Status(format!(
                            "Structured prompt unavailable; building a local Ideogram caption ({err})"
                        )));
                        // A transient LLM failure must never downgrade an
                        // Ideogram request back to chat prose. Build the
                        // documented caption locally so the model still gets
                        // `compositional_deconstruction` and the layer order.
                        if matches!(
                            req.model_id
                                .as_deref()
                                .and_then(crate::image_prompt::prompt_enrichment_kind),
                            Some(crate::image_prompt::PromptEnrichmentKind::Ideogram4)
                        ) {
                            let fallback = parse_composition_blocks(&req.composition_blocks)
                                .filter(|blocks| !blocks.is_empty())
                                .map(|blocks| {
                                    crate::image_composition::compose_prompt_with_layout(
                                        &assistant_source,
                                        &blocks,
                                        req.model_id.as_deref(),
                                    )
                                })
                                .unwrap_or_else(|| {
                                    let raw = serde_json::json!({
                                        "high_level_description": assistant_source
                                    });
                                    crate::image_prompt::normalize_ideogram_caption(
                                        &raw.to_string(),
                                        &original_prompt,
                                    )
                                    .unwrap_or_else(|_| original_prompt.clone())
                                });
                            generation_prompt = Some(fallback);
                        }
                    }
                }
            }
        }
        if !req.use_edited_enriched {
            if let Some(blocks) = parse_composition_blocks(&req.composition_blocks) {
                if !blocks.is_empty() {
                    let base = generation_prompt.as_deref().unwrap_or(&original_prompt);
                    let merged = crate::image_composition::finalize_prompt_with_layout(
                        base,
                        &blocks,
                        req.model_id.as_deref(),
                        generation_prompt.is_some(),
                    );
                    if merged != base {
                        generation_prompt = Some(merged);
                    }
                }
            }
        }
        if let Some(generated) = generation_prompt.as_deref() {
            let _ = evt_tx_bg.send(Evt::ModuleUiPromptGenerated {
                module: module_bg.clone(),
                prompt: generated.to_string(),
            });
        }
        if let Some(ref generated) = generation_prompt {
            req.prompt = generated.clone();
            req.generation_prompt = Some(generated.clone());
        }
        req.actor = "human:ui".into();
        req.caps = vec!["media.generate".into(), "fs.write:/downloads/**".into()];
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
                let job = media_job_succeeded(
                    &job_id,
                    &response,
                    &prompt,
                    req.generation_prompt.as_deref(),
                );
                let _ = evt_tx_bg.send(Evt::ModuleUiJobUpdate {
                    module: module_bg.clone(),
                    subscription_id: sub.clone(),
                    job: job.clone(),
                });
                if module_bg == "create" {
                    let metadata = crate::image_history::ImageGenMeta::new(
                        response.path.clone(),
                        prompt.clone(),
                        req.generation_prompt.clone(),
                        parse_composition_blocks(&req.composition_blocks).unwrap_or_default(),
                        response.model_id.clone(),
                        response.engine.clone(),
                    );
                    if let Err(error) = crate::image_history::write_image_meta(&metadata) {
                        eprintln!("Create image metadata write failed: {error}");
                    }
                    let media_mode = if crate::media_image_defaults::is_video_options(&req.options)
                    {
                        "video"
                    } else {
                        "image"
                    };
                    let record_args = serde_json::json!({
                        "path": response.path,
                        "prompt": prompt,
                        "model_id": response.model_id,
                        "engine": response.engine,
                        "media_mode": media_mode,
                        "width": req.options.width,
                        "height": req.options.height,
                        "steps": req.options.steps,
                        "params": {
                            "media_mode": media_mode,
                            "model_id": req.model_id,
                            "format": req.format_preset,
                            "intent": req.intent_preset,
                            "profile": req.quality_profile,
                            "camera_preset": req.camera_preset,
                            "negative_prompt": req.options.negative_prompt,
                            "width": req.options.width,
                            "height": req.options.height,
                            "steps": req.options.steps,
                            "cfg_scale": req.options.cfg_scale,
                            "seed": req.options.seed,
                            "sampling_method": req.options.sampling_method,
                            "flow_shift": req.options.flow_shift,
                            "video_frames": req.options.video_frames,
                            "fps": req.options.fps,
                            "styles": req.options.styles,
                            "loras": req.options.loras,
                            "lora_scale": req.options.lora_scale,
                            "vae": req.options.vae,
                            "init_image": req.options.init_image,
                            "end_image": req.options.end_image,
                            "strength": req.options.strength,
                            "mask_image": req.options.mask_image,
                            "upscale_model": req.options.upscale_model,
                            "upscale_repeats": req.options.upscale_repeats,
                            "upscale_tile_size": req.options.upscale_tile_size,
                            "enrich_prompt": req.enrich_prompt,
                            "enhance_prompt_chat": req.enhance_prompt_chat,
                            "use_edited_enriched": req.use_edited_enriched,
                            "enriched_prompt": req.generation_prompt,
                            "composition_layers": req.composition_blocks,
                        },
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

fn parse_composition_blocks(
    values: &[Value],
) -> Option<Vec<crate::image_composition::CompositionBlock>> {
    if values.is_empty() {
        return Some(Vec::new());
    }
    let mut blocks = Vec::with_capacity(values.len());
    for value in values {
        let object = value.as_object()?;
        let id = object.get("id").and_then(Value::as_u64)?;
        let x = object.get("x").and_then(Value::as_f64).unwrap_or(0.0) as f32;
        let y = object.get("y").and_then(Value::as_f64).unwrap_or(0.0) as f32;
        let w = object.get("w").and_then(Value::as_f64).unwrap_or(0.3) as f32;
        let h = object.get("h").and_then(Value::as_f64).unwrap_or(0.3) as f32;
        // Rich declarative layers use `prompt` (and `label` for display),
        // while the legacy native generator called the same field `desc`.
        let desc = object
            .get("prompt")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .or_else(|| object.get("label").and_then(Value::as_str))
            .or_else(|| object.get("desc").and_then(Value::as_str))
            .unwrap_or("")
            .to_string();
        blocks.push(crate::image_composition::CompositionBlock {
            id,
            x,
            y,
            w,
            h,
            desc,
        });
    }
    Some(blocks)
}

/// Rewrite each non-empty layer description with the same chat assistant used
/// for the global prompt. Layer geometry/order stays untouched; only the
/// editable `prompt` field is replaced and published back to the UI.
async fn enhance_create_layer_prompts(
    bus: &BusClient,
    evt_tx: &Sender<Evt>,
    layers: &mut [Value],
) -> bool {
    let mut changed = false;
    for layer in layers.iter_mut() {
        let source = layer.as_object().and_then(|object| {
            object
                .get("prompt")
                .and_then(Value::as_str)
                .filter(|text| !text.trim().is_empty())
                .or_else(|| {
                    object
                        .get("label")
                        .and_then(Value::as_str)
                        .filter(|text| !text.trim().is_empty())
                })
                .or_else(|| {
                    object
                        .get("desc")
                        .and_then(Value::as_str)
                        .filter(|text| !text.trim().is_empty())
                })
                .map(str::to_owned)
        });
        let Some(source) = source else { continue };
        match crate::runtime::enrich_layer_prompt_for_module(bus, evt_tx, &source).await {
            Ok(enriched) if !enriched.trim().is_empty() && enriched.trim() != source.trim() => {
                if let Some(object) = layer.as_object_mut() {
                    object.insert("prompt".into(), Value::String(enriched));
                }
                changed = true;
            }
            Ok(_) => {}
            Err(error) => {
                let _ = evt_tx.send(Evt::Status(format!(
                    "Amélioration du calque indisponible; description conservée ({error})"
                )));
            }
        }
    }
    changed
}

fn apply_create_presets(req: &mut aos_proto::MediaImageGenerateRequest) {
    let options = &mut req.options;
    // Native Create applies an aspect ratio to the current model dimensions;
    // it does not replace them with a fixed 512/768 recipe. The declarative
    // host synchronizes model/profile defaults, while this final guard keeps
    // replayed actions faithful when a format is selected programmatically.
    let base = options
        .width
        .unwrap_or(512)
        .max(options.height.unwrap_or(512))
        .clamp(256, 2048);
    match req.format_preset.as_deref() {
        Some("16:9") => {
            options.width = Some(base);
            options.height = Some((base * 9 / 16).max(64));
        }
        Some("9:16") => {
            options.width = Some((base * 9 / 16).max(64));
            options.height = Some(base);
        }
        Some("1:1") => {
            options.width = Some(base);
            options.height = Some(base);
        }
        _ => {}
    }
    let camera = match req.camera_preset.as_deref() {
        Some("static") => None,
        Some("push") => Some("Camera movement: slow push-in."),
        Some("track") => Some("Camera movement: smooth lateral tracking shot."),
        Some("follow") => Some("Camera movement: gentle follow shot."),
        _ => None,
    };
    if let Some(camera) = camera {
        if !req.prompt.contains(camera) {
            req.prompt = format!("{}\n{}", req.prompt.trim(), camera);
        }
    }
    let intent = match req.intent_preset.as_deref() {
        Some("portrait") => Some("Intent: portrait framing and subject emphasis."),
        Some("product") => Some("Intent: clean product presentation with controlled lighting."),
        Some("illustration") => {
            Some("Intent: illustrated composition with deliberate graphic shapes.")
        }
        Some("cinematic") => Some("Intent: cinematic composition and dramatic light."),
        _ => None,
    };
    if let Some(intent) = intent {
        if !req.prompt.contains(intent) {
            req.prompt = format!("{}\n{}", req.prompt.trim(), intent);
        }
    }
}

/// Declarative text fields use an empty string for an unset optional value.
/// The native panel serializes those values as `None`; normalize before
/// handing the request to modeld (an empty VAE path is otherwise interpreted
/// as an explicit, invalid file).
fn normalize_create_options(options: &mut aos_proto::MediaImageOptions) {
    for value in [
        &mut options.sampling_method,
        &mut options.vae,
        &mut options.backend,
        &mut options.params_backend,
        &mut options.max_vram,
        &mut options.upscale_model,
        &mut options.init_image,
        &mut options.end_image,
        &mut options.mask_image,
    ] {
        if value.as_deref().is_some_and(|text| text.trim().is_empty()) {
            *value = None;
        }
    }
    if options.threads == Some(0) {
        options.threads = None;
    }
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
        let _ = bus
            .call::<(), bool>("media.image.cancel", &(), vec![])
            .await;
        let (step, total) = read_image_gen_progress_file().unwrap_or((0, 1));
        let _ = evt_tx.send(Evt::ModuleUiJobUpdate {
            module: module.to_string(),
            subscription_id: subscription_id.to_string(),
            job: media_job_cancelled(job_id, step, total.max(1)),
        });
    }
}

#[cfg(test)]
mod create_regression_tests {
    use super::{apply_create_presets, normalize_create_options, parse_composition_blocks};
    use aos_proto::MediaImageGenerateRequest;

    #[test]
    fn native_create_presets_keep_dimensions_and_steps() {
        for (format, expected) in [
            ("1:1", (512, 512)),
            ("16:9", (512, 288)),
            ("9:16", (288, 512)),
        ] {
            let mut req = MediaImageGenerateRequest {
                prompt: "subject".into(),
                path: None,
                model_id: None,
                options: aos_proto::MediaImageOptions {
                    width: Some(512),
                    height: Some(512),
                    steps: Some(24),
                    ..Default::default()
                },
                generation_prompt: None,
                enrich_prompt: false,
                enhance_prompt_chat: false,
                use_edited_enriched: false,
                composition_blocks: Vec::new(),
                format_preset: Some(format.into()),
                intent_preset: None,
                quality_profile: Some("balanced".into()),
                camera_preset: None,
                actor: String::new(),
                caps: Vec::new(),
                trace_id: String::new(),
                session_id: None,
            };
            apply_create_presets(&mut req);
            assert_eq!(
                (req.options.width, req.options.height),
                (Some(expected.0), Some(expected.1))
            );
            assert_eq!(req.options.steps, Some(24));
        }
    }

    #[test]
    fn declarative_layer_prompt_maps_to_native_composition_description() {
        let blocks = parse_composition_blocks(&[serde_json::json!({
            "id": 7,
            "x": 0.1,
            "y": 0.2,
            "w": 0.4,
            "h": 0.5,
            "prompt": "a red fox",
            "label": "Fox",
        })])
        .expect("valid layer");
        assert_eq!(blocks[0].id, 7);
        assert_eq!(blocks[0].desc, "a red fox");
        assert!((blocks[0].w - 0.4).abs() < f32::EPSILON);
    }

    #[test]
    fn declarative_empty_optional_strings_match_native_none_values() {
        let mut options = aos_proto::MediaImageOptions {
            sampling_method: Some(String::new()),
            vae: Some(String::new()),
            backend: Some("  ".into()),
            params_backend: Some(String::new()),
            max_vram: Some(String::new()),
            init_image: Some(String::new()),
            end_image: Some(String::new()),
            mask_image: Some(String::new()),
            threads: Some(0),
            ..Default::default()
        };
        normalize_create_options(&mut options);
        assert!(options.sampling_method.is_none());
        assert!(options.vae.is_none());
        assert!(options.backend.is_none());
        assert!(options.params_backend.is_none());
        assert!(options.max_vram.is_none());
        assert!(options.init_image.is_none());
        assert!(options.end_image.is_none());
        assert!(options.mask_image.is_none());
        assert!(options.threads.is_none());
    }
}
