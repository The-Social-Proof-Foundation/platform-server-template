use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::post_events::RawGrpcEvent;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowPayload {
    pub follower: String,
    pub following: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnfollowPayload {
    pub follower: String,
    pub unfollowed: String,
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

pub fn parse(raw: &RawGrpcEvent) -> Option<super::post_events::ParsedChainEvent> {
    use super::post_events::ParsedChainEvent;
    let name = raw.event_type.as_deref().and_then(event_name_from_type)?;
    let data = json_to_record(&raw.json);

    match name {
        "FollowEvent" | "UserFollowedEvent" => Some(ParsedChainEvent::Follow(FollowPayload {
            follower: as_string(data.get("follower").unwrap_or(&Value::Null)).to_lowercase(),
            following: as_string(data.get("following").unwrap_or(&Value::Null)).to_lowercase(),
        })),
        "UnfollowEvent" | "UserUnfollowedEvent" => Some(ParsedChainEvent::Unfollow(UnfollowPayload {
            follower: as_string(data.get("follower").unwrap_or(&Value::Null)).to_lowercase(),
            unfollowed: as_string(data.get("unfollowed").unwrap_or(&Value::Null)).to_lowercase(),
        })),
        _ => None,
    }
}
