pub mod chain_user_resolver;
pub mod config;
pub mod cursor;
pub mod filters;
pub mod grpc;
pub mod handlers;
pub mod metrics;
pub mod parsers;
pub mod processor;

pub use config::{load_from_env, IndexerConfig};
pub use metrics::{IndexerMetrics, SharedIndexerMetrics};
pub use processor::{spawn_indexer, IndexerHandle};

pub async fn check_startup_status(grpc_url: &str) -> Result<(), String> {
    grpc::client::check_startup_status(grpc_url).await
}
