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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(event_type: EventType, path: &str) -> FsEvent {
        FsEvent::new(event_type, PathBuf::from(path), PathBuf::from("/watch"))
    }

    #[test]
    fn test_dedup_first_event_passes() {
        let mut dedup = EventDeduplicator::new(Duration::from_millis(500));
        let event = make_event(EventType::Created, "/file.txt");
        assert!(dedup.process(event).is_some());
    }

    #[test]
    fn test_dedup_duplicate_within_window() {
        let mut dedup = EventDeduplicator::new(Duration::from_secs(5));
        let e1 = make_event(EventType::Created, "/file.txt");
        let e2 = make_event(EventType::Created, "/file.txt");

        assert!(dedup.process(e1).is_some());
        assert!(dedup.process(e2).is_none());
    }

    #[test]
    fn test_dedup_different_type_same_path() {
        let mut dedup = EventDeduplicator::new(Duration::from_secs(5));
        let created = make_event(EventType::Created, "/file.txt");
        let modified = make_event(EventType::Modified, "/file.txt");

        assert!(dedup.process(created).is_some());
        assert!(dedup.process(modified).is_some());
    }

    #[test]
    fn test_dedup_different_path_same_type() {
        let mut dedup = EventDeduplicator::new(Duration::from_secs(5));
        let e1 = make_event(EventType::Created, "/a.txt");
        let e2 = make_event(EventType::Created, "/b.txt");

        assert!(dedup.process(e1).is_some());
        assert!(dedup.process(e2).is_some());
    }

    #[test]
    fn test_dedup_batch() {
        let mut dedup = EventDeduplicator::new(Duration::from_secs(5));
        let events = vec![
            make_event(EventType::Created, "/a.txt"),
            make_event(EventType::Created, "/a.txt"),  // dup
            make_event(EventType::Modified, "/a.txt"), // different type
            make_event(EventType::Created, "/b.txt"),
            make_event(EventType::Created, "/b.txt"), // dup
        ];

        let result = dedup.process_batch(events);
        assert_eq!(result.len(), 3); // a.txt/created, a.txt/modified, b.txt/created
    }

    #[test]
    fn test_dedup_after_window_expires() {
        let mut dedup = EventDeduplicator::new(Duration::from_millis(50));
        let e1 = make_event(EventType::Created, "/file.txt");
        assert!(dedup.process(e1).is_some());

        // Wait for window to expire
        std::thread::sleep(Duration::from_millis(100));

        let e2 = make_event(EventType::Created, "/file.txt");
        assert!(dedup.process(e2).is_some());
    }
}
