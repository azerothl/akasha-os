//! Opt-in, unauthenticated LAN advertisements.
//!
//! Discovery is only a candidate mechanism. Advertisements are validated for
//! shape, source address and LAN scope, then become `Unpaired` registry
//! entries. Pairing and transport authentication remain separate operations.

use crate::{LanNode, NodeTrust};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use tokio::net::UdpSocket;

/// Default TCP port advertised when the listen field is empty.
pub const LAN_WORKER_PORT: u16 = 9001;
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
        if !is_lan_ip(source.ip()) {
            return Err("annonce LAN hors réseau local".into());
        }
        if !advertised.ip().is_unspecified() && !is_lan_ip(advertised.ip()) {
            return Err("annonce LAN hors réseau local".into());
        }
        let reachability_ip = if is_wildcard_or_loopback(advertised.ip()) {
            source.ip()
        } else if source.ip() == advertised.ip() {
            advertised.ip()
        } else {
            return Err(format!(
                "adresse source {} différente de l'adresse annoncée {}",
                source.ip(),
                advertised.ip()
            ));
        };
        Ok(LanNode {
            node_id: self.node_id.clone(),
            display_name: self.display_name.clone(),
            address: SocketAddr::new(reachability_ip, advertised.port()).to_string(),
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

fn is_wildcard_or_loopback(ip: IpAddr) -> bool {
    ip.is_unspecified() || ip.is_loopback()
}

/// Best-effort IPv4 of the interface used for LAN broadcast.
pub fn local_lan_ip() -> Option<IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    let _ = socket.set_broadcast(true);
    for target in [
        "255.255.255.255:47821",
        "10.255.255.255:1",
        "192.168.255.255:1",
    ] {
        if socket.connect(target).is_err() {
            continue;
        }
        let Ok(addr) = socket.local_addr() else {
            continue;
        };
        if is_lan_ip(addr.ip()) && !is_wildcard_or_loopback(addr.ip()) {
            return Some(addr.ip());
        }
    }
    None
}

/// Empty, unspecified, or loopback listen values that should be replaced.
pub fn is_placeholder_listen_address(listen: &str) -> bool {
    let trimmed = listen.trim();
    if trimmed.is_empty() {
        return true;
    }
    trimmed
        .parse::<SocketAddr>()
        .map(|addr| is_wildcard_or_loopback(addr.ip()))
        .unwrap_or(false)
}

/// Replace a placeholder listen address with this machine's LAN IP.
///
/// An explicit private or link-local address is left unchanged. When no LAN
/// IP can be detected, the original value is returned (or `127.0.0.1:9001`
/// when the field is empty).
pub fn reachable_lan_address(listen: &str) -> String {
    let trimmed = listen.trim();
    let parsed = trimmed.parse::<SocketAddr>().ok();
    let port = parsed.map(|addr| addr.port()).unwrap_or(LAN_WORKER_PORT);
    let needs_fill = trimmed.is_empty()
        || parsed
            .map(|addr| is_wildcard_or_loopback(addr.ip()))
            .unwrap_or(false);
    if needs_fill {
        if let Some(ip) = local_lan_ip() {
            return SocketAddr::new(ip, port).to_string();
        }
        if trimmed.is_empty() {
            return SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port).to_string();
        }
    }
    if trimmed.is_empty() {
        return SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port).to_string();
    }
    trimmed.to_string()
}

/// Bind address for the worker listener.
///
/// Loopback is rewritten to all-interfaces so a paired LAN peer can connect
/// while the advertised/reachability address stays the observed LAN IP.
pub fn worker_bind_address(listen: &str) -> String {
    let Ok(advertised) = listen.trim().parse::<SocketAddr>() else {
        return listen.trim().to_string();
    };
    if advertised.ip().is_loopback() {
        let wildcard = match advertised.ip() {
            IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
        };
        return SocketAddr::new(wildcard, advertised.port()).to_string();
    }
    listen.trim().to_string()
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
        assert_eq!(node.address, "127.0.0.1:9001");
    }

    #[test]
    fn annonce_loopback_prend_l_ip_source_lan() {
        let node = advertisement()
            .validate_from("192.168.1.20:47821".parse().unwrap())
            .unwrap();
        assert_eq!(node.address, "192.168.1.20:9001");
        assert_eq!(node.trust, NodeTrust::Unpaired);
    }

    #[test]
    fn annonce_wildcard_prend_l_ip_source_lan() {
        let mut ad = advertisement();
        ad.address = "0.0.0.0:9001".into();
        let node = ad
            .validate_from("10.0.0.8:47821".parse().unwrap())
            .unwrap();
        assert_eq!(node.address, "10.0.0.8:9001");
    }

    #[test]
    fn annonce_refuse_une_source_differente() {
        let mut ad = advertisement();
        ad.address = "192.168.1.20:9001".into();
        let error = ad
            .validate_from("192.168.1.21:47821".parse().unwrap())
            .unwrap_err();
        assert!(error.contains("192.168.1.21"));
        assert!(error.contains("192.168.1.20"));
    }

    #[test]
    fn placeholder_listen_detecte_loopback_et_vide() {
        assert!(is_placeholder_listen_address(""));
        assert!(is_placeholder_listen_address("   "));
        assert!(is_placeholder_listen_address("127.0.0.1:9001"));
        assert!(is_placeholder_listen_address("0.0.0.0:9001"));
        assert!(!is_placeholder_listen_address("192.168.1.20:9001"));
    }

    #[test]
    fn worker_bind_ouvre_toutes_les_interfaces_sur_loopback() {
        assert_eq!(worker_bind_address("127.0.0.1:9001"), "0.0.0.0:9001");
        assert_eq!(
            worker_bind_address("192.168.1.20:9001"),
            "192.168.1.20:9001"
        );
    }

    #[test]
    fn reachable_address_preserves_explicit_lan_ip() {
        assert_eq!(
            reachable_lan_address("192.168.1.20:9001"),
            "192.168.1.20:9001"
        );
    }

    #[test]
    fn reachable_address_rewrites_loopback_when_lan_ip_exists() {
        if local_lan_ip().is_some() {
            let filled = reachable_lan_address("127.0.0.1:9001");
            assert!(!filled.starts_with("127.0.0.1"));
            assert!(filled.ends_with(":9001"));
        }
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
