//! Small manual acceptance-test client for an already-running Preview.
use aos_ipc::BusClient;
use aos_proto::{InferRequest, TokenEvent};
use serde_json::Value;
#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert!(
        args.len() >= 3,
        "usage: preview_probe INTENT JSON|@file [TIMEOUT_SECONDS]"
    );
    let seconds = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(120);
    let bus_addr = std::env::var("AOS_PROBE_BUS").unwrap_or_else(|_| "127.0.0.1:24701".into());
    let bus = BusClient::connect(&bus_addr, "recette-20260909")
        .await
        .expect("Preview bus");
    let start = std::time::Instant::now();
    if args[1] == "batch" {
        let path = &args[2];
        let timeout = std::time::Duration::from_secs(seconds);
        let content = std::fs::read_to_string(path).expect("batch JSONL");
        let summary = std::env::var("AOS_PROBE_RESPONSE_SUMMARY").ok().as_deref() == Some("1");
        let mut results = Vec::new();
        for (index, line) in content.lines().enumerate() {
            if line.trim().is_empty() { continue; }
            let operation_start = std::time::Instant::now();
            let entry: Value = serde_json::from_str(line).expect("batch entry JSON");
            let intent = entry["intent"].as_str().expect("batch intent");
            let request = entry.get("request").cloned().unwrap_or(Value::Null);
            let fixture_id = entry.get("fixture_id").cloned().unwrap_or(Value::Null);
            let result = if intent == "model.infer" {
                let req: InferRequest = serde_json::from_value(request).expect("model.infer request");
                match tokio::time::timeout(
                    timeout,
                    bus.call_stream::<InferRequest, TokenEvent>(intent, &req, vec![]),
                )
                .await
                {
                    Ok(Ok(mut rx)) => {
                        let mut text = String::new();
                        let mut ttft_ms = 0.0;
                        let mut tok_s = 0.0;
                        let mut error = None;
                        while let Some(event) = rx.recv().await {
                            match event {
                                Ok(TokenEvent::Delta { text: delta }) => text.push_str(&delta),
                                Ok(TokenEvent::Done { ttft_ms: t, tok_s: s, .. }) => {
                                    ttft_ms = t;
                                    tok_s = s;
                                }
                                Ok(TokenEvent::Error { message }) => {
                                    error = Some(message);
                                    break;
                                }
                                Ok(_) => {}
                                Err(err) => {
                                    error = Some(err.to_string());
                                    break;
                                }
                            }
                        }
                        match error {
                            Some(error) => Err(error),
                            None => Ok(serde_json::json!({
                                "text": text,
                                "ttft_ms": ttft_ms,
                                "tok_s": tok_s,
                            })),
                        }
                    }
                    Ok(Err(error)) => Err(error.to_string()),
                    Err(_) => Err("timeout".into()),
                }
            } else {
                match tokio::time::timeout(timeout, bus.call::<Value, Value>(intent, &request, vec![])).await {
                    Ok(Ok(response)) => Ok(response),
                    Ok(Err(error)) => Err(error.to_string()),
                    Err(_) => Err("timeout".into()),
                }
            };
            match result {
                Ok(response) => {
                    let response = if summary {
                        match intent {
                            "model.infer" => serde_json::json!({
                                "text": response.get("text"),
                                "ttft_ms": response.get("ttft_ms"),
                                "tok_s": response.get("tok_s"),
                            }),
                            "mem.context" => serde_json::json!({
                                "object_ids": response.get("objects").and_then(Value::as_array).map(|items| items.iter().filter_map(|item| item.get("id").and_then(Value::as_u64)).collect::<Vec<_>>()),
                                "shadow": response.get("shadow"),
                                "memory_warnings": response.get("memory_warnings"),
                            }),
                            "mem.graph.query" => serde_json::json!({
                                "root": response.get("root").and_then(|item| item.get("id")),
                                "node_ids": response.get("nodes").and_then(Value::as_array).map(|items| items.iter().filter_map(|item| item.get("id").and_then(Value::as_u64)).collect::<Vec<_>>()),
                                "relation_pairs": response.get("relations").and_then(Value::as_array).map(|items| items.iter().filter_map(|item| Some(serde_json::json!({"from": item.get("from")?, "kind": item.get("kind")?, "to": item.get("to")}))).collect::<Vec<_>>()),
                                "truncated": response.get("truncated"),
                            }),
                            "mem.timeline" => serde_json::json!({
                                "object_ids": response.get("objects").and_then(Value::as_array).map(|items| items.iter().filter_map(|item| item.get("id").and_then(Value::as_u64)).collect::<Vec<_>>()),
                                "truncated": response.get("truncated"),
                            }),
                            "mem.mind_palace.query" => serde_json::json!({
                                "object_ids": response.get("objects").and_then(Value::as_array).map(|items| items.iter().filter_map(|item| item.get("id").and_then(Value::as_u64)).collect::<Vec<_>>()),
                                "relation_count": response.get("relations").and_then(Value::as_array).map(|items| items.len()),
                                "truncated": response.get("truncated"),
                            }),
                            "mem.object.list" => serde_json::json!({
                                "object_ids": response.as_array().map(|items| items.iter().filter_map(|item| item.get("id").and_then(Value::as_u64)).collect::<Vec<_>>()),
                                "namespaces": response.as_array().map(|items| items.iter().filter_map(|item| item.get("namespace").and_then(Value::as_str)).collect::<Vec<_>>()),
                            }),
                            "mem.explain" => serde_json::json!({
                                "object_id": response.get("object").and_then(|item| item.get("id")),
                                "source_count": response.get("supporting_sources").and_then(Value::as_array).map(|items| items.len()),
                                "relation_count": response.get("relations").and_then(Value::as_array).map(|items| items.len()),
                                "freshness_warning": response.get("freshness_warning"),
                            }),
                            "mem.narrative.generate" => serde_json::json!({
                                "object_id": response.get("id"),
                                "kind": response.get("kind"),
                                "source_count": response.get("source_refs").and_then(Value::as_array).map(|items| items.len()),
                            }),
                            _ => response,
                        }
                    } else { response };
                    results.push(serde_json::json!({"index": index, "intent": intent, "fixture_id": fixture_id, "ok": true, "elapsed_ms": operation_start.elapsed().as_millis(), "response": response}));
                }
                Err(error) => results.push(serde_json::json!({"index": index, "intent": intent, "fixture_id": fixture_id, "ok": false, "elapsed_ms": operation_start.elapsed().as_millis(), "error": error})),
            }
        }
        let output = serde_json::to_string(&results).expect("batch results JSON");
        if let Ok(output_path) = std::env::var("AOS_PROBE_BATCH_OUTPUT") {
            std::fs::write(output_path, output).expect("batch output");
        } else {
            println!("{output}");
        }
        return;
    }
    let req_raw = if args[2].starts_with('@') {
        std::fs::read_to_string(&args[2][1..]).expect("request JSON file")
    } else {
        args[2].clone()
    };
    let req: Value = serde_json::from_str(&req_raw).expect("request JSON");
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
