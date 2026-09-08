//! Contrat IPC pour l'accès hors sandbox à un dossier hôte validé (issue #157).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub mod intents {
    pub const ACCESS: &str = "fs.host.access";
}

/// Action de confirmation / politique pour un dossier externe.
pub const HOST_FOLDER_ACCESS_ACTION: &str = intents::ACCESS;

/// Choix explicite présenté par l'UI après confirmation (même famille que device.*).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HostFolderPermission {
    #[default]
    Ask,
    AllowOnce,
    Always,
}

/// Opération demandée sur un chemin hors sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HostFolderOperation {
    Read,
    Write,
    List,
    Generate,
}

/// `fs.host.access` — lecture/écriture/liste sur un dossier hôte validé.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HostFolderAccessRequest {
    pub agent_id: String,
    pub session_id: String,
    pub path: String,
    pub operation: HostFolderOperation,
    /// Contenu pour write / generate.
    pub content: Option<String>,
    /// Format pour generate (md, txt, …).
    pub format: Option<String>,
    pub permission: HostFolderPermission,
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HostFolderAccessResponse {
    pub ok: bool,
    pub content: Option<String>,
    pub entries: Option<Vec<HostFolderEntry>>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HostFolderEntry {
    pub name: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HostFolderPermissionInfo {
    pub agent_id: String,
    pub folder_key: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HostFolderPermissionRevokeRequest {
    pub agent_id: String,
    pub folder_key: String,
}

/// Dernier segment d'un chemin (jamais le chemin brut complet dans l'UI).
pub fn folder_display_name(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        return "?".into();
    }
    let normalized = trimmed.replace('\\', "/");
    normalized
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("?")
        .to_string()
}

/// Dossier porteur du grant (parent si `path` ressemble à un fichier).
pub fn grant_folder_for_path(path: &str) -> String {
    use std::path::Path;
    let trimmed = path.trim().replace('\\', "/");
    if trimmed.is_empty() {
        return String::new();
    }
    let p = Path::new(&trimmed);
    let folder = if trimmed.ends_with('/') {
        p
    } else if p.extension().is_some() && p.parent().is_some() {
        p.parent().unwrap_or(p)
    } else {
        p
    };
    normalize_folder_key(folder.to_string_lossy().as_ref())
}

/// Clé stable pour stocker un grant persistant.
pub fn normalize_folder_key(path: &str) -> String {
    let mut out = path.trim().replace('\\', "/");
    while out.ends_with('/') && out.len() > 1 {
        out.pop();
    }
    if out.len() >= 2 && out.as_bytes()[1] == b':' {
        let (drive, rest) = out.split_at(2);
        format!("{}{}", drive.to_ascii_lowercase(), rest)
    } else {
        out
    }
}

/// True pour un chemin disque hôte (Windows ou Unix absolu hors sandbox logique).
pub fn looks_like_host_path(path: &str) -> bool {
    let trimmed = path.trim();
    let bytes = trimmed.as_bytes();
    if trimmed.contains('\\') {
        return true;
    }
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return true;
    }
    trimmed.starts_with('/')
        && !trimmed.starts_with("/documents")
        && !trimmed.starts_with("/downloads")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_uses_last_segment() {
        assert_eq!(folder_display_name("e:/test/test"), "test");
        assert_eq!(folder_display_name(r"E:\data\proj"), "proj");
    }

    #[test]
    fn grant_folder_from_file_path() {
        assert_eq!(grant_folder_for_path("e:/test/test/out.md"), "e:/test/test");
        assert_eq!(grant_folder_for_path("e:/test/test/"), "e:/test/test");
    }

    #[test]
    fn normalize_folder_key_lowercases_drive() {
        assert_eq!(normalize_folder_key("E:/Test/Test"), "e:/Test/Test");
    }
}
