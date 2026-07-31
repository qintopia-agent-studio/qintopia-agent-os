use std::collections::BTreeSet;

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Row};
use url::Url;
use uuid::Uuid;
use zeroize::Zeroizing;

#[cfg(any(test, feature = "xiaoman-feishu-poster-adapter"))]
use crate::bounded_http::{HttpClient, HttpResponse};
use crate::{config::Cli, db};

const WORKER_ID: &str = "xiaoman-feishu-poster-delivery";
const ENABLE_ENV: &str = "QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED";
const APPROVAL_ENV: &str = "QINTOPIA_XIAOMAN_FEISHU_POSTER_APPROVAL";
const APPROVAL_PHRASE: &str = "approved-production-xiaoman-feishu-poster-return";
const RELEASE_SHA_ENV: &str = "QINTOPIA_XIAOMAN_FEISHU_POSTER_RELEASE_SHA";
const DEPLOYED_SHA_ENV: &str = "QINTOPIA_DEPLOYED_COMMIT_SHA";
const DATABASE_HASH_ENV: &str = "QINTOPIA_XIAOMAN_FEISHU_POSTER_DATABASE_URL_SHA256";
const APP_ID_ENV: &str = "QINTOPIA_XIAOMAN_FEISHU_APP_ID";
const APP_SECRET_ENV: &str = "QINTOPIA_XIAOMAN_FEISHU_APP_SECRET";
const ALLOWED_CHAT_IDS_ENV: &str = "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_CHAT_IDS";
const ALLOWED_USER_IDS_ENV: &str = "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_USER_IDS";
const MEDIA_HOSTS_ENV: &str = "QINTOPIA_XIAOMAN_POSTER_MEDIA_ALLOWED_HOSTS";
const OFFICIAL_API_ROOT: &str = "https://open.feishu.cn/open-apis/";
const MAX_JSON_BYTES: usize = 1024 * 1024;
const DEFAULT_MAX_MEDIA_BYTES: usize = 20 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct WorkerOptions {
    pub once: bool,
    pub apply: bool,
    pub dry_run: bool,
    pub notification_id: Option<Uuid>,
}

#[derive(Debug)]
struct DeliveryCandidate {
    notification_id: Uuid,
    work_item_id: Uuid,
    artifact_id: Option<Uuid>,
    artifact_uri: Option<String>,
    content_hash: Option<String>,
    mime_type: Option<String>,
    byte_size: Option<usize>,
    notification_kind: String,
    failure_code: Option<String>,
    conversation_id: String,
    requester_user_id: String,
}

#[derive(Debug)]
struct DeliveryClaim {
    candidate: DeliveryCandidate,
    attempt_id: Uuid,
    claim_token: String,
}

#[cfg(any(
    feature = "xiaoman-feishu-poster-adapter",
    feature = "postgres-integration-tests"
))]
#[derive(Debug)]
enum ClaimNotificationOutcome {
    Claimed(Box<DeliveryClaim>),
    Rejected {
        notification_id: Uuid,
        artifact_id: Option<Uuid>,
    },
    Empty,
}

#[derive(Debug)]
struct AdapterConfig {
    api_root: Url,
    app_id: Zeroizing<String>,
    app_secret: Zeroizing<String>,
    allowed_chat_ids: BTreeSet<String>,
    allowed_user_ids: BTreeSet<String>,
    media_allowed_hosts: BTreeSet<String>,
    max_media_bytes: usize,
}

#[derive(Debug, Serialize)]
struct PreflightReport {
    success: bool,
    adapter_compiled: bool,
    action_status: &'static str,
    enabled: bool,
    chat_allowlist_count: usize,
    user_allowlist_count: usize,
    media_host_allowlist_count: usize,
    external_calls_executed: bool,
    database_writes_executed: bool,
}

#[derive(Debug, Serialize)]
struct WorkerReport {
    success: bool,
    action_status: &'static str,
    notification_id: Option<Uuid>,
    artifact_id: Option<Uuid>,
    external_send_executed: Option<bool>,
    automatic_retry_allowed: bool,
    sensitive_fields_redacted: bool,
}

#[derive(Debug)]
enum DeliveryFailure {
    Failed(&'static str),
    Ambiguous(&'static str),
}

impl AdapterConfig {
    #[cfg(feature = "xiaoman-feishu-poster-adapter")]
    fn from_env(database_url: &str) -> Result<Self> {
        if std::env::var(ENABLE_ENV).ok().as_deref() != Some("1") {
            bail!("Xiaoman Feishu poster delivery is disabled");
        }
        if std::env::var(APPROVAL_ENV).ok().as_deref() != Some(APPROVAL_PHRASE) {
            bail!("Xiaoman Feishu poster delivery owner approval is missing");
        }
        let expected_release = required_env(RELEASE_SHA_ENV)?;
        let deployed_release = required_env(DEPLOYED_SHA_ENV)?;
        if !is_lower_hex(&expected_release, 40) || expected_release != deployed_release {
            bail!("Xiaoman Feishu poster delivery release binding is invalid");
        }
        let expected_database_hash = required_env(DATABASE_HASH_ENV)?;
        if !is_lower_hex(&expected_database_hash, 64)
            || expected_database_hash != sha256_hex(database_url.as_bytes())
        {
            bail!("Xiaoman Feishu poster delivery database binding is invalid");
        }
        let api_root = Url::parse(OFFICIAL_API_ROOT).expect("official Feishu API root is valid");
        Ok(Self {
            api_root,
            app_id: Zeroizing::new(required_env(APP_ID_ENV)?),
            app_secret: Zeroizing::new(required_env(APP_SECRET_ENV)?),
            allowed_chat_ids: required_identifier_set(ALLOWED_CHAT_IDS_ENV)?,
            allowed_user_ids: required_identifier_set(ALLOWED_USER_IDS_ENV)?,
            media_allowed_hosts: required_host_set(MEDIA_HOSTS_ENV)?,
            max_media_bytes: DEFAULT_MAX_MEDIA_BYTES,
        })
    }
}

pub fn run_preflight(cli: &Cli) -> Result<()> {
    #[cfg(not(feature = "xiaoman-feishu-poster-adapter"))]
    {
        let _ = cli;
        println!(
            "{}",
            serde_json::to_string_pretty(&PreflightReport {
                success: false,
                adapter_compiled: false,
                action_status: "adapter_not_compiled",
                enabled: false,
                chat_allowlist_count: 0,
                user_allowlist_count: 0,
                media_host_allowlist_count: 0,
                external_calls_executed: false,
                database_writes_executed: false,
            })?
        );
        bail!("Xiaoman Feishu poster delivery adapter is not compiled");
    }
    #[cfg(feature = "xiaoman-feishu-poster-adapter")]
    {
        let database_url = cli.database_url_required()?;
        let config = AdapterConfig::from_env(database_url)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&PreflightReport {
                success: true,
                adapter_compiled: true,
                action_status: "adapter_config_ready",
                enabled: true,
                chat_allowlist_count: config.allowed_chat_ids.len(),
                user_allowlist_count: config.allowed_user_ids.len(),
                media_host_allowlist_count: config.media_allowed_hosts.len(),
                external_calls_executed: false,
                database_writes_executed: false,
            })?
        );
        Ok(())
    }
}

pub async fn run_worker(cli: &Cli, options: WorkerOptions) -> Result<()> {
    if !options.once || options.apply == options.dry_run {
        bail!("poster delivery worker requires --once and exactly one of --apply or --dry-run");
    }
    if options.apply {
        return run_apply(cli, options.notification_id).await;
    }
    let pool = db::connect(cli.database_url_required()?, cli.db_max_connections).await?;
    let candidate = preview_candidate(&pool, options.notification_id).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&WorkerReport {
            success: true,
            action_status: if candidate.is_some() {
                "delivery_preview"
            } else {
                "no_pending_notification"
            },
            notification_id: candidate.as_ref().map(|item| item.notification_id),
            artifact_id: candidate.as_ref().and_then(|item| item.artifact_id),
            external_send_executed: Some(false),
            automatic_retry_allowed: false,
            sensitive_fields_redacted: true,
        })?
    );
    Ok(())
}

async fn run_apply(cli: &Cli, notification_id: Option<Uuid>) -> Result<()> {
    #[cfg(not(feature = "xiaoman-feishu-poster-adapter"))]
    {
        let _ = (cli, notification_id);
        bail!("Xiaoman Feishu poster delivery adapter is not compiled");
    }
    #[cfg(feature = "xiaoman-feishu-poster-adapter")]
    {
        let database_url = cli.database_url_required()?;
        let config = AdapterConfig::from_env(database_url)?;
        let pool = db::connect(database_url, cli.db_max_connections).await?;
        let claim = match claim_notification(&pool, notification_id, &config).await? {
            ClaimNotificationOutcome::Claimed(claim) => claim,
            ClaimNotificationOutcome::Rejected {
                notification_id,
                artifact_id,
            } => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&WorkerReport {
                        success: false,
                        action_status: "conversation_notification_failed",
                        notification_id: Some(notification_id),
                        artifact_id,
                        external_send_executed: Some(false),
                        automatic_retry_allowed: false,
                        sensitive_fields_redacted: true,
                    })?
                );
                return Ok(());
            }
            ClaimNotificationOutcome::Empty => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&WorkerReport {
                        success: true,
                        action_status: "no_pending_notification",
                        notification_id: None,
                        artifact_id: None,
                        external_send_executed: Some(false),
                        automatic_retry_allowed: false,
                        sensitive_fields_redacted: true,
                    })?
                );
                return Ok(());
            }
        };
        let result = deliver_claim(
            &pool,
            database_url,
            &config,
            &claim,
            &HttpClient::production(),
        )
        .await;
        let report = match result {
            Ok(message_ref) => {
                complete_delivery(&pool, &claim, &message_ref).await?;
                WorkerReport {
                    success: true,
                    action_status: "conversation_notification_delivered",
                    notification_id: Some(claim.candidate.notification_id),
                    artifact_id: claim.candidate.artifact_id,
                    external_send_executed: Some(true),
                    automatic_retry_allowed: false,
                    sensitive_fields_redacted: true,
                }
            }
            Err(DeliveryFailure::Failed(code)) => {
                fail_delivery(&pool, &claim, code, false).await?;
                WorkerReport {
                    success: false,
                    action_status: "conversation_notification_failed",
                    notification_id: Some(claim.candidate.notification_id),
                    artifact_id: claim.candidate.artifact_id,
                    external_send_executed: Some(false),
                    automatic_retry_allowed: false,
                    sensitive_fields_redacted: true,
                }
            }
            Err(DeliveryFailure::Ambiguous(code)) => {
                fail_delivery(&pool, &claim, code, true).await?;
                WorkerReport {
                    success: false,
                    action_status: "conversation_notification_ambiguous",
                    notification_id: Some(claim.candidate.notification_id),
                    artifact_id: claim.candidate.artifact_id,
                    external_send_executed: None,
                    automatic_retry_allowed: false,
                    sensitive_fields_redacted: true,
                }
            }
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        Ok(())
    }
}

async fn preview_candidate(
    pool: &PgPool,
    notification_id: Option<Uuid>,
) -> Result<Option<DeliveryCandidate>> {
    let row = sqlx::query(&candidate_select(false))
        .bind(notification_id)
        .fetch_optional(pool)
        .await
        .context("preview poster notification")?;
    row.map(candidate_from_row).transpose()
}

#[cfg(any(
    feature = "xiaoman-feishu-poster-adapter",
    feature = "postgres-integration-tests"
))]
async fn claim_notification(
    pool: &PgPool,
    notification_id: Option<Uuid>,
    config: &AdapterConfig,
) -> Result<ClaimNotificationOutcome> {
    let mut tx = pool.begin().await.context("begin poster delivery claim")?;
    reconcile_one_stale_claim(&mut tx, notification_id).await?;
    let row = sqlx::query(&candidate_select(true))
        .bind(notification_id)
        .fetch_optional(&mut *tx)
        .await
        .context("lock poster notification")?;
    let Some(row) = row else {
        tx.commit()
            .await
            .context("commit empty poster delivery claim")?;
        return Ok(ClaimNotificationOutcome::Empty);
    };
    let candidate = candidate_from_row(row)?;
    let rejection_code = if !config.allowed_chat_ids.contains(&candidate.conversation_id)
        || !config
            .allowed_user_ids
            .contains(&candidate.requester_user_id)
    {
        Some("return_target_not_allowlisted")
    } else if validate_candidate(&candidate, config).is_err() {
        Some("notification_identity_invalid")
    } else {
        None
    };
    if let Some(failure_code) = rejection_code {
        terminalize_rejected_candidate(&mut tx, &candidate, failure_code).await?;
        tx.commit()
            .await
            .context("commit rejected poster delivery candidate")?;
        return Ok(ClaimNotificationOutcome::Rejected {
            notification_id: candidate.notification_id,
            artifact_id: candidate.artifact_id,
        });
    }
    let claim_token = format!("{WORKER_ID}:{}", Uuid::new_v4());
    let attempt_number: i32 = sqlx::query_scalar(
        r#"
        UPDATE qintopia_agent_os.poster_notifications
        SET status = 'claimed', claimed_by = $2, claimed_at = now(),
            claim_expires_at = now() + interval '5 minutes',
            attempt_count = attempt_count + 1, updated_at = now()
        WHERE id = $1 AND status = 'pending'
        RETURNING attempt_count
        "#,
    )
    .bind(candidate.notification_id)
    .bind(&claim_token)
    .fetch_one(&mut *tx)
    .await
    .context("claim poster notification")?;
    let updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET status = 'processing', claimed_by = $2, locked_at = now(),
            claim_expires_at = now() + interval '5 minutes', attempts = attempts + 1,
            updated_at = now()
        WHERE id = $1 AND status = 'queued'
        "#,
    )
    .bind(candidate.work_item_id)
    .bind(&claim_token)
    .execute(&mut *tx)
    .await
    .context("claim poster notification work item")?;
    if updated.rows_affected() != 1 {
        bail!("poster notification work item is not claimable");
    }
    let attempt_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO qintopia_agent_os.poster_notification_attempts
            (notification_id, attempt_number, claim_token, status, audit_metadata)
        VALUES ($1, $2, $3, 'uploading',
                '{"automatic_retry_allowed":false,"external_send_outcome":"not_started"}'::jsonb)
        RETURNING id
        "#,
    )
    .bind(candidate.notification_id)
    .bind(attempt_number)
    .bind(&claim_token)
    .fetch_one(&mut *tx)
    .await
    .context("persist poster delivery attempt")?;
    append_event(
        &mut tx,
        candidate.work_item_id,
        candidate.artifact_id,
        "conversation_notification_delivery_started",
        json!({"attempt_id": attempt_id, "automatic_retry_allowed": false, "external_send_executed": false}),
    )
    .await?;
    tx.commit().await.context("commit poster delivery claim")?;
    Ok(ClaimNotificationOutcome::Claimed(Box::new(DeliveryClaim {
        candidate,
        attempt_id,
        claim_token,
    })))
}

#[cfg(any(
    feature = "xiaoman-feishu-poster-adapter",
    feature = "postgres-integration-tests"
))]
async fn terminalize_rejected_candidate(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    candidate: &DeliveryCandidate,
    failure_code: &'static str,
) -> Result<()> {
    let notification = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.poster_notifications
        SET status = 'failed', last_error_code = $2,
            claimed_by = NULL, claimed_at = NULL, claim_expires_at = NULL,
            updated_at = now()
        WHERE id = $1 AND status = 'pending'
        "#,
    )
    .bind(candidate.notification_id)
    .bind(failure_code)
    .execute(&mut **tx)
    .await
    .context("terminalize rejected poster notification")?;
    if notification.rows_affected() != 1 {
        bail!("rejected poster notification changed before terminalization");
    }

    let work_item = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET status = 'failed', last_error = $2,
            claimed_by = NULL, locked_at = NULL, claim_expires_at = NULL,
            updated_at = now()
        WHERE id = $1 AND status = 'queued'
        "#,
    )
    .bind(candidate.work_item_id)
    .bind(failure_code)
    .execute(&mut **tx)
    .await
    .context("terminalize rejected poster notification work item")?;
    if work_item.rows_affected() != 1 {
        bail!("rejected poster notification work item changed before terminalization");
    }

    append_event(
        tx,
        candidate.work_item_id,
        candidate.artifact_id,
        "conversation_notification_failed",
        json!({
            "failure_code": failure_code,
            "external_send_executed": false,
            "automatic_retry_allowed": false,
            "rejected_before_external_io": true,
            "group_send_authorized": false
        }),
    )
    .await?;
    Ok(())
}

fn candidate_select(for_update: bool) -> String {
    format!(
        r#"
        SELECT notification.id AS notification_id, notification.work_item_id,
               notification.generated_image_artifact_id AS artifact_id,
               notification.notification_kind, notification.failure_code,
               artifact.artifact_uri, artifact.content_hash,
               artifact.metadata->>'mime_type' AS mime_type,
               artifact.metadata->>'byte_size' AS byte_size,
               target.conversation_id, target.requester_user_id
        FROM qintopia_agent_os.poster_notifications notification
        JOIN qintopia_agent_os.work_items item ON item.id = notification.work_item_id
        LEFT JOIN qintopia_agent_os.artifacts artifact
          ON artifact.id = notification.generated_image_artifact_id
        JOIN qintopia_agent_os.poster_return_targets target
          ON target.origin_ref = notification.origin_ref
        WHERE notification.status = 'pending'
          AND item.status = 'queued'
          AND (
              (notification.notification_kind = 'image_ready'
               AND artifact.artifact_type = 'generated_image'
               AND artifact.review_status = 'pending')
              OR
              (notification.notification_kind IN ('generation_failed', 'generation_ambiguous')
               AND notification.generated_image_artifact_id IS NULL)
          )
          AND target.platform = 'feishu'
          AND target.conversation_type = 'direct'
          AND ($1::uuid IS NULL OR notification.id = $1)
        ORDER BY notification.created_at ASC
        LIMIT 1
        {}
        "#,
        if for_update {
            "FOR UPDATE OF notification, item SKIP LOCKED"
        } else {
            ""
        }
    )
}

fn candidate_from_row(row: sqlx::postgres::PgRow) -> Result<DeliveryCandidate> {
    let byte_size = row
        .try_get::<Option<String>, _>("byte_size")?
        .and_then(|value| value.parse::<usize>().ok());
    Ok(DeliveryCandidate {
        notification_id: row.try_get("notification_id")?,
        work_item_id: row.try_get("work_item_id")?,
        artifact_id: row.try_get("artifact_id")?,
        artifact_uri: row.try_get("artifact_uri")?,
        content_hash: row.try_get("content_hash")?,
        mime_type: row.try_get("mime_type")?,
        byte_size,
        notification_kind: row.try_get("notification_kind")?,
        failure_code: row.try_get("failure_code")?,
        conversation_id: row.try_get("conversation_id")?,
        requester_user_id: row.try_get("requester_user_id")?,
    })
}

fn validate_candidate(candidate: &DeliveryCandidate, config: &AdapterConfig) -> Result<()> {
    if !matches!(
        candidate.notification_kind.as_str(),
        "image_ready" | "generation_failed" | "generation_ambiguous"
    ) {
        bail!("poster notification kind is invalid");
    }
    if candidate.notification_kind != "image_ready" {
        if candidate.artifact_id.is_some()
            || candidate.failure_code.as_deref().is_none_or(str::is_empty)
        {
            bail!("poster status notification identity is invalid");
        }
        return Ok(());
    }
    let artifact_id = candidate
        .artifact_id
        .context("poster artifact id is missing")?;
    let artifact_uri = candidate
        .artifact_uri
        .as_deref()
        .context("poster artifact URI is missing")?;
    let content_hash = candidate
        .content_hash
        .as_deref()
        .context("poster content hash is missing")?;
    let byte_size = candidate.byte_size.context("poster byte size is missing")?;
    if candidate.mime_type.as_deref() != Some("image/jpeg")
        || byte_size == 0
        || byte_size > config.max_media_bytes
        || !canonical_sha256(content_hash)
    {
        bail!("poster artifact identity is invalid");
    }
    if artifact_uri.starts_with("feishu-base://") {
        let expected = format!("feishu-base://huabaosi-generated-image/{}", artifact_id);
        if artifact_uri != expected {
            bail!("poster Feishu storage URI is invalid");
        }
        return Ok(());
    }
    let uri = Url::parse(artifact_uri).context("poster artifact URI is invalid")?;
    if uri.scheme() != "https"
        || !uri.username().is_empty()
        || uri.password().is_some()
        || uri.query().is_some()
        || uri.fragment().is_some()
        || uri.host_str().is_none_or(|host| {
            !config
                .media_allowed_hosts
                .contains(&host.to_ascii_lowercase())
        })
    {
        bail!("poster artifact URI is not allowlisted");
    }
    Ok(())
}

#[cfg(feature = "xiaoman-feishu-poster-adapter")]
async fn deliver_claim(
    pool: &PgPool,
    database_url: &str,
    config: &AdapterConfig,
    claim: &DeliveryClaim,
    client: &HttpClient,
) -> std::result::Result<String, DeliveryFailure> {
    if claim.candidate.notification_kind != "image_ready" {
        let token = tenant_token(config, client)?;
        mark_sending(pool, claim, None)
            .await
            .map_err(|_| DeliveryFailure::Ambiguous("send_gate_persistence_failed"))?;
        return send_status_message(config, client, &token, claim);
    }
    let bytes = load_exact_image(pool, database_url, config, &claim.candidate, client).await?;
    let token = tenant_token(config, client)?;
    let artifact_id = claim
        .candidate
        .artifact_id
        .ok_or(DeliveryFailure::Failed("artifact_identity_mismatch"))?;
    let image_key = upload_image(config, client, &token, artifact_id, &bytes)?;
    mark_sending(pool, claim, Some(&image_key))
        .await
        .map_err(|_| DeliveryFailure::Ambiguous("send_gate_persistence_failed"))?;
    send_review_card(config, client, &token, &image_key, claim)
}

#[cfg(feature = "xiaoman-feishu-poster-adapter")]
async fn load_exact_image(
    pool: &PgPool,
    database_url: &str,
    config: &AdapterConfig,
    candidate: &DeliveryCandidate,
    client: &HttpClient,
) -> std::result::Result<Zeroizing<Vec<u8>>, DeliveryFailure> {
    let artifact_uri = candidate
        .artifact_uri
        .as_deref()
        .ok_or(DeliveryFailure::Failed("artifact_uri_invalid"))?;
    let content_hash = candidate
        .content_hash
        .as_deref()
        .ok_or(DeliveryFailure::Failed("artifact_identity_mismatch"))?;
    let bytes = if artifact_uri.starts_with("feishu-base://") {
        let stored = crate::huabaosi_feishu_artifact_mirror::revalidate_primary_storage_for_review_notification(
            pool,
            candidate
                .artifact_id
                .ok_or(DeliveryFailure::Failed("artifact_identity_mismatch"))?,
            database_url,
        )
        .await
        .map_err(|_| DeliveryFailure::Failed("artifact_readback_failed"))?;
        if stored.content_hash != content_hash {
            return Err(DeliveryFailure::Failed("artifact_identity_mismatch"));
        }
        stored.bytes
    } else {
        let uri = Url::parse(artifact_uri)
            .map_err(|_| DeliveryFailure::Failed("artifact_uri_invalid"))?;
        let mut response = client
            .request(
                "GET",
                &uri,
                &[("Accept", "image/jpeg".to_string())],
                &[],
                config.max_media_bytes,
            )
            .map_err(|_| DeliveryFailure::Failed("artifact_fetch_failed"))?;
        if response.status != 200 {
            return Err(DeliveryFailure::Failed("artifact_fetch_rejected"));
        }
        Zeroizing::new(std::mem::take(&mut response.body))
    };
    if Some(bytes.len()) != candidate.byte_size
        || !bytes.starts_with(&[0xff, 0xd8, 0xff])
        || content_hash != sha256_marker(&bytes)
    {
        return Err(DeliveryFailure::Failed("artifact_identity_mismatch"));
    }
    Ok(bytes)
}

#[cfg(any(test, feature = "xiaoman-feishu-poster-adapter"))]
fn tenant_token(
    config: &AdapterConfig,
    client: &HttpClient,
) -> std::result::Result<Zeroizing<String>, DeliveryFailure> {
    let endpoint = config
        .api_root
        .join("auth/v3/tenant_access_token/internal")
        .map_err(|_| DeliveryFailure::Failed("token_endpoint_invalid"))?;
    let body = serde_json::to_vec(
        &json!({"app_id": config.app_id.as_str(), "app_secret": config.app_secret.as_str()}),
    )
    .map_err(|_| DeliveryFailure::Failed("token_request_invalid"))?;
    let response = client
        .request(
            "POST",
            &endpoint,
            &[("Content-Type", "application/json".to_string())],
            &body,
            MAX_JSON_BYTES,
        )
        .map_err(|_| DeliveryFailure::Failed("token_request_failed"))?;
    let parsed = parse_success_json(&response, "token_response_invalid")?;
    let token = parsed
        .get("tenant_access_token")
        .and_then(Value::as_str)
        .filter(|value| valid_external_id(value))
        .ok_or(DeliveryFailure::Failed("token_response_invalid"))?;
    Ok(Zeroizing::new(token.to_string()))
}

#[cfg(any(test, feature = "xiaoman-feishu-poster-adapter"))]
fn upload_image(
    config: &AdapterConfig,
    client: &HttpClient,
    token: &str,
    artifact_id: Uuid,
    bytes: &[u8],
) -> std::result::Result<Zeroizing<String>, DeliveryFailure> {
    let endpoint = config
        .api_root
        .join("im/v1/images")
        .map_err(|_| DeliveryFailure::Failed("image_endpoint_invalid"))?;
    let boundary = format!("qintopia-{}", artifact_id.simple());
    let body = multipart_image(&boundary, artifact_id, bytes);
    let response = client
        .request(
            "POST",
            &endpoint,
            &[
                ("Authorization", format!("Bearer {token}")),
                (
                    "Content-Type",
                    format!("multipart/form-data; boundary={boundary}"),
                ),
            ],
            &body,
            MAX_JSON_BYTES,
        )
        .map_err(|error| {
            if error.request_may_have_been_sent() {
                DeliveryFailure::Ambiguous("image_upload_outcome_ambiguous")
            } else {
                DeliveryFailure::Failed("image_upload_failed")
            }
        })?;
    let parsed = parse_success_json(&response, "image_upload_outcome_ambiguous")?;
    let image_key = parsed
        .pointer("/data/image_key")
        .and_then(Value::as_str)
        .filter(|value| valid_external_id(value))
        .ok_or(DeliveryFailure::Ambiguous("image_upload_outcome_ambiguous"))?;
    Ok(Zeroizing::new(image_key.to_string()))
}

#[cfg(any(test, feature = "xiaoman-feishu-poster-adapter"))]
fn send_review_card(
    config: &AdapterConfig,
    client: &HttpClient,
    token: &str,
    image_key: &str,
    claim: &DeliveryClaim,
) -> std::result::Result<String, DeliveryFailure> {
    let endpoint = config
        .api_root
        .join("im/v1/messages?receive_id_type=chat_id")
        .map_err(|_| DeliveryFailure::Ambiguous("card_endpoint_invalid"))?;
    let content = serde_json::to_string(&review_card(image_key, claim))
        .map_err(|_| DeliveryFailure::Ambiguous("card_request_invalid"))?;
    let body = serde_json::to_vec(&json!({
        "receive_id": claim.candidate.conversation_id,
        "msg_type": "interactive",
        "content": content,
        "uuid": format!("poster-notification-{}", claim.candidate.notification_id)
    }))
    .map_err(|_| DeliveryFailure::Ambiguous("card_request_invalid"))?;
    let response = client
        .request(
            "POST",
            &endpoint,
            &[
                ("Authorization", format!("Bearer {token}")),
                ("Content-Type", "application/json".to_string()),
            ],
            &body,
            MAX_JSON_BYTES,
        )
        .map_err(|_| DeliveryFailure::Ambiguous("card_send_outcome_ambiguous"))?;
    let parsed = parse_success_json(&response, "card_send_outcome_ambiguous")?;
    parsed
        .pointer("/data/message_id")
        .and_then(Value::as_str)
        .filter(|value| valid_external_id(value))
        .map(str::to_string)
        .ok_or(DeliveryFailure::Ambiguous("card_send_outcome_ambiguous"))
}

#[cfg(any(test, feature = "xiaoman-feishu-poster-adapter"))]
fn send_status_message(
    config: &AdapterConfig,
    client: &HttpClient,
    token: &str,
    claim: &DeliveryClaim,
) -> std::result::Result<String, DeliveryFailure> {
    let endpoint = config
        .api_root
        .join("im/v1/messages?receive_id_type=chat_id")
        .map_err(|_| DeliveryFailure::Ambiguous("status_endpoint_invalid"))?;
    let message = if claim.candidate.notification_kind == "generation_ambiguous" {
        "海报生成结果暂不确定，系统已停止自动重试，请稍后向小满确认。"
    } else {
        "海报生成失败，本次任务未产生可审稿图片，请稍后重新发起。"
    };
    let content = serde_json::to_string(&json!({"text": message}))
        .map_err(|_| DeliveryFailure::Ambiguous("status_request_invalid"))?;
    let body = serde_json::to_vec(&json!({
        "receive_id": claim.candidate.conversation_id,
        "msg_type": "text",
        "content": content,
        "uuid": format!("poster-notification-{}", claim.candidate.notification_id)
    }))
    .map_err(|_| DeliveryFailure::Ambiguous("status_request_invalid"))?;
    let response = client
        .request(
            "POST",
            &endpoint,
            &[
                ("Authorization", format!("Bearer {token}")),
                ("Content-Type", "application/json".to_string()),
            ],
            &body,
            MAX_JSON_BYTES,
        )
        .map_err(|_| DeliveryFailure::Ambiguous("status_send_outcome_ambiguous"))?;
    let parsed = parse_success_json(&response, "status_send_outcome_ambiguous")?;
    parsed
        .pointer("/data/message_id")
        .and_then(Value::as_str)
        .filter(|value| valid_external_id(value))
        .map(str::to_string)
        .ok_or(DeliveryFailure::Ambiguous("status_send_outcome_ambiguous"))
}

#[cfg(any(test, feature = "xiaoman-feishu-poster-adapter"))]
fn parse_success_json(
    response: &HttpResponse,
    code: &'static str,
) -> std::result::Result<Value, DeliveryFailure> {
    if !(200..300).contains(&response.status) {
        return Err(DeliveryFailure::Ambiguous(code));
    }
    let value: Value =
        serde_json::from_slice(&response.body).map_err(|_| DeliveryFailure::Ambiguous(code))?;
    if value.get("code").and_then(Value::as_i64) != Some(0) {
        return Err(DeliveryFailure::Ambiguous(code));
    }
    Ok(value)
}

fn review_card(image_key: &str, claim: &DeliveryClaim) -> Value {
    let button = |text: &str, action: &str, button_type: &str| {
        json!({
            "tag": "button",
            "text": {"tag": "plain_text", "content": text},
            "type": button_type,
            "value": {
                "schema_version": 1,
                "notification_id": claim.candidate.notification_id,
                "artifact_id": claim.candidate.artifact_id,
                "action": action
            }
        })
    };
    json!({
        "schema": "2.0",
        "config": {"update_multi": true},
        "header": {"title": {"tag": "plain_text", "content": "海报已生成，待你审稿"}, "template": "blue"},
        "body": {"elements": [
            {"tag": "img", "img_key": image_key, "alt": {"tag": "plain_text", "content": "活动海报审核图"}, "mode": "fit_horizontal"},
            {"tag": "action", "actions": [
                button("通过", "approve", "primary"),
                button("修改", "modify", "default"),
                button("放弃", "abandon", "danger")
            ]}
        ]}
    })
}

fn multipart_image(boundary: &str, artifact_id: Uuid, bytes: &[u8]) -> Zeroizing<Vec<u8>> {
    let mut body = Zeroizing::new(Vec::with_capacity(bytes.len() + 512));
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"image_type\"\r\n\r\nmessage\r\n").as_bytes());
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"poster-{artifact_id}.jpg\"\r\nContent-Type: image/jpeg\r\n\r\n").as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

#[cfg(feature = "xiaoman-feishu-poster-adapter")]
async fn mark_sending(pool: &PgPool, claim: &DeliveryClaim, image_key: Option<&str>) -> Result<()> {
    let updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.poster_notification_attempts
        SET status = 'sending', image_key_hash = $2, send_started_at = now(), updated_at = now(),
            audit_metadata = audit_metadata || '{"external_upload_outcome":"accepted","external_send_outcome":"not_started"}'::jsonb
        WHERE id = $1 AND status = 'uploading' AND claim_token = $3
        "#,
    )
    .bind(claim.attempt_id)
    .bind(image_key.map(|value| sha256_marker(value.as_bytes())))
    .bind(&claim.claim_token)
    .execute(pool)
    .await
    .context("open poster card send gate")?;
    if updated.rows_affected() != 1 {
        bail!("poster delivery attempt changed before send gate");
    }
    Ok(())
}

#[cfg(feature = "xiaoman-feishu-poster-adapter")]
async fn complete_delivery(pool: &PgPool, claim: &DeliveryClaim, message_ref: &str) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("begin poster delivery completion")?;
    let message_hash = sha256_marker(message_ref.as_bytes());
    let attempt = sqlx::query(
        r#"UPDATE qintopia_agent_os.poster_notification_attempts
           SET status='delivered', external_message_ref_hash=$2, completed_at=now(), updated_at=now(),
               audit_metadata=audit_metadata || '{"external_send_outcome":"accepted","automatic_retry_allowed":false}'::jsonb
           WHERE id=$1 AND status='sending' AND claim_token=$3"#,
    ).bind(claim.attempt_id).bind(&message_hash).bind(&claim.claim_token).execute(&mut *tx).await?;
    if attempt.rows_affected() != 1 {
        bail!("poster delivery attempt changed before completion");
    }
    let notification = sqlx::query(
        r#"UPDATE qintopia_agent_os.poster_notifications
           SET status='delivered', external_message_ref_hash=$2, delivered_at=now(),
               claimed_by=NULL, claimed_at=NULL, claim_expires_at=NULL, updated_at=now()
           WHERE id=$1 AND status='claimed' AND claimed_by=$3"#,
    )
    .bind(claim.candidate.notification_id)
    .bind(&message_hash)
    .bind(&claim.claim_token)
    .execute(&mut *tx)
    .await?;
    if notification.rows_affected() != 1 {
        bail!("poster notification claim changed before completion");
    }
    release_work_item(&mut tx, claim, "completed", None).await?;
    append_event(&mut tx, claim.candidate.work_item_id, claim.candidate.artifact_id,
        "conversation_notification_delivered",
        json!({"attempt_id":claim.attempt_id,"external_message_ref_hash":message_hash,"external_send_executed":true,"group_send_authorized":false})).await?;
    tx.commit()
        .await
        .context("commit poster delivery completion")?;
    Ok(())
}

#[cfg(feature = "xiaoman-feishu-poster-adapter")]
async fn fail_delivery(
    pool: &PgPool,
    claim: &DeliveryClaim,
    code: &str,
    ambiguous: bool,
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("begin poster delivery failure")?;
    let status = if ambiguous { "ambiguous" } else { "failed" };
    let external = if ambiguous { Value::Null } else { json!(false) };
    let attempt = sqlx::query(
        r#"UPDATE qintopia_agent_os.poster_notification_attempts
           SET status=$2, failure_code=$3, completed_at=now(), updated_at=now(),
               audit_metadata=audit_metadata || jsonb_build_object('external_send_outcome', $4::text, 'automatic_retry_allowed', false)
           WHERE id=$1 AND status IN ('uploading','sending') AND claim_token=$5"#,
    ).bind(claim.attempt_id).bind(status).bind(code).bind(if ambiguous {"unknown"} else {"not_sent"}).bind(&claim.claim_token).execute(&mut *tx).await?;
    if attempt.rows_affected() != 1 {
        bail!("poster delivery attempt changed before failure");
    }
    let notification = sqlx::query(
        r#"UPDATE qintopia_agent_os.poster_notifications
           SET status=$2, last_error_code=$3, claimed_by=NULL, claimed_at=NULL,
               claim_expires_at=NULL, updated_at=now()
           WHERE id=$1 AND status='claimed' AND claimed_by=$4"#,
    )
    .bind(claim.candidate.notification_id)
    .bind(status)
    .bind(code)
    .bind(&claim.claim_token)
    .execute(&mut *tx)
    .await?;
    if notification.rows_affected() != 1 {
        bail!("poster notification claim changed before failure");
    }
    release_work_item(&mut tx, claim, "failed", Some(code)).await?;
    append_event(&mut tx, claim.candidate.work_item_id, claim.candidate.artifact_id,
        if ambiguous {"conversation_notification_ambiguous"} else {"conversation_notification_failed"},
        json!({"attempt_id":claim.attempt_id,"failure_code":code,"external_send_executed":external,"automatic_retry_allowed":false,"group_send_authorized":false})).await?;
    tx.commit()
        .await
        .context("commit poster delivery failure")?;
    Ok(())
}

#[cfg(any(
    feature = "xiaoman-feishu-poster-adapter",
    feature = "postgres-integration-tests"
))]
async fn reconcile_one_stale_claim(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    notification_id: Option<Uuid>,
) -> Result<()> {
    let row = sqlx::query(
        r#"SELECT attempt.id AS attempt_id, attempt.notification_id, attempt.claim_token,
                  notification.work_item_id, notification.generated_image_artifact_id AS artifact_id
           FROM qintopia_agent_os.poster_notification_attempts attempt
           JOIN qintopia_agent_os.poster_notifications notification ON notification.id=attempt.notification_id
           WHERE attempt.status IN ('uploading','sending') AND notification.status='claimed'
             AND notification.claimed_by=attempt.claim_token AND notification.claim_expires_at<=now()
             AND ($1::uuid IS NULL OR notification.id=$1)
           ORDER BY attempt.created_at ASC LIMIT 1
           FOR UPDATE OF attempt, notification SKIP LOCKED"#,
    ).bind(notification_id).fetch_optional(&mut **tx).await.context("lock stale poster delivery")?;
    let Some(row) = row else {
        return Ok(());
    };
    let attempt_id: Uuid = row.try_get("attempt_id")?;
    let notification_id: Uuid = row.try_get("notification_id")?;
    let work_item_id: Uuid = row.try_get("work_item_id")?;
    let artifact_id: Option<Uuid> = row.try_get("artifact_id")?;
    let claim_token: String = row.try_get("claim_token")?;
    sqlx::query("UPDATE qintopia_agent_os.poster_notification_attempts SET status='ambiguous', failure_code='claim_expired_outcome_ambiguous', completed_at=now(), updated_at=now(), audit_metadata=audit_metadata || '{\"external_send_outcome\":\"unknown\",\"automatic_retry_allowed\":false,\"reconciled_after_claim_expiry\":true}'::jsonb WHERE id=$1 AND claim_token=$2")
        .bind(attempt_id).bind(&claim_token).execute(&mut **tx).await?;
    sqlx::query("UPDATE qintopia_agent_os.poster_notifications SET status='ambiguous', last_error_code='claim_expired_outcome_ambiguous', claimed_by=NULL, claimed_at=NULL, claim_expires_at=NULL, updated_at=now() WHERE id=$1 AND claimed_by=$2")
        .bind(notification_id).bind(&claim_token).execute(&mut **tx).await?;
    sqlx::query("UPDATE qintopia_agent_os.work_items SET status='failed', last_error='claim_expired_outcome_ambiguous', claimed_by=NULL, locked_at=NULL, claim_expires_at=NULL, updated_at=now() WHERE id=$1 AND claimed_by=$2")
        .bind(work_item_id).bind(&claim_token).execute(&mut **tx).await?;
    append_event(tx, work_item_id, artifact_id, "conversation_notification_ambiguous",
        json!({"attempt_id":attempt_id,"failure_code":"claim_expired_outcome_ambiguous","external_send_executed":Value::Null,"automatic_retry_allowed":false,"reconciled_after_claim_expiry":true})).await?;
    Ok(())
}

async fn release_work_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    claim: &DeliveryClaim,
    status: &str,
    error: Option<&str>,
) -> Result<()> {
    let updated = sqlx::query("UPDATE qintopia_agent_os.work_items SET status=$2, last_error=$3, claimed_by=NULL, locked_at=NULL, claim_expires_at=NULL, updated_at=now() WHERE id=$1 AND status='processing' AND claimed_by=$4")
        .bind(claim.candidate.work_item_id).bind(status).bind(error).bind(&claim.claim_token).execute(&mut **tx).await?;
    if updated.rows_affected() != 1 {
        bail!("poster notification work item claim changed");
    }
    Ok(())
}

async fn append_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Uuid,
    artifact_id: Option<Uuid>,
    event_type: &str,
    data: Value,
) -> Result<()> {
    sqlx::query("INSERT INTO qintopia_agent_os.work_item_events (work_item_id, event_type, actor_type, actor_id, artifact_id, message, data) VALUES ($1,$2,'worker',$3,$4,$2,$5)")
        .bind(work_item_id).bind(event_type).bind(WORKER_ID).bind(artifact_id).bind(data).execute(&mut **tx).await?;
    Ok(())
}

fn required_env(name: &str) -> Result<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .with_context(|| format!("{name} is required"))
}

fn required_identifier_set(name: &str) -> Result<BTreeSet<String>> {
    let values = required_env(name)?
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    if values.is_empty() || values.iter().any(|value| !valid_external_id(value)) {
        bail!("{name} contains an invalid identifier");
    }
    Ok(values)
}

fn required_host_set(name: &str) -> Result<BTreeSet<String>> {
    let values = required_env(name)?
        .split(',')
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>();
    if values.is_empty()
        || values
            .iter()
            .any(|value| value.contains('/') || value.contains(':') || value.contains('@'))
    {
        bail!("{name} contains an invalid host");
    }
    Ok(values)
}

fn valid_external_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 240
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':' | b'.'))
}

fn canonical_sha256(value: &str) -> bool {
    value.len() == 71 && value.starts_with("sha256:") && is_lower_hex(&value[7..], 64)
}
fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn sha256_hex(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
fn sha256_marker(value: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        thread,
        time::Duration,
    };

    fn claim() -> DeliveryClaim {
        DeliveryClaim {
            candidate: DeliveryCandidate {
                notification_id: Uuid::new_v4(),
                work_item_id: Uuid::new_v4(),
                artifact_id: Some(Uuid::new_v4()),
                artifact_uri: Some("https://media.example.test/poster.jpg".to_string()),
                content_hash: Some(format!("sha256:{}", "a".repeat(64))),
                mime_type: Some("image/jpeg".to_string()),
                byte_size: Some(3),
                notification_kind: "image_ready".to_string(),
                failure_code: None,
                conversation_id: "oc_fixture".to_string(),
                requester_user_id: "ou_fixture".to_string(),
            },
            attempt_id: Uuid::new_v4(),
            claim_token: "claim-fixture".to_string(),
        }
    }

    #[test]
    fn claim_query_does_not_lock_nullable_artifact_join() {
        let query = candidate_select(true);
        assert!(query.contains("FOR UPDATE OF notification, item SKIP LOCKED"));
        assert!(!query.contains("FOR UPDATE OF notification, item, artifact"));
    }

    #[test]
    fn card_contains_bound_actions_and_no_group_target() {
        let claim = claim();
        let card = review_card("img_fixture", &claim);
        let raw = serde_json::to_string(&card).unwrap();
        assert!(raw.contains("approve") && raw.contains("modify") && raw.contains("abandon"));
        assert!(raw.contains(&claim.candidate.notification_id.to_string()));
        assert!(!raw.contains("group_message_request"));
        assert!(!raw.contains(&claim.candidate.conversation_id));
    }

    #[test]
    fn multipart_upload_preserves_exact_jpeg_bytes() {
        let jpeg = [0xff, 0xd8, 0xff, 0x01, 0x02, 0xff, 0xd9];
        let body = multipart_image("boundary", Uuid::nil(), &jpeg);
        assert!(body.windows(jpeg.len()).any(|window| window == jpeg));
        assert!(String::from_utf8_lossy(&body).contains("image_type"));
    }

    #[test]
    fn candidate_rejects_non_allowlisted_media_and_target() {
        let claim = claim();
        let config = AdapterConfig {
            api_root: Url::parse(OFFICIAL_API_ROOT).unwrap(),
            app_id: Zeroizing::new("app".into()),
            app_secret: Zeroizing::new("secret".into()),
            allowed_chat_ids: BTreeSet::from(["oc_fixture".into()]),
            allowed_user_ids: BTreeSet::from(["ou_fixture".into()]),
            media_allowed_hosts: BTreeSet::from(["media.example.test".into()]),
            max_media_bytes: 1024,
        };
        assert!(validate_candidate(&claim.candidate, &config).is_ok());
        let mut bad = claim.candidate;
        bad.artifact_uri = Some("https://other.example.test/poster.jpg".into());
        assert!(validate_candidate(&bad, &config).is_err());
    }

    #[cfg(feature = "postgres-integration-tests")]
    fn postgres_integration_database_url() -> String {
        assert_eq!(
            std::env::var("QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE").as_deref(),
            Ok("1"),
            "PostgreSQL integration test requires the explicit apply-smoke guard"
        );
        let database_url = std::env::var("QINTOPIA_SIDECAR_DATABASE_URL")
            .expect("PostgreSQL integration test requires QINTOPIA_SIDECAR_DATABASE_URL");
        let parsed = Url::parse(&database_url).expect("integration database URL must parse");
        assert!(
            matches!(parsed.scheme(), "postgres" | "postgresql"),
            "PostgreSQL integration test requires a postgres URL"
        );
        assert!(
            matches!(parsed.host_str(), Some("127.0.0.1" | "::1")),
            "PostgreSQL integration test may only use a literal loopback database"
        );
        assert_eq!(
            parsed.path().trim_start_matches('/'),
            "qintopia_test",
            "PostgreSQL integration test may only use qintopia_test"
        );
        database_url
    }

    #[cfg(feature = "postgres-integration-tests")]
    async fn seed_claim_candidate(
        pool: &PgPool,
        suffix: &str,
        label: &str,
        conversation_id: &str,
        requester_user_id: &str,
        created_second: i32,
    ) -> (Uuid, Uuid) {
        let root_id = Uuid::new_v4();
        let image_work_item_id = Uuid::new_v4();
        let notification_work_item_id = Uuid::new_v4();
        let artifact_id = Uuid::new_v4();
        let notification_id = Uuid::new_v4();
        let origin_ref = format!("poster-claim-{label}-{suffix}");

        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.work_items
                (id, parent_work_item_id, work_item_type, status, requester_agent,
                 target_agent, capability_key, brief_summary, dedupe_key, idempotency_key)
            VALUES
                ($1, NULL, 'poster_production_request', 'completed', 'xiaoman', 'xiaoman',
                 'xiaoman.notify_direct_conversation', 'claim root fixture', $4, $5),
                ($2, $1, 'image_generation_request', 'completed', 'xiaoman', 'huabaosi',
                 'xiaoman.notify_direct_conversation', 'claim image fixture', $6, $7),
                ($3, $1, 'conversation_notification_request', 'queued', 'xiaoman', 'xiaoman',
                 'xiaoman.notify_direct_conversation', 'claim notification fixture', $8, $9)
            "#,
        )
        .bind(root_id)
        .bind(image_work_item_id)
        .bind(notification_work_item_id)
        .bind(format!("claim-root-dedupe-{label}-{suffix}"))
        .bind(format!("claim-root-idempotency-{label}-{suffix}"))
        .bind(format!("claim-image-dedupe-{label}-{suffix}"))
        .bind(format!("claim-image-idempotency-{label}-{suffix}"))
        .bind(format!("claim-notification-dedupe-{label}-{suffix}"))
        .bind(format!("claim-notification-idempotency-{label}-{suffix}"))
        .execute(pool)
        .await
        .expect("seed poster claim work items");

        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.poster_return_targets
                (origin_ref, platform, conversation_type, conversation_id,
                 requester_user_id, source_message_id)
            VALUES ($1, 'feishu', 'direct', $2, $3, $4)
            "#,
        )
        .bind(&origin_ref)
        .bind(conversation_id)
        .bind(requester_user_id)
        .bind(format!("om-{label}-{suffix}"))
        .execute(pool)
        .await
        .expect("seed poster return target");

        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.artifacts
                (id, work_item_id, artifact_type, review_status, created_by_agent,
                 title, summary, artifact_uri, content_hash, metadata)
            VALUES ($1, $2, 'generated_image', 'pending', 'huabaosi',
                    'claim image fixture', 'sanitized fixture', $3, $4,
                    '{"mime_type":"image/jpeg","byte_size":3}'::jsonb)
            "#,
        )
        .bind(artifact_id)
        .bind(image_work_item_id)
        .bind(format!("https://media.example.test/{label}-{suffix}.jpg"))
        .bind(format!("sha256:{}", "a".repeat(64)))
        .execute(pool)
        .await
        .expect("seed poster claim artifact");

        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.poster_notifications
                (id, work_item_id, source_work_item_id, workflow_root_id,
                 generated_image_artifact_id, notification_kind, origin_ref, status,
                 created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, 'image_ready', $6, 'pending',
                    TIMESTAMPTZ '1900-01-01 00:00:00+00' + ($7 * interval '1 second'),
                    now())
            "#,
        )
        .bind(notification_id)
        .bind(notification_work_item_id)
        .bind(image_work_item_id)
        .bind(root_id)
        .bind(artifact_id)
        .bind(&origin_ref)
        .bind(created_second)
        .execute(pool)
        .await
        .expect("seed poster notification claim candidate");

        (notification_id, notification_work_item_id)
    }

    #[tokio::test]
    #[cfg(feature = "postgres-integration-tests")]
    #[ignore = "requires guarded disposable PostgreSQL qintopia_test"]
    async fn postgres_rejected_candidate_does_not_block_next_notification() {
        let database_url = postgres_integration_database_url();
        let pool = db::connect(&database_url, 2)
            .await
            .expect("connect guarded poster claim integration database");
        db::run_migrations(&pool)
            .await
            .expect("migrate guarded poster claim integration database");
        let suffix = Uuid::new_v4().simple().to_string();
        let (rejected_notification_id, rejected_work_item_id) = seed_claim_candidate(
            &pool,
            &suffix,
            "rejected",
            "oc_not_allowlisted",
            "ou_fixture",
            0,
        )
        .await;
        let (eligible_notification_id, _) =
            seed_claim_candidate(&pool, &suffix, "eligible", "oc_fixture", "ou_fixture", 1).await;
        let config = test_config(Url::parse(OFFICIAL_API_ROOT).unwrap());

        let rejected = claim_notification(&pool, None, &config)
            .await
            .expect("terminalize oldest rejected candidate");
        assert!(matches!(
            rejected,
            ClaimNotificationOutcome::Rejected { notification_id, .. }
                if notification_id == rejected_notification_id
        ));
        let rejected_state: (String, Option<String>, String, Option<String>, i64, Value) =
            sqlx::query_as(
                r#"
                SELECT notification.status, notification.last_error_code,
                       item.status, item.last_error,
                       count(attempt.id), event.data
                FROM qintopia_agent_os.poster_notifications notification
                JOIN qintopia_agent_os.work_items item ON item.id=notification.work_item_id
                LEFT JOIN qintopia_agent_os.poster_notification_attempts attempt
                  ON attempt.notification_id=notification.id
                JOIN qintopia_agent_os.work_item_events event
                  ON event.work_item_id=item.id
                 AND event.event_type='conversation_notification_failed'
                WHERE notification.id=$1 AND item.id=$2
                GROUP BY notification.status, notification.last_error_code,
                         item.status, item.last_error, event.data
                "#,
            )
            .bind(rejected_notification_id)
            .bind(rejected_work_item_id)
            .fetch_one(&pool)
            .await
            .expect("load rejected notification terminal state");
        assert_eq!(rejected_state.0, "failed");
        assert_eq!(
            rejected_state.1.as_deref(),
            Some("return_target_not_allowlisted")
        );
        assert_eq!(rejected_state.2, "failed");
        assert_eq!(
            rejected_state.3.as_deref(),
            Some("return_target_not_allowlisted")
        );
        assert_eq!(
            rejected_state.4, 0,
            "policy rejection must not create an attempt"
        );
        assert_eq!(rejected_state.5["external_send_executed"], false);
        assert_eq!(rejected_state.5["group_send_authorized"], false);
        assert_eq!(rejected_state.5["rejected_before_external_io"], true);

        let claimed = claim_notification(&pool, None, &config)
            .await
            .expect("claim next eligible candidate");
        let claim = match claimed {
            ClaimNotificationOutcome::Claimed(claim) => claim,
            other => panic!("expected next candidate to be claimed, got {other:?}"),
        };
        assert_eq!(claim.candidate.notification_id, eligible_notification_id);
        let eligible_state: (String, String, i64) = sqlx::query_as(
            r#"
            SELECT notification.status, item.status, count(attempt.id)
            FROM qintopia_agent_os.poster_notifications notification
            JOIN qintopia_agent_os.work_items item ON item.id=notification.work_item_id
            LEFT JOIN qintopia_agent_os.poster_notification_attempts attempt
              ON attempt.notification_id=notification.id
            WHERE notification.id=$1
            GROUP BY notification.status, item.status
            "#,
        )
        .bind(eligible_notification_id)
        .fetch_one(&pool)
        .await
        .expect("load eligible notification claim state");
        assert_eq!(
            eligible_state,
            ("claimed".to_string(), "processing".to_string(), 1)
        );
    }

    #[test]
    fn fake_feishu_accepts_token_exact_jpeg_and_review_card() {
        let (token_root, token_request) =
            serve_once(r#"{"code":0,"tenant_access_token":"tenant_fixture"}"#);
        let mut config = test_config(token_root);
        let client = HttpClient::test_only_with_timeout(Duration::from_secs(2));
        let token = tenant_token(&config, &client).expect("tenant token");
        let token_request = token_request.recv().expect("token request");
        assert!(String::from_utf8_lossy(&token_request)
            .starts_with("POST /open-apis/auth/v3/tenant_access_token/internal"));

        let (upload_root, upload_request) =
            serve_once(r#"{"code":0,"data":{"image_key":"img_fixture"}}"#);
        config.api_root = upload_root;
        let jpeg = [0xff, 0xd8, 0xff, 0x11, 0x22, 0xff, 0xd9];
        let image_key = upload_image(
            &config,
            &client,
            &token,
            claim().candidate.artifact_id.unwrap(),
            &jpeg,
        )
        .expect("image upload");
        let upload_request = upload_request.recv().expect("upload request");
        assert!(
            String::from_utf8_lossy(&upload_request).starts_with("POST /open-apis/im/v1/images")
        );
        assert!(upload_request
            .windows(jpeg.len())
            .any(|window| window == jpeg));

        let (message_root, message_request) =
            serve_once(r#"{"code":0,"data":{"message_id":"om_fixture"}}"#);
        config.api_root = message_root;
        let claim = claim();
        let message_id = send_review_card(&config, &client, &token, &image_key, &claim)
            .expect("review card send");
        assert_eq!(message_id, "om_fixture");
        let message_request = message_request.recv().expect("message request");
        let request_text = String::from_utf8_lossy(&message_request);
        assert!(request_text.starts_with("POST /open-apis/im/v1/messages?receive_id_type=chat_id"));
        assert!(request_text.contains("oc_fixture"));
        assert!(request_text.contains("approve"));
        assert!(request_text.contains("modify"));
        assert!(request_text.contains("abandon"));
        assert!(!request_text.contains("group_message_request"));
    }

    #[test]
    fn fake_feishu_accepts_sanitized_generation_failure_status() {
        let (message_root, message_request) =
            serve_once(r#"{"code":0,"data":{"message_id":"om_status_fixture"}}"#);
        let config = test_config(message_root);
        let client = HttpClient::test_only_with_timeout(Duration::from_secs(2));
        let mut claim = claim();
        claim.candidate.notification_kind = "generation_failed".to_string();
        claim.candidate.failure_code = Some("generation_failed".to_string());
        claim.candidate.artifact_id = None;
        claim.candidate.artifact_uri = None;
        claim.candidate.content_hash = None;
        claim.candidate.mime_type = None;
        claim.candidate.byte_size = None;

        let message_id = send_status_message(&config, &client, "tenant_fixture", &claim)
            .expect("status notification send");
        assert_eq!(message_id, "om_status_fixture");
        let request = String::from_utf8_lossy(&message_request.recv().unwrap()).to_string();
        assert!(request.contains("海报生成失败"));
        assert!(!request.contains("provider"));
        assert!(!request.contains("file_path"));
        assert!(!request.contains("group_message_request"));
    }

    fn test_config(api_root: Url) -> AdapterConfig {
        AdapterConfig {
            api_root,
            app_id: Zeroizing::new("cli_fixture".into()),
            app_secret: Zeroizing::new("secret_fixture".into()),
            allowed_chat_ids: BTreeSet::from(["oc_fixture".into()]),
            allowed_user_ids: BTreeSet::from(["ou_fixture".into()]),
            media_allowed_hosts: BTreeSet::from(["media.example.test".into()]),
            max_media_bytes: 1024,
        }
    }

    fn serve_once(body: &'static str) -> (Url, mpsc::Receiver<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake Feishu server");
        let address = listener.local_addr().expect("fake Feishu address");
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept fake Feishu request");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set fake server timeout");
            let request = read_http_request(&mut stream);
            sender.send(request).expect("record fake Feishu request");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write fake Feishu response");
        });
        (
            Url::parse(&format!("http://{address}/open-apis/")).expect("fake API root"),
            receiver,
        )
    }

    fn read_http_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        let mut expected = None;
        loop {
            let count = stream.read(&mut buffer).expect("read fake request");
            if count == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..count]);
            if expected.is_none() {
                if let Some(header_end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..header_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    expected = Some(header_end + 4 + content_length);
                }
            }
            if expected.is_some_and(|length| request.len() >= length) {
                break;
            }
        }
        request
    }
}
