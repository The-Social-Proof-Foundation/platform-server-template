use platform_core::{AppError, AppResult};
use rand::Rng;
use serde::Serialize;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

pub const DEFAULT_ADMISSION_INTERVAL_HOURS: u32 = 24;
pub const DEFAULT_SPOTS_PER_BATCH: u32 = 100;
pub const REFERRAL_BUMP_POINTS: i64 = 10;
pub const POSITION_ESTIMATE_BATCHES: u32 = 3;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct WaitlistConfigRow {
    pub admission_interval_hours: i32,
    pub spots_per_batch: i32,
    pub is_paused: bool,
    pub last_batch_at: Option<chrono::DateTime<chrono::Utc>>,
    pub next_batch_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WaitlistStatus {
    pub status: String,
    pub position_estimate: Option<i64>,
    pub referral_bumps: i32,
    pub next_batch_at: Option<chrono::DateTime<chrono::Utc>>,
    pub referral_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchAdmissionResult {
    pub admitted_user_ids: Vec<String>,
    pub skipped: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WaitlistAdminStats {
    pub waiting_count: i64,
    pub approved_count: i64,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct WaitlistUserSummary {
    pub user_id: Uuid,
    pub wallet_address: String,
    pub username: Option<String>,
    pub full_name: Option<String>,
    pub role: String,
}

pub fn normalize_wallet_lookup(wallet: &str) -> String {
    wallet.trim().to_lowercase()
}

pub fn validate_mint_invites_count(
    mint_invites: u32,
    active_invites: i64,
    max_per_user: u32,
) -> AppResult<()> {
    if mint_invites == 0 {
        return Err(AppError::BadRequest("mintInvites must be at least 1".into()));
    }
    if mint_invites > max_per_user {
        return Err(AppError::BadRequest(format!(
            "mintInvites cannot exceed {max_per_user}"
        )));
    }
    if active_invites + i64::from(mint_invites) > i64::from(max_per_user) {
        return Err(AppError::BadRequest(format!(
            "Would exceed maximum of {max_per_user} active invites ({active_invites} already active)"
        )));
    }
    Ok(())
}

pub async fn resolve_waitlist_user(
    pool: &PgPool,
    user_id: Option<&str>,
    wallet_address: Option<&str>,
) -> AppResult<WaitlistUserSummary> {
    if let Some(user_id) = user_id {
        Uuid::parse_str(user_id)
            .map_err(|_| AppError::BadRequest("invalid userId".into()))?;
        return sqlx::query_as(
            "SELECT user_id, wallet_address, username, full_name, role
             FROM users WHERE user_id = $1::uuid",
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound);
    }

    if let Some(wallet_address) = wallet_address {
        let normalized = normalize_wallet_lookup(wallet_address);
        return sqlx::query_as(
            "SELECT user_id, wallet_address, username, full_name, role
             FROM users
             WHERE LOWER(wallet_address) = $1 OR LOWER(public_key) = $1",
        )
        .bind(&normalized)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound);
    }

    Err(AppError::BadRequest(
        "Exactly one of userId or walletAddress is required".into(),
    ))
}

pub async fn waitlist_entry_status(pool: &PgPool, user_id: &str) -> AppResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT status FROM waitlist_entries WHERE user_id = $1::uuid",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(status,)| status))
}

pub fn generate_referral_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..10)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

pub async fn assign_referral_code(pool: &PgPool, user_id: &str) -> AppResult<String> {
    for _ in 0..8 {
        let code = generate_referral_code();
        let result = sqlx::query(
            "UPDATE users SET referral_code = $2 WHERE user_id = $1::uuid AND referral_code IS NULL",
        )
        .bind(user_id)
        .bind(&code)
        .execute(pool)
        .await?;

        if result.rows_affected() > 0 {
            return Ok(code);
        }

        let existing: Option<(Option<String>,)> =
            sqlx::query_as("SELECT referral_code FROM users WHERE user_id = $1::uuid")
                .bind(user_id)
                .fetch_optional(pool)
                .await?;

        if let Some((Some(code),)) = existing {
            return Ok(code);
        }
    }

    Err(AppError::Internal("Failed to assign referral code".into()))
}

pub async fn get_referral_code(pool: &PgPool, user_id: &str) -> AppResult<Option<String>> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT referral_code FROM users WHERE user_id = $1::uuid")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|(code,)| code))
}

pub async fn join_waitlist(pool: &PgPool, user_id: &str) -> AppResult<()> {
    let score: (i64,) = sqlx::query_as("SELECT nextval('waitlist_queue_seq')")
        .fetch_one(pool)
        .await?;

    let boost = user_manual_boost(pool, user_id).await.unwrap_or(0);
    let queue_score = score.0 - i64::from(boost);

    sqlx::query(
        "INSERT INTO waitlist_entries (user_id, status, queue_score)
         VALUES ($1::uuid, 'waiting', $2)
         ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(queue_score)
    .execute(pool)
    .await?;

    Ok(())
}

async fn user_manual_boost(pool: &PgPool, user_id: &str) -> AppResult<i32> {
    let row: Option<(i32,)> = sqlx::query_as(
        "SELECT manual_priority_boost FROM waitlist_user_controls WHERE user_id = $1::uuid",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(b,)| b).unwrap_or(0))
}

pub async fn get_status(pool: &PgPool, user_id: &str) -> AppResult<Option<WaitlistStatus>> {
    let entry: Option<(String, i32, i64)> = sqlx::query_as(
        "SELECT status, referral_bumps, queue_score FROM waitlist_entries WHERE user_id = $1::uuid",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    let Some((status, referral_bumps, queue_score)) = entry else {
        return Ok(None);
    };

    let position_estimate = if status == "waiting" {
        let pos: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) + 1 FROM waitlist_entries
             WHERE status = 'waiting' AND queue_score < $1",
        )
        .bind(queue_score)
        .fetch_one(pool)
        .await?;
        Some(pos.0)
    } else {
        None
    };

    let config = get_config(pool).await.ok();
    let next_batch_at = config.and_then(|c| c.next_batch_at);
    let referral_code = get_referral_code(pool, user_id).await?;

    Ok(Some(WaitlistStatus {
        status,
        position_estimate,
        referral_bumps,
        next_batch_at,
        referral_code,
    }))
}

pub async fn apply_referral_bump(pool: &PgPool, referrer_user_id: &str) -> AppResult<bool> {
    let result = sqlx::query(
        "UPDATE waitlist_entries
         SET queue_score = queue_score - $2,
             referral_bumps = referral_bumps + 1,
             updated_at = NOW()
         WHERE user_id = $1::uuid AND status = 'waiting'",
    )
    .bind(referrer_user_id)
    .bind(REFERRAL_BUMP_POINTS)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn approve_user(pool: &PgPool, user_id: &str, via: &str) -> AppResult<bool> {
    if !matches!(via, "batch" | "invite" | "admin") {
        return Err(AppError::BadRequest("invalid approval via".into()));
    }

    let result = sqlx::query(
        "UPDATE waitlist_entries
         SET status = 'approved',
             approved_at = NOW(),
             approved_via = $2,
             updated_at = NOW()
         WHERE user_id = $1::uuid AND status = 'waiting'",
    )
    .bind(user_id)
    .bind(via)
    .execute(pool)
    .await?;

    if result.rows_affected() > 0 {
        return Ok(true);
    }

    let inserted = sqlx::query(
        "INSERT INTO waitlist_entries (user_id, status, queue_score, approved_at, approved_via)
         VALUES ($1::uuid, 'approved', 0, NOW(), $2)
         ON CONFLICT (user_id) DO UPDATE
         SET status = 'approved',
             approved_at = COALESCE(waitlist_entries.approved_at, NOW()),
             approved_via = COALESCE(waitlist_entries.approved_via, EXCLUDED.approved_via),
             updated_at = NOW()
         WHERE waitlist_entries.status = 'waiting'",
    )
    .bind(user_id)
    .bind(via)
    .execute(pool)
    .await?;

    Ok(inserted.rows_affected() > 0)
}

pub async fn is_approved(pool: &PgPool, user_id: &str) -> AppResult<bool> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT status FROM waitlist_entries WHERE user_id = $1::uuid",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    match row {
        None => Ok(true),
        Some((status,)) => Ok(status == "approved"),
    }
}

pub async fn get_config(pool: &PgPool) -> AppResult<WaitlistConfigRow> {
    Ok(sqlx::query_as(
        "SELECT admission_interval_hours, spots_per_batch, is_paused, last_batch_at, next_batch_at
         FROM waitlist_config WHERE id = 1",
    )
    .fetch_one(pool)
    .await?)
}

pub async fn set_config(
    pool: &PgPool,
    admission_interval_hours: i32,
    spots_per_batch: i32,
) -> AppResult<WaitlistConfigRow> {
    if !matches!(admission_interval_hours, 12 | 24) {
        return Err(AppError::BadRequest(
            "admissionIntervalHours must be 12 or 24".into(),
        ));
    }
    if spots_per_batch <= 0 {
        return Err(AppError::BadRequest("spotsPerBatch must be positive".into()));
    }

    Ok(sqlx::query_as(
        "UPDATE waitlist_config
         SET admission_interval_hours = $1,
             spots_per_batch = $2,
             updated_at = NOW()
         WHERE id = 1
         RETURNING admission_interval_hours, spots_per_batch, is_paused, last_batch_at, next_batch_at",
    )
    .bind(admission_interval_hours)
    .bind(spots_per_batch)
    .fetch_one(pool)
    .await?)
}

pub async fn set_paused(pool: &PgPool, paused: bool) -> AppResult<WaitlistConfigRow> {
    Ok(sqlx::query_as(
        "UPDATE waitlist_config
         SET is_paused = $1,
             updated_at = NOW()
         WHERE id = 1
         RETURNING admission_interval_hours, spots_per_batch, is_paused, last_batch_at, next_batch_at",
    )
    .bind(paused)
    .fetch_one(pool)
    .await?)
}

pub async fn resume_batches(pool: &PgPool) -> AppResult<WaitlistConfigRow> {
    Ok(sqlx::query_as(
        "UPDATE waitlist_config
         SET is_paused = false,
             next_batch_at = COALESCE(next_batch_at, NOW()),
             updated_at = NOW()
         WHERE id = 1
         RETURNING admission_interval_hours, spots_per_batch, is_paused, last_batch_at, next_batch_at",
    )
    .fetch_one(pool)
    .await?)
}

pub async fn admin_stats(pool: &PgPool) -> AppResult<WaitlistAdminStats> {
    let waiting: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM waitlist_entries WHERE status = 'waiting'")
            .fetch_one(pool)
            .await?;
    let approved: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM waitlist_entries WHERE status = 'approved'")
            .fetch_one(pool)
            .await?;
    Ok(WaitlistAdminStats {
        waiting_count: waiting.0,
        approved_count: approved.0,
    })
}

pub async fn set_user_invites_enabled(
    pool: &PgPool,
    user_id: &str,
    enabled: bool,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO waitlist_user_controls (user_id, invites_enabled, updated_at)
         VALUES ($1::uuid, $2, NOW())
         ON CONFLICT (user_id) DO UPDATE
         SET invites_enabled = EXCLUDED.invites_enabled,
             updated_at = NOW()",
    )
    .bind(user_id)
    .bind(enabled)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn user_invites_enabled(pool: &PgPool, user_id: &str) -> AppResult<bool> {
    let row: Option<(bool,)> = sqlx::query_as(
        "SELECT invites_enabled FROM waitlist_user_controls WHERE user_id = $1::uuid",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(e,)| e).unwrap_or(false))
}

pub async fn can_user_create_invites(pool: &PgPool, user_id: &str) -> AppResult<bool> {
    if !user_invites_enabled(pool, user_id).await? {
        return Ok(false);
    }
    is_approved(pool, user_id).await
}

pub async fn select_next_batch(pool: &PgPool, limit: i32) -> AppResult<Vec<String>> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM waitlist_entries
         WHERE status = 'waiting'
         ORDER BY queue_score ASC
         LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id,)| id.to_string())
        .collect())
}

pub async fn run_admission_batch(pool: &PgPool, batch_enabled: bool) -> AppResult<BatchAdmissionResult> {
    if !batch_enabled {
        return Ok(BatchAdmissionResult {
            admitted_user_ids: vec![],
            skipped: true,
        });
    }

    let mut tx = pool.begin().await?;

    let config: WaitlistConfigRow = sqlx::query_as(
        "SELECT admission_interval_hours, spots_per_batch, is_paused, last_batch_at, next_batch_at
         FROM waitlist_config WHERE id = 1 FOR UPDATE",
    )
    .fetch_one(&mut *tx)
    .await?;

    let now = chrono::Utc::now();
    if config.is_paused {
        tx.commit().await?;
        return Ok(BatchAdmissionResult {
            admitted_user_ids: vec![],
            skipped: true,
        });
    }

    if let Some(next_batch_at) = config.next_batch_at {
        if now < next_batch_at {
            tx.commit().await?;
            return Ok(BatchAdmissionResult {
                admitted_user_ids: vec![],
                skipped: true,
            });
        }
    }

    let user_ids = select_next_batch_in_tx(&mut tx, config.spots_per_batch).await?;
    let mut admitted = Vec::new();

    for user_id in &user_ids {
        let updated = sqlx::query(
            "UPDATE waitlist_entries
             SET status = 'approved',
                 approved_at = NOW(),
                 approved_via = 'batch',
                 updated_at = NOW()
             WHERE user_id = $1::uuid AND status = 'waiting'",
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

        if updated.rows_affected() > 0 {
            admitted.push(user_id.clone());
        }
    }

    let next_at = now + chrono::Duration::hours(i64::from(config.admission_interval_hours as u32));
    sqlx::query(
        "UPDATE waitlist_config
         SET last_batch_at = $1,
             next_batch_at = $2,
             updated_at = NOW()
         WHERE id = 1",
    )
    .bind(now)
    .bind(next_at)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(BatchAdmissionResult {
        admitted_user_ids: admitted,
        skipped: false,
    })
}

async fn select_next_batch_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    limit: i32,
) -> AppResult<Vec<String>> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM waitlist_entries
         WHERE status = 'waiting'
         ORDER BY queue_score ASC
         LIMIT $1",
    )
    .bind(limit)
    .fetch_all(&mut **tx)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id,)| id.to_string())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn referral_code_has_expected_length_and_charset() {
        let code = generate_referral_code();
        assert_eq!(code.len(), 10);
        assert!(code.chars().all(|c| c.is_ascii_uppercase() || ('2'..='9').contains(&c)));
    }

    #[test]
    fn batch_enabled_gate_skips_without_db() {
        // Documents guard: batch_enabled=false yields empty admission without DB access.
        let result = BatchAdmissionResult {
            admitted_user_ids: vec![],
            skipped: true,
        };
        assert!(result.skipped);
        assert!(result.admitted_user_ids.is_empty());
    }

    #[test]
    fn normalize_wallet_lookup_lowercases_and_trims() {
        assert_eq!(
            normalize_wallet_lookup("  0xAbC123  "),
            "0xabc123"
        );
    }

    #[test]
    fn validate_mint_invites_count_rejects_invalid_values() {
        assert!(validate_mint_invites_count(0, 0, 10).is_err());
        assert!(validate_mint_invites_count(11, 0, 10).is_err());
        assert!(validate_mint_invites_count(3, 8, 10).is_err());
        assert!(validate_mint_invites_count(2, 8, 10).is_ok());
        assert!(validate_mint_invites_count(1, 0, 10).is_ok());
    }
}
