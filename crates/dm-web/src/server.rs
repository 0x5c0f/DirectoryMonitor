use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::Html;
use axum::routing::{delete, get, post, put};
use axum::Router;
use dm_core::config::AppConfig;
use dm_metrics::MetricsRegistry;
use dm_processor::EventFilter;
use dm_storage::EventStore;
use dm_watcher::WatcherManager;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, RwLock};
use tracing::info;

use crate::auth::{auth_login_handler, auth_status_handler, auth_verify_handler};

/// Token time-to-live (24 hours).
pub(crate) const TOKEN_TTL_SECS: u64 = 24 * 60 * 60;
use crate::frontend::INDEX_HTML;
use crate::hub::EventPayload;
use crate::routes::{config, events, metrics, watchers};

/// Shared application state for the web server.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<AppConfig>>,
    pub config_path: PathBuf,
    pub store: Option<EventStore>,
    pub event_tx: broadcast::Sender<EventPayload>,
    /// Active tokens mapped to their creation time.
    pub tokens: Arc<RwLock<HashMap<String, Instant>>>,
    pub watcher_manager: Arc<WatcherManager>,
    pub filters: Arc<RwLock<Vec<(PathBuf, EventFilter)>>>,
    pub metrics: Arc<MetricsRegistry>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("config_path", &self.config_path)
            .field("has_store", &self.store.is_some())
            .finish_non_exhaustive()
    }
}

/// Build the axum `Router` with all routes and shared state.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/ws", get(ws_handler))
        .route("/api/events", get(events::events_handler))
        .route("/api/config", get(config::config_get_handler))
        .route(
            "/api/config/watches",
            post(config::config_add_watch_handler),
        )
        .route(
            "/api/config/watches/{idx}",
            put(config::config_put_watch_handler),
        )
        .route(
            "/api/config/watches/{idx}",
            delete(config::config_delete_watch_handler),
        )
        .route("/api/config/global", put(config::config_put_global_handler))
        .route("/api/watchers", get(watchers::watchers_list_handler))
        .route(
            "/api/watchers/reload",
            post(watchers::watchers_reload_handler),
        )
        .route("/api/auth/status", get(auth_status_handler))
        .route("/api/auth/login", post(auth_login_handler))
        .route("/api/auth/verify", get(auth_verify_handler))
        .route("/metrics", get(metrics::metrics_prometheus_handler))
        .route("/api/metrics/chart", get(metrics::metrics_chart_handler))
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
) -> std::io::Result<()> {
    let addr = format!("{}:{}", config.server.bind, config.server.port);

    let state = AppState {
        config: Arc::new(RwLock::new(config)),
        config_path,
        store,
        event_tx,
        tokens: Arc::new(RwLock::new(HashMap::new())),
        watcher_manager,
        filters,
        metrics,
    };

    // Spawn token cleanup task (removes expired tokens every 5 minutes)
    let cleanup_tokens = state.tokens.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            let mut tokens = cleanup_tokens.write().await;
            let before = tokens.len();
            tokens.retain(|_, created| created.elapsed().as_secs() < TOKEN_TTL_SECS);
            let removed = before - tokens.len();
            if removed > 0 {
                info!("Cleaned up {removed} expired tokens");
            }
        }
    });

    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!("Web server listening on http://{addr}");

    axum::serve(listener, app).await
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
        match tokens.get(&token) {
            Some(created) if created.elapsed().as_secs() < TOKEN_TTL_SECS => {}
            _ => return Err(StatusCode::UNAUTHORIZED),
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
