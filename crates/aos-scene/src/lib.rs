//! # aos-scene — SceneGraph core (Illustration Studio)
//!
//! Numeric contract is frozen in ADR 0011:
//! right-handed, **Y-up**, forward **−Z**, metres, quaternion **`[x,y,z,w]`**,
//! TRS compose `T * R * S`, column-major matrices.
//!
//! Backends convert; this crate never speaks Blender Z-up.
//!
//! Product suite: backend-agnostic [`RenderService`] (stub + CPU + optional
//! Blender-isolated beauty), NPR [`style`] packs (Sketch / Pencil / Ink / Comic-Manga), [`comic`] page/panel layouts, [`storyboard`] shot timeline, local [`pack_catalogue`] + marketplace hooks, and
//! minimal [`assets`] pack format. Blender / `bpy` stay in the separate
//! Renderer Pack — never linked here.
//!
//! Edit viewport: [`viewport`] (feature `viewport`) is a wgpu lit MeshBox view
//! of the same SceneGraph (DeclUI `scene3d`). It is **not** a beauty / NPR
//! RenderService backend. Platform host_call paths depend on this crate with
//! `default-features = false`.
//!
//! Neural mesh assist: [`neural_mesh`] (`mesh.assist`) inserts validated MeshBox
//! props (stub) or [`mesh_asset`] GLB nodes (neural pack mock / spawn) into the
//! SceneGraph. Preview default is the procedural stub; neural is fail-closed
//! without the opt-in Model Pack (no GGUF weights in this crate).

mod assets;
mod camera;
mod comic;
mod compose;
mod scene_intent;
mod object_edit;
mod edit;
pub mod animation;
pub mod history;
pub mod fx;
mod ik;
mod light;
mod locks;
mod math;
mod mesh_asset;
mod neural_mesh;
mod neural_mesh_isolate;
mod ops;
mod pack_catalogue;
mod png;
mod pose;
mod project;
mod render;
mod render_stub;
mod scene;
mod storyboard;
mod style;
#[cfg(feature = "viewport")]
pub mod viewport;

pub use assets::{
    assert_asset_path, embedded_primitives_pack, instantiate_asset, load_asset_pack_yaml,
    AssetEntry, AssetError, AssetPack, InstantiateResult, PrefabNode, PrefabNodeKind,
    ASSET_ILLUSTRATION_READ_CAP, ASSET_PACK_FORMAT_VERSION, EMBEDDED_PRIMITIVES_PACK_YAML,
    ILLUSTRATION_ASSETS_PREFIX,
};
pub use camera::{
    active_camera_eye_target, apply_orbit_to_active_camera, camera_transform_look_at,
    eye_from_orbit, fovy_from_hfov, hfov_from_fovy, hfov_rad, orbit_from_active_camera,
    orbit_from_eye_target, rotation_look_at, set_focal_from_hfov,
};
pub use compose::{
    compose_from_prompt, compose_from_prompt_with_pack, ComposeError, ComposeIntent, ComposeResult,
    SCENE_COMPOSE_CAP, SCENE_COMPOSE_SERVICE,
};
pub use scene_intent::{
    parse_scene_intent, plan_scene_intent, IntentError, IntentObject, RelationKind, SceneIntent,
    SpatialRelation,
};
pub use object_edit::{align_nodes, duplicate_nodes, group_nodes, snap_nodes, AlignMode, EditAxis};
pub use comic::{
    apply_comic_layout, bind_panel_scene, layout_rects, load_comic_yaml, render_comic_page,
    save_comic_yaml, ComicError, ComicLayoutId, ComicPage, ComicPanel, ComicProject,
    ComicRenderResult, PanelRect, COMIC_FORMAT_VERSION, COMIC_LAYOUT_CAP, COMIC_LAYOUT_SERVICE,
    COMIC_MAX_PAGE_EDGE, COMIC_MAX_PANEL_EDGE, COMIC_RENDER_CAP, COMIC_RENDER_SERVICE,
    DEFAULT_COMIC_PAGE_PATH,
};
pub use storyboard::{
    apply_frame, capture_frame, delete_frame, move_active_frame, ApplyTarget, StoryFrame,
    Storyboard, StoryboardError, DEFAULT_FRAME_DURATION_MS, STORYBOARD_APPLY_SERVICE,
    STORYBOARD_CAPTURE_SERVICE, STORYBOARD_DELETE_SERVICE, STORYBOARD_EDIT_CAP,
    STORYBOARD_FORMAT_VERSION, STORYBOARD_MOVE_SERVICE,
};
pub use pack_catalogue::{
    describe_local_pack, embedded_pack_catalogue, format_pack_list_summary, list_local_packs,
    load_pack_catalogue_yaml, marketplace_fetch_pack, resolve_asset_pack, CatalogueNetworkPolicy,
    CatalogueSource, PackCatalogue, PackCatalogueEntry, PackCatalogueError, PackDescribeResult,
    PackKind, ASSET_MARKETPLACE_FETCH_SERVICE, ASSET_PACK_DESCRIBE_SERVICE, ASSET_PACK_LIST_SERVICE,
    EMBEDDED_PACK_CATALOGUE_YAML, NETWORK_FETCH_CAP, PACK_CATALOGUE_FORMAT_VERSION,
    PACK_CATALOGUE_PATH,
};
pub use edit::{
    apply_batch, apply_one, merge_trs, require_batch_caps, require_edit_caps, AgentEditOp,
    EditActorKind, EditError, EditSnapshot, SCENE_APPLY_SERVICE, SCENE_CAMERA_SERVICE,
    SCENE_EDIT_CAP, SCENE_GET_SERVICE, SCENE_LIGHT_SERVICE, SCENE_SELECT_SERVICE,
    SCENE_TRS_SERVICE,
};
pub use light::{
    average_light_tint, collect_lights, color_from_srgb_u8, light_type_as_str, linear_to_srgb_u8,
    merge_light_params, parse_light_type, shade_diffuse, srgb_u8_to_linear, ResolvedLight,
};
pub use ik::{solve_two_bone, IkError, TwoBoneIkResult};
pub use locks::{
    LockEntryWire, LockError, LockKind, LockScope, LockTable, MutateKind, SemanticLock,
    SCENE_LOCKS_SERVICE, SCENE_LOCK_CAP, SCENE_LOCK_SERVICE, SCENE_UNLOCK_SERVICE,
};
pub use math::{Mat4, Quat, Vec3, EPSILON};
pub use mesh_asset::{
    default_mesh_search_roots, insert_mesh_asset, load_gltf_mesh, load_gltf_mesh_cached, resolve_mesh_uri, CpuTriangleMesh, MeshAssetError,
    MAX_MESH_TRIANGLES, MAX_MESH_VERTICES,
};
pub use neural_mesh::{
    apply_proposal, mesh_assist, neural_mesh_pack_status, propose_mesh_assist, validate_proposal,
    MeshAssistBackendId, MeshAssistProposal, MeshAssistRequest, MeshAssistResult, MeshPart,
    NeuralMeshError, NeuralMeshPackStatus, TrellisQualitySettings, MAX_ABS_SCALE, MAX_ABS_TRANSLATION, MAX_MESH_PARTS,
    MESH_ASSIST_SERVICE, MESH_NEURAL_CAP, MIN_ABS_SCALE,
};
pub use neural_mesh_isolate::{
    probe_pack_status, NeuralMeshRunMode, NeuralMeshRunnerKind, DEFAULT_NEURAL_MESH_TIMEOUT_SECS,
};
pub use ops::{SceneOp, UndoStack};
pub use pose::{
    apply_ik_chain, apply_pose, apply_pose_preset, joint_node_id, IkChain, JointId, PoseError,
    PoseOp, PosePreset, SCENE_POSE_CAP, SCENE_POSE_SERVICE,
};
pub use project::{load_project_yaml, save_project_yaml, ProjectFile, PROJECT_FORMAT_VERSION};
pub use fx::{EffectKind, RenderPreset, RenderQuality, SceneEffect};
pub use animation::{AnimationError, AnimationState, Keyframe, NamedPose, Rig};
pub use history::{HistoryError, ProjectHistory, SceneVariant, SceneVersion};
pub use render::{
    blender_pack_status, isolation_matrix, parse_backend, parse_pass, parse_style,
    render_object_id_map, ObjectIdMap,
    AkashaSceneExport, BlenderPackStatus, BlenderRenderBackend, BlenderRunMode,
    CpuWireframeBackend, IsolationRow, JobState, RenderBackend, RenderBackendId, RenderError,
    RenderJobStatus, RenderOutput, RenderPassKind, RenderRequest, RenderResult, RenderService,
    RenderSubmit, StubRenderBackend, AKASHA_SCENE_EXPORT_VERSION, DEFAULT_BLENDER_BEAUTY_PATH,
    DEFAULT_RENDER_BACKEND,
};
pub use render_stub::{stub_beauty_png, STUB_BEAUTY_SIZE};
pub use scene::{
    CameraParams, LightParams, LightType, MaterialOverride, NodeKind, SceneGraph, SceneNode, Transform,
    DEFAULT_SCENE_YAML_PATH, ILLUSTRATIONS_DOCUMENTS_PREFIX,
};
pub use style::{
    embedded_styles, load_style_pack_manifest_yaml, load_style_yaml, parse_optional_style,
    resolve_style, ResolvedStyle, StyleColor, StyleControls, StyleDef, StyleError, StyleFamily, StyleLine,
    StylePackManifest, StylePaper, StyleShading, DEFAULT_STYLE_ID, EMBEDDED_STYLE_INK_YAML,
    EMBEDDED_STYLE_PACK_MANIFEST_YAML, EMBEDDED_STYLE_PENCIL_YAML, EMBEDDED_STYLE_SKETCH_YAML,
    ILLUSTRATION_STYLES_PREFIX, STYLE_PACK_FORMAT_VERSION,
};
#[cfg(feature = "viewport")]
pub use viewport::{
    collect_mesh_instances, look_at_rh, perspective_rh, project_point_ndc, MeshInstance,
    ViewportCamera, ViewportError, ViewportRenderer, BEAUTY_ROLE, VIEWPORT_ROLE,
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
