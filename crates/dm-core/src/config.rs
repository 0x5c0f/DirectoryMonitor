use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Root configuration for Directory Monitor.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// General settings.
    pub general: GeneralConfig,
    /// Directories to monitor.
    pub watches: Vec<WatchConfig>,
    /// Notification settings.
    pub notifications: NotificationsConfig,
    /// Database settings.
    pub database: DatabaseConfig,
    /// Logging settings.
    pub logging: LoggingConfig,
    /// Web server settings.
    pub server: ServerConfig,
}

impl AppConfig {
    /// Load configuration from a TOML file.
    pub fn load(path: &Path) -> Result<Self, crate::error::ConfigError> {
        let content =
            std::fs::read_to_string(path).map_err(|e| crate::error::ConfigError::ReadFailed {
                path: path.display().to_string(),
                source: e,
            })?;
        toml::from_str(&content).map_err(crate::error::ConfigError::ParseFailed)
    }

    /// Save configuration to a TOML file.
    pub fn save(&self, path: &Path) -> Result<(), crate::error::ConfigError> {
        let content =
            toml::to_string_pretty(self).map_err(crate::error::ConfigError::SerializeFailed)?;
        std::fs::write(path, content).map_err(|e| crate::error::ConfigError::WriteFailed {
            path: path.display().to_string(),
            source: e,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct GeneralConfig {}

/// Configuration for a single watched directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchConfig {
    /// Path to monitor.
    pub path: PathBuf,
    /// Whether to watch subdirectories recursively.
    #[serde(default = "default_true")]
    pub recursive: bool,
    /// Glob patterns to include (empty = all).
    #[serde(default)]
    pub include: Vec<String>,
    /// Glob patterns to exclude.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Event types to monitor (empty = all).
    #[serde(default)]
    pub event_types: Vec<String>,
    /// Custom log format with macro placeholders.
    pub log_format: Option<String>,
    /// Script/command to execute on events.
    pub script: Option<String>,
    /// Script execution mode: "sync" or "async".
    #[serde(default = "default_script_mode")]
    pub script_mode: String,
    /// Event types that trigger script execution (empty = use event_types).
    #[serde(default)]
    pub script_events: Vec<String>,
    /// Email recipients for this watch.
    #[serde(default)]
    pub email_recipients: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_script_mode() -> String {
    "async".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationsConfig {
    /// Email notification settings.
    pub email: EmailConfig,
    /// Syslog settings.
    pub syslog: SyslogConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmailConfig {
    pub enabled: bool,
    pub smtp_server: String,
    pub smtp_port: u16,
    pub username: String,
    pub password: String,
    pub from_address: String,
    pub use_tls: bool,
    /// Batch events before sending (0 = send immediately).
    pub batch_size: usize,
    /// Max emails per minute (throttle).
    pub max_per_minute: u32,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            smtp_server: String::new(),
            smtp_port: 587,
            username: String::new(),
            password: String::new(),
            from_address: String::new(),
            use_tls: true,
            batch_size: 0,
            max_per_minute: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SyslogConfig {
    pub enabled: bool,
    pub server: String,
    pub port: u16,
    /// RFC format: "rfc3164" or "rfc5424".
    pub format: String,
    pub facility: String,
    pub message_format: Option<String>,
}

impl Default for SyslogConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            server: "localhost".to_string(),
            port: 514,
            format: "rfc5424".to_string(),
            facility: "user".to_string(),
            message_format: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub enabled: bool,
    /// SQLite database file path.
    pub path: PathBuf,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: PathBuf::from("directory-monitor.db"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    /// Global log file path.
    pub file: Option<PathBuf>,
    /// Default log format with macro placeholders.
    pub format: String,
    /// Log rotation: "daily", "monthly", or "never".
    pub rotation: String,
    /// Minimum log level for application logs.
    pub level: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            file: None,
            format: "%timestamp% [%event%] %path%".to_string(),
            rotation: "daily".to_string(),
            level: "info".to_string(),
        }
    }
}

/// Web server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Whether the web server is enabled.
    pub enabled: bool,
    /// Bind address.
    pub bind: String,
    /// Bind port.
    pub port: u16,
    /// Password for web dashboard authentication. Empty = no auth required.
    pub password: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind: "127.0.0.1".to_string(),
            port: 8080,
            password: String::new(),
        }
    }
}
