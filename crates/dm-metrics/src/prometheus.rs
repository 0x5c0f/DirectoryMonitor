use crate::counter::LabeledCounter;
use crate::window::LabeledWindow;

/// Format metrics in Prometheus text exposition format.
///
/// See: https://prometheus.io/docs/instrumenting/exposition_formats/
pub struct PrometheusFormatter;

impl PrometheusFormatter {
    /// Format a labeled counter as Prometheus metrics.
    pub fn format_counter(counter: &LabeledCounter, help: &str) -> String {
        let mut output = String::new();

        // HELP line
        output.push_str(&format!("# HELP {} {}\n", counter.name(), help));

        // TYPE line
        output.push_str(&format!("# TYPE {} counter\n", counter.name()));

        // Values
        let snapshot = counter.snapshot();
        if snapshot.is_empty() {
            // Emit a zero value with no labels
            output.push_str(&format!("{} 0\n", counter.name()));
        } else {
            for (labels, value) in snapshot {
                if labels.is_empty() {
                    output.push_str(&format!("{} {}\n", counter.name(), value));
                } else {
                    let label_str = labels
                        .iter()
                        .map(|(k, v)| format!("{}=\"{}\"", k, escape_label_value(v)))
                        .collect::<Vec<_>>()
                        .join(",");
                    output.push_str(&format!("{{{}}} {}\n", label_str, value));
                    // Need to prepend the metric name
                    output = output.replace(
                        &format!("{{{}}}", label_str),
                        &format!("{}{{{}}}", counter.name(), label_str),
                    );
                }
            }
        }

        output
    }

    /// Format a gauge metric.
    pub fn format_gauge(name: &str, help: &str, value: i64) -> String {
        format!(
            "# HELP {} {}\n# TYPE {} gauge\n{} {}\n",
            name, help, name, name, value
        )
    }

    /// Format a gauge with labels.
    pub fn format_gauge_with_labels(
        name: &str,
        help: &str,
        labels: &[(&str, &str)],
        value: i64,
    ) -> String {
        let label_str = if labels.is_empty() {
            String::new()
        } else {
            let s = labels
                .iter()
                .map(|(k, v)| format!("{}=\"{}\"", k, escape_label_value(v)))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{}}}", s)
        };
        format!(
            "# HELP {} {}\n# TYPE {} gauge\n{}{} {}\n",
            name, help, name, name, label_str, value
        )
    }

    /// Format a window as a summary (showing recent values).
    pub fn format_window_summary(name: &str, help: &str, window: &LabeledWindow) -> String {
        let mut output = String::new();

        output.push_str(&format!("# HELP {} {}\n", name, help));
        output.push_str(&format!("# TYPE {} gauge\n", name));

        for series_name in window.series_names() {
            let sum = window.sum(&series_name);
            output.push_str(&format!(
                "{}{{series=\"{}\"}} {}\n",
                name,
                escape_label_value(&series_name),
                sum
            ));
        }

        output
    }
}

/// Escape special characters in label values.
fn escape_label_value(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::counter::LabeledCounter;
    use crate::window::{LabeledWindow, WindowConfig};
    use chrono::Duration;

    #[test]
    fn test_format_counter() {
        let counter = LabeledCounter::new("dm_events_total");
        counter.inc(&[("type", "create")]);
        counter.inc(&[("type", "delete")]);

        let output = PrometheusFormatter::format_counter(&counter, "Total events");
        assert!(output.contains("# HELP dm_events_total Total events"));
        assert!(output.contains("# TYPE dm_events_total counter"));
        assert!(output.contains("dm_events_total{type=\"create\"} 1"));
        assert!(output.contains("dm_events_total{type=\"delete\"} 1"));
    }

    #[test]
    fn test_format_counter_empty() {
        let counter = LabeledCounter::new("dm_events_total");
        let output = PrometheusFormatter::format_counter(&counter, "Total events");
        assert!(output.contains("# HELP dm_events_total Total events"));
        assert!(output.contains("# TYPE dm_events_total counter"));
        assert!(output.contains("dm_events_total 0"));
    }

    #[test]
    fn test_format_gauge() {
        let output = PrometheusFormatter::format_gauge("dm_uptime_seconds", "Uptime", 3600);
        assert!(output.contains("# HELP dm_uptime_seconds Uptime"));
        assert!(output.contains("# TYPE dm_uptime_seconds gauge"));
        assert!(output.contains("dm_uptime_seconds 3600"));
    }

    #[test]
    fn test_format_gauge_with_labels() {
        let output = PrometheusFormatter::format_gauge_with_labels(
            "dm_db_size_bytes",
            "Database size",
            &[("path", "/data/events.db")],
            1024,
        );
        assert!(output.contains("# HELP dm_db_size_bytes Database size"));
        assert!(output.contains("# TYPE dm_db_size_bytes gauge"));
        assert!(output.contains("dm_db_size_bytes{path=\"/data/events.db\"} 1024"));
    }

    #[test]
    fn test_format_gauge_with_multiple_labels() {
        let output = PrometheusFormatter::format_gauge_with_labels(
            "dm_test",
            "Test",
            &[("a", "1"), ("b", "2")],
            42,
        );
        assert!(output.contains("a=\"1\""));
        assert!(output.contains("b=\"2\""));
        assert!(output.contains(" 42"));
    }

    #[test]
    fn test_format_gauge_with_empty_labels() {
        let output = PrometheusFormatter::format_gauge_with_labels("dm_test", "Test", &[], 42);
        assert!(output.contains("dm_test 42"));
        assert!(!output.contains("{}"));
    }

    #[test]
    fn test_format_window_summary() {
        let config = WindowConfig::new(Duration::seconds(10), 6);
        let window = LabeledWindow::new(config);

        window.record("create", 5);
        window.record("delete", 3);

        let output =
            PrometheusFormatter::format_window_summary("dm_event_rate", "Event rate", &window);
        assert!(output.contains("# HELP dm_event_rate Event rate"));
        assert!(output.contains("# TYPE dm_event_rate gauge"));
        assert!(output.contains("dm_event_rate{series=\"create\"} 5"));
        assert!(output.contains("dm_event_rate{series=\"delete\"} 3"));
    }

    #[test]
    fn test_escape_label_value_empty() {
        assert_eq!(escape_label_value(""), "");
    }

    #[test]
    fn test_escape_label_value_backslash() {
        assert_eq!(escape_label_value("path\\to"), "path\\\\to");
    }

    #[test]
    fn test_escape_label_value_all_special() {
        assert_eq!(escape_label_value("\"\\\n"), "\\\"\\\\\\n");
    }
}
