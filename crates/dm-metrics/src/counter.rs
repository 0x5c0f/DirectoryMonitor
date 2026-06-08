use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

/// Label key-value pairs used to identify a counter.
type Labels = Vec<(String, String)>;

/// A thread-safe counter that can be incremented atomically.
#[derive(Debug)]
pub struct AtomicCounter {
    value: AtomicI64,
}

impl AtomicCounter {
    /// Create a new counter with initial value 0.
    pub fn new() -> Self {
        Self {
            value: AtomicI64::new(0),
        }
    }

    /// Create a new counter with a specific initial value.
    pub fn with_value(value: i64) -> Self {
        Self {
            value: AtomicI64::new(value),
        }
    }

    /// Increment the counter by 1.
    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the counter by a given amount.
    pub fn add(&self, value: i64) {
        self.value.fetch_add(value, Ordering::Relaxed);
    }

    /// Decrement the counter by 1.
    pub fn dec(&self) {
        self.value.fetch_sub(1, Ordering::Relaxed);
    }

    /// Get the current value.
    pub fn get(&self) -> i64 {
        self.value.load(Ordering::Relaxed)
    }

    /// Set the counter to a specific value.
    pub fn set(&self, value: i64) {
        self.value.store(value, Ordering::Relaxed);
    }

    /// Reset the counter to 0 and return the previous value.
    pub fn reset(&self) -> i64 {
        self.value.swap(0, Ordering::Relaxed)
    }
}

impl Default for AtomicCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for AtomicCounter {
    fn clone(&self) -> Self {
        Self::with_value(self.get())
    }
}

/// A labeled counter that supports Prometheus-style labels.
#[derive(Debug)]
pub struct LabeledCounter {
    /// Metric name (e.g., "dm_events_total")
    name: String,
    /// Counters keyed by label combination
    counters: parking_lot::RwLock<HashMap<Labels, Arc<AtomicCounter>>>,
}

impl LabeledCounter {
    /// Create a new labeled counter.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            counters: parking_lot::RwLock::new(HashMap::new()),
        }
    }

    /// Get or create a counter for the given labels.
    pub fn with_labels(&self, labels: &[(&str, &str)]) -> Arc<AtomicCounter> {
        let key: Labels = labels
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        // Fast path: read lock
        {
            let counters = self.counters.read();
            if let Some(counter) = counters.get(&key) {
                return counter.clone();
            }
        }

        // Slow path: write lock
        let mut counters = self.counters.write();
        // Double-check after acquiring write lock
        if let Some(counter) = counters.get(&key) {
            return counter.clone();
        }
        let counter = Arc::new(AtomicCounter::new());
        counters.insert(key, counter.clone());
        counter
    }

    /// Increment the counter with the given labels.
    pub fn inc(&self, labels: &[(&str, &str)]) {
        self.with_labels(labels).inc();
    }

    /// Increment the counter with the given labels by a specific amount.
    pub fn add(&self, value: i64, labels: &[(&str, &str)]) {
        self.with_labels(labels).add(value);
    }

    /// Get the metric name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get all label combinations and their current values.
    pub fn snapshot(&self) -> Vec<(Labels, i64)> {
        let counters = self.counters.read();
        counters
            .iter()
            .map(|(labels, counter)| (labels.clone(), counter.get()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atomic_counter() {
        let counter = AtomicCounter::new();
        assert_eq!(counter.get(), 0);

        counter.inc();
        assert_eq!(counter.get(), 1);

        counter.add(5);
        assert_eq!(counter.get(), 6);

        counter.dec();
        assert_eq!(counter.get(), 5);

        let old = counter.reset();
        assert_eq!(old, 5);
        assert_eq!(counter.get(), 0);
    }

    #[test]
    fn test_labeled_counter() {
        let counter = LabeledCounter::new("test_total");

        counter.inc(&[("type", "create")]);
        counter.inc(&[("type", "create")]);
        counter.inc(&[("type", "delete")]);

        assert_eq!(counter.with_labels(&[("type", "create")]).get(), 2);
        assert_eq!(counter.with_labels(&[("type", "delete")]).get(), 1);

        let snapshot = counter.snapshot();
        assert_eq!(snapshot.len(), 2);
    }
}
