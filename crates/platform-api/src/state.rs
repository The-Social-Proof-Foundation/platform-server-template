use std::sync::Arc;

use platform_analytics::RedpandaProducer;
use platform_core::AppState;
use platform_db::CounterFlushManager;
use platform_core::SharedIndexerMetrics;
use platform_notify::NotificationService;

#[derive(Clone)]
pub struct ApiState {
    pub inner: AppState,
    pub notify: NotificationService,
    pub counters: CounterFlushManager,
    pub indexer_metrics: SharedIndexerMetrics,
    pub redpanda: Option<RedpandaProducer>,
}

impl ApiState {
    pub fn new(
        inner: AppState,
        notify: NotificationService,
        counters: CounterFlushManager,
        indexer_metrics: SharedIndexerMetrics,
        redpanda: Option<RedpandaProducer>,
    ) -> Self {
        Self {
            inner,
            notify,
            counters,
            indexer_metrics,
            redpanda,
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
