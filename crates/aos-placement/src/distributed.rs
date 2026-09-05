//! Safe LAN distribution contracts.
//!
//! This is intentionally a policy layer, not a listener. A transport may be
//! added later, but it must use these states so an unpaired/revoked node can
//! never receive a job by discovery alone.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

#[derive(Debug, Clone, Default)]
pub struct LanPairingRegistry {
    nodes: HashMap<String, LanNode>,
}

impl LanPairingRegistry {
    pub fn discover(&mut self, node: LanNode) {
        self.nodes.entry(node.node_id.clone()).or_insert(node);
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
        self.nodes.get_mut(node_id).map(|node| {
            node.trust = NodeTrust::Revoked;
            true
        }).unwrap_or(false)
    }

    pub fn authorize(&self, node_id: &str, work: &DistributedWork) -> Result<(), String> {
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
        if !work.encrypted_transport {
            return Err("transport chiffré obligatoire".into());
        }
        if work.allow_sensitive_data && !node.capabilities.iter().any(|cap| cap == "sensitive-data") {
            return Err("politique sensible non autorisée pour ce nœud".into());
        }
        Ok(())
    }

    pub fn get(&self, node_id: &str) -> Option<&LanNode> {
        self.nodes.get(node_id)
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
        let work = DistributedWork { work_id: "w".into(), model_id: "m".into(), shard_ids: vec![1], allow_sensitive_data: false, encrypted_transport: true };
        assert!(registry.authorize("n1", &work).is_err());
        registry.pair("n1", "abc").unwrap();
        assert!(registry.authorize("n1", &work).is_ok());
        registry.revoke("n1");
        assert!(registry.authorize("n1", &work).is_err());
    }
}
