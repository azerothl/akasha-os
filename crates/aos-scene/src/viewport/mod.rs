//! Interactive **edit viewport** (wgpu) — approximate realtime mesh view.
//!
//! This is **not** [`crate::RenderService`] beauty / NPR. SceneGraph remains the
//! only source of truth (ADR 0011); the viewport only reads world matrices for
//! `MeshBox` nodes and draws a lit/unlit solid + optional wire overlay.
//!
//! Caps: no `render.*` backend is registered here — beauty stays stub/CPU/Blender.

mod gpu;
mod mesh;

pub use gpu::{ViewportError, ViewportRenderer};
pub use mesh::{
    collect_mesh_instances, eye_from_orbit, look_at_rh, perspective_rh, project_point_ndc,
    MeshInstance, ViewportCamera, UNIT_CUBE_HALF,
};

/// Human-facing distinction for docs / chrome: edit view ≠ beauty pass.
pub const VIEWPORT_ROLE: &str = "edit_view";
/// Beauty / NPR jobs go through RenderService backends, never this viewport.
pub const BEAUTY_ROLE: &str = "render_service_beauty";
