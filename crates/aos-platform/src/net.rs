//! Network Egress Control (§9.5, F-SEC-08) : deny-by-default, capacités
//! `net.connect:<host>:<port>`, mode offline strict, journal d'egress pour
//! le monitoring (Gate P3).
//!
//! Note d'architecture v1 : tout l'egress du système transite par les
//! services (Backend Manager pour les modèles distants, modules via host
//! calls) — le point de contrôle unique est donc effectif en userspace.

use aos_proto::EgressEntry;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NetMode {
    #[default]
    Online,
    OfflineStrict,
}

/// Le contrôleur d'egress.
#[derive(Default)]
pub struct EgressControl {
    mode: NetMode,
    log: Vec<EgressEntry>,
    /// Caps `net.connect` accordées globalement (config).
    granted: HashSet<String>,
}

impl EgressControl {
    pub fn new() -> Self {
        Self {
            mode: NetMode::Online,
            log: Vec::new(),
            granted: HashSet::new(),
        }
    }

    pub fn set_mode(&mut self, mode: NetMode) {
        self.mode = mode;
    }

    pub fn mode(&self) -> NetMode {
        self.mode
    }

    /// Grant d'une cap réseau (install de module, config backend).
    pub fn grant(&mut self, cap: String) {
        self.granted.insert(cap);
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    /// `net.connect` présent dans `caps` pour ce host:port ?
    fn has_cap(caps: &[String], host: &str, port: u16) -> bool {
        caps.iter().any(|c| {
            c.strip_prefix("net.connect:")
                .map(|p| {
                    // formes acceptées: host:port, host:*, *:443
                    match p.rsplit_once(':') {
                        Some((h, pport)) => {
                            let port_ok = pport == "*" || pport == port.to_string();
                            let host_ok = h == "*" || h == host;
                            host_ok && port_ok
                        }
                        None => false,
                    }
                })
                .unwrap_or(false)
        })
    }

    fn is_loopback(host: &str) -> bool {
        matches!(
            host,
            "127.0.0.1" | "localhost" | "::1" | "[::1]"
        )
    }

    /// Vérifie (et journalise) une demande de connexion sortante.
    pub fn check(&mut self, actor: &str, host: &str, port: u16, caps: &[String]) -> bool {
        let allowed = match self.mode {
            NetMode::OfflineStrict => Self::is_loopback(host),
            NetMode::Online => {
                Self::is_loopback(host)
                    || Self::has_cap(caps, host, port)
                    || Self::has_cap(
                        &self.granted.iter().cloned().collect::<Vec<_>>(),
                        host,
                        port,
                    )
            }
        };
        self.log.push(EgressEntry {
            ts_ms: Self::now_ms(),
            actor: actor.into(),
            host: host.into(),
            port,
            allowed,
        });
        allowed
    }

    /// Journal d'egress (monitoring).
    pub fn log(&self) -> &[EgressEntry] {
        &self.log
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_par_defaut() {
        let mut e = EgressControl::new();
        assert!(!e.check("module:x", "api.example.com", 443, &[]));
        assert!(!e.log()[0].allowed);
    }

    #[test]
    fn cap_explicite_autorise() {
        let mut e = EgressControl::new();
        let caps = vec!["net.connect:api.example.com:443".to_string()];
        assert!(e.check("module:x", "api.example.com", 443, &caps));
        // Mauvais hôte → refus.
        assert!(!e.check("module:x", "autre.com", 443, &caps));
        // Mauvais port → refus.
        assert!(!e.check("module:x", "api.example.com", 80, &caps));
    }

    #[test]
    fn offline_strict_coupe_tout() {
        let mut e = EgressControl::new();
        e.grant("net.connect:*:443".into());
        assert!(e.check("service:modeld", "api.openai.com", 443, &[]));
        e.set_mode(NetMode::OfflineStrict);
        assert!(!e.check("service:modeld", "api.openai.com", 443, &[]));
    }

    #[test]
    fn loopback_compte_comme_local() {
        let mut e = EgressControl::new();
        e.set_mode(NetMode::OfflineStrict);
        assert!(e.check("service:modeld", "127.0.0.1", 11434, &[]));
        assert!(e.check("service:modeld", "localhost", 1234, &[]));
        assert!(!e.check("service:modeld", "api.openai.com", 443, &[]));
    }
}
