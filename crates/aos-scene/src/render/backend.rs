//! Render backend trait + request types.

use crate::scene::SceneGraph;
use crate::style::ResolvedStyle;
use thiserror::Error;

/// Stable backend identifiers (DeclUI / caps map to these).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderBackendId {
    /// Solid placeholder PNG — always available offline.
    Stub,
    /// CPU SceneGraph wireframe / flat beauty (no GPU, no Blender).
    Cpu,
    /// Optional Blender beauty via isolated Renderer Pack (or deterministic mock).
    Blender,
}

impl RenderBackendId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stub => "stub",
            Self::Cpu => "cpu",
            Self::Blender => "blender",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "stub" | "solid" => Some(Self::Stub),
            "cpu" | "cpu_wireframe" | "wireframe" => Some(Self::Cpu),
            "blender" | "beauty_blender" | "eevee" | "cycles" => Some(Self::Blender),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderPassKind {
    /// Beauty placeholder / flat shaded pass.
    Beauty,
    /// Wireframe-only edges.
    Wireframe,
}

impl RenderPassKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Beauty => "beauty",
            Self::Wireframe => "wireframe",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "beauty" | "beauty_pass" => Some(Self::Beauty),
            "wireframe" | "wire" => Some(Self::Wireframe),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RenderRequest {
    pub scene: SceneGraph,
    pub pass: RenderPassKind,
    pub width: u32,
    pub height: u32,
    /// Optional stub solid color (ignored by CPU backend).
    pub stub_rgb: (u8, u8, u8),
    /// Optional NPR style (Sketch / Pencil / Ink / Comic-Manga). `None` = legacy wireframe look.
    pub style: Option<ResolvedStyle>,
    pub preset: Option<crate::fx::RenderPreset>,
}

#[derive(Debug, Clone)]
pub struct RenderOutput {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub backend_id: RenderBackendId,
    pub pass: RenderPassKind,
    pub engine: &'static str,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RenderError {
    #[error("render job capacity reached ({0} retained jobs)")]
    JobCapacityReached(usize),
    #[error("unknown backend `{0}`")]
    UnknownBackend(String),
    #[error("unknown pass `{0}`")]
    UnknownPass(String),
    #[error("unknown job `{0}`")]
    UnknownJob(String),
    #[error("job `{0}` not finished")]
    JobNotReady(String),
    #[error("job `{0}` failed: {1}")]
    JobFailed(String, String),
    #[error("invalid dimensions {0}x{1}")]
    InvalidDimensions(u32, u32),
    #[error("scene: {0}")]
    Scene(String),
    #[error("encode: {0}")]
    Encode(String),
    #[error("path outside illustrations tree: {0}")]
    PathDenied(String),
    #[error("backend unavailable: {0}")]
    BackendUnavailable(String),
    #[error("isolation: {0}")]
    Isolation(String),
    #[error("unknown style `{0}`")]
    UnknownStyle(String),
}

/// Pluggable render backend (host-side; never Blender / bpy).
pub trait RenderBackend: Send + Sync {
    fn id(&self) -> RenderBackendId;
    fn render(&self, req: &RenderRequest) -> Result<RenderOutput, RenderError>;
}
