use std::sync::Arc;
use std::time::Duration;

use platform_core::{AppResult, Config};
use platform_db::{fetch_unpublished_outbox, mark_outbox_published};
use rdkafka::config::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use sqlx::PgPool;
use tracing::{info, warn};

pub type RedpandaProducer = Arc<FutureProducer>;

pub fn create_producer(config: &Config) -> AppResult<Option<RedpandaProducer>> {
    if !config.analytics_enabled {
        return Ok(None);
    }
    let Some(brokers) = config.redpanda_brokers.as_ref() else {
        return Ok(None);
    };

    let mut client_config = ClientConfig::new();
    client_config
        .set("bootstrap.servers", brokers)
        .set("message.timeout.ms", "5000")
        .set("acks", "all")
        .set("retries", "3");

    if config.redpanda_ssl_enabled {
        client_config.set("security.protocol", "ssl");
    }

    let producer: FutureProducer = client_config
        .create()
        .map_err(|e| platform_core::AppError::Internal(format!("Redpanda producer error: {e}")))?;

    Ok(Some(Arc::new(producer)))
}

pub async fn publish_json(
    producer: &FutureProducer,
    topic: &str,
    key: &str,
    payload: &serde_json::Value,
) -> AppResult<()> {
    let body = payload.to_string();
    producer
        .send(
            FutureRecord::to(topic).key(key).payload(&body),
            Duration::from_secs(5),
        )
        .await
        .map_err(|(err, _)| platform_core::AppError::Internal(format!("Redpanda publish failed: {err}")))?;
    Ok(())
}

pub fn spawn_outbox_poller(pool: PgPool, producer: RedpandaProducer) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            interval.tick().await;
            if let Err(err) = poll_once(&pool, &producer).await {
                warn!(error = %err, "outbox poll failed");
            }
        }
    });
}

async fn poll_once(pool: &PgPool, producer: &FutureProducer) -> AppResult<()> {
    let rows = fetch_unpublished_outbox(pool, 100).await?;
    for row in rows {
        let payload = serde_json::json!({
            "eventType": row.event_type,
            "payload": row.payload,
        });
        publish_json(producer, &row.topic, &row.id.to_string(), &payload).await?;
        mark_outbox_published(pool, row.id).await?;
    }
    Ok(())
}

pub async fn ensure_clickhouse_schema(config: &Config) -> AppResult<()> {
    if !config.clickhouse_ingest_enabled {
        return Ok(());
    }
    info!("clickhouse ingest enabled for database {}", config.clickhouse_database);
    Ok(())
}
