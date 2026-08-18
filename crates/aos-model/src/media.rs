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
    let tmp = unique_media_temp("aos-img", "png");
    let engine = tokio::task::spawn_blocking(move || {
        aos_sd::generate_image(&weights, &prompt, &tmp).map(|e| (e, tmp))
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
    let tmp = unique_media_temp("aos-tts", "wav");
    let engine = tokio::task::spawn_blocking(move || {
        aos_sd::generate_speech(&voice, &text, &tmp).map(|e| (e, tmp))
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
}
