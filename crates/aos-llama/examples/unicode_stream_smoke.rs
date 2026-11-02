//! Real GGUF regression for token fragments across all three generation paths.
//! Usage: unicode_stream_smoke <absolute GGUF path> [gpu layers, default 0]
use aos_llama::{BatchItem, GenParams, LlamaBackend, LlamaContext, LlamaModel, LoadOptions};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

fn messages(text: &str) -> Vec<(String, String)> {
    vec![
        (
            "system".into(),
            "You are a copying machine. Output only the requested text, without explanation."
                .into(),
        ),
        (
            "user".into(),
            format!("Copy exactly the text between brackets, without brackets: [{text}]"),
        ),
    ]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("expected absolute GGUF path")?;
    if !path.is_absolute() || !path.is_file() {
        return Err("expected existing absolute GGUF".into());
    }
    let layers = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "0".into())
        .parse::<i32>()?;
    let _backend = LlamaBackend::init();
    if layers > 0
        && (!LlamaBackend::supports_gpu_offload() || LlamaBackend::gpu_device_count() == 0)
    {
        return Err("GPU offload requested but unavailable".into());
    }
    let opts = LoadOptions {
        n_gpu_layers: layers,
        n_ctx: 4096,
        n_seq_max: 2,
        n_threads: 8,
        ..Default::default()
    };
    let model = Arc::new(LlamaModel::load(&path, &opts)?);
    let mut ctx = LlamaContext::new(model, &opts)?;
    let params = GenParams {
        max_tokens: 32,
        temperature: 0.0,
        top_p: 1.0,
        seed: 42,
    };
    let texts = ["🧭", "été 猫"];
    let mut cases = Vec::new();
    for path in ["plain", "lookup"] {
        for text in texts {
            let mut output = String::new();
            let callback = |piece: &str| {
                output.push_str(piece);
                true
            };
            let stats = if path == "plain" {
                ctx.generate(&messages(text), &params, callback)?
            } else {
                ctx.generate_lookup(
                    &messages(text),
                    &params,
                    Arc::new(AtomicBool::new(false)),
                    Arc::new(AtomicBool::new(false)),
                    12,
                    callback,
                )?
            };
            cases.push(json!({"path":path,"expected":text,"output":output,"exact":output.trim()==text,"generated_tokens":stats.generated_tokens}));
        }
    }
    let items: Vec<_> = texts
        .iter()
        .map(|text| BatchItem {
            messages: messages(text),
            tools: vec![],
            params: params.clone(),
            abort: Arc::new(AtomicBool::new(false)),
            pause: Arc::new(AtomicBool::new(false)),
        })
        .collect();
    let mut outputs = [String::new(), String::new()];
    let results = ctx.generate_batch(&items, |index, piece| {
        outputs[index].push_str(piece);
        true
    });
    for (index, result) in results.into_iter().enumerate() {
        let stats = result?;
        cases.push(json!({"path":"batch","expected":texts[index],"output":outputs[index],"exact":outputs[index].trim()==texts[index],"generated_tokens":stats.generated_tokens}));
    }
    let ok = cases.len() == 6 && cases.iter().all(|c| c["exact"] == true);
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"ok":ok,"gpu_layers":layers,"cases":cases,
        "scope":"Unicode streaming regression through plain, lookup and paired batch generation with real GGUF; not native UI acceptance"}))?
    );
    if !ok {
        return Err("Unicode streaming regression failed".into());
    }
    Ok(())
}
