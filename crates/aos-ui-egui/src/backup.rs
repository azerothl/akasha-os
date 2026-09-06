//! Sauvegarde / restauration locale (S1, phase 1).
//!
//! Copie + `manifest.json` (tailles + sha256). **NON chiffré v1** : `var/secrets`
//! est exclu (listé dans `skipped`, clés à ressaisir après restore) et le
//! dossier doit rester sur un support de confiance. Le chiffrement via le
//! vault (phase 2) demandera une API d'enveloppe côté `aos-platform`.
//! La restauration exige de **relancer Preview** (agents + stores en mémoire).

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
    BackupScope { id: "sessions", dir: "sessions", label_fr: "Sessions + canvas", label_en: "Sessions + canvas" },
    BackupScope { id: "memory", dir: "memory", label_fr: "Mémoire", label_en: "Memory" },
    BackupScope { id: "storage", dir: "storage", label_fr: "Documents", label_en: "Documents" },
    BackupScope { id: "modules", dir: "modules", label_fr: "Données modules (notes/tâches)", label_en: "Module data (notes/tasks)" },
    BackupScope { id: "schedules", dir: "schedules", label_fr: "Planifications", label_en: "Schedules" },
    BackupScope { id: "feedback", dir: "feedback", label_fr: "Retours", label_en: "Feedback" },
    BackupScope { id: "mcp", dir: "mcp", label_fr: "Config MCP", label_en: "MCP config" },
    BackupScope { id: "skills", dir: "skills", label_fr: "Skills perso", label_en: "Custom skills" },
    BackupScope { id: "audit", dir: "audit", label_fr: "Journal d'audit", label_en: "Audit journal" },
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
    ("secrets/", "clés chiffrées — à ressaisir après restore (non exportées par sécurité)"),
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
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).map_err(|e| format!("lecture {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn copy_dir_recursive(src: &Path, dst: &Path, out: &mut Vec<BackupFileEntry>, rel_base: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| format!("mkdir {}: {e}", dst.display()))?;
    let entries =
        std::fs::read_dir(src).map_err(|e| format!("list {}: {e}", src.display()))?;
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
            copy_dir_recursive(&src_path, &dst.join(&name), out, rel_base)?;
        } else if file_type.is_file() {
            let dst_path = dst.join(&name);
            std::fs::copy(&src_path, &dst_path)
                .map_err(|e| format!("copie {}: {e}", src_path.display()))?;
            let rel = dst_path
                .strip_prefix(rel_base)
                .map_err(|_| "chemin hors backup".to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let len = std::fs::metadata(&dst_path)
                .map(|m| m.len())
                .unwrap_or(0);
            let sha256 = sha256_file(&dst_path)?;
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
pub fn do_backup(
    home: &Path,
    dest_parent: &Path,
    prefix: &str,
    kind: &str,
    mask: &[bool],
) -> Result<(PathBuf, usize, u64), String> {
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
        copy_dir_recursive(&src, &dir.join(scope.dir), &mut files, &dir)?;
        scopes.push(scope.id.to_string());
    }
    for rel in BACKUP_CONFIG_FILES {
        let src = home.join("var").join(rel);
        if src.is_file() {
            let dst = dir.join(rel);
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
            }
            std::fs::copy(&src, &dst).map_err(|e| format!("copie {rel}: {e}"))?;
            let len = std::fs::metadata(&dst).map(|m| m.len()).unwrap_or(0);
            let sha256 = sha256_file(&dst)?;
            files.push(BackupFileEntry { rel: rel.replace('\\', "/"), len, sha256 });
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
        skipped: BACKUP_SKIPPED.iter().map(|(d, why)| format!("{d} — {why}")).collect(),
        files,
    };
    let raw =
        serde_json::to_string_pretty(&manifest).map_err(|e| format!("manifest: {e}"))?;
    std::fs::write(dir.join("manifest.json"), raw)
        .map_err(|e| format!("écriture manifest: {e}"))?;
    let total: u64 = manifest.files.iter().map(|f| f.len).sum();
    let n = manifest.files.len();
    Ok((dir, n, total))
}

/// Vérifie manifest + version + présence/taille/sha256 de chaque fichier.
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
        let meta =
            std::fs::metadata(&path).map_err(|_| format!("fichier manquant : {}", f.rel))?;
        if meta.len() != f.len {
            return Err(format!("taille différente : {} ({} ≠ {})", f.rel, meta.len(), f.len));
        }
        if sha256_file(&path)? != f.sha256 {
            return Err(format!("sha256 différent : {}", f.rel));
        }
    }
    Ok(manifest)
}

/// Restaure (vérifie d'abord) : remplace les scopes + configs, exige relance.
pub fn do_restore(home: &Path, dir: &Path) -> Result<usize, String> {
    let manifest = verify_backup(dir)?;
    let var = home.join("var");
    // Garde-fou : seuls les scopes + run/* connus sont restaurés.
    for f in &manifest.files {
        let first = f.rel.split('/').next().unwrap_or_default();
        let allowed = BACKUP_SCOPES.iter().any(|s| s.dir == first)
            || (first == "run" && BACKUP_CONFIG_FILES.contains(&f.rel.as_str()));
        if !allowed {
            return Err(format!("entrée hors scopes autorisés : {}", f.rel));
        }
    }
    // Remplace chaque scope d'un bloc (évite le mélange ancien/nouveau).
    for scope in BACKUP_SCOPES.iter().map(|s| s.dir).chain(std::iter::once("run")) {
        let src = dir.join(scope);
        if src.is_dir() {
            let dst = var.join(scope);
            if scope == "run" {
                // run/ : uniquement les 3 fichiers de config, jamais tout le dossier.
                for rel in BACKUP_CONFIG_FILES {
                    let s = dir.join(rel);
                    if s.is_file() {
                        let d = var.join(rel);
                        if let Some(parent) = d.parent() {
                            std::fs::create_dir_all(parent)
                                .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
                        }
                        std::fs::copy(&s, &d)
                            .map_err(|e| format!("restore {rel}: {e}"))?;
                    }
                }
            } else {
                if dst.exists() {
                    std::fs::remove_dir_all(&dst)
                        .map_err(|e| format!("nettoyage {}: {e}", dst.display()))?;
                }
                copy_dir_recursive(&src, &dst, &mut Vec::new(), &src)?;
            }
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
            "Copie + manifest (tailles + sha256) vers un dossier. NON chiffré v1 : gardez le support pour vous. Clés (var/secrets) exclues — à ressaisir après restauration. Relancez Preview après une restauration."
        } else {
            "Copy + manifest (sizes + sha256) to a folder. NOT encrypted v1: keep the media to yourself. Keys (var/secrets) excluded — re-enter after restore. Relaunch Preview after restoring."
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(if fr { "Dossier parent" } else { "Parent folder" });
            ui.add(
                egui::TextEdit::singleline(&mut self.backup_ui.dest_parent)
                    .desired_width(280.0),
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
            if ui.button(if fr { "Sauvegarder" } else { "Back up" }).clicked() {
                let home = crate::os_open::aos_home();
                let dest = PathBuf::from(self.backup_ui.dest_parent.clone());
                match do_backup(&home, &dest, BACKUP_DIR_PREFIX, "backup", &self.backup_ui.scopes.clone()) {
                    Ok((dir, n, total)) => {
                        let msg = if fr {
                            format!("Sauvegardé : {} ({} fichiers, {})", dir.display(), n, human_bytes(total))
                        } else {
                            format!("Backed up: {} ({n} files, {})", dir.display(), human_bytes(total))
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
            if ui.button(if fr { "Exporter JSON" } else { "Export JSON" }).clicked() {
                // Export lisible : conversations + mémoire + documents.
                let mut mask = vec![false; BACKUP_SCOPES.len()];
                for (i, s) in BACKUP_SCOPES.iter().enumerate() {
                    if matches!(s.id, "sessions" | "memory" | "storage") {
                        mask[i] = true;
                    }
                }
                let home = crate::os_open::aos_home();
                let dest = PathBuf::from(self.backup_ui.dest_parent.clone());
                match do_backup(&home, &dest, BACKUP_EXPORT_PREFIX, "export", &mask) {
                    Ok((dir, n, total)) => {
                        let msg = if fr {
                            format!("Exporté : {} ({} fichiers, {})", dir.display(), n, human_bytes(total))
                        } else {
                            format!("Exported: {} ({n} files, {})", dir.display(), human_bytes(total))
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
            ui.label(if fr { "Restaurer depuis" } else { "Restore from" });
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
        let armed_label = if fr { "Confirmer l'écrasement ?" } else { "Confirm overwrite?" };
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
        let (dir, n, total) = do_backup(&home, &dest, BACKUP_DIR_PREFIX, "backup", &mask)
            .expect("backup");
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
        let (dir, _, _) = do_backup(&home, &dest, BACKUP_DIR_PREFIX, "backup", &mask)
            .expect("backup");
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
}
