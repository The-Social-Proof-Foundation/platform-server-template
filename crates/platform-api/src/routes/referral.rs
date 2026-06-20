use axum::extract::{Extension, Query};
use axum::Json;
use platform_db::{list_referrals, record_referral, referral_stats, REFERRALS_REQUIRED};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::ApiResult;
use crate::middleware::AuthUser;
use crate::state::SharedApiState;

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn referral_stats_handler(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
) -> ApiResult<Json<Value>> {
    let (count, threshold_reached) = referral_stats(state.pg_read(), &auth.user_id).await?;
    Ok(Json(json!({
        "referralCount": count,
        "referralsRequired": REFERRALS_REQUIRED,
        "thresholdReached": threshold_reached,
    })))
}

pub async fn list_referrals_handler(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    Query(query): Query<PaginationQuery>,
) -> ApiResult<Json<Value>> {
    let rows = list_referrals(
        state.pg_read(),
        &auth.user_id,
        query.limit.unwrap_or(50),
        query.offset.unwrap_or(0),
    )
    .await?;
    Ok(Json(json!({ "referrals": rows })))
}

#[derive(Debug, Deserialize)]
pub struct RecordReferralRequest {
    pub referred_user_id: Option<String>,
    #[serde(rename = "referredUserId")]
    pub referred_user_id_camel: Option<String>,
    pub referral_code: Option<String>,
    #[serde(rename = "referralCode")]
    pub referral_code_camel: Option<String>,
    pub referrer_id: Option<String>,
    #[serde(rename = "referrerId")]
    pub referrer_id_camel: Option<String>,
}

pub async fn record_referral_handler(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<RecordReferralRequest>,
) -> ApiResult<Json<Value>> {
    let referred_user_id = body
        .referred_user_id
        .or(body.referred_user_id_camel)
        .ok_or_else(|| platform_core::AppError::BadRequest("referredUserId required".into()))?;
    let referral_code = body.referral_code.or(body.referral_code_camel);
    let _ = body.referrer_id.or(body.referrer_id_camel);

    Uuid::parse_str(&referred_user_id)
        .map_err(|_| platform_core::AppError::BadRequest("invalid referredUserId".into()))?;

    record_referral(
        state.pg(),
        &auth.user_id,
        &referred_user_id,
        referral_code.as_deref(),
    )
    .await?;

    Ok(Json(json!({ "ok": true })))
}
