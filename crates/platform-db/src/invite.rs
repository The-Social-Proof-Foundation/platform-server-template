use platform_core::{AppError, AppResult};
use rand::Rng;
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::waitlist;

/// Max active invite codes a single user can create.
pub const MAX_INVITES_PER_USER: u32 = 10;

/// Days until an unused invite expires.
pub const INVITE_EXPIRY_DAYS: u32 = 7;

/// Max invites a user can accept (optional cap on invitee side).
pub const MAX_ACCEPTED_INVITES_PER_USER: u32 = 1;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct InviteRow {
    pub invite_id: Uuid,
    pub invite_code: String,
    pub invitee_user_id: Option<Uuid>,
    pub status: String,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub accepted_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InviteCirculationStats {
    pub active_invite_codes: i64,
    pub unique_inviters: i64,
    pub created_last_24h: i64,
}

pub async fn count_circulating_invites(pool: &PgPool) -> AppResult<InviteCirculationStats> {
    let active: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM user_invites
         WHERE status = 'pending'
           AND (expires_at IS NULL OR expires_at > NOW())",
    )
    .fetch_one(pool)
    .await?;

    let unique: (i64,) = sqlx::query_as(
        "SELECT COUNT(DISTINCT inviter_user_id) FROM user_invites
         WHERE status = 'pending'
           AND (expires_at IS NULL OR expires_at > NOW())",
    )
    .fetch_one(pool)
    .await?;

    let recent: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM user_invites
         WHERE created_at >= NOW() - INTERVAL '24 hours'",
    )
    .fetch_one(pool)
    .await?;

    Ok(InviteCirculationStats {
        active_invite_codes: active.0,
        unique_inviters: unique.0,
        created_last_24h: recent.0,
    })
}

pub async fn count_active_invites(pool: &PgPool, inviter_user_id: &str) -> AppResult<i64> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM user_invites
         WHERE inviter_user_id = $1::uuid AND status = 'pending'
           AND (expires_at IS NULL OR expires_at > NOW())",
    )
    .bind(inviter_user_id)
    .fetch_one(pool)
    .await?;
    Ok(count.0)
}

pub async fn list_invites(
    pool: &PgPool,
    inviter_user_id: &str,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<InviteRow>> {
    Ok(sqlx::query_as(
        "SELECT invite_id, invite_code, invitee_user_id, status, expires_at, created_at, accepted_at
         FROM user_invites
         WHERE inviter_user_id = $1::uuid
         ORDER BY created_at DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(inviter_user_id)
    .bind(limit.clamp(1, 200))
    .bind(offset.max(0))
    .fetch_all(pool)
    .await?)
}

pub async fn create_invite(
    pool: &PgPool,
    inviter_user_id: &str,
    waitlist_enabled: bool,
) -> AppResult<InviteRow> {
    if waitlist_enabled && !waitlist::can_user_create_invites(pool, inviter_user_id).await? {
        return Err(AppError::Forbidden);
    }

    let active = count_active_invites(pool, inviter_user_id).await?;
    if active >= i64::from(MAX_INVITES_PER_USER) {
        return Err(AppError::BadRequest(format!(
            "Maximum of {MAX_INVITES_PER_USER} active invites reached"
        )));
    }

    let invite_code = generate_invite_code();
    let expires_at =
        chrono::Utc::now() + chrono::Duration::days(i64::from(INVITE_EXPIRY_DAYS));

    Ok(sqlx::query_as(
        "INSERT INTO user_invites (inviter_user_id, invite_code, expires_at)
         VALUES ($1::uuid, $2, $3)
         RETURNING invite_id, invite_code, invitee_user_id, status, expires_at, created_at, accepted_at",
    )
    .bind(inviter_user_id)
    .bind(&invite_code)
    .bind(expires_at)
    .fetch_one(pool)
    .await?)
}

pub async fn get_invite_by_code(pool: &PgPool, invite_code: &str) -> AppResult<Option<InviteRow>> {
    Ok(sqlx::query_as(
        "SELECT invite_id, invite_code, invitee_user_id, status, expires_at, created_at, accepted_at
         FROM user_invites WHERE invite_code = $1",
    )
    .bind(invite_code)
    .fetch_optional(pool)
    .await?)
}

pub async fn count_accepted_invites(pool: &PgPool, invitee_user_id: &str) -> AppResult<i64> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM user_invites
         WHERE invitee_user_id = $1::uuid AND status = 'accepted'",
    )
    .bind(invitee_user_id)
    .fetch_one(pool)
    .await?;
    Ok(count.0)
}

pub async fn accept_invite(
    pool: &PgPool,
    invitee_user_id: &str,
    invite_code: &str,
    approve_waitlist: bool,
) -> AppResult<InviteRow> {
    let invite = get_invite_by_code(pool, invite_code)
        .await?
        .ok_or_else(|| AppError::NotFound)?;

    if invite.status != "pending" {
        return Err(AppError::BadRequest("Invite is no longer valid".into()));
    }

    if let Some(expires_at) = invite.expires_at {
        if expires_at < chrono::Utc::now() {
            sqlx::query("UPDATE user_invites SET status = 'expired' WHERE invite_id = $1")
                .bind(invite.invite_id)
                .execute(pool)
                .await?;
            return Err(AppError::BadRequest("Invite has expired".into()));
        }
    }

    let inviter_id = sqlx::query_as::<_, (Uuid,)>(
        "SELECT inviter_user_id FROM user_invites WHERE invite_id = $1",
    )
    .bind(invite.invite_id)
    .fetch_one(pool)
    .await?;

    if inviter_id.0.to_string() == invitee_user_id {
        return Err(AppError::BadRequest("Cannot accept your own invite".into()));
    }

    let accepted = count_accepted_invites(pool, invitee_user_id).await?;
    if accepted >= i64::from(MAX_ACCEPTED_INVITES_PER_USER) {
        return Err(AppError::BadRequest("Invite acceptance limit reached".into()));
    }

    let updated = sqlx::query_as(
        "UPDATE user_invites
         SET invitee_user_id = $2::uuid, status = 'accepted', accepted_at = NOW()
         WHERE invite_id = $1 AND status = 'pending'
         RETURNING invite_id, invite_code, invitee_user_id, status, expires_at, created_at, accepted_at",
    )
    .bind(invite.invite_id)
    .bind(invitee_user_id)
    .fetch_one(pool)
    .await?;

    on_invite_accepted(
        pool,
        &inviter_id.0.to_string(),
        invitee_user_id,
        invite_code,
        approve_waitlist,
    )
    .await?;

    Ok(updated)
}

/// Called when an invite is accepted by a new or existing user.
pub async fn on_invite_accepted(
    pool: &PgPool,
    inviter_user_id: &str,
    invitee_user_id: &str,
    invite_code: &str,
    approve_waitlist: bool,
) -> AppResult<()> {
    if approve_waitlist {
        waitlist::approve_user(pool, invitee_user_id, "invite").await?;
    }
    let _ = (inviter_user_id, invite_code);
    Ok(())
}

fn generate_invite_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..8)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}
