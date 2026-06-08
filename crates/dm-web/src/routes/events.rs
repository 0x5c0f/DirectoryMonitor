use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};

use crate::auth::check_auth;
use crate::server::{AppState, EventPayload};

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
pub(crate) async fn events_handler(
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
