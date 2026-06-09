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
/// - node_id: filter by specific node ID (cluster mode only)
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
    let node_filter = params
        .get("node_id")
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty());
    let offset = (page - 1) * per_page;

    // Use ClusterQueryAggregator if available (cluster mode)
    if let Some(ref aggregator) = state.cluster_aggregator {
        let query = dm_storage::EventQuery {
            limit: per_page + offset, // Fetch enough for pagination
            offset: 0,                // Aggregator handles offset internally
            event_types: event_types.clone(),
            watch_root: None,
            search: search.map(|s| s.to_string()),
            after: after.map(|s| s.to_string()),
            before: before.map(|s| s.to_string()),
            is_dir,
            node_id: node_filter.map(|s| s.to_string()),
        };

        match aggregator.query_all(&query, node_filter).await {
            Ok(cluster_events) => {
                let total = cluster_events.len();
                // Apply pagination to aggregated results
                let paginated: Vec<_> = cluster_events
                    .into_iter()
                    .skip(offset)
                    .take(per_page)
                    .collect();

                let events: Vec<serde_json::Value> = paginated
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "id": e.id,
                            "timestamp": e.timestamp,
                            "event_type": e.event_type,
                            "path": e.path,
                            "target_path": e.old_path,
                            "is_dir": e.is_directory,
                            "watch_root": "",
                            "node_id": e.node_id,
                            "node_name": e.node_name,
                        })
                    })
                    .collect();

                let total_pages = if total == 0 { 1 } else { total.div_ceil(per_page) };

                return Ok(axum::response::Json(serde_json::json!({
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
                        "node_id": node_filter,
                    }
                })));
            }
            Err(e) => {
                tracing::error!("Cluster query failed, falling back to local: {e}");
                // Fall through to local query
            }
        }
    }

    // If cluster aggregator is available but query failed, try local store
    if state.cluster_aggregator.is_some() && state.store.is_some() {
        tracing::info!("Cluster query failed, falling back to local store");
    }

    // Local-only query (standalone mode or cluster query failed)
    let (events, total) = if let Some(ref store) = state.store {
        let total = store
            .count_filtered(&event_types, None, search, after, before, is_dir, node_filter)
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
                node_filter,
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
