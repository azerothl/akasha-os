//! Contrats Preview 0.19 / #247 P0 — workspace host bind, search, patch.
//!
//! Foundations only: types + cap path helpers. Search/patch execution and
//! full host I/O land in later DA.* lots; `workspace.bind` persists bindings
//! in platformd.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub mod intents {
    pub const BIND: &str = "workspace.bind";
    pub const UNBIND: &str = "workspace.unbind";
    pub const LIST: &str = "workspace.list";
    pub const FS_SEARCH: &str = "fs.search";
    pub const CODE_SEARCH: &str = "code.search";
    pub const APPLY_PATCH: &str = "fs.apply_patch";
}

/// Logical VFS prefix for a bound host workspace: `/host/<id>`.
pub fn host_vfs_prefix(workspace_id: &str) -> String {
    format!("/host/{workspace_id}")
}

/// Cap string `fs.read:/host/<id>/**`.
pub fn fs_read_cap(workspace_id: &str) -> String {
    format!("fs.read:/host/{workspace_id}/**")
}

/// Cap string `fs.write:/host/<id>/**`.
pub fn fs_write_cap(workspace_id: &str) -> String {
    format!("fs.write:/host/{workspace_id}/**")
}

/// Both read+write caps for a bound workspace.
pub fn fs_caps(workspace_id: &str) -> Vec<String> {
    vec![fs_read_cap(workspace_id), fs_write_cap(workspace_id)]
}

/// Stable kebab workspace id from a host folder path (display slug + short hash).
pub fn workspace_id_for_host_path(host_path: &str) -> String {
    let display = crate::host_folder::folder_display_name(host_path);
    let mut slug: String = display
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "ws" } else { slug };
    let slug: String = slug.chars().take(24).collect();

    let key = crate::host_folder::normalize_folder_key(host_path);
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    let hash = hasher.finish();
    format!("{slug}-{:04x}", (hash & 0xffff) as u16)
}

/// True when `path` is under `/host/<id>/…` (logical VFS).
pub fn looks_like_host_vfs_path(path: &str) -> bool {
    let p = path.trim().replace('\\', "/");
    let rest = match p.strip_prefix("/host/") {
        Some(r) => r,
        None => return false,
    };
    let id = rest.split('/').next().unwrap_or("");
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Extract workspace id from `/host/<id>/…` or `/host/<id>`.
pub fn workspace_id_from_vfs_path(path: &str) -> Option<String> {
    let p = path.trim().replace('\\', "/");
    let rest = p.strip_prefix("/host/")?;
    let id = rest.split('/').next()?.to_string();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

/// Relative path inside a workspace from a logical `/host/<id>/rel` path.
pub fn vfs_relative_path(path: &str) -> Option<String> {
    let id = workspace_id_from_vfs_path(path)?;
    let prefix = format!("/host/{id}");
    let p = path.trim().replace('\\', "/");
    if p == prefix {
        return Some(String::new());
    }
    p.strip_prefix(&format!("{prefix}/"))
        .map(|s| s.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceBindRequest {
    /// Absolute host folder to bind (same rules as `fs.host.access`).
    pub host_path: String,
    /// Optional stable id; generated from path when absent.
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub trace_id: String,
    /// When true, also write persistent host-folder grant (extends #157 UX).
    #[serde(default)]
    pub grant_persistent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceBindResponse {
    pub ok: bool,
    #[serde(default)]
    pub workspace_id: Option<String>,
    /// Logical root `/host/<id>`.
    #[serde(default)]
    pub vfs_root: Option<String>,
    /// Absolute host folder that was bound.
    #[serde(default)]
    pub host_path: Option<String>,
    /// Caps to request/grant: `fs.read/write:/host/<id>/**`.
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceUnbindRequest {
    pub workspace_id: String,
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceUnbindResponse {
    pub ok: bool,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceListRequest {
    #[serde(default)]
    pub agent_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceBindingInfo {
    pub workspace_id: String,
    pub host_path: String,
    pub vfs_root: String,
    pub display_name: String,
    pub caps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceListResponse {
    pub bindings: Vec<WorkspaceBindingInfo>,
}

/// Default max matches for bounded ripgrep (DA.2).
pub const FS_SEARCH_DEFAULT_LIMIT: u32 = 50;
/// Hard ceiling so agents cannot request unbounded scans.
pub const FS_SEARCH_MAX_LIMIT: u32 = 200;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FsSearchRequest {
    /// Logical `/host/<id>` root or subpath, or bound workspace id alone.
    pub root: String,
    pub query: String,
    #[serde(default)]
    pub glob: Option<String>,
    #[serde(default = "default_search_limit")]
    pub limit: u32,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub trace_id: String,
}

fn default_search_limit() -> u32 {
    FS_SEARCH_DEFAULT_LIMIT
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FsSearchHit {
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FsSearchResponse {
    pub ok: bool,
    #[serde(default)]
    pub hits: Vec<FsSearchHit>,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub message: Option<String>,
}

/// Alias request for `code.search` (same payload as `fs.search` in P0).
pub type CodeSearchRequest = FsSearchRequest;
pub type CodeSearchResponse = FsSearchResponse;

/// Max files touched in one `fs.apply_patch` transaction (DA.3).
pub const APPLY_PATCH_MAX_FILES: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FsPatchHunk {
    /// Logical `/host/<id>/…` path.
    pub path: String,
    /// Unified diff body for this file (or full replacement when `replace_all`).
    pub diff: String,
    #[serde(default)]
    pub replace_all: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FsApplyPatchRequest {
    pub hunks: Vec<FsPatchHunk>,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub trace_id: String,
    /// When set, groups undo for multi-file rollback.
    #[serde(default)]
    pub undo_group_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FsApplyPatchResponse {
    pub ok: bool,
    #[serde(default)]
    pub applied: Vec<String>,
    #[serde(default)]
    pub undo_group_id: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_use_host_id_glob() {
        assert_eq!(fs_read_cap("my-repo-ab12"), "fs.read:/host/my-repo-ab12/**");
        assert_eq!(fs_write_cap("my-repo-ab12"), "fs.write:/host/my-repo-ab12/**");
        assert_eq!(host_vfs_prefix("my-repo-ab12"), "/host/my-repo-ab12");
    }

    #[test]
    fn workspace_id_is_stable_and_safe() {
        let a = workspace_id_for_host_path("/home/u/Code/Akasha");
        let b = workspace_id_for_host_path("/home/u/Code/Akasha");
        assert_eq!(a, b);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
        assert!(!a.contains('/'));
    }

    #[test]
    fn vfs_path_helpers() {
        assert!(looks_like_host_vfs_path("/host/ws-1/src/main.rs"));
        assert!(!looks_like_host_vfs_path("/documents/notes/a.md"));
        assert_eq!(
            workspace_id_from_vfs_path("/host/ws-1/src/main.rs").as_deref(),
            Some("ws-1")
        );
        assert_eq!(
            vfs_relative_path("/host/ws-1/src/main.rs").as_deref(),
            Some("src/main.rs")
        );
    }

    #[test]
    fn search_limit_defaults() {
        assert_eq!(FS_SEARCH_DEFAULT_LIMIT, 50);
        assert!(FS_SEARCH_MAX_LIMIT >= FS_SEARCH_DEFAULT_LIMIT);
        assert!(APPLY_PATCH_MAX_FILES >= 1);
    }
}
