use platform_core::{AppResult, Config, SharedPlatformMetrics};
use platform_db::{get_delivery_config, notification_allowed, NotificationChannel};
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use uuid::Uuid;

use crate::apns::ApnsClient;
use crate::fcm::FcmClient;
use crate::resend::ResendClient;
use crate::ws_hub::{WsHub, WsOutbound};

#[derive(Clone)]
pub struct NotificationService {
    pub ws_hub: WsHub,
    apns: ApnsClient,
    fcm: FcmClient,
    resend: ResendClient,
    global_config: Config,
    metrics: Option<SharedPlatformMetrics>,
}

impl NotificationService {
    pub fn new(config: &Config, metrics: Option<SharedPlatformMetrics>) -> AppResult<Self> {
        Ok(Self {
            ws_hub: WsHub::new(metrics.clone()),
            apns: ApnsClient::from_config(config)?,
            fcm: FcmClient::new(None),
            resend: ResendClient::from_config(config),
            global_config: config.clone(),
            metrics,
        })
    }

    async fn resolve_user_id(pool: &PgPool, recipient: &str) -> Option<String> {
        let row: Option<(Uuid,)> = sqlx::query_as(
            "SELECT user_id FROM users WHERE user_id::text = $1 OR wallet_address = $1 LIMIT 1",
        )
        .bind(recipient)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        row.map(|(id,)| id.to_string())
    }

    pub async fn deliver_notification(
        &self,
        pool: &PgPool,
        redis: &mut ConnectionManager,
        user_id: &str,
        title: &str,
        message: &str,
        notification_type: &str,
        object_id: Option<&str>,
    ) -> AppResult<()> {
        let resolved_user_id = Self::resolve_user_id(pool, user_id)
            .await
            .unwrap_or_else(|| user_id.to_string());

        let sent_ws = self
            .ws_hub
            .send_to_user(
                &resolved_user_id,
                WsOutbound {
                    msg_type: "notification".into(),
                    data: serde_json::json!({
                        "title": title,
                        "message": message,
                        "type": notification_type,
                        "objectId": object_id,
                    }),
                },
            )
            .await;

        if sent_ws {
            if let Some(metrics) = &self.metrics {
                metrics.inc_notification_delivered("ws");
            }
            return Ok(());
        }

        if platform_db::is_user_online(redis, &resolved_user_id).await? {
            return Ok(());
        }

        let platform_id = self
            .global_config
            .platform_id
            .as_deref()
            .unwrap_or_default();
        let delivery = get_delivery_config(pool, redis, platform_id).await?;

        let apns = ApnsClient::from_delivery(&self.global_config, delivery.as_ref())?;
        let fcm_key = delivery
            .as_ref()
            .and_then(|d| d.fcm_server_key.clone())
            .or_else(|| std::env::var("FCM_SERVER_KEY").ok());
        let fcm = FcmClient::new(fcm_key);

        let push_allowed = notification_allowed(
            pool,
            &resolved_user_id,
            notification_type,
            NotificationChannel::Push,
        )
        .await
        .unwrap_or(true);

        if push_allowed {
            let tokens: Vec<(String, String)> = sqlx::query_as(
                "SELECT device_token, device_type FROM device_tokens WHERE user_id::text = $1 OR user_id = $1::uuid",
            )
            .bind(&resolved_user_id)
            .fetch_all(pool)
            .await
            .unwrap_or_default();

            for (token, device_type) in tokens {
                let deep_link =
                    object_id.map(|id| format!("projectyz://content/{notification_type}/{id}"));
                let is_android = device_type.eq_ignore_ascii_case("android");
                let result = if is_android {
                    fcm.send_push(
                        &token,
                        title,
                        message,
                        deep_link.map(|link| serde_json::json!({ "deepLink": link })),
                    )
                    .await
                } else {
                    apns
                        .send_push(&token, title, message, deep_link)
                        .await
                };

                if result.is_ok() {
                    if let Some(metrics) = &self.metrics {
                        metrics.inc_notification_delivered(if is_android {
                            "fcm"
                        } else {
                            "apns"
                        });
                    }
                }
            }
        }

        let email_allowed = notification_allowed(
            pool,
            &resolved_user_id,
            notification_type,
            NotificationChannel::Email,
        )
        .await
        .unwrap_or(true);

        if email_allowed {
            let email: Option<(Option<String>,)> = sqlx::query_as(
                "SELECT email FROM users WHERE user_id::text = $1 OR wallet_address = $1 LIMIT 1",
            )
            .bind(&resolved_user_id)
            .fetch_optional(pool)
            .await?;

            if let Some((Some(email),)) = email {
                let verified: Option<(Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,)> = sqlx::query_as(
                    "SELECT email_verified_at FROM users WHERE email = $1 LIMIT 1",
                )
                .bind(&email)
                .fetch_optional(pool)
                .await?;

                let verified_ok = verified
                    .and_then(|(at,)| at)
                    .is_some();

                if verified_ok {
                    let resend_key = delivery
                        .as_ref()
                        .and_then(|d| d.resend_api_key.as_deref())
                        .or(self.global_config.resend_api_key.as_deref());
                    let from_email = delivery
                        .as_ref()
                        .and_then(|d| d.resend_from_email.as_deref())
                        .or(self.global_config.resend_from_email.as_deref());

                    if self
                        .resend
                        .send_email(
                            &email,
                            title,
                            &format!("<p>{message}</p>"),
                            resend_key,
                            from_email,
                        )
                        .await
                        .is_ok()
                    {
                        if let Some(metrics) = &self.metrics {
                            metrics.inc_notification_delivered("resend");
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn insert_notification(
        &self,
        pool: &PgPool,
        redis: &mut ConnectionManager,
        recipient: &str,
        sender: Option<&str>,
        notification_type: &str,
        object_id: &str,
        object_type: &str,
        title: &str,
        message: &str,
    ) -> AppResult<()> {
        let notification_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO notifications (notification_id, user_id, sender_wallet_address, type, object_id, object_type, title, message, created_at)
             SELECT $1, user_id, $3, $4, $5, $6, $7, $8, EXTRACT(EPOCH FROM NOW())::BIGINT
             FROM users WHERE wallet_address = $2 OR user_id::text = $2",
        )
        .bind(notification_id)
        .bind(recipient)
        .bind(sender)
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
            recipient,
            "notificationCount",
        );
        counter.increment_by(1).await?;

        self.deliver_notification(
            pool,
            redis,
            recipient,
            title,
            message,
            notification_type,
            Some(object_id),
        )
        .await
    }

    pub async fn notify_mentions(
        &self,
        pool: &PgPool,
        redis: &mut ConnectionManager,
        mentions: &[String],
        post_id: &str,
        sender_wallet: &str,
    ) -> AppResult<()> {
        for mention in mentions {
            if mention.eq_ignore_ascii_case(sender_wallet) {
                continue;
            }
            self.insert_notification(
                pool,
                redis,
                mention,
                Some(sender_wallet),
                "mention",
                &post_id.to_string(),
                "post",
                "You were mentioned",
                "Someone mentioned you in a post",
            )
            .await?;
        }
        Ok(())
    }

    pub async fn notify_comment(
        &self,
        pool: &PgPool,
        redis: &mut ConnectionManager,
        author_wallet: &str,
        commenter_wallet: &str,
        post_id: &str,
        comment_id: &str,
    ) -> AppResult<()> {
        if author_wallet.eq_ignore_ascii_case(commenter_wallet) {
            return Ok(());
        }
        self.insert_notification(
            pool,
            redis,
            author_wallet,
            Some(commenter_wallet),
            "comment",
            comment_id,
            "comment",
            "New comment",
            "Someone commented on your post",
        )
        .await?;
        let _ = post_id;
        Ok(())
    }

    pub async fn notify_like(
        &self,
        pool: &PgPool,
        redis: &mut ConnectionManager,
        author_wallet: &str,
        liker_wallet: &str,
        post_id: &str,
    ) -> AppResult<()> {
        if author_wallet.eq_ignore_ascii_case(liker_wallet) {
            return Ok(());
        }
        self.insert_notification(
            pool,
            redis,
            author_wallet,
            Some(liker_wallet),
            "like",
            &post_id.to_string(),
            "post",
            "New like",
            "Someone liked your post",
        )
        .await
    }

    pub async fn send_verification_email(&self, to: &str, verify_url: &str) -> AppResult<()> {
        self.resend
            .send_email(
                to,
                "Verify your email",
                &format!("<p>Verify your email: <a href=\"{verify_url}\">{verify_url}</a></p>"),
                None,
                None,
            )
            .await
    }

    pub async fn notify_referral_reward(
        &self,
        pool: &PgPool,
        redis: &mut ConnectionManager,
        referrer_user_id: &str,
    ) -> AppResult<()> {
        self.insert_notification(
            pool,
            redis,
            referrer_user_id,
            None,
            "referral_reward",
            referrer_user_id,
            "referral",
            "Referral reward unlocked",
            "You reached the referral milestone — claim your reward in the app",
        )
        .await
    }

    pub async fn fanout_stream_event(
        &self,
        user_id: &str,
        event: serde_json::Value,
    ) -> AppResult<()> {
        self.ws_hub
            .send_to_user(
                user_id,
                WsOutbound {
                    msg_type: "stream_event".into(),
                    data: event,
                },
            )
            .await;
        Ok(())
    }
}
