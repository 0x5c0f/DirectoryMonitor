use crate::grpc::client::GrpcClient;
use crate::peer::NodeRegistry;
use crate::sync::EventCache;
use crate::types::ClusterEvent;
use dm_storage::{EventQuery, EventStore};
use tracing::{debug, error};

/// Aggregates queries across cluster nodes.
#[derive(Clone)]
pub struct ClusterQueryAggregator {
    local_store: EventStore,
    cache: EventCache,
    registry: NodeRegistry,
}

impl ClusterQueryAggregator {
    /// Create a new aggregator.
    pub fn new(local_store: EventStore, cache: EventCache, registry: NodeRegistry) -> Self {
        Self {
            local_store,
            cache,
            registry,
        }
    }

    /// Query events from the local cache (recent cluster events).
    pub fn query_cache(
        &self,
        event_types: &[String],
        node_id: Option<&str>,
        limit: usize,
    ) -> Vec<ClusterEvent> {
        self.cache.query(event_types, node_id, limit)
    }

    /// Query local SQLite store.
    pub async fn query_local(&self, query: &EventQuery) -> Result<Vec<ClusterEvent>, String> {
        let events = self
            .local_store
            .query_events(query.clone())
            .await
            .map_err(|e| format!("Local query failed: {e}"))?;

        let local_id = self.registry.local_node_id().await;

        Ok(events
            .iter()
            .map(|e| ClusterEvent::from_fs_event(e, &local_id, "local"))
            .collect())
    }

    /// Query all nodes and merge results.
    /// Queries local store + remote nodes via gRPC + cache.
    pub async fn query_all(
        &self,
        query: &EventQuery,
        node_filter: Option<&str>,
    ) -> Result<Vec<ClusterEvent>, String> {
        let mut all_events = Vec::new();

        // 1. Query local store
        let local_events = self.query_local(query).await?;
        all_events.extend(local_events);

        // 2. Query remote nodes via gRPC
        let peers = self.registry.online_nodes().await;
        for peer in peers {
            if let Some(filter) = node_filter {
                if peer.id != filter {
                    continue;
                }
            }

            match GrpcClient::connect(&peer.addr).await {
                Ok(mut client) => {
                    match client
                        .query_events(
                            query.limit as u32,
                            query.offset as u32,
                            query.event_types.clone(),
                            query.watch_root.clone(),
                            query.search.clone(),
                            query.after.clone(),
                            query.before.clone(),
                            query.is_dir,
                        )
                        .await
                    {
                        Ok((events, _total, node_id, node_name)) => {
                            debug!(
                                "Got {} events from node {} ({})",
                                events.len(),
                                node_name,
                                node_id
                            );
                            let cluster_events: Vec<ClusterEvent> = events
                                .iter()
                                .map(|e| ClusterEvent::from_fs_event(e, &node_id, &node_name))
                                .collect();
                            all_events.extend(cluster_events);
                        }
                        Err(e) => {
                            error!("Failed to query node {}: {e}", peer.name);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to connect to node {} at {}: {e}", peer.name, peer.addr);
                }
            }
        }

        // 3. Query cache (recent remote events not yet in DB)
        let cache_events = self.cache.query(&query.event_types, node_filter, query.limit);
        all_events.extend(cache_events);

        // 4. Sort by timestamp descending
        all_events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // 5. Deduplicate by event ID
        all_events.dedup_by(|a, b| a.id == b.id);

        // 6. Apply limit
        all_events.truncate(query.limit);

        Ok(all_events)
    }

    /// Get cluster-wide node count.
    pub async fn node_count(&self) -> usize {
        self.registry.list_nodes().await.len()
    }

    /// Get online node count.
    pub async fn online_node_count(&self) -> usize {
        self.registry.online_nodes().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::EventCache;

    #[test]
    fn test_event_cache_operations() {
        let mut cache = EventCache::new(3);

        cache.push(ClusterEvent {
            id: "1".into(),
            node_id: "node-1".into(),
            node_name: "test".into(),
            timestamp: "2026-01-01T00:00:00Z".parse().unwrap(),
            event_type: "CREATE".into(),
            path: "/a".into(),
            old_path: None,
            size: None,
            is_directory: false,
        });

        cache.push(ClusterEvent {
            id: "2".into(),
            node_id: "node-2".into(),
            node_name: "test2".into(),
            timestamp: "2026-01-01T00:00:01Z".parse().unwrap(),
            event_type: "MODIFY".into(),
            path: "/b".into(),
            old_path: None,
            size: None,
            is_directory: false,
        });

        assert_eq!(cache.len(), 2);

        let recent = cache.recent(10);
        assert_eq!(recent.len(), 2);

        let filtered = cache.query(&["CREATE".to_string()], None, 10);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].path, "/a");

        let by_node = cache.query(&[], Some("node-2"), 10);
        assert_eq!(by_node.len(), 1);
        assert_eq!(by_node[0].path, "/b");
    }

    #[test]
    fn test_event_cache_eviction() {
        let mut cache = EventCache::new(2);

        for i in 0..3 {
            cache.push(ClusterEvent {
                id: i.to_string(),
                node_id: "node-1".into(),
                node_name: "test".into(),
                timestamp: format!("2026-01-01T00:00:0{i}Z").parse().unwrap(),
                event_type: "CREATE".into(),
                path: format!("/file-{i}"),
                old_path: None,
                size: None,
                is_directory: false,
            });
        }

        assert_eq!(cache.len(), 2);
        let recent = cache.recent(10);
        assert_eq!(recent[0].id, "2"); // Most recent
        assert_eq!(recent[1].id, "1"); // Second most recent
    }
}
