//! Isolated model.infer contract corpus, paired requests, two chronological passes.
//! Usage: model_conversation_probe <modeld> <qwen.gguf> <spark.gguf> <corpus.json> <cpu|cuda>
use aos_ipc::BusClient;
use aos_proto::{ChatMessage, InferParams, InferRequest, ModelInfo, TokenEvent};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Deserialize)]
struct Corpus {
    version: u32,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    category: String,
    messages: Vec<ChatMessage>,
    #[serde(default)]
    tools: Vec<Value>,
    expected: Expected,
}
#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum Expected {
    Exact(String),
    Json(Value),
    ToolCall { name: String, arguments: Value },
}

fn matches_expected(raw: &str, expected: &Expected, tools: &[Value]) -> bool {
    let answer = aos_agent::actions::strip_reasoning(raw);
    match expected {
        Expected::Exact(value) => answer.trim() == value,
        Expected::Json(value) => serde_json::from_str::<Value>(&answer).is_ok_and(|v| v == *value),
        Expected::ToolCall { name, arguments } => {
            let offered: Vec<_> = tools
                .iter()
                .filter_map(|tool| {
                    let function = tool.get("function")?;
                    Some(aos_agent::tools::ToolDesc {
                        name: function.get("name")?.as_str()?.into(),
                        description: String::new(),
                        input_schema: function.get("parameters")?.clone(),
                        backend: aos_agent::tools::ToolBackend::Native,
                        required_caps: vec![],
                    })
                })
                .collect();
            let actions = aos_agent::actions::parse_actions_with_tools(raw, &offered);
            actions.len() == 1 && actions[0].action == *name && actions[0].args == *arguments
        }
    }
}

fn hash(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

async fn infer(bus: &BusClient, model: &str, case: &Case, pass: usize) -> Result<Value> {
    let request = InferRequest {
        model_id: Some(model.into()),
        messages: case.messages.clone(),
        tools: case.tools.clone(),
        params: InferParams {
            max_tokens: 128,
            temperature: 0.0,
            top_p: 1.0,
            seed: Some(42),
        },
        priority: 2,
        data_refs: vec![],
        images: vec![],
        routing: Some("local_only".into()),
    };
    let start = Instant::now();
    let mut stream = bus
        .call_stream::<_, TokenEvent>("model.infer", &request, vec![])
        .await?;
    let mut text = String::new();
    let mut metrics = Value::Null;
    let mut complete = false;
    while let Some(event) = stream.recv().await {
        match event? {
            TokenEvent::Delta { text: piece } => text.push_str(&piece),
            TokenEvent::Done {
                prompt_tokens,
                generated_tokens,
                ttft_ms,
                tok_s,
            } => {
                metrics = json!({"prompt_tokens":prompt_tokens,"generated_tokens":generated_tokens,"ttft_ms":ttft_ms,"tok_s":tok_s});
                complete = true;
                break;
            }
            TokenEvent::Error { message } => return Err(message.into()),
            _ => {}
        }
    }
    if !complete {
        return Err("stream closed without Done".into());
    }
    let contract_ok = matches_expected(&text, &case.expected, &case.tools);
    Ok(
        json!({"id":case.id,"category":case.category,"pass":pass,"transport_ok":true,"contract_ok":contract_ok,
        "expected":case.expected,"output":text,"visible_output":aos_agent::actions::strip_reasoning(&text),
        "elapsed_ms":start.elapsed().as_millis(),"metrics":metrics}),
    )
}

async fn run(args: &[PathBuf], mode: &str, home: &Path, report: &mut Value) -> Result<()> {
    let [modeld, qwen, spark, fixture] = args else {
        return Err("expected four input paths".into());
    };
    let raw = std::fs::read(fixture)?;
    let corpus: Corpus = serde_json::from_slice(&raw)?;
    if corpus.version != 1 || corpus.cases.is_empty() || corpus.cases.len() > 64 {
        return Err("invalid corpus version/count".into());
    }
    let ids: std::collections::HashSet<_> = corpus.cases.iter().map(|c| &c.id).collect();
    if ids.len() != corpus.cases.len() {
        return Err("duplicate case identifiers".into());
    }
    for c in &corpus.cases {
        if c.messages.is_empty()
            || c.messages.len() > 32
            || c.messages.iter().any(|m| m.content.len() > 16384)
        {
            return Err("unbounded case".into());
        }
    }
    report["corpus_sha256"] = json!(hash(fixture)?);
    report["case_count"] = json!(corpus.cases.len());
    report["modeld_sha256"] = json!(hash(modeld)?);
    let mut assets = Vec::new();
    for (file, pin) in [
        (
            qwen,
            "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4",
        ),
        (
            spark,
            "adfcfa19a4ed6a5985da8bf565fe15f8e1a7e131d79bae2d19d48d1c40109428",
        ),
    ] {
        let digest = hash(file)?;
        if digest != pin {
            return Err("model weights differ from pinned evaluation assets".into());
        }
        assets.push(
            json!({"filename":file.file_name(),"bytes":file.metadata()?.len(),"sha256":digest}),
        );
    }
    report["assets"] = json!(assets);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?.to_string();
    let broker = tokio::spawn(aos_ipc::broker::serve(listener));
    let bus = BusClient::connect(&address, "test:model-conversation-probe").await?;
    let models = [
        ("local:conversation-qwen", qwen, 32_u32, 65536_u64),
        ("local:conversation-spark", spark, 36, 147456),
    ];
    let mut overrides = serde_json::Map::new();
    let mut catalogue = Vec::new();
    for (id, file, layers, kv) in models {
        overrides.insert(id.into(),json!({"path":file,"n_layers":layers,"weights_bytes":file.metadata()?.len(),"embed_bytes":200000000,"kv_bytes_per_token":kv}));
        catalogue.push(json!({"id":id,"name":id,"modality":"text","format":"gguf","source":{"type":"local_file","path":file,"sha256":"0".repeat(64)},
            "architecture":{"n_layers":layers,"n_params":4.0e9,"context_length":16384},
            "resource_hints":{"weights_bytes":file.metadata()?.len(),"embed_bytes":200000000,"kv_bytes_per_token":kv,"supports_layer_offload":true},
            "capabilities":["chat"],"backends_compatible":["llamacpp"],"privacy_class":"local"}));
    }
    std::fs::create_dir_all(home.join("data/models"))?;
    std::fs::write(
        home.join("data/models/catalog.yaml"),
        serde_json::to_vec_pretty(&json!({"models":catalogue}))?,
    )?;
    let config = json!({"bus":address,"gpu":mode=="cuda","vram_total_bytes":17179869184_u64,
        "os_reserve_vram_bytes":1073741824_u64,"os_reserve_ram_bytes":2147483648_u64,"n_threads":8,
        "default_model":"local:conversation-qwen","default_kv_tokens":4096,"n_seq_max":4,"batch_window_ms":150,
        "adaptive_planner":false,"routing":"local_only","models":overrides});
    std::fs::write(home.join("model.yaml"), serde_json::to_vec_pretty(&config)?)?;
    report["configuration"] = json!({"mode":mode,"n_seq_max":4,"concurrent_requests":2,"passes":2,"seed":42,"temperature":0,"max_tokens":128,"default_kv_tokens":4096});
    let log = std::fs::File::create(home.join("modeld.log"))?;
    let mut command = tokio::process::Command::new(modeld);
    command
        .arg(home.join("model.yaml"))
        .env("AOS_HOME", home)
        .current_dir(home)
        .stdout(log.try_clone()?)
        .stderr(log)
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command.spawn()?;
    let outcome=async {
        tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                if child.try_wait()?.is_some() {return Err::<(),Box<dyn std::error::Error+Send+Sync>>("owned daemon exited".into());}
                if bus.lookup("model.infer").await? {return Ok(());}
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }).await??;
        let mut results=Vec::new();
        for (id,_,_,_) in models {
            for pass in 0..2 {
                for pair in corpus.cases.chunks(2) {
                    eprintln!("{mode} {id}: pass {pass}, {}",pair[0].id);
                    let call=async {
                        if pair.len()==2 {
                            let (a,b)=tokio::join!(infer(&bus,id,&pair[0],pass),infer(&bus,id,&pair[1],pass));
                            Ok::<_,Box<dyn std::error::Error+Send+Sync>>(vec![a?,b?])
                        } else {Ok(vec![infer(&bus,id,&pair[0],pass).await?])}
                    };
                    let entries=tokio::time::timeout(Duration::from_secs(180),call).await??;
                    for mut entry in entries {entry["model_id"]=json!(id);results.push(entry);}
                    report["results"]=json!(results);
                    std::fs::write(home.join("report.partial.json"),serde_json::to_vec_pretty(report)?)?;
                }
            }
        }
        let mut replay=Vec::new();
        for (id,_,_,_) in models {for case in &corpus.cases {
            let pair:Vec<_>=results.iter().filter(|v|v["model_id"]==id&&v["id"]==case.id).collect();
            replay.push(json!({"model_id":id,"id":case.id,"equal":pair.len()==2&&pair[0]["output"]==pair[1]["output"]}));
        }}
        report["replays"]=json!(replay);
        report["contract_passed"]=json!(results.iter().filter(|r|r["contract_ok"]==true).count());
        report["contract_total"]=json!(results.len());
        report["replay_equal"]=json!(replay.iter().filter(|r|r["equal"]==true).count());
        let infos:Vec<ModelInfo>=bus.call("model.list",&(),vec![]).await?;
        report["models"]=json!(infos);
        Ok::<(),Box<dyn std::error::Error+Send+Sync>>(())
    }.await;
    if child.try_wait()?.is_none() {
        child.kill().await?;
    }
    child.wait().await?;
    report["owned_modeld_stopped"] = json!(true);
    broker.abort();
    outcome?;
    // Native stderr may contain raw tokenizer bytes; keep its hash lossless and
    // decode only the diagnostic text tolerantly. Model replies remain typed UTF-8.
    let native_bytes = std::fs::read(home.join("modeld.log"))?;
    report["native_log_valid_utf8"] = json!(std::str::from_utf8(&native_bytes).is_ok());
    let native = String::from_utf8_lossy(&native_bytes);
    let buffers: Vec<_> = native
        .lines()
        .filter(|l| l.contains("CUDA0 model buffer size"))
        .map(str::to_owned)
        .collect();
    report["cuda_model_buffers"] = json!(buffers);
    report["two_job_batches"] = json!(native
        .lines()
        .filter(|l| l.contains("batch 2 job(s)"))
        .count());
    if mode == "cuda" && buffers.len() < 2 {
        return Err("requested CUDA but native buffers for both models were not observed".into());
    }
    if mode == "cpu" && !buffers.is_empty() {
        return Err("requested CPU but native model buffers were placed on CUDA".into());
    }
    report["native_log_sha256"] = json!(hash(&home.join("modeld.log"))?);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 5 {
        return Err("expected modeld, Qwen, Spark, corpus and cpu|cuda".into());
    }
    let mode = arguments[4].to_str().ok_or("invalid mode")?;
    if !matches!(mode, "cpu" | "cuda") {
        return Err("expected cpu|cuda".into());
    }
    let paths: Vec<PathBuf> = arguments[..4].iter().map(PathBuf::from).collect();
    if paths.iter().any(|p| !p.is_absolute() || !p.is_file()) {
        return Err("expected existing absolute input files".into());
    }
    let home = std::env::temp_dir().join(format!(
        "akasha-conversation-proof-{:032x}",
        rand::random::<u128>()
    ));
    std::fs::create_dir(&home)?;
    eprintln!("isolated home: {}", home.display());
    let mut report = json!({"scope":"authored corpus via real model.infer; two paired-request passes; not a representative quality/security benchmark or installed UI acceptance"});
    let outcome = run(&paths, mode, &home, &mut report).await;
    report["transport_ok"] = json!(outcome.is_ok());
    if let Err(error) = &outcome {
        report["error"] = json!(error.to_string());
    }
    std::fs::write(
        home.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grader_uses_answer_not_reasoning_and_rejects_surplus_json() {
        assert!(matches_expected(
            "<think>not the answer</think>397",
            &Expected::Exact("397".into()),
            &[]
        ));
        assert!(!matches_expected(
            "<think>397",
            &Expected::Exact("397".into()),
            &[]
        ));
        assert!(!matches_expected(
            "{\"x\":1} extra",
            &Expected::Json(json!({"x":1})),
            &[]
        ));
        assert!(matches_expected(
            "{\"x\":1}",
            &Expected::Json(json!({"x":1})),
            &[]
        ));
    }
    #[test]
    fn tool_grader_requires_one_expected_action() {
        let e = Expected::ToolCall {
            name: "notes.list".into(),
            arguments: json!({}),
        };
        let tools = vec![
            json!({"type":"function","function":{"name":"notes.list","parameters":{"type":"object","properties":{}}}}),
        ];
        assert!(matches_expected(
            "<tool_call>{\"name\":\"notes.list\",\"arguments\":{}}</tool_call>",
            &e,
            &tools
        ));
        assert!(matches_expected(
            "<tool_call><function=notes.list></function></tool_call>",
            &e,
            &tools
        ));
        assert!(!matches_expected(
            "<tool_call><function=notes.list>",
            &e,
            &tools
        ));
        assert!(!matches_expected("TOOL: notes.delete {}", &e, &tools));
        assert!(!matches_expected(
            "TOOL: notes.list {\"extra\":1}",
            &e,
            &tools
        ));
    }
}
