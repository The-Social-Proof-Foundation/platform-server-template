use axum::extract::{Extension, Query};
use axum::Json;
use platform_core::AppError;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiResult;
use crate::middleware::AuthUser;
use crate::recommend::blended::merge_blended_feed;
use crate::recommend::cache::RecommendationCache;
use crate::recommend::friend::FriendRecommendationEngine;
use crate::recommend::timeline::TimelineRecommendationEngine;
use crate::state::SharedApiState;

#[derive(Debug, Deserialize)]
pub struct BlendedFeedQuery {
    #[serde(rename = "chronoLimit", default = "default_limit")]
    pub chrono_limit: i64,
    #[serde(rename = "discoverLimit", default = "default_limit")]
    pub discover_limit: i64,
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Deserialize)]
pub struct ModerationRequest {
    pub r#override: Option<String>,
    #[serde(rename = "reviewedBy")]
    pub reviewed_by: Option<String>,
}

pub async fn recommendation_feed(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
) -> ApiResult<Json<serde_json::Value>> {
    let wallet: (String,) = sqlx::query_as("SELECT wallet_address FROM users WHERE user_id = $1::uuid")
        .bind(&auth.user_id)
        .fetch_one(state.pg_read())
        .await?;
    let cache = RecommendationCache::new(state.redis());
    let engine = TimelineRecommendationEngine::new(state.pg_read().clone(), cache);
    let feed = engine.get_personalized_feed(&wallet.0, 50, true, None).await?;
    Ok(Json(json!({ "feed": feed })))
}

pub async fn blended_feed(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    Query(query): Query<BlendedFeedQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let wallet: (String,) = sqlx::query_as("SELECT wallet_address FROM users WHERE user_id = $1::uuid")
        .bind(&auth.user_id)
        .fetch_one(state.pg_read())
        .await?;

    let chrono_limit = query.chrono_limit.clamp(1, 200) as usize;
    let discover_limit = query.discover_limit.clamp(1, 200) as usize;

    let following_ids = if let Some(client) = &state.mysocial {
        client
            .following_post_ids(&wallet.0, chrono_limit as i64)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let cache = RecommendationCache::new(state.redis());
    let engine = TimelineRecommendationEngine::new(state.pg_read().clone(), cache);
    let discovery = engine
        .get_personalized_feed(&wallet.0, discover_limit as i64, true, None)
        .await?;

    let items = merge_blended_feed(following_ids, discovery, chrono_limit, discover_limit);
    Ok(Json(json!({ "items": items })))
}

pub async fn friend_recommendations(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
) -> ApiResult<Json<serde_json::Value>> {
    let wallet: (String,) = sqlx::query_as("SELECT wallet_address FROM users WHERE user_id = $1::uuid")
        .bind(&auth.user_id)
        .fetch_one(state.pg_read())
        .await?;
    let cache = RecommendationCache::new(state.redis());
    let engine = FriendRecommendationEngine::new(state.pg_read().clone(), cache);
    let friends = engine.get_friend_suggestions(&wallet.0, 20).await?;
    Ok(Json(json!({ "suggestions": friends })))
}

pub async fn moderate_content(
    Extension(state): Extension<SharedApiState>,
    axum::extract::Path(content_id): axum::extract::Path<String>,
    Json(body): Json<ModerationRequest>,
) -> ApiResult<Json<Value>> {
    let override_val = body.r#override.as_deref();
    if let Some(val) = override_val {
        if val != "force_block" && val != "force_allow" {
            return Err(AppError::BadRequest(
                "override must be force_block, force_allow, or null".into(),
            )
            .into());
        }
    }

    sqlx::query(
        "UPDATE content_vectors
         SET moderation_override = $2,
             moderation_reviewed_by = $3,
             moderation_reviewed_at = NOW()
         WHERE content_id = $1",
    )
    .bind(&content_id)
    .bind(override_val)
    .bind(body.reviewed_by.as_deref())
    .execute(state.pg())
    .await?;

    Ok(Json(json!({ "ok": true, "contentId": content_id })))
}

pub async fn indexer_metrics(
    Extension(state): Extension<SharedApiState>,
) -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(state.indexer_metrics.snapshot()))
}
