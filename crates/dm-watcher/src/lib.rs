//! Filesystem monitoring engine for Directory Monitor.
//!
//! Wraps the [`notify`] crate to provide cross-platform filesystem monitoring
//! with debouncing, batch event delivery, and dynamic watcher management.
//!
//! Key types:
//! - [`FsWatcher`] — single-directory watcher with debounce
//! - [`WatcherManager`] — manages multiple watchers with hot-reload support
//! - [`WatchEvent`] — event envelope (single event, batch, or error)

pub mod manager;
pub mod snapshot;
mod watcher;

pub use manager::{ReloadResult, WatcherInfo, WatcherManager};
pub use watcher::{FsWatcher, WatchEvent};
