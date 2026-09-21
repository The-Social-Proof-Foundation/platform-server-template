use platform_core::AppResult;
use sqlx::PgPool;
use uuid::Uuid;

/// Ensure a users row exists for a MySocial wallet. Returns `(user_id, wallet_address)`.
pub async fn upsert_wallet_user(pool: &PgPool, wallet_address: &str) -> AppResult<(String, String)> {
    let inserted: Option<(Uuid, String)> = sqlx::query_as(
        "INSERT INTO users (wallet_address, public_key, chain_address)
         VALUES ($1, $1, $1)
         ON CONFLICT (wallet_address) DO NOTHING
         RETURNING user_id, wallet_address",
    )
    .bind(wallet_address)
    .fetch_optional(pool)
    .await?;

    if let Some((user_id, wallet)) = inserted {
        return Ok((user_id.to_string(), wallet));
    }

    let existing: (Uuid, String) = sqlx::query_as(
        "SELECT user_id, wallet_address FROM users WHERE wallet_address = $1 LIMIT 1",
    )
    .bind(wallet_address)
    .fetch_one(pool)
    .await?;
    Ok((existing.0.to_string(), existing.1))
}

pub async fn wallet_for_user(pool: &PgPool, user_id: &str) -> AppResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT wallet_address FROM users WHERE user_id = $1::uuid LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}
