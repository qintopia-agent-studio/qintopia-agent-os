use std::{
    collections::BTreeSet,
    io::{self, Read},
};

use anyhow::{anyhow, bail, Context, Result};
#[cfg(any(test, feature = "qiwe-staging-adapter"))]
use md5::Md5;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

#[cfg(any(test, feature = "qiwe-staging-adapter"))]
use crate::bounded_http::{HttpClient, HttpResponse};
#[cfg(any(test, feature = "qiwe-staging-adapter"))]
use crate::qiwe_image_send_state::QiweUploadClaim;
#[cfg(feature = "qiwe-staging-adapter")]
use crate::qiwe_image_send_state::{
    CallbackClaimOutcome, QiweCallbackFileIdentity, SendFailureDisposition,
    UploadFailureDisposition,
};
use crate::{config::Cli, db, qiwe_image_send_state, url_policy};
use url::Url;

const WORKER_ID: &str = "qiwe-image-send-adapter";
const ASYNC_UPLOAD_METHOD: &str = "/cloud/cdnUploadByUrlAsync";
#[cfg(any(test, feature = "qiwe-staging-adapter"))]
const TEMPORARY_STORAGE_UPLOAD_METHOD: &str = "/cloud/cloudUpload";
const SEND_IMAGE_METHOD: &str = "/msg/sendImage";
#[cfg(any(test, feature = "qiwe-staging-adapter"))]
const FILE_API_PATH: &str = "/qiwe/api/qw/doFileApi";
#[cfg(any(test, feature = "qiwe-staging-adapter"))]
const FEISHU_PRIMARY_STORAGE_URI_PREFIX: &str = "feishu-base://huabaosi-generated-image/";
const IMAGE_FILE_TYPE: u8 = 1;
const ASYNC_EVENT_COMMAND: i64 = 20_000;
const SEND_SUCCESS_VALUE: i64 = 1;
const MAX_JSON_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_CALLBACK_INPUT_BYTES: usize = 64 * 1024;
const STAGING_APPROVAL_ENV: &str = "QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL";
const STAGING_APPROVAL_PHRASE: &str = "approved-staging-qiwe-image-send";
const STAGING_DATABASE_URL_SHA256_ENV: &str = "QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256";
const REQUIRED_QIWE_IMAGE_SEND_CONFIGURATION: &[&str] = &[
    "QIWE_API_URL",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS",
];
const REQUIRED_FEISHU_DELIVERY_CONFIGURATION: &[&str] = &[
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED",
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL",
    "QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA",
    "QINTOPIA_DEPLOYED_COMMIT_SHA",
    "QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256",
    "QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN",
    "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS",
    "QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID",
    "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS",
    "QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH",
    "QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION",
];

#[derive(Debug, Serialize)]
pub struct QiweImageSendPreflightReport {
    pub success: bool,
    pub worker: &'static str,
    pub action_status: &'static str,
    pub adapter_compiled: bool,
    pub feishu_delivery_bridge_compiled: bool,
    pub send_enabled: bool,
    pub config_valid: bool,
    pub webhook_ready: bool,
    pub allowed_host_count: usize,
    pub media_allowed_host_count: usize,
    pub allowed_group_count: usize,
    pub missing_configuration: Vec<&'static str>,
    pub protocol: &'static str,
    pub safe_for_chat: bool,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct QiweImageSendStagingPreflightReport {
    pub success: bool,
    pub worker: &'static str,
    pub action_status: &'static str,
    pub adapter_compiled: bool,
    pub feishu_delivery_bridge_compiled: bool,
    pub send_enabled: bool,
    pub owner_approval_valid: bool,
    pub config_valid: bool,
    pub database_boundary_valid: bool,
    pub webhook_ready: bool,
    pub allowed_host_count: usize,
    pub media_allowed_host_count: usize,
    pub allowed_group_count: usize,
    pub missing_configuration: Vec<&'static str>,
    pub protocol: &'static str,
    pub safe_for_chat: bool,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct QiweImageSendWorkerReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub worker: &'static str,
    pub phase: &'static str,
    pub action_status: String,
    pub work_item_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_content_hash: Option<String>,
    pub external_upload_requested: bool,
    pub callback_received: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_credential_schema: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_additional_field_count: Option<usize>,
    pub external_send_executed: Option<bool>,
    pub safe_for_chat: bool,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Clone)]
struct AdapterConfig {
    #[cfg(any(test, feature = "qiwe-staging-adapter"))]
    api_url: Url,
    #[cfg(any(test, feature = "qiwe-staging-adapter"))]
    token: String,
    #[cfg(any(test, feature = "qiwe-staging-adapter"))]
    guid: String,
    allowed_hosts: BTreeSet<String>,
    media_allowed_hosts: BTreeSet<String>,
    allowed_groups: BTreeSet<String>,
    webhook_ready: bool,
}

struct SendBoundaryPolicy {
    allowed_groups: BTreeSet<String>,
    media_allowed_hosts: BTreeSet<String>,
}

impl SendBoundaryPolicy {
    fn from_env() -> Result<Self> {
        let media_allowed_hosts =
            parse_csv_set(&required_env("QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS")?);
        if media_allowed_hosts.is_empty() {
            bail!("at least one generated-image media host must be allowlisted");
        }
        let allowed_groups =
            parse_csv_exact_set(&required_env("QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS")?);
        if allowed_groups.is_empty() {
            bail!("at least one QiWe target group must be allowlisted");
        }
        Ok(Self {
            allowed_groups,
            media_allowed_hosts,
        })
    }
}

impl Drop for AdapterConfig {
    fn drop(&mut self) {
        #[cfg(any(test, feature = "qiwe-staging-adapter"))]
        {
            self.token.zeroize();
            self.guid.zeroize();
        }
    }
}

#[derive(Serialize)]
struct ApiRequest<T> {
    method: &'static str,
    params: T,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AsyncUploadParams<'a> {
    guid: &'a str,
    filename: &'a str,
    file_url: &'a str,
    file_type: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SendImageParams<'a> {
    guid: &'a str,
    file_aes_key: &'a str,
    file_id: &'a str,
    file_md5: &'a str,
    file_size: u64,
    filename: &'a str,
    to_id: &'a str,
}

#[derive(Deserialize)]
struct ApiResponse<T> {
    code: i64,
    data: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadAcceptedData {
    request_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg(any(test, feature = "qiwe-staging-adapter"))]
struct TemporaryStorageAcceptedData {
    cloud_url: String,
}

#[cfg(any(test, feature = "qiwe-staging-adapter"))]
struct SensitiveUrl {
    raw: Zeroizing<String>,
}

#[cfg(any(test, feature = "qiwe-staging-adapter"))]
impl SensitiveUrl {
    fn new(raw: Zeroizing<String>) -> Self {
        Self { raw }
    }

    fn with_url<T>(&self, f: impl FnOnce(&Url) -> T) -> Result<T> {
        // url::Url owns another buffer, so parsed values must stay short-lived.
        let url = Url::parse(&self.raw).context("parse QiWe temporary-storage URL")?;
        Ok(f(&url))
    }

    fn as_str(&self) -> &str {
        &self.raw
    }
}

#[derive(Deserialize)]
struct CallbackEnvelope {
    code: i64,
    data: Vec<CallbackEvent>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CallbackEvent {
    request_id: String,
    cmd: i64,
    msg_data: Option<Value>,
}

struct ParsedCallback {
    request_id: String,
    credential_shape: CallbackCredentialShape,
    #[cfg(any(test, feature = "qiwe-staging-adapter"))]
    credentials: QiweImageCredentials,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CallbackCredentialShape {
    schema_id: &'static str,
    additional_field_count: usize,
}

impl Drop for ParsedCallback {
    fn drop(&mut self) {
        self.request_id.zeroize();
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QiweImageCredentials {
    #[serde(alias = "fileAeskey")]
    file_aes_key: String,
    file_id: String,
    file_md5: String,
    file_size: u64,
    #[serde(alias = "fileName")]
    filename: String,
}

impl Drop for QiweImageCredentials {
    fn drop(&mut self) {
        self.file_aes_key.zeroize();
        self.file_id.zeroize();
        self.file_md5.zeroize();
        self.filename.zeroize();
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendImageData {
    is_send_success: i64,
    msg_unique_identifier: String,
    seq: i64,
    timestamp: i64,
}

pub struct QiweSendReceipt {
    pub is_send_success: i64,
    pub message_identifier: String,
    pub sequence: i64,
    pub timestamp: i64,
}

impl Drop for QiweSendReceipt {
    fn drop(&mut self) {
        self.message_identifier.zeroize();
    }
}

#[cfg(any(test, feature = "qiwe-staging-adapter"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UploadCallFailure {
    Rejected,
    OutcomeUnknown,
}

#[cfg(any(test, feature = "qiwe-staging-adapter"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SendCallFailure {
    NotSent,
    Ambiguous,
}

struct WorkerReportState {
    success: bool,
    dry_run: bool,
    apply_requested: bool,
    phase: &'static str,
    action_status: String,
    work_item_id: Option<Uuid>,
    external_upload_requested: bool,
    callback_received: bool,
    external_send_executed: Option<bool>,
}

struct PreflightReportState {
    config_valid: bool,
    send_enabled: bool,
    adapter_compiled: bool,
    feishu_delivery_bridge_compiled: bool,
    webhook_ready: bool,
    allowed_host_count: usize,
    media_allowed_host_count: usize,
    allowed_group_count: usize,
    missing_configuration: Vec<&'static str>,
}

struct StagingPreflightReportState {
    adapter_compiled: bool,
    feishu_delivery_bridge_compiled: bool,
    send_enabled: bool,
    owner_approval_valid: bool,
    config_valid: bool,
    database_boundary_valid: bool,
    webhook_ready: bool,
    allowed_host_count: usize,
    media_allowed_host_count: usize,
    allowed_group_count: usize,
    missing_configuration: Vec<&'static str>,
}

pub fn run_preflight() -> Result<()> {
    validate_contract()?;
    let send_enabled = env_flag("QINTOPIA_QIWE_IMAGE_SEND_ENABLED")?;
    let adapter_compiled = qiwe_staging_adapter_compiled();
    let report = match AdapterConfig::from_env() {
        Ok(config) => preflight_report(PreflightReportState {
            config_valid: true,
            send_enabled,
            adapter_compiled,
            feishu_delivery_bridge_compiled: feishu_delivery_bridge_compiled(),
            webhook_ready: config.webhook_ready,
            allowed_host_count: config.allowed_hosts.len(),
            media_allowed_host_count: config.media_allowed_hosts.len(),
            allowed_group_count: config.allowed_groups.len(),
            missing_configuration: Vec::new(),
        }),
        Err(_) => preflight_report(PreflightReportState {
            config_valid: false,
            send_enabled,
            adapter_compiled,
            feishu_delivery_bridge_compiled: feishu_delivery_bridge_compiled(),
            webhook_ready: false,
            allowed_host_count: 0,
            media_allowed_host_count: 0,
            allowed_group_count: 0,
            missing_configuration: missing_qiwe_image_send_configuration(),
        }),
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report.success {
        Ok(())
    } else {
        bail!("QiWe image send adapter preflight configuration is invalid")
    }
}

pub fn run_staging_preflight(cli: &Cli) -> Result<()> {
    validate_contract()?;
    let adapter_compiled = qiwe_staging_adapter_compiled();
    let feishu_delivery_bridge_compiled = feishu_delivery_bridge_compiled();
    let send_enabled = env_flag("QINTOPIA_QIWE_IMAGE_SEND_ENABLED").unwrap_or(false);
    let owner_approval_valid =
        validate_staging_owner_approval(std::env::var(STAGING_APPROVAL_ENV).ok().as_deref())
            .is_ok();
    let database_boundary_valid = cli
        .database_url_required()
        .and_then(validate_staging_database_boundary)
        .is_ok();
    let (
        config_valid,
        webhook_ready,
        allowed_host_count,
        media_allowed_host_count,
        allowed_group_count,
    ) = match AdapterConfig::from_env().and_then(|config| {
        validate_feishu_delivery_config(cli.database_url_required()?)?;
        Ok(config)
    }) {
        Ok(config) => (
            true,
            config.webhook_ready,
            config.allowed_hosts.len(),
            config.media_allowed_hosts.len(),
            config.allowed_groups.len(),
        ),
        Err(_) => (false, false, 0, 0, 0),
    };
    let report = staging_preflight_report(StagingPreflightReportState {
        adapter_compiled,
        feishu_delivery_bridge_compiled,
        send_enabled,
        owner_approval_valid,
        config_valid,
        database_boundary_valid,
        webhook_ready,
        allowed_host_count,
        media_allowed_host_count,
        allowed_group_count,
        missing_configuration: missing_qiwe_image_staging_configuration(cli),
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report.success {
        Ok(())
    } else {
        bail!("QiWe image send staging preflight is not approved")
    }
}

pub async fn run_upload_worker(
    cli: &Cli,
    once: bool,
    work_item_id: Option<Uuid>,
    apply: bool,
    dry_run: bool,
) -> Result<()> {
    if !once {
        bail!("QiWe image-send worker currently supports --once only");
    }
    if apply && dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let apply_requested = apply && !dry_run;
    if !apply_requested {
        let policy = match SendBoundaryPolicy::from_env() {
            Ok(policy) => policy,
            Err(_) => {
                let report = worker_report(WorkerReportState {
                    success: false,
                    dry_run: true,
                    apply_requested: false,
                    phase: "upload",
                    action_status: "preview_policy_not_configured".to_string(),
                    work_item_id,
                    external_upload_requested: false,
                    callback_received: false,
                    external_send_executed: Some(false),
                });
                println!("{}", serde_json::to_string_pretty(&report)?);
                bail!("QiWe image-send preview policy is invalid");
            }
        };
        let database_url = cli.database_url_required()?;
        let pool = db::connect(database_url, cli.db_max_connections).await?;
        let preview = qiwe_image_send_state::preview_ready_work_item(
            &pool,
            work_item_id,
            &policy.allowed_groups,
            &policy.media_allowed_hosts,
        )
        .await?;
        let report = match preview {
            Some(preview) => worker_report(WorkerReportState {
                success: true,
                dry_run: true,
                apply_requested: false,
                phase: "upload",
                action_status: "image_upload_preview".to_string(),
                work_item_id: Some(preview.work_item_id),
                external_upload_requested: false,
                callback_received: false,
                external_send_executed: Some(false),
            }),
            None => worker_report(WorkerReportState {
                success: true,
                dry_run: true,
                apply_requested: false,
                phase: "upload",
                action_status: "no_claimable_send_request".to_string(),
                work_item_id: None,
                external_upload_requested: false,
                callback_received: false,
                external_send_executed: Some(false),
            }),
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    #[cfg(not(feature = "qiwe-staging-adapter"))]
    {
        let report = worker_report(WorkerReportState {
            success: false,
            dry_run: false,
            apply_requested: true,
            phase: "upload",
            action_status: "staging_adapter_not_compiled".to_string(),
            work_item_id,
            external_upload_requested: false,
            callback_received: false,
            external_send_executed: Some(false),
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
        bail!("QiWe staging adapter is not compiled into this binary");
    }

    #[cfg(feature = "qiwe-staging-adapter")]
    {
        if !env_flag("QINTOPIA_QIWE_IMAGE_SEND_ENABLED")? {
            let policy = match SendBoundaryPolicy::from_env() {
                Ok(policy) => policy,
                Err(_) => {
                    let report = worker_report(WorkerReportState {
                        success: false,
                        dry_run: false,
                        apply_requested: true,
                        phase: "upload",
                        action_status: "disabled_preview_policy_not_configured".to_string(),
                        work_item_id,
                        external_upload_requested: false,
                        callback_received: false,
                        external_send_executed: Some(false),
                    });
                    println!("{}", serde_json::to_string_pretty(&report)?);
                    bail!("QiWe image-send disabled preview policy is invalid");
                }
            };
            let database_url = cli.database_url_required()?;
            let pool = db::connect(database_url, cli.db_max_connections).await?;
            let preview = qiwe_image_send_state::preview_ready_work_item(
                &pool,
                work_item_id,
                &policy.allowed_groups,
                &policy.media_allowed_hosts,
            )
            .await?;
            let report = worker_report(WorkerReportState {
                success: true,
                dry_run: false,
                apply_requested: true,
                phase: "upload",
                action_status: "image_send_disabled".to_string(),
                work_item_id: preview.map(|item| item.work_item_id),
                external_upload_requested: false,
                callback_received: false,
                external_send_executed: Some(false),
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(());
        }
        run_enabled_upload_worker(cli, work_item_id).await
    }
}

#[cfg(feature = "qiwe-staging-adapter")]
async fn run_enabled_upload_worker(cli: &Cli, work_item_id: Option<Uuid>) -> Result<()> {
    let config = match staging_apply_config(cli) {
        Ok(config) => config,
        Err(_) => {
            let report = worker_report(WorkerReportState {
                success: false,
                dry_run: false,
                apply_requested: true,
                phase: "upload",
                action_status: "staging_boundary_not_approved".to_string(),
                work_item_id,
                external_upload_requested: false,
                callback_received: false,
                external_send_executed: Some(false),
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
            bail!("QiWe image-send staging boundary is not approved");
        }
    };
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let Some(claim) = qiwe_image_send_state::claim_ready_work_item(
        &pool,
        work_item_id,
        &config.allowed_groups,
        &config.media_allowed_hosts,
    )
    .await?
    else {
        let report = worker_report(WorkerReportState {
            success: true,
            dry_run: false,
            apply_requested: true,
            phase: "upload",
            action_status: "no_claimable_send_request".to_string(),
            work_item_id: None,
            external_upload_requested: false,
            callback_received: false,
            external_send_executed: Some(false),
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    };
    let claim_id = claim.work_item_id;
    let delivery_bytes = match prepare_feishu_delivery_bytes(&pool, &claim, database_url).await {
        Ok(bytes) => bytes,
        Err(_) => {
            qiwe_image_send_state::record_upload_failure(
                &pool,
                &claim,
                UploadFailureDisposition::Rejected,
            )
            .await?;
            let report = worker_report(WorkerReportState {
                success: false,
                dry_run: false,
                apply_requested: true,
                phase: "upload",
                action_status: "feishu_delivery_revalidation_failed".to_string(),
                work_item_id: Some(claim_id),
                external_upload_requested: false,
                callback_received: false,
                external_send_executed: Some(false),
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(());
        }
    };
    let worker_config = config.clone();
    let worker_claim = claim.clone();
    let upload = tokio::task::spawn_blocking(move || {
        request_claim_upload_with(
            &worker_config,
            &worker_claim,
            delivery_bytes.as_ref().map(|bytes| bytes.as_slice()),
            &HttpClient::production(),
        )
    })
    .await;
    let upload = match upload {
        Ok(upload) => upload,
        Err(_) => Err(UploadCallFailure::OutcomeUnknown),
    };
    match upload {
        Ok(request_id) => {
            if qiwe_image_send_state::record_upload_acceptance(&pool, &claim, &request_id)
                .await
                .is_err()
            {
                let terminalized = qiwe_image_send_state::record_upload_failure(
                    &pool,
                    &claim,
                    UploadFailureDisposition::OutcomeUnknown,
                )
                .await
                .is_ok();
                let report = worker_report(WorkerReportState {
                    success: false,
                    dry_run: false,
                    apply_requested: true,
                    phase: "upload",
                    action_status: if terminalized {
                        "upload_state_persistence_ambiguous"
                    } else {
                        "upload_state_persistence_failed"
                    }
                    .to_string(),
                    work_item_id: Some(claim_id),
                    external_upload_requested: true,
                    callback_received: false,
                    external_send_executed: Some(false),
                });
                println!("{}", serde_json::to_string_pretty(&report)?);
                bail!("QiWe upload acceptance could not be persisted");
            }
            let mut report = worker_report(WorkerReportState {
                success: true,
                dry_run: false,
                apply_requested: true,
                phase: "upload",
                action_status: "image_upload_accepted".to_string(),
                work_item_id: Some(claim_id),
                external_upload_requested: true,
                callback_received: false,
                external_send_executed: Some(false),
            });
            report.artifact_content_hash = Some(claim.artifact_content_hash.clone());
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        Err(failure) => {
            let disposition = match failure {
                UploadCallFailure::Rejected => UploadFailureDisposition::Rejected,
                UploadCallFailure::OutcomeUnknown => UploadFailureDisposition::OutcomeUnknown,
            };
            qiwe_image_send_state::record_upload_failure(&pool, &claim, disposition).await?;
            let report = worker_report(WorkerReportState {
                success: false,
                dry_run: false,
                apply_requested: true,
                phase: "upload",
                action_status: match failure {
                    UploadCallFailure::Rejected => "image_upload_rejected",
                    UploadCallFailure::OutcomeUnknown => "image_upload_outcome_unknown",
                }
                .to_string(),
                work_item_id: Some(claim_id),
                external_upload_requested: true,
                callback_received: false,
                external_send_executed: Some(false),
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
    }
}

pub async fn run_callback_processor(cli: &Cli, apply: bool, dry_run: bool) -> Result<()> {
    if apply && dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let apply_requested = apply && !dry_run;
    #[cfg(not(feature = "qiwe-staging-adapter"))]
    if apply_requested {
        let _ = cli;
        let report = worker_report(WorkerReportState {
            success: false,
            dry_run: false,
            apply_requested: true,
            phase: "callback",
            action_status: "staging_adapter_not_compiled".to_string(),
            work_item_id: None,
            external_upload_requested: false,
            callback_received: false,
            external_send_executed: Some(false),
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
        bail!("QiWe staging adapter is not compiled into this binary");
    }
    #[cfg(feature = "qiwe-staging-adapter")]
    if apply_requested && !env_flag("QINTOPIA_QIWE_IMAGE_SEND_ENABLED")? {
        let report = worker_report(WorkerReportState {
            success: true,
            dry_run: false,
            apply_requested: true,
            phase: "callback",
            action_status: "image_send_disabled".to_string(),
            work_item_id: None,
            external_upload_requested: false,
            callback_received: false,
            external_send_executed: Some(false),
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    #[cfg(feature = "qiwe-staging-adapter")]
    let config = if apply_requested {
        match staging_apply_config(cli) {
            Ok(config) => Some(config),
            Err(_) => {
                let report = worker_report(WorkerReportState {
                    success: false,
                    dry_run: false,
                    apply_requested: true,
                    phase: "callback",
                    action_status: "staging_boundary_not_approved".to_string(),
                    work_item_id: None,
                    external_upload_requested: false,
                    callback_received: false,
                    external_send_executed: Some(false),
                });
                println!("{}", serde_json::to_string_pretty(&report)?);
                bail!("QiWe image-send callback staging boundary is not approved");
            }
        }
    } else {
        None
    };
    let callback = read_callback_stdin()?;
    let parsed = parse_single_async_upload_callback(&callback)?;
    if !apply_requested {
        let report = callback_worker_report(
            WorkerReportState {
                success: true,
                dry_run: true,
                apply_requested: false,
                phase: "callback",
                action_status: "callback_preview".to_string(),
                work_item_id: None,
                external_upload_requested: false,
                callback_received: true,
                external_send_executed: None,
            },
            parsed.credential_shape,
        );
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    #[cfg(feature = "qiwe-staging-adapter")]
    {
        run_enabled_callback_processor(
            cli,
            config.context("QiWe callback apply configuration is missing")?,
            &parsed,
            &callback,
        )
        .await
    }

    #[cfg(not(feature = "qiwe-staging-adapter"))]
    {
        drop(parsed);
        bail!("QiWe staging adapter is not compiled into this binary")
    }
}

#[cfg(feature = "qiwe-staging-adapter")]
async fn run_enabled_callback_processor(
    cli: &Cli,
    config: AdapterConfig,
    parsed: &ParsedCallback,
    callback: &[u8],
) -> Result<()> {
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let callback_file = QiweCallbackFileIdentity {
        filename: &parsed.credentials.filename,
        file_md5: &parsed.credentials.file_md5,
        file_size: parsed.credentials.file_size,
    };
    let outcome = qiwe_image_send_state::claim_callback_for_send(
        &pool,
        &parsed.request_id,
        callback,
        &callback_file,
    )
    .await?;
    let send_claim = match outcome {
        CallbackClaimOutcome::Duplicate { status } => {
            let report = callback_worker_report(
                WorkerReportState {
                    success: true,
                    dry_run: false,
                    apply_requested: true,
                    phase: "callback",
                    action_status: format!("callback_duplicate_{status}"),
                    work_item_id: None,
                    external_upload_requested: false,
                    callback_received: true,
                    external_send_executed: None,
                },
                parsed.credential_shape,
            );
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(());
        }
        CallbackClaimOutcome::Expired => {
            let report = callback_worker_report(
                WorkerReportState {
                    success: true,
                    dry_run: false,
                    apply_requested: true,
                    phase: "callback",
                    action_status: "callback_expired".to_string(),
                    work_item_id: None,
                    external_upload_requested: false,
                    callback_received: true,
                    external_send_executed: Some(false),
                },
                parsed.credential_shape,
            );
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(());
        }
        CallbackClaimOutcome::Ready(claim) => claim,
    };
    let work_item_id = send_claim.work_item_id;
    let send_body = match build_send_image_request(
        &config.guid,
        &send_claim.target_group_id,
        &parsed.credentials,
        &config.allowed_groups,
    ) {
        Ok(body) => Zeroizing::new(body),
        Err(_) => {
            qiwe_image_send_state::record_send_failure(
                &pool,
                &send_claim,
                SendFailureDisposition::Rejected,
            )
            .await?;
            let mut report = callback_worker_report(
                WorkerReportState {
                    success: false,
                    dry_run: false,
                    apply_requested: true,
                    phase: "callback",
                    action_status: "send_request_rejected".to_string(),
                    work_item_id: Some(work_item_id),
                    external_upload_requested: false,
                    callback_received: true,
                    external_send_executed: Some(false),
                },
                parsed.credential_shape,
            );
            report.artifact_content_hash = Some(send_claim.artifact_content_hash.clone());
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(());
        }
    };
    let worker_config = config.clone();
    let send_result = tokio::task::spawn_blocking(move || {
        request_send_image_with(&worker_config, &send_body, &HttpClient::production())
    })
    .await;
    let send_result = match send_result {
        Ok(result) => result,
        Err(_) => Err(SendCallFailure::Ambiguous),
    };
    match send_result {
        Ok(receipt) => {
            qiwe_image_send_state::record_send_success(&pool, &send_claim, &receipt).await?;
            let mut report = callback_worker_report(
                WorkerReportState {
                    success: true,
                    dry_run: false,
                    apply_requested: true,
                    phase: "callback",
                    action_status: "image_send_completed".to_string(),
                    work_item_id: Some(work_item_id),
                    external_upload_requested: false,
                    callback_received: true,
                    external_send_executed: Some(true),
                },
                parsed.credential_shape,
            );
            report.artifact_content_hash = Some(send_claim.artifact_content_hash.clone());
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Err(failure) => {
            let disposition = match failure {
                SendCallFailure::NotSent => SendFailureDisposition::Rejected,
                SendCallFailure::Ambiguous => SendFailureDisposition::Ambiguous,
            };
            qiwe_image_send_state::record_send_failure(&pool, &send_claim, disposition).await?;
            let report = callback_worker_report(
                WorkerReportState {
                    success: false,
                    dry_run: false,
                    apply_requested: true,
                    phase: "callback",
                    action_status: match failure {
                        SendCallFailure::NotSent => "image_send_not_sent",
                        SendCallFailure::Ambiguous => "image_send_ambiguous",
                    }
                    .to_string(),
                    work_item_id: Some(work_item_id),
                    external_upload_requested: false,
                    callback_received: true,
                    external_send_executed: match failure {
                        SendCallFailure::NotSent => Some(false),
                        SendCallFailure::Ambiguous => None,
                    },
                },
                parsed.credential_shape,
            );
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }
    Ok(())
}

#[cfg(feature = "qiwe-staging-adapter")]
async fn prepare_feishu_delivery_bytes(
    pool: &sqlx::PgPool,
    claim: &QiweUploadClaim,
    database_url: &str,
) -> Result<Option<Zeroizing<Vec<u8>>>> {
    if !claim
        .artifact_uri
        .starts_with(FEISHU_PRIMARY_STORAGE_URI_PREFIX)
    {
        return Ok(None);
    }

    #[cfg(not(feature = "huabaosi-staging-adapter"))]
    {
        let _ = (pool, database_url);
        bail!("Feishu delivery bridge requires the combined staging feature build");
    }

    #[cfg(feature = "huabaosi-staging-adapter")]
    {
        let artifact =
            crate::huabaosi_feishu_artifact_mirror::revalidate_primary_storage_for_delivery(
                pool,
                claim.generated_image_artifact_id,
                database_url,
            )
            .await?;
        validate_feishu_delivery_artifact(claim, &artifact)?;
        Ok(Some(artifact.bytes))
    }
}

#[cfg(all(feature = "huabaosi-staging-adapter", feature = "qiwe-staging-adapter"))]
fn validate_feishu_delivery_artifact(
    claim: &QiweUploadClaim,
    artifact: &crate::huabaosi_feishu_artifact_mirror::FeishuPrimaryStorageDeliveryArtifact,
) -> Result<()> {
    if artifact.artifact_id != claim.generated_image_artifact_id
        || artifact.artifact_uri != claim.artifact_uri
        || artifact.content_hash != claim.artifact_content_hash
        || artifact.file_md5 != claim.artifact_file_md5
        || artifact.byte_size != claim.artifact_byte_size
    {
        bail!("Feishu delivery revalidation does not match the locked QiWe claim");
    }
    Ok(())
}

#[cfg(any(test, feature = "qiwe-staging-adapter"))]
fn request_claim_upload_with(
    config: &AdapterConfig,
    claim: &QiweUploadClaim,
    feishu_bytes: Option<&[u8]>,
    client: &HttpClient,
) -> std::result::Result<Zeroizing<String>, UploadCallFailure> {
    if claim
        .artifact_uri
        .starts_with(FEISHU_PRIMARY_STORAGE_URI_PREFIX)
    {
        let bytes = feishu_bytes.ok_or(UploadCallFailure::Rejected)?;
        request_feishu_bridge_upload_with(config, claim, bytes, client)
    } else if feishu_bytes.is_some() {
        Err(UploadCallFailure::Rejected)
    } else {
        request_async_upload_with(config, claim, client)
    }
}

#[cfg(any(test, feature = "qiwe-staging-adapter"))]
fn request_async_upload_with(
    config: &AdapterConfig,
    claim: &QiweUploadClaim,
    client: &HttpClient,
) -> std::result::Result<Zeroizing<String>, UploadCallFailure> {
    let artifact_url =
        strict_media_url(&claim.artifact_uri).map_err(|_| UploadCallFailure::Rejected)?;
    request_async_upload_url_with(
        config,
        claim,
        &artifact_url,
        &config.media_allowed_hosts,
        client,
    )
}

#[cfg(any(test, feature = "qiwe-staging-adapter"))]
fn request_async_upload_url_with(
    config: &AdapterConfig,
    claim: &QiweUploadClaim,
    file_url: &Url,
    allowed_hosts: &BTreeSet<String>,
    client: &HttpClient,
) -> std::result::Result<Zeroizing<String>, UploadCallFailure> {
    let host = file_url
        .host_str()
        .ok_or(UploadCallFailure::Rejected)?
        .to_ascii_lowercase();
    if !allowed_hosts.contains(&host) {
        return Err(UploadCallFailure::Rejected);
    }
    let body = Zeroizing::new(
        build_async_upload_request_from_validated_url(
            &config.guid,
            &claim.filename,
            file_url,
            "image/jpeg",
        )
        .map_err(|_| UploadCallFailure::Rejected)?,
    );
    let response = client
        .request(
            "POST",
            &config.api_url,
            &[
                ("Content-Type", "application/json".to_string()),
                ("Accept", "application/json".to_string()),
                ("x-qiwei-token", config.token.clone()),
            ],
            &body,
            MAX_JSON_RESPONSE_BYTES,
        )
        .map_err(|error| {
            if error.request_may_have_been_sent() {
                UploadCallFailure::OutcomeUnknown
            } else {
                UploadCallFailure::Rejected
            }
        })?;
    if !(200..300).contains(&response.status) {
        return Err(UploadCallFailure::Rejected);
    }
    parse_upload_acceptance_for_call(&response)
}

#[cfg(any(test, feature = "qiwe-staging-adapter"))]
fn request_feishu_bridge_upload_with(
    config: &AdapterConfig,
    claim: &QiweUploadClaim,
    bytes: &[u8],
    client: &HttpClient,
) -> std::result::Result<Zeroizing<String>, UploadCallFailure> {
    validate_claim_bytes(claim, bytes).map_err(|_| UploadCallFailure::Rejected)?;
    let cloud_url = request_temporary_storage_upload_with(config, claim, bytes, client)?;
    readback_temporary_storage_with(config, claim, &cloud_url, client)?;
    let cloud_url = strict_temporary_storage_url(
        &cloud_url,
        &config.media_allowed_hosts,
        client.allows_insecure_http(),
    )
    .map_err(|_| UploadCallFailure::OutcomeUnknown)?;
    cloud_url
        .with_url(|url| {
            request_async_upload_url_with(config, claim, url, &config.media_allowed_hosts, client)
        })
        .map_err(|_| UploadCallFailure::OutcomeUnknown)?
}

#[cfg(any(test, feature = "qiwe-staging-adapter"))]
fn request_temporary_storage_upload_with(
    config: &AdapterConfig,
    claim: &QiweUploadClaim,
    bytes: &[u8],
    client: &HttpClient,
) -> std::result::Result<Zeroizing<String>, UploadCallFailure> {
    let file_api_url =
        file_api_url_from_api_url(&config.api_url).map_err(|_| UploadCallFailure::Rejected)?;
    let boundary = format!("qintopia-{}", Uuid::new_v4().simple());
    let body = build_temporary_storage_upload_body(&boundary, &config.guid, &claim.filename, bytes)
        .map_err(|_| UploadCallFailure::Rejected)?;
    let response = client
        .request(
            "POST",
            &file_api_url,
            &[
                (
                    "Content-Type",
                    format!("multipart/form-data; boundary={boundary}"),
                ),
                ("Accept", "application/json".to_string()),
                ("x-qiwei-token", config.token.clone()),
            ],
            &body,
            MAX_JSON_RESPONSE_BYTES,
        )
        .map_err(|error| {
            if error.request_may_have_been_sent() {
                UploadCallFailure::OutcomeUnknown
            } else {
                UploadCallFailure::Rejected
            }
        })?;
    if !(200..300).contains(&response.status) {
        return Err(UploadCallFailure::OutcomeUnknown);
    }
    parse_temporary_storage_acceptance_for_call(
        &response,
        &config.media_allowed_hosts,
        client.allows_insecure_http(),
    )
}

#[cfg(any(test, feature = "qiwe-staging-adapter"))]
fn parse_temporary_storage_acceptance_for_call(
    response: &HttpResponse,
    allowed_hosts: &BTreeSet<String>,
    allow_insecure_http: bool,
) -> std::result::Result<Zeroizing<String>, UploadCallFailure> {
    validate_json_body_size(&response.body).map_err(|_| UploadCallFailure::OutcomeUnknown)?;
    let mut response: ApiResponse<TemporaryStorageAcceptedData> =
        serde_json::from_slice(&response.body).map_err(|_| UploadCallFailure::OutcomeUnknown)?;
    let cloud_url_value = Zeroizing::new(std::mem::take(&mut response.data.cloud_url));
    if response.code != 0 {
        return Err(UploadCallFailure::OutcomeUnknown);
    }
    let cloud_url =
        strict_temporary_storage_url(&cloud_url_value, allowed_hosts, allow_insecure_http)
            .map_err(|_| UploadCallFailure::OutcomeUnknown)?;
    Ok(Zeroizing::new(cloud_url.as_str().to_string()))
}

#[cfg(any(test, feature = "qiwe-staging-adapter"))]
fn readback_temporary_storage_with(
    config: &AdapterConfig,
    claim: &QiweUploadClaim,
    cloud_url: &str,
    client: &HttpClient,
) -> std::result::Result<(), UploadCallFailure> {
    let cloud_url = strict_temporary_storage_url(
        cloud_url,
        &config.media_allowed_hosts,
        client.allows_insecure_http(),
    )
    .map_err(|_| UploadCallFailure::OutcomeUnknown)?;
    let max_bytes =
        usize::try_from(claim.artifact_byte_size).map_err(|_| UploadCallFailure::OutcomeUnknown)?;
    let response = cloud_url
        .with_url(|url| {
            client.request(
                "GET",
                url,
                &[("Accept", "image/jpeg".to_string())],
                &[],
                max_bytes,
            )
        })
        .map_err(|_| UploadCallFailure::OutcomeUnknown)?
        .map_err(|_| UploadCallFailure::OutcomeUnknown)?;
    if !(200..300).contains(&response.status) {
        return Err(UploadCallFailure::OutcomeUnknown);
    }
    validate_claim_bytes(claim, &response.body).map_err(|_| UploadCallFailure::OutcomeUnknown)
}

#[cfg(any(test, feature = "qiwe-staging-adapter"))]
fn parse_upload_acceptance_for_call(
    response: &HttpResponse,
) -> std::result::Result<Zeroizing<String>, UploadCallFailure> {
    validate_json_body_size(&response.body).map_err(|_| UploadCallFailure::OutcomeUnknown)?;
    let response: ApiResponse<UploadAcceptedData> =
        serde_json::from_slice(&response.body).map_err(|_| UploadCallFailure::OutcomeUnknown)?;
    if response.code != 0 {
        return Err(UploadCallFailure::Rejected);
    }
    validate_plain_value(&response.data.request_id, "QiWe upload request id")
        .map_err(|_| UploadCallFailure::OutcomeUnknown)?;
    Ok(Zeroizing::new(response.data.request_id))
}

#[cfg(any(test, feature = "qiwe-staging-adapter"))]
fn request_send_image_with(
    config: &AdapterConfig,
    body: &[u8],
    client: &HttpClient,
) -> std::result::Result<QiweSendReceipt, SendCallFailure> {
    let response = client
        .request(
            "POST",
            &config.api_url,
            &[
                ("Content-Type", "application/json".to_string()),
                ("Accept", "application/json".to_string()),
                ("x-qiwei-token", config.token.clone()),
            ],
            body,
            MAX_JSON_RESPONSE_BYTES,
        )
        .map_err(|error| {
            if error.request_may_have_been_sent() {
                SendCallFailure::Ambiguous
            } else {
                SendCallFailure::NotSent
            }
        })?;
    if !(200..300).contains(&response.status) {
        return Err(SendCallFailure::Ambiguous);
    }
    parse_send_response_for_call(&response)
}

#[cfg(any(test, feature = "qiwe-staging-adapter"))]
fn parse_send_response_for_call(
    response: &HttpResponse,
) -> std::result::Result<QiweSendReceipt, SendCallFailure> {
    validate_json_body_size(&response.body).map_err(|_| SendCallFailure::Ambiguous)?;
    let response: ApiResponse<SendImageData> =
        serde_json::from_slice(&response.body).map_err(|_| SendCallFailure::Ambiguous)?;
    if response.code != 0 || response.data.is_send_success != SEND_SUCCESS_VALUE {
        return Err(SendCallFailure::Ambiguous);
    }
    validate_plain_value(
        &response.data.msg_unique_identifier,
        "QiWe message identifier",
    )
    .map_err(|_| SendCallFailure::Ambiguous)?;
    Ok(QiweSendReceipt {
        is_send_success: response.data.is_send_success,
        message_identifier: response.data.msg_unique_identifier,
        sequence: response.data.seq,
        timestamp: response.data.timestamp,
    })
}

fn read_callback_stdin() -> Result<Zeroizing<Vec<u8>>> {
    let mut input = Vec::new();
    io::stdin()
        .lock()
        .take((MAX_CALLBACK_INPUT_BYTES + 1) as u64)
        .read_to_end(&mut input)
        .context("read bounded QiWe callback input")?;
    if input.is_empty() || input.len() > MAX_CALLBACK_INPUT_BYTES {
        bail!("QiWe callback input size is invalid");
    }
    Ok(Zeroizing::new(input))
}

fn parse_single_async_upload_callback(body: &[u8]) -> Result<ParsedCallback> {
    validate_json_body_size(body)?;
    let envelope: CallbackEnvelope = serde_json::from_slice(body)
        .map_err(|_| anyhow!("QiWe async upload callback shape is invalid"))?;
    if envelope.code != 0 {
        bail!("QiWe async upload callback reported failure");
    }
    let mut events = envelope
        .data
        .into_iter()
        .filter(|event| event.cmd == ASYNC_EVENT_COMMAND)
        .collect::<Vec<_>>();
    if events.len() != 1 {
        bail!("QiWe callback input must contain exactly one async upload event");
    }
    let event = events.pop().expect("single callback event exists");
    validate_plain_value(&event.request_id, "QiWe upload request id")?;
    let msg_data = event
        .msg_data
        .context("QiWe async upload callback is missing msgData")?;
    let credential_shape = classify_callback_credential_shape(&msg_data)?;
    let credentials: QiweImageCredentials = serde_json::from_value(msg_data)
        .map_err(|_| anyhow!("QiWe async upload callback credentials are invalid"))?;
    credentials.validate()?;
    #[cfg(not(any(test, feature = "qiwe-staging-adapter")))]
    drop(credentials);
    Ok(ParsedCallback {
        request_id: event.request_id,
        credential_shape,
        #[cfg(any(test, feature = "qiwe-staging-adapter"))]
        credentials,
    })
}

fn classify_callback_credential_shape(msg_data: &Value) -> Result<CallbackCredentialShape> {
    let fields = msg_data
        .as_object()
        .context("QiWe async upload callback msgData must be an object")?;
    let aes_key_is_canonical = fields.contains_key("fileAesKey");
    let aes_key_is_alias = fields.contains_key("fileAeskey");
    if aes_key_is_canonical == aes_key_is_alias {
        bail!("QiWe callback must contain exactly one file AES key field spelling");
    }
    let filename_is_canonical = fields.contains_key("filename");
    let filename_is_alias = fields.contains_key("fileName");
    if filename_is_canonical == filename_is_alias {
        bail!("QiWe callback must contain exactly one filename field spelling");
    }
    if !["fileId", "fileMd5", "fileSize"]
        .into_iter()
        .all(|field| fields.contains_key(field))
    {
        bail!("QiWe callback is missing required credential fields");
    }

    // Field names are staging evidence; values remain memory-only, so aliases collapse into fixed public schema IDs.
    let schema_id = match (aes_key_is_canonical, filename_is_canonical) {
        (true, true) => "fileAesKey+fileId+fileMd5+fileSize+filename",
        (false, true) => "fileAeskey+fileId+fileMd5+fileSize+filename",
        (true, false) => "fileAesKey+fileId+fileMd5+fileSize+fileName",
        (false, false) => "fileAeskey+fileId+fileMd5+fileSize+fileName",
    };
    let reviewed_fields = [
        "fileAesKey",
        "fileAeskey",
        "fileId",
        "fileMd5",
        "fileSize",
        "filename",
        "fileName",
    ];
    let additional_field_count = fields
        .keys()
        .filter(|field| !reviewed_fields.contains(&field.as_str()))
        .count();

    Ok(CallbackCredentialShape {
        schema_id,
        additional_field_count,
    })
}

fn worker_report(state: WorkerReportState) -> QiweImageSendWorkerReport {
    QiweImageSendWorkerReport {
        success: state.success,
        dry_run: state.dry_run,
        apply_requested: state.apply_requested,
        worker: WORKER_ID,
        phase: state.phase,
        action_status: state.action_status,
        work_item_id: state.work_item_id,
        artifact_content_hash: None,
        external_upload_requested: state.external_upload_requested,
        callback_received: state.callback_received,
        callback_credential_schema: None,
        callback_additional_field_count: None,
        external_send_executed: state.external_send_executed,
        safe_for_chat: false,
        limitations: vec![
            "the upload worker and callback processor each handle one state transition and are not production scheduled".to_string(),
            "callback credentials remain memory-only and cannot be retried after the sending gate".to_string(),
        ],
        guardrails: vec![
            "Postgres remains the system source of truth".to_string(),
            "default production builds exclude the staging-only live QiWe adapter".to_string(),
            "Feishu bytes and QiWe temporary-storage URLs remain memory-only and are zeroized after same-byte readback".to_string(),
            "tokens, device ids, group ids, media URLs, request ids, callback credentials, response bodies, and message ids are excluded from reports".to_string(),
            "no Feishu writeback or unrelated external adapter is called".to_string(),
        ],
    }
}

fn callback_worker_report(
    state: WorkerReportState,
    credential_shape: CallbackCredentialShape,
) -> QiweImageSendWorkerReport {
    let mut report = worker_report(state);
    report.callback_credential_schema = Some(credential_shape.schema_id);
    report.callback_additional_field_count = Some(credential_shape.additional_field_count);
    report
}

fn validate_contract() -> Result<()> {
    let media_allowed_hosts = BTreeSet::from(["media.example.test".to_string()]);
    let allowed_group_ids = BTreeSet::from(["contract-group".to_string()]);
    build_async_upload_request(
        "contract-device",
        "contract-image.jpg",
        "https://media.example.test/contract-image.jpg",
        "image/jpeg",
        &media_allowed_hosts,
    )?;
    let request_id = parse_async_upload_acceptance(
        br#"{"code":0,"data":{"requestId":"contract-upload"},"msg":"success"}"#,
    )?;
    let credentials = parse_async_upload_callback(
        br#"{
          "code":0,
          "data":[{
            "requestId":"contract-upload",
            "cmd":20000,
            "msgData":{
              "fileAesKey":"contract-aes-key",
              "fileId":"contract-file-id",
              "fileMd5":"98e7c2acf4391f8b4a2bbd39e364c5e3",
              "fileSize":48300,
              "filename":"contract-image.jpg"
            }
          }]
        }"#,
        &request_id,
    )?;
    build_send_image_request(
        "contract-device",
        "contract-group",
        &credentials,
        &allowed_group_ids,
    )?;
    let receipt = parse_send_image_response(
        br#"{
          "code":0,
          "data":{
            "isSendSuccess":1,
            "msgServerId":1,
            "msgType":14,
            "msgUniqueIdentifier":"contract-message",
            "seq":2,
            "timestamp":3
          },
          "msg":"success"
        }"#,
    )?;
    if receipt.message_identifier != "contract-message"
        || receipt.is_send_success != SEND_SUCCESS_VALUE
        || receipt.sequence != 2
        || receipt.timestamp != 3
    {
        bail!("QiWe image-send contract self-check failed");
    }
    Ok(())
}

fn preflight_report(state: PreflightReportState) -> QiweImageSendPreflightReport {
    let success = state.config_valid && !state.send_enabled && !state.adapter_compiled;
    QiweImageSendPreflightReport {
        success,
        worker: WORKER_ID,
        action_status: if !state.config_valid {
            "adapter_not_configured"
        } else if state.adapter_compiled {
            "staging_adapter_compiled_requires_owner_review"
        } else if state.send_enabled {
            "adapter_enablement_not_approved"
        } else {
            "adapter_contract_ready"
        },
        adapter_compiled: state.adapter_compiled,
        feishu_delivery_bridge_compiled: state.feishu_delivery_bridge_compiled,
        send_enabled: state.send_enabled,
        config_valid: state.config_valid,
        webhook_ready: state.webhook_ready,
        allowed_host_count: state.allowed_host_count,
        media_allowed_host_count: state.media_allowed_host_count,
        allowed_group_count: state.allowed_group_count,
        missing_configuration: state.missing_configuration,
        protocol: "qiwe_async_url_upload_then_send_image",
        safe_for_chat: false,
        limitations: vec![
            "this preflight validates local configuration only; it does not contact QiWe, upload media, or send a message".to_string(),
            "the official async upload callback must provide complete file credentials before a send request can be built".to_string(),
            "the generated-image contract requires the deterministic final JPEG; owner-approved staging must still verify isolated media upload and same-byte readback".to_string(),
        ],
        guardrails: vec![
            "production artifacts use default Cargo features and cannot compile the staging-only live adapter".to_string(),
            "a staging build still requires explicit enablement, owner approval, and exact endpoint and group allowlists".to_string(),
            "tokens, device ids, group ids, media URLs, file credentials, and message identifiers are not emitted".to_string(),
            "no timer, production runtime configuration, Feishu writeback, or external send is installed by this contract".to_string(),
        ],
    }
}

fn staging_preflight_report(
    state: StagingPreflightReportState,
) -> QiweImageSendStagingPreflightReport {
    let success = state.adapter_compiled
        && state.feishu_delivery_bridge_compiled
        && state.send_enabled
        && state.owner_approval_valid
        && state.config_valid
        && state.database_boundary_valid
        && state.webhook_ready;
    QiweImageSendStagingPreflightReport {
        success,
        worker: WORKER_ID,
        action_status: if !state.adapter_compiled {
            "staging_adapter_not_compiled"
        } else if !state.feishu_delivery_bridge_compiled {
            "feishu_delivery_bridge_not_compiled"
        } else if !state.send_enabled {
            "staging_send_not_enabled"
        } else if !state.owner_approval_valid {
            "staging_owner_approval_required"
        } else if !state.database_boundary_valid {
            "staging_database_not_approved"
        } else if !state.config_valid || !state.webhook_ready {
            "adapter_not_configured"
        } else {
            "staging_adapter_ready"
        },
        adapter_compiled: state.adapter_compiled,
        feishu_delivery_bridge_compiled: state.feishu_delivery_bridge_compiled,
        send_enabled: state.send_enabled,
        owner_approval_valid: state.owner_approval_valid,
        config_valid: state.config_valid,
        database_boundary_valid: state.database_boundary_valid,
        webhook_ready: state.webhook_ready,
        allowed_host_count: state.allowed_host_count,
        media_allowed_host_count: state.media_allowed_host_count,
        allowed_group_count: state.allowed_group_count,
        missing_configuration: state.missing_configuration,
        protocol: "qiwe_feishu_temp_storage_then_async_upload_then_send_image_staging_v1",
        safe_for_chat: false,
        limitations: vec![
            "staging preflight validates local configuration only; it does not connect to Postgres, read callback stdin, upload media, or send a message".to_string(),
            "the callback phase must receive one owner-approved callback directly from bounded stdin".to_string(),
        ],
        guardrails: vec![
            "staging apply requires the exact owner phrase and expected database URL hash before Postgres, callback stdin, or network access".to_string(),
            "API hosts, media hosts, and target group ids use exact reviewed allowlists".to_string(),
            "Feishu primary-storage delivery requires the combined Huabaosi and QiWe staging feature build and same-byte temporary-storage readback".to_string(),
            "production artifacts exclude the staging adapter and no listener, service, or timer is installed".to_string(),
        ],
    }
}

const fn qiwe_staging_adapter_compiled() -> bool {
    cfg!(feature = "qiwe-staging-adapter")
}

const fn feishu_delivery_bridge_compiled() -> bool {
    cfg!(all(
        feature = "huabaosi-staging-adapter",
        feature = "qiwe-staging-adapter"
    ))
}

impl AdapterConfig {
    fn from_env() -> Result<Self> {
        let api_url = strict_api_url(&required_env("QIWE_API_URL")?)?;
        let token = Zeroizing::new(required_env("QIWE_TOKEN")?);
        let guid = Zeroizing::new(required_env("QIWE_GUID")?);
        validate_header_value(&token)?;
        validate_header_value(&guid)?;

        let allowed_hosts = parse_csv_set(&required_env("QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS")?);
        let api_host = api_url
            .host_str()
            .context("QiWe API URL host is missing")?
            .to_ascii_lowercase();
        if !allowed_hosts.contains(&api_host) {
            bail!("QiWe API host is not allowlisted");
        }
        let boundary_policy = SendBoundaryPolicy::from_env()?;
        let webhook_ready = env_flag("QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY")?;
        if !webhook_ready {
            bail!("QiWe async upload webhook must be reviewed and ready");
        }

        // Production still validates the URL and allowlist although it omits the live client.
        #[cfg(not(any(test, feature = "qiwe-staging-adapter")))]
        drop(api_url);

        Ok(Self {
            #[cfg(any(test, feature = "qiwe-staging-adapter"))]
            api_url,
            #[cfg(any(test, feature = "qiwe-staging-adapter"))]
            token: token.to_string(),
            #[cfg(any(test, feature = "qiwe-staging-adapter"))]
            guid: guid.to_string(),
            allowed_hosts,
            media_allowed_hosts: boundary_policy.media_allowed_hosts,
            allowed_groups: boundary_policy.allowed_groups,
            webhook_ready,
        })
    }
}

#[cfg(any(test, feature = "qiwe-staging-adapter"))]
fn build_temporary_storage_upload_body(
    boundary: &str,
    guid: &str,
    filename: &str,
    bytes: &[u8],
) -> Result<Zeroizing<Vec<u8>>> {
    if boundary.is_empty()
        || boundary.len() > 70
        || !boundary
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        bail!("QiWe temporary-storage multipart boundary is invalid");
    }
    validate_plain_value(guid, "QiWe device id")?;
    validate_jpeg_filename(filename)?;
    if !filename
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        bail!("QiWe temporary-storage filename is not multipart-safe");
    }
    if bytes.is_empty() {
        bail!("QiWe temporary-storage JPEG is empty");
    }
    if bytes
        .windows(boundary.len())
        .any(|window| window == boundary.as_bytes())
    {
        bail!("QiWe temporary-storage multipart boundary collides with JPEG bytes");
    }

    let mut body = Zeroizing::new(Vec::with_capacity(bytes.len() + 1024));
    append_multipart_text(
        &mut body,
        boundary,
        "method",
        TEMPORARY_STORAGE_UPLOAD_METHOD,
    );
    append_multipart_text(&mut body, boundary, "guid", guid);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: image/jpeg\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Ok(body)
}

#[cfg(any(test, feature = "qiwe-staging-adapter"))]
fn append_multipart_text(body: &mut Vec<u8>, boundary: &str, name: &str, value: &str) {
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
        )
        .as_bytes(),
    );
}

#[cfg(any(test, feature = "qiwe-staging-adapter"))]
fn file_api_url_from_api_url(api_url: &Url) -> Result<Url> {
    if api_url.path() != "/qiwe/api/qw/doApi"
        || api_url.query().is_some()
        || api_url.fragment().is_some()
    {
        bail!("QiWe API URL does not match the reviewed doApi endpoint");
    }
    let mut file_api_url = api_url.clone();
    file_api_url.set_path(FILE_API_PATH);
    Ok(file_api_url)
}

#[cfg(any(test, feature = "qiwe-staging-adapter"))]
fn strict_temporary_storage_url(
    value: &str,
    allowed_hosts: &BTreeSet<String>,
    allow_insecure_http: bool,
) -> Result<SensitiveUrl> {
    if value.is_empty() || value.len() > 4096 {
        bail!("QiWe temporary-storage URL length is invalid");
    }
    url_policy::reject_path_separator_ambiguity(value, "QiWe temporary-storage URL")?;
    let url = Url::parse(value).context("parse QiWe temporary-storage URL")?;
    let scheme_allowed = url.scheme() == "https" || (allow_insecure_http && url.scheme() == "http");
    if !scheme_allowed
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("QiWe temporary-storage URL is outside the reviewed boundary");
    }
    let host = url
        .host_str()
        .context("QiWe temporary-storage URL host is missing")?
        .to_ascii_lowercase();
    if !allowed_hosts.contains(&host) {
        bail!("QiWe temporary-storage URL host is not allowlisted");
    }
    Ok(SensitiveUrl::new(Zeroizing::new(value.to_string())))
}

#[cfg(any(test, feature = "qiwe-staging-adapter"))]
fn validate_claim_bytes(claim: &QiweUploadClaim, bytes: &[u8]) -> Result<()> {
    if u64::try_from(bytes.len()).ok() != Some(claim.artifact_byte_size)
        || format!("sha256:{:x}", Sha256::digest(bytes)) != claim.artifact_content_hash
        || format!("{:x}", Md5::digest(bytes)) != claim.artifact_file_md5
    {
        bail!("QiWe delivery bytes do not match the approved generated image");
    }
    Ok(())
}

pub fn build_async_upload_request(
    guid: &str,
    filename: &str,
    artifact_uri: &str,
    mime_type: &str,
    media_allowed_hosts: &BTreeSet<String>,
) -> Result<Vec<u8>> {
    validate_plain_value(guid, "QiWe device id")?;
    validate_jpeg_filename(filename)?;
    if mime_type != "image/jpeg" {
        bail!("QiWe image upload requires the documented JPEG MIME type");
    }
    let artifact_uri = strict_media_url(artifact_uri)?;
    let artifact_host = artifact_uri
        .host_str()
        .context("approved generated-image URI host is missing")?
        .to_ascii_lowercase();
    if !media_allowed_hosts.contains(&artifact_host) {
        bail!("approved generated-image URI host is not allowlisted");
    }
    build_async_upload_request_from_validated_url(guid, filename, &artifact_uri, mime_type)
}

fn build_async_upload_request_from_validated_url(
    guid: &str,
    filename: &str,
    artifact_uri: &Url,
    mime_type: &str,
) -> Result<Vec<u8>> {
    validate_plain_value(guid, "QiWe device id")?;
    validate_jpeg_filename(filename)?;
    if mime_type != "image/jpeg" {
        bail!("QiWe image upload requires the documented JPEG MIME type");
    }
    serde_json::to_vec(&ApiRequest {
        method: ASYNC_UPLOAD_METHOD,
        params: AsyncUploadParams {
            guid,
            filename,
            file_url: artifact_uri.as_str(),
            file_type: IMAGE_FILE_TYPE,
        },
    })
    .context("serialize QiWe async upload request")
}

pub fn parse_async_upload_acceptance(body: &[u8]) -> Result<String> {
    validate_json_body_size(body)?;
    let response: ApiResponse<UploadAcceptedData> =
        serde_json::from_slice(body).context("parse QiWe async upload response")?;
    if response.code != 0 {
        bail!("QiWe async upload request was rejected");
    }
    validate_plain_value(&response.data.request_id, "QiWe upload request id")?;
    Ok(response.data.request_id)
}

pub fn parse_async_upload_callback(
    body: &[u8],
    expected_request_id: &str,
) -> Result<QiweImageCredentials> {
    validate_json_body_size(body)?;
    validate_plain_value(expected_request_id, "QiWe upload request id")?;
    let envelope: CallbackEnvelope =
        serde_json::from_slice(body).context("parse QiWe async upload callback")?;
    if envelope.code != 0 {
        bail!("QiWe async upload callback reported failure");
    }
    let matching = envelope
        .data
        .into_iter()
        .filter(|event| event.cmd == ASYNC_EVENT_COMMAND && event.request_id == expected_request_id)
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        bail!("QiWe async upload callback must contain exactly one matching event");
    }
    let credentials: QiweImageCredentials = serde_json::from_value(
        matching
            .into_iter()
            .next()
            .and_then(|event| event.msg_data)
            .context("QiWe async upload callback is missing msgData")?,
    )
    .context("QiWe async upload callback is missing file credentials")?;
    credentials.validate()?;
    Ok(credentials)
}

pub fn build_send_image_request(
    guid: &str,
    target_group_id: &str,
    credentials: &QiweImageCredentials,
    allowed_group_ids: &BTreeSet<String>,
) -> Result<Vec<u8>> {
    validate_plain_value(guid, "QiWe device id")?;
    validate_plain_value(target_group_id, "QiWe target group id")?;
    if !allowed_group_ids.contains(target_group_id) {
        bail!("QiWe target group id is not allowlisted");
    }
    credentials.validate()?;
    serde_json::to_vec(&ApiRequest {
        method: SEND_IMAGE_METHOD,
        params: SendImageParams {
            guid,
            file_aes_key: &credentials.file_aes_key,
            file_id: &credentials.file_id,
            file_md5: &credentials.file_md5,
            file_size: credentials.file_size,
            filename: &credentials.filename,
            to_id: target_group_id,
        },
    })
    .context("serialize QiWe send-image request")
}

pub fn parse_send_image_response(body: &[u8]) -> Result<QiweSendReceipt> {
    validate_json_body_size(body)?;
    let response: ApiResponse<SendImageData> =
        serde_json::from_slice(body).context("parse QiWe send-image response")?;
    if response.code != 0 {
        bail!("QiWe send-image request was rejected");
    }
    if response.data.is_send_success != SEND_SUCCESS_VALUE {
        bail!("QiWe send-image response did not confirm success");
    }
    validate_plain_value(
        &response.data.msg_unique_identifier,
        "QiWe message identifier",
    )?;
    Ok(QiweSendReceipt {
        is_send_success: response.data.is_send_success,
        message_identifier: response.data.msg_unique_identifier,
        sequence: response.data.seq,
        timestamp: response.data.timestamp,
    })
}

pub fn validate_header_value(value: &str) -> Result<()> {
    if value.is_empty() || value.chars().any(char::is_control) {
        bail!("HTTP header value is invalid");
    }
    Ok(())
}

impl QiweImageCredentials {
    fn validate(&self) -> Result<()> {
        validate_plain_value(&self.file_aes_key, "QiWe file AES key")?;
        validate_plain_value(&self.file_id, "QiWe file id")?;
        validate_canonical_md5(&self.file_md5, "QiWe file MD5")?;
        validate_jpeg_filename(&self.filename)?;
        if self.file_size == 0 {
            bail!("QiWe file size must be positive");
        }
        Ok(())
    }
}

fn validate_canonical_md5(value: &str, label: &str) -> Result<()> {
    if value.len() != 32
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("{label} must be canonical md5");
    }
    Ok(())
}

fn required_env(name: &str) -> Result<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && !is_placeholder(value))
        .ok_or_else(|| anyhow!("required QiWe image-send configuration is missing"))
}

#[cfg(feature = "qiwe-staging-adapter")]
fn staging_apply_config(cli: &Cli) -> Result<AdapterConfig> {
    validate_staging_owner_approval(std::env::var(STAGING_APPROVAL_ENV).ok().as_deref())?;
    let database_url = cli.database_url_required()?;
    validate_staging_database_boundary(database_url)?;
    validate_feishu_delivery_config(database_url)?;
    AdapterConfig::from_env()
}

fn validate_feishu_delivery_config(database_url: &str) -> Result<()> {
    #[cfg(all(feature = "huabaosi-staging-adapter", feature = "qiwe-staging-adapter"))]
    {
        let _ = crate::huabaosi_feishu_artifact_mirror::FeishuPrimaryStorageConfig::from_env(
            database_url,
        )?;
        Ok(())
    }

    #[cfg(not(all(feature = "huabaosi-staging-adapter", feature = "qiwe-staging-adapter")))]
    {
        let _ = database_url;
        bail!("Feishu delivery bridge requires the combined staging feature build")
    }
}

fn validate_staging_owner_approval(value: Option<&str>) -> Result<()> {
    if value != Some(STAGING_APPROVAL_PHRASE) {
        bail!("QiWe image send staging owner approval is required");
    }
    Ok(())
}

fn validate_staging_database_boundary(database_url: &str) -> Result<()> {
    let expected_hash = std::env::var(STAGING_DATABASE_URL_SHA256_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("QiWe staging database URL hash is required"))?;
    validate_staging_database_boundary_with_expected_hash(database_url, &expected_hash)
}

fn validate_staging_database_boundary_with_expected_hash(
    database_url: &str,
    expected_hash: &str,
) -> Result<()> {
    if database_url.is_empty() || database_url.chars().any(char::is_control) {
        bail!("QiWe staging database URL is invalid");
    }
    if expected_hash.len() != 64
        || !expected_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("QiWe staging database URL hash must be canonical SHA-256");
    }
    let actual_hash = format!("{:x}", Sha256::digest(database_url.as_bytes()));
    if actual_hash != expected_hash {
        bail!("QiWe staging database URL hash does not match the approved command");
    }
    let parsed = Url::parse(database_url).context("parse QiWe staging database URL")?;
    if !matches!(parsed.scheme(), "postgres" | "postgresql") || parsed.host_str().is_none() {
        bail!("QiWe staging database URL must use PostgreSQL and include a host");
    }
    let database_name = parsed
        .path()
        .strip_prefix('/')
        .filter(|value| !value.is_empty() && !value.contains('/'))
        .ok_or_else(|| anyhow!("QiWe staging database URL must name exactly one database"))?;
    if !database_name.to_ascii_lowercase().contains("staging") {
        bail!("QiWe image send apply requires a staging database");
    }
    Ok(())
}

fn missing_qiwe_image_send_configuration() -> Vec<&'static str> {
    missing_required_configuration_with(REQUIRED_QIWE_IMAGE_SEND_CONFIGURATION, |name| {
        std::env::var(name).ok()
    })
}

fn missing_qiwe_image_staging_configuration(cli: &Cli) -> Vec<&'static str> {
    let mut missing = missing_qiwe_image_send_configuration();
    if feishu_delivery_bridge_compiled() {
        missing.extend(missing_required_configuration_with(
            REQUIRED_FEISHU_DELIVERY_CONFIGURATION,
            |name| std::env::var(name).ok(),
        ));
    }
    if cli.database_url_required().is_err() {
        missing.push("QINTOPIA_SIDECAR_DATABASE_URL");
    }
    if std::env::var(STAGING_DATABASE_URL_SHA256_ENV)
        .ok()
        .is_none_or(|value| value.trim().is_empty())
    {
        missing.push(STAGING_DATABASE_URL_SHA256_ENV);
    }
    missing
}

fn missing_required_configuration_with<F>(
    required: &'static [&'static str],
    mut read: F,
) -> Vec<&'static str>
where
    F: FnMut(&str) -> Option<String>,
{
    required
        .iter()
        .copied()
        .filter(|name| {
            read(name)
                .map(|value| value.trim().to_string())
                .is_none_or(|value| value.is_empty() || is_placeholder(&value))
        })
        .collect()
}

fn env_flag(name: &str) -> Result<bool> {
    match std::env::var(name)
        .unwrap_or_else(|_| "0".to_string())
        .trim()
    {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => bail!("QiWe image-send flag must be 0 or 1"),
    }
}

fn is_placeholder(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    normalized.contains("replace-with") || normalized == "change-me" || normalized == "placeholder"
}

fn strict_api_url(value: &str) -> Result<Url> {
    let url = strict_https_url(value, "QiWe API URL")?;
    if url.path() != "/qiwe/api/qw/doApi" {
        bail!("QiWe API URL path must match the reviewed doApi endpoint");
    }
    Ok(url)
}

fn strict_media_url(value: &str) -> Result<Url> {
    strict_https_url(value, "approved generated-image URI")
}

fn strict_https_url(value: &str, label: &str) -> Result<Url> {
    url_policy::reject_path_separator_ambiguity(value, label)?;
    let url = Url::parse(value).with_context(|| format!("parse {label}"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("{label} must be an HTTPS URL without credentials, query, or fragment");
    }
    Ok(url)
}

fn validate_plain_value(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        bail!("{label} is invalid");
    }
    Ok(())
}

fn validate_jpeg_filename(filename: &str) -> Result<()> {
    validate_plain_value(filename, "QiWe image filename")?;
    if filename.len() > 255 || filename.contains(['/', '\\']) {
        bail!("QiWe image filename is invalid");
    }
    let normalized = filename.to_ascii_lowercase();
    if !normalized.ends_with(".jpg") && !normalized.ends_with(".jpeg") {
        bail!("QiWe image filename must use the documented JPG format");
    }
    Ok(())
}

fn parse_csv_set(value: &str) -> BTreeSet<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| item.to_ascii_lowercase())
        .collect()
}

fn parse_csv_exact_set(value: &str) -> BTreeSet<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn validate_json_body_size(body: &[u8]) -> Result<()> {
    if body.is_empty() || body.len() > MAX_JSON_RESPONSE_BYTES {
        bail!("QiWe JSON response size is invalid");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        thread,
        time::Duration,
    };

    #[cfg(not(feature = "qiwe-staging-adapter"))]
    use clap::Parser;

    use super::*;

    fn test_preflight_report(
        config_valid: bool,
        send_enabled: bool,
        adapter_compiled: bool,
    ) -> QiweImageSendPreflightReport {
        preflight_report(PreflightReportState {
            config_valid,
            send_enabled,
            adapter_compiled,
            feishu_delivery_bridge_compiled: adapter_compiled,
            webhook_ready: true,
            allowed_host_count: 1,
            media_allowed_host_count: 1,
            allowed_group_count: 1,
            missing_configuration: Vec::new(),
        })
    }

    fn ready_staging_preflight_state() -> StagingPreflightReportState {
        StagingPreflightReportState {
            adapter_compiled: true,
            feishu_delivery_bridge_compiled: true,
            send_enabled: true,
            owner_approval_valid: true,
            config_valid: true,
            database_boundary_valid: true,
            webhook_ready: true,
            allowed_host_count: 1,
            media_allowed_host_count: 1,
            allowed_group_count: 1,
            missing_configuration: Vec::new(),
        }
    }

    fn test_callback_body(
        aes_key_field: &str,
        filename_field: &str,
        additional_fields: &[(&str, Value)],
    ) -> Vec<u8> {
        let mut msg_data = serde_json::Map::new();
        msg_data.insert(
            aes_key_field.to_string(),
            Value::String("callback-aes-secret".to_string()),
        );
        msg_data.insert(
            "fileId".to_string(),
            Value::String("callback-file-secret".to_string()),
        );
        msg_data.insert(
            "fileMd5".to_string(),
            Value::String("98e7c2acf4391f8b4a2bbd39e364c5e3".to_string()),
        );
        msg_data.insert("fileSize".to_string(), Value::from(48_301));
        msg_data.insert(
            filename_field.to_string(),
            Value::String("private-activity-poster.jpg".to_string()),
        );
        for (name, value) in additional_fields {
            msg_data.insert((*name).to_string(), value.clone());
        }

        serde_json::to_vec(&serde_json::json!({
            "code": 0,
            "data": [
                {"requestId": "ignored-sync-event", "cmd": 10000, "msgData": {}},
                {
                    "requestId": "callback-request-secret",
                    "cmd": 20000,
                    "msgData": msg_data,
                }
            ]
        }))
        .expect("serialize test callback")
    }

    #[test]
    fn async_upload_request_matches_official_contract() {
        let media_allowed_hosts = BTreeSet::from(["media.example.test".to_string()]);
        let body = build_async_upload_request(
            "device-guid",
            "activity-poster.jpg",
            "https://media.example.test/activity-poster.jpg",
            "image/jpeg",
            &media_allowed_hosts,
        )
        .expect("build upload request");
        let value: Value = serde_json::from_slice(&body).expect("parse upload request");

        assert_eq!(value["method"], ASYNC_UPLOAD_METHOD);
        assert_eq!(value["params"]["fileType"], IMAGE_FILE_TYPE);
        assert_eq!(value["params"]["guid"], "device-guid");
        assert_eq!(
            value["params"]["fileUrl"],
            "https://media.example.test/activity-poster.jpg"
        );
    }

    #[test]
    fn current_png_artifact_fails_closed() {
        let media_allowed_hosts = BTreeSet::from(["media.example.test".to_string()]);
        let error = build_async_upload_request(
            "device-guid",
            "activity-poster.png",
            "https://media.example.test/activity-poster.png",
            "image/png",
            &media_allowed_hosts,
        )
        .expect_err("PNG must not bypass the documented JPG boundary");

        assert!(error.to_string().contains("documented JPG format"));
    }

    #[test]
    fn upload_acceptance_requires_success_and_request_id() {
        let request_id = parse_async_upload_acceptance(
            br#"{"code":0,"data":{"requestId":"upload-request-1"},"msg":"success"}"#,
        )
        .expect("parse upload acceptance");
        assert_eq!(request_id, "upload-request-1");

        assert!(parse_async_upload_acceptance(br#"{"code":1,"data":{"requestId":"x"}}"#).is_err());
    }

    #[test]
    fn upload_acceptance_rejects_invalid_payloads_before_state_changes() {
        for body in [
            b"".as_slice(),
            &vec![b'x'; MAX_JSON_RESPONSE_BYTES + 1],
            br#"{"code":0,"data":{"requestId":""}}"#,
            br#"{"code":0,"data":{"requestId":"upload-request\nsecret"}}"#,
            br#"{"code":0,"data":{}}"#,
        ] {
            assert!(
                parse_async_upload_acceptance(body).is_err(),
                "accepted invalid upload response {:?}",
                String::from_utf8_lossy(body)
            );
        }
    }

    #[test]
    fn upload_call_parser_classifies_rejected_and_unknown_outcomes() {
        let accepted = parse_upload_acceptance_for_call(&http_json_response(
            200,
            br#"{"code":0,"data":{"requestId":"upload-request-1"}}"#,
        ))
        .expect("accepted upload request id");
        assert_eq!(accepted.as_str(), "upload-request-1");

        assert_eq!(
            parse_upload_acceptance_for_call(&http_json_response(
                200,
                br#"{"code":1,"data":{"requestId":"upload-request-1"}}"#,
            ))
            .expect_err("provider rejection is known"),
            UploadCallFailure::Rejected
        );
        assert_eq!(
            parse_upload_acceptance_for_call(&http_json_response(200, br#"not-json"#))
                .expect_err("malformed response is uncertain"),
            UploadCallFailure::OutcomeUnknown
        );
        assert_eq!(
            parse_upload_acceptance_for_call(&http_json_response(
                200,
                br#"{"code":0,"data":{"requestId":"upload-request-1\nsecret"}}"#,
            ))
            .expect_err("invalid request id is uncertain"),
            UploadCallFailure::OutcomeUnknown
        );
    }

    #[test]
    fn callback_matches_request_and_extracts_complete_credentials() {
        let callback = br#"{
          "code": 0,
          "data": [{
            "requestId": "upload-request-1",
            "cmd": 20000,
            "msgData": {
              "fileAesKey": "aes-key",
              "fileId": "file-id",
              "fileMd5": "98e7c2acf4391f8b4a2bbd39e364c5e3",
              "fileSize": 48300,
              "filename": "activity-poster.jpg"
            }
          }]
        }"#;
        let credentials = parse_async_upload_callback(callback, "upload-request-1")
            .expect("parse matching callback");
        let allowed_group_ids = BTreeSet::from(["group-id".to_string()]);
        let request =
            build_send_image_request("device-guid", "group-id", &credentials, &allowed_group_ids)
                .expect("build send request");
        let value: Value = serde_json::from_slice(&request).expect("parse send request");

        assert_eq!(value["method"], SEND_IMAGE_METHOD);
        assert_eq!(value["params"]["fileAesKey"], "aes-key");
        assert_eq!(value["params"]["fileId"], "file-id");
        assert_eq!(value["params"]["toId"], "group-id");
    }

    #[test]
    fn callback_without_send_credentials_fails_closed() {
        let callback = br#"{
          "code": 0,
          "data": [{
            "requestId": "upload-request-1",
            "cmd": 20000,
            "msgData": {"cloudUrl":"https://media.example.test/activity-poster.jpg"}
          }]
        }"#;

        assert!(parse_async_upload_callback(callback, "upload-request-1").is_err());
        assert!(parse_async_upload_callback(callback, "another-request").is_err());
    }

    #[test]
    fn duplicate_matching_callback_events_fail_closed() {
        let event = r#"{
          "requestId":"upload-request-1",
          "cmd":20000,
          "msgData":{
            "fileAesKey":"aes-key",
            "fileId":"file-id",
            "fileMd5":"98e7c2acf4391f8b4a2bbd39e364c5e3",
            "fileSize":48300,
            "filename":"activity-poster.jpg"
          }
        }"#;
        let callback = format!(r#"{{"code":0,"data":[{event},{event}]}}"#);

        assert!(parse_async_upload_callback(callback.as_bytes(), "upload-request-1").is_err());
    }

    #[test]
    fn callback_parser_rejects_failed_or_untrusted_credentials() {
        assert!(
            parse_async_upload_callback(br#"{"code":1,"data":[]}"#, "upload-request-1").is_err()
        );
        assert!(
            parse_async_upload_callback(br#"{"code":0,"data":[]}"#, "upload-request\nsecret")
                .is_err()
        );

        for msg_data in [
            r#"{"fileAesKey":"aes-key","fileId":"file-id","fileMd5":"98e7c2acf4391f8b4a2bbd39e364c5e3","fileSize":0,"filename":"activity-poster.jpg"}"#,
            r#"{"fileAesKey":"aes-key","fileId":"file-id","fileMd5":"98e7c2acf4391f8b4a2bbd39e364c5e3","fileSize":48300,"filename":"activity-poster.png"}"#,
            r#"{"fileAesKey":"aes-key\nsecret","fileId":"file-id","fileMd5":"98e7c2acf4391f8b4a2bbd39e364c5e3","fileSize":48300,"filename":"activity-poster.jpg"}"#,
        ] {
            let callback = format!(
                r#"{{"code":0,"data":[{{"requestId":"upload-request-1","cmd":20000,"msgData":{msg_data}}}]}}"#
            );
            assert!(
                parse_async_upload_callback(callback.as_bytes(), "upload-request-1").is_err(),
                "accepted invalid callback credentials {msg_data}"
            );
        }
    }

    #[test]
    fn send_response_parses_internal_receipt() {
        let receipt = parse_send_image_response(
            br#"{
              "code":0,
              "data":{
                "isSendSuccess":1,
                "msgServerId":1,
                "msgType":14,
                "msgUniqueIdentifier":"message-1",
                "seq":2,
                "timestamp":3
              },
              "msg":"success"
            }"#,
        )
        .expect("parse send response");

        assert_eq!(receipt.is_send_success, SEND_SUCCESS_VALUE);
        assert_eq!(receipt.message_identifier, "message-1");
        assert_eq!(receipt.sequence, 2);
        assert_eq!(receipt.timestamp, 3);
    }

    #[test]
    fn send_response_requires_explicit_provider_success() {
        let response = br#"{
          "code":0,
          "data":{
            "isSendSuccess":0,
            "msgUniqueIdentifier":"message-1",
            "seq":2,
            "timestamp":3
          }
        }"#;

        assert!(parse_send_image_response(response).is_err());
    }

    #[test]
    fn send_response_rejects_invalid_payloads_before_receipt_use() {
        for body in [
            b"".as_slice(),
            &vec![b'x'; MAX_JSON_RESPONSE_BYTES + 1],
            br#"{"code":0,"data":{"isSendSuccess":1,"msgUniqueIdentifier":"","seq":2,"timestamp":3}}"#,
            br#"{"code":0,"data":{"isSendSuccess":1,"msgUniqueIdentifier":"message\nsecret","seq":2,"timestamp":3}}"#,
            br#"{"code":0,"data":{"isSendSuccess":1,"seq":2,"timestamp":3}}"#,
        ] {
            assert!(
                parse_send_image_response(body).is_err(),
                "accepted invalid send response {:?}",
                String::from_utf8_lossy(body)
            );
        }
    }

    #[test]
    fn send_call_parser_treats_every_post_send_parse_failure_as_ambiguous() {
        let receipt = parse_send_response_for_call(&http_json_response(
            200,
            br#"{
              "code":0,
              "data":{
                "isSendSuccess":1,
                "msgUniqueIdentifier":"message-1",
                "seq":2,
                "timestamp":3
              }
            }"#,
        ))
        .expect("parse successful send receipt");
        assert_eq!(receipt.message_identifier, "message-1");

        for body in [
            br#"not-json"#.as_slice(),
            br#"{"code":1,"data":{"isSendSuccess":0,"msgUniqueIdentifier":"message-1","seq":2,"timestamp":3}}"#,
            br#"{"code":0,"data":{"isSendSuccess":0,"msgUniqueIdentifier":"message-1","seq":2,"timestamp":3}}"#,
            br#"{"code":0,"data":{"isSendSuccess":1,"msgUniqueIdentifier":"message-1\nsecret","seq":2,"timestamp":3}}"#,
        ] {
            let Err(error) = parse_send_response_for_call(&http_json_response(200, body)) else {
                panic!("post-send parse failure must be ambiguous");
            };
            assert_eq!(
                error,
                SendCallFailure::Ambiguous,
                "post-send parse failure is ambiguous"
            );
        }
    }

    #[test]
    fn send_request_rejects_non_allowlisted_group() {
        let credentials = QiweImageCredentials {
            file_aes_key: "aes-key".to_string(),
            file_id: "file-id".to_string(),
            file_md5: "98e7c2acf4391f8b4a2bbd39e364c5e3".to_string(),
            file_size: 48_300,
            filename: "activity-poster.jpg".to_string(),
        };
        let allowed_group_ids = BTreeSet::from(["reviewed-group".to_string()]);

        assert!(build_send_image_request(
            "device-guid",
            "unreviewed-group",
            &credentials,
            &allowed_group_ids,
        )
        .is_err());
    }

    #[test]
    fn send_request_group_allowlist_is_case_sensitive() {
        let credentials = QiweImageCredentials {
            file_aes_key: "aes-key".to_string(),
            file_id: "file-id".to_string(),
            file_md5: "98e7c2acf4391f8b4a2bbd39e364c5e3".to_string(),
            file_size: 48_300,
            filename: "activity-poster.jpg".to_string(),
        };
        let allowed_group_ids = BTreeSet::from(["group-id".to_string()]);

        assert!(build_send_image_request(
            "device-guid",
            "GROUP-ID",
            &credentials,
            &allowed_group_ids,
        )
        .is_err());
    }

    #[test]
    fn send_request_rejects_invalid_memory_only_credentials() {
        let allowed_group_ids = BTreeSet::from(["group-id".to_string()]);
        for credentials in [
            QiweImageCredentials {
                file_aes_key: "aes-key".to_string(),
                file_id: "file-id".to_string(),
                file_md5: "98e7c2acf4391f8b4a2bbd39e364c5e3".to_string(),
                file_size: 0,
                filename: "activity-poster.jpg".to_string(),
            },
            QiweImageCredentials {
                file_aes_key: "aes-key\nsecret".to_string(),
                file_id: "file-id".to_string(),
                file_md5: "98e7c2acf4391f8b4a2bbd39e364c5e3".to_string(),
                file_size: 48_300,
                filename: "activity-poster.jpg".to_string(),
            },
            QiweImageCredentials {
                file_aes_key: "aes-key".to_string(),
                file_id: "file-id".to_string(),
                file_md5: "98e7c2acf4391f8b4a2bbd39e364c5e3".to_string(),
                file_size: 48_300,
                filename: "activity-poster.png".to_string(),
            },
        ] {
            assert!(build_send_image_request(
                "device-guid",
                "group-id",
                &credentials,
                &allowed_group_ids,
            )
            .is_err());
        }
    }

    #[test]
    fn headers_and_json_bodies_are_bounded() {
        assert!(validate_header_value("token\r\nInjected: true").is_err());
        assert!(validate_header_value("token\0suffix").is_err());
        assert!(validate_header_value("token\tsuffix").is_err());
        assert!(validate_header_value("valid-token").is_ok());
        assert!(parse_async_upload_acceptance(&vec![b'x'; MAX_JSON_RESPONSE_BYTES + 1]).is_err());
    }

    #[test]
    fn api_url_requires_https_and_reviewed_path() {
        assert!(strict_api_url("http://manager.qiweapi.com/qiwe/api/qw/doApi").is_err());
        assert!(strict_api_url("https://manager.qiweapi.com/qiwe/api/qw/doApi").is_ok());
        assert!(strict_api_url("https://manager.qiweapi.com/other").is_err());
        assert!(strict_api_url("https://user:pass@manager.qiweapi.com/qiwe/api/qw/doApi").is_err());
        assert!(
            strict_api_url("https://manager.qiweapi.com/qiwe/api/qw/doApi?token=secret").is_err()
        );
        assert!(strict_api_url("https://manager.qiweapi.com/qiwe/api/qw/doApi#fragment").is_err());
    }

    #[test]
    fn upload_rejects_non_allowlisted_media_host() {
        let media_allowed_hosts = BTreeSet::from(["media.example.test".to_string()]);
        let result = build_async_upload_request(
            "device-guid",
            "activity-poster.jpg",
            "https://unapproved.example.test/activity-poster.jpg",
            "image/jpeg",
            &media_allowed_hosts,
        );

        assert!(result.is_err());
    }

    #[test]
    fn host_parsing_normalizes_hosts_but_preserves_group_ids() {
        let hosts = parse_csv_set(" Manager.QIWEAPI.com, manager.qiweapi.com ,, ");
        assert_eq!(hosts, BTreeSet::from(["manager.qiweapi.com".to_string()]));

        let groups = parse_csv_exact_set("Group-A, group-a,, ");
        assert!(groups.contains("Group-A"));
        assert!(groups.contains("group-a"));
        assert_ne!(groups.len(), 1);
    }

    #[test]
    fn configuration_helpers_trim_placeholders_without_exposing_values() {
        assert!(is_placeholder("change-me"));
        assert!(is_placeholder("REPLACE-WITH-QIWE-TOKEN"));
        assert!(is_placeholder("placeholder"));
        assert!(!is_placeholder("real-looking-value"));

        let missing = missing_required_configuration_with(
            &["QIWE_TOKEN", "QIWE_GUID", "QIWE_API_URL", "QIWE_GROUP"],
            |name| match name {
                "QIWE_TOKEN" => Some("  super-secret-token  ".to_string()),
                "QIWE_GUID" => Some("replace-with-guid".to_string()),
                "QIWE_API_URL" => Some(" ".to_string()),
                _ => None,
            },
        );

        assert_eq!(missing, vec!["QIWE_GUID", "QIWE_API_URL", "QIWE_GROUP"]);
        let serialized = serde_json::to_string(&missing).expect("serialize missing names");
        assert!(!serialized.contains("super-secret-token"));
        assert!(!serialized.contains("replace-with-guid"));
    }

    #[test]
    fn url_and_filename_boundaries_reject_unstable_inputs() {
        assert!(strict_media_url("https://media.example.test/poster.jpg").is_ok());
        assert!(strict_media_url("https://user@media.example.test/poster.jpg").is_err());
        assert!(strict_media_url("https://media.example.test/poster.jpg?token=secret").is_err());
        assert!(strict_media_url("https://media.example.test/poster.jpg#fragment").is_err());
        assert!(strict_media_url("https://media.example.test/poster\\private.jpg").is_err());
        assert!(strict_media_url("https://media.example.test/poster%5Cprivate.jpg").is_err());
        assert!(strict_media_url("https://media.example.test/posters%2Fprivate.jpg").is_err());
        assert!(strict_api_url("https://manager.qiweapi.com\\private/qiwe/api/qw/doApi").is_err());
        assert!(strict_api_url("https://manager.qiweapi.com%2Fprivate/qiwe/api/qw/doApi").is_err());
        assert!(validate_jpeg_filename(&format!("{}.jpg", "a".repeat(252))).is_err());
    }

    #[test]
    fn preflight_report_never_exposes_configuration_values() {
        let report = test_preflight_report(true, false, false);
        let output = serde_json::to_string(&report).expect("serialize preflight report");

        assert!(report.success);
        assert!(!report.adapter_compiled);
        assert!(!report.send_enabled);
        assert!(!report.safe_for_chat);
        assert!(report.missing_configuration.is_empty());
        assert!(output.contains("deterministic final JPEG"));
        assert!(!output.contains("current generated-image artifact is PNG"));
        for sensitive_value in [
            "super-secret-token",
            "live-device-guid",
            "reviewed-group-123",
            "private-file-id",
            "private-file-aes-key",
            "live-message-identifier",
            "https://private-media.example/poster.jpg",
        ] {
            assert!(!output.contains(sensitive_value));
        }
    }

    #[test]
    fn staging_owner_approval_requires_exact_phrase() {
        assert!(validate_staging_owner_approval(Some(STAGING_APPROVAL_PHRASE)).is_ok());
        assert!(validate_staging_owner_approval(None).is_err());
        assert!(validate_staging_owner_approval(Some("approved-production-send")).is_err());
        assert_eq!(
            STAGING_APPROVAL_ENV,
            "QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL"
        );
    }

    #[test]
    fn staging_database_boundary_requires_matching_hash_and_staging_name() {
        let staging_url = "postgres://staging-user:secret@127.0.0.1:5432/qintopia_staging";
        let staging_hash = format!("{:x}", Sha256::digest(staging_url.as_bytes()));
        assert!(
            validate_staging_database_boundary_with_expected_hash(staging_url, &staging_hash)
                .is_ok()
        );
        let uppercase_name_url = "postgres://staging-user:secret@127.0.0.1:5432/QINTOPIA_STAGING";
        let uppercase_name_hash = format!("{:x}", Sha256::digest(uppercase_name_url.as_bytes()));
        assert!(validate_staging_database_boundary_with_expected_hash(
            uppercase_name_url,
            &uppercase_name_hash
        )
        .is_ok());
        assert!(validate_staging_database_boundary_with_expected_hash(
            staging_url,
            &"0".repeat(64)
        )
        .is_err());
        assert!(validate_staging_database_boundary_with_expected_hash(
            "postgres://user:secret@127.0.0.1:5432/qintopia",
            &format!(
                "{:x}",
                Sha256::digest(b"postgres://user:secret@127.0.0.1:5432/qintopia")
            )
        )
        .is_err());
        let uppercase_hash = "A".repeat(64);
        for invalid_hash in ["0", uppercase_hash.as_str()] {
            assert!(validate_staging_database_boundary_with_expected_hash(
                staging_url,
                invalid_hash
            )
            .is_err());
        }
        let control_url = "postgres://staging-user:secret@127.0.0.1:5432/qintopia_staging\nprivate";
        let control_hash = format!("{:x}", Sha256::digest(control_url.as_bytes()));
        assert!(
            validate_staging_database_boundary_with_expected_hash(control_url, &control_hash)
                .is_err()
        );
    }

    #[test]
    fn staging_preflight_reports_each_gate_without_configuration_values() {
        let ready = staging_preflight_report(ready_staging_preflight_state());
        assert!(ready.success);
        assert_eq!(ready.action_status, "staging_adapter_ready");

        let cases = [
            ("staging_adapter_not_compiled", 0),
            ("feishu_delivery_bridge_not_compiled", 1),
            ("staging_send_not_enabled", 2),
            ("staging_owner_approval_required", 3),
            ("staging_database_not_approved", 4),
            ("adapter_not_configured", 5),
        ];
        for (expected_status, gate) in cases {
            let mut state = ready_staging_preflight_state();
            match gate {
                0 => state.adapter_compiled = false,
                1 => state.feishu_delivery_bridge_compiled = false,
                2 => state.send_enabled = false,
                3 => state.owner_approval_valid = false,
                4 => state.database_boundary_valid = false,
                5 => state.config_valid = false,
                _ => unreachable!(),
            }
            let report = staging_preflight_report(state);
            assert!(!report.success);
            assert_eq!(report.action_status, expected_status);
        }

        let serialized = serde_json::to_string(&ready).expect("serialize staging preflight");
        for sensitive in [
            "postgres://staging-user:secret@127.0.0.1/qintopia_staging",
            "private-token",
            "private-device-guid",
            "private-group-id",
            "https://private-media.example/poster.jpg",
        ] {
            assert!(!serialized.contains(sensitive));
        }
    }

    #[test]
    fn enabled_preflight_fails_closed() {
        let report = test_preflight_report(true, true, false);

        assert!(!report.success);
        assert!(report.config_valid);
        assert!(report.send_enabled);
        assert_eq!(report.action_status, "adapter_enablement_not_approved");
    }

    #[test]
    fn compiled_staging_adapter_fails_production_preflight() {
        let report = test_preflight_report(true, false, true);

        assert!(!report.success);
        assert!(report.adapter_compiled);
        assert_eq!(
            report.action_status,
            "staging_adapter_compiled_requires_owner_review"
        );
    }

    #[cfg(not(feature = "qiwe-staging-adapter"))]
    #[test]
    fn default_build_excludes_qiwe_staging_adapter() {
        assert!(!qiwe_staging_adapter_compiled());
        assert!(!feishu_delivery_bridge_compiled());
    }

    #[cfg(all(
        feature = "qiwe-staging-adapter",
        not(feature = "huabaosi-staging-adapter")
    ))]
    #[test]
    fn qiwe_only_build_excludes_feishu_delivery_bridge() {
        assert!(qiwe_staging_adapter_compiled());
        assert!(!feishu_delivery_bridge_compiled());
    }

    #[cfg(all(feature = "qiwe-staging-adapter", feature = "huabaosi-staging-adapter"))]
    #[test]
    fn combined_staging_build_contains_feishu_delivery_bridge() {
        assert!(qiwe_staging_adapter_compiled());
        assert!(feishu_delivery_bridge_compiled());
    }

    #[cfg(not(feature = "qiwe-staging-adapter"))]
    #[tokio::test]
    async fn default_upload_apply_stops_before_database_and_network() {
        let cli = Cli::parse_from(["qintopia-message-sidecar", "check"]);

        let error = run_upload_worker(&cli, true, None, true, false)
            .await
            .expect_err("default build must reject apply before database access");

        assert!(error.to_string().contains("not compiled"));
    }

    #[cfg(not(feature = "qiwe-staging-adapter"))]
    #[tokio::test]
    async fn default_callback_apply_stops_before_stdin_database_and_network() {
        let cli = Cli::parse_from(["qintopia-message-sidecar", "check"]);

        let error = run_callback_processor(&cli, true, false)
            .await
            .expect_err("default build must reject callback apply before stdin");

        assert!(error.to_string().contains("not compiled"));
    }

    #[test]
    fn invalid_preflight_reports_public_missing_names_only() {
        let report = preflight_report(PreflightReportState {
            config_valid: false,
            send_enabled: false,
            adapter_compiled: false,
            feishu_delivery_bridge_compiled: false,
            webhook_ready: false,
            allowed_host_count: 0,
            media_allowed_host_count: 0,
            allowed_group_count: 0,
            missing_configuration: vec!["QIWE_TOKEN", "QIWE_GUID"],
        });
        let output = serde_json::to_string(&report).expect("serialize preflight report");

        assert!(!report.success);
        assert!(!report.config_valid);
        assert_eq!(report.action_status, "adapter_not_configured");
        assert_eq!(
            report.missing_configuration,
            vec!["QIWE_TOKEN", "QIWE_GUID"]
        );
        for sensitive in [
            "secret-token-value",
            "live-device-guid",
            "group-id",
            "https://media.example.test/activity-poster.jpg",
        ] {
            assert!(!output.contains(sensitive));
        }
    }

    #[test]
    fn qiwe_preflight_missing_configuration_is_public_and_deterministic() {
        let missing = missing_required_configuration_with(
            &["PUBLIC_READY", "PUBLIC_PLACEHOLDER", "PUBLIC_ABSENT"],
            |name| match name {
                "PUBLIC_READY" => Some("configured".to_string()),
                "PUBLIC_PLACEHOLDER" => Some("change-me".to_string()),
                _ => None,
            },
        );

        assert_eq!(missing, vec!["PUBLIC_PLACEHOLDER", "PUBLIC_ABSENT"]);
    }

    #[test]
    fn feishu_bridge_uploads_reads_back_and_starts_existing_async_protocol() {
        let bytes = b"reviewed-final-jpeg-bytes".to_vec();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind bridge fake server");
        let port = listener.local_addr().expect("bridge fake address").port();
        let expected_bytes = bytes.clone();
        let server = thread::spawn(move || {
            let (mut upload_stream, _) = listener.accept().expect("accept temporary upload");
            let upload = read_test_request(&mut upload_stream);
            assert!(upload
                .headers
                .starts_with("POST /qiwe/api/qw/doFileApi HTTP/1.1"));
            assert!(upload.headers.contains("multipart/form-data; boundary="));
            assert!(upload
                .body
                .windows(TEMPORARY_STORAGE_UPLOAD_METHOD.len())
                .any(|window| window == TEMPORARY_STORAGE_UPLOAD_METHOD.as_bytes()));
            assert!(upload
                .body
                .windows(expected_bytes.len())
                .any(|window| window == expected_bytes));
            upload_stream
                .write_all(&json_response(&format!(
                    r#"{{"code":0,"data":{{"cloudUrl":"http://127.0.0.1:{port}/temporary/reviewed.jpg"}}}}"#
                )))
                .expect("write temporary upload response");
            drop(upload_stream);

            let (mut readback_stream, _) = listener.accept().expect("accept temporary readback");
            let readback = read_test_request(&mut readback_stream);
            assert!(readback
                .headers
                .starts_with("GET /temporary/reviewed.jpg HTTP/1.1"));
            readback_stream
                .write_all(&binary_response(&expected_bytes))
                .expect("write temporary readback");
            drop(readback_stream);

            let (mut async_stream, _) = listener.accept().expect("accept async upload");
            let async_request = read_test_request(&mut async_stream);
            assert!(async_request
                .headers
                .starts_with("POST /qiwe/api/qw/doApi HTTP/1.1"));
            let body: Value =
                serde_json::from_slice(&async_request.body).expect("parse async upload body");
            assert_eq!(body["method"], ASYNC_UPLOAD_METHOD);
            assert_eq!(
                body["params"]["fileUrl"],
                format!("http://127.0.0.1:{port}/temporary/reviewed.jpg")
            );
            async_stream
                .write_all(&json_response(
                    r#"{"code":0,"data":{"requestId":"bridge-upload-request"}}"#,
                ))
                .expect("write async upload response");
        });

        let config = test_adapter_config(port);
        let claim = test_feishu_upload_claim(&bytes);
        let request_id =
            request_claim_upload_with(&config, &claim, Some(&bytes), &HttpClient::test_only())
                .expect("Feishu bridge starts the existing async upload");
        assert_eq!(request_id.as_str(), "bridge-upload-request");
        server.join().expect("join bridge fake server");
    }

    #[test]
    fn feishu_bridge_fails_closed_before_async_upload_on_readback_drift() {
        let bytes = b"reviewed-final-jpeg-bytes".to_vec();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind drift fake server");
        let port = listener.local_addr().expect("drift fake address").port();
        let changed = b"reviewed-final-jpeg-byteX".to_vec();
        assert_eq!(bytes.len(), changed.len());
        let server = thread::spawn(move || {
            let (mut upload_stream, _) = listener.accept().expect("accept temporary upload");
            let _ = read_test_request(&mut upload_stream);
            upload_stream
                .write_all(&json_response(&format!(
                    r#"{{"code":0,"data":{{"cloudUrl":"http://127.0.0.1:{port}/temporary/drift.jpg"}}}}"#
                )))
                .expect("write temporary upload response");
            drop(upload_stream);
            let (mut readback_stream, _) = listener.accept().expect("accept temporary readback");
            let _ = read_test_request(&mut readback_stream);
            readback_stream
                .write_all(&binary_response(&changed))
                .expect("write drifted readback");
            drop(readback_stream);
        });

        let result = request_claim_upload_with(
            &test_adapter_config(port),
            &test_feishu_upload_claim(&bytes),
            Some(&bytes),
            &HttpClient::test_only(),
        );
        assert_eq!(
            result.expect_err("drifted readback fails closed"),
            UploadCallFailure::OutcomeUnknown
        );
        server.join().expect("join drift fake server");
    }

    #[test]
    fn temporary_storage_post_request_failures_are_ambiguous() {
        let hosts = BTreeSet::from(["temporary.example.test".to_string()]);
        let business_failure = http_json_response(
            200,
            br#"{"code":1,"data":{"cloudUrl":"https://temporary.example.test/possibly-written.jpg"}}"#,
        );
        assert_eq!(
            parse_temporary_storage_acceptance_for_call(&business_failure, &hosts, false)
                .expect_err("business failure cannot prove no temporary write"),
            UploadCallFailure::OutcomeUnknown
        );

        let bytes = b"reviewed-final-jpeg-bytes".to_vec();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind non-2xx fake server");
        let port = listener.local_addr().expect("non-2xx fake address").port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept temporary upload");
            let _ = read_test_request(&mut stream);
            stream
                .write_all(
                    b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .expect("write non-2xx response");
        });
        let result = request_claim_upload_with(
            &test_adapter_config(port),
            &test_feishu_upload_claim(&bytes),
            Some(&bytes),
            &HttpClient::test_only(),
        );
        assert_eq!(
            result.expect_err("non-2xx cannot prove no temporary write"),
            UploadCallFailure::OutcomeUnknown
        );
        server.join().expect("join non-2xx fake server");
    }

    #[test]
    fn temporary_storage_cloud_url_uses_media_allowlist_not_api_allowlist() {
        let api_hosts = BTreeSet::from(["manager.qiweapi.com".to_string()]);
        let media_hosts = BTreeSet::from(["temporary.example.test".to_string()]);
        let response = http_json_response(
            200,
            br#"{"code":0,"data":{"cloudUrl":"https://temporary.example.test/reviewed.jpg"}}"#,
        );

        assert_eq!(
            parse_temporary_storage_acceptance_for_call(&response, &api_hosts, false)
                .expect_err("temporary URL must not use the API allowlist"),
            UploadCallFailure::OutcomeUnknown
        );
        assert_eq!(
            parse_temporary_storage_acceptance_for_call(&response, &media_hosts, false)
                .expect("temporary URL uses reviewed media allowlist")
                .as_str(),
            "https://temporary.example.test/reviewed.jpg"
        );
    }

    #[cfg(all(feature = "huabaosi-staging-adapter", feature = "qiwe-staging-adapter"))]
    #[test]
    fn feishu_delivery_identity_must_match_the_locked_qiwe_claim() {
        use crate::huabaosi_feishu_artifact_mirror::FeishuPrimaryStorageDeliveryArtifact;

        let bytes = b"reviewed-final-jpeg-bytes".to_vec();
        let claim = test_feishu_upload_claim(&bytes);
        let fixture = || FeishuPrimaryStorageDeliveryArtifact {
            artifact_id: claim.generated_image_artifact_id,
            artifact_uri: claim.artifact_uri.clone(),
            content_hash: claim.artifact_content_hash.clone(),
            file_md5: claim.artifact_file_md5.clone(),
            byte_size: claim.artifact_byte_size,
            bytes: Zeroizing::new(bytes.clone()),
        };
        validate_feishu_delivery_artifact(&claim, &fixture())
            .expect("matching delivery artifact is accepted");

        for field in [
            "artifact_id",
            "artifact_uri",
            "content_hash",
            "file_md5",
            "byte_size",
        ] {
            let mut artifact = fixture();
            match field {
                "artifact_id" => artifact.artifact_id = Uuid::new_v4(),
                "artifact_uri" => artifact.artifact_uri.push_str("-drifted"),
                "content_hash" => artifact.content_hash.push('0'),
                "file_md5" => artifact.file_md5.push('0'),
                "byte_size" => artifact.byte_size += 1,
                _ => unreachable!(),
            }
            assert!(
                validate_feishu_delivery_artifact(&claim, &artifact).is_err(),
                "accepted drifted delivery identity field {field}"
            );
        }
    }

    #[test]
    fn temporary_storage_url_and_file_api_are_exactly_bounded() {
        let api = Url::parse("https://manager.qiweapi.com/qiwe/api/qw/doApi")
            .expect("fixture API URL is valid");
        assert_eq!(
            file_api_url_from_api_url(&api)
                .expect("derive fixed file API path")
                .as_str(),
            "https://manager.qiweapi.com/qiwe/api/qw/doFileApi"
        );
        let hosts = BTreeSet::from(["temporary.example.test".to_string()]);
        assert!(strict_temporary_storage_url(
            "https://temporary.example.test/reviewed.jpg",
            &hosts,
            false
        )
        .is_ok());
        for url in [
            "https://other.example.test/reviewed.jpg",
            "https://temporary.example.test/reviewed.jpg?token=secret",
            "http://temporary.example.test/reviewed.jpg",
        ] {
            assert!(strict_temporary_storage_url(url, &hosts, false).is_err());
        }
        assert!(strict_temporary_storage_url(
            &format!("https://temporary.example.test/{}", "a".repeat(4096)),
            &hosts,
            false
        )
        .is_err());
        assert!(build_temporary_storage_upload_body(
            "qintopia-fixed-boundary",
            "device-guid",
            "generated-image.jpg",
            b"jpeg-qintopia-fixed-boundary-bytes"
        )
        .is_err());
    }

    #[test]
    fn fake_qiwe_server_completes_upload_and_send_contract() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake QiWe server");
        let port = listener.local_addr().expect("fake QiWe address").port();
        let server = thread::spawn(move || {
            let mut captured = Vec::new();
            for response_body in [
                r#"{"code":0,"data":{"requestId":"fake-upload-request"}}"#,
                r#"{"code":0,"data":{"isSendSuccess":1,"msgUniqueIdentifier":"fake-message-id","seq":2,"timestamp":3}}"#,
            ] {
                let (mut stream, _) = listener.accept().expect("accept fake QiWe request");
                let request = read_test_request(&mut stream);
                captured.push(request);
                stream
                    .write_all(&json_response(response_body))
                    .expect("write fake QiWe response");
            }
            captured
        });
        let config = test_adapter_config(port);
        let claim = test_upload_claim();
        let request_id = request_async_upload_with(&config, &claim, &HttpClient::test_only())
            .expect("fake async upload accepted");
        assert_eq!(request_id.as_str(), "fake-upload-request");
        let callback = parse_single_async_upload_callback(
            br#"{
              "code":0,
              "data":[{
                "requestId":"fake-upload-request",
                "cmd":20000,
                "msgData":{
                  "fileAesKey":"fake-aes-secret",
                  "fileId":"fake-file-secret",
                  "fileMd5":"98e7c2acf4391f8b4a2bbd39e364c5e3",
                  "fileSize":48300,
                  "filename":"activity-poster.jpg"
                }
              }]
            }"#,
        )
        .expect("parse fake callback");
        let send_body = build_send_image_request(
            &config.guid,
            "group-id",
            &callback.credentials,
            &config.allowed_groups,
        )
        .expect("build fake send request");
        let receipt = request_send_image_with(&config, &send_body, &HttpClient::test_only())
            .expect("fake image send succeeds");
        assert_eq!(receipt.message_identifier, "fake-message-id");

        let captured = server.join().expect("join fake QiWe server");
        assert_eq!(captured.len(), 2);
        for request in &captured {
            assert!(request
                .headers
                .starts_with("POST /qiwe/api/qw/doApi HTTP/1.1"));
            assert!(request.headers.contains("x-qiwei-token: fake-token"));
        }
        let upload: Value = serde_json::from_slice(&captured[0].body).expect("parse upload body");
        assert_eq!(upload["method"], ASYNC_UPLOAD_METHOD);
        assert_eq!(upload["params"]["fileUrl"], claim.artifact_uri);
        let send: Value = serde_json::from_slice(&captured[1].body).expect("parse send body");
        assert_eq!(send["method"], SEND_IMAGE_METHOD);
        assert_eq!(send["params"]["toId"], "group-id");
    }

    #[test]
    fn post_send_failures_are_ambiguous() {
        let oversized_listener =
            TcpListener::bind("127.0.0.1:0").expect("bind oversized fake server");
        let oversized_port = oversized_listener.local_addr().unwrap().port();
        let oversized_server = thread::spawn(move || {
            let (mut stream, _) = oversized_listener.accept().unwrap();
            let _ = read_test_request(&mut stream);
            let body = vec![b'x'; MAX_JSON_RESPONSE_BYTES + 1];
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(&body).unwrap();
        });
        let body = test_send_body();
        let result = request_send_image_with(
            &test_adapter_config(oversized_port),
            &body,
            &HttpClient::test_only(),
        );
        assert_eq!(
            result.err().expect("oversized response fails"),
            SendCallFailure::Ambiguous
        );
        oversized_server.join().unwrap();

        let slow_listener = TcpListener::bind("127.0.0.1:0").expect("bind slow fake server");
        let slow_port = slow_listener.local_addr().unwrap().port();
        let slow_server = thread::spawn(move || {
            let (mut stream, _) = slow_listener.accept().unwrap();
            let _ = read_test_request(&mut stream);
            thread::sleep(Duration::from_millis(200));
        });
        let result = request_send_image_with(
            &test_adapter_config(slow_port),
            &body,
            &HttpClient::test_only_with_timeout(Duration::from_millis(30)),
        );
        assert_eq!(
            result.err().expect("slow response fails"),
            SendCallFailure::Ambiguous
        );
        slow_server.join().unwrap();

        for response in [
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec(),
            json_response(
                r#"{"code":1,"data":{"isSendSuccess":0,"msgUniqueIdentifier":"not-confirmed","seq":0,"timestamp":0}}"#,
            ),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind failure fake server");
            let port = listener.local_addr().unwrap().port();
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let _ = read_test_request(&mut stream);
                stream.write_all(&response).unwrap();
            });
            let result = request_send_image_with(
                &test_adapter_config(port),
                &body,
                &HttpClient::test_only(),
            );
            assert_eq!(
                result.err().expect("non-success response fails closed"),
                SendCallFailure::Ambiguous
            );
            server.join().unwrap();
        }
    }

    #[test]
    fn connection_refusal_and_header_injection_stop_before_send() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("reserve refused port");
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let body = test_send_body();
        let result = request_send_image_with(
            &test_adapter_config(port),
            &body,
            &HttpClient::test_only_with_timeout(Duration::from_millis(30)),
        );
        assert_eq!(
            result.err().expect("refused connection fails"),
            SendCallFailure::NotSent
        );

        let mut config = test_adapter_config(port);
        config.token = "token\r\nInjected: true".to_string();
        let result =
            request_async_upload_with(&config, &test_upload_claim(), &HttpClient::test_only());
        assert_eq!(
            result.expect_err("injected header fails"),
            UploadCallFailure::Rejected
        );
    }

    #[test]
    fn callback_processor_requires_one_event_and_reports_no_secrets() {
        let duplicate = br#"{
          "code":0,
          "data":[
            {"requestId":"one","cmd":20000,"msgData":{}},
            {"requestId":"two","cmd":20000,"msgData":{}}
          ]
        }"#;
        assert!(parse_single_async_upload_callback(duplicate).is_err());

        let report = worker_report(WorkerReportState {
            success: false,
            dry_run: false,
            apply_requested: true,
            phase: "callback",
            action_status: "image_send_ambiguous".to_string(),
            work_item_id: Some(Uuid::nil()),
            external_upload_requested: false,
            callback_received: true,
            external_send_executed: None,
        });
        let serialized = serde_json::to_string(&report).expect("serialize worker report");
        assert!(!report.safe_for_chat);
        assert_eq!(report.external_send_executed, None);
        for sensitive in [
            "fake-token",
            "fake-device-guid",
            "group-id",
            "fake-upload-request",
            "fake-aes-secret",
            "fake-file-secret",
            "fake-message-id",
            "https://media.example.test/activity-poster.jpg",
        ] {
            assert!(!serialized.contains(sensitive));
        }
    }

    #[test]
    fn callback_parser_rejects_non_success_or_unrelated_events() {
        assert!(parse_single_async_upload_callback(br#"{"code":1,"data":[]}"#).is_err());
        assert!(parse_single_async_upload_callback(
            br#"{"code":0,"data":[{"requestId":"one","cmd":1,"msgData":{}}]}"#
        )
        .is_err());
        assert!(parse_async_upload_callback(
            br#"{"code":0,"data":[{"requestId":"one","cmd":1,"msgData":{}}]}"#,
            "one",
        )
        .is_err());
    }

    #[test]
    fn contract_self_check_covers_upload_callback_and_send_shapes() {
        validate_contract().expect("QiWe adapter contract self-check stays valid");
    }

    #[test]
    fn callback_parser_reports_each_reviewed_credential_schema() {
        for (aes_key_field, filename_field, schema_id) in [
            (
                "fileAesKey",
                "filename",
                "fileAesKey+fileId+fileMd5+fileSize+filename",
            ),
            (
                "fileAeskey",
                "filename",
                "fileAeskey+fileId+fileMd5+fileSize+filename",
            ),
            (
                "fileAesKey",
                "fileName",
                "fileAesKey+fileId+fileMd5+fileSize+fileName",
            ),
            (
                "fileAeskey",
                "fileName",
                "fileAeskey+fileId+fileMd5+fileSize+fileName",
            ),
        ] {
            let body = test_callback_body(aes_key_field, filename_field, &[]);
            let parsed = parse_single_async_upload_callback(&body)
                .expect("parse reviewed callback credential schema");

            assert_eq!(parsed.request_id, "callback-request-secret");
            assert_eq!(parsed.credentials.file_aes_key, "callback-aes-secret");
            assert_eq!(parsed.credentials.filename, "private-activity-poster.jpg");
            assert_eq!(parsed.credential_shape.schema_id, schema_id);
            assert_eq!(parsed.credential_shape.additional_field_count, 0);
        }
    }

    #[test]
    fn callback_parser_rejects_ambiguous_credential_spellings() {
        for body in [
            test_callback_body(
                "fileAesKey",
                "filename",
                &[("fileAeskey", Value::String("alias-secret".to_string()))],
            ),
            test_callback_body(
                "fileAesKey",
                "filename",
                &[(
                    "fileName",
                    Value::String("alias-private-poster.jpg".to_string()),
                )],
            ),
        ] {
            assert!(parse_single_async_upload_callback(&body).is_err());
        }
    }

    #[test]
    fn callback_shape_report_counts_additional_fields_without_leaking_input() {
        let body = test_callback_body(
            "fileAesKey",
            "filename",
            &[
                (
                    "providerOpaqueCredential",
                    Value::String("unknown-provider-secret".to_string()),
                ),
                (
                    "cloudUrl",
                    Value::String("https://private.example.test/file.jpg".to_string()),
                ),
            ],
        );
        let parsed = parse_single_async_upload_callback(&body)
            .expect("parse callback with additional provider fields");
        let report = callback_worker_report(
            WorkerReportState {
                success: true,
                dry_run: true,
                apply_requested: false,
                phase: "callback",
                action_status: "callback_preview".to_string(),
                work_item_id: None,
                external_upload_requested: false,
                callback_received: true,
                external_send_executed: None,
            },
            parsed.credential_shape,
        );
        let serialized = serde_json::to_string(&report).expect("serialize callback report");
        let value: Value = serde_json::from_str(&serialized).expect("parse callback report");

        assert_eq!(
            value["callback_credential_schema"],
            "fileAesKey+fileId+fileMd5+fileSize+filename"
        );
        assert_eq!(value["callback_additional_field_count"], 2);
        for sensitive in [
            "callback-request-secret",
            "callback-aes-secret",
            "callback-file-secret",
            "98e7c2acf4391f8b4a2bbd39e364c5e3",
            "48301",
            "private-activity-poster.jpg",
            "providerOpaqueCredential",
            "unknown-provider-secret",
            "cloudUrl",
            "https://private.example.test/file.jpg",
        ] {
            assert!(!serialized.contains(sensitive));
        }
    }

    #[test]
    fn callback_parse_errors_do_not_echo_malformed_values() {
        let invalid_credentials = test_callback_body(
            "fileAesKey",
            "filename",
            &[("fileSize", Value::String("private-size-secret".to_string()))],
        );
        let invalid_envelope = br#"{
          "code":0,
          "data":[{
            "requestId":"private-request-secret",
            "cmd":"private-command-secret",
            "msgData":{}
          }]
        }"#;

        for (body, sensitive) in [
            (invalid_credentials.as_slice(), "private-size-secret"),
            (invalid_envelope.as_slice(), "private-command-secret"),
        ] {
            let Err(error) = parse_single_async_upload_callback(body) else {
                panic!("malformed callback must fail closed");
            };
            assert!(!format!("{error:#}").contains(sensitive));
        }
    }

    #[test]
    fn single_callback_parser_rejects_inputs_before_send_gate() {
        let oversized = vec![b'x'; MAX_JSON_RESPONSE_BYTES + 1];
        for body in [
            b"".as_slice(),
            oversized.as_slice(),
            br#"{"code":0,"data":[{"requestId":"upload-request\nsecret","cmd":20000,"msgData":{}}]}"#,
            br#"{"code":0,"data":[{"requestId":"upload-request-1","cmd":20000}]}"#,
            br#"{"code":0,"data":[{"requestId":"upload-request-1","cmd":20000,"msgData":{"fileAesKey":"aes-key","fileId":"file-id","fileMd5":"98e7c2acf4391f8b4a2bbd39e364c5e3","fileSize":48300,"filename":"activity-poster.png"}}]}"#,
        ] {
            assert!(
                parse_single_async_upload_callback(body).is_err(),
                "accepted invalid single callback {:?}",
                String::from_utf8_lossy(body)
            );
        }
    }

    #[test]
    fn worker_reports_encode_send_boundary_states() {
        let upload_preview = worker_report(WorkerReportState {
            success: true,
            dry_run: true,
            apply_requested: false,
            phase: "upload",
            action_status: "image_upload_preview".to_string(),
            work_item_id: Some(Uuid::nil()),
            external_upload_requested: false,
            callback_received: false,
            external_send_executed: Some(false),
        });
        assert!(upload_preview.success);
        assert!(upload_preview.dry_run);
        assert_eq!(upload_preview.external_send_executed, Some(false));
        assert!(!upload_preview.safe_for_chat);

        let ambiguous_callback = worker_report(WorkerReportState {
            success: false,
            dry_run: false,
            apply_requested: true,
            phase: "callback",
            action_status: "image_send_ambiguous".to_string(),
            work_item_id: Some(Uuid::nil()),
            external_upload_requested: false,
            callback_received: true,
            external_send_executed: None,
        });
        assert!(!ambiguous_callback.success);
        assert!(ambiguous_callback.apply_requested);
        assert!(ambiguous_callback.callback_received);
        assert_eq!(ambiguous_callback.external_send_executed, None);
    }

    struct TestRequest {
        headers: String,
        body: Vec<u8>,
    }

    fn read_test_request(stream: &mut TcpStream) -> TestRequest {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let count = stream.read(&mut buffer).unwrap();
            assert_ne!(count, 0, "request ended before headers");
            request.extend_from_slice(&buffer[..count]);
        }
        let header_end = request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap()
            + 4;
        let headers = String::from_utf8(request[..header_end].to_vec()).unwrap();
        let content_length = headers
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .unwrap_or("0")
            .parse::<usize>()
            .unwrap();
        while request.len() < header_end + content_length {
            let count = stream.read(&mut buffer).unwrap();
            assert_ne!(count, 0, "request ended before body");
            request.extend_from_slice(&buffer[..count]);
        }
        TestRequest {
            headers,
            body: request[header_end..header_end + content_length].to_vec(),
        }
    }

    fn json_response(body: &str) -> Vec<u8> {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    }

    fn binary_response(body: &[u8]) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend_from_slice(body);
        response
    }

    fn http_json_response(status: u16, body: &[u8]) -> HttpResponse {
        HttpResponse {
            status,
            headers: std::collections::BTreeMap::new(),
            body: body.to_vec(),
        }
    }

    fn test_adapter_config(port: u16) -> AdapterConfig {
        AdapterConfig {
            api_url: Url::parse(&format!("http://127.0.0.1:{port}/qiwe/api/qw/doApi")).unwrap(),
            token: "fake-token".to_string(),
            guid: "fake-device-guid".to_string(),
            allowed_hosts: BTreeSet::from(["127.0.0.1".to_string()]),
            media_allowed_hosts: BTreeSet::from([
                "127.0.0.1".to_string(),
                "media.example.test".to_string(),
            ]),
            allowed_groups: BTreeSet::from(["group-id".to_string()]),
            webhook_ready: true,
        }
    }

    fn test_upload_claim() -> QiweUploadClaim {
        QiweUploadClaim {
            attempt_id: Uuid::new_v4(),
            work_item_id: Uuid::new_v4(),
            generated_image_artifact_id: Uuid::new_v4(),
            attempt_number: 1,
            claim_token: format!("qiwe-image-send-adapter:{}", Uuid::new_v4()),
            artifact_uri: "https://media.example.test/activity-poster.jpg".to_string(),
            artifact_content_hash:
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
            artifact_file_md5: "98e7c2acf4391f8b4a2bbd39e364c5e3".to_string(),
            artifact_byte_size: 48_300,
            filename: "activity-poster.jpg".to_string(),
            target_group_id: "group-id".to_string(),
        }
    }

    fn test_feishu_upload_claim(bytes: &[u8]) -> QiweUploadClaim {
        let artifact_id =
            Uuid::parse_str("53afddaf-53c3-4b57-a3b8-853d1c07d32f").expect("fixture UUID is valid");
        QiweUploadClaim {
            attempt_id: Uuid::new_v4(),
            work_item_id: Uuid::new_v4(),
            generated_image_artifact_id: artifact_id,
            attempt_number: 1,
            claim_token: format!("qiwe-image-send-adapter:{}", Uuid::new_v4()),
            artifact_uri: format!("{FEISHU_PRIMARY_STORAGE_URI_PREFIX}{artifact_id}"),
            artifact_content_hash: format!("sha256:{:x}", Sha256::digest(bytes)),
            artifact_file_md5: format!("{:x}", Md5::digest(bytes)),
            artifact_byte_size: u64::try_from(bytes.len()).expect("fixture size fits u64"),
            filename: format!("generated-image-{artifact_id}.jpg"),
            target_group_id: "group-id".to_string(),
        }
    }

    fn test_send_body() -> Vec<u8> {
        build_send_image_request(
            "fake-device-guid",
            "group-id",
            &QiweImageCredentials {
                file_aes_key: "fake-aes-secret".to_string(),
                file_id: "fake-file-secret".to_string(),
                file_md5: "98e7c2acf4391f8b4a2bbd39e364c5e3".to_string(),
                file_size: 48_300,
                filename: "activity-poster.jpg".to_string(),
            },
            &BTreeSet::from(["group-id".to_string()]),
        )
        .unwrap()
    }
}
