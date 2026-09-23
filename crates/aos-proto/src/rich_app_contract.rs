//! Frozen rich-application contracts (issue #150, lot 0).
//!
//! Version gates, document limits, and vocabulary direction for installable
//! rich apps.  Runtime enforcement lands in later lots; this module names and
//! versions the contracts.  See `docs/rich-app-contract.md`,
//! `docs/create-contract.md`, and `docs/adr/0009-rich-module-app-contract.md`.

/// Host module API level — same gate as [`crate::OS_API_VERSION`].
pub const MIN_OS_API_FIELD: &str = "min_os_api";

/// Manifest field for the declarative UI vocabulary major version.
pub const UI_CONTRACT_FIELD: &str = "ui.contract";

/// Manifest field for the UI document path (preferred over legacy `ui.entry`).
pub const UI_DOCUMENT_FIELD: &str = "ui.document";

/// Legacy UI entry path still accepted during migration (`ui/index.html`).
pub const LEGACY_UI_ENTRY: &str = "ui/index.html";

/// Preferred UI document path for rich apps.
pub const RICH_UI_DOCUMENT: &str = "ui/index.json";

/// Declarative UI vocabulary shipped today (Tasks / Notes parity widgets).
pub const UI_CONTRACT_V1: u32 = 1;

/// Rich-app vocabulary (state, bindings, actions, jobs, `image_view`, …).
pub const UI_CONTRACT_V2: u32 = 2;

/// Highest UI contract major version this host generation is expected to render.
pub const HOST_UI_CONTRACT_MAX: u32 = UI_CONTRACT_V2;

/// Target UI contract for Create extraction (lot 2+).
pub const CREATE_TARGET_UI_CONTRACT: u32 = UI_CONTRACT_V2;

/// Generic long-running operation subscription facade (lot 1+).
pub const JOBS_SERVICE_VERSION: u32 = 1;

/// `media.image.generate` / `media.image.cancel` / `media.image.upscale` facade.
pub const MEDIA_IMAGE_SERVICE_VERSION: u32 = 1;

/// Manifest key for service version map (`services.jobs`, `services.media_image`).
pub const SERVICES_JOBS_FIELD: &str = "services.jobs";

pub const SERVICES_MEDIA_IMAGE_FIELD: &str = "services.media_image";

/// Closed widget kinds for [`UI_CONTRACT_V1`] (must match `decl_ui::WIDGET_KINDS`).
pub const UI_V1_WIDGET_KINDS: &[&str] = &[
    "column",
    "row",
    "heading",
    "text",
    "markdown",
    "stat_row",
    "table",
    "line_chart",
    "bar_chart",
    "pie",
    "scatter",
    "form",
    "button",
    "select",
    "radio",
    "checkbox",
    "textarea",
    "image",
    "audio",
    "empty_state",
    "count_label",
];

/// Additional widget kinds introduced with [`UI_CONTRACT_V2`] (lot 1+).
pub const UI_V2_ADDITIONAL_WIDGET_KINDS: &[&str] = &[
    "slider",
    "number",
    "progress",
    "job",
    "image_view",
    "split",
    "scroll",
    "tabs",
    "spacer",
    "layer_canvas",
    "layer_list",
    "undo_redo",
    "scene3d",
    "scene_tree",
    "section",
    "text_input",
    "file_picker",
    "multiselect",
    "prompt_starters",
    "asset_import",
];

/// Maximum widget nodes in a single UI document tree.
pub const MAX_UI_NODES: u32 = 2_000;

/// Maximum nesting depth of the widget tree.
pub const MAX_UI_DEPTH: u32 = 32;

/// Maximum declared local + document state slots per app instance.
pub const MAX_STATE_SLOTS: u32 = 128;

/// Maximum tool bindings per app instance.
pub const MAX_BINDINGS: u32 = 64;

/// Maximum concurrent job/event subscriptions per app instance.
pub const MAX_SUBSCRIPTIONS: u32 = 32;

/// Maximum predicate AST depth for visibility / enablement expressions.
pub const MAX_PREDICATE_DEPTH: u32 = 8;

/// Maximum predicate AST node count per expression.
pub const MAX_PREDICATE_NODES: u32 = 64;

/// Maximum string length for a single local state slot (e.g. prompt).
pub const MAX_STATE_STRING_LENGTH: u32 = 8_000;

/// Semantic interaction phases for native components (zoom/pan/resize/drag).
pub const INTERACTION_PHASES: &[&str] = &["start", "update", "commit", "cancel"];

/// Generic job lifecycle states exposed to packages.
pub const JOB_STATES: &[&str] = &["queued", "running", "succeeded", "failed", "cancelled"];

/// Platform bus methods that remain host-owned (not module tools).
pub const PLATFORM_MEDIA_IMAGE_METHODS: &[&str] = &[
    "media.image.generate",
    "media.image.cancel",
    "media.image.upscale",
];

/// Capability required to call `media.image.generate` from a rich app action.
pub const MEDIA_GENERATE_CAP: &str = "media.generate";

/// Read scope for generated artefacts under downloads (today's Image Studio output).
pub const FS_READ_DOWNLOADS_CAP: &str = "fs.read:/downloads/**";

/// Write scope for generated artefacts under downloads.
pub const FS_WRITE_DOWNLOADS_CAP: &str = "fs.write:/downloads/**";

/// Future Create document store (lot 2+); user data survives uninstall.
pub const CREATE_DOCUMENTS_PREFIX: &str = "/documents/create/";

/// Future read cap for Create package documents.
pub const CREATE_FS_READ_CAP: &str = "fs.read:/documents/create/**";

/// Future write cap for Create package documents.
pub const CREATE_FS_WRITE_CAP: &str = "fs.write:/documents/create/**";

/// Illustration Studio document tree (SceneGraph YAML projects).
pub const ILLUSTRATION_DOCUMENTS_PREFIX: &str = "/documents/illustrations/";

/// Read cap for Illustration Studio documents.
pub const ILLUSTRATION_FS_READ_CAP: &str = "fs.read:/documents/illustrations/**";

/// Write cap for Illustration Studio documents.
pub const ILLUSTRATION_FS_WRITE_CAP: &str = "fs.write:/documents/illustrations/**";

/// Cap: host stub beauty-pass renderer (no Blender / no external engine).
pub const RENDER_STUB_CAP: &str = "render.stub";

/// Cap: host CPU SceneGraph wireframe / beauty backend (no Blender / no wgpu).
pub const RENDER_CPU_CAP: &str = "render.cpu";

/// Cap: isolated Blender beauty backend (Renderer Pack process or deterministic mock).
pub const RENDER_BLENDER_CAP: &str = "render.blender";

/// Cap: read Illustration Studio asset packs under `/assets/illustration/**`.
pub const ASSET_ILLUSTRATION_READ_CAP: &str = "asset.read:/assets/illustration/**";

/// Illustration asset tree prefix (fail-closed with `ASSET_ILLUSTRATION_READ_CAP`).
pub const ILLUSTRATION_ASSETS_PREFIX: &str = "/assets/illustration/";

/// Cap: heuristic prompt → SceneGraph compose.
pub const SCENE_COMPOSE_CAP: &str = "scene.compose";

/// Cap: pose / IK-lite ops on SceneGraph humanoids.
pub const SCENE_POSE_CAP: &str = "scene.pose";

/// Cap: select / TRS / transactional scene.apply batches.
pub const SCENE_EDIT_CAP: &str = "scene.edit";

/// Cap: set / clear semantic locks on SceneGraph nodes.
pub const SCENE_LOCK_CAP: &str = "scene.lock";
/// Cap: neural / AI mesh assist (fail-closed; stub procedural is Preview default).
pub const MESH_NEURAL_CAP: &str = "mesh.neural";

/// DeclUI service: stub solid/viewport beauty placeholder PNG.
pub const RENDER_STUB_SERVICE: &str = "render.stub.beauty";

/// DeclUI service: submit a render job via backend-agnostic RenderService.
pub const RENDER_SUBMIT_SERVICE: &str = "render.submit";

/// DeclUI service: poll render job status.
pub const RENDER_STATUS_SERVICE: &str = "render.status";

/// DeclUI service: fetch completed render job result.
pub const RENDER_RESULT_SERVICE: &str = "render.result";

/// DeclUI service: instantiate an asset pack entry into the SceneGraph.
pub const ASSET_INSTANTIATE_SERVICE: &str = "asset.instantiate";

/// DeclUI service: compose SceneGraph from a short prompt (heuristics / templates).
pub const SCENE_COMPOSE_SERVICE: &str = "scene.compose";

/// DeclUI service: apply pose / IK-lite to a humanoid in the SceneGraph.
pub const SCENE_POSE_SERVICE: &str = "scene.pose";

/// DeclUI / host_call: read SceneGraph project snapshot.
pub const SCENE_GET_SERVICE: &str = "scene.get";

/// DeclUI / host_call: set selection.
pub const SCENE_SELECT_SERVICE: &str = "scene.select";

/// DeclUI / host_call: set node TRS.
pub const SCENE_TRS_SERVICE: &str = "scene.trs";

/// DeclUI / host_call: set active camera orbit / look-at / FOV.
pub const SCENE_CAMERA_SERVICE: &str = "scene.camera";

/// DeclUI / host_call: add / edit SceneGraph Light nodes.
pub const SCENE_LIGHT_SERVICE: &str = "scene.light";

/// DeclUI / host_call: transactional agent edit batch.
pub const SCENE_APPLY_SERVICE: &str = "scene.apply";

/// DeclUI / host_call: acquire semantic lock.
pub const SCENE_LOCK_SERVICE: &str = "scene.lock";

/// DeclUI / host_call: release semantic lock.
pub const SCENE_UNLOCK_SERVICE: &str = "scene.unlock";

/// DeclUI / host_call: list semantic locks.
pub const SCENE_LOCKS_SERVICE: &str = "scene.locks";
/// DeclUI service: neural / stub mesh assist → SceneGraph MeshBox / MeshAsset insert.
pub const MESH_ASSIST_SERVICE: &str = "mesh.assist";
/// DeclUI service: probe Neural Mesh Model Pack status (EN/FR summary).
pub const MESH_PACK_STATUS_SERVICE: &str = "mesh.pack.status";

/// DeclUI service: download the host-managed Illustration Studio runtimes.
pub const ILLUSTRATION_DEPENDENCIES_INSTALL_SERVICE: &str = "illustration.dependencies.install";

/// DeclUI service: probe Illustration Renderer Pack (Blender) status (EN/FR summary).
pub const RENDER_PACK_STATUS_SERVICE: &str = "render.pack.status";

/// Cap: install only the fixed, host-managed Illustration Studio dependencies.
pub const ILLUSTRATION_DEPENDENCIES_INSTALL_CAP: &str = "illustration.dependencies.install";

/// Host-only import or selected Poly Haven download into Illustration Studio.
pub const ILLUSTRATION_ASSET_IMPORT_SERVICE: &str = "illustration.asset.import";
pub const ILLUSTRATION_ASSET_IMPORT_CAP: &str = "illustration.asset.import";

/// Cap: create / mutate comic page panel layouts.
pub const COMIC_LAYOUT_CAP: &str = "comic.layout";

/// Cap: composite comic page beauty PNG.
pub const COMIC_RENDER_CAP: &str = "comic.render";
/// DeclUI service: apply a comic page layout template (panels + SceneGraph snapshots).
pub const COMIC_LAYOUT_SERVICE: &str = "comic.layout";

/// DeclUI service: render / composite a comic page to PNG.
pub const COMIC_RENDER_SERVICE: &str = "comic.render";

/// Cap: mutate Illustration Studio storyboard timeline (shots / frames).
pub const STORYBOARD_EDIT_CAP: &str = "storyboard.edit";

/// DeclUI service: capture current SceneGraph as a storyboard frame.
pub const STORYBOARD_CAPTURE_SERVICE: &str = "storyboard.capture";

/// DeclUI service: apply / step a storyboard frame onto the SceneGraph.
pub const STORYBOARD_APPLY_SERVICE: &str = "storyboard.apply";

/// DeclUI service: delete a storyboard frame.
pub const STORYBOARD_DELETE_SERVICE: &str = "storyboard.delete";

/// DeclUI service: reorder the active storyboard frame earlier / later.
pub const STORYBOARD_MOVE_SERVICE: &str = "storyboard.move";

/// DeclUI service: list local Illustration pack catalogue entries (offline).
pub const ASSET_PACK_LIST_SERVICE: &str = "asset.pack.list";

/// DeclUI service: describe one local Illustration pack.
pub const ASSET_PACK_DESCRIBE_SERVICE: &str = "asset.pack.describe";

/// DeclUI service: remote marketplace pack fetch hook (fail-closed in Preview).
pub const ASSET_MARKETPLACE_FETCH_SERVICE: &str = "asset.marketplace.fetch";

/// Cap: outbound network fetch (opt-in). Illustration Studio does **not** attest this.
pub const NETWORK_FETCH_CAP: &str = "network.fetch";

/// Reference machine assumptions for budget gates (documented, not enforced in lot 0).
pub const REFERENCE_MACHINE_NOTE: &str =
    "Preview reference: 8-core x86_64, 16 GiB RAM, 1080p display, release build";

/// p95 frame budget for local pan/zoom/resize (ms).
pub const BUDGET_LOCAL_INTERACTION_P95_MS: f32 = 16.7;

/// p95 budget for action validation + local state commit (ms).
pub const BUDGET_ACTION_COMMIT_P95_MS: f32 = 8.0;

/// p95 budget for binding invalidation first paint after local action (ms).
pub const BUDGET_BINDING_REFRESH_P95_MS: f32 = 100.0;

/// First visible job progress update after service event (ms).
pub const BUDGET_JOB_PROGRESS_FIRST_MS: f32 = 250.0;

/// Maximum rendered job progress updates per second per job.
pub const BUDGET_JOB_PROGRESS_MAX_HZ: u32 = 10;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decl_ui::WIDGET_KINDS;

    #[test]
    fn ui_v1_widget_kinds_match_decl_ui() {
        assert_eq!(UI_V1_WIDGET_KINDS, WIDGET_KINDS);
    }

    #[test]
    fn ui_v2_kinds_do_not_overlap_v1() {
        for kind in UI_V2_ADDITIONAL_WIDGET_KINDS {
            assert!(
                !UI_V1_WIDGET_KINDS.contains(kind),
                "v2 kind {kind} must not duplicate v1"
            );
        }
    }

    #[test]
    fn host_ui_max_includes_v2_rich_contract() {
        assert_eq!(HOST_UI_CONTRACT_MAX, UI_CONTRACT_V2);
        const _: () = assert!(CREATE_TARGET_UI_CONTRACT <= HOST_UI_CONTRACT_MAX);
    }

    #[test]
    fn document_limits_are_positive_and_ordered() {
        const _: () = assert!(MAX_UI_NODES >= MAX_BINDINGS);
        const _: () = assert!(MAX_STATE_SLOTS >= MAX_SUBSCRIPTIONS);
        const _: () = assert!(MAX_UI_DEPTH > 0);
    }

    #[test]
    fn interaction_phases_are_closed_set() {
        assert_eq!(INTERACTION_PHASES.len(), 4);
    }

    #[test]
    fn job_states_cover_terminal_and_active() {
        for state in ["queued", "running", "succeeded", "failed", "cancelled"] {
            assert!(JOB_STATES.contains(&state));
        }
    }

    #[test]
    fn create_documents_prefix_is_under_documents() {
        assert!(CREATE_DOCUMENTS_PREFIX.starts_with("/documents/"));
    }

    #[test]
    fn media_image_methods_include_generate_and_cancel() {
        assert!(PLATFORM_MEDIA_IMAGE_METHODS.contains(&"media.image.generate"));
        assert!(PLATFORM_MEDIA_IMAGE_METHODS.contains(&"media.image.cancel"));
    }
}
