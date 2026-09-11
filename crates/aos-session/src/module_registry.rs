//! Structured read/write for `var/modules/registry.yaml`.
//!
//! Replaces raw string appends (which produced invalid YAML after `removed: []`).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModuleRegistry {
    #[serde(default)]
    pub installed: Vec<InstalledEntry>,
    #[serde(default)]
    pub removed: Vec<RemovedEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstalledEntry {
    pub name: String,
    pub granted_caps: Vec<String>,
    #[serde(default)]
    pub quarantined: bool,
    #[serde(default)]
    pub preinstalled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemovedEntry {
    pub name: String,
    pub user_removed: bool,
    #[serde(default)]
    pub preinstalled: bool,
    #[serde(default)]
    pub granted_caps: Vec<String>,
}

pub fn load_registry(path: &Path) -> ModuleRegistry {
    if !path.is_file() {
        return ModuleRegistry::default();
    }
    let raw = fs::read_to_string(path).unwrap_or_default();
    serde_yaml::from_str(&raw).unwrap_or_default()
}

pub fn save_registry(path: &Path, registry: &ModuleRegistry) -> Result<(), String> {
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
    pub fn user_removed(&self, name: &str) -> bool {
        self.removed
            .iter()
            .any(|e| e.name == name && e.user_removed)
    }

    pub fn ensure_installed(&mut self, name: &str, granted_caps: Vec<String>, preinstalled: bool) {
        let quarantined = granted_caps.is_empty();
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

pub fn registry_dirty(path: &Path, registry: &ModuleRegistry) -> bool {
    let current = load_registry(path);
    current.installed != registry.installed || current.removed != registry.removed
}

/// Ensure `name` appears under `installed:` when `installed_dir/module.wasm` exists.
/// Returns `true` when the registry file was updated.
pub fn ensure_packaged_module_registry_entry(
    registry_path: &Path,
    installed_dir: &Path,
    name: &str,
    granted_caps: Vec<String>,
    preinstalled: bool,
) -> Result<bool, String> {
    if !installed_dir.join("module.wasm").is_file() {
        return Ok(false);
    }
    let mut registry = load_registry(registry_path);
    if registry.user_removed(name) {
        return Ok(false);
    }
    registry.ensure_installed(name, granted_caps, preinstalled);
    if !registry_dirty(registry_path, &registry) {
        return Ok(false);
    }
    save_registry(registry_path, &registry)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_home(tag: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "aos-module-registry-{}-{}-{}",
            tag,
            std::process::id(),
            nanos
        ))
    }

    #[test]
    fn canvas_entry_stays_under_installed_after_create_migration_shape() {
        let home = temp_home("canvas-after-create");
        let modules = home.join("var/modules");
        let canvas_dir = modules.join("canvas");
        fs::create_dir_all(&canvas_dir).unwrap();
        fs::write(canvas_dir.join("module.wasm"), b"canvas wasm").unwrap();
        let reg = modules.join("registry.yaml");
        fs::create_dir_all(reg.parent().unwrap()).unwrap();
        fs::write(
            &reg,
            "installed:\n  - name: create\n    granted_caps:\n      - fs.read:/documents/create/**\n      - fs.write:/documents/create/**\n    quarantined: false\n    preinstalled: true\nremoved: []\n",
        )
        .unwrap();

        let updated = ensure_packaged_module_registry_entry(
            &reg,
            &canvas_dir,
            "canvas",
            vec!["fs.write:/downloads/**".into()],
            true,
        )
        .unwrap();
        assert!(updated);

        let raw = fs::read_to_string(&reg).unwrap();
        let parsed: ModuleRegistry = serde_yaml::from_str(&raw).expect("valid yaml");
        assert_eq!(parsed.installed.len(), 2);
        assert!(parsed.installed.iter().any(|e| e.name == "create"));
        let canvas = parsed
            .installed
            .iter()
            .find(|e| e.name == "canvas")
            .expect("canvas under installed");
        assert!(canvas
            .granted_caps
            .iter()
            .any(|c| c == "fs.write:/downloads/**"));
        assert!(parsed.removed.is_empty());

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn raw_append_shape_is_invalid_yaml() {
        let corrupt = "installed:\n  - name: create\n    granted_caps: []\n    quarantined: false\nremoved: []\n\n  - name: canvas\n    granted_caps:\n      - fs.write:/downloads/**\n    quarantined: false\n";
        assert!(serde_yaml::from_str::<ModuleRegistry>(corrupt).is_err());
    }
}
