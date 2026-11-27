use aos_agent::{intents, ControlCmd, ControlResp, ReportPayload};
use aos_ipc::{BusClient, BusService, Status};
use aos_proto::{AgentOutputEvent, AgentState};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

#[tokio::test]
async fn worker_completes_price_calculation_without_model_or_tools() {
    check_worker(false).await;
}

#[tokio::test]
async fn worker_waits_for_the_missing_discount_then_calculates_without_a_model() {
    check_worker(true).await;
}

async fn check_worker(clarify: bool) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let broker = tokio::spawn(aos_ipc::broker::serve(listener));
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut service = BusService::new("price-test");
    let captured = events.clone();
    service.on(intents::REPORT, move |ctx| {
        let captured = captured.clone();
        async move {
            let payload: ReportPayload = ctx.payload().unwrap();
            captured.lock().await.push(payload.event);
            ctx.respond(Status::Ok, &true).await.unwrap();
        }
    });
    let service_address = address.clone();
    let service_task = tokio::spawn(async move { service.serve(&service_address).await });
    let bus = BusClient::connect(&address, "price-test-client")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !bus.lookup(intents::REPORT).await.unwrap() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    let id = "agent-local-price-test";
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let home = std::env::temp_dir().join(format!("aos-local-price-{}-{stamp}", std::process::id()));
    std::fs::create_dir(&home).unwrap();
    let request = "Un abonnement coûte 80 euros. Une remise de 25 % s'applique, puis on ajoute 5 euros de frais. Quel est le total ? Réponds en une seule phrase en français avec le calcul.";
    let request = if clarify {
        request.replace(" de 25 %", "")
    } else {
        request.to_owned()
    };
    let spec = serde_json::json!({
        "agent_id": id,
        "goal": {
            "statement": request
        },
        "skills": ["planner"]
    });
    let spec_path = home.join("probe-spec.json");
    std::fs::write(&spec_path, serde_json::to_vec(&spec).unwrap()).unwrap();
    let started = Instant::now();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_aos-agent-worker"));
    command
        .args(["--agent-id", id, "--bus", &address, "--spec-path"])
        .arg(&spec_path)
        .current_dir(&home)
        .env("AOS_HOME", &home)
        .kill_on_drop(true);
    let mut child = command.spawn().unwrap();
    if clarify {
        let control = format!("agent.{id}.control");
        tokio::time::timeout(Duration::from_secs(5), async {
            while !events.lock().await.iter().any(|event| {
                matches!(
                    event,
                    AgentOutputEvent::StateChanged {
                        state: AgentState::Blocked
                    }
                )
            }) || !bus.lookup(&control).await.unwrap()
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let response = bus
            .call::<_, ControlResp>(
                &control,
                &ControlCmd::Steer {
                    directive: "25 %".into(),
                },
                vec![],
            )
            .await
            .unwrap();
        assert!(matches!(response, ControlResp::Ack));
    }
    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .unwrap()
        .unwrap();
    let elapsed = started.elapsed();
    let agent_dir = home.join("var/agents").join(id);
    let state_path = agent_dir.join("state.json");
    let persisted = std::fs::read_to_string(&state_path);
    // Leave any failure evidence available in the isolated temporary home.
    assert!(status.success());
    let recorded = events.lock().await;
    let expected_states = if clarify {
        vec![
            AgentState::Running,
            AgentState::Blocked,
            AgentState::Running,
            AgentState::Done,
        ]
    } else {
        vec![AgentState::Running, AgentState::Done]
    };
    let states: Vec<_> = recorded
        .iter()
        .filter_map(|event| match event {
            AgentOutputEvent::StateChanged { state } => Some(state.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(states, expected_states);
    let steps: Vec<_> = recorded
        .iter()
        .filter_map(|event| match event {
            AgentOutputEvent::Step(step) => Some(step),
            _ => None,
        })
        .collect();
    if clarify {
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].action, "local.clarify");
        assert!(steps[0].response.contains("pourcentage de remise"));
        assert_eq!(steps[0].args["model_calls"], 0);
    } else {
        assert_eq!(steps.len(), 1);
    }
    let step = steps.last().unwrap();
    let answer = "Le total est de 65 € (80 € × (1 − 25 %) + 5 € = 65 €).";
    assert_eq!(step.action, "local.calculate");
    assert_eq!(step.response, answer);
    assert_eq!(step.args["model_calls"], 0);
    assert!(persisted.unwrap().contains(answer));
    eprintln!("Native worker cold-process round trip: {elapsed:?}");
    drop(recorded);
    service_task.abort();
    broker.abort();
    std::fs::remove_file(state_path).unwrap();
    std::fs::remove_file(spec_path).unwrap();
    for directory in [agent_dir, home.join("var/agents"), home.join("var"), home] {
        std::fs::remove_dir(directory).unwrap();
    }
}
