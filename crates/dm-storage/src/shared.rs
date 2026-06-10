use chrono::{DateTime, Utc};
use dm_core::event::{EventType, FsEvent};
use rusqlite::Row;
use std::path::PathBuf;
use std::str::FromStr;

/// Map a database row to an `FsEvent`.
///
/// Expected column order: id, timestamp, event_type, path, target_path,
/// is_dir, user_name, process_name, watch_root.
pub(crate) fn row_to_event(row: &Row<'_>) -> rusqlite::Result<FsEvent> {
    let id_str: String = row.get(0)?;
    let timestamp_str: String = row.get(1)?;
    let event_type_str: String = row.get(2)?;
    let path_str: String = row.get(3)?;
    let target_path_str: Option<String> = row.get(4)?;
    let is_dir: Option<bool> = row.get(5)?;
    let user: Option<String> = row.get(6)?;
    let process: Option<String> = row.get(7)?;
    let watch_root_str: String = row.get(8)?;

    let id = id_str.parse::<uuid::Uuid>().map_err(|e| {
        rusqlite::Error::InvalidParameterName(format!("Invalid UUID '{}': {}", id_str, e))
    })?;
    let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::InvalidParameterName(format!(
                "Invalid timestamp '{}': {}",
                timestamp_str, e
            ))
        })?;
    let event_type = EventType::from_str(&event_type_str)
        .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;

    Ok(FsEvent {
        id,
        timestamp,
        event_type,
        path: PathBuf::from(path_str),
        target_path: target_path_str.map(PathBuf::from),
        is_dir,
        user,
        process,
        watch_root: PathBuf::from(watch_root_str),
    })
}

/// Build a WHERE clause and parameter list from filter options.
pub(crate) fn build_where_clause(
    event_types: &[String],
    watch_root: Option<&str>,
    search: Option<&str>,
    after: Option<&str>,
    before: Option<&str>,
    is_dir: Option<bool>,
) -> (String, Vec<Box<dyn rusqlite::types::ToSql>>) {
    let mut clause = String::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if !event_types.is_empty() {
        let placeholders: Vec<&str> = event_types.iter().map(|_| "?").collect();
        clause.push_str(&format!(" AND event_type IN ({})", placeholders.join(",")));
        for et in event_types {
            param_values.push(Box::new(et.clone()));
        }
    }
    if let Some(root) = watch_root {
        clause.push_str(" AND watch_root = ?");
        param_values.push(Box::new(root.to_string()));
    }
    if let Some(s) = search {
        if !s.is_empty() {
            clause.push_str(" AND (path LIKE ? OR target_path LIKE ?)");
            let pattern = format!("%{}%", s);
            param_values.push(Box::new(pattern.clone()));
            param_values.push(Box::new(pattern));
        }
    }
    if let Some(after_ts) = after {
        clause.push_str(" AND timestamp >= ?");
        param_values.push(Box::new(after_ts.to_string()));
    }
    if let Some(before_ts) = before {
        clause.push_str(" AND timestamp <= ?");
        param_values.push(Box::new(before_ts.to_string()));
    }
    if let Some(dir) = is_dir {
        clause.push_str(" AND is_dir = ?");
        param_values.push(Box::new(dir));
    }

    (clause, param_values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_where_clause_empty() {
        let (clause, params) = build_where_clause(&[], None, None, None, None, None);
        assert!(clause.is_empty());
        assert!(params.is_empty());
    }

    #[test]
    fn test_build_where_clause_event_types() {
        let types = vec!["CREATE".to_string(), "MODIFY".to_string()];
        let (clause, params) = build_where_clause(&types, None, None, None, None, None);
        assert!(clause.contains("event_type IN"));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_build_where_clause_all_filters() {
        let types = vec!["CREATE".to_string()];
        let (clause, params) = build_where_clause(
            &types,
            Some("/watch"),
            Some("test"),
            Some("2025-01-01"),
            Some("2025-12-31"),
            Some(true),
        );
        assert!(clause.contains("event_type IN"));
        assert!(clause.contains("watch_root = ?"));
        assert!(clause.contains("path LIKE ?"));
        assert!(clause.contains("timestamp >= ?"));
        assert!(clause.contains("timestamp <= ?"));
        assert!(clause.contains("is_dir = ?"));
        assert_eq!(params.len(), 7); // 1 type + 1 root + 2 search + 1 after + 1 before + 1 is_dir
    }
}
