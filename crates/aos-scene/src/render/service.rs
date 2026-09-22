//! In-process RenderService: submit / status / result.

use super::backend::{
    RenderBackend, RenderBackendId, RenderError, RenderPassKind, RenderRequest,
};
use super::blender::BlenderRenderBackend;
use super::cpu::CpuWireframeBackend;
use super::stub::StubRenderBackend;
use crate::scene::{SceneGraph, ILLUSTRATIONS_DOCUMENTS_PREFIX};
use crate::style::{parse_optional_style, ResolvedStyle};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Default backend id string for DeclUI when omitted.
pub const DEFAULT_RENDER_BACKEND: &str = "stub";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RenderSubmit {
    pub scene: SceneGraph,
    pub backend: RenderBackendId,
    pub pass: RenderPassKind,
    pub width: u32,
    pub height: u32,
    pub output_path: String,
    pub stub_rgb: (u8, u8, u8),
    /// Optional NPR style id resolved before submit (or `None` for legacy look).
    pub style: Option<ResolvedStyle>,
}

#[derive(Debug, Clone)]
pub struct RenderJobStatus {
    pub job_id: String,
    pub state: JobState,
    pub backend: RenderBackendId,
    pub pass: RenderPassKind,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RenderResult {
    pub job_id: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub backend: RenderBackendId,
    pub pass: RenderPassKind,
    pub png: Vec<u8>,
}

struct JobRecord {
    status: RenderJobStatus,
    result: Option<RenderResult>,
}

/// Host-owned render orchestrator. Backends are sync for stub/cpu; jobs still
/// expose submit/status/result so a future Blender pack can stay async.
pub struct RenderService {
    backends: HashMap<RenderBackendId, Arc<dyn RenderBackend>>,
    default_backend: RenderBackendId,
    jobs: Mutex<HashMap<String, JobRecord>>,
    next_id: AtomicU64,
}

impl Default for RenderService {
    fn default() -> Self {
        Self::with_default_backends()
    }
}

impl RenderService {
    pub fn with_default_backends() -> Self {
        let mut backends: HashMap<RenderBackendId, Arc<dyn RenderBackend>> = HashMap::new();
        backends.insert(RenderBackendId::Stub, Arc::new(StubRenderBackend));
        backends.insert(RenderBackendId::Cpu, Arc::new(CpuWireframeBackend));
        backends.insert(RenderBackendId::Blender, Arc::new(BlenderRenderBackend::default()));
        Self {
            backends,
            default_backend: RenderBackendId::Stub,
            jobs: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    pub fn default_backend(&self) -> RenderBackendId {
        self.default_backend
    }

    pub fn has_backend(&self, id: RenderBackendId) -> bool {
        self.backends.contains_key(&id)
    }

    /// Validate output path stays under the illustrations document tree.
    pub fn assert_illustration_path(path: &str) -> Result<(), RenderError> {
        if path.starts_with(ILLUSTRATIONS_DOCUMENTS_PREFIX) && !path.contains("..") {
            Ok(())
        } else {
            Err(RenderError::PathDenied(path.into()))
        }
    }

    pub fn submit(&self, submit: RenderSubmit) -> Result<String, RenderError> {
        Self::assert_illustration_path(&submit.output_path)?;
        if submit.width == 0 || submit.height == 0 {
            return Err(RenderError::InvalidDimensions(submit.width, submit.height));
        }
        let backend = self
            .backends
            .get(&submit.backend)
            .ok_or_else(|| RenderError::UnknownBackend(submit.backend.as_str().into()))?
            .clone();

        let job_id = format!("rj-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        {
            let mut jobs = self.jobs.lock().expect("render jobs lock");
            jobs.insert(
                job_id.clone(),
                JobRecord {
                    status: RenderJobStatus {
                        job_id: job_id.clone(),
                        state: JobState::Running,
                        backend: submit.backend,
                        pass: submit.pass,
                        error: None,
                    },
                    result: None,
                },
            );
        }

        let req = RenderRequest {
            scene: submit.scene,
            pass: submit.pass,
            width: submit.width,
            height: submit.height,
            stub_rgb: submit.stub_rgb,
            style: submit.style,
        };

        match backend.render(&req) {
            Ok(out) => {
                let result = RenderResult {
                    job_id: job_id.clone(),
                    path: submit.output_path.clone(),
                    width: out.width,
                    height: out.height,
                    backend: out.backend_id,
                    pass: out.pass,
                    png: out.png,
                };
                let mut jobs = self.jobs.lock().expect("render jobs lock");
                if let Some(rec) = jobs.get_mut(&job_id) {
                    rec.status.state = JobState::Succeeded;
                    rec.result = Some(result);
                }
                Ok(job_id)
            }
            Err(e) => {
                let msg = e.to_string();
                let mut jobs = self.jobs.lock().expect("render jobs lock");
                if let Some(rec) = jobs.get_mut(&job_id) {
                    rec.status.state = JobState::Failed;
                    rec.status.error = Some(msg.clone());
                }
                Err(RenderError::JobFailed(job_id, msg))
            }
        }
    }

    /// Convenience: submit and return the completed result (stub/cpu are sync).
    pub fn submit_and_result(&self, submit: RenderSubmit) -> Result<RenderResult, RenderError> {
        let job_id = self.submit(submit)?;
        self.result(&job_id)
    }

    pub fn status(&self, job_id: &str) -> Result<RenderJobStatus, RenderError> {
        let jobs = self.jobs.lock().expect("render jobs lock");
        jobs.get(job_id)
            .map(|r| r.status.clone())
            .ok_or_else(|| RenderError::UnknownJob(job_id.into()))
    }

    pub fn result(&self, job_id: &str) -> Result<RenderResult, RenderError> {
        let jobs = self.jobs.lock().expect("render jobs lock");
        let rec = jobs
            .get(job_id)
            .ok_or_else(|| RenderError::UnknownJob(job_id.into()))?;
        match rec.status.state {
            JobState::Succeeded => rec
                .result
                .clone()
                .ok_or_else(|| RenderError::JobNotReady(job_id.into())),
            JobState::Failed => Err(RenderError::JobFailed(
                job_id.into(),
                rec.status
                    .error
                    .clone()
                    .unwrap_or_else(|| "failed".into()),
            )),
            _ => Err(RenderError::JobNotReady(job_id.into())),
        }
    }

    /// Helper used by DeclUI when only a solid stub is requested.
    pub fn stub_beauty(
        &self,
        path: &str,
        rgb: (u8, u8, u8),
    ) -> Result<RenderResult, RenderError> {
        self.submit_and_result(RenderSubmit {
            scene: SceneGraph::demo_scene(),
            backend: RenderBackendId::Stub,
            pass: RenderPassKind::Beauty,
            width: 64,
            height: 64,
            output_path: path.into(),
            stub_rgb: rgb,
            style: None,
        })
    }
}

/// Parse backend id from DeclUI input with default.
pub fn parse_backend(s: Option<&str>) -> Result<RenderBackendId, RenderError> {
    match s {
        None | Some("") => Ok(RenderBackendId::Stub),
        Some(v) => RenderBackendId::parse(v)
            .ok_or_else(|| RenderError::UnknownBackend(v.into())),
    }
}

pub fn parse_pass(s: Option<&str>) -> Result<RenderPassKind, RenderError> {
    match s {
        None | Some("") => Ok(RenderPassKind::Beauty),
        Some(v) => {
            RenderPassKind::parse(v).ok_or_else(|| RenderError::UnknownPass(v.into()))
        }
    }
}

/// Parse optional NPR style from DeclUI (`style` / `style_id`). Fail-closed on unknown.
pub fn parse_style(s: Option<&str>) -> Result<Option<ResolvedStyle>, RenderError> {
    parse_optional_style(s).map_err(|e| match e {
        crate::style::StyleError::UnknownStyle(id) => RenderError::UnknownStyle(id),
        other => RenderError::Scene(other.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::SceneGraph;

    #[test]
    fn submit_status_result_stub() {
        let svc = RenderService::default();
        let job = svc
            .submit(RenderSubmit {
                scene: SceneGraph::demo_scene(),
                backend: RenderBackendId::Stub,
                pass: RenderPassKind::Beauty,
                width: 64,
                height: 64,
                output_path: "/documents/illustrations/beauty-stub.png".into(),
                stub_rgb: (10, 20, 30),
                style: None,
            })
            .expect("submit");
        let st = svc.status(&job).expect("status");
        assert_eq!(st.state, JobState::Succeeded);
        let res = svc.result(&job).expect("result");
        assert!(res.png.starts_with(&[0x89, 0x50, 0x4e, 0x47]));
        assert_eq!(res.path, "/documents/illustrations/beauty-stub.png");
    }

    #[test]
    fn denies_path_outside_tree() {
        let svc = RenderService::default();
        let err = svc
            .submit(RenderSubmit {
                scene: SceneGraph::demo_scene(),
                backend: RenderBackendId::Stub,
                pass: RenderPassKind::Beauty,
                width: 64,
                height: 64,
                output_path: "/tmp/evil.png".into(),
                stub_rgb: (0, 0, 0),
                style: None,
            })
            .unwrap_err();
        assert!(matches!(err, RenderError::PathDenied(_)));
    }

    #[test]
    fn cpu_backend_job() {
        let svc = RenderService::default();
        let res = svc
            .submit_and_result(RenderSubmit {
                scene: SceneGraph::demo_scene(),
                backend: RenderBackendId::Cpu,
                pass: RenderPassKind::Wireframe,
                width: 96,
                height: 72,
                output_path: "/documents/illustrations/beauty-cpu.png".into(),
                stub_rgb: (0, 0, 0),
                style: None,
            })
            .expect("cpu");
        assert_eq!(res.backend, RenderBackendId::Cpu);
        assert_eq!(res.width, 96);
    }

    #[test]
    fn cpu_npr_pencil_job() {
        let svc = RenderService::default();
        let style = crate::style::resolve_style("pencil").unwrap();
        let res = svc
            .submit_and_result(RenderSubmit {
                scene: SceneGraph::demo_scene(),
                backend: RenderBackendId::Cpu,
                pass: RenderPassKind::Beauty,
                width: 128,
                height: 96,
                output_path: "/documents/illustrations/beauty-pencil.png".into(),
                stub_rgb: (0, 0, 0),
                style: Some(style),
            })
            .expect("cpu pencil");
        assert!(res.png.starts_with(&[0x89, 0x50, 0x4e, 0x47]));
    }

    #[test]
    fn blender_backend_registered() {
        let svc = RenderService::default();
        assert!(svc.has_backend(RenderBackendId::Blender));
        // Auto/mock path must succeed without a Blender binary.
        let res = svc
            .submit_and_result(RenderSubmit {
                scene: SceneGraph::demo_scene(),
                backend: RenderBackendId::Blender,
                pass: RenderPassKind::Beauty,
                width: 80,
                height: 60,
                output_path: "/documents/illustrations/beauty-blender.png".into(),
                stub_rgb: (0, 0, 0),
                style: None,
            })
            .expect("blender auto/mock");
        assert_eq!(res.backend, RenderBackendId::Blender);
        assert!(res.png.starts_with(&[0x89, 0x50, 0x4e, 0x47]));
    }

    #[test]
    fn unknown_style_rejected() {
        let err = parse_style(Some("watercolor_wet")).unwrap_err();
        assert!(matches!(err, RenderError::UnknownStyle(_)));
    }
}
