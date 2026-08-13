//! # aos-ipc — Semantic IPC Bus v1 (P1.5)
//!
//! Implémentation userspace du bus d'intentions typées de `specs-techniques.md`
//! §2.4 :
//!
//! - messages **typés** : [`Intent`] avec `intent` + `version` + payload CBOR ;
//! - **caps attachées** : URIs `cap://...` transportées dans l'enveloppe
//!   (validation logique P1-P3 ; caps natives `cap://kernel/<id>` vérifiées
//!   par `aos-capkd` en P4) ;
//! - **corrélation** request/response + **streams** (tokens, métriques) ;
//! - **découverte** de services via le broker (`aos-busd`) : les services
//!   enregistrent les intents qu'ils servent, les clients adressent par nom
//!   (`call("model.infer", ...)`).
//!
//! Transport : TCP localhost + frames CBOR préfixées par longueur. La
//! sémantique (intents typés + caps d'enveloppe) est celle du bus natif ;
//! le remplacement du transport par les primitives IPC d'un microkernel
//! (seL4) est la **phase PV** — ADR 0001.

pub mod broker;
pub mod client;
pub mod codec;
pub mod msg;
pub mod service;

pub use client::{BusClient, CallError};
pub use msg::{Frame, Intent, Status, StreamItem, CAP_SCHEME};
pub use service::{BusService, IntentCtx, ServiceError, StreamHandle};

/// Port par défaut du broker (`aos-busd`).
pub const DEFAULT_BUS_PORT: u16 = 24701;

/// Préfixe des capacités natives du noyau `aos-capkd` (P4.2).
pub const KERNEL_CAP_PREFIX: &str = "cap://kernel/";

/// URI d'enveloppe pour une capacité kernel (`cap://kernel/<id>`).
pub fn kernel_cap(id: u64) -> String {
    format!("{KERNEL_CAP_PREFIX}{id}")
}

/// Parse une URI `cap://kernel/<id>` ; `None` si ce n'en est pas une.
pub fn parse_kernel_cap(uri: &str) -> Option<u64> {
    uri.strip_prefix(KERNEL_CAP_PREFIX)?.parse().ok()
}

/// Encode une valeur en CBOR.
pub fn to_cbor<T: serde::Serialize>(
    v: &T,
) -> Result<Vec<u8>, ciborium::ser::Error<std::io::Error>> {
    let mut buf = Vec::new();
    ciborium::into_writer(v, &mut buf)?;
    Ok(buf)
}

/// Décode une valeur CBOR.
pub fn from_cbor<T: serde::de::DeserializeOwned>(
    buf: &[u8],
) -> Result<T, ciborium::de::Error<std::io::Error>> {
    ciborium::from_reader(buf)
}
