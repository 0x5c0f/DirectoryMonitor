//! SQLite-backed persistent event storage for Directory Monitor.
//!
//! Provides an async event store backed by SQLite with WAL mode:
//! - [`EventStore`] — thread-safe store using a dedicated worker thread
//! - [`EventQuery`] — flexible query builder with pagination and filtering
//! - Time-series aggregation for dashboard charts

pub mod query;
pub mod schema;
mod shared;
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
