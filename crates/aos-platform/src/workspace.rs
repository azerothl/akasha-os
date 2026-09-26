//! Bindings host workspace → `/host/<id>/**` (#247 P0 / DA.1).
//!
//! Extends the one-off `fs.host.access` grant model with a named, re-usable
//! workspace root and the cap strings agents must carry for host I/O.

use aos_proto::host_folder::{folder_display_name, looks_like_host_path, normalize_folder_key};
use aos_proto::workspace::{
    fs_caps, host_vfs_prefix, workspace_id_for_host_path, WorkspaceBindingInfo,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WorkspaceError {
    #[error("chemin hôte invalide: {0}")]
    InvalidPath(String),
    #[error("workspace inconnu: {0}")]
    NotFound(String),
    #[error("id workspace invalide: {0}")]
    InvalidId(String),
    #[error("io: {0}")]
    Io(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredBinding {
    workspace_id: String,
    host_path: String,
    agent_id: String,
    display_name: String,
}

pub struct WorkspaceBindManager {
    store_path: PathBuf,
    bindings: Vec<StoredBinding>,
}

impl WorkspaceBindManager {
    pub fn open(sessions_root: impl Into<PathBuf>) -> Result<Self, WorkspaceError> {
        let sessions_root = sessions_root.into();
        fs::create_dir_all(&sessions_root).map_err(|e| WorkspaceError::Io(e.to_string()))?;
        let store_path = sessions_root.join("workspace-bindings.json");
        let bindings = match fs::read_to_string(&store_path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(WorkspaceError::Io(e.to_string())),
        };
        Ok(Self {
            store_path,
            bindings,
        })
    }

    fn save(&self) -> Result<(), WorkspaceError> {
        let raw = serde_json::to_string_pretty(&self.bindings)
            .map_err(|e| WorkspaceError::Io(e.to_string()))?;
        fs::write(&self.store_path, raw).map_err(|e| WorkspaceError::Io(e.to_string()))
    }

    fn validate_id(id: &str) -> Result<(), WorkspaceError> {
        if id.is_empty()
            || id.len() > 48
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(WorkspaceError::InvalidId(id.into()));
        }
        Ok(())
    }

    pub fn bind(
        &mut self,
        host_path: &str,
        workspace_id: Option<&str>,
        agent_id: &str,
    ) -> Result<WorkspaceBindingInfo, WorkspaceError> {
        if !looks_like_host_path(host_path) {
            return Err(WorkspaceError::InvalidPath(host_path.into()));
        }
        let host_path = normalize_folder_key(host_path);
        let id = match workspace_id {
            Some(id) => {
                Self::validate_id(id)?;
                id.to_string()
            }
            None => workspace_id_for_host_path(&host_path),
        };
        Self::validate_id(&id)?;

        // Replace existing same id or same host path for this agent.
        self.bindings.retain(|b| {
            !(b.agent_id == agent_id && (b.workspace_id == id || b.host_path == host_path))
        });
        let display_name = folder_display_name(&host_path);
        self.bindings.push(StoredBinding {
            workspace_id: id.clone(),
            host_path: host_path.clone(),
            agent_id: agent_id.to_string(),
            display_name: display_name.clone(),
        });
        self.save()?;
        Ok(WorkspaceBindingInfo {
            workspace_id: id.clone(),
            host_path,
            vfs_root: host_vfs_prefix(&id),
            display_name,
            caps: fs_caps(&id),
        })
    }

    pub fn unbind(&mut self, workspace_id: &str, agent_id: &str) -> Result<(), WorkspaceError> {
        let before = self.bindings.len();
        self.bindings
            .retain(|b| !(b.agent_id == agent_id && b.workspace_id == workspace_id));
        if self.bindings.len() == before {
            return Err(WorkspaceError::NotFound(workspace_id.into()));
        }
        self.save()
    }

    pub fn list(&self, agent_id: &str) -> Vec<WorkspaceBindingInfo> {
        self.bindings
            .iter()
            .filter(|b| b.agent_id == agent_id || agent_id.is_empty())
            .map(|b| WorkspaceBindingInfo {
                workspace_id: b.workspace_id.clone(),
                host_path: b.host_path.clone(),
                vfs_root: host_vfs_prefix(&b.workspace_id),
                display_name: b.display_name.clone(),
                caps: fs_caps(&b.workspace_id),
            })
            .collect()
    }

    pub fn resolve_host_path(&self, workspace_id: &str) -> Option<String> {
        self.bindings
            .iter()
            .find(|b| b.workspace_id == workspace_id)
            .map(|b| b.host_path.clone())
    }

    /// Map `/host/<id>/rel` → absolute host path when bound.
    pub fn resolve_vfs_to_host(&self, vfs_path: &str) -> Option<PathBuf> {
        let id = aos_proto::workspace_id_from_vfs_path(vfs_path)?;
        let root = self.resolve_host_path(&id)?;
        let rel = aos_proto::vfs_relative_path(vfs_path).unwrap_or_default();
        if rel.is_empty() {
            return Some(PathBuf::from(root));
        }
        // Reject path escape.
        let mut out = PathBuf::from(&root);
        for comp in std::path::Path::new(&rel).components() {
            match comp {
                std::path::Component::Normal(s) => out.push(s),
                std::path::Component::CurDir => {}
                _ => return None,
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "aos-ws-{}-{}-{}",
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
    fn bind_list_unbind_roundtrip() {
        let root = temp_root("roundtrip");
        let mut mgr = WorkspaceBindManager::open(&root).unwrap();
        let info = mgr
            .bind("/tmp/aos-demo-project", None, "agent:test")
            .unwrap();
        assert!(info.vfs_root.starts_with("/host/"));
        assert!(info.caps.iter().any(|c| c.starts_with("fs.read:/host/")));
        assert!(info.caps.iter().any(|c| c.starts_with("fs.write:/host/")));

        let listed = mgr.list("agent:test");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].workspace_id, info.workspace_id);

        let host = mgr
            .resolve_vfs_to_host(&format!("{}/README.md", info.vfs_root))
            .unwrap();
        assert!(host.ends_with("README.md"));

        mgr.unbind(&info.workspace_id, "agent:test").unwrap();
        assert!(mgr.list("agent:test").is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_non_host_path() {
        let root = temp_root("bad");
        let mut mgr = WorkspaceBindManager::open(&root).unwrap();
        let err = mgr
            .bind("/documents/notes", None, "agent:test")
            .unwrap_err();
        assert!(matches!(err, WorkspaceError::InvalidPath(_)));
        let _ = fs::remove_dir_all(&root);
    }
}
