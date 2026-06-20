use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use platform_core::AppResult;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use sqlx::PgPool;
use tokio::sync::Mutex;
use tokio::time;
use tracing::warn;

#[derive(Clone)]
pub struct ShardedCounter {
    redis: ConnectionManager,
    pool: PgPool,
    entity_type: String,
    entity_id: String,
    counter_type: String,
}

impl ShardedCounter {
    pub fn new(
        redis: ConnectionManager,
        pool: PgPool,
        entity_type: &str,
        entity_id: &str,
        counter_type: &str,
    ) -> Self {
        Self {
            redis,
            pool,
            entity_type: entity_type.to_string(),
            entity_id: entity_id.to_string(),
            counter_type: counter_type.to_string(),
        }
    }

    fn redis_key(&self) -> String {
        format!(
            "counter:{}:{}:{}",
            self.entity_type, self.entity_id, self.counter_type
        )
    }

    pub async fn increment_by(&self, delta: i64) -> AppResult<i64> {
        let mut redis = self.redis.clone();
        let key = self.redis_key();
        let val: i64 = redis::cmd("INCRBY")
            .arg(&key)
            .arg(delta)
            .query_async(&mut redis)
            .await?;
        Ok(val)
    }

    pub async fn get(&self) -> AppResult<i64> {
        let mut redis = self.redis.clone();
        let key = self.redis_key();
        let val: Option<i64> = redis.get(key).await?;
        Ok(val.unwrap_or(0))
    }
}

#[derive(Clone)]
pub struct CounterFlushManager {
    pool: PgPool,
    pending: Arc<Mutex<HashMap<String, i64>>>,
}

impl CounterFlushManager {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn record_increment(&self, key: String, delta: i64) {
        let mut pending = self.pending.lock().await;
        *pending.entry(key).or_insert(0) += delta;
    }

    pub fn spawn_flush_task(self, redis: ConnectionManager) {
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(5));
            loop {
                interval.tick().await;
                if let Err(err) = self.flush_once(redis.clone()).await {
                    warn!(error = %err, "counter flush failed");
                }
            }
        });
    }

    async fn flush_once(&self, mut redis: ConnectionManager) -> AppResult<()> {
        let batch = {
            let mut pending = self.pending.lock().await;
            if pending.is_empty() {
                return Ok(());
            }
            std::mem::take(&mut *pending)
        };

        for (key, delta) in batch {
            if delta == 0 {
                continue;
            }
            if let Some((entity_type, entity_id, counter_type)) = parse_counter_key(&key) {
                flush_counter_to_postgres(&self.pool, entity_type, entity_id, counter_type, delta)
                    .await?;
            }
            let _: () = redis.del::<_, ()>(&key).await?;
        }
        Ok(())
    }

    pub async fn shutdown_flush(&self, redis: ConnectionManager) -> AppResult<()> {
        self.flush_once(redis).await
    }
}

fn parse_counter_key(key: &str) -> Option<(&str, &str, &str)> {
    let parts: Vec<&str> = key.split(':').collect();
    if parts.len() == 4 && parts[0] == "counter" {
        Some((parts[1], parts[2], parts[3]))
    } else {
        None
    }
}

async fn flush_counter_to_postgres(
    pool: &PgPool,
    entity_type: &str,
    entity_id: &str,
    counter_type: &str,
    delta: i64,
) -> AppResult<()> {
    match (entity_type, counter_type) {
        ("user", "followerCount") => {
            sqlx::query(
                "UPDATE users SET follower_count = follower_count + $1 WHERE wallet_address = $2",
            )
            .bind(delta)
            .bind(entity_id)
            .execute(pool)
            .await?;
        }
        ("user", "followingCount") => {
            sqlx::query(
                "UPDATE users SET following_count = following_count + $1 WHERE wallet_address = $2",
            )
            .bind(delta)
            .bind(entity_id)
            .execute(pool)
            .await?;
        }
        ("user", "postCount") => {
            sqlx::query(
                "UPDATE users SET notification_count = notification_count WHERE wallet_address = $1",
            )
            .bind(entity_id)
            .execute(pool)
            .await?;
            sqlx::query(
                "UPDATE users SET follower_count = follower_count WHERE wallet_address = $1",
            )
            .bind(entity_id)
            .execute(pool)
            .await?;
            let _ = delta;
        }
        ("user", "notificationCount") => {
            sqlx::query(
                "UPDATE users SET notification_count = notification_count + $1 WHERE user_id::text = $2 OR wallet_address = $2",
            )
            .bind(delta)
            .bind(entity_id)
            .execute(pool)
            .await?;
        }
        _ => {
            let _ = (entity_type, entity_id, counter_type, delta);
        }
    }
    Ok(())
}
