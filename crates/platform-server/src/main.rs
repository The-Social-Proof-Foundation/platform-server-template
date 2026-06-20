use std::net::SocketAddr;
use std::sync::Arc;

use platform_analytics::{create_producer, ensure_clickhouse_schema, spawn_outbox_poller};
use platform_api::{build_router, ApiState};
use platform_core::{AppState, Config};
use platform_db::{default_migrations_dir, run_migrations, CounterFlushManager};
use platform_indexer::{load_from_env, spawn_indexer, IndexerMetrics};
use platform_notify::NotificationService;
use tokio::signal;
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::from_env()?;
    let app_state = AppState::new(config.clone()).await?;
    run_migrations(&app_state.pg_pool, default_migrations_dir()).await?;

    let notify = NotificationService::new(&config)?;
    let counters = CounterFlushManager::new(app_state.pg_pool.clone());
    counters.clone().spawn_flush_task(app_state.redis.clone());

    let indexer_metrics = Arc::new(IndexerMetrics::default());
    let redpanda = create_producer(&config)?;
    if let Some(producer) = redpanda.clone() {
        spawn_outbox_poller(app_state.pg_pool.clone(), producer);
    }
    ensure_clickhouse_schema(&config).await?;

    let api_state = ApiState::new(
        app_state.clone(),
        notify.clone(),
        counters.clone(),
        indexer_metrics.clone(),
        redpanda,
    );

    let mut indexer_handle = None;
    if let Some(indexer_config) = load_from_env()? {
        if let Some(grpc_url) = config.myso_grpc_url.clone() {
            if let Err(err) = platform_indexer::check_startup_status(&grpc_url).await {
                error!(error = %err, "grpc startup probe failed");
            }
        }
        indexer_handle = Some(spawn_indexer(
            app_state.pg_pool.clone(),
            app_state.redis.clone(),
            notify,
            indexer_config,
            indexer_metrics,
        ));
    }

    let router = build_router(Arc::new(api_state));
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    info!(%addr, "platform-server listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    if let Some(handle) = indexer_handle {
        handle.stop();
    }
    counters.shutdown_flush(app_state.redis.clone()).await?;

    Ok(())
}

async fn shutdown_signal() {
    let _ = signal::ctrl_c().await;
    info!("shutdown signal received");
}
