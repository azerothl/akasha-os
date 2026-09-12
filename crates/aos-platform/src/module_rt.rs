#![allow(clippy::items_after_test_module)]
//! Module Runtime (§7) : sandbox WASM (wasmtime), injection de caps,
//! introspection de schémas.
//!
//! ## Contrat binaire guest (cf. `aos-module-sdk`)
//!
//! - exports : `alloc(u32) -> u32`, `dealloc(u32, u32)`,
//!   `invoke(ptr, len) -> u64` (réponse = (ptr << 32) | len) ;
//! - import : `env.host_call(svc_ptr, svc_len, args_ptr, args_len) -> u64` —
//!   **seul canal** du module vers le système (pas de WASI, aucun accès
//!   ambiant, §7.4) ;
//! - les échanges sont du JSON UTF-8 : requête `{tool, args}`, réponse
//!   `{ok, result|error}`.
//!
//! ## Sécurité
//!
//! - chaque `host_call` est vérifié contre les **caps approuvées à
//!   l'installation** (injection de capacités, P2.2) — refus audité
//!   (`policy.deny`) ;
//! - l'appelant (agent) doit détenir `tool.invoke:<module>` (les humains
//!   sont admin en v1 mono-utilisateur, §12) ;
//! - bornes par invocation : fuel CPU + mémoire linéaire limitée (§7.4).

use aos_proto::decl_ui::{self, DeclUiDocument, ModuleUiResponse, PreviewProfile};
use aos_proto::rich_decl_ui::{validate_manifest_services, validate_ui_contract_supported};
use aos_proto::{ModuleInfo, ModuleManifest, OS_API_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use wasmtime::{Caller, Config, Engine, Linker, Store};

#[derive(Debug, Error)]
pub enum ModuleError {
    #[error("module inconnu: {0}")]
    NotFound(String),
    #[error("outil inconnu: {0}")]
    UnknownTool(String),
    #[error("module protégé par l'hôte (politique): {0}")]
    ProtectedByHost(String),
    #[error("module en quarantaine: {0}")]
    Quarantined(String),
    #[error("permission refusée: l'acteur doit détenir tool.invoke:{0}")]
    ActorDenied(String),
    #[error("manifeste invalide: {0}")]
    BadManifest(String),
    #[error("hash du binaire WASM non conforme au manifeste")]
    HashMismatch,
    #[error("piège WASM: {0}")]
    Trap(String),
    #[error("erreur d'exécution du module: {0}")]
    Guest(String),
    #[error("revue de caps requise: {0}")]
    CapReviewRequired(String),
    #[error("hash catalogue non conforme pour {0}")]
    CatalogueMismatch(String),
    #[error("signature catalogue invalide")]
    CatalogueSignature,
    #[error("UI déclarative invalide: {0}")]
    DeclUiInvalid(String),
    #[error("ui.contract non supporté: le module exige {required}, l'hôte expose {current}")]
    UiContractUnsupported { required: u32, current: u32 },
    #[error("services.* incompatible: {0}")]
    ServicesVersionInvalid(String),
    #[error("module bundlé non désinstallable: {0}")]
    Bundled(String),
    #[error("min_os_api trop récent: le module exige {required}, l'hôte expose {current}")]
    OsApiTooNew { required: u32, current: u32 },
    #[error("intégrité du package: {0}")]
    PackageIntegrity(String),
    #[error("io: {0}")]
    Io(String),
}

impl From<std::io::Error> for ModuleError {
    fn from(e: std::io::Error) -> Self {
        ModuleError::Io(e.to_string())
    }
}

/// Services offerts aux modules (implémentés par le daemon plateforme).
pub trait HostServices: Send + Sync {
    /// Exécute un appel système au nom d'un module.
    /// `ctx` porte les caps du module ; le service vérifie l'autorisation.
    fn call(
        &self,
        ctx: &HostCallCtx,
        service: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
}

/// Contexte d'un appel système émis par un module.
#[derive(Debug, Clone)]
pub struct HostCallCtx {
    pub module: String,
    /// Répertoire d'installation (lecture d'assets sans re-lock du registry).
    pub module_dir: PathBuf,
    pub granted_caps: Vec<String>,
    pub actor: String,
    pub trace_id: String,
}

/// Lit un asset relatif au répertoire d'un module installé.
pub fn read_module_asset_from_dir(dir: &Path, rel: &str) -> Result<Vec<u8>, ModuleError> {
    let rel = rel.trim_start_matches('/').trim_start_matches('\\');
    if rel.contains("..") {
        return Err(ModuleError::BadManifest("chemin asset invalide".into()));
    }
    let path = dir.join(rel);
    if !path.starts_with(dir) {
        return Err(ModuleError::BadManifest("chemin asset hors module".into()));
    }
    std::fs::read(&path).map_err(|e| ModuleError::Io(e.to_string()))
}

/// État interne d'un Store wasmtime.
struct StoreState {
    ctx: HostCallCtx,
    services: Arc<dyn HostServices>,
    denied: Vec<String>,
    limits: wasmtime::StoreLimits,
}

/// Requête/réponse du contrat binaire.
#[derive(Debug, Serialize)]
struct GuestRequest<'a> {
    tool: &'a str,
    args: &'a serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct GuestResponse {
    ok: bool,
    #[serde(default)]
    result: serde_json::Value,
    #[serde(default)]
    error: Option<String>,
}

/// Module installé (registry local, §7.1).
pub struct InstalledModule {
    pub manifest: ModuleManifest,
    pub granted_caps: Vec<String>,
    pub quarantined: bool,
    /// Outils réellement dispatchés par le WASM (manifest ∩ exports guest).
    verified_tools: Vec<String>,
    dir: PathBuf,
    compiled: wasmtime::Module,
}

/// True when the guest handler rejected a tool name (not a validation error).
pub fn is_unknown_tool_guest_error(err: &str, tool: &str) -> bool {
    err == format!("outil inconnu: {tool}")
}

/// Registry record for a module the user explicitly removed (survives file wipe).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemovedModuleEntry {
    pub name: String,
    pub user_removed: bool,
    #[serde(default)]
    pub preinstalled: bool,
    #[serde(default)]
    pub granted_caps: Vec<String>,
}

/// Le runtime de modules.
pub struct ModuleRuntime {
    engine: Engine,
    dir: PathBuf,
    installed: HashMap<String, InstalledModule>,
    removed: HashMap<String, RemovedModuleEntry>,
    services: Arc<dyn HostServices>,
    catalogue: Option<crate::catalogue::SignedCatalogue>,
    extra_catalogue: Option<crate::catalogue::SignedCatalogue>,
}

/// Bornes d'exécution par invocation (§7.4).
const FUEL_PER_CALL: u64 = 200_000_000;
const MEMORY_MAX_BYTES: usize = 64 << 20;

impl ModuleRuntime {
    pub fn open(
        dir: impl AsRef<Path>,
        services: Arc<dyn HostServices>,
    ) -> Result<Self, ModuleError> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)?;
        let mut config = Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config).map_err(|e| ModuleError::Trap(e.to_string()))?;
        let mut rt = Self {
            engine,
            dir,
            installed: HashMap::new(),
            removed: HashMap::new(),
            services,
            catalogue: None,
            extra_catalogue: None,
        };
        rt.recover_incomplete_installs()?;
        rt.load_registry()?;
        Ok(rt)
    }

    /// Charge le catalogue local signé (E10). Absent → pas de check hash.
    pub fn set_catalogue(&mut self, catalogue: crate::catalogue::SignedCatalogue) {
        self.catalogue = Some(catalogue);
    }

    pub fn catalogue(&self) -> Option<&crate::catalogue::SignedCatalogue> {
        self.catalogue.as_ref()
    }

    pub fn set_extra_catalogue(&mut self, catalogue: Option<crate::catalogue::SignedCatalogue>) {
        self.extra_catalogue = catalogue;
    }

    pub fn extra_catalogue(&self) -> Option<&crate::catalogue::SignedCatalogue> {
        self.extra_catalogue.as_ref()
    }

    fn registry_path(&self) -> PathBuf {
        self.dir.join("registry.yaml")
    }

    fn staging_root(&self) -> PathBuf {
        self.dir.join(".staging")
    }

    fn previous_root(&self) -> PathBuf {
        self.dir.join(".previous")
    }

    /// True when the user explicitly uninstalled this module (registry survives file wipe).
    pub fn user_removed(&self, name: &str) -> bool {
        self.removed.get(name).is_some_and(|e| e.user_removed)
    }

    /// Whether an automatic preinstall/sync may install this module.
    pub fn should_auto_install(&self, name: &str) -> bool {
        !self.user_removed(name)
    }

    /// Install or upgrade from a packaged source unless the user removed it.
    pub fn install_preinstalled(
        &mut self,
        source_dir: &Path,
        approved_caps: Option<Vec<String>>,
    ) -> Result<Option<ModuleInfo>, ModuleError> {
        let manifest = read_manifest_from_dir(source_dir)?;
        if self.user_removed(&manifest.name) {
            return Ok(None);
        }
        self.install(source_dir, approved_caps).map(Some)
    }

    fn load_registry(&mut self) -> Result<(), ModuleError> {
        #[derive(Deserialize)]
        struct Reg {
            #[serde(default)]
            installed: Vec<RegEntry>,
            #[serde(default)]
            removed: Vec<RemovedModuleEntry>,
        }
        #[derive(Deserialize)]
        struct RegEntry {
            name: String,
            granted_caps: Vec<String>,
            #[serde(default)]
            quarantined: bool,
            #[serde(default)]
            #[allow(dead_code)]
            preinstalled: bool,
        }
        let path = self.registry_path();
        if !path.exists() {
            return Ok(());
        }
        let reg: Reg = serde_yaml::from_str(&std::fs::read_to_string(&path)?)
            .map_err(|e| ModuleError::BadManifest(e.to_string()))?;
        for entry in reg.removed {
            if entry.user_removed {
                self.removed.insert(entry.name.clone(), entry);
            }
        }
        for entry in reg.installed {
            let mdir = self.dir.join(&entry.name);
            if !mdir.join("manifest.yaml").exists() {
                continue;
            }
            let manifest = read_manifest_from_dir(&mdir)?;
            let compiled = self.compile(&mdir.join("module.wasm"))?;
            let verified_tools = self.verify_manifest_tools(
                &manifest,
                &entry.name,
                &mdir,
                &entry.granted_caps,
                &compiled,
            );
            self.installed.insert(
                entry.name.clone(),
                InstalledModule {
                    manifest,
                    granted_caps: entry.granted_caps,
                    quarantined: entry.quarantined,
                    verified_tools,
                    dir: mdir,
                    compiled,
                },
            );
        }
        Ok(())
    }

    fn save_registry(&self) -> Result<(), ModuleError> {
        #[derive(Serialize)]
        struct Reg<'a> {
            installed: Vec<RegEntry<'a>>,
            removed: Vec<&'a RemovedModuleEntry>,
        }
        #[derive(Serialize)]
        struct RegEntry<'a> {
            name: &'a str,
            granted_caps: &'a [String],
            quarantined: bool,
            preinstalled: bool,
        }
        let mut installed: Vec<_> = self
            .installed
            .values()
            .map(|m| RegEntry {
                name: &m.manifest.name,
                granted_caps: &m.granted_caps,
                quarantined: m.quarantined,
                preinstalled: is_preinstalled_for_runtime(&self.dir, &m.manifest.name),
            })
            .collect();
        installed.sort_by(|a, b| a.name.cmp(b.name));
        let mut removed: Vec<_> = self.removed.values().collect();
        removed.sort_by(|a, b| a.name.cmp(&b.name));
        let reg = Reg { installed, removed };
        let path = self.registry_path();
        let tmp = path.with_extension("yaml.tmp");
        std::fs::write(&tmp, serde_yaml::to_string(&reg).unwrap())?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    fn recover_incomplete_installs(&self) -> Result<(), ModuleError> {
        let staging_root = self.staging_root();
        if staging_root.is_dir() {
            let _ = std::fs::remove_dir_all(&staging_root);
        }
        if !self.previous_root().is_dir() {
            return Ok(());
        }
        for entry in std::fs::read_dir(self.previous_root())? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let active = self.dir.join(&name);
            let needs_restore = !active.is_dir()
                || !active.join("module.wasm").exists()
                || !active.join("manifest.yaml").exists();
            if needs_restore {
                if active.is_dir() {
                    let _ = std::fs::remove_dir_all(&active);
                }
                std::fs::rename(entry.path(), &active)
                    .map_err(|e| ModuleError::Io(e.to_string()))?;
            }
        }
        let _ = std::fs::remove_dir_all(self.previous_root());
        Ok(())
    }

    fn compile(&self, wasm_path: &Path) -> Result<wasmtime::Module, ModuleError> {
        wasmtime::Module::from_file(&self.engine, wasm_path)
            .map_err(|e| ModuleError::BadManifest(format!("compile {}: {e}", wasm_path.display())))
    }

    /// `module.install` : stage, validate, activate atomically (F-MOD-01).
    /// `approved_caps` is **obligatoire** for first install (revue F-EXT-05) — `None` →
    /// [`ModuleError::CapReviewRequired`] with the liste demandée.
    pub fn install(
        &mut self,
        source_dir: &Path,
        approved_caps: Option<Vec<String>>,
    ) -> Result<ModuleInfo, ModuleError> {
        let validated = self.validate_package(source_dir)?;
        let manifest = validated.manifest.clone();
        let existing = self.installed.get(&manifest.name);
        let granted = self.resolve_granted_caps(&manifest, existing, approved_caps)?;
        for cap in &granted {
            if !manifest.permissions.required_caps.contains(cap) {
                return Err(ModuleError::BadManifest(format!(
                    "cap approuvée non demandée par le manifeste: {cap}"
                )));
            }
        }
        let quarantined = granted.is_empty() && !manifest.permissions.required_caps.is_empty();
        let dest = self.dir.join(&manifest.name);
        let staging = self.staging_root().join(&manifest.name);
        let backup = self.previous_root().join(&manifest.name);
        let _ = std::fs::remove_dir_all(&staging);
        let _ = std::fs::remove_dir_all(&backup);
        std::fs::create_dir_all(self.staging_root())?;
        copy_dir(source_dir, &staging)?;
        self.validate_package(&staging)?;
        let compiled = self.compile(&staging.join("module.wasm"))?;
        let verified_tools =
            self.verify_manifest_tools(&manifest, &manifest.name, &staging, &granted, &compiled);
        if dest.exists() {
            std::fs::create_dir_all(self.previous_root())?;
            std::fs::rename(&dest, &backup).map_err(|e| ModuleError::Io(e.to_string()))?;
        }
        if let Err(e) = (|| -> Result<(), ModuleError> {
            std::fs::rename(&staging, &dest).map_err(|e| ModuleError::Io(e.to_string()))?;
            self.installed.insert(
                manifest.name.clone(),
                InstalledModule {
                    manifest: manifest.clone(),
                    granted_caps: granted.clone(),
                    quarantined,
                    verified_tools: verified_tools.clone(),
                    dir: dest.clone(),
                    compiled,
                },
            );
            self.removed.remove(&manifest.name);
            self.save_registry()?;
            Ok(())
        })() {
            if backup.is_dir() {
                let _ = std::fs::remove_dir_all(&dest);
                let _ = std::fs::rename(&backup, &dest);
            }
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e);
        }
        let _ = std::fs::remove_dir_all(&backup);
        let _ = std::fs::remove_dir_all(&staging);
        Ok(module_info_from_installed(
            &manifest,
            &verified_tools,
            granted,
            quarantined,
            Some(&dest),
        ))
    }

    fn resolve_granted_caps(
        &self,
        manifest: &ModuleManifest,
        existing: Option<&InstalledModule>,
        approved_caps: Option<Vec<String>>,
    ) -> Result<Vec<String>, ModuleError> {
        let increased = existing
            .map(|m| caps_increased(&m.manifest, &m.granted_caps, manifest))
            .unwrap_or_default();
        match (existing, approved_caps) {
            (_, Some(caps)) => Ok(caps),
            (Some(_existing), None) if !increased.is_empty() => {
                Err(ModuleError::CapReviewRequired(increased.join(", ")))
            }
            (Some(existing), None) => Ok(existing.granted_caps.clone()),
            (None, None) if manifest.permissions.required_caps.is_empty() => Ok(Vec::new()),
            (None, None) => Err(ModuleError::CapReviewRequired(
                manifest.permissions.required_caps.join(", "),
            )),
        }
    }

    fn validate_package(&self, source_dir: &Path) -> Result<ValidatedPackage, ModuleError> {
        validate_package_tree(source_dir)?;
        let manifest = read_manifest_from_dir(source_dir)?;
        if manifest.min_os_api > OS_API_VERSION {
            return Err(ModuleError::OsApiTooNew {
                required: manifest.min_os_api,
                current: OS_API_VERSION,
            });
        }
        let wasm_path = source_dir.join("module.wasm");
        let wasm = std::fs::read(&wasm_path)?;
        let hash = sha256_hex(&wasm);
        if manifest.hash != hash && manifest.hash != format!("sha256:{hash}") {
            return Err(ModuleError::HashMismatch);
        }
        crate::catalogue::check_hash_in(
            self.catalogue.as_ref(),
            self.extra_catalogue.as_ref(),
            &manifest.name,
            "module",
            &hash,
        )
        .map_err(|e| match e {
            crate::catalogue::CatalogueError::HashMismatch(n) => ModuleError::CatalogueMismatch(n),
            crate::catalogue::CatalogueError::BadSignature => ModuleError::CatalogueSignature,
            other => ModuleError::BadManifest(other.to_string()),
        })?;
        validate_package_descriptors(source_dir, &manifest)?;
        let _ = self.compile(&wasm_path)?;
        Ok(ValidatedPackage { manifest })
    }

    /// Lit le manifeste d'un package sans installer (pour revue UI).
    pub fn peek_required_caps(source_dir: &Path) -> Result<(String, Vec<String>), ModuleError> {
        let manifest: ModuleManifest =
            serde_yaml::from_str(&std::fs::read_to_string(source_dir.join("manifest.yaml"))?)
                .map_err(|e| ModuleError::BadManifest(e.to_string()))?;
        Ok((manifest.name, manifest.permissions.required_caps))
    }

    pub fn uninstall(&mut self, name: &str) -> Result<(), ModuleError> {
        if decl_ui::is_protected_by_host(name) {
            return Err(ModuleError::ProtectedByHost(name.into()));
        }
        let m = self
            .installed
            .remove(name)
            .ok_or_else(|| ModuleError::NotFound(name.into()))?;
        let _ = std::fs::remove_dir_all(&m.dir);
        self.removed.insert(
            name.to_string(),
            RemovedModuleEntry {
                name: name.to_string(),
                user_removed: true,
                preinstalled: is_preinstalled_for_runtime(&self.dir, name),
                granted_caps: m.granted_caps,
            },
        );
        self.save_registry()?;
        Ok(())
    }

    pub fn list(&self) -> Vec<ModuleInfo> {
        self.installed
            .values()
            .map(|m| {
                module_info_from_installed(
                    &m.manifest,
                    &m.verified_tools,
                    m.granted_caps.clone(),
                    m.quarantined,
                    Some(&m.dir),
                )
            })
            .collect()
    }

    /// Re-read manifest + WASM from disk and refresh `verified_tools`.
    /// Used after `share/modules/*.aospkg` is synced into `var/modules/<name>`.
    pub fn reload_installed(&mut self, name: &str) -> Result<ModuleInfo, ModuleError> {
        let (mdir, granted_caps, quarantined) = {
            let m = self
                .installed
                .get(name)
                .ok_or_else(|| ModuleError::NotFound(name.into()))?;
            (m.dir.clone(), m.granted_caps.clone(), m.quarantined)
        };
        let manifest: ModuleManifest =
            serde_yaml::from_str(&std::fs::read_to_string(mdir.join("manifest.yaml"))?)
                .map_err(|e| ModuleError::BadManifest(e.to_string()))?;
        let wasm = std::fs::read(mdir.join("module.wasm"))?;
        let hash = sha256_hex(&wasm);
        if manifest.hash != hash && manifest.hash != format!("sha256:{hash}") {
            return Err(ModuleError::HashMismatch);
        }
        let compiled = self.compile(&mdir.join("module.wasm"))?;
        let verified_tools =
            self.verify_manifest_tools(&manifest, name, &mdir, &granted_caps, &compiled);
        let info = module_info_from_installed(
            &manifest,
            &verified_tools,
            granted_caps.clone(),
            quarantined,
            Some(&mdir),
        );
        self.installed.insert(
            name.to_string(),
            InstalledModule {
                manifest,
                granted_caps,
                quarantined,
                verified_tools,
                dir: mdir,
                compiled,
            },
        );
        Ok(info)
    }

    /// `module.ui` — charge et valide le document UI déclaratif (E15).
    pub fn load_ui(&self, name: &str) -> Result<ModuleUiResponse, ModuleError> {
        let m = self
            .installed
            .get(name)
            .ok_or_else(|| ModuleError::NotFound(name.into()))?;
        if m.quarantined {
            return Err(ModuleError::Quarantined(name.into()));
        }
        let ui = m
            .manifest
            .ui
            .as_ref()
            .ok_or_else(|| ModuleError::BadManifest("pas de section ui".into()))?;
        if ui.mode != "declarative_ui" {
            return Err(ModuleError::BadManifest(format!(
                "mode ui non supporté en Preview: {}",
                ui.mode
            )));
        }
        let contract = ui.contract_version();
        validate_ui_contract_supported(contract).map_err(|_| {
            ModuleError::UiContractUnsupported {
                required: contract,
                current: aos_proto::rich_app_contract::HOST_UI_CONTRACT_MAX,
            }
        })?;
        let raw = self.read_asset(name, ui.document_path())?;
        let tool_names: Vec<&str> = m.manifest.tools.iter().map(|t| t.name.as_str()).collect();
        let document = DeclUiDocument::parse_json_with_contract(&raw, contract)
            .map_err(|e| ModuleError::DeclUiInvalid(e.to_string()))?;
        validate_manifest_services(
            m.manifest.services.jobs,
            m.manifest.services.media_image,
            &document,
            contract,
        )
        .map_err(|e| ModuleError::ServicesVersionInvalid(e.to_string()))?;
        document
            .validate_with_contract(contract, &tool_names, &m.granted_caps)
            .map_err(|e| ModuleError::DeclUiInvalid(e.to_string()))?;
        Ok(ModuleUiResponse {
            module: name.to_string(),
            document,
            tools: m.manifest.tools.clone(),
        })
    }

    /// `module.describe` : manifeste + schémas (introspection, F-MOD-03).
    pub fn describe(&self, name: &str) -> Result<(&ModuleManifest, &[String]), ModuleError> {
        let m = self
            .installed
            .get(name)
            .ok_or_else(|| ModuleError::NotFound(name.into()))?;
        Ok((&m.manifest, &m.granted_caps))
    }

    /// Chemin d'installation d'un module (pour lecture d'assets).
    pub fn module_dir(&self, name: &str) -> Result<&Path, ModuleError> {
        self.installed
            .get(name)
            .map(|m| m.dir.as_path())
            .ok_or_else(|| ModuleError::NotFound(name.into()))
    }

    /// Lit un fichier asset relatif au package installé (ex. handlers.yaml).
    pub fn read_asset(&self, name: &str, rel: &str) -> Result<Vec<u8>, ModuleError> {
        read_module_asset_from_dir(self.module_dir(name)?, rel)
    }

    pub fn set_quarantined(&mut self, name: &str, quarantined: bool) -> Result<(), ModuleError> {
        let m = self
            .installed
            .get_mut(name)
            .ok_or_else(|| ModuleError::NotFound(name.into()))?;
        m.quarantined = quarantined;
        self.save_registry()?;
        Ok(())
    }

    /// `module.invoke` : appel sandboxé d'un outil (F-MOD-04).
    pub fn invoke(
        &self,
        module: &str,
        tool: &str,
        args: &serde_json::Value,
        actor: &str,
        actor_caps: &[String],
        trace_id: &str,
    ) -> Result<serde_json::Value, ModuleError> {
        let m = self
            .installed
            .get(module)
            .ok_or_else(|| ModuleError::NotFound(module.into()))?;
        if m.quarantined {
            return Err(ModuleError::Quarantined(module.into()));
        }
        if !m.verified_tools.iter().any(|t| t == tool) {
            return Err(ModuleError::UnknownTool(tool.into()));
        }
        // Autorisation de l'appelant (§4.4) : les humains sont admin en v1.
        if !actor.starts_with("human:")
            && !actor_caps
                .iter()
                .any(|c| c == &format!("tool.invoke:{module}") || c == "tool.invoke:*")
        {
            return Err(ModuleError::ActorDenied(module.into()));
        }

        self.guest_invoke_compiled(
            module,
            &m.dir,
            &m.granted_caps,
            &m.compiled,
            tool,
            args,
            actor,
            trace_id,
        )
    }

    fn verify_manifest_tools(
        &self,
        manifest: &ModuleManifest,
        module_name: &str,
        module_dir: &Path,
        granted_caps: &[String],
        compiled: &wasmtime::Module,
    ) -> Vec<String> {
        manifest
            .tools
            .iter()
            .filter(|t| {
                self.probe_wasm_exports_tool(
                    module_name,
                    module_dir,
                    granted_caps,
                    compiled,
                    &t.name,
                )
            })
            .map(|t| t.name.clone())
            .collect()
    }

    fn probe_wasm_exports_tool(
        &self,
        module_name: &str,
        module_dir: &Path,
        granted_caps: &[String],
        compiled: &wasmtime::Module,
        tool: &str,
    ) -> bool {
        let probe_args = probe_args_for_tool(tool);
        match self.guest_invoke_compiled(
            module_name,
            module_dir,
            granted_caps,
            compiled,
            tool,
            &probe_args,
            "human:probe",
            "module-probe",
        ) {
            Ok(_) => true,
            Err(ModuleError::Guest(e)) if is_unknown_tool_guest_error(&e, tool) => false,
            Err(_) => true,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn guest_invoke_compiled(
        &self,
        module: &str,
        module_dir: &Path,
        granted_caps: &[String],
        compiled: &wasmtime::Module,
        tool: &str,
        args: &serde_json::Value,
        actor: &str,
        trace_id: &str,
    ) -> Result<serde_json::Value, ModuleError> {
        let mut store = Store::new(
            &self.engine,
            StoreState {
                ctx: HostCallCtx {
                    module: module.into(),
                    module_dir: module_dir.to_path_buf(),
                    granted_caps: granted_caps.to_vec(),
                    actor: actor.into(),
                    trace_id: trace_id.into(),
                },
                services: self.services.clone(),
                denied: Vec::new(),
                limits: wasmtime::StoreLimitsBuilder::new()
                    .memory_size(MEMORY_MAX_BYTES)
                    .build(),
            },
        );
        store
            .set_fuel(FUEL_PER_CALL)
            .map_err(|e| ModuleError::Trap(e.to_string()))?;
        store.limiter(|state| &mut state.limits);

        let mut linker = Linker::<StoreState>::new(&self.engine);
        linker
            .func_wrap(
                "env",
                "host_call",
                |mut caller: Caller<'_, StoreState>,
                 svc_ptr: u32,
                 svc_len: u32,
                 args_ptr: u32,
                 args_len: u32|
                 -> i64 {
                    let (response, deny_note) = {
                        let mem = match caller.get_export("memory") {
                            Some(wasmtime::Extern::Memory(m)) => m,
                            _ => return 0,
                        };
                        let svc = read_guest_str(&caller, mem, svc_ptr, svc_len);
                        let args = read_guest_str(&caller, mem, args_ptr, args_len);
                        let state = caller.data();
                        let result = match (svc, args) {
                            (Some(svc), Some(args_str)) => {
                                let args_json: serde_json::Value =
                                    serde_json::from_str(&args_str).unwrap_or_default();
                                state.services.call(&state.ctx, &svc, args_json)
                            }
                            _ => Err("host_call illisible".into()),
                        };
                        let deny = result.as_ref().err().cloned();
                        let json = match result {
                            Ok(v) => serde_json::json!({"ok": true, "result": v}),
                            Err(e) => serde_json::json!({"ok": false, "error": e}),
                        };
                        (json.to_string(), deny)
                    };
                    if let Some(d) = deny_note {
                        if d.starts_with("permission refusée") {
                            caller.data_mut().denied.push(d);
                        }
                    }
                    write_guest_response(&mut caller, &response).unwrap_or(0)
                },
            )
            .map_err(|e| ModuleError::Trap(e.to_string()))?;

        let instance = linker
            .instantiate(&mut store, compiled)
            .map_err(|e| ModuleError::Trap(e.to_string()))?;
        let req = serde_json::to_string(&GuestRequest { tool, args }).unwrap();
        let response = call_guest_invoke(&mut store, &instance, &req)?;

        let denied = std::mem::take(&mut store.data_mut().denied);
        let parsed: GuestResponse =
            serde_json::from_str(&response).map_err(|e| ModuleError::Guest(e.to_string()))?;
        if !denied.is_empty() {
            return Err(ModuleError::Guest(format!(
                "permission refusée (auditée): {}",
                denied.join("; ")
            )));
        }
        if !parsed.ok {
            return Err(ModuleError::Guest(
                parsed.error.unwrap_or_else(|| "erreur inconnue".into()),
            ));
        }
        Ok(parsed.result)
    }
}

/// Lit une chaîne UTF-8 dans la mémoire du guest.
fn read_guest_str(
    caller: &Caller<'_, StoreState>,
    mem: wasmtime::Memory,
    ptr: u32,
    len: u32,
) -> Option<String> {
    let mut buf = vec![0u8; len as usize];
    mem.read(caller, ptr as usize, &mut buf).ok()?;
    String::from_utf8(buf).ok()
}

/// Écrit `response` dans la mémoire du guest via son `alloc`, retourne
/// (ptr << 32) | len.
fn write_guest_response(
    mut caller: &mut Caller<'_, StoreState>,
    response: &str,
) -> Result<i64, ModuleError> {
    let mem = match caller.get_export("memory") {
        Some(wasmtime::Extern::Memory(m)) => m,
        _ => return Err(ModuleError::Trap("memory export manquant".into())),
    };
    let alloc = caller
        .get_export("alloc")
        .and_then(|e| e.into_func())
        .and_then(|f| f.typed::<u32, u32>(&caller).ok())
        .ok_or_else(|| ModuleError::Trap("alloc export manquant".into()))?;
    let bytes = response.as_bytes();
    let ptr = alloc
        .call(&mut caller, bytes.len() as u32)
        .map_err(|e| ModuleError::Trap(e.to_string()))?;
    mem.write(&mut caller, ptr as usize, bytes)
        .map_err(|e| ModuleError::Trap(e.to_string()))?;
    Ok((((ptr as u64) << 32) | bytes.len() as u64) as i64)
}

/// Appelle l'export `invoke` du guest et lit sa réponse.
fn call_guest_invoke(
    store: &mut Store<StoreState>,
    instance: &wasmtime::Instance,
    request: &str,
) -> Result<String, ModuleError> {
    let mem = instance
        .get_memory(&mut *store, "memory")
        .ok_or_else(|| ModuleError::Trap("memory export manquant".into()))?;
    let alloc = instance
        .get_typed_func::<u32, u32>(&mut *store, "alloc")
        .map_err(|e| ModuleError::Trap(e.to_string()))?;
    let invoke = instance
        .get_typed_func::<(u32, u32), i64>(&mut *store, "invoke")
        .map_err(|e| ModuleError::Trap(e.to_string()))?;
    let bytes = request.as_bytes();
    let ptr = alloc
        .call(&mut *store, bytes.len() as u32)
        .map_err(|e| ModuleError::Trap(e.to_string()))?;
    mem.write(&mut *store, ptr as usize, bytes)
        .map_err(|e| ModuleError::Trap(e.to_string()))?;
    let packed = invoke
        .call(&mut *store, (ptr, bytes.len() as u32))
        .map_err(|e| ModuleError::Trap(e.to_string()))?;
    let rptr = (packed >> 32) as u32;
    let rlen = (packed & 0xFFFF_FFFF) as u32;
    let mut buf = vec![0u8; rlen as usize];
    mem.read(&mut *store, rptr as usize, &mut buf)
        .map_err(|e| ModuleError::Trap(e.to_string()))?;
    String::from_utf8(buf).map_err(|e| ModuleError::Guest(e.to_string()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(bytes);
    let mut s = String::with_capacity(64);
    for b in d {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

struct ValidatedPackage {
    manifest: ModuleManifest,
}

fn read_manifest_from_dir(dir: &Path) -> Result<ModuleManifest, ModuleError> {
    serde_yaml::from_str(&std::fs::read_to_string(dir.join("manifest.yaml"))?)
        .map_err(|e| ModuleError::BadManifest(e.to_string()))
}

fn preview_profile_for_modules_dir(dir: &Path) -> PreviewProfile {
    let home = dir
        .parent()
        .and_then(|var| var.parent())
        .unwrap_or_else(|| Path::new("."));
    decl_ui::resolve_preview_profile(home)
}

fn is_preinstalled_for_runtime(dir: &Path, name: &str) -> bool {
    let profile = preview_profile_for_modules_dir(dir);
    decl_ui::is_preinstalled_for_profile(name, profile)
}

fn caps_increased(
    old_manifest: &ModuleManifest,
    _old_granted: &[String],
    new_manifest: &ModuleManifest,
) -> Vec<String> {
    new_manifest
        .permissions
        .required_caps
        .iter()
        .filter(|cap| !old_manifest.permissions.required_caps.contains(cap))
        .cloned()
        .collect()
}

fn validate_package_tree(source_dir: &Path) -> Result<(), ModuleError> {
    if !source_dir.join("manifest.yaml").is_file() {
        return Err(ModuleError::PackageIntegrity(
            "manifest.yaml manquant".into(),
        ));
    }
    if !source_dir.join("module.wasm").is_file() {
        return Err(ModuleError::PackageIntegrity("module.wasm manquant".into()));
    }
    walk_package_files(source_dir, source_dir, &mut |rel| {
        if rel.contains("..") {
            return Err(ModuleError::PackageIntegrity(format!(
                "chemin package invalide: {rel}"
            )));
        }
        Ok(())
    })
}

fn validate_package_descriptors(
    source_dir: &Path,
    manifest: &ModuleManifest,
) -> Result<(), ModuleError> {
    for tool in &manifest.tools {
        if tool.name.trim().is_empty() {
            return Err(ModuleError::PackageIntegrity(
                "outil sans nom dans le manifeste".into(),
            ));
        }
        if tool.description.trim().is_empty() {
            return Err(ModuleError::PackageIntegrity(format!(
                "outil {} sans description",
                tool.name
            )));
        }
    }
    if let Some(ui) = manifest.ui.as_ref() {
        if ui.entry.contains("..") || ui.document.as_ref().is_some_and(|d| d.contains("..")) {
            return Err(ModuleError::PackageIntegrity(
                "chemin ui.entry invalide".into(),
            ));
        }
        let doc_path = ui.document_path();
        let ui_path = source_dir.join(doc_path);
        if !ui_path.starts_with(source_dir) {
            return Err(ModuleError::PackageIntegrity(
                "ui.document hors du package".into(),
            ));
        }
        if !ui_path.is_file() {
            return Err(ModuleError::PackageIntegrity(format!(
                "ui.document introuvable: {doc_path}"
            )));
        }
        let contract = ui.contract_version();
        validate_ui_contract_supported(contract).map_err(|_| {
            ModuleError::UiContractUnsupported {
                required: contract,
                current: aos_proto::rich_app_contract::HOST_UI_CONTRACT_MAX,
            }
        })?;
        let raw = std::fs::read(&ui_path).map_err(|e| ModuleError::Io(e.to_string()))?;
        if ui.mode == "declarative_ui" {
            let doc: serde_json::Value = serde_json::from_slice(&raw)
                .map_err(|e| ModuleError::DeclUiInvalid(e.to_string()))?;
            if doc.get("type").and_then(|t| t.as_str()) != Some("declarative_ui") {
                return Err(ModuleError::DeclUiInvalid(
                    "type must be declarative_ui".into(),
                ));
            }
            if doc.get("root").is_some() {
                let tool_names: Vec<&str> =
                    manifest.tools.iter().map(|t| t.name.as_str()).collect();
                let granted = manifest.permissions.required_caps.clone();
                let document = DeclUiDocument::parse_json_with_contract(&raw, contract)
                    .map_err(|e| ModuleError::DeclUiInvalid(e.to_string()))?;
                validate_manifest_services(
                    manifest.services.jobs,
                    manifest.services.media_image,
                    &document,
                    contract,
                )
                .map_err(|e| ModuleError::ServicesVersionInvalid(e.to_string()))?;
                document
                    .validate_with_contract(contract, &tool_names, &granted)
                    .map_err(|e| ModuleError::DeclUiInvalid(e.to_string()))?;
            }
        }
    }
    Ok(())
}

fn walk_package_files(
    root: &Path,
    dir: &Path,
    visit: &mut dyn FnMut(&str) -> Result<(), ModuleError>,
) -> Result<(), ModuleError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .map_err(|_| ModuleError::PackageIntegrity("chemin package hors racine".into()))?
            .to_string_lossy()
            .replace('\\', "/");
        visit(&rel)?;
        if entry.file_type()?.is_dir() {
            walk_package_files(root, &path, visit)?;
        }
    }
    Ok(())
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Minimal args for WASM tool probes (`verify_manifest_tools`).
pub fn probe_args_for_tool(tool: &str) -> serde_json::Value {
    match tool {
        "canvas.path" | "canvas.stroke" | "canvas.spline" => serde_json::json!({
            "session_id": "__probe__",
            "points": [
                {"x": 0.1, "y": 0.1},
                {"x": 0.2, "y": 0.1},
                {"x": 0.15, "y": 0.2}
            ]
        }),
        "canvas.line" => serde_json::json!({
            "session_id": "__probe__",
            "p0": {"x": 0.1, "y": 0.1},
            "p1": {"x": 0.2, "y": 0.2}
        }),
        "canvas.rect" | "canvas.ellipse" => serde_json::json!({
            "session_id": "__probe__",
            "x": 0.1,
            "y": 0.1,
            "w": 0.1,
            "h": 0.1
        }),
        "canvas.fill" => serde_json::json!({
            "session_id": "__probe__",
            "x": 0.5,
            "y": 0.5
        }),
        "canvas.text" => serde_json::json!({
            "session_id": "__probe__",
            "x": 0.1,
            "y": 0.1,
            "text": "probe"
        }),
        "canvas.set_guides" => serde_json::json!({
            "session_id": "__probe__",
            "show_grid": true,
            "snap": true,
            "grid_size": 0.01,
            "snap_mode": "grid"
        }),
        "canvas.compose" => serde_json::json!({
            "session_id": "__probe__",
            "scene": {
                "version": 1,
                "profile": "primitives",
                "elements": [{
                    "id": "probe",
                    "geometry": {"kind": "rect", "x": 0.1, "y": 0.1, "w": 0.1, "h": 0.1}
                }]
            }
        }),
        "canvas.erase" => serde_json::json!({
            "session_id": "__probe__",
            "points": [{"x": 0.1, "y": 0.1}, {"x": 0.2, "y": 0.2}]
        }),
        "notes.create" => serde_json::json!({
            "title": "",
            "content": ""
        }),
        "notes.update" => serde_json::json!({
            "title": "__probe__",
            "content": ""
        }),
        "notes.read" | "notes.links" | "notes.related" => serde_json::json!({
            "title": "__probe__"
        }),
        "notes.search" => serde_json::json!({
            "query": "__probe__"
        }),
        "notes.list" => serde_json::json!({}),
        "tasks.list" => serde_json::json!({}),
        "tasks.create" => serde_json::json!({"title": ""}),
        "tasks.update" | "tasks.complete" => serde_json::json!({"id": "__probe_nonexistent__"}),
        "create.history.list"
        | "create.document.load"
        | "create.result.get"
        | "create.models.list" => {
            serde_json::json!({})
        }
        "create.history.get" => serde_json::json!({"id": "__probe_nonexistent__"}),
        "create.history.record" => serde_json::json!({
            "path": "",
            "prompt": ""
        }),
        "create.document.save" => serde_json::json!("__probe_invalid__"),
        _ => serde_json::json!({"session_id": "__probe__"}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::rich_app_contract::UI_CONTRACT_V2;

    /// Module WAT de test : `invoke` appelle host_call("echo", args) et
    /// retourne sa réponse. Bump allocator volontairement naïf.
    const TEST_WAT: &str = r#"
(module
  (import "env" "host_call" (func $host_call (param i32 i32 i32 i32) (result i64)))
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 1024))
  (func (export "alloc") (param $size i32) (result i32)
    (local $ptr i32)
    (local.set $ptr (global.get $heap))
    (global.set $heap (i32.add (global.get $heap) (local.get $size)))
    (local.get $ptr))
  (func (export "dealloc") (param i32 i32))
  (func (export "invoke") (param $ptr i32) (param $len i32) (result i64)
    (call $host_call
      (i32.const 16) (i32.const 4)
      (local.get $ptr) (local.get $len)))
  (data (i32.const 16) "echo"))
"#;

    struct EchoServices;
    impl HostServices for EchoServices {
        fn call(
            &self,
            _ctx: &HostCallCtx,
            service: &str,
            args: serde_json::Value,
        ) -> Result<serde_json::Value, String> {
            if service == "echo" {
                Ok(args)
            } else {
                Err(format!("service inconnu: {service}"))
            }
        }
    }

    fn make_package(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        let wasm = wat::parse_str(TEST_WAT).unwrap();
        std::fs::write(dir.join("module.wasm"), &wasm).unwrap();
        let hash = sha256_hex(&wasm);
        let manifest = format!(
            r#"name: echo-test
version: 0.1.0
hash: {hash}
permissions:
  required_caps: []
tools:
  - name: echo.ping
    description: renvoie les args
ui: ~
min_os_api: 1
"#
        );
        std::fs::write(dir.join("manifest.yaml"), manifest).unwrap();
    }

    fn tmpbase(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "aos-mod-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn installe_decrit_invoque_desinstalle() {
        let base = tmpbase("full");
        let pkg = base.join("pkg");
        make_package(&pkg);
        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(EchoServices)).unwrap();
        let info = rt.install(&pkg, Some(vec![])).unwrap();
        assert_eq!(info.name, "echo-test");
        assert_eq!(rt.list().len(), 1);
        let (manifest, _) = rt.describe("echo-test").unwrap();
        assert_eq!(manifest.tools[0].name, "echo.ping");

        let out = rt
            .invoke(
                "echo-test",
                "echo.ping",
                &serde_json::json!({"msg": "bonjour"}),
                "human:ui",
                &[],
                "t1",
            )
            .unwrap();
        // Le guest renvoie la requête brute {tool, args} échoïsée.
        assert!(out.to_string().contains("bonjour"));

        rt.uninstall("echo-test").unwrap();
        assert!(rt.list().is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn preinstalled_module_can_uninstall_and_persists_user_removed() {
        let base = tmpbase("user-removed");
        let pkg = base.join("pkg");
        make_package(&pkg);
        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(EchoServices)).unwrap();
        rt.install(&pkg, Some(vec![])).unwrap();
        rt.uninstall("echo-test").unwrap();
        assert!(rt.user_removed("echo-test"));
        assert!(!rt.should_auto_install("echo-test"));
        let reg = std::fs::read_to_string(base.join("modules/registry.yaml")).unwrap();
        assert!(reg.contains("user_removed: true"));
        assert!(reg.contains("removed:"));
        // Simulate file wipe (boot sync deleting var/modules/* without registry).
        assert!(!base.join("modules/echo-test").exists());
        let rt2 = ModuleRuntime::open(base.join("modules"), Arc::new(EchoServices)).unwrap();
        assert!(rt2.user_removed("echo-test"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn invalid_install_keeps_last_good_version() {
        let base = tmpbase("stage-fail");
        let pkg = base.join("pkg");
        make_package(&pkg);
        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(EchoServices)).unwrap();
        rt.install(&pkg, Some(vec![])).unwrap();
        let good_wasm = std::fs::read(base.join("modules/echo-test/module.wasm")).unwrap();
        let bad_pkg = base.join("bad");
        copy_dir(&pkg, &bad_pkg).unwrap();
        let manifest = std::fs::read_to_string(bad_pkg.join("manifest.yaml")).unwrap();
        std::fs::write(
            bad_pkg.join("manifest.yaml"),
            manifest.replace("version: 0.1.0", "version: 9.9.9"),
        )
        .unwrap();
        std::fs::write(bad_pkg.join("module.wasm"), b"not valid wasm").unwrap();
        let err = rt.install(&bad_pkg, Some(vec![])).unwrap_err();
        assert!(
            matches!(
                err,
                ModuleError::Trap(_)
                    | ModuleError::BadManifest(_)
                    | ModuleError::HashMismatch
                    | ModuleError::PackageIntegrity(_)
            ),
            "unexpected err: {err}"
        );
        let kept = std::fs::read(base.join("modules/echo-test/module.wasm")).unwrap();
        assert_eq!(kept, good_wasm);
        assert!(rt.describe("echo-test").is_ok());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn refuses_min_os_api_too_new() {
        let base = tmpbase("os-api");
        let pkg = base.join("pkg");
        make_package(&pkg);
        let manifest = std::fs::read_to_string(pkg.join("manifest.yaml")).unwrap();
        std::fs::write(
            pkg.join("manifest.yaml"),
            manifest.replace("min_os_api: 1", "min_os_api: 999"),
        )
        .unwrap();
        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(EchoServices)).unwrap();
        let err = rt.install(&pkg, Some(vec![])).unwrap_err();
        assert!(matches!(
            err,
            ModuleError::OsApiTooNew {
                required: 999,
                current: 1
            }
        ));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn install_preinstalled_skips_user_removed() {
        let base = tmpbase("preinstalled-skip");
        let pkg = base.join("pkg");
        make_package(&pkg);
        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(EchoServices)).unwrap();
        rt.install(&pkg, Some(vec![])).unwrap();
        rt.uninstall("echo-test").unwrap();
        let out = rt.install_preinstalled(&pkg, Some(vec![])).unwrap();
        assert!(out.is_none());
        assert!(rt.list().is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cap_increase_requires_review_on_replace() {
        let base = tmpbase("cap-inc");
        let pkg = base.join("pkg");
        make_package(&pkg);
        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(EchoServices)).unwrap();
        rt.install(&pkg, Some(vec![])).unwrap();
        let manifest = r#"name: echo-test
version: 0.2.0
hash: PLACEHOLDER
permissions:
  required_caps:
    - fs.read:/documents/**
tools:
  - name: echo.ping
    description: renvoie les args
ui: ~
min_os_api: 1
"#;
        let bad_pkg = base.join("bad");
        copy_dir(&pkg, &bad_pkg).unwrap();
        let wasm = std::fs::read(bad_pkg.join("module.wasm")).unwrap();
        let hash = sha256_hex(&wasm);
        std::fs::write(
            bad_pkg.join("manifest.yaml"),
            manifest.replace("PLACEHOLDER", &hash),
        )
        .unwrap();
        let err = rt.install(&bad_pkg, None).unwrap_err();
        assert!(matches!(err, ModuleError::CapReviewRequired(_)));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn probe_verification_does_not_write_user_documents() {
        struct TrackingHost {
            writes: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        }
        impl HostServices for TrackingHost {
            fn call(
                &self,
                _ctx: &HostCallCtx,
                service: &str,
                args: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                if service == "fs.write" {
                    if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                        self.writes.lock().unwrap().push(path.to_string());
                    }
                    return Ok(serde_json::json!({"version": 1u64}));
                }
                if service == "fs.read" {
                    return Ok(serde_json::json!({"content": "{\"tasks\":[]}"}));
                }
                Err(format!("unexpected service: {service}"))
            }
        }

        let share_pkg =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../share/modules/tasks.aospkg");
        if !share_pkg.join("module.wasm").is_file() {
            eprintln!("skip probe write test: tasks wasm missing");
            return;
        }

        let writes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let base = tmpbase("tasks-probe");
        let mut rt = ModuleRuntime::open(
            base.join("modules"),
            Arc::new(TrackingHost {
                writes: writes.clone(),
            }),
        )
        .unwrap();
        let caps = vec![
            "fs.read:/documents/tasks/**".into(),
            "fs.write:/documents/tasks/**".into(),
        ];
        rt.install(&share_pkg, Some(caps)).expect("install tasks");
        assert!(
            writes.lock().unwrap().is_empty(),
            "tool probes must not fs.write user documents: {:?}",
            writes.lock().unwrap()
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn preinstalled_echo_uninstalls_when_not_protected() {
        let base = tmpbase("bundled");
        let pkg = base.join("pkg");
        make_package(&pkg);
        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(EchoServices)).unwrap();
        rt.install(&pkg, Some(vec![])).unwrap();
        assert!(!decl_ui::is_protected_by_host("echo-test"));
        rt.uninstall("echo-test").unwrap();
        assert!(rt.user_removed("echo-test"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn rejects_package_path_escape() {
        let base = tmpbase("escape");
        let pkg = base.join("pkg");
        make_package(&pkg);
        std::fs::write(pkg.join("bad..slot.txt"), b"evil").unwrap();
        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(EchoServices)).unwrap();
        let err = rt.install(&pkg, Some(vec![])).unwrap_err();
        assert!(matches!(err, ModuleError::PackageIntegrity(_)));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn catalogue_hash_mismatch_refuse() {
        let base = tmpbase("cat");
        let pkg = base.join("pkg");
        make_package(&pkg);
        let wasm = std::fs::read(pkg.join("module.wasm")).unwrap();
        let real = sha256_hex(&wasm);
        let yaml = "version: 1\nentries:\n  - name: echo-test\n    version: \"0.1.0\"\n    kind: module\n    path: pkg\n    hash: sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n    attested_caps: []\n"
            .to_string();
        let yaml_path = base.join("catalogue.yaml");
        std::fs::write(&yaml_path, &yaml).unwrap();
        let (pk, sig) = crate::catalogue::sign_preview_catalogue(yaml.as_bytes());
        std::fs::write(base.join("catalogue.pub"), pk).unwrap();
        std::fs::write(base.join("catalogue.yaml.sig"), sig).unwrap();
        let cat = crate::catalogue::SignedCatalogue::load(&yaml_path).unwrap();
        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(EchoServices)).unwrap();
        rt.set_catalogue(cat);
        let err = rt.install(&pkg, Some(vec![])).unwrap_err();
        assert!(matches!(err, ModuleError::CatalogueMismatch(_)));
        // Honest hash in a second catalogue should install.
        let yaml_ok = format!(
            "version: 1\nentries:\n  - name: echo-test\n    version: \"0.1.0\"\n    kind: module\n    path: pkg\n    hash: sha256:{real}\n    attested_caps: []\n"
        );
        std::fs::write(&yaml_path, &yaml_ok).unwrap();
        let (pk, sig) = crate::catalogue::sign_preview_catalogue(yaml_ok.as_bytes());
        std::fs::write(base.join("catalogue.pub"), pk).unwrap();
        std::fs::write(base.join("catalogue.yaml.sig"), sig).unwrap();
        let cat = crate::catalogue::SignedCatalogue::load(&yaml_path).unwrap();
        rt.set_catalogue(cat);
        assert!(rt.install(&pkg, Some(vec![])).is_ok());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn acteur_sans_cap_refuse() {
        let base = tmpbase("deny");
        let pkg = base.join("pkg");
        make_package(&pkg);
        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(EchoServices)).unwrap();
        rt.install(&pkg, Some(vec![])).unwrap();
        let err = rt
            .invoke(
                "echo-test",
                "echo.ping",
                &serde_json::json!({}),
                "agent:agent-1",
                &[],
                "t2",
            )
            .unwrap_err();
        assert!(matches!(err, ModuleError::ActorDenied(_)));
        // Avec la cap, ça passe.
        assert!(rt
            .invoke(
                "echo-test",
                "echo.ping",
                &serde_json::json!({}),
                "agent:agent-1",
                &["tool.invoke:echo-test".to_string()],
                "t2",
            )
            .is_ok());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Regression: ext-rt must load handlers without re-locking `ModuleRuntime` (deadlock).
    #[test]
    fn ext_rt_handlers_invoke_sans_deadlock() {
        struct ExtRtServices;
        impl HostServices for ExtRtServices {
            fn call(
                &self,
                ctx: &HostCallCtx,
                service: &str,
                args: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                match service {
                    "ext.load_handlers" | "ext.asset_read" => {
                        let rel = args["path"]
                            .as_str()
                            .or_else(|| args["rel"].as_str())
                            .unwrap_or("handlers.yaml");
                        let bytes = read_module_asset_from_dir(&ctx.module_dir, rel)
                            .map_err(|e| e.to_string())?;
                        Ok(serde_json::json!({
                            "content": String::from_utf8_lossy(&bytes),
                        }))
                    }
                    other => Err(format!("service inconnu: {other}")),
                }
            }
        }

        let base = tmpbase("ext-rt");
        let pkg = base.join("pkg");
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../dist/AgentOS-Preview-0.8.0-windows-x64/decldemo.aospkg");
        if !src.join("module.wasm").exists() {
            eprintln!("skip ext_rt_handlers_invoke_sans_deadlock: decldemo.aospkg absent");
            return;
        }
        std::fs::create_dir_all(&pkg).unwrap();
        for entry in std::fs::read_dir(&src).unwrap() {
            let entry = entry.unwrap();
            let ty = entry.file_type().unwrap();
            let dest = pkg.join(entry.file_name());
            if ty.is_dir() {
                copy_dir_all(&entry.path(), &dest);
            } else {
                std::fs::copy(entry.path(), dest).unwrap();
            }
        }

        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(ExtRtServices)).unwrap();
        rt.install(&pkg, Some(vec![])).unwrap();
        let out = rt
            .invoke(
                "decldemo",
                "decldemo.snapshot",
                &serde_json::json!({}),
                "human:ui",
                &[],
                "t-ext-rt",
            )
            .unwrap();
        assert_eq!(out.get("count").and_then(|v| v.as_i64()), Some(3));
        assert_eq!(out.get("ok").and_then(|v| v.as_bool()), Some(true));
        let _ = std::fs::remove_dir_all(&base);
    }

    fn copy_dir_all(src: &Path, dst: &Path) {
        std::fs::create_dir_all(dst).unwrap();
        for entry in std::fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let dest = dst.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir_all(&entry.path(), &dest);
            } else {
                std::fs::copy(entry.path(), dest).unwrap();
            }
        }
    }

    /// Packaged `share/modules/canvas.aospkg` must dispatch `canvas.set_style` / `canvas.fill`.
    #[test]
    fn packaged_canvas_wasm_invokes_set_style_and_fill() {
        struct CanvasHost;
        impl HostServices for CanvasHost {
            fn call(
                &self,
                _ctx: &HostCallCtx,
                service: &str,
                args: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                match service {
                    "canvas.set_style" => {
                        let color = args
                            .get("color")
                            .and_then(|v| v.as_str())
                            .unwrap_or("#3ee0c4");
                        Ok(serde_json::json!({
                            "pen": {"color": color, "width": 0.015},
                            "next_seq": 1,
                            "canvas_open": true,
                        }))
                    }
                    "canvas.apply" => Ok(serde_json::json!({"applied": true})),
                    _ => Err(format!("unexpected host_call: {service}")),
                }
            }
        }

        let share_pkg =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../share/modules/canvas.aospkg");
        assert!(
            share_pkg.join("module.wasm").is_file(),
            "share/modules/canvas.aospkg/module.wasm missing"
        );

        let base = tmpbase("canvas-packaged");
        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(CanvasHost)).unwrap();
        rt.install(&share_pkg, Some(vec!["fs.write:/downloads/**".into()]))
            .expect("install packaged canvas");

        let style = rt.invoke(
            "canvas",
            "canvas.set_style",
            &serde_json::json!({"session_id": "sess-test", "color": "#F40009"}),
            "human:ui",
            &[],
            "t-canvas-style",
        );
        assert!(
            style.is_ok(),
            "packaged wasm must handle canvas.set_style: {:?}",
            style.err()
        );
        let style_json = style.unwrap();
        assert_eq!(
            style_json["pen"]["color"].as_str(),
            Some("#F40009"),
            "set_style should return pen color from host"
        );

        let fill = rt.invoke(
            "canvas",
            "canvas.fill",
            &serde_json::json!({
                "session_id": "sess-test",
                "x": 0.5,
                "y": 0.5,
                "color": "#00ff00"
            }),
            "human:ui",
            &[],
            "t-canvas-fill",
        );
        assert!(
            fill.is_ok(),
            "packaged wasm must handle canvas.fill: {:?}",
            fill.err()
        );

        let unknown_short = rt.invoke(
            "canvas",
            "set_style",
            &serde_json::json!({"session_id": "sess-test", "color": "#F40009"}),
            "human:ui",
            &[],
            "t-canvas-short",
        );
        assert!(
            unknown_short.is_err(),
            "short tool name set_style must not match — agents use canvas.set_style"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// Packaged canvas must export canvas.path when the manifest lists it (PR #102 follow-up).
    #[test]
    fn packaged_canvas_wasm_exports_path_when_manifest_lists_it() {
        struct CanvasHost;
        impl HostServices for CanvasHost {
            fn call(
                &self,
                _ctx: &HostCallCtx,
                service: &str,
                _args: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                match service {
                    "canvas.apply" => Ok(serde_json::json!({"next_seq": 1})),
                    _ => Err(format!("unexpected host_call: {service}")),
                }
            }
        }

        let share_pkg =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../share/modules/canvas.aospkg");
        assert!(
            share_pkg.join("module.wasm").is_file(),
            "share/modules/canvas.aospkg/module.wasm missing"
        );
        let manifest = std::fs::read_to_string(share_pkg.join("manifest.yaml")).unwrap();
        if !manifest.contains("canvas.path") {
            eprintln!("skip: manifest has no canvas.path");
            return;
        }

        let base = tmpbase("canvas-packaged-path");
        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(CanvasHost)).unwrap();
        let info = rt
            .install(&share_pkg, Some(vec!["fs.write:/downloads/**".into()]))
            .expect("install packaged canvas");
        assert!(
            info.tools.iter().any(|t| t == "canvas.path"),
            "module.list must expose canvas.path when wasm exports it: {:?}",
            info.tools
        );

        let path = rt.invoke(
            "canvas",
            "canvas.path",
            &serde_json::json!({
                "session_id": "sess-test",
                "points": [
                    {"x": 0.1, "y": 0.7},
                    {"x": 0.5, "y": 0.55},
                    {"x": 0.9, "y": 0.7}
                ],
                "fill": true
            }),
            "human:ui",
            &[],
            "t-canvas-path",
        );
        assert!(
            path.is_ok(),
            "packaged wasm must handle canvas.path: {:?}",
            path.err()
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// When manifest advertises a tool the wasm does not dispatch, module.list must omit it.
    #[test]
    fn verified_tools_omit_manifest_only_tool_names() {
        struct CanvasHost;
        impl HostServices for CanvasHost {
            fn call(
                &self,
                _ctx: &HostCallCtx,
                _service: &str,
                _args: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                Ok(serde_json::json!({}))
            }
        }

        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output = std::process::Command::new("git")
            .args(["show", "28e44cc:share/modules/canvas.aospkg/module.wasm"])
            .current_dir(&repo)
            .output()
            .expect("git show legacy canvas.wasm");
        if !output.status.success() {
            eprintln!("skip verified_tools test: legacy wasm unavailable");
            return;
        }

        let base = tmpbase("canvas-verified-tools");
        let pkg = base.join("pkg");
        std::fs::create_dir_all(pkg.join("ui")).unwrap();
        std::fs::write(pkg.join("module.wasm"), &output.stdout).unwrap();
        let old_hash = sha256_hex(&output.stdout);
        let manifest = format!(
            r#"name: canvas
version: 1.0.0
hash: {old_hash}
permissions:
  required_caps:
    - fs.write:/downloads/**
tools:
  - name: canvas.set_style
    description: set pen
    input_schema:
      type: object
      properties:
        session_id: {{ type: string }}
      required: [session_id]
  - name: canvas.path
    description: phantom path in manifest only
    input_schema:
      type: object
      properties:
        session_id: {{ type: string }}
        points: {{ type: array }}
      required: [session_id, points]
min_os_api: 1
"#
        );
        std::fs::write(pkg.join("manifest.yaml"), manifest).unwrap();

        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(CanvasHost)).unwrap();
        let info = rt
            .install(&pkg, Some(vec!["fs.write:/downloads/**".into()]))
            .expect("install legacy canvas package");
        assert!(
            !info.tools.iter().any(|t| t == "canvas.path"),
            "phantom manifest tool must not appear in module.list: {:?}",
            info.tools
        );
        assert!(
            !info.tools.iter().any(|t| t == "canvas.set_style"),
            "legacy wasm must not advertise set_style either: {:?}",
            info.tools
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn pre_pr41_canvas_wasm_rejects_set_style() {
        struct CanvasHost;
        impl HostServices for CanvasHost {
            fn call(
                &self,
                _ctx: &HostCallCtx,
                service: &str,
                _args: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                Err(format!("unexpected host_call: {service}"))
            }
        }

        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output = std::process::Command::new("git")
            .args(["show", "28e44cc:share/modules/canvas.aospkg/module.wasm"])
            .current_dir(&repo)
            .output()
            .expect("git show legacy canvas.wasm");
        if !output.status.success() {
            eprintln!("skip pre_pr41 test: legacy wasm unavailable");
            return;
        }
        let old_wasm_bytes = output.stdout;
        let old_hash = sha256_hex(&old_wasm_bytes);
        assert_eq!(
            old_hash,
            "7946b3227ab76bdeda8633c14f69c82315452c79cdae525f2ebde3543136251f"
        );

        let base = tmpbase("canvas-legacy");
        let pkg = base.join("pkg");
        std::fs::create_dir_all(pkg.join("ui")).unwrap();
        std::fs::write(pkg.join("module.wasm"), &old_wasm_bytes).unwrap();
        let manifest = format!(
            r#"name: canvas
version: 1.0.0
hash: {old_hash}
permissions:
  required_caps:
    - fs.write:/downloads/**
tools:
  - name: canvas.set_style
    description: set pen
    input_schema:
      type: object
      properties:
        session_id: {{ type: string }}
        color: {{ type: string }}
      required: [session_id]
min_os_api: 1
"#
        );
        std::fs::write(pkg.join("manifest.yaml"), manifest).unwrap();

        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(CanvasHost)).unwrap();
        rt.install(&pkg, Some(vec!["fs.write:/downloads/**".into()]))
            .expect("install legacy canvas package");

        let err = rt
            .invoke(
                "canvas",
                "canvas.set_style",
                &serde_json::json!({"session_id": "sess-test", "color": "#F40009"}),
                "human:ui",
                &[],
                "t-legacy",
            )
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("Unknown tool") || err.contains("outil inconnu"),
            "legacy wasm must not expose set_style via module.list/invoke: {err}"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn reload_installed_refreshes_verified_tools_after_wasm_swap() {
        struct CanvasHost;
        impl HostServices for CanvasHost {
            fn call(
                &self,
                _ctx: &HostCallCtx,
                _service: &str,
                _args: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                Ok(serde_json::json!({}))
            }
        }

        let share_pkg =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../share/modules/canvas.aospkg");
        if !share_pkg.join("module.wasm").is_file() {
            eprintln!("skip reload test: packaged canvas missing");
            return;
        }

        let base = tmpbase("canvas-reload-path");
        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(CanvasHost)).unwrap();
        let info = rt
            .install(&share_pkg, Some(vec!["fs.write:/downloads/**".into()]))
            .expect("install packaged canvas");
        assert!(
            info.tools.iter().any(|t| t == "canvas.path"),
            "initial install must export canvas.path: {:?}",
            info.tools
        );

        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output = std::process::Command::new("git")
            .args(["show", "28e44cc:share/modules/canvas.aospkg/module.wasm"])
            .current_dir(&repo)
            .output()
            .expect("git show legacy canvas.wasm");
        if !output.status.success() {
            eprintln!("skip reload test: legacy wasm unavailable");
            let _ = std::fs::remove_dir_all(&base);
            return;
        }

        let dest = base.join("modules/canvas");
        std::fs::write(dest.join("module.wasm"), &output.stdout).unwrap();
        let old_hash = sha256_hex(&output.stdout);
        let manifest = format!(
            "name: canvas\nversion: 1.0.0\nhash: {old_hash}\npermissions:\n  required_caps:\n    - fs.write:/downloads/**\ntools:\n  - name: canvas.path\n    description: path\n    input_schema:\n      type: object\n      properties:\n        session_id: {{ type: string }}\n        points: {{ type: array }}\n      required: [session_id, points]\nmin_os_api: 1\n"
        );
        std::fs::write(dest.join("manifest.yaml"), manifest).unwrap();

        let reloaded = rt.reload_installed("canvas").expect("reload canvas");
        assert!(
            !reloaded.tools.iter().any(|t| t == "canvas.path"),
            "legacy wasm on disk must drop canvas.path after reload: {:?}",
            reloaded.tools
        );

        copy_dir_all(&share_pkg, &dest);
        let reloaded = rt.reload_installed("canvas").expect("reload canvas again");
        assert!(
            reloaded.tools.iter().any(|t| t == "canvas.path"),
            "packaged wasm on disk must restore canvas.path after reload: {:?}",
            reloaded.tools
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn probe_args_for_path_includes_three_points() {
        let args = probe_args_for_tool("canvas.path");
        assert_eq!(args["session_id"], "__probe__");
        assert!(args["points"].as_array().is_some_and(|p| p.len() >= 3));
    }

    #[test]
    fn probe_args_for_notes_create_includes_title() {
        let args = probe_args_for_tool("notes.create");
        assert_eq!(args["title"], "");
        assert!(args.get("content").is_some());
    }

    /// Packaged notes WASM must export all manifest tools (issue #111).
    #[test]
    fn packaged_notes_wasm_exports_create_and_list() {
        struct NotesHost;
        impl HostServices for NotesHost {
            fn call(
                &self,
                _ctx: &HostCallCtx,
                service: &str,
                _args: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                match service {
                    "fs.read" => Ok(serde_json::json!({"content": ""})),
                    "fs.write" => Ok(serde_json::json!({"version": 1u64})),
                    "fs.list" => Ok(serde_json::json!({"entries": []})),
                    "mem.episodic_write" => Ok(serde_json::json!({"id": 1u64})),
                    "mem.episodic_query" => Ok(serde_json::json!({"hits": []})),
                    "mem.episodic_delete" => Ok(serde_json::json!({"count": 0})),
                    other => Err(format!("unexpected host_call: {other}")),
                }
            }
        }

        let share_pkg =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../share/modules/notes.aospkg");
        if !share_pkg.join("module.wasm").is_file() {
            eprintln!("skip packaged notes test: wasm missing");
            return;
        }

        let base = tmpbase("notes-packaged");
        let caps = vec![
            "fs.read:/documents/notes/**".into(),
            "fs.write:/documents/notes/**".into(),
            "mem.write:module:notes".into(),
            "mem.query:module:notes".into(),
        ];
        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(NotesHost)).unwrap();
        let info = rt.install(&share_pkg, Some(caps)).expect("install notes");
        for tool in [
            "notes.create",
            "notes.list",
            "notes.read",
            "notes.search",
            "notes.update",
            "notes.links",
            "notes.related",
        ] {
            assert!(
                info.tools.iter().any(|t| t == tool),
                "packaged notes must verify {tool}: {:?}",
                info.tools
            );
        }

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn rejects_ui_contract_above_host_max_at_install() {
        let base = tmpbase("contract-reject");
        let pkg = base.join("pkg");
        make_package(&pkg);
        let mut manifest = std::fs::read_to_string(pkg.join("manifest.yaml")).unwrap();
        manifest = manifest
            .replace("name: echo-test", "name: rich-v9")
            .replace("ui: ~\n", "");
        manifest.push_str(
            "ui:\n  contract: 9\n  document: ui/index.json\n  entry: ui/index.json\n  mode: declarative_ui\nservices:\n  jobs: 1\n",
        );
        std::fs::write(pkg.join("manifest.yaml"), manifest).unwrap();
        std::fs::create_dir_all(pkg.join("ui")).unwrap();
        std::fs::write(
            pkg.join("ui/index.json"),
            br#"{"type":"declarative_ui","contract":9,"title":"X","root":{"kind":"column","children":[]}}"#,
        )
        .unwrap();
        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(EchoServices)).unwrap();
        let err = rt.install(&pkg, Some(vec![])).unwrap_err();
        assert!(matches!(err, ModuleError::UiContractUnsupported { .. }));
        let _ = std::fs::remove_dir_all(&base);
    }

    fn tasks_test_caps() -> Vec<String> {
        vec![
            "fs.read:/documents/tasks/**".into(),
            "fs.write:/documents/tasks/**".into(),
        ]
    }

    #[test]
    fn tasks_package_validates_at_install() {
        let share =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../share/modules/tasks.aospkg");
        if !share.join("module.wasm").is_file() {
            eprintln!("skip tasks test: run modules/build-tasks.sh first");
            return;
        }
        let base = tmpbase("tasks");
        let caps = tasks_test_caps();
        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(EchoServices)).unwrap();
        let info = rt.install(&share, Some(caps)).expect("tasks install");
        assert_eq!(info.name, "tasks");
        let ui = rt.load_ui("tasks").expect("load tasks ui");
        assert_eq!(ui.document.doc_type, "declarative_ui");
        assert_eq!(ui.document.root.kind, "column");
        assert_eq!(ui.document.chrome_title("fr"), "Tâches");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn create_package_validates_at_install() {
        let share =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../share/modules/create.aospkg");
        if !share.join("module.wasm").is_file() {
            eprintln!("skip create test: run modules/build-create.sh first");
            return;
        }
        let base = tmpbase("create");
        let caps = create_test_caps();
        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(EchoServices)).unwrap();
        let info = rt.install(&share, Some(caps)).expect("create install");
        assert_eq!(info.name, "create");
        let ui = rt.load_ui("create").expect("load ui");
        assert_eq!(ui.document.contract, Some(UI_CONTRACT_V2));
        assert!(ui.document.uses_media_image_service());
        assert!(!ui.document.subscriptions.is_empty());
        assert_eq!(ui.document.catalogue_title(), "Create");
        let docs = base.join("var/storage/data/documents/create");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(docs.join("history.json"), br#"{"items":[]}"#).unwrap();
        rt.uninstall("create").unwrap();
        assert!(!base.join("modules/create").exists());
        assert!(
            docs.join("history.json").is_file(),
            "uninstall must keep /documents/create/**"
        );
        assert!(rt.user_removed("create"));
        assert!(rt
            .install_preinstalled(&share, Some(create_test_caps()))
            .unwrap()
            .is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    fn create_test_caps() -> Vec<String> {
        vec![
            "fs.read:/documents/create/**".into(),
            "fs.write:/documents/create/**".into(),
            "fs.read:/downloads/**".into(),
            "fs.write:/downloads/**".into(),
            "media.generate".into(),
            "tool.invoke:create".into(),
        ]
    }

    #[test]
    fn create_invalid_upgrade_keeps_last_good_version() {
        let share =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../share/modules/create.aospkg");
        if !share.join("module.wasm").is_file() {
            eprintln!("skip create rollback test: run modules/build-create.sh first");
            return;
        }
        let base = tmpbase("create-rollback");
        let caps = create_test_caps();
        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(EchoServices)).unwrap();
        rt.install(&share, Some(caps.clone()))
            .expect("create install");
        let good_wasm = std::fs::read(base.join("modules/create/module.wasm")).unwrap();
        let bad_pkg = base.join("bad-create");
        copy_dir(&share, &bad_pkg).unwrap();
        let manifest = std::fs::read_to_string(bad_pkg.join("manifest.yaml")).unwrap();
        std::fs::write(
            bad_pkg.join("manifest.yaml"),
            manifest.replace("version: 1.0.0", "version: 9.9.9"),
        )
        .unwrap();
        std::fs::write(bad_pkg.join("module.wasm"), b"not valid wasm").unwrap();
        let err = rt.install(&bad_pkg, Some(caps)).unwrap_err();
        assert!(
            matches!(
                err,
                ModuleError::Trap(_)
                    | ModuleError::BadManifest(_)
                    | ModuleError::HashMismatch
                    | ModuleError::PackageIntegrity(_)
            ),
            "unexpected err: {err}"
        );
        let kept = std::fs::read(base.join("modules/create/module.wasm")).unwrap();
        assert_eq!(kept, good_wasm);
        assert!(rt.load_ui("create").is_ok());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn create_rejects_unsupported_ui_contract_before_replace() {
        let share =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../share/modules/create.aospkg");
        if !share.join("module.wasm").is_file() {
            eprintln!("skip create contract test: run modules/build-create.sh first");
            return;
        }
        let base = tmpbase("create-contract");
        let caps = create_test_caps();
        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(EchoServices)).unwrap();
        rt.install(&share, Some(caps.clone()))
            .expect("create install");
        let good_wasm = std::fs::read(base.join("modules/create/module.wasm")).unwrap();
        let bad_pkg = base.join("bad-contract");
        copy_dir(&share, &bad_pkg).unwrap();
        let manifest = std::fs::read_to_string(bad_pkg.join("manifest.yaml")).unwrap();
        let manifest = manifest
            .replace("contract: 2", "contract: 9")
            .replace("version: 1.0.0", "version: 2.0.0");
        std::fs::write(bad_pkg.join("manifest.yaml"), manifest).unwrap();
        std::fs::write(
            bad_pkg.join("ui/index.json"),
            br#"{"type":"declarative_ui","contract":9,"title":"Create","root":{"kind":"column","children":[]}}"#,
        )
        .unwrap();
        let err = rt.install(&bad_pkg, Some(caps)).unwrap_err();
        assert!(matches!(err, ModuleError::UiContractUnsupported { .. }));
        let kept = std::fs::read(base.join("modules/create/module.wasm")).unwrap();
        assert_eq!(kept, good_wasm);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn create_probe_verification_does_not_write_user_documents() {
        struct TrackingHost {
            writes: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        }
        impl HostServices for TrackingHost {
            fn call(
                &self,
                _ctx: &HostCallCtx,
                service: &str,
                args: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                if service == "fs.write" {
                    if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                        self.writes.lock().unwrap().push(path.to_string());
                    }
                    return Ok(serde_json::json!({"version": 1u64}));
                }
                if service == "fs.read" {
                    return Ok(serde_json::json!({"content": "{\"items\":[]}"}));
                }
                Err(format!("unexpected service: {service}"))
            }
        }

        let share_pkg =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../share/modules/create.aospkg");
        if !share_pkg.join("module.wasm").is_file() {
            eprintln!("skip create probe test: create wasm missing");
            return;
        }

        let writes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let base = tmpbase("create-probe");
        let mut rt = ModuleRuntime::open(
            base.join("modules"),
            Arc::new(TrackingHost {
                writes: writes.clone(),
            }),
        )
        .unwrap();
        rt.install(&share_pkg, Some(create_test_caps()))
            .expect("install create");
        assert!(
            writes.lock().unwrap().is_empty(),
            "create tool probes must not fs.write user documents: {:?}",
            writes.lock().unwrap()
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn gallery_demo_package_validates_at_install() {
        let share = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../share/modules/gallery-demo.aospkg");
        if !share.join("module.wasm").is_file() {
            eprintln!("skip gallery-demo test: run modules/build-gallery-demo.sh first");
            return;
        }
        let base = tmpbase("gallery-demo");
        let caps = vec![
            "fs.read:/documents/gallery-demo/**".into(),
            "fs.write:/documents/gallery-demo/**".into(),
            "tool.invoke:gallery-demo".into(),
        ];
        let mut rt = ModuleRuntime::open(base.join("modules"), Arc::new(EchoServices)).unwrap();
        let info = rt
            .install(&share, Some(caps))
            .expect("gallery-demo install");
        assert_eq!(info.name, "gallery-demo");
        let ui = rt.load_ui("gallery-demo").expect("load ui");
        assert_eq!(ui.document.contract, Some(UI_CONTRACT_V2));
        assert_eq!(ui.document.catalogue_title(), "Gallery Demo");
        assert!(!ui.document.subscriptions.is_empty());
        rt.uninstall("gallery-demo").unwrap();
        let _ = std::fs::remove_dir_all(&base);
    }
}

fn module_info_from_installed(
    manifest: &ModuleManifest,
    verified_tools: &[String],
    granted_caps: Vec<String>,
    quarantined: bool,
    dir: Option<&Path>,
) -> ModuleInfo {
    let (ui_mode, ui_title) = ui_meta_from_manifest(manifest, dir);
    ModuleInfo {
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        granted_caps,
        tools: verified_tools.to_vec(),
        quarantined,
        ui_mode,
        ui_title,
    }
}

fn ui_meta_from_manifest(
    manifest: &ModuleManifest,
    dir: Option<&Path>,
) -> (Option<String>, Option<String>) {
    let Some(ui) = manifest.ui.as_ref() else {
        return (None, None);
    };
    let mode = Some(ui.mode.clone());
    let mut title = Some(manifest.name.clone());
    if ui.mode == "declarative_ui" {
        if let Some(d) = dir {
            let path = d.join(ui.document_path());
            if let Ok(raw) = std::fs::read(&path) {
                let contract = ui.contract_version();
                if let Ok(doc) = DeclUiDocument::parse_json_with_contract(&raw, contract) {
                    title = Some(doc.catalogue_title());
                } else if let Ok(doc) = DeclUiDocument::parse_json(&raw) {
                    title = Some(doc.catalogue_title());
                }
            }
        }
    }
    (mode, title)
}
