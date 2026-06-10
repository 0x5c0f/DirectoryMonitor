use dm_core::error::WatcherError;
use dm_core::event::{EventType, FsEvent};
use notify::event::{AccessKind, AccessMode, CreateKind, ModifyKind, RemoveKind, RenameMode};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, error, info};

/// Event emitted by the watcher to consumers.
#[derive(Debug, Clone)]
pub enum WatchEvent {
    /// A filesystem event.
    Event(FsEvent),
    /// A batch of debounced events.
    Batch(Vec<FsEvent>),
    /// Watcher error.
    Error(String),
}

/// Wraps the notify crate to provide filesystem monitoring with debouncing.
pub struct FsWatcher {
    _watcher: RecommendedWatcher,
    /// Thread ID where the debounce loop runs.
    thread_id: Option<String>,
}

impl FsWatcher {
    /// Create a new watcher that sends events to the provided broadcast channel.
    /// Events are debounced with the given timeout.
    pub fn new(
        tx: broadcast::Sender<WatchEvent>,
        debounce_duration: Duration,
    ) -> Result<Self, WatcherError> {
        // Channel from notify (sync) to our processing thread
        let (notify_tx, notify_rx) = mpsc::channel();

        let config = Config::default();

        let watcher = RecommendedWatcher::new(
            move |result: notify::Result<Event>| {
                if let Err(e) = notify_tx.send(result) {
                    error!("Failed to send notify event: {e}");
                }
            },
            config,
        )
        .map_err(|e| WatcherError::Init(format!("Failed to create watcher: {e}")))?;

        // Spawn a thread to receive events, debounce, and forward to async channel
        let handle = std::thread::spawn(move || {
            debounce_loop(notify_rx, tx, debounce_duration);
        });

        let thread_id = format!("{:?}", handle.thread().id());

        Ok(Self {
            _watcher: watcher,
            thread_id: Some(thread_id),
        })
    }

    /// Get the thread ID where this watcher's debounce loop runs.
    pub fn thread_id(&self) -> Option<&str> {
        self.thread_id.as_deref()
    }

    /// Add a directory to watch based on a WatchConfig.
    pub fn add_watch(&mut self, config: &dm_core::config::WatchConfig) -> Result<(), WatcherError> {
        if !config.path.exists() {
            return Err(WatcherError::Init(format!(
                "Watch path does not exist: {}",
                config.path.display()
            )));
        }

        let recursive = if config.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        self._watcher
            .watch(&config.path, recursive)
            .map_err(|e| WatcherError::AddWatch {
                path: config.path.display().to_string(),
                source: Box::new(e),
            })?;

        info!(
            "Watching: {} (recursive: {})",
            config.path.display(),
            config.recursive
        );
        Ok(())
    }

    /// Remove a directory from watching.
    pub fn remove_watch(&mut self, path: &Path) -> Result<(), WatcherError> {
        self._watcher
            .unwatch(path)
            .map_err(|e| WatcherError::RemoveWatch {
                path: path.display().to_string(),
                source: Box::new(e),
            })?;
        info!("Stopped watching: {}", path.display());
        Ok(())
    }
}

/// Debounce loop: collects events within a time window, then sends as a batch.
/// All events are preserved — deduplication only removes truly identical events
/// (same path + same type within the window).
fn debounce_loop(
    rx: mpsc::Receiver<notify::Result<Event>>,
    tx: broadcast::Sender<WatchEvent>,
    debounce_duration: Duration,
) {
    let mut pending: Vec<FsEvent> = Vec::new();

    loop {
        match rx.recv_timeout(debounce_duration) {
            Ok(Ok(event)) => {
                if let Some(fs_event) = convert_event(&event) {
                    pending.push(fs_event);
                }
            }
            Ok(Err(e)) => {
                error!("Watch error: {:?}", e);
                let _ = tx.send(WatchEvent::Error(format!("{e:?}")));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Flush all pending events as a batch
                if !pending.is_empty() {
                    let events: Vec<FsEvent> = mem::take(&mut pending);

                    debug!("Debounced batch: {} events", events.len());
                    let _ = tx.send(WatchEvent::Batch(events));
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                info!("Watcher channel disconnected");
                break;
            }
        }
    }
}

/// Convert a notify event into our FsEvent type.
///
/// Maps notify's event types to inotifywait-compatible categories:
///   ACCESS, MODIFY, ATTRIB, CLOSE_WRITE, CLOSE_NOWRITE, OPEN,
///   MOVED_TO, MOVED_FROM, CREATE, DELETE, RENAME
fn convert_event(event: &Event) -> Option<FsEvent> {
    let (event_type, path, target_path, is_dir) = match event.kind {
        // CREATE → Created (with file type from CreateKind)
        EventKind::Create(CreateKind::File) => (
            EventType::Created,
            event.paths.first()?.clone(),
            None,
            Some(false),
        ),
        EventKind::Create(CreateKind::Folder) => (
            EventType::Created,
            event.paths.first()?.clone(),
            None,
            Some(true),
        ),
        EventKind::Create(_) => {
            // CreateKind::Any or Other — try to stat the path
            let path = event.paths.first()?.clone();
            let is_dir = std::fs::metadata(&path).ok().map(|m| m.is_dir());
            (EventType::Created, path, None, is_dir)
        }

        // MODIFY → depends on sub-type
        EventKind::Modify(ModifyKind::Data(_)) => {
            // Content changed (size, content)
            let path = event.paths.first()?.clone();
            let is_dir = std::fs::metadata(&path).ok().map(|m| m.is_dir());
            (EventType::Modified, path, None, is_dir)
        }
        EventKind::Modify(ModifyKind::Metadata(_)) => {
            // Metadata changed (permissions, timestamps, owner)
            let path = event.paths.first()?.clone();
            let is_dir = std::fs::metadata(&path).ok().map(|m| m.is_dir());
            (EventType::Attrib, path, None, is_dir)
        }
        // Rename events: prefer MOVED_FROM + MOVED_TO pair (inotifywait style).
        // Skip RenameMode::Both to avoid duplicate rename reporting.
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
            // Skip: prefer the more specific From/To events.
            return None;
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
            // MOVED_FROM: file moved out of watched directory.
            (
                EventType::MovedFrom,
                event.paths.first()?.clone(),
                None,
                None,
            )
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
            // MOVED_TO: file moved into watched directory.
            (EventType::MovedTo, event.paths.first()?.clone(), None, None)
        }
        EventKind::Modify(_) => {
            // Catch-all for other modify events
            let path = event.paths.first()?.clone();
            let is_dir = std::fs::metadata(&path).ok().map(|m| m.is_dir());
            (EventType::Modified, path, None, is_dir)
        }

        // DELETE → Deleted (with file type from RemoveKind)
        EventKind::Remove(RemoveKind::File) => (
            EventType::Deleted,
            event.paths.first()?.clone(),
            None,
            Some(false),
        ),
        EventKind::Remove(RemoveKind::Folder) => (
            EventType::Deleted,
            event.paths.first()?.clone(),
            None,
            Some(true),
        ),
        EventKind::Remove(_) => {
            // RemoveKind::Any or Other — unknown type
            (EventType::Deleted, event.paths.first()?.clone(), None, None)
        }

        // ACCESS → depends on sub-type (this is where CLOSE_WRITE lives!)
        EventKind::Access(AccessKind::Read) => {
            // File contents were read
            let path = event.paths.first()?.clone();
            let is_dir = std::fs::metadata(&path).ok().map(|m| m.is_dir());
            (EventType::Accessed, path, None, is_dir)
        }
        EventKind::Access(AccessKind::Open(_)) => {
            // File was opened
            let path = event.paths.first()?.clone();
            let is_dir = std::fs::metadata(&path).ok().map(|m| m.is_dir());
            (EventType::Opened, path, None, is_dir)
        }
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => {
            // CLOSE_WRITE: file was written to and then closed.
            // This is the most reliable signal that a write is complete.
            let path = event.paths.first()?.clone();
            let is_dir = std::fs::metadata(&path).ok().map(|m| m.is_dir());
            (EventType::CloseWrite, path, None, is_dir)
        }
        EventKind::Access(AccessKind::Close(AccessMode::Read)) => {
            // CLOSE_NOWRITE: read-only file was closed
            let path = event.paths.first()?.clone();
            let is_dir = std::fs::metadata(&path).ok().map(|m| m.is_dir());
            (EventType::CloseNoWrite, path, None, is_dir)
        }
        EventKind::Access(AccessKind::Close(_)) => {
            // Close with unknown mode
            let path = event.paths.first()?.clone();
            let is_dir = std::fs::metadata(&path).ok().map(|m| m.is_dir());
            (EventType::CloseNoWrite, path, None, is_dir)
        }
        EventKind::Access(_) => {
            // Catch-all for other access events
            let path = event.paths.first()?.clone();
            let is_dir = std::fs::metadata(&path).ok().map(|m| m.is_dir());
            (EventType::Accessed, path, None, is_dir)
        }

        _ => {
            debug!("Ignoring event kind: {:?}", event.kind);
            return None;
        }
    };

    let watch_root = event
        .paths
        .first()
        .and_then(|p| find_watch_root(p))
        .unwrap_or_else(|| PathBuf::from("/"));

    let mut fs_event = FsEvent::new(event_type, path, watch_root);
    if let Some(target) = target_path {
        fs_event = fs_event.with_target(target);
    }
    if let Some(dir) = is_dir {
        fs_event = fs_event.with_is_dir(dir);
    }

    Some(fs_event)
}

/// Try to determine the watch root (parent directory being watched).
fn find_watch_root(path: &Path) -> Option<PathBuf> {
    path.parent().map(|p| p.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{
        AccessKind, AccessMode, CreateKind, DataChange, MetadataKind, ModifyKind, RemoveKind,
        RenameMode,
    };

    fn make_notify_event(kind: EventKind, paths: Vec<PathBuf>) -> Event {
        let mut event = Event::new(kind);
        event.paths = paths;
        event
    }

    // === Create events ===

    #[test]
    fn test_convert_create_file() {
        let event = make_notify_event(
            EventKind::Create(CreateKind::File),
            vec![PathBuf::from("/test/file.txt")],
        );
        let result = convert_event(&event).unwrap();
        assert_eq!(result.event_type, EventType::Created);
        assert_eq!(result.path, PathBuf::from("/test/file.txt"));
        assert_eq!(result.is_dir, Some(false));
        assert!(result.target_path.is_none());
    }

    #[test]
    fn test_convert_create_folder() {
        let event = make_notify_event(
            EventKind::Create(CreateKind::Folder),
            vec![PathBuf::from("/test/newdir")],
        );
        let result = convert_event(&event).unwrap();
        assert_eq!(result.event_type, EventType::Created);
        assert_eq!(result.is_dir, Some(true));
    }

    // === Modify events ===

    #[test]
    fn test_convert_modify_data() {
        let event = make_notify_event(
            EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            vec![PathBuf::from("/test/file.txt")],
        );
        let result = convert_event(&event).unwrap();
        assert_eq!(result.event_type, EventType::Modified);
    }

    #[test]
    fn test_convert_modify_metadata() {
        let event = make_notify_event(
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::Any)),
            vec![PathBuf::from("/test/file.txt")],
        );
        let result = convert_event(&event).unwrap();
        assert_eq!(result.event_type, EventType::Attrib);
    }

    // === Rename events ===

    #[test]
    fn test_convert_rename_both_skipped() {
        let event = make_notify_event(
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            vec![
                PathBuf::from("/test/old.txt"),
                PathBuf::from("/test/new.txt"),
            ],
        );
        assert!(convert_event(&event).is_none());
    }

    #[test]
    fn test_convert_rename_from() {
        let event = make_notify_event(
            EventKind::Modify(ModifyKind::Name(RenameMode::From)),
            vec![PathBuf::from("/test/old.txt")],
        );
        let result = convert_event(&event).unwrap();
        assert_eq!(result.event_type, EventType::MovedFrom);
    }

    #[test]
    fn test_convert_rename_to() {
        let event = make_notify_event(
            EventKind::Modify(ModifyKind::Name(RenameMode::To)),
            vec![PathBuf::from("/test/new.txt")],
        );
        let result = convert_event(&event).unwrap();
        assert_eq!(result.event_type, EventType::MovedTo);
    }

    // === Remove events ===

    #[test]
    fn test_convert_remove_file() {
        let event = make_notify_event(
            EventKind::Remove(RemoveKind::File),
            vec![PathBuf::from("/test/file.txt")],
        );
        let result = convert_event(&event).unwrap();
        assert_eq!(result.event_type, EventType::Deleted);
        assert_eq!(result.is_dir, Some(false));
    }

    #[test]
    fn test_convert_remove_folder() {
        let event = make_notify_event(
            EventKind::Remove(RemoveKind::Folder),
            vec![PathBuf::from("/test/olddir")],
        );
        let result = convert_event(&event).unwrap();
        assert_eq!(result.event_type, EventType::Deleted);
        assert_eq!(result.is_dir, Some(true));
    }

    // === Access events ===

    #[test]
    fn test_convert_access_read() {
        let event = make_notify_event(
            EventKind::Access(AccessKind::Read),
            vec![PathBuf::from("/test/file.txt")],
        );
        let result = convert_event(&event).unwrap();
        assert_eq!(result.event_type, EventType::Accessed);
    }

    #[test]
    fn test_convert_access_open() {
        let event = make_notify_event(
            EventKind::Access(AccessKind::Open(AccessMode::Any)),
            vec![PathBuf::from("/test/file.txt")],
        );
        let result = convert_event(&event).unwrap();
        assert_eq!(result.event_type, EventType::Opened);
    }

    #[test]
    fn test_convert_access_close_write() {
        let event = make_notify_event(
            EventKind::Access(AccessKind::Close(AccessMode::Write)),
            vec![PathBuf::from("/test/file.txt")],
        );
        let result = convert_event(&event).unwrap();
        assert_eq!(result.event_type, EventType::CloseWrite);
    }

    #[test]
    fn test_convert_access_close_read() {
        let event = make_notify_event(
            EventKind::Access(AccessKind::Close(AccessMode::Read)),
            vec![PathBuf::from("/test/file.txt")],
        );
        let result = convert_event(&event).unwrap();
        assert_eq!(result.event_type, EventType::CloseNoWrite);
    }

    // === Edge cases ===

    #[test]
    fn test_convert_unknown_event_kind() {
        let event = make_notify_event(EventKind::Any, vec![PathBuf::from("/test/file.txt")]);
        assert!(convert_event(&event).is_none());
    }

    #[test]
    fn test_convert_empty_paths() {
        let event = make_notify_event(EventKind::Create(CreateKind::File), vec![]);
        assert!(convert_event(&event).is_none());
    }

    #[test]
    fn test_convert_watch_root_is_parent() {
        let event = make_notify_event(
            EventKind::Create(CreateKind::File),
            vec![PathBuf::from("/home/user/project/file.txt")],
        );
        let result = convert_event(&event).unwrap();
        assert_eq!(result.watch_root, PathBuf::from("/home/user/project"));
    }
}
