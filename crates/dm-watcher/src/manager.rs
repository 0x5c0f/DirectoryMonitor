use crate::watcher::{FsWatcher, WatchEvent};
use dm_core::config::{AppConfig, WatchConfig};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tracing::info;

/// Entry for a managed watcher.
struct WatcherEntry {
    /// Unique ID for this watcher.
    id: usize,
    /// The watcher instance (kept alive to maintain monitoring).
    _watcher: FsWatcher,
    /// The config this watcher was created from.
    config: WatchConfig,
}

/// Manages multiple filesystem watchers with independent lifecycles.
pub struct WatcherManager {
    /// Active watchers indexed by ID.
    watchers: Arc<Mutex<HashMap<usize, WatcherEntry>>>,
    /// Channel to send events to consumers.
    event_tx: broadcast::Sender<WatchEvent>,
    /// Next available watcher ID.
    next_id: Arc<Mutex<usize>>,
    /// Debounce duration for new watchers.
    debounce_duration: std::time::Duration,
}

impl WatcherManager {
    /// Create a new WatcherManager.
    pub fn new(event_tx: broadcast::Sender<WatchEvent>) -> Self {
        Self {
            watchers: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
            next_id: Arc::new(Mutex::new(0)),
            debounce_duration: std::time::Duration::from_millis(100),
        }
    }

    /// Add a new watcher for the given config.
    /// Returns the watcher ID on success.
    pub async fn add_watcher(&self, config: WatchConfig) -> Result<usize, String> {
        let mut watchers = self.watchers.lock().await;
        let mut next_id = self.next_id.lock().await;

        // Check if path is already being watched
        for entry in watchers.values() {
            if entry.config.path == config.path {
                return Err(format!(
                    "Path {} is already being watched",
                    config.path.display()
                ));
            }
        }

        // Create new watcher
        let mut watcher = FsWatcher::new(self.event_tx.clone(), self.debounce_duration)?;
        watcher.add_watch(&config)?;

        let id = *next_id;
        *next_id += 1;

        let thread_id = watcher.thread_id().map(String::from);

        let entry = WatcherEntry {
            id,
            _watcher: watcher,
            config: config.clone(),
        };

        watchers.insert(id, entry);
        info!(
            "Added watcher[{}] for {} (thread: {:?})",
            id,
            config.path.display(),
            thread_id
        );

        Ok(id)
    }

    /// Remove a watcher by ID.
    pub async fn remove_watcher(&self, id: usize) -> Result<PathBuf, String> {
        let mut watchers = self.watchers.lock().await;

        if let Some(entry) = watchers.remove(&id) {
            let path = entry.config.path.clone();
            info!("Removed watcher[{}] for {}", id, path.display());
            Ok(path)
        } else {
            Err(format!("Watcher {} not found", id))
        }
    }

    /// List all active watchers.
    pub async fn list_watchers(&self) -> Vec<WatcherInfo> {
        let watchers = self.watchers.lock().await;
        watchers
            .values()
            .map(|entry| WatcherInfo {
                id: entry.id,
                path: entry.config.path.display().to_string(),
                recursive: entry.config.recursive,
                include: entry.config.include.clone(),
                exclude: entry.config.exclude.clone(),
                event_types: entry.config.event_types.clone(),
                thread_id: entry._watcher.thread_id().map(String::from),
            })
            .collect()
    }

    /// Get the number of active watchers.
    pub async fn count(&self) -> usize {
        let watchers = self.watchers.lock().await;
        watchers.len()
    }

    /// Reload watchers based on new config.
    /// Compares current watchers with new config and applies differences.
    pub async fn reload(&self, new_config: &AppConfig) -> Result<ReloadResult, String> {
        let mut watchers = self.watchers.lock().await;
        let mut next_id = self.next_id.lock().await;

        let mut result = ReloadResult::default();

        // Build set of current watched paths
        let current_paths: HashMap<PathBuf, usize> = watchers
            .values()
            .map(|entry| (entry.config.path.clone(), entry.id))
            .collect();

        // Build set of new config paths
        let new_paths: HashMap<PathBuf, &WatchConfig> = new_config
            .watches
            .iter()
            .map(|w| (w.path.clone(), w))
            .collect();

        // Find watchers to remove (in current but not in new config)
        for (path, id) in &current_paths {
            if !new_paths.contains_key(path) {
                if let Some(entry) = watchers.remove(id) {
                    result.removed.push(entry.config.path.clone());
                    info!("Reload: removed watcher[{}] for {}", id, path.display());
                }
            }
        }

        // Find watchers to add (in new config but not in current)
        for (path, config) in &new_paths {
            if !current_paths.contains_key(path) {
                let mut watcher = FsWatcher::new(self.event_tx.clone(), self.debounce_duration)?;
                watcher.add_watch(config)?;

                let id = *next_id;
                *next_id += 1;

                let entry = WatcherEntry {
                    id,
                    _watcher: watcher,
                    config: (*config).clone(),
                };

                watchers.insert(id, entry);
                result.added.push(path.clone());
                info!("Reload: added watcher[{}] for {}", id, path.display());
            }
        }

        // Update config for existing watchers (if settings changed)
        for (path, new_watch_config) in &new_paths {
            if let Some(id) = current_paths.get(path) {
                if let Some(entry) = watchers.get_mut(id) {
                    // Update config (watcher itself doesn't need to change for most settings)
                    entry.config = (*new_watch_config).clone();
                    result.kept.push(path.clone());
                }
            }
        }

        info!(
            "Reload complete: added={}, removed={}, kept={}",
            result.added.len(),
            result.removed.len(),
            result.kept.len()
        );

        Ok(result)
    }
}

/// Information about an active watcher.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WatcherInfo {
    pub id: usize,
    pub path: String,
    pub recursive: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub event_types: Vec<String>,
    pub thread_id: Option<String>,
}

/// Result of a reload operation.
#[derive(Debug, Default)]
pub struct ReloadResult {
    pub added: Vec<PathBuf>,
    pub removed: Vec<PathBuf>,
    pub kept: Vec<PathBuf>,
}
