use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use std::collections::VecDeque;

/// A time-series data point.
#[derive(Debug, Clone, Serialize)]
pub struct DataPoint {
    /// Timestamp of the bucket.
    pub timestamp: DateTime<Utc>,
    /// Value in this bucket.
    pub value: i64,
}

/// Configuration for a sliding window.
#[derive(Debug, Clone)]
pub struct WindowConfig {
    /// Duration of each bucket.
    pub bucket_duration: Duration,
    /// Number of buckets to keep.
    pub bucket_count: usize,
}

impl WindowConfig {
    /// Create a new window config.
    pub fn new(bucket_duration: Duration, bucket_count: usize) -> Self {
        Self {
            bucket_duration,
            bucket_count,
        }
    }

    /// Total duration covered by the window.
    pub fn total_duration(&self) -> Duration {
        self.bucket_duration * self.bucket_count as i32
    }

    /// Per-minute buckets for 1 hour.
    pub fn per_minute_1h() -> Self {
        Self::new(Duration::minutes(1), 60)
    }

    /// Per-minute buckets for 24 hours.
    pub fn per_minute_24h() -> Self {
        Self::new(Duration::minutes(1), 1440)
    }

    /// Per-hour buckets for 7 days.
    pub fn per_hour_7d() -> Self {
        Self::new(Duration::hours(1), 168)
    }
}

/// A sliding window that aggregates values into time buckets.
#[derive(Debug)]
pub struct SlidingWindow {
    /// Configuration.
    config: WindowConfig,
    /// Buckets stored as (timestamp, value) pairs, ordered by timestamp.
    buckets: VecDeque<(DateTime<Utc>, i64)>,
}

impl SlidingWindow {
    /// Create a new sliding window.
    pub fn new(config: WindowConfig) -> Self {
        Self {
            config,
            buckets: VecDeque::new(),
        }
    }

    /// Get the bucket timestamp for a given time.
    fn bucket_time(&self, time: DateTime<Utc>) -> DateTime<Utc> {
        let bucket_secs = self.config.bucket_duration.num_seconds();
        let timestamp_secs = time.timestamp();
        let aligned = timestamp_secs - (timestamp_secs % bucket_secs);
        DateTime::from_timestamp(aligned, 0).unwrap_or(time)
    }

    /// Add a value to the appropriate bucket.
    pub fn record(&mut self, value: i64) {
        self.record_at(value, Utc::now());
    }

    /// Add a value at a specific time (for testing).
    pub fn record_at(&mut self, value: i64, time: DateTime<Utc>) {
        let bucket_time = self.bucket_time(time);

        // Try to update existing bucket
        if let Some(last) = self.buckets.back_mut() {
            if last.0 == bucket_time {
                last.1 += value;
                self.trim();
                return;
            }
        }

        // Add new bucket
        self.buckets.push_back((bucket_time, value));
        self.trim();
    }

    /// Remove old buckets outside the window.
    fn trim(&mut self) {
        let cutoff = Utc::now() - self.config.total_duration();
        while let Some(front) = self.buckets.front() {
            if front.0 < cutoff {
                self.buckets.pop_front();
            } else {
                break;
            }
        }
    }

    /// Get all data points in the window.
    pub fn data_points(&self) -> Vec<DataPoint> {
        self.buckets
            .iter()
            .map(|(ts, val)| DataPoint {
                timestamp: *ts,
                value: *val,
            })
            .collect()
    }

    /// Get the sum of all values in the window.
    pub fn sum(&self) -> i64 {
        self.buckets.iter().map(|(_, v)| *v).sum()
    }

    /// Get the number of buckets with data.
    pub fn len(&self) -> usize {
        self.buckets.len()
    }

    /// Check if the window is empty.
    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }

    /// Clear all data.
    pub fn clear(&mut self) {
        self.buckets.clear();
    }

    /// Get the latest value.
    pub fn latest(&self) -> Option<i64> {
        self.buckets.back().map(|(_, v)| *v)
    }
}

/// A labeled sliding window that supports multiple time series.
#[derive(Debug)]
pub struct LabeledWindow {
    /// Window configuration.
    config: WindowConfig,
    /// Named windows.
    windows: parking_lot::RwLock<std::collections::HashMap<String, SlidingWindow>>,
}

impl LabeledWindow {
    /// Create a new labeled window.
    pub fn new(config: WindowConfig) -> Self {
        Self {
            config,
            windows: parking_lot::RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Record a value for a named series.
    pub fn record(&self, name: &str, value: i64) {
        let mut windows = self.windows.write();
        let window = windows
            .entry(name.to_string())
            .or_insert_with(|| SlidingWindow::new(self.config.clone()));
        window.record(value);
    }

    /// Get data points for a named series.
    pub fn data_points(&self, name: &str) -> Vec<DataPoint> {
        let windows = self.windows.read();
        windows
            .get(name)
            .map(|w| w.data_points())
            .unwrap_or_default()
    }

    /// Get all series names.
    pub fn series_names(&self) -> Vec<String> {
        let windows = self.windows.read();
        windows.keys().cloned().collect()
    }

    /// Get the sum for a named series.
    pub fn sum(&self, name: &str) -> i64 {
        let windows = self.windows.read();
        windows.get(name).map(|w| w.sum()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === WindowConfig ===

    #[test]
    fn test_window_config_total_duration() {
        let config = WindowConfig::new(Duration::seconds(10), 6);
        assert_eq!(config.total_duration(), Duration::seconds(60));
    }

    #[test]
    fn test_window_config_per_minute_1h() {
        let config = WindowConfig::per_minute_1h();
        assert_eq!(config.bucket_duration, Duration::minutes(1));
        assert_eq!(config.bucket_count, 60);
        assert_eq!(config.total_duration(), Duration::hours(1));
    }

    #[test]
    fn test_window_config_per_minute_24h() {
        let config = WindowConfig::per_minute_24h();
        assert_eq!(config.bucket_duration, Duration::minutes(1));
        assert_eq!(config.bucket_count, 1440);
        assert_eq!(config.total_duration(), Duration::hours(24));
    }

    #[test]
    fn test_window_config_per_hour_7d() {
        let config = WindowConfig::per_hour_7d();
        assert_eq!(config.bucket_duration, Duration::hours(1));
        assert_eq!(config.bucket_count, 168);
        assert_eq!(config.total_duration(), Duration::days(7));
    }

    // === SlidingWindow ===

    #[test]
    fn test_sliding_window_basic() {
        let config = WindowConfig::new(Duration::seconds(10), 6);
        let mut window = SlidingWindow::new(config);

        window.record(1);
        window.record(2);
        window.record(3);

        assert_eq!(window.sum(), 6);
        assert_eq!(window.len(), 1); // All in same 10s bucket
    }

    #[test]
    fn test_sliding_window_multiple_buckets() {
        let config = WindowConfig::new(Duration::seconds(1), 10);
        let mut window = SlidingWindow::new(config);

        let now = Utc::now();
        window.record_at(1, now - Duration::seconds(3));
        window.record_at(2, now - Duration::seconds(2));
        window.record_at(3, now - Duration::seconds(1));
        window.record_at(4, now);

        assert_eq!(window.sum(), 10);
        assert!(window.len() >= 3); // May be 3 or 4 depending on alignment
    }

    #[test]
    fn test_sliding_window_latest() {
        let config = WindowConfig::new(Duration::seconds(10), 6);
        let mut window = SlidingWindow::new(config);

        assert_eq!(window.latest(), None);

        window.record(10);
        assert_eq!(window.latest(), Some(10));

        window.record(20);
        assert_eq!(window.latest(), Some(30)); // Same bucket, accumulated
    }

    #[test]
    fn test_sliding_window_is_empty() {
        let config = WindowConfig::new(Duration::seconds(10), 6);
        let mut window = SlidingWindow::new(config);

        assert!(window.is_empty());

        window.record(1);
        assert!(!window.is_empty());
    }

    #[test]
    fn test_sliding_window_clear() {
        let config = WindowConfig::new(Duration::seconds(10), 6);
        let mut window = SlidingWindow::new(config);

        window.record(1);
        window.record(2);
        assert!(!window.is_empty());

        window.clear();
        assert!(window.is_empty());
        assert_eq!(window.sum(), 0);
    }

    #[test]
    fn test_sliding_window_data_points() {
        let config = WindowConfig::new(Duration::seconds(1), 10);
        let mut window = SlidingWindow::new(config);

        let now = Utc::now();
        window.record_at(5, now - Duration::seconds(2));
        window.record_at(10, now);

        let points = window.data_points();
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].value, 5);
        assert_eq!(points[1].value, 10);
    }

    // === LabeledWindow ===

    #[test]
    fn test_labeled_window() {
        let config = WindowConfig::new(Duration::seconds(10), 6);
        let window = LabeledWindow::new(config);

        window.record("create", 1);
        window.record("create", 2);
        window.record("delete", 5);

        assert_eq!(window.sum("create"), 3);
        assert_eq!(window.sum("delete"), 5);

        let names = window.series_names();
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn test_labeled_window_sum_unknown() {
        let config = WindowConfig::new(Duration::seconds(10), 6);
        let window = LabeledWindow::new(config);

        assert_eq!(window.sum("nonexistent"), 0);
    }

    #[test]
    fn test_labeled_window_data_points_unknown() {
        let config = WindowConfig::new(Duration::seconds(10), 6);
        let window = LabeledWindow::new(config);

        let points = window.data_points("nonexistent");
        assert!(points.is_empty());
    }

    #[test]
    fn test_labeled_window_series_names_empty() {
        let config = WindowConfig::new(Duration::seconds(10), 6);
        let window = LabeledWindow::new(config);

        let names = window.series_names();
        assert!(names.is_empty());
    }
}
