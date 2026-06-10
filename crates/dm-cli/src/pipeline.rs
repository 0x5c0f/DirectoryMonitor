use anyhow::{Context, Result};
use dm_core::config::AppConfig;
use dm_processor::{EventDeduplicator, EventFilter};
use dm_storage::EventStore;
use dm_watcher::{FsWatcher, WatchEvent};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info, warn};

/// Type alias for the monitoring setup tuple.
pub(crate) type MonitorComponents = (
    FsWatcher,
    broadcast::Sender<WatchEvent>,
    Vec<(PathBuf, EventFilter)>,
    Arc<Mutex<EventDeduplicator>>,
    Option<EventStore>,
);

/// Create shared monitoring components: (watcher, event_sender, filters, dedup, store).
pub(crate) fn setup_monitoring(config: &AppConfig) -> Result<MonitorComponents> {
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
            warn!(
                "Skipping non-existent path: {}",
                watch_config.path.display()
            );
        }
    }

    // Create filters for each watch
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

/// Process a watch event through the pipeline.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn process_watch_event(
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
                        // Check script_events filter: if configured, only matching events trigger script
                        let should_trigger = if watch_config.script_events.is_empty() {
                            true // Empty = use event_types (all events that passed the filter)
                        } else {
                            let event_type_str = event.event_type.to_string().to_lowercase();
                            watch_config.script_events.iter().any(|t| {
                                let t_lower = t.to_lowercase();
                                t_lower == event_type_str
                                    || t_lower == event_type_str.clone() + "d"
                                    || t_lower
                                        == event_type_str.trim_end_matches('e').to_owned() + "ed"
                            })
                        };

                        if should_trigger {
                            let executor = script_executor.clone();
                            let script = script.clone();
                            let event = event.clone();
                            let mode = watch_config.script_mode.clone();
                            tokio::spawn(async move {
                                if mode == "sync" {
                                    if let Err(e) = executor.execute_sync(&script, &event, &[]) {
                                        error!("Script execution failed: {e}");
                                    }
                                } else if let Err(e) = executor.execute(&script, &event, &[]).await
                                {
                                    error!("Script execution failed: {e}");
                                }
                            });
                        }
                        break;
                    }
                }
            }
        }
    }
}
