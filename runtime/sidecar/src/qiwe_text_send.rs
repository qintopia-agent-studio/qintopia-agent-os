#![cfg_attr(
    not(any(
        test,
        feature = "qiwe-staging-adapter",
        feature = "qiwe-production-adapter"
    )),
    allow(dead_code, unused_imports)
)]
#![cfg_attr(test, allow(dead_code))]

use std::collections::BTreeSet;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Row};
use url::Url;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
use crate::bounded_http::HttpClient;
use crate::{config::Cli, db};

const WORKER_ID: &str = "qiwe-text-send-worker";
const WORK_ITEM_TYPE: &str = "group_message_request";
const CAPABILITY_KEY: &str = "erhua.send_group_message";
const WORKFLOW_TYPE: &str = "text_activity_announcement";
const ARTIFACT_TYPE: &str = "text_announcement";
const SEND_METHOD: &str = "/msg/sendHyperText";
const MAX_SEND_ATTEMPTS: i32 = 3;
const MAX_JSON_RESPONSE_BYTES: usize = 64 * 1024;
#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
const PRODUCTION_APPROVAL_ENV: &str = "QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_APPROVAL";
#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
const PRODUCTION_APPROVAL_PHRASE: &str = "approved-production-qiwe-text-send";
#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
const DATABASE_URL_SHA256_ENV: &str = "QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_DATABASE_URL_SHA256";
#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
const ENABLE_ENV: &str = "QINTOPIA_QIWE_TEXT_SEND_ENABLED";

#[derive(Debug, Serialize)]
pub struct QiweTextSendWorkerReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub fixture_mode: bool,
    pub worker: &'static str,
    pub action_status: String,
    pub work_item_id: Option<Uuid>,
    pub current_status: String,
    pub target_group_id_present: bool,
    pub approved_artifact_id: Option<Uuid>,
    pub message_preview: String,
    pub external_send_executed: Option<bool>,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug)]
struct TextWorkItem {
    id: Uuid,
    status: String,
    review_policy: String,
    payload: Value,
    artifact_id: Uuid,
    artifact_content_hash: String,
}

#[derive(Debug)]
struct TextSendPlan {
    target_group_id: String,
    approved_artifact_id: Uuid,
    message_text: String,
    content_hash: String,
}

struct ReportStatus<'a> {
    dry_run: bool,
    apply_requested: bool,
    fixture_mode: bool,
    action_status: &'a str,
    work_item_id: Option<Uuid>,
    current_status: &'a str,
    external_send_executed: Option<bool>,
}

#[derive(Clone)]
struct AdapterConfig {
    #[cfg(any(
        test,
        feature = "qiwe-staging-adapter",
        feature = "qiwe-production-adapter"
    ))]
    api_url: Url,
    #[cfg(any(
        test,
        feature = "qiwe-staging-adapter",
        feature = "qiwe-production-adapter"
    ))]
    token: String,
    #[cfg(any(
        test,
        feature = "qiwe-staging-adapter",
        feature = "qiwe-production-adapter"
    ))]
    guid: String,
    allowed_groups: BTreeSet<String>,
}

impl Drop for AdapterConfig {
    fn drop(&mut self) {
        #[cfg(any(
            test,
            feature = "qiwe-staging-adapter",
            feature = "qiwe-production-adapter"
        ))]
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
struct SendTextParams<'a> {
    guid: &'a str,
    #[serde(rename = "toId")]
    to_id: &'a str,
    #[serde(rename = "isNoNeedRead")]
    is_no_need_read: bool,
    content: Vec<HyperTextSegment<'a>>,
}

#[derive(Serialize)]
struct HyperTextSegment<'a> {
    #[serde(rename = "type")]
    segment_type: &'static str,
    text: &'a str,
}

#[derive(Deserialize)]
struct ApiResponse {
    code: Option<i64>,
    data: Option<Value>,
    msg: Option<String>,
}

enum SendOutcome {
    Sent {
        response_summary: Value,
    },
    FailedBeforeSend {
        reason: &'static str,
    },
    Ambiguous {
        reason: &'static str,
        response_summary: Option<Value>,
    },
}

pub async fn run(
    cli: &Cli,
    once: bool,
    work_item_id: Option<Uuid>,
    apply: bool,
    dry_run: bool,
    fixture_mode: bool,
) -> Result<()> {
    if apply && dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let apply_requested = apply && !dry_run;
    if !once && !fixture_mode {
        bail!("run-qiwe-text-send-worker currently requires --once");
    }
    let report = if fixture_mode {
        run_fixture(apply_requested)?
    } else if apply_requested {
        run_apply(cli, work_item_id).await?
    } else {
        run_preview(cli, work_item_id).await?
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report.success {
        Ok(())
    } else {
        bail!("QiWe text send worker failed")
    }
}

fn run_fixture(apply_requested: bool) -> Result<QiweTextSendWorkerReport> {
    if apply_requested {
        bail!("fixture-mode cannot be used with --apply");
    }
    let work_item = TextWorkItem {
        id: Uuid::nil(),
        status: "queued".to_string(),
        review_policy: "human_final_confirmation".to_string(),
        payload: json!({
            "workflow_type": WORKFLOW_TYPE,
            "approved_artifact_id": "02dd5f47-81f8-4b8c-898d-b4c926fcf9b5",
            "approved_artifact_type": ARTIFACT_TYPE,
            "approved_artifact_content_hash": content_hash_for_text("早上好，二花早报来啦。"),
            "target_channel": "qiwe",
            "target_group_id": "fixture-group",
            "message_text": "早上好，二花早报来啦。",
            "external_send_executed": false
        }),
        artifact_id: Uuid::parse_str("02dd5f47-81f8-4b8c-898d-b4c926fcf9b5")?,
        artifact_content_hash: content_hash_for_text("早上好，二花早报来啦。"),
    };
    let config = AdapterConfig::fixture();
    let plan = validate_work_item(&work_item, &config)?;
    Ok(report_from_plan(
        ReportStatus {
            dry_run: true,
            apply_requested: false,
            fixture_mode: true,
            action_status: "fixture_dry_run_ok",
            work_item_id: Some(work_item.id),
            current_status: &work_item.status,
            external_send_executed: Some(false),
        },
        &plan,
    ))
}

async fn run_preview(cli: &Cli, work_item_id: Option<Uuid>) -> Result<QiweTextSendWorkerReport> {
    let config = AdapterConfig::from_cli(cli)?;
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let Some(work_item) = peek_work_item(&pool, work_item_id).await? else {
        return Ok(empty_report(
            false,
            false,
            "no_claimable_text_group_message_request",
        ));
    };
    let plan = validate_work_item(&work_item, &config)?;
    Ok(report_from_plan(
        ReportStatus {
            dry_run: true,
            apply_requested: false,
            fixture_mode: false,
            action_status: "dry_run_ok",
            work_item_id: Some(work_item.id),
            current_status: &work_item.status,
            external_send_executed: Some(false),
        },
        &plan,
    ))
}

async fn run_apply(cli: &Cli, work_item_id: Option<Uuid>) -> Result<QiweTextSendWorkerReport> {
    #[cfg(not(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter")))]
    {
        let _ = (cli, work_item_id);
        Ok(empty_report(false, true, "qiwe_text_adapter_not_compiled"))
    }

    #[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
    {
        let config = match AdapterConfig::from_env(cli) {
            Ok(config) => config,
            Err(_) => return Ok(empty_report(false, true, "boundary_not_approved")),
        };
        if !env_flag(ENABLE_ENV)? {
            return Ok(empty_report(false, true, "text_send_disabled"));
        }
        let database_url = cli.database_url_required()?;
        let pool = db::connect(database_url, cli.db_max_connections).await?;
        let mut tx = pool
            .begin()
            .await
            .context("begin QiWe text-send transaction")?;
        let Some(work_item) = lock_work_item(&mut tx, work_item_id).await? else {
            tx.commit().await.context("commit empty text-send claim")?;
            return Ok(empty_report(
                false,
                true,
                "no_claimable_text_group_message_request",
            ));
        };
        let plan = match validate_work_item(&work_item, &config) {
            Ok(plan) => plan,
            Err(err) => {
                record_failed(&mut tx, &work_item, "policy_denied", Some(&err.to_string())).await?;
                tx.commit()
                    .await
                    .context("commit text-send policy denial")?;
                return Ok(empty_report(false, true, "policy_denied"));
            }
        };
        tx.commit()
            .await
            .context("commit QiWe text-send claim before external call")?;

        let outcome = request_send_text_with(&config, &plan, &HttpClient::production());
        let mut tx = pool
            .begin()
            .await
            .context("begin QiWe text-send outcome transaction")?;
        let report = match outcome {
            SendOutcome::Sent { response_summary } => {
                record_sent(&mut tx, &work_item, &plan, response_summary).await?;
                report_from_plan(
                    ReportStatus {
                        dry_run: false,
                        apply_requested: true,
                        fixture_mode: false,
                        action_status: "text_send_executed",
                        work_item_id: Some(work_item.id),
                        current_status: "completed",
                        external_send_executed: Some(true),
                    },
                    &plan,
                )
            }
            SendOutcome::FailedBeforeSend { reason } => {
                record_failed(&mut tx, &work_item, reason, None).await?;
                report_from_plan(
                    ReportStatus {
                        dry_run: false,
                        apply_requested: true,
                        fixture_mode: false,
                        action_status: "text_send_failed_before_external_send",
                        work_item_id: Some(work_item.id),
                        current_status: "failed",
                        external_send_executed: Some(false),
                    },
                    &plan,
                )
            }
            SendOutcome::Ambiguous {
                reason,
                response_summary,
            } => {
                record_ambiguous(&mut tx, &work_item, reason, response_summary).await?;
                report_from_plan(
                    ReportStatus {
                        dry_run: false,
                        apply_requested: true,
                        fixture_mode: false,
                        action_status: "text_send_outcome_ambiguous",
                        work_item_id: Some(work_item.id),
                        current_status: "failed",
                        external_send_executed: None,
                    },
                    &plan,
                )
            }
        };
        tx.commit().await.context("commit QiWe text-send outcome")?;
        Ok(report)
    }
}

async fn peek_work_item(pool: &PgPool, work_item_id: Option<Uuid>) -> Result<Option<TextWorkItem>> {
    let query = text_work_item_query();
    let row = sqlx::query(&query)
        .bind(work_item_id)
        .fetch_optional(pool)
        .await
        .context("peek QiWe text-send work item")?;
    row.map(work_item_from_row).transpose()
}

async fn lock_work_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Option<Uuid>,
) -> Result<Option<TextWorkItem>> {
    let query = format!(
        r#"
        WITH claimable AS (
            SELECT
                request.id,
                artifact.id AS artifact_id,
                artifact.content_hash AS artifact_content_hash
            FROM qintopia_agent_os.work_items request
            JOIN qintopia_agent_os.artifacts artifact
              ON artifact.id::text = request.payload->>'approved_artifact_id'
            WHERE ($1::uuid IS NULL OR request.id = $1)
              AND request.status = 'queued'
              AND request.available_at <= now()
              AND request.attempts < {MAX_SEND_ATTEMPTS}
              AND request.work_item_type = '{WORK_ITEM_TYPE}'
              AND request.capability_key = '{CAPABILITY_KEY}'
              AND request.requester_agent = 'xiaoman'
              AND request.target_agent = 'erhua'
              AND request.review_policy = 'human_final_confirmation'
              AND request.payload->>'workflow_type' = '{WORKFLOW_TYPE}'
              AND request.payload->>'approved_artifact_type' = '{ARTIFACT_TYPE}'
              AND artifact.artifact_type = '{ARTIFACT_TYPE}'
              AND artifact.review_status = 'approved'
              AND EXISTS (
                  SELECT 1
                  FROM qintopia_agent_os.work_item_events confirmation
                  WHERE confirmation.work_item_id = request.id
                    AND confirmation.event_type = 'group_message_final_confirmation_recorded'
                    AND confirmation.data->>'decision' = 'confirmed'
                    AND confirmation.data->>'send_executed' = 'false'
              )
              AND EXISTS (
                  SELECT 1
                  FROM qintopia_agent_os.work_item_events ready
                  WHERE ready.work_item_id = request.id
                    AND ready.event_type = 'group_message_send_ready_recorded'
                    AND ready.data->>'send_executed' = 'false'
                    AND ready.data->>'target_group_id' = request.payload->>'target_group_id'
                    AND ready.data->>'approved_artifact_id' = request.payload->>'approved_artifact_id'
              )
              AND NOT EXISTS (
                  SELECT 1
                  FROM qintopia_agent_os.work_item_events sent
                  WHERE sent.work_item_id = request.id
                    AND sent.event_type IN ('qiwe_text_send_executed','qiwe_text_send_ambiguous')
              )
            ORDER BY request.priority DESC, request.available_at ASC, request.created_at ASC
            LIMIT 1
            FOR UPDATE OF request SKIP LOCKED
        )
        UPDATE qintopia_agent_os.work_items request
        SET status = 'processing',
            claimed_by = $2,
            locked_at = now(),
            claim_expires_at = now() + interval '2 minutes',
            attempts = attempts + 1,
            updated_at = now()
        FROM claimable
        WHERE request.id = claimable.id
        RETURNING request.id, request.status, request.review_policy, request.payload,
                  claimable.artifact_id AS artifact_id,
                  claimable.artifact_content_hash AS artifact_content_hash
        "#
    );
    let row = sqlx::query(&query)
        .bind(work_item_id)
        .bind(WORKER_ID)
        .fetch_optional(&mut **tx)
        .await
        .context("lock QiWe text-send work item")?;
    row.map(work_item_from_row).transpose()
}

fn text_work_item_query() -> String {
    format!(
        r#"
        SELECT request.id, request.status, request.review_policy, request.payload,
               artifact.id AS artifact_id, artifact.content_hash AS artifact_content_hash
        FROM qintopia_agent_os.work_items request
        JOIN qintopia_agent_os.artifacts artifact
          ON artifact.id::text = request.payload->>'approved_artifact_id'
        WHERE ($1::uuid IS NULL OR request.id = $1)
          AND request.status = 'queued'
          AND request.available_at <= now()
          AND request.attempts < {MAX_SEND_ATTEMPTS}
          AND request.work_item_type = '{WORK_ITEM_TYPE}'
          AND request.capability_key = '{CAPABILITY_KEY}'
          AND request.requester_agent = 'xiaoman'
          AND request.target_agent = 'erhua'
          AND request.review_policy = 'human_final_confirmation'
          AND request.payload->>'workflow_type' = '{WORKFLOW_TYPE}'
          AND request.payload->>'approved_artifact_type' = '{ARTIFACT_TYPE}'
          AND artifact.artifact_type = '{ARTIFACT_TYPE}'
          AND artifact.review_status = 'approved'
          AND EXISTS (
              SELECT 1
              FROM qintopia_agent_os.work_item_events confirmation
              WHERE confirmation.work_item_id = request.id
                AND confirmation.event_type = 'group_message_final_confirmation_recorded'
                AND confirmation.data->>'decision' = 'confirmed'
                AND confirmation.data->>'send_executed' = 'false'
          )
          AND EXISTS (
              SELECT 1
              FROM qintopia_agent_os.work_item_events ready
              WHERE ready.work_item_id = request.id
                AND ready.event_type = 'group_message_send_ready_recorded'
                AND ready.data->>'send_executed' = 'false'
                AND ready.data->>'target_group_id' = request.payload->>'target_group_id'
                AND ready.data->>'approved_artifact_id' = request.payload->>'approved_artifact_id'
          )
          AND NOT EXISTS (
              SELECT 1
              FROM qintopia_agent_os.work_item_events sent
              WHERE sent.work_item_id = request.id
                AND sent.event_type IN ('qiwe_text_send_executed','qiwe_text_send_ambiguous')
          )
        ORDER BY request.priority DESC, request.available_at ASC, request.created_at ASC
        LIMIT 1
        "#
    )
}

fn work_item_from_row(row: sqlx::postgres::PgRow) -> Result<TextWorkItem> {
    Ok(TextWorkItem {
        id: row.try_get("id")?,
        status: row.try_get("status")?,
        review_policy: row.try_get("review_policy")?,
        payload: row.try_get("payload")?,
        artifact_id: row.try_get("artifact_id")?,
        artifact_content_hash: row
            .try_get::<Option<String>, _>("artifact_content_hash")?
            .ok_or_else(|| anyhow!("text announcement artifact is missing content_hash"))?,
    })
}

fn validate_work_item(work_item: &TextWorkItem, config: &AdapterConfig) -> Result<TextSendPlan> {
    if work_item.status != "queued" && work_item.status != "processing" {
        bail!("QiWe text-send work item must be queued or processing");
    }
    if work_item.review_policy != "human_final_confirmation" {
        bail!("QiWe text-send requires human_final_confirmation");
    }
    if contains_sensitive_value(&work_item.payload) {
        bail!("QiWe text-send payload contains disallowed sensitive content");
    }
    require_text(&work_item.payload, "workflow_type", WORKFLOW_TYPE)?;
    require_text(&work_item.payload, "approved_artifact_type", ARTIFACT_TYPE)?;
    require_text(&work_item.payload, "target_channel", "qiwe")?;
    let approved_artifact_id = required_uuid(&work_item.payload, "approved_artifact_id")?;
    if approved_artifact_id != work_item.artifact_id {
        bail!("approved_artifact_id does not match locked text artifact");
    }
    let content_hash = required_string(&work_item.payload, "approved_artifact_content_hash")?;
    validate_canonical_sha256(&content_hash)?;
    if content_hash != work_item.artifact_content_hash {
        bail!("approved_artifact_content_hash does not match text artifact");
    }
    let message_text = required_string(&work_item.payload, "message_text")?;
    if content_hash_for_text(&message_text) != content_hash {
        bail!("message_text does not match approved text artifact");
    }
    let target_group_id = required_string(&work_item.payload, "target_group_id")?;
    if !config.allowed_groups.contains(&target_group_id) {
        bail!("target_group_id is not allowlisted for QiWe text-send");
    }
    Ok(TextSendPlan {
        target_group_id,
        approved_artifact_id,
        message_text,
        content_hash,
    })
}

#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
fn request_send_text_with(
    config: &AdapterConfig,
    plan: &TextSendPlan,
    client: &HttpClient,
) -> SendOutcome {
    let body =
        match build_send_text_request(&config.guid, &plan.target_group_id, &plan.message_text) {
            Ok(body) => Zeroizing::new(body),
            Err(_) => {
                return SendOutcome::FailedBeforeSend {
                    reason: "send_text_request_build",
                }
            }
        };
    let response = match client.request(
        "POST",
        &config.api_url,
        &[
            ("Content-Type", "application/json".to_string()),
            ("Accept", "application/json".to_string()),
            ("x-qiwei-token", config.token.clone()),
        ],
        &body,
        MAX_JSON_RESPONSE_BYTES,
    ) {
        Ok(response) => response,
        Err(error) => {
            if error.request_may_have_been_sent() {
                return SendOutcome::Ambiguous {
                    reason: "http_request_after_send",
                    response_summary: None,
                };
            }
            return SendOutcome::FailedBeforeSend {
                reason: "http_request_not_sent",
            };
        }
    };
    if !(200..300).contains(&response.status) {
        return SendOutcome::Ambiguous {
            reason: "http_status",
            response_summary: Some(json!({"status": response.status})),
        };
    }
    parse_send_text_response(&response.body)
}

#[cfg(not(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
)))]
fn request_send_text_with(
    _config: &AdapterConfig,
    _plan: &TextSendPlan,
    _client: &(),
) -> SendOutcome {
    SendOutcome::FailedBeforeSend {
        reason: "qiwe_text_adapter_not_compiled",
    }
}

fn build_send_text_request(
    guid: &str,
    target_group_id: &str,
    message_text: &str,
) -> Result<Vec<u8>> {
    let guid = guid.trim();
    let target_group_id = target_group_id.trim();
    let message_text = message_text.trim();
    if guid.is_empty() || target_group_id.is_empty() || message_text.is_empty() {
        bail!("QiWe text-send request requires guid, target_group_id, and message_text");
    }
    if contains_control(guid) || contains_control(target_group_id) || contains_control(message_text)
    {
        bail!("QiWe text-send request contains control characters");
    }
    Ok(serde_json::to_vec(&ApiRequest {
        method: SEND_METHOD,
        params: SendTextParams {
            guid,
            to_id: target_group_id,
            is_no_need_read: false,
            content: vec![HyperTextSegment {
                segment_type: "text",
                text: message_text,
            }],
        },
    })?)
}

fn parse_send_text_response(body: &[u8]) -> SendOutcome {
    if body.len() > MAX_JSON_RESPONSE_BYTES {
        return SendOutcome::Ambiguous {
            reason: "response_too_large",
            response_summary: None,
        };
    }
    let Ok(response) = serde_json::from_slice::<ApiResponse>(body) else {
        return SendOutcome::Ambiguous {
            reason: "invalid_json",
            response_summary: None,
        };
    };
    let code = response.code.unwrap_or(-1);
    let summary = safe_response_summary(code, response.msg.as_deref(), response.data.as_ref());
    if matches!(code, 0 | 200) {
        SendOutcome::Sent {
            response_summary: summary,
        }
    } else {
        SendOutcome::Ambiguous {
            reason: "business_response",
            response_summary: Some(summary),
        }
    }
}

async fn record_sent(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item: &TextWorkItem,
    plan: &TextSendPlan,
    response_summary: Value,
) -> Result<()> {
    update_terminal_work_item(tx, work_item, "completed", None, Some(true)).await?;
    append_event(
        tx,
        work_item.id,
        "qiwe_text_send_executed",
        "QiWe text message sent",
        json!({
            "workflow_type": WORKFLOW_TYPE,
            "target_channel": "qiwe",
            "target_group_id_sha256": sha256_hex(&plan.target_group_id),
            "approved_artifact_id": plan.approved_artifact_id,
            "approved_artifact_content_hash": plan.content_hash,
            "external_send_executed": true,
            "external_send_outcome": "sent",
            "message_preview": message_preview(&plan.message_text),
            "qiwe_response": response_summary,
        }),
    )
    .await
}

async fn record_failed(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item: &TextWorkItem,
    reason: &str,
    detail: Option<&str>,
) -> Result<()> {
    update_terminal_work_item(tx, work_item, "failed", Some(reason), Some(false)).await?;
    append_event(
        tx,
        work_item.id,
        "qiwe_text_send_failed",
        "QiWe text message rejected before external send",
        json!({
            "workflow_type": WORKFLOW_TYPE,
            "failure_code": reason,
            "failure_detail": detail.map(trim_error),
            "external_send_executed": false,
            "external_send_outcome": "not_sent",
        }),
    )
    .await
}

async fn record_ambiguous(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item: &TextWorkItem,
    reason: &str,
    response_summary: Option<Value>,
) -> Result<()> {
    update_terminal_work_item(tx, work_item, "failed", Some(reason), None).await?;
    append_event(
        tx,
        work_item.id,
        "qiwe_text_send_ambiguous",
        "QiWe text message outcome is ambiguous",
        json!({
            "workflow_type": WORKFLOW_TYPE,
            "failure_code": reason,
            "qiwe_response": response_summary,
            "external_send_executed": Value::Null,
            "external_send_outcome": "unknown",
            "automatic_retry_allowed": false,
        }),
    )
    .await
}

async fn update_terminal_work_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item: &TextWorkItem,
    status: &str,
    last_error: Option<&str>,
    external_send_executed: Option<bool>,
) -> Result<()> {
    let updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET status = $2,
            claimed_by = NULL,
            locked_at = NULL,
            claim_expires_at = NULL,
            last_error = $3,
            metadata = metadata || $4,
            updated_at = now()
        WHERE id = $1
          AND status = 'processing'
          AND claimed_by = $5
        "#,
    )
    .bind(work_item.id)
    .bind(status)
    .bind(last_error.map(trim_error))
    .bind(json!({
        "qiwe_text_send_worker": {
            "external_send_executed": external_send_executed,
            "worker_id": WORKER_ID
        }
    }))
    .bind(WORKER_ID)
    .execute(&mut **tx)
    .await
    .context("record QiWe text-send terminal work item state")?;
    if updated.rows_affected() != 1 {
        bail!("QiWe text-send claim changed before outcome was recorded");
    }
    Ok(())
}

async fn append_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Uuid,
    event_type: &str,
    message: &str,
    data: Value,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_item_events
            (work_item_id, event_type, actor_type, actor_id, message, data)
        VALUES ($1, $2, 'worker', $3, $4, $5)
        "#,
    )
    .bind(work_item_id)
    .bind(event_type)
    .bind(WORKER_ID)
    .bind(message)
    .bind(data)
    .execute(&mut **tx)
    .await
    .context("append QiWe text-send event")?;
    Ok(())
}

impl AdapterConfig {
    fn from_cli(cli: &Cli) -> Result<Self> {
        let allowed_groups = cli
            .operations_allowed_group_ids
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
            .collect::<BTreeSet<_>>();
        if allowed_groups.is_empty() {
            bail!("QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS is required for QiWe text-send");
        }
        Ok(Self {
            #[cfg(any(
                test,
                feature = "qiwe-staging-adapter",
                feature = "qiwe-production-adapter"
            ))]
            api_url: Url::parse("https://manager.qiweapi.com/qiwe/api/qw/doApi")?,
            #[cfg(any(
                test,
                feature = "qiwe-staging-adapter",
                feature = "qiwe-production-adapter"
            ))]
            token: String::new(),
            #[cfg(any(
                test,
                feature = "qiwe-staging-adapter",
                feature = "qiwe-production-adapter"
            ))]
            guid: String::new(),
            allowed_groups,
        })
    }

    fn fixture() -> Self {
        Self {
            #[cfg(any(
                test,
                feature = "qiwe-staging-adapter",
                feature = "qiwe-production-adapter"
            ))]
            api_url: Url::parse("http://127.0.0.1/qiwe/api/qw/doApi").expect("fixture URL parses"),
            #[cfg(any(
                test,
                feature = "qiwe-staging-adapter",
                feature = "qiwe-production-adapter"
            ))]
            token: "fixture-token".to_string(),
            #[cfg(any(
                test,
                feature = "qiwe-staging-adapter",
                feature = "qiwe-production-adapter"
            ))]
            guid: "fixture-guid".to_string(),
            allowed_groups: BTreeSet::from(["fixture-group".to_string()]),
        }
    }

    #[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
    fn from_env(cli: &Cli) -> Result<Self> {
        validate_production_owner_approval()?;
        validate_database_boundary(cli.database_url_required()?)?;
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
        let allowed_groups = parse_csv_set(&required_env("QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS")?);
        if allowed_groups.is_empty() {
            bail!("QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS is required");
        }
        Ok(Self {
            api_url,
            token: token.to_string(),
            guid: guid.to_string(),
            allowed_groups,
        })
    }
}

fn report_from_plan(status: ReportStatus<'_>, plan: &TextSendPlan) -> QiweTextSendWorkerReport {
    QiweTextSendWorkerReport {
        success: true,
        dry_run: status.dry_run,
        apply_requested: status.apply_requested,
        fixture_mode: status.fixture_mode,
        worker: WORKER_ID,
        action_status: status.action_status.to_string(),
        work_item_id: status.work_item_id,
        current_status: status.current_status.to_string(),
        target_group_id_present: true,
        approved_artifact_id: Some(plan.approved_artifact_id),
        message_preview: message_preview(&plan.message_text),
        external_send_executed: status.external_send_executed,
        limitations: limitations(),
        guardrails: guardrails(),
    }
}

fn empty_report(
    fixture_mode: bool,
    apply_requested: bool,
    action_status: &str,
) -> QiweTextSendWorkerReport {
    QiweTextSendWorkerReport {
        success: !matches!(
            action_status,
            "boundary_not_approved" | "qiwe_text_adapter_not_compiled" | "text_send_disabled"
        ),
        dry_run: !apply_requested,
        apply_requested,
        fixture_mode,
        worker: WORKER_ID,
        action_status: action_status.to_string(),
        work_item_id: None,
        current_status: "none".to_string(),
        target_group_id_present: false,
        approved_artifact_id: None,
        message_preview: String::new(),
        external_send_executed: Some(false),
        limitations: limitations(),
        guardrails: guardrails(),
    }
}

fn limitations() -> Vec<String> {
    vec![
        "only text_activity_announcement text_announcement artifacts are eligible".to_string(),
        "ambiguous QiWe outcomes are terminal and require manual reconciliation".to_string(),
    ]
}

fn guardrails() -> Vec<String> {
    vec![
        "requires approved text artifact, final confirmation, send-ready evidence, and target group allowlist".to_string(),
        "apply requires explicit QiWe text-send production approval and database URL hash".to_string(),
        "records external_send_executed=true only after a successful QiWe business response".to_string(),
    ]
}

fn required_string(payload: &Value, field: &str) -> Result<String> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("{field} is required"))
}

fn require_text(payload: &Value, field: &str, expected: &str) -> Result<()> {
    let value = required_string(payload, field)?;
    if value != expected {
        bail!("{field} must be {expected}");
    }
    Ok(())
}

fn required_uuid(payload: &Value, field: &str) -> Result<Uuid> {
    Uuid::parse_str(&required_string(payload, field)?)
        .with_context(|| format!("{field} must be a uuid"))
}

fn contains_sensitive_value(value: &Value) -> bool {
    let text = value.to_string().to_ascii_lowercase();
    [
        "postgres://",
        "postgresql://",
        "access_token",
        "api_key",
        "client_secret",
        "bearer ",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

fn validate_canonical_sha256(value: &str) -> Result<()> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        bail!("content hash must be canonical sha256");
    };
    if hex.len() != 64
        || !hex
            .chars()
            .all(|character| matches!(character, '0'..='9' | 'a'..='f'))
    {
        bail!("content hash must be canonical sha256");
    }
    Ok(())
}

fn content_hash_for_text(value: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(value.as_bytes()))
}

fn message_preview(value: &str) -> String {
    let mut preview = value.trim().replace('\n', " ");
    if preview.chars().count() > 80 {
        preview = preview.chars().take(79).collect();
        preview.push('…');
    }
    preview
}

fn trim_error(value: &str) -> String {
    let mut text = value.replace('\n', " ");
    if text.chars().count() > 220 {
        text = text.chars().take(220).collect();
    }
    text
}

fn safe_response_summary(code: i64, msg: Option<&str>, data: Option<&Value>) -> Value {
    let message = data
        .and_then(|data| data.get("msgUniqueIdentifier"))
        .and_then(Value::as_str);
    json!({
        "qiwe_code": code,
        "qiwe_msg": msg.unwrap_or_default(),
        "message_id_present": message.map(|value| !value.is_empty()).unwrap_or(false),
    })
}

fn sha256_hex(value: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(value.as_bytes()))
}

fn database_hash_matches(database_url: &str, expected: &str) -> bool {
    let actual = sha256_hex(database_url);
    expected == actual || actual.strip_prefix("sha256:") == Some(expected)
}

fn contains_control(value: &str) -> bool {
    value
        .chars()
        .any(|character| character.is_control() && character != '\n' && character != '\t')
}

fn env_flag(name: &str) -> Result<bool> {
    match std::env::var(name).unwrap_or_default().trim() {
        "" | "0" | "false" | "FALSE" => Ok(false),
        "1" | "true" | "TRUE" => Ok(true),
        _ => bail!("{name} must be 0 or 1"),
    }
}

fn required_env(name: &str) -> Result<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && !value.starts_with('<'))
        .ok_or_else(|| anyhow!("{name} is required"))
}

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
fn validate_production_owner_approval() -> Result<()> {
    if std::env::var(PRODUCTION_APPROVAL_ENV).unwrap_or_default() != PRODUCTION_APPROVAL_PHRASE {
        bail!("QiWe text-send production owner approval is required");
    }
    Ok(())
}

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
fn validate_database_boundary(database_url: &str) -> Result<()> {
    let expected = required_env(DATABASE_URL_SHA256_ENV)?;
    if !database_hash_matches(database_url, &expected) {
        bail!("QiWe text-send database URL hash does not match owner-approved boundary");
    }
    Ok(())
}

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
fn strict_api_url(value: &str) -> Result<Url> {
    let url = Url::parse(value).context("parse QiWe API URL")?;
    if url.scheme() != "https" || url.username() != "" || url.password().is_some() {
        bail!("QiWe API URL must be HTTPS without credentials");
    }
    if url.query().is_some() || url.fragment().is_some() || url.path() != "/qiwe/api/qw/doApi" {
        bail!("QiWe API URL does not match reviewed doApi endpoint");
    }
    Ok(url)
}

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
fn parse_csv_set(value: &str) -> BTreeSet<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
fn validate_header_value(value: &str) -> Result<()> {
    if value.is_empty() || value.chars().any(char::is_control) {
        bail!("HTTP header value is invalid");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_send_text_request_uses_hypertext_group_shape() {
        let body = build_send_text_request("guid-1", "group-1", "早上好，二花早报来啦。")
            .expect("request builds");
        let value: Value = serde_json::from_slice(&body).expect("request parses");
        assert_eq!(value["method"], SEND_METHOD);
        assert_eq!(value["params"]["guid"], "guid-1");
        assert_eq!(value["params"]["toId"], "group-1");
        assert_eq!(value["params"]["content"][0]["type"], "text");
        assert_eq!(
            value["params"]["content"][0]["text"],
            "早上好，二花早报来啦。"
        );
    }

    #[test]
    fn validate_work_item_requires_text_artifact_hash_binding() {
        let mut work_item = TextWorkItem {
            id: Uuid::nil(),
            status: "queued".to_string(),
            review_policy: "human_final_confirmation".to_string(),
            payload: json!({
                "workflow_type": WORKFLOW_TYPE,
                "approved_artifact_id": "02dd5f47-81f8-4b8c-898d-b4c926fcf9b5",
                "approved_artifact_type": ARTIFACT_TYPE,
                "approved_artifact_content_hash": content_hash_for_text("早报文本"),
                "target_channel": "qiwe",
                "target_group_id": "group-1",
                "message_text": "早报文本"
            }),
            artifact_id: Uuid::parse_str("02dd5f47-81f8-4b8c-898d-b4c926fcf9b5").unwrap(),
            artifact_content_hash: content_hash_for_text("早报文本"),
        };
        let config = AdapterConfig {
            api_url: Url::parse("http://127.0.0.1/qiwe/api/qw/doApi").unwrap(),
            token: "token".to_string(),
            guid: "guid".to_string(),
            allowed_groups: BTreeSet::from(["group-1".to_string()]),
        };

        validate_work_item(&work_item, &config).expect("valid text send request passes");
        work_item.payload["message_text"] = json!("替换文本");
        assert!(validate_work_item(&work_item, &config).is_err());
    }

    #[test]
    fn parse_send_text_response_requires_success_code() {
        match parse_send_text_response(br#"{"code":0,"data":{"msgUniqueIdentifier":"m1"}}"#) {
            SendOutcome::Sent { response_summary } => {
                assert_eq!(response_summary["message_id_present"], true);
            }
            _ => panic!("code 0 should be sent"),
        }
        match parse_send_text_response(br#"{"code":500,"msg":"maybe failed"}"#) {
            SendOutcome::Ambiguous { .. } => {}
            _ => panic!("non-success code should be ambiguous"),
        }
    }

    #[test]
    fn disabled_apply_report_is_not_successful() {
        let report = empty_report(false, true, "text_send_disabled");
        assert!(!report.success);
        assert_eq!(report.external_send_executed, Some(false));
    }

    #[test]
    fn database_hash_boundary_accepts_prefixed_or_bare_sha256() {
        let database_url = "postgres://user:pass@example.invalid/qintopia";
        let prefixed = sha256_hex(database_url);
        let bare = prefixed.strip_prefix("sha256:").unwrap();

        assert!(database_hash_matches(database_url, &prefixed));
        assert!(database_hash_matches(database_url, bare));
        assert!(!database_hash_matches(
            database_url,
            "0".repeat(64).as_str()
        ));
    }

    #[test]
    fn previews_truncate_at_utf8_character_boundaries() {
        let long_message = "二花早报".repeat(40);
        let preview = message_preview(&long_message);
        assert!(preview.ends_with('…'));
        assert_eq!(preview.chars().count(), 80);

        let long_error = "发送失败".repeat(80);
        assert_eq!(trim_error(&long_error).chars().count(), 220);
    }
}
