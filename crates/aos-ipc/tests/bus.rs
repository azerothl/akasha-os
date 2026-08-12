//! Tests d'intégration du Semantic IPC Bus (P1.5).

use aos_ipc::msg::Status;
use aos_ipc::service::BusService;
use aos_ipc::{broker, BusClient};
use std::time::Duration;

async fn start_bus() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    tokio::spawn(async move {
        broker::serve(listener).await.unwrap();
    });
    addr
}

async fn start_echo_service(addr: &str) {
    let mut svc = BusService::new("echo");
    svc.on("echo.say", |ctx| async move {
        #[derive(serde::Deserialize, serde::Serialize)]
        struct Msg {
            text: String,
        }
        let msg: Msg = ctx.payload().unwrap();
        // Vérifie la propagation des caps (P1 : transportées, P3 : validées).
        assert!(ctx.intent.caps.iter().any(|c| c.starts_with("cap://")));
        ctx.respond(Status::Ok, &Msg { text: msg.text })
            .await
            .unwrap();
    })
    .on("echo.stream", |ctx| async move {
        let n: u32 = ctx.payload().unwrap();
        let stream = ctx.open_stream();
        for i in 0..n {
            stream.send(&i).await.unwrap();
        }
        stream.finish(Status::Ok).await.unwrap();
    });
    let addr = addr.to_string();
    tokio::spawn(async move { svc.serve(&addr).await });
    // Laisse le temps de s'enregistrer.
    tokio::time::sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn appel_unaire_a_travers_le_broker() {
    let addr = start_bus().await;
    start_echo_service(&addr).await;
    let client = BusClient::connect(&addr, "test").await.unwrap();

    #[derive(serde::Serialize, serde::Deserialize)]
    struct Msg {
        text: String,
    }
    let resp: Msg = client
        .call(
            "echo.say",
            &Msg {
                text: "bonjour".into(),
            },
            vec!["cap://fs/read/notes/a.md".into()],
        )
        .await
        .unwrap();
    assert_eq!(resp.text, "bonjour");
}

#[tokio::test]
async fn flux_de_reponses() {
    let addr = start_bus().await;
    start_echo_service(&addr).await;
    let client = BusClient::connect(&addr, "test").await.unwrap();

    let mut rx = client
        .call_stream::<u32, u32>("echo.stream", &5u32, vec![])
        .await
        .unwrap();
    let mut got = Vec::new();
    while let Some(item) = rx.recv().await {
        got.push(item.unwrap());
    }
    assert_eq!(got, vec![0, 1, 2, 3, 4]);
}

#[tokio::test]
async fn decouverte_de_service() {
    let addr = start_bus().await;
    start_echo_service(&addr).await;
    let client = BusClient::connect(&addr, "test").await.unwrap();

    assert!(client.lookup("echo.say").await.unwrap());
    assert!(!client.lookup("model.infer").await.unwrap());
}

#[tokio::test]
async fn intent_inconnu_renvoie_not_found() {
    let addr = start_bus().await;
    let client = BusClient::connect(&addr, "test").await.unwrap();
    let err = client
        .call::<u32, u32>("nope.rien", &1u32, vec![])
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        aos_ipc::CallError::Status {
            status: Status::NotFound,
            ..
        }
    ));
}

#[tokio::test]
async fn deux_clients_concurrents() {
    let addr = start_bus().await;
    start_echo_service(&addr).await;
    let c1 = BusClient::connect(&addr, "c1").await.unwrap();
    let c2 = BusClient::connect(&addr, "c2").await.unwrap();

    let (r1, r2) = tokio::join!(
        c1.call_stream::<u32, u32>("echo.stream", &3u32, vec![]),
        c2.call_stream::<u32, u32>("echo.stream", &3u32, vec![]),
    );
    let mut v1 = Vec::new();
    let mut rx = r1.unwrap();
    while let Some(i) = rx.recv().await {
        v1.push(i.unwrap());
    }
    let mut v2 = Vec::new();
    let mut rx = r2.unwrap();
    while let Some(i) = rx.recv().await {
        v2.push(i.unwrap());
    }
    assert_eq!(v1.len(), 3);
    assert_eq!(v2.len(), 3);
}
