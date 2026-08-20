//! E16 media generate: cap check helpers + dest paths.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static MEDIA_TMP_SEQ: AtomicU64 = AtomicU64::new(1);

fn aos_home() -> PathBuf {
    std::env::var("AOS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn image_gen_progress_path() -> PathBuf {
    aos_home().join("var/run/image-gen-progress.json")
}

fn write_image_gen_progress(step: u32, total: u32) {
    let path = image_gen_progress_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let payload = serde_json::json!({ "step": step, "total": total });
    let _ = std::fs::write(path, payload.to_string());
}

fn clear_image_gen_progress() {
    let _ = std::fs::remove_file(image_gen_progress_path());
}

pub fn read_image_gen_progress() -> Option<(u32, u32)> {
    let raw = std::fs::read_to_string(image_gen_progress_path()).ok()?;
    let v = serde_json::from_str::<serde_json::Value>(&raw).ok()?;
    let step = v.get("step")?.as_u64()? as u32;
    let total = v.get("total")?.as_u64()? as u32;
    Some((step, total))
}

fn unique_media_temp(prefix: &str, ext: &str) -> PathBuf {
    let n = MEDIA_TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{n}.{ext}",
        std::process::id()
    ))
}

pub fn actor_may_generate(actor: &str, caps: &[String]) -> bool {
    if actor.is_empty() || actor.starts_with("human:") || actor.starts_with("service:") {
        return true;
    }
    caps.iter()
        .any(|c| c == "media.generate" || c.starts_with("media.generate:"))
}

pub fn default_image_path() -> String {
    format!(
        "/downloads/image-{}.png",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    )
}

pub fn default_audio_path() -> String {
    format!(
        "/downloads/speech-{}.wav",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    )
}

fn engine_name(eng: aos_sd::MediaEngine) -> &'static str {
    match eng {
        aos_sd::MediaEngine::SdCpp => "sdcpp",
        aos_sd::MediaEngine::Piper => "piper",
        aos_sd::MediaEngine::Stub => "stub",
    }
}

/// Inline base64 IPC is capped (~8 MiB frames); larger media uses host file ingest.
const INLINE_MEDIA_MAX: usize = 4 * 1024 * 1024;

async fn persist_media_file(
    bus: &aos_ipc::BusClient,
    dest: &str,
    source: &std::path::Path,
    actor: &str,
    caps: &[String],
    trace_id: &str,
    action: &str,
    model_id: &str,
    engine: &str,
) -> Result<u64, String> {
    use aos_proto::{AuditAppendRequest, FsWriteFromPathRequest, FsWriteFromPathResponse};
    let nbytes = std::fs::metadata(source).map(|m| m.len()).unwrap_or(0);
    let mut write_caps = caps.to_vec();
    if !write_caps.iter().any(|c| c.starts_with("fs.write:")) {
        write_caps.push("fs.write:/downloads/**".into());
    }
    let actor = if actor.is_empty() {
        "human:ui".to_string()
    } else {
        actor.to_string()
    };
    let resp = bus
        .call::<FsWriteFromPathRequest, FsWriteFromPathResponse>(
            "fs.write_from_path",
            &FsWriteFromPathRequest {
                path: dest.to_string(),
                source_host_path: source.to_string_lossy().into_owned(),
                actor: actor.clone(),
                caps: write_caps,
                trace_id: trace_id.to_string(),
            },
            vec![],
        )
        .await
        .map_err(|e| format!("fs.write_from_path: {e}"))?;
    let _ = bus
        .call::<AuditAppendRequest, bool>(
            "audit.append",
            &AuditAppendRequest {
                trace_id: trace_id.to_string(),
                actor,
                action: action.into(),
                target: dest.to_string(),
                detail: serde_json::json!({
                    "model_id": model_id,
                    "engine": engine,
                    "bytes": resp.bytes.max(nbytes),
                }),
            },
            vec![],
        )
        .await;
    Ok(resp.bytes.max(nbytes))
}

async fn persist_media(
    bus: &aos_ipc::BusClient,
    dest: &str,
    bytes: &[u8],
    actor: &str,
    caps: &[String],
    trace_id: &str,
    action: &str,
    model_id: &str,
    engine: &str,
) -> Result<u64, String> {
    if bytes.len() > INLINE_MEDIA_MAX {
        let tmp = unique_media_temp("aos-persist", "bin");
        std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
        let out = persist_media_file(
            bus, dest, &tmp, actor, caps, trace_id, action, model_id, engine,
        )
        .await;
        let _ = std::fs::remove_file(&tmp);
        return out;
    }
    use aos_proto::{AuditAppendRequest, FsWriteBytesRequest};
    use base64::Engine as _;
    let mut write_caps = caps.to_vec();
    if !write_caps.iter().any(|c| c.starts_with("fs.write:")) {
        write_caps.push("fs.write:/downloads/**".into());
    }
    let actor = if actor.is_empty() {
        "human:ui".to_string()
    } else {
        actor.to_string()
    };
    let _ = bus
        .call::<FsWriteBytesRequest, serde_json::Value>(
            "fs.write_bytes",
            &FsWriteBytesRequest {
                path: dest.to_string(),
                content_b64: base64::engine::general_purpose::STANDARD.encode(bytes),
                actor: actor.clone(),
                caps: write_caps,
                trace_id: trace_id.to_string(),
            },
            vec![],
        )
        .await
        .map_err(|e| format!("fs.write_bytes: {e}"))?;
    let _ = bus
        .call::<AuditAppendRequest, bool>(
            "audit.append",
            &AuditAppendRequest {
                trace_id: trace_id.to_string(),
                actor,
                action: action.into(),
                target: dest.to_string(),
                detail: serde_json::json!({
                    "model_id": model_id,
                    "engine": engine,
                    "bytes": bytes.len(),
                }),
            },
            vec![],
        )
        .await;
    Ok(bytes.len() as u64)
}

pub async fn run_image(
    sub: &crate::ModelSubsystem,
    bus: &aos_ipc::BusClient,
    req: &aos_proto::MediaImageGenerateRequest,
    dest: &str,
) -> Result<aos_proto::MediaGenerateResponse, String> {
    use aos_placement::PlacementProfile;
    let lookup = sub.find_media_model("image", req.model_id.as_deref());
    let (model_id, weights) = match lookup {
        Ok((id, path)) => (id, path),
        Err(_) => (
            req.model_id
                .clone()
                .unwrap_or_else(|| "local:sd-v1-5".into()),
            std::path::PathBuf::from("missing.safetensors"),
        ),
    };
    if weights.exists() && aos_sd::image_engine_available() {
        sub.ensure_loaded(&model_id, PlacementProfile::Balanced, 0)
            .await
            .map_err(|e| format!("placement: {e}"))?;
    }
    let prompt = req.prompt.clone();
    let opts = build_image_gen_opts(&model_id, &req.options, &sub.inference_pin())?;
    let total_steps = opts.steps.max(1);
    write_image_gen_progress(0, total_steps);
    sub.media_gen_begin(&model_id, total_steps);
    let sub_progress = sub.clone();
    let model_id_progress = model_id.clone();
    let tmp = unique_media_temp("aos-img", "png");
    let engine_result = tokio::task::spawn_blocking(move || {
        let result = aos_sd::generate_image_opts_progress(&weights, &prompt, &tmp, &opts, move |step, total| {
            write_image_gen_progress(step, total);
            sub_progress.media_gen_progress(&model_id_progress, step, total);
        })
        .map(|e| (e, tmp));
        clear_image_gen_progress();
        result
    })
    .await
    .map_err(|e| e.to_string())?;
    sub.media_gen_end(&model_id);
    let engine = engine_result.map_err(|e| e.to_string())?;
    let (eng, tmp) = engine;
    let engine_s = engine_name(eng);
    let n = persist_media_file(
        bus,
        dest,
        &tmp,
        &req.actor,
        &req.caps,
        &req.trace_id,
        "media.image.generate",
        &model_id,
        engine_s,
    )
    .await?;
    let _ = std::fs::remove_file(&tmp);
    maybe_auto_migrate(sub).await;
    Ok(aos_proto::MediaGenerateResponse {
        path: dest.to_string(),
        bytes: n,
        engine: engine_s.into(),
        model_id,
    })
}

pub fn default_upscaled_path(source: &str) -> String {
    let source = source.trim();
    if let Some(dot) = source.rfind('.') {
        if dot > source.rfind('/').unwrap_or(0) {
            return format!("{}-upscaled{}", &source[..dot], &source[dot..]);
        }
    }
    format!("{source}-upscaled.png")
}

fn logical_media_path(logical: &str) -> PathBuf {
    let trimmed = logical.trim();
    if trimmed.starts_with('/') {
        aos_home().join("var/storage/data").join(trimmed.trim_start_matches('/'))
    } else {
        PathBuf::from(trimmed)
    }
}

pub async fn run_image_upscale(
    bus: &aos_ipc::BusClient,
    req: &aos_proto::MediaImageUpscaleRequest,
    dest: &str,
) -> Result<aos_proto::MediaGenerateResponse, String> {
    let source_host = logical_media_path(&req.source_path);
    if !source_host.is_file() {
        return Err(format!(
            "image source introuvable: {} ({})",
            req.source_path,
            source_host.display()
        ));
    }
    let upscale_path = resolve_media_asset(Some("upscale"), &req.upscale_model).ok_or_else(|| {
        format!(
            "modèle upscale introuvable: {} (share/models/upscale/)",
            req.upscale_model
        )
    })?;
    let opts = aos_sd::UpscaleOpts {
        upscale_model_path: upscale_path,
        upscale_repeats: req.upscale_repeats.unwrap_or(1).clamp(1, 4),
        upscale_tile_size: req.upscale_tile_size.map(|t| t.clamp(32, 512)),
    };
    let tmp = unique_media_temp("aos-upscale", "png");
    let source = source_host.clone();
    let engine = tokio::task::spawn_blocking(move || {
        aos_sd::upscale_image(&source, &tmp, &opts).map(|e| (e, tmp))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    let (eng, tmp) = engine;
    let engine_s = engine_name(eng);
    let n = persist_media_file(
        bus,
        dest,
        &tmp,
        &req.actor,
        &req.caps,
        &req.trace_id,
        "media.image.upscale",
        &req.upscale_model,
        engine_s,
    )
    .await?;
    let _ = std::fs::remove_file(&tmp);
    Ok(aos_proto::MediaGenerateResponse {
        path: dest.to_string(),
        bytes: n,
        engine: engine_s.into(),
        model_id: req.upscale_model.clone(),
    })
}

pub async fn run_tts(
    sub: &crate::ModelSubsystem,
    bus: &aos_ipc::BusClient,
    req: &aos_proto::MediaAudioGenerateRequest,
    dest: &str,
) -> Result<aos_proto::MediaGenerateResponse, String> {
    use aos_placement::PlacementProfile;
    let lookup = sub.find_media_model("tts", req.model_id.as_deref());
    let (model_id, voice) = match lookup {
        Ok((id, path)) => (id, path),
        Err(_) => (
            req.model_id
                .clone()
                .unwrap_or_else(|| "local:piper-en-us".into()),
            std::path::PathBuf::from("missing.onnx"),
        ),
    };
    if voice.exists() && aos_sd::speech_engine_available() {
        sub.ensure_loaded(&model_id, PlacementProfile::CpuOnly, 0)
            .await
            .map_err(|e| format!("placement: {e}"))?;
    }
    let text = req.text.clone();
    let opts = proto_audio_opts(&req.options);
    let tmp = unique_media_temp("aos-tts", "wav");
    let engine = tokio::task::spawn_blocking(move || {
        aos_sd::generate_speech_opts(&voice, &text, &tmp, &opts).map(|e| (e, tmp))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    let (eng, tmp) = engine;
    let bytes = std::fs::read(&tmp).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&tmp);
    let engine_s = engine_name(eng);
    let n = persist_media(
        bus,
        dest,
        &bytes,
        &req.actor,
        &req.caps,
        &req.trace_id,
        "media.audio.generate",
        &model_id,
        engine_s,
    )
    .await?;
    Ok(aos_proto::MediaGenerateResponse {
        path: dest.to_string(),
        bytes: n,
        engine: engine_s.into(),
        model_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_sans_cap_refuse() {
        assert!(!actor_may_generate("agent:abc", &[]));
        assert!(actor_may_generate(
            "agent:abc",
            &["media.generate".into()]
        ));
        assert!(actor_may_generate("human:ui", &[]));
    }

    #[test]
    fn unique_media_temp_does_not_reuse_pid_path() {
        let a = unique_media_temp("aos-img", "png");
        let b = unique_media_temp("aos-img", "png");
        assert_ne!(a, b);
    }

    #[test]
    fn proto_image_opts_defaults() {
        let o = build_image_gen_opts("local:unknown", &aos_proto::MediaImageOptions::default(), "auto")
            .unwrap();
        assert_eq!(o.width, 512);
        assert_eq!(o.steps, 20);
    }

    #[test]
    fn qwen_catalog_applies_offload_when_user_leaves_option_unset() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let catalog = root.join("share/models/catalog-offerings.json");
        if !catalog.is_file() {
            return;
        }
        std::env::set_var("AOS_HOME", &root);
        let opts = build_image_gen_opts(
            "local:qwen-image-2512",
            &aos_proto::MediaImageOptions::default(),
            "auto",
        )
        .unwrap();
        assert!(opts.offload_to_cpu);
        assert!(opts.stream_layers);
        assert!(opts.diffusion_fa);
        assert_eq!(
            opts.backend.as_deref(),
            Some("te=cpu,llm=cpu,diffusion=gpu,vae=cpu")
        );
        assert_eq!(opts.params_backend.as_deref(), Some("cpu"));
    }

    #[test]
    fn user_can_disable_catalog_offload_explicitly() {
        let user = aos_proto::MediaImageOptions {
            offload_to_cpu: Some(false),
            ..Default::default()
        };
        let opts = build_image_gen_opts("local:qwen-image-2512", &user, "auto").unwrap();
        assert!(!opts.offload_to_cpu);
    }
}

fn is_heavy_image_model(model_id: &str) -> bool {
    model_id.contains("ideogram")
        || model_id.contains("flux")
        || model_id.contains("z-image")
        || model_id.contains("qwen-image")
        || model_id.contains("krea")
        || model_id.contains("wan")
        || model_id.contains("ltx")
        || model_id.contains("sdxl")
}

/// Mixed sd.cpp backend for DiT + LLM packs: encoders on CPU, diffusion on GPU.
const HEAVY_MIXED_BACKEND: &str = "te=cpu,llm=cpu,diffusion=gpu,vae=cpu";

fn build_image_gen_opts(
    model_id: &str,
    user: &aos_proto::MediaImageOptions,
    pin: &str,
) -> Result<aos_sd::ImageGenOpts, String> {
    let mut opts = aos_sd::ImageGenOpts::default();
    apply_offering_sidecars(&mut opts, model_id);
    apply_catalog_extras(&mut opts, user);
    apply_upscale(&mut opts, user)?;
    apply_user_image_opts(&mut opts, user);
    apply_heavy_image_defaults(&mut opts, model_id);
    apply_inference_backend(&mut opts, pin, model_id);
    Ok(opts)
}

fn apply_user_image_opts(opts: &mut aos_sd::ImageGenOpts, o: &aos_proto::MediaImageOptions) {
    if let Some(v) = o.width {
        opts.width = v.clamp(64, 2048);
    }
    if let Some(v) = o.height {
        opts.height = v.clamp(64, 2048);
    }
    if let Some(v) = o.steps {
        opts.steps = v.clamp(1, 150);
    }
    if o.cfg_scale.is_some() {
        opts.cfg_scale = o.cfg_scale;
    }
    if o.seed.is_some() {
        opts.seed = o.seed;
    }
    if o.sampling_method.is_some() {
        opts.sampling_method = o.sampling_method.clone();
    }
    if o.negative_prompt.is_some() {
        opts.negative_prompt = o.negative_prompt.clone();
    }
    if o.threads.is_some() {
        opts.threads = o.threads;
    }
    if o.backend.is_some() {
        opts.backend = o
            .backend
            .as_deref()
            .and_then(aos_sd::sanitize_backend_spec);
    }
    if o.params_backend.is_some() {
        opts.params_backend = o
            .params_backend
            .as_deref()
            .and_then(aos_sd::sanitize_backend_spec);
    }
    if let Some(v) = o.offload_to_cpu {
        opts.offload_to_cpu = v;
    }
    if let Some(v) = o.diffusion_fa {
        opts.diffusion_fa = v;
    }
    if let Some(v) = o.auto_fit {
        opts.auto_fit = v;
    }
    if o.max_vram.is_some() {
        opts.max_vram = o.max_vram.as_deref().and_then(aos_sd::sanitize_max_vram);
    }
    if let Some(v) = o.stream_layers {
        opts.stream_layers = v;
    }
    if let Some(v) = o.flow_shift {
        opts.flow_shift = Some(v);
    }
    if o.sd_mode.is_some() {
        opts.sd_mode = o
            .sd_mode
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
    }
    if let Some(v) = o.video_frames.filter(|n| *n > 0) {
        opts.video_frames = Some(v);
    }
}

fn apply_heavy_image_defaults(opts: &mut aos_sd::ImageGenOpts, model_id: &str) {
    if !is_heavy_image_model(model_id) {
        return;
    }
    if opts.params_backend.is_none() && (opts.offload_to_cpu || opts.stream_layers) {
        opts.params_backend = Some("cpu".into());
    }
    if opts.backend.is_none() {
        opts.backend = Some(HEAVY_MIXED_BACKEND.into());
    }
}

fn proto_audio_opts(o: &aos_proto::MediaAudioOptions) -> aos_sd::SpeechGenOpts {
    aos_sd::SpeechGenOpts {
        length_scale: o.length_scale,
        noise_scale: o.noise_scale,
        noise_w: o.noise_w,
        sentence_silence: o.sentence_silence,
        speaker: o.speaker,
    }
}

fn models_dir() -> PathBuf {
    std::env::var("AOS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("share/models")
}

const ASSET_EXTS: &[&str] = &["safetensors", "gguf", "bin", "onnx", "txt", "pth", "pt"];

fn resolve_media_asset(role: Option<&str>, id: &str) -> Option<PathBuf> {
    if id.is_empty() || id.contains("..") || id.contains('/') || id.contains('\\') {
        return None;
    }
    let dir = models_dir();
    let mut candidates = Vec::new();
    if let Some(role) = role {
        let role_dir = dir.join(role);
        candidates.push(role_dir.join(id));
        for ext in ASSET_EXTS {
            candidates.push(role_dir.join(format!("{id}.{ext}")));
        }
    }
    candidates.push(dir.join(id));
    for ext in ASSET_EXTS {
        candidates.push(dir.join(format!("{id}.{ext}")));
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn resolve_style_text(id: &str) -> Option<String> {
    if id.is_empty() {
        return None;
    }
    if let Some(path) = resolve_media_asset(Some("styles"), id) {
        if path.extension().and_then(|e| e.to_str()) == Some("txt") {
            return std::fs::read_to_string(&path)
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }
        return path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
    }
    Some(id.to_string())
}

fn lora_stem_from_id(id: &str) -> Option<String> {
    if let Some(path) = resolve_media_asset(Some("lora"), id) {
        return path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
    }
    PathBuf::from(id)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
}

fn apply_catalog_extras(opts: &mut aos_sd::ImageGenOpts, o: &aos_proto::MediaImageOptions) {
    let style_parts: Vec<String> = o
        .styles
        .iter()
        .filter_map(|id| resolve_style_text(id))
        .collect();
    if !style_parts.is_empty() {
        opts.style_prefix = Some(style_parts.join(", "));
    }
    if let Some(id) = o.vae.as_deref() {
        opts.vae_path = resolve_media_asset(Some("vae"), id);
    }
    let scale = o.lora_scale.unwrap_or(1.0).clamp(0.0, 2.0);
    for id in &o.loras {
        if let Some(stem) = lora_stem_from_id(id) {
            if !stem.is_empty()
                && !opts
                    .lora_entries
                    .iter()
                    .any(|e| e.stem.eq_ignore_ascii_case(&stem))
            {
                opts.lora_entries.push(aos_sd::LoraEntry { stem, scale });
            }
        }
    }
    if !opts.lora_entries.is_empty() && opts.lora_model_dir.is_none() {
        opts.lora_model_dir = Some(models_dir().join("lora"));
    }
}

fn apply_upscale(
    opts: &mut aos_sd::ImageGenOpts,
    o: &aos_proto::MediaImageOptions,
) -> Result<(), String> {
    let Some(id) = o.upscale_model.as_deref().filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    let path = resolve_media_asset(Some("upscale"), id).ok_or_else(|| {
        format!(
            "modèle upscale introuvable: {id} (téléchargez-le depuis Modèles ou placez-le dans share/models/upscale/)"
        )
    })?;
    opts.upscale_model_path = Some(path);
    opts.upscale_repeats = o.upscale_repeats.unwrap_or(1).clamp(1, 4);
    opts.upscale_tile_size = o.upscale_tile_size.map(|t| t.clamp(32, 512));
    Ok(())
}

fn apply_offering_sidecars(opts: &mut aos_sd::ImageGenOpts, model_id: &str) {
    let path = models_dir().join("catalog-offerings.json");
    let Ok(raw) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return;
    };
    let Some(models) = v.get("models").and_then(|m| m.as_array()) else {
        return;
    };
    let Some(m) = models.iter().find(|x| x.get("id").and_then(|i| i.as_str()) == Some(model_id))
    else {
        return;
    };
    if let Some(extras) = m.get("extra_files").and_then(|e| e.as_array()) {
        for f in extras {
            let fname = f.get("filename").and_then(|x| x.as_str()).unwrap_or("");
            let role = f.get("role").and_then(|x| x.as_str()).unwrap_or("");
            let resolved = resolve_media_asset(None, fname);
            match role {
                "vae" if opts.vae_path.is_none() => opts.vae_path = resolved,
                "lora" if opts.lora_entries.is_empty() => {
                    if let Some(path) = resolve_media_asset(Some("lora"), fname).or(resolved) {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            opts.lora_entries.push(aos_sd::LoraEntry {
                                stem: stem.to_string(),
                                scale: 1.0,
                            });
                            opts.lora_model_dir = Some(models_dir().join("lora"));
                        }
                    }
                }
                "clip_l" => opts.clip_l_path = resolved,
                "clip_g" => opts.clip_g_path = resolved,
                "t5xxl" => opts.t5xxl_path = resolved,
                "uncond" | "uncond-diffusion" => opts.uncond_diffusion_model = resolved,
                "llm" => opts.llm_path = resolved,
                "high-noise" | "high-noise-diffusion" => {
                    opts.high_noise_diffusion_model = resolved
                }
                "embeddings-connectors" | "embeddings_connectors" => {
                    opts.embeddings_connectors = resolved
                }
                "audio-vae" | "audio_vae" => opts.audio_vae_path = resolved,
                _ => {}
            }
        }
    }
    let args = m.get("engine_args").and_then(|a| a.as_object());
    let Some(args) = args else {
        return;
    };
    if opts.diffusion_model.is_none() {
        if let Some(dm) = args.get("diffusion-model").and_then(|x| x.as_str()) {
            opts.diffusion_model = resolve_media_asset(None, dm);
        }
    }
    if opts.uncond_diffusion_model.is_none() {
        if let Some(u) = args.get("uncond-diffusion-model").and_then(|x| x.as_str()) {
            opts.uncond_diffusion_model = resolve_media_asset(None, u);
        }
    }
    if opts.llm_path.is_none() {
        if let Some(llm) = args.get("llm").and_then(|x| x.as_str()) {
            opts.llm_path = resolve_media_asset(None, llm);
        }
    }
    if opts.high_noise_diffusion_model.is_none() {
        if let Some(h) = args
            .get("high-noise-diffusion-model")
            .and_then(|x| x.as_str())
        {
            opts.high_noise_diffusion_model = resolve_media_asset(None, h);
        }
    }
    if opts.embeddings_connectors.is_none() {
        if let Some(c) = args
            .get("embeddings-connectors")
            .and_then(|x| x.as_str())
        {
            opts.embeddings_connectors = resolve_media_asset(None, c);
        }
    }
    if opts.audio_vae_path.is_none() {
        if let Some(av) = args.get("audio-vae").and_then(|x| x.as_str()) {
            opts.audio_vae_path = resolve_media_asset(None, av);
        }
    }
    if opts.sd_mode.is_none() {
        if let Some(m) = args.get("mode").and_then(|x| x.as_str()) {
            opts.sd_mode = Some(m.to_string());
        }
    }
    if opts.flow_shift.is_none() {
        if let Some(fs) = args.get("flow-shift").and_then(|x| x.as_f64()) {
            opts.flow_shift = Some(fs as f32);
        } else if let Some(fs) = args.get("flow-shift").and_then(|x| x.as_str()) {
            if let Ok(v) = fs.parse::<f32>() {
                opts.flow_shift = Some(v);
            }
        }
    }
    if opts.video_frames.is_none() {
        if let Some(vf) = args.get("video-frames").and_then(|x| x.as_u64()) {
            opts.video_frames = Some(vf as u32);
        } else if let Some(vf) = args.get("video-frames").and_then(|x| x.as_str()) {
            if let Ok(v) = vf.parse::<u32>() {
                opts.video_frames = Some(v);
            }
        }
    }
    if opts.backend.is_none() {
        if let Some(b) = args
            .get("backend")
            .and_then(|x| x.as_str())
            .and_then(aos_sd::sanitize_backend_spec)
        {
            opts.backend = Some(b);
        }
    }
    if opts.params_backend.is_none() {
        if let Some(b) = args
            .get("params-backend")
            .and_then(|x| x.as_str())
            .and_then(aos_sd::sanitize_backend_spec)
        {
            opts.params_backend = Some(b);
        }
    }
    if truthy_engine_arg(args.get("offload-to-cpu")) {
        opts.offload_to_cpu = true;
    }
    if truthy_engine_arg(args.get("diffusion-fa")) {
        opts.diffusion_fa = true;
    }
    if truthy_engine_arg(args.get("auto-fit")) {
        opts.auto_fit = true;
    }
    if opts.max_vram.is_none() {
        if let Some(v) = args
            .get("max-vram")
            .and_then(|x| x.as_str())
            .and_then(aos_sd::sanitize_max_vram)
        {
            opts.max_vram = Some(v);
        }
    }
    if truthy_engine_arg(args.get("stream-layers")) {
        opts.stream_layers = true;
        opts.offload_to_cpu = true;
    }
}

fn truthy_engine_arg(v: Option<&serde_json::Value>) -> bool {
    match v {
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::String(s)) => {
            matches!(s.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
        }
        Some(serde_json::Value::Number(n)) => n.as_u64() == Some(1),
        _ => false,
    }
}

fn apply_inference_backend(opts: &mut aos_sd::ImageGenOpts, pin: &str, model_id: &str) {
    if opts.auto_fit {
        return;
    }
    match pin.trim().to_ascii_lowercase().as_str() {
        "cpu" => {
            opts.backend = Some("cpu".into());
        }
        "gpu" if opts.backend.is_none() && is_heavy_image_model(model_id) => {
            opts.backend = Some(HEAVY_MIXED_BACKEND.into());
            if opts.params_backend.is_none() {
                opts.params_backend = Some("cpu".into());
            }
        }
        "gpu" if opts.backend.is_none() => {
            opts.backend = Some("gpu".into());
        }
        _ => {}
    }
}

async fn maybe_auto_migrate(sub: &crate::ModelSubsystem) {
    let pin = sub.inference_pin();
    if pin.eq_ignore_ascii_case("gpu") || pin.eq_ignore_ascii_case("cpu") {
        return;
    }
    if !sub.has_live_infer() {
        return;
    }
    let _ = sub.migrate("auto").await;
}
