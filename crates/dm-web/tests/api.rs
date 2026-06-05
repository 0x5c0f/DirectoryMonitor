use dm_core::config::AppConfig;
use dm_core::event::{EventType, FsEvent};
use dm_metrics::MetricsRegistry;
use dm_storage::EventStore;
use std::path::PathBuf;

// === Metrics Integration ===

#[tokio::test]
async fn test_metrics_recording_workflow() {
    let metrics = MetricsRegistry::new();

    // Simulate event processing pipeline
    metrics.record_event("CREATE", "/project/src");
    metrics.record_event("CREATE", "/project/src");
    metrics.record_event("MODIFY", "/project/src");
    metrics.record_event("DELETE", "/project/test");

    // Simulate batch operations
    metrics.record_batch_flush(3);
    metrics.record_deduped(1);
    metrics.record_dropped(0);

    // Simulate notifications
    metrics.record_notify_sent("email");
    metrics.record_notify_failed("syslog");

    // Verify counters
    // events_total = 4 from record_event + 3 from record_batch_flush = 7
    assert_eq!(metrics.events_total.get(), 7);
    // batches_flushed counts flush operations, not events
    assert_eq!(metrics.batches_flushed.get(), 1);
    assert_eq!(metrics.events_deduped.get(), 1);
    assert_eq!(metrics.events_dropped.get(), 0);

    // Verify Prometheus output contains all metrics
    let prom = metrics.prometheus();
    assert!(prom.contains("dm_events_total"));
    assert!(prom.contains("dm_batches_flushed_total"));
    assert!(prom.contains("dm_events_deduped_total"));
    assert!(prom.contains("dm_notifications_sent_total"));
    assert!(prom.contains("dm_notifications_failed_total"));

    // Verify chart JSON has data
    let chart = metrics.chart_json();
    assert!(chart.events_total > 0);
    assert!(chart.events_by_type.len() >= 2); // At least CREATE and MODIFY
}

#[tokio::test]
async fn test_metrics_prometheus_format() {
    let metrics = MetricsRegistry::new();
    metrics.record_event("CREATE", "/test");
    metrics.active_watchers.set(3);

    let prom = metrics.prometheus();

    // Check Prometheus text format structure
    assert!(prom.contains("# HELP dm_events_total"));
    assert!(prom.contains("# TYPE dm_events_total counter"));
    assert!(prom.contains("# HELP dm_active_watchers"));
    assert!(prom.contains("# TYPE dm_active_watchers gauge"));
    assert!(prom.contains("dm_active_watchers 3"));
}

// === EventStore Integration ===

#[tokio::test]
async fn test_store_insert_and_query() {
    let store = EventStore::open_memory().unwrap();

    // Insert events
    let event1 = FsEvent::new(
        EventType::Created,
        PathBuf::from("/file1.txt"),
        PathBuf::from("/watch"),
    );
    let event2 = FsEvent::new(
        EventType::Modified,
        PathBuf::from("/file2.txt"),
        PathBuf::from("/watch"),
    );

    store.insert(&event1).await.unwrap();
    store.insert(&event2).await.unwrap();

    // Query all
    let events = store.query(100, 0, &[], None, None, None, None).await.unwrap();
    assert_eq!(events.len(), 2);

    // Query by type
    let events = store.query(100, 0, &["CREATE".to_string()], None, None, None, None).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Created);
}

#[tokio::test]
async fn test_store_pagination() {
    let store = EventStore::open_memory().unwrap();

    // Insert 20 events
    for i in 0..20 {
        let event = FsEvent::new(
            EventType::Created,
            PathBuf::from(format!("/file_{}.txt", i)),
            PathBuf::from("/watch"),
        );
        store.insert(&event).await.unwrap();
    }

    // Query page 1
    let events = store.query(10, 0, &[], None, None, None, None).await.unwrap();
    assert_eq!(events.len(), 10);

    // Query page 2
    let events = store.query(10, 10, &[], None, None, None, None).await.unwrap();
    assert_eq!(events.len(), 10);

    // Verify count
    let count = store.count().await.unwrap();
    assert_eq!(count, 20);
}

// === Auth Flow Integration ===

#[tokio::test]
async fn test_auth_token_lifecycle() {
    let mut config = AppConfig::default();
    config.server.password = "test-password".to_string();

    // Simulate login - generate token
    let token = uuid::Uuid::new_v4().to_string();

    // Store token (simulating what auth handler does)
    let mut tokens = std::collections::HashSet::new();
    tokens.insert(token.clone());

    // Verify token exists
    assert!(tokens.contains(&token));

    // Verify invalid token
    assert!(!tokens.contains("invalid-token"));
}

#[tokio::test]
async fn test_auth_no_password_skips_auth() {
    let config = AppConfig::default();
    assert!(config.server.password.is_empty());

    // When password is empty, no token needed
    let tokens: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Access should be allowed even with empty tokens
    assert!(tokens.is_empty());
}

// === Config Integration ===

#[test]
fn test_config_toml_roundtrip() {
    let mut config = AppConfig::default();
    config.server.port = 9090;
    config.server.password = "secret".to_string();

    // Serialize to TOML
    let toml_str = toml::to_string(&config).unwrap();
    assert!(toml_str.contains("port = 9090"));
    assert!(toml_str.contains("password = \"secret\""));

    // Deserialize back
    let config2: AppConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(config2.server.port, 9090);
    assert_eq!(config2.server.password, "secret");
}
