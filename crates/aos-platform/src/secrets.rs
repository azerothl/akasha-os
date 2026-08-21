//! Secrets vault (E7 / Preview 0.4 + E7-keyring / 0.6 + E7-TPM / 0.10) :
//! chiffrement au repos, distribution restreinte aux **services**
//! (jamais aux agents — F-SEC-04 / §9.2).
//!
//! - Magasin live : `var/secrets/vault.enc` (ChaCha20-Poly1305).
//! - Clé maître (ordre) : TPM seal (`master.tpm`) → OS keyring → fichier
//!   `master.key` (DPAPI sous Windows, 0600 sous Linux).
//! - TPM (Windows) : enveloppe via Platform Crypto Provider
//!   (`NCryptEncrypt` / `TPM_RSA_SRK_SEAL_KEY`). Presence alone is not enough —
//!   `MasterBackend::Tpm` only when the blob was sealed with that path.
//! - Linux Preview : pas de scellage tpm2 encore ; `/dev/tpmrm0` presence does
//!   not select the TPM backend (fallback keyring/file). Pas de PCR.
//! - Legacy `TPM1` blobs (DPAPI/plaintext mislabeled as TPM) are migrated away.
//! - Import optionnel : si `keys.yaml` clair existe encore, migration
//!   automatique puis renommage en `keys.yaml.migrated`.
//! - Forcer le fichier : `AOS_SECRETS_FILE_KEY=1` (tests / Linux headless).
//! - Désactiver TPM : `AOS_SECRETS_TPM=0`.

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("crypto: {0}")]
    Crypto(String),
    #[error("secret inconnu: {0}")]
    NotFound(String),
    #[error("acteur non autorisé à lire un secret brut: {0}")]
    Forbidden(String),
}

/// Identités bus autorisées à lire un secret brut (`secrets.get`).
pub fn may_get_raw_secret(from: &str) -> bool {
    matches!(
        from,
        "platformd" | "modeld" | "agentd" | "session" | "session-health" | "session-trust"
    ) || from.starts_with("service:")
}

/// Identités autorisées à écrire / lister (sans valeur).
pub fn may_manage_secrets(from: &str) -> bool {
    may_get_raw_secret(from)
        || matches!(from, "ui-egui" | "ui" | "ui-iced")
        || from.starts_with("session")
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct VaultFile {
    #[serde(default)]
    keys: HashMap<String, String>,
}

/// Backend de la clé maître (audit Preview 0.6+ ; TPM = 0.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasterBackend {
    Tpm,
    Keyring,
    File,
}

impl MasterBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tpm => "tpm",
            Self::Keyring => "keyring",
            Self::File => "file",
        }
    }
}

/// Le magasin de secrets chiffré.
pub struct SecretStore {
    dir: PathBuf,
    keys: HashMap<String, String>,
    master: [u8; 32],
    backend: MasterBackend,
}

impl SecretStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SecretError> {
        // `path` historiquement pointe vers keys.yaml ; on utilise le parent.
        let path = path.as_ref();
        let dir = if path.extension().and_then(|e| e.to_str()) == Some("yaml")
            || path.file_name().and_then(|n| n.to_str()) == Some("keys.yaml")
        {
            path.parent()
                .unwrap_or_else(|| Path::new("var/secrets"))
                .to_path_buf()
        } else if path.is_dir() || !path.exists() {
            path.to_path_buf()
        } else {
            path.parent()
                .unwrap_or_else(|| Path::new("var/secrets"))
                .to_path_buf()
        };
        std::fs::create_dir_all(&dir)?;

        let (master, backend) = load_or_create_master(&dir)?;
        write_backend_marker(&dir, backend);
        let mut store = Self {
            dir: dir.clone(),
            keys: HashMap::new(),
            master,
            backend,
        };

        let vault = dir.join("vault.enc");
        if vault.exists() {
            store.keys = decrypt_vault(&vault, &store.master)?;
        }

        // Migration depuis keys.yaml clair (Preview ≤0.3).
        let yaml = dir.join("keys.yaml");
        if yaml.exists() {
            #[derive(Deserialize)]
            struct File {
                #[serde(default)]
                keys: HashMap<String, String>,
            }
            if let Ok(raw) = std::fs::read_to_string(&yaml) {
                if let Ok(file) = serde_yaml::from_str::<File>(&raw) {
                    for (k, v) in file.keys {
                        store.keys.entry(k).or_insert(v);
                    }
                    store.persist()?;
                    let migrated = dir.join("keys.yaml.migrated");
                    let _ = std::fs::rename(&yaml, &migrated);
                }
            }
        }

        Ok(store)
    }

    fn vault_path(&self) -> PathBuf {
        self.dir.join("vault.enc")
    }

    fn persist(&self) -> Result<(), SecretError> {
        encrypt_vault(&self.vault_path(), &self.master, &self.keys)
    }

    /// Lecture d'un secret brut — réservée aux services système.
    pub fn get(&self, name: &str, actor: &str) -> Result<String, SecretError> {
        if !may_get_raw_secret(actor) {
            return Err(SecretError::Forbidden(actor.into()));
        }
        self.keys
            .get(name)
            .cloned()
            .ok_or_else(|| SecretError::NotFound(name.into()))
    }

    /// Écriture (UI Settings / services). Persiste chiffré.
    pub fn set(&mut self, name: &str, value: &str, actor: &str) -> Result<(), SecretError> {
        if !may_manage_secrets(actor) {
            return Err(SecretError::Forbidden(actor.into()));
        }
        if value.is_empty() {
            self.keys.remove(name);
        } else {
            self.keys.insert(name.into(), value.into());
        }
        self.persist()
    }

    /// Liste les **noms** uniquement (jamais les valeurs).
    pub fn list_names(&self, actor: &str) -> Result<Vec<String>, SecretError> {
        if !may_manage_secrets(actor) {
            return Err(SecretError::Forbidden(actor.into()));
        }
        let mut names: Vec<String> = self.keys.keys().cloned().collect();
        names.sort();
        Ok(names)
    }

    /// Export clair optionnel (backup opérateur) — services seulement.
    pub fn export_yaml(&self, actor: &str) -> Result<String, SecretError> {
        if !may_get_raw_secret(actor) {
            return Err(SecretError::Forbidden(actor.into()));
        }
        #[derive(Serialize)]
        struct File<'a> {
            keys: &'a HashMap<String, String>,
        }
        Ok(serde_yaml::to_string(&File { keys: &self.keys })?)
    }

    /// True si le magasin n'est plus un YAML clair.
    pub fn is_encrypted(&self) -> bool {
        self.vault_path().exists()
    }

    /// `tpm` | `keyring` | `file`.
    pub fn master_backend(&self) -> MasterBackend {
        self.backend
    }

    /// Retire l'entrée keyring de ce magasin (tests).
    pub fn delete_keyring_entry(&self) {
        let _ = keyring_delete(&self.dir);
    }
}

const KEYRING_SERVICE: &str = "akasha-os";

fn force_file_backend() -> bool {
    matches!(
        std::env::var("AOS_SECRETS_FILE_KEY").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE")
    )
}

fn tpm_disabled() -> bool {
    matches!(
        std::env::var("AOS_SECRETS_TPM").ok().as_deref(),
        Some("0") | Some("false") | Some("FALSE") | Some("no") | Some("NO")
    )
}

/// True when a host TPM device/provider is reachable (not PCR-bound).
/// Presence alone does **not** imply `MasterBackend::Tpm` — sealing must succeed.
pub fn tpm_present() -> bool {
    if force_file_backend() || tpm_disabled() {
        return false;
    }
    #[cfg(windows)]
    {
        tpm_seal_available_windows()
    }
    #[cfg(not(windows))]
    {
        // Device node may exist, but Preview has no Linux tpm2 seal path yet.
        Path::new("/dev/tpmrm0").exists() || Path::new("/dev/tpm0").exists()
    }
}

fn tpm_path(dir: &Path) -> PathBuf {
    dir.join("master.tpm")
}

/// Real Platform Crypto seal (`TPM2` prefix). Legacy `TPM1` was DPAPI/plaintext.
const TPM_BLOB_V2: &[u8] = b"TPM2";
const TPM_BLOB_V1_LEGACY: &[u8] = b"TPM1";

fn key32(plain: &[u8]) -> Option<[u8; 32]> {
    if plain.len() != 32 {
        return None;
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(plain);
    Some(key)
}

/// Load a **real** TPM-sealed master (`TPM2`). Legacy `TPM1` blobs are migrated
/// out of the TPM backend (they only used DPAPI/plaintext).
fn tpm_get(dir: &Path) -> Option<[u8; 32]> {
    if force_file_backend() || tpm_disabled() {
        return None;
    }
    let path = tpm_path(dir);
    if !path.exists() {
        return None;
    }
    let raw = std::fs::read(&path).ok()?;
    if let Some(rest) = raw.strip_prefix(TPM_BLOB_V2) {
        return key32(&tpm_unseal(rest).ok()?);
    }
    None
}

/// If a legacy mislabeled `TPM1` blob exists, recover the key and delete the file
/// so we stop advertising hardware protection we never had.
fn tpm_migrate_legacy(dir: &Path) -> Option<[u8; 32]> {
    let path = tpm_path(dir);
    if !path.exists() {
        return None;
    }
    let raw = std::fs::read(&path).ok()?;
    let plain = if let Some(rest) = raw.strip_prefix(TPM_BLOB_V1_LEGACY) {
        unprotect_master(rest).ok()?
    } else if raw.starts_with(TPM_BLOB_V2) {
        return None;
    } else {
        // Untagged blob next to a tpm marker — treat as legacy plaintext/DPAPI.
        unprotect_master(&raw).ok()?
    };
    let key = key32(&plain)?;
    let _ = std::fs::remove_file(&path);
    Some(key)
}

fn tpm_set(dir: &Path, key: &[u8; 32]) -> bool {
    if force_file_backend() || tpm_disabled() {
        return false;
    }
    let Ok(sealed) = tpm_seal(key) else {
        return false;
    };
    let mut tagged = Vec::with_capacity(TPM_BLOB_V2.len() + sealed.len());
    tagged.extend_from_slice(TPM_BLOB_V2);
    tagged.extend_from_slice(&sealed);
    if std::fs::write(tpm_path(dir), &tagged).is_err() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(tpm_path(dir), std::fs::Permissions::from_mode(0o600));
    }
    tpm_get(dir).as_ref() == Some(key)
}

#[cfg(windows)]
fn tpm_seal_available_windows() -> bool {
    tpm_with_seal_key_windows(|_key| Ok(())).is_ok()
}

#[cfg(windows)]
fn tpm_seal(plain: &[u8]) -> Result<Vec<u8>, SecretError> {
    tpm_with_seal_key_windows(|key| {
        use windows::Win32::Security::Cryptography::{NCryptEncrypt, NCRYPT_PAD_PKCS1_FLAG};
        let mut needed = 0u32;
        unsafe {
            NCryptEncrypt(key, Some(plain), None, None, &mut needed, NCRYPT_PAD_PKCS1_FLAG)
                .map_err(|e| SecretError::Crypto(format!("NCryptEncrypt size: {e}")))?;
        }
        let mut out = vec![0u8; needed as usize];
        let mut written = 0u32;
        unsafe {
            NCryptEncrypt(
                key,
                Some(plain),
                None,
                Some(&mut out),
                &mut written,
                NCRYPT_PAD_PKCS1_FLAG,
            )
            .map_err(|e| SecretError::Crypto(format!("NCryptEncrypt: {e}")))?;
        }
        out.truncate(written as usize);
        Ok(out)
    })
}

#[cfg(windows)]
fn tpm_unseal(cipher: &[u8]) -> Result<Vec<u8>, SecretError> {
    tpm_with_seal_key_windows(|key| {
        use windows::Win32::Security::Cryptography::{NCryptDecrypt, NCRYPT_PAD_PKCS1_FLAG};
        let mut needed = 0u32;
        unsafe {
            NCryptDecrypt(key, Some(cipher), None, None, &mut needed, NCRYPT_PAD_PKCS1_FLAG)
                .map_err(|e| SecretError::Crypto(format!("NCryptDecrypt size: {e}")))?;
        }
        let mut out = vec![0u8; needed as usize];
        let mut written = 0u32;
        unsafe {
            NCryptDecrypt(
                key,
                Some(cipher),
                None,
                Some(&mut out),
                &mut written,
                NCRYPT_PAD_PKCS1_FLAG,
            )
            .map_err(|e| SecretError::Crypto(format!("NCryptDecrypt: {e}")))?;
        }
        out.truncate(written as usize);
        Ok(out)
    })
}

#[cfg(windows)]
fn tpm_with_seal_key_windows<T>(
    f: impl FnOnce(windows::Win32::Security::Cryptography::NCRYPT_KEY_HANDLE) -> Result<T, SecretError>,
) -> Result<T, SecretError> {
    use windows::Win32::Security::Cryptography::{
        NCryptFreeObject, NCryptOpenKey, NCryptOpenStorageProvider, CERT_KEY_SPEC,
        MS_PLATFORM_CRYPTO_PROVIDER, NCRYPT_FLAGS, NCRYPT_HANDLE, NCRYPT_KEY_HANDLE,
        NCRYPT_PROV_HANDLE, TPM_RSA_SRK_SEAL_KEY,
    };
    unsafe {
        let mut prov = NCRYPT_PROV_HANDLE::default();
        NCryptOpenStorageProvider(&mut prov, MS_PLATFORM_CRYPTO_PROVIDER, 0).map_err(|e| {
            SecretError::Crypto(format!("NCryptOpenStorageProvider: {e}"))
        })?;
        let mut key = NCRYPT_KEY_HANDLE::default();
        let open = NCryptOpenKey(
            prov,
            &mut key,
            TPM_RSA_SRK_SEAL_KEY,
            CERT_KEY_SPEC(0),
            NCRYPT_FLAGS(0),
        );
        if open.is_err() {
            let _ = NCryptFreeObject(NCRYPT_HANDLE(prov.0));
            return Err(SecretError::Crypto(format!("NCryptOpenKey seal: {open:?}")));
        }
        let result = f(key);
        let _ = NCryptFreeObject(NCRYPT_HANDLE(key.0));
        let _ = NCryptFreeObject(NCRYPT_HANDLE(prov.0));
        result
    }
}

#[cfg(not(windows))]
fn tpm_seal(_plain: &[u8]) -> Result<Vec<u8>, SecretError> {
    Err(SecretError::Crypto(
        "TPM seal unavailable on this platform (Linux tpm2 envelope not wired)".into(),
    ))
}

#[cfg(not(windows))]
fn tpm_unseal(_cipher: &[u8]) -> Result<Vec<u8>, SecretError> {
    Err(SecretError::Crypto(
        "TPM unseal unavailable on this platform (Linux tpm2 envelope not wired)".into(),
    ))
}

fn sha256_hex16(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(bytes);
    let mut s = String::with_capacity(16);
    for b in d.iter().take(8) {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn keyring_user(dir: &Path) -> String {
    let marker = dir.join("master.keyring-user");
    if let Ok(s) = std::fs::read_to_string(&marker) {
        let t = s.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    let canon = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let user = format!(
        "vault-master:{}",
        sha256_hex16(canon.to_string_lossy().as_bytes())
    );
    let _ = std::fs::write(&marker, &user);
    user
}

fn key_to_hex(key: &[u8; 32]) -> String {
    key.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_to_key(s: &str) -> Option<[u8; 32]> {
    let s = s.trim();
    if s.len() != 64 {
        return None;
    }
    let mut key = [0u8; 32];
    for i in 0..32 {
        key[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(key)
}

fn keyring_get(dir: &Path) -> Option<[u8; 32]> {
    if force_file_backend() {
        return None;
    }
    let entry = keyring::Entry::new(KEYRING_SERVICE, &keyring_user(dir)).ok()?;
    hex_to_key(&entry.get_password().ok()?)
}

fn keyring_set(dir: &Path, key: &[u8; 32]) -> bool {
    if force_file_backend() {
        return false;
    }
    let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, &keyring_user(dir)) else {
        return false;
    };
    if entry.set_password(&key_to_hex(key)).is_err() {
        return false;
    }
    keyring_get(dir).as_ref() == Some(key)
}

fn keyring_delete(dir: &Path) -> bool {
    let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, &keyring_user(dir)) else {
        return false;
    };
    entry.delete_credential().is_ok()
}

fn write_backend_marker(dir: &Path, backend: MasterBackend) {
    let _ = std::fs::write(dir.join("master.backend"), backend.as_str());
}

fn load_or_create_master(dir: &Path) -> Result<([u8; 32], MasterBackend), SecretError> {
    let path = dir.join("master.key");

    if let Some(key) = tpm_get(dir) {
        return Ok((key, MasterBackend::Tpm));
    }

    // Legacy TPM1 (DPAPI/plaintext with a tpm marker) — recover then re-home.
    if let Some(key) = tpm_migrate_legacy(dir) {
        if tpm_set(dir, &key) {
            if path.exists() {
                let _ = std::fs::remove_file(&path);
            }
            return Ok((key, MasterBackend::Tpm));
        }
        if keyring_set(dir, &key) {
            if path.exists() {
                let _ = std::fs::remove_file(&path);
            }
            return Ok((key, MasterBackend::Keyring));
        }
        let protected = protect_master(&key)?;
        std::fs::write(&path, protected)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        return Ok((key, MasterBackend::File));
    }

    if let Some(key) = keyring_get(dir) {
        if tpm_set(dir, &key) {
            if path.exists() {
                let _ = std::fs::remove_file(&path);
            }
            return Ok((key, MasterBackend::Tpm));
        }
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
        return Ok((key, MasterBackend::Keyring));
    }

    if path.exists() {
        let raw = std::fs::read(&path)?;
        let plain = unprotect_master(&raw)?;
        if plain.len() != 32 {
            return Err(SecretError::Crypto("master.key taille invalide".into()));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&plain);
        if tpm_set(dir, &key) {
            let _ = std::fs::remove_file(&path);
            return Ok((key, MasterBackend::Tpm));
        }
        if keyring_set(dir, &key) {
            let _ = std::fs::remove_file(&path);
            return Ok((key, MasterBackend::Keyring));
        }
        return Ok((key, MasterBackend::File));
    }

    if dir.join("vault.enc").exists() {
        return Err(SecretError::Crypto(
            "vault.enc présent mais clé maître introuvable (tpm/keyring/file)".into(),
        ));
    }

    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    if tpm_set(dir, &key) {
        return Ok((key, MasterBackend::Tpm));
    }
    if keyring_set(dir, &key) {
        return Ok((key, MasterBackend::Keyring));
    }
    let protected = protect_master(&key)?;
    std::fs::write(&path, protected)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok((key, MasterBackend::File))
}

fn encrypt_vault(
    path: &Path,
    master: &[u8; 32],
    keys: &HashMap<String, String>,
) -> Result<(), SecretError> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(master));
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = serde_json::to_vec(&VaultFile {
        keys: keys.clone(),
    })?;
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_ref())
        .map_err(|e| SecretError::Crypto(e.to_string()))?;
    let mut out = Vec::with_capacity(12 + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    std::fs::write(path, out)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn decrypt_vault(path: &Path, master: &[u8; 32]) -> Result<HashMap<String, String>, SecretError> {
    let raw = std::fs::read(path)?;
    if raw.len() < 13 {
        return Err(SecretError::Crypto("vault.enc trop court".into()));
    }
    let (nonce_bytes, ciphertext) = raw.split_at(12);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(master));
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| SecretError::Crypto(e.to_string()))?;
    let file: VaultFile = serde_json::from_slice(&plaintext)?;
    Ok(file.keys)
}

#[cfg(windows)]
fn protect_master(plain: &[u8]) -> Result<Vec<u8>, SecretError> {
    use std::ptr;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
    };

    let mut in_blob = CRYPT_INTEGER_BLOB {
        cbData: plain.len() as u32,
        pbData: plain.as_ptr() as *mut u8,
    };
    let mut out_blob = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &mut in_blob,
            PCWSTR::null(),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        )
    };
    if ok.is_err() {
        return Err(SecretError::Crypto(format!("DPAPI protect: {ok:?}")));
    }
    let slice =
        unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) };
    let out = slice.to_vec();
    unsafe {
        let _ = LocalFree(HLOCAL(out_blob.pbData as *mut _));
    }
    // Prefix so we know it's DPAPI-wrapped.
    let mut tagged = Vec::with_capacity(4 + out.len());
    tagged.extend_from_slice(b"DPA1");
    tagged.extend_from_slice(&out);
    Ok(tagged)
}

#[cfg(windows)]
fn unprotect_master(raw: &[u8]) -> Result<Vec<u8>, SecretError> {
    use std::ptr;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
    };

    if let Some(rest) = raw.strip_prefix(b"DPA1") {
        let mut in_blob = CRYPT_INTEGER_BLOB {
            cbData: rest.len() as u32,
            pbData: rest.as_ptr() as *mut u8,
        };
        let mut out_blob = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: ptr::null_mut(),
        };
        let ok = unsafe {
            CryptUnprotectData(
                &mut in_blob,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out_blob,
            )
        };
        if ok.is_err() {
            return Err(SecretError::Crypto(format!("DPAPI unprotect: {ok:?}")));
        }
        let slice =
            unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) };
        let out = slice.to_vec();
        unsafe {
            let _ = LocalFree(HLOCAL(out_blob.pbData as *mut _));
        }
        return Ok(out);
    }
    // Legacy / raw master key (dev).
    Ok(raw.to_vec())
}

#[cfg(not(windows))]
fn protect_master(plain: &[u8]) -> Result<Vec<u8>, SecretError> {
    Ok(plain.to_vec())
}

#[cfg(not(windows))]
fn unprotect_master(raw: &[u8]) -> Result<Vec<u8>, SecretError> {
    Ok(raw.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "aos-secrets-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn services_seulement() {
        let dir = tmp_dir();
        let path = dir.join("keys.yaml");
        let mut s = SecretStore::open(&path).unwrap();
        s.set("openai_key", "sk-test", "ui-egui").unwrap();
        assert!(s.get("openai_key", "modeld").is_ok());
        assert!(s.get("openai_key", "platformd").is_ok());
        assert!(matches!(
            s.get("openai_key", "agent:1"),
            Err(SecretError::Forbidden(_))
        ));
        assert!(matches!(
            s.get("openai_key", "ui-egui"),
            Err(SecretError::Forbidden(_))
        ));
        let names = s.list_names("ui-egui").unwrap();
        assert_eq!(names, vec!["openai_key".to_string()]);
        s.delete_keyring_entry();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn persist_encrypted_and_migrate_yaml() {
        let dir = tmp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("keys.yaml"),
            "keys:\n  brave_search_api_key: BSA-test\n",
        )
        .unwrap();
        {
            let s = SecretStore::open(dir.join("keys.yaml")).unwrap();
            assert_eq!(
                s.get("brave_search_api_key", "platformd").unwrap(),
                "BSA-test"
            );
            assert!(s.is_encrypted());
            assert!(!dir.join("keys.yaml").exists());
            assert!(dir.join("keys.yaml.migrated").exists());
        }
        let s2 = SecretStore::open(&dir).unwrap();
        assert_eq!(
            s2.get("brave_search_api_key", "service:platformd").unwrap(),
            "BSA-test"
        );
        let raw = std::fs::read(dir.join("vault.enc")).unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("BSA-test"));
        if s2.master_backend() == MasterBackend::Keyring {
            assert!(!dir.join("master.key").exists());
        } else if s2.master_backend() == MasterBackend::Tpm {
            assert!(dir.join("master.tpm").exists());
            assert!(!dir.join("master.key").exists());
        } else {
            assert!(dir.join("master.key").exists());
        }
        assert!(dir.join("master.backend").exists());
        s2.delete_keyring_entry();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_tpm1_blob_migrates_off_tpm_marker() {
        // TPM1 was DPAPI/plaintext with a misleading tpm backend marker.
        let dir = tmp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AOS_SECRETS_TPM", "0");
        std::env::set_var("AOS_SECRETS_FILE_KEY", "1");
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        let protected = protect_master(&key).unwrap();
        let mut tagged = Vec::new();
        tagged.extend_from_slice(b"TPM1");
        tagged.extend_from_slice(&protected);
        std::fs::write(dir.join("master.tpm"), &tagged).unwrap();
        // Also need vault.enc so open doesn't create a fresh key after migrate…
        // Actually: migrate removes master.tpm then falls through; with FILE_KEY
        // and no vault, load_or_create would create a new key. Seed vault first.
        {
            let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
            let nonce_bytes = [7u8; 12];
            let nonce = Nonce::from_slice(&nonce_bytes);
            let plaintext = serde_json::to_vec(&VaultFile {
                keys: HashMap::from([("k".into(), "v".into())]),
            })
            .unwrap();
            let ciphertext = cipher.encrypt(nonce, plaintext.as_ref()).unwrap();
            let mut out = Vec::new();
            out.extend_from_slice(&nonce_bytes);
            out.extend_from_slice(&ciphertext);
            std::fs::write(dir.join("vault.enc"), out).unwrap();
        }
        std::env::remove_var("AOS_SECRETS_TPM");
        // Keep FILE_KEY so we don't pick keyring / real TPM for the re-home.
        let s = SecretStore::open(&dir).unwrap();
        assert_ne!(s.master_backend(), MasterBackend::Tpm);
        assert!(!dir.join("master.tpm").exists());
        assert_eq!(s.get("k", "platformd").unwrap(), "v");
        assert_eq!(
            std::fs::read_to_string(dir.join("master.backend"))
                .unwrap()
                .trim(),
            s.master_backend().as_str()
        );
        s.delete_keyring_entry();
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("AOS_SECRETS_FILE_KEY");
    }

    #[test]
    fn tpm_present_is_safe_to_call() {
        // Must not panic on CI / VMs without TPM.
        let _ = tpm_present();
    }

    #[test]
    fn tpm_set_does_not_claim_tpm_without_seal() {
        // With TPM disabled, presence-gated DPAPI must not write a tpm marker.
        let dir = tmp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AOS_SECRETS_TPM", "0");
        std::env::set_var("AOS_SECRETS_FILE_KEY", "1");
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        assert!(!tpm_set(&dir, &key));
        assert!(!dir.join("master.tpm").exists());
        std::env::remove_var("AOS_SECRETS_TPM");
        std::env::remove_var("AOS_SECRETS_FILE_KEY");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
