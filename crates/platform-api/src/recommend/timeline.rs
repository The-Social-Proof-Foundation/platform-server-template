use redis::AsyncCommands;
use serde::Serialize;
use sqlx::{PgPool, Row};

use super::cache::RecommendationCache;

#[derive(Debug, Serialize, serde::Deserialize)]
pub struct ContentRecommendation {
    pub content_id: String,
    pub score: f64,
    pub reason: Option<String>,
    pub nsfw: bool,
}

pub struct TimelineRecommendationEngine {
    pool: PgPool,
    cache: RecommendationCache,
}

impl TimelineRecommendationEngine {
    pub fn new(pool: PgPool, cache: RecommendationCache) -> Self {
        Self { pool, cache }
    }

    pub async fn get_personalized_feed(
        &self,
        wallet_address: &str,
        limit: i64,
        allow_nsfw: bool,
        platform_id: Option<&str>,
    ) -> platform_core::AppResult<Vec<ContentRecommendation>> {
        let bucket = (chrono::Utc::now().timestamp() / 300).to_string();
        let cache_key = format!("feed:{wallet_address}:{bucket}:{}", if allow_nsfw { "all" } else { "sfw" });
        let mut redis = self.cache.redis.clone();
        let raw: Option<String> = redis.get(&cache_key).await.ok().flatten();
        if let Some(raw) = raw {
            if let Ok(feed) = serde_json::from_str::<Vec<ContentRecommendation>>(&raw) {
                return Ok(feed);
            }
        }

        let blocked = self.cache.get_blocked_user_ids(wallet_address).await?;
        let nsfw_filter = if allow_nsfw {
            String::new()
        } else {
            "AND COALESCE(cv.nsfw, FALSE) = FALSE".into()
        };
        let platform_filter = if platform_id.is_some() {
            "AND cv.platform_id = $4"
        } else {
            ""
        };

        let sql = format!(
            "WITH user_preferences AS (
                SELECT uv.profile_embedding AS user_embedding
                FROM user_vectors uv WHERE uv.wallet_address = $1
             ),
             phase_a AS (
                SELECT cv.content_id, cv.creator_wallet_address, cv.content_embedding,
                       COALESCE(cv.nsfw, FALSE) AS nsfw
                FROM content_vectors cv
                WHERE cv.created_at > NOW() - INTERVAL '7 days'
                  {nsfw_filter}
                  {platform_filter}
                  AND NOT (cv.creator_wallet_address = ANY($3))
                  AND COALESCE(cv.extra_metadata->>'deleted', 'false') <> 'true'
                  AND COALESCE(cv.moderation_override, '') <> 'force_block'
                ORDER BY cv.created_at DESC
                LIMIT 500
             ),
             scored AS (
                SELECT pa.content_id, pa.nsfw,
                       (pa.content_embedding <=> up.user_embedding) AS similarity_score
                FROM phase_a pa, user_preferences up
                WHERE up.user_embedding IS NOT NULL AND pa.content_embedding IS NOT NULL
             )
             SELECT content_id, similarity_score, nsfw
             FROM scored
             ORDER BY similarity_score ASC NULLS LAST
             LIMIT $2"
        );

        let mut query = sqlx::query(&sql).bind(wallet_address).bind(limit).bind(&blocked);
        if let Some(platform_id) = platform_id {
            query = query.bind(platform_id);
        }

        let rows = query.fetch_all(&self.pool).await?;
        let feed: Vec<ContentRecommendation> = rows
            .into_iter()
            .map(|row| ContentRecommendation {
                content_id: row.get("content_id"),
                score: row.get::<f64, _>("similarity_score"),
                reason: Some("Similar to your recent interests".into()),
                nsfw: row.get("nsfw"),
            })
            .collect();

        let _: () = redis
            .set_ex(cache_key, serde_json::to_string(&feed).unwrap_or_default(), 300)
            .await?;
        Ok(feed)
    }
}
