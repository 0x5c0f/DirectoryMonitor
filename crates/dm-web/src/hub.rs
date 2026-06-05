use dm_core::event::FsEvent;
use serde::Serialize;

/// Serializable event payload sent to WebSocket clients.
#[derive(Debug, Clone, Serialize)]
pub struct EventPayload {
    pub id: String,
    pub timestamp: String,
    pub event_type: String,
    pub path: String,
    pub target_path: Option<String>,
    pub watch_root: String,
}

impl From<&FsEvent> for EventPayload {
    fn from(e: &FsEvent) -> Self {
        Self {
            id: e.id.to_string(),
            timestamp: e.timestamp.to_rfc3339(),
            event_type: e.event_type.to_string(),
            path: e.path.display().to_string(),
            target_path: e.target_path.as_ref().map(|p| p.display().to_string()),
            watch_root: e.watch_root.display().to_string(),
        }
    }
}
