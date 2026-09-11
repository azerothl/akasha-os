//! Lot 4 (#149): migrate historical Tasks installs and manage preinstall without boot resync.
//!
//! Replaces the unconditional `sync_packaged_module` + registry cap append in `main.rs`.
//! - Historical installs keep granted caps; none are added silently.
//! - User uninstall (`user_removed`) persists across boots.
//! - Bundled host package never downgrades a newer standalone install.
//! - Registry and installed package are backed up before a package switch.

use crate::bootstrap;
use aos_proto::tasks_contract::{MANIFEST_FS_CAPS, MODULE_NAME};
use aos_proto::decl_ui;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const MIGRATION_MARKER: &str = "var/modules/.migrations/tasks-managed-app-v1.yaml";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct MigrationMarker {
    version: u32,
    applied_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct ModuleRegistry {
    #[serde(default)]
    installed: Vec<InstalledEntry>,
    #[serde(default)]
    removed: Vec<RemovedEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct InstalledEntry {
    name: String,
    granted_caps: Vec<String>,
    #[serde(default)]
    quarantined: bool,
    #[serde(default)]
    preinstalled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct RemovedEntry {
    name: String,
    user_removed: bool,
    #[serde(default)]
    preinstalled: bool,
    #[serde(default)]
    granted_caps: Vec<String>,
}

/// Manage the Tasks module at boot. Returns `true` when the installed package changed
/// (caller should `module.reload`).
pub fn manage_tasks_module(home: &Path) -> bool {
    match manage_tasks_module_inner(home) {
        Ok(synced) => synced,
        Err(e) => {
            eprintln!("[aos-session] tasks migration: {e}");
            false
        }
    }
}

fn manage_tasks_module_inner(home: &Path) -> Result<bool, String> {
    let profile = decl_ui::resolve_preview_profile(home);
    let registry_path = modules_root(home).join("registry.yaml");
    let installed_dir = modules_root(home).join(MODULE_NAME);
    let mut registry = load_registry(&registry_path);

    if registry.user_removed(MODULE_NAME) {
        return Ok(false);
    }

    let had_historical = is_historical_tasks_install(home, &registry);
    let preinstall = decl_ui::is_preinstalled_for_profile(MODULE_NAME, profile);
    if !preinstall && !had_historical && !installed_dir.is_dir() {
        return Ok(false);
    }

    let share = match resolve_tasks_share_pkg(home) {
        Ok(p) => p,
        Err(e) => {
            if had_historical || installed_dir.is_dir() {
                ensure_managed_registry_entry(&installed_dir, &mut registry);
                if registry_dirty(&registry_path, &registry) {
                    backup_registry(&registry_path);
                    save_registry(&registry_path, &registry)?;
                }
                return Ok(false);
            }
            if !preinstall {
                return Ok(false);
            }
            return Err(e);
        }
    };
    if !migration_marker_exists(home) && had_historical {
        migrate_historical_install(home, &registry_path, &installed_dir, &mut registry)?;
        write_migration_marker(home)?;
    }

    let package_changed = sync_tasks_package_if_needed(home, &share, &installed_dir, &registry)?;
    ensure_managed_registry_entry(&installed_dir, &mut registry);
    if package_changed && registry.granted_caps(MODULE_NAME).is_empty() && !had_historical {
        let caps: Vec<String> = MANIFEST_FS_CAPS.iter().map(|c| c.to_string()).collect();
        registry.ensure_installed(MODULE_NAME, caps, true);
    }

    if registry_dirty(&registry_path, &registry) {
        backup_registry(&registry_path);
        save_registry(&registry_path, &registry)?;
    }

    Ok(package_changed)
}

fn modules_root(home: &Path) -> PathBuf {
    home.join("var/modules")
}

/// Resolve bundled Tasks package (Preview share path, dev repo fallback).
fn resolve_tasks_share_pkg(home: &Path) -> Result<PathBuf, String> {
    let candidates = [
        home.join("share/modules/tasks.aospkg"),
        home.join("modules/tasks.aospkg"),
        PathBuf::from("share/modules/tasks.aospkg"),
    ];
    for cand in candidates {
        if cand.join("module.wasm").is_file() {
            return Ok(cand);
        }
    }
    Err("tasks.aospkg introuvable".into())
}

fn load_registry(path: &Path) -> ModuleRegistry {
    if !path.is_file() {
        return ModuleRegistry::default();
    }
    let raw = fs::read_to_string(path).unwrap_or_default();
    serde_yaml::from_str(&raw).unwrap_or_default()
}

fn save_registry(path: &Path, registry: &ModuleRegistry) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("yaml.tmp");
    let body = serde_yaml::to_string(registry).map_err(|e| e.to_string())?;
    fs::write(&tmp, body).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}

impl ModuleRegistry {
    fn user_removed(&self, name: &str) -> bool {
        self.removed
            .iter()
            .any(|e| e.name == name && e.user_removed)
    }

    fn installed_entry(&self, name: &str) -> Option<&InstalledEntry> {
        self.installed.iter().find(|e| e.name == name)
    }

    fn granted_caps(&self, name: &str) -> Vec<String> {
        self.installed_entry(name)
            .map(|e| e.granted_caps.clone())
            .or_else(|| {
                self.removed
                    .iter()
                    .find(|e| e.name == name)
                    .map(|e| e.granted_caps.clone())
            })
            .unwrap_or_default()
    }

    fn ensure_installed(&mut self, name: &str, granted_caps: Vec<String>, preinstalled: bool) {
        let quarantined = granted_caps.is_empty() && !MANIFEST_FS_CAPS.is_empty();
        if let Some(entry) = self.installed.iter_mut().find(|e| e.name == name) {
            entry.granted_caps = granted_caps;
            entry.quarantined = quarantined;
            entry.preinstalled = preinstalled;
            return;
        }
        self.installed.push(InstalledEntry {
            name: name.to_string(),
            granted_caps,
            quarantined,
            preinstalled,
        });
        self.installed.sort_by(|a, b| a.name.cmp(&b.name));
    }
}

fn is_historical_tasks_install(home: &Path, registry: &ModuleRegistry) -> bool {
    let installed_dir = modules_root(home).join(MODULE_NAME);
    if installed_dir.join("module.wasm").is_file() {
        return true;
    }
    if registry.installed_entry(MODULE_NAME).is_some() {
        return true;
    }
    registry.removed.iter().any(|e| e.name == MODULE_NAME)
}

fn migration_marker_exists(home: &Path) -> bool {
    home.join(MIGRATION_MARKER).is_file()
}

fn write_migration_marker(home: &Path) -> Result<(), String> {
    let path = home.join(MIGRATION_MARKER);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let marker = MigrationMarker {
        version: 1,
        applied_ms: now_ms(),
    };
    let body = serde_yaml::to_string(&marker).map_err(|e| e.to_string())?;
    fs::write(&path, body).map_err(|e| e.to_string())?;
    Ok(())
}

fn migrate_historical_install(
    home: &Path,
    registry_path: &Path,
    installed_dir: &Path,
    registry: &mut ModuleRegistry,
) -> Result<(), String> {
    eprintln!("[aos-session] migration Tasks (#149 lot 4) — enregistrement application gérée");
    backup_registry(registry_path);
    if installed_dir.join("module.wasm").is_file() {
        backup_installed_package(home, installed_dir);
    }
    let caps = registry.granted_caps(MODULE_NAME);
    registry.ensure_installed(MODULE_NAME, caps, true);
    Ok(())
}

fn backup_registry(registry_path: &Path) {
    if !registry_path.is_file() {
        return;
    }
    let backup = registry_path.with_extension("yaml.pre-tasks-migration");
    if backup.exists() {
        return;
    }
    let _ = fs::copy(registry_path, &backup);
}

fn backup_installed_package(home: &Path, installed_dir: &Path) {
    let previous_root = modules_root(home).join(".previous");
    let backup = previous_root.join(MODULE_NAME);
    if backup.join("module.wasm").is_file() {
        return;
    }
    let _ = fs::create_dir_all(&previous_root);
    if installed_dir.is_dir() {
        let _ = fs::remove_dir_all(&backup);
        let _ = bootstrap::copy_dir_recursive(installed_dir, &backup);
    }
}

fn sync_tasks_package_if_needed(
    home: &Path,
    share: &Path,
    installed_dir: &Path,
    _registry: &ModuleRegistry,
) -> Result<bool, String> {
    if !bootstrap::should_upgrade_packaged_module(share, installed_dir) {
        return Ok(false);
    }
    let need_install = !installed_dir.join("module.wasm").is_file();
    backup_installed_package(home, installed_dir);
    let synced = bootstrap::sync_packaged_module(share, installed_dir);
    if synced {
        eprintln!(
            "[aos-session] Tasks {} depuis {}",
            if need_install {
                "préinstallé"
            } else {
                "mis à jour"
            },
            share.display()
        );
    }
    Ok(synced)
}

fn ensure_managed_registry_entry(installed_dir: &Path, registry: &mut ModuleRegistry) {
    if !installed_dir.join("module.wasm").is_file() {
        return;
    }
    let caps = registry.granted_caps(MODULE_NAME);
    if registry.installed_entry(MODULE_NAME).is_some() {
        if let Some(entry) = registry
            .installed
            .iter_mut()
            .find(|e| e.name == MODULE_NAME)
        {
            entry.preinstalled = true;
        }
        return;
    }
    registry.ensure_installed(MODULE_NAME, caps, true);
}

fn registry_dirty(path: &Path, registry: &ModuleRegistry) -> bool {
    let current = load_registry(path);
    current.installed != registry.installed || current.removed != registry.removed
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_home(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "aos-tasks-mig-{}-{}-{}",
            tag,
            std::process::id(),
            nanos
        ))
    }

    fn write_share_pkg(home: &Path, version: &str, wasm: &[u8]) {
        let share = home.join("share/modules/tasks.aospkg");
        fs::create_dir_all(share.join("ui")).unwrap();
        use sha2::{Digest, Sha256};
        let hash = format!("{:x}", Sha256::digest(wasm));
        let manifest = format!(
            "name: tasks\nversion: {version}\nhash: {hash}\npermissions:\n  required_caps:\n    - fs.read:/documents/tasks/**\n    - fs.write:/documents/tasks/**\ntools:\n  - name: tasks.list\n    description: List\n    input_schema:\n      type: object\nui:\n  entry: ui/index.html\n  mode: declarative_ui\nmin_os_api: 1\n"
        );
        fs::write(share.join("manifest.yaml"), manifest).unwrap();
        fs::write(share.join("module.wasm"), wasm).unwrap();
        fs::write(share.join("ui/index.html"), "{}").unwrap();
    }

    fn write_installed_pkg(home: &Path, version: &str, wasm: &[u8]) {
        let dir = modules_root(home).join(MODULE_NAME);
        fs::create_dir_all(&dir).unwrap();
        use sha2::{Digest, Sha256};
        let hash = format!("{:x}", Sha256::digest(wasm));
        let manifest = format!(
            "name: tasks\nversion: {version}\nhash: {hash}\npermissions:\n  required_caps:\n    - fs.read:/documents/tasks/**\n    - fs.write:/documents/tasks/**\ntools: []\nmin_os_api: 1\n"
        );
        fs::write(dir.join("manifest.yaml"), manifest).unwrap();
        fs::write(dir.join("module.wasm"), wasm).unwrap();
    }

    fn write_registry(home: &Path, body: &str) {
        let path = modules_root(home).join("registry.yaml");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, body).unwrap();
    }

    fn write_migration_marker_for_test(home: &Path) {
        let path = home.join(MIGRATION_MARKER);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "version: 1\napplied_ms: 1\n").unwrap();
    }

    #[test]
    fn historical_install_preserves_granted_caps_without_adding() {
        let home = temp_home("preserve-caps");
        write_share_pkg(&home, "1.0.0", b"share wasm v1");
        write_installed_pkg(&home, "0.9.0", b"old wasm");
        write_registry(
            &home,
            "installed:\n  - name: tasks\n    granted_caps:\n      - fs.read:/documents/tasks/**\n    quarantined: false\n",
        );

        assert!(manage_tasks_module(&home));
        let reg = fs::read_to_string(modules_root(&home).join("registry.yaml")).unwrap();
        assert!(reg.contains("preinstalled: true"));
        assert!(reg.contains("fs.read:/documents/tasks/**"));
        assert!(!reg.contains("fs.write:/documents/tasks/**"));
        assert!(home.join(MIGRATION_MARKER).is_file());
        assert!(modules_root(&home)
            .join("registry.yaml.pre-tasks-migration")
            .is_file());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn user_removed_skips_sync_and_second_boot_is_noop() {
        let home = temp_home("user-removed");
        write_share_pkg(&home, "2.0.0", b"share wasm v2");
        write_registry(
            &home,
            "installed: []\nremoved:\n  - name: tasks\n    user_removed: true\n    preinstalled: true\n    granted_caps:\n      - fs.read:/documents/tasks/**\n      - fs.write:/documents/tasks/**\n",
        );

        assert!(!manage_tasks_module(&home));
        assert!(!modules_root(&home).join(MODULE_NAME).exists());
        let reg1 = fs::read_to_string(modules_root(&home).join("registry.yaml")).unwrap();
        assert!(!manage_tasks_module(&home));
        let reg2 = fs::read_to_string(modules_root(&home).join("registry.yaml")).unwrap();
        assert_eq!(reg1, reg2);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn does_not_downgrade_newer_standalone_package() {
        let home = temp_home("no-downgrade");
        write_share_pkg(&home, "1.0.0", b"bundled wasm");
        write_installed_pkg(&home, "2.0.0", b"standalone wasm v2");
        write_registry(
            &home,
            "installed:\n  - name: tasks\n    granted_caps:\n      - fs.read:/documents/tasks/**\n      - fs.write:/documents/tasks/**\n    quarantined: false\n    preinstalled: true\n",
        );
        write_migration_marker_for_test(&home);

        assert!(!manage_tasks_module(&home));
        assert_eq!(
            fs::read(modules_root(&home).join(MODULE_NAME).join("module.wasm")).unwrap(),
            b"standalone wasm v2"
        );
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn upgrades_when_ui_differs_at_same_version() {
        let home = temp_home("ui-upgrade");
        let share = home.join("share/modules/tasks.aospkg");
        fs::create_dir_all(share.join("ui")).unwrap();
        use sha2::{Digest, Sha256};
        let wasm = b"tasks wasm v1";
        let hash = format!("{:x}", Sha256::digest(wasm));
        let manifest = format!(
            "name: tasks\nversion: 1.0.0\nhash: {hash}\npermissions:\n  required_caps:\n    - fs.read:/documents/tasks/**\n    - fs.write:/documents/tasks/**\ntools:\n  - name: tasks.list\n    description: List\n    input_schema:\n      type: object\nui:\n  entry: ui/index.html\n  mode: declarative_ui\nmin_os_api: 1\n"
        );
        fs::write(share.join("manifest.yaml"), manifest).unwrap();
        fs::write(share.join("module.wasm"), wasm).unwrap();
        fs::write(
            share.join("ui/index.html"),
            br#"{"type":"declarative_ui","title":"Tasks","root":{"kind":"column","children":[]}}"#,
        )
        .unwrap();

        write_installed_pkg(&home, "1.0.0", wasm);
        let ui_dir = modules_root(&home).join(MODULE_NAME).join("ui");
        fs::create_dir_all(&ui_dir).unwrap();
        fs::write(
            ui_dir.join("index.html"),
            br#"{"type":"declarative_ui","title":"Tasks","commands":["tasks.list"]}"#,
        )
        .unwrap();
        write_registry(
            &home,
            "installed:\n  - name: tasks\n    granted_caps:\n      - fs.read:/documents/tasks/**\n      - fs.write:/documents/tasks/**\n    quarantined: false\n",
        );
        write_migration_marker_for_test(&home);

        assert!(manage_tasks_module(&home));
        let installed_ui = fs::read_to_string(ui_dir.join("index.html"))
        .unwrap();
        assert!(installed_ui.contains("\"root\""));
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn upgrades_when_bundled_is_newer_with_backup() {
        let home = temp_home("upgrade");
        write_share_pkg(&home, "2.0.0", b"bundled wasm v2");
        write_installed_pkg(&home, "1.0.0", b"old wasm v1");
        write_registry(
            &home,
            "installed:\n  - name: tasks\n    granted_caps:\n      - fs.read:/documents/tasks/**\n      - fs.write:/documents/tasks/**\n    quarantined: false\n",
        );
        write_migration_marker_for_test(&home);

        assert!(manage_tasks_module(&home));
        assert_eq!(
            fs::read(modules_root(&home).join(MODULE_NAME).join("module.wasm")).unwrap(),
            b"bundled wasm v2"
        );
        assert!(modules_root(&home)
            .join(".previous")
            .join(MODULE_NAME)
            .join("module.wasm")
            .is_file());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn migration_is_idempotent_on_second_boot() {
        let home = temp_home("idempotent");
        write_share_pkg(&home, "1.0.0", b"wasm");
        write_installed_pkg(&home, "1.0.0", b"wasm");
        write_registry(
            &home,
            "installed:\n  - name: tasks\n    granted_caps:\n      - fs.read:/documents/tasks/**\n      - fs.write:/documents/tasks/**\n    quarantined: false\n",
        );

        assert!(!manage_tasks_module(&home));
        let reg1 = fs::read_to_string(modules_root(&home).join("registry.yaml")).unwrap();
        assert!(!manage_tasks_module(&home));
        let reg2 = fs::read_to_string(modules_root(&home).join("registry.yaml")).unwrap();
        assert_eq!(reg1, reg2);
        assert!(home.join(MIGRATION_MARKER).is_file());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn minimal_profile_skips_fresh_preinstall() {
        let home = temp_home("minimal-profile");
        write_share_pkg(&home, "1.0.0", b"fresh wasm");
        fs::write(
            home.join("share/preview-profile.yaml"),
            "profile: minimal\n",
        )
        .unwrap();
        assert!(!manage_tasks_module(&home));
        assert!(!modules_root(&home).join(MODULE_NAME).exists());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn fresh_preinstall_grants_manifest_caps() {
        let home = temp_home("fresh");
        write_share_pkg(&home, "1.0.0", b"fresh wasm");
        assert!(manage_tasks_module(&home));
        let reg = fs::read_to_string(modules_root(&home).join("registry.yaml")).unwrap();
        assert!(reg.contains("preinstalled: true"));
        for cap in MANIFEST_FS_CAPS {
            assert!(reg.contains(cap));
        }
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn resolves_dev_repo_package_path() {
        let home = temp_home("dev-path");
        let share = home.join("modules/tasks.aospkg");
        fs::create_dir_all(&share).unwrap();
        fs::write(share.join("module.wasm"), b"x").unwrap();
        fs::write(
            share.join("manifest.yaml"),
            "name: tasks\nversion: 1\nhash: ab\npermissions:\n  required_caps: []\nmin_os_api: 1\n",
        )
        .unwrap();
        let resolved = resolve_tasks_share_pkg(&home).unwrap();
        assert_eq!(resolved, share);
        let _ = fs::remove_dir_all(&home);
    }
}
