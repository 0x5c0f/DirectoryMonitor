use thiserror::Error;

/// Top-level error type for Directory Monitor.
#[derive(Debug, Error)]
pub enum DmError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("Watcher error: {0}")]
    Watcher(#[source] WatcherError),

    #[error("Storage error: {0}")]
    Storage(#[source] StorageError),

    #[error("Notification error: {0}")]
    Notification(#[from] NotificationError),

    #[error("Service error: {0}")]
    Service(String),
}

/// Watcher-related errors.
#[derive(Debug, Error)]
pub enum WatcherError {
    #[error("Failed to initialize watcher: {0}")]
    Init(String),

    #[error("Failed to add watch for '{path}': {source}")]
    AddWatch {
        path: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Failed to remove watch for '{path}': {source}")]
    RemoveWatch {
        path: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Storage-related errors.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Configuration-related errors.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read config file '{path}': {source}")]
    ReadFailed {
        path: String,
        source: std::io::Error,
    },

    #[error("Failed to parse config: {0}")]
    ParseFailed(#[source] toml::de::Error),

    #[error("Failed to serialize config: {0}")]
    SerializeFailed(#[source] toml::ser::Error),

    #[error("Failed to write config file '{path}': {source}")]
    WriteFailed {
        path: String,
        source: std::io::Error,
    },

    #[error("Invalid watch path: {0}")]
    InvalidWatchPath(String),
}

/// Notification-related errors.
#[derive(Debug, Error)]
pub enum NotificationError {
    #[error("Email error: {0}")]
    Email(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("Syslog error: {0}")]
    Syslog(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("Script execution error: {0}")]
    Script(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("Sound error: {0}")]
    Sound(#[source] Box<dyn std::error::Error + Send + Sync>),
}
