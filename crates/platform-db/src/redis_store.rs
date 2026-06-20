use platform_core::AppResult;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;

pub async fn store_auth_nonce(
    redis: &mut ConnectionManager,
    wallet_address: &str,
    nonce: &str,
    ttl_secs: u64,
) -> AppResult<()> {
    let key = format!("auth:nonce:{wallet_address}");
    redis.set_ex::<_, _, ()>(key, nonce, ttl_secs).await?;
    Ok(())
}

pub async fn consume_auth_nonce(
    redis: &mut ConnectionManager,
    wallet_address: &str,
) -> AppResult<Option<String>> {
    let key = format!("auth:nonce:{wallet_address}");
    let nonce: Option<String> = redis.get_del(&key).await?;
    Ok(nonce)
}

pub async fn store_refresh_token(
    redis: &mut ConnectionManager,
    user_id: &str,
    token: &str,
    ttl_secs: u64,
) -> AppResult<()> {
    let key = format!("refreshToken:{user_id}");
    redis.set_ex::<_, _, ()>(key, token, ttl_secs).await?;
    Ok(())
}

pub async fn get_refresh_token(
    redis: &mut ConnectionManager,
    user_id: &str,
) -> AppResult<Option<String>> {
    let key = format!("refreshToken:{user_id}");
    let token: Option<String> = redis.get(key).await?;
    Ok(token)
}

pub async fn delete_refresh_token(
    redis: &mut ConnectionManager,
    user_id: &str,
) -> AppResult<()> {
    let key = format!("refreshToken:{user_id}");
    redis.del::<_, ()>(key).await?;
    Ok(())
}

pub async fn set_user_online(
    redis: &mut ConnectionManager,
    user_id: &str,
    ttl_secs: u64,
) -> AppResult<()> {
    let key = format!("user:{user_id}:isOnline");
    redis.set_ex::<_, _, ()>(key, "1", ttl_secs).await?;
    Ok(())
}

pub async fn is_user_online(redis: &mut ConnectionManager, user_id: &str) -> AppResult<bool> {
    let key = format!("user:{user_id}:isOnline");
    let val: Option<String> = redis.get(key).await?;
    Ok(val.is_some())
}

pub async fn rate_limit_incr(
    redis: &mut ConnectionManager,
    key_prefix: &str,
    client_key: &str,
    window_secs: u64,
) -> AppResult<u64> {
    let key = format!("{key_prefix}:{client_key}");
    let count: u64 = redis.incr(&key, 1).await?;
    if count == 1 {
        redis.expire::<_, ()>(&key, window_secs as i64).await?;
    }
    Ok(count)
}
