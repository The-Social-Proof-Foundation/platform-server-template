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
