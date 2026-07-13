use std::collections::BTreeSet;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

const WORKER_ID: &str = "qiwe-image-send-adapter";
const ASYNC_UPLOAD_METHOD: &str = "/cloud/cdnUploadByUrlAsync";
const SEND_IMAGE_METHOD: &str = "/msg/sendImage";
const IMAGE_FILE_TYPE: u8 = 1;
const ASYNC_EVENT_COMMAND: i64 = 20_000;
const SEND_SUCCESS_VALUE: i64 = 1;
const MAX_JSON_RESPONSE_BYTES: usize = 64 * 1024;

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
    pub protocol: &'static str,
    pub safe_for_chat: bool,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

struct AdapterConfig {
    _api_url: Url,
    _token: String,
    _guid: String,
    allowed_hosts: BTreeSet<String>,
    media_allowed_hosts: BTreeSet<String>,
    allowed_groups: BTreeSet<String>,
    webhook_ready: bool,
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
        ),
        Err(_) => preflight_report(false, send_enabled, false, 0, 0, 0),
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report.success {
        Ok(())
    } else {
        bail!("QiWe image send adapter preflight configuration is invalid")
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
) -> QiweImageSendPreflightReport {
    QiweImageSendPreflightReport {
        success: config_valid,
        worker: WORKER_ID,
        action_status: if config_valid {
            "adapter_contract_ready"
        } else {
            "adapter_not_configured"
        },
        send_enabled,
        config_valid,
        webhook_ready,
        allowed_host_count,
        media_allowed_host_count,
        allowed_group_count,
        protocol: "qiwe_async_url_upload_then_send_image",
        safe_for_chat: false,
        limitations: vec![
            "this preflight validates local configuration only; it does not contact QiWe, upload media, or send a message".to_string(),
            "the official async upload callback must provide complete file credentials before a send request can be built".to_string(),
            "the official send-image documentation names JPG as the supported format; the current generated-image artifact is PNG and requires a separately reviewed compatibility decision".to_string(),
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

        let allowed_groups = parse_csv_set(&required_env("QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS")?);
        if allowed_groups.is_empty() {
            bail!("at least one QiWe target group must be allowlisted");
        }
        let webhook_ready = env_flag("QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY")?;
        if !webhook_ready {
            bail!("QiWe async upload webhook must be reviewed and ready");
        }

        Ok(Self {
            _api_url: api_url,
            _token: token,
            _guid: guid,
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
    if !allowed_group_ids.contains(&target_group_id.to_ascii_lowercase()) {
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
    if value.is_empty() || value.contains(['\r', '\n']) {
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
    if value.trim().is_empty() || value.contains(['\r', '\n']) {
        bail!("{label} is invalid");
    }
    Ok(())
}

fn validate_jpeg_filename(filename: &str) -> Result<()> {
    validate_plain_value(filename, "QiWe image filename")?;
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

fn validate_json_body_size(body: &[u8]) -> Result<()> {
    if body.is_empty() || body.len() > MAX_JSON_RESPONSE_BYTES {
        bail!("QiWe JSON response size is invalid");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
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
    fn headers_and_json_bodies_are_bounded() {
        assert!(validate_header_value("token\r\nInjected: true").is_err());
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
        let report = preflight_report(true, false, true, 1, 1, 1);
        let output = serde_json::to_string(&report).expect("serialize preflight report");

        assert!(report.success);
        assert!(!report.send_enabled);
        assert!(!report.safe_for_chat);
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
}
