use crate::peer::NodeRegistry;
use crate::peer_manager::PeerManager;
use crate::types::{ClusterEvent, NodeHeartbeat};
use std::collections::VecDeque;
use tokio::sync::broadcast;
use tracing::{error, info};

/// Ring buffer cache for recent cluster events.
#[derive(Clone)]
pub struct EventCache {
    buffer: VecDeque<ClusterEvent>,
    capacity: usize,
}

impl EventCache {
    /// Create a new event cache with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Push an event, evicting the oldest if full.
    pub fn push(&mut self, event: ClusterEvent) {
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(event);
    }

    /// Get the N most recent events.
    pub fn recent(&self, n: usize) -> Vec<ClusterEvent> {
        self.buffer
            .iter()
            .rev()
            .take(n)
            .cloned()
            .collect()
    }

    /// Query events with basic filtering.
    pub fn query(
        &self,
        event_types: &[String],
        node_id: Option<&str>,
        limit: usize,
    ) -> Vec<ClusterEvent> {
        self.buffer
            .iter()
            .rev()
            .filter(|e| {
                if event_types.is_empty() {
                    true
                } else {
                    event_types.contains(&e.event_type)
                }
            })
            .filter(|e| {
                if let Some(nid) = node_id {
                    e.node_id == nid
                } else {
                    true
                }
            })
            .take(limit)
            .cloned()
            .collect()
    }

    /// Current cache size.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if cache is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

/// Service for syncing events via gRPC.
pub struct EventSyncService {
    peer_manager: PeerManager,
    cache: EventCache,
}

impl EventSyncService {
    /// Create a new sync service.
    pub fn new(peer_manager: PeerManager, cache_size: usize) -> Self {
        Self {
            peer_manager,
            cache: EventCache::new(cache_size),
        }
    }

    /// Start publishing local events to all peers via gRPC.
    pub async fn start_publish_loop(
        &self,
        mut event_rx: broadcast::Receiver<dm_watcher::WatchEvent>,
    ) {
        loop {
            match event_rx.recv().await {
                Ok(watch_event) => {
                    let events = match watch_event {
                        dm_watcher::WatchEvent::Event(e) => vec![e],
                        dm_watcher::WatchEvent::Batch(events) => events,
                        dm_watcher::WatchEvent::Error(_) => continue,
                    };

                    for event in &events {
                        let cluster_event = ClusterEvent {
                            id: event.id.to_string(),
                            event_type: event.event_type.to_string(),
                            path: event.path.to_string_lossy().to_string(),
                            old_path: event.target_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                            timestamp: event.timestamp,
                            size: None,
                            is_directory: event.is_dir.unwrap_or(false),
                            node_id: event.node_id.clone(),
                            node_name: String::new(), // Will be filled by receiver
                        };

                        if let Err(errors) = self.peer_manager.publish_event_to_all(&cluster_event).await {
                            for e in errors {
                                error!("Failed to publish event to peer: {e}");
                            }
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    info!("Event publish lagged, skipped {n} events");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    }

    /// Get the event cache.
    pub fn cache(&self) -> &EventCache {
        &self.cache
    }
}

/// Service for heartbeat exchange via gRPC.
pub struct HeartbeatService {
    peer_manager: PeerManager,
    registry: NodeRegistry,
    listen_addr: String,
}

impl HeartbeatService {
    /// Create a new heartbeat service.
    pub fn new(peer_manager: PeerManager, registry: NodeRegistry, listen_addr: String) -> Self {
        Self {
            peer_manager,
            registry,
            listen_addr,
        }
    }

    /// Start publishing heartbeats periodically.
    pub async fn start_publish_loop(&self, interval_secs: u64) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));

        loop {
            interval.tick().await;

            let nodes = self.registry.list_nodes().await;
            let local = &nodes[0];

            let heartbeat = NodeHeartbeat {
                node_id: local.id.clone(),
                node_name: local.name.clone(),
                listen_addr: self.listen_addr.clone(),
                watcher_count: local.watcher_count,
                event_count: local.event_count,
                timestamp: chrono::Utc::now(),
            };

            if let Err(errors) = self.peer_manager.send_heartbeat_to_all(&heartbeat).await {
                for e in errors {
                    error!("Failed to send heartbeat: {e}");
                }
            } else {
                info!("Sent heartbeat for {} (watchers: {}, events: {})", local.name, local.watcher_count, local.event_count);
            }
        }
    }
}
