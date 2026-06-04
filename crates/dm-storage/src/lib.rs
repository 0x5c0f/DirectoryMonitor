pub mod sqlite;
pub mod schema;

pub use sqlite::EventStore;

/// Storage-specific error type.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Database migration failed: {0}")]
    MigrationFailed(String),

    #[error("Database not initialized")]
    NotInitialized,
}
