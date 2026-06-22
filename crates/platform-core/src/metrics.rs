use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use prometheus::{
    register_counter_vec_with_registry, register_counter_with_registry,
    register_gauge_vec_with_registry, register_gauge_with_registry,
    register_histogram_vec_with_registry, Counter, CounterVec, Encoder, Gauge, HistogramVec,
    Registry, TextEncoder,
};
use sqlx::PgPool;

#[derive(Debug)]
pub struct IndexerMetrics {
    pub checkpoints_processed: AtomicU64,
    pub events_processed: AtomicU64,
    pub events_skipped: AtomicU64,
    pub last_checkpoint_seq: Mutex<Option<String>>,
    pub last_error: Mutex<Option<String>>,
    prom_checkpoints: Counter,
    prom_events_processed: Counter,
    prom_events_skipped: Counter,
    prom_errors: Counter,
    prom_last_checkpoint: Gauge,
}

impl IndexerMetrics {
    fn register(registry: &Registry) -> Self {
        Self {
            checkpoints_processed: AtomicU64::new(0),
            events_processed: AtomicU64::new(0),
            events_skipped: AtomicU64::new(0),
            last_checkpoint_seq: Mutex::new(None),
            last_error: Mutex::new(None),
            prom_checkpoints: register_counter_with_registry!(
                "indexer_checkpoints_processed_total",
                "Total checkpoints processed by the indexer",
                registry
            )
            .expect("register indexer_checkpoints_processed_total"),
            prom_events_processed: register_counter_with_registry!(
                "indexer_events_processed_total",
                "Total chain events processed by the indexer",
                registry
            )
            .expect("register indexer_events_processed_total"),
            prom_events_skipped: register_counter_with_registry!(
                "indexer_events_skipped_total",
                "Total chain events skipped by the indexer",
                registry
            )
            .expect("register indexer_events_skipped_total"),
            prom_errors: register_counter_with_registry!(
                "indexer_errors_total",
                "Total indexer handler errors",
                registry
            )
            .expect("register indexer_errors_total"),
            prom_last_checkpoint: register_gauge_with_registry!(
                "indexer_last_checkpoint_sequence",
                "Last checkpoint sequence processed by the indexer",
                registry
            )
            .expect("register indexer_last_checkpoint_sequence"),
        }
    }

    pub fn inc_events_skipped(&self) {
        self.events_skipped.fetch_add(1, Ordering::Relaxed);
        self.prom_events_skipped.inc();
    }

    pub fn inc_events_processed(&self) {
        self.events_processed.fetch_add(1, Ordering::Relaxed);
        self.prom_events_processed.inc();
    }

    pub fn inc_checkpoint(&self, seq: u64) {
        self.checkpoints_processed.fetch_add(1, Ordering::Relaxed);
        self.prom_checkpoints.inc();
        self.prom_last_checkpoint.set(seq as f64);
        if let Ok(mut guard) = self.last_checkpoint_seq.lock() {
            *guard = Some(seq.to_string());
        }
    }

    pub fn record_error(&self, message: impl Into<String>) {
        let msg = message.into();
        self.prom_errors.inc();
        if let Ok(mut guard) = self.last_error.lock() {
            *guard = Some(msg);
        }
    }

    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "checkpointsProcessed": self.checkpoints_processed.load(Ordering::Relaxed),
            "eventsProcessed": self.events_processed.load(Ordering::Relaxed),
            "eventsSkipped": self.events_skipped.load(Ordering::Relaxed),
            "lastCheckpointSeq": self.last_checkpoint_seq.lock().ok().and_then(|g| g.clone()),
            "lastError": self.last_error.lock().ok().and_then(|g| g.clone()),
        })
    }
}

impl Default for IndexerMetrics {
    fn default() -> Self {
        let registry = Registry::new();
        Self::register(&registry)
    }
}

pub type SharedIndexerMetrics = Arc<IndexerMetrics>;

#[derive(Clone)]
pub struct PlatformMetrics {
    pub registry: Registry,
    pub indexer: SharedIndexerMetrics,
    http_requests_total: CounterVec,
    http_request_duration_seconds: HistogramVec,
    ws_connections_active: Gauge,
    notifications_delivered_total: CounterVec,
    analytics_outbox_published_total: Counter,
    analytics_outbox_publish_errors_total: Counter,
    counter_flush_errors_total: Counter,
    pg_pool_connections_active: Gauge,
    pg_pool_connections_idle: Gauge,
    pg_read_pool_connections_active: Gauge,
    pg_read_pool_connections_idle: Gauge,
    search_requests_total: Counter,
}

impl PlatformMetrics {
    pub fn new(environment: &str, version: &str) -> Self {
        let registry = Registry::new_custom(Some("platform".into()), None)
            .expect("create prometheus registry");

        let build_info = register_gauge_vec_with_registry!(
            "build_info",
            "Platform server build information",
            &["version", "environment"],
            registry
        )
        .expect("register build_info");
        build_info
            .with_label_values(&[version, environment])
            .set(1.0);

        let indexer = Arc::new(IndexerMetrics::register(&registry));

        let http_requests_total = register_counter_vec_with_registry!(
            "http_requests_total",
            "Total HTTP requests",
            &["method", "route", "status"],
            registry
        )
        .expect("register http_requests_total");

        let http_request_duration_seconds = register_histogram_vec_with_registry!(
            "http_request_duration_seconds",
            "HTTP request duration in seconds",
            &["method", "route"],
            registry
        )
        .expect("register http_request_duration_seconds");

        let ws_connections_active = register_gauge_with_registry!(
            "ws_connections_active",
            "Active WebSocket connections",
            registry
        )
        .expect("register ws_connections_active");

        let notifications_delivered_total = register_counter_vec_with_registry!(
            "notifications_delivered_total",
            "Notifications delivered by channel",
            &["channel"],
            registry
        )
        .expect("register notifications_delivered_total");

        let analytics_outbox_published_total = register_counter_with_registry!(
            "analytics_outbox_published_total",
            "Analytics outbox rows published to Redpanda",
            registry
        )
        .expect("register analytics_outbox_published_total");

        let analytics_outbox_publish_errors_total = register_counter_with_registry!(
            "analytics_outbox_publish_errors_total",
            "Analytics outbox publish failures",
            registry
        )
        .expect("register analytics_outbox_publish_errors_total");

        let counter_flush_errors_total = register_counter_with_registry!(
            "counter_flush_errors_total",
            "Sharded counter flush failures",
            registry
        )
        .expect("register counter_flush_errors_total");

        let pg_pool_connections_active = register_gauge_with_registry!(
            "pg_pool_connections_active",
            "Active Postgres pool connections",
            registry
        )
        .expect("register pg_pool_connections_active");

        let pg_pool_connections_idle = register_gauge_with_registry!(
            "pg_pool_connections_idle",
            "Idle Postgres pool connections",
            registry
        )
        .expect("register pg_pool_connections_idle");

        let pg_read_pool_connections_active = register_gauge_with_registry!(
            "pg_read_pool_connections_active",
            "Active Postgres read pool connections",
            registry
        )
        .expect("register pg_read_pool_connections_active");

        let pg_read_pool_connections_idle = register_gauge_with_registry!(
            "pg_read_pool_connections_idle",
            "Idle Postgres read pool connections",
            registry
        )
        .expect("register pg_read_pool_connections_idle");

        let search_requests_total = register_counter_with_registry!(
            "search_requests_total",
            "Total authenticated search requests served",
            registry
        )
        .expect("register search_requests_total");

        Self {
            registry,
            indexer,
            http_requests_total,
            http_request_duration_seconds,
            ws_connections_active,
            notifications_delivered_total,
            analytics_outbox_published_total,
            analytics_outbox_publish_errors_total,
            counter_flush_errors_total,
            pg_pool_connections_active,
            pg_pool_connections_idle,
            pg_read_pool_connections_active,
            pg_read_pool_connections_idle,
            search_requests_total,
        }
    }

    pub fn observe_http_request(&self, method: &str, route: &str, status: u16, duration: Duration) {
        let status = status.to_string();
        self.http_requests_total
            .with_label_values(&[method, route, &status])
            .inc();
        self.http_request_duration_seconds
            .with_label_values(&[method, route])
            .observe(duration.as_secs_f64());
    }

    pub fn set_ws_connections(&self, count: usize) {
        self.ws_connections_active.set(count as f64);
    }

    pub fn inc_notification_delivered(&self, channel: &str) {
        self.notifications_delivered_total
            .with_label_values(&[channel])
            .inc();
    }

    pub fn inc_outbox_published(&self) {
        self.analytics_outbox_published_total.inc();
    }

    pub fn inc_outbox_publish_error(&self) {
        self.analytics_outbox_publish_errors_total.inc();
    }

    pub fn inc_counter_flush_error(&self) {
        self.counter_flush_errors_total.inc();
    }

    pub fn inc_search_request(&self) {
        self.search_requests_total.inc();
    }

    pub fn update_pg_pool_stats(&self, pool: &PgPool, read_pool: &PgPool) {
        self.pg_pool_connections_active.set(pool.size() as f64);
        self.pg_pool_connections_idle.set(pool.num_idle() as f64);
        self.pg_read_pool_connections_active
            .set(read_pool.size() as f64);
        self.pg_read_pool_connections_idle
            .set(read_pool.num_idle() as f64);
    }

    pub fn gather(&self) -> String {
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        TextEncoder::new()
            .encode(&metric_families, &mut buffer)
            .expect("encode prometheus metrics");
        String::from_utf8(buffer).expect("prometheus metrics utf8")
    }
}

pub type SharedPlatformMetrics = Arc<PlatformMetrics>;

pub fn spawn_pg_pool_metrics_task(
    metrics: SharedPlatformMetrics,
    pool: PgPool,
    read_pool: PgPool,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(15));
        loop {
            interval.tick().await;
            metrics.update_pg_pool_stats(&pool, &read_pool);
        }
    });
}
