//! Local, real-engine benchmark. Usage:
//! cargo run -p aos-sd --example illustration_passes -- MODEL_DIR OUTPUT_DIR [SUBJECT] [CONSTRUCTION_FILE] [SEED] [POSE_REFERENCE]
//! Set AOS_SD_BIN to a compatible sd-cli executable. OUTPUT_DIR must be new.
//! Optional AOS_BENCH_KLEIN_BASE_WEIGHTS selects an undistilled 4B checkpoint,
//! with CFG 4 / 20 steps. Text encoder and VAE still come from MODEL_DIR.
use aos_sd::{illustration::{generate_planned_passes, generate_pose_guided_passes}, ImageGenOpts};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 { return Err("expected MODEL_DIR OUTPUT_DIR [SUBJECT] [CONSTRUCTION_FILE] [SEED] [POSE_REFERENCE]".into()); }
    let model = PathBuf::from(&args[1]);
    let output = PathBuf::from(&args[2]);
    let subject = args.get(3).map(String::as_str).unwrap_or(
        "An elderly gardener smoking a pipe in an armchair, looking at his garden. Three-quarter side view, entire seated figure visible, feet supported on the ground, one bent arm holding the pipe close to his mouth. The garden is visible in the direction of his gaze. Traditional pencil and restrained watercolor illustration.");
    let base_weights = std::env::var_os("AOS_BENCH_KLEIN_BASE_WEIGHTS").map(PathBuf::from);
    let (weights, steps, cfg) = benchmark_profile(&model, base_weights);
    let seed: i64 = args.get(5).map(|s| s.parse()).transpose()?.unwrap_or(42);
    if seed < 0 { return Err("benchmark seed must be nonnegative for reproducibility".into()); }
    let reference = args.get(6).map(std::path::absolute).transpose()?;
    if let Some(path) = &reference { image::open(path)?; }
    let opts = ImageGenOpts {
        width: 768, height: 768, steps, cfg_scale: Some(cfg), seed: Some(seed),
        sampling_method: Some("euler".into()),
        diffusion_model: Some(weights.clone()),
        llm_path: Some(model.join("Qwen3-4B-Q4_K_M.gguf")),
        vae_path: Some(model.join("split_files/vae/flux2-vae.safetensors")),
        offload_to_cpu: true, diffusion_fa: true,
        ..Default::default()
    };
    for path in [&weights, opts.llm_path.as_ref().unwrap(), opts.vae_path.as_ref().unwrap()] {
        if !path.is_file() { return Err(format!("missing model file: {}", path.display()).into()); }
    }
    aos_sd::clear_media_cancel();
    println!("Real local image-editing run; seed: {seed}; subject: {subject}");
    // Optional separate construction brief, supplied by a scene planner or a
    // benchmark fixture. No gardener-specific pose is baked into the engine.
    let construction = match args.get(4) {
        Some(path) => std::fs::read_to_string(path)?,
        None => subject.to_owned(),
    };
    let publish = |pass: aos_sd::illustration::DrawingPass, path: &std::path::Path| {
        if pass == aos_sd::illustration::DrawingPass::Skeleton {
            if let Some(reference) = &reference {
                // Archive the exact input alongside the first published pass.
                std::fs::copy(reference, output.join("pose-reference.png"))?;
                std::fs::write(output.join("pose-reference-policy.txt"), "initial pass only; subsequent passes use the previous output")?;
            }
        }
        println!("PASS READY {}: {} (visual review still required)", pass.name(), path.display());
        Ok(())
    };
    let paths = match &reference {
        Some(pose) => generate_pose_guided_passes(&weights, subject, &construction, pose, &output, &opts, publish),
        None => generate_planned_passes(&weights, subject, &construction, &output, &opts, publish),
    }?;
    std::fs::write(output.join("seed.txt"), seed.to_string())?;
    std::fs::write(output.join("render-config.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "diffusion_model":weights, "text_encoder":opts.llm_path, "vae":opts.vae_path,
        "seed":seed, "steps":steps, "cfg_scale":cfg, "width":opts.width, "height":opts.height,
        "sampling_method":opts.sampling_method, "pose_reference":reference,
        "pose_reference_policy":"initial_only"
    }))?)?;
    println!("{} passes generated. This is not a visual quality acceptance or UI integration test.", paths.len());
    Ok(())
}

fn benchmark_profile(model: &std::path::Path, base: Option<PathBuf>) -> (PathBuf, u32, f32) {
    match base {
        Some(weights) => (weights, 20, 4.0),
        None => (model.join("flux-2-klein-4b-Q8_0.gguf"), 4, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn base_profile_does_not_change_the_default_distilled_settings() {
        let model = PathBuf::from("models");
        assert_eq!(benchmark_profile(&model, None), (model.join("flux-2-klein-4b-Q8_0.gguf"), 4, 1.0));
        let base = PathBuf::from("base.gguf");
        assert_eq!(benchmark_profile(&model, Some(base.clone())), (base, 20, 4.0));
    }
}
