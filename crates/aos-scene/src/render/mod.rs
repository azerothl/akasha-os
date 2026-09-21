//! Backend-agnostic RenderService (Illustration Studio product suite).
//!
//! Job-shaped ABI: `submit` → `status` → `result`. Backends are pluggable;
//! default is the solid PNG **stub**. Optional **cpu** backend draws a
//! SceneGraph wireframe/beauty pass in software (no Blender, no wgpu).

mod backend;
mod cpu;
mod service;
mod stub;

pub use backend::{
    RenderBackend, RenderBackendId, RenderError, RenderOutput, RenderPassKind, RenderRequest,
};
pub use cpu::CpuWireframeBackend;
pub use service::{
    parse_backend, parse_pass, JobState, RenderJobStatus, RenderResult, RenderService,
    RenderSubmit, DEFAULT_RENDER_BACKEND,
};
pub use stub::StubRenderBackend;
