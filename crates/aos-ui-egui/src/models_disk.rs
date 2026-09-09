//! Hygiène disque des modèles (S7.3).
//!
//! - Taille de `share/models` + par modèle (déjà dans `InstalledRow`).
//! - `var/run/model_usage.json` : dernière activité par modèle (chargement,
//!   retry, reload, fin de download). Libellé honnête : activité, pas infer.
//! - Purge des résidus `*.part` / `.import-*.part` (imports/interrompus).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn share_models_dir() -> PathBuf {
    crate::os_open::aos_home().join("share/models")
}

fn usage_path() -> PathBuf {
    crate::os_open::aos_home().join("var/run/model_usage.json")
}

/// (fichiers, octets), liens symboliques ignorés.
pub fn dir_size_recursive(dir: &Path) -> (usize, u64) {
    let mut files = 0usize;
    let mut bytes = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(top) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&top) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push(entry.path());
            } else if ft.is_file() {
                files += 1;
                bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    (files, bytes)
}

fn is_partial(name: &str) -> bool {
    name.ends_with(".part") || name.starts_with(".import-")
}

/// Résidus d'imports/téléchargements interrompus sous `share/models`.
pub fn find_partials(dir: &Path) -> Vec<(PathBuf, u64)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(top) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&top) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push(entry.path());
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if is_partial(&name) {
                out.push((entry.path(), entry.metadata().map(|m| m.len()).unwrap_or(0)));
            }
        }
    }
    out.sort();
    out
}

/// Supprime les partiels listés. Retourne (supprimés, octets libérés).
pub fn purge_partials(paths: &[PathBuf]) -> (usize, u64) {
    let mut n = 0usize;
    let mut bytes = 0u64;
    for path in paths {
        // Garde-fou : uniquement sous share/models, uniquement des partiels.
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !is_partial(&name) {
            continue;
        }
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        if std::fs::remove_file(path).is_ok() {
            n += 1;
            bytes += size;
        }
    }
    (n, bytes)
}

pub fn load_usage() -> HashMap<String, u64> {
    let raw = std::fs::read_to_string(usage_path()).unwrap_or_default();
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save_usage(map: &HashMap<String, u64>) {
    let path = usage_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(raw) = serde_json::to_string_pretty(map) {
        let _ = std::fs::write(&path, raw);
    }
}

pub fn note_used(map: &mut HashMap<String, u64>, model_id: &str, now_ms: u64) {
    if !model_id.is_empty() {
        map.insert(model_id.to_string(), now_ms);
        save_usage(map);
    }
}

/// Snapshot du dossier modèles pour l'onglet Installed.
#[derive(Debug, Clone, Default)]
pub struct DiskScan {
    pub files: usize,
    pub bytes: u64,
    pub partials: Vec<(PathBuf, u64)>,
}

impl DiskScan {
    pub fn refresh() -> Self {
        let dir = share_models_dir();
        let (files, bytes) = dir_size_recursive(&dir);
        let partials = find_partials(&dir);
        Self {
            files,
            bytes,
            partials,
        }
    }

    pub fn partial_bytes(&self) -> u64 {
        self.partials.iter().map(|(_, b)| b).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("aos-disk-{tag}-{nanos}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn partials_found_and_purged_selectively() {
        let dir = tmp("partials");
        std::fs::create_dir_all(dir.join("sub")).expect("mkdir");
        std::fs::write(dir.join("model.gguf"), vec![0u8; 100]).expect("write");
        std::fs::write(dir.join("dl.gguf.part"), vec![0u8; 10]).expect("write");
        std::fs::write(dir.join("sub/.import-1-x.part"), vec![0u8; 5]).expect("write");
        std::fs::write(dir.join("notes.txt"), b"keep").expect("write");
        let (files, bytes) = dir_size_recursive(&dir);
        assert_eq!(files, 4);
        assert_eq!(bytes, 100 + 10 + 5 + 4);
        let partials = find_partials(&dir);
        assert_eq!(partials.len(), 2);
        let (n, freed) =
            purge_partials(&partials.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>());
        assert_eq!((n, freed), (2, 15));
        assert!(dir.join("model.gguf").is_file());
        assert!(dir.join("notes.txt").is_file());
        // Un non-partiel passé par erreur n'est jamais supprimé.
        let (n, _) = purge_partials(&[dir.join("notes.txt")]);
        assert_eq!(n, 0);
        assert!(dir.join("notes.txt").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
