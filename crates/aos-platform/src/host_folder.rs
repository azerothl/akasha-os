//! Accès hors sandbox à un dossier hôte validé (issue #157).
//!
//! Les grants sont limités à un dossier précis (pas le disque entier). Aucun
//! mode « confiance ultime » : chaque dossier externe demande sa validation.

use aos_proto::host_folder::{
    folder_display_name, grant_folder_for_path, looks_like_host_path, normalize_folder_key,
    HostFolderEntry, HostFolderOperation,
};
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HostFolderError {
    #[error("chemin invalide: {0}")]
    InvalidPath(String),
    #[error("permission refusée pour le dossier")]
    PermissionDenied,
    #[error("fichier introuvable: {0}")]
    NotFound(String),
    #[error("io: {0}")]
    Io(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PersistentHostFolderGrant {
    agent_id: String,
    folder_key: String,
    display_name: String,
}

pub struct HostFolderGrantManager {
    permissions_path: PathBuf,
    permissions: Vec<PersistentHostFolderGrant>,
    /// Grants « une fois » valides uniquement pour la requête en cours (agent+folder).
    once_grants: HashSet<(String, String)>,
}

impl HostFolderGrantManager {
    pub fn open(sessions_root: impl Into<PathBuf>) -> Result<Self, HostFolderError> {
        let sessions_root = sessions_root.into();
        fs::create_dir_all(&sessions_root)
            .map_err(|e| HostFolderError::Io(e.to_string()))?;
        let permissions_path = sessions_root.join("host-folder-permissions.json");
        let permissions = match fs::read_to_string(&permissions_path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(HostFolderError::Io(e.to_string())),
        };
        Ok(Self {
            permissions_path,
            permissions,
            once_grants: HashSet::new(),
        })
    }

    pub fn has_persistent_grant(&self, agent_id: &str, folder_key: &str) -> bool {
        self.permissions
            .iter()
            .any(|p| p.agent_id == agent_id && p.folder_key == folder_key)
    }

    pub fn consume_once_grant(&mut self, agent_id: &str, folder_key: &str) -> bool {
        self.once_grants.remove(&(agent_id.to_string(), folder_key.to_string()))
    }

    pub fn grant_once(&mut self, agent_id: &str, folder_key: &str) {
        self.once_grants
            .insert((agent_id.to_string(), folder_key.to_string()));
    }

    pub fn grant_persistent(
        &mut self,
        agent_id: &str,
        folder_key: &str,
    ) -> Result<(), HostFolderError> {
        if !self.has_persistent_grant(agent_id, folder_key) {
            self.permissions.push(PersistentHostFolderGrant {
                agent_id: agent_id.into(),
                folder_key: folder_key.into(),
                display_name: folder_display_name(folder_key),
            });
            self.persist_permissions()?;
        }
        Ok(())
    }

    pub fn revoke(&mut self, agent_id: &str, folder_key: &str) -> Result<(), HostFolderError> {
        self.permissions
            .retain(|p| !(p.agent_id == agent_id && p.folder_key == folder_key));
        self.persist_permissions()?;
        Ok(())
    }

    pub fn persistent_permissions(
        &self,
        agent_id: Option<&str>,
    ) -> Vec<aos_proto::HostFolderPermissionInfo> {
        self.permissions
            .iter()
            .filter(|p| agent_id.map(|a| a == p.agent_id).unwrap_or(true))
            .map(|p| aos_proto::HostFolderPermissionInfo {
                agent_id: p.agent_id.clone(),
                folder_key: p.folder_key.clone(),
                display_name: p.display_name.clone(),
            })
            .collect()
    }

    pub fn is_authorized(
        &self,
        agent_id: &str,
        folder_key: &str,
        include_once: bool,
    ) -> bool {
        self.has_persistent_grant(agent_id, folder_key)
            || (include_once && self.once_grants.contains(&(agent_id.to_string(), folder_key.to_string())))
    }

    pub fn execute(
        &mut self,
        agent_id: &str,
        folder_key: &str,
        path: &str,
        operation: HostFolderOperation,
        content: Option<&str>,
        format: Option<&str>,
    ) -> Result<String, HostFolderError> {
        if !self.is_authorized(agent_id, folder_key, true) {
            return Err(HostFolderError::PermissionDenied);
        }
        let via_once = !self.has_persistent_grant(agent_id, folder_key)
            && self
                .once_grants
                .contains(&(agent_id.to_string(), folder_key.to_string()));
        let result = if looks_like_host_path(path) {
            self.execute_host(agent_id, folder_key, path, operation, content, format)
        } else {
            Err(HostFolderError::InvalidPath(
                "chemin logique hors sandbox: utilise fs.* après grant".into(),
            ))
        };
        if via_once {
            if result.is_ok() {
                self.consume_once_grant(agent_id, folder_key);
            }
        }
        result
    }

    fn execute_host(
        &self,
        agent_id: &str,
        folder_key: &str,
        path: &str,
        operation: HostFolderOperation,
        content: Option<&str>,
        format: Option<&str>,
    ) -> Result<String, HostFolderError> {
        let _ = agent_id;
        let host_path = host_path_from_token(path)?;
        let folder_path = host_path_from_token(folder_key)?;
        ensure_within_folder(&host_path, &folder_path)?;
        match operation {
            HostFolderOperation::Read => {
                let data = fs::read_to_string(&host_path)
                    .map_err(|e| HostFolderError::Io(e.to_string()))?;
                Ok(data)
            }
            HostFolderOperation::Write => {
                let content = content.unwrap_or("");
                if let Some(parent) = host_path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| HostFolderError::Io(e.to_string()))?;
                }
                fs::write(&host_path, content)
                    .map_err(|e| HostFolderError::Io(e.to_string()))?;
                Ok(format!("écrit {}", folder_display_name(path)))
            }
            HostFolderOperation::Generate => {
                let content = content.unwrap_or("");
                let ext = format.unwrap_or("md");
                let target = if host_path.extension().is_some() {
                    host_path
                } else {
                    host_path.with_extension(ext)
                };
                ensure_within_folder(&target, &folder_path)?;
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| HostFolderError::Io(e.to_string()))?;
                }
                fs::write(&target, content)
                    .map_err(|e| HostFolderError::Io(e.to_string()))?;
                Ok(target.to_string_lossy().into_owned())
            }
            HostFolderOperation::List => {
                let dir = if host_path.is_dir() {
                    host_path
                } else {
                    folder_path.clone()
                };
                ensure_within_folder(&dir, &folder_path)?;
                let entries = fs::read_dir(&dir)
                    .map_err(|e| HostFolderError::Io(e.to_string()))?
                    .filter_map(|e| e.ok())
                    .map(|e| {
                        let name = e.file_name().to_string_lossy().into_owned();
                        HostFolderEntry {
                            name,
                            is_dir: e.path().is_dir(),
                        }
                    })
                    .collect::<Vec<_>>();
                Ok(serde_json::to_string(&entries).unwrap_or_else(|_| "[]".into()))
            }
        }
    }

    fn persist_permissions(&self) -> Result<(), HostFolderError> {
        let raw = serde_json::to_vec_pretty(&self.permissions)
            .map_err(|e| HostFolderError::Io(e.to_string()))?;
        let tmp = self.permissions_path.with_extension("json.tmp");
        fs::write(&tmp, raw).map_err(|e| HostFolderError::Io(e.to_string()))?;
        fs::rename(&tmp, &self.permissions_path)
            .map_err(|e| HostFolderError::Io(e.to_string()))
    }
}

pub fn folder_key_for_request(path: &str) -> String {
    normalize_folder_key(&grant_folder_for_path(path))
}

pub fn path_within_granted_folder(path: &str, folder_key: &str) -> bool {
    if looks_like_host_path(path) {
        match (host_path_from_token(path), host_path_from_token(folder_key)) {
            (Ok(p), Ok(f)) => path_within_folder(&p, &f),
            _ => normalize_folder_key(path).starts_with(&normalize_folder_key(folder_key)),
        }
    } else {
        let p = normalize_folder_key(path);
        let f = normalize_folder_key(folder_key);
        p == f || p.starts_with(&format!("{f}/"))
    }
}

fn host_path_from_token(path: &str) -> Result<PathBuf, HostFolderError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(HostFolderError::InvalidPath("chemin vide".into()));
    }
    if !looks_like_host_path(trimmed) {
        return Err(HostFolderError::InvalidPath("pas un chemin hôte".into()));
    }
    let pb = PathBuf::from(trimmed);
    if pb.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(HostFolderError::InvalidPath(".. interdit".into()));
    }
    Ok(pb)
}

fn path_within_folder(path: &Path, folder: &Path) -> bool {
    let path_comps: Vec<_> = path.components().collect();
    let folder_comps: Vec<_> = folder.components().collect();
    if path_comps.len() < folder_comps.len() {
        return false;
    }
    path_comps[..folder_comps.len()] == folder_comps
}

fn ensure_within_folder(path: &Path, folder: &Path) -> Result<(), HostFolderError> {
    if path_within_folder(path, folder) {
        Ok(())
    } else {
        Err(HostFolderError::PermissionDenied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aos-host-folder-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn external_path_not_opened_without_grant() {
        let root = temp_root();
        let mut mgr = HostFolderGrantManager::open(&root).unwrap();
        let folder = root.join("sandbox");
        fs::create_dir_all(&folder).unwrap();
        let folder_key = folder.to_string_lossy().into_owned();
        let file = folder.join("secret.txt");
        fs::write(&file, "x").unwrap();
        let err = mgr
            .execute(
                "agent:a1",
                &folder_key,
                &file.to_string_lossy(),
                HostFolderOperation::Read,
                None,
                None,
            )
            .unwrap_err();
        assert_eq!(err, HostFolderError::PermissionDenied);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn allow_once_scoped_to_folder_not_disk() {
        let root = temp_root();
        let mut mgr = HostFolderGrantManager::open(&root).unwrap();
        let granted = root.join("granted");
        let other = root.join("other");
        fs::create_dir_all(&granted).unwrap();
        fs::create_dir_all(&other).unwrap();
        let g_key = granted.to_string_lossy().into_owned();
        let o_key = other.to_string_lossy().into_owned();
        let mut mgr = mgr;
        mgr.grant_once("agent:a1", &g_key);
        let f1 = granted.join("a.txt");
        fs::write(&f1, "ok").unwrap();
        assert!(mgr
            .execute(
                "agent:a1",
                &g_key,
                &f1.to_string_lossy(),
                HostFolderOperation::Read,
                None,
                None,
            )
            .is_ok());
        // once grant consumed
        assert!(!mgr.is_authorized("agent:a1", &g_key, true));
        mgr.grant_once("agent:a1", &g_key);
        let f2 = other.join("b.txt");
        fs::write(&f2, "nope").unwrap();
        assert_eq!(
            mgr.execute(
                "agent:a1",
                &g_key,
                &f2.to_string_lossy(),
                HostFolderOperation::Read,
                None,
                None,
            )
            .unwrap_err(),
            HostFolderError::PermissionDenied
        );
        assert!(mgr
            .execute("agent:a1", &o_key, &f2.to_string_lossy(), HostFolderOperation::Read, None, None)
            .is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn always_persists_for_that_folder_only() {
        let root = temp_root();
        let mut mgr = HostFolderGrantManager::open(&root).unwrap();
        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        let a_key = a.to_string_lossy().into_owned();
        let b_key = b.to_string_lossy().into_owned();
        mgr.grant_persistent("agent:a1", &a_key).unwrap();
        assert!(mgr.has_persistent_grant("agent:a1", &a_key));
        assert!(!mgr.has_persistent_grant("agent:a1", &b_key));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn deny_does_not_touch_host_path() {
        let root = temp_root();
        let mut mgr = HostFolderGrantManager::open(&root).unwrap();
        let folder = root.join("data");
        fs::create_dir_all(&folder).unwrap();
        let target = folder.join("out.txt");
        assert!(!target.exists());
        let folder_key = folder.to_string_lossy().into_owned();
        let err = mgr
            .execute(
                "agent:a1",
                &folder_key,
                &target.to_string_lossy(),
                HostFolderOperation::Write,
                Some("hello"),
                None,
            )
            .unwrap_err();
        assert_eq!(err, HostFolderError::PermissionDenied);
        assert!(!target.exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn path_within_granted_folder_checks_scope() {
        let root = temp_root();
        let granted = root.join("granted");
        let nested = granted.join("sub");
        fs::create_dir_all(&nested).unwrap();
        let key = granted.to_string_lossy().into_owned();
        let file = nested.join("x.txt");
        assert!(path_within_granted_folder(&file.to_string_lossy(), &key));
        let outside = root.join("outside.txt");
        assert!(!path_within_granted_folder(&outside.to_string_lossy(), &key));
        let _ = fs::remove_dir_all(&root);
    }
}
