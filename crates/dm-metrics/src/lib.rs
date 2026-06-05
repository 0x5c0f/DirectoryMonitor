//! # dm-metrics
//!
//! Performance metrics collection and Prometheus export for Directory Monitor.
//!
//! This crate provides:
//! - Atomic counters for real-time metrics
//! - Sliding windows for time-series data
//! - Prometheus text format output
//! - JSON export for frontend charts

pub mod counter;
pub mod prometheus;
pub mod window;

use chrono::{DateTime, Utc};
use counter::{AtomicCounter, LabeledCounter};
use prometheus::PrometheusFormatter;
use serde::Serialize;
use std::sync::Arc;
use window::{DataPoint, LabeledWindow, SlidingWindow, WindowConfig};

/// Central registry for all metrics.
///
/// This is the main entry point for recording and querying metrics.
/// It should be created once and shared across the application via `Arc`.
pub struct MetricsRegistry {
    // === Event counters ===
    /// Total events by type
    pub events_by_type: LabeledCounter,
    /// Total events by watch root
    pub events_by_root: LabeledCounter,
    /// Total events by type AND root (combined labels)
    pub events_by_type_root: LabeledCounter,
    /// Total events processed (all)
    pub events_total: AtomicCounter,

    // === Pipeline counters ===
    /// Total batches flushed
    pub batches_flushed: AtomicCounter,
    /// Total events dropped by filter
    pub events_dropped: AtomicCounter,
    /// Total events deduplicated
    pub events_deduped: AtomicCounter,

    // === Notification counters ===
    /// Notifications sent by type
    pub notify_sent: LabeledCounter,
    /// Notifications failed by type
    pub notify_failed: LabeledCounter,

    // === Time-series windows ===
    /// Event rate per minute (last 1 hour)
    pub event_rate_1h: Arc<parking_lot::RwLock<SlidingWindow>>,
    /// Event rate per hour (last 7 days)
    pub event_rate_7d: Arc<parking_lot::RwLock<SlidingWindow>>,
    /// Events by type over time (last 1 hour)
    pub events_by_type_1h: LabeledWindow,

    // === System gauges ===
    /// Process start time
    pub start_time: DateTime<Utc>,
    /// Current active watchers
    pub active_watchers: AtomicCounter,
    /// Database size in bytes
    pub db_size_bytes: AtomicCounter,
    /// Current event queue depth
    pub queue_depth: AtomicCounter,
}

impl MetricsRegistry {
    /// Create a new metrics registry.
    pub fn new() -> Self {
        Self {
            // Event counters
            events_by_type: LabeledCounter::new("dm_events_total"),
            events_by_root: LabeledCounter::new("dm_events_by_root_total"),
            events_by_type_root: LabeledCounter::new("dm_events_by_type_root"),
            events_total: AtomicCounter::new(),

            // Pipeline counters
            batches_flushed: AtomicCounter::new(),
            events_dropped: AtomicCounter::new(),
            events_deduped: AtomicCounter::new(),

            // Notification counters
            notify_sent: LabeledCounter::new("dm_notifications_sent_total"),
            notify_failed: LabeledCounter::new("dm_notifications_failed_total"),

            // Time-series windows
            event_rate_1h: Arc::new(parking_lot::RwLock::new(SlidingWindow::new(
                WindowConfig::per_minute_1h(),
            ))),
            event_rate_7d: Arc::new(parking_lot::RwLock::new(SlidingWindow::new(
                WindowConfig::per_hour_7d(),
            ))),
            events_by_type_1h: LabeledWindow::new(WindowConfig::per_minute_1h()),

            // System gauges
            start_time: Utc::now(),
            active_watchers: AtomicCounter::new(),
            db_size_bytes: AtomicCounter::new(),
            queue_depth: AtomicCounter::new(),
        }
    }

    /// Record a filesystem event.
    pub fn record_event(&self, event_type: &str, watch_root: &str) {
        self.events_total.inc();
        self.events_by_type.inc(&[("type", event_type)]);
        self.events_by_root.inc(&[("root", watch_root)]);
        self.events_by_type_root.inc(&[("type", event_type), ("root", watch_root)]);

        // Update time-series
        self.event_rate_1h.write().record(1);
        self.event_rate_7d.write().record(1);
        self.events_by_type_1h.record(event_type, 1);
    }

    /// Record a batch flush.
    pub fn record_batch_flush(&self, count: i64) {
        self.batches_flushed.inc();
        self.events_total.add(count);
    }

    /// Record dropped events.
    pub fn record_dropped(&self, count: i64) {
        self.events_dropped.add(count);
    }

    /// Record deduped events.
    pub fn record_deduped(&self, count: i64) {
        self.events_deduped.add(count);
    }

    /// Record a notification sent.
    pub fn record_notify_sent(&self, notify_type: &str) {
        self.notify_sent.inc(&[("type", notify_type)]);
    }

    /// Record a notification failure.
    pub fn record_notify_failed(&self, notify_type: &str) {
        self.notify_failed.inc(&[("type", notify_type)]);
    }

    /// Get uptime in seconds.
    pub fn uptime_seconds(&self) -> i64 {
        (Utc::now() - self.start_time).num_seconds()
    }

    /// Export all metrics in Prometheus text format.
    pub fn prometheus(&self) -> String {
        let mut output = String::new();

        // Event counters
        output.push_str(&PrometheusFormatter::format_counter(
            &self.events_by_type,
            "Total filesystem events by type",
        ));
        output.push('\n');

        output.push_str(&PrometheusFormatter::format_counter(
            &self.events_by_root,
            "Total filesystem events by watch root",
        ));
        output.push('\n');

        output.push_str(&PrometheusFormatter::format_counter(
            &self.events_by_type_root,
            "Total filesystem events by type and root",
        ));
        output.push('\n');

        // Pipeline counters
        output.push_str(&PrometheusFormatter::format_gauge(
            "dm_batches_flushed_total",
            "Total number of batch flushes",
            self.batches_flushed.get(),
        ));
        output.push('\n');

        output.push_str(&PrometheusFormatter::format_gauge(
            "dm_events_dropped_total",
            "Total events dropped by filter",
            self.events_dropped.get(),
        ));
        output.push('\n');

        output.push_str(&PrometheusFormatter::format_gauge(
            "dm_events_deduped_total",
            "Total events deduplicated",
            self.events_deduped.get(),
        ));
        output.push('\n');

        // Notification counters
        output.push_str(&PrometheusFormatter::format_counter(
            &self.notify_sent,
            "Notifications sent by type",
        ));
        output.push('\n');

        output.push_str(&PrometheusFormatter::format_counter(
            &self.notify_failed,
            "Notifications failed by type",
        ));
        output.push('\n');

        // System gauges
        output.push_str(&PrometheusFormatter::format_gauge(
            "dm_active_watchers",
            "Number of active file system watchers",
            self.active_watchers.get(),
        ));
        output.push('\n');

        output.push_str(&PrometheusFormatter::format_gauge(
            "dm_uptime_seconds",
            "Process uptime in seconds",
            self.uptime_seconds(),
        ));
        output.push('\n');

        output.push_str(&PrometheusFormatter::format_gauge(
            "dm_db_size_bytes",
            "Database file size in bytes",
            self.db_size_bytes.get(),
        ));
        output.push('\n');

        output.push_str(&PrometheusFormatter::format_gauge(
            "dm_event_queue_depth",
            "Current event processing queue depth",
            self.queue_depth.get(),
        ));
        output.push('\n');

        // Time-series summary
        output.push_str(&PrometheusFormatter::format_window_summary(
            "dm_event_rate_1h",
            "Event rate per minute over the last hour",
            &self.events_by_type_1h,
        ));

        output
    }

    /// Export chart data as JSON for the frontend.
    pub fn chart_json(&self) -> ChartData {
        ChartData {
            events_total: self.events_total.get(),
            events_by_type: self
                .events_by_type
                .snapshot()
                .into_iter()
                .map(|(labels, value)| TypeCount {
                    event_type: labels
                        .iter()
                        .find(|(k, _)| k == "type")
                        .map(|(_, v)| v.clone())
                        .unwrap_or_default(),
                    count: value,
                })
                .collect(),
            events_by_root: self
                .events_by_root
                .snapshot()
                .into_iter()
                .map(|(labels, value)| TypeCount {
                    event_type: labels
                        .iter()
                        .find(|(k, _)| k == "root")
                        .map(|(_, v)| v.clone())
                        .unwrap_or_default(),
                    count: value,
                })
                .collect(),
            events_by_type_root: self
                .events_by_type_root
                .snapshot()
                .into_iter()
                .map(|(labels, value)| {
                    let event_type = labels
                        .iter()
                        .find(|(k, _)| k == "type")
                        .map(|(_, v)| v.clone())
                        .unwrap_or_default();
                    let root = labels
                        .iter()
                        .find(|(k, _)| k == "root")
                        .map(|(_, v)| v.clone())
                        .unwrap_or_default();
                    TypeRootCount {
                        event_type,
                        root,
                        count: value,
                    }
                })
                .collect(),
            event_rate_1h: {
                let window = self.event_rate_1h.read();
                window.data_points()
            },
            event_rate_7d: {
                let window = self.event_rate_7d.read();
                window.data_points()
            },
            events_by_type_1h: self
                .events_by_type_1h
                .series_names()
                .into_iter()
                .map(|name| SeriesData {
                    name: name.clone(),
                    points: self.events_by_type_1h.data_points(&name),
                })
                .collect(),
            system: SystemInfo {
                uptime_seconds: self.uptime_seconds(),
                active_watchers: self.active_watchers.get(),
                db_size_bytes: self.db_size_bytes.get(),
                queue_depth: self.queue_depth.get(),
                batches_flushed: self.batches_flushed.get(),
                events_dropped: self.events_dropped.get(),
                events_deduped: self.events_deduped.get(),
            },
            notifications: NotificationStats {
                sent: self
                    .notify_sent
                    .snapshot()
                    .into_iter()
                    .map(|(labels, value)| TypeCount {
                        event_type: labels
                            .iter()
                            .find(|(k, _)| k == "type")
                            .map(|(_, v)| v.clone())
                            .unwrap_or_default(),
                        count: value,
                    })
                    .collect(),
                failed: self
                    .notify_failed
                    .snapshot()
                    .into_iter()
                    .map(|(labels, value)| TypeCount {
                        event_type: labels
                            .iter()
                            .find(|(k, _)| k == "type")
                            .map(|(_, v)| v.clone())
                            .unwrap_or_default(),
                        count: value,
                    })
                    .collect(),
            },
        }
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Chart data structure for JSON export.
#[derive(Debug, Serialize)]
pub struct ChartData {
    pub events_total: i64,
    pub events_by_type: Vec<TypeCount>,
    pub events_by_root: Vec<TypeCount>,
    pub events_by_type_root: Vec<TypeRootCount>,
    pub event_rate_1h: Vec<DataPoint>,
    pub event_rate_7d: Vec<DataPoint>,
    pub events_by_type_1h: Vec<SeriesData>,
    pub system: SystemInfo,
    pub notifications: NotificationStats,
}

#[derive(Debug, Serialize)]
pub struct TypeCount {
    pub event_type: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct TypeRootCount {
    pub event_type: String,
    pub root: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct SeriesData {
    pub name: String,
    pub points: Vec<DataPoint>,
}

#[derive(Debug, Serialize)]
pub struct SystemInfo {
    pub uptime_seconds: i64,
    pub active_watchers: i64,
    pub db_size_bytes: i64,
    pub queue_depth: i64,
    pub batches_flushed: i64,
    pub events_dropped: i64,
    pub events_deduped: i64,
}

#[derive(Debug, Serialize)]
pub struct NotificationStats {
    pub sent: Vec<TypeCount>,
    pub failed: Vec<TypeCount>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_registry() {
        let registry = MetricsRegistry::new();

        // Record some events
        registry.record_event("CREATE", "/home/user/docs");
        registry.record_event("CREATE", "/home/user/docs");
        registry.record_event("DELETE", "/home/user/docs");
        registry.record_event("MODIFY", "/tmp/watch");

        // Check counters
        assert_eq!(registry.events_total.get(), 4);
        assert_eq!(
            registry.events_by_type.with_labels(&[("type", "CREATE")]).get(),
            2
        );

        // Check Prometheus output
        let prom = registry.prometheus();
        assert!(prom.contains("dm_events_total"));
        assert!(prom.contains("dm_active_watchers"));

        // Check chart JSON
        let chart = registry.chart_json();
        assert_eq!(chart.events_total, 4);
        assert_eq!(chart.events_by_type.len(), 3); // CREATE, DELETE, MODIFY
    }

    #[test]
    fn test_metrics_registry_record_batch_flush() {
        let registry = MetricsRegistry::new();
        assert_eq!(registry.batches_flushed.get(), 0);

        registry.record_batch_flush(10);
        registry.record_batch_flush(5);
        // batches_flushed counts flushes, not events
        assert_eq!(registry.batches_flushed.get(), 2);
        // events_total accumulates the count
        assert_eq!(registry.events_total.get(), 15);
    }

    #[test]
    fn test_metrics_registry_record_dropped() {
        let registry = MetricsRegistry::new();
        assert_eq!(registry.events_dropped.get(), 0);

        registry.record_dropped(3);
        assert_eq!(registry.events_dropped.get(), 3);
    }

    #[test]
    fn test_metrics_registry_record_deduped() {
        let registry = MetricsRegistry::new();
        assert_eq!(registry.events_deduped.get(), 0);

        registry.record_deduped(2);
        registry.record_deduped(1);
        assert_eq!(registry.events_deduped.get(), 3);
    }

    #[test]
    fn test_metrics_registry_notify_counters() {
        let registry = MetricsRegistry::new();

        registry.record_notify_sent("email");
        registry.record_notify_sent("email");
        registry.record_notify_sent("syslog");
        registry.record_notify_failed("email");

        assert_eq!(
            registry.notify_sent.with_labels(&[("type", "email")]).get(),
            2
        );
        assert_eq!(
            registry.notify_sent.with_labels(&[("type", "syslog")]).get(),
            1
        );
        assert_eq!(
            registry.notify_failed.with_labels(&[("type", "email")]).get(),
            1
        );
    }

    #[test]
    fn test_metrics_registry_active_watchers() {
        let registry = MetricsRegistry::new();
        assert_eq!(registry.active_watchers.get(), 0);

        registry.active_watchers.set(5);
        assert_eq!(registry.active_watchers.get(), 5);
    }

    #[test]
    fn test_metrics_registry_uptime() {
        let registry = MetricsRegistry::new();
        let uptime = registry.uptime_seconds();
        assert!(uptime >= 0);
    }

    #[test]
    fn test_metrics_registry_prometheus_format() {
        let registry = MetricsRegistry::new();
        registry.record_event("CREATE", "/test");
        registry.active_watchers.set(3);

        let prom = registry.prometheus();
        // Check Prometheus text format structure
        assert!(prom.contains("# HELP dm_events_total"));
        assert!(prom.contains("# TYPE dm_events_total counter"));
        assert!(prom.contains("# HELP dm_active_watchers"));
        assert!(prom.contains("# TYPE dm_active_watchers gauge"));
    }

    #[test]
    fn test_metrics_registry_chart_json_structure() {
        let registry = MetricsRegistry::new();
        registry.record_event("CREATE", "/dir1");
        registry.record_event("MODIFY", "/dir1");
        registry.record_event("CREATE", "/dir2");

        let chart = registry.chart_json();
        assert_eq!(chart.events_total, 3);
        assert_eq!(chart.events_by_type.len(), 2); // CREATE, MODIFY
        assert_eq!(chart.events_by_root.len(), 2); // /dir1, /dir2
        assert_eq!(chart.events_by_type_root.len(), 3); // CREATE/dir1, MODIFY/dir1, CREATE/dir2
    }

    #[test]
    fn test_metrics_registry_zero_counts() {
        let registry = MetricsRegistry::new();
        let chart = registry.chart_json();

        assert_eq!(chart.events_total, 0);
        assert!(chart.events_by_type.is_empty());
        assert!(chart.events_by_root.is_empty());
    }
}
