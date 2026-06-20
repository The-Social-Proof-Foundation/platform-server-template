use platform_core::{AppError, AppResult};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// Number of successful referrals before the reward hook fires.
pub const REFERRALS_REQUIRED: u32 = 5;

/// Minimum account age (days) before a referral counts — optional anti-abuse knob.
pub const REFERRAL_MIN_ACCOUNT_AGE_DAYS: u32 = 0;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ReferralRow {
    pub referral_id: Uuid,
    pub referred_user_id: Uuid,
    pub referral_code: Option<String>,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReferralRewardResult {
    pub referrer_user_id: String,
    pub referral_count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecordReferralOutcome {
    pub inserted: bool,
    pub bump_applied: bool,
    pub reward: Option<ReferralRewardResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmailVerificationToken {
    pub token: String,
    pub user_id: String,
}

pub async fn count_completed_referrals(pool: &PgPool, referrer_user_id: &str) -> AppResult<i64> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM user_referrals
         WHERE referrer_user_id = $1::uuid AND status IN ('completed', 'rewarded')",
    )
    .bind(referrer_user_id)
    .fetch_one(pool)
    .await?;
    Ok(count.0)
}

pub async fn list_referrals(
    pool: &PgPool,
    referrer_user_id: &str,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<ReferralRow>> {
    Ok(sqlx::query_as(
        "SELECT referral_id, referred_user_id, referral_code, status, created_at
         FROM user_referrals
         WHERE referrer_user_id = $1::uuid
         ORDER BY created_at DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(referrer_user_id)
    .bind(limit.clamp(1, 200))
    .bind(offset.max(0))
    .fetch_all(pool)
    .await?)
}

pub async fn record_referral(
    pool: &PgPool,
    referrer_user_id: &str,
    referred_user_id: &str,
    referral_code: Option<&str>,
) -> AppResult<RecordReferralOutcome> {
    if referrer_user_id == referred_user_id {
        return Err(AppError::BadRequest("Cannot refer yourself".into()));
    }

    let inserted = sqlx::query(
        "INSERT INTO user_referrals (referrer_user_id, referred_user_id, referral_code, status)
         VALUES ($1::uuid, $2::uuid, $3, 'completed')
         ON CONFLICT (referred_user_id) DO NOTHING",
    )
    .bind(referrer_user_id)
    .bind(referred_user_id)
    .bind(referral_code)
    .execute(pool)
    .await?;

    if inserted.rows_affected() > 0 {
        let bump = crate::waitlist::apply_referral_bump(pool, referrer_user_id).await?;
        let count = count_completed_referrals(pool, referrer_user_id).await? as u32;
        let reward = if count >= REFERRALS_REQUIRED {
            mark_referrer_rewarded_if_needed(pool, referrer_user_id).await?;
            on_referral_threshold_reached(pool, referrer_user_id, count).await?
        } else {
            None
        };
        return Ok(RecordReferralOutcome {
            inserted: true,
            bump_applied: bump,
            reward,
        });
    }

    Ok(RecordReferralOutcome {
        inserted: false,
        bump_applied: false,
        reward: None,
    })
}

async fn mark_referrer_rewarded_if_needed(pool: &PgPool, referrer_user_id: &str) -> AppResult<()> {
    sqlx::query(
        "UPDATE user_referrals SET status = 'rewarded'
         WHERE referrer_user_id = $1::uuid AND status = 'completed'",
    )
    .bind(referrer_user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Called once when referrer reaches REFERRALS_REQUIRED completed referrals.
pub async fn on_referral_threshold_reached(
    pool: &PgPool,
    referrer_user_id: &str,
    referral_count: u32,
) -> AppResult<Option<ReferralRewardResult>> {
    let already_claimed =
        crate::settings::get_bool_setting(pool, referrer_user_id, "referral.reward.claimed", false)
            .await?;
    if already_claimed {
        return Ok(None);
    }

    crate::settings::upsert_setting(pool, referrer_user_id, "referral.reward.claimed", "true")
        .await?;

    Ok(Some(ReferralRewardResult {
        referrer_user_id: referrer_user_id.to_string(),
        referral_count,
    }))
}

pub async fn resolve_referrer_by_code(
    pool: &PgPool,
    referral_code: &str,
) -> AppResult<Option<String>> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM users WHERE referral_code = $1",
    )
    .bind(referral_code)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id,)| id.to_string()))
}

pub async fn referral_stats(pool: &PgPool, referrer_user_id: &str) -> AppResult<(i64, bool)> {
    let count = count_completed_referrals(pool, referrer_user_id).await?;
    let threshold_reached = count >= i64::from(REFERRALS_REQUIRED);
    Ok((count, threshold_reached))
}

pub async fn set_email_verification_token(
    pool: &PgPool,
    user_id: &str,
    email: &str,
    token: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE users SET email = $2, email_verification_token = $3, email_verification_sent_at = NOW(), email_verified_at = NULL
         WHERE user_id = $1::uuid",
    )
    .bind(user_id)
    .bind(email)
    .bind(token)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn verify_email_by_token(pool: &PgPool, token: &str) -> AppResult<Option<String>> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "UPDATE users SET email_verified_at = NOW(), email_verification_token = NULL
         WHERE email_verification_token = $1
         RETURNING user_id",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id,)| id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn referral_reward_result_serializes() {
        let reward = ReferralRewardResult {
            referrer_user_id: "user".into(),
            referral_count: 5,
        };
        assert!(serde_json::to_string(&reward).unwrap().contains("referral_count"));
    }
}
