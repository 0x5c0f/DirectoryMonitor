use dm_core::config::WatchConfig;
use dm_core::event::{EventType, FsEvent};
use dm_processor::batch::EventBatcher;
use dm_processor::dedup::EventDeduplicator;
use dm_processor::filter::EventFilter;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

fn make_config(
    event_types: Vec<String>,
    include: Vec<String>,
    exclude: Vec<String>,
) -> WatchConfig {
    WatchConfig {
        path: PathBuf::from("/test"),
        recursive: true,
        include,
        exclude,
        event_types,
        log_format: None,
        script: None,
        script_mode: "async".to_string(),
        email_recipients: vec![],
    }
}

fn make_event(event_type: EventType, path: &str) -> FsEvent {
    FsEvent::new(event_type, PathBuf::from(path), PathBuf::from("/test"))
}

// === Full Pipeline: Filter → Dedup → Batch ===

#[test]
fn test_full_pipeline_filter_dedup_batch() {
    let config = make_config(vec!["create".to_string()], vec![], vec![]);
    let filter = EventFilter::from_config(&config).unwrap();
    let mut dedup = EventDeduplicator::new(Duration::from_secs(5));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut batcher = EventBatcher::new(3, Duration::from_secs(10), tx);

    // Mix of events: some should be filtered, some deduped
    let events = vec![
        make_event(EventType::Created, "/a.txt"), // passes filter, new
        make_event(EventType::Modified, "/b.txt"), // filtered out (not CREATE)
        make_event(EventType::Created, "/a.txt"), // passes filter, duplicate
        make_event(EventType::Created, "/c.txt"), // passes filter, new
        make_event(EventType::Created, "/d.txt"), // passes filter, new → triggers batch flush
    ];

    // Step 1: Filter
    let filtered = filter.filter_events(events);
    assert_eq!(filtered.len(), 4); // 4 CREATED events

    // Step 2: Dedup
    let deduped = dedup.process_batch(filtered);
    assert_eq!(deduped.len(), 3); // a.txt dup removed

    // Step 3: Batch (threshold=3, so auto-flushes)
    for event in deduped {
        batcher.push(event);
    }

    let batch = rx.try_recv().unwrap();
    assert_eq!(batch.len(), 3);
    assert!(batch.iter().all(|e| e.event_type == EventType::Created));
}

// === Filter + Dedup Interaction ===

#[test]
fn test_filter_dedup_interaction() {
    let config = make_config(vec!["create".to_string()], vec![], vec![]);
    let filter = EventFilter::from_config(&config).unwrap();
    let mut dedup = EventDeduplicator::new(Duration::from_secs(5));

    // Same file, different event types
    let events = vec![
        make_event(EventType::Created, "/file.txt"),
        make_event(EventType::Modified, "/file.txt"), // filtered by type
        make_event(EventType::Created, "/file.txt"),  // duplicate
    ];

    let filtered = filter.filter_events(events);
    assert_eq!(filtered.len(), 2); // both CREATED pass filter

    let deduped = dedup.process_batch(filtered);
    assert_eq!(deduped.len(), 1); // second CREATED is duplicate
}

// === Filter + Exclude ===

#[test]
fn test_pipeline_with_exclude() {
    let config = make_config(vec![], vec![], vec!["**/.git/**".to_string()]);
    let filter = EventFilter::from_config(&config).unwrap();
    let mut dedup = EventDeduplicator::new(Duration::from_secs(5));

    let events = vec![
        make_event(EventType::Created, "/project/src/main.rs"),
        make_event(EventType::Modified, "/project/.git/config"),
        make_event(EventType::Created, "/project/.git/objects/abc"),
        make_event(EventType::Deleted, "/project/src/lib.rs"),
    ];

    let filtered = filter.filter_events(events);
    assert_eq!(filtered.len(), 2); // .git events excluded

    let deduped = dedup.process_batch(filtered);
    assert_eq!(deduped.len(), 2); // all unique
}

// === Batch Auto-Flush ===

#[test]
fn test_batch_auto_flush_at_threshold() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut batcher = EventBatcher::new(5, Duration::from_secs(10), tx);

    for i in 0..4 {
        batcher.push(make_event(EventType::Created, &format!("/file_{}.txt", i)));
    }
    assert!(rx.try_recv().is_err()); // not flushed yet
    assert_eq!(batcher.len(), 4);

    batcher.push(make_event(EventType::Created, "/file_4.txt"));
    let batch = rx.try_recv().unwrap();
    assert_eq!(batch.len(), 5);
    assert_eq!(batcher.len(), 0);
}

// === Batch Manual Flush ===

#[test]
fn test_batch_manual_flush() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut batcher = EventBatcher::new(10, Duration::from_secs(10), tx);

    batcher.push(make_event(EventType::Created, "/a.txt"));
    batcher.push(make_event(EventType::Created, "/b.txt"));

    batcher.flush();
    let batch = rx.try_recv().unwrap();
    assert_eq!(batch.len(), 2);
}

// === Dedup Different Types Same Path ===

#[test]
fn test_dedup_different_types_same_path_not_duplicates() {
    let mut dedup = EventDeduplicator::new(Duration::from_secs(5));

    let events = vec![
        make_event(EventType::Created, "/file.txt"),
        make_event(EventType::Modified, "/file.txt"),
        make_event(EventType::Deleted, "/file.txt"),
    ];

    let result = dedup.process_batch(events);
    assert_eq!(result.len(), 3); // all different types, no duplicates
}

// === Empty Pipeline ===

#[test]
fn test_pipeline_empty_input() {
    let config = make_config(vec![], vec![], vec![]);
    let filter = EventFilter::from_config(&config).unwrap();
    let mut dedup = EventDeduplicator::new(Duration::from_secs(5));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut batcher = EventBatcher::new(5, Duration::from_secs(10), tx);

    let events: Vec<FsEvent> = vec![];
    let filtered = filter.filter_events(events);
    let deduped = dedup.process_batch(filtered);

    for event in deduped {
        batcher.push(event);
    }

    batcher.flush();
    assert!(rx.try_recv().is_err()); // no batch sent
}
