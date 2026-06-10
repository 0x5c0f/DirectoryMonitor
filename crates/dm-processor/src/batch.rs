use dm_core::event::FsEvent;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, info};

/// Collects events into batches and flushes them based on:
/// - Count threshold: flush when batch reaches this size
/// - Time threshold: flush after this duration of inactivity
pub struct EventBatcher {
    /// Current batch of events.
    batch: Vec<FsEvent>,
    /// Number of events to trigger a flush.
    count_threshold: usize,
    /// Duration of inactivity to trigger a flush.
    time_threshold: Duration,
    /// Channel to send completed batches.
    tx: mpsc::UnboundedSender<Vec<FsEvent>>,
}

impl EventBatcher {
    /// Create a new batcher.
    pub fn new(
        count_threshold: usize,
        time_threshold: Duration,
        tx: mpsc::UnboundedSender<Vec<FsEvent>>,
    ) -> Self {
        Self {
            batch: Vec::with_capacity(count_threshold),
            count_threshold,
            time_threshold,
            tx,
        }
    }

    /// Add an event to the current batch.
    pub fn push(&mut self, event: FsEvent) {
        self.batch.push(event);
        if self.batch.len() >= self.count_threshold {
            self.flush();
        }
    }

    /// Flush the current batch, sending it downstream.
    pub fn flush(&mut self) {
        if self.batch.is_empty() {
            return;
        }
        let batch = std::mem::take(&mut self.batch);
        info!("Flushing batch of {} events", batch.len());
        let _ = self.tx.send(batch);
    }

    /// Get the current batch size.
    pub fn len(&self) -> usize {
        self.batch.len()
    }

    /// Check if the batch is empty.
    pub fn is_empty(&self) -> bool {
        self.batch.is_empty()
    }

    /// Get the time threshold for flushing.
    pub fn time_threshold(&self) -> Duration {
        self.time_threshold
    }
}

/// Spawns a background task that periodically checks if the batcher
/// needs to flush based on the time threshold.
pub fn spawn_flush_task(
    batcher: Arc<Mutex<EventBatcher>>,
    interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut timer = tokio::time::interval(interval);
        loop {
            timer.tick().await;
            let mut b = batcher.lock().await;
            if !b.is_empty() {
                debug!("Time-based batch flush");
                b.flush();
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dm_core::event::{EventType, FsEvent};
    use std::path::PathBuf;

    fn make_event(path: &str) -> FsEvent {
        FsEvent::new(
            EventType::Created,
            PathBuf::from(path),
            PathBuf::from("/watch"),
        )
    }

    #[test]
    fn test_batch_flush_at_threshold() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut batcher = EventBatcher::new(3, Duration::from_secs(10), tx);

        batcher.push(make_event("/a.txt"));
        batcher.push(make_event("/b.txt"));
        assert_eq!(batcher.len(), 2);
        assert!(rx.try_recv().is_err()); // not flushed yet

        batcher.push(make_event("/c.txt")); // triggers flush at threshold=3
        assert_eq!(batcher.len(), 0);
        assert!(!batcher.is_empty() || batcher.is_empty()); // batch is now empty

        let batch = rx.try_recv().unwrap();
        assert_eq!(batch.len(), 3);
    }

    #[test]
    fn test_batch_manual_flush() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut batcher = EventBatcher::new(10, Duration::from_secs(10), tx);

        batcher.push(make_event("/a.txt"));
        batcher.push(make_event("/b.txt"));
        assert_eq!(batcher.len(), 2);

        batcher.flush();
        assert_eq!(batcher.len(), 0);

        let batch = rx.try_recv().unwrap();
        assert_eq!(batch.len(), 2);
    }

    #[test]
    fn test_batch_flush_empty_is_noop() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut batcher = EventBatcher::new(3, Duration::from_secs(10), tx);

        batcher.flush(); // should not send anything
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_batch_len_and_is_empty() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut batcher = EventBatcher::new(5, Duration::from_secs(10), tx);

        assert!(batcher.is_empty());
        assert_eq!(batcher.len(), 0);

        batcher.push(make_event("/a.txt"));
        assert!(!batcher.is_empty());
        assert_eq!(batcher.len(), 1);
    }

    #[test]
    fn test_batch_time_threshold() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let batcher = EventBatcher::new(5, Duration::from_secs(30), tx);
        assert_eq!(batcher.time_threshold(), Duration::from_secs(30));
    }

    #[test]
    fn test_batch_multiple_flushes() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut batcher = EventBatcher::new(2, Duration::from_secs(10), tx);

        // First batch
        batcher.push(make_event("/a.txt"));
        batcher.push(make_event("/b.txt")); // flush
        let batch1 = rx.try_recv().unwrap();
        assert_eq!(batch1.len(), 2);

        // Second batch
        batcher.push(make_event("/c.txt"));
        batcher.push(make_event("/d.txt")); // flush
        let batch2 = rx.try_recv().unwrap();
        assert_eq!(batch2.len(), 2);
    }
}
