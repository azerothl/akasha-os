//! Repeated and shortened prompts against a real GGUF, with lookup enabled.
//! Usage: prefix_replay_smoke <absolute GGUF path> [gpu layers, default 0]
use aos_llama::{GenParams, LlamaBackend, LlamaContext, LlamaModel, LoadOptions};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("expected absolute GGUF path")?;
    if !path.is_absolute() || !path.is_file() {
        return Err("GGUF must be an existing absolute file".into());
    }
    let layers = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "0".into())
        .parse::<i32>()?;
    let _backend = LlamaBackend::init();
    let gpu_offload_supported = LlamaBackend::supports_gpu_offload();
    let gpu_devices = LlamaBackend::gpu_device_count();
    if layers > 0 && (!gpu_offload_supported || gpu_devices == 0) {
        return Err("GPU offload requested but no backend GPU is available".into());
    }
    let opts = LoadOptions {
        n_gpu_layers: layers,
        n_ctx: 2048,
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
    let prompts = [
        (
            "repeat-a",
            "Repeat exactly: blue fox walks home blue fox walks home blue fox walks home.",
        ),
        (
            "repeat-b",
            "Repeat exactly: blue fox walks home blue fox walks home blue fox walks home.",
        ),
        ("shortened", "Reply with one word: Paris."),
        ("repeat-shortened", "Reply with one word: Paris."),
    ];
    let mut cases = Vec::new();
    for (name, prompt) in prompts {
        let mut output = String::new();
        let result = ctx.generate_lookup(
            &[("user".into(), prompt.into())],
            &params,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            12,
            |piece| {
                output.push_str(piece);
                true
            },
        );
        cases.push(match result {
            Ok(stats) => json!({"case":name,"ok":true,"output":output,"generated_tokens":stats.generated_tokens,
                "prefix_hit_tokens":stats.prefix_hit_tokens,"draft_proposed":stats.draft_proposed,
                "draft_disabled":stats.draft_disabled,"draft_disable_reason":stats.draft_disable_reason.map(|reason| reason.as_str())}),
            Err(error) => json!({"case":name,"ok":false,"error":error.to_string()}),
        });
    }
    let replay_equal = cases[0]["output"].is_string()
        && cases[0]["output"] == cases[1]["output"]
        && cases[2]["output"].is_string()
        && cases[2]["output"] == cases[3]["output"];
    let ok = cases.iter().all(|case| case["ok"] == true) && replay_equal;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"ok":ok,"replay_equal":replay_equal,"gpu_layers":layers,"gpu_offload_supported":gpu_offload_supported,"gpu_devices":gpu_devices,"cases":cases,
        "scope":"real GGUF, repeated and shortened text prompts through prefix/lookup generation; synthetic requests; no native UI"})
        )?
    );
    if !ok {
        return Err("prefix replay failed".into());
    }
    Ok(())
}
