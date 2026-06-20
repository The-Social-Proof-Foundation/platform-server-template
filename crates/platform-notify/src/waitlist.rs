use platform_core::AppResult;
use sqlx::PgPool;
use redis::aio::ConnectionManager;

use crate::service::NotificationService;

impl NotificationService {
    async fn notify_user(
        &self,
        pool: &PgPool,
        redis: &mut ConnectionManager,
        user_id: &str,
        notification_type: &str,
        title: &str,
        message: &str,
        object_id: &str,
        object_type: &str,
    ) -> AppResult<()> {
        let notification_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO notifications (notification_id, user_id, sender_wallet_address, type, object_id, object_type, title, message, created_at)
             VALUES ($1, $2::uuid, NULL, $3, $4, $5, $6, $7, EXTRACT(EPOCH FROM NOW())::BIGINT)",
        )
        .bind(notification_id)
        .bind(user_id)
        .bind(notification_type)
        .bind(object_id)
        .bind(object_type)
        .bind(title)
        .bind(message)
        .execute(pool)
        .await?;

        let counter = platform_db::ShardedCounter::new(
            redis.clone(),
            pool.clone(),
            "user",
            user_id,
            "notificationCount",
        );
        counter.increment_by(1).await?;

        self.deliver_notification(pool, redis, user_id, title, message, notification_type, Some(object_id))
            .await
    }

    pub async fn notify_waitlist_joined(
        &self,
        pool: &PgPool,
        redis: &mut ConnectionManager,
        user_id: &str,
    ) -> AppResult<()> {
        self.notify_user(
            pool,
            redis,
            user_id,
            "waitlist_joined",
            "You're on the waitlist",
            "We'll notify you when you get early access",
            user_id,
            "waitlist",
        )
        .await
    }

    pub async fn notify_waitlist_bump(
        &self,
        pool: &PgPool,
        redis: &mut ConnectionManager,
        referrer_user_id: &str,
    ) -> AppResult<()> {
        self.notify_user(
            pool,
            redis,
            referrer_user_id,
            "waitlist_bump",
            "You moved up the waitlist",
            "Someone signed up with your referral code",
            referrer_user_id,
            "waitlist",
        )
        .await
    }

    pub async fn notify_referral_claimed(
        &self,
        pool: &PgPool,
        redis: &mut ConnectionManager,
        referrer_user_id: &str,
        referred_user_id: &str,
    ) -> AppResult<()> {
        self.notify_user(
            pool,
            redis,
            referrer_user_id,
            "referral_claimed",
            "New referral",
            "Someone joined using your referral code",
            referred_user_id,
            "referral",
        )
        .await
    }

    pub async fn notify_invite_accepted(
        &self,
        pool: &PgPool,
        redis: &mut ConnectionManager,
        inviter_user_id: &str,
        invitee_user_id: &str,
    ) -> AppResult<()> {
        self.notify_user(
            pool,
            redis,
            inviter_user_id,
            "invite_accepted",
            "Invite accepted",
            "Someone used your invite code",
            invitee_user_id,
            "invite",
        )
        .await
    }

    pub async fn notify_waitlist_approved(
        &self,
        pool: &PgPool,
        redis: &mut ConnectionManager,
        user_id: &str,
        invites_minted: u32,
    ) -> AppResult<()> {
        let message = if invites_minted > 0 {
            format!(
                "You now have full platform access. {invites_minted} early-access invite codes are ready in the app."
            )
        } else {
            "You now have full platform access".into()
        };
        self.notify_user(
            pool,
            redis,
            user_id,
            "waitlist_approved",
            "Early access granted",
            &message,
            user_id,
            "waitlist",
        )
        .await
    }
}
