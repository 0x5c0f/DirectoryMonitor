use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Node status in the cluster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeStatus {
    Online,
    Offline,
    Unknown,
}

impl std::fmt::Display for NodeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeStatus::Online => write!(f, "Online"),
            NodeStatus::Offline => write!(f, "Offline"),
            NodeStatus::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Information about a cluster node.
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: String,
    pub name: String,
    pub addr: String,
    pub status: NodeStatus,
    pub last_seen: DateTime<Utc>,
    pub watcher_count: usize,
    pub event_count: u64,
}

/// Registry of known cluster nodes.
#[derive(Clone)]
pub struct NodeRegistry {
    inner: Arc<RwLock<NodeRegistryInner>>,
}

struct NodeRegistryInner {
    /// This node's info.
    local_node: NodeInfo,
    /// Known peer nodes.
    peers: HashMap<String, NodeInfo>,
    /// Timeout duration for marking nodes as offline.
    timeout: Duration,
}

impl NodeRegistry {
    /// Create a new registry with the local node info.
    pub fn new(
        node_id: String,
        node_name: String,
        listen_addr: String,
        timeout_secs: i64,
    ) -> Self {
        let local_node = NodeInfo {
            id: node_id,
            name: node_name,
            addr: listen_addr,
            status: NodeStatus::Online,
            last_seen: Utc::now(),
            watcher_count: 0,
            event_count: 0,
        };

        Self {
            inner: Arc::new(RwLock::new(NodeRegistryInner {
                local_node,
                peers: HashMap::new(),
                timeout: Duration::seconds(timeout_secs),
            })),
        }
    }

    /// Get local node ID.
    pub async fn local_node_id(&self) -> String {
        self.inner.read().await.local_node.id.clone()
    }

    /// Add a static peer from config.
    pub async fn add_peer(&self, id: String, name: String, addr: String) {
        let mut inner = self.inner.write().await;
        inner.peers.entry(id.clone()).or_insert_with(|| NodeInfo {
            id,
            name,
            addr,
            status: NodeStatus::Unknown,
            last_seen: Utc::now(),
            watcher_count: 0,
            event_count: 0,
        });
    }

    /// Update heartbeat from a remote node.
    pub async fn update_heartbeat(
        &self,
        node_id: &str,
        node_name: &str,
        listen_addr: &str,
        watcher_count: usize,
        event_count: u64,
    ) {
        let mut inner = self.inner.write().await;

        // Don't update our own entry via heartbeat
        if node_id == inner.local_node.id {
            return;
        }

        let node = inner.peers.entry(node_id.to_string()).or_insert_with(|| {
            NodeInfo {
                id: node_id.to_string(),
                name: node_name.to_string(),
                addr: listen_addr.to_string(),
                status: NodeStatus::Unknown,
                last_seen: Utc::now(),
                watcher_count: 0,
                event_count: 0,
            }
        });

        node.name = node_name.to_string();
        node.addr = listen_addr.to_string();
        node.status = NodeStatus::Online;
        node.last_seen = Utc::now();
        node.watcher_count = watcher_count;
        node.event_count = event_count;
    }

    /// Mark nodes as offline if they haven't sent a heartbeat recently.
    pub async fn check_timeouts(&self) {
        let mut inner = self.inner.write().await;
        let now = Utc::now();
        let timeout = inner.timeout;

        for node in inner.peers.values_mut() {
            if node.status == NodeStatus::Online && now - node.last_seen > timeout {
                node.status = NodeStatus::Offline;
            }
        }
    }

    /// List all known nodes (including local).
    pub async fn list_nodes(&self) -> Vec<NodeInfo> {
        let inner = self.inner.read().await;
        let mut nodes = vec![inner.local_node.clone()];
        nodes.extend(inner.peers.values().cloned());
        nodes
    }

    /// Get only online nodes.
    pub async fn online_nodes(&self) -> Vec<NodeInfo> {
        let inner = self.inner.read().await;
        let mut nodes = vec![inner.local_node.clone()];
        nodes.extend(
            inner
                .peers
                .values()
                .filter(|n| n.status == NodeStatus::Online)
                .cloned(),
        );
        nodes
    }

    /// Get a specific node by ID.
    pub async fn get_node(&self, id: &str) -> Option<NodeInfo> {
        let inner = self.inner.read().await;
        if inner.local_node.id == id {
            return Some(inner.local_node.clone());
        }
        inner.peers.get(id).cloned()
    }

    /// Update local node stats.
    pub async fn update_local_stats(&self, watcher_count: usize, event_count: u64) {
        let mut inner = self.inner.write().await;
        inner.local_node.watcher_count = watcher_count;
        inner.local_node.event_count = event_count;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_local_node() {
        let registry =
            NodeRegistry::new("node-1".into(), "test".into(), "0.0.0.0:9100".into(), 30);

        let nodes = registry.list_nodes().await;
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, "node-1");
        assert_eq!(nodes[0].status, NodeStatus::Online);
    }

    #[tokio::test]
    async fn test_registry_add_peer() {
        let registry =
            NodeRegistry::new("node-1".into(), "test".into(), "0.0.0.0:9100".into(), 30);

        registry
            .add_peer("node-2".into(), "peer".into(), "192.168.1.2:9100".into())
            .await;

        let nodes = registry.list_nodes().await;
        assert_eq!(nodes.len(), 2);
    }

    #[tokio::test]
    async fn test_registry_heartbeat() {
        let registry =
            NodeRegistry::new("node-1".into(), "test".into(), "0.0.0.0:9100".into(), 30);

        registry
            .update_heartbeat("node-2", "peer", "192.168.1.2:9100", 3, 100)
            .await;

        let node = registry.get_node("node-2").await.unwrap();
        assert_eq!(node.status, NodeStatus::Online);
        assert_eq!(node.watcher_count, 3);
        assert_eq!(node.event_count, 100);
    }

    #[tokio::test]
    async fn test_registry_timeout() {
        let registry =
            NodeRegistry::new("node-1".into(), "test".into(), "0.0.0.0:9100".into(), 0);

        // Add peer and immediately check timeout (timeout=0 means instant)
        registry
            .update_heartbeat("node-2", "peer", "192.168.1.2:9100", 0, 0)
            .await;

        // Manually set last_seen to the past
        {
            let mut inner = registry.inner.write().await;
            if let Some(node) = inner.peers.get_mut("node-2") {
                node.last_seen = Utc::now() - Duration::seconds(10);
            }
        }

        registry.check_timeouts().await;

        let node = registry.get_node("node-2").await.unwrap();
        assert_eq!(node.status, NodeStatus::Offline);
    }
}
