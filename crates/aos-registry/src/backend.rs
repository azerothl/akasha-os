//! Backends simulés (P0.3) — API unifiée §3.3, temps de réponse simulés.
//!
//! Le backend local simule les latences via le [`CostModel`] appliqué au plan
//! de placement courant ; le backend distant simule RTT réseau + débit fixe.
//! Tous les événements sont horodatés en **temps logique** (ms) — aucune
//! attente réelle n'est effectuée.

use aos_placement::{PlacementSim, PrivacyClass};
use thiserror::Error;

/// Requête d'inférence simulée.
#[derive(Debug, Clone)]
pub struct InferRequest {
    pub request_id: u64,
    pub model_id: String,
    pub prompt_tokens: u32,
    pub max_output_tokens: u32,
    /// Contexte KV actif (fenêtre courante).
    pub ctx_tokens: u32,
}

/// Événement de flux de tokens (horodatage logique, ms).
#[derive(Debug, Clone, PartialEq)]
pub enum TokenEvent {
    Started { ts_ms: f64 },
    FirstToken { ts_ms: f64 },
    Token { ts_ms: f64, index: u32 },
    Finished { ts_ms: f64, reason: FinishReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishReason {
    Complete,
    Cancelled,
    Error,
}

/// Résultat d'une inférence simulée.
#[derive(Debug, Clone)]
pub struct SimulatedGeneration {
    pub request_id: u64,
    pub model_id: String,
    pub ttft_ms: f64,
    pub tok_s: f64,
    pub total_ms: f64,
    pub events: Vec<TokenEvent>,
}

/// Santé d'un backend (`health()` §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Ok,
    Degraded,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum Error {
    #[error("modèle non placé: {0}")]
    ModelNotPlaced(String),
    #[error("backend hors-ligne")]
    Offline,
    #[error("modèle distant inconnu: {0}")]
    UnknownModel(String),
}

pub use Error as BackendError;

/// API unifiée interne (§3.3).
pub trait SimBackend {
    fn infer(&self, req: &InferRequest) -> Result<SimulatedGeneration, Error>;
    fn health(&self) -> Health;
    fn cancel(&self, request_id: u64) -> bool;
}

/// PRNG xorshift64 déterministe (jitter reproductible des mesures).
struct XorShift(u64);

impl XorShift {
    fn next_f64(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Jitter multiplicatif dans [1 ± spread].
    fn jitter(&mut self, spread: f64) -> f64 {
        1.0 - spread + 2.0 * spread * self.next_f64()
    }
}

fn build_events(
    req: &InferRequest,
    ttft_ms: f64,
    tok_s: f64,
    mut jitter: impl FnMut() -> f64,
) -> (Vec<TokenEvent>, f64) {
    let mut events = Vec::with_capacity(req.max_output_tokens as usize + 3);
    events.push(TokenEvent::Started { ts_ms: 0.0 });
    events.push(TokenEvent::FirstToken { ts_ms: ttft_ms });
    let per_token_ms = 1000.0 / tok_s;
    let mut ts = ttft_ms;
    for i in 1..req.max_output_tokens {
        ts += per_token_ms * jitter();
        events.push(TokenEvent::Token {
            ts_ms: ts,
            index: i,
        });
    }
    events.push(TokenEvent::Finished {
        ts_ms: ts,
        reason: FinishReason::Complete,
    });
    (events, ts)
}

/// Backend local simulé : délègue les estimations au [`PlacementSim`].
pub struct FakeLocalBackend<'a> {
    sim: &'a PlacementSim,
    /// Graine de jitter (reproductibilité des runs).
    seed: u64,
}

impl<'a> FakeLocalBackend<'a> {
    pub fn new(sim: &'a PlacementSim) -> Self {
        Self {
            sim,
            seed: 0x9E3779B97F4A7C15,
        }
    }

    pub fn with_seed(sim: &'a PlacementSim, seed: u64) -> Self {
        Self { sim, seed }
    }
}

impl SimBackend for FakeLocalBackend<'_> {
    fn infer(&self, req: &InferRequest) -> Result<SimulatedGeneration, Error> {
        let est = self
            .sim
            .estimate(&req.model_id, req.prompt_tokens, req.ctx_tokens)
            .ok_or_else(|| Error::ModelNotPlaced(req.model_id.clone()))?;
        let mut rng = XorShift(self.seed ^ req.request_id.wrapping_mul(0xD1B54A32D192ED03));
        let (events, total_ms) = build_events(req, est.ttft_ms, est.tok_s, || rng.jitter(0.05));
        Ok(SimulatedGeneration {
            request_id: req.request_id,
            model_id: req.model_id.clone(),
            ttft_ms: est.ttft_ms,
            tok_s: est.tok_s,
            total_ms,
            events,
        })
    }

    fn health(&self) -> Health {
        Health::Ok
    }

    fn cancel(&self, _request_id: u64) -> bool {
        // Le simulateur est synchrone : rien à annuler a posteriori.
        false
    }
}

/// Backend distant simulé (Remote OpenAI-compatible, §3.3).
pub struct FakeRemoteBackend {
    pub endpoint: String,
    /// RTT réseau aller-simple supposé (ms).
    pub rtt_ms: f64,
    /// Débit distant supposé (tok/s), typiquement élevé côté cloud.
    pub remote_tok_s: f64,
    /// Surcharge d'établissement (TLS, auth) en ms.
    pub setup_ms: f64,
    online: bool,
    served_models: Vec<String>,
}

impl FakeRemoteBackend {
    pub fn new(endpoint: impl Into<String>, served_models: Vec<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            rtt_ms: 60.0,
            remote_tok_s: 80.0,
            setup_ms: 120.0,
            online: true,
            served_models,
        }
    }

    /// Bascule hors-ligne (mode offline strict, F-MDL-06 / §9.5).
    pub fn set_online(&mut self, online: bool) {
        self.online = online;
    }

    /// Refuse de servir un modèle `local` (privacy, §3.7) — garde-fou.
    pub fn check_privacy(&self, class: PrivacyClass) -> Result<(), Error> {
        match class {
            PrivacyClass::Remote => Ok(()),
            // Un modèle classé local ne doit jamais partir sur un backend
            // distant ; le Policy Engine (P3) en fera une règle dure.
            PrivacyClass::Local => Err(Error::UnknownModel(
                "modèle privacy_class=local non serviable à distance".into(),
            )),
        }
    }
}

impl SimBackend for FakeRemoteBackend {
    fn infer(&self, req: &InferRequest) -> Result<SimulatedGeneration, Error> {
        if !self.online {
            return Err(Error::Offline);
        }
        if !self.served_models.contains(&req.model_id) {
            return Err(Error::UnknownModel(req.model_id.clone()));
        }
        let ttft_ms = self.setup_ms + 2.0 * self.rtt_ms + f64::from(req.prompt_tokens) * 0.01; // prefill distant rapide
        let mut rng = XorShift(0xC0FFEE ^ req.request_id);
        let (events, total_ms) = build_events(req, ttft_ms, self.remote_tok_s, || rng.jitter(0.1));
        Ok(SimulatedGeneration {
            request_id: req.request_id,
            model_id: req.model_id.clone(),
            ttft_ms,
            tok_s: self.remote_tok_s,
            total_ms,
            events,
        })
    }

    fn health(&self) -> Health {
        if self.online {
            Health::Ok
        } else {
            Health::Offline
        }
    }

    fn cancel(&self, _request_id: u64) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_placement::{
        CostModel, HardwareProfile, ModelDesc, PlacementProfile, Priority, PrivacyClass,
    };

    const GIB: u64 = 1 << 30;

    fn sim_with_model() -> (PlacementSim, ModelDesc) {
        let m = ModelDesc {
            id: "local:embedded-instruct".into(),
            name: "3B".into(),
            n_layers: 28,
            n_params: 3e9,
            weights_bytes: 2 * GIB,
            embed_bytes: 200_000_000,
            kv_bytes_per_token: 120_000,
            context_length: 8192,
            supports_layer_offload: true,
            privacy_class: PrivacyClass::Local,
        };
        let mut sim = PlacementSim::new(HardwareProfile::reference_v1(), CostModel::default());
        sim.place(&m, PlacementProfile::Latency, Priority::Interactive, 2048)
            .unwrap();
        (sim, m)
    }

    #[test]
    fn backend_local_produit_un_flux_coherent() {
        let (sim, m) = sim_with_model();
        let be = FakeLocalBackend::new(&sim);
        let req = InferRequest {
            request_id: 1,
            model_id: m.id.clone(),
            prompt_tokens: 128,
            max_output_tokens: 16,
            ctx_tokens: 1024,
        };
        let gen = be.infer(&req).unwrap();
        assert!(gen.ttft_ms > 0.0);
        assert!(gen.tok_s > 1.0);
        assert_eq!(gen.events.len(), 16 + 2);
        assert!(matches!(
            gen.events.last(),
            Some(TokenEvent::Finished {
                reason: FinishReason::Complete,
                ..
            })
        ));
        // Modèle non placé → erreur explicite.
        let bad = InferRequest {
            model_id: "local:inconnu".into(),
            ..req.clone()
        };
        assert!(matches!(be.infer(&bad), Err(Error::ModelNotPlaced(_))));
    }

    #[test]
    fn backend_distant_offline_refuse() {
        let mut be = FakeRemoteBackend::new(
            "https://api.example.com/v1",
            vec!["remote:gpt-x".to_string()],
        );
        let req = InferRequest {
            request_id: 1,
            model_id: "remote:gpt-x".into(),
            prompt_tokens: 64,
            max_output_tokens: 8,
            ctx_tokens: 512,
        };
        assert!(be.infer(&req).is_ok());
        be.set_online(false);
        assert!(matches!(be.infer(&req), Err(Error::Offline)));
        assert_eq!(be.health(), Health::Offline);
        assert!(be.check_privacy(PrivacyClass::Local).is_err());
        assert!(be.check_privacy(PrivacyClass::Remote).is_ok());
    }

    #[test]
    fn jitter_deterministe() {
        let (sim, m) = sim_with_model();
        let be = FakeLocalBackend::new(&sim);
        let req = InferRequest {
            request_id: 7,
            model_id: m.id,
            prompt_tokens: 32,
            max_output_tokens: 8,
            ctx_tokens: 256,
        };
        let a = be.infer(&req).unwrap();
        let b = be.infer(&req).unwrap();
        assert_eq!(a.total_ms, b.total_ms);
    }
}
