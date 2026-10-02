//! Read-only git status/diff for bound host workspaces (#247 P1 / DA.5).
//!
//! Invokes the host `git` binary with `-C <workspace>` only. No commit, push,
//! fetch, or other mutating/network commands.

use crate::storage::StorageFs;
use crate::workspace::WorkspaceBindManager;
use aos_proto::workspace::{
    fs_read_cap, host_vfs_prefix, looks_like_host_vfs_path, vfs_relative_path,
    workspace_id_from_vfs_path, GitDiffRequest, GitDiffResponse, GitStatusEntry, GitStatusRequest,
    GitStatusResponse, GIT_DIFF_MAX_BYTES, GIT_STATUS_MAX_ENTRIES,
};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Porcelain `git status -b --porcelain=v1` under a bound workspace.
pub fn git_status(mgr: &WorkspaceBindManager, req: &GitStatusRequest) -> GitStatusResponse {
    let (workspace_id, host_root, vfs_prefix) = match resolve_git_root(mgr, &req.root) {
        Ok(v) => v,
        Err(msg) => {
            return GitStatusResponse {
                ok: false,
                branch: None,
                entries: vec![],
                truncated: false,
                message: Some(msg),
            };
        }
    };

    if let Err(msg) = require_read_cap(&req.caps, &workspace_id, &vfs_prefix) {
        return GitStatusResponse {
            ok: false,
            branch: None,
            entries: vec![],
            truncated: false,
            message: Some(msg),
        };
    }

    if let Err(msg) = ensure_git_repo(&host_root) {
        return GitStatusResponse {
            ok: false,
            branch: None,
            entries: vec![],
            truncated: false,
            message: Some(msg),
        };
    }

    let output = match Command::new("git")
        .arg("-C")
        .arg(&host_root)
        .args(["status", "--porcelain=v1", "-b", "--untracked-files=normal"])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            return GitStatusResponse {
                ok: false,
                branch: None,
                entries: vec![],
                truncated: false,
                message: Some(format!("git unavailable: {e}")),
            };
        }
    };
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return GitStatusResponse {
            ok: false,
            branch: None,
            entries: vec![],
            truncated: false,
            message: Some(format!(
                "git status failed: {}",
                err.trim().chars().take(240).collect::<String>()
            )),
        };
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut branch = None;
    let mut entries = Vec::new();
    for line in stdout.lines() {
        if line.starts_with("## ") {
            branch = Some(line[3..].to_string());
            continue;
        }
        if line.len() < 4 {
            continue;
        }
        // Porcelain v1: XY PATH or XY ORIG -> PATH
        let status = line[..2].to_string();
        let rest = line[3..].trim();
        let (path, orig_path) = if let Some((a, b)) = rest.split_once(" -> ") {
            (
                vfs_from_rel(&vfs_prefix, b.trim()),
                Some(vfs_from_rel(&vfs_prefix, a.trim())),
            )
        } else {
            (vfs_from_rel(&vfs_prefix, rest), None)
        };
        entries.push(GitStatusEntry {
            status,
            path,
            orig_path,
        });
        if entries.len() >= GIT_STATUS_MAX_ENTRIES {
            break;
        }
    }
    let truncated = entries.len() >= GIT_STATUS_MAX_ENTRIES;
    GitStatusResponse {
        ok: true,
        branch,
        entries,
        truncated,
        message: None,
    }
}

/// Unified `git diff` (worktree or `--cached`) under a bound workspace.
pub fn git_diff(mgr: &WorkspaceBindManager, req: &GitDiffRequest) -> GitDiffResponse {
    let (workspace_id, host_root, vfs_prefix) = match resolve_git_root(mgr, &req.root) {
        Ok(v) => v,
        Err(msg) => {
            return GitDiffResponse {
                ok: false,
                diff: String::new(),
                truncated: false,
                message: Some(msg),
            };
        }
    };

    if let Err(msg) = require_read_cap(&req.caps, &workspace_id, &vfs_prefix) {
        return GitDiffResponse {
            ok: false,
            diff: String::new(),
            truncated: false,
            message: Some(msg),
        };
    }

    if let Err(msg) = ensure_git_repo(&host_root) {
        return GitDiffResponse {
            ok: false,
            diff: String::new(),
            truncated: false,
            message: Some(msg),
        };
    }

    let path_arg = match req.path.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(p) => match resolve_diff_path(&host_root, &vfs_prefix, &workspace_id, p) {
            Ok(rel) => Some(rel),
            Err(msg) => {
                return GitDiffResponse {
                    ok: false,
                    diff: String::new(),
                    truncated: false,
                    message: Some(msg),
                };
            }
        },
        None => None,
    };

    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(&host_root).arg("diff").arg("--no-color");
    if req.staged {
        cmd.arg("--cached");
    }
    // Bound binary output roughly via external truncation after capture.
    if let Some(rel) = &path_arg {
        cmd.arg("--").arg(rel);
    }

    let output = match cmd.output() {
        Ok(o) => o,
        Err(e) => {
            return GitDiffResponse {
                ok: false,
                diff: String::new(),
                truncated: false,
                message: Some(format!("git unavailable: {e}")),
            };
        }
    };
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return GitDiffResponse {
            ok: false,
            diff: String::new(),
            truncated: false,
            message: Some(format!(
                "git diff failed: {}",
                err.trim().chars().take(240).collect::<String>()
            )),
        };
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let truncated = raw.len() > GIT_DIFF_MAX_BYTES;
    let diff = if truncated {
        raw.chars().take(GIT_DIFF_MAX_BYTES).collect()
    } else {
        raw.into_owned()
    };
    GitDiffResponse {
        ok: true,
        diff,
        truncated,
        message: None,
    }
}

fn require_read_cap(caps: &[String], workspace_id: &str, vfs_prefix: &str) -> Result<(), String> {
    let vfs_file = format!("{vfs_prefix}/");
    let allowed = StorageFs::check_cap(caps, "fs.read", &vfs_file).is_ok()
        || StorageFs::check_cap(caps, "fs.read", vfs_prefix).is_ok();
    if allowed {
        Ok(())
    } else {
        Err(format!(
            "permission refusée: {} requis (workspace {workspace_id})",
            fs_read_cap(workspace_id)
        ))
    }
}

fn ensure_git_repo(host_root: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(host_root)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map_err(|e| format!("git unavailable: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err("pas un dépôt git (initialisez le projet hôte avant git.status/diff)".into())
    }
}

fn resolve_git_root(
    mgr: &WorkspaceBindManager,
    root: &str,
) -> Result<(String, PathBuf, String), String> {
    let root = root.trim();
    if root.is_empty() {
        return Err("root vide".into());
    }
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

fn vfs_from_rel(vfs_prefix: &str, rel: &str) -> String {
    let rel = rel.trim().trim_matches('"').replace('\\', "/");
    if rel.is_empty() {
        vfs_prefix.to_string()
    } else {
        format!("{vfs_prefix}/{rel}")
    }
}

fn resolve_diff_path(
    host_root: &Path,
    vfs_prefix: &str,
    workspace_id: &str,
    path: &str,
) -> Result<String, String> {
    if looks_like_host_vfs_path(path) {
        let id = workspace_id_from_vfs_path(path)
            .ok_or_else(|| format!("chemin VFS invalide: {path}"))?;
        if id != workspace_id {
            return Err(format!(
                "path hors workspace {workspace_id} (reçu /host/{id}/…)"
            ));
        }
        let rel = vfs_relative_path(path).unwrap_or_default();
        if rel.is_empty() {
            return Err("path doit viser un fichier sous /host/<id>/…".into());
        }
        // Reject escapes via components already handled by WorkspaceBindManager;
        // here only ensure relative form for git.
        let abs = host_root.join(Path::new(&rel));
        let canon_rel = abs
            .strip_prefix(host_root)
            .map_err(|_| "path escape".to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let _ = vfs_prefix; // documented logical root
        return Ok(canon_rel);
    }
    // Repo-relative path: reject absolute / parent escapes.
    let p = Path::new(path);
    if p.is_absolute() {
        return Err("path absolu hôte interdit — utilisez /host/<id>/… ou un chemin relatif".into());
    }
    for comp in p.components() {
        match comp {
            std::path::Component::Normal(_) | std::path::Component::CurDir => {}
            _ => return Err("path relatif invalide".into()),
        }
    }
    Ok(path.replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::WorkspaceBindManager;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "aos-git-{}-{}-{}",
            label,
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        p
    }

    fn init_repo(proj: &Path) {
        fs::create_dir_all(proj).unwrap();
        assert!(Command::new("git")
            .args(["init"])
            .current_dir(proj)
            .status()
            .unwrap()
            .success());
        // Identity for commit in CI.
        let _ = Command::new("git")
            .args(["config", "user.email", "aos@test.local"])
            .current_dir(proj)
            .status();
        let _ = Command::new("git")
            .args(["config", "user.name", "aos-test"])
            .current_dir(proj)
            .status();
        fs::write(proj.join("README.md"), "hello\n").unwrap();
        assert!(Command::new("git")
            .args(["add", "README.md"])
            .current_dir(proj)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(proj)
            .status()
            .unwrap()
            .success());
    }

    #[test]
    fn status_and_diff_for_dirty_file() {
        let root = temp_dir("ok");
        let proj = root.join("proj");
        init_repo(&proj);
        fs::write(proj.join("README.md"), "hello\nworld\n").unwrap();

        let sessions = root.join("sessions");
        let mut mgr = WorkspaceBindManager::open(&sessions).unwrap();
        let info = mgr
            .bind(proj.to_str().unwrap(), Some("git-ws"), "agent:t")
            .unwrap();

        let status = git_status(
            &mgr,
            &GitStatusRequest {
                root: info.workspace_id.clone(),
                actor: "agent:t".into(),
                caps: info.caps.clone(),
                trace_id: "t".into(),
            },
        );
        assert!(status.ok, "{:?}", status.message);
        assert!(status.branch.is_some());
        assert!(status
            .entries
            .iter()
            .any(|e| e.path.contains("README.md")));

        let diff = git_diff(
            &mgr,
            &GitDiffRequest {
                root: info.vfs_root.clone(),
                path: None,
                staged: false,
                actor: "agent:t".into(),
                caps: info.caps.clone(),
                trace_id: "t".into(),
            },
        );
        assert!(diff.ok, "{:?}", diff.message);
        assert!(diff.diff.contains("world") || diff.diff.contains("README"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn status_requires_read_cap() {
        let root = temp_dir("cap");
        let proj = root.join("proj");
        init_repo(&proj);
        let sessions = root.join("sessions");
        let mut mgr = WorkspaceBindManager::open(&sessions).unwrap();
        let info = mgr
            .bind(proj.to_str().unwrap(), Some("cap-git"), "agent:t")
            .unwrap();
        let status = git_status(
            &mgr,
            &GitStatusRequest {
                root: info.workspace_id.clone(),
                actor: "agent:t".into(),
                caps: vec![],
                trace_id: "t".into(),
            },
        );
        assert!(!status.ok);
        assert!(status.message.as_deref().unwrap_or("").contains("permission"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_non_git_folder() {
        let root = temp_dir("nogit");
        let proj = root.join("proj");
        fs::create_dir_all(&proj).unwrap();
        fs::write(proj.join("a.txt"), "x\n").unwrap();
        let sessions = root.join("sessions");
        let mut mgr = WorkspaceBindManager::open(&sessions).unwrap();
        let info = mgr
            .bind(proj.to_str().unwrap(), Some("plain"), "agent:t")
            .unwrap();
        let status = git_status(
            &mgr,
            &GitStatusRequest {
                root: info.workspace_id.clone(),
                actor: "agent:t".into(),
                caps: info.caps.clone(),
                trace_id: "t".into(),
            },
        );
        assert!(!status.ok);
        assert!(status
            .message
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase()
            .contains("git"));
        let _ = fs::remove_dir_all(&root);
    }
}
