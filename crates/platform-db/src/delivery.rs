use platform_core::AppResult;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

const CACHE_TTL_SECS: u64 = 300;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct DeliveryConfigRow {
    pub platform_id: String,
    pub apns_bundle_id: Option<String>,
    pub apns_key_id: Option<String>,
    pub apns_team_id: Option<String>,
    pub apns_key_path: Option<String>,
    pub apns_key_content: Option<String>,
    pub fcm_server_key: Option<String>,
    pub resend_api_key: Option<String>,
    pub resend_from_email: Option<String>,
}

fn cache_key(platform_id: &str) -> String {
    format!("delivery:{platform_id}")
}

pub async fn get_delivery_config(
    pool: &PgPool,
    redis: &mut ConnectionManager,
    platform_id: &str,
) -> AppResult<Option<DeliveryConfigRow>> {
    let key = cache_key(platform_id);
    let cached: Option<String> = redis.get(&key).await.ok().flatten();
    if let Some(raw) = cached {
        if let Ok(row) = serde_json::from_str::<DeliveryConfigRow>(&raw) {
            return Ok(Some(row));
        }
    }

    let row: Option<DeliveryConfigRow> = sqlx::query_as(
        "SELECT platform_id, apns_bundle_id, apns_key_id, apns_team_id, apns_key_path,
                apns_key_content, fcm_server_key, resend_api_key, resend_from_email
         FROM platform_delivery_config WHERE platform_id = $1",
    )
    .bind(platform_id)
    .fetch_optional(pool)
    .await?;

    if let Some(ref config) = row {
        if let Ok(json) = serde_json::to_string(config) {
            let _: () = redis.set_ex(&key, json, CACHE_TTL_SECS).await.unwrap_or(());
        }
    }

    Ok(row)
}
