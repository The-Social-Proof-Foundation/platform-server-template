use platform_core::AppResult;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;

fn normalize_addr(addr: &str) -> String {
    addr.to_lowercase()
}

pub fn follows_key(follower: &str) -> String {
    format!("follows:{}", normalize_addr(follower))
}

pub fn blocked_key(blocker: &str) -> String {
    format!("blocked:{}", normalize_addr(blocker))
}

fn post_author_key(chain_post_id: &str) -> String {
    format!("post_author:{}", normalize_addr(chain_post_id))
}

fn post_platform_key(chain_post_id: &str) -> String {
    format!("post_platform:{}", normalize_addr(chain_post_id))
}

pub async fn add_follow(
    redis: &mut ConnectionManager,
    follower: &str,
    followee: &str,
) -> AppResult<()> {
    let key = follows_key(follower);
    let _: () = redis.sadd(key, normalize_addr(followee)).await?;
    Ok(())
}

pub async fn remove_follow(
    redis: &mut ConnectionManager,
    follower: &str,
    followee: &str,
) -> AppResult<()> {
    let key = follows_key(follower);
    let _: () = redis.srem(key, normalize_addr(followee)).await?;
    Ok(())
}

pub async fn add_block(
    redis: &mut ConnectionManager,
    blocker: &str,
    blocked: &str,
) -> AppResult<()> {
    let key = blocked_key(blocker);
    let _: () = redis.sadd(key, normalize_addr(blocked)).await?;
    Ok(())
}

pub async fn remove_block(
    redis: &mut ConnectionManager,
    blocker: &str,
    blocked: &str,
) -> AppResult<()> {
    let key = blocked_key(blocker);
    let _: () = redis.srem(key, normalize_addr(blocked)).await?;
    Ok(())
}

pub async fn list_follows(
    redis: &mut ConnectionManager,
    follower: &str,
) -> AppResult<Vec<String>> {
    let key = follows_key(follower);
    let members: Vec<String> = redis.smembers(key).await.unwrap_or_default();
    Ok(members)
}

pub async fn list_blocked(
    redis: &mut ConnectionManager,
    blocker: &str,
) -> AppResult<Vec<String>> {
    let key = blocked_key(blocker);
    let members: Vec<String> = redis.smembers(key).await.unwrap_or_default();
    Ok(members)
}

pub async fn blocked_count_for_wallet(
    redis: &mut ConnectionManager,
    wallet: &str,
) -> AppResult<i64> {
    let key = blocked_key(wallet);
    let count: i64 = redis.scard(key).await.unwrap_or(0);
    Ok(count)
}

pub async fn set_post_author(
    redis: &mut ConnectionManager,
    chain_post_id: &str,
    author_wallet: &str,
) -> AppResult<()> {
    let key = post_author_key(chain_post_id);
    let _: () = redis.set(key, normalize_addr(author_wallet)).await?;
    Ok(())
}

pub async fn get_post_author(
    redis: &mut ConnectionManager,
    chain_post_id: &str,
) -> AppResult<Option<String>> {
    let key = post_author_key(chain_post_id);
    let author: Option<String> = redis.get(key).await?;
    Ok(author)
}

pub async fn set_post_platform(
    redis: &mut ConnectionManager,
    chain_post_id: &str,
    platform_id: &str,
) -> AppResult<()> {
    let key = post_platform_key(chain_post_id);
    let _: () = redis.set(key, normalize_addr(platform_id)).await?;
    Ok(())
}

pub async fn get_post_platform(
    redis: &mut ConnectionManager,
    chain_post_id: &str,
) -> AppResult<Option<String>> {
    let key = post_platform_key(chain_post_id);
    let platform: Option<String> = redis.get(key).await?;
    Ok(platform)
}
