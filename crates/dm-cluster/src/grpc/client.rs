use super::proto::cluster_service_client::ClusterServiceClient;
use super::proto::{
    EventRecord, HeartbeatRequest, NodeStatusRequest, PublishEventRequest, QueryEventsRequest,
};
use dm_core::event::FsEvent;
use tonic::Request;
use tracing::info;

use crate::types::{ClusterEvent, NodeHeartbeat};

/// gRPC client for querying remote cluster nodes.
#[derive(Clone)]
pub struct GrpcClient {
    client: ClusterServiceClient<tonic::transport::Channel>,
    addr: String,
}

impl GrpcClient {
    /// Connect to a remote node's gRPC server.
    pub async fn connect(addr: &str) -> Result<Self, String> {
        let endpoint = format!("http://{addr}");
        let client = ClusterServiceClient::connect(endpoint)
            .await
            .map_err(|e| format!("Failed to connect to gRPC server at {addr}: {e}"))?;

        info!("Connected to gRPC server at {addr}");

        Ok(Self {
            client,
            addr: addr.to_string(),
        })
    }

    /// Publish an event to the remote node.
    pub async fn publish_event(&mut self, event: &ClusterEvent) -> Result<(), String> {
        let request = Request::new(PublishEventRequest {
            node_id: event.node_id.clone(),
            node_name: event.node_name.clone(),
            event: Some(EventRecord {
                id: event.id.clone(),
                timestamp: event.timestamp.to_rfc3339(),
                event_type: event.event_type.clone(),
                path: event.path.clone(),
                target_path: event.old_path.clone(),
                is_dir: Some(event.is_directory),
                user: None,
                process: None,
                watch_root: String::new(),
                node_id: event.node_id.clone(),
            }),
        });

        self.client
            .publish_event(request)
            .await
            .map_err(|e| format!("Publish event failed: {e}"))?;

        Ok(())
    }

    /// Send a heartbeat to the remote node.
    pub async fn heartbeat(&mut self, heartbeat: &NodeHeartbeat) -> Result<(), String> {
        let request = Request::new(HeartbeatRequest {
            node_id: heartbeat.node_id.clone(),
            node_name: heartbeat.node_name.clone(),
            listen_addr: heartbeat.listen_addr.clone(),
            watcher_count: heartbeat.watcher_count as u32,
            event_count: heartbeat.event_count,
            timestamp: heartbeat.timestamp.timestamp(),
        });

        self.client
            .heartbeat(request)
            .await
            .map_err(|e| format!("Heartbeat failed: {e}"))?;

        Ok(())
    }

    /// Query events from the remote node.
    pub async fn query_events(
        &mut self,
        limit: u32,
        offset: u32,
        event_types: Vec<String>,
        watch_root: Option<String>,
        search: Option<String>,
        after: Option<String>,
        before: Option<String>,
        is_dir: Option<bool>,
    ) -> Result<(Vec<FsEvent>, u64, String, String), String> {
        let request = Request::new(QueryEventsRequest {
            limit,
            offset,
            event_types,
            watch_root,
            search,
            after,
            before,
            is_dir,
            node_id: None,
        });

        let response = self
            .client
            .query_events(request)
            .await
            .map_err(|e| format!("gRPC query failed: {e}"))?;

        let resp = response.into_inner();

        let events: Vec<FsEvent> = resp
            .events
            .iter()
            .map(super::server::record_to_event)
            .collect::<Result<Vec<_>, _>>()?;

        Ok((events, resp.total, resp.node_id, resp.node_name))
    }

    /// Get the remote node's status.
    pub async fn get_node_status(&mut self) -> Result<NodeStatusInfo, String> {
        let request = Request::new(NodeStatusRequest {});

        let response = self
            .client
            .get_node_status(request)
            .await
            .map_err(|e| format!("gRPC status failed: {e}"))?;

        let resp = response.into_inner();

        Ok(NodeStatusInfo {
            node_id: resp.node_id,
            node_name: resp.node_name,
            listen_addr: resp.listen_addr,
            event_count: resp.event_count,
            watcher_count: resp.watcher_count as usize,
            uptime: resp.uptime,
        })
    }

    /// Get the server address.
    pub fn addr(&self) -> &str {
        &self.addr
    }
}

/// Node status information from a remote gRPC query.
#[derive(Debug, Clone)]
pub struct NodeStatusInfo {
    pub node_id: String,
    pub node_name: String,
    pub listen_addr: String,
    pub event_count: u64,
    pub watcher_count: usize,
    pub uptime: String,
}
