//! Secrets vault (E7 / Preview 0.4) : chiffrement au repos, distribution
//! restreinte aux **services** (jamais aux agents — F-SEC-04 / §9.2).
//!
//! - Magasin live : `var/secrets/vault.enc` (ChaCha20-Poly1305).
//! - Clé maître : `var/secrets/master.key` (32 octets) ; sous Windows le
//!   contenu est protégé par DPAPI ; sous Linux permissions 0600.
//! - Import optionnel : si `keys.yaml` clair existe encore, migration
//!   automatique puis renommage en `keys.yaml.migrated`.

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

/// Le magasin de secrets chiffré.
pub struct SecretStore {
    dir: PathBuf,
    keys: HashMap<String, String>,
    master: [u8; 32],
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

        let master = load_or_create_master(&dir)?;
        let mut store = Self {
            dir: dir.clone(),
            keys: HashMap::new(),
            master,
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
}

fn load_or_create_master(dir: &Path) -> Result<[u8; 32], SecretError> {
    let path = dir.join("master.key");
    if path.exists() {
        let raw = std::fs::read(&path)?;
        let plain = unprotect_master(&raw)?;
        if plain.len() != 32 {
            return Err(SecretError::Crypto("master.key taille invalide".into()));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&plain);
        return Ok(key);
    }
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    let protected = protect_master(&key)?;
    std::fs::write(&path, protected)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(key)
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
        let _ = std::fs::remove_dir_all(&dir);
    }
}
