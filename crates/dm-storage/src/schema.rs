use rusqlite::Connection;
use tracing::info;

/// Current schema version.
pub const SCHEMA_VERSION: i32 = 1;

/// SQL statements to create the initial schema.
const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    event_type TEXT NOT NULL,
    path TEXT NOT NULL,
    target_path TEXT,
    user_name TEXT,
    process_name TEXT,
    watch_root TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
CREATE INDEX IF NOT EXISTS idx_events_path ON events(path);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_watch_root ON events(watch_root);

CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER NOT NULL
);
";

/// Initialize the database schema.
pub fn initialize(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(SCHEMA_SQL)
        .map_err(|e| format!("Failed to create schema: {e}"))?;

    // Set schema version
    let current_version = get_version(conn)?;
    if current_version == 0 {
        conn.execute(
            "INSERT INTO schema_version (version) VALUES (?1)",
            [SCHEMA_VERSION],
        )
        .map_err(|e| format!("Failed to set schema version: {e}"))?;
        info!("Database schema initialized at version {}", SCHEMA_VERSION);
    } else {
        info!("Database schema at version {}", current_version);
    }

    Ok(())
}

/// Get the current schema version (0 if not initialized).
pub fn get_version(conn: &Connection) -> Result<i32, String> {
    let version: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to read schema version: {e}"))?;
    Ok(version)
}
