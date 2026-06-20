use axum::extract::Extension;
use axum::body::Bytes;
use axum::http::HeaderMap;
use axum::Json;
use hmac::{Hmac, Mac};
use platform_indexer::handlers::dispatcher::{handle_parsed_event, EventMeta};
use platform_indexer::parsers::ParsedChainEvent;
use platform_core::AppError;
use serde::Deserialize;
use serde_json::json;
use sha2::Sha256;

use crate::error::ApiResult;
use crate::state::SharedApiState;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Deserialize)]
pub struct SocialEventWebhookPayload {
    pub event: ParsedChainEvent,
    pub tx_digest: String,
    pub checkpoint_seq: i64,
}

pub async fn social_events(
    Extension(state): Extension<SharedApiState>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<serde_json::Value>> {
    let secret = state
        .config()
        .social_webhook_secret
        .as_deref()
        .ok_or_else(|| AppError::Config("SOCIAL_WEBHOOK_SECRET not configured".into()))?;

    let signature = headers
        .get("x-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| AppError::Internal(e.to_string()))?;
    mac.update(&body);
    let expected = hex::encode(mac.finalize().into_bytes());
    if expected != signature {
        return Err(AppError::Unauthorized.into());
    }

    let payload: SocialEventWebhookPayload = serde_json::from_slice(&body)
        .map_err(|e| AppError::BadRequest(format!("invalid payload: {e}")))?;

    let platform_id = state
        .config()
        .platform_id
        .as_deref()
        .unwrap_or_default()
        .to_string();

    let mut redis = state.redis();
    handle_parsed_event(
        state.pg(),
        &mut redis,
        &state.notify,
        Some(state.embeddings.as_ref()),
        &platform_id,
        payload.event,
        EventMeta {
            tx_digest: payload.tx_digest,
            checkpoint_seq: payload.checkpoint_seq,
        },
    )
    .await?;

    Ok(Json(json!({ "ok": true })))
}
