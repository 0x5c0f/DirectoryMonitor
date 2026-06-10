//! Event processing pipeline for Directory Monitor.
//!
//! Provides filtering, deduplication, and batching of filesystem events:
//! - [`EventFilter`] — glob-based include/exclude filtering with event type matching
//! - [`EventDeduplicator`] — time-window deduplication of identical events
//! - [`EventBatcher`] — count/time-based batching for downstream consumers

pub mod batch;
pub mod dedup;
pub mod filter;

pub use batch::EventBatcher;
pub use dedup::EventDeduplicator;
pub use filter::EventFilter;
