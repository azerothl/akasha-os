//! État de l'onglet Fichiers (S5).

use aos_proto::{DataClass, FsEntry};

#[derive(Debug)]
pub(crate) struct FilesUiState {
    /// Préfixe courant ("" = racine logique, toujours avec '/' final sauf "").
    pub(crate) prefix: String,
    pub(crate) entries: Vec<FsEntry>,
    /// Fichier ouvert en lecture/édition.
    pub(crate) open_path: Option<String>,
    pub(crate) open_content: String,
    pub(crate) open_class: DataClass,
    pub(crate) open_version: u64,
    pub(crate) open_dirty: bool,
    /// Création : nom (chemin relatif accepté, crée les dossiers).
    pub(crate) new_name: String,
    /// Renommage : destination (popup).
    pub(crate) rename_open: bool,
    pub(crate) rename_target: String,
    /// Renommage : source (le contenu est lu d'abord si pas déjà ouvert).
    pub(crate) rename_source: Option<String>,
    /// Renommage en attente de lecture : (source, destination).
    pub(crate) pending_rename: Option<(String, String)>,
    /// Suppression en attente de confirm.
    pub(crate) delete_confirm: Option<String>,
    pub(crate) filter: String,
}

impl Default for FilesUiState {
    fn default() -> Self {
        Self {
            prefix: String::new(),
            entries: Vec::new(),
            open_path: None,
            open_content: String::new(),
            open_class: DataClass::default(),
            open_version: 0,
            open_dirty: false,
            new_name: String::new(),
            rename_open: false,
            rename_target: String::new(),
            rename_source: None,
            pending_rename: None,
            delete_confirm: None,
            filter: String::new(),
        }
    }
}

impl FilesUiState {
    /// Sous-dossiers directs du préfixe courant, déduits des chemins plats.
    pub(crate) fn child_dirs(&self) -> Vec<String> {
        let mut dirs = Vec::new();
        for e in &self.entries {
            if let Some(rest) = e.path.strip_prefix(&self.prefix) {
                if let Some((head, _)) = rest.split_once('/') {
                    if !head.is_empty() && !dirs.iter().any(|d| d == head) {
                        dirs.push(head.to_string());
                    }
                }
            }
        }
        dirs.sort();
        dirs
    }

    /// Fichiers directs (sans '/' restant), filtrés par la recherche.
    pub(crate) fn child_files(&self) -> Vec<&FsEntry> {
        let q = self.filter.trim().to_lowercase();
        let mut out: Vec<&FsEntry> = self
            .entries
            .iter()
            .filter(|e| {
                e.path
                    .strip_prefix(&self.prefix)
                    .is_some_and(|rest| !rest.is_empty() && !rest.contains('/'))
            })
            .filter(|e| q.is_empty() || e.path.to_lowercase().contains(q.as_str()))
            .collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    /// Fil d'Ariane : [("", racine), ("docs/", docs), ...].
    pub(crate) fn crumbs(&self) -> Vec<(String, String)> {
        let mut crumbs = vec![(String::new(), "/".to_string())];
        let mut acc = String::new();
        for part in self.prefix.split('/').filter(|p| !p.is_empty()) {
            acc.push_str(part);
            acc.push('/');
            crumbs.push((acc.clone(), part.to_string()));
        }
        crumbs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str) -> FsEntry {
        FsEntry {
            path: path.into(),
            class: DataClass::Private,
            version: 1,
            size_bytes: 10,
        }
    }

    #[test]
    fn dirs_and_files_split_flat_prefix() {
        let mut s = FilesUiState::default();
        s.prefix = "docs/".into();
        s.entries = vec![
            entry("docs/a.md"),
            entry("docs/sub/b.md"),
            entry("docs/sub/c.md"),
            entry("other/d.md"),
        ];
        assert_eq!(s.child_dirs(), vec!["sub".to_string()]);
        let files: Vec<&str> = s.child_files().iter().map(|e| e.path.as_str()).collect();
        assert_eq!(files, vec!["docs/a.md"]);
        s.filter = "A.MD".into();
        assert_eq!(s.child_files().len(), 1);
        s.filter = "zzz".into();
        assert!(s.child_files().is_empty());
    }

    #[test]
    fn crumbs_build_prefixes() {
        let mut s = FilesUiState::default();
        s.prefix = "a/b/".into();
        let crumbs = s.crumbs();
        assert_eq!(crumbs.len(), 3);
        assert_eq!(crumbs[2].0, "a/b/");
    }
}
