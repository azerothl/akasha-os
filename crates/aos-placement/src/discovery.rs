//! Opt-in, unauthenticated LAN advertisements.
//!
//! Discovery is only a candidate mechanism. Advertisements are validated for
//! shape, source address and LAN scope, then become `Unpaired` registry
//! entries. Pairing and transport authentication remain separate operations.

use crate::{LanNode, NodeTrust};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::net::{IpAddr, SocketAddr};

use tokio::net::UdpSocket;

pub const LAN_DISCOVERY_PROTOCOL_VERSION: u16 = 1;
pub const LAN_DISCOVERY_PORT: u16 = 47_821;
pub const LAN_DISCOVERY_MAGIC: &str = "akasha-os-lan-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanDiscoveryAdvertisement {
    pub magic: String,
    pub protocol_version: u16,
    pub node_id: String,
    pub display_name: String,
    pub address: String,
    pub public_key_fingerprint: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl LanDiscoveryAdvertisement {
    pub fn from_node(node: &LanNode) -> Self {
        Self {
            magic: LAN_DISCOVERY_MAGIC.into(),
            protocol_version: LAN_DISCOVERY_PROTOCOL_VERSION,
            node_id: node.node_id.clone(),
            display_name: node.display_name.clone(),
            address: node.address.clone(),
            public_key_fingerprint: node.public_key_fingerprint.clone(),
            capabilities: node.capabilities.clone(),
        }
    }

    pub fn validate_from(&self, source: SocketAddr) -> Result<LanNode, String> {
        if self.magic != LAN_DISCOVERY_MAGIC {
            return Err("annonce LAN inconnue".into());
        }
        if self.protocol_version != LAN_DISCOVERY_PROTOCOL_VERSION {
            return Err("version d'annonce LAN non supportée".into());
        }
        if self.node_id.trim().is_empty()
            || self.display_name.trim().is_empty()
            || self.public_key_fingerprint.trim().is_empty()
        {
            return Err("annonce LAN incomplète".into());
        }
        let advertised = self
            .address
            .parse::<SocketAddr>()
            .map_err(|_| "adresse annoncée LAN invalide".to_string())?;
        if !is_lan_ip(source.ip()) || !is_lan_ip(advertised.ip()) {
            return Err("annonce LAN hors réseau local".into());
        }
        if source.ip() != advertised.ip() {
            return Err("adresse source différente de l'adresse annoncée".into());
        }
        Ok(LanNode {
            node_id: self.node_id.clone(),
            display_name: self.display_name.clone(),
            address: self.address.clone(),
            public_key_fingerprint: self.public_key_fingerprint.clone(),
            trust: NodeTrust::Unpaired,
            capabilities: self.capabilities.clone(),
        })
    }
}

pub struct LanDiscoverySocket {
    socket: UdpSocket,
    broadcast_address: SocketAddr,
}

impl LanDiscoverySocket {
    pub async fn bind(bind_address: &str, broadcast_address: &str) -> Result<Self, String> {
        let socket = UdpSocket::bind(bind_address)
            .await
            .map_err(|error| format!("écoute découverte LAN impossible: {error}"))?;
        socket
            .set_broadcast(true)
            .map_err(|error| format!("broadcast LAN impossible: {error}"))?;
        let broadcast_address = broadcast_address
            .parse::<SocketAddr>()
            .map_err(|_| "adresse broadcast LAN invalide".to_string())?;
        if !is_lan_ip(broadcast_address.ip()) && !is_broadcast_ip(broadcast_address.ip()) {
            return Err("adresse broadcast LAN hors réseau local".into());
        }
        Ok(Self {
            socket,
            broadcast_address,
        })
    }

    pub async fn announce(&self, advertisement: &LanDiscoveryAdvertisement) -> Result<(), String> {
        let mut encoded = Vec::new();
        ciborium::into_writer(advertisement, &mut encoded)
            .map_err(|error| format!("encodage annonce LAN: {error}"))?;
        if encoded.len() > 16 * 1024 {
            return Err("annonce LAN trop volumineuse".into());
        }
        self.socket
            .send_to(&encoded, self.broadcast_address)
            .await
            .map_err(|error| format!("envoi annonce LAN: {error}"))?;
        Ok(())
    }

    pub async fn receive(&self) -> Result<LanNode, String> {
        let mut buffer = [0u8; 16 * 1024];
        let (length, source) = self
            .socket
            .recv_from(&mut buffer)
            .await
            .map_err(|error| format!("réception annonce LAN: {error}"))?;
        let advertisement: LanDiscoveryAdvertisement =
            ciborium::from_reader(Cursor::new(&buffer[..length]))
                .map_err(|error| format!("décodage annonce LAN: {error}"))?;
        advertisement.validate_from(source)
    }
}

fn is_lan_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || (ip.segments()[0] & 0xfe00) == 0xfc00
                || (ip.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

fn is_broadcast_ip(ip: IpAddr) -> bool {
    matches!(ip, IpAddr::V4(ip) if ip == std::net::Ipv4Addr::BROADCAST)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advertisement() -> LanDiscoveryAdvertisement {
        LanDiscoveryAdvertisement {
            magic: LAN_DISCOVERY_MAGIC.into(),
            protocol_version: LAN_DISCOVERY_PROTOCOL_VERSION,
            node_id: "worker-1".into(),
            display_name: "worker".into(),
            address: "127.0.0.1:9001".into(),
            public_key_fingerprint: "sha256:test".into(),
            capabilities: vec!["sensitive-data".into()],
        }
    }

    #[test]
    fn annonce_valide_devient_un_noeud_non_appaire() {
        let node = advertisement()
            .validate_from("127.0.0.1:47821".parse().unwrap())
            .unwrap();
        assert_eq!(node.node_id, "worker-1");
        assert_eq!(node.trust, NodeTrust::Unpaired);
    }

    #[test]
    fn annonce_refuse_une_source_differente() {
        assert!(advertisement()
            .validate_from("127.0.0.2:47821".parse().unwrap())
            .is_err());
    }

    #[test]
    fn annonce_est_serialisable_en_cbor() {
        let original = advertisement();
        let mut encoded = Vec::new();
        ciborium::into_writer(&original, &mut encoded).unwrap();
        let decoded: LanDiscoveryAdvertisement =
            ciborium::from_reader(Cursor::new(encoded)).unwrap();
        assert_eq!(decoded, original);
    }

    #[tokio::test]
    async fn socket_accepte_le_broadcast_global() {
        LanDiscoverySocket::bind("127.0.0.1:0", "255.255.255.255:47821")
            .await
            .unwrap();
    }
}
