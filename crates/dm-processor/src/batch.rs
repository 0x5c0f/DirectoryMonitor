use dm_core::event::FsEvent;
use std::time::Duration;
use tokio::sync::mpsc;
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

use std::sync::Arc;
use tokio::sync::Mutex;
