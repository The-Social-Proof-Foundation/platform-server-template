use axum::extract::{Extension, Path};
use axum::Json;
use platform_core::AppError;
use serde::Deserialize;
use serde_json::json;
use sqlx::FromRow;
use sqlx::Row;
use uuid::Uuid;

use crate::auth::jwt::{
    persist_refresh_token, sign_access_token, sign_refresh_token,
    validate_refresh_token,
};
use crate::auth::wallet::{
    generate_login_message, issue_auth_nonce, normalize_wallet_address, verify_wallet_signature,
};
use crate::error::ApiResult;
use crate::middleware::AuthUser;
use crate::state::SharedApiState;
use crate::waitlist_events::{
    dispatch_invite_accepted, dispatch_referral_signup_notifications, dispatch_waitlist_joined,
    dispatch_waitlist_approved,
};

#[derive(Deserialize)]
pub struct SignatureRequest {
    pub public_key: String,
}

#[derive(Deserialize)]
pub struct SignupRequest {
    pub username: Option<String>,
    pub full_name: Option<String>,
    pub bio: Option<String>,
    pub public_key: String,
    pub signature: String,
    pub referrer_id: Option<String>,
    #[serde(rename = "referrerId")]
    pub referrer_id_camel: Option<String>,
    pub referral_code: Option<String>,
    #[serde(rename = "referralCode")]
    pub referral_code_camel: Option<String>,
    pub invite_code: Option<String>,
    #[serde(rename = "inviteCode")]
    pub invite_code_camel: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub user_id: Option<String>,
    pub address: String,
    pub signature: String,
}

#[derive(Deserialize)]
pub struct RefreshRequest {
    pub user_id: String,
    pub refresh_token: String,
}

#[derive(Deserialize, serde::Serialize)]
pub struct UpdateUserRequest {
    pub username: Option<String>,
    pub full_name: Option<String>,
    pub bio: Option<String>,
    pub profile_image: Option<String>,
    pub profile_image_icon: Option<String>,
    pub cover_image: Option<String>,
}

#[derive(Deserialize)]
pub struct DeviceTokenRequest {
    pub device_token: String,
    pub device_type: Option<String>,
}

#[derive(Debug, FromRow, serde::Serialize)]
pub struct UserRow {
    pub user_id: Uuid,
    pub wallet_address: String,
    pub username: Option<String>,
    pub full_name: Option<String>,
    pub bio: Option<String>,
    pub role: String,
    pub follower_count: i32,
    pub following_count: i32,
    pub notification_count: i32,
}

const UPDATABLE_FIELDS: &[(&str, &str)] = &[
    ("username", "username"),
    ("full_name", "full_name"),
    ("bio", "bio"),
    ("profile_image", "profile_image"),
    ("profile_image_icon", "profile_image_icon"),
    ("cover_image", "cover_image"),
];

pub async fn request_signature(
    Extension(state): Extension<SharedApiState>,
    Json(body): Json<SignatureRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut redis = state.redis();
    let nonce = issue_auth_nonce(&mut redis, &body.public_key).await?;
    let normalized = normalize_wallet_address(&body.public_key);
    Ok(Json(json!({
        "nonce": nonce,
        "message": generate_login_message(&normalized, &nonce),
        "address": normalized,
    })))
}

pub async fn create_user(
    Extension(state): Extension<SharedApiState>,
    Json(body): Json<SignupRequest>,
) -> ApiResult<(axum::http::StatusCode, Json<UserRow>)> {
    let mut redis = state.redis();
    let wallet = verify_wallet_signature(&mut redis, &body.public_key, &body.signature).await?;

    let existing: Option<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM users WHERE LOWER(public_key) = $1 OR LOWER(wallet_address) = $1",
    )
    .bind(&wallet)
    .fetch_optional(state.pg())
    .await?;

    if existing.is_some() {
        return Err(AppError::Conflict(
            "An account already exists for this wallet address".into(),
        )
        .into());
    }

    let user = sqlx::query_as::<_, UserRow>(
        "INSERT INTO users (wallet_address, public_key, chain_address, username, full_name, bio)
         VALUES ($1,$2,$3,$4,$5,$6)
         RETURNING user_id, wallet_address, username, full_name, bio, role, follower_count, following_count, notification_count",
    )
    .bind(&wallet)
    .bind(&wallet)
    .bind(&wallet)
    .bind(body.username)
    .bind(body.full_name)
    .bind(body.bio)
    .fetch_one(state.pg())
    .await?;

    let user_id = user.user_id.to_string();

    if state.config().waitlist_enabled || state.config().referrals_enabled {
        let _ = platform_db::assign_referral_code(state.pg(), &user_id)
            .await
            .inspect_err(|e| tracing::warn!("Failed to assign referral code: {e}"));
    }

    let invite_code = body.invite_code.or(body.invite_code_camel);
    let mut approved_via_invite = false;

    if state.config().invites_enabled {
        if let Some(ref code) = invite_code {
            if state.config().effective_invite_bypass() {
                match platform_db::accept_invite(state.pg(), &user_id, code, true).await {
                    Ok(invite) => {
                        approved_via_invite = true;
                        if let Some(inviter) = sqlx::query_as::<_, (Uuid,)>(
                            "SELECT inviter_user_id FROM user_invites WHERE invite_id = $1",
                        )
                        .bind(invite.invite_id)
                        .fetch_optional(state.pg())
                        .await?
                        {
                            dispatch_invite_accepted(
                                &state,
                                &inviter.0.to_string(),
                                &user_id,
                            )
                            .await;
                            dispatch_waitlist_approved(&state, &user_id, 0).await;
                        }
                    }
                    Err(err) => tracing::warn!("Invite signup failed: {err}"),
                }
            }
        }
    }

    if state.config().waitlist_enabled && !approved_via_invite {
        let _ = platform_db::join_waitlist(state.pg(), &user_id)
            .await
            .inspect_err(|e| tracing::warn!("Failed to join waitlist: {e}"));
        dispatch_waitlist_joined(&state, &user_id).await;
    }

    if state.config().referrals_enabled && !approved_via_invite {
        let referral_code = body.referral_code.or(body.referral_code_camel);
        let referrer_id = if let Some(code) = referral_code.as_deref() {
            platform_db::resolve_referrer_by_code(state.pg(), code).await?
        } else {
            body.referrer_id.or(body.referrer_id_camel)
        };

        if let Some(referrer_id) = referrer_id {
            match platform_db::record_referral(
                state.pg(),
                &referrer_id,
                &user_id,
                referral_code.as_deref(),
            )
            .await
            {
                Ok((inserted, bump_applied)) if inserted => {
                    dispatch_referral_signup_notifications(
                        &state,
                        &referrer_id,
                        &user_id,
                        bump_applied,
                    )
                    .await;
                }
                Ok(_) => {}
                Err(err) => tracing::warn!("Failed to record referral: {err}"),
            }
        }
    }

    Ok((axum::http::StatusCode::CREATED, Json(user)))
}

pub async fn login(
    Extension(state): Extension<SharedApiState>,
    Json(body): Json<LoginRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut redis = state.redis();
    let wallet = verify_wallet_signature(&mut redis, &body.address, &body.signature).await?;

    let user: UserRow = if let Some(user_id) = body.user_id {
        sqlx::query_as(
            "SELECT user_id, wallet_address, username, full_name, bio, role, follower_count, following_count, notification_count
             FROM users WHERE user_id = $1::uuid AND LOWER(public_key) = $2",
        )
        .bind(user_id)
        .bind(&wallet)
        .fetch_optional(state.pg())
        .await?
        .ok_or(AppError::Unauthorized)?
    } else {
        sqlx::query_as(
            "SELECT user_id, wallet_address, username, full_name, bio, role, follower_count, following_count, notification_count
             FROM users WHERE LOWER(public_key) = $1",
        )
        .bind(&wallet)
        .fetch_optional(state.pg())
        .await?
        .ok_or(AppError::Unauthorized)?
    };

    let access = sign_access_token(
        &user.user_id,
        &state.config().jwt_secret,
        state.config().jwt_access_token_duration_secs,
    )?;
    let refresh = sign_refresh_token(
        &user.user_id,
        &state.config().jwt_refresh_secret,
        state.config().jwt_refresh_token_duration_secs,
    )?;
    persist_refresh_token(
        &mut redis,
        &user.user_id,
        &refresh,
        state.config().jwt_refresh_token_duration_secs as u64,
    )
    .await?;

    Ok(Json(json!({
        "user": user,
        "accessToken": access,
        "refreshToken": refresh,
    })))
}

pub async fn refresh_session(
    Extension(state): Extension<SharedApiState>,
    Json(body): Json<RefreshRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let user_id = Uuid::parse_str(&body.user_id).map_err(|_| AppError::BadRequest("invalid user_id".into()))?;
    let mut redis = state.redis();
    validate_refresh_token(
        &mut redis,
        &user_id,
        &body.refresh_token,
        &state.config().jwt_refresh_secret,
    )
    .await?;

    let access = sign_access_token(
        &user_id,
        &state.config().jwt_secret,
        state.config().jwt_access_token_duration_secs,
    )?;
    Ok(Json(json!({ "accessToken": access })))
}

pub async fn refresh_action(
    Extension(state): Extension<SharedApiState>,
    Json(body): Json<RefreshRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    refresh_session(Extension(state), Json(body)).await
}

pub async fn get_user(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<String>,
) -> ApiResult<Json<UserRow>> {
    let _ = auth;
    let user = sqlx::query_as(
        "SELECT user_id, wallet_address, username, full_name, bio, role, follower_count, following_count, notification_count
         FROM users WHERE user_id = $1::uuid OR wallet_address = $1",
    )
    .bind(id)
    .fetch_optional(state.pg_read())
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(Json(user))
}

pub async fn update_user(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<UpdateUserRequest>,
) -> ApiResult<Json<UserRow>> {
    let mut sets = Vec::new();
    let mut values: Vec<String> = Vec::new();
    let raw = serde_json::to_value(body).unwrap_or_default();
    for (api_field, db_field) in UPDATABLE_FIELDS {
        if let Some(value) = raw.get(api_field).and_then(|v| v.as_str()) {
            values.push(value.to_string());
            sets.push(format!("{db_field} = ${}", values.len()));
        }
    }
    if sets.is_empty() {
        return Err(AppError::BadRequest("No valid fields provided for update".into()).into());
    }
    values.push(auth.user_id.clone());
    let sql = format!(
        "UPDATE users SET {} WHERE user_id = ${}::uuid
         RETURNING user_id, wallet_address, username, full_name, bio, role, follower_count, following_count, notification_count",
        sets.join(", "),
        values.len()
    );

    let mut query = sqlx::query_as::<_, UserRow>(&sql);
    for value in values {
        query = query.bind(value);
    }
    Ok(Json(query.fetch_one(state.pg()).await?))
}

pub async fn register_device_token(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<DeviceTokenRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query(
        "INSERT INTO device_tokens (user_id, device_token, device_type)
         VALUES ($1::uuid, $2, $3)
         ON CONFLICT (user_id, device_token) DO UPDATE SET device_type = EXCLUDED.device_type, updated_at = NOW()",
    )
    .bind(auth.user_id)
    .bind(body.device_token)
    .bind(body.device_type.unwrap_or_else(|| "ios".into()))
    .execute(state.pg())
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn follow_user(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    Path(followee): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let follower_wallet: (String,) = sqlx::query_as("SELECT wallet_address FROM users WHERE user_id = $1::uuid")
        .bind(&auth.user_id)
        .fetch_one(state.pg())
        .await?;

    let deleted = sqlx::query(
        "DELETE FROM follows WHERE follower_wallet_address = $1 AND followee_wallet_address = $2",
    )
    .bind(&follower_wallet.0)
    .bind(&followee)
    .execute(state.pg())
    .await?;

    if deleted.rows_affected() > 0 {
        return Ok(Json(json!({ "following": false })));
    }

    sqlx::query(
        "INSERT INTO follows (follower_wallet_address, followee_wallet_address) VALUES ($1, $2)",
    )
    .bind(&follower_wallet.0)
    .bind(&followee)
    .execute(state.pg())
    .await?;

    Ok(Json(json!({ "following": true })))
}

pub async fn block_user(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    Path(blocked): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let blocker_wallet: (String,) = sqlx::query_as("SELECT wallet_address FROM users WHERE user_id = $1::uuid")
        .bind(&auth.user_id)
        .fetch_one(state.pg())
        .await?;

    let deleted = sqlx::query(
        "DELETE FROM blocked WHERE blocker_wallet_address = $1 AND blocked_wallet_address = $2",
    )
    .bind(&blocker_wallet.0)
    .bind(&blocked)
    .execute(state.pg())
    .await?;

    if deleted.rows_affected() > 0 {
        return Ok(Json(json!({ "blocked": false })));
    }

    sqlx::query(
        "INSERT INTO blocked (blocker_wallet_address, blocked_wallet_address) VALUES ($1, $2)",
    )
    .bind(&blocker_wallet.0)
    .bind(&blocked)
    .execute(state.pg())
    .await?;

    Ok(Json(json!({ "blocked": true })))
}

pub async fn get_blocked(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
) -> ApiResult<Json<Vec<String>>> {
    let wallet: (String,) = sqlx::query_as("SELECT wallet_address FROM users WHERE user_id = $1::uuid")
        .bind(&auth.user_id)
        .fetch_one(state.pg_read())
        .await?;
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT blocked_wallet_address FROM blocked WHERE blocker_wallet_address = $1",
    )
    .bind(wallet.0)
    .fetch_all(state.pg_read())
    .await?;
    Ok(Json(rows.into_iter().map(|r| r.0).collect()))
}

pub async fn get_notifications(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let rows = sqlx::query(
        "SELECT notification_id, type, object_id, object_type, title, message, created_at
         FROM notifications WHERE user_id = $1::uuid ORDER BY created_at DESC LIMIT 100",
    )
    .bind(&auth.user_id)
    .fetch_all(state.pg_read())
    .await?;

    let notifications = rows
        .into_iter()
        .map(|row| {
            json!({
                "notificationId": row.get::<Uuid, _>("notification_id"),
                "type": row.get::<String, _>("type"),
                "objectId": row.get::<Option<String>, _>("object_id"),
                "objectType": row.get::<Option<String>, _>("object_type"),
                "title": row.get::<Option<String>, _>("title"),
                "message": row.get::<Option<String>, _>("message"),
                "createdAt": row.get::<i64, _>("created_at"),
            })
        })
        .collect();
    Ok(Json(notifications))
}

pub async fn mark_notifications_read(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query("UPDATE notifications SET read_at = NOW() WHERE user_id = $1::uuid AND read_at IS NULL")
        .bind(&auth.user_id)
        .execute(state.pg())
        .await?;
    Ok(Json(json!({ "ok": true })))
}
