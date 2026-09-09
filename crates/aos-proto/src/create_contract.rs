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

#[cfg(test)]
mod tests {
    use super::*;
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
}
