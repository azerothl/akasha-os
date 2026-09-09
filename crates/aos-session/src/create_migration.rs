//! Lot 3 (#150): manage optional Create installs without boot resync or forced reinstall.
//!
//! Mirrors `tasks_migration` for upgrades and `user_removed`, but Create is **not**
//! preinstalled — only already-installed packages are synced at boot.
//! Idempotently imports legacy `/downloads/*.meta.json` into `/documents/create/history.json`.

use crate::bootstrap;
use crate::update;
use aos_proto::create_contract::{HISTORY_PATH, INVOKE_CAP, MANIFEST_FS_CAPS, MODULE_NAME};
use aos_proto::{MEDIA_GENERATE_CAP, ModuleManifest};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const MIGRATION_MARKER: &str = "var/modules/.migrations/create-managed-app-v1.yaml";
const LEGACY_HISTORY_MARKER: &str = "var/modules/.migrations/create-legacy-history-v1.yaml";

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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyImageMeta {
    pub version: u32,
    pub created_unix: u64,
    pub path: String,
    #[serde(default)]
    pub model_id: String,
    #[serde(default)]
    pub engine: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct HistoryStore {
    #[serde(default)]
    items: Vec<HistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HistoryEntry {
    id: String,
    created_unix: u64,
    when: String,
    path: String,
    prompt: String,
    #[serde(default)]
    model_id: String,
    #[serde(default)]
    engine: String,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    steps: Option<u32>,
}

/// Manage the Create module at boot. Returns `true` when the installed package changed
/// (caller should `module.reload`).
pub fn manage_create_module(home: &Path) -> bool {
    match manage_create_module_inner(home) {
        Ok(synced) => synced,
        Err(e) => {
            eprintln!("[aos-session] create migration: {e}");
            false
        }
    }
}

fn manage_create_module_inner(home: &Path) -> Result<bool, String> {
    let registry_path = modules_root(home).join("registry.yaml");
    let installed_dir = modules_root(home).join(MODULE_NAME);
    let mut registry = load_registry(&registry_path);

    if registry.user_removed(MODULE_NAME) {
        return Ok(false);
    }

    let had_historical = is_historical_create_install(home, &registry);
    if !had_historical && !installed_dir.is_dir() {
        return Ok(false);
    }

    let share = match resolve_create_share_pkg(home) {
        Ok(p) => p,
        Err(e) => {
            if had_historical || installed_dir.is_dir() {
                ensure_managed_registry_entry(&installed_dir, &mut registry);
                if registry_dirty(&registry_path, &registry) {
                    backup_registry(&registry_path);
                    save_registry(&registry_path, &registry)?;
                }
                import_legacy_history_if_needed(home)?;
                return Ok(false);
            }
            return Err(e);
        }
    };

    if !migration_marker_exists(home) && had_historical {
        migrate_historical_install(home, &registry_path, &installed_dir, &mut registry)?;
        write_migration_marker(home)?;
    }

    let package_changed = sync_create_package_if_needed(home, &share, &installed_dir, &registry)?;
    ensure_managed_registry_entry(&installed_dir, &mut registry);
    if package_changed && registry.granted_caps(MODULE_NAME).is_empty() && !had_historical {
        let caps: Vec<String> = full_manifest_caps();
        registry.ensure_installed(MODULE_NAME, caps, false);
    }

    if registry_dirty(&registry_path, &registry) {
        backup_registry(&registry_path);
        save_registry(&registry_path, &registry)?;
    }

    if installed_dir.join("module.wasm").is_file() {
        import_legacy_history_if_needed(home)?;
    }

    Ok(package_changed)
}

fn full_manifest_caps() -> Vec<String> {
    let mut caps: Vec<String> = MANIFEST_FS_CAPS.iter().map(|c| c.to_string()).collect();
    caps.push(MEDIA_GENERATE_CAP.to_string());
    caps.push(INVOKE_CAP.to_string());
    caps
}

fn modules_root(home: &Path) -> PathBuf {
    home.join("var/modules")
}

fn documents_host_root(home: &Path) -> PathBuf {
    home.join("var/storage/data/documents/create")
}

fn downloads_host_dir(home: &Path) -> PathBuf {
    home.join("var/storage/data/downloads")
}

fn history_host_path(home: &Path) -> PathBuf {
    documents_host_root(home).join("history.json")
}

/// Resolve bundled Create package (Preview share path, dev repo fallback).
fn resolve_create_share_pkg(home: &Path) -> Result<PathBuf, String> {
    let candidates = [
        home.join("share/modules/create.aospkg"),
        home.join("modules/create.aospkg"),
        PathBuf::from("share/modules/create.aospkg"),
    ];
    for cand in candidates {
        if cand.join("module.wasm").is_file() {
            return Ok(cand);
        }
    }
    Err("create.aospkg introuvable".into())
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
        let quarantined = granted_caps.is_empty() && !full_manifest_caps().is_empty();
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

fn is_historical_create_install(home: &Path, registry: &ModuleRegistry) -> bool {
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

fn legacy_history_marker_exists(home: &Path) -> bool {
    home.join(LEGACY_HISTORY_MARKER).is_file()
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

fn write_legacy_history_marker(home: &Path, imported: usize) -> Result<(), String> {
    let path = home.join(LEGACY_HISTORY_MARKER);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let body = format!("version: 1\nimported: {imported}\napplied_ms: {}\n", now_ms());
    fs::write(&path, body).map_err(|e| e.to_string())?;
    Ok(())
}

fn migrate_historical_install(
    home: &Path,
    registry_path: &Path,
    installed_dir: &Path,
    registry: &mut ModuleRegistry,
) -> Result<(), String> {
    eprintln!("[aos-session] migration Create (#150 lot 3) — enregistrement application gérée");
    backup_registry(registry_path);
    if installed_dir.join("module.wasm").is_file() {
        backup_installed_package(home, installed_dir);
    }
    let caps = registry.granted_caps(MODULE_NAME);
    registry.ensure_installed(MODULE_NAME, caps, false);
    Ok(())
}

fn backup_registry(registry_path: &Path) {
    if !registry_path.is_file() {
        return;
    }
    let backup = registry_path.with_extension("yaml.pre-create-migration");
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

fn read_manifest_version(dir: &Path) -> Option<String> {
    let raw = fs::read_to_string(dir.join("manifest.yaml")).ok()?;
    serde_yaml::from_str::<ModuleManifest>(&raw)
        .ok()
        .map(|m| m.version)
}

fn bundled_is_newer_than_installed(share: &Path, installed_dir: &Path) -> bool {
    let bundled = read_manifest_version(share).unwrap_or_default();
    let installed = read_manifest_version(installed_dir).unwrap_or_default();
    if installed.is_empty() {
        return true;
    }
    update::is_newer(&installed, &bundled)
}

fn sync_create_package_if_needed(
    home: &Path,
    share: &Path,
    installed_dir: &Path,
    _registry: &ModuleRegistry,
) -> Result<bool, String> {
    let wasm_present = installed_dir.join("module.wasm").is_file();
    let need_install = !wasm_present;
    let need_upgrade = wasm_present && bundled_is_newer_than_installed(share, installed_dir);
    if !need_install && !need_upgrade {
        return Ok(false);
    }
    backup_installed_package(home, installed_dir);
    let synced = bootstrap::sync_packaged_module(share, installed_dir);
    if synced {
        eprintln!(
            "[aos-session] Create {} depuis {}",
            if need_install {
                "installé"
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
        return;
    }
    registry.ensure_installed(MODULE_NAME, caps, false);
}

fn registry_dirty(path: &Path, registry: &ModuleRegistry) -> bool {
    let current = load_registry(path);
    current.installed != registry.installed || current.removed != registry.removed
}

fn utc_date_time_label(secs: u64) -> String {
    if secs == 0 {
        return "—".into();
    }
    let z = secs / 86_400;
    let time = secs % 86_400;
    let h = time / 3600;
    let m = (time % 3600) / 60;
    let (y, mo, d) = days_to_ymd(z);
    format!("{:04}-{:02}-{:02} {:02}:{:02}", y, mo, d, h, m)
}

fn days_to_ymd(z: u64) -> (u64, u64, u64) {
    let z = z + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mp < 10 { y } else { y + 1 };
    (y, m, d)
}

fn load_history_store(path: &Path) -> HistoryStore {
    if !path.is_file() {
        return HistoryStore::default();
    }
    let raw = fs::read_to_string(path).unwrap_or_default();
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save_history_store(path: &Path, store: &HistoryStore) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let raw = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    fs::write(path, raw).map_err(|e| e.to_string())?;
    Ok(())
}

/// Idempotent import of native Image Studio sidecars into Create history.
pub fn import_legacy_history_if_needed(home: &Path) -> Result<usize, String> {
    if legacy_history_marker_exists(home) {
        return Ok(0);
    }
    let downloads = downloads_host_dir(home);
    if !downloads.is_dir() {
        write_legacy_history_marker(home, 0)?;
        return Ok(0);
    }

    let mut metas = Vec::new();
    for entry in fs::read_dir(&downloads).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.ends_with(".meta.json") {
            continue;
        }
        let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if let Ok(meta) = serde_json::from_str::<LegacyImageMeta>(&raw) {
            if !meta.path.is_empty() && !meta.prompt.is_empty() {
                metas.push(meta);
            }
        }
    }
    metas.sort_by(|a, b| b.created_unix.cmp(&a.created_unix));

    let history_path = history_host_path(home);
    let mut store = load_history_store(&history_path);
    let existing_paths: HashSet<String> = store.items.iter().map(|e| e.path.clone()).collect();
    let mut imported = 0usize;
    let mut next_id = store
        .items
        .iter()
        .map(|e| e.created_unix)
        .max()
        .unwrap_or(0)
        .saturating_add(1);

    for meta in metas {
        if existing_paths.contains(&meta.path) {
            continue;
        }
        let entry = HistoryEntry {
            id: format!("hist-{next_id}"),
            created_unix: meta.created_unix.max(next_id),
            when: utc_date_time_label(meta.created_unix),
            path: meta.path,
            prompt: meta.prompt,
            model_id: meta.model_id,
            engine: meta.engine,
            width: None,
            height: None,
            steps: None,
        };
        next_id = entry.created_unix.saturating_add(1);
        store.items.push(entry);
        imported += 1;
    }
    store.items.sort_by(|a, b| b.created_unix.cmp(&a.created_unix));
    if store.items.len() > 40 {
        store.items.truncate(40);
    }

    if imported > 0 {
        save_history_store(&history_path, &store)?;
        eprintln!(
            "[aos-session] Create — import historique natif: {imported} entrée(s) → {}",
            HISTORY_PATH
        );
    }
    write_legacy_history_marker(home, imported)?;
    Ok(imported)
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
            "aos-create-mig-{}-{}-{}",
            tag,
            std::process::id(),
            nanos
        ))
    }

    fn write_share_pkg(home: &Path, version: &str, wasm: &[u8]) {
        let share = home.join("share/modules/create.aospkg");
        fs::create_dir_all(share.join("ui")).unwrap();
        use sha2::{Digest, Sha256};
        let hash = format!("{:x}", Sha256::digest(wasm));
        let manifest = format!(
            "name: create\nversion: {version}\nhash: {hash}\npermissions:\n  required_caps:\n    - fs.read:/documents/create/**\n    - fs.write:/documents/create/**\n    - fs.read:/downloads/**\n    - fs.write:/downloads/**\n    - media.generate\n    - tool.invoke:create\nservices:\n  jobs: 1\n  media_image: 1\ntools:\n  - name: create.history.list\n    description: List\n    input_schema:\n      type: object\nui:\n  contract: 2\n  document: ui/index.json\n  entry: ui/index.json\n  mode: declarative_ui\nmin_os_api: 1\n"
        );
        fs::write(share.join("manifest.yaml"), manifest).unwrap();
        fs::write(share.join("module.wasm"), wasm).unwrap();
        fs::write(share.join("ui/index.json"), r#"{"type":"declarative_ui","contract":2,"title":"Create","root":{"kind":"column","children":[]}}"#).unwrap();
    }

    fn write_installed_pkg(home: &Path, version: &str, wasm: &[u8]) {
        let dir = modules_root(home).join(MODULE_NAME);
        fs::create_dir_all(&dir).unwrap();
        use sha2::{Digest, Sha256};
        let hash = format!("{:x}", Sha256::digest(wasm));
        let manifest = format!(
            "name: create\nversion: {version}\nhash: {hash}\npermissions:\n  required_caps:\n    - fs.read:/documents/create/**\n    - fs.write:/documents/create/**\n    - fs.read:/downloads/**\n    - fs.write:/downloads/**\n    - media.generate\n    - tool.invoke:create\nservices:\n  jobs: 1\n  media_image: 1\ntools: []\nmin_os_api: 1\n"
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
    fn user_removed_skips_sync_and_second_boot_is_noop() {
        let home = temp_home("user-removed");
        write_share_pkg(&home, "2.0.0", b"share wasm v2");
        write_registry(
            &home,
            "installed: []\nremoved:\n  - name: create\n    user_removed: true\n    preinstalled: false\n    granted_caps:\n      - fs.read:/documents/create/**\n      - fs.write:/documents/create/**\n",
        );

        assert!(!manage_create_module(&home));
        assert!(!modules_root(&home).join(MODULE_NAME).exists());
        let reg1 = fs::read_to_string(modules_root(&home).join("registry.yaml")).unwrap();
        assert!(!manage_create_module(&home));
        let reg2 = fs::read_to_string(modules_root(&home).join("registry.yaml")).unwrap();
        assert_eq!(reg1, reg2);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn fresh_boot_does_not_preinstall_create() {
        let home = temp_home("no-preinstall");
        write_share_pkg(&home, "1.0.0", b"fresh wasm");
        assert!(!manage_create_module(&home));
        assert!(!modules_root(&home).join(MODULE_NAME).exists());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn does_not_downgrade_newer_standalone_package() {
        let home = temp_home("no-downgrade");
        write_share_pkg(&home, "1.0.0", b"bundled wasm");
        write_installed_pkg(&home, "2.0.0", b"standalone wasm v2");
        write_registry(
            &home,
            "installed:\n  - name: create\n    granted_caps:\n      - fs.read:/documents/create/**\n      - fs.write:/documents/create/**\n    quarantined: false\n    preinstalled: false\n",
        );
        write_migration_marker_for_test(&home);

        assert!(!manage_create_module(&home));
        assert_eq!(
            fs::read(modules_root(&home).join(MODULE_NAME).join("module.wasm")).unwrap(),
            b"standalone wasm v2"
        );
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn upgrades_when_bundled_is_newer_with_backup() {
        let home = temp_home("upgrade");
        write_share_pkg(&home, "2.0.0", b"bundled wasm v2");
        write_installed_pkg(&home, "1.0.0", b"old wasm v1");
        write_registry(
            &home,
            "installed:\n  - name: create\n    granted_caps:\n      - fs.read:/documents/create/**\n      - fs.write:/documents/create/**\n    quarantined: false\n",
        );
        write_migration_marker_for_test(&home);

        assert!(manage_create_module(&home));
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
    fn import_legacy_meta_is_idempotent() {
        let home = temp_home("legacy-import");
        write_share_pkg(&home, "1.0.0", b"wasm");
        write_installed_pkg(&home, "1.0.0", b"wasm");
        write_registry(
            &home,
            "installed:\n  - name: create\n    granted_caps:\n      - fs.read:/documents/create/**\n      - fs.write:/documents/create/**\n    quarantined: false\n",
        );
        write_migration_marker_for_test(&home);

        let downloads = downloads_host_dir(&home);
        fs::create_dir_all(&downloads).unwrap();
        let meta = LegacyImageMeta {
            version: 1,
            created_unix: 1_700_000_000,
            path: "/downloads/image-test.png".into(),
            model_id: "local:sd".into(),
            engine: "stub".into(),
            prompt: "a cat".into(),
        };
        fs::write(
            downloads.join("image-test.meta.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();

        let n1 = import_legacy_history_if_needed(&home).unwrap();
        assert_eq!(n1, 1);
        let history = load_history_store(&history_host_path(&home));
        assert_eq!(history.items.len(), 1);
        assert_eq!(history.items[0].path, "/downloads/image-test.png");

        let n2 = import_legacy_history_if_needed(&home).unwrap();
        assert_eq!(n2, 0);
        let history2 = load_history_store(&history_host_path(&home));
        assert_eq!(history2.items.len(), 1);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn uninstall_preserves_documents_directory() {
        let home = temp_home("docs-preserve");
        let docs = documents_host_root(&home);
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("history.json"), br#"{"items":[]}"#).unwrap();
        write_registry(
            &home,
            "installed: []\nremoved:\n  - name: create\n    user_removed: true\n    preinstalled: false\n    granted_caps: []\n",
        );
        assert!(!manage_create_module(&home));
        assert!(docs.join("history.json").is_file());
        let _ = fs::remove_dir_all(&home);
    }
}
