//! Storage Subsystem v1 (§6) : FS versionné, transactions, undo, caps.
//!
//! Fallback « copy-on-write logique » userspace (pas de btrfs/ZFS sur
//! l'hôte) : chaque écriture crée une **version** ; un snapshot = manifeste
//! path→version (§6.1) ; les transactions sont des overlays journalisés
//! (§6.3) ; `undo` restaure la version précédente (F-AGT-08).
//!
//! Caps fichiers (§6.1, F-FS-04) : `fs.read:<glob>` / `fs.write:<glob>` /
//! `fs.reclassify:<glob>` — pas d'accès ambiant. Classification de
//! sensibilité (§6.4) avec héritage par dossier.

use aos_proto::{DataClass, FsEntry};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum FsError {
    #[error("chemin invalide: {0}")]
    InvalidPath(String),
    #[error("permission refusée: {cap} requis sur {path}")]
    PermissionDenied { cap: String, path: String },
    #[error("fichier introuvable: {0}")]
    NotFound(String),
    #[error("transaction inconnue: {0}")]
    UnknownTx(String),
    #[error("rien à annuler sur: {0}")]
    NothingToUndo(String),
    #[error("conflit de ressources: {0}")]
    Conflict(String),
    #[error("io: {0}")]
    Io(String),
}

impl From<std::io::Error> for FsError {
    fn from(e: std::io::Error) -> Self {
        FsError::Io(e.to_string())
    }
}

/// Métadonnées d'un fichier (index persistant).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMeta {
    pub class: DataClass,
    pub version: u64,
    pub created_by: String,
    pub deleted: bool,
}

/// Opération stagée dans une transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
enum TxOp {
    Write { path: String, content: String },
    Delete { path: String },
}

/// Transaction ouverte (§6.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub tx_id: String,
    pub actor: String,
    ops: Vec<TxOp>,
}

/// Le FS versionné.
pub struct StorageFs {
    root: PathBuf,
    index: HashMap<String, FileMeta>,
    txs: HashMap<String, Transaction>,
    next_tx: u64,
}

impl StorageFs {
    pub fn open(root: impl AsRef<Path>) -> std::io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(root.join("data"))?;
        std::fs::create_dir_all(root.join("versions"))?;
        std::fs::create_dir_all(root.join("snapshots"))?;
        let index_path = root.join("index.json");
        let index = if index_path.exists() {
            serde_json::from_str(&std::fs::read_to_string(&index_path)?).unwrap_or_default()
        } else {
            HashMap::new()
        };
        Ok(Self {
            root,
            index,
            txs: HashMap::new(),
            next_tx: 1,
        })
    }

    fn save_index(&self) -> std::io::Result<()> {
        std::fs::write(
            self.root.join("index.json"),
            serde_json::to_string_pretty(&self.index)?,
        )
    }

    /// Sécurise un chemin logique (`/documents/x.md`) en chemin hôte.
    fn resolve(&self, logical: &str) -> Result<PathBuf, FsError> {
        let rel = logical.trim_start_matches('/');
        let mut out = self.root.join("data");
        for comp in Path::new(rel).components() {
            match comp {
                Component::Normal(c) => out.push(c),
                _ => return Err(FsError::InvalidPath(logical.into())),
            }
        }
        if out.as_os_str().is_empty() {
            return Err(FsError::InvalidPath(logical.into()));
        }
        Ok(out)
    }

    fn dir_of(&self, logical: &str) -> String {
        match logical.rfind('/') {
            Some(0) | None => "/".into(),
            Some(i) => logical[..i].to_string(),
        }
    }

    /// Classe par défaut héritée du dossier parent (§6.4).
    fn default_class(&self, logical: &str) -> DataClass {
        if logical.starts_with("/home/") && logical.contains("/secrets/") {
            DataClass::Secret
        } else if self.dir_of(logical).starts_with("/system") {
            DataClass::Public
        } else {
            DataClass::Private
        }
    }

    /// Vérifie une capacité `kind` (`fs.read` / `fs.write` / `fs.reclassify`)
    /// sur `path` dans la liste de caps (glob `**` et `*`).
    pub fn check_cap(caps: &[String], kind: &str, path: &str) -> Result<(), FsError> {
        let want = format!("{kind}:{path}");
        for cap in caps {
            if let Some(pattern) = cap.strip_prefix(&format!("{kind}:")) {
                if glob_match(pattern, path) {
                    return Ok(());
                }
            }
        }
        Err(FsError::PermissionDenied {
            cap: want,
            path: path.into(),
        })
    }

    // --- lecture publique (pour le Module Runtime, avec caps) ---

    pub fn read(&self, path: &str, caps: &[String]) -> Result<(String, DataClass, u64), FsError> {
        Self::check_cap(caps, "fs.read", path)?;
        let meta = self
            .index
            .get(path)
            .filter(|m| !m.deleted)
            .ok_or_else(|| FsError::NotFound(path.into()))?;
        let content = std::fs::read_to_string(self.resolve(path)?)?;
        Ok((content, meta.class, meta.version))
    }

    pub fn list(&self, prefix: &str, caps: &[String]) -> Vec<FsEntry> {
        self.index
            .iter()
            .filter(|(p, m)| !m.deleted && p.starts_with(prefix))
            .filter(|(p, _)| Self::check_cap(caps, "fs.read", p).is_ok())
            .map(|(p, m)| FsEntry {
                path: p.clone(),
                class: m.class,
                version: m.version,
                size_bytes: self
                    .resolve(p)
                    .ok()
                    .and_then(|f| std::fs::metadata(f).ok())
                    .map(|md| md.len())
                    .unwrap_or(0),
            })
            .collect()
    }

    // --- écriture via transactions ---

    pub fn begin_tx(&mut self, actor: &str) -> String {
        let id = format!("tx-{}", self.next_tx);
        self.next_tx += 1;
        self.txs.insert(
            id.clone(),
            Transaction {
                tx_id: id.clone(),
                actor: actor.into(),
                ops: Vec::new(),
            },
        );
        id
    }

    pub fn stage_write(
        &mut self,
        tx_id: &str,
        path: &str,
        content: &str,
        caps: &[String],
    ) -> Result<(), FsError> {
        Self::check_cap(caps, "fs.write", path)?;
        self.resolve(path)?;
        let tx = self
            .txs
            .get_mut(tx_id)
            .ok_or_else(|| FsError::UnknownTx(tx_id.into()))?;
        tx.ops.push(TxOp::Write {
            path: path.into(),
            content: content.into(),
        });
        Ok(())
    }

    pub fn stage_delete(
        &mut self,
        tx_id: &str,
        path: &str,
        caps: &[String],
    ) -> Result<(), FsError> {
        Self::check_cap(caps, "fs.write", path)?;
        let tx = self
            .txs
            .get_mut(tx_id)
            .ok_or_else(|| FsError::UnknownTx(tx_id.into()))?;
        tx.ops.push(TxOp::Delete { path: path.into() });
        Ok(())
    }

    /// Écriture directe = transaction unitaire (réversibilité par défaut,
    /// F-AGT-08).
    pub fn write(
        &mut self,
        path: &str,
        content: &str,
        actor: &str,
        caps: &[String],
    ) -> Result<u64, FsError> {
        let tx = self.begin_tx(actor);
        self.stage_write(&tx, path, content, caps)?;
        self.commit(&tx)
    }

    /// Écriture binaire directe (images, PDF, téléchargements).
    pub fn write_bytes(
        &mut self,
        path: &str,
        content: &[u8],
        actor: &str,
        caps: &[String],
    ) -> Result<u64, FsError> {
        Self::check_cap(caps, "fs.write", path)?;
        self.version_existing(path)?;
        let host = self.resolve(path)?;
        if let Some(parent) = host.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&host, content)?;
        let default_class = self.default_class(path);
        let version = {
            let meta = self.index.entry(path.into()).or_insert(FileMeta {
                class: default_class,
                version: 0,
                created_by: actor.into(),
                deleted: false,
            });
            meta.version += 1;
            meta.deleted = false;
            meta.version
        };
        self.save_index()?;
        Ok(version)
    }

    /// Chemins hôtes autorisés pour `write_bytes_from_path` (sorties moteurs locaux).
    pub fn allowed_ingest_source(path: &Path) -> Result<PathBuf, FsError> {
        let canonical = std::fs::canonicalize(path)
            .map_err(|e| FsError::Io(format!("source introuvable: {e}")))?;
        let temp = std::env::temp_dir();
        if let Ok(temp_canon) = std::fs::canonicalize(&temp) {
            if canonical.starts_with(&temp_canon) {
                return Ok(canonical);
            }
        }
        if let Ok(home) = std::env::var("AOS_HOME") {
            let var_tmp = PathBuf::from(home).join("var/tmp");
            if let Ok(vt) = std::fs::canonicalize(&var_tmp) {
                if canonical.starts_with(&vt) {
                    return Ok(canonical);
                }
            }
        }
        Err(FsError::PermissionDenied {
            cap: "fs.write_from_path".into(),
            path: canonical.display().to_string(),
        })
    }

    /// Copie un fichier hôte vers le storage versionné (évite base64 IPC pour gros PNG).
    pub fn write_bytes_from_path(
        &mut self,
        path: &str,
        source_host: &Path,
        actor: &str,
        caps: &[String],
    ) -> Result<(u64, u64), FsError> {
        Self::check_cap(caps, "fs.write", path)?;
        let source = Self::allowed_ingest_source(source_host)?;
        self.version_existing(path)?;
        let host = self.resolve(path)?;
        if let Some(parent) = host.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&source, &host)?;
        let nbytes = std::fs::metadata(&host).map(|m| m.len()).unwrap_or(0);
        let default_class = self.default_class(path);
        let version = {
            let meta = self.index.entry(path.into()).or_insert(FileMeta {
                class: default_class,
                version: 0,
                created_by: actor.into(),
                deleted: false,
            });
            meta.version += 1;
            meta.deleted = false;
            meta.version
        };
        self.save_index()?;
        Ok((version, nbytes))
    }

    pub fn read_bytes(
        &self,
        path: &str,
        caps: &[String],
    ) -> Result<(Vec<u8>, DataClass, u64), FsError> {
        Self::check_cap(caps, "fs.read", path)?;
        let meta = self
            .index
            .get(path)
            .filter(|m| !m.deleted)
            .ok_or_else(|| FsError::NotFound(path.into()))?;
        let content = std::fs::read(self.resolve(path)?)?;
        Ok((content, meta.class, meta.version))
    }

    pub fn delete(&mut self, path: &str, actor: &str, caps: &[String]) -> Result<u64, FsError> {
        let tx = self.begin_tx(actor);
        self.stage_delete(&tx, path, caps)?;
        self.commit(&tx)
    }

    /// Commit : versionne l'existant puis applique l'overlay (COW logique).
    /// Arbitrage de conflits (§4.6) : si une autre transaction en attente
    /// touche les mêmes chemins, le commit est refusé (le perdant est
    /// notifié via le superviseur).
    pub fn commit(&mut self, tx_id: &str) -> Result<u64, FsError> {
        let tx = self
            .txs
            .get(tx_id)
            .ok_or_else(|| FsError::UnknownTx(tx_id.into()))?
            .clone();
        // Détection de conflit avec les autres transactions en attente.
        let my_paths: Vec<&String> = tx
            .ops
            .iter()
            .map(|op| match op {
                TxOp::Write { path, .. } => path,
                TxOp::Delete { path } => path,
            })
            .collect();
        for (other_id, other) in &self.txs {
            if other_id == tx_id {
                continue;
            }
            for op in &other.ops {
                let p = match op {
                    TxOp::Write { path, .. } => path,
                    TxOp::Delete { path } => path,
                };
                if my_paths.contains(&p) {
                    return Err(FsError::Conflict(format!(
                        "chemin {p} déjà engagé par la transaction {other_id}"
                    )));
                }
            }
        }
        let mut applied = 0;
        for op in &tx.ops {
            match op {
                TxOp::Write { path, content } => {
                    self.version_existing(path)?;
                    let host = self.resolve(path)?;
                    if let Some(parent) = host.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&host, content)?;
                    let default_class = self.default_class(path);
                    let meta = self.index.entry(path.clone()).or_insert(FileMeta {
                        class: default_class,
                        version: 0,
                        created_by: tx.actor.clone(),
                        deleted: false,
                    });
                    meta.version += 1;
                    meta.deleted = false;
                    applied += 1;
                }
                TxOp::Delete { path } => {
                    self.version_existing(path)?;
                    let host = self.resolve(path)?;
                    if host.exists() {
                        std::fs::remove_file(&host)?;
                    }
                    if let Some(meta) = self.index.get_mut(path) {
                        meta.version += 1;
                        meta.deleted = true;
                    }
                    applied += 1;
                }
            }
        }
        self.txs.remove(tx_id);
        self.save_index()?;
        Ok(applied)
    }

    pub fn rollback(&mut self, tx_id: &str) -> Result<(), FsError> {
        self.txs
            .remove(tx_id)
            .ok_or_else(|| FsError::UnknownTx(tx_id.into()))?;
        Ok(())
    }

    /// Archive la version courante d'un fichier (avant modification).
    fn version_existing(&self, path: &str) -> Result<(), FsError> {
        let host = self.resolve(path)?;
        if !host.exists() {
            return Ok(()); // création : rien à versionner
        }
        let current_version = self.index.get(path).map(|m| m.version).unwrap_or(0);
        let enc = encode_path(path);
        let vdir = self.root.join("versions").join(&enc);
        std::fs::create_dir_all(&vdir)?;
        std::fs::copy(&host, vdir.join(format!("v{current_version:04}")))?;
        Ok(())
    }

    /// Undo : restaure la version précédente (F-AGT-08).
    pub fn undo(&mut self, path: &str) -> Result<(Option<u64>, String), FsError> {
        let meta = self
            .index
            .get(path)
            .ok_or_else(|| FsError::NotFound(path.into()))?;
        let current = meta.version;
        let enc = encode_path(path);
        let vdir = self.root.join("versions").join(&enc);
        let host = self.resolve(path)?;

        if current == 0 || !vdir.exists() {
            // Jamais modifié depuis sa création → l'état antérieur est
            // « n'existait pas ».
            if host.exists() {
                std::fs::remove_file(&host)?;
            }
            if let Some(m) = self.index.get_mut(path) {
                m.deleted = true;
                m.version += 1;
            }
            self.save_index()?;
            return Ok((None, "fichier supprimé (n'existait pas avant)".into()));
        }
        let prev = vdir.join(format!("v{:04}", current - 1));
        if !prev.exists() {
            return Err(FsError::NothingToUndo(path.into()));
        }
        // La version courante devient à son tour récupérable.
        self.version_existing(path)?;
        std::fs::copy(&prev, &host)?;
        if let Some(m) = self.index.get_mut(path) {
            m.deleted = false;
            m.version += 1;
        }
        self.save_index()?;
        Ok((
            Some(current - 1),
            format!("version {} restaurée", current - 1),
        ))
    }

    /// Snapshot logique nommé : manifeste path→version (§6.1).
    pub fn snapshot(&mut self, name: &str) -> Result<usize, FsError> {
        let manifest: HashMap<&String, &FileMeta> =
            self.index.iter().filter(|(_, m)| !m.deleted).collect();
        let n = manifest.len();
        std::fs::write(
            self.root.join("snapshots").join(format!("{name}.json")),
            serde_json::to_string_pretty(&manifest).map_err(|e| FsError::Io(e.to_string()))?,
        )?;
        Ok(n)
    }

    /// Classification explicite (cap `fs.reclassify`, §6.4).
    pub fn set_class(
        &mut self,
        path: &str,
        class: DataClass,
        caps: &[String],
    ) -> Result<(), FsError> {
        Self::check_cap(caps, "fs.reclassify", path)?;
        let meta = self
            .index
            .get_mut(path)
            .ok_or_else(|| FsError::NotFound(path.into()))?;
        meta.class = class;
        self.save_index()?;
        Ok(())
    }

    pub fn class_of(&self, path: &str) -> Option<DataClass> {
        self.index.get(path).map(|m| m.class)
    }
}

/// Encodage d'un chemin logique en nom de dossier sûr.
fn encode_path(path: &str) -> String {
    path.trim_start_matches('/').replace(['/', '\\'], "__")
}

/// Glob minimal : `**` = tout suffixe, `*` = un segment.
pub fn glob_match(pattern: &str, path: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix("**") {
        return path.starts_with(prefix);
    }
    let p: Vec<&str> = pattern.split('/').collect();
    let t: Vec<&str> = path.split('/').collect();
    if p.len() != t.len() {
        return false;
    }
    p.iter().zip(t.iter()).all(|(a, b)| *a == "*" || a == b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fs() -> (StorageFs, PathBuf) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "aos-fs-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        (StorageFs::open(&dir).unwrap(), dir)
    }

    fn caps() -> Vec<String> {
        vec![
            "fs.write:/documents/**".into(),
            "fs.read:/documents/**".into(),
        ]
    }

    #[test]
    fn write_read_versionning() {
        let (mut fs, dir) = fs();
        let v1 = fs.write("/documents/a.md", "v1", "test", &caps()).unwrap();
        assert_eq!(v1, 1);
        let (c, class, _v) = fs.read("/documents/a.md", &caps()).unwrap();
        assert_eq!(c, "v1");
        assert_eq!(class, DataClass::Private);
        fs.write("/documents/a.md", "v2", "test", &caps()).unwrap();
        let (c2, _, v2) = fs.read("/documents/a.md", &caps()).unwrap();
        assert_eq!(c2, "v2");
        assert_eq!(v2, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_restaure_la_version_precedente() {
        let (mut fs, dir) = fs();
        fs.write("/documents/a.md", "v1", "test", &caps()).unwrap();
        fs.write("/documents/a.md", "v2", "test", &caps()).unwrap();
        let (restored, _) = fs.undo("/documents/a.md").unwrap();
        assert_eq!(restored, Some(1));
        let (c, _, _) = fs.read("/documents/a.md", &caps()).unwrap();
        assert_eq!(c, "v1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_sur_creation_supprime_le_fichier() {
        let (mut fs, dir) = fs();
        fs.write("/documents/nouveau.md", "x", "agent:1", &caps())
            .unwrap();
        let (restored, desc) = fs.undo("/documents/nouveau.md").unwrap();
        assert_eq!(restored, None);
        assert!(desc.contains("n'existait pas"));
        assert!(fs.read("/documents/nouveau.md", &caps()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rollback_annule_la_transaction() {
        let (mut fs, dir) = fs();
        let tx = fs.begin_tx("agent:1");
        fs.stage_write(&tx, "/documents/b.md", "brouillon", &caps())
            .unwrap();
        fs.rollback(&tx).unwrap();
        assert!(fs.read("/documents/b.md", &caps()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cap_manquante_refusee() {
        let (mut fs, dir) = fs();
        let err = fs
            .write("/etc/secret", "x", "agent:1", &caps())
            .unwrap_err();
        assert!(matches!(err, FsError::PermissionDenied { .. }));
        let err2 = fs.read("/documents/a.md", &[]).unwrap_err();
        assert!(matches!(err2, FsError::PermissionDenied { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn classification_et_heritage() {
        let (mut fs, dir) = fs();
        fs.write(
            "/home/u/secrets/key.txt",
            "k",
            "human",
            &["fs.write:/home/**".into()],
        )
        .unwrap();
        assert_eq!(
            fs.class_of("/home/u/secrets/key.txt"),
            Some(DataClass::Secret)
        );
        // Reclassify exige la cap dédiée.
        assert!(fs
            .set_class(
                "/home/u/secrets/key.txt",
                DataClass::Public,
                &["fs.write:/home/**".into()]
            )
            .is_err());
        fs.set_class(
            "/home/u/secrets/key.txt",
            DataClass::Public,
            &["fs.reclassify:/home/**".into()],
        )
        .unwrap();
        assert_eq!(
            fs.class_of("/home/u/secrets/key.txt"),
            Some(DataClass::Public)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn glob_matching() {
        assert!(glob_match("/documents/notes/**", "/documents/notes/a/b.md"));
        assert!(glob_match("/documents/*", "/documents/x"));
        assert!(!glob_match("/documents/*", "/documents/a/b"));
        assert!(glob_match("/documents/a.md", "/documents/a.md"));
        assert!(!glob_match("/documents/a.md", "/documents/b.md"));
    }
}
