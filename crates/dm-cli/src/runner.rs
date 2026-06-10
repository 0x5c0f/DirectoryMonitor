use anyhow::{Context, Result};
use dm_core::config::AppConfig;
use dm_processor::{EventDeduplicator, EventFilter};
use dm_storage::EventStore;
use dm_watcher::{WatchEvent, WatcherManager};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Mutex, RwLock};
use tracing::{error, info, warn};

use dm_web::EventPayload;

use crate::pipeline::{process_watch_event, setup_monitoring, ProcessHooks};

/// Run in monitor-only mode (no web server).
pub(crate) async fn run_monitor(config: AppConfig) -> Result<()> {
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
        Some(
            dm_notify::SyslogNotifier::new(&config.notifications.syslog)
                .map_err(|e| anyhow::anyhow!("Failed to initialize syslog notifier: {e}"))?,
        )
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
                    None,
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

/// Run with web dashboard.
pub(crate) async fn run_serve(
    mut config: AppConfig,
    config_path: PathBuf,
    bind: &Option<String>,
) -> Result<()> {
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

    // Create metrics registry
    let metrics = Arc::new(dm_metrics::MetricsRegistry::new());

    // Create broadcast event channel
    let (event_tx, _) = broadcast::channel::<WatchEvent>(4096);

    // Create WatcherManager
    let watcher_manager = Arc::new(WatcherManager::new(event_tx.clone()));

    // Add initial watches from config
    for watch_config in &config.watches {
        if watch_config.path.exists() {
            match watcher_manager.add_watcher(watch_config.clone()).await {
                Ok(id) => {
                    info!(
                        "Added initial watcher[{}] for {}",
                        id,
                        watch_config.path.display()
                    );
                    metrics.active_watchers.inc();
                }
                Err(e) => warn!(
                    "Failed to add watcher for {}: {e}",
                    watch_config.path.display()
                ),
            }
        } else {
            warn!(
                "Skipping non-existent path: {}",
                watch_config.path.display()
            );
        }
    }

    // Create filters, deduplicator, and store
    let filters: Vec<(PathBuf, EventFilter)> = config
        .watches
        .iter()
        .filter_map(|w| match EventFilter::from_config(w) {
            Ok(f) => Some((w.path.clone(), f)),
            Err(e) => {
                error!("Failed to create filter for {}: {e}", w.path.display());
                None
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
    let process_config = config.clone();
    let process_filters = shared_filters.clone();
    let process_store = store.clone();
    let process_dedup = deduplicator.clone();

    // Create notification senders for the processing task
    let email_notifier = if config.notifications.email.enabled {
        Some(Arc::new(dm_notify::EmailNotifier::new(
            config.notifications.email.clone(),
        )))
    } else {
        None
    };
    let syslog_notifier = if config.notifications.syslog.enabled {
        Some(
            dm_notify::SyslogNotifier::new(&config.notifications.syslog)
                .map_err(|e| anyhow::anyhow!("Failed to initialize syslog notifier: {e}"))?,
        )
    } else {
        None
    };
    let script_executor = Arc::new(dm_notify::ScriptExecutor::new(true));

    let hooks = ProcessHooks {
        metrics: Some(metrics.clone()),
        web_tx: Some(web_event_tx.clone()),
    };

    tokio::spawn(async move {
        let mut rx = process_tx.subscribe();
        loop {
            match rx.recv().await {
                Ok(watch_event) => {
                    // Get the current filters
                    let filters = process_filters.read().await;
                    process_watch_event(
                        watch_event,
                        &filters,
                        &process_dedup,
                        &process_store,
                        &process_config,
                        &email_notifier,
                        &syslog_notifier,
                        &script_executor,
                        Some(&hooks),
                    )
                    .await;
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
    let metrics_clone = metrics.clone();

    #[cfg(unix)]
    tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let mut stream = match signal(SignalKind::hangup()) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to register SIGHUP handler: {e}");
                return;
            }
        };
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
                                .filter_map(|w| match EventFilter::from_config(w) {
                                    Ok(f) => Some((w.path.clone(), f)),
                                    Err(e) => {
                                        error!(
                                            "Failed to create filter for {}: {e}",
                                            w.path.display()
                                        );
                                        None
                                    }
                                })
                                .collect();
                            *filters_clone.write().await = new_filters;

                            // Update watcher count
                            let net_change =
                                result.added.len() as i64 - result.removed.len() as i64;
                            if net_change != 0 {
                                metrics_clone.active_watchers.add(net_change);
                            }

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

    // Spawn background task to update database size
    let db_path = config.database.path.clone();
    let metrics_db = metrics.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            if let Ok(metadata) = tokio::fs::metadata(&db_path).await {
                metrics_db.db_size_bytes.set(metadata.len() as i64);
            }
        }
    });

    tokio::select! {
        result = dm_web::run_server(config, config_path, store_for_web, web_event_tx, watcher_manager, filters_for_web, metrics) => {
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

/// Take a snapshot of a directory.
pub(crate) fn take_snapshot(path: &Path, output: &Path) -> Result<()> {
    use dm_watcher::snapshot::DirectorySnapshot;

    info!("Taking snapshot of {}...", path.display());
    let snapshot = DirectorySnapshot::new(path, true)
        .with_context(|| format!("Failed to snapshot {}", path.display()))?;

    let json = serde_json::to_string_pretty(&snapshot).context("Failed to serialize snapshot")?;
    std::fs::write(output, json)
        .with_context(|| format!("Failed to write {}", output.display()))?;

    info!(
        "Snapshot saved: {} entries -> {}",
        snapshot.files.len(),
        output.display()
    );
    Ok(())
}
