use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use std::path::PathBuf;
use tracing::{error, info};

use crate::auth::check_auth;
use crate::server::AppState;

/// GET /api/config — return current configuration.
pub(crate) async fn config_get_handler(
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
pub(crate) async fn config_put_watch_handler(
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
pub(crate) async fn config_add_watch_handler(
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
        log_format: None,
        script: None,
        script_mode: "async".to_string(),
        script_events: Vec::new(),
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
pub(crate) async fn config_delete_watch_handler(
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
pub(crate) async fn config_put_global_handler(
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
