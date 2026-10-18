//! Protocol integration with a real IPC broker and deterministic OS service doubles.
//! This checks routing/identity/capability/audit behavior, not image-engine output.
use aos_ipc::{broker, msg::Status, service::BusService};
use aos_mcp::{BusState, OsToolPolicy};
use aos_proto::{AuditAppendRequest, MediaImageGenerateRequest, ModuleInvokeRequest};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[tokio::test]
async fn media_and_installed_module_calls_keep_caps_identity_and_audit() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let broker_task = tokio::spawn(async move { broker::serve(listener).await.unwrap() });
    let audits = Arc::new(Mutex::new(Vec::<AuditAppendRequest>::new()));
    let media = Arc::new(Mutex::new(Vec::<MediaImageGenerateRequest>::new()));
    let mut svc = BusService::new("mcp-test-os");
    let seen = media.clone();
    svc.on("media.image.generate", move |ctx| {
        let seen = seen.clone();
        async move {
            let req: MediaImageGenerateRequest = ctx.payload().unwrap();
            assert_eq!(ctx.intent.from, "service:mcp-test");
            assert_eq!(req.actor, "mcp:service:mcp-test");
            assert!(req.caps.contains(&"media.generate".to_string()));
            assert_eq!(ctx.intent.caps, req.caps);
            let result =
                json!({"path":req.path,"bytes":128,"engine":"test-double","model_id":"fixture"});
            seen.lock().unwrap().push(req);
            ctx.respond(Status::Ok, &result).await.unwrap();
        }
    });
    svc.on("module.describe", |ctx| async move {
        ctx.respond(Status::Ok, &json!({"manifest":{"tools":[
            {"name":"notes.list","description":"List notes","input_schema":{"type":"object"}},
            {"name":"notes.delete","description":"Delete notes","input_schema":{"type":"object"}}
        ]}})).await.unwrap();
    });
    svc.on("module.invoke", |ctx| async move {
        let req: ModuleInvokeRequest = ctx.payload().unwrap();
        assert_eq!(req.actor, "mcp:service:mcp-test");
        assert_eq!(req.module, "notes");
        assert_eq!(req.tool, "notes.list");
        assert!(req.actor_caps.contains(&"tool.invoke:notes".to_string()));
        ctx.respond(Status::Ok, &json!({"ok":true,"result":{"notes":[]}}))
            .await
            .unwrap();
    });
    let records = audits.clone();
    svc.on("audit.append", move |ctx| {
        let records = records.clone();
        async move {
            records
                .lock()
                .unwrap()
                .push(ctx.payload::<AuditAppendRequest>().unwrap());
            ctx.respond(Status::Ok, &true).await.unwrap();
        }
    });
    let service_addr = addr.clone();
    let service_task = tokio::spawn(async move { svc.serve(&service_addr).await.unwrap() });
    let bus =
        BusState::new(addr.clone(), "service:mcp-test".into()).with_os_tools(OsToolPolicy::new(
            vec![
                "akasha_image_generate".into(),
                "akasha_video_generate".into(),
                "notes.list".into(),
                "notes.delete".into(),
            ],
            vec![
                "media.generate".into(),
                "fs.write:/downloads/**".into(),
                "tool.invoke:notes".into(),
            ],
        ));
    // Wait for service registration through the protocol, rather than a fixed delay.
    let probe = aos_ipc::BusClient::connect(&addr, "test:probe")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !probe.lookup("module.describe").await.unwrap() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let catalog = bus.tool_catalog().await;
    let names: Vec<_> = catalog["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(names.contains(&"akasha_image_generate"));
    assert!(names.contains(&"akasha_video_generate"));
    assert!(names.contains(&"akasha_module__notes__list"));
    assert!(!names.contains(&"akasha_module__notes__delete"));
    for tool in ["akasha_image_generate", "akasha_video_generate"] {
        let result = bus.call_tool(tool, json!({"prompt":"test"})).await;
        assert_eq!(result["isError"], false, "{result}");
    }
    assert_eq!(media.lock().unwrap().len(), 2);
    assert_eq!(
        media.lock().unwrap()[1].options.sd_mode.as_deref(),
        Some("vid_gen")
    );
    let result = bus.call_tool("akasha_module__notes__list", json!({})).await;
    assert_eq!(result["isError"], false, "{result}");
    let denied = BusState::new(addr.clone(), "service:mcp-test".into());
    assert_eq!(
        denied
            .call_tool("akasha_image_generate", json!({"prompt":"test"}))
            .await["isError"],
        true
    );
    assert_eq!(
        denied
            .call_tool("akasha_module__notes__list", json!({}))
            .await["isError"],
        true
    );
    assert_eq!(
        media.lock().unwrap().len(),
        2,
        "denied calls must not reach the engine"
    );
    let records = audits.lock().unwrap();
    assert_eq!(records.len(), 5);
    assert!(records
        .iter()
        .all(|r| r.action == "mcp.tool.call" && r.actor == "mcp:service:mcp-test"));
    assert_eq!(
        records
            .iter()
            .filter(|r| r.detail["ok"] == Value::Bool(false))
            .count(),
        2
    );
    service_task.abort();
    broker_task.abort();
}
