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
            if !crate::models_page::is_model_installed(model_id) {
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
        // Match the native panel's precedence rule even for replayed or
        // programmatic actions that bypass the checkbox interaction.
        if req.enhance_prompt_chat {
            req.enrich_prompt = false;
        }
        // Keep the user's text for history, but honor the same prompt
        // assistant and composition pipeline that the former native Create
        // panel used before it became a module.
        let original_prompt = req.prompt.clone();
        apply_create_presets(&mut req);
        let mut generation_prompt = req
            .generation_prompt
            .clone()
            .filter(|text| req.use_edited_enriched && !text.trim().is_empty());
        let json_enrichment_supported =
            crate::image_prompt::supports_json_prompt_enrichment(req.model_id.as_deref());
        if req.enrich_prompt && !json_enrichment_supported {
            let _ = evt_tx_bg.send(Evt::Status(
                "Structured prompt is unavailable for this model; using the original prompt".into(),
            ));
            req.enrich_prompt = false;
        }
        if generation_prompt.is_none() && (req.enrich_prompt || req.enhance_prompt_chat) {
            match crate::runtime::enrich_prompt_for_module(
                &bus_bg,
                &evt_tx_bg,
                &original_prompt,
                req.model_id.as_deref(),
                req.enhance_prompt_chat,
            )
            .await
            {
                Ok(text) if !text.trim().is_empty() => generation_prompt = Some(text),
                Ok(_) => {}
                Err(err) => {
                    let _ = evt_tx_bg.send(Evt::Status(format!(
                        "Prompt assistant unavailable; using the original prompt ({err})"
                    )));
                }
            }
        }
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
                let job = media_job_succeeded(&job_id, &response, &prompt);
                let _ = evt_tx_bg.send(Evt::ModuleUiJobUpdate {
                    module: module_bg.clone(),
                    subscription_id: sub.clone(),
                    job: job.clone(),
                });
                if module_bg == "create" {
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

fn apply_create_presets(req: &mut aos_proto::MediaImageGenerateRequest) {
    let options = &mut req.options;
    match req.format_preset.as_deref() {
        Some("16:9") => {
            options.width = Some(768);
            options.height = Some(432);
        }
        Some("9:16") => {
            options.width = Some(432);
            options.height = Some(768);
        }
        Some("1:1") => {
            options.width = Some(512);
            options.height = Some(512);
        }
        _ => {}
    }
    match req.quality_profile.as_deref() {
        Some("fast") => options.steps = Some(12),
        Some("balanced") => options.steps = Some(20),
        Some("quality") => options.steps = Some(32),
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

#[cfg(test)]
mod create_regression_tests {
    use super::{apply_create_presets, parse_composition_blocks};
    use aos_proto::MediaImageGenerateRequest;

    #[test]
    fn native_create_presets_keep_dimensions_and_steps() {
        for (format, expected) in [
            ("1:1", (512, 512)),
            ("16:9", (768, 432)),
            ("9:16", (432, 768)),
        ] {
            let mut req = MediaImageGenerateRequest {
                prompt: "subject".into(),
                path: None,
                model_id: None,
                options: Default::default(),
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
            };
            apply_create_presets(&mut req);
            assert_eq!(
                (req.options.width, req.options.height),
                (Some(expected.0), Some(expected.1))
            );
            assert_eq!(req.options.steps, Some(20));
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
