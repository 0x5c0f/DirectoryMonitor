use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Root configuration for Directory Monitor.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
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
    /// Cluster settings.
    pub cluster: ClusterConfig,
}

impl AppConfig {
    /// Load configuration from a TOML file.
    pub fn load(path: &Path) -> Result<Self, crate::error::ConfigError> {
        let content =
            std::fs::read_to_string(path).map_err(|e| crate::error::ConfigError::ReadFailed {
                path: path.display().to_string(),
                source: e,
            })?;
        let mut config: AppConfig =
            toml::from_str(&content).map_err(crate::error::ConfigError::ParseFailed)?;
        // Resolve relative paths against the config file's directory
        if let Some(config_dir) = path.parent() {
            config.resolve_paths(config_dir);
        }
        Ok(config)
    }

    /// Resolve relative paths in the config against a base directory.
    /// This ensures relative database paths work correctly regardless of
    /// the process's current working directory (e.g., under systemd).
    fn resolve_paths(&mut self, base_dir: &Path) {
        if self.database.enabled && self.database.path.is_relative() {
            self.database.path = base_dir.join(&self.database.path);
        }
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
    /// Sound alert settings.
    pub sound: SoundConfig,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundConfig {
    pub enabled: bool,
    /// Path to WAV file to play.
    pub file: Option<PathBuf>,
    /// Whether to loop until acknowledged.
    #[serde(default)]
    pub loop_until_viewed: bool,
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

/// Cluster configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClusterConfig {
    /// Whether cluster mode is enabled.
    pub enabled: bool,
    /// Node display name.
    pub node_name: String,
    /// Unique node ID (auto-generated if empty).
    pub node_id: String,
    /// Address to listen for cluster connections (gRPC).
    pub listen_addr: String,
    /// Heartbeat interval in seconds.
    pub heartbeat_interval_secs: u64,
    /// Node timeout in seconds (mark as offline).
    pub node_timeout_secs: u64,
    /// Max events in cluster event cache (ring buffer).
    pub event_cache_size: usize,
    /// TLS configuration.
    pub tls: TlsConfig,
    /// Static peer list.
    pub peers: Vec<PeerConfig>,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            node_name: hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "unknown".to_string()),
            node_id: String::new(), // Will be auto-generated and persisted on first run
            listen_addr: "0.0.0.0:9101".to_string(),
            heartbeat_interval_secs: 5,
            node_timeout_secs: 30,
            event_cache_size: 10000,
            tls: TlsConfig::default(),
            peers: Vec::new(),
        }
    }
}

/// TLS configuration for cluster connections.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TlsConfig {
    /// Whether TLS is enabled.
    pub enabled: bool,
    /// Path to TLS certificate file.
    pub cert_file: Option<String>,
    /// Path to TLS private key file.
    pub key_file: Option<String>,
    /// Path to CA certificate file.
    pub ca_file: Option<String>,
    /// Whether to verify client certificates (mTLS).
    pub verify_client: bool,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cert_file: None,
            key_file: None,
            ca_file: None,
            verify_client: false,
        }
    }
}

/// Peer node configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConfig {
    /// Peer address (host:port).
    pub addr: String,
    /// Custom timeout for this peer (e.g., for public network peers).
    pub timeout_secs: Option<u64>,
}
