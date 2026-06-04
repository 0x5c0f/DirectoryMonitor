use dm_core::event::{EventType, FsEvent};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::{debug, info};

/// Metadata for a single file in a snapshot.
#[derive(Debug, Clone)]
pub struct FileMeta {
    pub path: PathBuf,
    pub modified: Option<SystemTime>,
    pub size: u64,
    pub is_dir: bool,
}

/// A point-in-time snapshot of a directory tree.
/// Used to detect changes that occurred while monitoring was offline.
#[derive(Debug, Clone)]
pub struct DirectorySnapshot {
    /// Path -> metadata mapping.
    pub files: HashMap<PathBuf, FileMeta>,
    /// When the snapshot was taken.
    pub taken_at: chrono::DateTime<chrono::Utc>,
}

impl DirectorySnapshot {
    /// Create a new snapshot of the given directory.
    pub fn new(root: &Path, recursive: bool) -> std::io::Result<Self> {
        let mut files = HashMap::new();
        Self::scan_directory(root, recursive, &mut files)?;
        info!("Snapshot of {} taken: {} entries", root.display(), files.len());
        Ok(Self {
            files,
            taken_at: chrono::Utc::now(),
        })
    }

    fn scan_directory(
        dir: &Path,
        recursive: bool,
        files: &mut HashMap<PathBuf, FileMeta>,
    ) -> std::io::Result<()> {
        let entries = std::fs::read_dir(dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let metadata = entry.metadata()?;

            let meta = FileMeta {
                path: path.clone(),
                modified: metadata.modified().ok(),
                size: metadata.len(),
                is_dir: metadata.is_dir(),
            };
            files.insert(path.clone(), meta);

            if recursive && metadata.is_dir() {
                Self::scan_directory(&path, true, files)?;
            }
        }
        Ok(())
    }

    /// Compare this snapshot with a newer one and produce events for differences.
    /// This is used to detect changes during network outages or power failures.
    pub fn diff(&self, newer: &DirectorySnapshot, watch_root: &Path) -> Vec<FsEvent> {
        let mut events = Vec::new();

        // Find new and modified files
        for (path, new_meta) in &newer.files {
            match self.files.get(path) {
                None => {
                    // File is new
                    debug!("New file detected: {}", path.display());
                    events.push(FsEvent::new(
                        EventType::Created,
                        path.clone(),
                        watch_root.to_path_buf(),
                    ));
                }
                Some(old_meta) => {
                    // Check if modified
                    if old_meta.modified != new_meta.modified || old_meta.size != new_meta.size {
                        debug!("Modified file detected: {}", path.display());
                        events.push(FsEvent::new(
                            EventType::Modified,
                            path.clone(),
                            watch_root.to_path_buf(),
                        ));
                    }
                }
            }
        }

        // Find deleted files
        for path in self.files.keys() {
            if !newer.files.contains_key(path) {
                debug!("Deleted file detected: {}", path.display());
                events.push(FsEvent::new(
                    EventType::Deleted,
                    path.clone(),
                    watch_root.to_path_buf(),
                ));
            }
        }

        info!("Snapshot diff produced {} events", events.len());
        events
    }
}
