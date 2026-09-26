//! Multi-file host workspace patch + undo (Preview 0.19 / #247 DA.3).

use crate::storage::StorageFs;
use crate::workspace::WorkspaceBindManager;
use aos_proto::workspace::{
    fs_write_cap, looks_like_host_vfs_path, workspace_id_from_vfs_path, FsApplyPatchRequest,
    FsApplyPatchResponse, FsUndoPatchRequest, FsUndoPatchResponse, APPLY_PATCH_MAX_FILES,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UndoFileSnapshot {
    vfs_path: String,
    host_path: String,
    /// Previous content; `None` means the file did not exist (delete on undo).
    previous: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UndoGroup {
    id: String,
    actor: String,
    files: Vec<UndoFileSnapshot>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct UndoStore {
    groups: Vec<UndoGroup>,
}

pub struct WorkspacePatchManager {
    store_path: PathBuf,
    store: UndoStore,
}

impl WorkspacePatchManager {
    pub fn open(sessions_root: impl Into<PathBuf>) -> Result<Self, String> {
        let sessions_root = sessions_root.into();
        fs::create_dir_all(&sessions_root).map_err(|e| e.to_string())?;
        let store_path = sessions_root.join("workspace-patch-undo.json");
        let store = match fs::read_to_string(&store_path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => UndoStore::default(),
            Err(e) => return Err(e.to_string()),
        };
        Ok(Self { store_path, store })
    }

    fn save(&self) -> Result<(), String> {
        let raw = serde_json::to_string_pretty(&self.store).map_err(|e| e.to_string())?;
        fs::write(&self.store_path, raw).map_err(|e| e.to_string())
    }

    pub fn apply(
        &mut self,
        binds: &WorkspaceBindManager,
        req: &FsApplyPatchRequest,
    ) -> FsApplyPatchResponse {
        if req.hunks.is_empty() {
            return FsApplyPatchResponse {
                ok: false,
                applied: vec![],
                undo_group_id: None,
                message: Some("aucun hunk".into()),
            };
        }
        if req.hunks.len() > APPLY_PATCH_MAX_FILES {
            return FsApplyPatchResponse {
                ok: false,
                applied: vec![],
                undo_group_id: None,
                message: Some(format!(
                    "trop de fichiers (max {APPLY_PATCH_MAX_FILES})"
                )),
            };
        }

        let mut planned: Vec<(String, PathBuf, String, Option<String>)> = Vec::new();
        // (vfs, host, new_content, previous)

        for hunk in &req.hunks {
            if !looks_like_host_vfs_path(&hunk.path) {
                return FsApplyPatchResponse {
                    ok: false,
                    applied: vec![],
                    undo_group_id: None,
                    message: Some(format!("chemin VFS invalide: {}", hunk.path)),
                };
            }
            let id = match workspace_id_from_vfs_path(&hunk.path) {
                Some(id) => id,
                None => {
                    return FsApplyPatchResponse {
                        ok: false,
                        applied: vec![],
                        undo_group_id: None,
                        message: Some(format!("workspace id manquant: {}", hunk.path)),
                    };
                }
            };
            if StorageFs::check_cap(&req.caps, "fs.write", &hunk.path).is_err() {
                return FsApplyPatchResponse {
                    ok: false,
                    applied: vec![],
                    undo_group_id: None,
                    message: Some(format!(
                        "permission refusée: {} requis",
                        fs_write_cap(&id)
                    )),
                };
            }
            let host = match binds.resolve_vfs_to_host(&hunk.path) {
                Some(p) => p,
                None => {
                    return FsApplyPatchResponse {
                        ok: false,
                        applied: vec![],
                        undo_group_id: None,
                        message: Some(format!("workspace non lié pour {}", hunk.path)),
                    };
                }
            };

            let previous = if host.exists() {
                match fs::read_to_string(&host) {
                    Ok(s) => Some(s),
                    Err(e) => {
                        return FsApplyPatchResponse {
                            ok: false,
                            applied: vec![],
                            undo_group_id: None,
                            message: Some(format!("lecture {}: {e}", host.display())),
                        };
                    }
                }
            } else {
                None
            };

            let new_content = if hunk.replace_all {
                hunk.diff.clone()
            } else {
                match apply_unified_diff(previous.as_deref().unwrap_or(""), &hunk.diff) {
                    Ok(s) => s,
                    Err(e) => {
                        return FsApplyPatchResponse {
                            ok: false,
                            applied: vec![],
                            undo_group_id: None,
                            message: Some(format!("diff {}: {e}", hunk.path)),
                        };
                    }
                }
            };
            planned.push((hunk.path.clone(), host, new_content, previous));
        }

        let group_id = req
            .undo_group_id
            .clone()
            .unwrap_or_else(new_undo_group_id);
        let mut snapshots = Vec::new();
        let mut applied = Vec::new();

        for (vfs, host, content, previous) in planned {
            if let Some(parent) = host.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    return FsApplyPatchResponse {
                        ok: false,
                        applied,
                        undo_group_id: None,
                        message: Some(format!("mkdir {}: {e}", parent.display())),
                    };
                }
            }
            if let Err(e) = fs::write(&host, &content) {
                return FsApplyPatchResponse {
                    ok: false,
                    applied,
                    undo_group_id: None,
                    message: Some(format!("écriture {}: {e}", host.display())),
                };
            }
            snapshots.push(UndoFileSnapshot {
                vfs_path: vfs.clone(),
                host_path: host.to_string_lossy().into(),
                previous,
            });
            applied.push(vfs);
        }

        self.store.groups.retain(|g| g.id != group_id);
        self.store.groups.push(UndoGroup {
            id: group_id.clone(),
            actor: req.actor.clone(),
            files: snapshots,
        });
        // Keep last 32 groups.
        if self.store.groups.len() > 32 {
            let drop_n = self.store.groups.len() - 32;
            self.store.groups.drain(0..drop_n);
        }
        if let Err(e) = self.save() {
            return FsApplyPatchResponse {
                ok: false,
                applied,
                undo_group_id: Some(group_id),
                message: Some(format!("patch écrit mais undo store: {e}")),
            };
        }

        FsApplyPatchResponse {
            ok: true,
            applied,
            undo_group_id: Some(group_id),
            message: None,
        }
    }

    pub fn undo(&mut self, req: &FsUndoPatchRequest) -> FsUndoPatchResponse {
        let Some(pos) = self.store.groups.iter().position(|g| g.id == req.undo_group_id) else {
            return FsUndoPatchResponse {
                ok: false,
                restored: vec![],
                message: Some(format!("undo group inconnu: {}", req.undo_group_id)),
            };
        };
        let mut group = self.store.groups.remove(pos);
        let mut restored = Vec::new();
        while let Some(file) = group.files.pop() {
            if StorageFs::check_cap(&req.caps, "fs.write", &file.vfs_path).is_err() {
                let denied = file.vfs_path.clone();
                group.files.push(file);
                self.store.groups.push(group);
                let _ = self.save();
                return FsUndoPatchResponse {
                    ok: false,
                    restored,
                    message: Some(format!("permission refusée pour {denied}")),
                };
            }
            let host = PathBuf::from(&file.host_path);
            match &file.previous {
                Some(content) => {
                    if let Some(parent) = host.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    if let Err(e) = fs::write(&host, content) {
                        return FsUndoPatchResponse {
                            ok: false,
                            restored,
                            message: Some(format!("restauration {}: {e}", host.display())),
                        };
                    }
                }
                None => {
                    if host.exists() {
                        if let Err(e) = fs::remove_file(&host) {
                            return FsUndoPatchResponse {
                                ok: false,
                                restored,
                                message: Some(format!("suppression {}: {e}", host.display())),
                            };
                        }
                    }
                }
            }
            restored.push(file.vfs_path);
        }
        let _ = self.save();
        FsUndoPatchResponse {
            ok: true,
            restored,
            message: None,
        }
    }
}

fn new_undo_group_id() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("undo-{ms}-{}", std::process::id())
}

/// Minimal unified-diff apply (single file, text). Supports `@@` hunks.
pub fn apply_unified_diff(original: &str, diff: &str) -> Result<String, String> {
    let orig_lines: Vec<&str> = if original.is_empty() {
        Vec::new()
    } else {
        original.lines().collect()
    };
    let mut out: Vec<String> = Vec::new();
    let mut src_idx: usize = 0;
    let mut saw_hunk = false;

    for line in diff.lines() {
        if line.starts_with("---") || line.starts_with("+++") || line.starts_with("diff ") {
            continue;
        }
        if line.starts_with("@@") {
            saw_hunk = true;
            // Parse `@@ -old_start,old_count +new_start,new_count @@`
            let old_start = parse_hunk_old_start(line).unwrap_or(1);
            let target = old_start.saturating_sub(1);
            while src_idx < target && src_idx < orig_lines.len() {
                out.push(orig_lines[src_idx].to_string());
                src_idx += 1;
            }
            continue;
        }
        if !saw_hunk {
            // Treat whole body as replacement if no hunk headers.
            return Ok(diff.to_string());
        }
        if line.starts_with('+') && !line.starts_with("+++") {
            out.push(line[1..].to_string());
        } else if line.starts_with('-') && !line.starts_with("---") {
            if src_idx >= orig_lines.len() {
                return Err("diff retire au-delà de la fin du fichier".into());
            }
            src_idx += 1;
        } else if let Some(rest) = line.strip_prefix(' ') {
            if src_idx >= orig_lines.len() || orig_lines[src_idx] != rest {
                return Err(format!(
                    "contexte mismatch ligne {}",
                    src_idx.saturating_add(1)
                ));
            }
            out.push(rest.to_string());
            src_idx += 1;
        } else if line.is_empty() || line == "\\ No newline at end of file" {
            continue;
        } else {
            return Err(format!("ligne de diff invalide: {line}"));
        }
    }

    while src_idx < orig_lines.len() {
        out.push(orig_lines[src_idx].to_string());
        src_idx += 1;
    }

    let mut s = out.join("\n");
    if (original.ends_with('\n') || (!original.is_empty() && saw_hunk))
        && !s.ends_with('\n')
        && !out.is_empty()
    {
        s.push('\n');
    }
    Ok(s)
}

fn parse_hunk_old_start(header: &str) -> Option<usize> {
    // @@ -12,3 +12,4 @@
    let rest = header.strip_prefix("@@")?;
    let minus = rest.find('-')?;
    let after = &rest[minus + 1..];
    let end = after.find([',', ' '])?;
    after[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::WorkspaceBindManager;
    use aos_proto::workspace::{fs_caps, FsApplyPatchRequest, FsPatchHunk, FsUndoPatchRequest};

    fn temp_root(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "aos-patch-{}-{}-{}",
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
    fn replace_all_and_undo() {
        let root = temp_root("repl");
        let proj = root.join("proj");
        fs::create_dir_all(&proj).unwrap();
        fs::write(proj.join("a.txt"), "old\n").unwrap();
        let sessions = root.join("sessions");
        let mut binds = WorkspaceBindManager::open(&sessions).unwrap();
        let info = binds
            .bind(proj.to_str().unwrap(), Some("patch-ws"), "agent:t")
            .unwrap();
        let mut patches = WorkspacePatchManager::open(&sessions).unwrap();
        let vfs = format!("{}/a.txt", info.vfs_root);
        let resp = patches.apply(
            &binds,
            &FsApplyPatchRequest {
                hunks: vec![FsPatchHunk {
                    path: vfs.clone(),
                    diff: "new\n".into(),
                    replace_all: true,
                }],
                actor: "agent:t".into(),
                caps: fs_caps(&info.workspace_id),
                trace_id: "t".into(),
                undo_group_id: None,
            },
        );
        assert!(resp.ok, "{:?}", resp.message);
        assert_eq!(fs::read_to_string(proj.join("a.txt")).unwrap(), "new\n");
        let undo = patches.undo(&FsUndoPatchRequest {
            undo_group_id: resp.undo_group_id.unwrap(),
            actor: "agent:t".into(),
            caps: fs_caps(&info.workspace_id),
            trace_id: "t".into(),
        });
        assert!(undo.ok, "{:?}", undo.message);
        assert_eq!(fs::read_to_string(proj.join("a.txt")).unwrap(), "old\n");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unified_diff_context() {
        let orig = "a\nb\nc\n";
        let diff = "@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n";
        let out = apply_unified_diff(orig, diff).unwrap();
        assert_eq!(out, "a\nB\nc\n");
    }
}
