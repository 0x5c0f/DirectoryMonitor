use crate::schema;
use crate::StorageError;
use chrono::{DateTime, Utc};
use dm_core::event::{EventType, FsEvent};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::thread;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info};

/// Commands sent to the DB worker thread.
enum DbCommand {
    Insert {
        event: FsEvent,
        resp: oneshot::Sender<Result<(), StorageError>>,
    },
    InsertBatch {
        events: Vec<FsEvent>,
        resp: oneshot::Sender<Result<(), StorageError>>,
    },
    Query {
        limit: usize,
        offset: usize,
        event_types: Vec<String>,
        watch_root: Option<String>,
        search: Option<String>,
        after: Option<String>,
        before: Option<String>,
        is_dir: Option<bool>,
        resp: oneshot::Sender<Result<Vec<FsEvent>, StorageError>>,
    },
    Count {
        resp: oneshot::Sender<Result<usize, StorageError>>,
    },
    CountFiltered {
        event_types: Vec<String>,
        watch_root: Option<String>,
        search: Option<String>,
        after: Option<String>,
        before: Option<String>,
        is_dir: Option<bool>,
        resp: oneshot::Sender<Result<usize, StorageError>>,
    },
    PurgeBefore {
        before: DateTime<Utc>,
        resp: oneshot::Sender<Result<usize, StorageError>>,
    },
    TimeSeries {
        after: DateTime<Utc>,
        bucket_secs: i64,
        resp: oneshot::Sender<Result<Vec<(DateTime<Utc>, i64)>, StorageError>>,
    },
}

/// SQLite-backed event store with a dedicated worker thread.
#[derive(Clone)]
pub struct EventStore {
    tx: mpsc::Sender<DbCommand>,
}

impl EventStore {
    /// Open or create the database at the given path.
    pub fn open(db_path: &Path) -> Result<Self, StorageError> {
        let path_str = db_path.display().to_string();
        let db_path = db_path.to_path_buf();
        let (tx, rx) = mpsc::channel(256);

        thread::spawn(move || {
            if let Err(e) = run_worker(db_path, rx) {
                tracing::error!("DB worker failed: {e}");
            }
        });

        info!("Event store opened: {}", path_str);
        Ok(Self { tx })
    }

    /// Open an in-memory database (for testing).
    pub fn open_memory() -> Result<Self, StorageError> {
        let (tx, rx) = mpsc::channel(256);

        thread::spawn(move || {
            if let Err(e) = run_worker_memory(rx) {
                tracing::error!("DB worker failed: {e}");
            }
        });

        Ok(Self { tx })
    }

    /// Store a single event.
    pub async fn insert(&self, event: &FsEvent) -> Result<(), StorageError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(DbCommand::Insert {
                event: event.clone(),
                resp: resp_tx,
            })
            .await
            .map_err(|_| StorageError::NotInitialized)?;
        resp_rx.await.map_err(|_| StorageError::NotInitialized)?
    }

    /// Store multiple events in a transaction.
    pub async fn insert_batch(&self, events: &[FsEvent]) -> Result<(), StorageError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(DbCommand::InsertBatch {
                events: events.to_vec(),
                resp: resp_tx,
            })
            .await
            .map_err(|_| StorageError::NotInitialized)?;
        resp_rx.await.map_err(|_| StorageError::NotInitialized)?
    }

    /// Query events with optional filters.
    #[allow(clippy::too_many_arguments)]
    pub async fn query(
        &self,
        limit: usize,
        offset: usize,
        event_types: &[String],
        watch_root: Option<&str>,
        search: Option<&str>,
        after: Option<&str>,
        before: Option<&str>,
        is_dir: Option<bool>,
    ) -> Result<Vec<FsEvent>, StorageError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(DbCommand::Query {
                limit,
                offset,
                event_types: event_types.to_vec(),
                watch_root: watch_root.map(String::from),
                search: search.map(String::from),
                after: after.map(String::from),
                before: before.map(String::from),
                is_dir,
                resp: resp_tx,
            })
            .await
            .map_err(|_| StorageError::NotInitialized)?;
        resp_rx.await.map_err(|_| StorageError::NotInitialized)?
    }

    /// Query events using an EventQuery struct.
    pub async fn query_events(
        &self,
        query: crate::EventQuery,
    ) -> Result<Vec<FsEvent>, StorageError> {
        self.query(
            query.limit,
            query.offset,
            &query.event_types,
            query.watch_root.as_deref(),
            query.search.as_deref(),
            query.after.as_deref(),
            query.before.as_deref(),
            query.is_dir,
        )
        .await
    }

    /// Get the total number of stored events.
    pub async fn count(&self) -> Result<usize, StorageError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(DbCommand::Count { resp: resp_tx })
            .await
            .map_err(|_| StorageError::NotInitialized)?;
        resp_rx.await.map_err(|_| StorageError::NotInitialized)?
    }

    /// Get the count of events matching filters.
    pub async fn count_filtered(
        &self,
        event_types: &[String],
        watch_root: Option<&str>,
        search: Option<&str>,
        after: Option<&str>,
        before: Option<&str>,
        is_dir: Option<bool>,
    ) -> Result<usize, StorageError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(DbCommand::CountFiltered {
                event_types: event_types.to_vec(),
                watch_root: watch_root.map(String::from),
                search: search.map(String::from),
                after: after.map(String::from),
                before: before.map(String::from),
                is_dir,
                resp: resp_tx,
            })
            .await
            .map_err(|_| StorageError::NotInitialized)?;
        resp_rx.await.map_err(|_| StorageError::NotInitialized)?
    }

    /// Delete events older than the given datetime.
    pub async fn purge_before(&self, before: DateTime<Utc>) -> Result<usize, StorageError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(DbCommand::PurgeBefore {
                before,
                resp: resp_tx,
            })
            .await
            .map_err(|_| StorageError::NotInitialized)?;
        resp_rx.await.map_err(|_| StorageError::NotInitialized)?
    }

    /// Query event counts aggregated by time bucket.
    ///
    /// Returns `(bucket_timestamp, count)` pairs ordered by time ascending.
    /// `bucket_secs` is the bucket duration in seconds (e.g. 60 for per-minute, 3600 for per-hour).
    pub async fn time_series(
        &self,
        after: DateTime<Utc>,
        bucket_secs: i64,
    ) -> Result<Vec<(DateTime<Utc>, i64)>, StorageError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(DbCommand::TimeSeries {
                after,
                bucket_secs,
                resp: resp_tx,
            })
            .await
            .map_err(|_| StorageError::NotInitialized)?;
        resp_rx.await.map_err(|_| StorageError::NotInitialized)?
    }
}

/// Run the DB worker loop with a file-based database.
fn run_worker(db_path: PathBuf, mut rx: mpsc::Receiver<DbCommand>) -> Result<(), StorageError> {
    let conn = Connection::open(&db_path)?;

    // Enable WAL mode for better concurrent performance
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "cache_size", -64000)?; // 64MB cache

    schema::initialize(&conn).map_err(StorageError::MigrationFailed)?;

    while let Some(cmd) = rx.blocking_recv() {
        match cmd {
            DbCommand::Insert { event, resp } => {
                let result = do_insert(&conn, &event);
                let _ = resp.send(result);
            }
            DbCommand::InsertBatch { events, resp } => {
                let result = do_insert_batch(&conn, &events);
                let _ = resp.send(result);
            }
            DbCommand::Query {
                limit,
                offset,
                event_types,
                watch_root,
                search,
                after,
                before,
                is_dir,
                resp,
            } => {
                let result = do_query(
                    &conn,
                    limit,
                    offset,
                    &event_types,
                    watch_root.as_deref(),
                    search.as_deref(),
                    after.as_deref(),
                    before.as_deref(),
                    is_dir,
                );
                let _ = resp.send(result);
            }
            DbCommand::Count { resp } => {
                let result = do_count(&conn);
                let _ = resp.send(result);
            }
            DbCommand::CountFiltered {
                event_types,
                watch_root,
                search,
                after,
                before,
                is_dir,
                resp,
            } => {
                let result = do_count_filtered(
                    &conn,
                    &event_types,
                    watch_root.as_deref(),
                    search.as_deref(),
                    after.as_deref(),
                    before.as_deref(),
                    is_dir,
                );
                let _ = resp.send(result);
            }
            DbCommand::PurgeBefore { before, resp } => {
                let result = do_purge_before(&conn, before);
                let _ = resp.send(result);
            }
            DbCommand::TimeSeries {
                after,
                bucket_secs,
                resp,
            } => {
                let result = do_time_series(&conn, after, bucket_secs);
                let _ = resp.send(result);
            }
        }
    }

    Ok(())
}

/// Run the DB worker loop with an in-memory database.
fn run_worker_memory(mut rx: mpsc::Receiver<DbCommand>) -> Result<(), StorageError> {
    let conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    schema::initialize(&conn).map_err(StorageError::MigrationFailed)?;

    while let Some(cmd) = rx.blocking_recv() {
        match cmd {
            DbCommand::Insert { event, resp } => {
                let result = do_insert(&conn, &event);
                let _ = resp.send(result);
            }
            DbCommand::InsertBatch { events, resp } => {
                let result = do_insert_batch(&conn, &events);
                let _ = resp.send(result);
            }
            DbCommand::Query {
                limit,
                offset,
                event_types,
                watch_root,
                search,
                after,
                before,
                is_dir,
                resp,
            } => {
                let result = do_query(
                    &conn,
                    limit,
                    offset,
                    &event_types,
                    watch_root.as_deref(),
                    search.as_deref(),
                    after.as_deref(),
                    before.as_deref(),
                    is_dir,
                );
                let _ = resp.send(result);
            }
            DbCommand::Count { resp } => {
                let result = do_count(&conn);
                let _ = resp.send(result);
            }
            DbCommand::CountFiltered {
                event_types,
                watch_root,
                search,
                after,
                before,
                is_dir,
                resp,
            } => {
                let result = do_count_filtered(
                    &conn,
                    &event_types,
                    watch_root.as_deref(),
                    search.as_deref(),
                    after.as_deref(),
                    before.as_deref(),
                    is_dir,
                );
                let _ = resp.send(result);
            }
            DbCommand::PurgeBefore { before, resp } => {
                let result = do_purge_before(&conn, before);
                let _ = resp.send(result);
            }
            DbCommand::TimeSeries {
                after,
                bucket_secs,
                resp,
            } => {
                let result = do_time_series(&conn, after, bucket_secs);
                let _ = resp.send(result);
            }
        }
    }

    Ok(())
}

fn do_insert(conn: &Connection, event: &FsEvent) -> Result<(), StorageError> {
    conn.execute(
        "INSERT INTO events (id, timestamp, event_type, path, target_path, is_dir, user_name, process_name, watch_root)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            event.id.to_string(),
            event.timestamp.to_rfc3339(),
            event.event_type.to_string(),
            event.path.to_string_lossy().to_string(),
            event.target_path.as_ref().map(|p| p.to_string_lossy().to_string()),
            event.is_dir,
            event.user,
            event.process,
            event.watch_root.to_string_lossy().to_string(),
        ],
    )?;
    debug!(
        "Stored event: {} {}",
        event.event_type,
        event.path.display()
    );
    Ok(())
}

fn do_insert_batch(conn: &Connection, events: &[FsEvent]) -> Result<(), StorageError> {
    let tx = conn.unchecked_transaction()?;
    for event in events {
        tx.execute(
            "INSERT INTO events (id, timestamp, event_type, path, target_path, is_dir, user_name, process_name, watch_root)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                event.id.to_string(),
                event.timestamp.to_rfc3339(),
                event.event_type.to_string(),
                event.path.to_string_lossy().to_string(),
                event.target_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                event.is_dir,
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

#[allow(clippy::too_many_arguments)]
fn do_query(
    conn: &Connection,
    limit: usize,
    offset: usize,
    event_types: &[String],
    watch_root: Option<&str>,
    search: Option<&str>,
    after: Option<&str>,
    before: Option<&str>,
    is_dir: Option<bool>,
) -> Result<Vec<FsEvent>, StorageError> {
    let (where_clause, mut param_values) =
        build_where_clause(event_types, watch_root, search, after, before, is_dir);
    let mut sql = format!(
        "SELECT id, timestamp, event_type, path, target_path, is_dir, user_name, process_name, watch_root
         FROM events WHERE 1=1{}",
        where_clause
    );

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
        let event_type = match event_type_str.as_str() {
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
            other => {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "Unknown event type: {}",
                    other
                )));
            }
        };

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
    })?;

    let mut events = Vec::new();
    for row in rows {
        events.push(row?);
    }
    Ok(events)
}

fn do_count(conn: &Connection) -> Result<usize, StorageError> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
    Ok(count as usize)
}

fn do_count_filtered(
    conn: &Connection,
    event_types: &[String],
    watch_root: Option<&str>,
    search: Option<&str>,
    after: Option<&str>,
    before: Option<&str>,
    is_dir: Option<bool>,
) -> Result<usize, StorageError> {
    let (where_clause, param_values) =
        build_where_clause(event_types, watch_root, search, after, before, is_dir);
    let sql = format!("SELECT COUNT(*) FROM events WHERE 1=1{}", where_clause);

    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|p| p.as_ref()).collect();

    let count: i64 = conn.query_row(&sql, params_ref.as_slice(), |row| row.get(0))?;
    Ok(count as usize)
}

fn do_purge_before(conn: &Connection, before: DateTime<Utc>) -> Result<usize, StorageError> {
    let deleted = conn.execute(
        "DELETE FROM events WHERE timestamp < ?1",
        [before.to_rfc3339()],
    )?;
    info!("Purged {} old events", deleted);
    Ok(deleted)
}

fn do_time_series(
    conn: &Connection,
    after: DateTime<Utc>,
    bucket_secs: i64,
) -> Result<Vec<(DateTime<Utc>, i64)>, StorageError> {
    let sql = "
        SELECT
            CAST(strftime('%s', timestamp) / ?1 AS INTEGER) * ?1 AS bucket_ts,
            COUNT(*) AS cnt
        FROM events
        WHERE timestamp >= ?2
        GROUP BY bucket_ts
        ORDER BY bucket_ts
    ";

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![bucket_secs, after.to_rfc3339()], |row| {
        let ts: i64 = row.get(0)?;
        let cnt: i64 = row.get(1)?;
        Ok((ts, cnt))
    })?;

    let mut result = Vec::new();
    for row in rows {
        let (ts, cnt) = row?;
        if let Some(dt) = DateTime::from_timestamp(ts, 0) {
            result.push((dt, cnt));
        }
    }
    Ok(result)
}

/// Build a WHERE clause and parameter list from filter options.
fn build_where_clause(
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
