use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dm_core::config::AppConfig;
use dm_processor::{EventDeduplicator, EventFilter};
use dm_storage::EventStore;
use dm_watcher::{FsWatcher, WatchEvent};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Mutex, RwLock};
use tracing::{error, info, warn};

use dm_web::server::EventPayload;

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
    /// Start monitoring with a web dashboard.
    Serve {
        /// Bind address (host:port). Overrides config file [server] settings.
        #[arg(short, long)]
        bind: Option<String>,
    },
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
            if config.server.enabled {
                info!("server.enabled = true, starting with web dashboard");
                run_serve(config, cli.config, &None).await?;
            } else {
                run_monitor(config).await?;
            }
        }
        Commands::Serve { bind } => {
            run_serve(config, cli.config, &bind).await?;
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

/// Create shared monitoring components: (watcher, event_sender, filters, dedup, store, notifiers).
fn setup_monitoring(
    config: &AppConfig,
) -> Result<(
    FsWatcher,
    broadcast::Sender<WatchEvent>,
    Vec<(PathBuf, EventFilter)>,
    Arc<Mutex<EventDeduplicator>>,
    Option<EventStore>,
)> {
    // Create broadcast event channel
    let (event_tx, _) = broadcast::channel::<WatchEvent>(4096);

    // Create watcher
    let mut watcher = FsWatcher::new(event_tx.clone(), Duration::from_millis(200))
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

    // Open database
    let store = if config.database.enabled {
        Some(EventStore::open(&config.database.path).context("Failed to open database")?)
    } else {
        None
    };

    Ok((watcher, event_tx, filters, deduplicator, store))
}

async fn run_monitor(config: AppConfig) -> Result<()> {
    info!("Starting Directory Monitor...");

    let (_watcher, event_tx, filters, deduplicator, store) = setup_monitoring(&config)?;

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

    // Subscribe to events
    let mut event_rx = event_tx.subscribe();

    // Process events
    loop {
        match event_rx.recv().await {
            Ok(watch_event) => {
                process_watch_event(
                    watch_event,
                    &filters,
                    &deduplicator,
                    &store,
                    &config,
                    &email_notifier,
                    &syslog_notifier,
                    &script_executor,
                )
                .await;
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!("Event processing lagged, skipped {n} events");
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }

    Ok(())
}

async fn run_serve(mut config: AppConfig, config_path: PathBuf, bind: &Option<String>) -> Result<()> {
    info!("Starting Directory Monitor with web dashboard...");

    // CLI --bind overrides config file [server] settings
    if let Some(bind_addr) = bind {
        if let Some((host, port)) = bind_addr.rsplit_once(':') {
            config.server.bind = host.to_string();
            if let Ok(p) = port.parse() {
                config.server.port = p;
            }
        }
    }

    // Create broadcast event channel
    let (event_tx, _) = broadcast::channel::<WatchEvent>(4096);

    // Create WatcherManager
    let watcher_manager = Arc::new(dm_watcher::WatcherManager::new(event_tx.clone()));

    // Add initial watches from config
    for watch_config in &config.watches {
        if watch_config.path.exists() {
            match watcher_manager.add_watcher(watch_config.clone()).await {
                Ok(id) => info!("Added initial watcher[{}] for {}", id, watch_config.path.display()),
                Err(e) => warn!("Failed to add watcher for {}: {e}", watch_config.path.display()),
            }
        } else {
            warn!("Skipping non-existent path: {}", watch_config.path.display());
        }
    }

    // Create filters, deduplicator, and store
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
    let shared_filters: Arc<RwLock<Vec<(PathBuf, EventFilter)>>> = Arc::new(RwLock::new(filters));

    let deduplicator = Arc::new(Mutex::new(EventDeduplicator::new(Duration::from_secs(2))));

    let store = if config.database.enabled {
        Some(EventStore::open(&config.database.path).context("Failed to open database")?)
    } else {
        None
    };

    // Create the web event broadcast channel (separate from watcher channel)
    let (web_event_tx, _) = broadcast::channel::<EventPayload>(1024);

    // Spawn event processing task
    let process_tx = event_tx.clone();
    let web_tx = web_event_tx.clone();
    let process_config = config.clone();
    let process_filters = shared_filters.clone();
    let process_store = store.clone();
    let process_dedup = deduplicator.clone();

    tokio::spawn(async move {
        let mut rx = process_tx.subscribe();
        loop {
            match rx.recv().await {
                Ok(watch_event) => {
                    // Unpack batch into individual events
                    let events = match watch_event {
                        WatchEvent::Batch(events) => events,
                        WatchEvent::Event(event) => vec![event],
                        WatchEvent::Error(msg) => {
                            error!("Watcher error: {}", msg);
                            continue;
                        }
                    };

                    for event in events {
                        // Deduplicate
                        let event = {
                            let mut dedup = process_dedup.lock().await;
                            dedup.process(event)
                        };
                        let Some(event) = event else { continue };

                        // Apply filter
                        let filters = process_filters.read().await;
                        let filtered = filters
                            .iter()
                            .find(|(root, _)| event.path.starts_with(root))
                            .map(|(_, f)| f.matches(&event))
                            .unwrap_or(true);
                        if !filtered {
                            continue;
                        }

                        // Log the event
                        let log_format = process_config
                            .watches
                            .iter()
                            .find(|w| event.path.starts_with(&w.path))
                            .and_then(|w| w.log_format.as_deref())
                            .unwrap_or(&process_config.logging.format);
                        info!("{}", event.format_with(log_format));

                        // Store in database
                        if let Some(ref store) = process_store {
                            if let Err(e) = store.insert(&event).await {
                                error!("Failed to store event: {e}");
                            }
                        }

                        // Send to web clients
                        let payload = EventPayload::from(&event);
                        let _ = web_tx.send(payload);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Event processing lagged, skipped {n} events");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    info!("Monitoring started. Press Ctrl+C to stop.");

    // Run the web server (blocks until Ctrl+C)
    let store_for_web = if config.database.enabled {
        Some(EventStore::open(&config.database.path).context("Failed to open database for web")?)
    } else {
        None
    };
    let filters_for_web = shared_filters.clone();

    // Setup SIGHUP handler for systemd reload
    #[cfg(unix)]
    let watcher_manager_clone = watcher_manager.clone();
    #[cfg(unix)]
    let config_path_clone = config_path.clone();
    #[cfg(unix)]
    let filters_clone = shared_filters.clone();

    #[cfg(unix)]
    tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let mut stream = signal(SignalKind::hangup()).expect("Failed to register SIGHUP handler");
        loop {
            stream.recv().await;
            info!("Received SIGHUP, reloading configuration...");

            match AppConfig::load(&config_path_clone) {
                Ok(new_config) => {
                    match watcher_manager_clone.reload(&new_config).await {
                        Ok(result) => {
                            // Update filters
                            let new_filters: Vec<(PathBuf, EventFilter)> = new_config
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
                            *filters_clone.write().await = new_filters;

                            info!(
                                "Reload complete: added={}, removed={}, kept={}",
                                result.added.len(),
                                result.removed.len(),
                                result.kept.len()
                            );
                        }
                        Err(e) => {
                            error!("Failed to reload watchers: {e}");
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to load config for reload: {e}");
                }
            }
        }
    });

    tokio::select! {
        result = dm_web::run_server(config, config_path, store_for_web, web_event_tx, watcher_manager, filters_for_web) => {
            if let Err(e) = result {
                error!("Web server error: {e}");
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Shutting down...");
        }
    }

    Ok(())
}

/// Process a single watch event (shared between run and serve modes).
async fn process_watch_event(
    watch_event: WatchEvent,
    filters: &[(PathBuf, EventFilter)],
    deduplicator: &Arc<Mutex<EventDeduplicator>>,
    store: &Option<EventStore>,
    config: &AppConfig,
    email_notifier: &Option<Arc<dm_notify::EmailNotifier>>,
    syslog_notifier: &Option<dm_notify::SyslogNotifier>,
    script_executor: &Arc<dm_notify::ScriptExecutor>,
) {
    // Unpack batch into individual events
    let events = match watch_event {
        WatchEvent::Batch(events) => events,
        WatchEvent::Event(event) => vec![event],
        WatchEvent::Error(msg) => {
            error!("Watcher error: {}", msg);
            return;
        }
    };

    for event in events {
        {
            // Deduplicate
            let event = {
                let mut dedup = deduplicator.lock().await;
                dedup.process(event)
            };

            let Some(event) = event else { return };

            // Apply event type filter (from config event_types)
            let filtered = filters
                .iter()
                .find(|(root, _)| event.path.starts_with(root))
                .map(|(_, f)| f.matches(&event))
                .unwrap_or(true); // No filter = allow all
            if !filtered {
                return;
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
    }
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
