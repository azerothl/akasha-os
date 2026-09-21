//! Local-only transport probe, not an artistic approval gate or a full agent test.
use aos_model::backend::RemoteOpenAiBackend;
use aos_proto::{ChatMessage, InferRequest, TokenEvent};
use std::sync::{Arc, atomic::AtomicBool};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let transient = args.get(4).is_some_and(|arg| arg == "--transient");
    if args.len() != 4 && !(args.len() == 5 && transient) {
        return Err("usage: illustration_vision_probe LOCAL_MODEL SOURCE_PNG CANDIDATE_PNG NEW_OUTPUT_DIR".into());
    }
    if args[0].contains("cloud") { return Err("local model required".into()); }
    let output = std::path::Path::new(&args[3]);
    std::fs::create_dir(output)?;
    let req = InferRequest {
        model_id: Some(args[0].clone()),
        messages: vec![ChatMessage { role: "user".into(), content:
            "Compare these two illustrations. Image 1 is the original; image 2 is a proposed revision. Describe only changes you can actually see, especially in footwear, pipe/smoke, chair linework and hands. Report uncertainty. Do not assume image 2 is better. Do not give an approval score.".into() }],
        tools: vec![],
        params: aos_proto::InferParams { max_tokens: 1200, temperature: 0.0, ..Default::default() },
        priority: 1, data_refs: args[1..3].to_vec(), images: args[1..3].to_vec(),
        routing: Some("local_only".into()),
    };
    std::fs::write(output.join("request-metadata.json"), serde_json::to_vec_pretty(&req)?)?;
    let mut backend = RemoteOpenAiBackend::new("http://127.0.0.1:11434/v1", &args[0], None);
    if transient { backend.enable_transient_ollama()?; }
    std::fs::write(output.join("transport.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "model":args[0], "transient_ollama":transient
    }))?)?;
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let task = tokio::spawn(async move {
        backend.infer_stream(&req, tx, Arc::new(AtomicBool::new(false))).await
    });
    let mut response = String::new();
    let mut done = false;
    while let Some(event) = rx.recv().await {
        match event {
            TokenEvent::Delta { text } => response.push_str(&text),
            TokenEvent::Done { .. } => done = true,
            TokenEvent::Error { message } => return Err(message.into()),
            _ => {}
        }
    }
    task.await??;
    if !done { return Err("no terminal completion".into()); }
    std::fs::write(output.join("response.txt"), &response)?;
    println!("{response}");
    if transient {
        let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(3)).build()?;
        let mut checks = Vec::new();
        let mut resident = true;
        for _ in 0..20 {
            let state: serde_json::Value = client.get("http://127.0.0.1:11434/api/ps").send().await?.json().await?;
            resident = state["models"].as_array().ok_or("missing model list")?.iter()
                .any(|m| m["name"].as_str() == Some(args[0].as_str()));
            checks.push(serde_json::json!({"model":args[0],"resident":resident}));
            if !resident { break; }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        std::fs::write(output.join("residency-after.json"), serde_json::to_vec_pretty(&checks)?)?;
        if resident { return Err("model still resident: release not confirmed (possibly another active request)".into()); }
        println!("Model residency after response: unloaded");
    }
    Ok(())
}
