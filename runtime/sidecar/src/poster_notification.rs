#[cfg(feature = "xiaoman-feishu-poster-adapter")]
use std::os::unix::fs::PermissionsExt;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    fs,
    io::{self, Read},
    os::unix::fs::FileTypeExt,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use base64ct::{Base64, Encoding};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Row};
#[cfg(feature = "xiaoman-feishu-poster-adapter")]
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    time::timeout,
};
use uuid::Uuid;

use crate::{
    config::Cli,
    db,
    operations::{
        self, ArtifactReviewDecisionRequest, OperationsPolicy, WorkItemCreateReport,
        WorkItemCreateRequest,
    },
};

const MAX_CALLBACK_BYTES: u64 = 64 * 1024;
const CALLBACK_KEY_ENV: &str = "QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY";
const POSTER_REVIEW_CALLBACK_KIND: &str = "xiaoman_poster_review";
const MAX_CALLBACK_CLOCK_SKEW_SECONDS: i64 = 300;
#[cfg(feature = "xiaoman-feishu-poster-adapter")]
const CALLBACK_IO_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(2);
const REVIEW_CALLBACK_TARGET_QUERY: &str = r#"
    SELECT notification.generated_image_artifact_id,
           notification.status,
           target.conversation_id,
           target.requester_user_id,
           artifact.review_status
    FROM qintopia_agent_os.poster_notifications notification
    JOIN qintopia_agent_os.poster_return_targets target
      ON target.origin_ref = notification.origin_ref
    JOIN qintopia_agent_os.artifacts artifact
      ON artifact.id = notification.generated_image_artifact_id
    WHERE notification.id = $1
      AND notification.notification_kind = 'image_ready'
    "#;

#[derive(Debug)]
struct Candidate {
    workflow_root_id: Uuid,
    image_work_item_id: Uuid,
    artifact_id: Option<Uuid>,
    artifact_hash: Option<String>,
    notification_kind: String,
    failure_code: Option<String>,
    origin_ref: String,
    human_owner: String,
    priority: String,
}

#[derive(Debug, Serialize)]
pub struct StarterReport {
    success: bool,
    dry_run: bool,
    apply_requested: bool,
    action_status: String,
    scanned_count: usize,
    created_count: usize,
    existing_count: usize,
    work_items: Vec<WorkItemCreateReport>,
    external_send_executed: bool,
    guardrails: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ReviewCallback {
    callback_event_id: String,
    notification_id: Uuid,
    artifact_id: Uuid,
    conversation_id: String,
    actor_user_id: String,
    action: String,
}

#[derive(Debug, Deserialize)]
struct SignedCallbackEnvelope {
    timestamp: String,
    nonce: String,
    signature: String,
    body_base64: String,
}

#[derive(Debug, Serialize)]
struct ReviewCallbackReport {
    success: bool,
    action_status: String,
    decision: String,
    notification_id: Uuid,
    artifact_id: Uuid,
    deduped: bool,
    group_send_authorized: bool,
    external_send_executed: bool,
}

#[cfg(feature = "xiaoman-feishu-poster-adapter")]
struct CallbackSocketGuard(PathBuf);

#[cfg(feature = "xiaoman-feishu-poster-adapter")]
impl Drop for CallbackSocketGuard {
    fn drop(&mut self) {
        if path_is_socket(&self.0) {
            let _ = fs::remove_file(&self.0);
        }
    }
}

pub async fn run_starter(
    cli: &Cli,
    check_only: bool,
    once: bool,
    apply: bool,
    batch_size: i64,
    work_item_id: Option<Uuid>,
) -> Result<()> {
    if check_only && apply {
        bail!("notification starter cannot combine --check-only and --apply");
    }
    if !once && !check_only {
        bail!("notification starter requires --once or --check-only");
    }
    let pool = db::connect(cli.database_url_required()?, cli.db_max_connections).await?;
    let policy = if apply {
        OperationsPolicy::from_cli(cli, true)
    } else {
        OperationsPolicy::dry_run()
    };
    let report = run_starter_batch(
        &pool,
        check_only,
        apply && !check_only,
        batch_size,
        work_item_id,
        &policy,
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

async fn run_starter_batch(
    pool: &PgPool,
    _check_only: bool,
    apply_requested: bool,
    batch_size: i64,
    work_item_id: Option<Uuid>,
    policy: &OperationsPolicy,
) -> Result<StarterReport> {
    let candidates = load_candidates(pool, work_item_id, batch_size).await?;
    let scanned_count = candidates.len();
    let mut work_items = Vec::with_capacity(scanned_count);
    let mut created_count = 0usize;
    let mut existing_count = 0usize;
    for candidate in candidates {
        let request = notification_request(&candidate)?;
        let report = if apply_requested {
            operations::create_work_item(pool, request, true, policy).await?
        } else {
            operations::create_work_item_dry_run(request)?
        };
        if report.existing {
            existing_count += 1;
        } else if apply_requested {
            created_count += 1;
        }
        if apply_requested {
            let notification_work_item_id = report
                .work_item_id
                .context("notification work item id is missing")?;
            upsert_notification(pool, &candidate, notification_work_item_id).await?;
        }
        work_items.push(report);
    }
    let action_status = if scanned_count == 0 {
        "no_pending_generated_image"
    } else if !apply_requested {
        "dry_run_ok"
    } else if created_count == 0 {
        "idempotent_existing"
    } else {
        "notification_work_created"
    };
    Ok(StarterReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        action_status: action_status.to_string(),
        scanned_count,
        created_count,
        existing_count,
        work_items,
        external_send_executed: false,
        guardrails: vec![
            "starter creates durable notification work only".to_string(),
            "generated image review remains pending".to_string(),
            "notification work never authorizes group send".to_string(),
        ],
    })
}

#[cfg(all(test, feature = "postgres-integration-tests"))]
pub(crate) async fn run_starter_for_postgres_integration(
    pool: &PgPool,
    image_work_item_id: Uuid,
) -> Result<()> {
    let report = run_starter_batch(
        pool,
        false,
        true,
        1,
        Some(image_work_item_id),
        &OperationsPolicy::dry_run(),
    )
    .await?;
    if report.scanned_count > 1 || report.created_count > 1 {
        bail!("poster integration notification starter created duplicate work");
    }
    Ok(())
}

async fn load_candidates(
    pool: &PgPool,
    work_item_id: Option<Uuid>,
    batch_size: i64,
) -> Result<Vec<Candidate>> {
    let rows = sqlx::query(
        r#"
        SELECT * FROM (
        SELECT root.id AS workflow_root_id,
               image_request.id AS image_work_item_id,
               artifact.id AS artifact_id,
               artifact.content_hash AS artifact_hash,
               'image_ready'::text AS notification_kind,
               NULL::text AS failure_code,
               root.metadata #>> '{workflow_metadata,origin_conversation_ref}' AS origin_ref,
               root.human_owner,
               root.priority
        FROM qintopia_agent_os.artifacts artifact
        JOIN qintopia_agent_os.work_items image_request
          ON image_request.id = artifact.work_item_id
         AND image_request.work_item_type = 'image_generation_request'
         AND image_request.status = 'awaiting_review'
        JOIN qintopia_agent_os.work_items visual
          ON visual.id = image_request.parent_work_item_id
         AND visual.work_item_type = 'visual_asset_request'
        JOIN qintopia_agent_os.work_items root
          ON root.id = visual.parent_work_item_id
         AND root.work_item_type = 'activity_promotion_request'
        JOIN qintopia_agent_os.poster_return_targets target
          ON target.origin_ref = root.metadata #>> '{workflow_metadata,origin_conversation_ref}'
         AND target.platform = 'feishu'
         AND target.conversation_type = 'direct'
        WHERE artifact.artifact_type = 'generated_image'
          AND artifact.review_status = 'pending'
          AND artifact.content_hash IS NOT NULL
          AND artifact.content_hash <> ''
          AND root.metadata #>> '{workflow_metadata,intake_channel}' = 'xiaoman_feishu_direct'
          AND EXISTS (
              SELECT 1 FROM qintopia_agent_os.work_item_events event
              WHERE event.work_item_id = image_request.id
                AND event.artifact_id = artifact.id
                AND event.event_type = 'generated_image_created'
          )
          AND NOT EXISTS (
              SELECT 1 FROM qintopia_agent_os.poster_notifications notification
              WHERE notification.generated_image_artifact_id = artifact.id
          )
          AND ($1::uuid IS NULL OR root.id = $1 OR image_request.id = $1 OR artifact.id = $1)
        UNION ALL
        SELECT root.id AS workflow_root_id,
               image_request.id AS image_work_item_id,
               NULL::uuid AS artifact_id,
               NULL::text AS artifact_hash,
               CASE WHEN EXISTS (
                   SELECT 1 FROM qintopia_agent_os.work_item_events event
                   WHERE event.work_item_id = image_request.id
                     AND event.event_type = 'image_generation_outcome_ambiguous'
               ) THEN 'generation_ambiguous' ELSE 'generation_failed' END AS notification_kind,
               CASE WHEN EXISTS (
                   SELECT 1 FROM qintopia_agent_os.work_item_events event
                   WHERE event.work_item_id = image_request.id
                     AND event.event_type = 'image_generation_outcome_ambiguous'
               ) THEN 'generation_outcome_ambiguous' ELSE 'generation_failed' END AS failure_code,
               root.metadata #>> '{workflow_metadata,origin_conversation_ref}' AS origin_ref,
               root.human_owner,
               root.priority
        FROM qintopia_agent_os.work_items image_request
        JOIN qintopia_agent_os.work_items visual
          ON visual.id = image_request.parent_work_item_id
         AND visual.work_item_type = 'visual_asset_request'
        JOIN qintopia_agent_os.work_items root
          ON root.id = visual.parent_work_item_id
         AND root.work_item_type = 'activity_promotion_request'
        JOIN qintopia_agent_os.poster_return_targets target
          ON target.origin_ref = root.metadata #>> '{workflow_metadata,origin_conversation_ref}'
         AND target.platform = 'feishu'
         AND target.conversation_type = 'direct'
        WHERE image_request.work_item_type = 'image_generation_request'
          AND image_request.status = 'failed'
          AND root.metadata #>> '{workflow_metadata,intake_channel}' = 'xiaoman_feishu_direct'
          AND NOT EXISTS (
              SELECT 1 FROM qintopia_agent_os.artifacts artifact
              WHERE artifact.work_item_id = image_request.id
                AND artifact.artifact_type = 'generated_image'
          )
          AND NOT EXISTS (
              SELECT 1 FROM qintopia_agent_os.poster_notifications notification
              WHERE notification.source_work_item_id = image_request.id
                AND notification.notification_kind IN ('generation_failed', 'generation_ambiguous')
          )
          AND ($1::uuid IS NULL OR root.id = $1 OR image_request.id = $1)
        ) candidates
        ORDER BY image_work_item_id ASC
        LIMIT $2
        "#,
    )
    .bind(work_item_id)
    .bind(batch_size.clamp(1, 100))
    .fetch_all(pool)
    .await
    .context("load poster notification candidates")?;
    rows.into_iter()
        .map(|row| {
            Ok(Candidate {
                workflow_root_id: row.try_get("workflow_root_id")?,
                image_work_item_id: row.try_get("image_work_item_id")?,
                artifact_id: row.try_get("artifact_id")?,
                artifact_hash: row.try_get("artifact_hash")?,
                notification_kind: row.try_get("notification_kind")?,
                failure_code: row.try_get("failure_code")?,
                origin_ref: row.try_get("origin_ref")?,
                human_owner: row.try_get("human_owner")?,
                priority: row.try_get("priority")?,
            })
        })
        .collect()
}

fn notification_request(candidate: &Candidate) -> Result<WorkItemCreateRequest> {
    let image_ready = candidate.notification_kind == "image_ready";
    if !valid_opaque_ref(&candidate.origin_ref)
        || (image_ready
            && (candidate.artifact_id.is_none()
                || candidate
                    .artifact_hash
                    .as_deref()
                    .is_none_or(|hash| !valid_content_hash(hash))))
        || (!image_ready && candidate.failure_code.is_none())
    {
        bail!("notification candidate identity is invalid");
    }
    let (brief_summary, purpose) = if image_ready {
        (
            "活动海报图片已生成，返回原会话等待人工审核",
            "generated_image_review_notification",
        )
    } else {
        (
            "活动海报生成未完成，返回原会话告知简化状态",
            "image_generation_status_notification",
        )
    };
    Ok(WorkItemCreateRequest {
        requester_agent: "xiaoman".to_string(),
        target_agent: "xiaoman".to_string(),
        capability_key: "xiaoman.notify_direct_conversation".to_string(),
        work_item_type: "conversation_notification_request".to_string(),
        brief_summary: brief_summary.to_string(),
        purpose: purpose.to_string(),
        human_owner: candidate.human_owner.clone(),
        priority: candidate.priority.clone(),
        source_type: "operations_workflow".to_string(),
        source_refs: json!({"source_record_ref": format!("image_generation_request:{}", candidate.image_work_item_id)}),
        source_event_signal_id: None,
        payload: json!({
            "workflow_type": "activity_promotion",
            "notification_type": candidate.notification_kind,
            "generated_image_artifact_id": candidate.artifact_id,
            "artifact_content_hash": candidate.artifact_hash,
            "failure_code": candidate.failure_code,
            "origin_conversation_ref": candidate.origin_ref,
            "review_status": if image_ready { "pending" } else { "not_applicable" },
            "group_send_authorized": false,
            "external_send_executed": false
        }),
        payload_redaction_policy: "summary_only".to_string(),
        idempotency_key: format!(
            "xiaoman_poster_notification:{}:{}",
            candidate.image_work_item_id, candidate.notification_kind
        ),
        dedupe_key: String::new(),
        metadata: json!({
            "workflow_root_id": candidate.workflow_root_id,
            "image_generation_work_item_id": candidate.image_work_item_id,
            "generated_image_artifact_id": candidate.artifact_id,
            "notification_type": candidate.notification_kind,
            "origin_conversation_ref": candidate.origin_ref,
            "group_send_authorized": false
        }),
        parent_work_item_id: Some(candidate.image_work_item_id),
        approved_artifact_id: None,
    })
}

async fn upsert_notification(
    pool: &PgPool,
    candidate: &Candidate,
    work_item_id: Uuid,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.poster_notifications
            (work_item_id, source_work_item_id, workflow_root_id,
             generated_image_artifact_id, notification_kind, failure_code, origin_ref)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (source_work_item_id, notification_kind) DO NOTHING
        "#,
    )
    .bind(work_item_id)
    .bind(candidate.image_work_item_id)
    .bind(candidate.workflow_root_id)
    .bind(candidate.artifact_id)
    .bind(&candidate.notification_kind)
    .bind(&candidate.failure_code)
    .bind(&candidate.origin_ref)
    .execute(pool)
    .await
    .context("upsert poster notification")?;
    Ok(())
}

pub async fn run_review_callback(cli: &Cli, apply: bool, dry_run: bool) -> Result<()> {
    if apply == dry_run {
        bail!("choose exactly one of --apply or --dry-run");
    }
    #[cfg(not(feature = "xiaoman-feishu-poster-adapter"))]
    if apply {
        bail!("Xiaoman Feishu poster callback adapter is not compiled");
    }
    let envelope = read_callback_envelope()?;
    let callback_key = std::env::var(CALLBACK_KEY_ENV)
        .ok()
        .filter(|value| !value.is_empty())
        .context("Xiaoman Feishu callback verification key is required")?;
    let callback = verify_and_parse_callback(&envelope, &callback_key, unix_timestamp_now()?)?;
    validate_callback(&callback)?;
    let pool = db::connect(cli.database_url_required()?, cli.db_max_connections).await?;
    let report = process_review_callback(&pool, cli, callback, apply).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_callback_ingress(cli: &Cli, socket_path: PathBuf) -> Result<()> {
    #[cfg(not(feature = "xiaoman-feishu-poster-adapter"))]
    {
        let _ = (cli, socket_path);
        bail!("Xiaoman Feishu poster callback adapter is not compiled");
    }
    #[cfg(feature = "xiaoman-feishu-poster-adapter")]
    {
        prepare_callback_socket(&socket_path)?;
        let callback_key = std::env::var(CALLBACK_KEY_ENV)
            .ok()
            .filter(|value| !value.is_empty())
            .context("Xiaoman Feishu callback verification key is required")?;
        let pool = db::connect(cli.database_url_required()?, cli.db_max_connections).await?;
        let listener = UnixListener::bind(&socket_path)
            .with_context(|| format!("bind poster callback socket {}", socket_path.display()))?;
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
            .context("set poster callback socket permissions")?;
        let _guard = CallbackSocketGuard(socket_path.clone());
        tracing::info!(socket_path = %socket_path.display(), "poster callback ingress started");
        loop {
            let (stream, _) = listener.accept().await.context("accept poster callback")?;
            if handle_callback_connection(stream, &pool, cli, &callback_key)
                .await
                .is_err()
            {
                tracing::warn!(
                    error_code = "poster_callback_rejected",
                    "poster callback rejected"
                );
            }
        }
    }
}

#[cfg(feature = "xiaoman-feishu-poster-adapter")]
async fn handle_callback_connection(
    mut stream: UnixStream,
    pool: &PgPool,
    cli: &Cli,
    callback_key: &str,
) -> Result<()> {
    let mut bytes = Vec::new();
    let mut reader = BufReader::new((&mut stream).take(MAX_CALLBACK_BYTES + 1));
    let count = timeout(CALLBACK_IO_TIMEOUT, reader.read_until(b'\n', &mut bytes))
        .await
        .context("poster callback read timed out")??;
    if count == 0 || bytes.len() as u64 > MAX_CALLBACK_BYTES {
        bail!("poster callback envelope length is invalid");
    }
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    let envelope: SignedCallbackEnvelope =
        serde_json::from_slice(&bytes).context("parse signed poster callback envelope")?;
    let callback = verify_and_parse_callback(&envelope, callback_key, unix_timestamp_now()?)?;
    validate_callback(&callback)?;
    let report = process_review_callback(pool, cli, callback, true).await?;
    let mut response = serde_json::to_vec(&report).context("serialize poster callback response")?;
    response.push(b'\n');
    timeout(CALLBACK_IO_TIMEOUT, stream.write_all(&response))
        .await
        .context("poster callback response timed out")??;
    Ok(())
}

#[cfg(feature = "xiaoman-feishu-poster-adapter")]
fn prepare_callback_socket(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path.file_name().and_then(|value| value.to_str()) != Some("poster-review-callback.sock")
    {
        bail!("poster callback socket path is invalid");
    }
    let parent = path
        .parent()
        .context("poster callback socket parent is missing")?;
    if !parent.is_dir() {
        bail!("poster callback socket parent is unavailable");
    }
    if path.exists() {
        if !path_is_socket(path) {
            bail!("poster callback path exists and is not a socket");
        }
        fs::remove_file(path).context("remove stale poster callback socket")?;
    }
    Ok(())
}

#[cfg(feature = "xiaoman-feishu-poster-adapter")]
fn path_is_socket(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_socket())
        .unwrap_or(false)
}

fn read_callback_envelope() -> Result<SignedCallbackEnvelope> {
    let mut body = Vec::new();
    io::stdin()
        .take(MAX_CALLBACK_BYTES + 1)
        .read_to_end(&mut body)
        .context("read poster review callback")?;
    if body.is_empty() || body.len() as u64 > MAX_CALLBACK_BYTES {
        bail!("poster review callback length is invalid");
    }
    serde_json::from_slice(&body).context("parse signed poster review callback envelope")
}

fn validate_callback(callback: &ReviewCallback) -> Result<()> {
    if callback.callback_event_id.is_empty()
        || callback.callback_event_id.len() > 240
        || !callback
            .callback_event_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':' | b'.'))
    {
        bail!("callback event id is invalid");
    }
    if callback.actor_user_id.is_empty()
        || callback.actor_user_id.len() > 200
        || !callback
            .actor_user_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':' | b'.'))
    {
        bail!("callback actor is invalid");
    }
    if callback.conversation_id.is_empty()
        || callback.conversation_id.len() > 200
        || !callback
            .conversation_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':' | b'.'))
    {
        bail!("callback conversation is invalid");
    }
    if !matches!(callback.action.as_str(), "approve" | "modify" | "abandon") {
        bail!("callback action is invalid");
    }
    Ok(())
}

fn verify_and_parse_callback(
    envelope: &SignedCallbackEnvelope,
    encrypt_key: &str,
    now: i64,
) -> Result<ReviewCallback> {
    let timestamp = envelope
        .timestamp
        .parse::<i64>()
        .context("Feishu callback timestamp is invalid")?;
    if timestamp <= 0 || now.abs_diff(timestamp) > MAX_CALLBACK_CLOCK_SKEW_SECONDS as u64 {
        bail!("Feishu callback timestamp is outside the accepted window");
    }
    if envelope.nonce.is_empty()
        || envelope.nonce.len() > 128
        || !envelope
            .nonce
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        || !is_lower_hex(&envelope.signature, 64)
        || encrypt_key.is_empty()
        || encrypt_key.len() > 512
    {
        bail!("Feishu callback signature envelope is invalid");
    }
    let body = Base64::decode_vec(&envelope.body_base64)
        .map_err(|_| anyhow::anyhow!("Feishu callback body encoding is invalid"))?;
    if body.is_empty() || body.len() as u64 > MAX_CALLBACK_BYTES {
        bail!("Feishu callback body length is invalid");
    }
    let mut hasher = Sha256::new();
    hasher.update(envelope.timestamp.as_bytes());
    hasher.update(envelope.nonce.as_bytes());
    hasher.update(encrypt_key.as_bytes());
    hasher.update(&body);
    let expected = format!("{:x}", hasher.finalize());
    if !constant_time_eq(expected.as_bytes(), envelope.signature.as_bytes()) {
        bail!("Feishu callback signature verification failed");
    }
    parse_feishu_card_callback(&body)
}

fn parse_feishu_card_callback(body: &[u8]) -> Result<ReviewCallback> {
    let value: serde_json::Value =
        serde_json::from_slice(body).context("parse Feishu card callback")?;
    let event = value
        .get("event")
        .context("Feishu card callback event is missing")?;
    let action_value = event
        .pointer("/action/value")
        .and_then(serde_json::Value::as_object)
        .context("Feishu card callback action value is missing")?;
    if action_value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
    {
        bail!("Feishu card callback schema is invalid");
    }
    if action_value
        .get("callback_kind")
        .and_then(serde_json::Value::as_str)
        != Some(POSTER_REVIEW_CALLBACK_KIND)
    {
        bail!("Feishu card callback kind is invalid");
    }
    let callback_event_id = value
        .pointer("/header/event_id")
        .or_else(|| event.get("event_id"))
        .and_then(serde_json::Value::as_str)
        .context("Feishu card callback event id is missing")?;
    let actor_user_id = event
        .pointer("/operator/open_id")
        .and_then(serde_json::Value::as_str)
        .context("Feishu card callback actor is missing")?;
    let conversation_id = event
        .pointer("/context/open_chat_id")
        .and_then(serde_json::Value::as_str)
        .context("Feishu card callback conversation is missing")?;
    let notification_id = action_value
        .get("notification_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .context("Feishu card callback notification id is invalid")?;
    let artifact_id = action_value
        .get("artifact_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .context("Feishu card callback artifact id is invalid")?;
    let action = action_value
        .get("action")
        .and_then(serde_json::Value::as_str)
        .context("Feishu card callback action is missing")?;
    Ok(ReviewCallback {
        callback_event_id: callback_event_id.to_string(),
        notification_id,
        artifact_id,
        conversation_id: conversation_id.to_string(),
        actor_user_id: actor_user_id.to_string(),
        action: action.to_string(),
    })
}

fn unix_timestamp_now() -> Result<i64> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before Unix epoch")?
            .as_secs(),
    )
    .context("system timestamp is outside the supported range")
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

async fn process_review_callback(
    pool: &PgPool,
    cli: &Cli,
    callback: ReviewCallback,
    apply: bool,
) -> Result<ReviewCallbackReport> {
    let row = sqlx::query(REVIEW_CALLBACK_TARGET_QUERY)
        .bind(callback.notification_id)
        .fetch_optional(pool)
        .await
        .context("load poster review callback target")?
        .context("poster notification is not found")?;
    let artifact_id: Uuid = row.try_get("generated_image_artifact_id")?;
    let notification_status: String = row.try_get("status")?;
    let conversation_id: String = row.try_get("conversation_id")?;
    let requester_user_id: String = row.try_get("requester_user_id")?;
    let review_status: String = row.try_get("review_status")?;
    if artifact_id != callback.artifact_id
        || conversation_id != callback.conversation_id
        || requester_user_id != callback.actor_user_id
        || notification_status != "delivered"
    {
        bail!("poster review callback does not match the delivered origin notification");
    }
    let existing = sqlx::query(
        r#"
        SELECT decision, notification_id, artifact_id, actor_ref
        FROM qintopia_agent_os.poster_review_actions
        WHERE callback_event_id = $1 OR notification_id = $2
        LIMIT 1
        "#,
    )
    .bind(&callback.callback_event_id)
    .bind(callback.notification_id)
    .fetch_optional(pool)
    .await
    .context("read poster review callback idempotency")?;
    let decision = callback_decision(&callback.action);
    if let Some(existing) = existing {
        let existing_decision: String = existing.try_get("decision")?;
        let existing_notification_id: Uuid = existing.try_get("notification_id")?;
        let existing_artifact_id: Uuid = existing.try_get("artifact_id")?;
        let existing_actor_ref: String = existing.try_get("actor_ref")?;
        if existing_decision != decision
            || existing_notification_id != callback.notification_id
            || existing_artifact_id != callback.artifact_id
            || existing_actor_ref != actor_ref(&callback.actor_user_id)
        {
            bail!("poster review callback was reused with different bound data");
        }
        return Ok(callback_report(
            &callback,
            decision,
            true,
            "idempotent_existing",
        ));
    }
    let desired_status = decision;
    if !apply {
        return Ok(callback_report(&callback, decision, false, "dry_run_ok"));
    }
    if review_status != desired_status {
        let policy = OperationsPolicy::from_cli(cli, true);
        operations::record_artifact_review_decision(
            pool,
            ArtifactReviewDecisionRequest {
                artifact_id: callback.artifact_id,
                reviewer_id: callback.actor_user_id.clone(),
                decision: decision.to_string(),
                expected_artifact_type: Some("generated_image".to_string()),
                expected_review_status: Some("pending".to_string()),
                reason: callback_reason(&callback.action).to_string(),
                source: "feishu_poster_review_card".to_string(),
                metadata: json!({
                    "notification_id": callback.notification_id,
                    "callback_event_ref": callback_event_ref(&callback.callback_event_id),
                    "group_send_authorized": false,
                    "external_send_executed": false
                }),
            },
            true,
            &policy,
        )
        .await?;
    }
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.poster_review_actions
            (callback_event_id, notification_id, artifact_id, actor_ref, decision)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(&callback.callback_event_id)
    .bind(callback.notification_id)
    .bind(callback.artifact_id)
    .bind(actor_ref(&callback.actor_user_id))
    .bind(decision)
    .execute(pool)
    .await
    .context("record poster review callback")?;
    Ok(callback_report(
        &callback,
        decision,
        false,
        "review_recorded",
    ))
}

#[cfg(all(test, feature = "postgres-integration-tests"))]
pub(crate) struct ReviewCallbackIntegrationInput<'a> {
    pub(crate) callback_event_id: &'a str,
    pub(crate) notification_id: Uuid,
    pub(crate) artifact_id: Uuid,
    pub(crate) conversation_id: &'a str,
    pub(crate) actor_user_id: &'a str,
    pub(crate) action: &'a str,
}

#[cfg(all(test, feature = "postgres-integration-tests"))]
pub(crate) async fn process_review_callback_for_postgres_integration(
    pool: &PgPool,
    database_url: &str,
    input: ReviewCallbackIntegrationInput<'_>,
) -> Result<bool> {
    use clap::Parser;

    let cli = Cli::try_parse_from([
        "qintopia-message-sidecar",
        "--database-url",
        database_url,
        "--operations-allowed-reviewer-ids",
        input.actor_user_id,
        "check",
    ])
    .context("build poster integration callback policy")?;
    let report = process_review_callback(
        pool,
        &cli,
        ReviewCallback {
            callback_event_id: input.callback_event_id.to_string(),
            notification_id: input.notification_id,
            artifact_id: input.artifact_id,
            conversation_id: input.conversation_id.to_string(),
            actor_user_id: input.actor_user_id.to_string(),
            action: input.action.to_string(),
        },
        true,
    )
    .await?;
    Ok(report.deduped)
}

fn callback_report(
    callback: &ReviewCallback,
    decision: &str,
    deduped: bool,
    action_status: &str,
) -> ReviewCallbackReport {
    ReviewCallbackReport {
        success: true,
        action_status: action_status.to_string(),
        decision: decision.to_string(),
        notification_id: callback.notification_id,
        artifact_id: callback.artifact_id,
        deduped,
        group_send_authorized: false,
        external_send_executed: false,
    }
}

fn callback_decision(action: &str) -> &'static str {
    match action {
        "approve" => "approved",
        "modify" => "changes_requested",
        "abandon" => "rejected",
        _ => unreachable!("callback action was validated"),
    }
}

fn callback_reason(action: &str) -> &'static str {
    match action {
        "approve" => "approved from originating Feishu poster review card",
        "modify" => "changes requested from originating Feishu poster review card",
        "abandon" => "poster abandoned from originating Feishu poster review card",
        _ => unreachable!("callback action was validated"),
    }
}

fn valid_opaque_ref(value: &str) -> bool {
    valid_prefixed_hash(value)
}

fn valid_content_hash(value: &str) -> bool {
    valid_prefixed_hash(value)
}

fn valid_prefixed_hash(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn actor_ref(actor_user_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"poster-review-actor-v1\0");
    hasher.update(actor_user_id.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

fn callback_event_ref(callback_event_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"poster-review-callback-v1\0");
    hasher.update(callback_event_id.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate() -> Candidate {
        Candidate {
            workflow_root_id: Uuid::new_v4(),
            image_work_item_id: Uuid::new_v4(),
            artifact_id: Some(Uuid::new_v4()),
            artifact_hash: Some(format!("sha256:{}", "a".repeat(64))),
            notification_kind: "image_ready".to_string(),
            failure_code: None,
            origin_ref: format!("sha256:{}", "b".repeat(64)),
            human_owner: format!("sha256:{}", "c".repeat(64)),
            priority: "normal".to_string(),
        }
    }

    #[test]
    fn notification_work_is_idempotent_and_never_group_authorized() {
        let candidate = candidate();
        let request = notification_request(&candidate).expect("notification request");
        assert_eq!(
            request.parent_work_item_id,
            Some(candidate.image_work_item_id)
        );
        assert_eq!(request.payload["group_send_authorized"], false);
        assert_eq!(
            request.idempotency_key,
            format!(
                "xiaoman_poster_notification:{}:{}",
                candidate.image_work_item_id, candidate.notification_kind
            )
        );
        assert!(request.payload.get("conversation_id").is_none());

        let mut noncanonical_candidate = candidate;
        noncanonical_candidate.origin_ref = format!("sha256:{}", "A".repeat(64));
        assert!(notification_request(&noncanonical_candidate).is_err());
    }

    #[test]
    fn generation_failure_notification_is_sanitized_and_has_no_artifact() {
        let mut candidate = candidate();
        candidate.artifact_id = None;
        candidate.artifact_hash = None;
        candidate.notification_kind = "generation_failed".to_string();
        candidate.failure_code = Some("generation_failed".to_string());
        let request = notification_request(&candidate).expect("failure notification request");
        assert_eq!(request.payload["notification_type"], "generation_failed");
        assert!(request.payload["generated_image_artifact_id"].is_null());
        assert_eq!(request.payload["group_send_authorized"], false);
    }

    #[test]
    fn non_image_notifications_cannot_satisfy_review_callback_target() {
        assert!(REVIEW_CALLBACK_TARGET_QUERY
            .contains("AND notification.notification_kind = 'image_ready'"));
    }

    #[test]
    fn callback_actions_map_to_existing_review_decisions() {
        assert_eq!(callback_decision("approve"), "approved");
        assert_eq!(callback_decision("modify"), "changes_requested");
        assert_eq!(callback_decision("abandon"), "rejected");
    }

    #[test]
    fn callback_requires_verified_direct_adapter_input() {
        let notification_id = Uuid::new_v4();
        let artifact_id = Uuid::new_v4();
        let body = serde_json::to_vec(&json!({
            "header": {"event_id": "evt_fixture"},
            "event": {
                "operator": {"open_id": "ou_fixture"},
                "context": {"open_chat_id": "oc_fixture"},
                "action": {"value": {
                    "schema_version": 1,
                    "callback_kind": POSTER_REVIEW_CALLBACK_KIND,
                    "notification_id": notification_id,
                    "artifact_id": artifact_id,
                    "action": "approve"
                }}
            }
        }))
        .unwrap();
        let timestamp = "1785456000";
        let nonce = "nonce_fixture";
        let key = "callback-key-fixture";
        let mut hasher = Sha256::new();
        hasher.update(timestamp.as_bytes());
        hasher.update(nonce.as_bytes());
        hasher.update(key.as_bytes());
        hasher.update(&body);
        let envelope = SignedCallbackEnvelope {
            timestamp: timestamp.to_string(),
            nonce: nonce.to_string(),
            signature: format!("{:x}", hasher.finalize()),
            body_base64: Base64::encode_string(&body),
        };
        let callback = verify_and_parse_callback(&envelope, key, 1_785_456_000).unwrap();
        assert_eq!(callback.notification_id, notification_id);
        assert_eq!(callback.conversation_id, "oc_fixture");
        assert!(verify_and_parse_callback(&envelope, "wrong-key", 1_785_456_000).is_err());
    }

    #[test]
    fn callback_envelope_matches_xiaoman_plugin_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../skills/qintopia-tools/variants/xiaoman/tests/fixtures/poster-review-callback-envelope.json"
        ))
        .unwrap();
        let envelope: SignedCallbackEnvelope =
            serde_json::from_value(fixture["envelope"].clone()).unwrap();
        let key = fixture["callback_key"].as_str().unwrap();
        let now = fixture["now"].as_i64().unwrap();
        let expected = &fixture["expected"];
        let callback = verify_and_parse_callback(&envelope, key, now).unwrap();

        assert_eq!(
            callback.callback_event_id,
            expected["callback_event_id"].as_str().unwrap()
        );
        assert_eq!(
            callback.conversation_id,
            expected["conversation_id"].as_str().unwrap()
        );
        assert_eq!(
            callback.actor_user_id,
            expected["actor_user_id"].as_str().unwrap()
        );
        assert_eq!(
            callback.notification_id,
            Uuid::parse_str(expected["notification_id"].as_str().unwrap()).unwrap()
        );
        assert_eq!(
            callback.artifact_id,
            Uuid::parse_str(expected["artifact_id"].as_str().unwrap()).unwrap()
        );
        assert_eq!(callback.action, expected["action"].as_str().unwrap());
    }
}
