use platform_core::AppResult;
use sqlx::PgPool;

pub fn build_profile_text(
    username: Option<&str>,
    full_name: Option<&str>,
    bio: Option<&str>,
) -> String {
    [username, full_name, bio]
        .into_iter()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" ")
}

fn vector_literal(vector: &[f32]) -> String {
    let body = vector
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",");
    format!("[{body}]")
}

pub async fn upsert_content_embedding(
    pool: &PgPool,
    content_id: &str,
    vector: &[f32],
    model: &str,
    nsfw: Option<bool>,
) -> AppResult<()> {
    let pg_vector = vector_literal(vector);
    sqlx::query(
        "UPDATE content_vectors
         SET content_embedding = $2::vector, embedding_model = $3, embedding_dim = $4,
             nsfw = COALESCE($5, nsfw)
         WHERE content_id = $1",
    )
    .bind(content_id)
    .bind(pg_vector)
    .bind(model)
    .bind(vector.len() as i32)
    .bind(nsfw)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn upsert_profile_embedding(
    pool: &PgPool,
    wallet: &str,
    vector: &[f32],
    model: &str,
) -> AppResult<()> {
    let pg_vector = vector_literal(vector);
    sqlx::query(
        "INSERT INTO user_vectors (wallet_address, profile_embedding, embedding_model, embedding_dim, last_updated)
         VALUES ($1, $2::vector, $3, $4, NOW())
         ON CONFLICT (wallet_address) DO UPDATE
         SET profile_embedding = EXCLUDED.profile_embedding,
             embedding_model = EXCLUDED.embedding_model,
             embedding_dim = EXCLUDED.embedding_dim,
             last_updated = NOW()",
    )
    .bind(wallet)
    .bind(pg_vector)
    .bind(model)
    .bind(vector.len() as i32)
    .execute(pool)
    .await?;
    Ok(())
}
