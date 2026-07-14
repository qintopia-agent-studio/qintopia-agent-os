use anyhow::{anyhow, Result};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

const QIWE_ASYNC_CALLBACK_COMMAND: i64 = 20_000;

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
        }
        .sanitized_for_storage())
    }

    pub(crate) fn sanitized_for_storage(&self) -> Self {
        let (payload, callback_sanitized) = sanitize_qiwe_raw_payload(self.payload.clone());
        let event_id = if callback_sanitized {
            callback_event_id(&self.event_id)
        } else {
            self.event_id.clone()
        };
        Self {
            event_id,
            received_at: self.received_at,
            source: if callback_sanitized {
                "qiwe".to_string()
            } else {
                self.source.clone()
            },
            payload,
        }
    }
}

pub fn dead_letter_payload_summary(payload: &[u8]) -> String {
    json!({
        "payload_bytes": payload.len(),
        "payload_sha256": format!("sha256:{}", sha256_hex(payload)),
        "raw_payload_stored": false,
    })
    .to_string()
}

fn sanitize_qiwe_raw_payload(value: Value) -> (Value, bool) {
    if is_sanitized_callback_payload(&value) {
        return (canonicalize_sanitized_callback_payload(&value), true);
    }
    let mut callback_events = Vec::new();
    collect_sanitized_callback_events(&value, &mut callback_events);
    if callback_events.is_empty() {
        return (value, false);
    }

    (
        json!({
            "callback_event_count": callback_events.len(),
            "callback_events": callback_events,
            "credentials_redacted": true,
            "source": "qiwe_async_callback",
        }),
        true,
    )
}

fn canonicalize_sanitized_callback_payload(value: &Value) -> Value {
    let callback_events = value
        .get("callback_events")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(canonicalize_sanitized_callback_event)
        .collect::<Vec<_>>();

    json!({
        "callback_event_count": callback_events.len(),
        "callback_events": callback_events,
        "credentials_redacted": true,
        "source": "qiwe_async_callback",
    })
}

fn canonicalize_sanitized_callback_event(value: &Value) -> Option<Value> {
    let object = value.as_object()?;
    if !is_async_callback_event(object) {
        return None;
    }
    let request_id_sha256 = value_for_normalized_key(object, "requestidsha256")
        .and_then(Value::as_str)
        .and_then(canonical_sha256_marker);
    let msg_data_summary = value_for_normalized_key(object, "msgdatasummary")
        .map(canonicalize_callback_msg_data_summary)
        .unwrap_or_else(|| callback_msg_data_summary(None));

    Some(json!({
        "cmd": QIWE_ASYNC_CALLBACK_COMMAND,
        "credentials_redacted": true,
        "msg_data_summary": msg_data_summary,
        "request_id_sha256": request_id_sha256,
    }))
}

fn canonicalize_callback_msg_data_summary(value: &Value) -> Value {
    let Some(object) = value.as_object() else {
        return callback_msg_data_summary(None);
    };
    let field_presence =
        value_for_normalized_key(object, "fieldpresence").and_then(Value::as_object);
    let present = |key: &str| {
        field_presence
            .and_then(|fields| value_for_normalized_key(fields, key))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    let file_aes_key = present("fileaeskey");
    let file_id = present("fileid");
    let file_md5 = present("filemd5");
    let file_size = present("filesize");
    let filename = present("filename");
    let cloud_url = present("cloudurl");

    json!({
        "required_fields_present": file_aes_key
            && file_id
            && file_md5
            && file_size
            && filename,
        "field_presence": {
            "cloud_url": cloud_url,
            "file_aes_key": file_aes_key,
            "file_id": file_id,
            "file_md5": file_md5,
            "file_size": file_size,
            "filename": filename,
        },
        "msg_data_object": value_for_normalized_key(object, "msgdataobject")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "msg_data_present": value_for_normalized_key(object, "msgdatapresent")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "unknown_field_count": value_for_normalized_key(object, "unknownfieldcount")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    })
}

fn canonical_sha256_marker(value: &str) -> Option<String> {
    let digest = value.strip_prefix("sha256:")?;
    (digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| format!("sha256:{}", digest.to_ascii_lowercase()))
}

fn is_sanitized_callback_payload(value: &Value) -> bool {
    value.get("source").and_then(Value::as_str) == Some("qiwe_async_callback")
        && value.get("credentials_redacted").and_then(Value::as_bool) == Some(true)
        && value
            .get("callback_events")
            .and_then(Value::as_array)
            .is_some()
}

fn collect_sanitized_callback_events(value: &Value, events: &mut Vec<Value>) {
    match value {
        Value::Object(object) if is_async_callback_event(object) => {
            events.push(sanitize_async_callback_event(object));
        }
        Value::Object(object) => {
            for value in object.values() {
                collect_sanitized_callback_events(value, events);
            }
        }
        Value::Array(items) => {
            for value in items {
                collect_sanitized_callback_events(value, events);
            }
        }
        _ => {}
    }
}

fn is_async_callback_event(object: &Map<String, Value>) -> bool {
    object
        .iter()
        .find(|(key, _)| normalize_json_key(key) == "cmd")
        .and_then(|(_, value)| match value {
            Value::Number(number) => number.as_i64(),
            Value::String(text) => text.trim().parse::<i64>().ok(),
            _ => None,
        })
        == Some(QIWE_ASYNC_CALLBACK_COMMAND)
}

fn sanitize_async_callback_event(object: &Map<String, Value>) -> Value {
    let request_id = value_for_normalized_key(object, "requestid")
        .and_then(value_to_string)
        .map(|value| format!("sha256:{}", sha256_hex(value.as_bytes())));
    let msg_data = value_for_normalized_key(object, "msgdata");

    json!({
        "cmd": QIWE_ASYNC_CALLBACK_COMMAND,
        "credentials_redacted": true,
        "msg_data_summary": callback_msg_data_summary(msg_data),
        "request_id_sha256": request_id,
    })
}

fn callback_msg_data_summary(value: Option<&Value>) -> Value {
    let Some(object) = value.and_then(Value::as_object) else {
        return json!({
            "required_fields_present": false,
            "field_presence": {},
            "msg_data_object": false,
            "msg_data_present": value.is_some(),
            "unknown_field_count": 0,
        });
    };

    let has = |key: &str| {
        object
            .keys()
            .any(|candidate| normalize_json_key(candidate) == key)
    };
    let file_aes_key = has("fileaeskey");
    let file_id = has("fileid");
    let file_md5 = has("filemd5");
    let file_size = has("filesize");
    let filename = has("filename");
    let cloud_url = has("cloudurl");
    let known_fields = [
        "fileaeskey",
        "fileid",
        "filemd5",
        "filesize",
        "filename",
        "cloudurl",
    ];
    let unknown_field_count = object
        .keys()
        .filter(|key| !known_fields.contains(&normalize_json_key(key).as_str()))
        .count();

    json!({
        "required_fields_present": file_aes_key
            && file_id
            && file_md5
            && file_size
            && filename,
        "field_presence": {
            "cloud_url": cloud_url,
            "file_aes_key": file_aes_key,
            "file_id": file_id,
            "file_md5": file_md5,
            "file_size": file_size,
            "filename": filename,
        },
        "msg_data_object": true,
        "msg_data_present": true,
        "unknown_field_count": unknown_field_count,
    })
}

fn value_for_normalized_key<'a>(
    object: &'a Map<String, Value>,
    expected: &str,
) -> Option<&'a Value> {
    object
        .iter()
        .find(|(key, _)| normalize_json_key(key) == expected)
        .map(|(_, value)| value)
}

fn normalize_json_key(key: &str) -> String {
    key.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn sha256_hex(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn callback_event_id(value: &str) -> String {
    if let Some(digest) = value.strip_prefix("qiwe-callback:") {
        if digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return format!("qiwe-callback:{}", digest.to_ascii_lowercase());
        }
    }
    format!("qiwe-callback:{}", sha256_hex(value.as_bytes()))
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
        let mut event_id = string_field(&value, "event_id")
            .or_else(|| string_field(&value, "message_id"))
            .ok_or_else(|| anyhow!("message event missing event_id"))?;
        let mut message_id = string_field(&value, "message_id").unwrap_or_else(|| event_id.clone());
        let mut platform = string_field(&value, "platform").unwrap_or_else(|| "qiwe".to_string());
        let mut sender_id = string_field(&value, "sender_id").unwrap_or_default();
        let group_id = string_field(&value, "group_id").unwrap_or_default();
        let mut chat_type = string_field(&value, "chat_type")
            .or_else(|| string_field(&value, "conversation_type"))
            .unwrap_or_else(|| {
                if group_id.is_empty() {
                    "direct".to_string()
                } else {
                    "group".to_string()
                }
            });
        let mut chat_id = string_field(&value, "chat_id").unwrap_or_else(|| {
            if chat_type == "group" {
                group_id.clone()
            } else {
                sender_id.clone()
            }
        });
        if message_id.trim().is_empty() {
            return Err(anyhow!("message event missing message_id"));
        }

        let raw = value
            .get("raw")
            .cloned()
            .or_else(|| value.get("raw_event_ref").cloned())
            .or_else(|| value.get("payload_ref").cloned())
            .unwrap_or_else(|| Value::Object(Default::default()));
        let (raw, callback_sanitized) = sanitize_qiwe_raw_payload(raw);
        if callback_sanitized {
            event_id = callback_event_id(&event_id);
            message_id = callback_event_id(&message_id);
            platform = "qiwe".to_string();
            chat_id = "qiwe_async_callback".to_string();
            chat_type = "system".to_string();
            sender_id = "qiwe_callback_system".to_string();
        }
        if chat_id.trim().is_empty() {
            return Err(anyhow!("message event missing chat_id"));
        }
        let mentions = if callback_sanitized {
            Vec::new()
        } else {
            array_field(&value, "mentions")
                .or_else(|| array_field(&value, "at_list"))
                .or_else(|| nested_array_field(&value, &["raw", "msgData", "atList"]))
                .or_else(|| nested_array_field(&value, &["raw_event_ref", "msgData", "atList"]))
                .unwrap_or_default()
        };

        Ok(Self {
            event_id,
            message_id,
            platform,
            chat_id,
            chat_type,
            sender_id,
            sender_name: (!callback_sanitized)
                .then(|| string_field(&value, "sender_name"))
                .flatten(),
            text: (!callback_sanitized)
                .then(|| string_field(&value, "text"))
                .flatten(),
            message_kind: if callback_sanitized {
                "system".to_string()
            } else {
                string_field(&value, "message_kind").unwrap_or_else(|| "unsupported".to_string())
            },
            is_mention_bot: !callback_sanitized
                && bool_field(&value, "is_mention_bot")
                    .or_else(|| bool_field(&value, "is_mentioned"))
                    .unwrap_or(false),
            should_trigger: !callback_sanitized
                && bool_field(&value, "should_trigger").unwrap_or(false),
            trigger_reason: if callback_sanitized {
                Some("qiwe_async_callback_sanitized".to_string())
            } else {
                string_field(&value, "trigger_reason").or_else(|| string_field(&value, "reason"))
            },
            sent_at: datetime_field(&value, "sent_at")
                .or_else(|| datetime_field(&value, "timestamp")),
            received_at: datetime_field(&value, "received_at").unwrap_or_else(Utc::now),
            raw,
            mentions,
            sender_identity: (!callback_sanitized)
                .then(|| SenderIdentityEvent::from_value(value.get("sender_identity")))
                .flatten(),
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

    #[cfg(feature = "postgres-integration-tests")]
    fn postgres_integration_database_url() -> String {
        assert_eq!(
            std::env::var("QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE").as_deref(),
            Ok("1"),
            "PostgreSQL integration test requires the explicit apply-smoke guard"
        );
        let database_url = std::env::var("QINTOPIA_SIDECAR_DATABASE_URL")
            .expect("PostgreSQL integration test requires QINTOPIA_SIDECAR_DATABASE_URL");
        let parsed = url::Url::parse(&database_url).expect("integration database URL must parse");
        assert!(
            matches!(parsed.scheme(), "postgres" | "postgresql"),
            "PostgreSQL integration test requires a postgres URL"
        );
        assert!(
            matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "::1")),
            "PostgreSQL integration test may only use a loopback database"
        );
        assert_eq!(
            parsed.path().trim_start_matches('/'),
            "qintopia_test",
            "PostgreSQL integration test may only use qintopia_test"
        );
        database_url
    }

    #[test]
    fn raw_async_callback_is_reduced_to_sanitized_structure() {
        let event = RawQiweEvent::from_value(json!({
            "event_id": "callback-event-sensitive",
            "source": "qiwe",
            "payload": {
                "code": 0,
                "outerUnknown": "outer-secret",
                "data": [{
                    "requestId": "upload-request-sensitive",
                    "cmd": 20000,
                    "msgData": {
                        "fileAesKey": "aes-secret",
                        "fileId": "file-secret",
                        "fileMd5": "98e7c2acf4391f8b4a2bbd39e364c5e3",
                        "fileSize": 48300,
                        "filename": "private-activity.jpg",
                        "cloudUrl": "https://media.example.test/private.jpg?token=secret",
                        "unexpectedCredential": "unknown-secret"
                    }
                }]
            }
        }))
        .expect("callback parses");

        let stored = serde_json::to_string(&event).expect("event serializes");
        let repeated = event.sanitized_for_storage();
        assert!(event.event_id.starts_with("qiwe-callback:"));
        assert_eq!(repeated.event_id, event.event_id);
        assert_eq!(repeated.payload, event.payload);
        assert_eq!(event.payload["callback_event_count"], json!(1));
        assert_eq!(event.payload["callback_events"][0]["cmd"], json!(20_000));
        assert_eq!(
            event.payload["callback_events"][0]["msg_data_summary"]["required_fields_present"],
            json!(true)
        );
        assert_eq!(
            event.payload["callback_events"][0]["msg_data_summary"]["unknown_field_count"],
            json!(1)
        );
        assert!(event.payload["callback_events"][0]["request_id_sha256"]
            .as_str()
            .is_some_and(|value| value.starts_with("sha256:")));
        for sensitive in [
            "callback-event-sensitive",
            "upload-request-sensitive",
            "aes-secret",
            "file-secret",
            "98e7c2acf4391f8b4a2bbd39e364c5e3",
            "private-activity.jpg",
            "token=secret",
            "unknown-secret",
            "outer-secret",
        ] {
            assert!(
                !stored.contains(sensitive),
                "stored callback leaked {sensitive}"
            );
        }
    }

    #[test]
    fn raw_non_callback_payload_is_preserved() {
        let payload = json!({
            "cmd": 10002,
            "msgData": {
                "fileId": "ordinary-message-file-id",
                "text": "ordinary message"
            }
        });
        let event = RawQiweEvent::from_value(json!({
            "event_id": "ordinary-event",
            "payload": payload
        }))
        .expect("ordinary raw event parses");

        assert_eq!(event.event_id, "ordinary-event");
        assert_eq!(event.payload, payload);
    }

    #[test]
    fn spoofed_redaction_marker_is_rebuilt_without_unknown_values() {
        let event = RawQiweEvent::from_value(json!({
            "event_id": "qiwe-callback:raw-callback-secret",
            "payload": {
                "source": "qiwe_async_callback",
                "credentials_redacted": true,
                "outer_secret": "must-not-survive",
                "callback_events": [{
                    "cmd": 20000,
                    "credentials_redacted": true,
                    "request_id_sha256": format!("sha256:{}", "a".repeat(64)),
                    "event_secret": "must-not-survive-either",
                    "msg_data_summary": {
                        "required_fields_present": true,
                        "field_presence": {
                            "file_aes_key": true,
                            "file_id": true,
                            "file_md5": true,
                            "file_size": true,
                            "filename": true,
                            "cloud_url": true,
                            "secret_field": "must-not-survive-summary"
                        },
                        "msg_data_object": true,
                        "msg_data_present": true,
                        "unknown_field_count": 3,
                        "summary_secret": "must-not-survive-summary-object"
                    }
                }]
            }
        }))
        .expect("spoofed redaction marker parses safely");
        let stored = serde_json::to_string(&event).expect("event serializes");
        let repeated = event.sanitized_for_storage();

        assert_eq!(repeated.payload, event.payload);
        assert_ne!(event.event_id, "qiwe-callback:raw-callback-secret");
        assert_eq!(event.event_id.len(), "qiwe-callback:".len() + 64);
        assert_eq!(event.payload["callback_event_count"], json!(1));
        assert_eq!(
            event.payload["callback_events"][0]["request_id_sha256"],
            json!(format!("sha256:{}", "a".repeat(64)))
        );
        for sensitive in [
            "must-not-survive",
            "must-not-survive-either",
            "must-not-survive-summary",
            "must-not-survive-summary-object",
            "outer_secret",
            "event_secret",
            "secret_field",
            "summary_secret",
            "raw-callback-secret",
        ] {
            assert!(!stored.contains(sensitive));
        }
    }

    #[test]
    fn raw_callback_detection_accepts_string_command_and_key_variants() {
        let event = RawQiweEvent::from_value(json!({
            "event_id": "variant-callback",
            "payload": {
                "events": [{
                    "CMD": "20000",
                    "request_id": "variant-request",
                    "msg_data": {
                        "fileAeskey": "variant-aes",
                        "file_id": "variant-file",
                        "file_md5": "variant-md5",
                        "file_size": 1,
                        "fileName": "variant.jpg",
                        "cloud_url": "https://media.example.test/variant.jpg"
                    }
                }]
            }
        }))
        .expect("callback key variants parse");
        let stored = serde_json::to_string(&event).expect("event serializes");
        let callback = &event.payload["callback_events"][0];

        assert_eq!(callback["cmd"], json!(20_000));
        assert_eq!(
            callback["msg_data_summary"]["required_fields_present"],
            json!(true)
        );
        assert_eq!(
            callback["msg_data_summary"]["field_presence"]["cloud_url"],
            json!(true)
        );
        for sensitive in [
            "variant-callback",
            "variant-request",
            "variant-aes",
            "variant-file",
            "variant-md5",
            "variant.jpg",
        ] {
            assert!(!stored.contains(sensitive));
        }
    }

    #[test]
    fn normalized_callback_removes_raw_credentials_and_message_content() {
        let event = NormalizedMessageEvent::from_value(json!({
            "event_id": "normalized-callback-event",
            "message_id": "normalized-callback-message",
            "platform": "sensitive-platform",
            "chat_id": "sensitive-chat-id",
            "chat_type": "direct",
            "sender_id": "sensitive-sender-id",
            "sender_name": "sensitive-sender-name",
            "text": "sensitive-callback-text",
            "message_kind": "image",
            "is_mention_bot": true,
            "should_trigger": true,
            "mentions": [{"userId": "sensitive-mention"}],
            "sender_identity": {
                "channel_user_id": "sensitive-identity",
                "display_name": "sensitive-display-name"
            },
            "raw": {
                "cmd": 20000,
                "requestId": "normalized-request-secret",
                "msgData": {
                    "fileAesKey": "normalized-aes-secret",
                    "fileId": "normalized-file-secret",
                    "fileMd5": "normalized-md5-secret",
                    "fileSize": 5,
                    "filename": "normalized-private.jpg"
                }
            }
        }))
        .expect("normalized callback parses");
        let stored = serde_json::to_string(&event).expect("event serializes");

        assert!(event.event_id.starts_with("qiwe-callback:"));
        assert!(event.message_id.starts_with("qiwe-callback:"));
        assert_eq!(event.message_kind, "system");
        assert_eq!(event.platform, "qiwe");
        assert_eq!(event.chat_id, "qiwe_async_callback");
        assert_eq!(event.chat_type, "system");
        assert_eq!(event.sender_id, "qiwe_callback_system");
        assert_eq!(
            event.trigger_reason.as_deref(),
            Some("qiwe_async_callback_sanitized")
        );
        assert!(event.text.is_none());
        assert!(event.sender_name.is_none());
        assert!(event.mentions.is_empty());
        assert!(event.sender_identity.is_none());
        assert!(!event.is_mention_bot);
        assert!(!event.should_trigger);
        for sensitive in [
            "normalized-callback-event",
            "normalized-callback-message",
            "sensitive-platform",
            "sensitive-chat-id",
            "sensitive-sender-id",
            "sensitive-sender-name",
            "sensitive-callback-text",
            "sensitive-mention",
            "sensitive-identity",
            "sensitive-display-name",
            "normalized-request-secret",
            "normalized-aes-secret",
            "normalized-file-secret",
            "normalized-md5-secret",
            "normalized-private.jpg",
        ] {
            assert!(!stored.contains(sensitive));
        }
    }

    #[test]
    fn normalized_callback_accepts_redacted_capture_route() {
        let callback_id = format!("qiwe-callback:{}", "A".repeat(64));
        let event = NormalizedMessageEvent::from_value(json!({
            "event_id": callback_id,
            "message_id": callback_id,
            "platform": "qiwe",
            "chat_id": "",
            "chat_type": "group",
            "sender_id": "",
            "sender_identity": {
                "platform": "qiwe",
                "chat_id": "",
                "channel_user_id": "",
                "display_name": "",
                "identity_source": "",
                "error": "display_name_unresolved"
            },
            "raw": {
                "source": "qiwe_async_callback",
                "credentials_redacted": true,
                "callback_event_count": 1,
                "callback_events": [{
                    "cmd": 20000,
                    "credentials_redacted": true,
                    "request_id_sha256": "sha256:already-redacted",
                    "msg_data_summary": {
                        "required_fields_present": true,
                        "field_presence": {
                            "file_aes_key": true,
                            "file_id": true,
                            "file_md5": true,
                            "file_size": true,
                            "filename": true,
                            "cloud_url": true
                        },
                        "msg_data_object": true,
                        "msg_data_present": true,
                        "unknown_field_count": 0
                    }
                }]
            }
        }))
        .expect("redacted producer callback parses");

        assert_eq!(event.event_id, format!("qiwe-callback:{}", "a".repeat(64)));
        assert_eq!(event.message_id, event.event_id);
        assert_eq!(event.chat_id, "qiwe_async_callback");
        assert_eq!(event.chat_type, "system");
        assert_eq!(event.sender_id, "qiwe_callback_system");
        assert!(event.sender_identity.is_none());
    }

    #[test]
    fn dead_letter_summary_never_contains_raw_payload() {
        let payload = br#"{"cmd":20000,"fileAesKey":"dead-letter-secret""#;
        let summary = dead_letter_payload_summary(payload);
        let parsed: Value = serde_json::from_str(&summary).expect("summary is JSON");

        assert_eq!(parsed["payload_bytes"], json!(payload.len()));
        assert_eq!(parsed["raw_payload_stored"], json!(false));
        assert!(parsed["payload_sha256"]
            .as_str()
            .is_some_and(|value| value.starts_with("sha256:")));
        assert!(!summary.contains("dead-letter-secret"));
    }

    #[tokio::test]
    #[cfg(feature = "postgres-integration-tests")]
    #[ignore = "requires guarded disposable qintopia_test PostgreSQL"]
    async fn postgres_callback_storage_redacts_credentials() {
        let database_url = postgres_integration_database_url();
        let pool = crate::db::connect(&database_url, 2)
            .await
            .expect("connect disposable PostgreSQL");
        crate::db::run_migrations(&pool)
            .await
            .expect("migrate disposable PostgreSQL");
        let suffix = uuid::Uuid::new_v4();
        let request_id = format!("integration-upload-{suffix}");
        let aes_key = format!("integration-aes-{suffix}");
        let file_id = format!("integration-file-{suffix}");
        let media_url = format!("https://media.example.test/{suffix}.jpg?token=secret");
        let event = RawQiweEvent {
            event_id: request_id.clone(),
            received_at: Utc::now(),
            source: "qiwe".to_string(),
            payload: json!({
                "code": 0,
                "data": [{
                    "requestId": request_id.clone(),
                    "cmd": 20000,
                    "msgData": {
                        "fileAesKey": aes_key.clone(),
                        "fileId": file_id.clone(),
                        "fileMd5": "98e7c2acf4391f8b4a2bbd39e364c5e3",
                        "fileSize": 48300,
                        "filename": "integration-private.jpg",
                        "cloudUrl": media_url.clone()
                    }
                }]
            }),
        };

        let row_id = crate::db::persist_raw_event(&pool, "qintopia.qiwe.raw", &event)
            .await
            .expect("persist sanitized callback");
        let stored: (String, Value) = sqlx::query_as(
            "SELECT event_id, payload FROM qintopia_messages.raw_events WHERE id = $1",
        )
        .bind(row_id)
        .fetch_one(&pool)
        .await
        .expect("read sanitized callback");
        let stored_json = serde_json::to_string(&stored).expect("serialize stored callback");

        assert!(stored.0.starts_with("qiwe-callback:"));
        assert_eq!(
            stored.1["callback_events"][0]["credentials_redacted"],
            json!(true)
        );
        assert_eq!(
            stored.1["callback_events"][0]["msg_data_summary"]["required_fields_present"],
            json!(true)
        );
        for sensitive in [
            request_id.as_str(),
            aes_key.as_str(),
            file_id.as_str(),
            media_url.as_str(),
            "98e7c2acf4391f8b4a2bbd39e364c5e3",
            "integration-private.jpg",
        ] {
            assert!(
                !stored_json.contains(sensitive),
                "stored callback leaked {sensitive}"
            );
        }

        let normalized = NormalizedMessageEvent::from_value(json!({
            "event_id": request_id.clone(),
            "message_id": request_id.clone(),
            "platform": "qiwe",
            "chat_id": "callback-integration-channel",
            "chat_type": "direct",
            "sender_id": "callback-system",
            "sender_name": "integration-sensitive-sender",
            "text": "integration-sensitive-text",
            "message_kind": "image",
            "should_trigger": true,
            "raw": event.payload.clone()
        }))
        .expect("build sanitized normalized callback");
        let message_row_id =
            crate::db::persist_message(&pool, "qintopia.qiwe.message", &normalized)
                .await
                .expect("persist sanitized normalized callback");
        let stored_message: (String, String, Value, Option<String>) = sqlx::query_as(
            "SELECT event_id, message_id, raw, text FROM qintopia_messages.messages WHERE id = $1",
        )
        .bind(message_row_id)
        .fetch_one(&pool)
        .await
        .expect("read sanitized normalized callback");
        let stored_message_json =
            serde_json::to_string(&stored_message).expect("serialize normalized callback");
        assert!(stored_message.0.starts_with("qiwe-callback:"));
        assert!(stored_message.1.starts_with("qiwe-callback:"));
        assert!(stored_message.3.is_none());
        for sensitive in [
            request_id.as_str(),
            aes_key.as_str(),
            file_id.as_str(),
            media_url.as_str(),
            "integration-sensitive-sender",
            "integration-sensitive-text",
        ] {
            assert!(
                !stored_message_json.contains(sensitive),
                "stored normalized callback leaked {sensitive}"
            );
        }

        sqlx::query("DELETE FROM qintopia_messages.messages WHERE id = $1")
            .bind(message_row_id)
            .execute(&pool)
            .await
            .expect("delete normalized callback integration fixture");

        sqlx::query("DELETE FROM qintopia_messages.raw_events WHERE id = $1")
            .bind(row_id)
            .execute(&pool)
            .await
            .expect("delete callback integration fixture");
    }

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
