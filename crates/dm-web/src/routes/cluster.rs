use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

use crate::server::AppState;

/// Cluster status response.
#[derive(Debug, Serialize)]
pub struct ClusterStatusResponse {
    pub enabled: bool,
    pub node_id: String,
    pub node_name: String,
}

/// Cluster node info.
#[derive(Debug, Serialize)]
pub struct ClusterNodeInfo {
    pub id: String,
    pub name: String,
    pub addr: String,
    pub status: String,
    pub watcher_count: usize,
    pub event_count: u64,
}

/// GET /api/cluster/status
pub async fn cluster_status(State(state): State<AppState>) -> Json<ClusterStatusResponse> {
    let config = state.config.read().await;
    Json(ClusterStatusResponse {
        enabled: config.cluster.enabled,
        node_id: state.cluster_node_id.clone(),
        node_name: state.cluster_node_name.clone(),
    })
}

/// GET /api/cluster/nodes
pub async fn cluster_nodes(
    State(state): State<AppState>,
) -> Result<Json<Vec<ClusterNodeInfo>>, StatusCode> {
    let config = state.config.read().await;
    if !config.cluster.enabled {
        return Err(StatusCode::NOT_FOUND);
    }

    // Use real NodeRegistry if available
    if let Some(ref registry) = state.node_registry {
        let nodes = registry.list_nodes().await;
        let node_infos: Vec<ClusterNodeInfo> = nodes
            .iter()
            .map(|n| ClusterNodeInfo {
                id: n.id.clone(),
                name: n.name.clone(),
                addr: n.addr.clone(),
                status: n.status.to_string(),
                watcher_count: n.watcher_count,
                event_count: n.event_count,
            })
            .collect();
        return Ok(Json(node_infos));
    }

    // Fallback: return local node only
    Ok(Json(vec![ClusterNodeInfo {
        id: state.cluster_node_id.clone(),
        name: state.cluster_node_name.clone(),
        addr: String::new(),
        status: "Online".to_string(),
        watcher_count: 0,
        event_count: 0,
    }]))
}
