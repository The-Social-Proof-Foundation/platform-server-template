use axum::extract::{Extension, Path, Query};
use axum::Json;
use chrono::Utc;
use platform_db::{
    admin_stats, approve_user, count_active_invites, count_circulating_invites, create_invite,
    get_config, get_referral_code, get_status, resume_batches, run_admission_batch, set_config,
    set_paused, set_user_invites_enabled, validate_mint_invites_count, waitlist_entry_status,
    resolve_waitlist_user, MAX_INVITES_PER_USER, POSITION_ESTIMATE_BATCHES,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::ApiResult;
use crate::middleware::AuthUser;
use crate::state::SharedApiState;
use crate::waitlist_events::{dispatch_batch_admissions, dispatch_waitlist_approved};

pub async fn waitlist_status_handler(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
) -> ApiResult<Json<Value>> {
    let status = get_status(state.pg_read(), &auth.user_id).await?;
    let config = get_config(state.pg()).await.ok();
    let circulation = if state.config().invites_enabled {
        count_circulating_invites(state.pg_read()).await.ok()
    } else {
        None
    };

    let batch_admission_enabled = state.config().effective_batch_admission();
    let invite_bypass_enabled = state.config().effective_invite_bypass();

    let next_batch_at = if batch_admission_enabled {
        config.as_ref().and_then(|c| c.next_batch_at)
    } else {
        None
    };

    let waitlist_status = status
        .as_ref()
        .map(|s| s.status.as_str())
        .unwrap_or("none");

    Ok(Json(json!({
        "status": waitlist_status,
        "positionEstimate": status.as_ref().and_then(|s| s.position_estimate),
        "referralBumps": status.as_ref().map(|s| s.referral_bumps).unwrap_or(0),
        "nextBatchAt": next_batch_at,
        "positionEstimateBatches": POSITION_ESTIMATE_BATCHES,
        "referralCode": status.as_ref().and_then(|s| s.referral_code.clone()),
        "batchAdmissionEnabled": batch_admission_enabled,
        "inviteBypassEnabled": invite_bypass_enabled,
        "activeInviteCodesInCirculation": circulation.as_ref().map(|c| c.active_invite_codes),
        "paused": config.as_ref().map(|c| c.is_paused).unwrap_or(false),
    })))
}

pub async fn invite_circulation_handler(
    Extension(state): Extension<SharedApiState>,
) -> ApiResult<Json<Value>> {
    if !state.config().invites_enabled || !state.config().invite_circulation_public {
        return Err(platform_core::AppError::NotFound.into());
    }

    let stats = count_circulating_invites(state.pg_read()).await?;
    Ok(Json(json!({
        "activeInviteCodes": stats.active_invite_codes,
        "uniqueInviters": stats.unique_inviters,
        "createdLast24h": stats.created_last_24h,
        "asOf": Utc::now(),
        "message": "Early-access invite codes are hidden — find one to skip the waitlist",
    })))
}

pub async fn admin_get_config_handler(
    Extension(state): Extension<SharedApiState>,
) -> ApiResult<Json<Value>> {
    let config = get_config(state.pg()).await?;
    let stats = admin_stats(state.pg()).await?;
    Ok(Json(json!({
        "admissionIntervalHours": config.admission_interval_hours,
        "spotsPerBatch": config.spots_per_batch,
        "isPaused": config.is_paused,
        "lastBatchAt": config.last_batch_at,
        "nextBatchAt": config.next_batch_at,
        "batchAdmissionEnabled": state.config().effective_batch_admission(),
        "inviteBypassEnabled": state.config().effective_invite_bypass(),
        "waitingCount": stats.waiting_count,
        "approvedCount": stats.approved_count,
    })))
}

#[derive(Debug, Deserialize)]
pub struct AdminConfigRequest {
    pub admission_interval_hours: Option<i32>,
    #[serde(rename = "admissionIntervalHours")]
    pub admission_interval_hours_camel: Option<i32>,
    pub spots_per_batch: Option<i32>,
    #[serde(rename = "spotsPerBatch")]
    pub spots_per_batch_camel: Option<i32>,
}

pub async fn admin_set_config_handler(
    Extension(state): Extension<SharedApiState>,
    Json(body): Json<AdminConfigRequest>,
) -> ApiResult<Json<Value>> {
    let current = get_config(state.pg()).await?;
    let interval = body
        .admission_interval_hours
        .or(body.admission_interval_hours_camel)
        .unwrap_or(current.admission_interval_hours);
    let spots = body
        .spots_per_batch
        .or(body.spots_per_batch_camel)
        .unwrap_or(current.spots_per_batch);

    let config = set_config(state.pg(), interval, spots).await?;
    Ok(Json(json!({
        "admissionIntervalHours": config.admission_interval_hours,
        "spotsPerBatch": config.spots_per_batch,
        "isPaused": config.is_paused,
        "nextBatchAt": config.next_batch_at,
    })))
}

pub async fn admin_pause_handler(
    Extension(state): Extension<SharedApiState>,
) -> ApiResult<Json<Value>> {
    let config = set_paused(state.pg(), true).await?;
    Ok(Json(json!({ "isPaused": config.is_paused })))
}

pub async fn admin_resume_handler(
    Extension(state): Extension<SharedApiState>,
) -> ApiResult<Json<Value>> {
    let config = resume_batches(state.pg()).await?;
    Ok(Json(json!({
        "isPaused": config.is_paused,
        "nextBatchAt": config.next_batch_at,
    })))
}

#[derive(Debug, Deserialize)]
pub struct RunBatchQuery {
    pub force: Option<bool>,
}

pub async fn admin_run_batch_handler(
    Extension(state): Extension<SharedApiState>,
    Query(query): Query<RunBatchQuery>,
) -> ApiResult<Json<Value>> {
    let batch_enabled = state.config().effective_batch_admission() || query.force.unwrap_or(false);
    let result = run_admission_batch(state.pg(), batch_enabled).await?;
    dispatch_batch_admissions(&state, &result.admitted_user_ids).await;
    Ok(Json(json!({
        "admitted": result.admitted_user_ids,
        "skipped": result.skipped,
    })))
}

pub async fn admin_approve_user_handler(
    Extension(state): Extension<SharedApiState>,
    Path(user_id): Path<String>,
) -> ApiResult<Json<Value>> {
    Uuid::parse_str(&user_id)
        .map_err(|_| platform_core::AppError::BadRequest("invalid user id".into()))?;
    let approved = approve_user(state.pg(), &user_id, "admin").await?;
    if approved {
        dispatch_waitlist_approved(&state, &user_id, 0).await;
    }
    Ok(Json(json!({ "approved": approved })))
}

#[derive(Debug, Deserialize)]
pub struct AdminGrantAccessRequest {
    pub user_id: Option<String>,
    #[serde(rename = "userId")]
    pub user_id_camel: Option<String>,
    pub wallet_address: Option<String>,
    #[serde(rename = "walletAddress")]
    pub wallet_address_camel: Option<String>,
    pub mint_invites: Option<u32>,
    #[serde(rename = "mintInvites")]
    pub mint_invites_camel: Option<u32>,
}

pub async fn admin_grant_access_handler(
    Extension(state): Extension<SharedApiState>,
    Json(body): Json<AdminGrantAccessRequest>,
) -> ApiResult<Json<Value>> {
    if !state.config().invites_enabled {
        return Err(platform_core::AppError::BadRequest(
            "INVITES_ENABLED must be true to grant access with invite codes".into(),
        )
        .into());
    }

    let user_id = body.user_id.or(body.user_id_camel);
    let wallet_address = body.wallet_address.or(body.wallet_address_camel);
    if user_id.is_some() && wallet_address.is_some() {
        return Err(platform_core::AppError::BadRequest(
            "Provide exactly one of userId or walletAddress".into(),
        )
        .into());
    }

    let mint_invites = body
        .mint_invites
        .or(body.mint_invites_camel)
        .ok_or_else(|| platform_core::AppError::BadRequest("mintInvites is required".into()))?;

    let user = resolve_waitlist_user(state.pg(), user_id.as_deref(), wallet_address.as_deref()).await?;
    let target_user_id = user.user_id.to_string();

    let entry_status = waitlist_entry_status(state.pg(), &target_user_id).await?;
    let already_approved = matches!(entry_status.as_deref(), Some("approved"));

    let newly_approved = if already_approved {
        false
    } else {
        approve_user(state.pg(), &target_user_id, "admin").await?
    };

    set_user_invites_enabled(state.pg(), &target_user_id, true).await?;

    let active = count_active_invites(state.pg(), &target_user_id).await?;
    validate_mint_invites_count(mint_invites, active, MAX_INVITES_PER_USER)?;

    let mut invites_minted = Vec::new();
    for _ in 0..mint_invites {
        let invite = create_invite(state.pg(), &target_user_id, true).await?;
        invites_minted.push(invite);
    }

    dispatch_waitlist_approved(&state, &target_user_id, mint_invites).await;

    Ok(Json(json!({
        "approved": newly_approved || already_approved,
        "alreadyApproved": already_approved,
        "user": {
            "userId": user.user_id,
            "walletAddress": user.wallet_address,
            "username": user.username,
            "fullName": user.full_name,
            "role": user.role,
        },
        "invitesEnabled": true,
        "invitesMinted": invites_minted,
        "notificationsSent": {
            "waitlistApproved": true,
        },
    })))
}

#[derive(Debug, Deserialize)]
pub struct UserInvitesRequest {
    pub enabled: bool,
}

pub async fn admin_user_invites_handler(
    Extension(state): Extension<SharedApiState>,
    Path(user_id): Path<String>,
    Json(body): Json<UserInvitesRequest>,
) -> ApiResult<Json<Value>> {
    Uuid::parse_str(&user_id)
        .map_err(|_| platform_core::AppError::BadRequest("invalid user id".into()))?;
    set_user_invites_enabled(state.pg(), &user_id, body.enabled).await?;
    Ok(Json(json!({ "invitesEnabled": body.enabled })))
}

pub async fn referral_code_handler(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
) -> ApiResult<Json<Value>> {
    let code = get_referral_code(state.pg_read(), &auth.user_id)
        .await?
        .ok_or(platform_core::AppError::NotFound)?;
    Ok(Json(json!({
        "referralCode": code,
        "shareUrl": format!("?referralCode={code}"),
    })))
}
