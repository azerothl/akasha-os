//! # aos-scene — SceneGraph core (Illustration Studio foundation)
//!
//! Numeric contract is frozen in ADR 0011:
//! right-handed, **Y-up**, forward **−Z**, metres, quaternion **`[x,y,z,w]`**,
//! TRS compose `T * R * S`, column-major matrices.
//!
//! Backends convert; this crate never speaks Blender Z-up.

mod math;
mod ops;
mod project;
mod render_stub;
mod scene;

pub use math::{Mat4, Quat, Vec3, EPSILON};
pub use ops::{SceneOp, UndoStack};
pub use project::{load_project_yaml, save_project_yaml, ProjectFile, PROJECT_FORMAT_VERSION};
pub use render_stub::{stub_beauty_png, STUB_BEAUTY_SIZE};
pub use scene::{
    CameraParams, NodeKind, SceneGraph, SceneNode, Transform, DEFAULT_SCENE_YAML_PATH,
    ILLUSTRATIONS_DOCUMENTS_PREFIX,
};

/// Cap: read illustration project documents.
pub const ILLUSTRATION_FS_READ_CAP: &str = "fs.read:/documents/illustrations/**";
/// Cap: write illustration project documents.
pub const ILLUSTRATION_FS_WRITE_CAP: &str = "fs.write:/documents/illustrations/**";
/// Cap: run the host stub beauty-pass renderer (no Blender / no GPU engine).
pub const RENDER_STUB_CAP: &str = "render.stub";

/// Host DeclUI service id for the stub beauty pass.
pub const RENDER_STUB_SERVICE: &str = "render.stub.beauty";
