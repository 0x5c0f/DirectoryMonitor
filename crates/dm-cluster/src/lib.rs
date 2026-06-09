pub mod discovery;
pub mod grpc;
pub mod peer;
pub mod peer_manager;
pub mod query;
pub mod sync;
pub mod tls;
pub mod types;

// Re-export key types
pub use peer::{NodeInfo, NodeRegistry, NodeStatus};
pub use peer_manager::PeerManager;
pub use query::ClusterQueryAggregator;
pub use sync::{EventCache, EventSyncService, HeartbeatService};
pub use types::{ClusterEvent, NodeHeartbeat};
