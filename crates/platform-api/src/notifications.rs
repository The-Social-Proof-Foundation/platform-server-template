use std::collections::BTreeMap;

use platform_core::AppError;
use redis::AsyncCommands;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::middleware::AuthUser;
use crate::state::SharedApiState;

const VALID_FILTERS: &[&str] = &[
    "all", "like", "follow", "mention", "comment", "repost", "tip", "spt",
];

#[derive(Debug, Deserialize)]
pub struct NotificationListQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default = "default_filter")]
    pub filter: String,
}

fn default_page() -> i64 {
    1
}

fn default_limit() -> i64 {
    20
}

fn default_filter() -> String {
    "all".into()
}

#[derive(Debug, Deserialize, Default)]
pub struct MarkReadRequest {
    #[serde(default, rename = "notificationIds", alias = "notification_ids")]
    pub notification_ids: Vec<String>,
}

pub async fn list_notifications(
    state: &SharedApiState,
    auth: &AuthUser,
    query: NotificationListQuery,
) -> ApiResult<Value> {
    let page = query.page.max(1);
    let limit = query.limit.clamp(1, 100);
    let filter = if query.filter.is_empty() {
        "all".to_string()
    } else {
        query.filter
    };
    if !VALID_FILTERS.contains(&filter.as_str()) {
        return Err(AppError::BadRequest("Invalid filter value".into()).into());
    }

    let offset = (page - 1) * limit;
    let rows = sqlx::query(
        "SELECT notification_id, sender_wallet_address, type, object_id, object_type, title, message,
                image_1, image_2, metadata_json, read_at IS NOT NULL AS is_read, created_at
         FROM notifications
         WHERE user_id = $1::uuid
           AND (
             $2 = 'all'
             OR ($2 = 'spt' AND type LIKE 'spt_%')
             OR type = $2
           )
         ORDER BY created_at DESC
         LIMIT $3 OFFSET $4",
    )
    .bind(&auth.user_id)
    .bind(&filter)
    .bind(limit)
    .bind(offset)
    .fetch_all(state.pg_read())
    .await?;

    let total_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications
         WHERE user_id = $1::uuid
           AND (
             $2 = 'all'
             OR ($2 = 'spt' AND type LIKE 'spt_%')
             OR type = $2
           )",
    )
    .bind(&auth.user_id)
    .bind(&filter)
    .fetch_one(state.pg_read())
    .await?;

    let unread_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE user_id = $1::uuid AND read_at IS NULL",
    )
    .bind(&auth.user_id)
    .fetch_one(state.pg_read())
    .await?;

    let senders: Vec<String> = rows
        .iter()
        .filter_map(|row| row.get::<Option<String>, _>("sender_wallet_address"))
        .collect();
    let profiles = hydrate_senders(state, &senders).await;

    let mut grouped: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for row in rows {
        let created_at: i64 = row.get("created_at");
        let day = created_at - created_at.rem_euclid(86_400);
        let sender = row
            .get::<Option<String>, _>("sender_wallet_address")
            .unwrap_or_default();
        let profile = profiles.get(&sender);
        let image_1 = row
            .get::<Option<String>, _>("image_1")
            .or_else(|| profile.and_then(|p| p.avatar_url.clone()));
        let item = json!({
            "notificationId": row.get::<Uuid, _>("notification_id"),
            "type": row.get::<String, _>("type"),
            "senderId": row.get::<Option<String>, _>("sender_wallet_address"),
            "objectId": row.get::<Option<String>, _>("object_id"),
            "objectType": row.get::<Option<String>, _>("object_type"),
            "title": row.get::<Option<String>, _>("title"),
            "message": row.get::<Option<String>, _>("message"),
            "image1": image_1,
            "image2": row.get::<Option<String>, _>("image_2"),
            "metadata": row.get::<Option<Value>, _>("metadata_json"),
            "isRead": row.get::<bool, _>("is_read"),
            "createdAt": created_at,
            "updatedAt": created_at,
            "senderUsername": profile.and_then(|p| p.username.clone()),
            "senderDisplayName": profile.and_then(|p| p.display_name.clone()),
            "senderAvatarUrl": profile.and_then(|p| p.avatar_url.clone()),
        });
        grouped.entry(day.to_string()).or_default().push(item);
    }

    let mut notifications = Map::new();
    for (day, items) in grouped.into_iter().rev() {
        notifications.insert(day, Value::Array(items));
    }

    let total_pages = if total_count == 0 {
        0
    } else {
        (total_count + limit - 1) / limit
    };

    Ok(json!({
        "notifications": notifications,
        "unreadCount": unread_count,
        "pagination": {
            "currentPage": page,
            "totalPages": total_pages,
            "totalCount": total_count,
            "limit": limit,
        }
    }))
}

pub async fn mark_notifications_read(
    state: &SharedApiState,
    auth: &AuthUser,
    body: MarkReadRequest,
) -> ApiResult<Value> {
    let mut ids = Vec::with_capacity(body.notification_ids.len());
    for raw in body.notification_ids {
        let id = Uuid::parse_str(raw.trim())
            .map_err(|_| AppError::BadRequest("Invalid notification id".into()))?;
        ids.push(id);
    }

    sqlx::query(
        "UPDATE notifications
         SET read_at = NOW()
         WHERE user_id = $1::uuid
           AND read_at IS NULL
           AND (cardinality($2::uuid[]) = 0 OR notification_id = ANY($2::uuid[]))",
    )
    .bind(&auth.user_id)
    .bind(&ids)
    .execute(state.pg())
    .await?;

    let unread_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE user_id = $1::uuid AND read_at IS NULL",
    )
    .bind(&auth.user_id)
    .fetch_one(state.pg())
    .await?;

    sqlx::query(
        "UPDATE users SET notification_count = $2 WHERE user_id = $1::uuid",
    )
    .bind(&auth.user_id)
    .bind(i32::try_from(unread_count).unwrap_or(i32::MAX))
    .execute(state.pg())
    .await?;

    // Reconciled from unread rows, so drop any pending shard delta for this user.
    let mut redis = state.redis();
    let keys = [
        format!("counter:user:{}:notificationCount", auth.user_id),
        format!("counter:user:{}:notificationCount", auth.wallet_address),
    ];
    for key in keys {
        if key.ends_with(":notificationCount") && !key.contains("::") {
            let _: Result<(), _> = redis.del(key).await;
        }
    }

    Ok(json!({ "ok": true, "unreadCount": unread_count }))
}

#[derive(Clone)]
struct SenderProfile {
    username: Option<String>,
    display_name: Option<String>,
    avatar_url: Option<String>,
}

async fn hydrate_senders(
    state: &SharedApiState,
    senders: &[String],
) -> std::collections::HashMap<String, SenderProfile> {
    let mut unique = Vec::new();
    for sender in senders {
        if !sender.is_empty() && !unique.contains(sender) {
            unique.push(sender.clone());
        }
    }

    let mut profiles = std::collections::HashMap::new();
    let mut redis = state.redis();
    let mut missing = Vec::new();
    for sender in &unique {
        let key = format!("profile-hydrate:{sender}");
        let cached: Option<String> = redis.get(&key).await.ok().flatten();
        if let Some(cached) = cached {
            if let Ok(profile) = serde_json::from_str::<SenderProfileJson>(&cached) {
                profiles.insert(sender.clone(), profile.into());
                continue;
            }
        }
        missing.push(sender.clone());
    }

    let Some(base) = state
        .config()
        .social_indexer_url
        .clone()
        .filter(|url| !url.is_empty())
    else {
        return profiles;
    };

    let client = reqwest::Client::new();
    for sender in missing {
        let url = format!(
            "{}/profiles/address/{}",
            base.trim_end_matches('/'),
            sender
        );
        let Ok(response) = client.get(url).send().await else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }
        let Ok(body) = response.json::<Value>().await else {
            continue;
        };
        let profile = profile_from_json(&body);
        let cache_key = format!("profile-hydrate:{sender}");
        let encoded = serde_json::to_string(&SenderProfileJson::from(profile.clone())).ok();
        if let Some(encoded) = encoded {
            let _: Result<(), _> = redis.set_ex(cache_key, encoded, 300).await;
        }
        profiles.insert(sender, profile);
    }
    profiles
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct SenderProfileJson {
    username: Option<String>,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "avatarUrl")]
    avatar_url: Option<String>,
}

impl From<SenderProfile> for SenderProfileJson {
    fn from(value: SenderProfile) -> Self {
        Self {
            username: value.username,
            display_name: value.display_name,
            avatar_url: value.avatar_url,
        }
    }
}

impl From<SenderProfileJson> for SenderProfile {
    fn from(value: SenderProfileJson) -> Self {
        Self {
            username: value.username,
            display_name: value.display_name,
            avatar_url: value.avatar_url,
        }
    }
}

fn profile_from_json(value: &Value) -> SenderProfile {
    let obj = value.get("profile").unwrap_or(value);
    SenderProfile {
        username: string_field(obj, &["username"]),
        display_name: string_field(obj, &["display_name", "displayName", "full_name", "fullname"]),
        avatar_url: string_field(obj, &["profile_photo", "profilePhoto", "avatar_url", "avatarUrl"]),
    }
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|v| v.as_str()).and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    })
}
