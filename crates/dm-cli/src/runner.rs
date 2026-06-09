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

use dm_web::server::EventPayload;

use crate::pipeline::{process_watch_event, setup_monitoring};
use crate::ClusterCommands;

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

    // Generate and persist node_id if empty (cluster mode)
    if config.cluster.enabled && config.cluster.node_id.is_empty() {
        let new_node_id = uuid::Uuid::new_v4().to_string();
        config.cluster.node_id = new_node_id.clone();
        info!("Generated new node ID: {new_node_id}");

        // Save to config file
        if let Ok(mut file_config) = dm_core::config::AppConfig::load(&config_path) {
            file_config.cluster.node_id = new_node_id;
            if let Err(e) = file_config.save(&config_path) {
                warn!("Failed to persist node ID to config: {e}");
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
    let web_tx = web_event_tx.clone();
    let process_config = config.clone();
    let process_filters = shared_filters.clone();
    let process_store = store.clone();
    let process_dedup = deduplicator.clone();
    let process_metrics = metrics.clone();

    tokio::spawn(async move {
        let mut rx = process_tx.subscribe();
        loop {
            match rx.recv().await {
                Ok(watch_event) => {
                    // Unpack batch into individual events
                    let events = match watch_event {
                        WatchEvent::Batch(events) => {
                            process_metrics.batches_flushed.inc();
                            events
                        }
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
                        let Some(event) = event else {
                            process_metrics.events_deduped.inc();
                            continue;
                        };

                        // Apply filter
                        let filters = process_filters.read().await;
                        let filtered = filters
                            .iter()
                            .find(|(root, _)| event.path.starts_with(root))
                            .map(|(_, f)| f.matches(&event))
                            .unwrap_or(true);
                        if !filtered {
                            process_metrics.events_dropped.inc();
                            continue;
                        }

                        // Record metrics
                        process_metrics.record_event(
                            &event.event_type.to_string(),
                            &event.watch_root.to_string_lossy(),
                        );

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
    let metrics_clone = metrics.clone();

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

    // ── Cluster initialization ─────────────────────────────────────────────────
    let mut cluster_node_id;
    let mut cluster_node_name;
    let mut node_registry: Option<dm_cluster::NodeRegistry> = None;
    let mut cluster_aggregator: Option<dm_cluster::ClusterQueryAggregator> = None;

    if config.cluster.enabled {
        let node_id = config.cluster.node_id.clone();
        let node_name = config.cluster.node_name.clone();
        let listen_addr = config.cluster.listen_addr.clone();

        cluster_node_id = node_id.clone();
        cluster_node_name = node_name.clone();

        // Build peer list from config
        let peers: Vec<(String, String)> = config.cluster.peers.iter()
            .enumerate()
            .map(|(i, p)| (format!("peer-{}", i), p.addr.clone()))
            .collect();

        // Create PeerManager
        match dm_cluster::PeerManager::new(node_id.clone(), peers).await {
            Ok(peer_manager) => {
                info!("PeerManager initialized with {} peers", config.cluster.peers.len());

                // Create NodeRegistry
                let registry = dm_cluster::NodeRegistry::new(
                    node_id.clone(),
                    node_name.clone(),
                    listen_addr.clone(),
                    config.cluster.node_timeout_secs as i64,
                );

                // Start EventSyncService
                let event_sync = dm_cluster::EventSyncService::new(
                    peer_manager.clone(),
                    config.cluster.event_cache_size,
                );
                let event_cache = event_sync.cache().clone();
                let sync_rx = event_tx.subscribe();
                tokio::spawn(async move {
                    event_sync.start_publish_loop(sync_rx).await;
                });

                // Start HeartbeatService
                let heartbeat = dm_cluster::HeartbeatService::new(
                    peer_manager.clone(),
                    registry.clone(),
                    listen_addr.clone(),
                );
                let hb_interval = config.cluster.heartbeat_interval_secs;
                tokio::spawn(async move {
                    heartbeat.start_publish_loop(hb_interval).await;
                });

                // Periodically update local node stats in registry
                let stats_registry = registry.clone();
                let stats_metrics = metrics.clone();
                tokio::spawn(async move {
                    let mut interval = tokio::time::interval(Duration::from_secs(10));
                    loop {
                        interval.tick().await;
                        let watcher_count = stats_metrics.active_watchers.get() as usize;
                        let event_count = stats_metrics.events_total.get() as u64;
                        stats_registry
                            .update_local_stats(watcher_count, event_count)
                            .await;
                    }
                });

                // Start gRPC server for distributed queries
                if let Ok(addr) = listen_addr.parse::<std::net::SocketAddr>() {
                    match dm_storage::EventStore::open(&config.database.path) {
                        Ok(grpc_store) => {
                            let grpc_node_id = node_id.clone();
                            let grpc_node_name = node_name.clone();
                            let grpc_cache = event_cache.clone();
                            let grpc_registry = registry.clone();
                            // Create a broadcast channel for cluster events
                            let (cluster_event_tx, _) = tokio::sync::broadcast::channel::<dm_cluster::ClusterEvent>(1024);
                            tokio::spawn(async move {
                                if let Err(e) = dm_cluster::grpc::server::start_grpc_server_with_cluster(
                                    addr,
                                    grpc_store,
                                    grpc_node_id,
                                    grpc_node_name,
                                    grpc_cache,
                                    grpc_registry,
                                    cluster_event_tx,
                                )
                                .await
                                {
                                    error!("gRPC server error: {e}");
                                }
                            });
                            info!("gRPC server starting on {addr}");
                        }
                        Err(e) => {
                            error!("Failed to open database for gRPC server: {e}");
                        }
                    }
                } else {
                    error!("Invalid cluster listen address: {listen_addr}");
                }

                // Spawn peer reconnection task
                peer_manager.spawn_reconnect_task();

                // Create ClusterQueryAggregator for cross-node event queries
                if let Some(ref web_store) = store_for_web {
                    let aggregator = dm_cluster::ClusterQueryAggregator::new(
                        web_store.clone(),
                        event_cache,
                        registry.clone(),
                    );
                    cluster_aggregator = Some(aggregator);
                    info!("Cluster query aggregator initialized");
                }

                node_registry = Some(registry);
            }
            Err(e) => {
                warn!("Failed to initialize PeerManager: {e}. Running in standalone mode.");
                cluster_node_id = String::new();
                cluster_node_name = String::new();
            }
        }
    } else {
        cluster_node_id = String::new();
        cluster_node_name = String::new();
    }

    tokio::select! {
        result = dm_web::run_server(config, config_path, store_for_web, web_event_tx, watcher_manager, filters_for_web, metrics, cluster_node_id, cluster_node_name, node_registry, cluster_aggregator) => {
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

    let json = serde_json::to_string_pretty(&snapshot.files.len())
        .context("Failed to serialize snapshot")?;
    std::fs::write(output, json)
        .with_context(|| format!("Failed to write {}", output.display()))?;

    info!(
        "Snapshot saved: {} entries -> {}",
        snapshot.files.len(),
        output.display()
    );
    Ok(())
}

/// Run cluster management commands.
pub(crate) async fn run_cluster(command: ClusterCommands, config: &AppConfig) -> Result<()> {
    if !config.cluster.enabled {
        anyhow::bail!("Cluster mode is not enabled. Set cluster.enabled = true in config.");
    }

    let node_id = config.cluster.node_id.clone();
    let node_name = config.cluster.node_name.clone();

    match command {
        ClusterCommands::Status => {
            info!("Cluster Status");
            info!("  Node ID:   {}", node_id);
            info!("  Node Name: {}", node_name);
            info!("  Listen:     {}", config.cluster.listen_addr);
            info!("  Peers:      {}", config.cluster.peers.len());
            info!("  Heartbeat:  {}s", config.cluster.heartbeat_interval_secs);
            info!("  Timeout:    {}s", config.cluster.node_timeout_secs);
            Ok(())
        }
        ClusterCommands::Nodes => {
            // Query each peer via gRPC to get node status
            info!("Querying cluster nodes via gRPC...");

            let registry = dm_cluster::NodeRegistry::new(
                node_id.clone(),
                node_name.clone(),
                config.cluster.listen_addr.clone(),
                config.cluster.node_timeout_secs as i64,
            );

            // Query each configured peer
            for peer in &config.cluster.peers {
                match dm_cluster::grpc::client::GrpcClient::connect(&peer.addr).await {
                    Ok(mut client) => {
                        match client.get_node_status().await {
                            Ok(status) => {
                                registry.update_heartbeat(
                                    &status.node_id,
                                    &status.node_name,
                                    &status.listen_addr,
                                    status.watcher_count,
                                    status.event_count,
                                ).await;
                                info!("  ✓ {} ({}) - Online", status.node_name, status.node_id);
                            }
                            Err(e) => {
                                warn!("  ✗ {} - Failed to get status: {}", peer.addr, e);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("  ✗ {} - Connection failed: {}", peer.addr, e);
                    }
                }
            }

            // Print nodes
            let nodes = registry.list_nodes().await;
            info!("Cluster nodes ({}):", nodes.len());
            for node in &nodes {
                info!(
                    "  [{}] {} ({}) - {} (watchers: {}, events: {})",
                    node.status,
                    node.name,
                    node.id,
                    node.addr,
                    node.watcher_count,
                    node.event_count,
                );
            }

            Ok(())
        }
    }
}
