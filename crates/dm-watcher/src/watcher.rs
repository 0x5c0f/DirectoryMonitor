use dm_core::event::{EventType, FsEvent};
use notify::event::{AccessKind, AccessMode, ModifyKind, RenameMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher, Config};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::mpsc as tokio_mpsc;
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
}

impl FsWatcher {
    /// Create a new watcher that sends events to the provided channel.
    /// Events are debounced with the given timeout.
    pub fn new(
        tx: tokio_mpsc::UnboundedSender<WatchEvent>,
        debounce_duration: Duration,
    ) -> Result<Self, String> {
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
        .map_err(|e| format!("Failed to create watcher: {e}"))?;

        // Spawn a thread to receive events, debounce, and forward to async channel
        std::thread::spawn(move || {
            debounce_loop(notify_rx, tx, debounce_duration);
        });

        Ok(Self { _watcher: watcher })
    }

    /// Add a directory to watch based on a WatchConfig.
    pub fn add_watch(
        &mut self,
        config: &dm_core::config::WatchConfig,
    ) -> Result<(), String> {
        if !config.path.exists() {
            return Err(format!("Watch path does not exist: {}", config.path.display()));
        }

        let recursive = if config.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        self._watcher
            .watch(&config.path, recursive)
            .map_err(|e| format!("Failed to watch {}: {e}", config.path.display()))?;

        info!("Watching: {} (recursive: {})", config.path.display(), config.recursive);
        Ok(())
    }

    /// Remove a directory from watching.
    pub fn remove_watch(&mut self, path: &Path) -> Result<(), String> {
        self._watcher
            .unwatch(path)
            .map_err(|e| format!("Failed to unwatch {}: {e}", path.display()))?;
        info!("Stopped watching: {}", path.display());
        Ok(())
    }
}


/// Debounce loop: collects events within a time window, then sends as a batch.
/// All events are preserved — deduplication only removes truly identical events
/// (same path + same type within the window).
fn debounce_loop(
    rx: mpsc::Receiver<notify::Result<Event>>,
    tx: tokio_mpsc::UnboundedSender<WatchEvent>,
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
                    let events: Vec<FsEvent> = pending.drain(..).collect();

                    debug!("Debounced batch: {} events", events.len());
                    let _ = tx.send(WatchEvent::Batch(events.clone()));
                    for event in events {
                        let _ = tx.send(WatchEvent::Event(event));
                    }
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
    let (event_type, path, target_path) = match event.kind {
        // CREATE → Created
        EventKind::Create(_) => (EventType::Created, event.paths.first()?.clone(), None),

        // MODIFY → depends on sub-type
        EventKind::Modify(ModifyKind::Data(_)) => {
            // Content changed (size, content)
            (EventType::Modified, event.paths.first()?.clone(), None)
        }
        EventKind::Modify(ModifyKind::Metadata(_)) => {
            // Metadata changed (permissions, timestamps, owner)
            (EventType::Attrib, event.paths.first()?.clone(), None)
        }
        // Rename events: prefer MOVED_FROM + MOVED_TO pair (inotifywait style).
        // Skip RenameMode::Both to avoid duplicate rename reporting.
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
            // Skip: prefer the more specific From/To events.
            return None;
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
            // MOVED_FROM: file moved out of watched directory.
            (EventType::MovedFrom, event.paths.first()?.clone(), None)
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
            // MOVED_TO: file moved into watched directory.
            (EventType::MovedTo, event.paths.first()?.clone(), None)
        }
        EventKind::Modify(_) => {
            // Catch-all for other modify events
            (EventType::Modified, event.paths.first()?.clone(), None)
        }

        // DELETE → Deleted
        EventKind::Remove(_) => (EventType::Deleted, event.paths.first()?.clone(), None),

        // ACCESS → depends on sub-type (this is where CLOSE_WRITE lives!)
        EventKind::Access(AccessKind::Read) => {
            // File contents were read
            (EventType::Accessed, event.paths.first()?.clone(), None)
        }
        EventKind::Access(AccessKind::Open(_)) => {
            // File was opened
            (EventType::Opened, event.paths.first()?.clone(), None)
        }
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => {
            // CLOSE_WRITE: file was written to and then closed.
            // This is the most reliable signal that a write is complete.
            (EventType::CloseWrite, event.paths.first()?.clone(), None)
        }
        EventKind::Access(AccessKind::Close(AccessMode::Read)) => {
            // CLOSE_NOWRITE: read-only file was closed
            (EventType::CloseNoWrite, event.paths.first()?.clone(), None)
        }
        EventKind::Access(AccessKind::Close(_)) => {
            // Close with unknown mode
            (EventType::CloseNoWrite, event.paths.first()?.clone(), None)
        }
        EventKind::Access(_) => {
            // Catch-all for other access events
            (EventType::Accessed, event.paths.first()?.clone(), None)
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

    Some(fs_event)
}

/// Try to determine the watch root (parent directory being watched).
fn find_watch_root(path: &Path) -> Option<PathBuf> {
    path.parent().map(|p| p.to_path_buf())
}
