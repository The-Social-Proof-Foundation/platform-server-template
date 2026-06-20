use platform_core::AppResult;
use serde_json::Value;
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow)]
pub struct InteractionRow {
    pub id: uuid::Uuid,
    pub wallet_address: String,
    pub content_id: String,
    pub interaction_type: String,
    pub engagement_score: Option<f32>,
    pub watch_duration: Option<i32>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub async fn insert_interaction(
    pool: &PgPool,
    wallet: &str,
    content_id: &str,
    interaction_type: &str,
    engagement_score: Option<f32>,
    watch_duration: Option<i32>,
    context: Option<Value>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO user_interactions (wallet_address, content_id, interaction_type, engagement_score, watch_duration, context_data)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(wallet)
    .bind(content_id)
    .bind(interaction_type)
    .bind(engagement_score)
    .bind(watch_duration)
    .bind(context.unwrap_or_else(|| serde_json::json!({})))
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn recent_interactions(
    pool: &PgPool,
    wallet: &str,
    limit: i64,
) -> AppResult<Vec<InteractionRow>> {
    Ok(sqlx::query_as(
        "SELECT id, wallet_address, content_id, interaction_type, engagement_score, watch_duration, timestamp
         FROM user_interactions
         WHERE wallet_address = $1
         ORDER BY timestamp DESC
         LIMIT $2",
    )
    .bind(wallet)
    .bind(limit.clamp(1, 500))
    .fetch_all(pool)
    .await?)
}

pub async fn update_engagement_patterns(
    pool: &PgPool,
    wallet: &str,
    content_id: &str,
    interaction_type: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO user_vectors (wallet_address, engagement_patterns, last_updated)
         VALUES ($1, jsonb_build_object('recent', jsonb_build_array(jsonb_build_object('contentId', $2, 'type', $3))), NOW())
         ON CONFLICT (wallet_address) DO UPDATE
         SET engagement_patterns = jsonb_set(
             COALESCE(user_vectors.engagement_patterns, '{}'::jsonb),
             '{recent}',
             (
                 SELECT jsonb_agg(elem)
                 FROM (
                     SELECT elem FROM jsonb_array_elements(
                         COALESCE(user_vectors.engagement_patterns->'recent', '[]'::jsonb) || jsonb_build_array(jsonb_build_object('contentId', $2, 'type', $3))
                     ) AS elem
                     ORDER BY elem->>'contentId'
                     LIMIT 50
                 ) sub
             ),
             true
         ),
         last_updated = NOW()",
    )
    .bind(wallet)
    .bind(content_id)
    .bind(interaction_type)
    .execute(pool)
    .await?;
    Ok(())
}
