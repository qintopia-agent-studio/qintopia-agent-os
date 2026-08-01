use std::{
    collections::{BTreeMap, BTreeSet},
    env,
};

use anyhow::{bail, Context, Result};
use base64ct::{Base64, Encoding};
use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Postgres, Row, Transaction};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::conversation_policy::{
    conversation_ref, parse_identifier_set, source_message_ref, valid_external_id,
    INGRESS_ALLOWED_CHAT_IDS_ENV, INGRESS_ALLOWED_USER_IDS_ENV, POSTER_PRODUCTION_CAPABILITY,
};

const INGRESS_SCHEMA_VERSION: u8 = 3;
const INGRESS_OPERATION: &str = "feishu_message_ingest";
const INGRESS_ENABLED_ENV: &str = "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE";
const INGRESS_HMAC_KEY_ENV: &str = "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY";
const CALLBACK_KEY_ENV: &str = "QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY";
const BOT_OPEN_ID_ENV: &str = "QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID";
const INTERNAL_GROUP_ENABLED_ENV: &str = "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED";
const MAX_INGRESS_BODY_BYTES: usize = 32 * 1024;
const MAX_TEXT_CHARS: usize = 4_000;
const MAX_PAST_SECONDS: i64 = 300;
const MAX_FUTURE_SECONDS: i64 = 60;
const NONCE_EXPIRY_SECONDS: i64 = 600;
const SIGNATURE_DOMAIN: &[u8] = b"qintopia-feishu-message-ingress-v3\n";

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedIngressEnvelope {
    timestamp: String,
    nonce: String,
    signature: String,
    body_base64: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FeishuMessageIngest {
    operation: String,
    schema_version: u8,
    message: FeishuIngressMessage,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FeishuIngressMessage {
    platform: String,
    event_id: String,
    message_id: String,
    chat_id: String,
    chat_type: String,
    sender_id: String,
    sender_type: String,
    message_kind: String,
    text: String,
    is_mention_bot: bool,
    should_trigger: bool,
    #[serde(default)]
    mentioned_bot_ref: String,
    #[serde(default)]
    thread_root_message_id: String,
    #[serde(default)]
    parent_message_id: String,
    sent_at: Option<DateTime<Utc>>,
}

#[derive(Debug)]
struct VerifiedIngress {
    message: FeishuIngressMessage,
    nonce_hash: String,
    payload_hash: String,
    signed_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

#[derive(Debug)]
struct ActivePolicy {
    id: Uuid,
    conversation_ref: String,
    conversation_type: String,
    audience_class: String,
    policy_version: i64,
    return_mode: String,
    initiation_rule: String,
    status_visibility: String,
}

#[derive(Debug, Serialize)]
pub struct IngressResponse {
    success: bool,
    accepted: bool,
    deduped: bool,
    source_message_ref: String,
    conversation_ref: String,
    conversation_type: String,
    audience_class: String,
    policy_version: i64,
    external_send_executed: bool,
    group_send_authorized: bool,
}

pub struct IngressConfig {
    hmac_key: Zeroizing<Vec<u8>>,
    allowed_chat_ids: BTreeSet<String>,
    allowed_user_ids: BTreeSet<String>,
    bot_open_id_ref: String,
    internal_group_enabled: bool,
}

impl IngressConfig {
    pub fn from_env_optional() -> Result<Option<Self>> {
        let mut values = BTreeMap::new();
        insert_env_value(&mut values, INGRESS_ENABLED_ENV)?;
        insert_env_value(&mut values, INTERNAL_GROUP_ENABLED_ENV)?;
        if !parse_binary_flag(&values, INGRESS_ENABLED_ENV, false)? {
            return Self::from_values(values);
        }
        for name in [
            INGRESS_HMAC_KEY_ENV,
            CALLBACK_KEY_ENV,
            INGRESS_ALLOWED_CHAT_IDS_ENV,
            INGRESS_ALLOWED_USER_IDS_ENV,
            BOT_OPEN_ID_ENV,
        ] {
            insert_env_value(&mut values, name)?;
        }
        Self::from_values(values)
    }

    fn from_values(mut values: BTreeMap<&str, String>) -> Result<Option<Self>> {
        let enabled = parse_binary_flag(&values, INGRESS_ENABLED_ENV, false)?;
        let internal_group_enabled = parse_binary_flag(&values, INTERNAL_GROUP_ENABLED_ENV, false)?;
        if !enabled {
            if internal_group_enabled {
                bail!("Xiaoman Feishu internal-group ingress requires authenticated ingress");
            }
            return Ok(None);
        }
        let callback_key = values.remove(CALLBACK_KEY_ENV).map(Zeroizing::new);
        let hmac_key = Zeroizing::new(required_config_value(&mut values, INGRESS_HMAC_KEY_ENV)?);
        if !(32..=512).contains(&hmac_key.len()) {
            bail!("Xiaoman Feishu ingress HMAC key is invalid");
        }
        if callback_key.as_deref().map(|value| value.trim()) == Some(hmac_key.as_str()) {
            bail!("Xiaoman Feishu ingress and callback keys must be distinct");
        }
        let bot_open_id = required_config_value(&mut values, BOT_OPEN_ID_ENV)?;
        if !valid_external_id(&bot_open_id) {
            bail!("Xiaoman Feishu Bot identity is invalid");
        }
        let allowed_chat_ids = parse_identifier_set(
            INGRESS_ALLOWED_CHAT_IDS_ENV,
            &required_config_value(&mut values, INGRESS_ALLOWED_CHAT_IDS_ENV)?,
        )?;
        let allowed_user_ids = parse_identifier_set(
            INGRESS_ALLOWED_USER_IDS_ENV,
            &required_config_value(&mut values, INGRESS_ALLOWED_USER_IDS_ENV)?,
        )?;
        Ok(Some(Self {
            hmac_key: Zeroizing::new(hmac_key.as_bytes().to_vec()),
            allowed_chat_ids,
            allowed_user_ids,
            bot_open_id_ref: bot_open_id_ref(&bot_open_id),
            internal_group_enabled,
        }))
    }

    #[cfg(test)]
    fn fixture(group_enabled: bool) -> Self {
        Self {
            hmac_key: Zeroizing::new(b"fixture-ingress-key-with-32-bytes-minimum".to_vec()),
            allowed_chat_ids: ["oc_direct_fixture", "oc_group_fixture"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            allowed_user_ids: ["ou_user_fixture"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            bot_open_id_ref: bot_open_id_ref("ou_xiaoman_bot_fixture"),
            internal_group_enabled: group_enabled,
        }
    }
}

pub async fn handle(
    pool: &PgPool,
    config: Option<&IngressConfig>,
    envelope: SignedIngressEnvelope,
) -> Result<Value> {
    let config = config.context("authenticated Feishu message ingress is disabled")?;
    let verified = verify_envelope(config, envelope, Utc::now())?;
    let response = persist_verified_message(pool, config, verified).await?;
    serde_json::to_value(response).context("serialize Feishu message ingress response")
}

fn verify_envelope(
    config: &IngressConfig,
    envelope: SignedIngressEnvelope,
    now: DateTime<Utc>,
) -> Result<VerifiedIngress> {
    if envelope.timestamp.is_empty()
        || envelope.timestamp.len() > 20
        || !envelope.timestamp.bytes().all(|byte| byte.is_ascii_digit())
        || !is_lower_hex(&envelope.nonce, 32)
        || !is_lower_hex(&envelope.signature, 64)
        || envelope.body_base64.is_empty()
    {
        bail!("Feishu message ingress envelope is invalid");
    }
    let timestamp = envelope
        .timestamp
        .parse::<i64>()
        .context("parse Feishu message ingress timestamp")?;
    let signed_at = Utc
        .timestamp_opt(timestamp, 0)
        .single()
        .context("Feishu message ingress timestamp is invalid")?;
    if signed_at < now - TimeDelta::seconds(MAX_PAST_SECONDS)
        || signed_at > now + TimeDelta::seconds(MAX_FUTURE_SECONDS)
    {
        bail!("Feishu message ingress timestamp is outside the accepted window");
    }
    let body = Base64::decode_vec(&envelope.body_base64)
        .map_err(|_| anyhow::anyhow!("decode Feishu message ingress body"))?;
    if body.is_empty() || body.len() > MAX_INGRESS_BODY_BYTES {
        bail!("Feishu message ingress body length is invalid");
    }
    let mut mac = HmacSha256::new_from_slice(&config.hmac_key)
        .expect("HMAC accepts keys of any bounded length");
    mac.update(SIGNATURE_DOMAIN);
    mac.update(envelope.timestamp.as_bytes());
    mac.update(b"\n");
    mac.update(envelope.nonce.as_bytes());
    mac.update(b"\n");
    mac.update(&body);
    let signature = decode_lower_hex(&envelope.signature)?;
    mac.verify_slice(&signature)
        .map_err(|_| anyhow::anyhow!("Feishu message ingress signature is invalid"))?;
    let request: FeishuMessageIngest =
        serde_json::from_slice(&body).context("parse Feishu message ingress body")?;
    if request.operation != INGRESS_OPERATION || request.schema_version != INGRESS_SCHEMA_VERSION {
        bail!("Feishu message ingress operation is invalid");
    }
    validate_message(config, &request.message)?;
    Ok(VerifiedIngress {
        message: request.message,
        nonce_hash: digest(&["feishu-ingress-nonce-v3", &envelope.nonce]),
        payload_hash: sha256_marker(&body),
        signed_at,
        expires_at: signed_at + TimeDelta::seconds(NONCE_EXPIRY_SECONDS),
    })
}

fn validate_message(config: &IngressConfig, message: &FeishuIngressMessage) -> Result<()> {
    if message.platform != "feishu"
        || message.sender_type != "user"
        || message.message_kind != "text"
        || !valid_external_id(&message.event_id)
        || !valid_external_id(&message.message_id)
        || !valid_external_id(&message.chat_id)
        || !valid_external_id(&message.sender_id)
        || !config.allowed_chat_ids.contains(&message.chat_id)
        || !config.allowed_user_ids.contains(&message.sender_id)
    {
        bail!("Feishu message ingress identity is invalid or outside the deployment allowlist");
    }
    let text = message.text.trim();
    if text.is_empty() || text.chars().count() > MAX_TEXT_CHARS || text.contains('\0') {
        bail!("Feishu message ingress text is invalid");
    }
    for value in [
        message.thread_root_message_id.as_str(),
        message.parent_message_id.as_str(),
    ] {
        if !value.is_empty() && !valid_external_id(value) {
            bail!("Feishu message ingress thread identity is invalid");
        }
    }
    match message.chat_type.as_str() {
        "direct" => {
            if !message.should_trigger
                || message.is_mention_bot
                || !message.mentioned_bot_ref.is_empty()
            {
                bail!("Feishu direct-message trigger binding is invalid");
            }
        }
        "group" => {
            if !config.internal_group_enabled {
                bail!("Xiaoman Feishu internal-group ingress is disabled");
            }
            if !message.should_trigger
                || !message.is_mention_bot
                || message.mentioned_bot_ref != config.bot_open_id_ref
                || message.thread_root_message_id.is_empty()
            {
                bail!("Feishu group mention binding is invalid");
            }
        }
        _ => bail!("Feishu message ingress chat type is invalid"),
    }
    Ok(())
}

async fn persist_verified_message(
    pool: &PgPool,
    config: &IngressConfig,
    verified: VerifiedIngress,
) -> Result<IngressResponse> {
    let message = &verified.message;
    let conversation_ref = conversation_ref(&message.platform, &message.chat_id);
    let source_message_ref = source_message_ref(&message.platform, &message.message_id);
    let mut tx = pool.begin().await.context("begin Feishu message ingress")?;
    let nonce = sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.feishu_message_ingress_nonces
            (nonce_hash, payload_hash, signed_at, expires_at)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (nonce_hash) DO NOTHING
        "#,
    )
    .bind(&verified.nonce_hash)
    .bind(&verified.payload_hash)
    .bind(verified.signed_at)
    .bind(verified.expires_at)
    .execute(&mut *tx)
    .await
    .context("reserve Feishu message ingress nonce")?;
    if nonce.rows_affected() != 1 {
        bail!("Feishu message ingress nonce was already used");
    }

    let policy = load_active_policy(&mut tx, &conversation_ref).await?;
    validate_active_policy(config, message, &policy)?;
    let raw = json!({
        "ingress": "xiaoman_pre_gateway_dispatch_v3",
        "authenticated": true,
        "schema_version": INGRESS_SCHEMA_VERSION,
    });
    let message_row = sqlx::query(
        r#"
        INSERT INTO qintopia_messages.messages
            (platform, message_id, event_id, chat_id, chat_type, sender_id,
             sender_type, message_kind, text, is_mention_bot, should_trigger,
             trigger_reason, sent_at, received_at, thread_root_message_id,
             parent_message_id, raw)
        VALUES
            ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
             'xiaoman_authenticated_feishu_message_v3', $12, now(),
             NULLIF($13, ''), NULLIF($14, ''), $15)
        ON CONFLICT (platform, message_id) DO UPDATE SET
            last_seen_at = now(),
            duplicate_count = qintopia_messages.messages.duplicate_count + 1,
            updated_at = now()
        WHERE qintopia_messages.messages.event_id = EXCLUDED.event_id
          AND qintopia_messages.messages.chat_id = EXCLUDED.chat_id
          AND qintopia_messages.messages.chat_type = EXCLUDED.chat_type
          AND qintopia_messages.messages.sender_id = EXCLUDED.sender_id
          AND qintopia_messages.messages.sender_type = EXCLUDED.sender_type
          AND qintopia_messages.messages.message_kind = EXCLUDED.message_kind
          AND qintopia_messages.messages.text IS NOT DISTINCT FROM EXCLUDED.text
          AND qintopia_messages.messages.is_mention_bot = EXCLUDED.is_mention_bot
          AND qintopia_messages.messages.should_trigger = EXCLUDED.should_trigger
          AND qintopia_messages.messages.thread_root_message_id
              IS NOT DISTINCT FROM EXCLUDED.thread_root_message_id
          AND qintopia_messages.messages.parent_message_id
              IS NOT DISTINCT FROM EXCLUDED.parent_message_id
        RETURNING id, duplicate_count
        "#,
    )
    .bind(&message.platform)
    .bind(&message.message_id)
    .bind(&message.event_id)
    .bind(&message.chat_id)
    .bind(&message.chat_type)
    .bind(&message.sender_id)
    .bind(&message.sender_type)
    .bind(&message.message_kind)
    .bind(message.text.trim())
    .bind(message.is_mention_bot)
    .bind(message.should_trigger)
    .bind(message.sent_at)
    .bind(&message.thread_root_message_id)
    .bind(&message.parent_message_id)
    .bind(raw)
    .fetch_optional(&mut *tx)
    .await
    .context("persist authenticated Feishu message")?
    .context("authenticated Feishu message conflicts with its existing binding")?;
    let message_row_id: Uuid = message_row.try_get("id")?;
    let message_duplicate_count: i32 = message_row.try_get("duplicate_count")?;
    let receipt = sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.feishu_message_ingress_receipts
            (source_message_ref, message_row_id, conversation_ref, policy_id,
             policy_version, payload_hash)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (source_message_ref) DO UPDATE SET
            last_received_at = now(),
            duplicate_count = qintopia_agent_os.feishu_message_ingress_receipts.duplicate_count + 1
        WHERE qintopia_agent_os.feishu_message_ingress_receipts.message_row_id
                  = EXCLUDED.message_row_id
          AND qintopia_agent_os.feishu_message_ingress_receipts.conversation_ref
                  = EXCLUDED.conversation_ref
          AND qintopia_agent_os.feishu_message_ingress_receipts.policy_id = EXCLUDED.policy_id
          AND qintopia_agent_os.feishu_message_ingress_receipts.policy_version
                  = EXCLUDED.policy_version
          AND qintopia_agent_os.feishu_message_ingress_receipts.payload_hash
                  = EXCLUDED.payload_hash
        RETURNING duplicate_count
        "#,
    )
    .bind(&source_message_ref)
    .bind(message_row_id)
    .bind(&conversation_ref)
    .bind(policy.id)
    .bind(policy.policy_version)
    .bind(&verified.payload_hash)
    .fetch_optional(&mut *tx)
    .await
    .context("persist Feishu message ingress receipt")?
    .context("Feishu message ingress receipt conflicts with its existing binding")?;
    let receipt_duplicate_count: i32 = receipt.try_get("duplicate_count")?;
    sqlx::query(
        r#"
        DELETE FROM qintopia_agent_os.feishu_message_ingress_nonces
        WHERE nonce_hash IN (
            SELECT nonce_hash
            FROM qintopia_agent_os.feishu_message_ingress_nonces
            WHERE expires_at < now() - interval '1 hour'
            ORDER BY expires_at
            LIMIT 1000
        )
        "#,
    )
    .execute(&mut *tx)
    .await
    .context("prune expired Feishu message ingress nonces")?;
    tx.commit()
        .await
        .context("commit authenticated Feishu message ingress")?;
    Ok(IngressResponse {
        success: true,
        accepted: true,
        deduped: message_duplicate_count > 0 || receipt_duplicate_count > 0,
        source_message_ref,
        conversation_ref,
        conversation_type: policy.conversation_type,
        audience_class: policy.audience_class,
        policy_version: policy.policy_version,
        external_send_executed: false,
        group_send_authorized: false,
    })
}

async fn load_active_policy(
    tx: &mut Transaction<'_, Postgres>,
    conversation_ref: &str,
) -> Result<ActivePolicy> {
    let row = sqlx::query(
        r#"
        SELECT id, conversation_ref, conversation_type, audience_class,
               allowed_capabilities, return_mode, initiation_rule,
               status_visibility, policy_version
        FROM qintopia_agent_os.conversation_policies
        WHERE platform = 'feishu' AND conversation_ref = $1 AND enabled
        ORDER BY policy_version DESC
        LIMIT 1
        FOR SHARE
        "#,
    )
    .bind(conversation_ref)
    .fetch_optional(&mut **tx)
    .await
    .context("load active Feishu conversation policy")?
    .context("Feishu conversation has no active policy")?;
    let capabilities: Vec<String> = row.try_get("allowed_capabilities")?;
    if !capabilities
        .iter()
        .any(|value| value == POSTER_PRODUCTION_CAPABILITY)
    {
        bail!("Feishu conversation policy does not allow poster production");
    }
    Ok(ActivePolicy {
        id: row.try_get("id")?,
        conversation_ref: row.try_get("conversation_ref")?,
        conversation_type: row.try_get("conversation_type")?,
        audience_class: row.try_get("audience_class")?,
        policy_version: row.try_get("policy_version")?,
        return_mode: row.try_get("return_mode")?,
        initiation_rule: row.try_get("initiation_rule")?,
        status_visibility: row.try_get("status_visibility")?,
    })
}

fn validate_active_policy(
    config: &IngressConfig,
    message: &FeishuIngressMessage,
    policy: &ActivePolicy,
) -> Result<()> {
    let expected_ref = conversation_ref(&message.platform, &message.chat_id);
    if policy.conversation_ref != expected_ref || policy.conversation_type != message.chat_type {
        bail!("Feishu conversation policy identity does not match the message");
    }
    match message.chat_type.as_str() {
        "direct"
            if policy.audience_class == "private"
                && policy.return_mode == "direct_chat"
                && policy.initiation_rule == "direct_message"
                && policy.status_visibility == "requester" => {}
        "group"
            if config.internal_group_enabled
                && policy.audience_class == "internal_collaboration"
                && policy.return_mode == "thread_reply"
                && policy.initiation_rule == "explicit_bot_mention"
                && policy.status_visibility == "conversation_members" => {}
        _ => bail!("Feishu conversation policy does not authorize this ingress"),
    }
    Ok(())
}

pub(crate) fn bot_open_id_ref(bot_open_id: &str) -> String {
    digest(&["xiaoman-feishu-bot-v3", bot_open_id])
}

fn required_config_value(values: &mut BTreeMap<&str, String>, name: &str) -> Result<String> {
    values
        .remove(name)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .with_context(|| format!("{name} is required"))
}

fn insert_env_value(values: &mut BTreeMap<&'static str, String>, name: &'static str) -> Result<()> {
    match env::var(name) {
        Ok(value) => {
            values.insert(name, value);
            Ok(())
        }
        Err(env::VarError::NotPresent) => Ok(()),
        Err(env::VarError::NotUnicode(_)) => bail!("{name} must be valid UTF-8"),
    }
}

fn parse_binary_flag(values: &BTreeMap<&str, String>, name: &str, default: bool) -> Result<bool> {
    match values.get(name).map(|value| value.trim()) {
        None => Ok(default),
        Some("0") => Ok(false),
        Some("1") => Ok(true),
        Some(_) => bail!("{name} must be 0 or 1"),
    }
}

fn digest(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    format!("sha256:{:x}", hasher.finalize())
}

fn sha256_marker(value: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(value))
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn decode_lower_hex(value: &str) -> Result<Vec<u8>> {
    if !value.len().is_multiple_of(2) || !is_lower_hex(value, value.len()) {
        bail!("hex value is invalid");
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).expect("validated hex is UTF-8");
            u8::from_str_radix(text, 16).context("decode hex byte")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ingress_values(enabled: &str) -> BTreeMap<&'static str, String> {
        BTreeMap::from([
            (INGRESS_ENABLED_ENV, enabled.to_string()),
            (
                INGRESS_HMAC_KEY_ENV,
                "fixture-ingress-key-with-32-bytes-minimum".to_string(),
            ),
            (
                CALLBACK_KEY_ENV,
                "distinct-fixture-callback-key".to_string(),
            ),
            (BOT_OPEN_ID_ENV, "ou_xiaoman_bot_fixture".to_string()),
            (
                INGRESS_ALLOWED_CHAT_IDS_ENV,
                "oc_direct_fixture,oc_group_fixture".to_string(),
            ),
            (INGRESS_ALLOWED_USER_IDS_ENV, "ou_user_fixture".to_string()),
            (INTERNAL_GROUP_ENABLED_ENV, "0".to_string()),
        ])
    }

    #[cfg(feature = "postgres-integration-tests")]
    fn postgres_integration_database_url() -> String {
        assert_eq!(
            env::var("QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE").as_deref(),
            Ok("1"),
            "PostgreSQL integration test requires the explicit apply-smoke guard"
        );
        let database_url = env::var("QINTOPIA_SIDECAR_DATABASE_URL")
            .expect("PostgreSQL integration test requires QINTOPIA_SIDECAR_DATABASE_URL");
        let parsed = url::Url::parse(&database_url).expect("integration database URL must parse");
        assert!(matches!(parsed.host_str(), Some("127.0.0.1" | "::1")));
        assert_eq!(parsed.path().trim_start_matches('/'), "qintopia_test");
        database_url
    }

    fn signed_envelope(
        config: &IngressConfig,
        message: Value,
        timestamp: i64,
        nonce: &str,
    ) -> SignedIngressEnvelope {
        let body = serde_json::to_vec(&json!({
            "operation": INGRESS_OPERATION,
            "schema_version": INGRESS_SCHEMA_VERSION,
            "message": message,
        }))
        .unwrap();
        let timestamp = timestamp.to_string();
        let mut mac = HmacSha256::new_from_slice(&config.hmac_key).unwrap();
        mac.update(SIGNATURE_DOMAIN);
        mac.update(timestamp.as_bytes());
        mac.update(b"\n");
        mac.update(nonce.as_bytes());
        mac.update(b"\n");
        mac.update(&body);
        SignedIngressEnvelope {
            timestamp,
            nonce: nonce.to_string(),
            signature: format!("{:x}", mac.finalize().into_bytes()),
            body_base64: Base64::encode_string(&body),
        }
    }

    fn direct_message() -> Value {
        json!({
            "platform": "feishu",
            "event_id": "evt_direct_fixture",
            "message_id": "om_direct_fixture",
            "chat_id": "oc_direct_fixture",
            "chat_type": "direct",
            "sender_id": "ou_user_fixture",
            "sender_type": "user",
            "message_kind": "text",
            "text": "please create a poster",
            "is_mention_bot": false,
            "should_trigger": true,
            "mentioned_bot_ref": "",
            "thread_root_message_id": "",
            "parent_message_id": "",
            "sent_at": "2026-08-01T08:00:00Z"
        })
    }

    #[test]
    fn valid_hmac_envelope_accepts_direct_candidate() {
        let config = IngressConfig::fixture(false);
        let now = Utc.with_ymd_and_hms(2026, 8, 1, 8, 0, 0).unwrap();
        let envelope = signed_envelope(
            &config,
            direct_message(),
            now.timestamp(),
            "0123456789abcdef0123456789abcdef",
        );
        let verified = verify_envelope(&config, envelope, now).expect("envelope verifies");
        assert_eq!(verified.message.chat_type, "direct");
        assert!(verified.payload_hash.starts_with("sha256:"));
    }

    #[test]
    fn explicit_enable_gate_controls_ingress_configuration() {
        let disabled = ingress_values("0");
        assert!(IngressConfig::from_values(disabled).unwrap().is_none());
        let mut disabled_without_flag = ingress_values("0");
        disabled_without_flag.remove(INGRESS_ENABLED_ENV);
        assert!(IngressConfig::from_values(disabled_without_flag)
            .unwrap()
            .is_none());

        let enabled = ingress_values("1");
        assert!(IngressConfig::from_values(enabled).unwrap().is_some());

        for invalid in ["", "true", "2"] {
            let values = ingress_values(invalid);
            assert!(IngressConfig::from_values(values).is_err());
        }

        let mut partial = ingress_values("1");
        partial.remove(INGRESS_HMAC_KEY_ENV);
        assert!(IngressConfig::from_values(partial).is_err());

        let mut disabled_group = ingress_values("0");
        disabled_group.insert(INTERNAL_GROUP_ENABLED_ENV, "1".to_string());
        assert!(IngressConfig::from_values(disabled_group).is_err());
    }

    #[test]
    fn shared_python_hmac_fixture_verifies_in_rust() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../fixtures/feishu/xiaoman-message-ingress-envelope-v3.json"
        ))
        .unwrap();
        let mut config = IngressConfig::fixture(false);
        config.hmac_key = Zeroizing::new(fixture["hmac_key"].as_str().unwrap().as_bytes().to_vec());
        let envelope: SignedIngressEnvelope =
            serde_json::from_value(fixture["envelope"].clone()).unwrap();
        let now = Utc
            .timestamp_opt(fixture["now"].as_i64().unwrap(), 0)
            .single()
            .unwrap();
        let verified = verify_envelope(&config, envelope, now).expect("fixture verifies");
        assert_eq!(verified.message.message_id, "om_direct_fixture");
    }

    #[test]
    fn tampered_or_stale_envelope_is_rejected() {
        let config = IngressConfig::fixture(false);
        let now = Utc.with_ymd_and_hms(2026, 8, 1, 8, 0, 0).unwrap();
        let mut tampered = signed_envelope(
            &config,
            direct_message(),
            now.timestamp(),
            "1123456789abcdef0123456789abcdef",
        );
        tampered.body_base64.push('A');
        assert!(verify_envelope(&config, tampered, now).is_err());
        let stale = signed_envelope(
            &config,
            direct_message(),
            now.timestamp() - MAX_PAST_SECONDS - 1,
            "2123456789abcdef0123456789abcdef",
        );
        assert!(verify_envelope(&config, stale, now).is_err());
    }

    #[test]
    fn group_candidate_requires_feature_and_exact_bot_binding() {
        let now = Utc.with_ymd_and_hms(2026, 8, 1, 8, 0, 0).unwrap();
        let group_message = json!({
            "platform": "feishu",
            "event_id": "evt_group_fixture",
            "message_id": "om_group_fixture",
            "chat_id": "oc_group_fixture",
            "chat_type": "group",
            "sender_id": "ou_user_fixture",
            "sender_type": "user",
            "message_kind": "text",
            "text": "@xiaoman create a poster",
            "is_mention_bot": true,
            "should_trigger": true,
            "mentioned_bot_ref": bot_open_id_ref("ou_xiaoman_bot_fixture"),
            "thread_root_message_id": "om_group_fixture",
            "parent_message_id": "",
            "sent_at": "2026-08-01T08:00:00Z"
        });
        let disabled = IngressConfig::fixture(false);
        let envelope = signed_envelope(
            &disabled,
            group_message.clone(),
            now.timestamp(),
            "3123456789abcdef0123456789abcdef",
        );
        assert!(verify_envelope(&disabled, envelope, now).is_err());
        let enabled = IngressConfig::fixture(true);
        let envelope = signed_envelope(
            &enabled,
            group_message,
            now.timestamp(),
            "4123456789abcdef0123456789abcdef",
        );
        assert!(verify_envelope(&enabled, envelope, now).is_ok());
    }

    #[test]
    fn bot_sender_and_wrong_user_are_rejected() {
        let config = IngressConfig::fixture(false);
        let now = Utc.with_ymd_and_hms(2026, 8, 1, 8, 0, 0).unwrap();
        for (index, message) in [
            {
                let mut message = direct_message();
                message["sender_type"] = json!("bot");
                message
            },
            {
                let mut message = direct_message();
                message["sender_id"] = json!("ou_not_allowed");
                message
            },
        ]
        .into_iter()
        .enumerate()
        {
            let nonce = format!("{index:032x}");
            let envelope = signed_envelope(&config, message, now.timestamp(), &nonce);
            assert!(verify_envelope(&config, envelope, now).is_err());
        }
    }

    #[tokio::test]
    #[cfg(feature = "postgres-integration-tests")]
    #[ignore = "requires guarded disposable PostgreSQL qintopia_test"]
    async fn postgres_signed_ingress_dedupes_and_enables_one_v3_direct_workflow() {
        let database_url = postgres_integration_database_url();
        let pool = crate::db::connect(&database_url, 2)
            .await
            .expect("connect guarded Feishu ingress database");
        crate::db::run_migrations(&pool)
            .await
            .expect("migrate guarded Feishu ingress database");
        let suffix = Uuid::new_v4().simple().to_string();
        let chat_id = format!("oc_{suffix}");
        let user_id = format!("ou_{suffix}");
        let message_id = format!("om_{suffix}");
        let event_id = format!("evt_{suffix}");
        let policy_id = Uuid::new_v4();
        let policy_ref = conversation_ref("feishu", &chat_id);
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.conversation_policies
                (id, platform, conversation_ref, conversation_type, audience_class,
                 allowed_capabilities, return_mode, initiation_rule, status_visibility,
                 policy_version, policy_digest, enabled)
            VALUES ($1, 'feishu', $2, 'direct', 'private',
                    ARRAY['poster_production_request']::text[], 'direct_chat',
                    'direct_message', 'requester', 1, $3, true)
            "#,
        )
        .bind(policy_id)
        .bind(&policy_ref)
        .bind(digest(&["integration-policy-v3", &suffix]))
        .execute(&pool)
        .await
        .expect("insert direct conversation policy");
        let config = IngressConfig {
            hmac_key: Zeroizing::new(b"fixture-ingress-key-with-32-bytes-minimum".to_vec()),
            allowed_chat_ids: [chat_id.clone()].into_iter().collect(),
            allowed_user_ids: [user_id.clone()].into_iter().collect(),
            bot_open_id_ref: bot_open_id_ref("ou_xiaoman_bot_fixture"),
            internal_group_enabled: false,
        };
        let request_text = "AgentOS 海报链路验收 2026-08-01 16:00 线上";
        let message = json!({
            "platform": "feishu",
            "event_id": event_id,
            "message_id": message_id,
            "chat_id": chat_id,
            "chat_type": "direct",
            "sender_id": user_id,
            "sender_type": "user",
            "message_kind": "text",
            "text": request_text,
            "is_mention_bot": false,
            "should_trigger": true,
            "mentioned_bot_ref": "",
            "thread_root_message_id": "",
            "parent_message_id": "",
            "sent_at": Utc::now()
        });
        let now = Utc::now();
        let first_nonce = Uuid::new_v4().simple().to_string();
        let second_nonce = Uuid::new_v4().simple().to_string();
        let before_work_items: i64 =
            sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.work_items")
                .fetch_one(&pool)
                .await
                .unwrap();
        let first = handle(
            &pool,
            Some(&config),
            signed_envelope(&config, message.clone(), now.timestamp(), &first_nonce),
        )
        .await
        .expect("persist first signed message");
        let duplicate = handle(
            &pool,
            Some(&config),
            signed_envelope(&config, message.clone(), now.timestamp(), &second_nonce),
        )
        .await
        .expect("dedupe signed message with a fresh nonce");
        let replay = handle(
            &pool,
            Some(&config),
            signed_envelope(&config, message, now.timestamp(), &second_nonce),
        )
        .await;
        assert_eq!(first["deduped"], false);
        assert_eq!(duplicate["deduped"], true);
        assert!(replay.is_err(), "a used nonce must be rejected");

        let poster_input = || crate::operations_intake::V3PosterIntegrationInput {
            conversation_id: &chat_id,
            requester_user_id: &user_id,
            source_message_id: &message_id,
            request: request_text,
            title: "AgentOS 海报链路验收",
            schedule: "2026-08-01 16:00",
            location: "线上",
        };
        let first_workflow = crate::operations_intake::submit_v3_poster_for_postgres_integration(
            &pool,
            poster_input(),
        )
        .await
        .expect("accept authenticated V3 direct poster request");
        let duplicate_workflow =
            crate::operations_intake::submit_v3_poster_for_postgres_integration(
                &pool,
                poster_input(),
            )
            .await
            .expect("dedupe authenticated V3 direct poster request");
        assert_eq!(
            first_workflow["workflow_root_id"],
            duplicate_workflow["workflow_root_id"]
        );
        assert_eq!(duplicate_workflow["deduped"], true);
        let root_id =
            Uuid::parse_str(first_workflow["workflow_root_id"].as_str().unwrap()).unwrap();
        let persisted: (String, Option<String>, i32, i32, i64, i64) = sqlx::query_as(
            r#"
            SELECT message.sender_type, message.thread_root_message_id,
                   message.duplicate_count, receipt.duplicate_count,
                   count(DISTINCT root.id),
                   count(DISTINCT forbidden.id)
            FROM qintopia_messages.messages message
            JOIN qintopia_agent_os.feishu_message_ingress_receipts receipt
              ON receipt.message_row_id = message.id
            LEFT JOIN qintopia_agent_os.work_items root ON root.id = $2
            LEFT JOIN qintopia_agent_os.work_items forbidden
              ON forbidden.work_item_type = 'group_message_request'
             AND (forbidden.id = root.id OR forbidden.parent_work_item_id = root.id)
            WHERE message.platform = 'feishu' AND message.message_id = $1
            GROUP BY message.sender_type, message.thread_root_message_id,
                     message.duplicate_count, receipt.duplicate_count
            "#,
        )
        .bind(&message_id)
        .bind(root_id)
        .fetch_one(&pool)
        .await
        .expect("read authenticated message and workflow state");
        assert_eq!(persisted, ("user".to_string(), None, 1, 1, 1, 0));
        let after_work_items: i64 =
            sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.work_items")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(after_work_items > before_work_items);
        let target: (String, String, i64, String) = sqlx::query_as(
            r#"
            SELECT audience_class, conversation_ref, policy_version, delivery_mode
            FROM qintopia_agent_os.poster_return_targets
            WHERE origin_ref = (
                SELECT metadata #>> '{workflow_metadata,origin_conversation_ref}'
                FROM qintopia_agent_os.work_items WHERE id = $1
            )
            "#,
        )
        .bind(root_id)
        .fetch_one(&pool)
        .await
        .expect("read V3 direct return target");
        assert_eq!(
            target,
            (
                "private".to_string(),
                policy_ref,
                1,
                "direct_chat".to_string()
            )
        );
    }
}
