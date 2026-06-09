use chrono::{DateTime, Utc};
use dm_core::event::FsEvent;
use serde::{Deserialize, Serialize};

/// Cross-node transport event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterEvent {
    pub id: String,
    pub event_type: String,
    pub path: String,
    pub old_path: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub size: Option<u64>,
    pub is_directory: bool,
    pub node_id: String,
    pub node_name: String,
}

impl ClusterEvent {
    /// Create a ClusterEvent from an FsEvent
    pub fn from_fs_event(event: &FsEvent, node_id: &str, node_name: &str) -> Self {
        Self {
            id: event.id.to_string(),
            event_type: event.event_type.to_string(),
            path: event.path.to_string_lossy().to_string(),
            old_path: event.target_path.as_ref().map(|p| p.to_string_lossy().to_string()),
            timestamp: event.timestamp,
            size: None, // FsEvent doesn't have size field
            is_directory: event.is_dir.unwrap_or(false),
            node_id: node_id.to_string(),
            node_name: node_name.to_string(),
        }
    }
}

/// Node heartbeat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHeartbeat {
    pub node_id: String,
    pub node_name: String,
    pub listen_addr: String,
    pub watcher_count: usize,
    pub event_count: u64,
    pub timestamp: DateTime<Utc>,
}
