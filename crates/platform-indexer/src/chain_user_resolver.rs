use platform_core::AppResult;
use sqlx::PgPool;

pub async fn resolve_wallet_from_chain_address(
    pool: &PgPool,
    chain_address: &str,
) -> AppResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT wallet_address FROM users WHERE LOWER(chain_address) = LOWER($1) OR LOWER(wallet_address) = LOWER($1) LIMIT 1",
    )
    .bind(chain_address)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}

pub async fn get_internal_post_id(pool: &PgPool, chain_post_id: &str) -> AppResult<Option<uuid::Uuid>> {
    let row: Option<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT post_id FROM chain_post_map WHERE chain_post_id = $1",
    )
    .bind(chain_post_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}

pub async fn is_platform_post(pool: &PgPool, chain_post_id: &str, platform_id: &str) -> AppResult<bool> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT platform_id FROM posts WHERE chain_post_id = $1 LIMIT 1",
    )
    .bind(chain_post_id)
    .fetch_optional(pool)
    .await?;
    Ok(row
        .map(|r| r.0.eq_ignore_ascii_case(platform_id))
        .unwrap_or(false))
}
