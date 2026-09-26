//! Bounded host workspace search (Preview 0.19 / #247 DA.2).
//!
//! Prefers the `rg` (ripgrep) binary when available; falls back to a
//! recursive walk + line scan so CI/dev hosts without `rg` still work.

use crate::storage::StorageFs;
use crate::workspace::WorkspaceBindManager;
use aos_proto::workspace::{
    fs_read_cap, host_vfs_prefix, looks_like_host_vfs_path, workspace_id_from_vfs_path, FsSearchHit,
    FsSearchRequest, FsSearchResponse, FS_SEARCH_MAX_LIMIT,
};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Resolve search root + run bounded search under a bound workspace.
pub fn search_workspace(mgr: &WorkspaceBindManager, req: &FsSearchRequest) -> FsSearchResponse {
    let limit = req.limit.clamp(1, FS_SEARCH_MAX_LIMIT);
    if req.query.trim().is_empty() {
        return FsSearchResponse {
            ok: false,
            hits: vec![],
            truncated: false,
            message: Some("query vide".into()),
        };
    }

    let (workspace_id, host_root, vfs_prefix) = match resolve_search_root(mgr, &req.root) {
        Ok(v) => v,
        Err(msg) => {
            return FsSearchResponse {
                ok: false,
                hits: vec![],
                truncated: false,
                message: Some(msg),
            };
        }
    };

    // Cap gate: caller must present `fs.read:/host/<id>/**` (or matching path).
    let vfs_file = format!("{vfs_prefix}/");
    let allowed = StorageFs::check_cap(&req.caps, "fs.read", &vfs_file).is_ok()
        || StorageFs::check_cap(&req.caps, "fs.read", &vfs_prefix).is_ok();
    if !allowed {
        return FsSearchResponse {
            ok: false,
            hits: vec![],
            truncated: false,
            message: Some(format!(
                "permission refusée: {} requis (workspace {workspace_id})",
                fs_read_cap(&workspace_id)
            )),
        };
    }

    let mut hits = match run_ripgrep(&host_root, &vfs_prefix, req, limit) {
        Ok(h) => h,
        Err(rg_err) => match walk_search(&host_root, &vfs_prefix, req, limit) {
            Ok(h) => h,
            Err(walk_err) => {
                return FsSearchResponse {
                    ok: false,
                    hits: vec![],
                    truncated: false,
                    message: Some(format!("search failed: rg={rg_err}; walk={walk_err}")),
                };
            }
        },
    };

    let truncated = hits.len() as u32 >= limit;
    if hits.len() > limit as usize {
        hits.truncate(limit as usize);
    }

    FsSearchResponse {
        ok: true,
        hits,
        truncated,
        message: None,
    }
}

fn resolve_search_root(
    mgr: &WorkspaceBindManager,
    root: &str,
) -> Result<(String, PathBuf, String), String> {
    let root = root.trim();
    if root.is_empty() {
        return Err("root vide".into());
    }

    // Bare workspace id.
    if !root.contains('/') && !root.contains('\\') {
        let host = mgr
            .resolve_host_path(root)
            .ok_or_else(|| format!("workspace inconnu: {root}"))?;
        return Ok((root.to_string(), PathBuf::from(host), host_vfs_prefix(root)));
    }

    if looks_like_host_vfs_path(root) {
        let id = workspace_id_from_vfs_path(root)
            .ok_or_else(|| format!("chemin VFS invalide: {root}"))?;
        let host = mgr
            .resolve_vfs_to_host(root)
            .ok_or_else(|| format!("workspace non lié: {id}"))?;
        return Ok((id.clone(), host, host_vfs_prefix(&id)));
    }

    Err(format!(
        "root doit être un workspace_id ou un chemin /host/<id>/… (reçu: {root})"
    ))
}

fn run_ripgrep(
    host_root: &Path,
    vfs_prefix: &str,
    req: &FsSearchRequest,
    limit: u32,
) -> Result<Vec<FsSearchHit>, String> {
    let mut cmd = Command::new("rg");
    cmd.arg("--line-number")
        .arg("--with-filename")
        .arg("--no-heading")
        .arg("--color=never")
        .arg("--max-count")
        .arg(limit.to_string())
        .arg("--max-filesize")
        .arg("1M");
    if !req.case_sensitive {
        cmd.arg("-i");
    }
    if let Some(glob) = &req.glob {
        if !glob.is_empty() {
            cmd.arg("--glob").arg(glob);
        }
    }
    // Fixed-string by default for agent safety (no ReDoS from LLM patterns).
    cmd.arg("-F").arg(&req.query).arg(host_root);
    cmd.stdout(Stdio::piped()).stderr(Stdio::null());

    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or_else(|| "rg stdout".to_string())?;
    let reader = BufReader::new(stdout);
    let mut hits = Vec::new();
    for line in reader.lines().map_while(Result::ok) {
        if let Some(hit) = parse_rg_line(&line, host_root, vfs_prefix) {
            hits.push(hit);
            if hits.len() as u32 >= limit {
                break;
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    Ok(hits)
}

/// `path:line:preview` (ripgrep default with -n --with-filename).
fn parse_rg_line(line: &str, host_root: &Path, vfs_prefix: &str) -> Option<FsSearchHit> {
    let bytes = line.as_bytes();
    let mut split_at = None;
    for i in 0..bytes.len().saturating_sub(2) {
        if bytes[i] == b':' && bytes[i + 1].is_ascii_digit() {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b':' && j > i + 1 {
                split_at = Some((i, j));
                break;
            }
        }
    }
    let (path_end, preview_start) = split_at?;
    let path_str = &line[..path_end];
    let line_no: u32 = line[path_end + 1..preview_start].parse().ok()?;
    let preview = line[preview_start + 1..].chars().take(240).collect::<String>();

    let abs = PathBuf::from(path_str);
    let rel = abs.strip_prefix(host_root).ok()?;
    let vfs = if rel.as_os_str().is_empty() {
        vfs_prefix.to_string()
    } else {
        format!("{vfs_prefix}/{}", rel.to_string_lossy().replace('\\', "/"))
    };
    Some(FsSearchHit {
        path: vfs,
        line: line_no,
        column: 1,
        preview,
    })
}

fn walk_search(
    host_root: &Path,
    vfs_prefix: &str,
    req: &FsSearchRequest,
    limit: u32,
) -> Result<Vec<FsSearchHit>, String> {
    let needle = if req.case_sensitive {
        req.query.clone()
    } else {
        req.query.to_ascii_lowercase()
    };
    let mut hits = Vec::new();
    walk_dir(host_root, host_root, vfs_prefix, &needle, req, limit, &mut hits)?;
    Ok(hits)
}

fn walk_dir(
    dir: &Path,
    host_root: &Path,
    vfs_prefix: &str,
    needle: &str,
    req: &FsSearchRequest,
    limit: u32,
    hits: &mut Vec<FsSearchHit>,
) -> Result<(), String> {
    if hits.len() as u32 >= limit {
        return Ok(());
    }
    let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;
    for ent in entries.flatten() {
        if hits.len() as u32 >= limit {
            break;
        }
        let path = ent.path();
        let name = ent.file_name().to_string_lossy().to_string();
        if name == ".git" || name == "node_modules" || name == "target" || name.starts_with('.') {
            continue;
        }
        let meta = match ent.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.is_dir() {
            walk_dir(&path, host_root, vfs_prefix, needle, req, limit, hits)?;
            continue;
        }
        if !meta.is_file() || meta.len() > 1_048_576 {
            continue;
        }
        if let Some(glob) = &req.glob {
            if !glob.is_empty() && !simple_glob_match(glob, &name) {
                continue;
            }
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        for (idx, line) in content.lines().enumerate() {
            let hay = if req.case_sensitive {
                line.to_string()
            } else {
                line.to_ascii_lowercase()
            };
            if hay.contains(needle) {
                let rel = path.strip_prefix(host_root).unwrap_or(&path);
                hits.push(FsSearchHit {
                    path: format!("{vfs_prefix}/{}", rel.to_string_lossy().replace('\\', "/")),
                    line: (idx + 1) as u32,
                    column: 1,
                    preview: line.chars().take(240).collect(),
                });
                if hits.len() as u32 >= limit {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

fn simple_glob_match(pattern: &str, name: &str) -> bool {
    if let Some(suf) = pattern.strip_prefix('*') {
        return name.ends_with(suf);
    }
    pattern == name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::WorkspaceBindManager;
    use aos_proto::workspace::FsSearchRequest;
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
}
