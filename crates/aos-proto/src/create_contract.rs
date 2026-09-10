//! Frozen public contract for the Create optional official app (issue #150, lot 3).
//!
//! Tool ids, document paths, and capability strings stay stable through extraction.
//! See `docs/create-contract.md` and `docs/adr/0009-rich-module-app-contract.md`.

use crate::rich_app_contract::{
    CREATE_FS_READ_CAP, CREATE_FS_WRITE_CAP, CREATE_TARGET_UI_CONTRACT,
    FS_READ_DOWNLOADS_CAP, FS_WRITE_DOWNLOADS_CAP, MEDIA_GENERATE_CAP,
};

/// Installed module / package name.
pub const MODULE_NAME: &str = "create";

/// Canonical generation history index (user data survives uninstall).
pub const HISTORY_PATH: &str = "/documents/create/history.json";

/// Package document state path.
pub const STATE_PATH: &str = "/documents/create/state.json";

/// Stable tool identifiers exposed by the Create WASM module (lot 2 slice).
pub const TOOL_IDS: &[&str] = &[
    "create.history.list",
    "create.history.get",
    "create.history.record",
    "create.document.load",
    "create.document.save",
    "create.result.get",
    "create.models.list",
];

/// Filesystem caps declared on the package manifest (invoke + media caps are separate).
pub const MANIFEST_FS_CAPS: &[&str] = &[
    CREATE_FS_READ_CAP,
    CREATE_FS_WRITE_CAP,
    FS_READ_DOWNLOADS_CAP,
    FS_WRITE_DOWNLOADS_CAP,
];

/// Capability required to invoke any `create.*` tool through `module.invoke`.
pub const INVOKE_CAP: &str = "tool.invoke:create";

/// All caps an actor needs for full Create UI + history via the module runtime.
pub const FULL_ACTOR_CAPS: &[&str] = &[
    CREATE_FS_READ_CAP,
    CREATE_FS_WRITE_CAP,
    FS_READ_DOWNLOADS_CAP,
    FS_WRITE_DOWNLOADS_CAP,
    MEDIA_GENERATE_CAP,
    INVOKE_CAP,
];

/// Target rich UI contract major version for Create.
pub const UI_CONTRACT: u32 = CREATE_TARGET_UI_CONTRACT;

/// Platform image generation remains available when Create is uninstalled.
pub const PLATFORM_IMAGE_TOOL: &str = "media.image.generate";

/// Designer + supervisor surface locks (issue #150 lot 2 revalidated on main 0998f06).
/// Painted copy is frozen — Lot 3+ must not drift without explicit design review.
pub mod surface {
    /// Rail / panel title (FR). Never the module id `create`.
    pub const FR_APP_TITLE: &str = "Créer";
    pub const EN_APP_TITLE: &str = "Create";

    pub const FR_TAB_PARAMS: &str = "Paramètres";
    pub const FR_TAB_PREVIEW: &str = "Aperçu";
    pub const FR_TAB_HISTORY: &str = "Historique";

    pub const FR_MODE_IMAGE: &str = "Image";
    pub const FR_MODE_VIDEO: &str = "Vidéo";
    pub const FR_IMAGE_PACK_LABEL: &str = "Pack image par défaut";
    pub const FR_VIDEO_PACK_LABEL: &str = "Pack vidéo";
    pub const FR_PROMPT_LABEL: &str = "Invite";
    pub const FR_NEGATIVE_LABEL: &str = "Prompt négatif";
    pub const FR_VIDEO_DURATION_LABEL: &str = "Durée";
    pub const FR_VIDEO_FPS_LABEL: &str = "Images/s";
    pub const FR_WIDTH_LABEL: &str = "Largeur";
    pub const FR_HEIGHT_LABEL: &str = "Hauteur";
    pub const FR_STEPS_LABEL: &str = "Étapes";

    pub const FR_GENERATE_LABEL: &str = "Générer";
    pub const FR_SAVE_LABEL: &str = "Enregistrer l'image";
    pub const FR_RESTORE_LABEL: &str = "Restaurer";
    pub const FR_JOB_LABEL: &str = "Génération";

    pub const FR_PREVIEW_EMPTY: &str =
        "Pas encore d'image — saisissez une invite et générez.";
    pub const FR_HISTORY_EMPTY: &str = "Aucune génération pour l'instant";

    /// Label keys declared in `modules/create/ui/index.json`.
    pub const LABEL_KEYS: &[&str] = &[
        "app_title",
        "tab_params",
        "tab_preview",
        "tab_history",
        "mode_image",
        "mode_video",
        "image_pack_label",
        "video_pack_label",
        "prompt_label",
        "negative_label",
        "width_label",
        "height_label",
        "steps_label",
        "video_duration_label",
        "video_duration_2s",
        "video_duration_3s",
        "video_duration_4s",
        "video_fps_label",
        "generate_label",
        "save_label",
        "job_label",
        "preview_empty",
        "preview_empty_video",
        "history_restore",
        "history_prompt",
        "history_when",
        "history_empty",
    ];
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::surface::{
        EN_APP_TITLE, FR_APP_TITLE, FR_GENERATE_LABEL, FR_HEIGHT_LABEL, FR_HISTORY_EMPTY,
        FR_IMAGE_PACK_LABEL, FR_JOB_LABEL, FR_MODE_IMAGE, FR_MODE_VIDEO, FR_NEGATIVE_LABEL,
        FR_PREVIEW_EMPTY, FR_PROMPT_LABEL, FR_RESTORE_LABEL, FR_SAVE_LABEL, FR_STEPS_LABEL,
        FR_TAB_HISTORY, FR_TAB_PARAMS, FR_TAB_PREVIEW, FR_VIDEO_DURATION_LABEL,
        FR_VIDEO_FPS_LABEL, FR_VIDEO_PACK_LABEL, FR_WIDTH_LABEL, LABEL_KEYS,
    };
    use crate::decl_ui::DeclUiDocument;
    use std::path::PathBuf;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn read_manifest_yaml() -> String {
        let path = workspace_root().join("share/modules/create.aospkg/manifest.yaml");
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    fn parse_manifest_tools_and_caps(raw: &str) -> (Vec<String>, Vec<String>) {
        let mut tools = Vec::new();
        let mut caps = Vec::new();
        let mut in_tools = false;
        let mut in_required_caps = false;
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("tools:") {
                in_tools = true;
                in_required_caps = false;
                continue;
            }
            if trimmed.starts_with("required_caps:") {
                in_required_caps = true;
                in_tools = false;
                continue;
            }
            if in_tools && trimmed.starts_with("- name:") {
                let name = trimmed.strip_prefix("- name:").unwrap().trim();
                tools.push(name.to_string());
            }
            if in_required_caps && trimmed.starts_with("- ") {
                let cap = trimmed.strip_prefix("- ").unwrap().trim();
                caps.push(cap.to_string());
            }
            if !line.starts_with(' ') && !line.starts_with('\t') && !trimmed.is_empty() {
                if !trimmed.starts_with("tools:")
                    && !trimmed.starts_with("permissions:")
                    && !trimmed.starts_with("required_caps:")
                    && !trimmed.starts_with("- ")
                {
                    in_tools = false;
                    in_required_caps = false;
                }
            }
        }
        (tools, caps)
    }

    #[test]
    fn frozen_tool_ids_match_shipped_manifest() {
        let raw = read_manifest_yaml();
        let (manifest_tools, _) = parse_manifest_tools_and_caps(&raw);
        let expected: Vec<&str> = TOOL_IDS.to_vec();
        assert_eq!(
            manifest_tools, expected,
            "share/modules/create.aospkg/manifest.yaml tools must match create_contract::TOOL_IDS"
        );
    }

    #[test]
    fn frozen_fs_caps_match_shipped_manifest() {
        let raw = read_manifest_yaml();
        let (_, manifest_caps) = parse_manifest_tools_and_caps(&raw);
        for cap in MANIFEST_FS_CAPS {
            assert!(
                manifest_caps.iter().any(|c| c == cap),
                "manifest required_caps must include {cap}"
            );
        }
        assert!(
            manifest_caps.iter().any(|c| c == MEDIA_GENERATE_CAP),
            "manifest must declare media.generate"
        );
        assert!(
            manifest_caps.iter().any(|c| c == INVOKE_CAP),
            "manifest must declare tool.invoke:create"
        );
    }

    #[test]
    fn frozen_fs_caps_match_signed_catalogue_when_present() {
        let catalogue_path = workspace_root().join("share/modules/catalogue.yaml");
        let raw = std::fs::read_to_string(&catalogue_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", catalogue_path.display()));
        if !raw.contains("- name: create") {
            return;
        }
        let section = raw
            .split("- name: create")
            .nth(1)
            .expect("catalogue must list create entry");
        for cap in MANIFEST_FS_CAPS {
            assert!(
                section.contains(cap),
                "catalogue create attested_caps must include {cap}"
            );
        }
    }

    #[test]
    fn history_path_is_under_documents_create() {
        assert_eq!(HISTORY_PATH, "/documents/create/history.json");
        assert!(HISTORY_PATH.starts_with("/documents/create/"));
    }

    #[test]
    fn invoke_cap_matches_module_name() {
        assert_eq!(INVOKE_CAP, format!("tool.invoke:{MODULE_NAME}"));
    }

    #[test]
    fn wasm_source_documents_same_history_path() {
        let lib_rs = workspace_root().join("modules/create/src/lib.rs");
        let raw = std::fs::read_to_string(&lib_rs)
            .unwrap_or_else(|e| panic!("read {}: {e}", lib_rs.display()));
        assert!(
            raw.contains(HISTORY_PATH),
            "modules/create/src/lib.rs must use create_contract::HISTORY_PATH value"
        );
    }

    #[test]
    fn ui_contract_matches_manifest() {
        let raw = read_manifest_yaml();
        assert!(
            raw.contains(&format!("contract: {UI_CONTRACT}")),
            "create manifest ui.contract must match create_contract::UI_CONTRACT"
        );
    }

    fn read_ui_document() -> DeclUiDocument {
        let path = workspace_root().join("share/modules/create.aospkg/ui/index.json");
        let raw = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        DeclUiDocument::parse_json_with_contract(&raw, UI_CONTRACT)
            .unwrap_or_else(|e| panic!("parse create ui: {e}"))
    }

    #[test]
    fn frozen_fr_surface_labels_match_shipped_ui_document() {
        let doc = read_ui_document();
        let labels = doc.labels.as_ref().expect("create ui must declare labels");
        let fr = &labels.fr;
        assert!(!fr.is_empty(), "create ui must declare fr labels");
        assert_eq!(fr.get("app_title").map(String::as_str), Some(FR_APP_TITLE));
        assert_eq!(fr.get("tab_params").map(String::as_str), Some(FR_TAB_PARAMS));
        assert_eq!(fr.get("tab_preview").map(String::as_str), Some(FR_TAB_PREVIEW));
        assert_eq!(fr.get("tab_history").map(String::as_str), Some(FR_TAB_HISTORY));
        assert_eq!(fr.get("mode_image").map(String::as_str), Some(FR_MODE_IMAGE));
        assert_eq!(fr.get("mode_video").map(String::as_str), Some(FR_MODE_VIDEO));
        assert!(
            fr.get("mode_label").is_none(),
            "image/video segment must not expose a separate mode heading"
        );
        assert_eq!(
            fr.get("image_pack_label").map(String::as_str),
            Some(FR_IMAGE_PACK_LABEL)
        );
        assert_eq!(
            fr.get("video_pack_label").map(String::as_str),
            Some(FR_VIDEO_PACK_LABEL)
        );
        assert_eq!(fr.get("prompt_label").map(String::as_str), Some(FR_PROMPT_LABEL));
        assert_eq!(fr.get("negative_label").map(String::as_str), Some(FR_NEGATIVE_LABEL));
        assert_eq!(
            fr.get("video_duration_label").map(String::as_str),
            Some(FR_VIDEO_DURATION_LABEL)
        );
        assert_eq!(
            fr.get("video_fps_label").map(String::as_str),
            Some(FR_VIDEO_FPS_LABEL)
        );
        assert_eq!(fr.get("width_label").map(String::as_str), Some(FR_WIDTH_LABEL));
        assert_eq!(fr.get("height_label").map(String::as_str), Some(FR_HEIGHT_LABEL));
        assert_eq!(fr.get("steps_label").map(String::as_str), Some(FR_STEPS_LABEL));
        assert_eq!(fr.get("generate_label").map(String::as_str), Some(FR_GENERATE_LABEL));
        assert_eq!(fr.get("save_label").map(String::as_str), Some(FR_SAVE_LABEL));
        assert_eq!(fr.get("job_label").map(String::as_str), Some(FR_JOB_LABEL));
        assert_eq!(
            fr.get("preview_empty").map(String::as_str),
            Some(FR_PREVIEW_EMPTY)
        );
        assert_eq!(fr.get("history_restore").map(String::as_str), Some(FR_RESTORE_LABEL));
        assert_eq!(
            fr.get("history_empty").map(String::as_str),
            Some(FR_HISTORY_EMPTY)
        );
        for key in LABEL_KEYS {
            assert!(fr.contains_key(*key), "missing fr label key {key}");
        }
    }

    #[test]
    fn catalogue_title_uses_human_labels_not_module_id() {
        let doc = read_ui_document();
        assert_eq!(doc.catalogue_title(), EN_APP_TITLE);
        assert_eq!(doc.chrome_title("fr"), FR_APP_TITLE);
        let labels = doc.labels.as_ref().unwrap();
        assert_eq!(
            labels.resolve("fr", "app_title").as_deref(),
            Some(FR_APP_TITLE)
        );
        // Wire module id must never appear in localized chrome (FR lock).
        assert_ne!(doc.chrome_title("fr"), MODULE_NAME);
        assert_ne!(labels.resolve("fr", "app_title").unwrap(), MODULE_NAME);
    }

    #[test]
    fn lot4_rail_labels_match_frozen_surface_lock() {
        use super::surface::{EN_APP_TITLE, FR_APP_TITLE};
        assert_ne!(EN_APP_TITLE, MODULE_NAME);
        assert_ne!(FR_APP_TITLE, MODULE_NAME);
        assert_eq!(EN_APP_TITLE, "Create");
        assert_eq!(FR_APP_TITLE, "Créer");
    }

    #[test]
    fn lot4_no_create_studio_widget_in_runtime_crates() {
        fn scan_dir(dir: &std::path::Path) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    scan_dir(&path);
                } else if path.extension().is_some_and(|e| e == "rs")
                    && !path.ends_with("create_contract.rs")
                {
                    let raw = std::fs::read_to_string(&path).unwrap();
                    let embeds_widget = raw.contains("mod create_studio")
                        || raw.contains("create_studio::")
                        || raw.contains("fn create_studio")
                        || raw.contains("struct CreateStudio");
                    assert!(
                        !embeds_widget,
                        "runtime crate must not embed create_studio widget: {}",
                        path.display()
                    );
                }
            }
        }
        scan_dir(&workspace_root().join("crates"));
    }

    #[test]
    fn lot4_primary_navigation_targets_module_not_native_image_tab() {
        let main_rs = workspace_root().join("crates/aos-ui-egui/src/main.rs");
        let raw = std::fs::read_to_string(&main_rs)
            .unwrap_or_else(|e| panic!("read {}: {e}", main_rs.display()));
        assert!(
            !raw.contains("Tab::Image"),
            "lot 4 cutover must not reference native Tab::Image in main.rs"
        );
        assert!(
            raw.contains("create_module_installed"),
            "main.rs must gate Create rail on installed package"
        );
        assert!(
            raw.contains("nav::create_module_tab"),
            "main.rs must route primary Create rail through module tab"
        );
    }

    #[test]
    fn lot5_native_image_studio_panel_removed() {
        let path = workspace_root().join("crates/aos-ui-egui/src/image_studio.rs");
        assert!(
            !path.exists(),
            "lot 5 must delete dead native image_studio.rs panel"
        );
    }

    #[test]
    fn lot5_composition_widgets_are_generic_not_create_named() {
        let kinds = crate::rich_app_contract::UI_V2_ADDITIONAL_WIDGET_KINDS;
        assert!(kinds.contains(&"layer_canvas"));
        assert!(kinds.contains(&"layer_list"));
        assert!(kinds.contains(&"undo_redo"));
        for kind in kinds {
            assert!(
                !kind.contains("create"),
                "widget kind must not embed app name: {kind}"
            );
        }
    }

    #[test]
    fn lot5_primary_rail_keeps_human_creer_label_not_module_id() {
        let main_rs = workspace_root().join("crates/aos-ui-egui/src/main.rs");
        let raw = std::fs::read_to_string(&main_rs)
            .unwrap_or_else(|e| panic!("read {}: {e}", main_rs.display()));
        assert!(
            raw.contains("t.tab_create"),
            "primary rail must paint human Créer/Create via tab_create, not module id"
        );
        assert!(
            !raw.contains("Tab::Image"),
            "lot 5 must not restore native Image Studio tab"
        );
    }

    #[test]
    fn lot5_host_renderer_has_no_app_name_branches() {
        for rel in [
            "crates/aos-ui-egui/src/decl_ui.rs",
            "crates/aos-ui-egui/src/rich_composition_ui.rs",
        ] {
            let path = workspace_root().join(rel);
            let raw = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            for forbidden in ["\"gallery-demo\"", "\"create\"", "create_studio"] {
                assert!(
                    !raw.contains(forbidden),
                    "host renderer must not branch on {forbidden}: {}",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn lot5_create_package_chrome_frozen() {
        let doc = read_ui_document();
        let fr = &doc.labels.as_ref().expect("create labels").fr;
        assert_eq!(fr.get("app_title").map(String::as_str), Some(FR_APP_TITLE));
        assert_eq!(fr.get("tab_params").map(String::as_str), Some(FR_TAB_PARAMS));
        assert_eq!(fr.get("tab_preview").map(String::as_str), Some(FR_TAB_PREVIEW));
        assert_eq!(fr.get("tab_history").map(String::as_str), Some(FR_TAB_HISTORY));
        assert_eq!(fr.get("generate_label").map(String::as_str), Some(FR_GENERATE_LABEL));
        assert_eq!(fr.get("save_label").map(String::as_str), Some(FR_SAVE_LABEL));
        assert_eq!(
            fr.get("history_restore").map(String::as_str),
            Some(FR_RESTORE_LABEL)
        );
    }

    #[test]
    fn lot6_create_ui_exposes_image_video_mode_selection() {
        let doc = read_ui_document();
        let fr = &doc.labels.as_ref().expect("create labels").fr;
        assert_eq!(fr.get("mode_image").map(String::as_str), Some(FR_MODE_IMAGE));
        assert_eq!(fr.get("mode_video").map(String::as_str), Some(FR_MODE_VIDEO));
        let state = doc.state.as_ref().expect("create state");
        assert!(state.local.contains_key("media_mode"));
        assert!(doc.actions.iter().any(|a| a.id == "generate_image"));
        assert!(doc.actions.iter().any(|a| a.id == "generate_video"));
        let ui_raw = std::fs::read_to_string(
            workspace_root().join("share/modules/create.aospkg/ui/index.json"),
        )
        .expect("create ui json");
        assert!(
            ui_raw.contains("\"state_key\": \"media_mode\"")
                && ui_raw.contains("\"mode_video\"")
                && !ui_raw.contains("\"mode_label\""),
            "params tab must render image/video segment without wire heading"
        );
        assert!(
            ui_raw.contains("\"image_pack_label\"")
                && ui_raw.contains("\"video_pack_label\""),
            "model picker must use native pack labels per mode"
        );
        assert!(
            doc.bindings.iter().any(|b| b.tool == "create.models.list"),
            "create must bind model catalog for picker"
        );
    }

    #[test]
    fn fr_surface_labels_avoid_wire_tokens_in_chrome() {
        let doc = read_ui_document();
        let fr = &doc.labels.as_ref().unwrap().fr;
        for (key, value) in fr {
            assert!(
                !value.contains("create."),
                "label {key} must not expose wire id: {value}"
            );
            assert!(
                !value.contains("media.image"),
                "label {key} must not expose service id: {value}"
            );
            assert!(
                !value.eq_ignore_ascii_case(MODULE_NAME),
                "label {key} must not show module id in chrome: {value}"
            );
        }
    }
}
