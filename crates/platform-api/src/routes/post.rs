use axum::extract::{Extension, Path};
use axum::Json;
use platform_core::AppError;
use serde_json::json;
use sqlx::FromRow;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::middleware::AuthUser;
use crate::recommend::cache::RecommendationCache;
use crate::recommend::friend::FriendRecommendationEngine;
use crate::recommend::timeline::TimelineRecommendationEngine;
use crate::state::SharedApiState;

#[derive(Debug, FromRow, serde::Serialize)]
pub struct PostRow {
    pub post_id: Uuid,
    pub author_wallet_address: String,
    pub description: Option<String>,
    pub timestamp: i64,
    pub platform_id: Option<String>,
}

pub async fn following_feed(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
) -> ApiResult<Json<Vec<PostRow>>> {
    let wallet: (String,) = sqlx::query_as("SELECT wallet_address FROM users WHERE user_id = $1::uuid")
        .bind(&auth.user_id)
        .fetch_one(state.pg_read())
        .await?;

    let posts = sqlx::query_as(
        "SELECT p.post_id, p.author_wallet_address, p.description, p.timestamp, p.platform_id
         FROM posts p
         INNER JOIN follows f ON f.followee_wallet_address = p.author_wallet_address
         WHERE f.follower_wallet_address = $1 AND p.deleted_at IS NULL
         ORDER BY p.timestamp DESC
         LIMIT 100",
    )
    .bind(wallet.0)
    .fetch_all(state.pg_read())
    .await?;
    Ok(Json(posts))
}

pub async fn posts_by_user(
    Extension(state): Extension<SharedApiState>,
    Path(user_id): Path<String>,
) -> ApiResult<Json<Vec<PostRow>>> {
    let posts = sqlx::query_as(
        "SELECT post_id, author_wallet_address, description, timestamp, platform_id
         FROM posts WHERE author_wallet_address = $1
         ORDER BY timestamp DESC LIMIT 100",
    )
    .bind(user_id)
    .fetch_all(state.pg_read())
    .await?;
    Ok(Json(posts))
}

pub async fn post_data(
    Extension(state): Extension<SharedApiState>,
    Path(post_id): Path<Uuid>,
) -> ApiResult<Json<PostRow>> {
    let post = sqlx::query_as(
        "SELECT post_id, author_wallet_address, description, timestamp, platform_id
         FROM posts WHERE post_id = $1",
    )
    .bind(post_id)
    .fetch_optional(state.pg_read())
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(Json(post))
}

pub async fn recommendation_feed(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
) -> ApiResult<Json<serde_json::Value>> {
    let wallet: (String,) = sqlx::query_as("SELECT wallet_address FROM users WHERE user_id = $1::uuid")
        .bind(&auth.user_id)
        .fetch_one(state.pg_read())
        .await?;
    let cache = RecommendationCache::new(state.redis(), state.pg_read().clone());
    let engine = TimelineRecommendationEngine::new(state.pg_read().clone(), cache);
    let feed = engine.get_personalized_feed(&wallet.0, 50, true, None).await?;
    Ok(Json(json!({ "feed": feed })))
}

pub async fn friend_recommendations(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
) -> ApiResult<Json<serde_json::Value>> {
    let wallet: (String,) = sqlx::query_as("SELECT wallet_address FROM users WHERE user_id = $1::uuid")
        .bind(&auth.user_id)
        .fetch_one(state.pg_read())
        .await?;
    let cache = RecommendationCache::new(state.redis(), state.pg_read().clone());
    let engine = FriendRecommendationEngine::new(state.pg_read().clone(), cache);
    let friends = engine.get_friend_suggestions(&wallet.0, 20).await?;
    Ok(Json(json!({ "suggestions": friends })))
}

pub async fn indexer_metrics(
    Extension(state): Extension<SharedApiState>,
) -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(state.indexer_metrics.snapshot()))
}
