//! E16 media generate: cap check helpers + dest paths.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static MEDIA_TMP_SEQ: AtomicU64 = AtomicU64::new(1);

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
    let mut opts = proto_image_opts(&req.options);
    apply_catalog_extras(&mut opts, &req.options);
    apply_offering_sidecars(&mut opts, &model_id);
    let tmp = unique_media_temp("aos-img", "png");
    let engine = tokio::task::spawn_blocking(move || {
        aos_sd::generate_image_opts(&weights, &prompt, &tmp, &opts).map(|e| (e, tmp))
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
        "media.image.generate",
        &model_id,
        engine_s,
    )
    .await?;
    maybe_auto_migrate(sub).await;
    Ok(aos_proto::MediaGenerateResponse {
        path: dest.to_string(),
        bytes: n,
        engine: engine_s.into(),
        model_id,
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
        let o = proto_image_opts(&aos_proto::MediaImageOptions::default());
        assert_eq!(o.width, 512);
        assert_eq!(o.steps, 20);
    }
}

fn proto_image_opts(o: &aos_proto::MediaImageOptions) -> aos_sd::ImageGenOpts {
    aos_sd::ImageGenOpts {
        width: o.width.unwrap_or(512).clamp(64, 2048),
        height: o.height.unwrap_or(512).clamp(64, 2048),
        steps: o.steps.unwrap_or(20).clamp(1, 150),
        cfg_scale: o.cfg_scale,
        seed: o.seed,
        sampling_method: o.sampling_method.clone(),
        negative_prompt: o.negative_prompt.clone(),
        threads: o.threads,
        style_prefix: o.style.clone(),
        ..aos_sd::ImageGenOpts::default()
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

fn resolve_media_asset(id: &str) -> Option<PathBuf> {
    if id.is_empty() || id.contains("..") || id.contains('/') || id.contains('\\') {
        return None;
    }
    let dir = models_dir();
    let p = dir.join(id);
    if p.is_file() {
        return Some(p);
    }
    for ext in ["safetensors", "gguf", "bin", "onnx"] {
        let p = dir.join(format!("{id}.{ext}"));
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn apply_catalog_extras(opts: &mut aos_sd::ImageGenOpts, o: &aos_proto::MediaImageOptions) {
    if let Some(id) = o.vae.as_deref() {
        opts.vae_path = resolve_media_asset(id);
    }
    if let Some(id) = o.lora.as_deref() {
        opts.lora_path = resolve_media_asset(id);
    }
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
            let resolved = resolve_media_asset(fname);
            match role {
                "vae" if opts.vae_path.is_none() => opts.vae_path = resolved,
                "lora" if opts.lora_path.is_none() => opts.lora_path = resolved,
                "clip_l" => opts.clip_l_path = resolved,
                "clip_g" => opts.clip_g_path = resolved,
                "t5xxl" => opts.t5xxl_path = resolved,
                _ => {}
            }
        }
    }
    if let Some(dm) = m
        .get("engine_args")
        .and_then(|a| a.get("diffusion-model"))
        .and_then(|x| x.as_str())
    {
        opts.diffusion_model = resolve_media_asset(dm);
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
