//! Background bus runtime: poll + `handle_cmd`.

use crate::cmd::{Cmd, Evt};
use crate::chat_room;
use crate::i18n;
use crate::os_open::{aos_home, bin_aos_session};
use crate::{
    agent_id_cmd, agent_panel, chat_delegate_agent_spec, chrono_like_stamp, format_local_time_hm,
    invoke_module_bind,
    invoke_module_tool, invoke_notes, invoke_tasks, load_module_ui, load_session,
    announce_and_load_session, run_troubleshoot,
    session_has_running_canvas_agent, spawn_chat_delegate_agent, CHAT_AGENT_MAX_SUBAGENTS,
};
use aos_agent::intents as agent_intents;
use aos_agent::{ControlCmd, ControlResp};
use aos_agent::schedule::{
    ScheduleCreateRequest, ScheduleEntry, ScheduleIdRequest, ScheduleListResponse,
};
use aos_ipc::BusClient;
use aos_proto::{
    AgentCreateRequest, AgentGoal, AgentIdRequest, AgentInfo, AgentKind, AgentPromptOptimizeRequest,
    AgentPromptOptimizeResponse, AgentRosterUpdateRequest, AgentSpecResponse, AgentState, AgentSteerRequest,
    AgentTrace, AuditEvent, AuditQueryRequest,
    CapInfo, CapListRequest, CapRevokeRequest, ChatAttachment, ChatMessage, ChatRoomMember,
    ChatSessionAppendRequest, ChatSessionCreateRequest, ChatSessionGetResponse,
    ChatSessionIdRequest, ChatSessionMembersAddRequest, ChatSessionMembersRemoveRequest,
    ChatSessionMeta, ChatSessionRoomTurnCancelRequest,
    ChatSessionRoomTurnRequest, ChatSessionRoomTurnResponse, ChatSessionSetModeRequest,
    ChatSessionRenameRequest, ChatSessionSetModelRequest, ConfirmResponseRequest,
    FeedbackSubmitRequest, FeedbackSubmitResponse, FilesGenerateRequest, FilesGenerateResponse,
    InferParams, InferRequest, McpServerInfo, MemContextRequest, MemContextResponse,
    MemEpisodicDeleteRequest, MemExtractRequest, MemExtractResponse, MemHit, MemListRequest,
    UserLibraryAddRequest, UserLibraryAddResponse, UserLibraryListResponse, UserLibraryRemoveRequest,
    UserLibraryRemoveResponse,
    MemRememberResponse, MemSweepStatus, MemUpdateRequest, MemUserRecallRequest, MemUserRememberRequest,
    MemWorkingRequest, LoadRequest, ModelInfo, ModelState, ModuleCatalogue,
    ModuleInfo, ModuleInstallRequest,
    ModuleUninstallRequest, CancelRequest, MediaAudioGenerateRequest, MediaGenerateResponse,
    MediaImageGenerateRequest, MediaImageUpscaleRequest, NetFetchRequest, NetFetchResponse, NetModeRequest,
    PendingConfirmation, ProviderIdRequest, ProviderListResponse, ProviderRecord,
    ProviderTestResponse, ProviderUpsertRequest, SecretListRequest, SecretListResponse,
    SecretSetRequest, SetRoutingRequest, SkillInfo, SkillPassPendingOffer, SkillPassRequest,
    SystemMetrics, TokenEvent, WebBrowseRequest,
    WebBrowseResponse, WebSearchRequest, WebSearchResponse,
    CHAT_DELEGATION_PROMPT, CHAT_SUPERVISOR_LOCK, SYSTEM_ASSISTANT_PROMPT, MigrateRequest, MigrateResponse,
};
use eframe::egui;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};

/// Coalesce : un seul `mem.extract` à la fois (fire-and-forget).
static MEM_EXTRACT_BUSY: AtomicBool = AtomicBool::new(false);

pub(crate) async fn runtime_main(cmd_rx: Receiver<Cmd>, evt_tx: Sender<Evt>, egui_ctx: egui::Context) {
    let bus = match BusClient::connect("127.0.0.1:24701", "ui-egui").await {
        Ok(b) => b,
        Err(e) => {
            let _ = evt_tx.send(Evt::Error(format!(
                "bus injoignable ({e}). Lancez via aos-session."
            )));
            return;
        }
    };

    // Poll métriques / agents / confirms
    {
        let bus = bus.clone();
        let evt_tx = evt_tx.clone();
        let egui_ctx = egui_ctx.clone();
        tokio::spawn(async move {
            loop {
                if let Ok(m) = bus
                    .call::<(), SystemMetrics>("model.metrics", &(), vec![])
                    .await
                {
                    let _ = evt_tx.send(Evt::Metrics(m));
                }
                if let Ok(a) = bus
                    .call::<(), Vec<AgentInfo>>(aos_agent::intents::LIST, &(), vec![])
                    .await
                {
                    let _ = evt_tx.send(Evt::Agents(a));
                }
                if let Ok(models) = bus
                    .call::<(), Vec<ModelInfo>>("model.list", &(), vec![])
                    .await
                {
                    let _ = evt_tx.send(Evt::Models(models));
                }
                if let Ok(c) = bus
                    .call::<(), Vec<PendingConfirmation>>("confirm.list", &(), vec![])
                    .await
                {
                    let _ = evt_tx.send(Evt::Confirms(c));
                }
                egui_ctx.request_repaint();
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
    }

    while let Ok(cmd) = cmd_rx.recv() {
        let bus = bus.clone();
        let evt_tx = evt_tx.clone();
        let egui_ctx = egui_ctx.clone();
        tokio::spawn(async move {
            handle_cmd(bus, evt_tx, egui_ctx.clone(), cmd).await;
            egui_ctx.request_repaint();
        });
    }
}

/// E14 : fire-and-forget `mem.extract` (coalesce si déjà en cours).
pub(crate) fn maybe_spawn_mem_extract(
    bus: Arc<BusClient>,
    evt_tx: Sender<Evt>,
    enabled: bool,
    session_id: String,
    user_text: String,
    assistant_text: String,
    model_id: Option<String>,
) {
    if !enabled {
        return;
    }
    if aos_proto::mem_extract::should_skip_mem_extract_turn(&user_text) {
        return;
    }
    let assistant_text = if aos_proto::mem_extract::looks_like_tool_trace(&assistant_text) {
        String::new()
    } else {
        assistant_text
    };
    if user_text.trim().is_empty() && assistant_text.trim().is_empty() {
        return;
    }
    if MEM_EXTRACT_BUSY
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return; // skip — extract précédent encore en cours
    }
    tokio::spawn(async move {
        let req = MemExtractRequest {
            user_text,
            assistant_text,
            session_id: Some(session_id),
            model_id,
            persist: true,
        };
        let result = bus
            .call::<MemExtractRequest, MemExtractResponse>("mem.extract", &req, vec![])
            .await;
        MEM_EXTRACT_BUSY.store(false, Ordering::SeqCst);
        match result {
            Ok(resp) if resp.stored > 0 => {
                let _ = evt_tx.send(Evt::MemExtracted { n: resp.stored });
            }
            Ok(_) => {}
            Err(e) => {
                let _ = evt_tx.send(Evt::Status(format!("mem.extract: {e}")));
            }
        }
    });
}

fn push_evt(evt_tx: &Sender<Evt>, egui_ctx: &egui::Context, evt: Evt) {
    let _ = evt_tx.send(evt);
    egui_ctx.request_repaint();
}

async fn handle_cmd(
    bus: Arc<BusClient>,
    evt_tx: Sender<Evt>,
    egui_ctx: egui::Context,
    cmd: Cmd,
) {
    match cmd {
        Cmd::SessionBootstrap => {
            let list: Vec<ChatSessionMeta> = bus
                .call("chat.session.list", &(), vec![])
                .await
                .unwrap_or_default();
            if list.is_empty() {
                match bus
                    .call::<ChatSessionCreateRequest, ChatSessionMeta>(
                        "chat.session.create",
                        &ChatSessionCreateRequest {
                            title: Some("Session 1".into()),
                            model_id: None,
                        },
                        vec![],
                    )
                    .await
                {
                    Ok(m) => {
                        let _ = evt_tx.send(Evt::Sessions(vec![m.clone()]));
                        announce_and_load_session(&bus, &evt_tx, &m.id).await;
                    }
                    Err(e) => {
                        let _ = evt_tx.send(Evt::Error(format!("session create: {e}")));
                    }
                }
            } else {
                let id = list[0].id.clone();
                let _ = evt_tx.send(Evt::Sessions(list));
                announce_and_load_session(&bus, &evt_tx, &id).await;
            }
        }
        Cmd::SessionCreate { title } => {
            match bus
                .call::<ChatSessionCreateRequest, ChatSessionMeta>(
                    "chat.session.create",
                    &ChatSessionCreateRequest {
                        title,
                        model_id: None,
                    },
                    vec![],
                )
                .await
            {
                Ok(m) => {
                    let list: Vec<ChatSessionMeta> = bus
                        .call("chat.session.list", &(), vec![])
                        .await
                        .unwrap_or_default();
                    let _ = evt_tx.send(Evt::Sessions(list));
                    announce_and_load_session(&bus, &evt_tx, &m.id).await;
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::SessionSelect { id } => {
            load_session(&bus, &evt_tx, &id).await;
        }
        Cmd::SessionRename { id, title } => {
            let _ = bus
                .call::<ChatSessionRenameRequest, ChatSessionMeta>(
                    "chat.session.rename",
                    &ChatSessionRenameRequest {
                        session_id: id.clone(),
                        title,
                    },
                    vec![],
                )
                .await;
            let list: Vec<ChatSessionMeta> = bus
                .call("chat.session.list", &(), vec![])
                .await
                .unwrap_or_default();
            let _ = evt_tx.send(Evt::Sessions(list));
        }
        Cmd::SessionDelete { id } => {
            let _ = bus
                .call::<ChatSessionIdRequest, bool>(
                    "chat.session.delete",
                    &ChatSessionIdRequest { session_id: id },
                    vec![],
                )
                .await;
            let _ = evt_tx.send(Evt::Status("session supprimée".into()));
            let list: Vec<ChatSessionMeta> = bus
                .call("chat.session.list", &(), vec![])
                .await
                .unwrap_or_default();
            if list.is_empty() {
                match bus
                    .call::<ChatSessionCreateRequest, ChatSessionMeta>(
                        "chat.session.create",
                        &ChatSessionCreateRequest {
                            title: Some("Session 1".into()),
                            model_id: None,
                        },
                        vec![],
                    )
                    .await
                {
                    Ok(m) => {
                        let list2: Vec<ChatSessionMeta> = bus
                            .call("chat.session.list", &(), vec![])
                            .await
                            .unwrap_or_default();
                        let _ = evt_tx.send(Evt::Sessions(list2));
                        announce_and_load_session(&bus, &evt_tx, &m.id).await;
                    }
                    Err(e) => {
                        let _ = evt_tx.send(Evt::Error(e.to_string()));
                    }
                }
            } else {
                let id = list[0].id.clone();
                let _ = evt_tx.send(Evt::Sessions(list));
                announce_and_load_session(&bus, &evt_tx, &id).await;
            }
        }
        Cmd::SessionExport { id } => {
            match bus
                .call::<ChatSessionIdRequest, String>(
                    "chat.session.export",
                    &ChatSessionIdRequest { session_id: id },
                    vec![],
                )
                .await
            {
                Ok(md) => {
                    let path = aos_home().join("var/downloads").join(format!(
                        "session-export-{}.md",
                        chrono_like_stamp()
                    ));
                    let _ = std::fs::create_dir_all(path.parent().unwrap());
                    match std::fs::write(&path, md) {
                        Ok(()) => {
                            let _ = evt_tx.send(Evt::FileOk(path.display().to_string()));
                        }
                        Err(e) => {
                            let _ = evt_tx.send(Evt::Error(e.to_string()));
                        }
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::Chat {
            session_id,
            history,
            user_text,
            model_id,
            images,
            documents,
            auto_remember,
            max_steps,
            routing,
            language,
            canvas_open,
            canvas_aspect,
        } => {
            let _ = evt_tx.send(Evt::Status(
                "assistant : génération en cours…".into(),
            ));
            let user_content =
                aos_proto::chat_document::merge_documents_into_user_content(&user_text, &documents);
            let mut attachments: Vec<ChatAttachment> = images
                .iter()
                .map(|path| ChatAttachment::Image {
                    path: path.clone(),
                    prompt: String::new(),
                })
                .collect();
            attachments.extend(documents.iter().map(|doc| ChatAttachment::Document {
                path: doc.path.clone(),
                label: doc.label.clone(),
            }));
            let _ = bus
                .call::<ChatSessionAppendRequest, aos_proto::ChatSessionMessage>(
                    "chat.session.append",
                    &ChatSessionAppendRequest {
                        session_id: session_id.clone(),
                        role: "user".into(),
                        content: user_content.clone(),
                        attachments,
                        speaker_id: None,
                        speaker_name: None,
                    },
                    vec![],
                )
                .await;

            let mem_block = bus
                .call::<MemContextRequest, MemContextResponse>(
                    "mem.context",
                    &MemContextRequest {
                        session_id: Some(session_id.clone()),
                        query: user_content.clone(),
                        k: 5,
                        product_k: 4,
                        user_doc_k: 0,
                    },
                    vec![],
                )
                .await
                .ok()
                .map(|r| r.prompt_block)
                .unwrap_or_default();

            let version = std::env::var("AOS_PREVIEW_VERSION")
                .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());
            let product = crate::product_context::chat_product_context(&version, &language);

            let mut system = SYSTEM_ASSISTANT_PROMPT.to_string();
            system.push_str(CHAT_DELEGATION_PROMPT);
            system.push_str(CHAT_SUPERVISOR_LOCK);
            system.push_str("\n\n");
            system.push_str(&product);
            if !mem_block.trim().is_empty() {
                system.push_str("\n\n");
                system.push_str(&mem_block);
            }
            let mut messages = vec![ChatMessage {
                role: "system".into(),
                content: system,
            }];
            messages.extend(history.into_iter().map(|(r, c)| ChatMessage {
                role: r,
                content: c,
            }));
            aos_proto::chat_document::apply_documents_to_infer_messages(&mut messages, &documents);
            let mut infer_images = images.clone();
            let canvas_png = if canvas_open {
                aos_agent::canvas_scene::begin_canvas_vision(
                    &bus,
                    &session_id,
                    model_id.as_deref(),
                    canvas_aspect,
                )
                .await
            } else {
                None
            };
            if let Some(ref png) = canvas_png {
                infer_images =
                    aos_agent::canvas_scene::merge_canvas_vision_refs(&infer_images, png);
            }
            let canvas_active = canvas_png.is_some();
            let req = InferRequest {
                model_id: model_id.clone(),
                messages,
                params: InferParams {
                    max_tokens: 1024,
                    ..Default::default()
                },
                priority: 8,
                data_refs: infer_images.clone(),
                images: infer_images,
                routing: Some(routing),
            };
            let sid = session_id.clone();
            let sid_canvas = session_id.clone();
            let infer = async {
                match bus
                    .call_stream::<InferRequest, TokenEvent>("model.infer", &req, vec![])
                    .await
                {
                    Ok(mut rx) => {
                        let mut full = String::new();
                        while let Some(ev) = rx.recv().await {
                            match ev {
                                Ok(TokenEvent::Started { inference_id }) => {
                                    let _ = evt_tx.send(Evt::InferStarted {
                                        session_id: sid.clone(),
                                        inference_id,
                                    });
                                }
                                Ok(TokenEvent::Delta { text }) => {
                                    full.push_str(&text);
                                    let _ = evt_tx.send(Evt::Delta {
                                        session_id: sid.clone(),
                                        text,
                                    });
                                }
                                Ok(TokenEvent::Done { .. }) => break,
                                Ok(TokenEvent::Error { message }) => {
                                    let _ = evt_tx.send(Evt::Error(message));
                                    return;
                                }
                                _ => {}
                            }
                        }
                        if full.is_empty() {
                            let _ = evt_tx.send(Evt::Done {
                                text: String::new(),
                                session_id: sid,
                                attachments: vec![],
                            });
                            return;
                        }

                        // Délégation : agent.spawn / filet module → worker en fond
                        if let Some((brief, skills, tools, prose)) =
                            chat_delegate_agent_spec(&user_text, &full, canvas_open, canvas_aspect)
                        {
                            let canvas_delegate = tools.iter().any(|t| t.starts_with("canvas."));
                            if canvas_delegate
                                && session_has_running_canvas_agent(&bus, &sid).await
                            {
                                let _ = bus
                                    .call::<ChatSessionAppendRequest, aos_proto::ChatSessionMessage>(
                                        "chat.session.append",
                                        &ChatSessionAppendRequest {
                                            session_id: sid.clone(),
                                            role: "assistant".into(),
                                            content: "Un agent canvas dessine déjà sur cette session — \
                                                      attends qu'il termine ou consulte le canvas mis à jour."
                                                .into(),
                                            attachments: vec![],
                                            speaker_id: None,
                                            speaker_name: None,
                                        },
                                        vec![],
                                    )
                                    .await;
                                let _ = evt_tx.send(Evt::Done {
                                    text: String::new(),
                                    session_id: sid,
                                    attachments: vec![],
                                });
                                return;
                            }
                            spawn_chat_delegate_agent(
                                bus.clone(),
                                evt_tx.clone(),
                                sid,
                                user_text,
                                brief,
                                skills,
                                tools,
                                prose,
                                auto_remember,
                                model_id,
                                max_steps,
                                canvas_aspect,
                            )
                            .await;
                            return;
                        }

                        let display = agent_panel::format_chat_assistant_display(&full);
                        let _ = bus
                            .call::<ChatSessionAppendRequest, aos_proto::ChatSessionMessage>(
                                "chat.session.append",
                                &ChatSessionAppendRequest {
                                    session_id: sid.clone(),
                                    role: "assistant".into(),
                                    content: display.clone(),
                                    attachments: vec![],
                                    speaker_id: None,
                                    speaker_name: None,
                                },
                                vec![],
                            )
                            .await;
                        maybe_spawn_mem_extract(
                            bus.clone(),
                            evt_tx.clone(),
                            auto_remember,
                            sid.clone(),
                            user_text.clone(),
                            display.clone(),
                            model_id.clone(),
                        );
                        let _ = evt_tx.send(Evt::Done {
                            text: display,
                            session_id: sid,
                            attachments: vec![],
                        });
                    }
                    Err(e) => {
                        let _ = evt_tx.send(Evt::Error(e.to_string()));
                    }
                }
            };
            match tokio::time::timeout(std::time::Duration::from_secs(180), infer).await {
                Ok(()) => {}
                Err(_) => {
                    let _ = evt_tx.send(Evt::Error(
                        "timeout chat (180 s) — modeld a peut-être planté (voir var/run/aos-modeld.stderr.log) ; relancez aos-session".into(),
                    ));
                }
            }
            if canvas_active {
                aos_agent::canvas_scene::end_canvas_vision(&bus, &sid_canvas).await;
            }
        }
        Cmd::MemRecall { query } => {
            match bus
                .call::<MemUserRecallRequest, Vec<MemHit>>(
                    "mem.user.recall",
                    &MemUserRecallRequest { query, k: 8 },
                    vec![],
                )
                .await
            {
                Ok(hits) => {
                    let _ = evt_tx.send(Evt::MemHits(hits));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::MemRemember { text, pinned } => {
            match bus
                .call::<MemUserRememberRequest, MemRememberResponse>(
                    "mem.user.remember",
                    &MemUserRememberRequest {
                        text,
                        metadata: serde_json::json!({"source": "ui"}),
                        pinned,
                        ..Default::default()
                    },
                    vec![],
                )
                .await
            {
                Ok(r) => {
                    let extra = if r.auto_relations.is_empty() {
                        String::new()
                    } else {
                        format!(" (+{} relation(s))", r.auto_relations.len())
                    };
                    let _ = evt_tx.send(Evt::Status(format!(
                        "mémoire enregistrée ({}{})",
                        r.id, extra
                    )));
                    // Refresh list
                    if let Ok(hits) = bus
                        .call::<MemListRequest, Vec<MemHit>>(
                            "mem.list",
                            &MemListRequest {
                                namespace: "user:default".into(),
                                include_superseded: true,
                            },
                            vec![],
                        )
                        .await
                    {
                        let _ = evt_tx.send(Evt::MemHits(hits));
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::MemList { include_superseded } => {
            match bus
                .call::<MemListRequest, Vec<MemHit>>(
                    "mem.list",
                    &MemListRequest {
                        namespace: "user:default".into(),
                        include_superseded,
                    },
                    vec![],
                )
                .await
            {
                Ok(hits) => {
                    let _ = evt_tx.send(Evt::MemHits(hits));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::MemSweepStatus => {
            match bus
                .call::<(), MemSweepStatus>("mem.sweep.status", &(), vec![])
                .await
            {
                Ok(status) => {
                    let offset = sweep_tz_offset_minutes();
                    let label = if status.last_pass_ms > 0 {
                        format_local_time_hm(status.last_pass_ms, offset)
                    } else {
                        String::new()
                    };
                    let _ = evt_tx.send(Evt::MemSweepStatus {
                        last_pass_ms: status.last_pass_ms,
                        last_pass_label: label,
                    });
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::SkillPassPending => {
            let offset = sweep_tz_offset_minutes();
            match bus
                .call::<SkillPassRequest, Option<SkillPassPendingOffer>>(
                    "skill.pass.pending",
                    &SkillPassRequest {
                        tz_offset_minutes: Some(offset),
                        force: false,
                    },
                    vec![],
                )
                .await
            {
                Ok(offer) => {
                    let _ = evt_tx.send(Evt::SkillPassPending(offer));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::SkillPassDismiss { pattern_id } => {
            let offset = sweep_tz_offset_minutes();
            let _ = bus
                .call::<aos_proto::SkillPassDismissRequest, ()>(
                    "skill.pass.dismiss",
                    &aos_proto::SkillPassDismissRequest {
                        pattern_id,
                        tz_offset_minutes: Some(offset),
                    },
                    vec![],
                )
                .await;
            let _ = evt_tx.send(Evt::SkillPassPending(None));
        }
        Cmd::SkillPassCreate { pattern_id } => {
            match bus
                .call::<aos_proto::SkillPassCreateRequest, SkillInfo>(
                    "skill.pass.create",
                    &aos_proto::SkillPassCreateRequest {
                        pattern_id: pattern_id.clone(),
                        actor: "human:ui".into(),
                    },
                    vec![],
                )
                .await
            {
                Ok(info) => {
                    let _ = evt_tx.send(Evt::SkillPassCreated {
                        pattern_id,
                        skill_name: info.name,
                    });
                    if let Ok(list) = bus.call::<(), Vec<SkillInfo>>("skill.list", &(), vec![]).await
                    {
                        let _ = evt_tx.send(Evt::Skills(list));
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::MemDelete { id } => {
            match bus
                .call::<MemEpisodicDeleteRequest, serde_json::Value>(
                    "mem.episodic_delete",
                    &MemEpisodicDeleteRequest {
                        id: Some(id),
                        namespace: None,
                        meta_key: None,
                        meta_value: None,
                    },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    let _ = evt_tx.send(Evt::Status(format!("souvenir {id} supprimé")));
                    if let Ok(hits) = bus
                        .call::<MemListRequest, Vec<MemHit>>(
                            "mem.list",
                            &MemListRequest {
                                namespace: "user:default".into(),
                                include_superseded: true,
                            },
                            vec![],
                        )
                        .await
                    {
                        let _ = evt_tx.send(Evt::MemHits(hits));
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::MemWipeUser => {
            match bus
                .call::<MemWorkingRequest, usize>(
                    "mem.wipe",
                    &MemWorkingRequest {
                        agent_id: "user:default".into(),
                        messages: vec![],
                    },
                    vec![],
                )
                .await
            {
                Ok(n) => {
                    let _ = evt_tx.send(Evt::Status(format!("mémoire utilisateur effacée ({n})")));
                    let _ = evt_tx.send(Evt::MemHits(Vec::new()));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::MemSupersede { id, text } => {
            match bus
                .call::<MemUpdateRequest, MemRememberResponse>(
                    "mem.update",
                    &MemUpdateRequest {
                        id,
                        text,
                        metadata: None,
                        pinned: Some(true),
                        supersede: true,
                    },
                    vec![],
                )
                .await
            {
                Ok(r) => {
                    let _ = evt_tx.send(Evt::Status(format!(
                        "remplacé → id {} (supersedes {id})",
                        r.id
                    )));
                    if let Ok(hits) = bus
                        .call::<MemListRequest, Vec<MemHit>>(
                            "mem.list",
                            &MemListRequest {
                                namespace: "user:default".into(),
                                include_superseded: true,
                            },
                            vec![],
                        )
                        .await
                    {
                        let _ = evt_tx.send(Evt::MemHits(hits));
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::MemEdit { id, text } => {
            match bus
                .call::<MemUpdateRequest, MemRememberResponse>(
                    "mem.update",
                    &MemUpdateRequest {
                        id,
                        text,
                        metadata: None,
                        pinned: None,
                        supersede: false,
                    },
                    vec![],
                )
                .await
            {
                Ok(r) => {
                    let _ = evt_tx.send(Evt::Status(format!("souvenir {} mis à jour", r.id)));
                    if let Ok(hits) = bus
                        .call::<MemListRequest, Vec<MemHit>>(
                            "mem.list",
                            &MemListRequest {
                                namespace: "user:default".into(),
                                include_superseded: true,
                            },
                            vec![],
                        )
                        .await
                    {
                        let _ = evt_tx.send(Evt::MemHits(hits));
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::SecretSet { name, value } => {
            match bus
                .call::<SecretSetRequest, bool>(
                    "secrets.set",
                    &SecretSetRequest { name: name.clone(), value },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    let _ = evt_tx.send(Evt::Status(format!("secret `{name}` enregistré (chiffré)")));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::ChatCancel {
            inference_id,
            session_id,
        } => {
            match bus
                .call::<CancelRequest, bool>(
                    "model.cancel",
                    &CancelRequest { inference_id },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    let _ = evt_tx.send(Evt::ChatCancelled {
                        session_id,
                    });
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::CatalogueRefresh => {
            match bus
                .call::<(), ModuleCatalogue>("module.catalogue", &(), vec![])
                .await
            {
                Ok(c) => {
                    let _ = evt_tx.send(Evt::Catalogue(c));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::ModuleList => {
            match bus
                .call::<(), Vec<ModuleInfo>>("module.list", &(), vec![])
                .await
            {
                Ok(list) => {
                    let _ = evt_tx.send(Evt::InstalledModules(list));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::ModuleInstall {
            source_dir,
            approved_caps,
        } => {
            match bus
                .call::<ModuleInstallRequest, aos_proto::ModuleInfo>(
                    "module.install",
                    &ModuleInstallRequest {
                        source_dir: source_dir.clone(),
                        approved_caps,
                        actor: "human:ui".into(),
                        actor_caps: vec![],
                    },
                    vec![],
                )
                .await
            {
                Ok(info) => {
                    let _ = evt_tx.send(Evt::ModuleInstalled(format!(
                        "{} v{} (quarantined={})",
                        info.name, info.version, info.quarantined
                    )));
                    if let Ok(list) = bus
                        .call::<(), Vec<ModuleInfo>>("module.list", &(), vec![])
                        .await
                    {
                        let _ = evt_tx.send(Evt::InstalledModules(list));
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::ModuleUninstall { name } => {
            match bus
                .call::<ModuleUninstallRequest, Result<(), String>>(
                    "module.uninstall",
                    &ModuleUninstallRequest {
                        module: name.clone(),
                        actor: "human:ui".into(),
                        actor_caps: vec![],
                    },
                    vec![],
                )
                .await
            {
                Ok(Ok(())) => {
                    let _ = evt_tx.send(Evt::ModuleUninstalled(name));
                    if let Ok(list) = bus
                        .call::<(), Vec<ModuleInfo>>("module.list", &(), vec![])
                        .await
                    {
                        let _ = evt_tx.send(Evt::InstalledModules(list));
                    }
                }
                Ok(Err(e)) => {
                    let _ = evt_tx.send(Evt::Error(e));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::ModuleUiLoad { module } => {
            load_module_ui(&bus, &evt_tx, &module).await;
        }
        Cmd::ModuleUiRefresh { module } => {
            load_module_ui(&bus, &evt_tx, &module).await;
        }
        Cmd::ModuleUiBind { module, tool } => {
            invoke_module_bind(&bus, &evt_tx, &module, &tool).await;
        }
        Cmd::ModuleUiInvoke {
            module,
            tool,
            args,
        } => {
            invoke_module_tool(&bus, &evt_tx, &module, &tool, args).await;
        }
        Cmd::SecretList => {
            match bus
                .call::<SecretListRequest, SecretListResponse>(
                    "secrets.list",
                    &SecretListRequest {},
                    vec![],
                )
                .await
            {
                Ok(r) => {
                    let names = if r.names.is_empty() {
                        "(aucun)".into()
                    } else {
                        r.names.join(", ")
                    };
                    let enc = if r.encrypted { "vault.enc" } else { "clair" };
                    let _ = evt_tx.send(Evt::SecretList {
                        names: r.names,
                        encrypted: r.encrypted,
                    });
                    let _ = evt_tx.send(Evt::Status(format!("secrets [{enc}]: {names}")));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::NetSetMode { online } => {
            let mode = if online {
                "online".to_string()
            } else {
                "offline_strict".to_string()
            };
            match bus
                .call::<NetModeRequest, bool>("net.set_mode", &NetModeRequest { mode }, vec![])
                .await
            {
                Ok(_) => {
                    let _ = evt_tx.send(Evt::NetMode(online));
                    let _ = evt_tx.send(Evt::Status(if online {
                        "réseau autorisé (online)".into()
                    } else {
                        "réseau coupé (offline_strict)".into()
                    }));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::SetRouting { mode } => {
            match bus
                .call::<SetRoutingRequest, Result<(), String>>(
                    "model.set_routing",
                    &SetRoutingRequest { mode: mode.clone() },
                    vec![],
                )
                .await
            {
                Ok(Ok(())) => {
                    let _ = evt_tx.send(Evt::Status(format!("routing → {mode}")));
                }
                Ok(Err(e)) => {
                    let _ = evt_tx.send(Evt::Error(format!("model.set_routing: {e}")));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("model.set_routing: {e}")));
                }
            }
        }
        Cmd::WebSearch { query, engine } => {
            let req = WebSearchRequest {
                query,
                max_results: 5,
                caps: vec![
                    "net.connect:html.duckduckgo.com:443".into(),
                    "net.connect:api.search.brave.com:443".into(),
                    "net.connect:www.bing.com:443".into(),
                    "net.connect:*:*".into(),
                ],
                actor: "human:ui".into(),
                engine,
            };
            match bus
                .call::<WebSearchRequest, WebSearchResponse>("web.search", &req, vec![])
                .await
            {
                Ok(r) => {
                    let _ = evt_tx.send(Evt::WebResults(r.results));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("web.search: {e}")));
                }
            }
        }
        Cmd::WebBrowse { url, max_chars } => {
            let req = WebBrowseRequest {
                url,
                max_chars,
                caps: vec!["net.connect:*:*".into()],
                actor: "human:ui".into(),
            };
            match bus
                .call::<WebBrowseRequest, WebBrowseResponse>("web.browse", &req, vec![])
                .await
            {
                Ok(r) => {
                    let body = if r.text.chars().count() > 2000 {
                        format!("{}…", r.text.chars().take(2000).collect::<String>())
                    } else {
                        r.text
                    };
                    let preview = format!("{}\n{}\n\n{}", r.title, r.final_url, body);
                    let _ = evt_tx.send(Evt::BrowsePreview(preview));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("web.browse: {e}")));
                }
            }
        }
        Cmd::NetFetch { url, max_bytes } => {
            let req = NetFetchRequest {
                url,
                dest_path: None,
                max_bytes,
                caps: vec![
                    "net.connect:*:*".into(),
                    "fs.write:/downloads/**".into(),
                ],
                actor: "human:ui".into(),
            };
            match bus
                .call::<NetFetchRequest, NetFetchResponse>("net.fetch", &req, vec![])
                .await
            {
                Ok(r) => {
                    let _ = evt_tx.send(Evt::FileOk(format!(
                        "téléchargé {} ({} octets, {})",
                        r.path, r.bytes, r.content_type
                    )));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("net.fetch: {e}")));
                }
            }
        }
        Cmd::FilesGenerate {
            format,
            path,
            content,
            title,
        } => {
            let req = FilesGenerateRequest {
                format,
                path,
                content,
                title,
                caps: vec!["fs.write:/downloads/**".into()],
                actor: "human:ui".into(),
            };
            match bus
                .call::<FilesGenerateRequest, FilesGenerateResponse>(
                    "files.generate",
                    &req,
                    vec![],
                )
                .await
            {
                Ok(r) => {
                    let _ = evt_tx.send(Evt::FileOk(format!(
                        "généré {} ({} octets)",
                        r.path, r.bytes
                    )));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("files.generate: {e}")));
                }
            }
        }
        Cmd::Help => {
            let mut services = Vec::new();
            for (name, probe) in [
                ("modeld", "model.list"),
                ("agentd", "agent.list"),
                ("platformd", "module.list"),
                ("capkd", "cap.check"),
            ] {
                let up = bus.lookup(probe).await.unwrap_or(false);
                services.push(format!("{name}: {}", if up { "up" } else { "DOWN" }));
            }
            let models: Vec<ModelInfo> = bus
                .call("model.list", &(), vec![])
                .await
                .unwrap_or_default();
            let loaded = models
                .iter()
                .filter(|m| matches!(m.state, ModelState::Loaded | ModelState::PartiallyOffloaded))
                .count();
            let agents: Vec<AgentInfo> = bus
                .call(aos_agent::intents::LIST, &(), vec![])
                .await
                .unwrap_or_default();
            let running = agents
                .iter()
                .filter(|a| matches!(a.state, AgentState::Running))
                .count();
            let metrics: Option<SystemMetrics> = bus.call("model.metrics", &(), vec![]).await.ok();
            let mut out = String::from("Akasha OS Preview — état\n");
            out.push_str(&format!("services : {}\n", services.join(", ")));
            out.push_str(&format!(
                "modèles : {loaded} chargés / {} au registry\n",
                models.len()
            ));
            out.push_str(&format!(
                "agents : {running} running / {} total\n",
                agents.len()
            ));
            if let Some(m) = metrics {
                out.push_str(&format!(
                    "hôte : RAM {:.1}/{:.1} GiB, CPU {:.0}%\n",
                    m.ram_used as f64 / (1 << 30) as f64,
                    m.ram_total as f64 / (1 << 30) as f64,
                    m.cpu_percent
                ));
            }
            out.push_str("→ /commands pour la liste des commandes");
            let _ = evt_tx.send(Evt::ChatSystem(out));
        }
        Cmd::NotesList => {
            invoke_notes(&bus, &evt_tx, "notes.list", serde_json::json!({})).await;
        }
        Cmd::NotesCreate { title, content } => {
            invoke_notes(
                &bus,
                &evt_tx,
                "notes.create",
                serde_json::json!({ "title": title, "content": content }),
            )
            .await;
        }
        Cmd::NotesUpdate {
            title,
            path,
            content,
        } => {
            invoke_notes(
                &bus,
                &evt_tx,
                "notes.update",
                serde_json::json!({ "title": title, "path": path, "content": content }),
            )
            .await;
        }
        Cmd::NotesRead { title, path, slug } => {
            let mut args = serde_json::json!({});
            if let Some(t) = title {
                args["title"] = serde_json::json!(t);
            }
            if let Some(p) = path {
                args["path"] = serde_json::json!(p);
            }
            if let Some(s) = slug {
                args["slug"] = serde_json::json!(s);
            }
            invoke_notes(&bus, &evt_tx, "notes.read", args).await;
        }
        Cmd::NotesSearch { query } => {
            invoke_notes(
                &bus,
                &evt_tx,
                "notes.search",
                serde_json::json!({ "query": query }),
            )
            .await;
        }
        Cmd::NotesRelated { path, topic } => {
            let mut args = serde_json::json!({ "path": path });
            if !topic.is_empty() {
                args["topic"] = serde_json::json!(topic);
            }
            invoke_notes(&bus, &evt_tx, "notes.related", args).await;
        }
        Cmd::UserLibraryList => {
            match bus
                .call::<(), UserLibraryListResponse>("user.library.list", &(), vec![])
                .await
            {
                Ok(resp) => {
                    let _ = evt_tx.send(Evt::UserLibraryListed(resp.docs));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::UserLibraryAdd { path } => {
            match bus
                .call::<UserLibraryAddRequest, UserLibraryAddResponse>(
                    "user.library.add",
                    &UserLibraryAddRequest { path },
                    vec![],
                )
                .await
            {
                Ok(_resp) => {
                    if let Ok(list) = bus
                        .call::<(), UserLibraryListResponse>("user.library.list", &(), vec![])
                        .await
                    {
                        let _ = evt_tx.send(Evt::UserLibraryListed(list.docs));
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::UserLibraryRemove { id } => {
            match bus
                .call::<UserLibraryRemoveRequest, UserLibraryRemoveResponse>(
                    "user.library.remove",
                    &UserLibraryRemoveRequest { id },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    if let Ok(list) = bus
                        .call::<(), UserLibraryListResponse>("user.library.list", &(), vec![])
                        .await
                    {
                        let _ = evt_tx.send(Evt::UserLibraryListed(list.docs));
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::Confirm { id, approved } => {
            match bus
                .call::<ConfirmResponseRequest, bool>(
                    "confirm.respond",
                    &ConfirmResponseRequest { id, approved },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    let _ = evt_tx.send(Evt::Status(if approved {
                        "confirmation acceptée".into()
                    } else {
                        "confirmation refusée".into()
                    }));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::AgentCreate {
            display_name,
            task,
            system_prompt,
            skills,
            tools,
            mcp_servers,
            documents,
            optimize_prompt,
            max_steps,
            timeout_secs,
            model_id,
            session_id,
            origin,
            join_active_room,
            library,
        } => {
            let name = display_name.trim().to_string();
            if name.is_empty() {
                let t = i18n::strings(&crate::prefs::load_preferences().language);
                let _ = evt_tx.send(Evt::Error(t.agents_label_required.into()));
                return;
            }
            let has_goal = !library && !task.trim().is_empty();
            let mut req = AgentCreateRequest::simple(if library {
                String::new()
            } else {
                task.clone()
            });
            req.kind = if library || !has_goal {
                AgentKind::Roster
            } else {
                AgentKind::Task
            };
            req.display_name = Some(name.clone());
            req.origin = Some(origin.clone());
            if library {
                let role = task.trim();
                req.system_prompt = if system_prompt.is_some() {
                    system_prompt
                } else if role.is_empty() {
                    None
                } else {
                    Some(role.to_string())
                };
            } else {
                req.system_prompt = system_prompt;
            }
            req.skills = skills;
            req.tools = tools;
            req.mcp_servers = mcp_servers;
            req.documents = documents;
            req.optimize_prompt = if library { false } else { optimize_prompt };
            req.session_id = session_id.clone();
            if has_goal {
                req.goal = Some(AgentGoal {
                    statement: task.clone(),
                    success_criteria: vec![],
                    max_steps,
                    max_subagents: CHAT_AGENT_MAX_SUBAGENTS,
                    timeout_secs,
                });
            }
            req.model_id = model_id;
            req.gate_mode = crate::prefs::load_preferences().agent_gate_mode.clone();
            if req.skills.iter().any(|s| s.contains("notes"))
                || req.tools.iter().any(|t| t.starts_with("notes."))
            {
                req.caps.push("tool.invoke:notes".into());
            }
            if req.tools.iter().any(|t| t.starts_with("module.")) {
                if !req.caps.iter().any(|c| c == "module.install") {
                    req.caps.push("module.install".into());
                }
            }
            match bus
                .call::<AgentCreateRequest, aos_proto::AgentCreateResponse>(
                    aos_agent::intents::CREATE,
                    &req,
                    vec![],
                )
                .await
            {
                Ok(r) => {
                    if join_active_room {
                        if let Some(sid) = session_id.clone() {
                            if let Ok(resp) = bus
                                .call::<ChatSessionIdRequest, ChatSessionGetResponse>(
                                    "chat.session.get",
                                    &ChatSessionIdRequest {
                                        session_id: sid.clone(),
                                    },
                                    vec![],
                                )
                                .await
                            {
                                if resp.meta.mode == aos_proto::ChatSessionMode::Room {
                                    let member = ChatRoomMember {
                                        agent_id: r.agent_id.clone(),
                                        display_name: name.clone(),
                                        persona_id: None,
                                        joined_ms: room_joined_ms(),
                                    };
                                    let _ = bus
                                        .call::<ChatSessionMembersAddRequest, ChatSessionMeta>(
                                            "chat.session.members.add",
                                            &ChatSessionMembersAddRequest {
                                                session_id: sid.clone(),
                                                member,
                                            },
                                            vec![],
                                        )
                                        .await;
                                    refresh_sessions(&bus, &evt_tx).await;
                                    load_session(&bus, &evt_tx, &sid).await;
                                }
                            }
                        }
                    }
                    if library {
                        if let Ok(list) = bus
                            .call::<(), Vec<AgentInfo>>(aos_agent::intents::LIST, &(), vec![])
                            .await
                        {
                            let _ = evt_tx.send(Evt::Agents(list));
                        }
                        let t = i18n::strings(&crate::prefs::load_preferences().language);
                        let _ = evt_tx.send(Evt::Status(
                            t.agents_library_added
                                .replace("{name}", &name)
                                .replace("{id}", &r.agent_id),
                        ));
                    } else if origin == "library" {
                        if let Ok(list) = bus
                            .call::<(), Vec<AgentInfo>>(aos_agent::intents::LIST, &(), vec![])
                            .await
                        {
                            let _ = evt_tx.send(Evt::Agents(list));
                        }
                        let t = i18n::strings(&crate::prefs::load_preferences().language);
                        let msg = if has_goal {
                            t.agents_task_launched.replace("{id}", &r.agent_id)
                        } else {
                            t.agents_roster_registered.replace("{id}", &r.agent_id)
                        };
                        let _ = evt_tx.send(Evt::Status(msg));
                    } else if let Some(sid) = session_id {
                        let t = i18n::strings(&crate::prefs::load_preferences().language);
                        let ack = if has_goal {
                            t.agents_task_launched.replace("{id}", &r.agent_id)
                        } else {
                            t.agents_roster_registered.replace("{id}", &r.agent_id)
                        };
                        let card_title = if has_goal {
                            name.clone()
                        } else {
                            name.clone()
                        };
                        let att = ChatAttachment::AgentRef {
                            agent_id: r.agent_id.clone(),
                            title: card_title.clone(),
                            origin: origin.clone(),
                        };
                        let _ = bus
                            .call::<ChatSessionAppendRequest, aos_proto::ChatSessionMessage>(
                                "chat.session.append",
                                &ChatSessionAppendRequest {
                                    session_id: sid.clone(),
                                    role: "assistant".into(),
                                    content: ack.clone(),
                                    attachments: vec![att],
                                    speaker_id: None,
                                    speaker_name: None,
                                },
                                vec![],
                            )
                            .await;
                        let _ = evt_tx.send(Evt::AgentSpawned {
                            session_id: sid,
                            agent_id: r.agent_id.clone(),
                            title: card_title,
                            origin,
                            ack,
                        });
                    } else {
                        let t = i18n::strings(&crate::prefs::load_preferences().language);
                        let kind = if has_goal {
                            t.agents_status_created
                        } else {
                            t.agents_status_roster
                        };
                        let _ = evt_tx.send(Evt::Status(
                            t.agents_created_status
                                .replace("{id}", &r.agent_id)
                                .replace("{kind}", kind),
                        ));
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::AgentCatalogRefresh => {
            if let Ok(list) = bus
                .call::<(), Vec<SkillInfo>>(aos_agent::intents::SKILL_LIST, &(), vec![])
                .await
            {
                let _ = evt_tx.send(Evt::Skills(list));
            }
            if let Ok(list) = bus
                .call::<(), Vec<McpServerInfo>>(aos_agent::intents::MCP_LIST, &(), vec![])
                .await
            {
                let _ = evt_tx.send(Evt::McpServers(list));
            }
        }
        Cmd::AgentSpecGet { id } => {
            match bus
                .call::<AgentIdRequest, AgentSpecResponse>(
                    aos_agent::intents::SPEC_GET,
                    &AgentIdRequest { agent_id: id },
                    vec![],
                )
                .await
            {
                Ok(resp) => {
                    let _ = evt_tx.send(Evt::AgentSpecLoaded { spec: resp.spec });
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::AgentRosterUpdate {
            agent_id,
            display_name,
            role,
            system_prompt,
            skills,
            tools,
            mcp_servers,
            model_id,
        } => {
            match bus
                .call::<AgentRosterUpdateRequest, AgentSpecResponse>(
                    aos_agent::intents::ROSTER_UPDATE,
                    &AgentRosterUpdateRequest {
                        agent_id: agent_id.clone(),
                        display_name: Some(display_name),
                        role: Some(role),
                        system_prompt,
                        skills,
                        tools,
                        mcp_servers,
                        model_id,
                    },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    let _ = evt_tx.send(Evt::AgentRosterSaved);
                    if let Ok(list) = bus
                        .call::<(), Vec<AgentInfo>>(aos_agent::intents::LIST, &(), vec![])
                        .await
                    {
                        let _ = evt_tx.send(Evt::Agents(list));
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::AgentPromptOptimize {
            goal,
            skills,
            tools,
            current,
        } => {
            match bus
                .call::<AgentPromptOptimizeRequest, AgentPromptOptimizeResponse>(
                    aos_agent::intents::PROMPT_OPTIMIZE,
                    &AgentPromptOptimizeRequest {
                        goal,
                        skills,
                        tools,
                        current_prompt: current,
                        model_id: None,
                    },
                    vec![],
                )
                .await
            {
                Ok(r) => {
                    let _ = evt_tx.send(Evt::PromptOptimized(r.optimized_prompt));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::AgentKill { id } => {
            agent_id_cmd(&bus, &evt_tx, aos_agent::intents::KILL, id).await;
        }
        Cmd::AgentPause { id } => {
            agent_id_cmd(&bus, &evt_tx, aos_agent::intents::PAUSE, id).await;
        }
        Cmd::AgentResume { id } => {
            agent_id_cmd(&bus, &evt_tx, aos_agent::intents::RESUME, id).await;
        }
        Cmd::AgentRetry { id } => {
            agent_id_cmd(&bus, &evt_tx, aos_agent::intents::RETRY, id).await;
        }
        Cmd::AgentSteer { id, text } => {
            match bus
                .call::<AgentSteerRequest, bool>(
                    aos_agent::intents::STEER,
                    &AgentSteerRequest {
                        agent_id: id,
                        directive: text,
                    },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    let _ = evt_tx.send(Evt::Status("steer envoyé".into()));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::AgentActDecision {
            agent_id,
            act_id,
            approved,
        } => {
            let intent = format!("agent.{agent_id}.control");
            match bus
                .call::<ControlCmd, ControlResp>(
                    &intent,
                    &ControlCmd::ActDecision { act_id, approved },
                    vec![],
                )
                .await
            {
                Ok(ControlResp::Ack) => {
                    let _ = evt_tx.send(Evt::Status(if approved {
                        "action autorisée une fois".into()
                    } else {
                        "action refusée".into()
                    }));
                }
                Ok(_other) => {
                    let _ = evt_tx.send(Evt::Error("act decision: réponse inattendue".into()));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::AgentTrace { id } => {
            match bus
                .call::<AgentIdRequest, AgentTrace>(
                    aos_agent::intents::TRACE,
                    &AgentIdRequest { agent_id: id },
                    vec![],
                )
                .await
            {
                Ok(t) => {
                    let _ = evt_tx.send(Evt::AgentTrace(t));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("agent.trace: {e}")));
                }
            }
        }
        Cmd::Audit { last } => {
            match bus
                .call::<AuditQueryRequest, Vec<AuditEvent>>(
                    "audit.query",
                    &AuditQueryRequest {
                        trace_id: None,
                        actor: None,
                        action: None,
                        last,
                    },
                    vec![],
                )
                .await
            {
                Ok(ev) => {
                    let _ = evt_tx.send(Evt::Audit(ev));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::CapList { holder } => {
            match bus
                .call::<CapListRequest, Vec<CapInfo>>(
                    "cap.list",
                    &CapListRequest {
                        holder: holder.clone(),
                    },
                    vec![],
                )
                .await
            {
                Ok(caps) => {
                    let _ = evt_tx.send(Evt::Caps { holder, caps });
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("cap.list: {e}")));
                }
            }
        }
        Cmd::CapRevoke {
            holder,
            cap_id,
            tree,
        } => {
            match bus
                .call::<CapRevokeRequest, u64>(
                    "cap.revoke",
                    &CapRevokeRequest {
                        holder: holder.clone(),
                        cap: cap_id,
                        tree,
                    },
                    vec![],
                )
                .await
            {
                Ok(n) => {
                    let _ = evt_tx.send(Evt::Status(format!(
                        "cap.revoke: {n} capacité(s) révoquée(s) (holder={holder}, cap={cap_id})"
                    )));
                    match bus
                        .call::<CapListRequest, Vec<CapInfo>>(
                            "cap.list",
                            &CapListRequest {
                                holder: holder.clone(),
                            },
                            vec![],
                        )
                        .await
                    {
                        Ok(caps) => {
                            let _ = evt_tx.send(Evt::Caps { holder, caps });
                        }
                        Err(e) => {
                            let _ = evt_tx.send(Evt::Error(format!("cap.list: {e}")));
                        }
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("cap.revoke: {e}")));
                }
            }
        }
        Cmd::ScheduleList => {
            match bus
                .call::<(), ScheduleListResponse>(agent_intents::SCHEDULE_LIST, &(), vec![])
                .await
            {
                Ok(r) => {
                    let _ = evt_tx.send(Evt::Schedules(r.schedules));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("schedule.list: {e}")));
                }
            }
        }
        Cmd::ScheduleCreate {
            goal,
            interval_secs,
            next_fire_ms,
            display_title,
        } => {
            match bus
                .call::<ScheduleCreateRequest, ScheduleEntry>(
                    agent_intents::SCHEDULE_CREATE,
                    &ScheduleCreateRequest {
                        goal,
                        interval_secs,
                        model_id: None,
                        next_fire_ms,
                        display_title,
                    },
                    vec![],
                )
                .await
            {
                Ok(e) => {
                    let _ = evt_tx.send(Evt::ScheduleCreated(e));
                    if let Ok(r) = bus
                        .call::<(), ScheduleListResponse>(agent_intents::SCHEDULE_LIST, &(), vec![])
                        .await
                    {
                        let _ = evt_tx.send(Evt::Schedules(r.schedules));
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("schedule.create: {e}")));
                }
            }
        }
        Cmd::ScheduleCancel { id } => {
            match bus
                .call::<ScheduleIdRequest, ScheduleEntry>(
                    agent_intents::SCHEDULE_CANCEL,
                    &ScheduleIdRequest { id: id.clone() },
                    vec![],
                )
                .await
            {
                Ok(e) => {
                    let _ = evt_tx.send(Evt::ScheduleUpdated(e));
                    if let Ok(r) = bus
                        .call::<(), ScheduleListResponse>(agent_intents::SCHEDULE_LIST, &(), vec![])
                        .await
                    {
                        let _ = evt_tx.send(Evt::Schedules(r.schedules));
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("schedule.cancel: {e}")));
                }
            }
        }
        Cmd::SchedulePause { id } => {
            match bus
                .call::<ScheduleIdRequest, ScheduleEntry>(
                    agent_intents::SCHEDULE_PAUSE,
                    &ScheduleIdRequest { id: id.clone() },
                    vec![],
                )
                .await
            {
                Ok(e) => {
                    let _ = evt_tx.send(Evt::ScheduleUpdated(e));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("schedule.pause: {e}")));
                }
            }
        }
        Cmd::ScheduleResume { id } => {
            match bus
                .call::<ScheduleIdRequest, ScheduleEntry>(
                    agent_intents::SCHEDULE_RESUME,
                    &ScheduleIdRequest { id: id.clone() },
                    vec![],
                )
                .await
            {
                Ok(e) => {
                    let _ = evt_tx.send(Evt::ScheduleUpdated(e));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("schedule.resume: {e}")));
                }
            }
        }
        Cmd::TasksList => {
            invoke_tasks(&bus, &evt_tx, "tasks.list", serde_json::json!({})).await;
        }
        Cmd::TasksCreate { title, notes } => {
            invoke_tasks(
                &bus,
                &evt_tx,
                "tasks.create",
                serde_json::json!({ "title": title, "notes": notes }),
            )
            .await;
        }
        Cmd::TasksComplete { id, done } => {
            invoke_tasks(
                &bus,
                &evt_tx,
                "tasks.complete",
                serde_json::json!({ "id": id, "done": done }),
            )
            .await;
        }
        Cmd::Feedback(req) => {
            match bus
                .call::<FeedbackSubmitRequest, FeedbackSubmitResponse>(
                    "feedback.submit",
                    &req,
                    vec![],
                )
                .await
            {
                Ok(r) => {
                    let _ = evt_tx.send(Evt::FeedbackOk(r));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::Troubleshoot => {
            run_troubleshoot(&bus, &evt_tx).await;
        }
        Cmd::KillAuditd => {
            #[cfg(windows)]
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/IM", "aos-auditd.exe"])
                .status();
            #[cfg(not(windows))]
            let _ = std::process::Command::new("pkill")
                .args(["-x", "aos-auditd"])
                .status();
            let _ = evt_tx.send(Evt::Status(
                "aos-auditd tué — le superviseur de session doit le redémarrer".into(),
            ));
        }
        Cmd::MigrateModeld { target } => {
            match bus
                .call::<MigrateRequest, MigrateResponse>(
                    "model.migrate",
                    &MigrateRequest {
                        target: target.clone(),
                    },
                    vec![],
                )
                .await
            {
                Ok(r) if r.ok && !r.fallback => {
                    let _ = evt_tx.send(Evt::Status(format!("migrate: {}", r.message)));
                }
                Ok(r) => {
                    let _ = evt_tx.send(Evt::Status(format!(
                        "migrate fallback ({}) — restarting modeld",
                        r.message
                    )));
                    let _ = evt_tx.send(Evt::Error(r.message));
                    // 0.8 fail-closed path
                    handle_restart_modeld(&bus, &evt_tx).await;
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Status(format!(
                        "migrate failed ({e}) — restarting modeld"
                    )));
                    handle_restart_modeld(&bus, &evt_tx).await;
                }
            }
        }
        Cmd::RestartModeld => {
            handle_restart_modeld(&bus, &evt_tx).await;
        }
        Cmd::RefreshConfirms => {
            if let Ok(c) = bus
                .call::<(), Vec<PendingConfirmation>>("confirm.list", &(), vec![])
                .await
            {
                let _ = evt_tx.send(Evt::Confirms(c));
            }
        }
        Cmd::ModelsRefresh => {
            if let Ok(models) = bus
                .call::<(), Vec<ModelInfo>>("model.list", &(), vec![])
                .await
            {
                let _ = evt_tx.send(Evt::Models(models));
            }
        }
        Cmd::ModelLoad { model_id } => {
            match bus
                .call::<LoadRequest, ()>(
                    "model.load",
                    &LoadRequest {
                        model_id: model_id.clone(),
                        profile: "balanced".into(),
                        kv_tokens: 8192,
                    },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    let _ = evt_tx.send(Evt::Status(format!("model load: {model_id}")));
                    if let Ok(models) = bus
                        .call::<(), Vec<ModelInfo>>("model.list", &(), vec![])
                        .await
                    {
                        let _ = evt_tx.send(Evt::Models(models));
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::ModelDownload { model_id } => {
            push_evt(
                &evt_tx,
                &egui_ctx,
                Evt::ModelDownloadStarted {
                    model_id: model_id.clone(),
                },
            );
            let mut child = match tokio::process::Command::new(bin_aos_session())
                .arg("--download-models")
                .arg(&model_id)
                .env("AOS_HOME", aos_home())
                .stderr(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::ModelDownloadFailed {
                            model_id,
                            error: format!("spawn failed: {e}"),
                        },
                    );
                    return;
                }
            };
            let stderr = child.stderr.take();
            let progress_id = model_id.clone();
            let evt_progress = evt_tx.clone();
            let ctx_progress = egui_ctx.clone();
            let read_stderr = tokio::spawn(async move {
                let Some(stderr) = stderr else {
                    return;
                };
                let mut rd = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = rd.next_line().await {
                    if let Some((pct, done, total)) = parse_download_progress_line(&line) {
                        push_evt(
                            &evt_progress,
                            &ctx_progress,
                            Evt::ModelDownloadProgress {
                                model_id: progress_id.clone(),
                                done_bytes: done,
                                total_bytes: total,
                                percent: pct,
                            },
                        );
                    }
                }
            });
            let wait = child.wait().await;
            let _ = read_stderr.await;
            match wait {
                Ok(st) if st.success() => {
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::ModelDownloadFinished {
                            model_id: model_id.clone(),
                        },
                    );
                    if let Ok(models) = bus.call::<(), Vec<ModelInfo>>("model.list", &(), vec![]).await {
                        push_evt(&evt_tx, &egui_ctx, Evt::Models(models));
                    }
                }
                Ok(st) => {
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::ModelDownloadFailed {
                            model_id,
                            error: format!("exit {st}"),
                        },
                    );
                }
                Err(e) => {
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::ModelDownloadFailed {
                            model_id,
                            error: e.to_string(),
                        },
                    );
                }
            }
        }
        Cmd::ModelDownloadHf { url, name } => {
            let label = name.clone().unwrap_or_else(|| {
                url.rsplit('/').next().unwrap_or("huggingface").to_string()
            });
            push_evt(
                &evt_tx,
                &egui_ctx,
                Evt::ModelDownloadStarted {
                    model_id: label.clone(),
                },
            );
            let mut child_cmd = tokio::process::Command::new(bin_aos_session());
            child_cmd
                .arg("--download-hf-url")
                .arg(&url)
                .env("AOS_HOME", aos_home());
            if let Some(n) = name.as_deref().filter(|s| !s.is_empty()) {
                child_cmd.arg("--name").arg(n);
            }
            let mut child = match child_cmd
                .stderr(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::ModelDownloadFailed {
                            model_id: label,
                            error: format!("spawn failed: {e}"),
                        },
                    );
                    return;
                }
            };
            let stderr = child.stderr.take();
            let progress_id = label.clone();
            let evt_progress = evt_tx.clone();
            let ctx_progress = egui_ctx.clone();
            let read_stderr = tokio::spawn(async move {
                let Some(stderr) = stderr else {
                    return;
                };
                let mut rd = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = rd.next_line().await {
                    if let Some((pct, done, total)) = parse_download_progress_line(&line) {
                        push_evt(
                            &evt_progress,
                            &ctx_progress,
                            Evt::ModelDownloadProgress {
                                model_id: progress_id.clone(),
                                done_bytes: done,
                                total_bytes: total,
                                percent: pct,
                            },
                        );
                    }
                }
            });
            let wait = child.wait().await;
            let _ = read_stderr.await;
            match wait {
                Ok(st) if st.success() => {
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::ModelDownloadFinished {
                            model_id: label,
                        },
                    );
                    if let Ok(models) = bus.call::<(), Vec<ModelInfo>>("model.list", &(), vec![]).await {
                        push_evt(&evt_tx, &egui_ctx, Evt::Models(models));
                    }
                }
                Ok(st) => {
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::ModelDownloadFailed {
                            model_id: label,
                            error: format!("exit {st}"),
                        },
                    );
                }
                Err(e) => {
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::ModelDownloadFailed {
                            model_id: label,
                            error: e.to_string(),
                        },
                    );
                }
            }
        }
        Cmd::ModelRemove { model_id } => {
            match tokio::process::Command::new(bin_aos_session())
                .arg("--remove-model")
                .arg(&model_id)
                .env("AOS_HOME", aos_home())
                .output()
                .await
            {
                Ok(out) if out.status.success() => {
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::Status(format!("model removed: {model_id}")),
                    );
                    if let Ok(models) = bus.call::<(), Vec<ModelInfo>>("model.list", &(), vec![]).await {
                        push_evt(&evt_tx, &egui_ctx, Evt::Models(models));
                    }
                }
                Ok(out) => {
                    let detail = String::from_utf8_lossy(&out.stderr);
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::ModelDownloadFailed {
                            model_id,
                            error: detail.trim().to_string(),
                        },
                    );
                }
                Err(e) => {
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::ModelDownloadFailed {
                            model_id,
                            error: format!("spawn failed: {e}"),
                        },
                    );
                }
            }
        }
        Cmd::ModelRedownload { model_id } => {
            push_evt(
                &evt_tx,
                &egui_ctx,
                Evt::ModelDownloadStarted {
                    model_id: model_id.clone(),
                },
            );
            let mut child = match tokio::process::Command::new(bin_aos_session())
                .arg("--redownload-models")
                .arg(&model_id)
                .env("AOS_HOME", aos_home())
                .stderr(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::ModelDownloadFailed {
                            model_id,
                            error: format!("spawn failed: {e}"),
                        },
                    );
                    return;
                }
            };
            let stderr = child.stderr.take();
            let progress_id = model_id.clone();
            let evt_progress = evt_tx.clone();
            let ctx_progress = egui_ctx.clone();
            let read_stderr = tokio::spawn(async move {
                let Some(stderr) = stderr else {
                    return;
                };
                let mut rd = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = rd.next_line().await {
                    if let Some((pct, done, total)) = parse_download_progress_line(&line) {
                        push_evt(
                            &evt_progress,
                            &ctx_progress,
                            Evt::ModelDownloadProgress {
                                model_id: progress_id.clone(),
                                done_bytes: done,
                                total_bytes: total,
                                percent: pct,
                            },
                        );
                    }
                }
            });
            let wait = child.wait().await;
            let _ = read_stderr.await;
            match wait {
                Ok(st) if st.success() => {
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::ModelDownloadFinished {
                            model_id: model_id.clone(),
                        },
                    );
                    if let Ok(models) = bus.call::<(), Vec<ModelInfo>>("model.list", &(), vec![]).await {
                        push_evt(&evt_tx, &egui_ctx, Evt::Models(models));
                    }
                }
                Ok(st) => {
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::ModelDownloadFailed {
                            model_id,
                            error: format!("exit {st}"),
                        },
                    );
                }
                Err(e) => {
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::ModelDownloadFailed {
                            model_id,
                            error: e.to_string(),
                        },
                    );
                }
            }
        }
        Cmd::ProviderList => {
            match bus
                .call::<(), ProviderListResponse>("provider.list", &(), vec![])
                .await
            {
                Ok(r) => {
                    let _ = evt_tx.send(Evt::Providers(r.providers));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("provider.list: {e}")));
                }
            }
        }
        Cmd::ProviderUpsert {
            provider,
            secret_value,
        } => {
            if let Some(val) = secret_value.filter(|s| !s.is_empty()) {
                if let Some(name) = provider.secret_name.clone() {
                    let _ = bus
                        .call::<SecretSetRequest, bool>(
                            "secrets.set",
                            &SecretSetRequest {
                                name,
                                value: val,
                            },
                            vec![],
                        )
                        .await;
                }
            }
            match bus
                .call::<ProviderUpsertRequest, ProviderRecord>(
                    "provider.upsert",
                    &ProviderUpsertRequest { provider },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    if let Ok(r) = bus
                        .call::<(), ProviderListResponse>("provider.list", &(), vec![])
                        .await
                    {
                        let _ = evt_tx.send(Evt::Providers(r.providers));
                    }
                    if let Ok(models) = bus
                        .call::<(), Vec<ModelInfo>>("model.list", &(), vec![])
                        .await
                    {
                        let _ = evt_tx.send(Evt::Models(models));
                    }
                    let _ = evt_tx.send(Evt::Status("provider saved".into()));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("provider.upsert: {e}")));
                }
            }
        }
        Cmd::ProviderRemove { id } => {
            match bus
                .call::<ProviderIdRequest, bool>(
                    "provider.remove",
                    &ProviderIdRequest { id },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    if let Ok(r) = bus
                        .call::<(), ProviderListResponse>("provider.list", &(), vec![])
                        .await
                    {
                        let _ = evt_tx.send(Evt::Providers(r.providers));
                    }
                    if let Ok(models) = bus
                        .call::<(), Vec<ModelInfo>>("model.list", &(), vec![])
                        .await
                    {
                        let _ = evt_tx.send(Evt::Models(models));
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("provider.remove: {e}")));
                }
            }
        }
        Cmd::ProviderTest { id } => {
            match bus
                .call::<ProviderIdRequest, ProviderTestResponse>(
                    "provider.test",
                    &ProviderIdRequest { id },
                    vec![],
                )
                .await
            {
                Ok(r) => {
                    let _ = evt_tx.send(Evt::ProviderTested {
                        ok: r.ok,
                        message: r.message,
                        models: r.models,
                    });
                    if let Ok(list) = bus
                        .call::<(), ProviderListResponse>("provider.list", &(), vec![])
                        .await
                    {
                        let _ = evt_tx.send(Evt::Providers(list.providers));
                    }
                    if let Ok(models) = bus
                        .call::<(), Vec<ModelInfo>>("model.list", &(), vec![])
                        .await
                    {
                        let _ = evt_tx.send(Evt::Models(models));
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("provider.test: {e}")));
                }
            }
        }
        Cmd::MediaImage {
            prompt,
            model_id,
            options,
            output_path,
            enrich_prompt,
            enhance_prompt_chat,
            generation_prompt,
            composition_blocks,
        } => {
            let steps = options.steps.unwrap_or(20);
            let upscale_enabled = options
                .upscale_model
                .as_deref()
                .is_some_and(|s| !s.is_empty());
            let original_prompt = prompt.clone();
            let had_prior = generation_prompt.is_some() || enrich_prompt || enhance_prompt_chat;
            let mut final_prompt = if let Some(gen) = generation_prompt {
                gen
            } else if enrich_prompt {
                run_prompt_enrichment_phase(
                    &bus,
                    &evt_tx,
                    &egui_ctx,
                    steps,
                    &prompt,
                    model_id.as_deref(),
                    PromptEnhanceMode::Json,
                )
                .await
            } else if enhance_prompt_chat {
                run_prompt_enrichment_phase(
                    &bus,
                    &evt_tx,
                    &egui_ctx,
                    steps,
                    &prompt,
                    model_id.as_deref(),
                    PromptEnhanceMode::ChatProse,
                )
                .await
            } else {
                prompt.clone()
            };
            if !composition_blocks.is_empty() {
                final_prompt = crate::image_composition::finalize_prompt_with_layout(
                    &final_prompt,
                    &composition_blocks,
                    model_id.as_deref(),
                    had_prior,
                );
                if final_prompt != original_prompt {
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::MediaImageEnriched {
                            enriched: final_prompt.clone(),
                        },
                    );
                }
            }
            let generation_prompt_sent = if final_prompt != original_prompt {
                Some(final_prompt.clone())
            } else {
                None
            };
            push_evt(
                &evt_tx,
                &egui_ctx,
                Evt::MediaImageStarted {
                    enriching: false,
                    upscaling: false,
                    total_steps: steps,
                },
            );
            let is_video = crate::image_studio::is_video_options(&options);
            let gen_bus = bus.clone();
            let gen_future = tokio::spawn(async move {
                gen_bus
                    .call::<MediaImageGenerateRequest, MediaGenerateResponse>(
                        "media.image.generate",
                        &MediaImageGenerateRequest {
                            prompt: final_prompt,
                            path: output_path,
                            model_id,
                            options,
                            actor: "human:ui".into(),
                            caps: vec![
                                "media.generate".into(),
                                "fs.write:/downloads/**".into(),
                            ],
                            trace_id: String::new(),
                        },
                        vec![],
                    )
                    .await
            });
            let ticker_evt = evt_tx.clone();
            let ticker_ctx = egui_ctx.clone();
            let ticker = tokio::spawn(async move {
                let start = std::time::Instant::now();
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    let elapsed = start.elapsed().as_secs();
                    let (step, total) = read_image_gen_progress_file().unwrap_or((0, steps));
                    let upscaling =
                        upscale_enabled && total > 0 && step >= total && step > 0;
                    push_evt(
                        &ticker_evt,
                        &ticker_ctx,
                        Evt::MediaImageProgress {
                            enriching: false,
                            upscaling,
                            step,
                            total_steps: if total > 0 { total } else { steps },
                            elapsed_secs: elapsed,
                        },
                    );
                }
            });
            let result = gen_future.await;
            ticker.abort();
            let _ = std::fs::remove_file(image_gen_progress_path());
            match result {
                Ok(Ok(r)) => {
                    let gen_prompt = generation_prompt_sent.clone();
                    let media_kind = if is_video { "video" } else { "image" };
                    let meta = crate::image_history::ImageGenMeta::new(
                        r.path.clone(),
                        original_prompt.clone(),
                        gen_prompt.clone(),
                        composition_blocks.clone(),
                        r.model_id.clone(),
                        r.engine.clone(),
                    );
                    if let Err(e) = crate::image_history::write_image_meta(&meta) {
                        eprintln!("image meta write failed: {e}");
                    }
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::MediaOk {
                            kind: media_kind.into(),
                            path: r.path,
                            bytes: r.bytes,
                            engine: r.engine,
                            prompt: original_prompt,
                            generation_prompt: gen_prompt,
                            composition_blocks,
                            model_id: r.model_id,
                        },
                    );
                }
                Ok(Err(e)) => {
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::Error(format!("media.image.generate: {e}")),
                    );
                }
                Err(e) => {
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::Error(format!("media.image.generate: {e}")),
                    );
                }
            }
        }
        Cmd::MediaImageUpscale {
            source_path,
            upscale_model,
            upscale_repeats,
            upscale_tile_size,
        } => {
            push_evt(
                &evt_tx,
                &egui_ctx,
                Evt::MediaImageStarted {
                    enriching: false,
                    upscaling: true,
                    total_steps: 0,
                },
            );
            let ticker_evt = evt_tx.clone();
            let ticker_ctx = egui_ctx.clone();
            let ticker = tokio::spawn(async move {
                let start = std::time::Instant::now();
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    push_evt(
                        &ticker_evt,
                        &ticker_ctx,
                        Evt::MediaImageProgress {
                            enriching: false,
                            upscaling: true,
                            step: 0,
                            total_steps: 0,
                            elapsed_secs: start.elapsed().as_secs(),
                        },
                    );
                }
            });
            let result = bus
                .call::<MediaImageUpscaleRequest, MediaGenerateResponse>(
                    "media.image.upscale",
                    &MediaImageUpscaleRequest {
                        source_path: source_path.clone(),
                        output_path: None,
                        upscale_model,
                        upscale_repeats: Some(upscale_repeats),
                        upscale_tile_size: Some(upscale_tile_size),
                        actor: "human:ui".into(),
                        caps: vec![
                            "media.generate".into(),
                            "fs.write:/downloads/**".into(),
                        ],
                        trace_id: String::new(),
                    },
                    vec![],
                )
                .await;
            ticker.abort();
            match result {
                Ok(r) => {
                    let _ = crate::image_history::clone_meta_for_new_path(
                        &source_path,
                        &r.path,
                        &r.engine,
                    );
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::MediaOk {
                            kind: "image".into(),
                            path: r.path,
                            bytes: r.bytes,
                            engine: r.engine,
                            prompt: String::new(),
                            generation_prompt: None,
                            composition_blocks: Vec::new(),
                            model_id: r.model_id,
                        },
                    );
                }
                Err(e) => {
                    push_evt(
                        &evt_tx,
                        &egui_ctx,
                        Evt::Error(format!("media.image.upscale: {e}")),
                    );
                }
            }
        }
        Cmd::MediaAudio {
            text,
            model_id,
            options,
        } => {
            match bus
                .call::<MediaAudioGenerateRequest, MediaGenerateResponse>(
                    "media.audio.generate",
                    &MediaAudioGenerateRequest {
                        text,
                        path: None,
                        model_id,
                        options,
                        actor: "human:ui".into(),
                        caps: vec!["media.generate".into(), "fs.write:/downloads/**".into()],
                        trace_id: String::new(),
                    },
                    vec![],
                )
                .await
            {
                Ok(r) => {
                    let _ = evt_tx.send(Evt::MediaOk {
                        kind: "audio".into(),
                        path: r.path,
                        bytes: r.bytes,
                        engine: r.engine,
                        prompt: String::new(),
                        generation_prompt: None,
                        composition_blocks: Vec::new(),
                        model_id: r.model_id,
                    });
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("media.audio.generate: {e}")));
                }
            }
        }
        Cmd::SessionSetModel {
            session_id,
            model_id,
        } => {
            match bus
                .call::<ChatSessionSetModelRequest, ChatSessionMeta>(
                    "chat.session.set_model",
                    &ChatSessionSetModelRequest {
                        session_id,
                        model_id,
                    },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    if let Ok(list) = bus
                        .call::<(), Vec<ChatSessionMeta>>("chat.session.list", &(), vec![])
                        .await
                    {
                        let _ = evt_tx.send(Evt::Sessions(list));
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::SessionAppend {
            session_id,
            role,
            content,
            attachments,
        } => {
            let _ = bus
                .call::<ChatSessionAppendRequest, aos_proto::ChatSessionMessage>(
                    "chat.session.append",
                    &ChatSessionAppendRequest {
                        session_id,
                        role,
                        content,
                        attachments,
                        speaker_id: None,
                        speaker_name: None,
                    },
                    vec![],
                )
                .await;
        }
        Cmd::SessionSetMode { session_id, mode } => {
            match bus
                .call::<ChatSessionSetModeRequest, ChatSessionMeta>(
                    "chat.session.set_mode",
                    &ChatSessionSetModeRequest { session_id: session_id.clone(), mode },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    refresh_sessions(&bus, &evt_tx).await;
                    load_session(&bus, &evt_tx, &session_id).await;
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::SessionMembersAdd { session_id, member } => {
            match bus
                .call::<ChatSessionMembersAddRequest, ChatSessionMeta>(
                    "chat.session.members.add",
                    &ChatSessionMembersAddRequest {
                        session_id: session_id.clone(),
                        member,
                    },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    refresh_sessions(&bus, &evt_tx).await;
                    load_session(&bus, &evt_tx, &session_id).await;
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::SessionMembersRemove {
            session_id,
            agent_id,
        } => {
            match bus
                .call::<ChatSessionMembersRemoveRequest, ChatSessionMeta>(
                    "chat.session.members.remove",
                    &ChatSessionMembersRemoveRequest {
                        session_id: session_id.clone(),
                        agent_id: agent_id.clone(),
                    },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    refresh_sessions(&bus, &evt_tx).await;
                    load_session(&bus, &evt_tx, &session_id).await;
                    let _ = evt_tx.send(Evt::Status(format!(
                        "retiré du salon : {agent_id}"
                    )));
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::RoomAddPersona {
            session_id,
            persona_id,
            model_id,
        } => {
            let Some(persona) = chat_room::persona_by_id(&persona_id) else {
                let _ = evt_tx.send(Evt::Error(format!("persona inconnue: {persona_id}")));
                return;
            };
            let canonical_name = persona.display_name.to_string();
            let mut req = aos_agent::room_personas::persona_create_request(persona, model_id);
            req.session_id = Some(session_id.clone());
            match bus
                .call::<AgentCreateRequest, aos_proto::AgentCreateResponse>(
                    agent_intents::CREATE,
                    &req,
                    vec![],
                )
                .await
            {
                Ok(r) => {
                    let member = ChatRoomMember {
                        agent_id: r.agent_id,
                        display_name: canonical_name.clone(),
                        persona_id: Some(persona.id.to_string()),
                        joined_ms: room_joined_ms(),
                    };
                    match bus
                        .call::<ChatSessionMembersAddRequest, ChatSessionMeta>(
                            "chat.session.members.add",
                            &ChatSessionMembersAddRequest {
                                session_id: session_id.clone(),
                                member,
                            },
                            vec![],
                        )
                        .await
                    {
                        Ok(_) => {
                            refresh_sessions(&bus, &evt_tx).await;
                            load_session(&bus, &evt_tx, &session_id).await;
                            let _ = evt_tx.send(Evt::Status(format!(
                                "« {canonical_name} » ajouté au salon"
                            )));
                        }
                        Err(e) => {
                            let _ = evt_tx.send(Evt::Error(e.to_string()));
                        }
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::RoomTurn {
            session_id,
            content,
            images,
        } => {
            match bus
                .call::<ChatSessionRoomTurnRequest, ChatSessionRoomTurnResponse>(
                    "chat.session.room.turn",
                    &ChatSessionRoomTurnRequest {
                        session_id: session_id.clone(),
                        content,
                        images,
                    },
                    vec![],
                )
                .await
            {
                Ok(resp) => {
                    load_session(&bus, &evt_tx, &session_id).await;
                    let _ = evt_tx.send(Evt::RoomTurnDone {
                        session_id,
                        agent_turns: resp.agent_turns,
                        cancelled: resp.cancelled,
                    });
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::RoomTurnCancel { session_id } => {
            match bus
                .call::<ChatSessionRoomTurnCancelRequest, bool>(
                    "chat.session.room.turn.cancel",
                    &ChatSessionRoomTurnCancelRequest {
                        session_id: session_id.clone(),
                    },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    let _ = evt_tx.send(Evt::ChatCancelled {
                        session_id,
                    });
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::CanvasSetOpen { session_id, open } => {
            match bus
                .call::<aos_proto::CanvasSetOpenRequest, ChatSessionMeta>(
                    "canvas.set_open",
                    &aos_proto::CanvasSetOpenRequest {
                        session_id: session_id.clone(),
                        open,
                    },
                    vec![],
                )
                .await
            {
                Ok(meta) => {
                    let _ = evt_tx.send(Evt::CanvasMeta(meta));
                    if open {
                        if let Ok(resp) = bus
                            .call::<aos_proto::CanvasGetRequest, aos_proto::CanvasGetResponse>(
                                "canvas.get",
                                &aos_proto::CanvasGetRequest {
                                    session_id: session_id.clone(),
                                    after_seq: None,
                                },
                                vec![],
                            )
                            .await
                        {
                            let _ = evt_tx.send(Evt::CanvasSnapshot {
                                session_id: resp.session_id,
                                canvas_open: resp.canvas_open,
                                next_seq: resp.next_seq,
                                ops: resp.ops,
                                pen: resp.pen,
                                delta: false,
                                canvas_seeing: Some(resp.canvas_seeing),
                            });
                        }
                    }
                    refresh_sessions(&bus, &evt_tx).await;
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::CanvasApply {
            session_id,
            author_id,
            op,
        } => {
            match bus
                .call::<aos_proto::CanvasApplyRequest, aos_proto::CanvasApplyResponse>(
                    "canvas.apply",
                    &aos_proto::CanvasApplyRequest {
                        session_id: session_id.clone(),
                        author_id,
                        op,
                    },
                    vec![],
                )
                .await
            {
                Ok(resp) => {
                    let _ = evt_tx.send(Evt::CanvasSnapshot {
                        session_id: session_id.clone(),
                        canvas_open: resp.canvas_open,
                        next_seq: resp.doc.next_seq,
                        ops: resp.doc.ops,
                        pen: resp.doc.pen,
                        delta: false,
                        canvas_seeing: None,
                    });
                    if resp.canvas_open {
                        refresh_sessions(&bus, &evt_tx).await;
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::CanvasSetStyle {
            session_id,
            color,
            width,
        } => {
            match bus
                .call::<aos_proto::CanvasSetStyleRequest, aos_proto::CanvasSetStyleResponse>(
                    "canvas.set_style",
                    &aos_proto::CanvasSetStyleRequest {
                        session_id: session_id.clone(),
                        color,
                        width,
                    },
                    vec![],
                )
                .await
            {
                Ok(resp) => {
                    let _ = evt_tx.send(Evt::CanvasSnapshot {
                        session_id: session_id.clone(),
                        canvas_open: resp.canvas_open,
                        next_seq: resp.doc.next_seq,
                        ops: resp.doc.ops,
                        pen: resp.pen,
                        delta: false,
                        canvas_seeing: None,
                    });
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::CanvasPoll {
            session_id,
            after_seq,
        } => {
            match bus
                .call::<aos_proto::CanvasGetRequest, aos_proto::CanvasGetResponse>(
                    "canvas.get",
                    &aos_proto::CanvasGetRequest {
                        session_id,
                        after_seq,
                    },
                    vec![],
                )
                .await
            {
                Ok(resp) => {
                    let delta = after_seq.is_some();
                    let _ = evt_tx.send(Evt::CanvasSnapshot {
                        session_id: resp.session_id,
                        canvas_open: resp.canvas_open,
                        next_seq: resp.next_seq,
                        ops: resp.ops,
                        pen: resp.pen,
                        delta,
                        canvas_seeing: Some(resp.canvas_seeing),
                    });
                }
                Err(_) => {}
            }
        }
        Cmd::CanvasSetAspect { session_id, aspect } => {
            match bus
                .call::<aos_proto::CanvasSetAspectRequest, ChatSessionMeta>(
                    "canvas.set_aspect",
                    &aos_proto::CanvasSetAspectRequest {
                        session_id: session_id.clone(),
                        aspect,
                    },
                    vec![],
                )
                .await
            {
                Ok(meta) => {
                    let _ = evt_tx.send(Evt::CanvasMeta(meta));
                    refresh_sessions(&bus, &evt_tx).await;
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
        Cmd::CanvasExport { session_id, aspect } => {
            let (width, height) = aspect.export_dimensions(1024);
            match bus
                .call::<aos_proto::CanvasExportRequest, serde_json::Value>(
                    "canvas.export",
                    &aos_proto::CanvasExportRequest {
                        session_id: session_id.clone(),
                        path: None,
                        width: Some(width),
                        height: Some(height),
                    },
                    vec![],
                )
                .await
            {
                Ok(v) => {
                    let path = v
                        .get("path")
                        .and_then(|p| p.as_str())
                        .unwrap_or("")
                        .to_string();
                    if !path.is_empty() {
                        let att = ChatAttachment::Image {
                            path: path.clone(),
                            prompt: "canvas export".into(),
                        };
                        let _ = bus
                            .call::<ChatSessionAppendRequest, aos_proto::ChatSessionMessage>(
                                "chat.session.append",
                                &ChatSessionAppendRequest {
                                    session_id: session_id.clone(),
                                    role: "assistant".into(),
                                    content: format!("Canvas exporté : {path}"),
                                    attachments: vec![att],
                                    speaker_id: None,
                                    speaker_name: None,
                                },
                                vec![],
                            )
                            .await;
                        let _ = evt_tx.send(Evt::CanvasExported { path, session_id });
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(e.to_string()));
                }
            }
        }
    }
}

pub(crate) fn sweep_tz_offset_minutes() -> i32 {
    if let Ok(out) = std::process::Command::new("date").args(["+%z"]).output() {
        if out.status.success() {
            if let Ok(s) = String::from_utf8(out.stdout) {
                return parse_tz_offset_minutes(s.trim()).unwrap_or(0);
            }
        }
    }
    0
}

fn parse_tz_offset_minutes(raw: &str) -> Option<i32> {
    let s = raw.trim();
    if s.len() < 3 {
        return None;
    }
    let sign = match s.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let digits: String = s.chars().skip(1).filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 3 {
        return None;
    }
    let hours: i32 = digits[..digits.len().saturating_sub(2)]
        .parse()
        .ok()?;
    let mins: i32 = digits[digits.len().saturating_sub(2)..]
        .parse()
        .ok()?;
    Some(sign * (hours * 60 + mins))
}

fn room_joined_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

pub fn joined_ms_now() -> u64 {
    room_joined_ms()
}

async fn refresh_sessions(bus: &Arc<BusClient>, evt_tx: &Sender<Evt>) {
    if let Ok(list) = bus
        .call::<(), Vec<ChatSessionMeta>>("chat.session.list", &(), vec![])
        .await
    {
        let _ = evt_tx.send(Evt::Sessions(list));
    }
}

fn image_gen_progress_path() -> std::path::PathBuf {
    crate::os_open::aos_home().join("var/run/image-gen-progress.json")
}

fn read_image_gen_progress_file() -> Option<(u32, u32)> {
    let raw = std::fs::read_to_string(image_gen_progress_path()).ok()?;
    let v = serde_json::from_str::<serde_json::Value>(&raw).ok()?;
    let step = v.get("step")?.as_u64()? as u32;
    let total = v.get("total")?.as_u64()? as u32;
    Some((step, total))
}

fn parse_download_progress_line(line: &str) -> Option<(u8, u64, u64)> {
    // Expected from aos-session bootstrap:
    // "[aos-session]   35% (123/456)"
    let pct_pos = line.find('%')?;
    let pct_start = line[..pct_pos].rfind(' ')?;
    let percent = line[pct_start..pct_pos].trim().parse::<u8>().ok()?;
    let open = line.find('(')?;
    let slash = line[open + 1..].find('/')? + open + 1;
    let close = line[slash + 1..].find(')')? + slash + 1;
    let done = line[open + 1..slash].trim().parse::<u64>().ok()?;
    let total = line[slash + 1..close].trim().parse::<u64>().ok()?;
    Some((percent.min(100), done, total))
}

async fn handle_restart_modeld(bus: &BusClient, evt_tx: &Sender<Evt>) {
    let _ = bus
        .call::<CancelRequest, bool>(
            "model.cancel",
            &CancelRequest { inference_id: 0 },
            vec![],
        )
        .await;
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/IM", "aos-modeld.exe"])
            .status();
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/IM", "aos-modeld-cpu.exe"])
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("pkill")
            .args(["-x", "aos-modeld"])
            .status();
        let _ = std::process::Command::new("pkill")
            .args(["-x", "aos-modeld-cpu"])
            .status();
    }
    let _ = evt_tx.send(Evt::Status(
        "modeld restarting with current inference setting".into(),
    ));
}

async fn run_prompt_enrichment_phase(
    bus: &BusClient,
    evt_tx: &Sender<Evt>,
    egui_ctx: &egui::Context,
    steps: u32,
    prompt: &str,
    model_id: Option<&str>,
    mode: PromptEnhanceMode,
) -> String {
    push_evt(
        evt_tx,
        egui_ctx,
        Evt::MediaImageStarted {
            enriching: true,
            upscaling: false,
            total_steps: steps,
        },
    );
    let enrich_ticker_evt = evt_tx.clone();
    let enrich_ticker_ctx = egui_ctx.clone();
    let enrich_ticker = tokio::spawn(async move {
        let start = std::time::Instant::now();
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            push_evt(
                &enrich_ticker_evt,
                &enrich_ticker_ctx,
                Evt::MediaImageProgress {
                    enriching: true,
                    upscaling: false,
                    step: 0,
                    total_steps: steps,
                    elapsed_secs: start.elapsed().as_secs(),
                },
            );
        }
    });
    let out = match mode {
        PromptEnhanceMode::Json => enrich_image_prompt(bus, evt_tx, prompt, model_id).await,
        PromptEnhanceMode::ChatProse => enhance_image_prompt_chat(bus, evt_tx, prompt).await,
    };
    enrich_ticker.abort();
    match out {
        Ok(text) => {
            push_evt(
                evt_tx,
                egui_ctx,
                Evt::MediaImageEnriched {
                    enriched: text.clone(),
                },
            );
            text
        }
        Err(e) => {
            let label = match mode {
                PromptEnhanceMode::Json => "JSON enrichment",
                PromptEnhanceMode::ChatProse => "prompt enhancement",
            };
            push_evt(
                evt_tx,
                egui_ctx,
                Evt::Error(format!("{label} failed, using raw prompt: {e}")),
            );
            prompt.to_string()
        }
    }
}

#[derive(Clone, Copy)]
enum PromptEnhanceMode {
    Json,
    ChatProse,
}

async fn enhance_image_prompt_chat(
    bus: &BusClient,
    evt_tx: &Sender<Evt>,
    user_prompt: &str,
) -> Result<String, String> {
    use crate::image_prompt::CHAT_ENHANCE_SYSTEM_PROMPT;
    let out = infer_llm_rewrite(
        bus,
        evt_tx,
        "Chat",
        CHAT_ENHANCE_SYSTEM_PROMPT,
        user_prompt,
    )
    .await?;
    Ok(normalize_prose_prompt(&out))
}

fn normalize_prose_prompt(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    if s.starts_with("```") {
        if let Some(rest) = s.strip_prefix("```") {
            let rest = rest.trim_start_matches("text").trim_start();
            if let Some(idx) = rest.rfind("```") {
                s = rest[..idx].trim().to_string();
            }
        }
    }
    if s.len() >= 2 {
        let bytes = s.as_bytes();
        if (bytes[0] == b'"' && bytes[s.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[s.len() - 1] == b'\'')
        {
            s = s[1..s.len() - 1].to_string();
        }
    }
    s.trim().to_string()
}

async fn infer_llm_rewrite(
    bus: &BusClient,
    evt_tx: &Sender<Evt>,
    label: &str,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, String> {
    use aos_proto::{ChatMessage, InferParams, InferRequest, TokenEvent};
    let req = InferRequest {
        model_id: None,
        messages: vec![
            ChatMessage {
                role: "system".into(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".into(),
                content: user_prompt.to_string(),
            },
        ],
        params: InferParams {
            max_tokens: 2048,
            temperature: 0.7,
            top_p: 0.95,
            seed: None,
        },
        priority: 2,
        data_refs: vec![],
        images: vec![],
        routing: None,
    };
    let mut rx = bus
        .call_stream::<InferRequest, TokenEvent>("model.infer", &req, vec![])
        .await
        .map_err(|e| e.to_string())?;
    let mut out = String::new();
    let mut token_count: u32 = 0;
    while let Some(ev) = rx.recv().await {
        match ev {
            Ok(TokenEvent::Started { .. }) => {
                let _ = evt_tx.send(Evt::Status(format!(
                    "{label}: LLM rewriting prompt…"
                )));
            }
            Ok(TokenEvent::Queued { position }) => {
                let _ = evt_tx.send(Evt::Status(format!(
                    "{label}: waiting in queue (position {position})…"
                )));
            }
            Ok(TokenEvent::Delta { text }) => {
                out.push_str(&text);
                token_count += 1;
                if token_count % 8 == 0 {
                    let preview = if out.len() > 80 {
                        format!("…{}", &out[out.len() - 80..])
                    } else {
                        out.clone()
                    };
                    let _ = evt_tx.send(Evt::Status(format!(
                        "{label}: rewriting ({token_count} tok) {preview}"
                    )));
                }
            }
            Ok(TokenEvent::Done { tok_s, .. }) => {
                let _ = evt_tx.send(Evt::Status(format!(
                    "{label}: prompt ready ({token_count} tok, {tok_s:.1} tok/s)"
                )));
                break;
            }
            Ok(TokenEvent::Error { message }) => return Err(message),
            _ => {}
        }
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        return Err("LLM returned empty response".into());
    }
    Ok(trimmed.to_string())
}

async fn enrich_image_prompt(
    bus: &BusClient,
    evt_tx: &Sender<Evt>,
    user_prompt: &str,
    model_id: Option<&str>,
) -> Result<String, String> {
    use crate::image_prompt::{
        enrichment_status_label, enrichment_system_prompt, prompt_enrichment_kind,
    };
    let kind = prompt_enrichment_kind(model_id.unwrap_or("")).ok_or_else(|| {
        "prompt enrichment not supported for this model".to_string()
    })?;
    let label = enrichment_status_label(kind);
    let system_prompt = enrichment_system_prompt(kind);
    let out = infer_llm_rewrite(bus, evt_tx, label, system_prompt, user_prompt).await?;
    let json_str = if let Some(start) = out.find('{') {
        if let Some(end) = out.rfind('}') {
            &out[start..=end]
        } else {
            out.as_str()
        }
    } else {
        out.as_str()
    };
    if serde_json::from_str::<serde_json::Value>(json_str).is_err() {
        return Err(format!(
            "LLM output is not valid JSON: {}",
            &json_str[..json_str.len().min(200)]
        ));
    }
    Ok(json_str.to_string())
}
