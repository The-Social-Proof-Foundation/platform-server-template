use axum::extract::Extension;
use axum::Json;
use platform_core::AppError;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiResult;
use crate::middleware::AuthUser;
use crate::state::SharedApiState;

#[derive(Debug, Deserialize)]
pub struct InteractionRequest {
    #[serde(rename = "contentId")]
    pub content_id: String,
    #[serde(rename = "interactionType")]
    pub interaction_type: String,
    #[serde(rename = "engagementScore")]
    pub engagement_score: Option<f32>,
    #[serde(rename = "watchDuration")]
    pub watch_duration: Option<i32>,
    #[serde(rename = "contextData", default)]
    pub context_data: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct BatchInteractionsRequest {
    pub events: Vec<InteractionRequest>,
}

const ALLOWED_INTERACTION_TYPES: &[&str] = &["watch", "open", "skip", "share"];
const MAX_BATCH_SIZE: usize = 50;

pub fn validate_interaction(body: &InteractionRequest) -> ApiResult<()> {
    if body.content_id.trim().is_empty() {
        return Err(AppError::BadRequest("contentId required".into()).into());
    }
    if !ALLOWED_INTERACTION_TYPES
        .iter()
        .any(|t| t.eq_ignore_ascii_case(&body.interaction_type))
    {
        return Err(AppError::BadRequest(format!(
            "interactionType must be one of: {}",
            ALLOWED_INTERACTION_TYPES.join(", ")
        ))
        .into());
    }
    Ok(())
}

pub async fn record_interaction(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<InteractionRequest>,
) -> ApiResult<Json<Value>> {
    validate_interaction(&body)?;
    let wallet: (String,) = sqlx::query_as("SELECT wallet_address FROM users WHERE user_id = $1::uuid")
        .bind(&auth.user_id)
        .fetch_one(state.pg())
        .await?;

    platform_db::insert_interaction(
        state.pg(),
        &wallet.0,
        &body.content_id,
        &body.interaction_type,
        body.engagement_score,
        body.watch_duration,
        body.context_data.clone(),
    )
    .await?;

    Ok(Json(json!({ "ok": true })))
}

pub async fn record_interactions_batch(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<BatchInteractionsRequest>,
) -> ApiResult<Json<Value>> {
    if body.events.is_empty() {
        return Err(AppError::BadRequest("events required".into()).into());
    }
    if body.events.len() > MAX_BATCH_SIZE {
        return Err(AppError::BadRequest(format!(
            "batch limited to {MAX_BATCH_SIZE} events"
        ))
        .into());
    }

    let wallet: (String,) = sqlx::query_as("SELECT wallet_address FROM users WHERE user_id = $1::uuid")
        .bind(&auth.user_id)
        .fetch_one(state.pg())
        .await?;

    for event in &body.events {
        validate_interaction(event)?;
        platform_db::insert_interaction(
            state.pg(),
            &wallet.0,
            &event.content_id,
            &event.interaction_type,
            event.engagement_score,
            event.watch_duration,
            event.context_data.clone(),
        )
        .await?;
        let _ = platform_db::update_engagement_patterns(
            state.pg(),
            &wallet.0,
            &event.content_id,
            &event.interaction_type,
        )
        .await;
    }

    Ok(Json(json!({ "ok": true, "count": body.events.len() })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interaction_type_validation() {
        let ok = InteractionRequest {
            content_id: "0xabc".into(),
            interaction_type: "watch".into(),
            engagement_score: Some(0.5),
            watch_duration: Some(10),
            context_data: None,
        };
        assert!(validate_interaction(&ok).is_ok());

        let bad = InteractionRequest {
            interaction_type: "invalid".into(),
            ..ok
        };
        assert!(validate_interaction(&bad).is_err());
    }
}
