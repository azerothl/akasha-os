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
pub const HOST_UI_CONTRACT_MAX: u32 = UI_CONTRACT_V1;

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
pub const JOB_STATES: &[&str] = &[
    "queued",
    "running",
    "succeeded",
    "failed",
    "cancelled",
];

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
    fn host_ui_max_is_current_shipped_contract() {
        assert_eq!(HOST_UI_CONTRACT_MAX, UI_CONTRACT_V1);
        assert!(CREATE_TARGET_UI_CONTRACT > HOST_UI_CONTRACT_MAX);
    }

    #[test]
    fn document_limits_are_positive_and_ordered() {
        assert!(MAX_UI_NODES >= MAX_BINDINGS);
        assert!(MAX_STATE_SLOTS >= MAX_SUBSCRIPTIONS);
        assert!(MAX_UI_DEPTH > 0);
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
