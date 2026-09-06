//! Safe LAN distribution contracts.
//!
//! This is intentionally a policy layer, not a listener. A transport may be
//! added later, but it must use these states so an unpaired/revoked node can
//! never receive a job by discovery alone.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;

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

/// Messages allowed on an authenticated LAN work channel.
///
/// The payload deliberately contains control-plane data only. Prompts,
/// generated tokens and KV pages need an explicit future data-plane contract;
/// they must not be smuggled through an untyped transport call.
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
}

impl LanWorkMessage {
    pub fn work_id(&self) -> &str {
        match self {
            Self::Hello { work_id, .. }
            | Self::Assign { work_id, .. }
            | Self::Cancel { work_id }
            | Self::Heartbeat { work_id }
            | Self::Ack { work_id, .. } => work_id,
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
    pub state: LanWorkerJobState,
}

/// Runtime state owned by a model worker. This is deliberately limited to
/// the authenticated control plane: actual weight loading and token/KV
/// transfer remain a separate data-plane feature.
#[derive(Debug, Clone, Default)]
pub struct LanWorkerRegistry {
    jobs: HashMap<String, LanWorkerJob>,
}

impl LanWorkerRegistry {
    pub fn assign(
        &mut self,
        local_node_id: &str,
        peer_node_id: &str,
        model_id: String,
        assignment: LanShardAssignment,
        work_id: String,
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
        self.jobs.insert(
            work_id.clone(),
            LanWorkerJob {
                work_id,
                model_id,
                shard_ids: assignment.shard_ids,
                kv_tokens: assignment.kv_tokens,
                state: LanWorkerJobState::Assigned,
            },
        );
        Ok(())
    }

    pub fn cancel(&mut self, work_id: &str) -> Result<(), String> {
        let job = self.jobs.get_mut(work_id).ok_or("travail LAN inconnu")?;
        job.state = LanWorkerJobState::Cancelled;
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
            LanWorkMessage::Assign { .. } => {
                message.validate_for(work, &self.channel.peer_node_id)?
            }
            _ => {}
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
        let lost = plan
            .assignments
            .iter()
            .position(|assignment| assignment.node_id == lost_node_id)
            .ok_or("nœud absent du travail LAN")?;
        let shards = plan.assignments.remove(lost).shard_ids;
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
            )
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
