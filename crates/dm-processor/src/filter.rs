use dm_core::config::WatchConfig;
use dm_core::event::{EventType, FsEvent};
use globset::{Glob, GlobSet, GlobSetBuilder};
use tracing::debug;

/// Filters events based on include/exclude glob patterns and event types.
pub struct EventFilter {
    /// Glob patterns to include (if empty, all paths match).
    include: Option<GlobSet>,
    /// Glob patterns to exclude.
    exclude: Option<GlobSet>,
    /// Allowed event types (if empty, all types match).
    event_types: Vec<EventType>,
}

impl EventFilter {
    /// Create a filter from a WatchConfig.
    pub fn from_config(config: &WatchConfig) -> Result<Self, String> {
        let include = if config.include.is_empty() {
            None
        } else {
            let mut builder = GlobSetBuilder::new();
            for pattern in &config.include {
                let glob = Glob::new(pattern)
                    .map_err(|e| format!("Invalid include pattern '{}': {e}", pattern))?;
                builder.add(glob);
            }
            Some(builder.build().map_err(|e| format!("Failed to build include set: {e}"))?)
        };

        let exclude = if config.exclude.is_empty() {
            None
        } else {
            let mut builder = GlobSetBuilder::new();
            for pattern in &config.exclude {
                let glob = Glob::new(pattern)
                    .map_err(|e| format!("Invalid exclude pattern '{}': {e}", pattern))?;
                builder.add(glob);
            }
            Some(builder.build().map_err(|e| format!("Failed to build exclude set: {e}"))?)
        };

        let event_types: Vec<EventType> = config
            .event_types
            .iter()
            .filter_map(|s| match s.to_lowercase().as_str() {
                "created" | "create" => Some(EventType::Created),
                "modified" | "modify" => Some(EventType::Modified),
                "attrib" => Some(EventType::Attrib),
                "close_write" | "closewrite" => Some(EventType::CloseWrite),
                "close_nowrite" | "closenowrite" | "close" => Some(EventType::CloseNoWrite),
                "open" | "opened" => Some(EventType::Opened),
                "moved_to" | "movedto" => Some(EventType::MovedTo),
                "moved_from" | "movedfrom" => Some(EventType::MovedFrom),
                "deleted" | "delete" | "remove" => Some(EventType::Deleted),
                "renamed" | "rename" => Some(EventType::Renamed),
                "accessed" | "access" => Some(EventType::Accessed),
                _ => {
                    debug!("Unknown event type in config: {}", s);
                    None
                }
            })
            .collect();

        Ok(Self {
            include,
            exclude,
            event_types,
        })
    }

    /// Check if an event passes this filter.
    /// Glob patterns match against the filename (not the full path),
    /// so `*.txt` correctly matches `/some/dir/file.txt`.
    pub fn matches(&self, event: &FsEvent) -> bool {
        // Check event type filter
        if !self.event_types.is_empty() && !self.event_types.contains(&event.event_type) {
            return false;
        }

        // Get filename for glob matching
        let filename = event
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Check exclude patterns
        if let Some(ref exclude) = self.exclude {
            if exclude.is_match(&filename) {
                debug!("Excluded by pattern: {}", filename);
                return false;
            }
        }

        // Check include patterns
        if let Some(ref include) = self.include {
            if !include.is_match(&filename) {
                debug!("Not matched by include pattern: {}", filename);
                return false;
            }
        }

        true
    }

    /// Filter a batch of events, returning only those that pass.
    pub fn filter_events(&self, events: Vec<FsEvent>) -> Vec<FsEvent> {
        events.into_iter().filter(|e| self.matches(e)).collect()
    }
}
