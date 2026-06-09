use chrono::{Duration, Utc};
use dm_core::event::{EventType, FsEvent};
use dm_storage::EventStore;
use std::path::{Path, PathBuf};

fn make_event(event_type: EventType, path: &str, watch_root: &str) -> FsEvent {
    FsEvent::new(event_type, PathBuf::from(path), PathBuf::from(watch_root))
}

fn make_event_with_time(
    event_type: EventType,
    path: &str,
    watch_root: &str,
    minutes_ago: i64,
) -> FsEvent {
    let mut event = FsEvent::new(event_type, PathBuf::from(path), PathBuf::from(watch_root));
    event.timestamp = Utc::now() - Duration::minutes(minutes_ago);
    event
}

// === Insert and Query ===

#[tokio::test]
async fn test_insert_and_query_single() {
    let store = EventStore::open_memory().unwrap();
    let event = make_event(EventType::Created, "/file.txt", "/watch");

    store.insert(&event).await.unwrap();

    let events = store
        .query(100, 0, &[], None, None, None, None, None)
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Created);
    assert_eq!(events[0].path, PathBuf::from("/file.txt"));
    assert_eq!(events[0].watch_root, PathBuf::from("/watch"));
}

#[tokio::test]
async fn test_insert_batch_and_query() {
    let store = EventStore::open_memory().unwrap();
    let events = vec![
        make_event(EventType::Created, "/a.txt", "/watch"),
        make_event(EventType::Modified, "/b.txt", "/watch"),
        make_event(EventType::Deleted, "/c.txt", "/watch"),
    ];

    store.insert_batch(&events).await.unwrap();

    let result = store
        .query(100, 0, &[], None, None, None, None, None)
        .await
        .unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(store.count().await.unwrap(), 3);
}

#[tokio::test]
async fn test_query_order_by_timestamp_desc() {
    let store = EventStore::open_memory().unwrap();

    let old = make_event_with_time(EventType::Created, "/old.txt", "/watch", 10);
    let new = make_event_with_time(EventType::Created, "/new.txt", "/watch", 0);

    store.insert(&old).await.unwrap();
    store.insert(&new).await.unwrap();

    let events = store
        .query(100, 0, &[], None, None, None, None, None)
        .await
        .unwrap();
    assert_eq!(events[0].path, PathBuf::from("/new.txt"));
    assert_eq!(events[1].path, PathBuf::from("/old.txt"));
}

// === Filtered Queries ===

#[tokio::test]
async fn test_filtered_query_by_type() {
    let store = EventStore::open_memory().unwrap();
    store
        .insert(&make_event(EventType::Created, "/a.txt", "/watch"))
        .await
        .unwrap();
    store
        .insert(&make_event(EventType::Modified, "/b.txt", "/watch"))
        .await
        .unwrap();
    store
        .insert(&make_event(EventType::Created, "/c.txt", "/watch"))
        .await
        .unwrap();

    let events = store
        .query(
            100,
            0,
            &["CREATE".to_string()],
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|e| e.event_type == EventType::Created));
}

#[tokio::test]
async fn test_filtered_query_by_watch_root() {
    let store = EventStore::open_memory().unwrap();
    store
        .insert(&make_event(EventType::Created, "/a.txt", "/dir1"))
        .await
        .unwrap();
    store
        .insert(&make_event(EventType::Created, "/b.txt", "/dir2"))
        .await
        .unwrap();
    store
        .insert(&make_event(EventType::Created, "/c.txt", "/dir1"))
        .await
        .unwrap();

    let events = store
        .query(100, 0, &[], Some("/dir1"), None, None, None, None)
        .await
        .unwrap();
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|e| e.watch_root == Path::new("/dir1")));
}

#[tokio::test]
async fn test_filtered_query_by_search() {
    let store = EventStore::open_memory().unwrap();
    store
        .insert(&make_event(
            EventType::Created,
            "/project/src/main.rs",
            "/watch",
        ))
        .await
        .unwrap();
    store
        .insert(&make_event(
            EventType::Created,
            "/project/test/main.rs",
            "/watch",
        ))
        .await
        .unwrap();
    store
        .insert(&make_event(
            EventType::Created,
            "/project/src/lib.rs",
            "/watch",
        ))
        .await
        .unwrap();

    let events = store
        .query(100, 0, &[], None, Some("src"), None, None, None)
        .await
        .unwrap();
    assert_eq!(events.len(), 2);
}

#[tokio::test]
async fn test_filtered_query_by_date_range() {
    let store = EventStore::open_memory().unwrap();

    let old = make_event_with_time(EventType::Created, "/old.txt", "/watch", 60);
    let recent = make_event_with_time(EventType::Created, "/recent.txt", "/watch", 5);
    let now = make_event_with_time(EventType::Created, "/now.txt", "/watch", 0);

    store.insert(&old).await.unwrap();
    store.insert(&recent).await.unwrap();
    store.insert(&now).await.unwrap();

    let after = (Utc::now() - Duration::minutes(10)).to_rfc3339();
    let events = store
        .query(100, 0, &[], None, None, Some(&after), None, None)
        .await
        .unwrap();
    assert_eq!(events.len(), 2);
}

#[tokio::test]
async fn test_filtered_query_combined() {
    let store = EventStore::open_memory().unwrap();
    store
        .insert(&make_event(EventType::Created, "/src/main.rs", "/project"))
        .await
        .unwrap();
    store
        .insert(&make_event(EventType::Modified, "/src/lib.rs", "/project"))
        .await
        .unwrap();
    store
        .insert(&make_event(EventType::Created, "/test/main.rs", "/project"))
        .await
        .unwrap();
    store
        .insert(&make_event(EventType::Created, "/src/main.rs", "/other"))
        .await
        .unwrap();

    // Filter by type + watch_root + search
    let events = store
        .query(
            100,
            0,
            &["CREATE".to_string()],
            Some("/project"),
            Some("src"),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].path, PathBuf::from("/src/main.rs"));
}

// === Count Filtered ===

#[tokio::test]
async fn test_count_filtered() {
    let store = EventStore::open_memory().unwrap();
    store
        .insert(&make_event(EventType::Created, "/a.txt", "/watch"))
        .await
        .unwrap();
    store
        .insert(&make_event(EventType::Modified, "/b.txt", "/watch"))
        .await
        .unwrap();
    store
        .insert(&make_event(EventType::Created, "/c.txt", "/watch"))
        .await
        .unwrap();

    let count = store
        .count_filtered(&["CREATE".to_string()], None, None, None, None, None, None)
        .await
        .unwrap();
    assert_eq!(count, 2);

    let count = store
        .count_filtered(&[], None, None, None, None, None, None)
        .await
        .unwrap();
    assert_eq!(count, 3);
}

// === Pagination ===

#[tokio::test]
async fn test_pagination() {
    let store = EventStore::open_memory().unwrap();

    // Insert 50 events
    for i in 0..50 {
        let event = make_event(EventType::Created, &format!("/file_{}.txt", i), "/watch");
        store.insert(&event).await.unwrap();
    }

    // First page
    let page1 = store
        .query(10, 0, &[], None, None, None, None, None)
        .await
        .unwrap();
    assert_eq!(page1.len(), 10);

    // Second page
    let page2 = store
        .query(10, 10, &[], None, None, None, None, None)
        .await
        .unwrap();
    assert_eq!(page2.len(), 10);

    // Last page
    let page5 = store
        .query(10, 40, &[], None, None, None, None, None)
        .await
        .unwrap();
    assert_eq!(page5.len(), 10);

    // Beyond end
    let empty = store
        .query(10, 50, &[], None, None, None, None, None)
        .await
        .unwrap();
    assert!(empty.is_empty());
}

// === Purge ===

#[tokio::test]
async fn test_purge_before() {
    let store = EventStore::open_memory().unwrap();

    let old1 = make_event_with_time(EventType::Created, "/old1.txt", "/watch", 120);
    let old2 = make_event_with_time(EventType::Created, "/old2.txt", "/watch", 90);
    let recent = make_event_with_time(EventType::Created, "/recent.txt", "/watch", 5);

    store.insert(&old1).await.unwrap();
    store.insert(&old2).await.unwrap();
    store.insert(&recent).await.unwrap();

    let cutoff = Utc::now() - Duration::minutes(60);
    let deleted = store.purge_before(cutoff).await.unwrap();
    assert_eq!(deleted, 2);
    assert_eq!(store.count().await.unwrap(), 1);

    let remaining = store
        .query(100, 0, &[], None, None, None, None, None)
        .await
        .unwrap();
    assert_eq!(remaining[0].path, PathBuf::from("/recent.txt"));
}

// === Schema ===

#[tokio::test]
async fn test_schema_initialization() {
    let store = EventStore::open_memory().unwrap();
    // Should be able to query immediately after creation
    let count = store.count().await.unwrap();
    assert_eq!(count, 0);
}

// === EventQuery struct ===

#[tokio::test]
async fn test_query_with_event_query_struct() {
    let store = EventStore::open_memory().unwrap();
    let event = make_event(EventType::Created, "/tmp/query-struct.txt", "/watch");
    store.insert(&event).await.unwrap();

    let query = dm_storage::EventQuery {
        limit: 10,
        offset: 0,
        event_types: vec!["CREATE".to_string()],
        ..dm_storage::EventQuery::default()
    };

    let events = store.query_events(query).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].path, PathBuf::from("/tmp/query-struct.txt"));
}
