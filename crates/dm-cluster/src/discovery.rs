use dm_core::config::PeerConfig;
use std::net::SocketAddr;
use tracing::warn;

/// Static peer discovery from configuration.
pub struct StaticDiscovery {
    peers: Vec<PeerConfig>,
}

impl StaticDiscovery {
    /// Create a new static discovery from config peers.
    pub fn new(peers: Vec<PeerConfig>) -> Self {
        Self { peers }
    }

    /// Resolve peer addresses to SocketAddr.
    /// Logs warnings for invalid addresses but doesn't fail.
    pub fn resolve(&self) -> Vec<(String, SocketAddr)> {
        let mut resolved = Vec::new();

        for peer in &self.peers {
            match peer.addr.parse::<SocketAddr>() {
                Ok(addr) => {
                    resolved.push((peer.addr.clone(), addr));
                }
                Err(e) => {
                    warn!("Invalid peer address '{}': {}", peer.addr, e);
                }
            }
        }

        resolved
    }

    /// Get raw peer configs.
    pub fn peers(&self) -> &[PeerConfig] {
        &self.peers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_valid_addresses() {
        let peers = vec![
            PeerConfig {
                addr: "192.168.1.1:9100".to_string(),
                timeout_secs: None,
            },
            PeerConfig {
                addr: "10.0.0.2:9100".to_string(),
                timeout_secs: Some(10),
            },
        ];

        let discovery = StaticDiscovery::new(peers);
        let resolved = discovery.resolve();
        assert_eq!(resolved.len(), 2);
    }

    #[test]
    fn test_resolve_invalid_address() {
        let peers = vec![
            PeerConfig {
                addr: "invalid".to_string(),
                timeout_secs: None,
            },
            PeerConfig {
                addr: "192.168.1.1:9100".to_string(),
                timeout_secs: None,
            },
        ];

        let discovery = StaticDiscovery::new(peers);
        let resolved = discovery.resolve();
        assert_eq!(resolved.len(), 1); // Only valid address
    }
}
