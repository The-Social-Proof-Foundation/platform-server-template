use platform_core::AppResult;
use sqlx::PgPool;

const CURSOR_KEY: &str = "checkpoint";

pub async fn get_checkpoint_cursor(pool: &PgPool) -> AppResult<Option<i64>> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT last_checkpoint_seq FROM indexer_state WHERE key = $1",
    )
    .bind(CURSOR_KEY)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}

pub async fn set_checkpoint_cursor(pool: &PgPool, seq: i64) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO indexer_state (key, last_checkpoint_seq, updated_at)
         VALUES ($1, $2, NOW())
         ON CONFLICT (key) DO UPDATE SET last_checkpoint_seq = EXCLUDED.last_checkpoint_seq, updated_at = NOW()",
    )
    .bind(CURSOR_KEY)
    .bind(seq)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn record_chain_event(
    pool: &PgPool,
    tx_digest: &str,
    event_index: i32,
    checkpoint_seq: i64,
    event_type: &str,
    payload: serde_json::Value,
) -> AppResult<bool> {
    let result = sqlx::query(
        "INSERT INTO chain_events (tx_digest, event_index, checkpoint_seq, event_type, payload)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (tx_digest, event_index) DO NOTHING",
    )
    .bind(tx_digest)
    .bind(event_index)
    .bind(checkpoint_seq)
    .bind(event_type)
    .bind(payload)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}
