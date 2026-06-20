use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use platform_db::{list_blocked, list_follows};

pub struct RecommendationCache {
    pub(crate) redis: ConnectionManager,
}

impl RecommendationCache {
    pub fn new(redis: ConnectionManager) -> Self {
        Self { redis }
    }

    pub async fn get_blocked_user_ids(
        &self,
        wallet_address: &str,
    ) -> platform_core::AppResult<Vec<String>> {
        let key = format!("blocks:{wallet_address}");
        let mut redis = self.redis.clone();
        let cached: Option<String> = redis.get(&key).await?;
        if let Some(raw) = cached {
            return Ok(serde_json::from_str(&raw).unwrap_or_default());
        }

        let blocked = list_blocked(&mut redis, wallet_address).await?;
        let _: () = redis
            .set_ex(
                key,
                serde_json::to_string(&blocked).unwrap_or_default(),
                600,
            )
            .await?;
        Ok(blocked)
    }

    pub async fn get_following_user_ids(
        &self,
        wallet_address: &str,
    ) -> platform_core::AppResult<Vec<String>> {
        let mut redis = self.redis.clone();
        list_follows(&mut redis, wallet_address).await
    }
}
