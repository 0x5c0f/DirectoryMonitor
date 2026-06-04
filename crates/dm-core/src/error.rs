use thiserror::Error;

/// Top-level error type for Directory Monitor.
#[derive(Debug, Error)]
pub enum DmError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("Watcher error: {0}")]
    Watcher(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Notification error: {0}")]
    Notification(#[from] NotificationError),

    #[error("Service error: {0}")]
    Service(String),
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
    SerializeFailed(String),

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
    Email(String),

    #[error("Syslog error: {0}")]
    Syslog(String),

    #[error("Script execution error: {0}")]
    Script(String),

    #[error("Sound error: {0}")]
    Sound(String),
}
