use platform_core::AppResult;
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

pub const MAX_SEARCH_HISTORY_PER_USER: i64 = 50;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SearchHistoryRow {
    pub search_history_id: Uuid,
    pub query: String,
    pub filter_types: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub async fn record_search(
    pool: &PgPool,
    user_id: &str,
    query: &str,
    filter_types: Option<&str>,
) -> AppResult<SearchHistoryRow> {
    let query = query.trim();
    if query.is_empty() {
        return Err(platform_core::AppError::BadRequest(
            "query must not be empty".into(),
        ));
    }

    let row: SearchHistoryRow = sqlx::query_as(
        "INSERT INTO search_history (user_id, query, filter_types)
         VALUES ($1::uuid, $2, $3)
         ON CONFLICT (user_id, query_key)
         DO UPDATE SET
             query = EXCLUDED.query,
             filter_types = COALESCE(EXCLUDED.filter_types, search_history.filter_types),
             updated_at = NOW()
         RETURNING search_history_id, query, filter_types, created_at, updated_at",
    )
    .bind(user_id)
    .bind(query)
    .bind(filter_types)
    .fetch_one(pool)
    .await?;

    prune_search_history(pool, user_id, MAX_SEARCH_HISTORY_PER_USER).await?;

    Ok(row)
}

pub async fn list_search_history(
    pool: &PgPool,
    user_id: &str,
    limit: i64,
) -> AppResult<Vec<SearchHistoryRow>> {
    let limit = limit.clamp(1, MAX_SEARCH_HISTORY_PER_USER);

    Ok(sqlx::query_as(
        "SELECT search_history_id, query, filter_types, created_at, updated_at
         FROM search_history
         WHERE user_id = $1::uuid
         ORDER BY updated_at DESC
         LIMIT $2",
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

pub async fn delete_search_history_entry(
    pool: &PgPool,
    user_id: &str,
    query: &str,
) -> AppResult<bool> {
    let query = query.trim();
    if query.is_empty() {
        return Err(platform_core::AppError::BadRequest(
            "query must not be empty".into(),
        ));
    }

    let result = sqlx::query(
        "DELETE FROM search_history
         WHERE user_id = $1::uuid AND query_key = lower(btrim($2))",
    )
    .bind(user_id)
    .bind(query)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn clear_search_history(pool: &PgPool, user_id: &str) -> AppResult<u64> {
    let result = sqlx::query("DELETE FROM search_history WHERE user_id = $1::uuid")
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

async fn prune_search_history(pool: &PgPool, user_id: &str, keep: i64) -> AppResult<()> {
    sqlx::query(
        "DELETE FROM search_history
         WHERE user_id = $1::uuid
           AND search_history_id NOT IN (
             SELECT search_history_id
             FROM search_history
             WHERE user_id = $1::uuid
             ORDER BY updated_at DESC
             LIMIT $2
           )",
    )
    .bind(user_id)
    .bind(keep)
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_history_cap_is_reasonable() {
        assert_eq!(MAX_SEARCH_HISTORY_PER_USER, 50);
    }
}
