//! État du panneau Sauvegarde (S1).

#[derive(Debug, Clone)]
pub(crate) struct BackupUiState {
    /// Dossier parent où créer `akasha-backup-<date>/`.
    pub(crate) dest_parent: String,
    /// Dossier de sauvegarde source pour la restauration.
    pub(crate) restore_dir: String,
    /// Un flag par entrée de `backup::BACKUP_SCOPES`.
    pub(crate) scopes: Vec<bool>,
    /// Dernier résultat affiché sous les boutons.
    pub(crate) last_result: String,
}

impl BackupUiState {
    pub(crate) fn with_defaults(dest_parent: String) -> Self {
        Self {
            dest_parent,
            restore_dir: String::new(),
            scopes: vec![true; crate::backup::BACKUP_SCOPES.len()],
            last_result: String::new(),
        }
    }
}
