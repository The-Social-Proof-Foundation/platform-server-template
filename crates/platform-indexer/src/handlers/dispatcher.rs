use platform_core::AppResult;
use platform_db::ShardedCounter;
use platform_notify::NotificationService;
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use uuid::Uuid;

use crate::chain_user_resolver::{
    get_internal_post_id, is_platform_post, resolve_wallet_from_chain_address,
};
use crate::filters::platform_filter::is_post_created_for_platform;
use crate::parsers::post_events::ParsedChainEvent;

pub struct EventMeta {
    pub tx_digest: String,
    pub checkpoint_seq: i64,
}

pub async fn handle_parsed_event(
    pool: &PgPool,
    redis: &mut ConnectionManager,
    notify: &NotificationService,
    platform_id: &str,
    event: ParsedChainEvent,
    meta: EventMeta,
) -> AppResult<()> {
    match event {
        ParsedChainEvent::PostCreated(data) => {
            handle_post_created(pool, redis, notify, platform_id, data, &meta).await
        }
        ParsedChainEvent::CommentCreated(data) => {
            handle_comment_created(pool, redis, notify, platform_id, data, &meta).await
        }
        ParsedChainEvent::Reaction(data) => {
            handle_reaction(pool, redis, notify, platform_id, data, &meta, false).await
        }
        ParsedChainEvent::RemoveReaction(data) => {
            handle_reaction(pool, redis, notify, platform_id, data, &meta, true).await
        }
        ParsedChainEvent::Tip(data) => handle_tip(pool, platform_id, data, &meta).await,
        ParsedChainEvent::PostDeleted(data) => handle_post_deleted(pool, data).await,
        ParsedChainEvent::CommentDeleted(data) => handle_comment_deleted(pool, data).await,
        ParsedChainEvent::PostUpdated(data) => handle_post_updated(pool, data).await,
        ParsedChainEvent::CommentUpdated(data) => handle_comment_updated(pool, data).await,
    }
}

async fn handle_post_created(
    pool: &PgPool,
    redis: &mut ConnectionManager,
    notify: &NotificationService,
    platform_id: &str,
    data: crate::parsers::post_events::PostCreatedPayload,
    meta: &EventMeta,
) -> AppResult<()> {
    if !is_post_created_for_platform(platform_id, &data) {
        return Ok(());
    }

    let wallet = resolve_wallet_from_chain_address(pool, &data.owner)
        .await?
        .unwrap_or_else(|| data.owner.to_lowercase());

    let post_id = Uuid::new_v4();
    let counter = ShardedCounter::new(redis.clone(), pool.clone(), "user", &wallet, "postCount");
    counter.increment_by(1).await?;

    sqlx::query(
        "INSERT INTO posts (
            post_id, author_wallet_address, description, hashtags, mentions, timestamp,
            chain_post_id, platform_id, owner_address, tx_digest, checkpoint_seq, post_type, media_urls, metadata_json
         ) VALUES ($1,$2,$3,$4,$5, EXTRACT(EPOCH FROM NOW())::BIGINT, $6,$7,$8,$9,$10,$11,$12,$13)",
    )
    .bind(post_id)
    .bind(&wallet)
    .bind(&data.content)
    .bind(&Vec::<String>::new())
    .bind(data.mentions.clone().unwrap_or_default())
    .bind(&data.post_id)
    .bind(&data.platform_id)
    .bind(&data.owner)
    .bind(&meta.tx_digest)
    .bind(meta.checkpoint_seq)
    .bind(data.post_type.as_deref())
    .bind(data.media_urls.as_ref().map(serde_json::to_value).transpose()?)
    .bind(data.metadata_json.as_deref())
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO chain_post_map (chain_post_id, post_id) VALUES ($1, $2)
         ON CONFLICT (chain_post_id) DO NOTHING",
    )
    .bind(&data.post_id)
    .bind(post_id)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO content_vectors (content_id, creator_wallet_address, platform_id, description, mentions, extra_metadata)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (content_id) DO NOTHING",
    )
    .bind(post_id)
    .bind(&wallet)
    .bind(&data.platform_id)
    .bind(&data.content)
    .bind(data.mentions.clone().unwrap_or_default())
    .bind(data.metadata_json.as_ref().map(|s| serde_json::json!({ "raw": s })))
    .execute(pool)
    .await?;

    notify
        .notify_mentions(pool, redis, &data.mentions.clone().unwrap_or_default(), post_id, &wallet)
        .await?;

    Ok(())
}

async fn handle_comment_created(
    pool: &PgPool,
    redis: &mut ConnectionManager,
    notify: &NotificationService,
    platform_id: &str,
    data: crate::parsers::post_events::CommentCreatedPayload,
    meta: &EventMeta,
) -> AppResult<()> {
    if !is_platform_post(pool, &data.post_id, platform_id).await? {
        return Ok(());
    }
    let internal_post_id = get_internal_post_id(pool, &data.post_id)
        .await?
        .ok_or(platform_core::AppError::NotFound)?;
    let wallet = resolve_wallet_from_chain_address(pool, &data.owner)
        .await?
        .unwrap_or_else(|| data.owner.to_lowercase());

    let comment_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO comments (comment_id, post_id, commenter_wallet_address, content, hashtags, mentions, timestamp, chain_comment_id, tx_digest, checkpoint_seq)
         VALUES ($1,$2,$3,$4,$5,$6, EXTRACT(EPOCH FROM NOW())::BIGINT, $7,$8,$9)",
    )
    .bind(comment_id)
    .bind(internal_post_id)
    .bind(&wallet)
    .bind(&data.content)
    .bind(&Vec::<String>::new())
    .bind(data.mentions.clone().unwrap_or_default())
    .bind(&data.comment_id)
    .bind(&meta.tx_digest)
    .bind(meta.checkpoint_seq)
    .execute(pool)
    .await?;

    let author: Option<(String,)> = sqlx::query_as(
        "SELECT author_wallet_address FROM posts WHERE post_id = $1",
    )
    .bind(internal_post_id)
    .fetch_optional(pool)
    .await?;

    if let Some((author_wallet,)) = author {
        notify
            .notify_comment(pool, redis, &author_wallet, &wallet, internal_post_id, comment_id)
            .await?;
    }
    Ok(())
}

async fn handle_reaction(
    pool: &PgPool,
    redis: &mut ConnectionManager,
    notify: &NotificationService,
    platform_id: &str,
    data: crate::parsers::post_events::ReactionPayload,
    meta: &EventMeta,
    remove: bool,
) -> AppResult<()> {
    let target_id = if data.is_post {
        get_internal_post_id(pool, &data.object_id).await?
    } else {
        sqlx::query_as::<_, (Uuid,)>(
            "SELECT comment_id FROM comments WHERE chain_comment_id = $1",
        )
        .bind(&data.object_id)
        .fetch_optional(pool)
        .await?
        .map(|r| r.0)
    };

    let Some(target_id) = target_id else {
        return Ok(());
    };

    let wallet = resolve_wallet_from_chain_address(pool, &data.user)
        .await?
        .unwrap_or_else(|| data.user.to_lowercase());

    if remove {
        sqlx::query(
            "DELETE FROM likes WHERE liker_wallet_address = $1 AND target_id = $2 AND COALESCE(reaction,'') = $3",
        )
        .bind(&wallet)
        .bind(target_id)
        .bind(&data.reaction)
        .execute(pool)
        .await?;
        return Ok(());
    }

    let like_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO likes (like_id, liker_wallet_address, target_id, target_type, reaction, tx_digest, checkpoint_seq)
         VALUES ($1,$2,$3,$4,$5,$6,$7)
         ON CONFLICT DO NOTHING",
    )
    .bind(like_id)
    .bind(&wallet)
    .bind(target_id)
    .bind(if data.is_post { "post" } else { "comment" })
    .bind(&data.reaction)
    .bind(&meta.tx_digest)
    .bind(meta.checkpoint_seq)
    .execute(pool)
    .await?;

    if data.is_post {
        let author: Option<(String,)> = sqlx::query_as(
            "SELECT author_wallet_address FROM posts WHERE post_id = $1 AND platform_id = $2",
        )
        .bind(target_id)
        .bind(platform_id)
        .fetch_optional(pool)
        .await?;
        if let Some((author_wallet,)) = author {
            notify
                .notify_like(pool, redis, &author_wallet, &wallet, target_id)
                .await?;
        }
    }
    Ok(())
}

async fn handle_tip(
    pool: &PgPool,
    _platform_id: &str,
    data: crate::parsers::post_events::TipPayload,
    meta: &EventMeta,
) -> AppResult<()> {
    let amount = data.amount.as_i64().or_else(|| data.amount.as_str().and_then(|s| s.parse().ok())).unwrap_or(0);
    sqlx::query(
        "INSERT INTO tips (object_id, from_address, to_address, amount, coin_type, is_post, tx_digest, checkpoint_seq)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(&data.object_id)
    .bind(&data.from)
    .bind(&data.to)
    .bind(amount)
    .bind(data.coin_type.as_deref())
    .bind(data.is_post)
    .bind(&meta.tx_digest)
    .bind(meta.checkpoint_seq)
    .execute(pool)
    .await?;
    Ok(())
}

async fn handle_post_deleted(
    pool: &PgPool,
    data: crate::parsers::post_events::DeletePostPayload,
) -> AppResult<()> {
    sqlx::query("UPDATE posts SET deleted_at = NOW() WHERE chain_post_id = $1")
        .bind(&data.post_id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn handle_comment_deleted(
    pool: &PgPool,
    data: crate::parsers::post_events::DeleteCommentPayload,
) -> AppResult<()> {
    sqlx::query("UPDATE comments SET deleted_at = NOW() WHERE chain_comment_id = $1")
        .bind(&data.comment_id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn handle_post_updated(
    pool: &PgPool,
    data: crate::parsers::post_events::PostUpdatedPayload,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE posts SET description = $1, metadata_json = $2 WHERE chain_post_id = $3",
    )
    .bind(&data.content)
    .bind(data.metadata_json.as_deref())
    .bind(&data.post_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn handle_comment_updated(
    pool: &PgPool,
    data: crate::parsers::post_events::CommentUpdatedPayload,
) -> AppResult<()> {
    sqlx::query("UPDATE comments SET content = $1 WHERE chain_comment_id = $2")
        .bind(&data.content)
        .bind(&data.comment_id)
        .execute(pool)
        .await?;
    Ok(())
}
