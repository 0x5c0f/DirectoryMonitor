use dm_core::event::{EventType, FsEvent};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::debug;

/// Key for dedup: same file + same event type.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct DedupKey {
    path: PathBuf,
    event_type: EventType,
}

/// Deduplicates filesystem events within a configurable time window.
/// Uses (path, event_type) as the key, so different event types for the
/// same file (e.g., CREATE + MODIFY) are NOT treated as duplicates.
pub struct EventDeduplicator {
    /// Time window for deduplication.
    window: Duration,
    /// Map of (path, event_type) -> (first_event_time, kept_event).
    seen: HashMap<DedupKey, (Instant, FsEvent)>,
}

impl EventDeduplicator {
    /// Create a new deduplicator with the given time window.
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            seen: HashMap::new(),
        }
    }

    /// Process an event: returns Some(event) if it's new, None if it's a duplicate.
    pub fn process(&mut self, event: FsEvent) -> Option<FsEvent> {
        let now = Instant::now();
        self.cleanup(now);

        let key = DedupKey {
            path: event.path.clone(),
            event_type: event.event_type,
        };

        match self.seen.get(&key) {
            Some((first_time, _)) => {
                if now.duration_since(*first_time) < self.window {
                    debug!(
                        "Deduplicated {:?} for: {}",
                        event.event_type,
                        event.path.display()
                    );
                    None
                } else {
                    // Outside window, treat as new
                    self.seen.insert(key, (now, event.clone()));
                    Some(event)
                }
            }
            None => {
                self.seen.insert(key, (now, event.clone()));
                Some(event)
            }
        }
    }

    /// Process a batch of events, filtering out duplicates.
    pub fn process_batch(&mut self, events: Vec<FsEvent>) -> Vec<FsEvent> {
        events.into_iter().filter_map(|e| self.process(e)).collect()
    }

    /// Remove entries older than the dedup window.
    fn cleanup(&mut self, now: Instant) {
        self.seen
            .retain(|_, (time, _)| now.duration_since(*time) < self.window);
    }
}
