//! Messages du bus (enveloppes CBOR) — §2.4.

use serde::{Deserialize, Serialize};

/// Préfixe des URIs de capacités transportées dans les intents.
pub const CAP_SCHEME: &str = "cap://";

/// Une intention typée (cf. exemple §2.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    /// Nom de l'intent, ex. `model.infer`.
    pub intent: String,
    /// Version du schéma de payload.
    pub version: u32,
    /// Payload CBOR (structure propre à l'intent).
    pub payload: Vec<u8>,
    /// Capacités présentées par l'appelant (`cap://...`).
    pub caps: Vec<String>,
    /// Identifiant de corrélation request/response.
    pub correlation_id: u64,
    /// `true` si l'appelant attend un flux de réponses.
    pub wants_stream: bool,
    /// Identité logique de l'appelant (agent, UI, service).
    pub from: String,
}

/// Statut d'une réponse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Ok,
    /// Intent mal formé / version non supportée.
    BadRequest,
    /// Capacité manquante ou invalide (F-SEC-01, `PermissionDenied`).
    PermissionDenied,
    /// Aucun service ne sert cet intent.
    NotFound,
    /// Erreur interne du service.
    InternalError,
    /// Action suspendue en attente de confirmation humaine (§9.4, P3).
    PendingConfirmation,
    /// Opération annulée (model.cancel, agent.kill...).
    Cancelled,
}

/// Élément d'un flux de réponse (token, métrique, événement...).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamItem {
    pub correlation_id: u64,
    /// Payload CBOR (structure propre au flux).
    pub payload: Vec<u8>,
}

/// Trames échangées sur le bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Frame {
    /// Client → broker → service : un intent à router.
    Intent(Intent),
    /// Service → broker → client : réponse terminale.
    Response {
        correlation_id: u64,
        status: Status,
        payload: Vec<u8>,
    },
    /// Service → broker → client : élément de flux (non terminal).
    Stream(StreamItem),
    /// Service → broker → client : fin de flux.
    StreamEnd { correlation_id: u64, status: Status },
    /// Service → broker : enregistrement d'intents servis.
    Register {
        service_name: String,
        intents: Vec<String>,
    },
    /// Client → broker : découvert — qui sert cet intent ?
    Lookup { intent: String },
    /// broker → client : réponse de découverte.
    LookupResult { intent: String, available: bool },
}

impl Frame {
    /// Encode la trame en CBOR.
    pub fn encode(&self) -> std::io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        ciborium::into_writer(self, &mut buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        Ok(buf)
    }

    /// Décode une trame CBOR.
    pub fn decode(buf: &[u8]) -> std::io::Result<Self> {
        ciborium::from_reader(buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
    }
}
