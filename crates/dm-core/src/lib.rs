//! Core types and configuration for Directory Monitor.
//!
//! This crate provides the foundational types shared across all other crates:
//! - [`AppConfig`] — application configuration (watches, server, notifications)
//! - [`FsEvent`] — filesystem event with metadata
//! - [`EventType`] — inotify-compatible event classification
//! - Error types (`WatcherError`, `ConfigError`, `NotificationError`)

pub mod config;
pub mod error;
pub mod event;
pub mod placeholders;

pub use config::AppConfig;
pub use error::DmError;
pub use event::{EventType, FsEvent};
