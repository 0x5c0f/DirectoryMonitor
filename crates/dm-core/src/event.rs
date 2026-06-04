use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Types of filesystem events, aligned with inotifywait categories.
///
/// - `CREATE` → `Created`
/// - `MODIFY` (data) → `Modified`
/// - `ATTRIB` (metadata) → `Attrib`
/// - `CLOSE_WRITE` → `CloseWrite`
/// - `CLOSE_NOWRITE` → `CloseNoWrite`
/// - `OPEN` → `Opened`
/// - `MOVED_TO` → `MovedTo`
/// - `MOVED_FROM` → `MovedFrom`
/// - `DELETE` / `DELETE_SELF` → `Deleted`
/// - `ACCESS` (read) → `Accessed`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// File or directory created.
    Created,
    /// File content was written/modified.
    Modified,
    /// File or directory attributes changed (permissions, timestamps, owner).
    Attrib,
    /// File closed after being opened in writable mode. Signals a write is complete.
    CloseWrite,
    /// File closed after being opened in read-only mode.
    CloseNoWrite,
    /// File or directory was opened.
    Opened,
    /// File or directory was moved into the watched directory.
    MovedTo,
    /// File or directory was moved out of the watched directory.
    MovedFrom,
    /// File or directory was deleted.
    Deleted,
    /// File or directory was renamed (legacy: from + to pair).
    Renamed,
    /// File or directory contents were read.
    Accessed,
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventType::Created => write!(f, "CREATE"),
            EventType::Modified => write!(f, "MODIFY"),
            EventType::Attrib => write!(f, "ATTRIB"),
            EventType::CloseWrite => write!(f, "CLOSE_WRITE"),
            EventType::CloseNoWrite => write!(f, "CLOSE_NOWRITE"),
            EventType::Opened => write!(f, "OPEN"),
            EventType::MovedTo => write!(f, "MOVED_TO"),
            EventType::MovedFrom => write!(f, "MOVED_FROM"),
            EventType::Deleted => write!(f, "DELETE"),
            EventType::Renamed => write!(f, "RENAME"),
            EventType::Accessed => write!(f, "ACCESS"),
        }
    }
}

impl EventType {
    /// Returns true if this event represents a meaningful content change.
    pub fn is_content_change(&self) -> bool {
        matches!(
            self,
            EventType::Created
                | EventType::Modified
                | EventType::CloseWrite
                | EventType::MovedTo
                | EventType::Deleted
                | EventType::MovedFrom
                | EventType::Renamed
        )
    }

    /// Returns true if this event represents a metadata-only change.
    pub fn is_metadata_change(&self) -> bool {
        matches!(self, EventType::Attrib)
    }

    /// Returns true if this event is informational (access, open, close_nowrite).
    pub fn is_access(&self) -> bool {
        matches!(
            self,
            EventType::Accessed | EventType::Opened | EventType::CloseNoWrite
        )
    }
}

/// A single filesystem event with full context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsEvent {
    /// Unique event ID.
    pub id: Uuid,
    /// When the event occurred.
    pub timestamp: DateTime<Utc>,
    /// Type of change.
    pub event_type: EventType,
    /// Primary file/directory path.
    pub path: PathBuf,
    /// For rename events, the destination path.
    pub target_path: Option<PathBuf>,
    /// The user who made the change (PRO feature, None if unavailable).
    pub user: Option<String>,
    /// The process that made the change (PRO feature, None if unavailable).
    pub process: Option<String>,
    /// The directory being monitored that triggered this event.
    pub watch_root: PathBuf,
}

impl FsEvent {
    pub fn new(
        event_type: EventType,
        path: PathBuf,
        watch_root: PathBuf,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type,
            path,
            target_path: None,
            user: None,
            process: None,
            watch_root,
        }
    }

    pub fn with_target(mut self, target: PathBuf) -> Self {
        self.target_path = Some(target);
        self
    }

    pub fn with_user(mut self, user: String) -> Self {
        self.user = Some(user);
        self
    }

    pub fn with_process(mut self, process: String) -> Self {
        self.process = Some(process);
        self
    }

    /// Get the filename (last component of path).
    pub fn filename(&self) -> Option<&str> {
        self.path.file_name().and_then(|n| n.to_str())
    }

    /// Get the parent directory.
    pub fn directory(&self) -> Option<&str> {
        self.path.parent().and_then(|p| p.to_str())
    }

    /// Format event using macro-style placeholders.
    /// Supported: %file%, %directory%, %event%, %timestamp%, %path%, %target%, %user%, %process%
    pub fn format_with(&self, template: &str) -> String {
        let mut result = template.to_string();
        result = result.replace("%file%", self.filename().unwrap_or(""));
        result = result.replace("%directory%", self.directory().unwrap_or(""));
        result = result.replace("%event%", &self.event_type.to_string());
        result = result.replace("%timestamp%", &self.timestamp.to_rfc3339());
        result = result.replace("%path%", self.path.to_str().unwrap_or(""));
        result = result.replace(
            "%target%",
            self.target_path.as_ref().and_then(|p| p.to_str()).unwrap_or(""),
        );
        result = result.replace("%user%", self.user.as_deref().unwrap_or("unknown"));
        result = result.replace("%process%", self.process.as_deref().unwrap_or("unknown"));
        result
    }
}
