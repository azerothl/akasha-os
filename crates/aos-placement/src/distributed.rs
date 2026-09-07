//! Safe LAN distribution contracts.
//!
//! This is intentionally a policy layer, not a listener. A transport may be
//! added later, but it must use these states so an unpaired/revoked node can
//! never receive a job by discovery alone.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NodeTrust {
    Unpaired,
    Paired,
    Revoked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanNode {
    pub node_id: String,
    pub display_name: String,
    pub address: String,
    pub public_key_fingerprint: String,
    pub trust: NodeTrust,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedWork {
    pub work_id: String,
    pub model_id: String,
    #[serde(default)]
    pub shard_ids: Vec<u32>,
    #[serde(default)]
    pub allow_sensitive_data: bool,
    /// Required by the transport adapter; false is rejected by the registry.
    pub encrypted_transport: bool,
}

/// Message textuelle explicitement autorisée sur un travail LAN chiffré.
///
/// Elle est distincte du protocole IPC `model.infer` afin qu'un appelant ne
/// puisse pas faire sortir un prompt du poste par simple sélection de modèle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanChatMessage {
    pub role: String,
    pub content: String,
}

/// Messages allowed on an authenticated LAN work channel.
///
/// Control-plane and explicitly typed data-plane messages share the same
/// authenticated channel. Data payloads remain bounded and require the work's
/// explicit sensitive-data policy; they must not be smuggled through an
/// untyped transport call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum LanWorkMessage {
    Hello {
        work_id: String,
        node_id: String,
        protocol_version: u16,
    },
    Assign {
        work_id: String,
        model_id: String,
        assignment: LanShardAssignment,
        #[serde(default)]
        allow_sensitive_data: bool,
    },
    Cancel {
        work_id: String,
    },
    Heartbeat {
        work_id: String,
    },
    Ack {
        work_id: String,
        operation: String,
    },
    Prefill {
        work_id: String,
        request_id: String,
        input_tokens: Vec<u32>,
        kv_tokens: u32,
    },
    Decode {
        work_id: String,
        request_id: String,
        max_tokens: u32,
    },
    KvRequest {
        work_id: String,
        request_id: String,
        seq_id: u32,
    },
    WeightRequest {
        work_id: String,
        request_id: String,
        shard_id: u32,
        offset: u64,
        length: u64,
        total_model_bytes: u64,
    },
    WeightBegin {
        work_id: String,
        request_id: String,
        shard_id: u32,
        offset: u64,
        length: u64,
        total_model_bytes: u64,
    },
    WeightPage {
        work_id: String,
        request_id: String,
        shard_id: u32,
        page_index: u32,
        offset: u64,
        data: Vec<u8>,
        final_page: bool,
    },
    ChatInfer {
        work_id: String,
        request_id: String,
        messages: Vec<LanChatMessage>,
        max_tokens: u32,
        /// Température et top-p en millièmes pour garder le contrat CBOR
        /// déterministe et sans flottants côté transport.
        temperature_milli: u32,
        top_p_milli: u32,
        seed: u64,
    },
    TokenBatch {
        work_id: String,
        request_id: String,
        tokens: Vec<u32>,
        finished: bool,
    },
    TextBatch {
        work_id: String,
        request_id: String,
        text: String,
        finished: bool,
        prompt_tokens: u32,
        generated_tokens: u32,
        ttft_ms_milli: u64,
        tok_s_milli: u64,
    },
    KvPage {
        work_id: String,
        request_id: String,
        page_index: u32,
        data: Vec<u8>,
        #[serde(default)]
        seq0_tokens: Vec<u32>,
        final_page: bool,
    },
    Nack {
        work_id: String,
        operation: String,
        reason: String,
    },
}

impl LanWorkMessage {
    pub fn work_id(&self) -> &str {
        match self {
            Self::Hello { work_id, .. }
            | Self::Assign { work_id, .. }
            | Self::Cancel { work_id }
            | Self::Heartbeat { work_id }
            | Self::Ack { work_id, .. }
            | Self::Prefill { work_id, .. }
            | Self::Decode { work_id, .. }
            | Self::KvRequest { work_id, .. }
            | Self::WeightRequest { work_id, .. }
            | Self::WeightBegin { work_id, .. }
            | Self::WeightPage { work_id, .. }
            | Self::ChatInfer { work_id, .. }
            | Self::TokenBatch { work_id, .. }
            | Self::TextBatch { work_id, .. }
            | Self::KvPage { work_id, .. }
            | Self::Nack { work_id, .. } => work_id,
        }
    }

    /// Check that a message can be applied to the declared work and peer.
    pub fn validate_for(&self, work: &DistributedWork, peer_node_id: &str) -> Result<(), String> {
        if self.work_id() != work.work_id {
            return Err("message LAN liée à un autre travail".into());
        }
        match self {
            Self::Hello {
                node_id,
                protocol_version,
                ..
            } => {
                if node_id != peer_node_id {
                    return Err("identité LAN annoncée inattendue".into());
                }
                if *protocol_version != 1 {
                    return Err("version de protocole LAN non supportée".into());
                }
            }
            Self::Assign {
                model_id,
                assignment,
                allow_sensitive_data,
                ..
            } => {
                if model_id != &work.model_id {
                    return Err("modèle LAN inattendu".into());
                }
                if assignment.node_id != peer_node_id {
                    return Err("assignment LAN destinée à un autre nœud".into());
                }
                if !assignment.encrypted_transport || !work.encrypted_transport {
                    return Err("assignment LAN non chiffrée refusée".into());
                }
                if *allow_sensitive_data != work.allow_sensitive_data {
                    return Err("politique de données sensibles LAN incohérente".into());
                }
                if assignment.shard_ids.is_empty()
                    || assignment
                        .shard_ids
                        .iter()
                        .any(|shard| !work.shard_ids.contains(shard))
                {
                    return Err("assignment LAN contient un shard non déclaré".into());
                }
            }
            Self::Cancel { .. } | Self::Heartbeat { .. } => {}
            Self::Ack { operation, .. } => {
                if operation.trim().is_empty() {
                    return Err("opération LAN vide".into());
                }
            }
            Self::Prefill {
                request_id,
                input_tokens,
                kv_tokens,
                ..
            } => {
                validate_request_id(request_id)?;
                if !work.allow_sensitive_data {
                    return Err("prefill LAN refusé sans politique sensible explicite".into());
                }
                if input_tokens.is_empty() || input_tokens.len() > 8192 {
                    return Err("taille de prefill LAN invalide".into());
                }
                if *kv_tokens > 1_048_576 {
                    return Err("budget KV LAN excessif".into());
                }
            }
            Self::Decode {
                request_id,
                max_tokens,
                ..
            } => {
                validate_request_id(request_id)?;
                if !work.allow_sensitive_data {
                    return Err("decode LAN refusé sans politique sensible explicite".into());
                }
                if *max_tokens == 0 || *max_tokens > 8192 {
                    return Err("budget de décodage LAN invalide".into());
                }
            }
            Self::ChatInfer {
                request_id,
                messages,
                max_tokens,
                temperature_milli,
                top_p_milli,
                ..
            } => {
                validate_request_id(request_id)?;
                if !work.allow_sensitive_data {
                    return Err(
                        "inférence texte LAN refusée sans politique sensible explicite".into(),
                    );
                }
                if messages.is_empty() || messages.len() > 64 {
                    return Err("nombre de messages LAN invalide".into());
                }
                let total_bytes: usize = messages
                    .iter()
                    .map(|message| message.role.len().saturating_add(message.content.len()))
                    .sum();
                if total_bytes == 0 || total_bytes > 1_048_576 {
                    return Err("taille de prompt LAN invalide".into());
                }
                if messages
                    .iter()
                    .any(|message| message.role.trim().is_empty() || message.role.len() > 32)
                {
                    return Err("rôle de message LAN invalide".into());
                }
                if *max_tokens == 0 || *max_tokens > 8192 {
                    return Err("budget de décodage LAN invalide".into());
                }
                if *temperature_milli > 5000 || *top_p_milli == 0 || *top_p_milli > 1000 {
                    return Err("paramètres d'échantillonnage LAN invalides".into());
                }
            }
            Self::KvRequest {
                request_id, seq_id, ..
            } => {
                validate_request_id(request_id)?;
                if !work.allow_sensitive_data {
                    return Err("export KV LAN refusé sans politique sensible explicite".into());
                }
                if *seq_id > 1024 {
                    return Err("séquence KV LAN invalide".into());
                }
            }
            Self::WeightRequest {
                request_id,
                shard_id,
                offset,
                length,
                total_model_bytes,
                ..
            }
            | Self::WeightBegin {
                request_id,
                shard_id,
                offset,
                length,
                total_model_bytes,
                ..
            } => {
                validate_request_id(request_id)?;
                if !work.allow_sensitive_data {
                    return Err("poids LAN refusés sans politique sensible explicite".into());
                }
                if !work.shard_ids.contains(shard_id) {
                    return Err("shard de poids LAN non déclaré".into());
                }
                if *shard_id > 65_535
                    || *length == 0
                    || *length > 64 * 1024 * 1024
                    || *total_model_bytes == 0
                    || offset.saturating_add(*length) > *total_model_bytes
                {
                    return Err("plage de poids LAN invalide".into());
                }
            }
            Self::TokenBatch {
                request_id,
                tokens,
                finished,
                ..
            } => {
                validate_request_id(request_id)?;
                if tokens.len() > 8192 || (tokens.is_empty() && !finished) {
                    return Err("batch de tokens LAN invalide".into());
                }
                if !work.allow_sensitive_data {
                    return Err("tokens LAN refusés sans politique sensible explicite".into());
                }
            }
            Self::TextBatch {
                request_id,
                text,
                finished,
                ..
            } => {
                validate_request_id(request_id)?;
                if text.len() > 1_048_576 || (text.is_empty() && !finished) {
                    return Err("batch texte LAN invalide".into());
                }
                if !work.allow_sensitive_data {
                    return Err("texte LAN refusé sans politique sensible explicite".into());
                }
            }
            Self::KvPage {
                request_id,
                page_index,
                data,
                seq0_tokens,
                ..
            } => {
                validate_request_id(request_id)?;
                if data.is_empty() || data.len() > 1_048_576 {
                    return Err("page KV LAN invalide".into());
                }
                if *page_index > 65_535 {
                    return Err("index de page KV LAN invalide".into());
                }
                if seq0_tokens.len() > 8192 || (*page_index != 0 && !seq0_tokens.is_empty()) {
                    return Err("métadonnées de tokens KV LAN invalides".into());
                }
                if !work.allow_sensitive_data {
                    return Err("KV LAN refusé sans politique sensible explicite".into());
                }
            }
            Self::WeightPage {
                request_id,
                shard_id,
                page_index,
                offset,
                data,
                ..
            } => {
                validate_request_id(request_id)?;
                if *shard_id > 65_535
                    || *page_index > 65_535
                    || data.is_empty()
                    || data.len() > 1_048_576
                    || *offset > u64::MAX - data.len() as u64
                {
                    return Err("page de poids LAN invalide".into());
                }
                if !work.allow_sensitive_data {
                    return Err("poids LAN refusés sans politique sensible explicite".into());
                }
                if !work.shard_ids.contains(shard_id) {
                    return Err("shard de poids LAN non déclaré".into());
                }
            }
            Self::Nack {
                operation, reason, ..
            } => {
                if operation.trim().is_empty() || reason.trim().is_empty() {
                    return Err("refus LAN incomplet".into());
                }
            }
        }
        Ok(())
    }

    fn validate_received_for(
        &self,
        work: &DistributedWork,
        local_node_id: &str,
        peer_node_id: &str,
    ) -> Result<(), String> {
        match self {
            Self::Hello {
                node_id,
                protocol_version,
                ..
            } => {
                if self.work_id() != work.work_id {
                    return Err("message LAN liée à un autre travail".into());
                }
                if node_id != peer_node_id {
                    return Err("identité LAN annoncée inattendue".into());
                }
                if *protocol_version != 1 {
                    return Err("version de protocole LAN non supportée".into());
                }
                Ok(())
            }
            Self::Assign { .. } => self.validate_for(work, local_node_id),
            _ => self.validate_for(work, peer_node_id),
        }
    }
}

fn validate_request_id(request_id: &str) -> Result<(), String> {
    if request_id.trim().is_empty() || request_id.len() > 128 {
        return Err("identifiant de requête LAN invalide".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LanJobState {
    Planned,
    Running,
    Degraded,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanShardAssignment {
    pub node_id: String,
    pub shard_ids: Vec<u32>,
    #[serde(default)]
    pub kv_tokens: u32,
    pub encrypted_transport: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LanWorkerJobState {
    Assigned,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanWorkerJob {
    pub work_id: String,
    pub model_id: String,
    pub shard_ids: Vec<u32>,
    pub kv_tokens: u32,
    pub allow_sensitive_data: bool,
    pub state: LanWorkerJobState,
}

/// Bounded range of a model file staged for a declared logical shard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanWeightRange {
    pub shard_id: u32,
    pub offset: u64,
    pub length: u64,
}

/// Coverage manifest for staged model ranges.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanShardManifest {
    pub model_id: String,
    pub total_model_bytes: u64,
    #[serde(default)]
    pub ranges: Vec<LanWeightRange>,
}

impl LanShardManifest {
    pub fn new(model_id: impl Into<String>, total_model_bytes: u64) -> Result<Self, String> {
        let model_id = model_id.into();
        if model_id.trim().is_empty() || total_model_bytes == 0 {
            return Err("manifeste de poids LAN invalide".into());
        }
        Ok(Self {
            model_id,
            total_model_bytes,
            ranges: Vec::new(),
        })
    }

    pub fn record_range(&mut self, range: LanWeightRange) -> Result<(), String> {
        self.validate()?;
        if range.shard_id > 65_535
            || range.length == 0
            || range.length > 64 * 1024 * 1024
            || range
                .offset
                .checked_add(range.length)
                .is_none_or(|end| end > self.total_model_bytes)
        {
            return Err("plage de poids LAN invalide".into());
        }
        if self.ranges.iter().any(|existing| {
            existing.shard_id == range.shard_id
                && existing.offset == range.offset
                && existing.length == range.length
        }) {
            return Ok(());
        }
        let range_end = range.offset + range.length;
        if self.ranges.iter().any(|existing| {
            let existing_end = existing.offset + existing.length;
            range.offset < existing_end && existing.offset < range_end
        }) {
            return Err("plages de poids LAN recouvrantes".into());
        }
        self.ranges.push(range);
        self.ranges
            .sort_by_key(|range| (range.offset, range.shard_id));
        Ok(())
    }

    pub fn is_complete(&self) -> bool {
        if self.validate().is_err() {
            return false;
        }
        let mut ranges = self.ranges.clone();
        ranges.sort_by_key(|range| (range.offset, range.shard_id));
        let mut cursor = 0u64;
        for range in &ranges {
            if range.offset != cursor {
                return false;
            }
            cursor = cursor.saturating_add(range.length);
        }
        cursor == self.total_model_bytes
    }

    pub fn missing_ranges(&self) -> Vec<(u64, u64)> {
        // Invalid persisted coverage must never make bytes appear available.
        if self.validate().is_err() {
            return vec![(0, self.total_model_bytes)];
        }
        let mut ranges = self.ranges.clone();
        ranges.sort_by_key(|range| (range.offset, range.shard_id));
        let mut missing = Vec::new();
        let mut cursor = 0u64;
        for range in &ranges {
            if range.offset > cursor {
                missing.push((cursor, range.offset - cursor));
            }
            cursor = cursor.max(range.offset.saturating_add(range.length));
        }
        if cursor < self.total_model_bytes {
            missing.push((cursor, self.total_model_bytes - cursor));
        }
        missing
    }

    /// Validate persisted manifests as well as values built through record_range.
    pub fn validate(&self) -> Result<(), String> {
        if self.model_id.trim().is_empty() || self.total_model_bytes == 0 {
            return Err("manifeste de poids LAN invalide".into());
        }
        let mut ranges = self.ranges.iter().collect::<Vec<_>>();
        ranges.sort_by_key(|range| range.offset);
        let mut cursor = 0;
        for range in ranges {
            let end = range
                .offset
                .checked_add(range.length)
                .ok_or("débordement de plage de poids LAN")?;
            if range.shard_id > 65_535
                || range.length == 0
                || range.length > 64 * 1024 * 1024
                || range.offset < cursor
                || end > self.total_model_bytes
            {
                return Err("couverture de poids LAN invalide".into());
            }
            cursor = end;
        }
        Ok(())
    }
}

/// Runtime state owned by a model worker. Assignment/cancellation state is
/// available now; model weight loading and token/KV execution are separate
/// runtime capabilities layered on top of the typed data-plane contract.
#[derive(Debug, Clone, Default)]
pub struct LanWorkerRegistry {
    jobs: HashMap<String, LanWorkerJob>,
    abort_flags: HashMap<String, Vec<std::sync::Weak<AtomicBool>>>,
}

impl LanWorkerRegistry {
    pub fn assign(
        &mut self,
        local_node_id: &str,
        peer_node_id: &str,
        model_id: String,
        assignment: LanShardAssignment,
        work_id: String,
        allow_sensitive_data: bool,
    ) -> Result<(), String> {
        if assignment.node_id != local_node_id {
            return Err("assignment LAN destinée à un autre worker".into());
        }
        if peer_node_id.trim().is_empty() || model_id.trim().is_empty() {
            return Err("identité du coordinateur ou modèle LAN vide".into());
        }
        if work_id.trim().is_empty() || assignment.shard_ids.is_empty() {
            return Err("assignment LAN vide".into());
        }
        if !assignment.encrypted_transport {
            return Err("assignment LAN non chiffrée refusée".into());
        }
        if let Some(job) = self.jobs.get(&work_id) {
            if job.state == LanWorkerJobState::Cancelled {
                return Err("travail LAN déjà annulé".into());
            }
            if job.model_id != model_id || job.allow_sensitive_data != allow_sensitive_data {
                return Err("réaffectation LAN incompatible avec le travail existant".into());
            }
        }
        self.jobs.insert(
            work_id.clone(),
            LanWorkerJob {
                work_id,
                model_id,
                shard_ids: assignment.shard_ids,
                kv_tokens: assignment.kv_tokens,
                allow_sensitive_data,
                state: LanWorkerJobState::Assigned,
            },
        );
        Ok(())
    }

    /// Associe le drapeau d'annulation du contexte actif à son travail.
    ///
    /// Le drapeau reste dans le registre plutôt que dans la connexion TCP :
    /// une annulation peut ainsi arriver sur une nouvelle connexion
    /// authentifiée pendant qu'une génération bloque la connexion de travail.
    pub fn register_abort(&mut self, work_id: &str, abort: Arc<AtomicBool>) -> Result<(), String> {
        let job = self.jobs.get(work_id).ok_or("travail LAN inconnu")?;
        if job.state == LanWorkerJobState::Cancelled {
            return Err("travail LAN déjà annulé".into());
        }
        let flags = self.abort_flags.entry(work_id.to_string()).or_default();
        flags.retain(|flag| flag.strong_count() > 0);
        flags.push(Arc::downgrade(&abort));
        Ok(())
    }

    /// Réarme un travail affecté avant une nouvelle requête sur sa session.
    pub fn reset_abort(&mut self, work_id: &str) -> Result<(), String> {
        let job = self.jobs.get(work_id).ok_or("travail LAN inconnu")?;
        if job.state == LanWorkerJobState::Cancelled {
            return Err("travail LAN annulé".into());
        }
        if let Some(flags) = self.abort_flags.get(work_id) {
            if flags
                .iter()
                .filter_map(std::sync::Weak::upgrade)
                .any(|abort| abort.load(Ordering::SeqCst))
            {
                return Err("travail LAN annulé".into());
            }
        }
        Ok(())
    }

    pub fn cancel(&mut self, work_id: &str) -> Result<(), String> {
        let job = self.jobs.get_mut(work_id).ok_or("travail LAN inconnu")?;
        job.state = LanWorkerJobState::Cancelled;
        if let Some(flags) = self.abort_flags.get(work_id) {
            for abort in flags.iter().filter_map(std::sync::Weak::upgrade) {
                abort.store(true, Ordering::SeqCst);
            }
        }
        Ok(())
    }

    pub fn get(&self, work_id: &str) -> Option<&LanWorkerJob> {
        self.jobs.get(work_id)
    }

    pub fn snapshot(&self) -> Vec<LanWorkerJob> {
        self.jobs.values().cloned().collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanWorkPlan {
    pub work_id: String,
    #[serde(default)]
    pub model_id: String,
    pub state: LanJobState,
    pub assignments: Vec<LanShardAssignment>,
    #[serde(default)]
    pub unassigned_shards: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanRecovery {
    pub work_id: String,
    pub lost_node_id: String,
    pub state: LanJobState,
    pub reassigned_shards: Vec<u32>,
    pub cancelled_nodes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LanPairingRegistry {
    nodes: HashMap<String, LanNode>,
}

impl LanPairingRegistry {
    pub fn discover(&mut self, node: LanNode) {
        let _ = self.try_discover(node);
    }

    /// Observe a LAN advertisement without allowing identity replacement.
    ///
    /// A changed fingerprint is rejected, including when the node was already
    /// paired. Endpoint/capability changes are accepted only for the same
    /// persistent identity and retain the current trust state.
    pub fn try_discover(&mut self, node: LanNode) -> Result<(), String> {
        if let Some(existing) = self.nodes.get_mut(&node.node_id) {
            if existing.public_key_fingerprint != node.public_key_fingerprint {
                return Err("empreinte de clé modifiée pour un nœud connu".into());
            }
            let trust = existing.trust;
            *existing = node;
            existing.trust = trust;
        } else {
            self.nodes.insert(node.node_id.clone(), node);
        }
        Ok(())
    }

    pub fn pair(&mut self, node_id: &str, fingerprint: &str) -> Result<(), String> {
        let node = self.nodes.get_mut(node_id).ok_or("nœud inconnu")?;
        if node.public_key_fingerprint != fingerprint {
            return Err("empreinte de clé inattendue".into());
        }
        node.trust = NodeTrust::Paired;
        Ok(())
    }

    pub fn revoke(&mut self, node_id: &str) -> bool {
        self.nodes
            .get_mut(node_id)
            .map(|node| {
                node.trust = NodeTrust::Revoked;
                true
            })
            .unwrap_or(false)
    }

    pub fn authorize(&self, node_id: &str, work: &DistributedWork) -> Result<(), String> {
        let node = self.nodes.get(node_id).ok_or("nœud non appairé")?;
        self.authorize_peer(node_id)?;
        if !work.encrypted_transport {
            return Err("transport chiffré obligatoire".into());
        }
        if work.allow_sensitive_data && !node.capabilities.iter().any(|cap| cap == "sensitive-data")
        {
            return Err("politique sensible non autorisée pour ce nœud".into());
        }
        Ok(())
    }

    /// Authorize an authenticated control connection before its work metadata
    /// has been received. Work-level policy is checked after the encrypted
    /// assignment arrives.
    pub fn authorize_peer(&self, node_id: &str) -> Result<(), String> {
        let node = self.nodes.get(node_id).ok_or("nœud non appairé")?;
        if node.trust != NodeTrust::Paired {
            return Err("nœud non appairé ou révoqué".into());
        }
        let host = node
            .address
            .parse::<std::net::SocketAddr>()
            .map_err(|_| "adresse LAN invalide".to_string())?
            .ip();
        let local = match host {
            std::net::IpAddr::V4(ip) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
            std::net::IpAddr::V6(ip) => {
                ip.is_loopback()
                    || (ip.segments()[0] & 0xfe00) == 0xfc00 // fc00::/7
                    || (ip.segments()[0] & 0xffc0) == 0xfe80 // fe80::/10
            }
        };
        if !local {
            return Err("adresse hors LAN refusée".into());
        }
        Ok(())
    }

    /// Resolve an incoming TCP peer using the address recorded at pairing
    /// time. The source port is intentionally ignored because it is ephemeral.
    pub fn node_for_ip(&self, ip: std::net::IpAddr) -> Option<&LanNode> {
        self.nodes.values().find(|node| {
            node.address
                .parse::<std::net::SocketAddr>()
                .map(|address| address.ip() == ip)
                .unwrap_or(false)
        })
    }

    pub fn get(&self, node_id: &str) -> Option<&LanNode> {
        self.nodes.get(node_id)
    }

    pub fn nodes(&self) -> impl Iterator<Item = &LanNode> {
        self.nodes.values()
    }

    pub fn paired_nodes(&self) -> impl Iterator<Item = &LanNode> {
        self.nodes
            .values()
            .filter(|node| node.trust == NodeTrust::Paired)
    }

    pub fn snapshot(&self) -> Vec<LanNode> {
        self.nodes.values().cloned().collect()
    }

    pub fn from_nodes(nodes: impl IntoIterator<Item = LanNode>) -> Result<Self, String> {
        let mut registry = Self::default();
        for node in nodes {
            registry.try_discover(node)?;
        }
        Ok(registry)
    }
}

/// Encrypted, authenticated application frame for a future LAN transport.
///
/// The nonce is derived from the monotonically increasing sequence number and
/// the session key. The node/job binding is authenticated as associated data,
/// so a frame cannot be moved to another paired node or job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanSecureFrame {
    pub node_id: String,
    pub work_id: String,
    pub sequence: u64,
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

#[derive(Clone)]
pub struct LanSessionKey([u8; 32]);

impl LanSessionKey {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != 32 {
            return Err("clé de session LAN : 32 octets requis".into());
        }
        let mut key = [0; 32];
        key.copy_from_slice(bytes);
        Ok(Self(key))
    }

    pub fn encrypt(
        &self,
        node_id: &str,
        work_id: &str,
        sequence: u64,
        plaintext: &[u8],
    ) -> Result<LanSecureFrame, String> {
        let nonce = nonce_for(sequence);
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.0));
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad: frame_aad(node_id, work_id, sequence).as_slice(),
                },
            )
            .map_err(|_| "chiffrement LAN impossible".to_string())?;
        Ok(LanSecureFrame {
            node_id: node_id.into(),
            work_id: work_id.into(),
            sequence,
            nonce,
            ciphertext,
        })
    }

    pub fn decrypt(&self, frame: &LanSecureFrame) -> Result<Vec<u8>, String> {
        let expected_nonce = nonce_for(frame.sequence);
        if frame.nonce != expected_nonce {
            return Err("nonce LAN invalide".into());
        }
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.0));
        cipher
            .decrypt(
                Nonce::from_slice(&frame.nonce),
                Payload {
                    msg: &frame.ciphertext,
                    aad: frame_aad(&frame.node_id, &frame.work_id, frame.sequence).as_slice(),
                },
            )
            .map_err(|_| "authentification LAN échouée".to_string())
    }
}

/// Sequenced channel wrapper for a transport adapter. It rejects stale or
/// replayed frames before handing plaintext to the caller.
pub struct LanSecureChannel {
    key: LanSessionKey,
    local_node_id: String,
    peer_node_id: String,
    work_id: String,
    next_send: u64,
    last_received: Option<u64>,
}

impl LanSecureChannel {
    pub fn new(
        key: LanSessionKey,
        local_node_id: impl Into<String>,
        peer_node_id: impl Into<String>,
        work_id: impl Into<String>,
    ) -> Self {
        Self {
            key,
            local_node_id: local_node_id.into(),
            peer_node_id: peer_node_id.into(),
            work_id: work_id.into(),
            next_send: 0,
            last_received: None,
        }
    }

    pub fn send(&mut self, plaintext: &[u8]) -> Result<LanSecureFrame, String> {
        let sequence = self.next_send;
        self.next_send = self
            .next_send
            .checked_add(1)
            .ok_or("séquence LAN épuisée")?;
        self.key
            .encrypt(&self.local_node_id, &self.work_id, sequence, plaintext)
    }

    pub fn receive(&mut self, frame: &LanSecureFrame) -> Result<Vec<u8>, String> {
        if frame.node_id != self.peer_node_id || frame.work_id != self.work_id {
            return Err("frame LAN liée à une autre identité ou un autre travail".into());
        }
        if self
            .last_received
            .is_some_and(|last| frame.sequence <= last)
        {
            return Err("frame LAN rejouée ou hors séquence".into());
        }
        let plaintext = self.key.decrypt(frame)?;
        self.last_received = Some(frame.sequence);
        Ok(plaintext)
    }
}

/// Explicit length-delimited TCP adapter for the secure frame contract.
///
/// Construction is the only operation that connects. This adapter does not
/// discover peers, bind a listener, retry outside the LAN, or choose a node.
/// The caller supplies a session key obtained from its secret store.
pub struct LanTcpTransport {
    stream: tokio::net::TcpStream,
    channel: LanSecureChannel,
    max_frame_bytes: usize,
}

/// Explicit listener for a paired LAN worker.
///
/// Binding is opt-in. The first message must be a typed `Hello` from the
/// expected peer, and the listener answers with its own `Hello` before it
/// hands the transport to the caller.
pub struct LanTcpListener {
    listener: tokio::net::TcpListener,
    local_node_id: String,
    registry: LanPairingRegistry,
}

impl LanTcpListener {
    pub async fn bind(
        local_node_id: impl Into<String>,
        address: &str,
        registry: &LanPairingRegistry,
    ) -> Result<Self, String> {
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .map_err(|e| format!("listener LAN impossible: {e}"))?;
        Ok(Self {
            listener,
            local_node_id: local_node_id.into(),
            registry: registry.clone(),
        })
    }

    pub fn local_addr(&self) -> Result<std::net::SocketAddr, String> {
        self.listener
            .local_addr()
            .map_err(|e| format!("adresse du listener LAN indisponible: {e}"))
    }

    pub async fn accept_authenticated(
        &self,
        peer_node_id: &str,
        work: &DistributedWork,
        key: LanSessionKey,
    ) -> Result<LanTcpTransport, String> {
        self.registry.authorize(peer_node_id, work)?;
        let registered_address = self
            .registry
            .get(peer_node_id)
            .map(|node| node.address.as_str())
            .ok_or("nœud LAN non appairé")?;
        let registered_ip = registered_address
            .parse::<std::net::SocketAddr>()
            .map_err(|_| "adresse LAN appairée invalide".to_string())?
            .ip();
        let (stream, remote_address) = self
            .listener
            .accept()
            .await
            .map_err(|e| format!("accept LAN impossible: {e}"))?;
        if remote_address.ip() != registered_ip {
            return Err("adresse source LAN différente du nœud appairé".into());
        }

        let mut transport = LanTcpTransport::from_stream(
            stream,
            self.local_node_id.clone(),
            peer_node_id,
            work.work_id.clone(),
            key,
        );
        let hello = transport.receive_message(work).await?;
        if !matches!(hello, LanWorkMessage::Hello { .. }) {
            return Err("handshake LAN sans message hello".into());
        }
        transport
            .send_message(
                &LanWorkMessage::Hello {
                    work_id: work.work_id.clone(),
                    node_id: self.local_node_id.clone(),
                    protocol_version: 1,
                },
                work,
            )
            .await?;
        Ok(transport)
    }

    /// Accept a worker connection when the work id is not known locally yet.
    ///
    /// The work id is visible only as authenticated frame metadata; the first
    /// encrypted message must still be a Hello from a paired peer. The worker
    /// can therefore receive a new assignment without opening an unauthenticated
    /// bootstrap channel.
    pub async fn accept_authenticated_any_work(
        &self,
        key: LanSessionKey,
    ) -> Result<LanTcpTransport, String> {
        self.accept_authenticated_any_work_with_registry(&self.registry, key)
            .await
    }

    /// Variant used by long-lived daemons so a UI pairing/revocation change
    /// can be observed without rebinding the TCP listener.
    pub async fn accept_authenticated_any_work_with_registry(
        &self,
        registry: &LanPairingRegistry,
        key: LanSessionKey,
    ) -> Result<LanTcpTransport, String> {
        let (mut stream, remote_address) = self
            .listener
            .accept()
            .await
            .map_err(|e| format!("accept LAN impossible: {e}"))?;
        let peer = registry
            .node_for_ip(remote_address.ip())
            .ok_or("adresse source LAN inconnue")?;
        let peer_node_id = peer.node_id.clone();
        registry.authorize_peer(&peer_node_id)?;

        let frame =
            read_frame_from_stream(&mut stream, LanTcpTransport::DEFAULT_MAX_FRAME_BYTES).await?;
        if frame.node_id != peer_node_id || frame.work_id.trim().is_empty() {
            return Err("première trame LAN inattendue".into());
        }
        let work_id = frame.work_id.clone();
        let mut transport = LanTcpTransport::from_stream(
            stream,
            self.local_node_id.clone(),
            peer_node_id.clone(),
            work_id.clone(),
            key,
        );
        let plaintext = transport.channel.receive(&frame)?;
        let hello: LanWorkMessage = ciborium::from_reader(Cursor::new(plaintext))
            .map_err(|e| format!("décodage de hello LAN: {e}"))?;
        match hello {
            LanWorkMessage::Hello {
                work_id: hello_work_id,
                node_id,
                protocol_version,
            } if hello_work_id == work_id && node_id == peer_node_id && protocol_version == 1 => {}
            _ => return Err("handshake LAN sans hello valide".into()),
        }

        let control_work = DistributedWork {
            work_id,
            model_id: String::new(),
            shard_ids: vec![0],
            allow_sensitive_data: false,
            encrypted_transport: true,
        };
        transport
            .send_message(
                &LanWorkMessage::Hello {
                    work_id: control_work.work_id.clone(),
                    node_id: self.local_node_id.clone(),
                    protocol_version: 1,
                },
                &control_work,
            )
            .await?;
        Ok(transport)
    }
}

impl LanTcpTransport {
    pub const DEFAULT_MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

    pub async fn connect(
        local_node_id: impl Into<String>,
        node_id: &str,
        address: &str,
        registry: &LanPairingRegistry,
        work: &DistributedWork,
        key: LanSessionKey,
    ) -> Result<Self, String> {
        registry.authorize(node_id, work)?;
        let registered_address = registry
            .get(node_id)
            .map(|node| node.address.as_str())
            .ok_or("nœud non appairé")?;
        if address != registered_address {
            return Err("adresse LAN différente de celle appairée".into());
        }
        let stream = tokio::net::TcpStream::connect(address)
            .await
            .map_err(|e| format!("connexion LAN impossible: {e}"))?;
        Ok(Self::from_stream(
            stream,
            local_node_id,
            node_id,
            work.work_id.clone(),
            key,
        ))
    }

    pub async fn connect_authenticated(
        local_node_id: impl Into<String>,
        node_id: &str,
        address: &str,
        registry: &LanPairingRegistry,
        work: &DistributedWork,
        key: LanSessionKey,
    ) -> Result<Self, String> {
        let local_node_id = local_node_id.into();
        let mut transport =
            Self::connect(local_node_id.clone(), node_id, address, registry, work, key).await?;
        transport
            .send_message(
                &LanWorkMessage::Hello {
                    work_id: work.work_id.clone(),
                    node_id: local_node_id,
                    protocol_version: 1,
                },
                work,
            )
            .await?;
        let hello = transport.receive_message(work).await?;
        if !matches!(hello, LanWorkMessage::Hello { .. }) {
            return Err("handshake LAN sans réponse hello".into());
        }
        Ok(transport)
    }

    pub fn from_stream(
        stream: tokio::net::TcpStream,
        local_node_id: impl Into<String>,
        peer_node_id: impl Into<String>,
        work_id: impl Into<String>,
        key: LanSessionKey,
    ) -> Self {
        Self {
            stream,
            channel: LanSecureChannel::new(key, local_node_id, peer_node_id, work_id),
            max_frame_bytes: Self::DEFAULT_MAX_FRAME_BYTES,
        }
    }

    pub fn local_node_id(&self) -> &str {
        &self.channel.local_node_id
    }

    pub fn peer_node_id(&self) -> &str {
        &self.channel.peer_node_id
    }

    pub fn work_id(&self) -> &str {
        &self.channel.work_id
    }

    pub fn with_max_frame_bytes(mut self, max_frame_bytes: usize) -> Result<Self, String> {
        if max_frame_bytes == 0 || max_frame_bytes > Self::DEFAULT_MAX_FRAME_BYTES {
            return Err("taille maximale de trame LAN invalide".into());
        }
        self.max_frame_bytes = max_frame_bytes;
        Ok(self)
    }

    pub async fn send(&mut self, plaintext: &[u8]) -> Result<(), String> {
        let frame = self.channel.send(plaintext)?;
        let mut encoded = Vec::new();
        ciborium::into_writer(&frame, &mut encoded)
            .map_err(|e| format!("encodage de trame LAN: {e}"))?;
        if encoded.len() > self.max_frame_bytes {
            return Err("trame LAN trop volumineuse".into());
        }
        self.stream
            .write_u32(encoded.len() as u32)
            .await
            .map_err(|e| format!("écriture de trame LAN: {e}"))?;
        self.stream
            .write_all(&encoded)
            .await
            .map_err(|e| format!("écriture de trame LAN: {e}"))
    }

    pub async fn send_message(
        &mut self,
        message: &LanWorkMessage,
        work: &DistributedWork,
    ) -> Result<(), String> {
        if message.work_id() != work.work_id || message.work_id() != self.channel.work_id {
            return Err("message LAN liée à un autre travail".into());
        }
        match message {
            LanWorkMessage::Hello { node_id, .. } if node_id != &self.channel.local_node_id => {
                return Err("identité LAN locale inattendue".into());
            }
            LanWorkMessage::Hello { .. } => {}
            _ => message.validate_for(work, &self.channel.peer_node_id)?,
        }
        let mut encoded = Vec::new();
        ciborium::into_writer(message, &mut encoded)
            .map_err(|e| format!("encodage de message LAN: {e}"))?;
        self.send(&encoded).await
    }

    pub async fn receive(&mut self) -> Result<Vec<u8>, String> {
        let frame = read_frame_from_stream(&mut self.stream, self.max_frame_bytes).await?;
        self.channel.receive(&frame)
    }

    /// Decode an authenticated message without applying work-level validation.
    /// This is only for the worker's first assignment/cancel message, where
    /// the model and shard set are supplied by that message itself.
    pub async fn receive_message_unchecked(&mut self) -> Result<LanWorkMessage, String> {
        let plaintext = self.receive().await?;
        ciborium::from_reader(Cursor::new(plaintext))
            .map_err(|e| format!("décodage de message LAN: {e}"))
    }

    pub async fn receive_message(
        &mut self,
        work: &DistributedWork,
    ) -> Result<LanWorkMessage, String> {
        let plaintext = self.receive().await?;
        let message: LanWorkMessage = ciborium::from_reader(Cursor::new(plaintext))
            .map_err(|e| format!("décodage de message LAN: {e}"))?;
        message.validate_received_for(
            work,
            &self.channel.local_node_id,
            &self.channel.peer_node_id,
        )?;
        Ok(message)
    }
}

async fn read_frame_from_stream(
    stream: &mut tokio::net::TcpStream,
    max_frame_bytes: usize,
) -> Result<LanSecureFrame, String> {
    let length = stream
        .read_u32()
        .await
        .map_err(|e| format!("lecture de trame LAN: {e}"))? as usize;
    if length == 0 || length > max_frame_bytes {
        return Err("taille de trame LAN refusée".into());
    }
    let mut encoded = vec![0; length];
    stream
        .read_exact(&mut encoded)
        .await
        .map_err(|e| format!("lecture de trame LAN: {e}"))?;
    ciborium::from_reader(Cursor::new(encoded)).map_err(|e| format!("décodage de trame LAN: {e}"))
}

fn nonce_for(sequence: u64) -> [u8; 12] {
    let mut nonce = [0; 12];
    nonce[4..].copy_from_slice(&sequence.to_be_bytes());
    nonce
}

fn frame_aad(node_id: &str, work_id: &str, sequence: u64) -> Vec<u8> {
    format!("aos-lan-v1\0{node_id}\0{work_id}\0{sequence}").into_bytes()
}

/// In-process LAN scheduler policy.
///
/// The coordinator deliberately has no socket or discovery side effect. A
/// future authenticated transport must call `plan`, `recover_node_loss` and
/// `cancel`, and must refuse to transmit anything that these methods did not
/// authorize.
#[derive(Debug, Clone, Default)]
pub struct LanCluster {
    registry: LanPairingRegistry,
    jobs: HashMap<String, LanWorkPlan>,
}

impl LanCluster {
    pub fn new(registry: LanPairingRegistry) -> Self {
        Self {
            registry,
            jobs: HashMap::new(),
        }
    }

    pub fn registry(&self) -> &LanPairingRegistry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut LanPairingRegistry {
        &mut self.registry
    }

    /// Partition a model's declared shards across paired LAN nodes.
    pub fn plan(&mut self, work: &DistributedWork, kv_tokens: u32) -> Result<LanWorkPlan, String> {
        if self
            .jobs
            .get(&work.work_id)
            .is_some_and(|plan| matches!(plan.state, LanJobState::Cancelled | LanJobState::Failed))
        {
            return Err("travail LAN terminé : utiliser un nouvel identifiant".into());
        }
        if work.work_id.trim().is_empty() {
            return Err("identifiant de travail LAN vide".into());
        }
        if work.shard_ids.is_empty() {
            return Err("aucun shard déclaré pour le travail LAN".into());
        }
        let nodes: Vec<String> = self
            .registry
            .paired_nodes()
            .map(|node| node.node_id.clone())
            .collect();
        if nodes.is_empty() {
            return Err("aucun nœud LAN appairé".into());
        }
        for node_id in &nodes {
            self.registry.authorize(node_id, work)?;
        }

        let mut assignments: Vec<LanShardAssignment> = nodes
            .iter()
            .map(|node_id| LanShardAssignment {
                node_id: node_id.clone(),
                shard_ids: Vec::new(),
                kv_tokens,
                encrypted_transport: work.encrypted_transport,
            })
            .collect();
        let assignment_count = assignments.len();
        for (index, shard_id) in work.shard_ids.iter().copied().enumerate() {
            assignments[index % assignment_count]
                .shard_ids
                .push(shard_id);
        }
        assignments.retain(|assignment| !assignment.shard_ids.is_empty());
        let plan = LanWorkPlan {
            work_id: work.work_id.clone(),
            model_id: work.model_id.clone(),
            state: LanJobState::Planned,
            assignments,
            unassigned_shards: Vec::new(),
        };
        self.jobs.insert(work.work_id.clone(), plan.clone());
        Ok(plan)
    }

    pub fn set_running(&mut self, work_id: &str) -> Result<(), String> {
        let plan = self.jobs.get_mut(work_id).ok_or("travail LAN inconnu")?;
        if plan.state == LanJobState::Cancelled || plan.state == LanJobState::Failed {
            return Err("travail LAN terminé".into());
        }
        plan.state = LanJobState::Running;
        Ok(())
    }

    /// Remove a lost node and redistribute its shards to surviving paired nodes.
    pub fn recover_node_loss(
        &mut self,
        work_id: &str,
        lost_node_id: &str,
    ) -> Result<LanRecovery, String> {
        let plan = self.jobs.get_mut(work_id).ok_or("travail LAN inconnu")?;
        if matches!(plan.state, LanJobState::Cancelled | LanJobState::Failed) {
            return Err("travail LAN terminé".into());
        }
        let lost = plan
            .assignments
            .iter()
            .position(|assignment| assignment.node_id == lost_node_id)
            .ok_or("nœud absent du travail LAN")?;
        let mut shards = plan.assignments.remove(lost).shard_ids;
        // Revoked assignments are no longer usable either. Preserve their
        // shards in recovery rather than leaving them attached to stale peers.
        plan.assignments.retain(|assignment| {
            let paired = self
                .registry
                .get(&assignment.node_id)
                .is_some_and(|node| node.trust == NodeTrust::Paired);
            if !paired {
                shards.extend_from_slice(&assignment.shard_ids);
            }
            paired
        });
        let survivors: Vec<String> = plan
            .assignments
            .iter()
            .filter(|assignment| {
                self.registry
                    .get(&assignment.node_id)
                    .is_some_and(|node| node.trust == NodeTrust::Paired)
            })
            .map(|assignment| assignment.node_id.clone())
            .collect();
        if survivors.is_empty() {
            plan.state = LanJobState::Failed;
            plan.unassigned_shards = shards.clone();
            return Ok(LanRecovery {
                work_id: work_id.into(),
                lost_node_id: lost_node_id.into(),
                state: plan.state,
                reassigned_shards: Vec::new(),
                cancelled_nodes: Vec::new(),
            });
        }
        for (index, shard_id) in shards.iter().copied().enumerate() {
            let node_id = &survivors[index % survivors.len()];
            if let Some(assignment) = plan
                .assignments
                .iter_mut()
                .find(|assignment| &assignment.node_id == node_id)
            {
                assignment.shard_ids.push(shard_id);
            }
        }
        plan.state = LanJobState::Degraded;
        Ok(LanRecovery {
            work_id: work_id.into(),
            lost_node_id: lost_node_id.into(),
            state: plan.state,
            reassigned_shards: shards,
            cancelled_nodes: Vec::new(),
        })
    }

    /// Mark the job cancelled and return every node that must receive a cancel.
    pub fn cancel(&mut self, work_id: &str) -> Result<LanRecovery, String> {
        let plan = self.jobs.get_mut(work_id).ok_or("travail LAN inconnu")?;
        plan.state = LanJobState::Cancelled;
        Ok(LanRecovery {
            work_id: work_id.into(),
            lost_node_id: String::new(),
            state: plan.state,
            reassigned_shards: Vec::new(),
            cancelled_nodes: plan
                .assignments
                .iter()
                .map(|assignment| assignment.node_id.clone())
                .collect(),
        })
    }

    pub fn job(&self, work_id: &str) -> Option<&LanWorkPlan> {
        self.jobs.get(work_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node() -> LanNode {
        LanNode {
            node_id: "n1".into(),
            display_name: "laptop".into(),
            address: "192.168.1.2:9000".into(),
            public_key_fingerprint: "abc".into(),
            trust: NodeTrust::Unpaired,
            capabilities: vec![],
        }
    }

    #[test]
    fn discovery_ne_authorise_pas_le_routage() {
        let mut registry = LanPairingRegistry::default();
        registry.discover(node());
        let work = DistributedWork {
            work_id: "w".into(),
            model_id: "m".into(),
            shard_ids: vec![1],
            allow_sensitive_data: false,
            encrypted_transport: true,
        };
        assert!(registry.authorize("n1", &work).is_err());
        registry.pair("n1", "abc").unwrap();
        assert!(registry.authorize("n1", &work).is_ok());
        registry.revoke("n1");
        assert!(registry.authorize("n1", &work).is_err());
    }

    #[test]
    fn cluster_repartit_reprend_et_annule_un_travail() {
        let mut registry = LanPairingRegistry::default();
        for (id, ip) in [("n1", "192.168.1.2:9000"), ("n2", "192.168.1.3:9000")] {
            registry.discover(LanNode {
                node_id: id.into(),
                display_name: id.into(),
                address: ip.into(),
                public_key_fingerprint: format!("{id}-key"),
                trust: NodeTrust::Unpaired,
                capabilities: vec![],
            });
            registry.pair(id, &format!("{id}-key")).unwrap();
        }
        let mut cluster = LanCluster::new(registry);
        let work = DistributedWork {
            work_id: "w1".into(),
            model_id: "m".into(),
            shard_ids: vec![1, 2, 3, 4],
            allow_sensitive_data: false,
            encrypted_transport: true,
        };
        let plan = cluster.plan(&work, 4096).unwrap();
        assert_eq!(plan.model_id, "m");
        assert_eq!(plan.assignments.len(), 2);
        assert_eq!(
            plan.assignments
                .iter()
                .map(|a| a.shard_ids.len())
                .sum::<usize>(),
            4
        );
        cluster.set_running("w1").unwrap();
        let recovery = cluster.recover_node_loss("w1", "n1").unwrap();
        assert_eq!(recovery.reassigned_shards.len(), 2);
        assert_eq!(cluster.job("w1").unwrap().state, LanJobState::Degraded);
        let cancelled = cluster.cancel("w1").unwrap();
        assert_eq!(cancelled.state, LanJobState::Cancelled);
        assert_eq!(cancelled.cancelled_nodes, vec!["n2"]);
        assert!(cluster.recover_node_loss("w1", "n2").is_err());
        assert!(cluster.plan(&work, 4096).is_err());
        assert_eq!(cluster.job("w1").unwrap().state, LanJobState::Cancelled);
        assert_eq!(cluster.job("w1").unwrap().assignments.len(), 1);

        let second = DistributedWork {
            work_id: "w2".into(),
            ..work
        };
        cluster.plan(&second, 4096).unwrap();
        cluster.registry_mut().revoke("n2");
        cluster.recover_node_loss("w2", "n1").unwrap();
        let failed = cluster.job("w2").unwrap();
        assert_eq!(failed.state, LanJobState::Failed);
        assert!(failed.assignments.is_empty());
        let mut missing = failed.unassigned_shards.clone();
        missing.sort_unstable();
        assert_eq!(missing, vec![1, 2, 3, 4]);
        assert!(cluster.plan(&second, 4096).is_err());
    }

    #[test]
    fn manifeste_poids_refuse_recouvrement_et_signale_les_trous() {
        let mut manifest = LanShardManifest::new("model-1", 100).unwrap();
        manifest
            .record_range(LanWeightRange {
                shard_id: 1,
                offset: 40,
                length: 20,
            })
            .unwrap();
        assert_eq!(manifest.missing_ranges(), vec![(0, 40), (60, 40)]);
        assert!(!manifest.is_complete());
        assert!(manifest
            .record_range(LanWeightRange {
                shard_id: 2,
                offset: 50,
                length: 10,
            })
            .is_err());
        manifest
            .record_range(LanWeightRange {
                shard_id: 0,
                offset: 0,
                length: 40,
            })
            .unwrap();
        manifest
            .record_range(LanWeightRange {
                shard_id: 2,
                offset: 60,
                length: 40,
            })
            .unwrap();
        assert!(manifest.is_complete());
        assert!(manifest.missing_ranges().is_empty());
    }

    #[test]
    fn manifeste_persistant_invalide_est_refuse_sans_panique() {
        let mut manifest = LanShardManifest::new("model", u64::MAX).unwrap();
        assert!(manifest
            .record_range(LanWeightRange {
                shard_id: 1,
                offset: u64::MAX,
                length: 1,
            })
            .is_err());
        // Public fields and deserialization bypass record_range.
        manifest.ranges.push(LanWeightRange {
            shard_id: 1,
            offset: 0,
            length: u64::MAX,
        });
        assert!(manifest.validate().is_err());
        assert!(!manifest.is_complete());
        assert_eq!(manifest.missing_ranges(), vec![(0, u64::MAX)]);
        assert!(manifest
            .record_range(LanWeightRange {
                shard_id: 2,
                offset: 0,
                length: 1,
            })
            .is_err());
    }

    #[test]
    fn identite_changee_est_refusee_et_etat_paire_est_conserve() {
        let mut registry = LanPairingRegistry::default();
        registry.discover(node());
        registry.pair("n1", "abc").unwrap();
        let mut changed = node();
        changed.address = "192.168.1.9:9000".into();
        changed.public_key_fingerprint = "evil".into();
        assert!(registry.try_discover(changed).is_err());
        let mut moved = node();
        moved.address = "192.168.1.9:9000".into();
        registry.try_discover(moved).unwrap();
        assert_eq!(registry.get("n1").unwrap().trust, NodeTrust::Paired);
        assert_eq!(registry.snapshot().len(), 1);
    }

    #[test]
    fn canal_chiffre_verifie_le_job_et_rejette_le_rejeu() {
        let key = LanSessionKey::from_bytes(&[7; 32]).unwrap();
        let mut sender = LanSecureChannel::new(key.clone(), "coordinator", "n1", "work");
        let mut receiver = LanSecureChannel::new(key, "n1", "coordinator", "work");
        let frame = sender.send(b"shard payload").unwrap();
        assert_eq!(receiver.receive(&frame).unwrap(), b"shard payload");
        assert!(receiver.receive(&frame).is_err());

        let mut wrong_job = frame.clone();
        wrong_job.work_id = "other-work".into();
        assert!(receiver.receive(&wrong_job).is_err());
        assert!(LanSessionKey::from_bytes(&[0; 31]).is_err());
    }

    #[test]
    fn protocole_lan_valide_le_noeud_les_shards_et_le_job() {
        let work = DistributedWork {
            work_id: "work-1".into(),
            model_id: "model-1".into(),
            shard_ids: vec![1, 2],
            allow_sensitive_data: false,
            encrypted_transport: true,
        };
        let hello = LanWorkMessage::Hello {
            work_id: "work-1".into(),
            node_id: "n1".into(),
            protocol_version: 1,
        };
        hello.validate_for(&work, "n1").unwrap();

        let assignment = LanWorkMessage::Assign {
            work_id: "work-1".into(),
            model_id: "model-1".into(),
            assignment: LanShardAssignment {
                node_id: "n1".into(),
                shard_ids: vec![1],
                kv_tokens: 128,
                encrypted_transport: true,
            },
            allow_sensitive_data: false,
        };
        let mut encoded = Vec::new();
        ciborium::into_writer(&assignment, &mut encoded).unwrap();
        let decoded: LanWorkMessage = ciborium::from_reader(Cursor::new(encoded)).unwrap();
        decoded.validate_for(&work, "n1").unwrap();

        let mut invalid = assignment.clone();
        if let LanWorkMessage::Assign { assignment, .. } = &mut invalid {
            assignment.shard_ids = vec![99];
        }
        assert!(invalid.validate_for(&work, "n1").is_err());
    }

    #[test]
    fn worker_enregistre_assignment_et_annulation() {
        let mut worker = LanWorkerRegistry::default();
        worker
            .assign(
                "n1",
                "coordinator",
                "model-1".into(),
                LanShardAssignment {
                    node_id: "n1".into(),
                    shard_ids: vec![3, 4],
                    kv_tokens: 512,
                    encrypted_transport: true,
                },
                "work-1".into(),
                false,
            )
            .unwrap();
        assert_eq!(worker.get("work-1").unwrap().shard_ids, vec![3, 4]);
        assert_eq!(worker.snapshot().len(), 1);
        worker.cancel("work-1").unwrap();
        assert_eq!(
            worker.get("work-1").unwrap().state,
            LanWorkerJobState::Cancelled
        );
        assert!(worker
            .assign(
                "n2",
                "coordinator",
                "model-1".into(),
                LanShardAssignment {
                    node_id: "n1".into(),
                    shard_ids: vec![1],
                    kv_tokens: 0,
                    encrypted_transport: true,
                },
                "work-2".into(),
                false,
            )
            .is_err());
    }

    #[test]
    fn annulation_worker_propage_le_drapeau_actif() {
        let mut worker = LanWorkerRegistry::default();
        worker
            .assign(
                "n1",
                "coordinator",
                "model-1".into(),
                LanShardAssignment {
                    node_id: "n1".into(),
                    shard_ids: vec![1],
                    kv_tokens: 128,
                    encrypted_transport: true,
                },
                "work-cancel".into(),
                true,
            )
            .unwrap();
        let abort = Arc::new(AtomicBool::new(false));
        worker.register_abort("work-cancel", abort.clone()).unwrap();
        let second = Arc::new(AtomicBool::new(false));
        worker
            .register_abort("work-cancel", second.clone())
            .unwrap();
        worker.cancel("work-cancel").unwrap();
        assert!(abort.load(Ordering::SeqCst));
        assert!(second.load(Ordering::SeqCst));
        assert!(worker.reset_abort("work-cancel").is_err());
        assert!(worker
            .assign(
                "n1",
                "coordinator",
                "model-1".into(),
                LanShardAssignment {
                    node_id: "n1".into(),
                    shard_ids: vec![1],
                    kv_tokens: 128,
                    encrypted_transport: true
                },
                "work-cancel".into(),
                true
            )
            .is_err());
        assert_eq!(
            worker.get("work-cancel").unwrap().state,
            LanWorkerJobState::Cancelled
        );
    }

    #[test]
    fn data_plane_exige_une_politique_et_borne_les_payloads() {
        let private_work = DistributedWork {
            work_id: "data-work".into(),
            model_id: "model-1".into(),
            shard_ids: vec![1],
            allow_sensitive_data: false,
            encrypted_transport: true,
        };
        let prefill = LanWorkMessage::Prefill {
            work_id: "data-work".into(),
            request_id: "req-1".into(),
            input_tokens: vec![1, 2, 3],
            kv_tokens: 128,
        };
        assert!(prefill.validate_for(&private_work, "n1").is_err());

        let sensitive_work = DistributedWork {
            allow_sensitive_data: true,
            ..private_work.clone()
        };
        assert!(prefill.validate_for(&sensitive_work, "n1").is_ok());
        assert!(LanWorkMessage::Prefill {
            work_id: "data-work".into(),
            request_id: "req-1".into(),
            input_tokens: Vec::new(),
            kv_tokens: 128,
        }
        .validate_for(&sensitive_work, "n1")
        .is_err());
        let chat = LanWorkMessage::ChatInfer {
            work_id: "data-work".into(),
            request_id: "req-2".into(),
            messages: vec![LanChatMessage {
                role: "user".into(),
                content: "bonjour".into(),
            }],
            max_tokens: 32,
            temperature_milli: 700,
            top_p_milli: 950,
            seed: 42,
        };
        assert!(chat.validate_for(&sensitive_work, "n1").is_ok());
        assert!(chat.validate_for(&private_work, "n1").is_err());
        assert!(LanWorkMessage::ChatInfer {
            work_id: "data-work".into(),
            request_id: "req-2".into(),
            messages: vec![LanChatMessage {
                role: "user".into(),
                content: "bonjour".into(),
            }],
            max_tokens: 32,
            temperature_milli: 700,
            top_p_milli: 0,
            seed: 42,
        }
        .validate_for(&sensitive_work, "n1")
        .is_err());
        assert!(LanWorkMessage::KvRequest {
            work_id: "data-work".into(),
            request_id: "req-kv".into(),
            seq_id: 0,
        }
        .validate_for(&private_work, "n1")
        .is_err());
        assert!(LanWorkMessage::KvRequest {
            work_id: "data-work".into(),
            request_id: "req-kv".into(),
            seq_id: 0,
        }
        .validate_for(&sensitive_work, "n1")
        .is_ok());
        assert!(LanWorkMessage::WeightRequest {
            work_id: "data-work".into(),
            request_id: "weight-unknown".into(),
            shard_id: 2,
            offset: 0,
            length: 4096,
            total_model_bytes: 8192,
        }
        .validate_for(&sensitive_work, "n1")
        .is_err());
        assert!(LanWorkMessage::WeightPage {
            work_id: "data-work".into(),
            request_id: "weight-unknown".into(),
            shard_id: 2,
            page_index: 0,
            offset: 0,
            data: vec![7; 4096],
            final_page: true,
        }
        .validate_for(&sensitive_work, "n1")
        .is_err());
        assert!(LanWorkMessage::WeightRequest {
            work_id: "data-work".into(),
            request_id: "weight-1".into(),
            shard_id: 1,
            offset: 0,
            length: 4096,
            total_model_bytes: 8192,
        }
        .validate_for(&sensitive_work, "n1")
        .is_ok());
        assert!(LanWorkMessage::WeightPage {
            work_id: "data-work".into(),
            request_id: "weight-1".into(),
            shard_id: 1,
            page_index: 0,
            offset: 0,
            data: vec![7; 4096],
            final_page: true,
        }
        .validate_for(&sensitive_work, "n1")
        .is_ok());
        assert!(LanWorkMessage::WeightRequest {
            work_id: "data-work".into(),
            request_id: "weight-1".into(),
            shard_id: 1,
            offset: 8192,
            length: 1,
            total_model_bytes: 8192,
        }
        .validate_for(&sensitive_work, "n1")
        .is_err());
        assert!(LanWorkMessage::KvPage {
            work_id: "data-work".into(),
            request_id: "req-kv".into(),
            page_index: 1,
            data: vec![1],
            seq0_tokens: vec![42],
            final_page: true,
        }
        .validate_for(&sensitive_work, "n1")
        .is_err());
        assert!(LanWorkMessage::KvPage {
            work_id: "data-work".into(),
            request_id: "req-1".into(),
            page_index: 0,
            data: vec![0; 1_048_577],
            seq0_tokens: Vec::new(),
            final_page: true,
        }
        .validate_for(&sensitive_work, "n1")
        .is_err());
        assert!(LanWorkMessage::Nack {
            work_id: "data-work".into(),
            operation: String::new(),
            reason: "refus".into(),
        }
        .validate_for(&sensitive_work, "n1")
        .is_err());
    }

    #[tokio::test]
    async fn transport_tcp_echange_des_trames_chiffrees() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let key_bytes = [9; 32];
        let server_key = LanSessionKey::from_bytes(&key_bytes).unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut transport =
                LanTcpTransport::from_stream(stream, "n1", "coordinator", "work-tcp", server_key);
            assert_eq!(transport.receive().await.unwrap(), b"hello");
            transport.send(b"ack").await.unwrap();
        });

        let mut registry = LanPairingRegistry::default();
        registry.discover(LanNode {
            node_id: "n1".into(),
            display_name: "worker".into(),
            address: address.to_string(),
            public_key_fingerprint: "tcp-key".into(),
            trust: NodeTrust::Unpaired,
            capabilities: vec![],
        });
        registry.pair("n1", "tcp-key").unwrap();
        let work = DistributedWork {
            work_id: "work-tcp".into(),
            model_id: "m".into(),
            shard_ids: vec![1],
            allow_sensitive_data: false,
            encrypted_transport: true,
        };
        let key = LanSessionKey::from_bytes(&key_bytes).unwrap();
        let mut client = LanTcpTransport::connect(
            "coordinator",
            "n1",
            &address.to_string(),
            &registry,
            &work,
            key,
        )
        .await
        .unwrap();
        client.send(b"hello").await.unwrap();
        assert_eq!(client.receive().await.unwrap(), b"ack");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn listener_tcp_exige_un_handshake_et_un_noeud_appaire() {
        let work = DistributedWork {
            work_id: "work-handshake".into(),
            model_id: "m".into(),
            shard_ids: vec![1],
            allow_sensitive_data: false,
            encrypted_transport: true,
        };
        let key_bytes = [5; 32];
        let server_key = LanSessionKey::from_bytes(&key_bytes).unwrap();
        let client_key = LanSessionKey::from_bytes(&key_bytes).unwrap();

        let mut server_registry = LanPairingRegistry::default();
        server_registry.discover(LanNode {
            node_id: "coordinator".into(),
            display_name: "coordinator".into(),
            address: "127.0.0.1:1".into(),
            public_key_fingerprint: "coordinator-key".into(),
            trust: NodeTrust::Unpaired,
            capabilities: vec![],
        });
        server_registry
            .pair("coordinator", "coordinator-key")
            .unwrap();
        let listener = LanTcpListener::bind("n1", "127.0.0.1:0", &server_registry)
            .await
            .unwrap();
        let address = listener.local_addr().unwrap().to_string();

        let server_work = work.clone();
        let server = tokio::spawn(async move {
            let mut transport = listener
                .accept_authenticated("coordinator", &server_work, server_key)
                .await
                .unwrap();
            let message = transport.receive_message(&server_work).await.unwrap();
            assert!(matches!(message, LanWorkMessage::Assign { .. }));
        });

        let mut client_registry = LanPairingRegistry::default();
        client_registry.discover(LanNode {
            node_id: "n1".into(),
            display_name: "worker".into(),
            address: address.clone(),
            public_key_fingerprint: "worker-key".into(),
            trust: NodeTrust::Unpaired,
            capabilities: vec![],
        });
        client_registry.pair("n1", "worker-key").unwrap();
        let mut client = LanTcpTransport::connect_authenticated(
            "coordinator",
            "n1",
            &address,
            &client_registry,
            &work,
            client_key,
        )
        .await
        .unwrap();
        client
            .send_message(
                &LanWorkMessage::Assign {
                    work_id: work.work_id.clone(),
                    model_id: work.model_id.clone(),
                    assignment: LanShardAssignment {
                        node_id: "n1".into(),
                        shard_ids: vec![1],
                        kv_tokens: 0,
                        encrypted_transport: true,
                    },
                    allow_sensitive_data: false,
                },
                &work,
            )
            .await
            .unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn listener_worker_accepte_un_travail_dont_l_id_est_inconnu() {
        let work = DistributedWork {
            work_id: "worker-work".into(),
            model_id: "model-1".into(),
            shard_ids: vec![7],
            allow_sensitive_data: false,
            encrypted_transport: true,
        };
        let key_bytes = [6; 32];
        let server_key = LanSessionKey::from_bytes(&key_bytes).unwrap();
        let client_key = LanSessionKey::from_bytes(&key_bytes).unwrap();

        let mut server_registry = LanPairingRegistry::default();
        server_registry.discover(LanNode {
            node_id: "coordinator".into(),
            display_name: "coordinator".into(),
            address: "127.0.0.1:1".into(),
            public_key_fingerprint: "coordinator-key".into(),
            trust: NodeTrust::Unpaired,
            capabilities: vec![],
        });
        server_registry
            .pair("coordinator", "coordinator-key")
            .unwrap();
        let listener = LanTcpListener::bind("n1", "127.0.0.1:0", &LanPairingRegistry::default())
            .await
            .unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let server_work = work.clone();

        let server = tokio::spawn(async move {
            let mut transport = listener
                .accept_authenticated_any_work_with_registry(&server_registry, server_key)
                .await
                .unwrap();
            let message = transport.receive_message_unchecked().await.unwrap();
            let LanWorkMessage::Assign { assignment, .. } = message else {
                panic!("assignment attendue");
            };
            assert_eq!(assignment.shard_ids, vec![7]);
            transport
                .send_message(
                    &LanWorkMessage::Ack {
                        work_id: server_work.work_id.clone(),
                        operation: "assign".into(),
                    },
                    &server_work,
                )
                .await
                .unwrap();
        });

        let mut client_registry = LanPairingRegistry::default();
        client_registry.discover(LanNode {
            node_id: "n1".into(),
            display_name: "worker".into(),
            address: address.clone(),
            public_key_fingerprint: "worker-key".into(),
            trust: NodeTrust::Unpaired,
            capabilities: vec![],
        });
        client_registry.pair("n1", "worker-key").unwrap();
        let mut client = LanTcpTransport::connect_authenticated(
            "coordinator",
            "n1",
            &address,
            &client_registry,
            &work,
            client_key,
        )
        .await
        .unwrap();
        client
            .send_message(
                &LanWorkMessage::Assign {
                    work_id: work.work_id.clone(),
                    model_id: work.model_id.clone(),
                    assignment: LanShardAssignment {
                        node_id: "n1".into(),
                        shard_ids: vec![7],
                        kv_tokens: 64,
                        encrypted_transport: true,
                    },
                    allow_sensitive_data: false,
                },
                &work,
            )
            .await
            .unwrap();
        assert!(matches!(
            client.receive_message(&work).await.unwrap(),
            LanWorkMessage::Ack { operation, .. } if operation == "assign"
        ));
        server.await.unwrap();
    }
}
