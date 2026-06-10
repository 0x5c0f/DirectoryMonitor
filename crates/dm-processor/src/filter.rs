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
            Some(
                builder
                    .build()
                    .map_err(|e| format!("Failed to build include set: {e}"))?,
            )
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
            Some(
                builder
                    .build()
                    .map_err(|e| format!("Failed to build exclude set: {e}"))?,
            )
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
    ///
    /// Pattern matching follows standard glob semantics:
    /// - Match against full path: `**/.git/**` excludes files inside .git directories
    /// - Match against filename: `*.txt` matches files with .txt extension
    ///
    /// Note: `.git` only matches a file named exactly `.git`, NOT the .git directory.
    /// To exclude a directory and its contents, use `**/.git/**`.
    pub fn matches(&self, event: &FsEvent) -> bool {
        // Check event type filter
        if !self.event_types.is_empty() && !self.event_types.contains(&event.event_type) {
            return false;
        }

        // Get both full path and filename for matching
        let path_str = event.path.to_string_lossy().to_string();
        let filename = event
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Check exclude patterns (match against both path and filename)
        if let Some(ref exclude) = self.exclude {
            if exclude.is_match(&path_str) || exclude.is_match(&filename) {
                debug!("Excluded by pattern: {}", path_str);
                return false;
            }
        }

        // Check include patterns (match against both path and filename)
        if let Some(ref include) = self.include {
            if !include.is_match(&path_str) && !include.is_match(&filename) {
                debug!("Not matched by include pattern: {}", path_str);
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

#[cfg(test)]
mod tests {
    use super::*;
    use dm_core::event::EventType;
    use std::path::PathBuf;

    fn make_config(
        event_types: Vec<String>,
        include: Vec<String>,
        exclude: Vec<String>,
    ) -> WatchConfig {
        WatchConfig {
            path: PathBuf::from("/test"),
            recursive: true,
            include,
            exclude,
            event_types,
            log_format: None,
            script: None,
            script_mode: "async".to_string(),
            script_events: vec![],
            email_recipients: vec![],
        }
    }

    fn make_event(event_type: EventType, path: &str) -> FsEvent {
        FsEvent::new(event_type, PathBuf::from(path), PathBuf::from("/test"))
    }

    // === Construction ===

    #[test]
    fn test_filter_empty_config() {
        let config = make_config(vec![], vec![], vec![]);
        let filter = EventFilter::from_config(&config).unwrap();
        // Empty config means allow all
        assert!(filter.matches(&make_event(EventType::Created, "/any/file.txt")));
        assert!(filter.matches(&make_event(EventType::Deleted, "/another.log")));
    }

    #[test]
    fn test_filter_invalid_glob_pattern() {
        let config = make_config(vec![], vec![], vec!["[invalid".to_string()]);
        let result = EventFilter::from_config(&config);
        assert!(result.is_err());
    }

    // === Event type filtering ===

    #[test]
    fn test_filter_event_type_created_only() {
        let config = make_config(vec!["create".to_string()], vec![], vec![]);
        let filter = EventFilter::from_config(&config).unwrap();

        assert!(filter.matches(&make_event(EventType::Created, "/file.txt")));
        assert!(!filter.matches(&make_event(EventType::Modified, "/file.txt")));
        assert!(!filter.matches(&make_event(EventType::Deleted, "/file.txt")));
    }

    #[test]
    fn test_filter_event_type_multiple() {
        let config = make_config(
            vec!["create".to_string(), "delete".to_string()],
            vec![],
            vec![],
        );
        let filter = EventFilter::from_config(&config).unwrap();

        assert!(filter.matches(&make_event(EventType::Created, "/file.txt")));
        assert!(filter.matches(&make_event(EventType::Deleted, "/file.txt")));
        assert!(!filter.matches(&make_event(EventType::Modified, "/file.txt")));
    }

    #[test]
    fn test_filter_event_type_closewrite() {
        let config = make_config(vec!["closewrite".to_string()], vec![], vec![]);
        let filter = EventFilter::from_config(&config).unwrap();

        assert!(filter.matches(&make_event(EventType::CloseWrite, "/file.txt")));
        assert!(!filter.matches(&make_event(EventType::Created, "/file.txt")));
    }

    // === Exclude patterns ===

    #[test]
    fn test_filter_exclude_git_directory() {
        let config = make_config(vec![], vec![], vec!["**/.git/**".to_string()]);
        let filter = EventFilter::from_config(&config).unwrap();

        assert!(!filter.matches(&make_event(EventType::Created, "/project/.git/objects/abc")));
        assert!(!filter.matches(&make_event(EventType::Modified, "/project/.git/config")));
        assert!(filter.matches(&make_event(EventType::Created, "/project/src/main.rs")));
    }

    #[test]
    fn test_filter_exclude_tmp_files() {
        let config = make_config(vec![], vec![], vec!["*.tmp".to_string()]);
        let filter = EventFilter::from_config(&config).unwrap();

        assert!(!filter.matches(&make_event(EventType::Created, "/data.tmp")));
        assert!(!filter.matches(&make_event(EventType::Created, "/path/to/file.tmp")));
        assert!(filter.matches(&make_event(EventType::Created, "/file.txt")));
    }

    #[test]
    fn test_filter_exclude_node_modules() {
        let config = make_config(vec![], vec![], vec!["**/node_modules/**".to_string()]);
        let filter = EventFilter::from_config(&config).unwrap();

        assert!(!filter.matches(&make_event(
            EventType::Created,
            "/project/node_modules/pkg/index.js"
        )));
        assert!(filter.matches(&make_event(EventType::Created, "/project/src/index.js")));
    }

    // === Include patterns ===

    #[test]
    fn test_filter_include_txt_only() {
        let config = make_config(vec![], vec!["*.txt".to_string()], vec![]);
        let filter = EventFilter::from_config(&config).unwrap();

        assert!(filter.matches(&make_event(EventType::Created, "/file.txt")));
        assert!(filter.matches(&make_event(EventType::Created, "/path/to/doc.txt")));
        assert!(!filter.matches(&make_event(EventType::Created, "/file.rs")));
        assert!(!filter.matches(&make_event(EventType::Created, "/file.log")));
    }

    #[test]
    fn test_filter_include_multiple_patterns() {
        let config = make_config(vec![], vec!["*.{rs,toml}".to_string()], vec![]);
        let filter = EventFilter::from_config(&config).unwrap();

        assert!(filter.matches(&make_event(EventType::Created, "/main.rs")));
        assert!(filter.matches(&make_event(EventType::Created, "/Cargo.toml")));
        assert!(!filter.matches(&make_event(EventType::Created, "/file.txt")));
    }

    // === Include + Exclude interaction ===

    #[test]
    fn test_filter_exclude_overrides_include() {
        let config = make_config(
            vec![],
            vec!["*.txt".to_string()],
            vec!["secret.txt".to_string()],
        );
        let filter = EventFilter::from_config(&config).unwrap();

        assert!(filter.matches(&make_event(EventType::Created, "/readme.txt")));
        assert!(!filter.matches(&make_event(EventType::Created, "/secret.txt")));
    }

    // === Matches both path and filename ===

    #[test]
    fn test_filter_matches_full_path() {
        let config = make_config(vec![], vec!["**/src/**/*.rs".to_string()], vec![]);
        let filter = EventFilter::from_config(&config).unwrap();

        assert!(filter.matches(&make_event(EventType::Created, "/project/src/main.rs")));
        assert!(filter.matches(&make_event(EventType::Created, "/project/src/lib/mod.rs")));
        assert!(!filter.matches(&make_event(EventType::Created, "/project/tests/test.rs")));
    }

    #[test]
    fn test_filter_matches_filename_glob() {
        // "*.txt" should match by filename, not full path
        let config = make_config(vec![], vec!["*.txt".to_string()], vec![]);
        let filter = EventFilter::from_config(&config).unwrap();

        assert!(filter.matches(&make_event(EventType::Created, "/any/path/file.txt")));
        assert!(filter.matches(&make_event(EventType::Created, "/file.txt")));
        assert!(!filter.matches(&make_event(EventType::Created, "/file.rs")));
    }

    // === filter_events batch ===

    #[test]
    fn test_filter_events_batch() {
        let config = make_config(vec!["create".to_string()], vec![], vec![]);
        let filter = EventFilter::from_config(&config).unwrap();

        let events = vec![
            make_event(EventType::Created, "/a.txt"),
            make_event(EventType::Modified, "/b.txt"),
            make_event(EventType::Created, "/c.txt"),
            make_event(EventType::Deleted, "/d.txt"),
        ];

        let filtered = filter.filter_events(events);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].event_type, EventType::Created);
        assert_eq!(filtered[1].event_type, EventType::Created);
    }

    // === Edge cases ===

    #[test]
    fn test_filter_event_type_case_insensitive() {
        let config = make_config(vec!["CREATE".to_string()], vec![], vec![]);
        let filter = EventFilter::from_config(&config).unwrap();

        assert!(filter.matches(&make_event(EventType::Created, "/file.txt")));
    }

    #[test]
    fn test_filter_unknown_event_type_string() {
        let config = make_config(vec!["unknown_type".to_string()], vec![], vec![]);
        let filter = EventFilter::from_config(&config).unwrap();

        // Unknown type string is ignored, so event_types is empty → allow all
        assert!(filter.matches(&make_event(EventType::Created, "/file.txt")));
    }
}
