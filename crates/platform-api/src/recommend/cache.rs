use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use sqlx::PgPool;

pub struct RecommendationCache {
    pub(crate) redis: ConnectionManager,
    pool: PgPool,
}

impl RecommendationCache {
    pub fn new(redis: ConnectionManager, pool: PgPool) -> Self {
        Self { redis, pool }
    }

    pub async fn get_blocked_user_ids(&self, wallet_address: &str) -> platform_core::AppResult<Vec<String>> {
        let key = format!("blocks:{wallet_address}");
        let mut redis = self.redis.clone();
        let cached: Option<String> = redis.get(&key).await?;
        if let Some(raw) = cached {
            return Ok(serde_json::from_str(&raw).unwrap_or_default());
        }

        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT blocked_wallet_address FROM blocked WHERE blocker_wallet_address = $1",
        )
        .bind(wallet_address)
        .fetch_all(&self.pool)
        .await?;
        let blocked: Vec<String> = rows.into_iter().map(|r| r.0).collect();
        let _: () = redis.set_ex(key, serde_json::to_string(&blocked).unwrap_or_default(), 600).await?;
        Ok(blocked)
    }
}
