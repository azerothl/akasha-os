//! Small manual acceptance-test client for an already-running Preview.
use aos_ipc::BusClient;
use aos_proto::{InferRequest, TokenEvent};
use serde_json::Value;
#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert!(
        args.len() >= 3,
        "usage: preview_probe INTENT JSON [TIMEOUT_SECONDS]"
    );
    let req: Value = serde_json::from_str(&args[2]).expect("request JSON");
    let seconds = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(120);
    let bus = BusClient::connect("127.0.0.1:24701", "recette-20260909")
        .await
        .expect("Preview bus");
    let start = std::time::Instant::now();
    if args[1] == "model.infer" {
        let req: InferRequest = serde_json::from_value(req).expect("model.infer request");
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(seconds),
            bus.call_stream::<InferRequest, TokenEvent>("model.infer", &req, vec![]),
        )
        .await;
        match result {
            Ok(Ok(mut rx)) => {
                let mut text = String::new();
                let mut ttft_ms = 0.0;
                let mut tok_s = 0.0;
                while let Some(event) = rx.recv().await {
                    match event {
                        Ok(TokenEvent::Delta { text: delta }) => text.push_str(&delta),
                        Ok(TokenEvent::Done {
                            ttft_ms: t,
                            tok_s: s,
                            ..
                        }) => {
                            ttft_ms = t;
                            tok_s = s;
                        }
                        Ok(TokenEvent::Error { message }) => {
                            eprintln!("elapsed_ms={} error={message}", start.elapsed().as_millis());
                            std::process::exit(1);
                        }
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("elapsed_ms={} error={e}", start.elapsed().as_millis());
                            std::process::exit(1);
                        }
                    }
                }
                println!(
                    "{}",
                    serde_json::json!({
                        "elapsed_ms": start.elapsed().as_millis(),
                        "intent": args[1],
                        "response": {"text": text, "ttft_ms": ttft_ms, "tok_s": tok_s}
                    })
                );
            }
            Ok(Err(e)) => {
                eprintln!("elapsed_ms={} error={e}", start.elapsed().as_millis());
                std::process::exit(1);
            }
            Err(_) => {
                eprintln!("elapsed_ms={} error=timeout", start.elapsed().as_millis());
                std::process::exit(1);
            }
        }
        return;
    }
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(seconds),
        bus.call::<Value, Value>(&args[1], &req, vec![]),
    )
    .await;
    match result {
        Ok(Ok(value)) => println!(
            "{}",
            serde_json::json!({"intent":args[1],"elapsed_ms":start.elapsed().as_millis(),"response":value})
        ),
        other => {
            eprintln!("elapsed_ms={} error={other:?}", start.elapsed().as_millis());
            std::process::exit(1);
        }
    }
}
