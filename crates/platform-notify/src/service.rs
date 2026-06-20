use platform_core::{AppResult, Config};
use platform_db::{is_user_online, ShardedCounter};
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use uuid::Uuid;

use crate::apns::ApnsClient;
use crate::resend::ResendClient;
use crate::ws_hub::{WsHub, WsOutbound};

#[derive(Clone)]
pub struct NotificationService {
    pub ws_hub: WsHub,
    apns: ApnsClient,
    resend: ResendClient,
    store_duration_secs: u64,
}

impl NotificationService {
    pub fn new(config: &Config) -> AppResult<Self> {
        Ok(Self {
            ws_hub: WsHub::new(),
            apns: ApnsClient::from_config(config)?,
            resend: ResendClient::from_config(config),
            store_duration_secs: config.redis_store_duration_secs,
        })
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
        let sent_ws = self.ws_hub.send_to_user(
            user_id,
            WsOutbound {
                msg_type: "notification".into(),
                data: serde_json::json!({
                    "title": title,
                    "message": message,
                    "type": notification_type,
                    "objectId": object_id,
                }),
            },
        ).await;

        if sent_ws {
            return Ok(());
        }

        if is_user_online(redis, user_id).await? {
            return Ok(());
        }

        let tokens: Vec<(String,)> = sqlx::query_as(
            "SELECT device_token FROM device_tokens WHERE user_id::text = $1 OR user_id = $1::uuid",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        for (token,) in tokens {
            let deep_link = object_id.map(|id| format!("projectyz://content/{notification_type}/{id}"));
            let _ = self
                .apns
                .send_push(&token, title, message, deep_link)
                .await;
        }

        let email: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT email FROM users WHERE user_id::text = $1 OR wallet_address = $1 LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

        if let Some((Some(email),)) = email {
            let _ = self
                .resend
                .send_email(&email, title, &format!("<p>{message}</p>"), None, None)
                .await;
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

        let counter = ShardedCounter::new(
            redis.clone(),
            pool.clone(),
            "user",
            recipient,
            "notificationCount",
        );
        counter.increment_by(1).await?;

        self.deliver_notification(pool, redis, recipient, title, message, notification_type, Some(object_id))
            .await
    }

    pub async fn notify_mentions(
        &self,
        pool: &PgPool,
        redis: &mut ConnectionManager,
        mentions: &[String],
        post_id: Uuid,
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
        post_id: Uuid,
        comment_id: Uuid,
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
            &comment_id.to_string(),
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
        post_id: Uuid,
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
