use anyhow::{anyhow, Result};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawQiweEvent {
    pub event_id: String,
    pub received_at: DateTime<Utc>,
    pub source: String,
    pub payload: Value,
}

impl RawQiweEvent {
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let value: Value = serde_json::from_slice(bytes)?;
        Self::from_value(value)
    }

    pub fn from_value(value: Value) -> Result<Self> {
        let event_id = string_field(&value, "event_id")
            .or_else(|| nested_string_field(&value, &["payload", "msgUniqueIdentifier"]))
            .ok_or_else(|| anyhow!("raw event missing event_id"))?;
        let received_at = datetime_field(&value, "received_at").unwrap_or_else(Utc::now);
        let source = string_field(&value, "source").unwrap_or_else(|| "qiwe".to_string());
        let payload = value
            .get("payload")
            .cloned()
            .ok_or_else(|| anyhow!("raw event missing payload"))?;

        Ok(Self {
            event_id,
            received_at,
            source,
            payload,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedMessageEvent {
    pub event_id: String,
    pub message_id: String,
    pub platform: String,
    pub chat_id: String,
    pub chat_type: String,
    pub sender_id: String,
    pub sender_name: Option<String>,
    pub text: Option<String>,
    pub message_kind: String,
    pub is_mention_bot: bool,
    pub should_trigger: bool,
    pub trigger_reason: Option<String>,
    pub sent_at: Option<DateTime<Utc>>,
    pub received_at: DateTime<Utc>,
    pub raw: Value,
    pub mentions: Vec<Value>,
    pub sender_identity: Option<SenderIdentityEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderIdentityEvent {
    pub platform: String,
    pub chat_id: String,
    pub channel_user_id: String,
    pub display_name: String,
    pub identity_source: String,
    pub resolved_at: Option<DateTime<Utc>>,
    pub metadata: Value,
}

impl NormalizedMessageEvent {
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let value: Value = serde_json::from_slice(bytes)?;
        Self::from_value(value)
    }

    pub fn from_value(value: Value) -> Result<Self> {
        let event_id = string_field(&value, "event_id")
            .or_else(|| string_field(&value, "message_id"))
            .ok_or_else(|| anyhow!("message event missing event_id"))?;
        let message_id = string_field(&value, "message_id").unwrap_or_else(|| event_id.clone());
        let platform = string_field(&value, "platform").unwrap_or_else(|| "qiwe".to_string());
        let sender_id = string_field(&value, "sender_id").unwrap_or_default();
        let group_id = string_field(&value, "group_id").unwrap_or_default();
        let chat_type = string_field(&value, "chat_type")
            .or_else(|| string_field(&value, "conversation_type"))
            .unwrap_or_else(|| {
                if group_id.is_empty() {
                    "direct".to_string()
                } else {
                    "group".to_string()
                }
            });
        let chat_id = string_field(&value, "chat_id").unwrap_or_else(|| {
            if chat_type == "group" {
                group_id.clone()
            } else {
                sender_id.clone()
            }
        });
        if message_id.trim().is_empty() {
            return Err(anyhow!("message event missing message_id"));
        }
        if chat_id.trim().is_empty() {
            return Err(anyhow!("message event missing chat_id"));
        }

        let raw = value
            .get("raw")
            .cloned()
            .or_else(|| value.get("raw_event_ref").cloned())
            .or_else(|| value.get("payload_ref").cloned())
            .unwrap_or_else(|| Value::Object(Default::default()));
        let mentions = array_field(&value, "mentions")
            .or_else(|| array_field(&value, "at_list"))
            .or_else(|| nested_array_field(&value, &["raw", "msgData", "atList"]))
            .or_else(|| nested_array_field(&value, &["raw_event_ref", "msgData", "atList"]))
            .unwrap_or_default();

        Ok(Self {
            event_id,
            message_id,
            platform,
            chat_id,
            chat_type,
            sender_id,
            sender_name: string_field(&value, "sender_name"),
            text: string_field(&value, "text"),
            message_kind: string_field(&value, "message_kind")
                .unwrap_or_else(|| "unsupported".to_string()),
            is_mention_bot: bool_field(&value, "is_mention_bot")
                .or_else(|| bool_field(&value, "is_mentioned"))
                .unwrap_or(false),
            should_trigger: bool_field(&value, "should_trigger").unwrap_or(false),
            trigger_reason: string_field(&value, "trigger_reason")
                .or_else(|| string_field(&value, "reason")),
            sent_at: datetime_field(&value, "sent_at")
                .or_else(|| datetime_field(&value, "timestamp")),
            received_at: datetime_field(&value, "received_at").unwrap_or_else(Utc::now),
            raw,
            mentions,
            sender_identity: SenderIdentityEvent::from_value(value.get("sender_identity")),
        })
    }
}

impl SenderIdentityEvent {
    fn from_value(value: Option<&Value>) -> Option<Self> {
        let value = value?;
        let display_name = string_field(value, "display_name")?;
        if display_name.trim().is_empty() {
            return None;
        }
        let channel_user_id = string_field(value, "channel_user_id")
            .or_else(|| string_field(value, "sender_id"))
            .or_else(|| string_field(value, "user_id"))?;
        if channel_user_id.trim().is_empty() {
            return None;
        }
        let metadata = value.as_object().map_or_else(
            || Value::Object(Default::default()),
            |object| {
                let mut object = object.clone();
                object.remove("platform");
                object.remove("chat_id");
                object.remove("channel_user_id");
                object.remove("sender_id");
                object.remove("user_id");
                object.remove("display_name");
                object.remove("identity_source");
                object.remove("resolved_at");
                Value::Object(object)
            },
        );
        Some(Self {
            platform: string_field(value, "platform").unwrap_or_else(|| "qiwe".to_string()),
            chat_id: string_field(value, "chat_id").unwrap_or_default(),
            channel_user_id,
            display_name,
            identity_source: string_field(value, "identity_source")
                .or_else(|| string_field(value, "source"))
                .unwrap_or_else(|| "observed".to_string()),
            resolved_at: datetime_field(value, "resolved_at"),
            metadata,
        })
    }
}

pub fn mention_key(value: &Value, index: usize) -> String {
    for key in ["user_id", "userId", "id", "wxid", "nickname", "name"] {
        if let Some(text) = string_field(value, key) {
            if !text.trim().is_empty() {
                return format!("{key}:{text}");
            }
        }
    }
    format!("index:{index}")
}

pub fn mention_user_id(value: &Value) -> Option<String> {
    string_field(value, "user_id")
        .or_else(|| string_field(value, "userId"))
        .or_else(|| string_field(value, "id"))
        .or_else(|| string_field(value, "wxid"))
}

pub fn mention_display_name(value: &Value) -> Option<String> {
    string_field(value, "display_name")
        .or_else(|| string_field(value, "nickname"))
        .or_else(|| string_field(value, "name"))
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(value_to_string)
}

fn nested_string_field(value: &Value, path: &[&str]) -> Option<String> {
    value_at_path(value, path).and_then(value_to_string)
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn bool_field(value: &Value, key: &str) -> Option<bool> {
    match value.get(key) {
        Some(Value::Bool(flag)) => Some(*flag),
        Some(Value::String(text)) => match text.trim().to_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        },
        Some(Value::Number(number)) => number.as_i64().map(|value| value != 0),
        _ => None,
    }
}

fn array_field(value: &Value, key: &str) -> Option<Vec<Value>> {
    value.get(key).and_then(|item| item.as_array()).cloned()
}

fn nested_array_field(value: &Value, path: &[&str]) -> Option<Vec<Value>> {
    value_at_path(value, path)
        .and_then(|item| item.as_array())
        .cloned()
}

fn value_at_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn datetime_field(value: &Value, key: &str) -> Option<DateTime<Utc>> {
    value.get(key).and_then(value_to_datetime)
}

fn value_to_datetime(value: &Value) -> Option<DateTime<Utc>> {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return None;
            }
            DateTime::parse_from_rfc3339(trimmed)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
                .or_else(|| trimmed.parse::<i64>().ok().and_then(timestamp_to_datetime))
        }
        Value::Number(number) => number.as_i64().and_then(timestamp_to_datetime),
        _ => None,
    }
}

fn timestamp_to_datetime(value: i64) -> Option<DateTime<Utc>> {
    let seconds = if value.abs() > 10_000_000_000 {
        value / 1000
    } else {
        value
    };
    Utc.timestamp_opt(seconds, 0).single()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_plan_message_payload() {
        let event = NormalizedMessageEvent::from_value(json!({
            "event_id": "evt-1",
            "message_id": "msg-1",
            "platform": "qiwe",
            "chat_id": "group-1",
            "chat_type": "group",
            "sender_id": "user-1",
            "sender_name": "Evans",
            "text": "hi",
            "message_kind": "text",
            "is_mention_bot": true,
            "should_trigger": true,
            "trigger_reason": "mentioned",
            "sent_at": "2026-06-18T10:00:00Z",
            "received_at": "2026-06-18T10:00:01Z",
            "raw": {"msgData": {"atList": [{"userId": "bot", "nickname": "二花"}]}}
        }))
        .unwrap();

        assert_eq!(event.message_id, "msg-1");
        assert_eq!(event.chat_id, "group-1");
        assert!(event.is_mention_bot);
        assert_eq!(event.mentions.len(), 1);
    }

    #[test]
    fn parses_legacy_qiwe_events_payload() {
        let event = NormalizedMessageEvent::from_value(json!({
            "event_id": "msg-2",
            "group_id": "group-2",
            "sender_id": "user-2",
            "conversation_type": "group",
            "is_mentioned": false,
            "message_kind": "solitaire",
            "text": "接龙",
            "timestamp": "2026-06-18T10:00:00Z",
            "raw_event_ref": {"msgData": {"atList": []}}
        }))
        .unwrap();

        assert_eq!(event.message_id, "msg-2");
        assert_eq!(event.chat_id, "group-2");
        assert_eq!(event.chat_type, "group");
        assert_eq!(event.message_kind, "solitaire");
    }

    #[test]
    fn parses_sender_identity_payload() {
        let event = NormalizedMessageEvent::from_value(json!({
            "event_id": "evt-1",
            "message_id": "msg-1",
            "platform": "qiwe",
            "chat_id": "room-1",
            "chat_type": "group",
            "sender_id": "user-1",
            "text": "hi",
            "message_kind": "text",
            "received_at": "2026-06-18T10:00:01Z",
            "sender_identity": {
                "platform": "qiwe",
                "chat_id": "room-1",
                "channel_user_id": "user-1",
                "display_name": "弦默",
                "identity_source": "room_member",
                "resolved_at": "2026-06-18T10:00:00Z",
                "error": "ignored"
            }
        }))
        .unwrap();

        let identity = event.sender_identity.unwrap();
        assert_eq!(identity.platform, "qiwe");
        assert_eq!(identity.chat_id, "room-1");
        assert_eq!(identity.channel_user_id, "user-1");
        assert_eq!(identity.display_name, "弦默");
        assert_eq!(identity.identity_source, "room_member");
        assert!(identity.metadata.get("error").is_some());
    }

    #[test]
    fn ignores_unresolved_sender_identity_payload() {
        let event = NormalizedMessageEvent::from_value(json!({
            "event_id": "evt-1",
            "message_id": "msg-1",
            "platform": "qiwe",
            "chat_id": "room-1",
            "chat_type": "group",
            "sender_id": "user-1",
            "text": "hi",
            "message_kind": "text",
            "received_at": "2026-06-18T10:00:01Z",
            "sender_identity": {
                "platform": "qiwe",
                "chat_id": "room-1",
                "channel_user_id": "user-1",
                "display_name": "",
                "identity_source": "",
                "error": "display_name_unresolved"
            }
        }))
        .unwrap();

        assert!(event.sender_identity.is_none());
    }
}
