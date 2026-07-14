use std::io::{self, Read};

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

const MAX_SHADOW_CAPTURE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct ShadowCaptureReport {
    pub success: bool,
    pub worker: &'static str,
    pub platform: &'static str,
    pub profile: &'static str,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub action_status: &'static str,
    pub safe_for_chat: bool,
    pub external_send_executed: bool,
    pub database_write_executed: bool,
    pub artifact_created: bool,
    pub payload_sha256: String,
    pub payload_byte_count: usize,
    pub event_kind: &'static str,
    pub message_type: &'static str,
    pub message_id_sha256: Option<String>,
    pub chat_id_sha256: Option<String>,
    pub sender_id_sha256: Option<String>,
    pub content_sha256: Option<String>,
    pub content_byte_count: Option<usize>,
    pub field_presence: ShadowFieldPresence,
    pub guardrails: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShadowFieldPresence {
    pub message_id: bool,
    pub chat_id: bool,
    pub sender_id: bool,
    pub text_content: bool,
    pub media_reference: bool,
    pub callback_file_credentials: bool,
    pub timestamp: bool,
}

pub fn run() -> Result<()> {
    let payload = read_stdin_payload()?;
    let report = shadow_capture_report(payload.as_bytes())?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub fn shadow_capture_report(payload: &[u8]) -> Result<ShadowCaptureReport> {
    if payload.is_empty() {
        bail!("Huabaosi WeCom shadow payload must not be empty");
    }
    if payload.len() > MAX_SHADOW_CAPTURE_BYTES {
        bail!("Huabaosi WeCom shadow payload exceeds the 64 KiB preview limit");
    }

    let event: Value =
        serde_json::from_slice(payload).context("parse Huabaosi WeCom shadow payload")?;
    let field_presence = ShadowFieldPresence {
        message_id: first_string(&event, MESSAGE_ID_FIELDS).is_some(),
        chat_id: first_string(&event, CHAT_ID_FIELDS).is_some(),
        sender_id: first_string(&event, SENDER_ID_FIELDS).is_some(),
        text_content: first_string(&event, TEXT_CONTENT_FIELDS).is_some(),
        media_reference: contains_key_name(
            &event,
            &[
                "media_id",
                "media_url",
                "image",
                "image_url",
                "file",
                "file_id",
                "file_url",
                "download_url",
                "attachment",
                "attachments",
            ],
        ),
        callback_file_credentials: contains_key_fragment(
            &event,
            &[
                "file_id",
                "fileid",
                "file_url",
                "download_url",
                "token",
                "credential",
            ],
        ),
        timestamp: first_string(&event, TIMESTAMP_FIELDS).is_some()
            || first_number(&event, TIMESTAMP_FIELDS).is_some(),
    };

    let content = first_string(&event, TEXT_CONTENT_FIELDS);
    Ok(ShadowCaptureReport {
        success: true,
        worker: "huabaosi-wecom-shadow-capture",
        platform: "wecom",
        profile: "huabaosi",
        dry_run: true,
        apply_requested: false,
        action_status: "shadow_capture_preview",
        safe_for_chat: false,
        external_send_executed: false,
        database_write_executed: false,
        artifact_created: false,
        payload_sha256: sha256_marker(payload),
        payload_byte_count: payload.len(),
        event_kind: classify_event_kind(&event),
        message_type: classify_message_type(&event, &field_presence),
        message_id_sha256: first_string(&event, MESSAGE_ID_FIELDS).map(sha256_str),
        chat_id_sha256: first_string(&event, CHAT_ID_FIELDS).map(sha256_str),
        sender_id_sha256: first_string(&event, SENDER_ID_FIELDS).map(sha256_str),
        content_sha256: content.map(sha256_str),
        content_byte_count: first_string(&event, TEXT_CONTENT_FIELDS).map(|value| value.len()),
        field_presence,
        guardrails: vec![
            "preview only; no database write is performed",
            "raw ids, text, media URLs, filenames, tokens, and callback credentials are never emitted",
            "this command must not send WeCom messages, call QiWe, call image providers, upload media, or write Feishu",
        ],
    })
}

const MESSAGE_ID_FIELDS: &[&str] = &["message_id", "msg_id", "msgid", "id"];
const CHAT_ID_FIELDS: &[&str] = &["chat_id", "room_id", "conversation_id", "group_id"];
const SENDER_ID_FIELDS: &[&str] = &["sender_id", "from_user", "from_user_name", "userid"];
const TEXT_CONTENT_FIELDS: &[&str] = &["content", "text", "message_text", "plain_text"];
const TIMESTAMP_FIELDS: &[&str] = &["timestamp", "time", "create_time", "created_at"];

fn read_stdin_payload() -> Result<String> {
    let stdin = io::stdin();
    read_bounded_payload(stdin.lock())
}

fn read_bounded_payload<R: Read>(reader: R) -> Result<String> {
    let mut payload = String::new();
    reader
        .take((MAX_SHADOW_CAPTURE_BYTES + 1) as u64)
        .read_to_string(&mut payload)
        .context("read Huabaosi WeCom shadow payload from stdin")?;
    if payload.len() > MAX_SHADOW_CAPTURE_BYTES {
        bail!("Huabaosi WeCom shadow payload exceeds the 64 KiB preview limit");
    }
    Ok(payload)
}

fn classify_event_kind(event: &Value) -> &'static str {
    let Some(kind) = first_string(event, &["event_type", "event", "type"]) else {
        return "unknown";
    };
    match normalize_token(kind).as_str() {
        "message" | "msg" | "text" | "image" | "file" => "message",
        "callback" | "cmd20000" | "cmd_20000" => "callback",
        "heartbeat" | "health" => "heartbeat",
        _ => "unknown",
    }
}

fn classify_message_type(event: &Value, presence: &ShadowFieldPresence) -> &'static str {
    if let Some(message_type) = first_string(event, &["msg_type", "message_type", "type"]) {
        return match normalize_token(message_type).as_str() {
            "text" => "text",
            "image" => "image",
            "file" | "attachment" => "file",
            "mixed" => "mixed",
            _ => "unknown",
        };
    }
    match (presence.text_content, presence.media_reference) {
        (true, true) => "mixed",
        (true, false) => "text",
        (false, true) => "media",
        (false, false) => "unknown",
    }
}

fn first_string<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(Value::String(value)) = map.get(*key) {
                    if !value.is_empty() {
                        return Some(value);
                    }
                }
            }
            map.values().find_map(|value| first_string(value, keys))
        }
        Value::Array(items) => items.iter().find_map(|value| first_string(value, keys)),
        _ => None,
    }
}

fn first_number<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a serde_json::Number> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(Value::Number(value)) = map.get(*key) {
                    return Some(value);
                }
            }
            map.values().find_map(|value| first_number(value, keys))
        }
        Value::Array(items) => items.iter().find_map(|value| first_number(value, keys)),
        _ => None,
    }
}

fn contains_key_fragment(value: &Value, fragments: &[&str]) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            let key = normalize_token(key);
            fragments
                .iter()
                .any(|fragment| key.contains(&normalize_token(fragment)))
                || contains_key_fragment(value, fragments)
        }),
        Value::Array(items) => items
            .iter()
            .any(|value| contains_key_fragment(value, fragments)),
        _ => false,
    }
}

fn contains_key_name(value: &Value, keys: &[&str]) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            let key = normalize_token(key);
            keys.iter()
                .any(|candidate| key == normalize_token(candidate))
                || contains_key_name(value, keys)
        }),
        Value::Array(items) => items.iter().any(|value| contains_key_name(value, keys)),
        _ => false,
    }
}

fn normalize_token(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace(['-', ' '], "_")
}

fn sha256_str(value: &str) -> String {
    sha256_marker(value.as_bytes())
}

fn sha256_marker(value: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct FixtureCase {
        name: String,
        payload: Value,
        expected_event_kind: String,
        expected_message_type: String,
        expected_text_content: bool,
        expected_media_reference: bool,
        expected_callback_file_credentials: bool,
    }

    #[test]
    fn shadow_capture_redacts_private_text_and_identifiers() {
        let raw = br#"{
          "event_type": "message",
          "msg_type": "text",
          "message_id": "live-message-id",
          "chat_id": "live-chat-id",
          "sender_id": "live-sender-id",
          "content": "private user request about a poster",
          "timestamp": 1784016000
        }"#;

        let report = shadow_capture_report(raw).expect("capture report");
        let serialized = serde_json::to_string(&report).expect("serialize report");

        assert!(report.success);
        assert!(report.dry_run);
        assert!(!report.safe_for_chat);
        assert!(!report.external_send_executed);
        assert!(!report.database_write_executed);
        assert!(!report.artifact_created);
        assert_eq!(report.event_kind, "message");
        assert_eq!(report.message_type, "text");
        assert_eq!(
            report.content_byte_count,
            Some("private user request about a poster".len())
        );
        assert!(report.message_id_sha256.unwrap().starts_with("sha256:"));
        assert!(report.chat_id_sha256.unwrap().starts_with("sha256:"));
        assert!(report.sender_id_sha256.unwrap().starts_with("sha256:"));
        assert!(report.content_sha256.unwrap().starts_with("sha256:"));

        for forbidden in [
            "live-message-id",
            "live-chat-id",
            "live-sender-id",
            "private user request",
        ] {
            assert!(
                !serialized.contains(forbidden),
                "shadow report leaked {forbidden}"
            );
        }
    }

    #[test]
    fn shadow_capture_reports_media_and_callback_presence_without_values() {
        let raw = br#"{
          "event": "callback",
          "room_id": "room-secret",
          "from_user": "sender-secret",
          "file": {
            "file_id": "credential-file-id",
            "download_url": "https://example.invalid/private.jpg",
            "filename": "poster-secret.jpg"
          }
        }"#;

        let report = shadow_capture_report(raw).expect("capture report");
        let serialized = serde_json::to_string(&report).expect("serialize report");

        assert_eq!(report.event_kind, "callback");
        assert_eq!(report.message_type, "media");
        assert!(report.field_presence.media_reference);
        assert!(report.field_presence.callback_file_credentials);
        for forbidden in [
            "room-secret",
            "sender-secret",
            "credential-file-id",
            "example.invalid",
            "poster-secret.jpg",
        ] {
            assert!(
                !serialized.contains(forbidden),
                "shadow report leaked {forbidden}"
            );
        }
    }

    #[test]
    fn shadow_capture_rejects_empty_or_oversized_payloads() {
        assert!(shadow_capture_report(b"").is_err());
        let oversized = vec![b' '; MAX_SHADOW_CAPTURE_BYTES + 1];
        assert!(shadow_capture_report(&oversized).is_err());
    }

    #[test]
    fn stdin_reader_enforces_preview_limit_while_reading() {
        let payload = vec![b' '; MAX_SHADOW_CAPTURE_BYTES + 8];
        assert!(read_bounded_payload(payload.as_slice()).is_err());
    }

    #[test]
    fn fixture_replay_covers_text_attachment_busy_and_unsupported_shapes() {
        let fixtures: Vec<FixtureCase> = serde_json::from_str(include_str!(
            "../fixtures/huabaosi_wecom_shadow_events.json"
        ))
        .expect("fixture json");

        for fixture in fixtures {
            let payload = serde_json::to_vec(&fixture.payload).expect("fixture payload");
            let report = shadow_capture_report(&payload).expect("capture report");
            let serialized = serde_json::to_string(&report).expect("serialize report");

            assert_eq!(
                report.event_kind, fixture.expected_event_kind,
                "{}",
                fixture.name
            );
            assert_eq!(
                report.message_type, fixture.expected_message_type,
                "{}",
                fixture.name
            );
            assert_eq!(
                report.field_presence.text_content, fixture.expected_text_content,
                "{}",
                fixture.name
            );
            assert_eq!(
                report.field_presence.media_reference, fixture.expected_media_reference,
                "{}",
                fixture.name
            );
            assert_eq!(
                report.field_presence.callback_file_credentials,
                fixture.expected_callback_file_credentials,
                "{}",
                fixture.name
            );
            assert!(report.dry_run, "{}", fixture.name);
            assert!(!report.apply_requested, "{}", fixture.name);
            assert!(!report.external_send_executed, "{}", fixture.name);
            assert!(!report.database_write_executed, "{}", fixture.name);

            for forbidden in [
                "fixture-message",
                "fixture-chat",
                "fixture-user",
                "fixture private",
                "busy internal",
                "download-token",
                "callback-secret",
                "fixture.example.invalid",
            ] {
                assert!(
                    !serialized.contains(forbidden),
                    "fixture {} leaked {forbidden}",
                    fixture.name
                );
            }
        }
    }
}
