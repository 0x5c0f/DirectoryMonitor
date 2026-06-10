use crate::StorageError;
use rusqlite::Connection;
use tracing::info;

/// Current schema version.
pub const SCHEMA_VERSION: i32 = 2;

/// SQL statements to create the initial schema.
const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    event_type TEXT NOT NULL,
    path TEXT NOT NULL,
    target_path TEXT,
    is_dir INTEGER,
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
pub fn initialize(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch(SCHEMA_SQL)?;

    // Set schema version
    let current_version = get_version(conn)?;
    if current_version == 0 {
        conn.execute(
            "INSERT INTO schema_version (version) VALUES (?1)",
            [SCHEMA_VERSION],
        )?;
        info!("Database schema initialized at version {}", SCHEMA_VERSION);
    } else if current_version < SCHEMA_VERSION {
        // Run migrations
        migrate(conn, current_version)?;
    } else {
        info!("Database schema at version {}", current_version);
    }

    Ok(())
}

/// Run database migrations from the given version to the current version.
fn migrate(conn: &Connection, from_version: i32) -> Result<(), StorageError> {
    info!(
        "Migrating database schema from version {} to {}",
        from_version, SCHEMA_VERSION
    );

    if from_version < 2 {
        // v1 → v2: add is_dir column
        conn.execute("ALTER TABLE events ADD COLUMN is_dir INTEGER", [])?;
        info!("Migration v1→v2: added is_dir column");
    }

    // Update schema version
    conn.execute("UPDATE schema_version SET version = ?1", [SCHEMA_VERSION])?;

    info!("Database schema migrated to version {}", SCHEMA_VERSION);
    Ok(())
}

/// Get the current schema version (0 if not initialized).
pub fn get_version(conn: &Connection) -> Result<i32, StorageError> {
    let version: i32 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get(0),
    )?;
    Ok(version)
}
