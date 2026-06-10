//! Notification backends for Directory Monitor.
//!
//! Supports multiple notification channels for filesystem events:
//! - [`EmailNotifier`] — SMTP email notifications with batching and rate limiting
//! - [`SyslogNotifier`] — RFC 3164/5424 syslog messages via UDP
//! - [`ScriptExecutor`] — shell script execution (async or sync)

pub mod email;
pub mod script;
pub mod syslog;

pub use email::EmailNotifier;
pub use script::ScriptExecutor;
pub use syslog::SyslogNotifier;
