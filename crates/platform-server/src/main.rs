use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use platform_analytics::{create_producer, ensure_clickhouse_schema, spawn_outbox_poller};
use platform_api::{build_metrics_router, build_router, spawn_waitlist_processor, ApiState};
use platform_api::mysocial::MySocialClient;
use platform_core::{spawn_pg_pool_metrics_task, AppState, Config, PlatformMetrics};
use platform_db::{default_migrations_dir, run_migrations, CounterFlushManager};
use platform_embeddings::EmbeddingService;
use platform_indexer::{load_from_env, spawn_indexer};
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

    let metrics = Arc::new(PlatformMetrics::new(
        &config.environment,
        env!("CARGO_PKG_VERSION"),
    ));
    spawn_pg_pool_metrics_task(
        metrics.clone(),
        app_state.pg_pool.clone(),
        app_state.pg_read_pool.clone(),
    );

    let notify = NotificationService::new(&config, Some(metrics.clone()))?;
    let embeddings = EmbeddingService::from_config(&config)?;
    let mysocial = MySocialClient::new(config.myso_graphql_url.clone());
    let counters = CounterFlushManager::new(app_state.pg_pool.clone(), Some(metrics.clone()));
    counters.clone().spawn_flush_task(app_state.redis.clone());

    let redpanda = create_producer(&config)?;
    if let Some(producer) = redpanda.clone() {
        spawn_outbox_poller(
            app_state.pg_pool.clone(),
            producer,
            Some(metrics.clone()),
        );
    }
    ensure_clickhouse_schema(&config).await?;

    let api_state = ApiState::new(
        app_state.clone(),
        notify.clone(),
        counters.clone(),
        metrics.clone(),
        redpanda,
        embeddings.clone(),
        mysocial,
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
            Some(embeddings),
            indexer_config,
            metrics.indexer.clone(),
        ));
    }

    let shared_state = Arc::new(api_state);
    spawn_waitlist_processor(shared_state.clone());

    if config.metrics_enabled {
        let bind: IpAddr = config
            .metrics_bind
            .parse()
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
        let metrics_port = config.metrics_port;
        let metrics_router = build_metrics_router(metrics.clone());
        tokio::spawn(async move {
            let addr = SocketAddr::new(bind, metrics_port);
            match tokio::net::TcpListener::bind(addr).await {
                Ok(listener) => {
                    info!(%addr, "metrics server listening");
                    if let Err(err) = axum::serve(listener, metrics_router).await {
                        error!(error = %err, "metrics server failed");
                    }
                }
                Err(err) => error!(error = %err, %addr, "failed to bind metrics server"),
            }
        });
    }

    let router = build_router(shared_state, metrics);
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
