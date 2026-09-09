//! Frozen public contract for the Tasks optional official app (issue #149, lot 0).
//!
//! Tool ids, store path, and capability strings stay stable through extraction.
//! See `docs/tasks-contract.md` and `docs/adr/0008-tasks-optional-official-app.md`.

/// Installed module / package name.
pub const MODULE_NAME: &str = "tasks";

/// Canonical JSON store path (humans and agents share this file).
pub const STORE_PATH: &str = "/documents/tasks/tasks.json";

/// Stable tool identifiers exposed by the Tasks WASM module.
pub const TOOL_IDS: &[&str] = &[
    "tasks.create",
    "tasks.list",
    "tasks.update",
    "tasks.complete",
];

/// Read scope for the task store directory.
pub const FS_READ_CAP: &str = "fs.read:/documents/tasks/**";

/// Write scope for the task store directory.
pub const FS_WRITE_CAP: &str = "fs.write:/documents/tasks/**";

/// Capability required to invoke any `tasks.*` tool through `module.invoke`.
pub const INVOKE_CAP: &str = "tool.invoke:tasks";

/// Filesystem caps declared on the package manifest (invoke cap is separate).
pub const MANIFEST_FS_CAPS: &[&str] = &[FS_READ_CAP, FS_WRITE_CAP];

/// All caps an actor needs for full Tasks CRUD via the module runtime.
pub const FULL_ACTOR_CAPS: &[&str] = &[FS_READ_CAP, FS_WRITE_CAP, INVOKE_CAP];

/// Documented follow-up: `modules/tasks/src/lib.rs` `load()` must not treat all
/// read errors as an empty store. Missing file = first launch only.
pub const LOAD_BEHAVIOUR_FOLLOWUP: &str =
    "Distinguish missing /documents/tasks/tasks.json from permission and parse errors";

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn read_manifest_yaml() -> String {
        let path = workspace_root().join("share/modules/tasks.aospkg/manifest.yaml");
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
            "share/modules/tasks.aospkg/manifest.yaml tools must match tasks_contract::TOOL_IDS"
        );
    }

    #[test]
    fn frozen_fs_caps_match_shipped_manifest() {
        let raw = read_manifest_yaml();
        let (_, manifest_caps) = parse_manifest_tools_and_caps(&raw);
        let expected: Vec<&str> = MANIFEST_FS_CAPS.to_vec();
        assert_eq!(
            manifest_caps, expected,
            "manifest required_caps must match tasks_contract::MANIFEST_FS_CAPS"
        );
    }

    #[test]
    fn frozen_fs_caps_match_signed_catalogue() {
        let catalogue_path = workspace_root().join("share/modules/catalogue.yaml");
        let raw = std::fs::read_to_string(&catalogue_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", catalogue_path.display()));
        let section = raw
            .split("- name: tasks")
            .nth(1)
            .expect("catalogue must list tasks entry");
        for cap in MANIFEST_FS_CAPS {
            assert!(
                section.contains(cap),
                "catalogue tasks attested_caps must include {cap}"
            );
        }
    }

    #[test]
    fn store_path_is_under_documents_tasks() {
        assert_eq!(STORE_PATH, "/documents/tasks/tasks.json");
        assert!(STORE_PATH.starts_with("/documents/tasks/"));
    }

    #[test]
    fn invoke_cap_matches_module_name() {
        assert_eq!(INVOKE_CAP, format!("tool.invoke:{MODULE_NAME}"));
    }

    #[test]
    fn wasm_source_documents_same_store_path() {
        let lib_rs = workspace_root().join("modules/tasks/src/lib.rs");
        let raw = std::fs::read_to_string(&lib_rs)
            .unwrap_or_else(|e| panic!("read {}: {e}", lib_rs.display()));
        assert!(
            raw.contains(STORE_PATH),
            "modules/tasks/src/lib.rs must use tasks_contract::STORE_PATH value"
        );
    }
}
