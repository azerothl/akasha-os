//! Sauvegarde / restauration locale (S1, phase 2 avec chiffrement).
//!
//! Copie + `manifest.json` (tailles + sha256). Chiffré (par défaut) via
//! l'enveloppe ChaCha20-Poly1305 du vault (`aous-platform::secrets`,
//! clé maître de CE pc/utilisateur) : chaque fichier est scellé en
//! `<rel>.enc`, le manifest reste lisible (noms/tailles visibles, contenus
//! chiffrés — documenté dans l'UI). `var/secrets` reste exclu (clés à
//! ressaisir après restore). Relance requise après restauration.

use aos_platform::secrets::SecretStore;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::UiApp;
use eframe::egui;

pub const BACKUP_MANIFEST_VERSION: u32 = 1;
pub const BACKUP_DIR_PREFIX: &str = "akasha-backup-";
pub const BACKUP_EXPORT_PREFIX: &str = "akasha-export-";

/// Dossiers `var/<dir>` sauvegardables, dans l'ordre d'affichage UI.
pub struct BackupScope {
    pub id: &'static str,
    pub dir: &'static str,
    pub label_fr: &'static str,
    pub label_en: &'static str,
}

pub const BACKUP_SCOPES: &[BackupScope] = &[
    BackupScope {
        id: "sessions",
        dir: "sessions",
        label_fr: "Sessions + canvas",
        label_en: "Sessions + canvas",
    },
    BackupScope {
        id: "memory",
        dir: "memory",
        label_fr: "Mémoire",
        label_en: "Memory",
    },
    BackupScope {
        id: "storage",
        dir: "storage",
        label_fr: "Documents",
        label_en: "Documents",
    },
    BackupScope {
        id: "modules",
        dir: "modules",
        label_fr: "Données modules (notes/tâches)",
        label_en: "Module data (notes/tasks)",
    },
    BackupScope {
        id: "schedules",
        dir: "schedules",
        label_fr: "Planifications",
        label_en: "Schedules",
    },
    BackupScope {
        id: "feedback",
        dir: "feedback",
        label_fr: "Retours",
        label_en: "Feedback",
    },
    BackupScope {
        id: "mcp",
        dir: "mcp",
        label_fr: "Config MCP",
        label_en: "MCP config",
    },
    BackupScope {
        id: "skills",
        dir: "skills",
        label_fr: "Skills perso",
        label_en: "Custom skills",
    },
    BackupScope {
        id: "audit",
        dir: "audit",
        label_fr: "Journal d'audit",
        label_en: "Audit journal",
    },
];

/// Fichiers de config toujours inclus (petits, hors scopes).
pub const BACKUP_CONFIG_FILES: &[&str] = &[
    "run/preferences.json",
    "run/onboarding.json",
    "run/composer_drafts.json",
];

/// Explicite et honnête : ce qui n'est PAS sauvegardé, et pourquoi.
pub const BACKUP_SKIPPED: &[(&str, &str)] = &[
    ("models/", "poids lourds — retéléchargeables depuis Modèles"),
    (
        "secrets/",
        "clés chiffrées — à ressaisir après restore (non exportées par sécurité)",
    ),
    ("downloads/", "cache généré — reconstructible"),
    ("updates/", "paquets d'update — retéléchargeables"),
    ("agents/", "état éphémère — relance requise de toute façon"),
    ("run/ (hors config)", "locks/sockets éphémères"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupFileEntry {
    pub rel: String,
    pub len: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub version: u32,
    pub kind: String,
    pub created_ms: u128,
    pub scopes: Vec<String>,
    pub files: Vec<BackupFileEntry>,
    pub skipped: Vec<String>,
    /// Chiffré (fichiers en `<rel>.enc`, sha256 = clair vérifié après déchiffrement).
    #[serde(default)]
    pub encrypted: bool,
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("lecture {}: {e}", path.display()))?;
    Ok(sha256_bytes(&bytes))
}

fn copy_dir_recursive(
    src: &Path,
    dst: &Path,
    out: &mut Vec<BackupFileEntry>,
    rel_base: &Path,
    seal: Option<&SecretStore>,
) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| format!("mkdir {}: {e}", dst.display()))?;
    let entries = std::fs::read_dir(src).map_err(|e| format!("list {}: {e}", src.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("entrée {}: {e}", src.display()))?;
        let file_type = entry
            .file_type()
            .map_err(|e| format!("type {}: {e}", entry.path().display()))?;
        // Liens symboliques ignorés (pas de cycle, pas de sortie du scope).
        if file_type.is_symlink() {
            continue;
        }
        let src_path = entry.path();
        let name = entry.file_name();
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst.join(&name), out, rel_base, seal)?;
        } else if file_type.is_file() {
            let plain = std::fs::read(&src_path)
                .map_err(|e| format!("lecture {}: {e}", src_path.display()))?;
            let sha256 = sha256_bytes(&plain);
            let rel_inner = dst
                .join(&name)
                .strip_prefix(rel_base)
                .map_err(|_| "chemin hors backup".to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let (payload, rel) = match seal {
                Some(store) => {
                    let sealed = store
                        .seal_bytes("ui-egui", &plain)
                        .map_err(|e| format!("scellement {}: {e}", src_path.display()))?;
                    (sealed, format!("{rel_inner}.enc"))
                }
                None => (plain, rel_inner),
            };
            let len = payload.len() as u64;
            let dst_path = rel_base.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            if let Some(parent) = dst_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
            }
            std::fs::write(&dst_path, &payload)
                .map_err(|e| format!("écriture {}: {e}", dst_path.display()))?;
            out.push(BackupFileEntry { rel, len, sha256 });
        }
    }
    Ok(())
}

fn timestamp_dir_name(prefix: &str) -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    // Calendrier civil (days-to-civil, Howard Hinnant, domaine public).
    let secs = (ms / 1000) as i64;
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    y += i64::from(m <= 2);
    let h = (secs % 86_400) / 3_600;
    let mi = (secs % 3_600) / 60;
    let s = secs % 60;
    format!("{prefix}{y:04}-{m:02}-{d:02}-{h:02}{mi:02}{s:02}-{ms}")
}

pub fn default_backup_parent() -> PathBuf {
    crate::os_open::aos_home().join("var/backups")
}

/// Crée `dest_parent/akasha-backup-<date>/` (+ manifest). `mask[i]` = scope i.
/// `kind` = "backup" (tout coché) ou "export" (scopes imposés par l'appelant).
/// `seal` = chiffrer chaque fichier sous la clé maître de CE pc/utilisateur
/// (noms/tailles visibles, contenus chiffrés).
pub fn do_backup(
    home: &Path,
    dest_parent: &Path,
    prefix: &str,
    kind: &str,
    mask: &[bool],
    seal: bool,
) -> Result<(PathBuf, usize, u64), String> {
    if mask.len() != BACKUP_SCOPES.len() {
        return Err("masque de scopes invalide".into());
    }
    if mask.len() != BACKUP_SCOPES.len() {
        return Err("masque de scopes invalide".into());
    }
    if !mask.iter().any(|b| *b) {
        return Err("aucun scope sélectionné".into());
    }
    std::fs::create_dir_all(dest_parent)
        .map_err(|e| format!("dossier destination {}: {e}", dest_parent.display()))?;
    let dir = dest_parent.join(timestamp_dir_name(prefix));
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    let store = if seal {
        Some(
            SecretStore::open(home.join("var/secrets"))
                .map_err(|e| format!("coffre indisponible : {e}"))?,
        )
    } else {
        None
    };
    let var = home.join("var");
    let mut files = Vec::new();
    let mut scopes = Vec::new();
    for (scope, enabled) in BACKUP_SCOPES.iter().zip(mask.iter()) {
        if !enabled {
            continue;
        }
        let src = var.join(scope.dir);
        if !src.is_dir() {
            continue;
        }
        copy_dir_recursive(&src, &dir.join(scope.dir), &mut files, &dir, store.as_ref())?;
        scopes.push(scope.id.to_string());
    }
    for rel in BACKUP_CONFIG_FILES {
        let src = home.join("var").join(rel);
        if src.is_file() {
            let plain = std::fs::read(&src).map_err(|e| format!("lecture {rel}: {e}"))?;
            let sha256 = sha256_bytes(&plain);
            let (payload, out_rel) = match &store {
                Some(st) => (
                    st.seal_bytes("ui-egui", &plain)
                        .map_err(|e| format!("scellement {rel}: {e}"))?,
                    format!("{rel}.enc"),
                ),
                None => (plain, rel.to_string()),
            };
            let len = payload.len() as u64;
            let dst = dir.join(out_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
            }
            std::fs::write(&dst, &payload).map_err(|e| format!("écriture {rel}: {e}"))?;
            files.push(BackupFileEntry {
                rel: out_rel,
                len,
                sha256,
            });
        }
    }
    if files.is_empty() {
        let _ = std::fs::remove_dir_all(&dir);
        return Err("rien à sauvegarder (dossiers vides ou absents)".into());
    }
    let manifest = BackupManifest {
        version: BACKUP_MANIFEST_VERSION,
        kind: kind.to_string(),
        created_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
        scopes,
        skipped: BACKUP_SKIPPED
            .iter()
            .map(|(d, why)| format!("{d} — {why}"))
            .collect(),
        files,
        encrypted: store.is_some(),
    };
    let raw = serde_json::to_string_pretty(&manifest).map_err(|e| format!("manifest: {e}"))?;
    std::fs::write(dir.join("manifest.json"), raw)
        .map_err(|e| format!("écriture manifest: {e}"))?;
    let total: u64 = manifest.files.iter().map(|f| f.len).sum();
    let n = manifest.files.len();
    Ok((dir, n, total))
}

/// Vérifie manifest + version + présence/taille (+ sha256 si en clair).
/// Chiffré : le sha256 porte sur le clair, vérifié après déchiffrement au
/// restore — ici on contrôle présence + taille du blob.
pub fn verify_backup(dir: &Path) -> Result<BackupManifest, String> {
    let raw = std::fs::read_to_string(dir.join("manifest.json"))
        .map_err(|_| "manifest.json introuvable — pas un dossier de sauvegarde".to_string())?;
    let manifest: BackupManifest =
        serde_json::from_str(&raw).map_err(|e| format!("manifest illisible: {e}"))?;
    if manifest.version != BACKUP_MANIFEST_VERSION {
        return Err(format!(
            "version manifest {} non supportée (attendue {})",
            manifest.version, BACKUP_MANIFEST_VERSION
        ));
    }
    for f in &manifest.files {
        if f.rel.contains("..") {
            return Err(format!("chemin suspect dans le manifest : {}", f.rel));
        }
        let path = dir.join(f.rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let meta = std::fs::metadata(&path).map_err(|_| format!("fichier manquant : {}", f.rel))?;
        if meta.len() != f.len {
            return Err(format!(
                "taille différente : {} ({} ≠ {})",
                f.rel,
                meta.len(),
                f.len
            ));
        }
        if !manifest.encrypted && sha256_file(&path)? != f.sha256 {
            return Err(format!("sha256 différent : {}", f.rel));
        }
    }
    Ok(manifest)
}

fn open_secrets_for_restore(home: &Path) -> Result<SecretStore, String> {
    SecretStore::open(home.join("var/secrets")).map_err(|e| {
        let msg = e.to_string().to_lowercase();
        if msg.contains("crypto") || msg.contains("aead") {
            "déchiffrement impossible (mauvaise machine/utilisateur ou clé maître différente)"
                .to_string()
        } else {
            format!("coffre indisponible (restauration chiffrée impossible ici) : {e}")
        }
    })
}

/// Restaure (vérifie d'abord) : remplace les scopes + configs, exige relance.
/// Chiffré : déchiffre sous la clé maître locale (autre PC/utilisateur =
/// erreur propre), vérifie le sha256 du clair à l'écriture.
pub fn do_restore(home: &Path, dir: &Path) -> Result<usize, String> {
    let manifest = verify_backup(dir)?;
    let store = if manifest.encrypted {
        Some(open_secrets_for_restore(home)?)
    } else {
        None
    };
    let var = home.join("var");
    // Lit un fichier du backup (déchiffre si besoin) + vérifie son sha256 clair.
    let read_entry = |rel: &str| -> Result<Vec<u8>, String> {
        let path = dir.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let blob = std::fs::read(&path).map_err(|e| format!("lecture {}: {e}", path.display()))?;
        let plain = match &store {
            Some(st) => st
                .unseal_bytes("ui-egui", &blob)
                .map_err(|_| format!("déchiffrement impossible : {rel} (mauvaise machine/utilisateur ou backup altéré)"))?,
            None => blob,
        };
        Ok(plain)
    };
    // Garde-fou : seuls les scopes + run/* connus sont restaurés (rel dest).
    for f in &manifest.files {
        let dest_rel = f.rel.strip_suffix(".enc").unwrap_or(&f.rel);
        let first = dest_rel.split('/').next().unwrap_or_default();
        let allowed = BACKUP_SCOPES.iter().any(|s| s.dir == first)
            || (first == "run" && BACKUP_CONFIG_FILES.contains(&dest_rel));
        if !allowed {
            return Err(format!("entrée hors scopes autorisés : {}", f.rel));
        }
    }
    // Remplace chaque scope d'un bloc (évite le mélange ancien/nouveau).
    for scope in BACKUP_SCOPES
        .iter()
        .map(|s| s.dir)
        .chain(std::iter::once("run"))
    {
        if scope == "run" {
            // run/ : uniquement les 3 fichiers de config, jamais tout le dossier.
            for rel in BACKUP_CONFIG_FILES {
                let src_rel = if manifest.encrypted {
                    format!("{rel}.enc")
                } else {
                    rel.to_string()
                };
                let found = manifest.files.iter().any(|f| f.rel == src_rel);
                if !found {
                    continue;
                }
                let plain = read_entry(&src_rel)?;
                let entry = manifest
                    .files
                    .iter()
                    .find(|f| f.rel == src_rel)
                    .expect("found");
                if sha256_bytes(&plain) != entry.sha256 {
                    return Err(format!("sha256 différent après déchiffrement : {src_rel}"));
                }
                let d = var.join(rel);
                if let Some(parent) = d.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
                }
                std::fs::write(&d, &plain).map_err(|e| format!("restore {rel}: {e}"))?;
            }
            continue;
        }
        let has_scope = manifest.files.iter().any(|f| {
            f.rel == scope
                || f.rel.starts_with(&format!("{scope}/"))
                || f.rel.starts_with(&format!("{scope}."))
        });
        if !has_scope {
            continue;
        }
        let dst = var.join(scope);
        if dst.exists() {
            std::fs::remove_dir_all(&dst)
                .map_err(|e| format!("nettoyage {}: {e}", dst.display()))?;
        }
        std::fs::create_dir_all(&dst).map_err(|e| format!("mkdir {}: {e}", dst.display()))?;
        for f in manifest
            .files
            .iter()
            .filter(|f| f.rel.starts_with(&format!("{scope}/")) || f.rel == scope)
        {
            let plain = read_entry(&f.rel)?;
            if sha256_bytes(&plain) != f.sha256 {
                return Err(format!("sha256 différent après déchiffrement : {}", f.rel));
            }
            let dest_rel = f.rel.strip_suffix(".enc").unwrap_or(&f.rel);
            let out = dst.join(
                dest_rel
                    .strip_prefix(&format!("{scope}/"))
                    .unwrap_or(dest_rel)
                    .replace('/', std::path::MAIN_SEPARATOR_STR),
            );
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
            }
            std::fs::write(&out, &plain).map_err(|e| format!("restore {}: {e}", f.rel))?;
        }
    }
    Ok(manifest.files.len())
}

fn human_bytes(n: u64) -> String {
    if n >= 1 << 30 {
        format!("{:.1} GiB", n as f64 / (1 << 30) as f64)
    } else if n >= 1 << 20 {
        format!("{:.1} MiB", n as f64 / (1 << 20) as f64)
    } else if n >= 1 << 10 {
        format!("{:.1} KiB", n as f64 / (1 << 10) as f64)
    } else {
        format!("{n} o")
    }
}

impl UiApp {
    pub(crate) fn ui_backup(&mut self, ui: &mut egui::Ui) {
        let fr = self.prefs.language == "fr";
        ui.heading(if fr { "Sauvegarde" } else { "Backup" });
        ui.weak(if fr {
            "Copie + manifest (tailles + sha256) vers un dossier. Chiffré par défaut sous la clé de CE pc/utilisateur (noms/tailles visibles, contenus chiffrés). Clés (var/secrets) exclues — à ressaisir après restauration. Relancez Preview après une restauration."
        } else {
            "Copy + manifest (sizes + sha256) to a folder. Encrypted by default under THIS machine/user key (names/sizes visible, contents encrypted). Keys (var/secrets) excluded — re-enter after restore. Relaunch Preview after restoring."
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(if fr {
                "Dossier parent"
            } else {
                "Parent folder"
            });
            ui.add(
                egui::TextEdit::singleline(&mut self.backup_ui.dest_parent).desired_width(280.0),
            );
            if ui.small_button("…").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.backup_ui.dest_parent = dir.to_string_lossy().into_owned();
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (i, scope) in BACKUP_SCOPES.iter().enumerate() {
                let label = if fr { scope.label_fr } else { scope.label_en };
                let mut on = self.backup_ui.scopes.get(i).copied().unwrap_or(false);
                if ui.checkbox(&mut on, label).changed() {
                    if self.backup_ui.scopes.len() != BACKUP_SCOPES.len() {
                        self.backup_ui.scopes = vec![true; BACKUP_SCOPES.len()];
                    }
                    self.backup_ui.scopes[i] = on;
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(
                &mut self.backup_ui.encrypted,
                if fr {
                    "Chiffrer (clé de ce PC — restaure ici uniquement)"
                } else {
                    "Encrypt (this PC key — restores here only)"
                },
            );
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(if fr { "Sauvegarder" } else { "Back up" })
                .clicked()
            {
                let home = crate::os_open::aos_home();
                let dest = PathBuf::from(self.backup_ui.dest_parent.clone());
                let seal = self.backup_ui.encrypted;
                match do_backup(
                    &home,
                    &dest,
                    BACKUP_DIR_PREFIX,
                    "backup",
                    &self.backup_ui.scopes.clone(),
                    seal,
                ) {
                    Ok((dir, n, total)) => {
                        let msg = if fr {
                            format!(
                                "Sauvegardé : {} ({} fichiers, {})",
                                dir.display(),
                                n,
                                human_bytes(total)
                            )
                        } else {
                            format!(
                                "Backed up: {} ({n} files, {})",
                                dir.display(),
                                human_bytes(total)
                            )
                        };
                        self.backup_ui.last_result = msg.clone();
                        self.push_status(msg.clone());
                        self.toasts.push_success(msg);
                    }
                    Err(e) => {
                        self.backup_ui.last_result = e.clone();
                        self.toasts.push_error(e);
                    }
                }
            }
            if ui
                .button(if fr { "Exporter JSON" } else { "Export JSON" })
                .clicked()
            {
                // Export lisible : conversations + mémoire + documents.
                let mut mask = vec![false; BACKUP_SCOPES.len()];
                for (i, s) in BACKUP_SCOPES.iter().enumerate() {
                    if matches!(s.id, "sessions" | "memory" | "storage") {
                        mask[i] = true;
                    }
                }
                let home = crate::os_open::aos_home();
                let dest = PathBuf::from(self.backup_ui.dest_parent.clone());
                match do_backup(&home, &dest, BACKUP_EXPORT_PREFIX, "export", &mask, false) {
                    Ok((dir, n, total)) => {
                        let msg = if fr {
                            format!(
                                "Exporté : {} ({} fichiers, {})",
                                dir.display(),
                                n,
                                human_bytes(total)
                            )
                        } else {
                            format!(
                                "Exported: {} ({n} files, {})",
                                dir.display(),
                                human_bytes(total)
                            )
                        };
                        self.backup_ui.last_result = msg.clone();
                        self.push_status(msg.clone());
                        self.toasts.push_success(msg);
                    }
                    Err(e) => {
                        self.backup_ui.last_result = e.clone();
                        self.toasts.push_error(e);
                    }
                }
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(if fr {
                "Restaurer depuis"
            } else {
                "Restore from"
            });
            ui.add(
                egui::TextEdit::singleline(&mut self.backup_ui.restore_dir)
                    .desired_width(280.0)
                    .hint_text("akasha-backup-…"),
            );
            if ui.small_button("…").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.backup_ui.restore_dir = dir.to_string_lossy().into_owned();
                }
            }
        });
        // Double-confirm : écrasement + relance requise.
        let armed_label = if fr {
            "Confirmer l'écrasement ?"
        } else {
            "Confirm overwrite?"
        };
        if crate::ui_primitives::danger_confirm_button(
            ui,
            "backup-restore",
            if fr { "Restaurer" } else { "Restore" },
            armed_label,
        ) {
            let home = crate::os_open::aos_home();
            let dir = PathBuf::from(self.backup_ui.restore_dir.clone());
            match do_restore(&home, &dir) {
                Ok(n) => {
                    let msg = if fr {
                        format!("Restauré ({n} fichiers vérifiés) — RELANCEZ Preview pour appliquer. Clés à ressaisir.")
                    } else {
                        format!("Restored ({n} verified files) — RELAUNCH Preview to apply. Re-enter keys.")
                    };
                    self.backup_ui.last_result = msg.clone();
                    self.push_status(msg.clone());
                    self.toasts.push_success(msg);
                }
                Err(e) => {
                    self.backup_ui.last_result = e.clone();
                    self.toasts.push_error(e);
                }
            }
        }
        if !self.backup_ui.last_result.is_empty() {
            ui.weak(&self.backup_ui.last_result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn tmp_home(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("aos-backup-test-{tag}-{nanos}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("var/sessions/s1")).expect("home");
        std::fs::write(dir.join("var/sessions/s1/chat.json"), r#"{"a":1}"#).expect("write");
        std::fs::create_dir_all(dir.join("var/memory")).expect("home");
        std::fs::write(dir.join("var/memory/facts.json"), "[]").expect("write");
        std::fs::create_dir_all(dir.join("var/secrets")).expect("home");
        std::fs::write(dir.join("var/secrets/keys.yaml"), "keys: {}").expect("write");
        dir
    }

    #[test]
    fn backup_then_verify_then_restore_roundtrip() {
        let home = tmp_home("roundtrip");
        let dest = home.join("out");
        let mask = vec![true; BACKUP_SCOPES.len()];
        let (dir, n, total) =
            do_backup(&home, &dest, BACKUP_DIR_PREFIX, "backup", &mask, false).expect("backup");
        assert!(n >= 2 && total > 0);
        let manifest = verify_backup(&dir).expect("verify");
        assert_eq!(manifest.version, BACKUP_MANIFEST_VERSION);
        // secrets jamais inclus.
        assert!(!manifest.files.iter().any(|f| f.rel.starts_with("secrets")));
        assert!(manifest.skipped.iter().any(|s| s.starts_with("secrets/")));
        // Corrompt une session puis restaure.
        std::fs::write(home.join("var/sessions/s1/chat.json"), "GARBAGE").expect("write");
        let restored = do_restore(&home, &dir).expect("restore");
        assert_eq!(restored, n);
        assert_eq!(
            std::fs::read_to_string(home.join("var/sessions/s1/chat.json")).expect("read"),
            r#"{"a":1}"#
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn verify_rejects_tampered_and_bad_version() {
        let home = tmp_home("tamper");
        let dest = home.join("out");
        let mask = vec![true; BACKUP_SCOPES.len()];
        let (dir, _, _) =
            do_backup(&home, &dest, BACKUP_DIR_PREFIX, "backup", &mask, false).expect("backup");
        std::fs::write(home.join("var/memory/facts.json"), "[tampered]").expect("write");
        // Le live est corrompu mais pas le backup : verify du backup OK.
        verify_backup(&dir).expect("backup intact");
        // Altère le backup lui-même.
        std::fs::write(dir.join("memory/facts.json"), "[x]").expect("write");
        assert!(verify_backup(&dir).is_err());
        // Mauvaise version.
        let raw = std::fs::read_to_string(dir.join("manifest.json")).expect("read");
        std::fs::write(
            dir.join("manifest.json"),
            raw.replace("\"version\":1", "\"version\":99"),
        )
        .expect("write");
        assert!(verify_backup(&dir).is_err());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn manifest_lists_expected_scopes() {
        let ids: HashSet<&str> = BACKUP_SCOPES.iter().map(|s| s.id).collect();
        for want in ["sessions", "memory", "storage", "schedules"] {
            assert!(ids.contains(want), "scope {want} manquant");
        }
        assert_eq!(BACKUP_SCOPES.len(), 9);
    }

    #[test]
    fn encrypted_roundtrip_needs_same_machine_key() {
        std::env::set_var("AOS_SECRETS_FILE_KEY", "1");
        let home = tmp_home("encloop");
        let dest = home.join("out");
        let mask = vec![true; BACKUP_SCOPES.len()];
        let (dir, n, _) =
            do_backup(&home, &dest, BACKUP_DIR_PREFIX, "backup", &mask, true).expect("backup");
        assert!(n >= 2);
        let manifest = verify_backup(&dir).expect("verify structure");
        assert!(manifest.encrypted);
        // Contenus illisibles au repos.
        let blob = std::fs::read(dir.join("sessions/s1/chat.json.enc")).expect("read");
        assert!(!String::from_utf8_lossy(&blob).contains("\"a\":1"));
        // Restore OK avec la clé locale.
        std::fs::write(home.join("var/sessions/s1/chat.json"), "GARBAGE").expect("write");
        assert_eq!(do_restore(&home, &dir).expect("restore"), n);
        assert_eq!(
            std::fs::read_to_string(home.join("var/sessions/s1/chat.json")).expect("read"),
            r#"{"a":1}"#
        );
        // Autre clé (autre machine/utilisateur) : échec propre.
        std::fs::remove_file(home.join("var/secrets/master.key")).expect("rm key");
        std::fs::write(home.join("var/secrets/master.key"), vec![7u8; 32]).expect("write");
        let err = do_restore(&home, &dir).expect_err("mauvaise clé");
        assert!(
            err.contains("déchiffrement"),
            "attendu déchiffrement, got: {err}"
        );
        std::env::remove_var("AOS_SECRETS_FILE_KEY");
        let _ = std::fs::remove_dir_all(&home);
    }
}
