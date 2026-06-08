pub mod query;
pub mod schema;
pub mod sqlite;
pub mod worker;

pub use query::EventQuery;
pub use worker::EventStore;

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
