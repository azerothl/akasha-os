//! Native, bounded workspace search using the shared ignore/credential policy.
//! Query text is always literal data; no search subprocess is executed.
use crate::{
    storage::StorageFs,
    workspace::WorkspaceBindManager,
    workspace_policy::{self, ScanLimits, WorkspacePolicy},
};
use aos_proto::workspace::{FsSearchHit, FsSearchRequest, FsSearchResponse, FS_SEARCH_MAX_LIMIT};
use std::path::{Path, PathBuf};

pub fn search_workspace(mgr: &WorkspaceBindManager, req: &FsSearchRequest) -> FsSearchResponse {
    match search(mgr, req) {
        Ok(response) => response,
        Err(message) => FsSearchResponse {
            ok: false,
            hits: vec![],
            truncated: false,
            message: Some(message),
        },
    }
}
fn search(mgr: &WorkspaceBindManager, req: &FsSearchRequest) -> Result<FsSearchResponse, String> {
    if req.query.trim().is_empty() || req.query.len() > 8192 || req.query.contains('\0') {
        return Err("query must contain 1..8192 non-NUL bytes".into());
    }
    let (id, root, relative) = resolve_search_root(mgr, &req.root)?;
    // This retains the P0 root read grant requirement. Subpaths cannot escape or
    // mislabel hits as files at the workspace root.
    let vfs = format!("/host/{id}/");
    StorageFs::check_cap(&req.caps, "fs.read", &vfs).map_err(|e| e.to_string())?;
    let mut policy = WorkspacePolicy::new(&root)?;
    let start = if relative.is_empty() {
        policy.root().to_path_buf()
    } else {
        let directory = root.join(&relative).is_dir();
        policy.checked(&relative, directory)?
    };
    let mut glob = ignore::gitignore::GitignoreBuilder::new(policy.root());
    if let Some(pattern) = req.glob.as_deref().filter(|p| !p.is_empty()) {
        if pattern.len() > 1024 || pattern == "!" {
            return Err("invalid search glob".into());
        }
        glob.add_line(None, pattern.strip_prefix('!').unwrap_or(pattern))
            .map_err(|e| e.to_string())?;
    }
    let glob = glob.build().map_err(|e| e.to_string())?;
    let needle = if req.case_sensitive {
        req.query.clone()
    } else {
        req.query.to_lowercase()
    };
    let limit = req.limit.clamp(1, FS_SEARCH_MAX_LIMIT) as usize;
    let mut hits = Vec::new();
    let report =
        workspace_policy::scan(&mut policy, &start, ScanLimits::default(), |path, text| {
            if req.glob.as_ref().is_some_and(|g| !g.is_empty()) {
                let matches = glob
                    .matched_path_or_any_parents(root.join(path), false)
                    .is_ignore();
                let negative = req
                    .glob
                    .as_ref()
                    .is_some_and(|pattern| pattern.starts_with('!'));
                if matches == negative {
                    return true;
                }
            }
            for (index, line) in text.lines().enumerate() {
                let matches = if req.case_sensitive {
                    line.contains(&needle)
                } else {
                    line.to_lowercase().contains(&needle)
                };
                if matches {
                    hits.push(FsSearchHit {
                        path: format!("/host/{id}/{path}"),
                        line: index as u32 + 1,
                        column: 1,
                        preview: line.chars().take(240).collect(),
                    });
                    if hits.len() > limit {
                        return false;
                    }
                }
            }
            true
        })?;
    let truncated = hits.len() > limit || !report.complete;
    hits.truncate(limit);
    Ok(FsSearchResponse{ok:true,hits,truncated,message:Some(format!("project policy: {} text files eligible, {} files withheld for credentials; scan_complete={}",report.eligible_files,report.finding_files,report.complete))})
}
fn resolve_search_root(
    mgr: &WorkspaceBindManager,
    raw: &str,
) -> Result<(String, PathBuf, String), String> {
    let raw = raw.trim();
    let logical = raw
        .strip_prefix("/host/")
        .unwrap_or(raw)
        .trim_end_matches('/');
    let (id, relative) = logical.split_once('/').unwrap_or((logical, ""));
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("root must be a bound workspace id or /host/<id>/path".into());
    }
    if !raw.starts_with("/host/") && !relative.is_empty() {
        return Err("subpaths require /host/<id>/path".into());
    }
    let host = mgr.resolve_host_path(id).ok_or("unknown workspace")?;
    let root = Path::new(&host).canonicalize().map_err(|e| e.to_string())?;
    Ok((id.into(), root, relative.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::WorkspaceBindManager;
    use aos_proto::workspace::FsSearchRequest;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "aos-search-{}-{}-{}",
            label,
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        p
    }

    #[test]
    fn walk_search_finds_line() {
        let root = temp_dir("walk");
        let proj = root.join("proj");
        fs::create_dir_all(proj.join("src")).unwrap();
        fs::write(
            proj.join("src/main.rs"),
            "fn main() {\n    hello_marker();\n}\n",
        )
        .unwrap();

        let sessions = root.join("sessions");
        let mut mgr = WorkspaceBindManager::open(&sessions).unwrap();
        let info = mgr
            .bind(proj.to_str().unwrap(), Some("demo-ws"), "agent:t")
            .unwrap();

        let req = FsSearchRequest {
            root: info.vfs_root.clone(),
            query: "hello_marker".into(),
            glob: Some("*.rs".into()),
            limit: 10,
            case_sensitive: true,
            actor: "agent:t".into(),
            caps: info.caps.clone(),
            trace_id: "t".into(),
        };
        let resp = search_workspace(&mgr, &req);
        assert!(resp.ok, "{:?}", resp.message);
        assert!(!resp.hits.is_empty());
        assert!(resp.hits[0].path.contains("/host/demo-ws/"));
        assert!(resp.hits[0].preview.contains("hello_marker"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn search_requires_read_cap() {
        let root = temp_dir("cap");
        let proj = root.join("proj");
        fs::create_dir_all(&proj).unwrap();
        fs::write(proj.join("a.txt"), "secret_token_xyz\n").unwrap();
        let sessions = root.join("sessions");
        let mut mgr = WorkspaceBindManager::open(&sessions).unwrap();
        let info = mgr
            .bind(proj.to_str().unwrap(), Some("cap-ws"), "agent:t")
            .unwrap();
        let req = FsSearchRequest {
            root: info.workspace_id.clone(),
            query: "secret_token".into(),
            glob: None,
            limit: 5,
            case_sensitive: false,
            actor: "agent:t".into(),
            caps: vec![],
            trace_id: "t".into(),
        };
        let resp = search_workspace(&mgr, &req);
        assert!(!resp.ok);
        assert!(resp.message.as_deref().unwrap_or("").contains("permission"));
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn subpaths_keep_their_vfs_prefix_and_queries_never_execute_options() {
        let home = temp_dir("subpath");
        let project = home.join("project");
        fs::create_dir_all(project.join("src")).unwrap();
        fs::write(
            project.join("src/main.rs"),
            "literal --pre=forbidden-command\nmarker\n",
        )
        .unwrap();
        fs::write(
            project.join("source.txt"),
            format!("marker\napi_key='ghp_{}'\n", "Z".repeat(36)),
        )
        .unwrap();
        fs::write(project.join(".ignore"), "ignored/\n").unwrap();
        fs::create_dir_all(project.join("ignored")).unwrap();
        fs::write(project.join("ignored/no.rs"), "marker\n").unwrap();
        let mut mgr = WorkspaceBindManager::open(home.join("sessions")).unwrap();
        let binding = mgr
            .bind(project.to_str().unwrap(), Some("demo"), "test")
            .unwrap();
        let mut req = FsSearchRequest {
            root: "/host/demo/src".into(),
            query: "--pre=forbidden-command".into(),
            glob: Some("*.rs".into()),
            limit: 10,
            case_sensitive: true,
            actor: "agent:test".into(),
            caps: binding.caps,
            trace_id: "test".into(),
        };
        let response = search_workspace(&mgr, &req);
        assert!(response.ok, "{:?}", response.message);
        assert_eq!(response.hits[0].path, "/host/demo/src/main.rs");
        req.root = "demo".into();
        req.query = "marker".into();
        req.glob = None;
        let response = search_workspace(&mgr, &req);
        assert!(response.ok);
        assert_eq!(response.hits.len(), 1);
        assert!(!response.hits[0].preview.contains(&"Z".repeat(36)));
        assert!(response.message.unwrap().contains("1 files withheld"));
        let _ = fs::remove_dir_all(home);
    }
    #[test]
    fn unicode_lowercase_matching_and_negative_globs_are_preserved() {
        let home = temp_dir("unicode");
        let project = home.join("project");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("source.rs"), "ÉCOLE\n").unwrap();
        fs::write(project.join("notes.md"), "ÉCOLE\n").unwrap();
        let mut mgr = WorkspaceBindManager::open(home.join("sessions")).unwrap();
        let binding = mgr
            .bind(project.to_str().unwrap(), Some("demo"), "test")
            .unwrap();
        let req = FsSearchRequest {
            root: "demo".into(),
            query: "école".into(),
            glob: Some("!*.md".into()),
            limit: 10,
            case_sensitive: false,
            actor: "test".into(),
            caps: binding.caps,
            trace_id: "test".into(),
        };
        let result = search_workspace(&mgr, &req);
        assert!(result.ok, "{:?}", result.message);
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].path, "/host/demo/source.rs");
        let _ = fs::remove_dir_all(home);
    }
}
