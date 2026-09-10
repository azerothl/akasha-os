//! Declarative `media.image.generate` job orchestration (issue #150 lot 2).
//!
//! Reuses the native progress ticker file and bus services; job handles use human
//! labels in the UI via `i18n::job_state_human_label`.

use aos_proto::rich_decl_ui::{RichJobHandle, RichJobProgress};
use aos_proto::{MediaGenerateResponse, MediaImageGenerateRequest};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

static MEDIA_JOBS: LazyLock<Mutex<MediaJobRegistry>> =
    LazyLock::new(|| Mutex::new(MediaJobRegistry::default()));

pub fn media_jobs() -> &'static Mutex<MediaJobRegistry> {
    &MEDIA_JOBS
}

pub fn image_gen_progress_path() -> PathBuf {
    crate::os_open::aos_home().join("var/run/image-gen-progress.json")
}

pub fn read_image_gen_progress_file() -> Option<(u32, u32)> {
    let raw = std::fs::read_to_string(image_gen_progress_path()).ok()?;
    let v = serde_json::from_str::<Value>(&raw).ok()?;
    let step = v.get("step")?.as_u64()? as u32;
    let total = v.get("total")?.as_u64()? as u32;
    Some((step, total))
}

#[derive(Debug)]
struct ActiveMediaJob {
    job_id: String,
    subscription_id: String,
    steps: u32,
}

#[derive(Default)]
pub struct MediaJobRegistry {
    jobs: HashMap<String, ActiveMediaJob>,
    last_progress: HashMap<String, Instant>,
}

impl MediaJobRegistry {
    pub fn register(&mut self, job_id: &str, subscription_id: &str, steps: u32) {
        self.jobs.insert(
            job_id.to_string(),
            ActiveMediaJob {
                job_id: job_id.to_string(),
                subscription_id: subscription_id.to_string(),
                steps,
            },
        );
    }

    pub fn remove(&mut self, job_id: &str) {
        self.jobs.remove(job_id);
        self.last_progress.remove(job_id);
    }

    pub fn contains(&self, job_id: &str) -> bool {
        self.jobs.contains_key(job_id)
    }

    pub fn allow_progress_emit(&mut self, job_id: &str) -> bool {
        let now = Instant::now();
        let min_gap = Duration::from_millis(100);
        if let Some(prev) = self.last_progress.get(job_id) {
            if now.duration_since(*prev) < min_gap {
                return false;
            }
        }
        self.last_progress.insert(job_id.to_string(), now);
        true
    }
}

pub fn media_job_queued(job_id: &str, steps: u32) -> RichJobHandle {
    RichJobHandle {
        job_id: Some(job_id.into()),
        kind: Some("media.image.generate".into()),
        state: Some("queued".into()),
        progress: Some(RichJobProgress {
            completed: 0,
            total: steps,
            unit: Some("step".into()),
        }),
        result: None,
        error: None,
    }
}

pub fn media_job_running(job_id: &str, step: u32, total: u32) -> RichJobHandle {
    RichJobHandle {
        job_id: Some(job_id.into()),
        kind: Some("media.image.generate".into()),
        state: Some("running".into()),
        progress: Some(RichJobProgress {
            completed: step,
            total,
            unit: Some("step".into()),
        }),
        result: None,
        error: None,
    }
}

pub fn media_job_succeeded(
    job_id: &str,
    response: &MediaGenerateResponse,
    prompt: &str,
) -> RichJobHandle {
    RichJobHandle {
        job_id: Some(job_id.into()),
        kind: Some("media.image.generate".into()),
        state: Some("succeeded".into()),
        progress: None,
        result: Some(json!({
            "path": response.path,
            "bytes": response.bytes,
            "engine": response.engine,
            "model_id": response.model_id,
            "prompt": prompt,
        })),
        error: None,
    }
}

pub fn media_job_failed(job_id: &str, message: &str) -> RichJobHandle {
    RichJobHandle {
        job_id: Some(job_id.into()),
        kind: Some("media.image.generate".into()),
        state: Some("failed".into()),
        progress: None,
        result: None,
        error: Some(message.into()),
    }
}

pub fn media_job_cancelled(job_id: &str, step: u32, total: u32) -> RichJobHandle {
    RichJobHandle {
        job_id: Some(job_id.into()),
        kind: Some("media.image.generate".into()),
        state: Some("cancelled".into()),
        progress: Some(RichJobProgress {
            completed: step,
            total,
            unit: Some("step".into()),
        }),
        result: None,
        error: None,
    }
}

/// Create acceptance must never treat the Preview PNG fallback as a real
/// generation. Other callers may still use the fallback explicitly.
pub fn media_engine_is_real(engine: &str) -> bool {
    !engine.trim().is_empty() && !engine.trim().eq_ignore_ascii_case("stub")
}

pub fn parse_media_generate_request(input: &Value) -> Result<MediaImageGenerateRequest, String> {
    serde_json::from_value(input.clone())
        .map_err(|e| format!("invalid media.image.generate input: {e}"))
}

pub fn new_media_job_id() -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("media-{ts}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_job_terminal_states() {
        let ok = media_job_succeeded(
            "j1",
            &MediaGenerateResponse {
                path: "/downloads/image-1.png".into(),
                bytes: 42,
                engine: "sdcpp".into(),
                model_id: "local:sd".into(),
            },
            "a cat",
        );
        assert_eq!(ok.state.as_deref(), Some("succeeded"));
        assert_eq!(
            ok.result
                .as_ref()
                .and_then(|r| r.get("path"))
                .and_then(|p| p.as_str()),
            Some("/downloads/image-1.png")
        );
        let fail = media_job_failed("j1", "boom");
        assert_eq!(fail.state.as_deref(), Some("failed"));
        let cancelled = media_job_cancelled("j1", 2, 10);
        assert_eq!(cancelled.state.as_deref(), Some("cancelled"));
    }

    #[test]
    fn preview_stub_is_not_a_real_engine_result() {
        assert!(!media_engine_is_real("stub"));
        assert!(!media_engine_is_real(" STUB "));
        assert!(!media_engine_is_real(""));
        assert!(media_engine_is_real("sdcpp"));
    }
}
