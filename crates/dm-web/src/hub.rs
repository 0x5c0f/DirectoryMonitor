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

#[cfg(test)]
mod tests {
    use super::*;
    use dm_core::event::EventType;
    use std::path::PathBuf;

    #[test]
    fn test_event_payload_from_fsevent() {
        let event = FsEvent::new(
            EventType::Created,
            PathBuf::from("/home/user/file.txt"),
            PathBuf::from("/home/user"),
        );

        let payload = EventPayload::from(&event);
        assert_eq!(payload.id, event.id.to_string());
        assert_eq!(payload.event_type, "CREATE");
        assert_eq!(payload.path, "/home/user/file.txt");
        assert_eq!(payload.watch_root, "/home/user");
        assert!(payload.target_path.is_none());
    }

    #[test]
    fn test_event_payload_with_target() {
        let event = FsEvent::new(
            EventType::Renamed,
            PathBuf::from("/old.txt"),
            PathBuf::from("/"),
        )
        .with_target(PathBuf::from("/new.txt"));

        let payload = EventPayload::from(&event);
        assert_eq!(payload.event_type, "RENAME");
        assert_eq!(payload.path, "/old.txt");
        assert_eq!(payload.target_path, Some("/new.txt".to_string()));
    }

    #[test]
    fn test_event_payload_timestamp_format() {
        let event = FsEvent::new(
            EventType::Modified,
            PathBuf::from("/file.txt"),
            PathBuf::from("/"),
        );

        let payload = EventPayload::from(&event);
        // Should contain 'T' separator (RFC 3339 basic check)
        assert!(payload.timestamp.contains('T'));
        assert!(payload.timestamp.contains('Z') || payload.timestamp.contains('+'));
    }
}
