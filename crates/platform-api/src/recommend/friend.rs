use serde::Serialize;
use sqlx::{PgPool, Row};

use super::cache::RecommendationCache;

#[derive(Debug, Serialize)]
pub struct FriendSuggestion {
    pub wallet_address: String,
    pub score: f64,
}

pub struct FriendRecommendationEngine {
    pool: PgPool,
    cache: RecommendationCache,
}

impl FriendRecommendationEngine {
    pub fn new(pool: PgPool, cache: RecommendationCache) -> Self {
        Self { pool, cache }
    }

    pub async fn get_friend_suggestions(
        &self,
        wallet_address: &str,
        limit: i64,
    ) -> platform_core::AppResult<Vec<FriendSuggestion>> {
        let blocked = self.cache.get_blocked_user_ids(wallet_address).await?;
        let rows = sqlx::query(
            "WITH me AS (
                SELECT profile_embedding FROM user_vectors WHERE wallet_address = $1
             )
             SELECT uv.wallet_address, (uv.profile_embedding <=> me.profile_embedding) AS score
             FROM user_vectors uv, me
             WHERE me.profile_embedding IS NOT NULL
               AND uv.profile_embedding IS NOT NULL
               AND uv.wallet_address <> $1
               AND NOT (uv.wallet_address = ANY($2))
               AND uv.wallet_address NOT IN (
                   SELECT followee_wallet_address FROM follows WHERE follower_wallet_address = $1
               )
             ORDER BY score ASC
             LIMIT $3",
        )
        .bind(wallet_address)
        .bind(blocked)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| FriendSuggestion {
                wallet_address: row.get("wallet_address"),
                score: row.get("score"),
            })
            .collect())
    }
}
