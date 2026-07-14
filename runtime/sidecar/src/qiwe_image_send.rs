use std::{
    collections::BTreeSet,
    io::{self, Read},
};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::{
    bounded_http::{HttpClient, HttpResponse},
    config::Cli,
    db,
    qiwe_image_send_state::{
        self, CallbackClaimOutcome, QiweUploadClaim, SendFailureDisposition,
        UploadFailureDisposition,
    },
};
use url::Url;

const WORKER_ID: &str = "qiwe-image-send-adapter";
const ASYNC_UPLOAD_METHOD: &str = "/cloud/cdnUploadByUrlAsync";
const SEND_IMAGE_METHOD: &str = "/msg/sendImage";
const IMAGE_FILE_TYPE: u8 = 1;
const ASYNC_EVENT_COMMAND: i64 = 20_000;
const SEND_SUCCESS_VALUE: i64 = 1;
const MAX_JSON_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_CALLBACK_INPUT_BYTES: usize = 64 * 1024;
const REQUIRED_QIWE_IMAGE_SEND_CONFIGURATION: &[&str] = &[
    "QIWE_API_URL",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS",
];

#[derive(Debug, Serialize)]
pub struct QiweImageSendPreflightReport {
    pub success: bool,
    pub worker: &'static str,
    pub action_status: &'static str,
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
pub struct QiweImageSendWorkerReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub worker: &'static str,
    pub phase: &'static str,
    pub action_status: String,
    pub work_item_id: Option<Uuid>,
    pub external_upload_requested: bool,
    pub callback_received: bool,
    pub external_send_executed: Option<bool>,
    pub safe_for_chat: bool,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Clone)]
struct AdapterConfig {
    api_url: Url,
    token: String,
    guid: String,
    allowed_hosts: BTreeSet<String>,
    media_allowed_hosts: BTreeSet<String>,
    allowed_groups: BTreeSet<String>,
    webhook_ready: bool,
}

impl Drop for AdapterConfig {
    fn drop(&mut self) {
        self.token.zeroize();
        self.guid.zeroize();
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
    credentials: QiweImageCredentials,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UploadCallFailure {
    Rejected,
    OutcomeUnknown,
}

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

pub fn run_preflight() -> Result<()> {
    validate_contract()?;
    let send_enabled = env_flag("QINTOPIA_QIWE_IMAGE_SEND_ENABLED")?;
    let report = match AdapterConfig::from_env() {
        Ok(config) => preflight_report(
            true,
            send_enabled,
            config.webhook_ready,
            config.allowed_hosts.len(),
            config.media_allowed_hosts.len(),
            config.allowed_groups.len(),
            Vec::new(),
        ),
        Err(_) => preflight_report(
            false,
            send_enabled,
            false,
            0,
            0,
            0,
            missing_qiwe_image_send_configuration(),
        ),
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report.success {
        Ok(())
    } else {
        bail!("QiWe image send adapter preflight configuration is invalid")
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
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let apply_requested = apply && !dry_run;
    if !apply_requested {
        let preview = qiwe_image_send_state::preview_ready_work_item(&pool, work_item_id).await?;
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

    if !env_flag("QINTOPIA_QIWE_IMAGE_SEND_ENABLED")? {
        let preview = qiwe_image_send_state::preview_ready_work_item(&pool, work_item_id).await?;
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
    let config = match AdapterConfig::from_env() {
        Ok(config) => config,
        Err(_) => {
            let report = worker_report(WorkerReportState {
                success: false,
                dry_run: false,
                apply_requested: true,
                phase: "upload",
                action_status: "adapter_not_configured".to_string(),
                work_item_id,
                external_upload_requested: false,
                callback_received: false,
                external_send_executed: Some(false),
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
            bail!("QiWe image-send worker configuration is invalid");
        }
    };
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
    let worker_config = config.clone();
    let worker_claim = claim.clone();
    let upload = tokio::task::spawn_blocking(move || {
        request_async_upload_with(&worker_config, &worker_claim, &HttpClient::production())
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
                let report = worker_report(WorkerReportState {
                    success: false,
                    dry_run: false,
                    apply_requested: true,
                    phase: "upload",
                    action_status: "upload_state_persistence_failed".to_string(),
                    work_item_id: Some(claim_id),
                    external_upload_requested: true,
                    callback_received: false,
                    external_send_executed: Some(false),
                });
                println!("{}", serde_json::to_string_pretty(&report)?);
                bail!("QiWe upload acceptance could not be persisted");
            }
            let report = worker_report(WorkerReportState {
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
    let callback = read_callback_stdin()?;
    let parsed = parse_single_async_upload_callback(&callback)?;
    let apply_requested = apply && !dry_run;
    if !apply_requested {
        let report = worker_report(WorkerReportState {
            success: true,
            dry_run: true,
            apply_requested: false,
            phase: "callback",
            action_status: "callback_preview".to_string(),
            work_item_id: None,
            external_upload_requested: false,
            callback_received: true,
            external_send_executed: None,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    if !env_flag("QINTOPIA_QIWE_IMAGE_SEND_ENABLED")? {
        let report = worker_report(WorkerReportState {
            success: true,
            dry_run: false,
            apply_requested: true,
            phase: "callback",
            action_status: "image_send_disabled".to_string(),
            work_item_id: None,
            external_upload_requested: false,
            callback_received: true,
            external_send_executed: Some(false),
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    let config = match AdapterConfig::from_env() {
        Ok(config) => config,
        Err(_) => {
            let report = worker_report(WorkerReportState {
                success: false,
                dry_run: false,
                apply_requested: true,
                phase: "callback",
                action_status: "adapter_not_configured".to_string(),
                work_item_id: None,
                external_upload_requested: false,
                callback_received: true,
                external_send_executed: Some(false),
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
            bail!("QiWe image-send callback configuration is invalid");
        }
    };
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let outcome =
        qiwe_image_send_state::claim_callback_for_send(&pool, &parsed.request_id, &callback)
            .await?;
    let send_claim = match outcome {
        CallbackClaimOutcome::Duplicate { status } => {
            let report = worker_report(WorkerReportState {
                success: true,
                dry_run: false,
                apply_requested: true,
                phase: "callback",
                action_status: format!("callback_duplicate_{status}"),
                work_item_id: None,
                external_upload_requested: false,
                callback_received: true,
                external_send_executed: None,
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(());
        }
        CallbackClaimOutcome::Expired => {
            let report = worker_report(WorkerReportState {
                success: true,
                dry_run: false,
                apply_requested: true,
                phase: "callback",
                action_status: "callback_expired".to_string(),
                work_item_id: None,
                external_upload_requested: false,
                callback_received: true,
                external_send_executed: Some(false),
            });
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
            let report = worker_report(WorkerReportState {
                success: false,
                dry_run: false,
                apply_requested: true,
                phase: "callback",
                action_status: "send_request_rejected".to_string(),
                work_item_id: Some(work_item_id),
                external_upload_requested: false,
                callback_received: true,
                external_send_executed: Some(false),
            });
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
            let report = worker_report(WorkerReportState {
                success: true,
                dry_run: false,
                apply_requested: true,
                phase: "callback",
                action_status: "image_send_completed".to_string(),
                work_item_id: Some(work_item_id),
                external_upload_requested: false,
                callback_received: true,
                external_send_executed: Some(true),
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Err(failure) => {
            let disposition = match failure {
                SendCallFailure::NotSent => SendFailureDisposition::Rejected,
                SendCallFailure::Ambiguous => SendFailureDisposition::Ambiguous,
            };
            qiwe_image_send_state::record_send_failure(&pool, &send_claim, disposition).await?;
            let report = worker_report(WorkerReportState {
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
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }
    Ok(())
}

fn request_async_upload_with(
    config: &AdapterConfig,
    claim: &QiweUploadClaim,
    client: &HttpClient,
) -> std::result::Result<Zeroizing<String>, UploadCallFailure> {
    let body = build_async_upload_request(
        &config.guid,
        &claim.filename,
        &claim.artifact_uri,
        "image/jpeg",
        &config.media_allowed_hosts,
    )
    .map_err(|_| UploadCallFailure::Rejected)?;
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
    let envelope: CallbackEnvelope =
        serde_json::from_slice(body).context("parse QiWe async upload callback")?;
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
    let credentials: QiweImageCredentials = serde_json::from_value(
        event
            .msg_data
            .context("QiWe async upload callback is missing msgData")?,
    )
    .context("QiWe async upload callback is missing file credentials")?;
    credentials.validate()?;
    Ok(ParsedCallback {
        request_id: event.request_id,
        credentials,
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
        external_upload_requested: state.external_upload_requested,
        callback_received: state.callback_received,
        external_send_executed: state.external_send_executed,
        safe_for_chat: false,
        limitations: vec![
            "the upload worker and callback processor each handle one state transition and are not production scheduled".to_string(),
            "callback credentials remain memory-only and cannot be retried after the sending gate".to_string(),
        ],
        guardrails: vec![
            "Postgres remains the system source of truth".to_string(),
            "tokens, device ids, group ids, media URLs, request ids, callback credentials, response bodies, and message ids are excluded from reports".to_string(),
            "no Feishu writeback or unrelated external adapter is called".to_string(),
        ],
    }
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

fn preflight_report(
    config_valid: bool,
    send_enabled: bool,
    webhook_ready: bool,
    allowed_host_count: usize,
    media_allowed_host_count: usize,
    allowed_group_count: usize,
    missing_configuration: Vec<&'static str>,
) -> QiweImageSendPreflightReport {
    let success = config_valid && !send_enabled;
    QiweImageSendPreflightReport {
        success,
        worker: WORKER_ID,
        action_status: if !config_valid {
            "adapter_not_configured"
        } else if send_enabled {
            "adapter_enablement_not_approved"
        } else {
            "adapter_contract_ready"
        },
        send_enabled,
        config_valid,
        webhook_ready,
        allowed_host_count,
        media_allowed_host_count,
        allowed_group_count,
        missing_configuration,
        protocol: "qiwe_async_url_upload_then_send_image",
        safe_for_chat: false,
        limitations: vec![
            "this preflight validates local configuration only; it does not contact QiWe, upload media, or send a message".to_string(),
            "the official async upload callback must provide complete file credentials before a send request can be built".to_string(),
            "the generated-image contract requires the deterministic final JPEG; owner-approved staging must still verify isolated media upload and same-byte readback".to_string(),
        ],
        guardrails: vec![
            "the adapter remains disabled unless its explicit enable flag and separate staging approval exist".to_string(),
            "tokens, device ids, group ids, media URLs, file credentials, and message identifiers are not emitted".to_string(),
            "no timer, production runtime configuration, Feishu writeback, or external send is installed by this contract".to_string(),
        ],
    }
}

impl AdapterConfig {
    fn from_env() -> Result<Self> {
        let api_url = strict_api_url(&required_env("QIWE_API_URL")?)?;
        let token = required_env("QIWE_TOKEN")?;
        let guid = required_env("QIWE_GUID")?;
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
        let webhook_ready = env_flag("QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY")?;
        if !webhook_ready {
            bail!("QiWe async upload webhook must be reviewed and ready");
        }

        Ok(Self {
            api_url,
            token,
            guid,
            allowed_hosts,
            media_allowed_hosts,
            allowed_groups,
            webhook_ready,
        })
    }
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
        validate_plain_value(&self.file_md5, "QiWe file MD5")?;
        validate_jpeg_filename(&self.filename)?;
        if self.file_size == 0 {
            bail!("QiWe file size must be positive");
        }
        Ok(())
    }
}

fn required_env(name: &str) -> Result<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && !is_placeholder(value))
        .ok_or_else(|| anyhow!("required QiWe image-send configuration is missing"))
}

fn missing_qiwe_image_send_configuration() -> Vec<&'static str> {
    missing_required_configuration_with(REQUIRED_QIWE_IMAGE_SEND_CONFIGURATION, |name| {
        std::env::var(name).ok()
    })
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

    use super::*;

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
    fn preflight_report_never_exposes_configuration_values() {
        let report = preflight_report(true, false, true, 1, 1, 1, Vec::new());
        let output = serde_json::to_string(&report).expect("serialize preflight report");

        assert!(report.success);
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
    fn enabled_preflight_fails_closed() {
        let report = preflight_report(true, true, true, 1, 1, 1, Vec::new());

        assert!(!report.success);
        assert!(report.config_valid);
        assert!(report.send_enabled);
        assert_eq!(report.action_status, "adapter_enablement_not_approved");
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

    fn test_adapter_config(port: u16) -> AdapterConfig {
        AdapterConfig {
            api_url: Url::parse(&format!("http://127.0.0.1:{port}/qiwe/api/qw/doApi")).unwrap(),
            token: "fake-token".to_string(),
            guid: "fake-device-guid".to_string(),
            allowed_hosts: BTreeSet::from(["127.0.0.1".to_string()]),
            media_allowed_hosts: BTreeSet::from(["media.example.test".to_string()]),
            allowed_groups: BTreeSet::from(["group-id".to_string()]),
            webhook_ready: true,
        }
    }

    fn test_upload_claim() -> QiweUploadClaim {
        QiweUploadClaim {
            work_item_id: Uuid::new_v4(),
            generated_image_artifact_id: Uuid::new_v4(),
            attempt_number: 1,
            claim_token: format!("qiwe-image-send-adapter:{}", Uuid::new_v4()),
            artifact_uri: "https://media.example.test/activity-poster.jpg".to_string(),
            artifact_content_hash:
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
            filename: "activity-poster.jpg".to_string(),
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
