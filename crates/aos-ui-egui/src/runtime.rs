//! Background bus runtime: poll + `handle_cmd`.

use crate::cmd::{Cmd, Evt};
use crate::os_open::aos_home;
use crate::{
    agent_id_cmd, agent_panel, chat_delegate_agent_spec, chrono_like_stamp, invoke_module_bind,
    invoke_module_tool, invoke_notes, invoke_tasks, load_module_ui, load_session, run_troubleshoot,
    spawn_chat_delegate_agent, CHAT_AGENT_MAX_SUBAGENTS,
};
use aos_agent::intents as agent_intents;
use aos_agent::schedule::{
    ScheduleCreateRequest, ScheduleEntry, ScheduleIdRequest, ScheduleListResponse,
};
use aos_ipc::BusClient;
use aos_proto::{
    AgentCreateRequest, AgentGoal, AgentIdRequest, AgentInfo, AgentPromptOptimizeRequest,
    AgentPromptOptimizeResponse, AgentSteerRequest, AgentTrace, AuditEvent, AuditQueryRequest,
    CapInfo, CapListRequest, CapRevokeRequest, ChatAttachment, ChatMessage, ChatSessionAppendRequest,
    ChatSessionCreateRequest, ChatSessionIdRequest, ChatSessionMeta,
    ChatSessionRenameRequest, ChatSessionSetModelRequest, ConfirmResponseRequest,
    FeedbackSubmitRequest, FeedbackSubmitResponse, FilesGenerateRequest, FilesGenerateResponse,
    InferParams, InferRequest, McpServerInfo, MemContextRequest, MemContextResponse,
    MemEpisodicDeleteRequest, MemExtractRequest, MemExtractResponse, MemHit, MemListRequest,
    MemRememberResponse, MemUpdateRequest, MemUserRecallRequest, MemUserRememberRequest,
    MemWorkingRequest, LoadRequest, ModelInfo, ModelState, ModuleCatalogue,
    ModuleInfo, ModuleInstallRequest,
    ModuleUninstallRequest, CancelRequest, MediaAudioGenerateRequest, MediaGenerateResponse,
    MediaImageGenerateRequest, NetFetchRequest, NetFetchResponse, NetModeRequest,
    PendingConfirmation, ProviderIdRequest, ProviderListResponse, ProviderRecord,
    ProviderTestResponse, ProviderUpsertRequest, SecretListRequest, SecretListResponse,
    SecretSetRequest, SetRoutingRequest, SkillInfo, SystemMetrics, TokenEvent, WebBrowseRequest,
    WebBrowseResponse, WebSearchRequest, WebSearchResponse, AgentState,
    CHAT_DELEGATION_PROMPT, SYSTEM_ASSISTANT_PROMPT,
};
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

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
            handle_cmd(bus, evt_tx, cmd).await;
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

async fn handle_cmd(bus: Arc<BusClient>, evt_tx: Sender<Evt>, cmd: Cmd) {
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
                        let _ = evt_tx.send(Evt::SessionLoaded {
                            id: m.id,
                            messages: vec![],
                        });
                    }
                    Err(e) => {
                        let _ = evt_tx.send(Evt::Error(format!("session create: {e}")));
                    }
                }
            } else {
                let id = list[0].id.clone();
                let _ = evt_tx.send(Evt::Sessions(list));
                load_session(&bus, &evt_tx, &id).await;
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
                    let _ = evt_tx.send(Evt::SessionLoaded {
                        id: m.id,
                        messages: vec![],
                    });
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
                        let _ = evt_tx.send(Evt::SessionLoaded {
                            id: m.id,
                            messages: vec![],
                        });
                    }
                    Err(e) => {
                        let _ = evt_tx.send(Evt::Error(e.to_string()));
                    }
                }
            } else {
                let id = list[0].id.clone();
                let _ = evt_tx.send(Evt::Sessions(list));
                load_session(&bus, &evt_tx, &id).await;
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
            auto_remember,
            max_steps,
            routing,
        } => {
            let _ = evt_tx.send(Evt::Status(
                "assistant : génération en cours…".into(),
            ));
            let _ = bus
                .call::<ChatSessionAppendRequest, aos_proto::ChatSessionMessage>(
                    "chat.session.append",
                    &ChatSessionAppendRequest {
                        session_id: session_id.clone(),
                        role: "user".into(),
                        content: user_text.clone(),
                        attachments: vec![],
                    },
                    vec![],
                )
                .await;

            let mem_block = bus
                .call::<MemContextRequest, MemContextResponse>(
                    "mem.context",
                    &MemContextRequest {
                        session_id: Some(session_id.clone()),
                        query: user_text.clone(),
                        k: 5,
                    },
                    vec![],
                )
                .await
                .ok()
                .map(|r| r.prompt_block)
                .unwrap_or_default();

            let mut system = SYSTEM_ASSISTANT_PROMPT.to_string();
            system.push_str(CHAT_DELEGATION_PROMPT);
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
            let req = InferRequest {
                model_id: model_id.clone(),
                messages,
                params: InferParams {
                    max_tokens: 512,
                    ..Default::default()
                },
                priority: 8,
                data_refs: vec![],
                routing: Some(routing),
            };
            let sid = session_id.clone();
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
                                    let _ = evt_tx.send(Evt::InferStarted { inference_id });
                                }
                                Ok(TokenEvent::Delta { text }) => {
                                    full.push_str(&text);
                                    let _ = evt_tx.send(Evt::Delta(text));
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
                            chat_delegate_agent_spec(&user_text, &full)
                        {
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
                            )
                            .await;
                            return;
                        }

                        let display = agent_panel::format_assistant_display(&full);
                        let _ = bus
                            .call::<ChatSessionAppendRequest, aos_proto::ChatSessionMessage>(
                                "chat.session.append",
                                &ChatSessionAppendRequest {
                                    session_id: sid.clone(),
                                    role: "assistant".into(),
                                    content: display.clone(),
                                    attachments: vec![],
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
        Cmd::ChatCancel { inference_id } => {
            match bus
                .call::<CancelRequest, bool>(
                    "model.cancel",
                    &CancelRequest { inference_id },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    let _ = evt_tx.send(Evt::ChatCancelled);
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
                .call::<SetRoutingRequest, bool>(
                    "model.set_routing",
                    &SetRoutingRequest { mode: mode.clone() },
                    vec![],
                )
                .await
            {
                Ok(_) => {
                    let _ = evt_tx.send(Evt::Status(format!("routing → {mode}")));
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
        } => {
            let mut req = AgentCreateRequest::simple(task.clone());
            req.system_prompt = system_prompt;
            req.skills = skills;
            req.tools = tools;
            req.mcp_servers = mcp_servers;
            req.documents = documents;
            req.optimize_prompt = optimize_prompt;
            req.session_id = session_id.clone();
            req.goal = Some(AgentGoal {
                statement: task.clone(),
                success_criteria: vec![],
                max_steps,
                max_subagents: CHAT_AGENT_MAX_SUBAGENTS,
                timeout_secs,
            });
            req.model_id = model_id;
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
                    if let Some(sid) = session_id {
                        let ack = format!("Agent {} lancé en fond.", r.agent_id);
                        let att = ChatAttachment::AgentRef {
                            agent_id: r.agent_id.clone(),
                            title: task.clone(),
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
                                },
                                vec![],
                            )
                            .await;
                        let _ = evt_tx.send(Evt::AgentSpawned {
                            session_id: sid,
                            agent_id: r.agent_id.clone(),
                            title: task,
                            origin,
                            ack,
                        });
                    } else {
                        let _ = evt_tx.send(Evt::Status(format!("agent créé : {}", r.agent_id)));
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
        } => {
            match bus
                .call::<ScheduleCreateRequest, ScheduleEntry>(
                    agent_intents::SCHEDULE_CREATE,
                    &ScheduleCreateRequest {
                        goal,
                        interval_secs,
                        model_id: None,
                    },
                    vec![],
                )
                .await
            {
                Ok(e) => {
                    let _ = evt_tx.send(Evt::Status(format!(
                        "schedule créé {} ({}s)",
                        e.id, e.interval_secs
                    )));
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
                Ok(_) => {
                    let _ = evt_tx.send(Evt::Status(format!("schedule annulé {id}")));
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
        Cmd::RestartModeld => {
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
        Cmd::MediaImage { prompt } => {
            match bus
                .call::<MediaImageGenerateRequest, MediaGenerateResponse>(
                    "media.image.generate",
                    &MediaImageGenerateRequest {
                        prompt,
                        path: None,
                        model_id: None,
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
                        kind: "image".into(),
                        path: r.path,
                        bytes: r.bytes,
                        engine: r.engine,
                    });
                }
                Err(e) => {
                    let _ = evt_tx.send(Evt::Error(format!("media.image.generate: {e}")));
                }
            }
        }
        Cmd::MediaAudio { text } => {
            match bus
                .call::<MediaAudioGenerateRequest, MediaGenerateResponse>(
                    "media.audio.generate",
                    &MediaAudioGenerateRequest {
                        text,
                        path: None,
                        model_id: None,
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
                    },
                    vec![],
                )
                .await;
        }
    }
}
