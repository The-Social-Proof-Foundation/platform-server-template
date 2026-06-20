use platform_core::AppResult;
use serde_json::Value;
use sqlx::FromRow;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn insert_outbox_event(
    pool: &PgPool,
    topic: &str,
    event_type: &str,
    payload: Value,
    idempotency_key: Option<&str>,
) -> AppResult<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO analytics_outbox (id, topic, event_type, payload, idempotency_key)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (idempotency_key) DO NOTHING",
    )
    .bind(id)
    .bind(topic)
    .bind(event_type)
    .bind(payload)
    .bind(idempotency_key)
    .execute(pool)
    .await?;
    Ok(id)
}

#[derive(Debug, FromRow)]
pub struct OutboxRow {
    pub id: Uuid,
    pub topic: String,
    pub event_type: String,
    pub payload: Value,
}

pub async fn fetch_unpublished_outbox(pool: &PgPool, limit: i64) -> AppResult<Vec<OutboxRow>> {
    let rows = sqlx::query_as::<_, OutboxRow>(
        "SELECT id, topic, event_type, payload
         FROM analytics_outbox
         WHERE published_at IS NULL
         ORDER BY created_at ASC
         LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn mark_outbox_published(pool: &PgPool, id: Uuid) -> AppResult<()> {
    sqlx::query("UPDATE analytics_outbox SET published_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
