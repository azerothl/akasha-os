//! Audit trail (§9.3, §12) : journal append-only, chaîne de hash + HMAC.
//!
//! - Chaque événement porte le hash du précédent (chaîne) et une signature
//!   HMAC-SHA256 avec une clé système locale (`var/audit/secret.key`) ;
//! - le journal est JSONL (`var/audit/audit.jsonl`), réécriture interdite ;
//! - [`AuditJournal::verify`] rejoue la chaîne complète (intégrité, F-SEC-05) ;
//! - la clé ne quitte jamais le service : les autres services appendent via
//!   l'intent `audit.append` sur le bus.

use aos_proto::{AuditAppendRequest, AuditEvent, AuditQueryRequest};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};

type HmacSha256 = Hmac<Sha256>;

/// Journal d'audit.
pub struct AuditJournal {
    dir: PathBuf,
    key: [u8; 32],
    events: Vec<AuditEvent>,
    last_hash: String,
}

impl AuditJournal {
    /// Ouvre (ou crée) le journal dans `dir`.
    pub fn open(dir: impl AsRef<Path>) -> std::io::Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)?;
        let key = Self::load_or_create_key(&dir)?;
        let mut journal = Self {
            dir,
            key,
            events: Vec::new(),
            last_hash: "0".repeat(64),
        };
        journal.replay()?;
        Ok(journal)
    }

    fn key_path(dir: &Path) -> PathBuf {
        dir.join("secret.key")
    }

    fn journal_path(dir: &Path) -> PathBuf {
        dir.join("audit.jsonl")
    }

    fn load_or_create_key(dir: &Path) -> std::io::Result<[u8; 32]> {
        let path = Self::key_path(dir);
        if path.exists() {
            let bytes = std::fs::read(&path)?;
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes[..32]);
            Ok(key)
        } else {
            // Clé locale pseudo-aléatoire (OS RNG via temps + pid + adresse
            // tas — suffisant pour la démo ; P3 : clé hardware/enveloppe §9.2).
            let mut key = [0u8; 32];
            let seed = format!(
                "{}:{}:{:p}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                std::process::id(),
                &key
            );
            let digest = Sha256::digest(seed.as_bytes());
            key.copy_from_slice(&digest);
            std::fs::write(&path, key)?;
            Ok(key)
        }
    }

    fn sign(&self, data: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(&self.key).expect("hmac");
        mac.update(data);
        hex(&mac.finalize().into_bytes())
    }

    #[allow(clippy::too_many_arguments)] // champs canoniques de l'événement
    fn hash_event(
        seq: u64,
        ts_ms: u64,
        trace_id: &str,
        actor: &str,
        action: &str,
        target: &str,
        detail: &serde_json::Value,
        prev_hash: &str,
    ) -> String {
        let canonical = format!(
            "{seq}|{ts_ms}|{trace_id}|{actor}|{action}|{target}|{}|{prev_hash}",
            detail
        );
        hex(&Sha256::digest(canonical.as_bytes()))
    }

    /// Rejoue le journal depuis le disque (tolère un journal absent).
    fn replay(&mut self) -> std::io::Result<()> {
        let path = Self::journal_path(&self.dir);
        if !path.exists() {
            return Ok(());
        }
        let content = std::fs::read_to_string(&path)?;
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let ev: AuditEvent = serde_json::from_str(line)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
            self.last_hash = ev.hash.clone();
            self.events.push(ev);
        }
        Ok(())
    }

    /// Append d'un événement (seul point d'écriture).
    pub fn append(&mut self, req: AuditAppendRequest) -> AuditEvent {
        let seq = self.events.len() as u64 + 1;
        let ts_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let hash = Self::hash_event(
            seq,
            ts_ms,
            &req.trace_id,
            &req.actor,
            &req.action,
            &req.target,
            &req.detail,
            &self.last_hash,
        );
        let signature = self.sign(hash.as_bytes());
        let ev = AuditEvent {
            seq,
            ts_ms,
            trace_id: req.trace_id,
            actor: req.actor,
            action: req.action,
            target: req.target,
            detail: req.detail,
            prev_hash: self.last_hash.clone(),
            hash: hash.clone(),
            signature,
        };
        self.last_hash = hash;
        // Append-only : une ligne par événement, jamais de réécriture.
        let path = Self::journal_path(&self.dir);
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(f, "{}", serde_json::to_string(&ev).unwrap_or_default());
        }
        self.events.push(ev.clone());
        ev
    }

    /// Requête sur le journal (filtres + limite).
    pub fn query(&self, req: &AuditQueryRequest) -> Vec<AuditEvent> {
        let mut out: Vec<AuditEvent> = self
            .events
            .iter()
            .filter(|e| {
                req.trace_id
                    .as_ref()
                    .map(|t| &e.trace_id == t)
                    .unwrap_or(true)
            })
            .filter(|e| req.actor.as_ref().map(|a| &e.actor == a).unwrap_or(true))
            .filter(|e| req.action.as_ref().map(|a| &e.action == a).unwrap_or(true))
            .cloned()
            .collect();
        if req.last > 0 && out.len() > req.last {
            out = out.split_off(out.len() - req.last);
        }
        out
    }

    /// Vérification d'intégrité complète (F-SEC-05) : chaîne + signatures.
    pub fn verify(&self) -> Result<(), String> {
        let mut prev = "0".repeat(64);
        for ev in &self.events {
            if ev.prev_hash != prev {
                return Err(format!("chaîne rompue à seq={}", ev.seq));
            }
            let expected = Self::hash_event(
                ev.seq,
                ev.ts_ms,
                &ev.trace_id,
                &ev.actor,
                &ev.action,
                &ev.target,
                &ev.detail,
                &ev.prev_hash,
            );
            if expected != ev.hash {
                return Err(format!("hash invalide à seq={}", ev.seq));
            }
            if self.sign(ev.hash.as_bytes()) != ev.signature {
                return Err(format!("signature invalide à seq={}", ev.seq));
            }
            prev = ev.hash.clone();
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

/// Hexadécimal minimal (évite une dépendance).
fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static N: AtomicU64 = AtomicU64::new(0);

    fn tmpdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aos-audit-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn req(trace: &str, actor: &str, action: &str, target: &str) -> AuditAppendRequest {
        AuditAppendRequest {
            trace_id: trace.into(),
            actor: actor.into(),
            action: action.into(),
            target: target.into(),
            detail: serde_json::json!({}),
        }
    }

    #[test]
    fn append_et_chaine_integre() {
        let dir = tmpdir("chain");
        let mut j = AuditJournal::open(&dir).unwrap();
        j.append(req("t1", "agent:1", "tool.invoke", "notes.create"));
        j.append(req(
            "t1",
            "module:notes",
            "fs.write",
            "/documents/notes/a.md",
        ));
        assert_eq!(j.len(), 2);
        j.verify().unwrap();
        // La chaîne lie bien les événements.
        let evs = j.query(&AuditQueryRequest {
            trace_id: Some("t1".into()),
            actor: None,
            action: None,
            last: 10,
        });
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[1].prev_hash, evs[0].hash);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejoue_le_journal_apres_reouverture() {
        let dir = tmpdir("replay");
        {
            let mut j = AuditJournal::open(&dir).unwrap();
            j.append(req("t", "a", "x", "y"));
        }
        let j2 = AuditJournal::open(&dir).unwrap();
        assert_eq!(j2.len(), 1);
        j2.verify().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detecte_la_falsification() {
        let dir = tmpdir("tamper");
        let mut j = AuditJournal::open(&dir).unwrap();
        j.append(req("t", "a", "x", "y"));
        // Falsifie l'événement en mémoire.
        j.events[0].action = "policy.allow".into();
        assert!(j.verify().is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
