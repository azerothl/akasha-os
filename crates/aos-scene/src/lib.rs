//! # aos-scene — SceneGraph core (Illustration Studio)
//!
//! Numeric contract is frozen in ADR 0011:
//! right-handed, **Y-up**, forward **−Z**, metres, quaternion **`[x,y,z,w]`**,
//! TRS compose `T * R * S`, column-major matrices.
//!
//! Backends convert; this crate never speaks Blender Z-up.
//!
//! Product suite: backend-agnostic [`RenderService`] (stub + CPU + optional
//! Blender-isolated beauty) and minimal [`assets`] pack format. Blender / `bpy`
//! stay in the separate Renderer Pack — never linked here.
//!
//! Edit viewport: [`viewport`] is a wgpu lit MeshBox view of the same SceneGraph
//! (DeclUI `scene3d`). It is **not** a beauty / NPR RenderService backend.

mod assets;
mod math;
mod ops;
mod png;
mod project;
mod render;
mod render_stub;
mod scene;
pub mod viewport;

pub use assets::{
    assert_asset_path, embedded_primitives_pack, instantiate_asset, load_asset_pack_yaml,
    AssetEntry, AssetError, AssetPack, InstantiateResult, PrefabNode, PrefabNodeKind,
    ASSET_ILLUSTRATION_READ_CAP, ASSET_PACK_FORMAT_VERSION, EMBEDDED_PRIMITIVES_PACK_YAML,
    ILLUSTRATION_ASSETS_PREFIX,
};
pub use math::{Mat4, Quat, Vec3, EPSILON};
pub use ops::{SceneOp, UndoStack};
pub use project::{load_project_yaml, save_project_yaml, ProjectFile, PROJECT_FORMAT_VERSION};
pub use render::{
    isolation_matrix, parse_backend, parse_pass, AkashaSceneExport, BlenderRenderBackend,
    BlenderRunMode, CpuWireframeBackend, IsolationRow, JobState, RenderBackend, RenderBackendId,
    RenderError, RenderJobStatus, RenderOutput, RenderPassKind, RenderRequest, RenderResult,
    RenderService, RenderSubmit, StubRenderBackend, AKASHA_SCENE_EXPORT_VERSION,
    DEFAULT_BLENDER_BEAUTY_PATH, DEFAULT_RENDER_BACKEND,
};
pub use render_stub::{stub_beauty_png, STUB_BEAUTY_SIZE};
pub use scene::{
    CameraParams, NodeKind, SceneGraph, SceneNode, Transform, DEFAULT_SCENE_YAML_PATH,
    ILLUSTRATIONS_DOCUMENTS_PREFIX,
};
pub use viewport::{
    collect_mesh_instances, eye_from_orbit, look_at_rh, perspective_rh, project_point_ndc,
    MeshInstance, ViewportCamera, ViewportError, ViewportRenderer, BEAUTY_ROLE, VIEWPORT_ROLE,
};

/// Cap: read illustration project documents.
pub const ILLUSTRATION_FS_READ_CAP: &str = "fs.read:/documents/illustrations/**";
/// Cap: write illustration project documents.
pub const ILLUSTRATION_FS_WRITE_CAP: &str = "fs.write:/documents/illustrations/**";
/// Cap: run the host stub beauty-pass renderer (no Blender / no GPU engine).
pub const RENDER_STUB_CAP: &str = "render.stub";
/// Cap: run the host CPU SceneGraph wireframe / beauty backend.
pub const RENDER_CPU_CAP: &str = "render.cpu";
/// Cap: run the isolated Blender beauty backend (Renderer Pack or mock).
pub const RENDER_BLENDER_CAP: &str = "render.blender";

/// Host DeclUI service id for the stub beauty pass (legacy thin wrapper).
pub const RENDER_STUB_SERVICE: &str = "render.stub.beauty";
/// DeclUI: submit a render job through RenderService.
pub const RENDER_SUBMIT_SERVICE: &str = "render.submit";
/// DeclUI: poll render job status.
pub const RENDER_STATUS_SERVICE: &str = "render.status";
/// DeclUI: fetch render job result metadata (+ path).
pub const RENDER_RESULT_SERVICE: &str = "render.result";
/// DeclUI: instantiate an asset pack entry into the SceneGraph.
pub const ASSET_INSTANTIATE_SERVICE: &str = "asset.instantiate";
