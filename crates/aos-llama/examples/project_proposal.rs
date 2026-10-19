//! Generate a real-model proposal for an explicitly marked, isolated acceptance fixture.
use aos_llama::{GenParams, LlamaBackend, LlamaContext, LlamaModel, LoadOptions};
use serde_json::{json, Value};
use std::{io::Write, path::PathBuf, sync::Arc};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err(
            "usage: project_proposal MODEL_GGUF FIXTURE_REQUEST_JSON NEW_RESULT_JSON".into(),
        );
    }
    let request = PathBuf::from(&args[1]);
    let output = PathBuf::from(&args[2]);
    let parent = request
        .parent()
        .ok_or("fixture parent missing")?
        .canonicalize()?;
    if !parent.join(".akasha-os-model-probe").is_file()
        || output
            .parent()
            .ok_or("output parent missing")?
            .canonicalize()?
            != parent
    {
        return Err(
            "model probe requires an isolated marked fixture; output must share its parent".into(),
        );
    }
    if request.metadata()?.len() > 65536 {
        return Err("fixture prompt exceeds 64 KiB".into());
    }
    let input: Value = serde_json::from_slice(&std::fs::read(&request)?)?;
    let messages: Vec<(String, String)> = serde_json::from_value(input["messages"].clone())?;
    let _backend = LlamaBackend::init();
    let opts = LoadOptions {
        n_gpu_layers: 999,
        n_ctx: 8192,
        n_threads: 8,
        ..Default::default()
    };
    let model = Arc::new(LlamaModel::load(&PathBuf::from(&args[0]), &opts)?);
    let mut context = LlamaContext::new(model, &opts)?;
    let params = GenParams {
        max_tokens: 2048,
        temperature: 0.1,
        top_p: 0.9,
        seed: 42,
    };
    let begun = std::time::Instant::now();
    let mut raw = String::new();
    let stats = context.generate(&messages, &params, |piece| {
        raw.push_str(piece);
        true
    })?;
    let report = json!({"raw_text":raw,"prompt_tokens":stats.prompt_tokens,"generated_tokens":stats.generated_tokens,"stop":format!("{:?}",stats.stopped),"generation_ms":begun.elapsed().as_millis(),"gpu_devices":LlamaBackend::gpu_device_count(),"scope":"real local model proposal; not autonomous agentd or overlapping GPU inference"});
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(&serde_json::to_vec_pretty(&report)?)?;
    file.sync_all()?;
    println!("generated {} tokens", stats.generated_tokens);
    Ok(())
}
