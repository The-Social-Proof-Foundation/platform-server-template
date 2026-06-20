use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use platform_core::{AppError, AppResult};
use platform_db::{delete_refresh_token, get_refresh_token, store_refresh_token};
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub user_id: String,
    pub exp: i64,
}

pub fn sign_access_token(user_id: &Uuid, secret: &str, ttl_secs: i64) -> AppResult<String> {
    let exp = (Utc::now() + Duration::seconds(ttl_secs)).timestamp();
    let claims = Claims {
        user_id: user_id.to_string(),
        exp,
    };
    Ok(encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    ).map_err(|_| AppError::Internal("failed to sign jwt".into()))?)
}

pub fn sign_refresh_token(user_id: &Uuid, secret: &str, ttl_secs: i64) -> AppResult<String> {
    sign_access_token(user_id, secret, ttl_secs)
}

pub fn verify_access_token(token: &str, secret: &str) -> AppResult<Claims> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    ).map_err(|_| AppError::Unauthorized)?;
    Ok(data.claims)
}

pub async fn persist_refresh_token(
    redis: &mut ConnectionManager,
    user_id: &Uuid,
    token: &str,
    ttl_secs: u64,
) -> AppResult<()> {
    store_refresh_token(redis, &user_id.to_string(), token, ttl_secs).await
}

pub async fn validate_refresh_token(
    redis: &mut ConnectionManager,
    user_id: &Uuid,
    token: &str,
    secret: &str,
) -> AppResult<()> {
    let stored = get_refresh_token(redis, &user_id.to_string()).await?;
    if stored.as_deref() != Some(token) {
        return Err(AppError::Unauthorized.into());
    }
    verify_access_token(token, secret)?;
    Ok(())
}

pub async fn revoke_refresh_token(redis: &mut ConnectionManager, user_id: &Uuid) -> AppResult<()> {
    delete_refresh_token(redis, &user_id.to_string()).await
}
