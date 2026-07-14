use std::{
    collections::BTreeSet,
    env,
    io::{self, Read},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
#[cfg(feature = "huabaosi-wecom-canary-gateway")]
use serde_json::json;
use sha2::{Digest, Sha256};
#[cfg(any(test, feature = "huabaosi-wecom-canary-gateway"))]
use url::Url;
use zeroize::Zeroize;
#[cfg(feature = "huabaosi-wecom-canary-gateway")]
use zeroize::Zeroizing;

#[cfg(feature = "huabaosi-wecom-canary-gateway")]
use crate::bounded_http::HttpClient;

const WORKER_ID: &str = "huabaosi-wecom-canary-gateway";
const MAX_CANARY_PAYLOAD_BYTES: usize = 64 * 1024;
#[cfg(feature = "huabaosi-wecom-canary-gateway")]
const MAX_CANARY_RESPONSE_BYTES: usize = 64 * 1024;
const CANARY_ENABLED_ENV: &str = "QINTOPIA_HUABAOSI_WECOM_CANARY_ENABLED";
const CANARY_APPROVAL_ENV: &str = "QINTOPIA_HUABAOSI_WECOM_CANARY_APPROVAL";
const CANARY_APPROVAL_PHRASE: &str = "approved-huabaosi-wecom-canary";
const CANARY_ENDPOINT_ENV: &str = "QINTOPIA_HUABAOSI_WECOM_CANARY_ENDPOINT";
const CANARY_TOKEN_ENV: &str = "QINTOPIA_HUABAOSI_WECOM_CANARY_TOKEN";
const CANARY_ALLOWED_BOT_IDS_ENV: &str = "QINTOPIA_HUABAOSI_WECOM_CANARY_ALLOWED_BOT_IDS";
const CANARY_ALLOWED_CHAT_IDS_ENV: &str = "QINTOPIA_HUABAOSI_WECOM_CANARY_ALLOWED_CHAT_IDS";
const CANARY_ALLOWED_USER_IDS_ENV: &str = "QINTOPIA_HUABAOSI_WECOM_CANARY_ALLOWED_USER_IDS";

const REQUIRED_CANARY_CONFIGURATION: &[&str] = &[
    CANARY_ENDPOINT_ENV,
    CANARY_TOKEN_ENV,
    CANARY_ALLOWED_BOT_IDS_ENV,
    CANARY_ALLOWED_CHAT_IDS_ENV,
    CANARY_ALLOWED_USER_IDS_ENV,
];

#[derive(Debug, Serialize)]
struct CanaryPreflightReport {
    pub success: bool,
    pub worker: &'static str,
    pub action_status: &'static str,
    pub adapter_compiled: bool,
    pub canary_enabled: bool,
    pub approval_present: bool,
    pub config_valid: bool,
    pub allowed_bot_count: usize,
    pub allowed_chat_count: usize,
    pub allowed_user_count: usize,
    pub missing_configuration: Vec<&'static str>,
    pub protocol: &'static str,
    pub rollback_command: &'static str,
    pub safe_for_chat: bool,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CanaryGatewayReport {
    pub success: bool,
    pub worker: &'static str,
    pub phase: &'static str,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub action_status: String,
    pub adapter_compiled: bool,
    pub canary_enabled: bool,
    pub allowlist_passed: bool,
    pub allowlist_scope_count: usize,
    pub bot_id_sha256: Option<String>,
    pub chat_id_sha256: Option<String>,
    pub user_id_sha256: Option<String>,
    pub message_sha256: String,
    pub message_byte_count: usize,
    pub idempotency_key_sha256: String,
    pub external_send_executed: Option<bool>,
    pub rollback_command: &'static str,
    pub safe_for_chat: bool,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CanaryPayload {
    bot_id: Option<String>,
    chat_id: Option<String>,
    user_id: Option<String>,
    message_text: String,
    idempotency_key: Option<String>,
}

impl Drop for CanaryPayload {
    fn drop(&mut self) {
        if let Some(value) = &mut self.bot_id {
            value.zeroize();
        }
        if let Some(value) = &mut self.chat_id {
            value.zeroize();
        }
        if let Some(value) = &mut self.user_id {
            value.zeroize();
        }
        self.message_text.zeroize();
        if let Some(value) = &mut self.idempotency_key {
            value.zeroize();
        }
    }
}

#[derive(Clone)]
struct CanaryConfig {
    #[cfg(any(test, feature = "huabaosi-wecom-canary-gateway"))]
    endpoint: Option<Url>,
    #[cfg(any(test, feature = "huabaosi-wecom-canary-gateway"))]
    token: Option<String>,
    canary_enabled: bool,
    approval_present: bool,
    allowed_bot_ids: BTreeSet<String>,
    allowed_chat_ids: BTreeSet<String>,
    allowed_user_ids: BTreeSet<String>,
    missing_configuration: Vec<&'static str>,
}

impl Drop for CanaryConfig {
    fn drop(&mut self) {
        #[cfg(any(test, feature = "huabaosi-wecom-canary-gateway"))]
        if let Some(token) = &mut self.token {
            token.zeroize();
        }
        self.allowed_bot_ids.clear();
        self.allowed_chat_ids.clear();
        self.allowed_user_ids.clear();
    }
}

trait CanarySender {
    fn send(&self, config: &CanaryConfig, payload: &CanaryPayload) -> SendOutcome;
}

#[derive(Debug, Clone)]
struct SendOutcome {
    success: bool,
    action_status: &'static str,
    external_send_executed: Option<bool>,
}

#[cfg(feature = "huabaosi-wecom-canary-gateway")]
struct BoundedHttpCanarySender {
    client: HttpClient,
}

#[cfg(feature = "huabaosi-wecom-canary-gateway")]
impl BoundedHttpCanarySender {
    fn production() -> Self {
        Self {
            client: HttpClient::production(),
        }
    }
}

#[cfg(feature = "huabaosi-wecom-canary-gateway")]
impl CanarySender for BoundedHttpCanarySender {
    fn send(&self, config: &CanaryConfig, payload: &CanaryPayload) -> SendOutcome {
        let Some(endpoint) = &config.endpoint else {
            return SendOutcome::terminal("missing_canary_endpoint");
        };
        let Some(token) = &config.token else {
            return SendOutcome::terminal("missing_canary_token");
        };
        let body = match canary_request_body(payload) {
            Ok(body) => body,
            Err(_) => return SendOutcome::terminal("invalid_canary_payload"),
        };
        let response = self.client.request(
            "POST",
            endpoint,
            &[("authorization", format!("Bearer {token}"))],
            &body,
            MAX_CANARY_RESPONSE_BYTES,
        );
        match response {
            Ok(response) if response.status == 200 => SendOutcome {
                success: true,
                action_status: "canary_send_accepted",
                external_send_executed: Some(true),
            },
            Ok(_) => SendOutcome {
                success: false,
                action_status: "canary_send_rejected",
                external_send_executed: Some(true),
            },
            Err(error) if error.request_may_have_been_sent() => SendOutcome {
                success: false,
                action_status: "canary_send_outcome_ambiguous",
                external_send_executed: None,
            },
            Err(_) => SendOutcome::terminal("canary_send_transport_failed_before_send"),
        }
    }
}

impl SendOutcome {
    #[cfg(feature = "huabaosi-wecom-canary-gateway")]
    fn terminal(action_status: &'static str) -> Self {
        Self {
            success: false,
            action_status,
            external_send_executed: Some(false),
        }
    }
}

pub fn run_preflight() -> Result<()> {
    let report = preflight_report();
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub fn run_gateway(apply: bool, dry_run: bool) -> Result<()> {
    #[cfg(feature = "huabaosi-wecom-canary-gateway")]
    {
        let sender = BoundedHttpCanarySender::production();
        let report = gateway_report(apply, dry_run, Some(&sender))?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        Ok(())
    }
    #[cfg(not(feature = "huabaosi-wecom-canary-gateway"))]
    {
        let report = gateway_report(apply, dry_run, None)?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        Ok(())
    }
}

fn preflight_report() -> CanaryPreflightReport {
    let config = CanaryConfig::from_env();
    let config_valid = config.is_valid_for_apply();
    CanaryPreflightReport {
        success: config_valid && canary_adapter_compiled(),
        worker: WORKER_ID,
        action_status: if !canary_adapter_compiled() {
            "staging_adapter_not_compiled"
        } else if config_valid {
            "canary_preflight_ready"
        } else {
            "canary_configuration_not_approved"
        },
        adapter_compiled: canary_adapter_compiled(),
        canary_enabled: config.canary_enabled,
        approval_present: config.approval_present,
        config_valid,
        allowed_bot_count: config.allowed_bot_ids.len(),
        allowed_chat_count: config.allowed_chat_ids.len(),
        allowed_user_count: config.allowed_user_ids.len(),
        missing_configuration: config.missing_configuration.clone(),
        protocol: "huabaosi_wecom_canary_https_json_v1",
        rollback_command: rollback_command(),
        safe_for_chat: false,
        limitations: vec![
            "default production builds exclude the canary sender".to_string(),
            "preflight does not open network or database connections".to_string(),
            "canary apply requires one reviewed staging command and exact allowlists".to_string(),
        ],
        guardrails: guardrails(),
    }
}

fn gateway_report(
    apply: bool,
    dry_run: bool,
    sender: Option<&dyn CanarySender>,
) -> Result<CanaryGatewayReport> {
    let config = CanaryConfig::from_env();
    if apply && !canary_adapter_compiled() {
        return Ok(CanaryGatewayReport::without_payload(
            apply,
            dry_run,
            config,
            "staging_adapter_not_compiled",
        ));
    }
    if apply && !dry_run && !config.is_valid_for_apply() {
        return Ok(CanaryGatewayReport::without_payload(
            apply,
            dry_run,
            config,
            "canary_configuration_not_approved",
        ));
    }

    let payload_json = read_stdin_payload()?;
    let payload_bytes = payload_json.as_bytes();
    let payload: CanaryPayload =
        serde_json::from_str(&payload_json).context("parse Huabaosi WeCom canary payload")?;
    let allowlist = allowlist_decision(&config, &payload);
    let idempotency_key = payload
        .idempotency_key
        .as_deref()
        .map(str::to_owned)
        .unwrap_or_else(|| sha256_marker(payload_bytes));
    let message_sha256 = sha256_str(&payload.message_text);

    let mut report = CanaryGatewayReport {
        success: false,
        worker: WORKER_ID,
        phase: "canary_gateway",
        dry_run: dry_run || !apply,
        apply_requested: apply,
        action_status: "canary_dry_run".to_string(),
        adapter_compiled: canary_adapter_compiled(),
        canary_enabled: config.canary_enabled,
        allowlist_passed: allowlist.passed,
        allowlist_scope_count: allowlist.scope_count,
        bot_id_sha256: payload.bot_id.as_deref().map(sha256_str),
        chat_id_sha256: payload.chat_id.as_deref().map(sha256_str),
        user_id_sha256: payload.user_id.as_deref().map(sha256_str),
        message_sha256,
        message_byte_count: payload.message_text.len(),
        idempotency_key_sha256: sha256_str(&idempotency_key),
        external_send_executed: Some(false),
        rollback_command: rollback_command(),
        safe_for_chat: false,
        limitations: vec![
            "canary gateway does not change the production Bot route".to_string(),
            "reports contain only hashes, counts, and fixed status labels".to_string(),
        ],
        guardrails: guardrails(),
    };

    if !allowlist.passed {
        report.action_status = allowlist.reason.to_string();
        return Ok(report);
    }
    if !apply || dry_run {
        report.success = true;
        report.action_status = "canary_dry_run_allowlisted".to_string();
        return Ok(report);
    }
    if !config.is_valid_for_apply() {
        report.action_status = "canary_configuration_not_approved".to_string();
        return Ok(report);
    }
    let Some(sender) = sender else {
        report.action_status = "staging_adapter_not_compiled".to_string();
        return Ok(report);
    };

    let outcome = sender.send(&config, &payload);
    report.success = outcome.success;
    report.action_status = outcome.action_status.to_string();
    report.external_send_executed = outcome.external_send_executed;
    Ok(report)
}

impl CanaryGatewayReport {
    fn without_payload(
        apply: bool,
        dry_run: bool,
        config: CanaryConfig,
        action_status: &'static str,
    ) -> Self {
        Self {
            success: false,
            worker: WORKER_ID,
            phase: "canary_gateway",
            dry_run,
            apply_requested: apply,
            action_status: action_status.to_string(),
            adapter_compiled: canary_adapter_compiled(),
            canary_enabled: config.canary_enabled,
            allowlist_passed: false,
            allowlist_scope_count: allowlist_scope_count(&config),
            bot_id_sha256: None,
            chat_id_sha256: None,
            user_id_sha256: None,
            message_sha256: sha256_str("payload_not_read"),
            message_byte_count: 0,
            idempotency_key_sha256: sha256_str("payload_not_read"),
            external_send_executed: Some(false),
            rollback_command: rollback_command(),
            safe_for_chat: false,
            limitations: vec![
                "apply stopped before reading stdin, database access, network access, or sending"
                    .to_string(),
            ],
            guardrails: guardrails(),
        }
    }
}

impl CanaryConfig {
    fn from_env() -> Self {
        let allowed_bot_ids = parse_csv_exact_set(&env::var(CANARY_ALLOWED_BOT_IDS_ENV).ok());
        let allowed_chat_ids = parse_csv_exact_set(&env::var(CANARY_ALLOWED_CHAT_IDS_ENV).ok());
        let allowed_user_ids = parse_csv_exact_set(&env::var(CANARY_ALLOWED_USER_IDS_ENV).ok());
        let mut missing_configuration = Vec::new();
        for name in REQUIRED_CANARY_CONFIGURATION {
            if env::var(name)
                .map(|value| value.trim().is_empty())
                .unwrap_or(true)
            {
                missing_configuration.push(*name);
            }
        }

        #[cfg(any(test, feature = "huabaosi-wecom-canary-gateway"))]
        let endpoint = env::var(CANARY_ENDPOINT_ENV)
            .ok()
            .and_then(|value| Url::parse(value.trim()).ok());
        #[cfg(any(test, feature = "huabaosi-wecom-canary-gateway"))]
        let token = env::var(CANARY_TOKEN_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty());

        Self {
            #[cfg(any(test, feature = "huabaosi-wecom-canary-gateway"))]
            endpoint,
            #[cfg(any(test, feature = "huabaosi-wecom-canary-gateway"))]
            token,
            canary_enabled: env_flag(CANARY_ENABLED_ENV),
            approval_present: env::var(CANARY_APPROVAL_ENV)
                .map(|value| value == CANARY_APPROVAL_PHRASE)
                .unwrap_or(false),
            allowed_bot_ids,
            allowed_chat_ids,
            allowed_user_ids,
            missing_configuration,
        }
    }

    fn is_valid_for_apply(&self) -> bool {
        self.canary_enabled
            && self.approval_present
            && allowlist_scope_count(self) > 0
            && self.missing_configuration.is_empty()
            && self.endpoint_valid_for_apply()
    }

    #[cfg(any(test, feature = "huabaosi-wecom-canary-gateway"))]
    fn endpoint_valid_for_apply(&self) -> bool {
        self.endpoint
            .as_ref()
            .map(|endpoint| endpoint.scheme() == "https" || cfg!(test))
            .unwrap_or(false)
            && self.token.is_some()
    }

    #[cfg(not(any(test, feature = "huabaosi-wecom-canary-gateway")))]
    fn endpoint_valid_for_apply(&self) -> bool {
        false
    }
}

struct AllowlistDecision {
    passed: bool,
    reason: &'static str,
    scope_count: usize,
}

fn allowlist_decision(config: &CanaryConfig, payload: &CanaryPayload) -> AllowlistDecision {
    let scope_count = allowlist_scope_count(config);
    if scope_count == 0 {
        return AllowlistDecision {
            passed: false,
            reason: "canary_allowlist_missing",
            scope_count,
        };
    }
    if !allowlist_dimension_matches(&config.allowed_bot_ids, payload.bot_id.as_deref()) {
        return AllowlistDecision {
            passed: false,
            reason: "canary_bot_not_allowlisted",
            scope_count,
        };
    }
    if !allowlist_dimension_matches(&config.allowed_chat_ids, payload.chat_id.as_deref()) {
        return AllowlistDecision {
            passed: false,
            reason: "canary_chat_not_allowlisted",
            scope_count,
        };
    }
    if !allowlist_dimension_matches(&config.allowed_user_ids, payload.user_id.as_deref()) {
        return AllowlistDecision {
            passed: false,
            reason: "canary_user_not_allowlisted",
            scope_count,
        };
    }
    AllowlistDecision {
        passed: true,
        reason: "canary_allowlisted",
        scope_count,
    }
}

fn allowlist_dimension_matches(allowlist: &BTreeSet<String>, value: Option<&str>) -> bool {
    allowlist.is_empty()
        || value
            .map(|value| allowlist.contains(value))
            .unwrap_or(false)
}

fn allowlist_scope_count(config: &CanaryConfig) -> usize {
    usize::from(!config.allowed_bot_ids.is_empty())
        + usize::from(!config.allowed_chat_ids.is_empty())
        + usize::from(!config.allowed_user_ids.is_empty())
}

#[cfg(feature = "huabaosi-wecom-canary-gateway")]
fn canary_request_body(payload: &CanaryPayload) -> Result<Zeroizing<Vec<u8>>> {
    let value = json!({
        "bot_id": payload.bot_id,
        "chat_id": payload.chat_id,
        "user_id": payload.user_id,
        "message_text": payload.message_text,
        "idempotency_key": payload.idempotency_key,
    });
    let body = serde_json::to_vec(&value).context("encode canary request")?;
    Ok(Zeroizing::new(body))
}

fn read_stdin_payload() -> Result<String> {
    let stdin = io::stdin();
    read_bounded_payload(stdin.lock())
}

fn read_bounded_payload<R: Read>(reader: R) -> Result<String> {
    let mut payload = String::new();
    reader
        .take((MAX_CANARY_PAYLOAD_BYTES + 1) as u64)
        .read_to_string(&mut payload)
        .context("read Huabaosi WeCom canary payload from stdin")?;
    if payload.len() > MAX_CANARY_PAYLOAD_BYTES {
        bail!("Huabaosi WeCom canary payload exceeds the 64 KiB preview limit");
    }
    Ok(payload)
}

fn parse_csv_exact_set(value: &Option<String>) -> BTreeSet<String> {
    value
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|item| {
            let item = item.trim();
            if item.is_empty() {
                None
            } else {
                Some(item.to_string())
            }
        })
        .collect()
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .map(|value| value.trim() == "1")
        .unwrap_or(false)
}

fn rollback_command() -> &'static str {
    "unset QINTOPIA_HUABAOSI_WECOM_CANARY_ENABLED and keep hermes-gateway-huabaosi.service as the production route"
}

fn guardrails() -> Vec<String> {
    vec![
        "no production Bot route is changed by this command".to_string(),
        "default builds exclude the live canary sender".to_string(),
        "raw ids, message text, tokens, endpoints, and response bodies are never emitted"
            .to_string(),
        "apply requires exact allowlist matching and owner-reviewed approval phrase".to_string(),
    ]
}

fn canary_adapter_compiled() -> bool {
    cfg!(feature = "huabaosi-wecom-canary-gateway")
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
    use anyhow::anyhow;
    use std::{
        collections::VecDeque,
        sync::{Mutex, MutexGuard, OnceLock},
    };

    struct EnvGuard {
        keys: Vec<&'static str>,
        _lock: MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn new(keys: Vec<&'static str>) -> Self {
            static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
            let lock = ENV_LOCK
                .get_or_init(|| Mutex::new(()))
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            for key in &keys {
                env::remove_var(key);
            }
            Self { keys, _lock: lock }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for key in &self.keys {
                env::remove_var(key);
            }
        }
    }

    struct FakeSender {
        outcomes: Mutex<VecDeque<SendOutcome>>,
    }

    impl FakeSender {
        fn new(outcome: SendOutcome) -> Self {
            Self {
                outcomes: Mutex::new(VecDeque::from([outcome])),
            }
        }
    }

    impl CanarySender for FakeSender {
        fn send(&self, _config: &CanaryConfig, _payload: &CanaryPayload) -> SendOutcome {
            self.outcomes
                .lock()
                .expect("fake sender")
                .pop_front()
                .expect("fake outcome")
        }
    }

    #[test]
    fn preflight_fails_closed_without_configuration_or_feature() {
        let _guard = EnvGuard::new(canary_env_keys());
        let report = preflight_report();

        assert_eq!(report.worker, WORKER_ID);
        assert!(!report.success);
        assert_eq!(report.adapter_compiled, canary_adapter_compiled());
        assert!(!report.canary_enabled);
        assert!(!report.approval_present);
        assert!(!report.config_valid);
        assert_eq!(report.allowed_bot_count, 0);
        assert_eq!(report.allowed_chat_count, 0);
        assert_eq!(report.allowed_user_count, 0);
        assert!(report.missing_configuration.contains(&CANARY_ENDPOINT_ENV));
        assert!(!report.safe_for_chat);
    }

    #[test]
    fn default_apply_stops_before_reading_payload_or_sending() {
        let _guard = EnvGuard::new(canary_env_keys());
        let report = gateway_report(true, false, None).expect("gateway report");

        assert!(!report.success);
        let expected_status = if canary_adapter_compiled() {
            "canary_configuration_not_approved"
        } else {
            "staging_adapter_not_compiled"
        };
        assert_eq!(report.action_status, expected_status);
        assert_eq!(report.message_byte_count, 0);
        assert_eq!(report.external_send_executed, Some(false));
    }

    #[test]
    fn dry_run_requires_exact_allowlist_and_never_leaks_payload() {
        let _guard = configured_canary_env();
        let raw = include_bytes!("../fixtures/huabaosi_wecom_canary_payload.json");

        let payload: CanaryPayload = serde_json::from_slice(raw).expect("payload");
        let config = CanaryConfig::from_env();
        let report = gateway_report_from_parts(false, true, &config, &payload, raw, None)
            .expect("gateway report");
        let serialized = serde_json::to_string(&report).expect("serialize report");

        assert!(report.success);
        assert_eq!(report.action_status, "canary_dry_run_allowlisted");
        assert!(report.allowlist_passed);
        assert_eq!(report.allowlist_scope_count, 3);
        assert_eq!(
            report.message_byte_count,
            "fixture private canary message".len()
        );
        assert_eq!(report.external_send_executed, Some(false));
        for forbidden in [
            "canary-bot-fixture",
            "canary-chat-fixture",
            "canary-user-fixture",
            "fixture private canary message",
            "fixture-canary-idempotency",
        ] {
            assert!(!serialized.contains(forbidden), "leaked {forbidden}");
        }
    }

    #[test]
    fn dry_run_rejects_non_allowlisted_chat_case_sensitively() {
        let _guard = configured_canary_env();
        let raw = br#"{
          "bot_id": "canary-bot-fixture",
          "chat_id": "CANARY-CHAT-FIXTURE",
          "user_id": "canary-user-fixture",
          "message_text": "fixture private canary message"
        }"#;

        let payload: CanaryPayload = serde_json::from_slice(raw).expect("payload");
        let config = CanaryConfig::from_env();
        let report = gateway_report_from_parts(false, true, &config, &payload, raw, None)
            .expect("gateway report");

        assert!(!report.success);
        assert!(!report.allowlist_passed);
        assert_eq!(report.action_status, "canary_chat_not_allowlisted");
        assert_eq!(report.external_send_executed, Some(false));
    }

    #[test]
    fn staging_apply_uses_sender_only_after_configuration_and_allowlist() {
        let _guard = configured_canary_env();
        let raw = br#"{
          "bot_id": "canary-bot-fixture",
          "chat_id": "canary-chat-fixture",
          "user_id": "canary-user-fixture",
          "message_text": "fixture private canary message"
        }"#;
        let payload: CanaryPayload = serde_json::from_slice(raw).expect("payload");
        let config = CanaryConfig::from_env();
        let sender = FakeSender::new(SendOutcome {
            success: true,
            action_status: "canary_send_accepted",
            external_send_executed: Some(true),
        });

        let report = gateway_report_from_parts(true, false, &config, &payload, raw, Some(&sender))
            .expect("gateway report");

        assert!(report.success);
        assert_eq!(report.action_status, "canary_send_accepted");
        assert_eq!(report.external_send_executed, Some(true));
    }

    #[test]
    fn staging_apply_reports_ambiguous_sender_outcome() {
        let _guard = configured_canary_env();
        let raw = br#"{
          "bot_id": "canary-bot-fixture",
          "chat_id": "canary-chat-fixture",
          "user_id": "canary-user-fixture",
          "message_text": "fixture private canary message"
        }"#;
        let payload: CanaryPayload = serde_json::from_slice(raw).expect("payload");
        let config = CanaryConfig::from_env();
        let sender = FakeSender::new(SendOutcome {
            success: false,
            action_status: "canary_send_outcome_ambiguous",
            external_send_executed: None,
        });

        let report = gateway_report_from_parts(true, false, &config, &payload, raw, Some(&sender))
            .expect("gateway report");

        assert!(!report.success);
        assert_eq!(report.action_status, "canary_send_outcome_ambiguous");
        assert_eq!(report.external_send_executed, None);
    }

    #[test]
    fn bounded_reader_rejects_oversized_payload() {
        let payload = vec![b' '; MAX_CANARY_PAYLOAD_BYTES + 8];
        assert!(read_bounded_payload(payload.as_slice()).is_err());
    }

    fn gateway_report_from_parts(
        apply: bool,
        dry_run: bool,
        config: &CanaryConfig,
        payload: &CanaryPayload,
        payload_bytes: &[u8],
        sender: Option<&dyn CanarySender>,
    ) -> Result<CanaryGatewayReport> {
        let allowlist = allowlist_decision(config, payload);
        let idempotency_key = payload
            .idempotency_key
            .as_deref()
            .map(str::to_owned)
            .unwrap_or_else(|| sha256_marker(payload_bytes));
        let mut report = CanaryGatewayReport {
            success: false,
            worker: WORKER_ID,
            phase: "canary_gateway",
            dry_run: dry_run || !apply,
            apply_requested: apply,
            action_status: "canary_dry_run".to_string(),
            adapter_compiled: canary_adapter_compiled(),
            canary_enabled: config.canary_enabled,
            allowlist_passed: allowlist.passed,
            allowlist_scope_count: allowlist.scope_count,
            bot_id_sha256: payload.bot_id.as_deref().map(sha256_str),
            chat_id_sha256: payload.chat_id.as_deref().map(sha256_str),
            user_id_sha256: payload.user_id.as_deref().map(sha256_str),
            message_sha256: sha256_str(&payload.message_text),
            message_byte_count: payload.message_text.len(),
            idempotency_key_sha256: sha256_str(&idempotency_key),
            external_send_executed: Some(false),
            rollback_command: rollback_command(),
            safe_for_chat: false,
            limitations: vec![],
            guardrails: guardrails(),
        };
        if !allowlist.passed {
            report.action_status = allowlist.reason.to_string();
            return Ok(report);
        }
        if !apply || dry_run {
            report.success = true;
            report.action_status = "canary_dry_run_allowlisted".to_string();
            return Ok(report);
        }
        if !config.is_valid_for_apply() {
            report.action_status = "canary_configuration_not_approved".to_string();
            return Ok(report);
        }
        let outcome = sender
            .ok_or_else(|| anyhow!("test sender required"))?
            .send(config, payload);
        report.success = outcome.success;
        report.action_status = outcome.action_status.to_string();
        report.external_send_executed = outcome.external_send_executed;
        Ok(report)
    }

    fn configured_canary_env() -> EnvGuard {
        let guard = EnvGuard::new(canary_env_keys());
        env::set_var(CANARY_ENABLED_ENV, "1");
        env::set_var(CANARY_APPROVAL_ENV, CANARY_APPROVAL_PHRASE);
        env::set_var(CANARY_ENDPOINT_ENV, "https://canary.example.test/wecom");
        env::set_var(CANARY_TOKEN_ENV, "fake-token");
        env::set_var(CANARY_ALLOWED_BOT_IDS_ENV, "canary-bot-fixture");
        env::set_var(CANARY_ALLOWED_CHAT_IDS_ENV, "canary-chat-fixture");
        env::set_var(CANARY_ALLOWED_USER_IDS_ENV, "canary-user-fixture");
        guard
    }

    fn canary_env_keys() -> Vec<&'static str> {
        vec![
            CANARY_ENABLED_ENV,
            CANARY_APPROVAL_ENV,
            CANARY_ENDPOINT_ENV,
            CANARY_TOKEN_ENV,
            CANARY_ALLOWED_BOT_IDS_ENV,
            CANARY_ALLOWED_CHAT_IDS_ENV,
            CANARY_ALLOWED_USER_IDS_ENV,
        ]
    }
}
