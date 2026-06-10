use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use chrono::{Duration, Utc};
use serde_json::json;

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

    let mut chart = serde_json::to_value(state.metrics.chart_json()).unwrap_or_default();

    // Override trend data with DB queries when store is available
    if let Some(ref store) = state.store {
        let now = Utc::now();

        // 1h: per-minute buckets (60s)
        if let Ok(rows) = store.time_series(now - Duration::hours(1), 60).await {
            chart["event_rate_1h"] = json!(
                rows.iter()
                    .map(|(ts, cnt)| json!({"timestamp": ts, "value": cnt}))
                    .collect::<Vec<_>>()
            );
        }

        // 7d: per-hour buckets (3600s)
        if let Ok(rows) = store.time_series(now - Duration::days(7), 3600).await {
            chart["event_rate_7d"] = json!(
                rows.iter()
                    .map(|(ts, cnt)| json!({"timestamp": ts, "value": cnt}))
                    .collect::<Vec<_>>()
            );
        }
    }

    Ok(axum::response::Json(chart))
}
