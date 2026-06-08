use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use dm_core::config::AppConfig;
use dm_processor::EventFilter;
use std::path::PathBuf;
use tracing::{error, info};

use crate::auth::check_auth;
use crate::server::AppState;

/// GET /api/watchers — list all active watchers.
pub(crate) async fn watchers_list_handler(
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
pub(crate) async fn watchers_reload_handler(
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
