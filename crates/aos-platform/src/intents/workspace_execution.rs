//! Capability and confirmation boundary for Dev Assistant host commands.
use crate::{subsystem::PlatformSubsystem, workspace_process};
use aos_ipc::{msg::Status, BusService};
use aos_proto::{
    workspace::{GitCommitRequest, WorkspaceTaskRequest},
    AuditAppendRequest,
};
use serde_json::{json, Value};
use std::sync::Arc;

pub fn register(svc: &mut BusService, sub: Arc<PlatformSubsystem>) {
    for intent in [
        "workspace.inspect",
        "workspace.inventory",
        "workspace.read",
        "refactor.create",
        "refactor.submit",
        "refactor.review",
        "refactor.apply",
        "refactor.undo",
        "process.run",
        "git.commit",
        "debug.start",
        "debug.request",
        "debug.stop",
        "studio.memory",
        "studio.memory.update",
        "studio.project",
        "studio.lsp",
        "studio.reindex",
    ] {
        let sub = sub.clone();
        svc.on(intent, move |ctx| {
            let sub = sub.clone();
            async move {
                let args = match ctx.payload::<Value>() {
                    Ok(value) => value,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(Status::BadRequest, "invalid workspace request")
                            .await;
                        return;
                    }
                };
                let actor = ctx.intent.from.clone();
                let caps = sub
                    .granted_caps
                    .lock()
                    .unwrap()
                    .get(&actor)
                    .cloned()
                    .unwrap_or_default();
                // Transport capabilities are the same explicit grants used by other
                // bus clients; caller-supplied actor/caps in the payload are ignored.
                let caps = if ctx.intent.caps.is_empty() {
                    caps
                } else {
                    ctx.intent.caps.clone()
                };
                let trace = format!(
                    "workspace-{}-{}",
                    std::process::id(),
                    ctx.intent.correlation_id
                );
                match dispatch(&sub, intent, args, &actor, &caps, &trace).await {
                    Ok(value) => {
                        let _ = ctx.respond(Status::Ok, &value).await;
                    }
                    Err(error) => {
                        let _ = ctx.respond_error(Status::PermissionDenied, &error).await;
                    }
                }
            }
        });
    }
    svc.on("net.status", move |ctx| {
        let sub = sub.clone();
        async move {
            let mode = match sub.net.lock().unwrap().mode() {
                crate::net::NetMode::Online => "online",
                crate::net::NetMode::OfflineStrict => "offline_strict",
            };
            let _ = ctx
                .respond(
                    Status::Ok,
                    &json!({"mode":mode,"host_mcp_allowed":mode=="online"}),
                )
                .await;
        }
    });
}

pub async fn dispatch(
    sub: &PlatformSubsystem,
    intent: &str,
    mut args: Value,
    actor: &str,
    caps: &[String],
    trace: &str,
) -> Result<Value, String> {
    if matches!(
        intent,
        "workspace.inspect" | "workspace.inventory" | "workspace.read"
    ) {
        return crate::intents::workspace_inventory::dispatch(
            sub, intent, args, actor, caps, trace,
        )
        .await;
    }
    if intent.starts_with("refactor.") {
        return crate::intents::workspace_refactor::dispatch(sub, intent, args, actor, caps, trace)
            .await;
    }
    if intent.starts_with("debug.") {
        return crate::intents::workspace_debug::dispatch(sub, intent, args, actor, caps, trace)
            .await;
    }
    if intent.starts_with("studio.") {
        return dispatch_studio(sub, intent, args, actor, caps, trace).await;
    }
    if !args.is_object() {
        return Err("arguments must be an object".into());
    }
    args["actor"] = json!(actor);
    args["caps"] = json!(caps);
    args["trace_id"] = json!(trace);
    let root = args["root"].as_str().unwrap_or("").to_owned();
    let prepared = {
        let mgr = sub.workspaces.lock().unwrap();
        match intent {
            "process.run" => serde_json::from_value::<WorkspaceTaskRequest>(args)
                .map_err(|e| e.to_string())
                .and_then(|req| {
                    workspace_process::prepare_task(&mgr, &req, &sub.workspace_process_policy)
                }),
            "git.commit" => serde_json::from_value::<GitCommitRequest>(args)
                .map_err(|e| e.to_string())
                .and_then(|req| workspace_process::prepare_commit(&mgr, &req)),
            _ => Err("unsupported workspace execution intent".into()),
        }
    };
    let command = match prepared {
        Ok(command) => command,
        Err(error) => {
            sub.audit(AuditAppendRequest {
                trace_id: trace.into(),
                actor: actor.into(),
                action: intent.into(),
                target: root,
                detail: json!({"ok":false,"error":error}),
            });
            return Err(error);
        }
    };
    let reason=format!("Autoriser {} {:?} dans /host/{} ? Cette commande utilise les droits de votre compte sur l'hÃƒÂ´te.",
        command.program.file_name().unwrap_or_default().to_string_lossy(),command.args,command.workspace_id);
    let (_, answer) = sub
        .confirm
        .ask(actor.into(), intent.into(), root.clone(), reason, Some(120))
        .await;
    if !answer.await.map(|result| result.approved).unwrap_or(false) {
        sub.audit(AuditAppendRequest {
            trace_id: trace.into(),
            actor: actor.into(),
            action: intent.into(),
            target: root,
            detail: json!({"ok":false,"error":"confirmation denied or expired"}),
        });
        return Err("confirmation denied or expired".into());
    }
    let started = std::time::Instant::now();
    let result = workspace_process::execute(command).await;
    sub.audit(AuditAppendRequest {trace_id:trace.into(),actor:actor.into(),action:intent.into(),target:root,
        detail:json!({"ok":result.as_ref().map(|r|r.ok).unwrap_or(false),
            "exit_code":result.as_ref().ok().and_then(|r|r.exit_code),"duration_ms":started.elapsed().as_millis(),
            "error":result.as_ref().err()})});
    serde_json::to_value(result?).map_err(|e| e.to_string())
}

async fn dispatch_studio(
    sub: &PlatformSubsystem,
    intent: &str,
    args: Value,
    actor: &str,
    caps: &[String],
    trace: &str,
) -> Result<Value, String> {
    let root = args["root"].as_str().unwrap_or("").to_owned();
    let prepared = serde_json::from_value::<crate::studio_adapter::Request>(args)
        .map_err(|e| e.to_string())
        .and_then(|request| {
            let mgr = sub.workspaces.lock().unwrap();
            crate::studio_adapter::prepare(
                &mgr,
                &sub.workspace_process_policy
                    .with_file_name("studio-adapter.json"),
                intent,
                request,
                caps,
            )
        });
    let result = match prepared {
        Ok(command) => {
            if matches!(intent, "studio.lsp" | "studio.reindex")
                && sub.net.lock().unwrap().mode() == crate::net::NetMode::OfflineStrict
            {
                Err("delegated Studio LSP/indexing is disabled in offline_strict: the external daemon is not network-sandboxed".into())
            } else if !matches!(intent, "studio.project" | "studio.memory") {
                let (_, answer) = sub.confirm.ask(actor.into(), intent.into(), root.clone(),
                    format!("Autoriser {intent} pour /host/{} via le daemon Code Studio local ? Le serveur de langage utilise les droits de votre compte.",command.workspace_id), Some(120)).await;
                if !answer.await.map(|r| r.approved).unwrap_or(false) {
                    Err("confirmation denied or expired".into())
                } else if matches!(intent, "studio.lsp" | "studio.reindex")
                    && sub.net.lock().unwrap().mode() == crate::net::NetMode::OfflineStrict
                {
                    Err("delegated Studio work disabled in offline_strict".into())
                } else {
                    command.execute().await
                }
            } else {
                command.execute().await
            }
        }
        Err(error) => Err(error),
    };
    sub.audit(AuditAppendRequest {
        trace_id: trace.into(),
        actor: actor.into(),
        action: intent.into(),
        target: root,
        detail: json!({"ok":result.is_ok(),"error":result.as_ref().err()}),
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subsystem::PlatformConfig;
    use std::path::PathBuf;
    struct Fixture {
        home: PathBuf,
        sub: Arc<PlatformSubsystem>,
        caps: Vec<String>,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.home);
        }
    }
    impl Fixture {
        fn new() -> Self {
            let home = std::env::temp_dir().join(format!(
                "aos-execution-test-{}-{}",
                std::process::id(),
                rand::random::<u64>()
            ));
            let dir = |name: &str| home.join(name).to_string_lossy().into_owned();
            let config = PlatformConfig {
                bus: "ipc://test".into(),
                audit_dir: dir("audit"),
                storage_dir: dir("storage"),
                memory_dir: dir("memory"),
                modules_dir: dir("modules"),
                catalogue_file: dir("absent.yaml"),
                community_catalogue_dir: dir("catalogue"),
                skills_dir: dir("skills"),
                sessions_dir: dir("sessions"),
                embed_model: None,
                policies_file: None,
                confirm_timeout_sec: 10,
                secrets_file: dir("secrets/store"),
                net_mode: "online".into(),
                memory_v2: false,
                memory_v2_shadow: false,
            };
            let sub = PlatformSubsystem::open(&config).unwrap();
            for part in ["one", "two"] {
                std::fs::create_dir_all(home.join("project").join(part)).unwrap();
                std::fs::write(home.join("project").join(part).join("test.txt"), "before").unwrap();
            }
            sub.workspaces
                .lock()
                .unwrap()
                .bind(&dir("project"), Some("demo"), "human:ui")
                .unwrap();
            let caps = [
                "refactor.manage",
                "refactor.submit",
                "fs.read",
                "fs.write",
                "debug.run",
                "studio.lsp",
            ]
            .iter()
            .map(|k| format!("{k}:/host/demo/**"))
            .collect();
            Self { home, sub, caps }
        }
        async fn invoke(&self, name: &str, args: Value, actor: &str) -> Result<Value, String> {
            dispatch(&self.sub, name, args, actor, &self.caps, "fixture-trace").await
        }
    }
    #[tokio::test]
    async fn refactor_dispatch_requires_live_confirmation_and_audits_real_file_apply_undo() {
        let f = Fixture::new();
        let result=f.invoke("refactor.create",json!({"root":"demo","assignments":[
            {"part_id":"one","actor":"agent:one","subtree":"one"},{"part_id":"two","actor":"agent:two","subtree":"two"}]}),"agent:lead").await.unwrap();
        let plan = result["plan_id"].clone();
        for part in ["one", "two"] {
            f.invoke("refactor.submit",json!({"root":"demo","plan_id":plan,"part_id":part,"changes":[{"path":format!("{part}/test.txt"),"base_sha256":crate::workspace_refactor::hash("before"),"content":format!("after {part}")}]}),&format!("agent:{part}")).await.unwrap();
        }
        let review = f
            .invoke(
                "refactor.review",
                json!({"root":"demo","plan_id":plan}),
                "human:ui",
            )
            .await
            .unwrap();
        assert!(review["diff"].as_str().unwrap().contains("+after one"));
        let mut notifications = f.sub.confirm.subscribe().await;
        let args = json!({"root":"demo","plan_id":plan,"review_sha256":review["review_sha256"]});
        for approved in [false, true] {
            let sub = f.sub.clone();
            let caps = f.caps.clone();
            let args = args.clone();
            let task = tokio::spawn(async move {
                dispatch(
                    &sub,
                    "refactor.apply",
                    args,
                    "human:ui",
                    &caps,
                    "confirmed-apply",
                )
                .await
            });
            let pending =
                tokio::time::timeout(std::time::Duration::from_secs(5), notifications.recv())
                    .await
                    .unwrap()
                    .unwrap();
            assert_eq!(pending.action, "refactor.apply");
            assert_eq!(
                std::fs::read_to_string(f.home.join("project/one/test.txt")).unwrap(),
                "before"
            );
            f.sub.confirm.respond(&pending.id, approved, false).await;
            assert_eq!(task.await.unwrap().is_ok(), approved);
        }
        let review = f
            .invoke(
                "refactor.review",
                json!({"root":"demo","plan_id":plan}),
                "human:ui",
            )
            .await
            .unwrap();
        let sub = f.sub.clone();
        let caps = f.caps.clone();
        let task = tokio::spawn(async move {
            dispatch(
                &sub,
                "refactor.undo",
                json!({"root":"demo","plan_id":plan,"review_sha256":review["review_sha256"]}),
                "human:ui",
                &caps,
                "confirmed-undo",
            )
            .await
        });
        let pending = notifications.recv().await.unwrap();
        f.sub.confirm.respond(&pending.id, true, false).await;
        task.await.unwrap().unwrap();
        assert_eq!(
            std::fs::read_to_string(f.home.join("project/one/test.txt")).unwrap(),
            "before"
        );
        let audit = std::fs::read_to_string(f.home.join("audit/audit.jsonl")).unwrap();
        assert!(audit.contains("confirmation denied"));
        assert!(audit.contains("refactor.undo"));
        f.sub.audit.lock().unwrap().verify().unwrap();
    }

    #[tokio::test]
    async fn packaged_reference_reaches_real_host_confirmation_through_wasm() {
        let f = Fixture::new();
        let mut notifications = f.sub.confirm.subscribe().await;
        let sub = f.sub.clone();
        let home = f.home.clone();
        let caps = f.caps.clone();
        let job = tokio::task::spawn_blocking(move || {
            let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
            let mut author = crate::ModuleAuthor::open(home.join("author")).unwrap();
            author.ext_rt_wasm = repo.join("share/modules/ext-rt.aospkg/module.wasm");
            let src = author.src_dir.join("dev-assistant");
            std::fs::create_dir_all(src.join("ui")).unwrap();
            for path in ["manifest.yaml", "handlers.yaml", "ui/index.html"] {
                std::fs::copy(
                    repo.join("community/modules/dev-assistant").join(path),
                    src.join(path),
                )
                .unwrap();
            }
            // The optional reference intentionally requests no host grants by
            // default. Configure this fixture's exact workspace before cap review.
            let manifest_path = src.join("manifest.yaml");
            let mut manifest: aos_proto::ModuleManifest =
                serde_yaml::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
            manifest.permissions.required_caps = caps.clone();
            std::fs::write(&manifest_path, serde_yaml::to_string(&manifest).unwrap()).unwrap();
            let pkg = author.package_script("dev-assistant").unwrap();
            let mut runtime = crate::ModuleRuntime::open(home.join("installed"), sub).unwrap();
            runtime
                .install(std::path::Path::new(&pkg.package_dir), Some(caps))
                .unwrap();
            let invoke = |name: &str, args: Value, actor: &str| {
                runtime.invoke(
                    "dev-assistant",
                    &format!("dev-assistant.{name}"),
                    &args,
                    actor,
                    &["tool.invoke:dev-assistant".into()],
                    "wasm-host-proof",
                )
            };
            let source = invoke(
                "read_safe",
                json!({"root":"demo","path":"one/test.txt"}),
                "human:ui",
            )
            .unwrap();
            assert_eq!(source["content"], "before");
            let mut inventory = invoke(
                "inspect",
                json!({"root":"demo","page_entries":1}),
                "human:ui",
            )
            .unwrap();
            assert_eq!(inventory["continuation"]["available"], true);
            while inventory["continuation"]["available"] == true {
                inventory = invoke("inspect_continue", json!({"root":"demo"}), "human:ui").unwrap();
            }
            assert_eq!(inventory["scan"]["complete"], true);
            assert_eq!(inventory["scan"]["eligible_files"], 2);
            let cached = invoke("inventory", json!({"root":"demo"}), "human:ui").unwrap();
            assert_eq!(cached["content_version"], inventory["content_version"]);
            let plan=invoke("refactor_create",json!({"root":"demo","assignments":[{"part_id":"one","actor":"agent:one","subtree":"one"},{"part_id":"two","actor":"agent:two","subtree":"two"}]}),"human:ui").unwrap()["plan_id"].clone();
            for part in ["one", "two"] {
                invoke("refactor_submit",json!({"root":"demo","plan_id":plan,"part_id":part,"changes":[{"path":format!("{part}/test.txt"),"base_sha256":crate::workspace_refactor::hash("before"),"content":format!("after {part}")}]}),&format!("agent:{part}")).unwrap();
            }
            let review = invoke(
                "refactor_review",
                json!({"root":"demo","plan_id":plan}),
                "human:ui",
            )
            .unwrap();
            for expected in [false, true] {
                let result = invoke(
                    "refactor_apply",
                    json!({"root":"demo","plan_id":plan,"review_sha256":review["review_sha256"]}),
                    "human:ui",
                );
                assert_eq!(result.is_ok(), expected, "{result:?}");
            }
        });
        for approved in [false, true] {
            let pending =
                tokio::time::timeout(std::time::Duration::from_secs(30), notifications.recv())
                    .await
                    .unwrap()
                    .unwrap();
            assert_eq!(pending.action, "refactor.apply");
            assert_eq!(pending.actor, "human:ui");
            assert_eq!(
                std::fs::read_to_string(f.home.join("project/one/test.txt")).unwrap(),
                "before"
            );
            f.sub.confirm.respond(&pending.id, approved, false).await;
        }
        job.await.unwrap();
        assert_eq!(
            std::fs::read_to_string(f.home.join("project/one/test.txt")).unwrap(),
            "after one"
        );
    }

    #[tokio::test]
    async fn source_search_audit_keeps_query_metadata_without_credential_values() {
        use crate::module_rt::{HostCallCtx, HostServices};
        let f = Fixture::new();
        let secret = format!("ghp_{}", "Z".repeat(36));
        let ctx = HostCallCtx {
            module: "dev-assistant".into(),
            module_dir: f.home.join("unused-module"),
            granted_caps: f.caps.clone(),
            actor: "agent:reader".into(),
            trace_id: "private-query".into(),
        };
        let response = f
            .sub
            .call(&ctx, "fs.search", json!({"root":"demo","query":secret}))
            .unwrap();
        assert_eq!(response["ok"], true);
        let mut malformed = json!({"root":"demo"});
        malformed[&secret] = json!(true);
        assert!(dispatch(
            &f.sub,
            "workspace.inspect",
            malformed,
            "agent:reader",
            &f.caps,
            "private-context-field"
        )
        .await
        .is_err());
        let audit = std::fs::read_to_string(f.home.join("audit/audit.jsonl")).unwrap();
        assert!(!audit.contains(&secret));
        assert!(audit.contains("query_redacted"));
        f.sub.audit.lock().unwrap().verify().unwrap();
    }

    #[tokio::test]
    async fn native_agent_context_reaches_real_bus_policy_and_audit() {
        let f = Fixture::new();
        for i in 0..100 {
            std::fs::write(
                f.home.join("project").join(format!("source-{i}.rs")),
                "fn sample() {}\n",
            )
            .unwrap();
        }
        std::fs::write(
            f.home.join("project/hidden.rs"),
            "password = 'SyntheticOnlyValue438927'",
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let broker = tokio::spawn(aos_ipc::broker::serve(listener));
        let mut svc = aos_ipc::BusService::new("context-fixture");
        register(&mut svc, f.sub.clone());
        let service_addr = addr.clone();
        let service = tokio::spawn(async move { svc.serve(&service_addr).await });
        let bus = aos_ipc::BusClient::connect(&addr, "agent:context-test")
            .await
            .unwrap();
        for _ in 0..100 {
            if bus
                .call::<(), bool>("bus.lookup:workspace.inspect", &(), vec![])
                .await
                .unwrap_or(false)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let caps = vec!["fs.read:/host/demo/**".into()];
        let invoke = |tool: &'static str, args: Value| {
            let bus = &bus;
            let caps = &caps;
            async move {
                aos_agent::workspace_context::invoke(bus, "context-test", caps, tool, &args).await
            }
        };
        let mut result: Value = serde_json::from_str(
            &invoke(
                "workspace.inspect",
                json!({"root":"demo","page_entries":32}),
            )
            .await,
        )
        .unwrap();
        assert_eq!(result["ok"], true);
        while result["continuation"]["available"] == true {
            result = serde_json::from_str(
                &invoke("workspace.inspect", json!({"root":"demo","resume":true})).await,
            )
            .unwrap();
            assert_eq!(result["ok"], true);
        }
        assert_eq!(result["scan"]["complete"], true);
        assert_eq!(result["agent_files_page"]["total"], 102);
        assert_eq!(result["files"].as_array().unwrap().len(), 64);
        let version = result["content_version"].clone();
        let page: Value = serde_json::from_str(
            &invoke(
                "workspace.inventory",
                json!({"root":"demo","files_offset":64,"content_version":version}),
            )
            .await,
        )
        .unwrap();
        assert_eq!(page["files"].as_array().unwrap().len(), 38);
        assert!(page["freshness"]
            .as_str()
            .unwrap()
            .starts_with("not_rechecked"));
        let read = invoke(
            "workspace.read",
            json!({"root":"demo","path":"source-0.rs"}),
        )
        .await;
        assert_eq!(
            serde_json::from_str::<Value>(&read).unwrap()["content"],
            "fn sample() {}\n"
        );
        for path in ["hidden.rs", "../outside.rs", ".env"] {
            let denied = invoke("workspace.read", json!({"root":"demo","path":path})).await;
            assert_eq!(serde_json::from_str::<Value>(&denied).unwrap()["ok"], false);
            assert!(!denied.contains("SyntheticOnlyValue"));
        }
        let denied = aos_agent::workspace_context::invoke(
            &bus,
            "context-test",
            &[],
            "workspace.inventory",
            &json!({"root":"demo"}),
        )
        .await;
        assert_eq!(serde_json::from_str::<Value>(&denied).unwrap()["ok"], false);
        let wrong = aos_agent::workspace_context::invoke(
            &bus,
            "other",
            &caps,
            "workspace.inventory",
            &json!({"root":"demo"}),
        )
        .await;
        assert!(wrong.contains("requires_actor_bound_bus"));
        let audit = std::fs::read_to_string(f.home.join("audit/audit.jsonl")).unwrap();
        assert!(audit.contains("agent:context-test"));
        assert!(!audit.contains("SyntheticOnlyValue"));
        f.sub.audit.lock().unwrap().verify().unwrap();
        service.abort();
        broker.abort();
    }

    #[tokio::test]
    async fn debug_denial_does_not_spawn_and_offline_delegates_are_refused() {
        let f = Fixture::new();
        std::fs::write(f.home.join("sessions/workspace-debug-policy.json"),json!({"workspaces":{"demo":{"fixture":{"adapter":{"program":std::env::current_exe().unwrap()},"program":"one/test.txt"}}}}).to_string()).unwrap();
        let mut notifications = f.sub.confirm.subscribe().await;
        let sub = f.sub.clone();
        let caps = f.caps.clone();
        let task = tokio::spawn(async move {
            dispatch(
                &sub,
                "debug.start",
                json!({"root":"demo","configuration":"fixture"}),
                "human:ui",
                &caps,
                "denied-debug",
            )
            .await
        });
        let pending = tokio::time::timeout(std::time::Duration::from_secs(5), notifications.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(pending.reason.contains("test.txt"));
        f.sub.confirm.respond(&pending.id, false, false).await;
        assert!(task
            .await
            .unwrap()
            .unwrap_err()
            .contains("confirmation denied"));
        f.sub
            .net
            .lock()
            .unwrap()
            .set_mode(crate::net::NetMode::OfflineStrict);
        assert!(f
            .invoke(
                "debug.start",
                json!({"root":"demo","configuration":"fixture"}),
                "human:ui"
            )
            .await
            .unwrap_err()
            .contains("offline_strict"));
        std::fs::write(f.home.join("sessions/studio-adapter.json"),json!({"daemon_url":"http://127.0.0.1:1","ui_url":"http://127.0.0.1:5178","workspaces":{"demo":"fixture"}}).to_string()).unwrap();
        assert!(f
            .invoke(
                "studio.lsp",
                json!({"root":"demo","action":"diagnostics","path":"one/test.txt"}),
                "human:ui"
            )
            .await
            .unwrap_err()
            .contains("offline_strict"));
        assert!(f.sub.confirm.pending_list().await.is_empty());
    }
}
