//! Signed module / skill / MCP catalogue (E10).
//!
//! Bundled index: `share/modules/catalogue.yaml` + ed25519 sidecars.
//! Opt-in extra source: a Git-hosted signed index (same format). Authenticity
//! is the pinned Preview key, not HTTPS. Tampering refuses install.

use aos_proto::{CatalogueEntry, ModuleCatalogue};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Seed Preview uniquement — sert à régénérer `.sig` / `.pub` en test.
/// Ce n'est pas une PKI de production.
pub const PREVIEW_CATALOGUE_SEED: [u8; 32] = *b"akasha-os-preview-catalogue-v01!";

/// Default raw index for the opt-in community tree in this repository.
pub const DEFAULT_COMMUNITY_CATALOGUE_URL: &str =
    "https://raw.githubusercontent.com/azerothl/akasha-os/main/community/catalogue.yaml";

const SOURCE_BUNDLED: &str = "bundled";
const SOURCE_COMMUNITY: &str = "community";
const INDEX_MAX_BYTES: u64 = 512 * 1024;
const PACKAGE_MAX_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum CatalogueError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml: {0}")]
    Yaml(String),
    #[error("signature catalogue invalide")]
    BadSignature,
    #[error("clé publique catalogue invalide")]
    BadPublicKey,
    #[error("hash catalogue non conforme pour {0}")]
    HashMismatch(String),
    #[error("source extra désactivée")]
    ExtraDisabled,
    #[error("entrée catalogue inconnue: {0}")]
    NotFound(String),
    #[error("fetch: {0}")]
    Fetch(String),
}

#[derive(Debug, Deserialize)]
struct CatalogueFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    entries: Vec<CatalogueEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExtraSourceFile {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    url: String,
}

/// Catalogue vérifié (signature + lookups).
#[derive(Debug, Clone)]
pub struct SignedCatalogue {
    pub inner: ModuleCatalogue,
    path: PathBuf,
}

impl SignedCatalogue {
    /// Charge `catalogue.yaml` + `.sig` + `.pub` à côté.
    pub fn load(yaml_path: impl AsRef<Path>) -> Result<Self, CatalogueError> {
        let yaml_path = yaml_path.as_ref().to_path_buf();
        let bytes = std::fs::read(&yaml_path)?;
        let sig_path = sidecar(&yaml_path, "sig");
        let pub_path = yaml_path.with_file_name("catalogue.pub");
        if sig_path.exists() && pub_path.exists() {
            let pk = parse_pub(&std::fs::read_to_string(&pub_path)?)?;
            let sig = std::fs::read_to_string(&sig_path)?;
            return Self::from_signed_bytes(&bytes, sig.trim(), &pk, yaml_path);
        }
        Ok(Self {
            inner: parse_catalogue_file(&bytes, false)?,
            path: yaml_path,
        })
    }

    /// Verify YAML bytes with an explicit public key (pinned extra source).
    pub fn from_signed_bytes(
        yaml: &[u8],
        sig_hex: &str,
        pk: &VerifyingKey,
        path: PathBuf,
    ) -> Result<Self, CatalogueError> {
        let sig = parse_sig(sig_hex)?;
        pk.verify(yaml, &sig)
            .map_err(|_| CatalogueError::BadSignature)?;
        Ok(Self {
            inner: parse_catalogue_file(yaml, true)?,
            path,
        })
    }

    pub fn proto(&self) -> &ModuleCatalogue {
        &self.inner
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn entry(&self, name: &str) -> Option<&CatalogueEntry> {
        self.inner.entries.iter().find(|e| e.name == name)
    }

    /// Si l'entrée est au catalogue, le hash doit correspondre.
    /// Absente → Ok (package hors registre, revue de caps inchangée).
    pub fn check_entry_hash(
        &self,
        name: &str,
        kind: Option<&str>,
        file_hash_hex: &str,
    ) -> Result<(), CatalogueError> {
        let Some(entry) = self.inner.entries.iter().find(|e| {
            e.name == name && kind.map(|k| e.kind == k).unwrap_or(true)
        }) else {
            return Ok(());
        };
        if !self.inner.signature_ok {
            return Err(CatalogueError::BadSignature);
        }
        if hashes_equal(&entry.hash, file_hash_hex) {
            Ok(())
        } else {
            Err(CatalogueError::HashMismatch(name.into()))
        }
    }

    /// Si le module est au catalogue, le hash WASM doit correspondre.
    /// Absent du catalogue → Ok (package hors registre, revue de caps inchangée).
    pub fn check_module_hash(&self, name: &str, wasm_hash_hex: &str) -> Result<(), CatalogueError> {
        self.check_entry_hash(name, Some("module"), wasm_hash_hex)
    }
}

fn parse_catalogue_file(bytes: &[u8], signature_ok: bool) -> Result<ModuleCatalogue, CatalogueError> {
    let file: CatalogueFile =
        serde_yaml::from_slice(bytes).map_err(|e| CatalogueError::Yaml(e.to_string()))?;
    Ok(ModuleCatalogue {
        version: if file.version == 0 { 1 } else { file.version },
        entries: file.entries,
        signature_ok,
        extra_enabled: false,
        extra_signature_ok: false,
        extra_cached: false,
        extra_error: String::new(),
        extra_url: String::new(),
    })
}

/// Opt-in extra catalogue (Git-hosted signed index, cached offline).
#[derive(Debug, Clone)]
pub struct ExtraCatalogueSource {
    pub enabled: bool,
    pub url: String,
    pub cache_dir: PathBuf,
    pub home: PathBuf,
    pub loaded: Option<SignedCatalogue>,
    pub cached: bool,
    pub last_error: String,
}

impl ExtraCatalogueSource {
    pub fn open(home: impl AsRef<Path>, cache_dir: impl AsRef<Path>) -> Self {
        let home = home.as_ref().to_path_buf();
        let cache_dir = if cache_dir.as_ref().is_absolute() {
            cache_dir.as_ref().to_path_buf()
        } else {
            home.join(cache_dir.as_ref())
        };
        let meta = read_source_file(&cache_dir.join("source.yaml"));
        Self {
            enabled: meta.enabled,
            url: if meta.url.is_empty() {
                DEFAULT_COMMUNITY_CATALOGUE_URL.into()
            } else {
                meta.url
            },
            cache_dir,
            home,
            loaded: None,
            cached: false,
            last_error: String::new(),
        }
    }

    pub fn persist(&self) -> Result<(), CatalogueError> {
        std::fs::create_dir_all(&self.cache_dir)?;
        let body = ExtraSourceFile {
            enabled: self.enabled,
            url: self.url.clone(),
        };
        let yaml = serde_yaml::to_string(&body).map_err(|e| CatalogueError::Yaml(e.to_string()))?;
        std::fs::write(self.cache_dir.join("source.yaml"), yaml)?;
        Ok(())
    }

    pub fn set_enabled(&mut self, enabled: bool) -> Result<(), CatalogueError> {
        self.enabled = enabled;
        if !enabled {
            self.loaded = None;
            self.cached = false;
            self.last_error.clear();
        }
        self.persist()?;
        if enabled {
            self.load_offline();
        }
        Ok(())
    }

    pub fn set_url(&mut self, url: String) -> Result<(), CatalogueError> {
        if !url.trim().is_empty() {
            self.url = url.trim().to_string();
        }
        self.persist()
    }

    /// Load cache or a local `community/catalogue.yaml` — never hits the network.
    pub fn load_offline(&mut self) {
        self.last_error.clear();
        self.loaded = None;
        self.cached = false;
        if !self.enabled {
            return;
        }
        match self.try_load_offline() {
            Ok(()) => {}
            Err(e) => {
                self.last_error = e.to_string();
                self.loaded = None;
            }
        }
    }

    fn try_load_offline(&mut self) -> Result<(), CatalogueError> {
        if let Some(local) = local_community_yaml(&self.home) {
            let cat = load_with_pinned_key(&local)?;
            self.loaded = Some(tag_loaded(cat, SOURCE_COMMUNITY));
            self.cached = true;
            return Ok(());
        }
        let cached = self.cache_dir.join("catalogue.yaml");
        if cached.is_file() {
            let cat = load_with_pinned_key(&cached)?;
            self.loaded = Some(tag_loaded(cat, SOURCE_COMMUNITY));
            self.cached = true;
            return Ok(());
        }
        Ok(())
    }

    /// Fetch (or copy local tree), verify with the pinned key, write cache.
    pub fn refresh(&mut self) -> Result<(), CatalogueError> {
        self.refresh_with(fetch_bytes)
    }

    pub fn refresh_with(
        &mut self,
        fetch: impl Fn(&str) -> Result<Vec<u8>, CatalogueError>,
    ) -> Result<(), CatalogueError> {
        if !self.enabled {
            return Err(CatalogueError::ExtraDisabled);
        }
        self.last_error.clear();
        match self.try_refresh(&fetch) {
            Ok(()) => Ok(()),
            Err(e) => {
                self.last_error = e.to_string();
                // Keep a previously verified cache so listing still works offline
                // after a failed refresh — unless the new bytes were tampered.
                if matches!(e, CatalogueError::BadSignature | CatalogueError::HashMismatch(_))
                {
                    self.loaded = None;
                    self.cached = false;
                } else if self.loaded.is_none() {
                    self.load_offline();
                }
                Err(e)
            }
        }
    }

    fn try_refresh(
        &mut self,
        fetch: &impl Fn(&str) -> Result<Vec<u8>, CatalogueError>,
    ) -> Result<(), CatalogueError> {
        let (yaml, sig) = if let Some(local) = local_community_yaml(&self.home) {
            let yaml = std::fs::read(&local)?;
            let sig = std::fs::read_to_string(sidecar(&local, "sig"))?;
            (yaml, sig)
        } else {
            let yaml = fetch(&self.url)?;
            let sig = String::from_utf8(fetch(&format!("{}.sig", self.url.trim()))?)
                .map_err(|e| CatalogueError::Fetch(e.to_string()))?;
            (yaml, sig)
        };
        let pk = preview_verifying_key();
        let dest = self.cache_dir.join("catalogue.yaml");
        std::fs::create_dir_all(&self.cache_dir)?;
        let cat = SignedCatalogue::from_signed_bytes(&yaml, sig.trim(), &pk, dest.clone())?;
        std::fs::write(&dest, &yaml)?;
        std::fs::write(sidecar(&dest, "sig"), sig.trim())?;
        self.loaded = Some(tag_loaded(cat, SOURCE_COMMUNITY));
        self.cached = true;
        Ok(())
    }

    pub fn proto_extra(&self) -> Option<ModuleCatalogue> {
        self.loaded.as_ref().map(|c| c.proto().clone())
    }
}

fn tag_loaded(mut cat: SignedCatalogue, source: &str) -> SignedCatalogue {
    for e in &mut cat.inner.entries {
        if e.source.is_empty() {
            e.source = source.into();
        }
    }
    cat
}

fn read_source_file(path: &Path) -> ExtraSourceFile {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return ExtraSourceFile {
            enabled: false,
            url: String::new(),
        };
    };
    serde_yaml::from_str(&raw).unwrap_or(ExtraSourceFile {
        enabled: false,
        url: String::new(),
    })
}

/// Local clone: `{AOS_HOME}/community/catalogue.yaml`.
pub fn local_community_yaml(home: &Path) -> Option<PathBuf> {
    let p = home.join("community/catalogue.yaml");
    p.is_file().then_some(p)
}

pub fn load_with_pinned_key(yaml_path: &Path) -> Result<SignedCatalogue, CatalogueError> {
    let bytes = std::fs::read(yaml_path)?;
    let sig_path = sidecar(yaml_path, "sig");
    if !sig_path.is_file() {
        return Err(CatalogueError::BadSignature);
    }
    let sig = std::fs::read_to_string(sig_path)?;
    SignedCatalogue::from_signed_bytes(&bytes, sig.trim(), &preview_verifying_key(), yaml_path.to_path_buf())
}

pub fn preview_verifying_key() -> VerifyingKey {
    SigningKey::from_bytes(&PREVIEW_CATALOGUE_SEED).verifying_key()
}

/// Merge bundled + extra. Bundled entries win on name collision.
pub fn merge_catalogues(
    bundled: Option<&ModuleCatalogue>,
    extra: &ExtraCatalogueSource,
) -> ModuleCatalogue {
    let mut out = bundled.cloned().unwrap_or(ModuleCatalogue {
        version: 1,
        entries: vec![],
        signature_ok: false,
        extra_enabled: false,
        extra_signature_ok: false,
        extra_cached: false,
        extra_error: String::new(),
        extra_url: String::new(),
    });
    for e in &mut out.entries {
        if e.source.is_empty() {
            e.source = SOURCE_BUNDLED.into();
        }
    }
    let bundled_names: std::collections::HashSet<String> =
        out.entries.iter().map(|e| e.name.clone()).collect();
    out.extra_enabled = extra.enabled;
    out.extra_url = extra.url.clone();
    out.extra_cached = extra.cached;
    out.extra_error = extra.last_error.clone();
    if let Some(ext) = extra.proto_extra() {
        out.extra_signature_ok = ext.signature_ok;
        if extra.enabled && ext.signature_ok {
            for e in ext.entries {
                if !bundled_names.contains(&e.name) {
                    out.entries.push(e);
                }
            }
        }
    } else {
        out.extra_signature_ok = false;
    }
    out
}

pub fn check_hash_in(
    bundled: Option<&SignedCatalogue>,
    extra: Option<&SignedCatalogue>,
    name: &str,
    kind: &str,
    hash_hex: &str,
) -> Result<(), CatalogueError> {
    if let Some(cat) = bundled {
        if cat.entry(name).is_some() {
            return cat.check_entry_hash(name, Some(kind), hash_hex);
        }
    }
    if let Some(cat) = extra {
        if cat.entry(name).is_some() {
            return cat.check_entry_hash(name, Some(kind), hash_hex);
        }
    }
    Ok(())
}

/// Find an entry in bundled or extra (extra only when enabled + signed).
pub fn find_entry<'a>(
    bundled: Option<&'a SignedCatalogue>,
    extra: &'a ExtraCatalogueSource,
    name: &str,
) -> Result<&'a CatalogueEntry, CatalogueError> {
    if let Some(cat) = bundled {
        if let Some(e) = cat.entry(name) {
            return Ok(e);
        }
    }
    if extra.enabled {
        if let Some(cat) = &extra.loaded {
            if !cat.inner.signature_ok {
                return Err(CatalogueError::BadSignature);
            }
            if let Some(e) = cat.entry(name) {
                return Ok(e);
            }
        }
    }
    Err(CatalogueError::NotFound(name.into()))
}

#[derive(Debug, Clone)]
pub enum ResolvedPackage {
    Skill { bytes: Vec<u8> },
    ModuleDir { path: PathBuf },
    File { path: PathBuf },
}

/// Resolve a listed package from the local tree, cache, or a fetch.
pub fn resolve_package(
    entry: &CatalogueEntry,
    extra: &ExtraCatalogueSource,
    home: &Path,
    fetch: impl Fn(&str) -> Result<Vec<u8>, CatalogueError>,
) -> Result<ResolvedPackage, CatalogueError> {
    match entry.kind.as_str() {
        "skill" => {
            let bytes = resolve_file_bytes(entry, extra, home, &fetch)?;
            let hash = sha256_hex(&bytes);
            if !hashes_equal(&entry.hash, &hash) {
                return Err(CatalogueError::HashMismatch(entry.name.clone()));
            }
            Ok(ResolvedPackage::Skill { bytes })
        }
        "module" => {
            let dir = resolve_module_dir(entry, extra, home, &fetch)?;
            let wasm = std::fs::read(dir.join("module.wasm"))?;
            let hash = sha256_hex(&wasm);
            if !hashes_equal(&entry.hash, &hash) {
                return Err(CatalogueError::HashMismatch(entry.name.clone()));
            }
            Ok(ResolvedPackage::ModuleDir { path: dir })
        }
        _ => {
            let bytes = resolve_file_bytes(entry, extra, home, &fetch)?;
            let hash = sha256_hex(&bytes);
            if !hashes_equal(&entry.hash, &hash) {
                return Err(CatalogueError::HashMismatch(entry.name.clone()));
            }
            let dest = extra
                .cache_dir
                .join("packages")
                .join(&entry.name)
                .join(file_name_of(&entry.path));
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&dest, bytes)?;
            Ok(ResolvedPackage::File { path: dest })
        }
    }
}

fn resolve_file_bytes(
    entry: &CatalogueEntry,
    extra: &ExtraCatalogueSource,
    home: &Path,
    fetch: &impl Fn(&str) -> Result<Vec<u8>, CatalogueError>,
) -> Result<Vec<u8>, CatalogueError> {
    let local = home.join(&entry.path);
    if local.is_file() {
        return Ok(std::fs::read(local)?);
    }
    let cached = extra
        .cache_dir
        .join("packages")
        .join(&entry.name)
        .join(file_name_of(&entry.path));
    if cached.is_file() {
        return Ok(std::fs::read(cached)?);
    }
    let url = package_url(&extra.url, &entry.path);
    let bytes = fetch(&url)?;
    if let Some(parent) = cached.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&cached, &bytes)?;
    Ok(bytes)
}

fn resolve_module_dir(
    entry: &CatalogueEntry,
    extra: &ExtraCatalogueSource,
    home: &Path,
    fetch: &impl Fn(&str) -> Result<Vec<u8>, CatalogueError>,
) -> Result<PathBuf, CatalogueError> {
    let local = home.join(&entry.path);
    if local.is_dir() && local.join("module.wasm").is_file() {
        return Ok(local);
    }
    let dest = extra.cache_dir.join("packages").join(&entry.name);
    if dest.join("module.wasm").is_file() && dest.join("manifest.yaml").is_file() {
        return Ok(dest);
    }
    std::fs::create_dir_all(&dest)?;
    let base = package_url(&extra.url, &entry.path);
    let manifest = fetch(&format!("{}/manifest.yaml", base.trim_end_matches('/')))?;
    let wasm = fetch(&format!("{}/module.wasm", base.trim_end_matches('/')))?;
    std::fs::write(dest.join("manifest.yaml"), manifest)?;
    std::fs::write(dest.join("module.wasm"), wasm)?;
    Ok(dest)
}

/// Index URL `…/community/catalogue.yaml` + path `community/skills/…` → raw file URL.
pub fn package_url(index_url: &str, path: &str) -> String {
    let path = path.replace('\\', "/").trim_start_matches('/').to_string();
    if path.starts_with("http://") || path.starts_with("https://") {
        return path;
    }
    let trimmed = index_url.trim();
    let without_file = trimmed
        .trim_end_matches("catalogue.yaml")
        .trim_end_matches('/');
    let repo_root = without_file
        .trim_end_matches("community")
        .trim_end_matches('/');
    if repo_root.is_empty() {
        return path;
    }
    format!("{repo_root}/{path}")
}

fn file_name_of(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "payload".into())
}

fn sidecar(yaml: &Path, ext: &str) -> PathBuf {
    let mut p = yaml.as_os_str().to_os_string();
    p.push(".");
    p.push(ext);
    PathBuf::from(p)
}

fn hashes_equal(attested: &str, computed_hex: &str) -> bool {
    let a = attested.trim().trim_start_matches("sha256:");
    let b = computed_hex.trim().trim_start_matches("sha256:");
    a.eq_ignore_ascii_case(b)
}

fn parse_pub(s: &str) -> Result<VerifyingKey, CatalogueError> {
    let raw = hex_decode(s.trim()).ok_or(CatalogueError::BadPublicKey)?;
    let arr: [u8; 32] = raw.try_into().map_err(|_| CatalogueError::BadPublicKey)?;
    VerifyingKey::from_bytes(&arr).map_err(|_| CatalogueError::BadPublicKey)
}

fn parse_sig(s: &str) -> Result<Signature, CatalogueError> {
    let raw = hex_decode(s.trim()).ok_or(CatalogueError::BadSignature)?;
    let arr: [u8; 64] = raw.try_into().map_err(|_| CatalogueError::BadSignature)?;
    Ok(Signature::from_bytes(&arr))
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let d = Sha256::digest(bytes);
    d.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Signe `catalogue.yaml` avec la seed Preview (tests / régénération).
pub fn sign_preview_catalogue(yaml_bytes: &[u8]) -> (String, String) {
    let sk = SigningKey::from_bytes(&PREVIEW_CATALOGUE_SEED);
    let sig = sk.sign(yaml_bytes);
    (
        hex_encode(sk.verifying_key().as_bytes()),
        hex_encode(&sig.to_bytes()),
    )
}

pub fn fetch_bytes(url: &str) -> Result<Vec<u8>, CatalogueError> {
    if let Some(path) = url.strip_prefix("file://") {
        return std::fs::read(path).map_err(CatalogueError::from);
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Akasha-OS-Preview-catalogue/1")
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
        .map_err(|e| CatalogueError::Fetch(e.to_string()))?;
    let resp = client
        .get(url)
        .send()
        .map_err(|e| CatalogueError::Fetch(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(CatalogueError::Fetch(format!("status {}", resp.status())));
    }
    let limit = if url.ends_with("catalogue.yaml") || url.ends_with("catalogue.yaml.sig") {
        INDEX_MAX_BYTES
    } else {
        PACKAGE_MAX_BYTES
    };
    if let Some(len) = resp.content_length() {
        if len > limit {
            return Err(CatalogueError::Fetch("payload too large".into()));
        }
    }
    let buf = resp.bytes().map_err(|e| CatalogueError::Fetch(e.to_string()))?;
    if buf.len() as u64 > limit {
        return Err(CatalogueError::Fetch("payload too large".into()));
    }
    Ok(buf.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aos-cat-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_signed(dir: &Path, yaml: &[u8]) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let yaml_path = dir.join("catalogue.yaml");
        std::fs::write(&yaml_path, yaml).unwrap();
        let (pk, sig) = sign_preview_catalogue(yaml);
        std::fs::write(dir.join("catalogue.pub"), pk).unwrap();
        std::fs::write(dir.join("catalogue.yaml.sig"), sig).unwrap();
        yaml_path
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let dir = temp_dir("rt");
        let yaml = b"version: 1\nentries: []\n";
        let yaml_path = write_signed(&dir, yaml);
        let cat = SignedCatalogue::load(&yaml_path).unwrap();
        assert!(cat.inner.signature_ok);
        cat.check_module_hash("unknown", "00").unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mismatch_listed_module() {
        let dir = temp_dir("mm");
        let yaml = b"version: 1\nentries:\n  - name: notes\n    version: \"1\"\n    kind: module\n    path: share/modules/notes.aospkg\n    hash: sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n    attested_caps: []\n";
        let yaml_path = write_signed(&dir, yaml);
        let cat = SignedCatalogue::load(&yaml_path).unwrap();
        assert!(matches!(
            cat.check_module_hash("notes", "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            Err(CatalogueError::HashMismatch(_))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tamper_refuses_pinned_verify() {
        let dir = temp_dir("tamper");
        let yaml = b"version: 1\nentries: []\n";
        let yaml_path = write_signed(&dir, yaml);
        std::fs::write(&yaml_path, b"version: 1\nentries:\n  - name: evil\n    version: \"1\"\n    kind: skill\n    path: x\n    hash: sha256:00\n").unwrap();
        let err = load_with_pinned_key(&yaml_path).unwrap_err();
        assert!(matches!(err, CatalogueError::BadSignature));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merge_bundled_and_community() {
        let dir = temp_dir("merge");
        let bundled_yaml = b"version: 1\nentries:\n  - name: notes\n    version: \"1\"\n    kind: module\n    path: share/modules/notes.aospkg\n    hash: sha256:aa\n    attested_caps: []\n";
        let extra_yaml = b"version: 1\nentries:\n  - name: morning-brief\n    version: \"1\"\n    kind: skill\n    path: community/skills/morning-brief/SKILL.md\n    hash: sha256:bb\n    license: MIT\n    attested_caps: []\n";
        let bundled = SignedCatalogue::load(&write_signed(&dir.join("b"), bundled_yaml)).unwrap();
        let extra_path = write_signed(&dir.join("e"), extra_yaml);
        let mut extra = ExtraCatalogueSource::open(&dir, dir.join("cache"));
        extra.enabled = true;
        extra.loaded = Some(tag_loaded(
            load_with_pinned_key(&extra_path).unwrap(),
            SOURCE_COMMUNITY,
        ));
        extra.cached = true;
        let merged = merge_catalogues(Some(bundled.proto()), &extra);
        assert!(merged.signature_ok);
        assert!(merged.extra_enabled);
        assert!(merged.extra_signature_ok);
        assert_eq!(merged.entries.len(), 2);
        assert_eq!(merged.entries[0].source, SOURCE_BUNDLED);
        assert_eq!(merged.entries[1].name, "morning-brief");
        assert_eq!(merged.entries[1].source, SOURCE_COMMUNITY);
        assert_eq!(merged.entries[1].license, "MIT");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn offline_lists_cached_extra() {
        let dir = temp_dir("off");
        let extra_yaml = b"version: 1\nentries:\n  - name: morning-brief\n    version: \"1\"\n    kind: skill\n    path: community/skills/morning-brief/SKILL.md\n    hash: sha256:bb\n    attested_caps: []\n";
        let cache = dir.join("var/catalogue/community");
        std::fs::create_dir_all(&cache).unwrap();
        let yaml_path = write_signed(&cache, extra_yaml);
        let _ = yaml_path;
        let mut extra = ExtraCatalogueSource::open(&dir, PathBuf::from("var/catalogue/community"));
        extra.enabled = true;
        extra.persist().unwrap();
        extra.load_offline();
        assert!(extra.cached);
        assert!(extra.loaded.as_ref().unwrap().inner.signature_ok);
        let merged = merge_catalogues(None, &extra);
        assert_eq!(merged.entries.len(), 1);
        assert_eq!(merged.entries[0].name, "morning-brief");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refresh_tamper_refuses_and_clears() {
        let dir = temp_dir("ref-tamper");
        let good = b"version: 1\nentries: []\n";
        let remote = dir.join("remote");
        std::fs::create_dir_all(&remote).unwrap();
        write_signed(&remote, good);
        let mut extra = ExtraCatalogueSource::open(&dir, dir.join("cache"));
        extra.enabled = true;
        extra.url = format!("file://{}/catalogue.yaml", remote.display()).replace('\\', "/");
        extra
            .refresh_with(|url| {
                let path = url.strip_prefix("file://").unwrap();
                std::fs::read(path).map_err(CatalogueError::from)
            })
            .unwrap();
        assert!(extra.loaded.is_some());

        std::fs::write(remote.join("catalogue.yaml"), b"version: 1\nentries: []\n# tampered\n")
            .unwrap();
        let err = extra
            .refresh_with(|url| {
                let path = url.strip_prefix("file://").unwrap();
                std::fs::read(path).map_err(CatalogueError::from)
            })
            .unwrap_err();
        assert!(matches!(err, CatalogueError::BadSignature));
        assert!(extra.loaded.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn package_url_from_community_index() {
        let url = package_url(
            DEFAULT_COMMUNITY_CATALOGUE_URL,
            "community/skills/morning-brief/SKILL.md",
        );
        assert_eq!(
            url,
            "https://raw.githubusercontent.com/azerothl/akasha-os/main/community/skills/morning-brief/SKILL.md"
        );
    }

    #[test]
    fn committed_catalogue_signature_matches() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let yaml_path = root.join("share/modules/catalogue.yaml");
        let yaml = std::fs::read(&yaml_path).expect("share/modules/catalogue.yaml");
        let (pk, sig) = sign_preview_catalogue(&yaml);
        let pub_path = root.join("share/modules/catalogue.pub");
        let sig_path = root.join("share/modules/catalogue.yaml.sig");
        if std::env::var("UPDATE_CATALOGUE").ok().as_deref() == Some("1") {
            std::fs::write(&pub_path, format!("{pk}\n")).unwrap();
            std::fs::write(&sig_path, format!("{sig}\n")).unwrap();
            return;
        }
        let got_pk = std::fs::read_to_string(&pub_path).unwrap_or_default();
        let got_sig = std::fs::read_to_string(&sig_path).unwrap_or_default();
        assert_eq!(
            got_pk.trim(),
            pk,
            "catalogue.pub stale; rerun with UPDATE_CATALOGUE=1"
        );
        assert_eq!(
            got_sig.trim(),
            sig,
            "catalogue.yaml.sig stale; rerun with UPDATE_CATALOGUE=1"
        );
        let cat = SignedCatalogue::load(&yaml_path).unwrap();
        assert!(cat.inner.signature_ok);
    }

    #[test]
    fn catalogue_canvas_hash_matches_packaged_wasm() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let wasm = std::fs::read(root.join("share/modules/canvas.aospkg/module.wasm"))
            .expect("canvas module.wasm");
        let mut hasher = Sha256::new();
        hasher.update(&wasm);
        let got = format!("{:x}", hasher.finalize());
        let yaml = std::fs::read_to_string(root.join("share/modules/catalogue.yaml")).unwrap();
        let want = format!("sha256:{got}");
        assert!(
            yaml.contains(&want),
            "catalogue.yaml canvas hash must match packaged wasm ({want})"
        );
        let manifest =
            std::fs::read_to_string(root.join("share/modules/canvas.aospkg/manifest.yaml"))
                .unwrap();
        assert!(
            manifest.contains(&format!("hash: {got}"))
                || manifest.contains(&format!("hash: sha256:{got}")),
            "manifest.yaml hash must match packaged wasm"
        );
    }

    #[test]
    fn community_catalogue_signature_matches() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let yaml_path = root.join("community/catalogue.yaml");
        let yaml = std::fs::read(&yaml_path).expect("community/catalogue.yaml");
        let (pk, sig) = sign_preview_catalogue(&yaml);
        let sig_path = root.join("community/catalogue.yaml.sig");
        let pub_path = root.join("community/catalogue.pub");
        if std::env::var("UPDATE_COMMUNITY_CATALOGUE").ok().as_deref() == Some("1") {
            std::fs::write(&sig_path, format!("{sig}\n")).unwrap();
            std::fs::write(&pub_path, format!("{pk}\n")).unwrap();
            return;
        }
        let got_sig = std::fs::read_to_string(&sig_path).unwrap_or_default();
        assert_eq!(
            got_sig.trim(),
            sig,
            "community/catalogue.yaml.sig stale; rerun with UPDATE_COMMUNITY_CATALOGUE=1"
        );
        let cat = load_with_pinned_key(&yaml_path).unwrap();
        assert!(cat.inner.signature_ok);
        let brief = cat.entry("morning-brief").expect("morning-brief listed");
        assert_eq!(brief.kind, "skill");
        assert_eq!(brief.license, "MIT");
        let md = std::fs::read(root.join("community/skills/morning-brief/SKILL.md")).unwrap();
        cat.check_entry_hash("morning-brief", Some("skill"), &sha256_hex(&md))
            .unwrap();
    }
}
