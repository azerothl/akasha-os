use aos_agent::{intents, ReportPayload};
use aos_ipc::{BusClient, BusService, Status};
use aos_proto::{AgentOutputEvent, AgentState, InferRequest, TokenEvent};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

#[tokio::test]
async fn definition_uses_one_inference_and_only_conversational_actions() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let broker = tokio::spawn(aos_ipc::broker::serve(listener));
    let events = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut service = BusService::new("definition-test");
    let captured = events.clone();
    service.on(intents::REPORT, move |ctx| {
        let captured = captured.clone();
        async move {
            let payload: ReportPayload = ctx.payload().unwrap();
            captured.lock().await.push(payload.event);
            ctx.respond(Status::Ok, &true).await.unwrap();
        }
    });
    let captured = calls.clone();
    let answer = "Un hexagone est un polygone à six côtés.";
    service.on("model.infer", move |ctx| {
        let captured = captured.clone();
        async move {
            let req: InferRequest = ctx.payload().unwrap();
            captured.lock().await.push(req);
            let stream = ctx.open_stream();
            stream
                .send(&TokenEvent::Delta {
                    text: answer.into(),
                })
                .await
                .unwrap();
            stream
                .send(&TokenEvent::Done {
                    prompt_tokens: 30,
                    generated_tokens: 11,
                    ttft_ms: 10.0,
                    tok_s: 100.0,
                })
                .await
                .unwrap();
            stream.finish(Status::Ok).await.unwrap();
        }
    });
    let service_address = address.clone();
    let service = tokio::spawn(async move { service.serve(&service_address).await });
    let bus = BusClient::connect(&address, "definition-test-client")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !bus.lookup("model.infer").await.unwrap()
            || !bus.lookup(intents::REPORT).await.unwrap()
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    let id = "agent-definition-test";
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let home = std::env::temp_dir().join(format!("aos-definition-{}-{stamp}", std::process::id()));
    std::fs::create_dir(&home).unwrap();
    let spec = serde_json::json!({
        "agent_id": id,
        "goal": {"statement": "Définis un hexagone en une phrase."},
        "model_id": "local:qwen-test"
    });
    let spec_path = home.join("spec.json");
    std::fs::write(&spec_path, serde_json::to_vec(&spec).unwrap()).unwrap();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_aos-agent-worker"));
    command
        .args(["--agent-id", id, "--bus", &address, "--spec-path"])
        .arg(&spec_path)
        .current_dir(&home)
        .env("AOS_HOME", &home)
        .kill_on_drop(true);
    let status = tokio::time::timeout(Duration::from_secs(10), command.status())
        .await
        .unwrap()
        .unwrap();
    assert!(status.success());

    let calls = calls.lock().await;
    assert_eq!(calls.len(), 1);
    assert!(calls[0].tools.is_empty());
    let prompt = &calls[0].messages[0].content;
    assert_eq!(prompt, aos_agent::assess::BRIEF_DEFINITION_INSTRUCTION);
    assert!(!prompt.contains("web.search"));
    assert!(!prompt.contains("module.list"));
    assert_eq!(calls[0].messages.len(), 2);
    assert_eq!(calls[0].messages[1].role, "user");
    assert_eq!(calls[0].messages[1].content, spec["goal"]["statement"]);
    let events = events.lock().await;
    assert!(!events
        .iter()
        .any(|event| matches!(event, AgentOutputEvent::PlanUpdated { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, AgentOutputEvent::Step(step)
        if step.action == "task.assess" && step.args["model_calls"] == 0)));
    assert!(events
        .iter()
        .any(|event| matches!(event, AgentOutputEvent::Log { line }
        if line == &format!("goal.complete : {answer}"))));
    assert!(matches!(
        events.last(),
        Some(AgentOutputEvent::StateChanged {
            state: AgentState::Done
        })
    ));
    service.abort();
    broker.abort();
    let home = home.canonicalize().unwrap();
    assert!(home.starts_with(std::env::temp_dir().canonicalize().unwrap()));
    std::fs::remove_dir_all(home).unwrap();
}
