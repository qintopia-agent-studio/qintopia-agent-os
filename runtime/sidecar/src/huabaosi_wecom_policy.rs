use std::io::{self, Read};

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

const MAX_POLICY_PREVIEW_BYTES: usize = 64 * 1024;
const SAFE_FALLBACK_COPY: &str =
    "我这边刚才没有整理出可读回复，先不把那段发出来。你可以直接说“重新说”，我会用更清楚的话继续。";

#[derive(Debug, Clone, Serialize)]
pub struct PolicyPreviewReport {
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
    pub message_classification: &'static str,
    pub direction: &'static str,
    pub message_type: &'static str,
    pub should_suppress: bool,
    pub suppression_reason: Option<&'static str>,
    pub busy_session_action: &'static str,
    pub formatting_fallback: FormattingFallbackPreview,
    pub user_safe_fallback_copy: Option<&'static str>,
    pub idempotency: IdempotencyPreview,
    pub guardrails: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FormattingFallbackPreview {
    pub detected: bool,
    pub classification: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdempotencyPreview {
    pub message_id_sha256: Option<String>,
    pub idempotency_key_sha256: String,
    pub duplicate_hint: bool,
    pub duplicate_action: &'static str,
}

pub fn run() -> Result<()> {
    let payload = read_stdin_payload()?;
    let report = policy_preview_report(payload.as_bytes())?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub fn policy_preview_report(payload: &[u8]) -> Result<PolicyPreviewReport> {
    if payload.is_empty() {
        bail!("Huabaosi WeCom policy payload must not be empty");
    }
    if payload.len() > MAX_POLICY_PREVIEW_BYTES {
        bail!("Huabaosi WeCom policy payload exceeds the 64 KiB preview limit");
    }

    let event: Value =
        serde_json::from_slice(payload).context("parse Huabaosi WeCom policy payload")?;
    let content = first_string(&event, TEXT_CONTENT_FIELDS).unwrap_or("");
    let direction = classify_direction(&event);
    let message_type = classify_message_type(&event);
    let internal_filter = classify_internal_filter(content, direction);
    let formatting_fallback = classify_formatting_fallback(content, direction);
    let formatting_fallback_detected = formatting_fallback.detected;
    let busy = first_bool(&event, BUSY_FIELDS).unwrap_or(false);
    let duplicate_hint = first_bool(&event, DUPLICATE_FIELDS).unwrap_or(false);
    let message_id = first_string(&event, MESSAGE_ID_FIELDS);
    let idempotency_key = message_id
        .map(|value| format!("huabaosi-wecom-message:{value}"))
        .unwrap_or_else(|| format!("huabaosi-wecom-payload:{}", sha256_marker(payload)));
    let should_suppress = internal_filter.should_suppress || duplicate_hint;

    Ok(PolicyPreviewReport {
        success: true,
        worker: "huabaosi-wecom-policy-preview",
        platform: "wecom",
        profile: "huabaosi",
        dry_run: true,
        apply_requested: false,
        action_status: "policy_preview",
        safe_for_chat: false,
        external_send_executed: false,
        database_write_executed: false,
        artifact_created: false,
        payload_sha256: sha256_marker(payload),
        payload_byte_count: payload.len(),
        message_classification: classify_message(content, message_type, &internal_filter),
        direction,
        message_type,
        should_suppress,
        suppression_reason: suppression_reason(&internal_filter, duplicate_hint),
        busy_session_action: busy_session_action(busy, internal_filter.should_suppress),
        formatting_fallback,
        user_safe_fallback_copy: user_safe_fallback_copy(
            busy,
            internal_filter.should_suppress,
            formatting_fallback_detected,
        ),
        idempotency: IdempotencyPreview {
            message_id_sha256: message_id.map(sha256_str),
            idempotency_key_sha256: sha256_str(&idempotency_key),
            duplicate_hint,
            duplicate_action: duplicate_action(duplicate_hint, message_id.is_some()),
        },
        guardrails: vec![
            "preview only; no database write is performed",
            "raw ids, text, media URLs, filenames, tokens, and callback credentials are never emitted",
            "this command must not send WeCom messages, call QiWe, call image providers, upload media, or write Feishu",
        ],
    })
}

#[derive(Debug, Clone, Copy)]
struct InternalFilter {
    should_suppress: bool,
    reason: Option<&'static str>,
}

const MESSAGE_ID_FIELDS: &[&str] = &["message_id", "msg_id", "msgid", "id"];
const TEXT_CONTENT_FIELDS: &[&str] = &["content", "text", "message_text", "plain_text"];
const DIRECTION_FIELDS: &[&str] = &["direction", "flow", "route"];
const MESSAGE_TYPE_FIELDS: &[&str] = &["msg_type", "message_type", "type"];
const BUSY_FIELDS: &[&str] = &["busy", "busy_session", "session_busy", "active_generation"];
const DUPLICATE_FIELDS: &[&str] = &["duplicate", "is_duplicate", "already_seen"];

fn read_stdin_payload() -> Result<String> {
    let stdin = io::stdin();
    read_bounded_payload(stdin.lock())
}

fn read_bounded_payload<R: Read>(reader: R) -> Result<String> {
    let mut payload = String::new();
    reader
        .take((MAX_POLICY_PREVIEW_BYTES + 1) as u64)
        .read_to_string(&mut payload)
        .context("read Huabaosi WeCom policy payload from stdin")?;
    if payload.len() > MAX_POLICY_PREVIEW_BYTES {
        bail!("Huabaosi WeCom policy payload exceeds the 64 KiB preview limit");
    }
    Ok(payload)
}

fn classify_direction(event: &Value) -> &'static str {
    let Some(direction) = first_string(event, DIRECTION_FIELDS) else {
        return "unknown";
    };
    match normalize_token(direction).as_str() {
        "outbound" | "outbound_candidate" | "bot_reply" | "reply" => "outbound_candidate",
        "inbound" | "incoming" | "user_message" => "inbound_user",
        _ => "unknown",
    }
}

fn classify_message_type(event: &Value) -> &'static str {
    if let Some(message_type) = first_string(event, MESSAGE_TYPE_FIELDS) {
        return match normalize_token(message_type).as_str() {
            "text" => "text",
            "image" => "image",
            "file" | "attachment" => "file",
            "mixed" => "mixed",
            _ => "unknown",
        };
    }
    if first_string(event, TEXT_CONTENT_FIELDS).is_some() {
        return "text";
    }
    if contains_key_name(
        event,
        &["image", "image_url", "file", "file_id", "attachment"],
    ) {
        return "media";
    }
    "unknown"
}

fn classify_message(
    content: &str,
    message_type: &'static str,
    internal_filter: &InternalFilter,
) -> &'static str {
    if internal_filter.should_suppress {
        return internal_filter.reason.unwrap_or("internal_process_status");
    }
    match message_type {
        "text" if !content.trim().is_empty() => "user_or_agent_text",
        "image" | "file" | "media" | "mixed" => "attachment_placeholder",
        _ => "unsupported_event_shape",
    }
}

fn classify_internal_filter(content: &str, direction: &str) -> InternalFilter {
    if direction == "inbound_user" {
        return InternalFilter {
            should_suppress: false,
            reason: None,
        };
    }

    let normalized = normalize_text(content);
    if normalized.starts_with("interrupting current task") {
        return InternalFilter {
            should_suppress: true,
            reason: Some("internal_process_status"),
        };
    }
    if normalized.starts_with("response formatting failed")
        || normalized.starts_with("failed to format response")
    {
        return InternalFilter {
            should_suppress: true,
            reason: Some("formatting_fallback"),
        };
    }
    if is_provider_retry_or_failure(&normalized) {
        return InternalFilter {
            should_suppress: true,
            reason: Some("provider_retry_or_failure_status"),
        };
    }
    InternalFilter {
        should_suppress: false,
        reason: None,
    }
}

fn is_provider_retry_or_failure(normalized: &str) -> bool {
    normalized.contains("retrying in ")
        || normalized.contains("api call failed after ")
        || normalized.contains("api failed after ")
        || (normalized.contains("http 503")
            && normalized.contains("service temporarily unavailable"))
}

fn classify_formatting_fallback(
    content: &str,
    direction: &'static str,
) -> FormattingFallbackPreview {
    let internal_filter = classify_internal_filter(content, direction);
    if internal_filter.reason == Some("formatting_fallback") {
        return FormattingFallbackPreview {
            detected: true,
            classification: "formatting_fallback",
        };
    }
    FormattingFallbackPreview {
        detected: false,
        classification: "none",
    }
}

fn suppression_reason(
    internal_filter: &InternalFilter,
    duplicate_hint: bool,
) -> Option<&'static str> {
    if duplicate_hint {
        return Some("duplicate_hint");
    }
    internal_filter.reason
}

fn busy_session_action(busy: bool, suppress_internal: bool) -> &'static str {
    match (busy, suppress_internal) {
        (true, true) => "suppress_internal_status_and_use_safe_fallback",
        (true, false) => "defer_or_queue_user_request_preview",
        (false, true) => "suppress_internal_status",
        (false, false) => "no_busy_session_action",
    }
}

fn user_safe_fallback_copy(
    busy: bool,
    suppress_internal: bool,
    formatting_fallback: bool,
) -> Option<&'static str> {
    if busy || suppress_internal || formatting_fallback {
        return Some(SAFE_FALLBACK_COPY);
    }
    None
}

fn duplicate_action(duplicate_hint: bool, has_message_id: bool) -> &'static str {
    match (duplicate_hint, has_message_id) {
        (true, _) => "suppress_duplicate_preview",
        (false, true) => "process_if_idempotency_key_unseen",
        (false, false) => "review_without_message_id",
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

fn first_bool(value: &Value, keys: &[&str]) -> Option<bool> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(Value::Bool(value)) = map.get(*key) {
                    return Some(*value);
                }
            }
            map.values().find_map(|value| first_bool(value, keys))
        }
        Value::Array(items) => items.iter().find_map(|value| first_bool(value, keys)),
        _ => None,
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

fn normalize_text(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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
        expected_message_classification: String,
        expected_should_suppress: bool,
        expected_suppression_reason: Option<String>,
        expected_busy_session_action: String,
        expected_formatting_fallback: bool,
        expected_duplicate_action: String,
        expected_fallback_copy: bool,
    }

    #[test]
    fn policy_preview_rejects_empty_or_oversized_payloads() {
        assert!(policy_preview_report(b"").is_err());
        let oversized = vec![b' '; MAX_POLICY_PREVIEW_BYTES + 1];
        assert!(policy_preview_report(&oversized).is_err());
    }

    #[test]
    fn stdin_reader_enforces_preview_limit_while_reading() {
        let payload = vec![b' '; MAX_POLICY_PREVIEW_BYTES + 8];
        assert!(read_bounded_payload(payload.as_slice()).is_err());
    }

    #[test]
    fn normal_plain_text_request_is_not_suppressed() {
        let raw = br#"{
          "direction": "inbound",
          "msg_type": "text",
          "message_id": "policy-message-plain",
          "content": "Please make the poster in plain text first"
        }"#;

        let report = policy_preview_report(raw).expect("policy report");
        let serialized = serde_json::to_string(&report).expect("serialize report");

        assert_eq!(report.message_classification, "user_or_agent_text");
        assert!(!report.should_suppress);
        assert!(!report.formatting_fallback.detected);
        assert!(report.user_safe_fallback_copy.is_none());
        assert!(!serialized.contains("Please make the poster"));
        assert!(!serialized.contains("plain text first"));
        assert!(!serialized.contains("policy-message-plain"));
    }

    #[test]
    fn unknown_direction_internal_template_is_still_suppressed() {
        let raw = br#"{
          "msg_type": "text",
          "message_id": "policy-message-internal-unknown-direction",
          "content": "Interrupting current task to handle new input"
        }"#;

        let report = policy_preview_report(raw).expect("policy report");
        let serialized = serde_json::to_string(&report).expect("serialize report");

        assert_eq!(report.message_classification, "internal_process_status");
        assert!(report.should_suppress);
        assert_eq!(report.suppression_reason, Some("internal_process_status"));
        assert!(!serialized.contains("Interrupting current task"));
        assert!(!serialized.contains("policy-message-internal-unknown-direction"));
    }

    #[test]
    fn fixture_replay_covers_policy_preview_contract() {
        let fixtures: Vec<FixtureCase> = serde_json::from_str(include_str!(
            "../fixtures/huabaosi_wecom_policy_events.json"
        ))
        .expect("fixture json");

        for fixture in fixtures {
            let payload = serde_json::to_vec(&fixture.payload).expect("fixture payload");
            let report = policy_preview_report(&payload).expect("policy report");
            let serialized = serde_json::to_string(&report).expect("serialize report");

            assert_eq!(
                report.message_classification, fixture.expected_message_classification,
                "{}",
                fixture.name
            );
            assert_eq!(
                report.should_suppress, fixture.expected_should_suppress,
                "{}",
                fixture.name
            );
            assert_eq!(
                report.suppression_reason.map(str::to_string),
                fixture.expected_suppression_reason,
                "{}",
                fixture.name
            );
            assert_eq!(
                report.busy_session_action, fixture.expected_busy_session_action,
                "{}",
                fixture.name
            );
            assert_eq!(
                report.formatting_fallback.detected, fixture.expected_formatting_fallback,
                "{}",
                fixture.name
            );
            assert_eq!(
                report.idempotency.duplicate_action, fixture.expected_duplicate_action,
                "{}",
                fixture.name
            );
            assert_eq!(
                report.user_safe_fallback_copy.is_some(),
                fixture.expected_fallback_copy,
                "{}",
                fixture.name
            );
            assert!(report.dry_run, "{}", fixture.name);
            assert!(!report.apply_requested, "{}", fixture.name);
            assert!(!report.external_send_executed, "{}", fixture.name);
            assert!(!report.database_write_executed, "{}", fixture.name);
            assert!(!report.artifact_created, "{}", fixture.name);
            assert!(report
                .idempotency
                .idempotency_key_sha256
                .starts_with("sha256:"));

            for forbidden in [
                "fixture-policy-message",
                "fixture policy private",
                "Interrupting current task",
                "Response formatting failed",
                "Retrying in",
                "API call failed",
                "HTTP 503",
                "Service temporarily unavailable",
                "plain text first",
                "fixture.example.invalid",
                "download-token",
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
