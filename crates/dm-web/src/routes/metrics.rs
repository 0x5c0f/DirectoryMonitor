use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};

use crate::auth::check_auth;
use crate::server::AppState;

/// GET /metrics — Prometheus text format (no auth required for scraping).
pub(crate) async fn metrics_prometheus_handler(State(state): State<AppState>) -> String {
    state.metrics.prometheus()
}

/// GET /api/metrics/chart — JSON chart data for the frontend dashboard.
pub(crate) async fn metrics_chart_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<axum::response::Json<serde_json::Value>, StatusCode> {
    check_auth(&headers, &state).await?;
    let chart = state.metrics.chart_json();
    Ok(axum::response::Json(
        serde_json::to_value(chart).unwrap_or_default(),
    ))
}
