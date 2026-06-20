use platform_core::settings;
use platform_core::AppResult;
use redis::aio::ConnectionManager;
use serde::Serialize;
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationChannel {
    Push,
    Email,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct UserSettingRow {
    pub setting_name: String,
    pub setting_value: String,
}

pub async fn list_settings(pool: &PgPool, user_id: &str) -> AppResult<Vec<UserSettingRow>> {
    Ok(sqlx::query_as(
        "SELECT setting_name, setting_value FROM settings WHERE user_id = $1::uuid ORDER BY setting_name",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}

pub async fn get_setting(pool: &PgPool, user_id: &str, name: &str) -> AppResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT setting_value FROM settings WHERE user_id = $1::uuid AND setting_name = $2",
    )
    .bind(user_id)
    .bind(name)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}

pub async fn upsert_setting(
    pool: &PgPool,
    user_id: &str,
    name: &str,
    value: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO settings (user_id, setting_name, setting_value) VALUES ($1::uuid, $2, $3)
         ON CONFLICT (user_id, setting_name) DO UPDATE SET setting_value = EXCLUDED.setting_value, updated_at = NOW()",
    )
    .bind(user_id)
    .bind(name)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_setting(pool: &PgPool, user_id: &str, name: &str) -> AppResult<bool> {
    let result = sqlx::query("DELETE FROM settings WHERE user_id = $1::uuid AND setting_name = $2")
        .bind(user_id)
        .bind(name)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn get_bool_setting(
    pool: &PgPool,
    user_id: &str,
    name: &str,
    fallback: bool,
) -> AppResult<bool> {
    let value = get_setting(pool, user_id, name).await?;
    Ok(value
        .map(|v| settings::parse_bool(&v, fallback))
        .unwrap_or(fallback))
}

fn type_pref_key(notification_type: &str) -> Option<&'static str> {
    match notification_type {
        "mention" => Some("notify.mentions"),
        "comment" => Some("notify.comments"),
        "like" => Some("notify.likes"),
        t if t.starts_with("referral") => Some("notify.referrals"),
        _ => None,
    }
}

pub async fn notification_allowed(
    pool: &PgPool,
    user_id: &str,
    notification_type: &str,
    channel: NotificationChannel,
) -> AppResult<bool> {
    let channel_key = match channel {
        NotificationChannel::Push => "notify.push.enabled",
        NotificationChannel::Email => "notify.email.enabled",
    };
    let channel_default = settings::default_for(channel_key)
        .and_then(|v| settings::parse_bool_default(v))
        .unwrap_or(true);
    if !get_bool_setting(pool, user_id, channel_key, channel_default).await? {
        return Ok(false);
    }

    if let Some(type_key) = type_pref_key(notification_type) {
        let type_default = settings::default_for(type_key)
            .and_then(|v| settings::parse_bool_default(v))
            .unwrap_or(true);
        return get_bool_setting(pool, user_id, type_key, type_default).await;
    }

    Ok(true)
}

pub async fn blocked_count(
    pool: &PgPool,
    redis: &mut ConnectionManager,
    user_id: &str,
) -> AppResult<i64> {
    let wallet: (String,) =
        sqlx::query_as("SELECT wallet_address FROM users WHERE user_id = $1::uuid")
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    crate::graph_cache::blocked_count_for_wallet(redis, &wallet.0).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_pref_maps_known_notification_types() {
        assert_eq!(type_pref_key("mention"), Some("notify.mentions"));
        assert_eq!(type_pref_key("comment"), Some("notify.comments"));
        assert_eq!(type_pref_key("like"), Some("notify.likes"));
        assert_eq!(type_pref_key("referral_claimed"), Some("notify.referrals"));
        assert!(type_pref_key("waitlist_joined").is_none());
    }
}
