//! Small manual acceptance-test client for an already-running Preview.
use aos_ipc::BusClient;
use serde_json::Value;
#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert!(args.len() >= 3, "usage: preview_probe INTENT JSON [TIMEOUT_SECONDS]");
    let req: Value = serde_json::from_str(&args[2]).expect("request JSON");
    let seconds = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(120);
    let bus = BusClient::connect("127.0.0.1:24701", "recette-20260909").await.expect("Preview bus");
    let start = std::time::Instant::now();
    let result = tokio::time::timeout(std::time::Duration::from_secs(seconds), bus.call::<Value, Value>(&args[1], &req, vec![])).await;
    match result {
        Ok(Ok(value)) => println!("{}", serde_json::json!({"intent":args[1],"elapsed_ms":start.elapsed().as_millis(),"response":value})),
        other => { eprintln!("elapsed_ms={} error={other:?}", start.elapsed().as_millis()); std::process::exit(1); }
    }
}
