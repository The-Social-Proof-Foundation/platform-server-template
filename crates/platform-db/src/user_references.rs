use platform_core::AppResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct UserReferenceRow {
    pub reference_id: Uuid,
    pub reference_type: String,
    pub reference_key: String,
    pub metadata: Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_references(
    pool: &PgPool,
    user_id: &str,
    reference_type: Option<&str>,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<UserReferenceRow>> {
    let limit = limit.clamp(1, 200);
    let offset = offset.max(0);

    if let Some(reference_type) = reference_type {
        Ok(sqlx::query_as(
            "SELECT reference_id, reference_type, reference_key, metadata, created_at, updated_at
             FROM user_references
             WHERE user_id = $1::uuid AND reference_type = $2
             ORDER BY created_at DESC
             LIMIT $3 OFFSET $4",
        )
        .bind(user_id)
        .bind(reference_type)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?)
    } else {
        Ok(sqlx::query_as(
            "SELECT reference_id, reference_type, reference_key, metadata, created_at, updated_at
             FROM user_references
             WHERE user_id = $1::uuid
             ORDER BY created_at DESC
             LIMIT $2 OFFSET $3",
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?)
    }
}

pub async fn upsert_reference(
    pool: &PgPool,
    user_id: &str,
    reference_type: &str,
    reference_key: &str,
    metadata: Value,
) -> AppResult<UserReferenceRow> {
    Ok(sqlx::query_as(
        "INSERT INTO user_references (user_id, reference_type, reference_key, metadata)
         VALUES ($1::uuid, $2, $3, $4)
         ON CONFLICT (user_id, reference_type, reference_key)
         DO UPDATE SET metadata = EXCLUDED.metadata, updated_at = NOW()
         RETURNING reference_id, reference_type, reference_key, metadata, created_at, updated_at",
    )
    .bind(user_id)
    .bind(reference_type)
    .bind(reference_key)
    .bind(metadata)
    .fetch_one(pool)
    .await?)
}

pub async fn delete_reference(
    pool: &PgPool,
    user_id: &str,
    reference_type: &str,
    reference_key: &str,
) -> AppResult<bool> {
    let result = sqlx::query(
        "DELETE FROM user_references WHERE user_id = $1::uuid AND reference_type = $2 AND reference_key = $3",
    )
    .bind(user_id)
    .bind(reference_type)
    .bind(reference_key)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn exists_reference(
    pool: &PgPool,
    user_id: &str,
    reference_type: &str,
    reference_key: &str,
) -> AppResult<bool> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT reference_id FROM user_references
         WHERE user_id = $1::uuid AND reference_type = $2 AND reference_key = $3",
    )
    .bind(user_id)
    .bind(reference_type)
    .bind(reference_key)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

#[derive(Debug, Deserialize)]
pub struct ReferenceInput {
    pub reference_type: String,
    pub reference_key: String,
    pub metadata: Option<Value>,
}
