//! Verifies live model registration and logical image paths through model.infer.
//! Run only against an isolated test bus. Does not change model or file policies.
use aos_ipc::BusClient;
use aos_proto::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let expect_error = args.get(5).is_some_and(|s| s == "--expect-error");
    if args.len() != 5 && !(args.len() == 6 && expect_error) {
        return Err("usage: illustration_vision_ipc_probe BUS MODEL SOURCE_PNG CANDIDATE_PNG NEW_OUTPUT_DIR [--expect-error]".into());
    }
    let output = std::path::Path::new(&args[4]);
    std::fs::create_dir(output)?;
    let bus = BusClient::connect(&args[0], "illustration-vision-ipc-probe").await?;
    let models: Vec<ModelInfo> = bus.call("model.list", &(), vec![]).await?;
    std::fs::write(output.join("models.json"), serde_json::to_vec_pretty(&models)?)?;
    let model = models.iter().find(|m| m.id == args[1]).ok_or("model absent")?;
    if !model.has_vision { return Err("model registered without vision".into()); }
    println!("Model registered with vision: {}", model.id);
    let mut classes = Vec::new();
    for path in &args[2..4] {
        let class = bus.call::<_, FsClassResponse>("fs.class", &FsClassRequest { path: path.clone() }, vec![]).await;
        classes.push(match class {
            Ok(class) => serde_json::json!({"path":path,"class":class}),
            Err(error) => serde_json::json!({"path":path,"error":error.to_string()}),
        });
    }
    std::fs::write(output.join("classes.json"), serde_json::to_vec_pretty(&classes)?)?;
    let req = InferRequest {
        model_id: Some(args[1].clone()),
        messages: vec![ChatMessage {role:"user".into(), content:
            "Inspect image 1 and image 2 separately. For each image, report the number of cats actually visible, where each cat is, and whether any limbs or tails are ambiguous. Then list visible differences. Do not assume the second image is better. Do not invent missing details.".into()}],
        tools: vec![],
        params: InferParams {max_tokens: 1000, temperature: 0.0, ..Default::default()},
        priority: 1, data_refs: args[2..4].to_vec(), images: args[2..4].to_vec(),
        routing: Some("local_only".into()),
    };
    std::fs::write(output.join("request.json"), serde_json::to_vec_pretty(&req)?)?;
    let mut rx = bus.call_stream::<InferRequest, TokenEvent>("model.infer", &req, vec![]).await?;
    let mut events = Vec::new();
    let mut answer = String::new();
    let mut completed = false;
    let mut failure = None;
    loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(180), rx.recv()).await?;
        let Some(event) = event else { break; };
        let event = event?;
        events.push(serde_json::to_value(&event)?);
        match event {
            TokenEvent::Delta {text} => answer.push_str(&text),
            TokenEvent::Done {..} => { completed = true; break; },
            TokenEvent::Error {message} => { failure = Some(message); break; },
            _ => {}
        }
    }
    std::fs::write(output.join("events.json"), serde_json::to_vec_pretty(&events)?)?;
    std::fs::write(output.join("response.txt"), &answer)?;
    if let Some(error) = failure {
        if expect_error && error.contains("classification indisponible") {
            println!("Expected privacy rejection: {error}");
            return Ok(());
        }
        return Err(error.into());
    }
    if expect_error { return Err("expected privacy denial, but inference was not rejected".into()); }
    if !completed { return Err("inference ended without completion".into()); }
    println!("{answer}");
    Ok(())
}
