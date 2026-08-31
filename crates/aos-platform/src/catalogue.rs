//! Catalogue local signé de modules / MCP (E10 / Preview 0.6).
//!
//! Pas un store réseau. `share/modules/catalogue.yaml` est signé ed25519
//! (`catalogue.yaml.sig` + `catalogue.pub`). Un package listé dont le hash
//! WASM ne correspond pas est refusé à l'install.

use aos_proto::{CatalogueEntry, ModuleCatalogue};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Seed Preview uniquement — sert à régénérer `.sig` / `.pub` en test.
/// Ce n'est pas une PKI de production.
pub const PREVIEW_CATALOGUE_SEED: [u8; 32] = *b"akasha-os-preview-catalogue-v01!";

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
}

#[derive(Debug, Deserialize)]
struct CatalogueFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    entries: Vec<CatalogueEntry>,
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
        let file: CatalogueFile =
            serde_yaml::from_slice(&bytes).map_err(|e| CatalogueError::Yaml(e.to_string()))?;
        let sig_path = sidecar(&yaml_path, "sig");
        let pub_path = yaml_path.with_file_name("catalogue.pub");
        let signature_ok = if sig_path.exists() && pub_path.exists() {
            let pk = parse_pub(&std::fs::read_to_string(&pub_path)?)?;
            let sig = parse_sig(&std::fs::read_to_string(&sig_path)?)?;
            pk.verify(&bytes, &sig)
                .map_err(|_| CatalogueError::BadSignature)?;
            true
        } else {
            false
        };
        if sig_path.exists() && pub_path.exists() && !signature_ok {
            return Err(CatalogueError::BadSignature);
        }
        Ok(Self {
            inner: ModuleCatalogue {
                version: if file.version == 0 { 1 } else { file.version },
                entries: file.entries,
                signature_ok,
            },
            path: yaml_path,
        })
    }

    pub fn proto(&self) -> &ModuleCatalogue {
        &self.inner
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Si le module est au catalogue, le hash WASM doit correspondre.
    /// Absent du catalogue → Ok (package hors registre, revue de caps inchangée).
    pub fn check_module_hash(&self, name: &str, wasm_hash_hex: &str) -> Result<(), CatalogueError> {
        let Some(entry) = self
            .inner
            .entries
            .iter()
            .find(|e| e.kind == "module" && e.name == name)
        else {
            return Ok(());
        };
        if !self.inner.signature_ok {
            return Err(CatalogueError::BadSignature);
        }
        if hashes_equal(&entry.hash, wasm_hash_hex) {
            Ok(())
        } else {
            Err(CatalogueError::HashMismatch(name.into()))
        }
    }
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
    use ed25519_dalek::{Signer, SigningKey};
    let sk = SigningKey::from_bytes(&PREVIEW_CATALOGUE_SEED);
    let sig = sk.sign(yaml_bytes);
    (
        hex_encode(sk.verifying_key().as_bytes()),
        hex_encode(&sig.to_bytes()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify_roundtrip() {
        let dir = std::env::temp_dir().join(format!("aos-cat-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let yaml = b"version: 1\nentries: []\n";
        let yaml_path = dir.join("catalogue.yaml");
        std::fs::write(&yaml_path, yaml).unwrap();
        let (pk, sig) = sign_preview_catalogue(yaml);
        std::fs::write(dir.join("catalogue.pub"), pk).unwrap();
        std::fs::write(dir.join("catalogue.yaml.sig"), sig).unwrap();
        let cat = SignedCatalogue::load(&yaml_path).unwrap();
        assert!(cat.inner.signature_ok);
        cat.check_module_hash("unknown", "00").unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mismatch_listed_module() {
        let dir = std::env::temp_dir().join(format!("aos-cat-mm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let yaml = b"version: 1\nentries:\n  - name: notes\n    version: \"1\"\n    kind: module\n    path: share/modules/notes.aospkg\n    hash: sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n    attested_caps: []\n";
        let yaml_path = dir.join("catalogue.yaml");
        std::fs::write(&yaml_path, yaml).unwrap();
        let (pk, sig) = sign_preview_catalogue(yaml);
        std::fs::write(dir.join("catalogue.pub"), pk).unwrap();
        std::fs::write(dir.join("catalogue.yaml.sig"), sig).unwrap();
        let cat = SignedCatalogue::load(&yaml_path).unwrap();
        assert!(matches!(
            cat.check_module_hash("notes", "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            Err(CatalogueError::HashMismatch(_))
        ));
        let _ = std::fs::remove_dir_all(&dir);
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
}
