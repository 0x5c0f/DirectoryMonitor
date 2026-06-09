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
    /// Whether the path is a directory (None if unknown).
    pub is_dir: Option<bool>,
    /// The user who made the change (PRO feature, None if unavailable).
    pub user: Option<String>,
    /// The process that made the change (PRO feature, None if unavailable).
    pub process: Option<String>,
    /// The directory being monitored that triggered this event.
    pub watch_root: PathBuf,
    /// The node that produced this event (empty string for local/single-node mode).
    pub node_id: String,
}

impl FsEvent {
    pub fn new(event_type: EventType, path: PathBuf, watch_root: PathBuf) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type,
            path,
            target_path: None,
            is_dir: None,
            user: None,
            process: None,
            watch_root,
            node_id: String::new(),
        }
    }

    #[must_use]
    pub fn with_target(mut self, target: PathBuf) -> Self {
        self.target_path = Some(target);
        self
    }

    #[must_use]
    pub fn with_is_dir(mut self, is_dir: bool) -> Self {
        self.is_dir = Some(is_dir);
        self
    }

    #[must_use]
    pub fn with_user(mut self, user: String) -> Self {
        self.user = Some(user);
        self
    }

    #[must_use]
    pub fn with_process(mut self, process: String) -> Self {
        self.process = Some(process);
        self
    }

    #[must_use]
    pub fn with_node_id(mut self, node_id: String) -> Self {
        self.node_id = node_id;
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
    /// Supported: %file%, %directory%, %event%, %timestamp%, %path%, %target%, %type%, %user%, %process%
    pub fn format_with(&self, template: &str) -> String {
        let type_str = match self.is_dir {
            Some(true) => "DIR",
            Some(false) => "FILE",
            None => "",
        };
        let mut result = template.to_string();
        result = result.replace("%file%", self.filename().unwrap_or(""));
        result = result.replace("%directory%", self.directory().unwrap_or(""));
        result = result.replace("%event%", &self.event_type.to_string());
        result = result.replace("%timestamp%", &self.timestamp.to_rfc3339());
        result = result.replace("%path%", self.path.to_str().unwrap_or(""));
        result = result.replace(
            "%target%",
            self.target_path
                .as_ref()
                .and_then(|p| p.to_str())
                .unwrap_or(""),
        );
        result = result.replace("%type%", type_str);
        result = result.replace("%user%", self.user.as_deref().unwrap_or("unknown"));
        result = result.replace("%process%", self.process.as_deref().unwrap_or("unknown"));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_event(event_type: EventType, path: &str) -> FsEvent {
        FsEvent::new(event_type, PathBuf::from(path), PathBuf::from("/watch"))
    }

    // === EventType Display ===

    #[test]
    fn test_event_type_display() {
        assert_eq!(EventType::Created.to_string(), "CREATE");
        assert_eq!(EventType::Modified.to_string(), "MODIFY");
        assert_eq!(EventType::Attrib.to_string(), "ATTRIB");
        assert_eq!(EventType::CloseWrite.to_string(), "CLOSE_WRITE");
        assert_eq!(EventType::CloseNoWrite.to_string(), "CLOSE_NOWRITE");
        assert_eq!(EventType::Opened.to_string(), "OPEN");
        assert_eq!(EventType::MovedTo.to_string(), "MOVED_TO");
        assert_eq!(EventType::MovedFrom.to_string(), "MOVED_FROM");
        assert_eq!(EventType::Deleted.to_string(), "DELETE");
        assert_eq!(EventType::Renamed.to_string(), "RENAME");
        assert_eq!(EventType::Accessed.to_string(), "ACCESS");
    }

    // === EventType classification ===

    #[test]
    fn test_event_type_is_content_change() {
        assert!(EventType::Created.is_content_change());
        assert!(EventType::Modified.is_content_change());
        assert!(EventType::CloseWrite.is_content_change());
        assert!(EventType::MovedTo.is_content_change());
        assert!(EventType::MovedFrom.is_content_change());
        assert!(EventType::Deleted.is_content_change());
        assert!(EventType::Renamed.is_content_change());

        assert!(!EventType::Attrib.is_content_change());
        assert!(!EventType::CloseNoWrite.is_content_change());
        assert!(!EventType::Opened.is_content_change());
        assert!(!EventType::Accessed.is_content_change());
    }

    #[test]
    fn test_event_type_is_metadata_change() {
        assert!(EventType::Attrib.is_metadata_change());

        assert!(!EventType::Created.is_metadata_change());
        assert!(!EventType::Modified.is_metadata_change());
        assert!(!EventType::Deleted.is_metadata_change());
        assert!(!EventType::Accessed.is_metadata_change());
    }

    #[test]
    fn test_event_type_is_access() {
        assert!(EventType::Accessed.is_access());
        assert!(EventType::Opened.is_access());
        assert!(EventType::CloseNoWrite.is_access());

        assert!(!EventType::Created.is_access());
        assert!(!EventType::Modified.is_access());
        assert!(!EventType::Deleted.is_access());
        assert!(!EventType::Attrib.is_access());
    }

    // === FsEvent::filename ===

    #[test]
    fn test_fsevent_filename_normal() {
        let event = make_event(EventType::Created, "/home/user/file.txt");
        assert_eq!(event.filename(), Some("file.txt"));
    }

    #[test]
    fn test_fsevent_filename_nested() {
        let event = make_event(EventType::Created, "/a/b/c/d.rs");
        assert_eq!(event.filename(), Some("d.rs"));
    }

    #[test]
    fn test_fsevent_filename_root() {
        let event = make_event(EventType::Created, "/");
        assert_eq!(event.filename(), None);
    }

    // === FsEvent::directory ===

    #[test]
    fn test_fsevent_directory_normal() {
        let event = make_event(EventType::Created, "/home/user/file.txt");
        assert_eq!(event.directory(), Some("/home/user"));
    }

    #[test]
    fn test_fsevent_directory_root() {
        let event = make_event(EventType::Created, "/file.txt");
        assert_eq!(event.directory(), Some("/"));
    }

    // === FsEvent::format_with ===

    #[test]
    fn test_fsevent_format_with_all_placeholders() {
        let event = FsEvent {
            id: Uuid::new_v4(),
            timestamp: chrono::DateTime::parse_from_rfc3339("2025-01-15T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            event_type: EventType::Created,
            path: PathBuf::from("/home/user/file.txt"),
            target_path: Some(PathBuf::from("/home/user/new.txt")),
            is_dir: Some(false),
            user: Some("alice".to_string()),
            process: Some("vim".to_string()),
            watch_root: PathBuf::from("/home/user"),
            node_id: String::new(),
        };

        let result =
            event.format_with("%event% %type% %file% in %directory% by %user% via %process%");
        assert_eq!(
            result,
            "CREATE FILE file.txt in /home/user by alice via vim"
        );
    }

    #[test]
    fn test_fsevent_format_with_path_and_target() {
        let event = FsEvent {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type: EventType::Renamed,
            path: PathBuf::from("/old/path.txt"),
            target_path: Some(PathBuf::from("/new/path.txt")),
            is_dir: None,
            user: None,
            process: None,
            watch_root: PathBuf::from("/"),
            node_id: String::new(),
        };

        let result = event.format_with("%path% -> %target%");
        assert_eq!(result, "/old/path.txt -> /new/path.txt");
    }

    #[test]
    fn test_fsevent_format_with_missing_optional_fields() {
        let event = make_event(EventType::Modified, "/tmp/test.log");

        let result = event.format_with("%event% %file% by %user% via %process%");
        assert_eq!(result, "MODIFY test.log by unknown via unknown");
    }

    #[test]
    fn test_fsevent_format_with_empty_template() {
        let event = make_event(EventType::Created, "/file.txt");
        assert_eq!(event.format_with(""), "");
    }

    #[test]
    fn test_fsevent_format_with_no_placeholders() {
        let event = make_event(EventType::Created, "/file.txt");
        assert_eq!(event.format_with("hello world"), "hello world");
    }

    #[test]
    fn test_fsevent_format_with_chinese_path() {
        let event = make_event(EventType::Created, "/home/用户/文档.txt");
        let result = event.format_with("%file% in %directory%");
        assert_eq!(result, "文档.txt in /home/用户");
    }

    #[test]
    fn test_fsevent_format_with_spaces_in_path() {
        let event = make_event(EventType::Created, "/my folder/my file.txt");
        let result = event.format_with("%file% at %path%");
        assert_eq!(result, "my file.txt at /my folder/my file.txt");
    }

    // === FsEvent builder methods ===

    #[test]
    fn test_fsevent_with_target() {
        let event =
            make_event(EventType::Renamed, "/old.txt").with_target(PathBuf::from("/new.txt"));
        assert_eq!(event.target_path, Some(PathBuf::from("/new.txt")));
    }

    #[test]
    fn test_fsevent_with_is_dir() {
        let event = make_event(EventType::Created, "/dir").with_is_dir(true);
        assert_eq!(event.is_dir, Some(true));

        let event = make_event(EventType::Created, "/file.txt").with_is_dir(false);
        assert_eq!(event.is_dir, Some(false));
    }

    #[test]
    fn test_fsevent_with_user() {
        let event = make_event(EventType::Created, "/file.txt").with_user("bob".to_string());
        assert_eq!(event.user, Some("bob".to_string()));
    }

    #[test]
    fn test_fsevent_with_process() {
        let event = make_event(EventType::Created, "/file.txt").with_process("git".to_string());
        assert_eq!(event.process, Some("git".to_string()));
    }

    // === is_dir type display ===

    #[test]
    fn test_fsevent_type_placeholder_dir() {
        let event = make_event(EventType::Created, "/mydir").with_is_dir(true);
        let result = event.format_with("%event% %type% %file%");
        assert_eq!(result, "CREATE DIR mydir");
    }

    #[test]
    fn test_fsevent_type_placeholder_file() {
        let event = make_event(EventType::Created, "/file.txt").with_is_dir(false);
        let result = event.format_with("%event% %type% %file%");
        assert_eq!(result, "CREATE FILE file.txt");
    }

    #[test]
    fn test_fsevent_type_placeholder_unknown() {
        let event = make_event(EventType::Modified, "/path");
        let result = event.format_with("%event% %type% %file%");
        assert_eq!(result, "MODIFY  path");
    }
}
