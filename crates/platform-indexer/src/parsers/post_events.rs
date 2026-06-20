use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const POST_EVENT_NAMES: &[(&str, &str)] = &[
    ("PostCreated", "PostCreatedEvent"),
    ("CommentCreated", "CommentCreatedEvent"),
    ("Reaction", "ReactionEvent"),
    ("RemoveReaction", "RemoveReactionEvent"),
    ("Tip", "TipEvent"),
    ("PostDeleted", "PostDeletedEvent"),
    ("CommentDeleted", "CommentDeletedEvent"),
    ("PostUpdated", "PostUpdatedEvent"),
    ("CommentUpdated", "CommentUpdatedEvent"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum ParsedChainEvent {
    PostCreated(PostCreatedPayload),
    CommentCreated(CommentCreatedPayload),
    Reaction(ReactionPayload),
    RemoveReaction(ReactionPayload),
    Tip(TipPayload),
    PostDeleted(DeletePostPayload),
    CommentDeleted(DeleteCommentPayload),
    PostUpdated(PostUpdatedPayload),
    CommentUpdated(CommentUpdatedPayload),
    Follow(super::social_graph_events::FollowPayload),
    Unfollow(super::social_graph_events::UnfollowPayload),
    Block(super::blocking_events::BlockPayload),
    Unblock(super::blocking_events::UnblockPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostCreatedPayload {
    pub post_id: String,
    pub owner: String,
    pub platform_id: String,
    pub content: String,
    #[serde(default)]
    pub post_type: Option<String>,
    #[serde(default)]
    pub mentions: Option<Vec<String>>,
    #[serde(default)]
    pub media_urls: Option<Vec<String>>,
    #[serde(default)]
    pub metadata_json: Option<String>,
    #[serde(default)]
    pub actor_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentCreatedPayload {
    pub comment_id: String,
    pub post_id: String,
    pub owner: String,
    pub content: String,
    #[serde(default)]
    pub mentions: Option<Vec<String>>,
    #[serde(default)]
    pub actor_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactionPayload {
    pub object_id: String,
    pub user: String,
    pub reaction: String,
    pub is_post: bool,
    #[serde(default)]
    pub principal_owner: Option<String>,
    #[serde(default)]
    pub actor_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TipPayload {
    pub object_id: String,
    pub from: String,
    pub to: String,
    pub amount: serde_json::Value,
    #[serde(default)]
    pub coin_type: Option<String>,
    pub is_post: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletePostPayload {
    pub post_id: String,
    #[serde(default)]
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteCommentPayload {
    pub comment_id: String,
    #[serde(default)]
    pub post_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostUpdatedPayload {
    pub post_id: String,
    pub content: String,
    #[serde(default)]
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentUpdatedPayload {
    pub comment_id: String,
    pub post_id: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct RawGrpcEvent {
    pub package_id: Option<String>,
    pub module: Option<String>,
    pub sender: Option<String>,
    pub event_type: Option<String>,
    pub json: Value,
}

fn event_name_from_type(event_type: &str) -> Option<&str> {
    event_type.rsplit("::").next().map(str::trim)
}

fn json_to_record(json: &Value) -> serde_json::Map<String, Value> {
    if let Some(fields) = json.get("fields").and_then(|v| v.as_object()) {
        return fields.clone();
    }
    json.as_object().cloned().unwrap_or_default()
}

fn as_string(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn as_bool(v: &Value) -> bool {
    matches!(v, Value::Bool(true))
        || v.as_str().is_some_and(|s| s.eq_ignore_ascii_case("true"))
        || v.as_i64() == Some(1)
}

fn as_string_array(v: &Value) -> Option<Vec<String>> {
    v.as_array().map(|arr| {
        arr.iter()
            .map(|x| as_string(x).to_lowercase())
            .filter(|s| !s.is_empty())
            .collect()
    })
}

pub fn parse_post_event(raw: &RawGrpcEvent) -> Option<ParsedChainEvent> {
    if raw.module.as_deref() != Some("post") {
        return None;
    }
    let name = raw.event_type.as_deref().and_then(event_name_from_type)?;
    let data = json_to_record(&raw.json);

    match name {
        "PostCreatedEvent" => Some(ParsedChainEvent::PostCreated(PostCreatedPayload {
            post_id: as_string(data.get("post_id").unwrap_or(&Value::Null)).to_lowercase(),
            owner: as_string(data.get("owner").unwrap_or(&Value::Null)).to_lowercase(),
            platform_id: as_string(data.get("platform_id").unwrap_or(&Value::Null)).to_lowercase(),
            content: as_string(data.get("content").unwrap_or(&Value::Null)),
            post_type: data.get("post_type").map(as_string).filter(|s| !s.is_empty()),
            mentions: data.get("mentions").and_then(as_string_array),
            media_urls: data
                .get("media_urls")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().map(as_string).collect()),
            metadata_json: data
                .get("metadata_json")
                .map(as_string)
                .filter(|s| !s.is_empty()),
            actor_address: data
                .get("actor_address")
                .map(as_string)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_lowercase()),
        })),
        "CommentCreatedEvent" => Some(ParsedChainEvent::CommentCreated(CommentCreatedPayload {
            comment_id: as_string(data.get("comment_id").unwrap_or(&Value::Null)).to_lowercase(),
            post_id: as_string(data.get("post_id").unwrap_or(&Value::Null)).to_lowercase(),
            owner: as_string(data.get("owner").unwrap_or(&Value::Null)).to_lowercase(),
            content: as_string(data.get("content").unwrap_or(&Value::Null)),
            mentions: data.get("mentions").and_then(as_string_array),
            actor_address: data
                .get("actor_address")
                .map(as_string)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_lowercase()),
        })),
        "ReactionEvent" => Some(ParsedChainEvent::Reaction(ReactionPayload {
            object_id: as_string(data.get("object_id").unwrap_or(&Value::Null)).to_lowercase(),
            user: as_string(data.get("user").unwrap_or(&Value::Null)).to_lowercase(),
            reaction: as_string(data.get("reaction").unwrap_or(&Value::Null)),
            is_post: as_bool(data.get("is_post").unwrap_or(&Value::Null)),
            principal_owner: data
                .get("principal_owner")
                .map(as_string)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_lowercase()),
            actor_address: data
                .get("actor_address")
                .map(as_string)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_lowercase()),
        })),
        "RemoveReactionEvent" => Some(ParsedChainEvent::RemoveReaction(ReactionPayload {
            object_id: as_string(data.get("object_id").unwrap_or(&Value::Null)).to_lowercase(),
            user: as_string(data.get("user").unwrap_or(&Value::Null)).to_lowercase(),
            reaction: as_string(data.get("reaction").unwrap_or(&Value::Null)),
            is_post: as_bool(data.get("is_post").unwrap_or(&Value::Null)),
            principal_owner: None,
            actor_address: None,
        })),
        "TipEvent" => Some(ParsedChainEvent::Tip(TipPayload {
            object_id: as_string(data.get("object_id").unwrap_or(&Value::Null)).to_lowercase(),
            from: as_string(data.get("from").unwrap_or(&Value::Null)).to_lowercase(),
            to: as_string(data.get("to").unwrap_or(&Value::Null)).to_lowercase(),
            amount: data
                .get("amount")
                .cloned()
                .unwrap_or(Value::Number(0.into())),
            coin_type: data.get("coin_type").map(as_string).filter(|s| !s.is_empty()),
            is_post: as_bool(data.get("is_post").unwrap_or(&Value::Bool(true))),
        })),
        "PostDeletedEvent" => Some(ParsedChainEvent::PostDeleted(DeletePostPayload {
            post_id: as_string(data.get("post_id").unwrap_or(&Value::Null)).to_lowercase(),
            owner: data
                .get("owner")
                .map(as_string)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_lowercase()),
        })),
        "CommentDeletedEvent" => Some(ParsedChainEvent::CommentDeleted(DeleteCommentPayload {
            comment_id: as_string(data.get("comment_id").unwrap_or(&Value::Null)).to_lowercase(),
            post_id: data
                .get("post_id")
                .map(as_string)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_lowercase()),
        })),
        "PostUpdatedEvent" => Some(ParsedChainEvent::PostUpdated(PostUpdatedPayload {
            post_id: as_string(data.get("post_id").unwrap_or(&Value::Null)).to_lowercase(),
            content: as_string(data.get("content").unwrap_or(&Value::Null)),
            metadata_json: data
                .get("metadata_json")
                .map(as_string)
                .filter(|s| !s.is_empty()),
        })),
        "CommentUpdatedEvent" => Some(ParsedChainEvent::CommentUpdated(CommentUpdatedPayload {
            comment_id: as_string(data.get("comment_id").unwrap_or(&Value::Null)).to_lowercase(),
            post_id: as_string(data.get("post_id").unwrap_or(&Value::Null)).to_lowercase(),
            content: as_string(data.get("content").unwrap_or(&Value::Null)),
        })),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_post_created() {
        let raw = RawGrpcEvent {
            package_id: Some("0x50c1".into()),
            module: Some("post".into()),
            sender: None,
            event_type: Some("0x1::post::PostCreatedEvent".into()),
            json: json!({
                "fields": {
                    "post_id": "0xABC",
                    "owner": "0xOwner",
                    "platform_id": "0xPlatform",
                    "content": "hello"
                }
            }),
        };
        let parsed = parse_post_event(&raw).expect("parsed");
        match parsed {
            ParsedChainEvent::PostCreated(p) => {
                assert_eq!(p.post_id, "0xabc");
                assert_eq!(p.content, "hello");
            }
            _ => panic!("wrong kind"),
        }
    }
}
