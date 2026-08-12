//! # aos-ipc — Semantic IPC Bus v1 (P1.5)
//!
//! Implémentation userspace du bus d'intentions typées de `specs-techniques.md`
//! §2.4 :
//!
//! - messages **typés** : [`Intent`] avec `intent` + `version` + payload CBOR ;
//! - **caps attachées** : URIs `cap://...` transportées dans l'enveloppe
//!   (validation logique par les services en P1 ; policy engine dur en P3) ;
//! - **corrélation** request/response + **streams** (tokens, métriques) ;
//! - **découverte** de services via le broker (`aos-busd`) : les services
//!   enregistrent les intents qu'ils servent, les clients adressent par nom
//!   (`call("model.infer", ...)`).
//!
//! Transport : TCP localhost + frames CBOR préfixées par longueur. En P4, ce
//! transport sera remplacé par les primitives IPC du microkernel ; la
//! sémantique des messages reste identique.

pub mod broker;
pub mod client;
pub mod codec;
pub mod msg;
pub mod service;

pub use client::{BusClient, CallError};
pub use msg::{Frame, Intent, Status, StreamItem, CAP_SCHEME};
pub use service::{BusService, IntentCtx, ServiceError, StreamHandle};

/// Port par défaut du broker (`aos-busd`).
pub const DEFAULT_BUS_PORT: u16 = 47001;

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
