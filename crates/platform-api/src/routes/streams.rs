use axum::extract::Extension;
use axum::Json;
use hmac::{Hmac, Mac};
use platform_core::AppError;
use platform_db::insert_outbox_event;
use serde::Deserialize;
use serde_json::json;
use sha2::Sha256;

use crate::error::ApiResult;
use crate::state::SharedApiState;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Deserialize)]
pub struct StreamWebhookPayload {
    pub user_id: String,
    pub event_type: String,
    #[serde(default)]
    pub data: serde_json::Value,
}

pub async fn webhook(
    Extension(state): Extension<SharedApiState>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<serde_json::Value>> {
    let secret = state
        .config()
        .stream_webhook_secret
        .as_deref()
        .ok_or_else(|| AppError::Config("STREAM_WEBHOOK_SECRET not configured".into()))?;

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

    let payload: StreamWebhookPayload = serde_json::from_slice(&body)
        .map_err(|e| AppError::BadRequest(format!("invalid payload: {e}")))?;

    insert_outbox_event(
        state.pg(),
        "platform.stream.events",
        &payload.event_type,
        json!({
            "userId": payload.user_id,
            "eventType": payload.event_type,
            "data": payload.data,
        }),
        None,
    )
    .await?;

    state
        .notify
        .fanout_stream_event(
            &payload.user_id,
            json!({
                "eventType": payload.event_type,
                "data": payload.data,
            }),
        )
        .await?;

    Ok(Json(json!({ "ok": true })))
}
