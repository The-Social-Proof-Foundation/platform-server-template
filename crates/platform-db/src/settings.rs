use platform_core::settings;
use platform_core::AppResult;
use serde::Serialize;
use sqlx::{FromRow, PgPool};

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

pub async fn blocked_count(pool: &PgPool, user_id: &str) -> AppResult<i64> {
    let wallet: (String,) =
        sqlx::query_as("SELECT wallet_address FROM users WHERE user_id = $1::uuid")
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM blocked WHERE blocker_wallet_address = $1",
    )
    .bind(wallet.0)
    .fetch_one(pool)
    .await?;
    Ok(count.0)
}
