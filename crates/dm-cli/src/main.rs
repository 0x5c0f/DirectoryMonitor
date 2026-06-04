use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dm_core::config::AppConfig;
use dm_processor::{EventDeduplicator, EventFilter};
use dm_storage::EventStore;
use dm_watcher::{FsWatcher, WatchEvent};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

#[derive(Parser)]
#[command(
    name = "directory-monitor",
    about = "Cross-platform filesystem monitoring tool",
    version,
    long_about = "A Rust implementation of Directory Monitor - watches directories for file system changes in real-time."
)]
struct Cli {
    /// Path to configuration file.
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    /// Log level (trace, debug, info, warn, error).
    #[arg(short, long, default_value = "info")]
    log_level: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start monitoring (default if no subcommand).
    Run,
    /// Validate the configuration file.
    Validate,
    /// Take a snapshot of a directory (for outage recovery).
    Snapshot {
        /// Directory to snapshot.
        #[arg(short, long)]
        path: PathBuf,
        /// Output file for the snapshot.
        #[arg(short, long)]
        output: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    init_logging(&cli.log_level)?;

    // Load configuration
    let config = AppConfig::load(&cli.config)
        .with_context(|| format!("Failed to load config from {}", cli.config.display()))?;

    match cli.command.unwrap_or(Commands::Run) {
        Commands::Validate => {
            validate_config(&config)?;
        }
        Commands::Run => {
            run_monitor(config).await?;
        }
        Commands::Snapshot { path, output } => {
            take_snapshot(&path, &output)?;
        }
    }

    Ok(())
}

fn init_logging(level: &str) -> Result<()> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .with_thread_ids(true)
        .init();

    Ok(())
}

fn validate_config(config: &AppConfig) -> Result<()> {
    info!("Validating configuration...");

    if config.watches.is_empty() {
        warn!("No watch directories configured");
    }

    for watch in &config.watches {
        if !watch.path.exists() {
            error!("Watch path does not exist: {}", watch.path.display());
        } else {
            info!("  ✓ {}", watch.path.display());
        }
    }

    if config.notifications.email.enabled {
        info!("  Email notifications: enabled (SMTP: {})", config.notifications.email.smtp_server);
    }
    if config.notifications.syslog.enabled {
        info!("  Syslog: enabled ({}:{})", config.notifications.syslog.server, config.notifications.syslog.port);
    }
    if config.database.enabled {
        info!("  Database: {}", config.database.path.display());
    }

    info!("Configuration valid");
    Ok(())
}

async fn run_monitor(config: AppConfig) -> Result<()> {
    info!("Starting Directory Monitor...");

    // Initialize storage
    let store = if config.database.enabled {
        Some(EventStore::open(&config.database.path).context("Failed to open database")?)
    } else {
        None
    };

    // Create event channel
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<WatchEvent>();

    // Create watcher — short debounce (200ms) to coalesce rapid OS events
    // without losing meaningful intermediate states.
    let mut watcher = FsWatcher::new(event_tx, Duration::from_millis(200))
        .map_err(|e| anyhow::anyhow!("Failed to create watcher: {e}"))?;

    // Add watches
    for watch_config in &config.watches {
        if watch_config.path.exists() {
            watcher
                .add_watch(watch_config)
                .map_err(|e| anyhow::anyhow!("Failed to add watch: {e}"))?;
        } else {
            warn!("Skipping non-existent path: {}", watch_config.path.display());
        }
    }

    // Create filters for each watch
    let filters: Vec<(PathBuf, EventFilter)> = config
        .watches
        .iter()
        .filter_map(|w| {
            match EventFilter::from_config(w) {
                Ok(f) => Some((w.path.clone(), f)),
                Err(e) => {
                    error!("Failed to create filter for {}: {e}", w.path.display());
                    None
                }
            }
        })
        .collect();

    // Create deduplicator
    let deduplicator = Arc::new(Mutex::new(EventDeduplicator::new(Duration::from_secs(2))));

    // Create notification senders
    let email_notifier = if config.notifications.email.enabled {
        Some(Arc::new(dm_notify::EmailNotifier::new(
            config.notifications.email.clone(),
        )))
    } else {
        None
    };

    let syslog_notifier = if config.notifications.syslog.enabled {
        Some(dm_notify::SyslogNotifier::new(&config.notifications.syslog))
    } else {
        None
    };

    let script_executor = Arc::new(dm_notify::ScriptExecutor::new(true));

    info!("Monitoring started. Press Ctrl+C to stop.");

    // Process events
    while let Some(watch_event) = event_rx.recv().await {
        match watch_event {
            WatchEvent::Event(event) => {
                // Deduplicate
                let event = {
                    let mut dedup = deduplicator.lock().await;
                    dedup.process(event)
                };

                let Some(event) = event else { continue };

                // Apply event type filter (from config event_types)
                let filtered = filters
                    .iter()
                    .find(|(root, _)| event.path.starts_with(root))
                    .map(|(_, f)| f.matches(&event))
                    .unwrap_or(true); // No filter = allow all
                if !filtered {
                    continue;
                }

                // Log the event
                let log_format = config
                    .watches
                    .iter()
                    .find(|w| event.path.starts_with(&w.path))
                    .and_then(|w| w.log_format.as_deref())
                    .unwrap_or(&config.logging.format);

                let formatted = event.format_with(log_format);
                info!("{}", formatted);

                // Store in database
                if let Some(ref store) = store {
                    if let Err(e) = store.insert(&event).await {
                        error!("Failed to store event: {e}");
                    }
                }

                // Send notifications
                if let Some(ref email) = email_notifier {
                    let recipients: Vec<String> = config
                        .watches
                        .iter()
                        .filter(|w| event.path.starts_with(&w.path))
                        .flat_map(|w| w.email_recipients.clone())
                        .collect();
                    if !recipients.is_empty() {
                        let email = email.clone();
                        let event = event.clone();
                        tokio::spawn(async move {
                            if let Err(e) = email.notify(&event, &recipients).await {
                                error!("Email notification failed: {e}");
                            }
                        });
                    }
                }

                if let Some(ref syslog) = syslog_notifier {
                    if let Err(e) = syslog.notify(&event) {
                        error!("Syslog notification failed: {e}");
                    }
                }

                // Execute scripts
                for watch_config in &config.watches {
                    if event.path.starts_with(&watch_config.path) {
                        if let Some(ref script) = watch_config.script {
                            let executor = script_executor.clone();
                            let script = script.clone();
                            let event = event.clone();
                            let mode = watch_config.script_mode.clone();
                            tokio::spawn(async move {
                                if mode == "sync" {
                                    if let Err(e) = executor.execute_sync(&script, &event, &[]) {
                                        error!("Script execution failed: {e}");
                                    }
                                } else if let Err(e) = executor.execute(&script, &event, &[]).await {
                                    error!("Script execution failed: {e}");
                                }
                            });
                            break;
                        }
                    }
                }
            }
            WatchEvent::Batch(events) => {
                debug!("Received batch of {} events", events.len());
            }
            WatchEvent::Error(msg) => {
                error!("Watcher error: {}", msg);
            }
        }
    }

    Ok(())
}

fn take_snapshot(path: &PathBuf, output: &PathBuf) -> Result<()> {
    use dm_watcher::snapshot::DirectorySnapshot;

    info!("Taking snapshot of {}...", path.display());
    let snapshot = DirectorySnapshot::new(path, true)
        .with_context(|| format!("Failed to snapshot {}", path.display()))?;

    let json = serde_json::to_string_pretty(&snapshot.files.len())
        .context("Failed to serialize snapshot")?;
    std::fs::write(output, json).with_context(|| format!("Failed to write {}", output.display()))?;

    info!(
        "Snapshot saved: {} entries -> {}",
        snapshot.files.len(),
        output.display()
    );
    Ok(())
}
