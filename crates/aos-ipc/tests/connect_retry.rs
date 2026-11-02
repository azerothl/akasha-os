use aos_ipc::{broker, BusClient, ConnectRetry};
use std::time::{Duration, Instant};

async fn start_bus_at(addr: &str) {
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    tokio::spawn(async move {
        broker::serve(listener).await.unwrap();
    });
}

#[tokio::test]
async fn connect_with_retry_waits_for_late_broker() {
    let port = {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        l.local_addr().unwrap().port()
    };
    let addr = format!("127.0.0.1:{port}");
    let policy = ConnectRetry {
        interval: Duration::from_millis(50),
        max_attempts: 40,
    };
    let started = Instant::now();
    let client_fut = BusClient::connect_with_retry(&addr, "retry-test", policy);
    tokio::time::sleep(Duration::from_millis(200)).await;
    start_bus_at(&addr).await;
    let client = tokio::time::timeout(Duration::from_secs(5), client_fut)
        .await
        .expect("timed out waiting for broker")
        .expect("connect_with_retry");
    assert!(started.elapsed() >= Duration::from_millis(150));
    let _ = client;
}

#[tokio::test]
async fn connect_with_retry_fails_when_broker_never_starts() {
    let port = {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        l.local_addr().unwrap().port()
    };
    let addr = format!("127.0.0.1:{port}");
    let policy = ConnectRetry {
        interval: Duration::from_millis(10),
        max_attempts: 3,
    };
    let err = match BusClient::connect_with_retry(&addr, "retry-fail", policy).await {
        Ok(_) => panic!("expected connect failure"),
        Err(e) => e,
    };
    assert!(matches!(err, aos_ipc::CallError::Io(_)));
}
