//! Backend-agnostic RenderService (Illustration Studio product suite).
//!
//! Job-shaped ABI: `submit` → `status` → `result`. Backends are pluggable;
//! default is the solid PNG **stub**. Optional **cpu** backend draws a
//! SceneGraph wireframe/beauty pass in software (no Blender, no wgpu).
//! Optional **blender** backend talks to an isolated Renderer Pack (or mock).

mod backend;
mod blender;
mod cpu;
mod export;
mod isolate;
mod service;
mod stub;

pub use backend::{
    RenderBackend, RenderBackendId, RenderError, RenderOutput, RenderPassKind, RenderRequest,
};
pub use blender::{blender_pack_status, BlenderRenderBackend, DEFAULT_BLENDER_BEAUTY_PATH};
pub use cpu::CpuWireframeBackend;
pub use export::{AkashaSceneExport, AKASHA_SCENE_EXPORT_VERSION};
pub use isolate::{isolation_matrix, BlenderPackStatus, BlenderRunMode, IsolationRow};
pub use service::{
    parse_backend, parse_pass, parse_style, JobState, RenderJobStatus, RenderResult, RenderService,
    RenderSubmit, DEFAULT_RENDER_BACKEND,
};
pub use stub::StubRenderBackend;
