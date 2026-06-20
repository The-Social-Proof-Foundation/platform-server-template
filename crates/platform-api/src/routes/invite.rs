use axum::extract::{Extension, Path, Query};
use axum::Json;
use platform_db::{accept_invite, create_invite, get_invite_by_code, list_invites};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiResult;
use crate::middleware::AuthUser;
use crate::state::SharedApiState;
use crate::waitlist_events::{dispatch_invite_accepted, dispatch_waitlist_approved};

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn create_invite_handler(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
) -> ApiResult<(axum::http::StatusCode, Json<Value>)> {
    let invite = create_invite(
        state.pg(),
        &auth.user_id,
        state.config().waitlist_enabled,
    )
    .await?;
    Ok((axum::http::StatusCode::CREATED, Json(json!({ "invite": invite }))))
}

pub async fn list_invites_handler(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    Query(query): Query<PaginationQuery>,
) -> ApiResult<Json<Value>> {
    let rows = list_invites(
        state.pg_read(),
        &auth.user_id,
        query.limit.unwrap_or(50),
        query.offset.unwrap_or(0),
    )
    .await?;
    Ok(Json(json!({ "invites": rows })))
}

#[derive(Debug, Deserialize)]
pub struct AcceptInviteRequest {
    pub invite_code: Option<String>,
    #[serde(rename = "inviteCode")]
    pub invite_code_camel: Option<String>,
}

pub async fn accept_invite_handler(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<AcceptInviteRequest>,
) -> ApiResult<Json<Value>> {
    let invite_code = body
        .invite_code
        .or(body.invite_code_camel)
        .ok_or_else(|| platform_core::AppError::BadRequest("inviteCode required".into()))?;

    let approve_waitlist = state.config().effective_invite_bypass();
    let invite = accept_invite(
        state.pg(),
        &auth.user_id,
        &invite_code,
        approve_waitlist,
    )
    .await?;

    if approve_waitlist {
        dispatch_waitlist_approved(&state, &auth.user_id, 0).await;
        if let Some(inviter) = sqlx::query_as::<_, (uuid::Uuid,)>(
            "SELECT inviter_user_id FROM user_invites WHERE invite_id = $1",
        )
        .bind(invite.invite_id)
        .fetch_optional(state.pg())
        .await?
        {
            dispatch_invite_accepted(&state, &inviter.0.to_string(), &auth.user_id).await;
        }
    }

    Ok(Json(json!({ "invite": invite })))
}

pub async fn preview_invite_handler(
    Extension(state): Extension<SharedApiState>,
    Path(code): Path<String>,
) -> ApiResult<Json<Value>> {
    let invite = get_invite_by_code(state.pg_read(), &code)
        .await?
        .ok_or(platform_core::AppError::NotFound)?;

    let expired = invite
        .expires_at
        .is_some_and(|t| t < chrono::Utc::now());

    Ok(Json(json!({
        "inviteCode": invite.invite_code,
        "status": invite.status,
        "expired": expired || invite.status != "pending",
        "expiresAt": invite.expires_at,
    })))
}
