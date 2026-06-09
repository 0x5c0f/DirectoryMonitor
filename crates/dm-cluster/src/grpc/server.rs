use super::proto::cluster_service_server::{ClusterService, ClusterServiceServer};
use super::proto::{
    EventRecord, HeartbeatRequest, HeartbeatResponse, NodeStatusRequest, NodeStatusResponse,
    PublishEventRequest, PublishEventResponse, QueryEventsRequest, QueryEventsResponse,
};
use dm_core::event::{EventType, FsEvent};
use dm_storage::{EventQuery, EventStore};
use std::path::PathBuf;
use tokio::sync::broadcast;
use tonic::{Request, Response, Status};
use tracing::info;

use crate::peer::NodeRegistry;
use crate::sync::EventCache;
use crate::types::ClusterEvent;

/// gRPC server implementation for cluster service.
pub struct ClusterServiceImpl {
    store: EventStore,
    node_id: String,
    node_name: String,
    cache: Option<EventCache>,
    registry: Option<NodeRegistry>,
    event_tx: Option<broadcast::Sender<ClusterEvent>>,
}

impl ClusterServiceImpl {
    pub fn new(store: EventStore, node_id: String, node_name: String) -> Self {
        Self {
            store,
            node_id,
            node_name,
            cache: None,
            registry: None,
            event_tx: None,
        }
    }

    pub fn with_cache(mut self, cache: EventCache) -> Self {
        self.cache = Some(cache);
        self
    }

    pub fn with_registry(mut self, registry: NodeRegistry) -> Self {
        self.registry = Some(registry);
        self
    }

    pub fn with_event_tx(mut self, event_tx: broadcast::Sender<ClusterEvent>) -> Self {
        self.event_tx = Some(event_tx);
        self
    }
}

#[tonic::async_trait]
impl ClusterService for ClusterServiceImpl {
    async fn query_events(
        &self,
        request: Request<QueryEventsRequest>,
    ) -> Result<Response<QueryEventsResponse>, Status> {
        let req = request.into_inner();

        let query = EventQuery {
            limit: req.limit.max(1) as usize,
            offset: req.offset as usize,
            event_types: req.event_types,
            watch_root: req.watch_root,
            search: req.search,
            after: req.after,
            before: req.before,
            is_dir: req.is_dir,
            node_id: None,
        };

        let events = self
            .store
            .query_events(query.clone())
            .await
            .map_err(|e| Status::internal(format!("Query failed: {e}")))?;

        let total = self
            .store
            .count_filtered(
                &query.event_types,
                query.watch_root.as_deref(),
                query.search.as_deref(),
                query.after.as_deref(),
                query.before.as_deref(),
                query.is_dir,
                None,
            )
            .await
            .map_err(|e| Status::internal(format!("Count failed: {e}")))?;

        let records: Vec<EventRecord> = events.iter().map(event_to_record).collect();

        Ok(Response::new(QueryEventsResponse {
            events: records,
            total: total as u64,
            node_id: self.node_id.clone(),
            node_name: self.node_name.clone(),
        }))
    }

    async fn get_node_status(
        &self,
        _request: Request<NodeStatusRequest>,
    ) -> Result<Response<NodeStatusResponse>, Status> {
        let event_count = self
            .store
            .count()
            .await
            .map_err(|e| Status::internal(format!("Count failed: {e}")))?;

        Ok(Response::new(NodeStatusResponse {
            node_id: self.node_id.clone(),
            node_name: self.node_name.clone(),
            listen_addr: String::new(),
            event_count: event_count as u64,
            watcher_count: 0,
            uptime: String::new(),
        }))
    }

    async fn publish_event(
        &self,
        request: Request<PublishEventRequest>,
    ) -> Result<Response<PublishEventResponse>, Status> {
        let req = request.into_inner();

        // Ignore self-published events
        if req.node_id == self.node_id {
            return Ok(Response::new(PublishEventResponse {}));
        }

        if let Some(event_record) = req.event {
            let cluster_event = ClusterEvent {
                id: event_record.id.clone(),
                event_type: event_record.event_type.clone(),
                path: event_record.path.clone(),
                old_path: event_record.target_path.clone(),
                timestamp: chrono::Utc::now(),
                size: None,
                is_directory: event_record.is_dir.unwrap_or(false),
                node_id: req.node_id.clone(),
                node_name: req.node_name.clone(),
            };

            // Note: EventCache is populated by the EventSyncService
            // The gRPC server only broadcasts events to subscribers

            // Broadcast to subscribers
            if let Some(tx) = &self.event_tx {
                let _ = tx.send(cluster_event);
            }

            info!("Received event from {} ({})", req.node_name, req.node_id);
        }

        Ok(Response::new(PublishEventResponse {}))
    }

    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();

        // Ignore self-heartbeats
        if req.node_id == self.node_id {
            return Ok(Response::new(HeartbeatResponse {}));
        }

        // Update registry
        if let Some(registry) = &self.registry {
            registry
                .update_heartbeat(
                    &req.node_id,
                    &req.node_name,
                    &req.listen_addr,
                    req.watcher_count as usize,
                    req.event_count,
                )
                .await;
        }

        info!("Received heartbeat from {} ({})", req.node_name, req.node_id);

        Ok(Response::new(HeartbeatResponse {}))
    }
}

/// Convert an FsEvent to a gRPC EventRecord.
fn event_to_record(event: &FsEvent) -> EventRecord {
    EventRecord {
        id: event.id.to_string(),
        timestamp: event.timestamp.to_rfc3339(),
        event_type: event.event_type.to_string(),
        path: event.path.to_string_lossy().to_string(),
        target_path: event
            .target_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
        is_dir: event.is_dir,
        user: event.user.clone(),
        process: event.process.clone(),
        watch_root: event.watch_root.to_string_lossy().to_string(),
        node_id: event.node_id.clone(),
    }
}

/// Convert a gRPC EventRecord to an FsEvent.
pub fn record_to_event(record: &EventRecord) -> Result<FsEvent, String> {
    let id = record
        .id
        .parse()
        .map_err(|e| format!("Invalid UUID '{}': {}", record.id, e))?;

    let timestamp = chrono::DateTime::parse_from_rfc3339(&record.timestamp)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|e| format!("Invalid timestamp '{}': {}", record.timestamp, e))?;

    let event_type = match record.event_type.as_str() {
        "CREATE" => EventType::Created,
        "MODIFY" => EventType::Modified,
        "ATTRIB" => EventType::Attrib,
        "CLOSE_WRITE" => EventType::CloseWrite,
        "CLOSE_NOWRITE" => EventType::CloseNoWrite,
        "OPEN" => EventType::Opened,
        "MOVED_TO" => EventType::MovedTo,
        "MOVED_FROM" => EventType::MovedFrom,
        "DELETE" => EventType::Deleted,
        "RENAME" => EventType::Renamed,
        "ACCESS" => EventType::Accessed,
        other => return Err(format!("Unknown event type: {}", other)),
    };

    Ok(FsEvent {
        id,
        timestamp,
        event_type,
        path: PathBuf::from(&record.path),
        target_path: record.target_path.as_ref().map(PathBuf::from),
        is_dir: record.is_dir,
        user: record.user.clone(),
        process: record.process.clone(),
        watch_root: PathBuf::from(&record.watch_root),
        node_id: record.node_id.clone(),
    })
}

/// Build and start the gRPC server.
pub async fn start_grpc_server(
    addr: std::net::SocketAddr,
    store: EventStore,
    node_id: String,
    node_name: String,
) -> Result<(), String> {
    let service = ClusterServiceImpl::new(store, node_id.clone(), node_name.clone());
    let server = ClusterServiceServer::new(service);

    info!("gRPC server starting on {addr} (node: {node_name})");

    tonic::transport::Server::builder()
        .add_service(server)
        .serve(addr)
        .await
        .map_err(|e| format!("gRPC server error: {e}"))
}

/// Build and start the gRPC server with cluster support.
pub async fn start_grpc_server_with_cluster(
    addr: std::net::SocketAddr,
    store: EventStore,
    node_id: String,
    node_name: String,
    cache: EventCache,
    registry: NodeRegistry,
    event_tx: broadcast::Sender<ClusterEvent>,
) -> Result<(), String> {
    let service = ClusterServiceImpl::new(store, node_id.clone(), node_name.clone())
        .with_cache(cache)
        .with_registry(registry)
        .with_event_tx(event_tx);
    let server = ClusterServiceServer::new(service);

    info!("gRPC server starting on {addr} (node: {node_name}, cluster mode)");

    tonic::transport::Server::builder()
        .add_service(server)
        .serve(addr)
        .await
        .map_err(|e| format!("gRPC server error: {e}"))
}
