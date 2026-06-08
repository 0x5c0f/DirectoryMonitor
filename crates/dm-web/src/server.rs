use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Html;
use axum::routing::{delete, get, post, put};
use axum::Router;
use dm_core::config::AppConfig;
use dm_metrics::MetricsRegistry;
use dm_processor::EventFilter;
use dm_storage::EventStore;
use dm_watcher::WatcherManager;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info};

use crate::frontend::INDEX_HTML;
pub use crate::hub::EventPayload;

/// Shared application state for the web server.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<AppConfig>>,
    pub config_path: PathBuf,
    pub store: Option<EventStore>,
    pub event_tx: broadcast::Sender<EventPayload>,
    pub tokens: Arc<RwLock<HashSet<String>>>,
    pub watcher_manager: Arc<WatcherManager>,
    pub filters: Arc<RwLock<Vec<(PathBuf, EventFilter)>>>,
    pub metrics: Arc<MetricsRegistry>,
}

/// Build the axum `Router` with all routes and shared state.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/ws", get(ws_handler))
        .route("/api/events", get(events_handler))
        .route("/api/config", get(config_get_handler))
        .route("/api/config/watches", post(config_add_watch_handler))
        .route("/api/config/watches/{idx}", put(config_put_watch_handler))
        .route(
            "/api/config/watches/{idx}",
            delete(config_delete_watch_handler),
        )
        .route("/api/config/global", put(config_put_global_handler))
        .route("/api/watchers", get(watchers_list_handler))
        .route("/api/watchers/reload", post(watchers_reload_handler))
        .route("/api/auth/status", get(auth_status_handler))
        .route("/api/auth/login", post(auth_login_handler))
        .route("/api/auth/verify", get(auth_verify_handler))
        .route("/metrics", get(metrics_prometheus_handler))
        .route("/api/metrics/chart", get(metrics_chart_handler))
        .with_state(state)
}

/// Run the axum web server.
pub async fn run_server(
    config: AppConfig,
    config_path: PathBuf,
    store: Option<EventStore>,
    event_tx: broadcast::Sender<EventPayload>,
    watcher_manager: Arc<WatcherManager>,
    filters: Arc<RwLock<Vec<(PathBuf, EventFilter)>>>,
    metrics: Arc<MetricsRegistry>,
) -> Result<(), String> {
    let addr = format!("{}:{}", config.server.bind, config.server.port);

    let state = AppState {
        config: Arc::new(RwLock::new(config)),
        config_path,
        store,
        event_tx,
        tokens: Arc::new(RwLock::new(HashSet::new())),
        watcher_manager,
        filters,
        metrics,
    };

    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("Failed to bind {addr}: {e}"))?;

    info!("Web server listening on http://{addr}");

    axum::serve(listener, app)
        .await
        .map_err(|e| format!("Server error: {e}"))
}

// ── Auth ──────────────────────────────────────────────────────────────────────

/// Check if auth is required and token is valid.
/// Returns Ok(()) if access is allowed, Err(status) otherwise.
async fn check_auth(headers: &HeaderMap, state: &AppState) -> Result<(), StatusCode> {
    let password = state.config.read().await.server.password.clone();
    if password.is_empty() {
        return Ok(()); // No auth configured
    }

    let token = extract_token(headers);
    match token {
        Some(t) => {
            let tokens = state.tokens.read().await;
            if tokens.contains(&t) {
                Ok(())
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        None => Err(StatusCode::UNAUTHORIZED),
    }
}

/// Extract bearer token from Authorization header.
fn extract_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// GET /api/auth/status — check if authentication is required (no auth needed).
async fn auth_status_handler(
    State(state): State<AppState>,
) -> axum::response::Json<serde_json::Value> {
    let password = state.config.read().await.server.password.clone();
    axum::response::Json(serde_json::json!({
        "auth_required": !password.is_empty()
    }))
}

/// POST /api/auth/login — authenticate with password.
async fn auth_login_handler(
    State(state): State<AppState>,
    axum::extract::Json(body): axum::extract::Json<serde_json::Value>,
) -> Result<axum::response::Json<serde_json::Value>, StatusCode> {
    let password = state.config.read().await.server.password.clone();

    if password.is_empty() {
        return Ok(axum::response::Json(serde_json::json!({
            "ok": true,
            "token": null,
            "message": "No password configured"
        })));
    }

    let provided = body.get("password").and_then(|v| v.as_str()).unwrap_or("");

    // Constant-time comparison to prevent timing attacks
    if constant_time_eq(provided.as_bytes(), password.as_bytes()) {
        let token = uuid::Uuid::new_v4().to_string();
        state.tokens.write().await.insert(token.clone());
        info!("Auth: login successful");
        Ok(axum::response::Json(serde_json::json!({
            "ok": true,
            "token": token
        })))
    } else {
        info!("Auth: login failed");
        Err(StatusCode::FORBIDDEN)
    }
}

/// Constant-time byte comparison to prevent timing attacks.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

/// GET /api/auth/verify — check if token is still valid.
async fn auth_verify_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<axum::response::Json<serde_json::Value>, StatusCode> {
    let password = state.config.read().await.server.password.clone();

    if password.is_empty() {
        return Ok(axum::response::Json(serde_json::json!({
            "ok": true,
            "auth_required": false
        })));
    }

    match extract_token(&headers) {
        Some(t) => {
            let tokens = state.tokens.read().await;
            if tokens.contains(&t) {
                Ok(axum::response::Json(serde_json::json!({
                    "ok": true,
                    "auth_required": true
                })))
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        None => Err(StatusCode::UNAUTHORIZED),
    }
}

// ── Metrics ───────────────────────────────────────────────────────────────────

/// GET /metrics — Prometheus text format (no auth required for scraping).
async fn metrics_prometheus_handler(State(state): State<AppState>) -> String {
    state.metrics.prometheus()
}

/// GET /api/metrics/chart — JSON chart data for the frontend dashboard.
async fn metrics_chart_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<axum::response::Json<serde_json::Value>, StatusCode> {
    check_auth(&headers, &state).await?;
    let chart = state.metrics.chart_json();
    Ok(axum::response::Json(
        serde_json::to_value(chart).unwrap_or_default(),
    ))
}

// ── Pages ─────────────────────────────────────────────────────────────────────

/// Serve the embedded HTML frontend.
async fn index_handler() -> Html<String> {
    Html(INDEX_HTML.clone())
}

// ── WebSocket ─────────────────────────────────────────────────────────────────

/// WebSocket handler: streams real-time events to the client.
async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<std::collections::HashMap<String, String>>,
    State(state): State<AppState>,
) -> Result<axum::response::Response, StatusCode> {
    // Check auth for WebSocket via query param
    let password = state.config.read().await.server.password.clone();
    if !password.is_empty() {
        let token = params.get("token").cloned().unwrap_or_default();
        let tokens = state.tokens.read().await;
        if !tokens.contains(&token) {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    Ok(ws.on_upgrade(move |socket| handle_ws(socket, state)))
}

/// Handle a single WebSocket connection.
async fn handle_ws(mut socket: WebSocket, state: AppState) {
    let mut rx = state.event_tx.subscribe();

    info!("WebSocket client connected");

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(payload) => {
                        if let Ok(json) = serde_json::to_string(&payload) {
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        info!("WebSocket client lagged, skipped {n} events");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
        }
    }

    info!("WebSocket client disconnected");
}

// ── REST API ──────────────────────────────────────────────────────────────────

/// GET /api/events — return paginated events from the database.
///
/// Query parameters:
/// - page: page number (default 1)
/// - per_page: items per page (default 50, max 200)
/// - search: search in path and target_path
/// - types: comma-separated event types
/// - after: ISO 8601 timestamp (inclusive start)
/// - before: ISO 8601 timestamp (inclusive end)
/// - target_type: "file" or "dir" to filter by file type
async fn events_handler(
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
    State(state): State<AppState>,
) -> Result<axum::response::Json<serde_json::Value>, StatusCode> {
    check_auth(&headers, &state).await?;

    let page: usize = params
        .get("page")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
        .max(1);
    let per_page: usize = params
        .get("per_page")
        .and_then(|v| v.parse().ok())
        .unwrap_or(50)
        .clamp(1, 200);
    let search = params
        .get("search")
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty());
    let event_types: Vec<String> = params
        .get("types")
        .map(|t| {
            t.split(',')
                .map(|s| s.trim().to_uppercase())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let after = params
        .get("after")
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty());
    let before = params
        .get("before")
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty());
    let is_dir = params
        .get("target_type")
        .and_then(|v| match v.to_lowercase().as_str() {
            "dir" | "directory" => Some(true),
            "file" => Some(false),
            _ => None,
        });
    let offset = (page - 1) * per_page;

    let (events, total) = if let Some(ref store) = state.store {
        let total = store
            .count_filtered(&event_types, None, search, after, before, is_dir)
            .await
            .unwrap_or(0);
        let evts = store
            .query(
                per_page,
                offset,
                &event_types,
                None,
                search,
                after,
                before,
                is_dir,
            )
            .await
            .unwrap_or_default();
        let events: Vec<serde_json::Value> = evts
            .iter()
            .map(|e| {
                let p = EventPayload::from(e);
                serde_json::to_value(&p).unwrap_or_default()
            })
            .collect();
        (events, total)
    } else {
        (vec![], 0)
    };

    let total_pages = if total == 0 {
        1
    } else {
        total.div_ceil(per_page)
    };

    Ok(axum::response::Json(serde_json::json!({
        "events": events,
        "total": total,
        "page": page,
        "per_page": per_page,
        "total_pages": total_pages,
        "filters": {
            "search": search,
            "types": event_types,
            "after": after,
            "before": before,
            "target_type": params.get("target_type"),
        }
    })))
}

/// GET /api/config — return current configuration.
async fn config_get_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<axum::response::Json<serde_json::Value>, StatusCode> {
    check_auth(&headers, &state).await?;

    let config = state.config.read().await;
    let watches: Vec<serde_json::Value> = config
        .watches
        .iter()
        .map(|w| {
            serde_json::json!({
                "path": w.path.display().to_string(),
                "recursive": w.recursive,
                "include": w.include,
                "exclude": w.exclude,
                "event_types": w.event_types,
            })
        })
        .collect();

    // Check if config differs from active watchers (pending reload)
    let active_watchers = state.watcher_manager.list_watchers().await;
    let pending_reload = if active_watchers.len() != config.watches.len() {
        true
    } else {
        let mut differs = false;
        for cw in &config.watches {
            let path_str = cw.path.display().to_string();
            match active_watchers.iter().find(|aw| aw.path == path_str) {
                Some(aw) => {
                    if aw.recursive != cw.recursive
                        || aw.include != cw.include
                        || aw.exclude != cw.exclude
                        || aw.event_types != cw.event_types
                    {
                        differs = true;
                        break;
                    }
                }
                None => {
                    differs = true;
                    break;
                }
            }
        }
        differs
    };

    Ok(axum::response::Json(serde_json::json!({
        "watches": watches,
        "pending_reload": pending_reload,
        "database_enabled": config.database.enabled,
        "database_path": config.database.path.display().to_string(),
        "logging_level": config.logging.level,
        "email_enabled": config.notifications.email.enabled,
        "email_smtp_server": config.notifications.email.smtp_server,
        "email_smtp_port": config.notifications.email.smtp_port,
        "email_username": config.notifications.email.username,
        "email_password": if config.notifications.email.password.is_empty() { "" } else { "••••••••" },
        "email_batch_size": config.notifications.email.batch_size,
        "email_max_per_minute": config.notifications.email.max_per_minute,
        "syslog_enabled": config.notifications.syslog.enabled,
        "syslog_server": config.notifications.syslog.server,
        "syslog_port": config.notifications.syslog.port,
        "syslog_format": config.notifications.syslog.format,
    })))
}

/// PUT /api/config/watches/:idx — update a watch configuration.
async fn config_put_watch_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(idx): Path<usize>,
    axum::extract::Json(body): axum::extract::Json<serde_json::Value>,
) -> Result<axum::response::Json<serde_json::Value>, StatusCode> {
    check_auth(&headers, &state).await?;

    let mut config = state.config.write().await;

    if idx >= config.watches.len() {
        return Err(StatusCode::NOT_FOUND);
    }

    // Update fields if provided
    if let Some(v) = body.get("recursive").and_then(|v| v.as_bool()) {
        config.watches[idx].recursive = v;
    }
    if let Some(v) = body.get("include").and_then(|v| v.as_array()) {
        config.watches[idx].include = v
            .iter()
            .filter_map(|s| s.as_str().map(String::from))
            .collect();
    }
    if let Some(v) = body.get("exclude").and_then(|v| v.as_array()) {
        config.watches[idx].exclude = v
            .iter()
            .filter_map(|s| s.as_str().map(String::from))
            .collect();
    }
    if let Some(v) = body.get("event_types").and_then(|v| v.as_array()) {
        config.watches[idx].event_types = v
            .iter()
            .filter_map(|s| s.as_str().map(String::from))
            .collect();
    }

    let watch_path = config.watches[idx].path.display().to_string();
    let watch_recursive = config.watches[idx].recursive;
    let watch_include = config.watches[idx].include.clone();
    let watch_exclude = config.watches[idx].exclude.clone();
    let watch_event_types = config.watches[idx].event_types.clone();

    // Save config to file
    if let Err(e) = config.save(&state.config_path) {
        error!("Failed to save config: {e}");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    info!("Config updated: watch[{}] {}", idx, watch_path);

    Ok(axum::response::Json(serde_json::json!({
        "ok": true,
        "watch": {
            "path": watch_path,
            "recursive": watch_recursive,
            "include": watch_include,
            "exclude": watch_exclude,
            "event_types": watch_event_types,
        }
    })))
}

/// POST /api/config/watches — add a new watch configuration.
async fn config_add_watch_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    axum::extract::Json(body): axum::extract::Json<serde_json::Value>,
) -> Result<axum::response::Json<serde_json::Value>, StatusCode> {
    check_auth(&headers, &state).await?;

    let path = body
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    if path.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut config = state.config.write().await;

    // Check if path already exists
    if config
        .watches
        .iter()
        .any(|w| w.path.display().to_string() == path)
    {
        return Err(StatusCode::CONFLICT);
    }

    let new_watch = dm_core::config::WatchConfig {
        path: PathBuf::from(path),
        recursive: body
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        include: body
            .get("include")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        exclude: body
            .get("exclude")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        event_types: body
            .get("event_types")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        log_file: None,
        log_format: None,
        script: None,
        script_mode: "async".to_string(),
        email_recipients: Vec::new(),
    };

    // Add watch to config (not yet active until reload)
    config.watches.push(new_watch);

    // Save config to file
    if let Err(e) = config.save(&state.config_path) {
        error!("Failed to save config: {e}");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let idx = config.watches.len() - 1;
    info!("Config updated: added watch[{}] {}", idx, path);

    Ok(axum::response::Json(serde_json::json!({
        "ok": true,
        "idx": idx,
        "watch": {
            "path": path,
            "recursive": config.watches[idx].recursive,
            "include": config.watches[idx].include,
            "exclude": config.watches[idx].exclude,
            "event_types": config.watches[idx].event_types,
        }
    })))
}

/// DELETE /api/config/watches/:idx — delete a watch configuration.
async fn config_delete_watch_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(idx): Path<usize>,
) -> Result<axum::response::Json<serde_json::Value>, StatusCode> {
    check_auth(&headers, &state).await?;

    let mut config = state.config.write().await;

    if idx >= config.watches.len() {
        return Err(StatusCode::NOT_FOUND);
    }

    let removed_path = config.watches[idx].path.clone();
    let removed_path_str = removed_path.display().to_string();

    // Remove from config (not yet removed from active watchers until reload)
    config.watches.remove(idx);

    // Save config to file
    if let Err(e) = config.save(&state.config_path) {
        error!("Failed to save config: {e}");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    info!(
        "Config updated: removed watch[{}] {}",
        idx, removed_path_str
    );

    Ok(axum::response::Json(serde_json::json!({
        "ok": true,
        "removed": removed_path_str
    })))
}

/// PUT /api/config/global — update global settings.
async fn config_put_global_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    axum::extract::Json(body): axum::extract::Json<serde_json::Value>,
) -> Result<axum::response::Json<serde_json::Value>, StatusCode> {
    check_auth(&headers, &state).await?;

    let mut config = state.config.write().await;

    // Update logging level
    if let Some(v) = body.get("logging_level").and_then(|v| v.as_str()) {
        config.logging.level = v.to_string();
    }

    // Update database settings
    if let Some(v) = body.get("database_enabled").and_then(|v| v.as_bool()) {
        config.database.enabled = v;
    }
    if let Some(v) = body.get("database_path").and_then(|v| v.as_str()) {
        config.database.path = std::path::PathBuf::from(v);
    }

    // Update email settings
    if let Some(v) = body.get("email_enabled").and_then(|v| v.as_bool()) {
        config.notifications.email.enabled = v;
    }
    if let Some(v) = body.get("email_smtp_server").and_then(|v| v.as_str()) {
        config.notifications.email.smtp_server = v.to_string();
    }
    if let Some(v) = body.get("email_smtp_port").and_then(|v| v.as_u64()) {
        config.notifications.email.smtp_port = v as u16;
    }
    if let Some(v) = body.get("email_username").and_then(|v| v.as_str()) {
        config.notifications.email.username = v.to_string();
    }
    if let Some(v) = body.get("email_password").and_then(|v| v.as_str()) {
        config.notifications.email.password = v.to_string();
    }
    if let Some(v) = body.get("email_batch_size").and_then(|v| v.as_u64()) {
        config.notifications.email.batch_size = v as usize;
    }
    if let Some(v) = body.get("email_max_per_minute").and_then(|v| v.as_u64()) {
        config.notifications.email.max_per_minute = v as u32;
    }

    // Update syslog settings
    if let Some(v) = body.get("syslog_enabled").and_then(|v| v.as_bool()) {
        config.notifications.syslog.enabled = v;
    }
    if let Some(v) = body.get("syslog_server").and_then(|v| v.as_str()) {
        config.notifications.syslog.server = v.to_string();
    }
    if let Some(v) = body.get("syslog_port").and_then(|v| v.as_u64()) {
        config.notifications.syslog.port = v as u16;
    }
    if let Some(v) = body.get("syslog_format").and_then(|v| v.as_str()) {
        config.notifications.syslog.format = v.to_string();
    }

    // Save config to file
    if let Err(e) = config.save(&state.config_path) {
        error!("Failed to save config: {e}");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    info!("Global config updated");

    Ok(axum::response::Json(serde_json::json!({
        "ok": true,
        "logging_level": config.logging.level,
        "database_enabled": config.database.enabled,
        "database_path": config.database.path,
        "email_enabled": config.notifications.email.enabled,
        "syslog_enabled": config.notifications.syslog.enabled,
    })))
}

/// GET /api/watchers — list all active watchers.
async fn watchers_list_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<axum::response::Json<serde_json::Value>, StatusCode> {
    check_auth(&headers, &state).await?;

    let watchers = state.watcher_manager.list_watchers().await;

    Ok(axum::response::Json(serde_json::json!({
        "ok": true,
        "watchers": watchers,
        "count": watchers.len(),
    })))
}

/// POST /api/watchers/reload — reload watchers from config file.
async fn watchers_reload_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<axum::response::Json<serde_json::Value>, StatusCode> {
    check_auth(&headers, &state).await?;

    // Re-read config from file
    let new_config = match AppConfig::load(&state.config_path) {
        Ok(config) => config,
        Err(e) => {
            error!("Failed to reload config: {e}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Reload watchers
    match state.watcher_manager.reload(&new_config).await {
        Ok(result) => {
            // Update stored config
            *state.config.write().await = new_config.clone();

            // Update filters
            let new_filters: Vec<(PathBuf, EventFilter)> = new_config
                .watches
                .iter()
                .filter_map(|w| match EventFilter::from_config(w) {
                    Ok(f) => Some((w.path.clone(), f)),
                    Err(e) => {
                        error!("Failed to create filter for {}: {e}", w.path.display());
                        None
                    }
                })
                .collect();
            *state.filters.write().await = new_filters;

            // Update metrics
            let net_change = result.added.len() as i64 - result.removed.len() as i64;
            if net_change != 0 {
                state.metrics.active_watchers.add(net_change);
            }

            info!(
                "Reloaded watchers: added={}, removed={}, kept={}, active={}",
                result.added.len(),
                result.removed.len(),
                result.kept.len(),
                state.metrics.active_watchers.get()
            );

            Ok(axum::response::Json(serde_json::json!({
                "ok": true,
                "added": result.added.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                "removed": result.removed.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                "kept": result.kept.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                "active_watchers": state.metrics.active_watchers.get(),
            })))
        }
        Err(e) => {
            error!("Failed to reload watchers: {e}");
            Ok(axum::response::Json(serde_json::json!({
                "ok": false,
                "error": e,
            })))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_extract_token_valid() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer my-secret-token-123"),
        );

        let token = extract_token(&headers);
        assert_eq!(token, Some("my-secret-token-123".to_string()));
    }

    #[test]
    fn test_extract_token_missing() {
        let headers = HeaderMap::new();
        let token = extract_token(&headers);
        assert_eq!(token, None);
    }

    #[test]
    fn test_extract_token_invalid_format() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Basic dXNlcjpwYXNz"),
        );

        let token = extract_token(&headers);
        assert_eq!(token, None);
    }

    #[test]
    fn test_extract_token_empty_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer "));

        let token = extract_token(&headers);
        assert_eq!(token, Some("".to_string()));
    }

    #[test]
    fn test_extract_token_invalid_header_value() {
        let mut headers = HeaderMap::new();
        // Non-ASCII bytes are invalid header values
        headers.insert(
            "authorization",
            HeaderValue::from_bytes(&[0xFF, 0xFE]).unwrap(),
        );

        let token = extract_token(&headers);
        assert_eq!(token, None);
    }

    #[test]
    fn test_constant_time_eq_identical() {
        assert!(constant_time_eq(b"hello", b"hello"));
    }

    #[test]
    fn test_constant_time_eq_different() {
        assert!(!constant_time_eq(b"hello", b"world"));
    }

    #[test]
    fn test_constant_time_eq_different_length() {
        assert!(!constant_time_eq(b"hello", b"hi"));
    }

    #[test]
    fn test_constant_time_eq_empty() {
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn test_constant_time_eq_single_byte_diff() {
        // Ensure every bit position is checked
        assert!(!constant_time_eq(b"\x00", b"\x01"));
        assert!(!constant_time_eq(b"\x00", b"\x80"));
    }
}
