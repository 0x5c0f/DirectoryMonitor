use crate::schema;
use crate::StorageError;
use chrono::{DateTime, Utc};
use dm_core::event::{EventType, FsEvent};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};

/// SQLite-backed event store with WAL mode.
#[derive(Clone)]
pub struct EventStore {
    conn: Arc<Mutex<Connection>>,
}

impl EventStore {
    /// Open or create the database at the given path.
    pub fn open(db_path: &Path) -> Result<Self, StorageError> {
        let conn = Connection::open(db_path)?;

        // Enable WAL mode for better concurrent performance
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "cache_size", -64000)?; // 64MB cache

        schema::initialize(&conn).map_err(StorageError::MigrationFailed)?;

        info!("Event store opened: {}", db_path.display());
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Open an in-memory database (for testing).
    pub fn open_memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        schema::initialize(&conn).map_err(StorageError::MigrationFailed)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Store a single event.
    pub async fn insert(&self, event: &FsEvent) -> Result<(), StorageError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO events (id, timestamp, event_type, path, target_path, user_name, process_name, watch_root)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                event.id.to_string(),
                event.timestamp.to_rfc3339(),
                event.event_type.to_string(),
                event.path.to_string_lossy().to_string(),
                event.target_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                event.user,
                event.process,
                event.watch_root.to_string_lossy().to_string(),
            ],
        )?;
        debug!("Stored event: {} {}", event.event_type, event.path.display());
        Ok(())
    }

    /// Store multiple events in a transaction.
    pub async fn insert_batch(&self, events: &[FsEvent]) -> Result<(), StorageError> {
        let conn = self.conn.lock().await;
        let tx = conn.unchecked_transaction()?;
        for event in events {
            tx.execute(
                "INSERT INTO events (id, timestamp, event_type, path, target_path, user_name, process_name, watch_root)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    event.id.to_string(),
                    event.timestamp.to_rfc3339(),
                    event.event_type.to_string(),
                    event.path.to_string_lossy().to_string(),
                    event.target_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                    event.user,
                    event.process,
                    event.watch_root.to_string_lossy().to_string(),
                ],
            )?;
        }
        tx.commit()?;
        info!("Stored {} events", events.len());
        Ok(())
    }

    /// Query events with optional filters.
    pub async fn query(
        &self,
        limit: usize,
        offset: usize,
        event_types: &[String],
        watch_root: Option<&str>,
        search: Option<&str>,
    ) -> Result<Vec<FsEvent>, StorageError> {
        let conn = self.conn.lock().await;
        let mut sql = String::from(
            "SELECT id, timestamp, event_type, path, target_path, user_name, process_name, watch_root
             FROM events WHERE 1=1",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if !event_types.is_empty() {
            let placeholders: Vec<&str> = event_types.iter().map(|_| "?").collect();
            sql.push_str(&format!(" AND event_type IN ({})", placeholders.join(",")));
            for et in event_types {
                param_values.push(Box::new(et.clone()));
            }
        }
        if let Some(root) = watch_root {
            sql.push_str(" AND watch_root = ?");
            param_values.push(Box::new(root.to_string()));
        }
        if let Some(s) = search {
            if !s.is_empty() {
                sql.push_str(" AND (path LIKE ? OR target_path LIKE ?)");
                let pattern = format!("%{}%", s);
                param_values.push(Box::new(pattern.clone()));
                param_values.push(Box::new(pattern));
            }
        }

        sql.push_str(" ORDER BY timestamp DESC LIMIT ? OFFSET ?");
        param_values.push(Box::new(limit as i64));
        param_values.push(Box::new(offset as i64));

        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_ref.as_slice(), |row| {
            let id_str: String = row.get(0)?;
            let timestamp_str: String = row.get(1)?;
            let event_type_str: String = row.get(2)?;
            let path_str: String = row.get(3)?;
            let target_path_str: Option<String> = row.get(4)?;
            let user: Option<String> = row.get(5)?;
            let process: Option<String> = row.get(6)?;
            let watch_root_str: String = row.get(7)?;

            Ok(FsEvent {
                id: id_str.parse().unwrap_or_default(),
                timestamp: DateTime::parse_from_rfc3339(&timestamp_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                event_type: match event_type_str.as_str() {
                    "CREATE" => EventType::Created,
                    "MODIFY" => EventType::Modified,
                    "ATTRIB" => EventType::Attrib,
                    "CLOSE_WRITE" => EventType::CloseWrite,
                    "CLOSE_NOWRITE" => EventType::CloseNoWrite,
                    "OPEN" => EventType::Opened,
                    "MOVED_TO" => EventType::MovedTo,
                    "MOVED_FROM" => EventType::MovedFrom,
                    "DELETE" => EventType::Deleted,
                    "RENAME" => EventType::Renamed,
                    "ACCESS" => EventType::Accessed,
                    // Legacy fallbacks
                    "Created" => EventType::Created,
                    "Modified" => EventType::Modified,
                    "Deleted" => EventType::Deleted,
                    "Renamed" => EventType::Renamed,
                    "Accessed" => EventType::Accessed,
                    _ => EventType::Modified,
                },
                path: PathBuf::from(path_str),
                target_path: target_path_str.map(PathBuf::from),
                user,
                process,
                watch_root: PathBuf::from(watch_root_str),
            })
        })?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    /// Get the total number of stored events.
    pub async fn count(&self) -> Result<usize, StorageError> {
        let conn = self.conn.lock().await;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Get the count of events matching filters.
    pub async fn count_filtered(
        &self,
        event_types: &[String],
        watch_root: Option<&str>,
        search: Option<&str>,
    ) -> Result<usize, StorageError> {
        let conn = self.conn.lock().await;
        let mut sql = String::from("SELECT COUNT(*) FROM events WHERE 1=1");
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if !event_types.is_empty() {
            let placeholders: Vec<&str> = event_types.iter().map(|_| "?").collect();
            sql.push_str(&format!(" AND event_type IN ({})", placeholders.join(",")));
            for et in event_types {
                param_values.push(Box::new(et.clone()));
            }
        }
        if let Some(root) = watch_root {
            sql.push_str(" AND watch_root = ?");
            param_values.push(Box::new(root.to_string()));
        }
        if let Some(s) = search {
            if !s.is_empty() {
                sql.push_str(" AND (path LIKE ? OR target_path LIKE ?)");
                let pattern = format!("%{}%", s);
                param_values.push(Box::new(pattern.clone()));
                param_values.push(Box::new(pattern));
            }
        }

        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let count: i64 = conn.query_row(&sql, params_ref.as_slice(), |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Delete events older than the given datetime.
    pub async fn purge_before(&self, before: DateTime<Utc>) -> Result<usize, StorageError> {
        let conn = self.conn.lock().await;
        let deleted = conn.execute(
            "DELETE FROM events WHERE timestamp < ?1",
            [before.to_rfc3339()],
        )?;
        info!("Purged {} old events", deleted);
        Ok(deleted)
    }
}
