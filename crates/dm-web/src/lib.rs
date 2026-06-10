//! Web dashboard and REST API for Directory Monitor.
//!
//! Provides an Axum-based web server with:
//! - REST API for configuration, events, metrics, and watcher management
//! - WebSocket endpoint for real-time event streaming
//! - Token-based authentication with constant-time comparison
//! - Embedded single-page frontend

mod auth;
mod frontend;
mod hub;
mod routes;
pub mod server;

pub use hub::EventPayload;
pub use server::{build_router, run_server, AppState};
