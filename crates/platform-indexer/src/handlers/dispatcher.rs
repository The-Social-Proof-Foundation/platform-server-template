use platform_core::AppResult;
use platform_embeddings::EmbeddingService;
use platform_db::graph_cache;
use platform_notify::NotificationService;
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use tracing::error;

use crate::chain_user_resolver::resolve_wallet_from_chain_address;
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
    embeddings: Option<&EmbeddingService>,
    platform_id: &str,
    event: ParsedChainEvent,
    meta: EventMeta,
) -> AppResult<()> {
    let _ = &meta;
    match event {
        ParsedChainEvent::PostCreated(data) => {
            handle_post_created(pool, redis, notify, embeddings, platform_id, data).await
        }
        ParsedChainEvent::CommentCreated(data) => {
            handle_comment_created(pool, redis, notify, platform_id, data).await
        }
        ParsedChainEvent::Reaction(data) => {
            handle_reaction(pool, redis, notify, platform_id, data).await
        }
        ParsedChainEvent::RemoveReaction(_) => Ok(()),
        ParsedChainEvent::Tip(_) => Ok(()),
        ParsedChainEvent::PostDeleted(data) => handle_post_deleted(pool, data).await,
        ParsedChainEvent::CommentDeleted(_) => Ok(()),
        ParsedChainEvent::PostUpdated(data) => {
            handle_post_updated(pool, embeddings, data).await
        }
        ParsedChainEvent::CommentUpdated(_) => Ok(()),
        ParsedChainEvent::Follow(data) => {
            graph_cache::add_follow(redis, &data.follower, &data.following).await
        }
        ParsedChainEvent::Unfollow(data) => {
            graph_cache::remove_follow(redis, &data.follower, &data.unfollowed).await
        }
        ParsedChainEvent::Block(data) => {
            graph_cache::add_block(redis, &data.blocker, &data.blocked).await?;
            graph_cache::remove_follow(redis, &data.blocker, &data.blocked).await?;
            graph_cache::remove_follow(redis, &data.blocked, &data.blocker).await
        }
        ParsedChainEvent::Unblock(data) => {
            graph_cache::remove_block(redis, &data.blocker, &data.unblocked).await
        }
    }
}

async fn handle_post_created(
    pool: &PgPool,
    redis: &mut ConnectionManager,
    notify: &NotificationService,
    embeddings: Option<&EmbeddingService>,
    platform_id: &str,
    data: crate::parsers::post_events::PostCreatedPayload,
) -> AppResult<()> {
    if !is_post_created_for_platform(platform_id, &data) {
        return Ok(());
    }

    let wallet = resolve_wallet_from_chain_address(pool, &data.owner)
        .await?
        .unwrap_or_else(|| data.owner.to_lowercase());

    graph_cache::set_post_author(redis, &data.post_id, &wallet).await?;
    graph_cache::set_post_platform(redis, &data.post_id, &data.platform_id).await?;

    sqlx::query(
        "INSERT INTO content_vectors (content_id, creator_wallet_address, platform_id, description, mentions, extra_metadata)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (content_id) DO NOTHING",
    )
    .bind(&data.post_id)
    .bind(&wallet)
    .bind(&data.platform_id)
    .bind(&data.content)
    .bind(data.mentions.clone().unwrap_or_default())
    .bind(data.metadata_json.as_ref().map(|s| serde_json::json!({ "raw": s })))
    .execute(pool)
    .await?;

    if let Some(svc) = embeddings {
        let pool = pool.clone();
        let content_id = data.post_id.clone();
        let description = data.content.clone();
        let svc = svc.clone();
        tokio::spawn(async move {
            if let Err(err) = svc
                .embed_and_store_content(&pool, &content_id, &description)
                .await
            {
                error!(error = %err, content_id, "content embedding failed");
            }
        });
    }

    notify
        .notify_mentions(
            pool,
            redis,
            &data.mentions.clone().unwrap_or_default(),
            &data.post_id,
            &wallet,
        )
        .await?;

    Ok(())
}

async fn handle_comment_created(
    pool: &PgPool,
    redis: &mut ConnectionManager,
    notify: &NotificationService,
    platform_id: &str,
    data: crate::parsers::post_events::CommentCreatedPayload,
) -> AppResult<()> {
    let post_platform = graph_cache::get_post_platform(redis, &data.post_id).await?;
    let Some(post_platform) = post_platform else {
        return Ok(());
    };
    if !post_platform.eq_ignore_ascii_case(platform_id) {
        return Ok(());
    }

    let wallet = resolve_wallet_from_chain_address(pool, &data.owner)
        .await?
        .unwrap_or_else(|| data.owner.to_lowercase());

    let author = graph_cache::get_post_author(redis, &data.post_id)
        .await?
        .unwrap_or_default();

    if !author.is_empty() {
        notify
            .notify_comment(
                pool,
                redis,
                &author,
                &wallet,
                &data.post_id,
                &data.comment_id,
            )
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
) -> AppResult<()> {
    if !data.is_post {
        return Ok(());
    }

    let post_platform = graph_cache::get_post_platform(redis, &data.object_id).await?;
    let Some(post_platform) = post_platform else {
        return Ok(());
    };
    if !post_platform.eq_ignore_ascii_case(platform_id) {
        return Ok(());
    }

    let wallet = resolve_wallet_from_chain_address(pool, &data.user)
        .await?
        .unwrap_or_else(|| data.user.to_lowercase());

    let author = data
        .principal_owner
        .clone()
        .or(graph_cache::get_post_author(redis, &data.object_id).await?)
        .unwrap_or_default();

    if !author.is_empty() {
        notify
            .notify_like(pool, redis, &author, &wallet, &data.object_id)
            .await?;
    }
    Ok(())
}

async fn handle_post_deleted(
    pool: &PgPool,
    data: crate::parsers::post_events::DeletePostPayload,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE content_vectors SET extra_metadata = COALESCE(extra_metadata, '{}'::jsonb) || '{\"deleted\": true}'::jsonb
         WHERE content_id = $1",
    )
    .bind(&data.post_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn handle_post_updated(
    pool: &PgPool,
    embeddings: Option<&EmbeddingService>,
    data: crate::parsers::post_events::PostUpdatedPayload,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE content_vectors SET description = $1, extra_metadata = COALESCE(extra_metadata, '{}'::jsonb) || jsonb_build_object('raw', $2)
         WHERE content_id = $3",
    )
    .bind(&data.content)
    .bind(data.metadata_json.as_deref())
    .bind(&data.post_id)
    .execute(pool)
    .await?;

    if let Some(svc) = embeddings {
        let pool = pool.clone();
        let content_id = data.post_id.clone();
        let description = data.content.clone();
        let svc = svc.clone();
        tokio::spawn(async move {
            if let Err(err) = svc
                .embed_and_store_content(&pool, &content_id, &description)
                .await
            {
                error!(error = %err, content_id, "content re-embedding failed");
            }
        });
    }

    Ok(())
}
