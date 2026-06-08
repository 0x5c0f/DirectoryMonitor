use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use dm_core::config::{AppConfig, WatchConfig};
use dm_core::event::{EventType, FsEvent};
use dm_metrics::MetricsRegistry;
use dm_processor::EventFilter;
use dm_storage::EventStore;
use dm_web::EventPayload;
use http_body_util::BodyExt;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::{broadcast, RwLock};
use tower::{Service, ServiceExt};

// ── Test helpers ──────────────────────────────────────────────────────────────

struct TestContext {
    config: Arc<RwLock<AppConfig>>,
    config_path: PathBuf,
    _config_dir: TempDir,
    store: Option<EventStore>,
    event_tx: broadcast::Sender<EventPayload>,
    tokens: Arc<RwLock<HashSet<String>>>,
    watcher_manager: Arc<dm_watcher::WatcherManager>,
    filters: Arc<RwLock<Vec<(PathBuf, EventFilter)>>>,
    metrics: Arc<MetricsRegistry>,
}

fn test_context() -> TestContext {
    test_context_with_config(AppConfig::default())
}

fn test_context_with_config(config: AppConfig) -> TestContext {
    let config_dir = TempDir::new().expect("failed to create temp dir");
    let config_path = config_dir.path().join("config.toml");
    config.save(&config_path).expect("failed to write initial config");

    let store = EventStore::open_memory().expect("failed to open memory store");
    let (event_tx, _) = broadcast::channel(4096);
    let (watch_tx, _) = broadcast::channel(4096);
    let watcher_manager = Arc::new(dm_watcher::WatcherManager::new(watch_tx));

    TestContext {
        config: Arc::new(RwLock::new(config)),
        config_path,
        _config_dir: config_dir,
        store: Some(store),
        event_tx,
        tokens: Arc::new(RwLock::new(HashSet::new())),
        watcher_manager,
        filters: Arc::new(RwLock::new(Vec::new())),
        metrics: Arc::new(MetricsRegistry::new()),
    }
}

fn build_router(ctx: &TestContext) -> Router {
    let state = dm_web::AppState {
        config: Arc::clone(&ctx.config),
        config_path: ctx.config_path.clone(),
        store: ctx.store.clone(),
        event_tx: ctx.event_tx.clone(),
        tokens: Arc::clone(&ctx.tokens),
        watcher_manager: Arc::clone(&ctx.watcher_manager),
        filters: Arc::clone(&ctx.filters),
        metrics: Arc::clone(&ctx.metrics),
    };
    dm_web::build_router(state)
}

/// Send a request through the router.
async fn call(router: &mut Router, req: Request<Body>) -> axum::response::Response {
    ServiceExt::<Request<Body>>::ready(router)
        .await
        .unwrap()
        .call(req)
        .await
        .unwrap()
}

fn json_post(path: &str, body: serde_json::Value, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if let Some(t) = token {
        builder = builder.header("authorization", format!("Bearer {t}"));
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

fn json_put(path: &str, body: serde_json::Value, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("PUT")
        .uri(path)
        .header("content-type", "application/json");
    if let Some(t) = token {
        builder = builder.header("authorization", format!("Bearer {t}"));
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

fn get_req(path: &str, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method("GET").uri(path);
    if let Some(t) = token {
        builder = builder.header("authorization", format!("Bearer {t}"));
    }
    builder.body(Body::empty()).unwrap()
}

fn delete_req(path: &str, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method("DELETE").uri(path);
    if let Some(t) = token {
        builder = builder.header("authorization", format!("Bearer {t}"));
    }
    builder.body(Body::empty()).unwrap()
}

async fn into_status_and_json(resp: axum::response::Response) -> (StatusCode, serde_json::Value) {
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
    (status, json)
}

async fn login(router: &mut Router, password: &str) -> String {
    let req = json_post("/api/auth/login", serde_json::json!({"password": password}), None);
    let (_, json) = into_status_and_json(call(router, req).await).await;
    json["token"].as_str().unwrap().to_string()
}

async fn seed_events(store: &EventStore, count: usize) {
    for i in 0..count {
        let event = FsEvent::new(
            EventType::Created,
            PathBuf::from(format!("/file_{i}.txt")),
            PathBuf::from("/watch"),
        );
        store.insert(&event).await.unwrap();
    }
}

// ── Index ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_index_returns_html() {
    let ctx = test_context();
    let mut router = build_router(&ctx);
    let req = Request::builder().uri("/").body(Body::empty()).unwrap();
    let resp = call(&mut router, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.contains("text/html"));
}

// ── Auth ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_auth_status_no_password() {
    let ctx = test_context();
    let mut router = build_router(&ctx);
    let (status, json) = into_status_and_json(call(&mut router, get_req("/api/auth/status", None)).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["auth_required"], false);
}

#[tokio::test]
async fn test_auth_status_with_password() {
    let mut config = AppConfig::default();
    config.server.password = "secret".to_string();
    let ctx = test_context_with_config(config);
    let mut router = build_router(&ctx);
    let (status, json) = into_status_and_json(call(&mut router, get_req("/api/auth/status", None)).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["auth_required"], true);
}

#[tokio::test]
async fn test_auth_login_no_password_returns_null_token() {
    let ctx = test_context();
    let mut router = build_router(&ctx);
    let req = json_post("/api/auth/login", serde_json::json!({}), None);
    let (status, json) = into_status_and_json(call(&mut router, req).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);
    assert!(json["token"].is_null());
}

#[tokio::test]
async fn test_auth_login_correct_password() {
    let mut config = AppConfig::default();
    config.server.password = "correct".to_string();
    let ctx = test_context_with_config(config);
    let mut router = build_router(&ctx);
    let req = json_post("/api/auth/login", serde_json::json!({"password": "correct"}), None);
    let (status, json) = into_status_and_json(call(&mut router, req).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);
    assert!(json["token"].as_str().is_some());
}

#[tokio::test]
async fn test_auth_login_wrong_password() {
    let mut config = AppConfig::default();
    config.server.password = "correct".to_string();
    let ctx = test_context_with_config(config);
    let mut router = build_router(&ctx);
    let req = json_post("/api/auth/login", serde_json::json!({"password": "wrong"}), None);
    let resp = call(&mut router, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_auth_verify_valid_token() {
    let mut config = AppConfig::default();
    config.server.password = "pass".to_string();
    let ctx = test_context_with_config(config);
    let mut router = build_router(&ctx);
    let token = login(&mut router, "pass").await;
    let (status, json) = into_status_and_json(call(&mut router, get_req("/api/auth/verify", Some(&token))).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);
    assert_eq!(json["auth_required"], true);
}

#[tokio::test]
async fn test_auth_verify_invalid_token() {
    let mut config = AppConfig::default();
    config.server.password = "pass".to_string();
    let ctx = test_context_with_config(config);
    let mut router = build_router(&ctx);
    let resp = call(&mut router, get_req("/api/auth/verify", Some("fake"))).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_auth_verify_no_password_required() {
    let ctx = test_context();
    let mut router = build_router(&ctx);
    let (status, json) = into_status_and_json(call(&mut router, get_req("/api/auth/verify", None)).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["auth_required"], false);
}

// ── Auth gate on protected endpoints ──────────────────────────────────────────

#[tokio::test]
async fn test_events_requires_auth() {
    let mut config = AppConfig::default();
    config.server.password = "pass".to_string();
    let ctx = test_context_with_config(config);
    let mut router = build_router(&ctx);
    let resp = call(&mut router, get_req("/api/events", None)).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_config_requires_auth() {
    let mut config = AppConfig::default();
    config.server.password = "pass".to_string();
    let ctx = test_context_with_config(config);
    let mut router = build_router(&ctx);
    let resp = call(&mut router, get_req("/api/config", None)).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_watchers_requires_auth() {
    let mut config = AppConfig::default();
    config.server.password = "pass".to_string();
    let ctx = test_context_with_config(config);
    let mut router = build_router(&ctx);
    let resp = call(&mut router, get_req("/api/watchers", None)).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── Events ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_events_empty_store() {
    let ctx = test_context();
    let mut router = build_router(&ctx);
    let (status, json) = into_status_and_json(call(&mut router, get_req("/api/events", None)).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["total"], 0);
    assert_eq!(json["page"], 1);
    assert_eq!(json["per_page"], 50);
    assert!(json["events"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_events_pagination_defaults() {
    let ctx = test_context();
    seed_events(ctx.store.as_ref().unwrap(), 5).await;
    let mut router = build_router(&ctx);
    let (_, json) = into_status_and_json(call(&mut router, get_req("/api/events", None)).await).await;
    assert_eq!(json["total"], 5);
    assert_eq!(json["total_pages"], 1);
    assert_eq!(json["events"].as_array().unwrap().len(), 5);
}

#[tokio::test]
async fn test_events_pagination_page2() {
    let ctx = test_context();
    seed_events(ctx.store.as_ref().unwrap(), 25).await;
    let mut router = build_router(&ctx);
    let (_, json) = into_status_and_json(call(&mut router, get_req("/api/events?page=2&per_page=10", None)).await).await;
    assert_eq!(json["total"], 25);
    assert_eq!(json["page"], 2);
    assert_eq!(json["per_page"], 10);
    assert_eq!(json["total_pages"], 3);
    assert_eq!(json["events"].as_array().unwrap().len(), 10);
}

#[tokio::test]
async fn test_events_per_page_clamped_max() {
    let ctx = test_context();
    let mut router = build_router(&ctx);
    let (_, json) = into_status_and_json(call(&mut router, get_req("/api/events?per_page=999", None)).await).await;
    assert_eq!(json["per_page"], 200);
}

#[tokio::test]
async fn test_events_per_page_clamped_min() {
    let ctx = test_context();
    let mut router = build_router(&ctx);
    let (_, json) = into_status_and_json(call(&mut router, get_req("/api/events?per_page=0", None)).await).await;
    assert_eq!(json["per_page"], 1);
}

#[tokio::test]
async fn test_events_filter_by_type() {
    let ctx = test_context();
    let store = ctx.store.as_ref().unwrap();
    store.insert(&FsEvent::new(EventType::Created, PathBuf::from("/a"), PathBuf::from("/w"))).await.unwrap();
    store.insert(&FsEvent::new(EventType::Modified, PathBuf::from("/b"), PathBuf::from("/w"))).await.unwrap();
    store.insert(&FsEvent::new(EventType::Deleted, PathBuf::from("/c"), PathBuf::from("/w"))).await.unwrap();
    let mut router = build_router(&ctx);
    let (_, json) = into_status_and_json(call(&mut router, get_req("/api/events?types=CREATE,DELETE", None)).await).await;
    assert_eq!(json["total"], 2);
}

#[tokio::test]
async fn test_events_filter_by_target_type() {
    let ctx = test_context();
    let store = ctx.store.as_ref().unwrap();

    let mut dir_event = FsEvent::new(EventType::Created, PathBuf::from("/dir"), PathBuf::from("/w"));
    dir_event.is_dir = Some(true);
    let mut file_event = FsEvent::new(EventType::Created, PathBuf::from("/file"), PathBuf::from("/w"));
    file_event.is_dir = Some(false);

    store.insert(&dir_event).await.unwrap();
    store.insert(&file_event).await.unwrap();

    let mut router = build_router(&ctx);

    let (_, json) = into_status_and_json(call(&mut router, get_req("/api/events?target_type=dir", None)).await).await;
    assert_eq!(json["total"], 1);
    assert_eq!(json["events"][0]["is_dir"], true);

    let (_, json) = into_status_and_json(call(&mut router, get_req("/api/events?target_type=file", None)).await).await;
    assert_eq!(json["total"], 1);
    assert_eq!(json["events"][0]["is_dir"], false);
}

#[tokio::test]
async fn test_events_no_store_returns_empty() {
    let mut ctx = test_context();
    ctx.store = None;
    let mut router = build_router(&ctx);
    let (_, json) = into_status_and_json(call(&mut router, get_req("/api/events", None)).await).await;
    assert_eq!(json["total"], 0);
}

// ── Config GET ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_config_get_defaults() {
    let ctx = test_context();
    let mut router = build_router(&ctx);
    let (status, json) = into_status_and_json(call(&mut router, get_req("/api/config", None)).await).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["watches"].as_array().unwrap().is_empty());
    assert_eq!(json["database_enabled"], true);
    assert_eq!(json["logging_level"], "info");
}

#[tokio::test]
async fn test_config_get_masks_email_password() {
    let mut config = AppConfig::default();
    config.notifications.email.password = "real-password".to_string();
    let ctx = test_context_with_config(config);
    let mut router = build_router(&ctx);
    let (_, json) = into_status_and_json(call(&mut router, get_req("/api/config", None)).await).await;
    assert_eq!(json["email_password"], "••••••••");
}

#[tokio::test]
async fn test_config_get_empty_password_not_masked() {
    let ctx = test_context();
    let mut router = build_router(&ctx);
    let (_, json) = into_status_and_json(call(&mut router, get_req("/api/config", None)).await).await;
    assert_eq!(json["email_password"], "");
}

// ── Config PUT global ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_config_put_global_updates() {
    let ctx = test_context();
    let mut router = build_router(&ctx);
    let body = serde_json::json!({
        "logging_level": "debug",
        "database_enabled": false,
        "email_enabled": true,
        "email_smtp_server": "smtp.example.com",
    });
    let (status, json) = into_status_and_json(call(&mut router, json_put("/api/config/global", body, None)).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);
    assert_eq!(json["logging_level"], "debug");
    assert_eq!(json["database_enabled"], false);

    // Verify persisted
    let config = ctx.config.read().await;
    assert_eq!(config.logging.level, "debug");
    assert_eq!(config.notifications.email.smtp_server, "smtp.example.com");
}

// ── Config watches CRUD ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_config_add_watch() {
    let ctx = test_context();
    let mut router = build_router(&ctx);
    let body = serde_json::json!({
        "path": "/tmp/test-watch",
        "recursive": true,
        "include": ["*.rs"],
        "exclude": ["target/**"],
    });
    let (status, json) = into_status_and_json(call(&mut router, json_post("/api/config/watches", body, None)).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);
    assert_eq!(json["idx"], 0);
    assert_eq!(json["watch"]["path"], "/tmp/test-watch");

    let config = ctx.config.read().await;
    assert_eq!(config.watches.len(), 1);
}

#[tokio::test]
async fn test_config_add_watch_duplicate_409() {
    let ctx = test_context();
    let mut router = build_router(&ctx);
    let body = serde_json::json!({"path": "/tmp/dup"});
    call(&mut router, json_post("/api/config/watches", body.clone(), None)).await;
    let resp = call(&mut router, json_post("/api/config/watches", body, None)).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_config_add_watch_empty_path_400() {
    let ctx = test_context();
    let mut router = build_router(&ctx);
    let resp = call(&mut router, json_post("/api/config/watches", serde_json::json!({"path": ""}), None)).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_config_add_watch_missing_path_400() {
    let ctx = test_context();
    let mut router = build_router(&ctx);
    let resp = call(&mut router, json_post("/api/config/watches", serde_json::json!({"recursive": true}), None)).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_config_put_watch_partial_update() {
    let mut config = AppConfig::default();
    config.watches.push(WatchConfig {
        path: PathBuf::from("/tmp/watch"),
        recursive: false,
        include: vec!["*.txt".to_string()],
        exclude: vec![],
        event_types: vec![],
        log_file: None,
        log_format: None,
        script: None,
        script_mode: "async".to_string(),
        email_recipients: vec![],
    });
    let ctx = test_context_with_config(config);
    let mut router = build_router(&ctx);
    let body = serde_json::json!({"recursive": true});
    let (_, json) = into_status_and_json(call(&mut router, json_put("/api/config/watches/0", body, None)).await).await;
    assert_eq!(json["watch"]["recursive"], true);
    assert_eq!(json["watch"]["include"], serde_json::json!(["*.txt"]));
}

#[tokio::test]
async fn test_config_put_watch_out_of_bounds_404() {
    let ctx = test_context();
    let mut router = build_router(&ctx);
    let resp = call(&mut router, json_put("/api/config/watches/99", serde_json::json!({"recursive": true}), None)).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_config_delete_watch() {
    let mut config = AppConfig::default();
    config.watches.push(WatchConfig {
        path: PathBuf::from("/tmp/to-delete"),
        recursive: true,
        include: vec![],
        exclude: vec![],
        event_types: vec![],
        log_file: None,
        log_format: None,
        script: None,
        script_mode: "async".to_string(),
        email_recipients: vec![],
    });
    let ctx = test_context_with_config(config);
    let mut router = build_router(&ctx);
    let (status, json) = into_status_and_json(call(&mut router, delete_req("/api/config/watches/0", None)).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["removed"], "/tmp/to-delete");

    let config = ctx.config.read().await;
    assert!(config.watches.is_empty());
}

#[tokio::test]
async fn test_config_delete_watch_out_of_bounds_404() {
    let ctx = test_context();
    let mut router = build_router(&ctx);
    let resp = call(&mut router, delete_req("/api/config/watches/0", None)).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ── Watchers ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_watchers_list_empty() {
    let ctx = test_context();
    let mut router = build_router(&ctx);
    let (status, json) = into_status_and_json(call(&mut router, get_req("/api/watchers", None)).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);
    assert_eq!(json["count"], 0);
}

// ── Metrics ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_metrics_prometheus_endpoint() {
    let ctx = test_context();
    ctx.metrics.record_event("CREATE", "/test");
    let mut router = build_router(&ctx);
    let resp = call(&mut router, get_req("/metrics", None)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("dm_events_total"));
}

#[tokio::test]
async fn test_metrics_chart_endpoint() {
    let ctx = test_context();
    ctx.metrics.record_event("CREATE", "/test");
    let mut router = build_router(&ctx);
    let (status, json) = into_status_and_json(call(&mut router, get_req("/api/metrics/chart", None)).await).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["events_total"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn test_metrics_chart_requires_auth() {
    let mut config = AppConfig::default();
    config.server.password = "pass".to_string();
    let ctx = test_context_with_config(config);
    let mut router = build_router(&ctx);
    let resp = call(&mut router, get_req("/api/metrics/chart", None)).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── Metrics integration ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_metrics_recording_workflow() {
    let metrics = MetricsRegistry::new();
    metrics.record_event("CREATE", "/project/src");
    metrics.record_event("CREATE", "/project/src");
    metrics.record_event("MODIFY", "/project/src");
    metrics.record_event("DELETE", "/project/test");
    metrics.record_batch_flush(3);
    metrics.record_deduped(1);
    metrics.record_dropped(0);
    metrics.record_notify_sent("email");
    metrics.record_notify_failed("syslog");

    assert_eq!(metrics.events_total.get(), 7);
    assert_eq!(metrics.batches_flushed.get(), 1);
    assert_eq!(metrics.events_deduped.get(), 1);
    assert_eq!(metrics.events_dropped.get(), 0);

    let prom = metrics.prometheus();
    assert!(prom.contains("dm_events_total"));
    assert!(prom.contains("dm_batches_flushed_total"));
    assert!(prom.contains("dm_notifications_sent_total"));

    let chart = metrics.chart_json();
    assert!(chart.events_total > 0);
    assert!(chart.events_by_type.len() >= 2);
}

#[tokio::test]
async fn test_metrics_prometheus_format() {
    let metrics = MetricsRegistry::new();
    metrics.record_event("CREATE", "/test");
    metrics.active_watchers.set(3);

    let prom = metrics.prometheus();
    assert!(prom.contains("# HELP dm_events_total"));
    assert!(prom.contains("# TYPE dm_events_total counter"));
    assert!(prom.contains("dm_active_watchers 3"));
}

// ── EventStore ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_store_insert_and_query() {
    let store = EventStore::open_memory().unwrap();
    store.insert(&FsEvent::new(EventType::Created, PathBuf::from("/a"), PathBuf::from("/w"))).await.unwrap();
    store.insert(&FsEvent::new(EventType::Modified, PathBuf::from("/b"), PathBuf::from("/w"))).await.unwrap();

    let events = store.query(100, 0, &[], None, None, None, None, None).await.unwrap();
    assert_eq!(events.len(), 2);

    let events = store.query(100, 0, &["CREATE".to_string()], None, None, None, None, None).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Created);
}

#[tokio::test]
async fn test_store_pagination() {
    let store = EventStore::open_memory().unwrap();
    for i in 0..20 {
        store.insert(&FsEvent::new(EventType::Created, PathBuf::from(format!("/f_{i}")), PathBuf::from("/w"))).await.unwrap();
    }

    let events = store.query(10, 0, &[], None, None, None, None, None).await.unwrap();
    assert_eq!(events.len(), 10);
    let events = store.query(10, 10, &[], None, None, None, None, None).await.unwrap();
    assert_eq!(events.len(), 10);
    assert_eq!(store.count().await.unwrap(), 20);
}

// ── Config roundtrip ──────────────────────────────────────────────────────────

#[test]
fn test_config_toml_roundtrip() {
    let mut config = AppConfig::default();
    config.server.port = 9090;
    config.server.password = "secret".to_string();

    let toml_str = toml::to_string(&config).unwrap();
    assert!(toml_str.contains("port = 9090"));

    let config2: AppConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(config2.server.port, 9090);
    assert_eq!(config2.server.password, "secret");
}
