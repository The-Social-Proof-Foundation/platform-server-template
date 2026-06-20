use std::sync::Arc;

use platform_analytics::RedpandaProducer;
use platform_core::{AppState, SharedIndexerMetrics, SharedPlatformMetrics};
use platform_db::CounterFlushManager;
use platform_embeddings::EmbeddingService;
use platform_notify::NotificationService;

use crate::mysocial::MySocialClient;

#[derive(Clone)]
pub struct ApiState {
    pub inner: AppState,
    pub notify: NotificationService,
    pub counters: CounterFlushManager,
    pub indexer_metrics: SharedIndexerMetrics,
    pub metrics: SharedPlatformMetrics,
    pub redpanda: Option<RedpandaProducer>,
    pub embeddings: Arc<EmbeddingService>,
    pub mysocial: Option<MySocialClient>,
}

impl ApiState {
    pub fn new(
        inner: AppState,
        notify: NotificationService,
        counters: CounterFlushManager,
        metrics: SharedPlatformMetrics,
        redpanda: Option<RedpandaProducer>,
        embeddings: EmbeddingService,
        mysocial: Option<MySocialClient>,
    ) -> Self {
        let indexer_metrics = metrics.indexer.clone();
        Self {
            inner,
            notify,
            counters,
            indexer_metrics,
            metrics,
            redpanda,
            embeddings: Arc::new(embeddings),
            mysocial,
        }
    }

    pub fn config(&self) -> &platform_core::Config {
        &self.inner.config
    }

    pub fn pg(&self) -> &sqlx::PgPool {
        &self.inner.pg_pool
    }

    pub fn pg_read(&self) -> &sqlx::PgPool {
        &self.inner.pg_read_pool
    }

    pub fn redis(&self) -> redis::aio::ConnectionManager {
        self.inner.redis.clone()
    }
}

pub type SharedApiState = Arc<ApiState>;
