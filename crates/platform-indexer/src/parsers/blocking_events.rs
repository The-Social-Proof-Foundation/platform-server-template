use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::post_events::RawGrpcEvent;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockPayload {
    pub blocker: String,
    pub blocked: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnblockPayload {
    pub blocker: String,
    pub unblocked: String,
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
        "UserBlockEvent" | "ProfileBlockEvent" | "UserBlockedEvent" => {
            Some(ParsedChainEvent::Block(BlockPayload {
                blocker: as_string(data.get("blocker").unwrap_or(&Value::Null)).to_lowercase(),
                blocked: as_string(data.get("blocked").unwrap_or(&Value::Null)).to_lowercase(),
            }))
        }
        "UserUnblockEvent" | "ProfileUnblockEvent" => {
            Some(ParsedChainEvent::Unblock(UnblockPayload {
                blocker: as_string(data.get("blocker").unwrap_or(&Value::Null)).to_lowercase(),
                unblocked: as_string(data.get("unblocked").unwrap_or(&Value::Null)).to_lowercase(),
            }))
        }
        _ => None,
    }
}
