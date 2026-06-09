use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::grpc::client::GrpcClient;
use crate::types::{ClusterEvent, NodeHeartbeat};

/// Connection to a peer node
struct PeerConnection {
    #[allow(dead_code)]
    peer_id: String,
    peer_addr: String,
    client: Option<GrpcClient>,
    connected: bool,
    last_error: Option<String>,
}

/// Manages gRPC connections to all peer nodes
#[derive(Clone)]
pub struct PeerManager {
    inner: Arc<RwLock<PeerManagerInner>>,
    #[allow(dead_code)]
    local_node_id: String,
}

struct PeerManagerInner {
    peers: HashMap<String, PeerConnection>,
}

impl PeerManager {
    /// Create a new PeerManager
    pub async fn new(
        local_node_id: String,
        peers: Vec<(String, String)>, // (peer_id, peer_addr)
    ) -> Result<Self, String> {
        let mut peer_map = HashMap::new();

        for (peer_id, peer_addr) in peers {
            info!("Connecting to peer {} at {}", peer_id, peer_addr);
            let client = match GrpcClient::connect(&peer_addr).await {
                Ok(client) => {
                    info!("Connected to peer {} at {}", peer_id, peer_addr);
                    Some(client)
                }
                Err(e) => {
                    warn!("Failed to connect to peer {} at {}: {}", peer_id, peer_addr, e);
                    None
                }
            };

            peer_map.insert(peer_id.clone(), PeerConnection {
                peer_id,
                peer_addr,
                client,
                connected: true,
                last_error: None,
            });
        }

        Ok(Self {
            inner: Arc::new(RwLock::new(PeerManagerInner { peers: peer_map })),
            local_node_id,
        })
    }

    /// Publish event to all peers
    pub async fn publish_event_to_all(&self, event: &ClusterEvent) -> Result<(), Vec<String>> {
        let errors = Vec::new();
        let mut inner = self.inner.write().await;

        for (peer_id, conn) in &mut inner.peers {
            if let Some(client) = &mut conn.client {
                if let Err(e) = client.publish_event(event).await {
                    tracing::warn!("Failed to publish event to {}: {}", peer_id, e);
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Send heartbeat to all peers
    pub async fn send_heartbeat_to_all(&self, heartbeat: &NodeHeartbeat) -> Result<(), Vec<String>> {
        let errors = Vec::new();
        let mut inner = self.inner.write().await;

        for (peer_id, conn) in &mut inner.peers {
            if let Some(client) = &mut conn.client {
                if let Err(e) = client.heartbeat(heartbeat).await {
                    tracing::warn!("Failed to send heartbeat to {}: {}", peer_id, e);
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Get client for a specific peer
    pub async fn client_for(&self, peer_id: &str) -> Option<GrpcClient> {
        let inner = self.inner.read().await;
        inner.peers.get(peer_id).and_then(|c| c.client.clone())
    }

    /// Check all peers status
    pub async fn check_all_peers(&self) -> HashMap<String, bool> {
        let inner = self.inner.read().await;
        inner.peers.iter().map(|(id, conn)| {
            (id.clone(), conn.connected)
        }).collect()
    }

    /// Spawn background reconnection task
    pub fn spawn_reconnect_task(&self) {
        let manager = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                manager.reconnect_failed_peers().await;
            }
        });
    }

    /// Reconnect failed peers
    async fn reconnect_failed_peers(&self) {
        let mut inner = self.inner.write().await;
        for (peer_id, conn) in &mut inner.peers {
            if conn.client.is_none() {
                match GrpcClient::connect(&conn.peer_addr).await {
                    Ok(client) => {
                        info!("Reconnected to peer {} at {}", peer_id, conn.peer_addr);
                        conn.client = Some(client);
                        conn.connected = true;
                        conn.last_error = None;
                    }
                    Err(e) => {
                        conn.last_error = Some(e.to_string());
                    }
                }
            }
        }
    }
}
