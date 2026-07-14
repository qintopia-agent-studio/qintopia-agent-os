use std::collections::BTreeSet;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Row};
use url::Url;
use uuid::Uuid;
use zeroize::Zeroize;

use crate::qiwe_image_send::QiweSendReceipt;

const WORKER_ID: &str = "qiwe-image-send-adapter";
const WORK_ITEM_TYPE: &str = "group_message_request";
const CAPABILITY_KEY: &str = "erhua.send_group_message";
const CLAIM_TTL_MINUTES: i64 = 10;
const SEND_CLAIM_TTL_MINUTES: i64 = 2;
const MAX_CALLBACK_PAYLOAD_BYTES: usize = 64 * 1024;

#[derive(Clone)]
pub struct QiweUploadClaim {
    pub attempt_id: Uuid,
    pub work_item_id: Uuid,
    pub generated_image_artifact_id: Uuid,
    pub attempt_number: i32,
    pub claim_token: String,
    pub artifact_uri: String,
    pub artifact_content_hash: String,
    pub artifact_file_md5: String,
    pub artifact_byte_size: u64,
    pub filename: String,
    pub target_group_id: String,
}

impl Drop for QiweUploadClaim {
    fn drop(&mut self) {
        self.claim_token.zeroize();
        self.artifact_uri.zeroize();
        self.filename.zeroize();
        self.target_group_id.zeroize();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QiweUploadPreview {
    pub work_item_id: Uuid,
}

#[derive(Clone, PartialEq, Eq)]
pub struct QiweCallbackSendClaim {
    pub attempt_id: Uuid,
    pub work_item_id: Uuid,
    pub generated_image_artifact_id: Uuid,
    pub claim_token: String,
    pub target_group_id: String,
}

pub struct QiweCallbackFileIdentity<'a> {
    pub filename: &'a str,
    pub file_md5: &'a str,
    pub file_size: u64,
}

impl Drop for QiweCallbackSendClaim {
    fn drop(&mut self) {
        self.claim_token.zeroize();
        self.target_group_id.zeroize();
    }
}

impl std::fmt::Debug for QiweCallbackSendClaim {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("QiweCallbackSendClaim")
            .field("attempt_id", &self.attempt_id)
            .field("work_item_id", &self.work_item_id)
            .field(
                "generated_image_artifact_id",
                &self.generated_image_artifact_id,
            )
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallbackClaimOutcome {
    Ready(QiweCallbackSendClaim),
    Duplicate { status: String },
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendFailureDisposition {
    Rejected,
    Ambiguous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadFailureDisposition {
    Rejected,
    OutcomeUnknown,
}

struct StoredAttempt {
    id: Uuid,
    work_item_id: Uuid,
    generated_image_artifact_id: Uuid,
    status: String,
    claim_token: String,
    request_id_sha256: String,
    callback_payload_sha256: Option<String>,
    target_group_sha256: String,
    artifact_content_hash: String,
    artifact_uri_sha256: String,
    artifact_file_md5: String,
    artifact_byte_size: u64,
}

struct ArtifactBoundary<'a> {
    uri: &'a str,
    content_hash: &'a str,
    mime_type: &'a str,
    file_md5: &'a str,
    byte_size: u64,
    target_group_id: &'a str,
}

struct ValidatedArtifactIdentity {
    filename: String,
    file_md5: String,
    byte_size: u64,
}

pub async fn claim_ready_work_item(
    pool: &PgPool,
    work_item_id: Option<Uuid>,
    allowed_group_ids: &BTreeSet<String>,
    media_allowed_hosts: &BTreeSet<String>,
) -> Result<Option<QiweUploadClaim>> {
    let claim_token = format!("{WORKER_ID}:{}", Uuid::new_v4());
    let mut tx = pool
        .begin()
        .await
        .context("begin QiWe image-send claim transaction")?;
    reconcile_one_stale_sending_attempt(&mut tx, work_item_id).await?;
    expire_one_stale_awaiting_callback(&mut tx, work_item_id).await?;
    reconcile_one_stale_uploading_attempt(&mut tx, work_item_id).await?;
    terminalize_one_stale_unrecorded_claim(&mut tx, work_item_id).await?;
    let row = sqlx::query(
        r#"
        WITH claimable AS (
            SELECT
                request.id,
                artifact.id AS generated_image_artifact_id,
                artifact.artifact_uri,
                artifact.content_hash AS artifact_content_hash,
                artifact.metadata->>'mime_type' AS mime_type,
                artifact.metadata->>'file_md5' AS artifact_file_md5,
                artifact.metadata->>'byte_size' AS artifact_byte_size,
                request.payload->>'target_group_id' AS target_group_id,
                COALESCE((
                    SELECT max(attempt.attempt_number) + 1
                    FROM qintopia_agent_os.qiwe_image_send_attempts attempt
                    WHERE attempt.work_item_id = request.id
                ), 1) AS attempt_number
            FROM qintopia_agent_os.work_items request
            JOIN qintopia_agent_os.artifacts artifact
              ON artifact.id::text = request.payload->>'approved_artifact_id'
             AND artifact.artifact_type = 'generated_image'
             AND artifact.review_status = 'approved'
             AND artifact.created_by_agent = 'huabaosi'
            JOIN qintopia_agent_os.work_items image_request
              ON image_request.id = artifact.work_item_id
             AND image_request.work_item_type = 'image_generation_request'
             AND image_request.capability_key = 'huabaosi.generate_image_asset'
             AND image_request.target_agent = 'huabaosi'
             AND image_request.status = 'completed'
            WHERE request.work_item_type = $1
              AND request.capability_key = $2
              AND request.requester_agent = 'xiaoman'
              AND request.target_agent = 'erhua'
              AND request.review_policy = 'human_final_confirmation'
              AND request.status = 'queued'
              AND request.available_at <= now()
              AND request.payload->>'target_channel' = 'qiwe'
              AND COALESCE(request.payload->>'target_group_id', '') <> ''
              AND ($3::uuid IS NULL OR request.id = $3)
              AND EXISTS (
                  SELECT 1
                  FROM qintopia_agent_os.work_item_events confirmation
                  WHERE confirmation.work_item_id = request.id
                    AND confirmation.event_type = 'group_message_final_confirmation_recorded'
                    AND confirmation.data->>'decision' = 'confirmed'
                    AND confirmation.data->>'current_status' = 'queued'
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
              AND EXISTS (
                  SELECT 1
                  FROM qintopia_agent_os.work_item_events created
                  WHERE created.work_item_id = artifact.work_item_id
                    AND created.artifact_id = artifact.id
                    AND created.event_type = 'generated_image_created'
              )
              AND NOT EXISTS (
                  SELECT 1
                  FROM qintopia_agent_os.qiwe_image_send_attempts attempt
                  WHERE attempt.work_item_id = request.id
                    AND attempt.status IN ('uploading', 'awaiting_callback', 'sending', 'sent')
              )
            ORDER BY request.priority DESC, request.available_at ASC, request.created_at ASC
            LIMIT 1
            FOR UPDATE OF request, artifact, image_request SKIP LOCKED
        )
        UPDATE qintopia_agent_os.work_items request
        SET
            status = 'processing',
            claimed_by = $4,
            locked_at = now(),
            claim_expires_at = now() + make_interval(mins => $5),
            attempts = attempts + 1,
            updated_at = now()
        FROM claimable
        WHERE request.id = claimable.id
        RETURNING
            request.id,
            claimable.generated_image_artifact_id,
            claimable.attempt_number,
            request.claimed_by AS claim_token,
            claimable.artifact_uri,
            claimable.artifact_content_hash,
            claimable.mime_type,
            claimable.artifact_file_md5,
            claimable.artifact_byte_size,
            claimable.target_group_id
        "#,
    )
    .bind(WORK_ITEM_TYPE)
    .bind(CAPABILITY_KEY)
    .bind(work_item_id)
    .bind(&claim_token)
    .bind(CLAIM_TTL_MINUTES as i32)
    .fetch_optional(&mut *tx)
    .await
    .context("claim send-ready QiWe image work item")?;

    let Some(row) = row else {
        tx.commit()
            .await
            .context("commit empty QiWe image-send claim")?;
        return Ok(None);
    };
    let artifact_uri: String = row
        .try_get("artifact_uri")
        .context("approved generated image is missing artifact_uri")?;
    let artifact_content_hash: String = row
        .try_get("artifact_content_hash")
        .context("approved generated image is missing content_hash")?;
    let mime_type: String = row
        .try_get("mime_type")
        .context("approved generated image is missing mime_type")?;
    let artifact_file_md5: String = row
        .try_get("artifact_file_md5")
        .context("approved generated image is missing file_md5")?;
    let artifact_byte_size = parse_positive_byte_size(
        &row.try_get::<String, _>("artifact_byte_size")
            .context("approved generated image is missing byte_size")?,
        "approved generated-image byte_size",
    )?;
    let target_group_id: String = row.try_get("target_group_id")?;
    let identity = validate_claim_boundary(
        ArtifactBoundary {
            uri: &artifact_uri,
            content_hash: &artifact_content_hash,
            mime_type: &mime_type,
            file_md5: &artifact_file_md5,
            byte_size: artifact_byte_size,
            target_group_id: &target_group_id,
        },
        allowed_group_ids,
        media_allowed_hosts,
    )?;
    let work_item_id: Uuid = row.try_get("id")?;
    let generated_image_artifact_id: Uuid = row.try_get("generated_image_artifact_id")?;
    let attempt_number: i32 = row.try_get("attempt_number")?;
    let stored_claim_token: String = row.try_get("claim_token")?;
    let target_group_sha256 = sha256_marker(target_group_id.as_bytes());
    let artifact_uri_sha256 = sha256_marker(artifact_uri.as_bytes());
    let attempt_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO qintopia_agent_os.qiwe_image_send_attempts
            (
                work_item_id,
                generated_image_artifact_id,
                attempt_number,
                status,
                claim_token,
                request_id_sha256,
                target_group_sha256,
                artifact_content_hash,
                artifact_uri_sha256,
                artifact_file_md5,
                artifact_byte_size,
                audit_metadata
            )
        VALUES
            ($1, $2, $3, 'uploading', $4, NULL, $5, $6, $7, $8, $9, $10)
        RETURNING id
        "#,
    )
    .bind(work_item_id)
    .bind(generated_image_artifact_id)
    .bind(attempt_number)
    .bind(&stored_claim_token)
    .bind(&target_group_sha256)
    .bind(&artifact_content_hash)
    .bind(&artifact_uri_sha256)
    .bind(&identity.file_md5)
    .bind(identity.byte_size as i64)
    .bind(json!({
        "automatic_retry_allowed": false,
        "callback_credentials_persisted": false,
        "external_upload_outcome": "not_started",
        "external_send_executed": false,
        "protocol": "qiwe_async_url_upload_then_send_image"
    }))
    .fetch_one(&mut *tx)
    .await
    .context("record QiWe upload attempt before external request")?;
    append_event(
        &mut tx,
        work_item_id,
        Some(generated_image_artifact_id),
        "qiwe_image_upload_started",
        json!({
            "attempt_id": attempt_id,
            "attempt_number": attempt_number,
            "target_group_sha256": target_group_sha256,
            "artifact_content_hash": artifact_content_hash,
            "artifact_uri_sha256": artifact_uri_sha256,
            "automatic_retry_allowed": false,
            "external_upload_outcome": "not_started",
            "external_send_executed": false,
            "send_executed": false
        }),
    )
    .await?;
    let claim = QiweUploadClaim {
        attempt_id,
        work_item_id,
        generated_image_artifact_id,
        attempt_number,
        claim_token: stored_claim_token,
        artifact_uri,
        artifact_content_hash,
        artifact_file_md5: identity.file_md5,
        artifact_byte_size: identity.byte_size,
        filename: identity.filename,
        target_group_id,
    };
    tx.commit().await.context("commit QiWe image-send claim")?;
    Ok(Some(claim))
}

pub async fn preview_ready_work_item(
    pool: &PgPool,
    work_item_id: Option<Uuid>,
    allowed_group_ids: &BTreeSet<String>,
    media_allowed_hosts: &BTreeSet<String>,
) -> Result<Option<QiweUploadPreview>> {
    let row = sqlx::query(
        r#"
        SELECT
            request.id,
            artifact.artifact_uri,
            artifact.content_hash AS artifact_content_hash,
            artifact.metadata->>'mime_type' AS mime_type,
            artifact.metadata->>'file_md5' AS artifact_file_md5,
            artifact.metadata->>'byte_size' AS artifact_byte_size,
            request.payload->>'target_group_id' AS target_group_id
        FROM qintopia_agent_os.work_items request
        JOIN qintopia_agent_os.artifacts artifact
          ON artifact.id::text = request.payload->>'approved_artifact_id'
         AND artifact.artifact_type = 'generated_image'
         AND artifact.review_status = 'approved'
         AND artifact.created_by_agent = 'huabaosi'
        JOIN qintopia_agent_os.work_items image_request
          ON image_request.id = artifact.work_item_id
         AND image_request.work_item_type = 'image_generation_request'
         AND image_request.capability_key = 'huabaosi.generate_image_asset'
         AND image_request.target_agent = 'huabaosi'
         AND image_request.status = 'completed'
        WHERE request.work_item_type = $1
          AND request.capability_key = $2
          AND request.requester_agent = 'xiaoman'
          AND request.target_agent = 'erhua'
          AND request.review_policy = 'human_final_confirmation'
          AND request.status = 'queued'
          AND request.available_at <= now()
          AND request.payload->>'target_channel' = 'qiwe'
          AND COALESCE(request.payload->>'target_group_id', '') <> ''
          AND ($3::uuid IS NULL OR request.id = $3)
          AND EXISTS (
              SELECT 1
              FROM qintopia_agent_os.work_item_events confirmation
              WHERE confirmation.work_item_id = request.id
                AND confirmation.event_type = 'group_message_final_confirmation_recorded'
                AND confirmation.data->>'decision' = 'confirmed'
                AND confirmation.data->>'current_status' = 'queued'
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
          AND EXISTS (
              SELECT 1
              FROM qintopia_agent_os.work_item_events created
              WHERE created.work_item_id = artifact.work_item_id
                AND created.artifact_id = artifact.id
                AND created.event_type = 'generated_image_created'
          )
          AND NOT EXISTS (
              SELECT 1
              FROM qintopia_agent_os.qiwe_image_send_attempts attempt
              WHERE attempt.work_item_id = request.id
                AND attempt.status IN ('uploading', 'awaiting_callback', 'sending', 'sent')
          )
        ORDER BY request.priority DESC, request.available_at ASC, request.created_at ASC
        LIMIT 1
        "#,
    )
    .bind(WORK_ITEM_TYPE)
    .bind(CAPABILITY_KEY)
    .bind(work_item_id)
    .fetch_optional(pool)
    .await
    .context("preview send-ready QiWe image work item")?;
    let Some(row) = row else {
        return Ok(None);
    };
    let artifact_uri: String = row.try_get("artifact_uri")?;
    let artifact_content_hash: String = row.try_get("artifact_content_hash")?;
    let mime_type: String = row.try_get("mime_type")?;
    let artifact_file_md5: String = row.try_get("artifact_file_md5")?;
    let artifact_byte_size = parse_positive_byte_size(
        &row.try_get::<String, _>("artifact_byte_size")?,
        "approved generated-image byte_size",
    )?;
    let target_group_id: String = row.try_get("target_group_id")?;
    validate_claim_boundary(
        ArtifactBoundary {
            uri: &artifact_uri,
            content_hash: &artifact_content_hash,
            mime_type: &mime_type,
            file_md5: &artifact_file_md5,
            byte_size: artifact_byte_size,
            target_group_id: &target_group_id,
        },
        allowed_group_ids,
        media_allowed_hosts,
    )?;
    Ok(Some(QiweUploadPreview {
        work_item_id: row.try_get("id")?,
    }))
}

pub async fn record_upload_acceptance(
    pool: &PgPool,
    claim: &QiweUploadClaim,
    request_id: &str,
) -> Result<Uuid> {
    validate_plain_value(request_id, "QiWe upload request id")?;
    let request_id_sha256 = sha256_marker(request_id.as_bytes());
    let target_group_sha256 = sha256_marker(claim.target_group_id.as_bytes());
    let artifact_uri_sha256 = sha256_marker(claim.artifact_uri.as_bytes());
    let mut tx = pool
        .begin()
        .await
        .context("begin QiWe upload-acceptance transaction")?;
    lock_current_claim(&mut tx, claim).await?;
    let attempt_id: Uuid = sqlx::query_scalar(
        r#"
        UPDATE qintopia_agent_os.qiwe_image_send_attempts
        SET
            status = 'awaiting_callback',
            request_id_sha256 = $6,
            failure_code = NULL,
            audit_metadata = audit_metadata || $12,
            updated_at = now()
        WHERE id = $1
          AND work_item_id = $2
          AND generated_image_artifact_id = $3
          AND attempt_number = $4
          AND claim_token = $5
          AND status = 'uploading'
          AND request_id_sha256 IS NULL
          AND target_group_sha256 = $7
          AND artifact_content_hash = $8
          AND artifact_uri_sha256 = $9
          AND artifact_file_md5 = $10
          AND artifact_byte_size = $11
        RETURNING id
        "#,
    )
    .bind(claim.attempt_id)
    .bind(claim.work_item_id)
    .bind(claim.generated_image_artifact_id)
    .bind(claim.attempt_number)
    .bind(&claim.claim_token)
    .bind(&request_id_sha256)
    .bind(&target_group_sha256)
    .bind(&claim.artifact_content_hash)
    .bind(&artifact_uri_sha256)
    .bind(&claim.artifact_file_md5)
    .bind(claim.artifact_byte_size as i64)
    .bind(json!({
        "callback_credentials_persisted": false,
        "external_upload_outcome": "accepted",
        "external_send_executed": false,
        "protocol": "qiwe_async_url_upload_then_send_image"
    }))
    .fetch_optional(&mut *tx)
    .await
    .context("record hashed QiWe upload acceptance")?
    .ok_or_else(|| anyhow!("QiWe uploading attempt changed before acceptance was recorded"))?;
    append_event(
        &mut tx,
        claim.work_item_id,
        Some(claim.generated_image_artifact_id),
        "qiwe_image_upload_accepted",
        json!({
            "attempt_id": attempt_id,
            "attempt_number": claim.attempt_number,
            "request_id_sha256": request_id_sha256,
            "target_group_sha256": target_group_sha256,
            "artifact_content_hash": claim.artifact_content_hash,
            "artifact_uri_sha256": artifact_uri_sha256,
            "callback_credentials_persisted": false,
            "send_executed": false
        }),
    )
    .await?;
    tx.commit().await.context("commit QiWe upload acceptance")?;
    Ok(attempt_id)
}

pub async fn record_upload_failure(
    pool: &PgPool,
    claim: &QiweUploadClaim,
    disposition: UploadFailureDisposition,
) -> Result<()> {
    let (attempt_status, failure_code, external_upload_outcome, request_may_have_been_accepted) =
        match disposition {
            UploadFailureDisposition::Rejected => {
                ("failed", "qiwe_upload_rejected", "rejected", Some(false))
            }
            UploadFailureDisposition::OutcomeUnknown => (
                "ambiguous",
                "qiwe_upload_outcome_ambiguous",
                "unknown",
                None,
            ),
        };
    let mut tx = pool
        .begin()
        .await
        .context("begin QiWe upload-failure transaction")?;
    lock_current_claim(&mut tx, claim).await?;
    let attempt_updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.qiwe_image_send_attempts
        SET
            status = $6,
            failure_code = $7,
            audit_metadata = audit_metadata || $8,
            completed_at = now(),
            updated_at = now()
        WHERE id = $1
          AND work_item_id = $2
          AND generated_image_artifact_id = $3
          AND attempt_number = $4
          AND claim_token = $5
          AND status = 'uploading'
          AND request_id_sha256 IS NULL
        "#,
    )
    .bind(claim.attempt_id)
    .bind(claim.work_item_id)
    .bind(claim.generated_image_artifact_id)
    .bind(claim.attempt_number)
    .bind(&claim.claim_token)
    .bind(attempt_status)
    .bind(failure_code)
    .bind(json!({
        "automatic_retry_allowed": false,
        "external_upload_outcome": external_upload_outcome,
        "request_may_have_been_accepted": request_may_have_been_accepted,
        "external_send_executed": false
    }))
    .execute(&mut *tx)
    .await
    .context("record QiWe upload attempt failure")?;
    if attempt_updated.rows_affected() != 1 {
        bail!("QiWe uploading attempt changed before failure was recorded");
    }
    let updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET
            status = 'failed',
            claimed_by = NULL,
            locked_at = NULL,
            claim_expires_at = NULL,
            last_error = $3,
            updated_at = now()
        WHERE id = $1
          AND status = 'processing'
          AND claimed_by = $2
        "#,
    )
    .bind(claim.work_item_id)
    .bind(&claim.claim_token)
    .bind(failure_code)
    .execute(&mut *tx)
    .await
    .context("record QiWe upload failure")?;
    if updated.rows_affected() != 1 {
        bail!("QiWe upload claim changed before failure was recorded");
    }
    append_event(
        &mut tx,
        claim.work_item_id,
        Some(claim.generated_image_artifact_id),
        "qiwe_image_upload_failed",
        json!({
            "attempt_number": claim.attempt_number,
            "attempt_id": claim.attempt_id,
            "failure_code": failure_code,
            "external_upload_outcome": external_upload_outcome,
            "request_may_have_been_accepted": request_may_have_been_accepted,
            "automatic_retry_allowed": false,
            "external_send_executed": false,
            "send_executed": false
        }),
    )
    .await?;
    tx.commit().await.context("commit QiWe upload failure")?;
    Ok(())
}

pub async fn claim_callback_for_send(
    pool: &PgPool,
    request_id: &str,
    callback_payload: &[u8],
    callback_file: &QiweCallbackFileIdentity<'_>,
) -> Result<CallbackClaimOutcome> {
    validate_plain_value(request_id, "QiWe upload request id")?;
    validate_callback_file_identity(callback_file)?;
    if callback_payload.is_empty() || callback_payload.len() > MAX_CALLBACK_PAYLOAD_BYTES {
        bail!("QiWe callback payload size is invalid");
    }
    let request_id_sha256 = sha256_marker(request_id.as_bytes());
    let callback_payload_sha256 = sha256_marker(callback_payload);
    let mut tx = pool
        .begin()
        .await
        .context("begin QiWe callback claim transaction")?;
    let row = sqlx::query(
        r#"
        SELECT
            id,
            work_item_id,
            generated_image_artifact_id,
            status,
            claim_token,
            request_id_sha256,
            callback_payload_sha256,
            target_group_sha256,
            artifact_content_hash,
            artifact_uri_sha256,
            artifact_file_md5,
            artifact_byte_size
        FROM qintopia_agent_os.qiwe_image_send_attempts
        WHERE request_id_sha256 = $1
        FOR UPDATE
        "#,
    )
    .bind(&request_id_sha256)
    .fetch_optional(&mut *tx)
    .await
    .context("load QiWe upload correlation")?
    .ok_or_else(|| anyhow!("QiWe callback does not match a known upload correlation"))?;
    let attempt = stored_attempt_from_row(row)?;
    if attempt.status != "awaiting_callback" {
        if attempt.status == "expired"
            || (attempt.callback_payload_sha256.as_deref() == Some(&callback_payload_sha256)
                && matches!(
                    attempt.status.as_str(),
                    "sending" | "sent" | "failed" | "ambiguous"
                ))
        {
            tx.commit()
                .await
                .context("commit duplicate QiWe callback check")?;
            return Ok(CallbackClaimOutcome::Duplicate {
                status: attempt.status,
            });
        }
        bail!("QiWe callback correlation is no longer awaiting callback");
    }

    let (target_group_id, claim_is_current) =
        lock_callback_policy(&mut tx, &attempt, callback_file).await?;
    if !claim_is_current {
        expire_awaiting_callback_attempt(&mut tx, &attempt, Some(&callback_payload_sha256)).await?;
        tx.commit()
            .await
            .context("commit expired QiWe callback claim")?;
        return Ok(CallbackClaimOutcome::Expired);
    }
    let updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.qiwe_image_send_attempts
        SET
            status = 'sending',
            callback_payload_sha256 = $2,
            callback_received_at = now(),
            send_started_at = now(),
            audit_metadata = audit_metadata || $3,
            updated_at = now()
        WHERE id = $1
          AND status = 'awaiting_callback'
        "#,
    )
    .bind(attempt.id)
    .bind(&callback_payload_sha256)
    .bind(json!({
        "callback_credentials_persisted": false,
        "send_gate_opened": true,
        "external_send_executed": false
    }))
    .execute(&mut *tx)
    .await
    .context("open QiWe send gate")?;
    if updated.rows_affected() != 1 {
        bail!("QiWe callback send gate changed concurrently");
    }
    let claim_extended = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET claim_expires_at = now() + make_interval(mins => $3), updated_at = now()
        WHERE id = $1
          AND status = 'processing'
          AND claimed_by = $2
        "#,
    )
    .bind(attempt.work_item_id)
    .bind(&attempt.claim_token)
    .bind(SEND_CLAIM_TTL_MINUTES as i32)
    .execute(&mut *tx)
    .await
    .context("extend QiWe send claim")?;
    if claim_extended.rows_affected() != 1 {
        bail!("QiWe image-send claim expired before callback processing");
    }
    append_event(
        &mut tx,
        attempt.work_item_id,
        Some(attempt.generated_image_artifact_id),
        "qiwe_image_callback_claimed",
        json!({
            "attempt_id": attempt.id,
            "request_id_sha256": request_id_sha256,
            "callback_payload_sha256": callback_payload_sha256,
            "callback_credentials_persisted": false,
            "send_gate_opened": true,
            "send_executed": false
        }),
    )
    .await?;
    tx.commit()
        .await
        .context("commit QiWe callback send gate")?;
    Ok(CallbackClaimOutcome::Ready(QiweCallbackSendClaim {
        attempt_id: attempt.id,
        work_item_id: attempt.work_item_id,
        generated_image_artifact_id: attempt.generated_image_artifact_id,
        claim_token: attempt.claim_token,
        target_group_id,
    }))
}

pub async fn record_send_success(
    pool: &PgPool,
    claim: &QiweCallbackSendClaim,
    receipt: &QiweSendReceipt,
) -> Result<()> {
    if receipt.is_send_success != 1 {
        bail!("QiWe send receipt does not confirm success");
    }
    validate_plain_value(
        &receipt.message_identifier,
        "QiWe provider message identifier",
    )?;
    let provider_message_id_sha256 = sha256_marker(receipt.message_identifier.as_bytes());
    let mut tx = pool
        .begin()
        .await
        .context("begin QiWe send-success transaction")?;
    lock_sending_claim(&mut tx, claim).await?;
    let attempt_updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.qiwe_image_send_attempts
        SET
            status = 'sent',
            provider_message_id_sha256 = $2,
            completed_at = now(),
            audit_metadata = audit_metadata || $3,
            updated_at = now()
        WHERE id = $1
          AND status = 'sending'
          AND claim_token = $4
        "#,
    )
    .bind(claim.attempt_id)
    .bind(&provider_message_id_sha256)
    .bind(json!({
        "provider_confirmed_success": true,
        "callback_credentials_persisted": false,
        "external_send_executed": true
    }))
    .bind(&claim.claim_token)
    .execute(&mut *tx)
    .await
    .context("record sanitized QiWe send success")?;
    if attempt_updated.rows_affected() != 1 {
        bail!("QiWe send attempt is no longer current");
    }
    let work_item_updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET
            status = 'completed',
            claimed_by = NULL,
            locked_at = NULL,
            claim_expires_at = NULL,
            last_error = NULL,
            metadata = metadata || $3,
            updated_at = now()
        WHERE id = $1
          AND status = 'processing'
          AND claimed_by = $2
        "#,
    )
    .bind(claim.work_item_id)
    .bind(&claim.claim_token)
    .bind(json!({
        "qiwe_image_send": {
            "attempt_id": claim.attempt_id,
            "provider_message_id_sha256": provider_message_id_sha256,
            "external_send_executed": true
        }
    }))
    .execute(&mut *tx)
    .await
    .context("complete QiWe group-message work item")?;
    if work_item_updated.rows_affected() != 1 {
        bail!("QiWe work-item claim changed before send success was recorded");
    }
    append_event(
        &mut tx,
        claim.work_item_id,
        Some(claim.generated_image_artifact_id),
        "qiwe_image_send_executed",
        json!({
            "attempt_id": claim.attempt_id,
            "provider_message_id_sha256": provider_message_id_sha256,
            "provider_confirmed_success": true,
            "callback_credentials_persisted": false,
            "send_executed": true
        }),
    )
    .await?;
    tx.commit().await.context("commit QiWe send success")?;
    Ok(())
}

pub async fn record_send_failure(
    pool: &PgPool,
    claim: &QiweCallbackSendClaim,
    disposition: SendFailureDisposition,
) -> Result<()> {
    let (status, failure_code, event_type, external_send_executed, external_send_outcome) =
        send_failure_state(disposition);
    let mut tx = pool
        .begin()
        .await
        .context("begin QiWe send-failure transaction")?;
    lock_sending_claim(&mut tx, claim).await?;
    let attempt_updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.qiwe_image_send_attempts
        SET
            status = $2,
            failure_code = $3,
            completed_at = now(),
            audit_metadata = audit_metadata || $4,
            updated_at = now()
        WHERE id = $1
          AND status = 'sending'
          AND claim_token = $5
        "#,
    )
    .bind(claim.attempt_id)
    .bind(status)
    .bind(failure_code)
    .bind(json!({
        "callback_credentials_persisted": false,
        "external_send_executed": external_send_executed,
        "external_send_outcome": external_send_outcome,
        "automatic_retry_allowed": false
    }))
    .bind(&claim.claim_token)
    .execute(&mut *tx)
    .await
    .context("record sanitized QiWe send failure")?;
    if attempt_updated.rows_affected() != 1 {
        bail!("QiWe send attempt is no longer current");
    }
    let work_item_updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET
            status = 'failed',
            claimed_by = NULL,
            locked_at = NULL,
            claim_expires_at = NULL,
            last_error = $3,
            updated_at = now()
        WHERE id = $1
          AND status = 'processing'
          AND claimed_by = $2
        "#,
    )
    .bind(claim.work_item_id)
    .bind(&claim.claim_token)
    .bind(failure_code)
    .execute(&mut *tx)
    .await
    .context("fail QiWe group-message work item")?;
    if work_item_updated.rows_affected() != 1 {
        bail!("QiWe work-item claim changed before send failure was recorded");
    }
    append_event(
        &mut tx,
        claim.work_item_id,
        Some(claim.generated_image_artifact_id),
        event_type,
        json!({
            "attempt_id": claim.attempt_id,
            "failure_code": failure_code,
            "callback_credentials_persisted": false,
            "external_send_executed": external_send_executed,
            "external_send_outcome": external_send_outcome,
            "automatic_retry_allowed": false,
            "send_executed": external_send_executed
        }),
    )
    .await?;
    tx.commit().await.context("commit QiWe send failure")?;
    Ok(())
}

fn send_failure_state(
    disposition: SendFailureDisposition,
) -> (
    &'static str,
    &'static str,
    &'static str,
    Option<bool>,
    &'static str,
) {
    match disposition {
        SendFailureDisposition::Rejected => (
            "failed",
            "send_rejected",
            "qiwe_image_send_rejected",
            Some(false),
            "rejected",
        ),
        SendFailureDisposition::Ambiguous => (
            "ambiguous",
            "send_outcome_ambiguous",
            "qiwe_image_send_outcome_ambiguous",
            None,
            "unknown",
        ),
    }
}

async fn expire_one_stale_awaiting_callback(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Option<Uuid>,
) -> Result<()> {
    let row = sqlx::query(
        r#"
        SELECT
            attempt.id,
            attempt.work_item_id,
            attempt.generated_image_artifact_id,
            attempt.status,
            attempt.claim_token,
            attempt.request_id_sha256,
            attempt.callback_payload_sha256,
            attempt.target_group_sha256,
            attempt.artifact_content_hash,
            attempt.artifact_uri_sha256,
            attempt.artifact_file_md5,
            attempt.artifact_byte_size
        FROM qintopia_agent_os.qiwe_image_send_attempts attempt
        JOIN qintopia_agent_os.work_items request ON request.id = attempt.work_item_id
        WHERE attempt.status = 'awaiting_callback'
          AND request.status = 'processing'
          AND request.claimed_by = attempt.claim_token
          AND request.claim_expires_at <= now()
          AND ($1::uuid IS NULL OR request.id = $1)
        ORDER BY attempt.created_at ASC
        LIMIT 1
        FOR UPDATE OF attempt, request SKIP LOCKED
        "#,
    )
    .bind(work_item_id)
    .fetch_optional(&mut **tx)
    .await
    .context("lock stale QiWe callback attempt")?;
    if let Some(row) = row {
        let attempt = stored_attempt_from_row(row)?;
        expire_awaiting_callback_attempt(tx, &attempt, None).await?;
    }
    Ok(())
}

async fn reconcile_one_stale_sending_attempt(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Option<Uuid>,
) -> Result<()> {
    let row = sqlx::query(
        r#"
        SELECT attempt.id, attempt.work_item_id, attempt.generated_image_artifact_id,
               attempt.claim_token
        FROM qintopia_agent_os.qiwe_image_send_attempts attempt
        JOIN qintopia_agent_os.work_items request ON request.id = attempt.work_item_id
        WHERE attempt.status = 'sending'
          AND request.status = 'processing'
          AND request.claimed_by = attempt.claim_token
          AND request.claim_expires_at <= now()
          AND ($1::uuid IS NULL OR request.id = $1)
        ORDER BY attempt.send_started_at ASC NULLS FIRST
        LIMIT 1
        FOR UPDATE OF attempt, request SKIP LOCKED
        "#,
    )
    .bind(work_item_id)
    .fetch_optional(&mut **tx)
    .await
    .context("lock stale QiWe sending attempt")?;
    let Some(row) = row else {
        return Ok(());
    };
    let attempt_id: Uuid = row.try_get("id")?;
    let work_item_id: Uuid = row.try_get("work_item_id")?;
    let artifact_id: Uuid = row.try_get("generated_image_artifact_id")?;
    let claim_token: String = row.try_get("claim_token")?;
    let attempt_updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.qiwe_image_send_attempts
        SET
            status = 'ambiguous',
            failure_code = 'send_outcome_ambiguous',
            completed_at = now(),
            audit_metadata = audit_metadata || $2,
            updated_at = now()
        WHERE id = $1
          AND status = 'sending'
          AND claim_token = $3
        "#,
    )
    .bind(attempt_id)
    .bind(json!({
        "callback_credentials_persisted": false,
        "external_send_executed": serde_json::Value::Null,
        "external_send_outcome": "unknown",
        "automatic_retry_allowed": false,
        "reconciled_after_claim_expiry": true
    }))
    .bind(&claim_token)
    .execute(&mut **tx)
    .await
    .context("reconcile stale QiWe sending attempt")?;
    if attempt_updated.rows_affected() != 1 {
        bail!("QiWe sending attempt changed before reconciliation");
    }
    let work_item_updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET
            status = 'failed',
            claimed_by = NULL,
            locked_at = NULL,
            claim_expires_at = NULL,
            last_error = 'send_outcome_ambiguous',
            updated_at = now()
        WHERE id = $1
          AND status = 'processing'
          AND claimed_by = $2
        "#,
    )
    .bind(work_item_id)
    .bind(&claim_token)
    .execute(&mut **tx)
    .await
    .context("fail work item after stale QiWe sending attempt")?;
    if work_item_updated.rows_affected() != 1 {
        bail!("QiWe sending work-item claim changed before reconciliation");
    }
    append_event(
        tx,
        work_item_id,
        Some(artifact_id),
        "qiwe_image_send_outcome_ambiguous",
        json!({
            "attempt_id": attempt_id,
            "failure_code": "send_outcome_ambiguous",
            "callback_credentials_persisted": false,
            "external_send_executed": serde_json::Value::Null,
            "external_send_outcome": "unknown",
            "automatic_retry_allowed": false,
            "reconciled_after_claim_expiry": true,
            "send_executed": serde_json::Value::Null
        }),
    )
    .await?;
    Ok(())
}

async fn reconcile_one_stale_uploading_attempt(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Option<Uuid>,
) -> Result<()> {
    let row = sqlx::query(
        r#"
        SELECT
            attempt.id,
            attempt.work_item_id,
            attempt.generated_image_artifact_id,
            attempt.claim_token
        FROM qintopia_agent_os.qiwe_image_send_attempts attempt
        JOIN qintopia_agent_os.work_items request
          ON request.id = attempt.work_item_id
         AND request.status = 'processing'
         AND request.claimed_by = attempt.claim_token
         AND request.claim_expires_at <= now()
        WHERE attempt.status = 'uploading'
          AND ($1::uuid IS NULL OR attempt.work_item_id = $1)
        ORDER BY attempt.created_at ASC
        LIMIT 1
        FOR UPDATE OF attempt, request SKIP LOCKED
        "#,
    )
    .bind(work_item_id)
    .fetch_optional(&mut **tx)
    .await
    .context("lock stale QiWe uploading attempt")?;
    let Some(row) = row else {
        return Ok(());
    };
    let attempt_id: Uuid = row.try_get("id")?;
    let work_item_id: Uuid = row.try_get("work_item_id")?;
    let artifact_id: Uuid = row.try_get("generated_image_artifact_id")?;
    let claim_token: String = row.try_get("claim_token")?;
    let attempt_updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.qiwe_image_send_attempts
        SET
            status = 'ambiguous',
            failure_code = 'qiwe_upload_outcome_ambiguous',
            completed_at = now(),
            audit_metadata = audit_metadata || $2,
            updated_at = now()
        WHERE id = $1
          AND status = 'uploading'
          AND claim_token = $3
        "#,
    )
    .bind(attempt_id)
    .bind(json!({
        "automatic_retry_allowed": false,
        "external_upload_outcome": "unknown",
        "external_send_executed": false,
        "reconciled_after_claim_expiry": true,
        "request_id_persisted": false
    }))
    .bind(&claim_token)
    .execute(&mut **tx)
    .await
    .context("reconcile stale QiWe uploading attempt")?;
    if attempt_updated.rows_affected() != 1 {
        bail!("QiWe uploading attempt changed before reconciliation");
    }
    let work_item_updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET
            status = 'failed',
            claimed_by = NULL,
            locked_at = NULL,
            claim_expires_at = NULL,
            last_error = 'qiwe_upload_outcome_ambiguous',
            updated_at = now()
        WHERE id = $1
          AND status = 'processing'
          AND claimed_by = $2
        "#,
    )
    .bind(work_item_id)
    .bind(&claim_token)
    .execute(&mut **tx)
    .await
    .context("fail work item after stale QiWe uploading attempt")?;
    if work_item_updated.rows_affected() != 1 {
        bail!("QiWe uploading work-item claim changed before reconciliation");
    }
    append_event(
        tx,
        work_item_id,
        Some(artifact_id),
        "qiwe_image_upload_outcome_ambiguous",
        json!({
            "attempt_id": attempt_id,
            "failure_code": "qiwe_upload_outcome_ambiguous",
            "automatic_retry_allowed": false,
            "external_upload_outcome": "unknown",
            "external_send_executed": false,
            "reconciled_after_claim_expiry": true,
            "request_id_persisted": false,
            "send_executed": false
        }),
    )
    .await?;
    Ok(())
}

async fn terminalize_one_stale_unrecorded_claim(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Option<Uuid>,
) -> Result<()> {
    let row = sqlx::query(
        r#"
        SELECT request.id, request.claimed_by
        FROM qintopia_agent_os.work_items request
        WHERE request.work_item_type = $1
          AND request.capability_key = $2
          AND request.status = 'processing'
          AND request.claimed_by LIKE 'qiwe-image-send-adapter:%'
          AND request.claim_expires_at <= now()
          AND ($3::uuid IS NULL OR request.id = $3)
          AND NOT EXISTS (
              SELECT 1
              FROM qintopia_agent_os.qiwe_image_send_attempts attempt
              WHERE attempt.work_item_id = request.id
                AND attempt.claim_token = request.claimed_by
          )
        ORDER BY request.locked_at ASC NULLS FIRST
        LIMIT 1
        FOR UPDATE OF request SKIP LOCKED
        "#,
    )
    .bind(WORK_ITEM_TYPE)
    .bind(CAPABILITY_KEY)
    .bind(work_item_id)
    .fetch_optional(&mut **tx)
    .await
    .context("lock stale unrecorded QiWe upload claim")?;
    let Some(row) = row else {
        return Ok(());
    };
    let work_item_id: Uuid = row.try_get("id")?;
    let claim_token: String = row.try_get("claimed_by")?;
    let updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET
            status = 'failed',
            claimed_by = NULL,
            locked_at = NULL,
            claim_expires_at = NULL,
            last_error = 'qiwe_upload_outcome_ambiguous',
            updated_at = now()
        WHERE id = $1
          AND status = 'processing'
          AND claimed_by = $2
        "#,
    )
    .bind(work_item_id)
    .bind(&claim_token)
    .execute(&mut **tx)
    .await
    .context("terminalize stale unrecorded QiWe upload claim")?;
    if updated.rows_affected() != 1 {
        bail!("QiWe unrecorded upload claim changed before terminalization");
    }
    append_event(
        tx,
        work_item_id,
        None,
        "qiwe_image_upload_outcome_ambiguous",
        json!({
            "failure_code": "qiwe_upload_outcome_ambiguous",
            "external_upload_outcome": "unknown",
            "request_id_persisted": false,
            "attempt_persisted": false,
            "automatic_retry_allowed": false,
            "external_send_executed": false,
            "send_executed": false
        }),
    )
    .await?;
    Ok(())
}

async fn expire_awaiting_callback_attempt(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    attempt: &StoredAttempt,
    callback_payload_sha256: Option<&str>,
) -> Result<()> {
    let callback_received = callback_payload_sha256.is_some();
    let event_type = if callback_received {
        "qiwe_image_callback_expired"
    } else {
        "qiwe_image_callback_timeout_expired"
    };
    let attempt_updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.qiwe_image_send_attempts
        SET
            status = 'expired',
            callback_payload_sha256 = COALESCE($2, callback_payload_sha256),
            callback_received_at = CASE WHEN $2::text IS NULL THEN callback_received_at ELSE now() END,
            failure_code = 'claim_expired',
            completed_at = now(),
            audit_metadata = audit_metadata || $3,
            updated_at = now()
        WHERE id = $1
          AND status = 'awaiting_callback'
          AND claim_token = $4
        "#,
    )
    .bind(attempt.id)
    .bind(callback_payload_sha256)
    .bind(json!({
        "callback_received": callback_received,
        "callback_credentials_persisted": false,
        "external_send_executed": false,
        "external_send_outcome": "not_started",
        "automatic_retry_allowed": true
    }))
    .bind(&attempt.claim_token)
    .execute(&mut **tx)
    .await
    .context("expire late QiWe callback attempt")?;
    if attempt_updated.rows_affected() != 1 {
        bail!("QiWe callback attempt changed before expiration");
    }
    let work_item_updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET
            status = 'queued',
            claimed_by = NULL,
            locked_at = NULL,
            claim_expires_at = NULL,
            available_at = now(),
            last_error = 'claim_expired',
            updated_at = now()
        WHERE id = $1
          AND status = 'processing'
          AND claimed_by = $2
        "#,
    )
    .bind(attempt.work_item_id)
    .bind(&attempt.claim_token)
    .execute(&mut **tx)
    .await
    .context("requeue work item after late QiWe callback")?;
    if work_item_updated.rows_affected() != 1 {
        bail!("QiWe callback work-item claim changed before expiration");
    }
    let mut event_data = json!({
        "attempt_id": attempt.id,
        "request_id_sha256": attempt.request_id_sha256,
        "failure_code": "claim_expired",
        "callback_received": callback_received,
        "callback_credentials_persisted": false,
        "external_send_executed": false,
        "external_send_outcome": "not_started",
        "automatic_retry_allowed": true,
        "send_executed": false
    });
    if let Some(callback_payload_sha256) = callback_payload_sha256 {
        event_data["callback_payload_sha256"] = json!(callback_payload_sha256);
    }
    append_event(
        tx,
        attempt.work_item_id,
        Some(attempt.generated_image_artifact_id),
        event_type,
        event_data,
    )
    .await?;
    Ok(())
}

fn validate_claim_boundary(
    artifact: ArtifactBoundary<'_>,
    allowed_group_ids: &BTreeSet<String>,
    media_allowed_hosts: &BTreeSet<String>,
) -> Result<ValidatedArtifactIdentity> {
    validate_canonical_sha256(artifact.content_hash, "generated-image content hash")?;
    validate_canonical_md5(artifact.file_md5, "generated-image file MD5")?;
    if artifact.byte_size == 0 {
        bail!("approved generated-image byte_size must be positive");
    }
    if artifact.mime_type != "image/jpeg" {
        bail!("approved generated image must use image/jpeg");
    }
    validate_plain_value(artifact.target_group_id, "QiWe target group id")?;
    if !allowed_group_ids.contains(artifact.target_group_id) {
        bail!("QiWe target group id is not allowlisted");
    }
    let uri = strict_media_url(artifact.uri)?;
    let host = uri
        .host_str()
        .context("approved generated-image URI host is missing")?
        .to_ascii_lowercase();
    if !media_allowed_hosts.contains(&host) {
        bail!("approved generated-image URI host is not allowlisted");
    }
    let filename = media_filename(&uri)?;
    validate_jpeg_filename(filename)?;
    Ok(ValidatedArtifactIdentity {
        filename: filename.to_string(),
        file_md5: artifact.file_md5.to_string(),
        byte_size: artifact.byte_size,
    })
}

async fn lock_current_claim(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    claim: &QiweUploadClaim,
) -> Result<()> {
    let current: Option<Uuid> = sqlx::query_scalar(
        r#"
        SELECT request.id
        FROM qintopia_agent_os.work_items request
        JOIN qintopia_agent_os.artifacts artifact
          ON artifact.id = $3
         AND artifact.id::text = request.payload->>'approved_artifact_id'
        JOIN qintopia_agent_os.work_items image_request
          ON image_request.id = artifact.work_item_id
         AND image_request.work_item_type = 'image_generation_request'
         AND image_request.capability_key = 'huabaosi.generate_image_asset'
         AND image_request.target_agent = 'huabaosi'
         AND image_request.status = 'completed'
        WHERE request.id = $1
          AND request.status = 'processing'
          AND request.claimed_by = $2
          AND request.claim_expires_at > now()
          AND request.payload->>'target_group_id' = $4
          AND artifact.artifact_type = 'generated_image'
          AND artifact.review_status = 'approved'
          AND artifact.created_by_agent = 'huabaosi'
          AND artifact.content_hash = $5
          AND artifact.artifact_uri = $6
          AND artifact.metadata->>'file_md5' = $7
          AND artifact.metadata->>'byte_size' = $8
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
          AND EXISTS (
              SELECT 1
              FROM qintopia_agent_os.work_item_events created
              WHERE created.work_item_id = artifact.work_item_id
                AND created.artifact_id = artifact.id
                AND created.event_type = 'generated_image_created'
          )
        FOR UPDATE OF request, artifact, image_request
        "#,
    )
    .bind(claim.work_item_id)
    .bind(&claim.claim_token)
    .bind(claim.generated_image_artifact_id)
    .bind(&claim.target_group_id)
    .bind(&claim.artifact_content_hash)
    .bind(&claim.artifact_uri)
    .bind(&claim.artifact_file_md5)
    .bind(claim.artifact_byte_size.to_string())
    .fetch_optional(&mut **tx)
    .await
    .context("recheck current QiWe upload claim")?;
    if current.is_none() {
        bail!("QiWe image-send claim or approved artifact is no longer current");
    }
    Ok(())
}

async fn lock_callback_policy(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    attempt: &StoredAttempt,
    callback_file: &QiweCallbackFileIdentity<'_>,
) -> Result<(String, bool)> {
    let row = sqlx::query(
        r#"
        SELECT request.payload->>'target_group_id' AS target_group_id,
               artifact.artifact_uri,
               artifact.metadata->>'file_md5' AS artifact_file_md5,
               artifact.metadata->>'byte_size' AS artifact_byte_size,
               COALESCE(request.claim_expires_at > now(), false) AS claim_is_current
        FROM qintopia_agent_os.work_items request
        JOIN qintopia_agent_os.artifacts artifact
          ON artifact.id = $3
         AND artifact.id::text = request.payload->>'approved_artifact_id'
        JOIN qintopia_agent_os.work_items image_request
          ON image_request.id = artifact.work_item_id
         AND image_request.work_item_type = 'image_generation_request'
         AND image_request.capability_key = 'huabaosi.generate_image_asset'
         AND image_request.target_agent = 'huabaosi'
         AND image_request.status = 'completed'
        WHERE request.id = $1
          AND request.status = 'processing'
          AND request.claimed_by = $2
          AND request.work_item_type = 'group_message_request'
          AND request.capability_key = 'erhua.send_group_message'
          AND request.requester_agent = 'xiaoman'
          AND request.target_agent = 'erhua'
          AND request.review_policy = 'human_final_confirmation'
          AND artifact.artifact_type = 'generated_image'
          AND artifact.review_status = 'approved'
          AND artifact.created_by_agent = 'huabaosi'
          AND artifact.content_hash = $4
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
          AND EXISTS (
              SELECT 1
              FROM qintopia_agent_os.work_item_events created
              WHERE created.work_item_id = artifact.work_item_id
                AND created.artifact_id = artifact.id
                AND created.event_type = 'generated_image_created'
          )
        FOR UPDATE OF request, artifact, image_request
        "#,
    )
    .bind(attempt.work_item_id)
    .bind(&attempt.claim_token)
    .bind(attempt.generated_image_artifact_id)
    .bind(&attempt.artifact_content_hash)
    .fetch_optional(&mut **tx)
    .await
    .context("lock QiWe callback policy facts")?
    .ok_or_else(|| anyhow!("QiWe callback policy or claim is no longer current"))?;
    let target_group_id: String = row.try_get("target_group_id")?;
    let artifact_uri: String = row.try_get("artifact_uri")?;
    let artifact_file_md5: String = row.try_get("artifact_file_md5")?;
    let artifact_byte_size = parse_positive_byte_size(
        &row.try_get::<String, _>("artifact_byte_size")?,
        "approved generated-image byte_size",
    )?;
    let artifact_url = strict_media_url(&artifact_uri)?;
    let artifact_filename = media_filename(&artifact_url)?;
    let claim_is_current: bool = row.try_get("claim_is_current")?;
    if sha256_marker(target_group_id.as_bytes()) != attempt.target_group_sha256
        || sha256_marker(artifact_uri.as_bytes()) != attempt.artifact_uri_sha256
        || artifact_file_md5 != attempt.artifact_file_md5
        || artifact_byte_size != attempt.artifact_byte_size
    {
        bail!("QiWe callback target or artifact changed after upload acceptance");
    }
    if callback_file.filename != artifact_filename
        || callback_file.file_md5 != attempt.artifact_file_md5
        || callback_file.file_size != attempt.artifact_byte_size
    {
        bail!("QiWe callback file identity does not match the approved generated image");
    }
    Ok((target_group_id, claim_is_current))
}

async fn lock_sending_claim(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    claim: &QiweCallbackSendClaim,
) -> Result<()> {
    let current: Option<Uuid> = sqlx::query_scalar(
        r#"
        SELECT attempt.id
        FROM qintopia_agent_os.qiwe_image_send_attempts attempt
        JOIN qintopia_agent_os.work_items request
          ON request.id = attempt.work_item_id
        WHERE attempt.id = $1
          AND attempt.work_item_id = $2
          AND attempt.generated_image_artifact_id = $3
          AND attempt.status = 'sending'
          AND attempt.claim_token = $4
          AND request.status = 'processing'
          AND request.claimed_by = $4
        FOR UPDATE OF attempt, request
        "#,
    )
    .bind(claim.attempt_id)
    .bind(claim.work_item_id)
    .bind(claim.generated_image_artifact_id)
    .bind(&claim.claim_token)
    .fetch_optional(&mut **tx)
    .await
    .context("lock current QiWe sending claim")?;
    if current.is_none() {
        bail!("QiWe sending claim is no longer current");
    }
    Ok(())
}

async fn append_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Uuid,
    artifact_id: Option<Uuid>,
    event_type: &str,
    data: serde_json::Value,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_item_events
            (work_item_id, artifact_id, event_type, actor_type, actor_id, message, data)
        VALUES ($1, $2, $3, 'worker', $4, 'QiWe image-send state transition recorded', $5)
        "#,
    )
    .bind(work_item_id)
    .bind(artifact_id)
    .bind(event_type)
    .bind(WORKER_ID)
    .bind(data)
    .execute(&mut **tx)
    .await
    .context("append QiWe image-send event")?;
    Ok(())
}

fn stored_attempt_from_row(row: sqlx::postgres::PgRow) -> Result<StoredAttempt> {
    Ok(StoredAttempt {
        id: row.try_get("id")?,
        work_item_id: row.try_get("work_item_id")?,
        generated_image_artifact_id: row.try_get("generated_image_artifact_id")?,
        status: row.try_get("status")?,
        claim_token: row.try_get("claim_token")?,
        request_id_sha256: row.try_get("request_id_sha256")?,
        callback_payload_sha256: row.try_get("callback_payload_sha256")?,
        target_group_sha256: row.try_get("target_group_sha256")?,
        artifact_content_hash: row.try_get("artifact_content_hash")?,
        artifact_uri_sha256: row.try_get("artifact_uri_sha256")?,
        artifact_file_md5: row.try_get("artifact_file_md5")?,
        artifact_byte_size: u64::try_from(row.try_get::<i64, _>("artifact_byte_size")?)
            .context("stored QiWe artifact byte size is invalid")?,
    })
}

fn sha256_marker(value: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(value))
}

fn validate_canonical_sha256(value: &str, label: &str) -> Result<()> {
    let digest = value
        .strip_prefix("sha256:")
        .ok_or_else(|| anyhow!("{label} must be canonical sha256"))?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("{label} must be canonical sha256");
    }
    Ok(())
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

fn validate_callback_file_identity(callback: &QiweCallbackFileIdentity<'_>) -> Result<()> {
    validate_jpeg_filename(callback.filename)?;
    validate_canonical_md5(callback.file_md5, "QiWe callback file MD5")?;
    if callback.file_size == 0 {
        bail!("QiWe callback file size must be positive");
    }
    Ok(())
}

fn parse_positive_byte_size(value: &str, label: &str) -> Result<u64> {
    let byte_size = value
        .parse::<u64>()
        .with_context(|| format!("{label} is invalid"))?;
    if byte_size == 0 || byte_size > i64::MAX as u64 {
        bail!("{label} is invalid");
    }
    Ok(byte_size)
}

fn validate_plain_value(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
        bail!("{label} is invalid");
    }
    Ok(())
}

fn strict_media_url(value: &str) -> Result<Url> {
    let url = Url::parse(value).context("parse approved generated-image URI")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("approved generated-image URI must be stable HTTPS");
    }
    Ok(url)
}

fn media_filename(url: &Url) -> Result<&str> {
    url.path_segments()
        .and_then(|mut segments| segments.next_back())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("approved generated-image URI is missing a filename"))
}

fn validate_jpeg_filename(filename: &str) -> Result<()> {
    validate_plain_value(filename, "approved generated-image filename")?;
    if filename.len() > 255 || filename.contains(['/', '\\']) {
        bail!("approved generated-image filename is invalid");
    }
    let filename = filename.to_ascii_lowercase();
    if !filename.ends_with(".jpg") && !filename.ends_with(".jpeg") {
        bail!("approved generated-image filename must use JPG");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "postgres-integration-tests")]
    use crate::db;

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
        assert!(matches!(
            parsed.host_str(),
            Some("127.0.0.1" | "localhost" | "::1")
        ));
        assert_eq!(parsed.path().trim_start_matches('/'), "qintopia_test");
        database_url
    }

    #[cfg(feature = "postgres-integration-tests")]
    fn integration_callback_file_identity() -> QiweCallbackFileIdentity<'static> {
        QiweCallbackFileIdentity {
            filename: "qiwe-state-integration.jpg",
            file_md5: "98e7c2acf4391f8b4a2bbd39e364c5e3",
            file_size: 48_300,
        }
    }

    #[cfg(feature = "postgres-integration-tests")]
    async fn insert_send_ready_fixture(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
        let image_work_item_id = Uuid::new_v4();
        let artifact_id = Uuid::new_v4();
        let group_work_item_id = Uuid::new_v4();
        let suffix = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.work_items
                (id, work_item_type, status, requester_agent, target_agent,
                 capability_key, brief_summary, source_type, dedupe_key,
                 idempotency_key, payload, review_policy)
            VALUES
                ($1, 'image_generation_request', 'completed', 'xiaoman', 'huabaosi',
                 'huabaosi.generate_image_asset', 'QiWe state integration image',
                 'integration_test', $2, $2, '{}'::jsonb, 'before_external_use')
            "#,
        )
        .bind(image_work_item_id)
        .bind(format!("qiwe-state-image:{suffix}"))
        .execute(pool)
        .await
        .expect("insert image work item");
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.artifacts
                (id, work_item_id, artifact_type, review_status, created_by_agent,
                 title, summary, artifact_uri, content_hash, metadata)
            VALUES
                ($1, $2, 'generated_image', 'approved', 'huabaosi',
                 'QiWe state integration JPEG', 'sanitized fixture',
                 'https://media.example.test/posters/qiwe-state-integration.jpg',
                 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                 '{"mime_type":"image/jpeg","file_md5":"98e7c2acf4391f8b4a2bbd39e364c5e3","byte_size":48300}'::jsonb)
            "#,
        )
        .bind(artifact_id)
        .bind(image_work_item_id)
        .execute(pool)
        .await
        .expect("insert approved generated image");
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.work_item_events
                (work_item_id, artifact_id, event_type, actor_type, actor_id, message, data)
            VALUES
                ($1, $2, 'generated_image_created', 'worker',
                 'huabaosi-image-generation-worker', 'sanitized integration fixture',
                 '{"external_send_executed":false}'::jsonb)
            "#,
        )
        .bind(image_work_item_id)
        .bind(artifact_id)
        .execute(pool)
        .await
        .expect("insert generated-image provenance event");
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.work_items
                (id, work_item_type, status, requester_agent, target_agent,
                 capability_key, brief_summary, source_type, dedupe_key,
                 idempotency_key, risk_level, payload, review_policy)
            VALUES
                ($1, 'group_message_request', 'queued', 'xiaoman', 'erhua',
                 'erhua.send_group_message', 'QiWe state integration send',
                 'integration_test', $2, $2, 'high', $3, 'human_final_confirmation')
            "#,
        )
        .bind(group_work_item_id)
        .bind(format!("qiwe-state-group:{suffix}"))
        .bind(json!({
            "approved_artifact_id": artifact_id,
            "approved_artifact_type": "generated_image",
            "workflow_type": "activity_promotion",
            "target_channel": "qiwe",
            "target_group_id": "integration-group-id",
            "message_text": "已审核活动海报"
        }))
        .execute(pool)
        .await
        .expect("insert group message work item");
        for (event_type, data) in [
            (
                "group_message_final_confirmation_recorded",
                json!({
                    "decision": "confirmed",
                    "current_status": "queued",
                    "send_executed": false
                }),
            ),
            (
                "group_message_send_ready_recorded",
                json!({
                    "approved_artifact_id": artifact_id,
                    "target_group_id": "integration-group-id",
                    "send_executed": false
                }),
            ),
        ] {
            sqlx::query(
                r#"
                INSERT INTO qintopia_agent_os.work_item_events
                    (work_item_id, artifact_id, event_type, actor_type, actor_id, message, data)
                VALUES ($1, $2, $3, 'integration_test', 'qiwe-state-test',
                        'sanitized integration fixture', $4)
                "#,
            )
            .bind(group_work_item_id)
            .bind(artifact_id)
            .bind(event_type)
            .bind(data)
            .execute(pool)
            .await
            .expect("insert send policy event");
        }
        (image_work_item_id, artifact_id, group_work_item_id)
    }

    #[cfg(feature = "postgres-integration-tests")]
    async fn delete_fixture(pool: &PgPool, image_id: Uuid, group_id: Uuid) {
        sqlx::query(
            "DELETE FROM qintopia_agent_os.qiwe_image_send_attempts WHERE work_item_id = $1",
        )
        .bind(group_id)
        .execute(pool)
        .await
        .expect("delete QiWe attempts");
        sqlx::query("DELETE FROM qintopia_agent_os.work_items WHERE id = $1")
            .bind(group_id)
            .execute(pool)
            .await
            .expect("delete group work item");
        sqlx::query("DELETE FROM qintopia_agent_os.work_items WHERE id = $1")
            .bind(image_id)
            .execute(pool)
            .await
            .expect("delete image work item");
    }

    #[test]
    fn canonical_hash_validation_rejects_prefixed_raw_values() {
        assert!(validate_canonical_sha256(
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "test hash"
        )
        .is_ok());
        assert!(validate_canonical_sha256("sha256:raw-secret", "test hash").is_err());
        assert!(validate_canonical_sha256(
            "sha256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "test hash"
        )
        .is_err());
    }

    #[test]
    fn claim_boundary_requires_reviewed_jpeg_host_and_group() {
        let groups = BTreeSet::from(["group-id".to_string()]);
        let hosts = BTreeSet::from(["media.example.test".to_string()]);
        let identity = validate_claim_boundary(
            ArtifactBoundary {
                uri: "https://media.example.test/posters/activity.jpg",
                content_hash:
                    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                mime_type: "image/jpeg",
                file_md5: "98e7c2acf4391f8b4a2bbd39e364c5e3",
                byte_size: 48_300,
                target_group_id: "group-id",
            },
            &groups,
            &hosts,
        )
        .expect("reviewed boundary is valid");
        assert_eq!(identity.filename, "activity.jpg");
        assert!(validate_claim_boundary(
            ArtifactBoundary {
                uri: "https://media.example.test/posters/activity.png",
                content_hash:
                    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                mime_type: "image/png",
                file_md5: "98e7c2acf4391f8b4a2bbd39e364c5e3",
                byte_size: 48_300,
                target_group_id: "group-id",
            },
            &groups,
            &hosts,
        )
        .is_err());
        assert!(validate_claim_boundary(
            ArtifactBoundary {
                uri: "https://media.example.test/posters/activity.jpg",
                content_hash:
                    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                mime_type: "image/jpeg",
                file_md5: "98e7c2acf4391f8b4a2bbd39e364c5e3",
                byte_size: 48_300,
                target_group_id: "GROUP-ID",
            },
            &groups,
            &hosts,
        )
        .is_err());
        assert!(validate_claim_boundary(
            "https://other.example.test/posters/activity.jpg",
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "image/jpeg",
            "group-id",
            &groups,
            &hosts,
        )
        .is_err());
        assert!(validate_claim_boundary(
            "https://media.example.test/posters/",
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "image/jpeg",
            "group-id",
            &groups,
            &hosts,
        )
        .is_err());
    }

    #[test]
    fn preview_boundary_requires_canonical_jpeg_https_uri() {
        let groups = BTreeSet::from(["group-id".to_string()]);
        let hosts = BTreeSet::from(["media.example.test".to_string()]);
        let validate = |uri, target_group_id| {
            validate_claim_boundary(
                ArtifactBoundary {
                    uri,
                    content_hash:
                        "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    mime_type: "image/jpeg",
                    file_md5: "98e7c2acf4391f8b4a2bbd39e364c5e3",
                    byte_size: 48_300,
                    target_group_id,
                },
                &groups,
                &hosts,
            )
        };
        assert!(validate(
            "https://media.example.test/posters/activity.jpeg",
            "group-id"
        )
        .is_ok());
        assert!(validate(
            "http://media.example.test/posters/activity.jpeg",
            "group-id"
        )
        .is_err());
        assert!(validate(
            "https://user@media.example.test/posters/activity.jpeg",
            "group-id"
        )
        .is_err());
        assert!(validate(
            "https://media.example.test/posters/activity.jpeg?token=secret",
            "group-id"
        )
        .is_err());
        assert!(validate(
            "https://media.example.test/posters/activity.jpeg#frag",
            "group-id"
        )
        .is_err());
        assert!(validate(
            "https://other.example.test/posters/activity.jpeg",
            "group-id"
        )
        .is_err());
        assert!(validate(
            "https://media.example.test/posters/activity.jpeg",
            "other-group"
        )
        .is_err());
        assert!(validate_preview_boundary(
            "https://media.example.test/posters/activity",
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "image/jpeg",
        )
        .is_err());
    }

    #[test]
    fn plain_values_and_filenames_reject_secret_shaped_inputs() {
        assert!(validate_plain_value("safe-value", "test value").is_ok());
        assert!(validate_plain_value(" \t ", "test value").is_err());
        assert!(validate_plain_value("line\nbreak", "test value").is_err());
        assert!(validate_jpeg_filename("poster.jpg").is_ok());
        assert!(validate_jpeg_filename("poster.JPEG").is_ok());
        assert!(validate_jpeg_filename("nested/poster.jpg").is_err());
        assert!(validate_jpeg_filename(&format!("{}.jpg", "a".repeat(252))).is_err());
        assert!(validate_jpeg_filename("poster.png").is_err());
    }

    #[test]
    fn sha256_marker_is_canonical_and_stable() {
        assert_eq!(
            sha256_marker(b"request-id"),
            "sha256:730e938abe361240c534ba7bd28251b3a345f3b937e9c97e509878c6a031d037"
        );
        validate_canonical_sha256(&sha256_marker(b"request-id"), "request id hash")
            .expect("marker is canonical");
    }

    #[test]
    fn ambiguous_failure_audit_preserves_unknown_outcome() {
        let (status, code, event, executed, outcome) =
            send_failure_state(SendFailureDisposition::Ambiguous);

        assert_eq!(status, "ambiguous");
        assert_eq!(code, "send_outcome_ambiguous");
        assert_eq!(event, "qiwe_image_send_outcome_ambiguous");
        assert_eq!(executed, None);
        assert_eq!(outcome, "unknown");
        let (status, code, event, rejected_executed, rejected_outcome) =
            send_failure_state(SendFailureDisposition::Rejected);
        assert_eq!(status, "failed");
        assert_eq!(code, "send_rejected");
        assert_eq!(event, "qiwe_image_send_rejected");
        assert_eq!(rejected_executed, Some(false));
        assert_eq!(rejected_outcome, "rejected");
    }

    #[test]
    fn callback_send_claim_debug_redacts_sensitive_fields() {
        let claim = QiweCallbackSendClaim {
            attempt_id: Uuid::nil(),
            work_item_id: Uuid::nil(),
            generated_image_artifact_id: Uuid::nil(),
            claim_token: "qiwe-image-send-adapter:secret-token".to_string(),
            target_group_id: "secret-group-id".to_string(),
        };

        let debug = format!("{claim:?}");

        assert!(debug.contains("QiweCallbackSendClaim"));
        assert!(debug.contains("attempt_id"));
        assert!(!debug.contains("secret-token"));
        assert!(!debug.contains("secret-group-id"));
    }

    #[tokio::test]
    #[cfg(feature = "postgres-integration-tests")]
    #[ignore = "requires guarded disposable qintopia_test PostgreSQL"]
    async fn postgres_qiwe_send_state_is_idempotent_and_redacted() {
        let database_url = postgres_integration_database_url();
        let pool = db::connect(&database_url, 2)
            .await
            .expect("connect disposable PostgreSQL");
        db::run_migrations(&pool)
            .await
            .expect("migrate disposable PostgreSQL");
        let (image_id, _artifact_id, group_id) = insert_send_ready_fixture(&pool).await;
        let groups = BTreeSet::from(["integration-group-id".to_string()]);
        let hosts = BTreeSet::from(["media.example.test".to_string()]);
        assert_eq!(
            preview_ready_work_item(&pool, Some(group_id), &groups, &hosts)
                .await
                .expect("preview send-ready work item"),
            Some(QiweUploadPreview {
                work_item_id: group_id
            })
        );
        let disallowed_groups = BTreeSet::from(["other-group-id".to_string()]);
        let group_error =
            preview_ready_work_item(&pool, Some(group_id), &disallowed_groups, &hosts)
                .await
                .expect_err("preview must enforce the target-group allowlist");
        assert!(group_error
            .to_string()
            .contains("group id is not allowlisted"));
        let disallowed_hosts = BTreeSet::from(["other.example.test".to_string()]);
        let host_error = preview_ready_work_item(&pool, Some(group_id), &groups, &disallowed_hosts)
            .await
            .expect_err("preview must enforce the media-host allowlist");
        assert!(host_error
            .to_string()
            .contains("URI host is not allowlisted"));
        let claim = claim_ready_work_item(&pool, Some(group_id), &groups, &hosts)
            .await
            .expect("claim send-ready work item")
            .expect("work item is claimable");
        let initial_attempt: (Uuid, String, Option<String>, serde_json::Value) = sqlx::query_as(
            r#"
                SELECT id, status, request_id_sha256, audit_metadata
                FROM qintopia_agent_os.qiwe_image_send_attempts
                WHERE work_item_id = $1
                "#,
        )
        .bind(group_id)
        .fetch_one(&pool)
        .await
        .expect("read pre-upload attempt");
        assert_eq!(initial_attempt.0, claim.attempt_id);
        assert_eq!(initial_attempt.1, "uploading");
        assert_eq!(initial_attempt.2, None);
        assert_eq!(initial_attempt.3["automatic_retry_allowed"], false);
        assert_eq!(initial_attempt.3["external_upload_outcome"], "not_started");
        let request_id = "raw-upload-request-secret";
        let attempt_id = record_upload_acceptance(&pool, &claim, request_id)
            .await
            .expect("record upload acceptance");
        assert_eq!(attempt_id, claim.attempt_id);
        let callback = br#"{
          "requestId":"raw-upload-request-secret",
          "cmd":20000,
          "msgData":{
            "fileAesKey":"raw-aes-secret",
            "fileId":"raw-file-secret",
            "fileMd5":"98e7c2acf4391f8b4a2bbd39e364c5e3",
            "fileSize":48300,
            "filename":"qiwe-state-integration.jpg"
          }
        }"#;
        let mismatched_file = QiweCallbackFileIdentity {
            filename: "different-image.jpg",
            ..integration_callback_file_identity()
        };
        let mismatch_error = claim_callback_for_send(&pool, request_id, callback, &mismatched_file)
            .await
            .expect_err("callback file identity must match the approved JPEG");
        assert!(mismatch_error
            .to_string()
            .contains("file identity does not match"));
        let callback_claim = match claim_callback_for_send(
            &pool,
            request_id,
            callback,
            &integration_callback_file_identity(),
        )
        .await
        .expect("claim callback once")
        {
            CallbackClaimOutcome::Ready(claim) => claim,
            CallbackClaimOutcome::Duplicate { .. } => panic!("first callback was duplicate"),
            CallbackClaimOutcome::Expired => panic!("current callback claim expired"),
        };
        assert_eq!(callback_claim.attempt_id, attempt_id);
        assert_eq!(callback_claim.target_group_id, "integration-group-id");
        let duplicate = claim_callback_for_send(
            &pool,
            request_id,
            callback,
            &integration_callback_file_identity(),
        )
        .await
        .expect("duplicate callback is idempotent");
        assert_eq!(
            duplicate,
            CallbackClaimOutcome::Duplicate {
                status: "sending".to_string()
            }
        );
        sqlx::query(
            "UPDATE qintopia_agent_os.work_items SET claim_expires_at = now() - interval '1 second' WHERE id = $1",
        )
        .bind(group_id)
        .execute(&pool)
        .await
        .expect("expire sending claim before success finalization");
        record_send_success(
            &pool,
            &callback_claim,
            &QiweSendReceipt {
                is_send_success: 1,
                message_identifier: "raw-provider-message-secret".to_string(),
                sequence: 7,
                timestamp: 8,
            },
        )
        .await
        .expect("record send success");

        let stored: (String, String, String, serde_json::Value) = sqlx::query_as(
            r#"
            SELECT attempt.status, request.status,
                   attempt.provider_message_id_sha256,
                   jsonb_build_object(
                       'attempt', to_jsonb(attempt),
                       'request_metadata', request.metadata,
                       'events', COALESCE(jsonb_agg(event.data) FILTER (WHERE event.id IS NOT NULL), '[]'::jsonb)
                   )
            FROM qintopia_agent_os.qiwe_image_send_attempts attempt
            JOIN qintopia_agent_os.work_items request ON request.id = attempt.work_item_id
            LEFT JOIN qintopia_agent_os.work_item_events event
              ON event.work_item_id = request.id
             AND event.event_type LIKE 'qiwe\_image\_%' ESCAPE '\'
            WHERE attempt.id = $1
            GROUP BY attempt.id, request.status, request.metadata
            "#,
        )
        .bind(attempt_id)
        .fetch_one(&pool)
        .await
        .expect("read sanitized send state");
        assert_eq!(stored.0, "sent");
        assert_eq!(stored.1, "completed");
        assert!(stored.2.starts_with("sha256:"));
        let serialized = serde_json::to_string(&stored.3).expect("serialize stored state");
        for sensitive in [
            request_id,
            "raw-aes-secret",
            "raw-file-secret",
            "raw-md5-secret",
            "raw-private.jpg",
            "raw-provider-message-secret",
            "integration-group-id",
            "https://media.example.test/posters/qiwe-state-integration.jpg",
        ] {
            assert!(
                !serialized.contains(sensitive),
                "stored state leaked {sensitive}"
            );
        }
        let duplicate_after_send = claim_callback_for_send(
            &pool,
            request_id,
            callback,
            &integration_callback_file_identity(),
        )
        .await
        .expect("delivered callback remains idempotent after send");
        assert_eq!(
            duplicate_after_send,
            CallbackClaimOutcome::Duplicate {
                status: "sent".to_string()
            }
        );
        delete_fixture(&pool, image_id, group_id).await;
    }

    #[tokio::test]
    #[cfg(feature = "postgres-integration-tests")]
    #[ignore = "requires guarded disposable qintopia_test PostgreSQL"]
    async fn postgres_qiwe_send_state_recovers_expired_callback_and_terminalizes_ambiguous_send() {
        let database_url = postgres_integration_database_url();
        let pool = db::connect(&database_url, 2)
            .await
            .expect("connect disposable PostgreSQL");
        db::run_migrations(&pool)
            .await
            .expect("migrate disposable PostgreSQL");
        let (image_id, _artifact_id, group_id) = insert_send_ready_fixture(&pool).await;
        let groups = BTreeSet::from(["integration-group-id".to_string()]);
        let hosts = BTreeSet::from(["media.example.test".to_string()]);
        let first_claim = claim_ready_work_item(&pool, Some(group_id), &groups, &hosts)
            .await
            .expect("claim first send-ready work item")
            .expect("first work item is claimable");
        let first_request_id = "late-upload-request-secret";
        let first_attempt_id = record_upload_acceptance(&pool, &first_claim, first_request_id)
            .await
            .expect("record first upload acceptance");
        sqlx::query(
            "UPDATE qintopia_agent_os.work_items SET claim_expires_at = now() - interval '1 second' WHERE id = $1",
        )
        .bind(group_id)
        .execute(&pool)
        .await
        .expect("expire first callback claim");
        let late_callback = br#"{
          "requestId":"late-upload-request-secret",
          "cmd":20000,
          "msgData":{"fileAesKey":"late-aes-secret","fileId":"late-file-secret"}
        }"#;
        let expired = claim_callback_for_send(
            &pool,
            first_request_id,
            late_callback,
            &integration_callback_file_identity(),
        )
        .await
        .expect("late callback reaches expired terminal state");
        assert_eq!(expired, CallbackClaimOutcome::Expired);
        let first_state: (
            String,
            Option<String>,
            String,
            Option<String>,
            String,
            serde_json::Value,
        ) = sqlx::query_as(
            r#"
                SELECT attempt.status, attempt.failure_code, request.status,
                       request.claimed_by, attempt.callback_payload_sha256, event.data
                FROM qintopia_agent_os.qiwe_image_send_attempts attempt
                JOIN qintopia_agent_os.work_items request ON request.id = attempt.work_item_id
                JOIN qintopia_agent_os.work_item_events event
                  ON event.work_item_id = request.id
                 AND event.event_type = 'qiwe_image_callback_expired'
                WHERE attempt.id = $1
                "#,
        )
        .bind(first_attempt_id)
        .fetch_one(&pool)
        .await
        .expect("read expired callback state");
        assert_eq!(first_state.0, "expired");
        assert_eq!(first_state.1.as_deref(), Some("claim_expired"));
        assert_eq!(first_state.2, "queued");
        assert_eq!(first_state.3, None);
        assert!(first_state.4.starts_with("sha256:"));
        assert_eq!(first_state.5["external_send_executed"], false);
        assert_eq!(first_state.5["automatic_retry_allowed"], true);
        let duplicate = claim_callback_for_send(
            &pool,
            first_request_id,
            late_callback,
            &integration_callback_file_identity(),
        )
        .await
        .expect("late callback replay is idempotent");
        assert_eq!(
            duplicate,
            CallbackClaimOutcome::Duplicate {
                status: "expired".to_string()
            }
        );

        let retry_claim = claim_ready_work_item(&pool, Some(group_id), &groups, &hosts)
            .await
            .expect("reclaim work item after callback expiration")
            .expect("expired callback releases active attempt");
        assert_eq!(retry_claim.attempt_number, 2);
        let retry_request_id = "retry-upload-request-secret";
        let retry_attempt_id = record_upload_acceptance(&pool, &retry_claim, retry_request_id)
            .await
            .expect("record retry upload acceptance");
        let retry_callback = br#"{
          "requestId":"retry-upload-request-secret",
          "cmd":20000,
          "msgData":{"fileAesKey":"retry-aes-secret","fileId":"retry-file-secret"}
        }"#;
        let retry_send_claim = match claim_callback_for_send(
            &pool,
            retry_request_id,
            retry_callback,
            &integration_callback_file_identity(),
        )
        .await
        .expect("claim retry callback")
        {
            CallbackClaimOutcome::Ready(claim) => claim,
            CallbackClaimOutcome::Duplicate { .. } => panic!("retry callback was duplicate"),
            CallbackClaimOutcome::Expired => panic!("retry callback unexpectedly expired"),
        };
        sqlx::query(
            "UPDATE qintopia_agent_os.work_items SET claim_expires_at = now() - interval '1 second' WHERE id = $1",
        )
        .bind(group_id)
        .execute(&pool)
        .await
        .expect("expire sending claim before ambiguous finalization");
        record_send_failure(&pool, &retry_send_claim, SendFailureDisposition::Ambiguous)
            .await
            .expect("record ambiguous terminal state after TTL");
        let terminal: (
            String,
            Option<String>,
            String,
            serde_json::Value,
            serde_json::Value,
        ) = sqlx::query_as(
            r#"
                SELECT attempt.status, attempt.failure_code, request.status,
                       attempt.audit_metadata, event.data
                FROM qintopia_agent_os.qiwe_image_send_attempts attempt
                JOIN qintopia_agent_os.work_items request ON request.id = attempt.work_item_id
                JOIN qintopia_agent_os.work_item_events event
                  ON event.work_item_id = request.id
                 AND event.event_type = 'qiwe_image_send_outcome_ambiguous'
                WHERE attempt.id = $1
                "#,
        )
        .bind(retry_attempt_id)
        .fetch_one(&pool)
        .await
        .expect("read ambiguous terminal state");
        assert_eq!(terminal.0, "ambiguous");
        assert_eq!(terminal.1.as_deref(), Some("send_outcome_ambiguous"));
        assert_eq!(terminal.2, "failed");
        assert_eq!(
            terminal.3["external_send_executed"],
            serde_json::Value::Null
        );
        assert_eq!(terminal.3["external_send_outcome"], "unknown");
        assert_eq!(
            terminal.4["external_send_executed"],
            serde_json::Value::Null
        );
        let serialized = serde_json::to_string(&(first_state, terminal))
            .expect("serialize expired and ambiguous states");
        for sensitive in [
            first_request_id,
            retry_request_id,
            "late-aes-secret",
            "late-file-secret",
            "retry-aes-secret",
            "retry-file-secret",
            "integration-group-id",
        ] {
            assert!(
                !serialized.contains(sensitive),
                "terminal state leaked {sensitive}"
            );
        }
        delete_fixture(&pool, image_id, group_id).await;
    }

    #[tokio::test]
    #[cfg(feature = "postgres-integration-tests")]
    #[ignore = "requires guarded disposable qintopia_test PostgreSQL"]
    async fn postgres_qiwe_send_state_expires_missing_callback_during_reclaim() {
        let database_url = postgres_integration_database_url();
        let pool = db::connect(&database_url, 2)
            .await
            .expect("connect disposable PostgreSQL");
        db::run_migrations(&pool)
            .await
            .expect("migrate disposable PostgreSQL");
        let (image_id, _artifact_id, group_id) = insert_send_ready_fixture(&pool).await;
        let groups = BTreeSet::from(["integration-group-id".to_string()]);
        let hosts = BTreeSet::from(["media.example.test".to_string()]);
        let first_claim = claim_ready_work_item(&pool, Some(group_id), &groups, &hosts)
            .await
            .expect("claim send-ready work item")
            .expect("work item is claimable");
        let request_id = "missing-callback-request-secret";
        let attempt_id = record_upload_acceptance(&pool, &first_claim, request_id)
            .await
            .expect("record upload acceptance without callback");
        sqlx::query(
            "UPDATE qintopia_agent_os.work_items SET claim_expires_at = now() - interval '1 second' WHERE id = $1",
        )
        .bind(group_id)
        .execute(&pool)
        .await
        .expect("expire missing-callback claim");

        let retry_claim = claim_ready_work_item(&pool, Some(group_id), &groups, &hosts)
            .await
            .expect("reclaim scans stale callback attempt")
            .expect("missing callback releases work item");
        assert_eq!(retry_claim.attempt_number, 2);
        assert_ne!(retry_claim.claim_token, first_claim.claim_token);
        let stored: (
            String,
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            serde_json::Value,
        ) = sqlx::query_as(
            r#"
            SELECT attempt.status, attempt.failure_code, attempt.callback_payload_sha256,
                   request.status, request.claimed_by, event.data
            FROM qintopia_agent_os.qiwe_image_send_attempts attempt
            JOIN qintopia_agent_os.work_items request ON request.id = attempt.work_item_id
            JOIN qintopia_agent_os.work_item_events event
              ON event.work_item_id = request.id
             AND event.event_type = 'qiwe_image_callback_timeout_expired'
            WHERE attempt.id = $1
            "#,
        )
        .bind(attempt_id)
        .fetch_one(&pool)
        .await
        .expect("read missing-callback timeout state");
        assert_eq!(stored.0, "expired");
        assert_eq!(stored.1.as_deref(), Some("claim_expired"));
        assert_eq!(stored.2, None);
        assert_eq!(stored.3, "processing");
        assert_eq!(stored.4.as_deref(), Some(retry_claim.claim_token.as_str()));
        assert_eq!(stored.5["callback_received"], false);
        assert!(stored.5.get("callback_payload_sha256").is_none());
        let serialized = serde_json::to_string(&stored).expect("serialize timeout state");
        for sensitive in [request_id, "integration-group-id"] {
            assert!(
                !serialized.contains(sensitive),
                "callback timeout state leaked {sensitive}"
            );
        }
        delete_fixture(&pool, image_id, group_id).await;
    }

    #[tokio::test]
    #[cfg(feature = "postgres-integration-tests")]
    #[ignore = "requires guarded disposable qintopia_test PostgreSQL"]
    async fn postgres_qiwe_send_state_terminalizes_stale_upload_and_send() {
        let database_url = postgres_integration_database_url();
        let pool = db::connect(&database_url, 2)
            .await
            .expect("connect disposable PostgreSQL");
        db::run_migrations(&pool)
            .await
            .expect("migrate disposable PostgreSQL");
        let groups = BTreeSet::from(["integration-group-id".to_string()]);
        let hosts = BTreeSet::from(["media.example.test".to_string()]);

        let (first_image_id, _first_artifact_id, first_group_id) =
            insert_send_ready_fixture(&pool).await;
        let abandoned_claim = claim_ready_work_item(&pool, Some(first_group_id), &groups, &hosts)
            .await
            .expect("claim first work item")
            .expect("first work item is claimable");
        sqlx::query(
            "UPDATE qintopia_agent_os.work_items SET claim_expires_at = now() - interval '1 second' WHERE id = $1",
        )
        .bind(first_group_id)
        .execute(&pool)
        .await
        .expect("expire unrecorded upload claim");
        let no_upload_retry = claim_ready_work_item(&pool, Some(first_group_id), &groups, &hosts)
            .await
            .expect("reconcile stale uploading attempt");
        assert!(no_upload_retry.is_none());
        let stale_upload: (
            String,
            Option<String>,
            Option<String>,
            String,
            serde_json::Value,
        ) = sqlx::query_as(
            r#"
            SELECT attempt.status, attempt.failure_code, attempt.request_id_sha256,
                   request.status, event.data
            FROM qintopia_agent_os.qiwe_image_send_attempts attempt
            JOIN qintopia_agent_os.work_items request ON request.id = attempt.work_item_id
            JOIN qintopia_agent_os.work_item_events event
              ON event.work_item_id = request.id
             AND event.event_type = 'qiwe_image_upload_outcome_ambiguous'
             AND event.data->>'reconciled_after_claim_expiry' = 'true'
            WHERE attempt.id = $1
            "#,
        )
        .bind(abandoned_claim.attempt_id)
        .fetch_one(&pool)
        .await
        .expect("read stale upload terminal state");
        assert_eq!(stale_upload.0, "ambiguous");
        assert_eq!(
            stale_upload.1.as_deref(),
            Some("qiwe_upload_outcome_ambiguous")
        );
        assert_eq!(stale_upload.2, None);
        assert_eq!(stale_upload.3, "failed");
        assert_eq!(stale_upload.4["automatic_retry_allowed"], false);
        assert_eq!(stale_upload.4["external_upload_outcome"], "unknown");
        assert_eq!(stale_upload.4["request_id_persisted"], false);
        assert_eq!(stale_upload.4["external_send_executed"], false);
        delete_fixture(&pool, first_image_id, first_group_id).await;

        let (second_image_id, _second_artifact_id, second_group_id) =
            insert_send_ready_fixture(&pool).await;
        let upload_claim = claim_ready_work_item(&pool, Some(second_group_id), &groups, &hosts)
            .await
            .expect("claim second work item")
            .expect("second work item is claimable");
        let request_id = "stale-sending-request-secret";
        let attempt_id = record_upload_acceptance(&pool, &upload_claim, request_id)
            .await
            .expect("record stale-sending upload acceptance");
        let callback = br#"{
          "requestId":"stale-sending-request-secret",
          "cmd":20000,
          "msgData":{"fileAesKey":"stale-aes-secret","fileId":"stale-file-secret"}
        }"#;
        let send_claim = match claim_callback_for_send(
            &pool,
            request_id,
            callback,
            &integration_callback_file_identity(),
        )
        .await
        .expect("open stale send gate")
        {
            CallbackClaimOutcome::Ready(claim) => claim,
            other => panic!("unexpected callback outcome: {other:?}"),
        };
        assert_eq!(send_claim.attempt_id, attempt_id);
        sqlx::query(
            "UPDATE qintopia_agent_os.work_items SET claim_expires_at = now() - interval '1 second' WHERE id = $1",
        )
        .bind(second_group_id)
        .execute(&pool)
        .await
        .expect("expire stale sending claim");
        let no_retry = claim_ready_work_item(&pool, Some(second_group_id), &groups, &hosts)
            .await
            .expect("reconcile stale sending claim");
        assert!(no_retry.is_none());
        let reconciled: (String, Option<String>, String, serde_json::Value) = sqlx::query_as(
            r#"
            SELECT attempt.status, attempt.failure_code, request.status, event.data
            FROM qintopia_agent_os.qiwe_image_send_attempts attempt
            JOIN qintopia_agent_os.work_items request ON request.id = attempt.work_item_id
            JOIN qintopia_agent_os.work_item_events event
              ON event.work_item_id = request.id
             AND event.event_type = 'qiwe_image_send_outcome_ambiguous'
             AND event.data->>'reconciled_after_claim_expiry' = 'true'
            WHERE attempt.id = $1
            "#,
        )
        .bind(attempt_id)
        .fetch_one(&pool)
        .await
        .expect("read stale send reconciliation");
        assert_eq!(reconciled.0, "ambiguous");
        assert_eq!(reconciled.1.as_deref(), Some("send_outcome_ambiguous"));
        assert_eq!(reconciled.2, "failed");
        assert_eq!(
            reconciled.3["external_send_executed"],
            serde_json::Value::Null
        );
        let serialized = serde_json::to_string(&reconciled).expect("serialize reconciliation");
        for sensitive in [
            request_id,
            "stale-aes-secret",
            "stale-file-secret",
            "integration-group-id",
        ] {
            assert!(!serialized.contains(sensitive));
        }
        delete_fixture(&pool, second_image_id, second_group_id).await;
    }

    #[tokio::test]
    #[cfg(feature = "postgres-integration-tests")]
    #[ignore = "requires guarded disposable qintopia_test PostgreSQL"]
    async fn postgres_qiwe_send_state_terminalizes_legacy_unrecorded_claim() {
        let database_url = postgres_integration_database_url();
        let pool = db::connect(&database_url, 2)
            .await
            .expect("connect disposable PostgreSQL");
        db::run_migrations(&pool)
            .await
            .expect("migrate disposable PostgreSQL");
        let (image_id, _artifact_id, group_id) = insert_send_ready_fixture(&pool).await;
        let groups = BTreeSet::from(["integration-group-id".to_string()]);
        let hosts = BTreeSet::from(["media.example.test".to_string()]);
        let claim = claim_ready_work_item(&pool, Some(group_id), &groups, &hosts)
            .await
            .expect("claim send-ready work item")
            .expect("work item is claimable");
        sqlx::query("DELETE FROM qintopia_agent_os.qiwe_image_send_attempts WHERE id = $1")
            .bind(claim.attempt_id)
            .execute(&pool)
            .await
            .expect("simulate a legacy claim without a persisted attempt");
        sqlx::query(
            "UPDATE qintopia_agent_os.work_items SET claim_expires_at = now() - interval '1 second' WHERE id = $1",
        )
        .bind(group_id)
        .execute(&pool)
        .await
        .expect("expire legacy unrecorded claim");

        let no_retry = claim_ready_work_item(&pool, Some(group_id), &groups, &hosts)
            .await
            .expect("terminalize legacy unrecorded claim");
        assert!(no_retry.is_none());
        let stored: (String, Option<String>, i64, serde_json::Value) = sqlx::query_as(
            r#"
            SELECT request.status, request.last_error,
                   (SELECT count(*) FROM qintopia_agent_os.qiwe_image_send_attempts
                    WHERE work_item_id = request.id),
                   event.data
            FROM qintopia_agent_os.work_items request
            JOIN qintopia_agent_os.work_item_events event
              ON event.work_item_id = request.id
             AND event.event_type = 'qiwe_image_upload_outcome_ambiguous'
             AND event.data->>'attempt_persisted' = 'false'
            WHERE request.id = $1
            "#,
        )
        .bind(group_id)
        .fetch_one(&pool)
        .await
        .expect("read legacy unrecorded terminal state");
        assert_eq!(stored.0, "failed");
        assert_eq!(stored.1.as_deref(), Some("qiwe_upload_outcome_ambiguous"));
        assert_eq!(stored.2, 0);
        assert_eq!(stored.3["automatic_retry_allowed"], false);
        assert_eq!(stored.3["external_upload_outcome"], "unknown");
        assert_eq!(stored.3["request_id_persisted"], false);
        assert_eq!(stored.3["external_send_executed"], false);
        delete_fixture(&pool, image_id, group_id).await;
    }

    #[tokio::test]
    #[cfg(feature = "postgres-integration-tests")]
    #[ignore = "requires guarded disposable qintopia_test PostgreSQL"]
    async fn postgres_qiwe_send_state_rejects_stale_claim() {
        let database_url = postgres_integration_database_url();
        let pool = db::connect(&database_url, 2)
            .await
            .expect("connect disposable PostgreSQL");
        db::run_migrations(&pool)
            .await
            .expect("migrate disposable PostgreSQL");
        let (image_id, _artifact_id, group_id) = insert_send_ready_fixture(&pool).await;
        let groups = BTreeSet::from(["integration-group-id".to_string()]);
        let hosts = BTreeSet::from(["media.example.test".to_string()]);
        let claim = claim_ready_work_item(&pool, Some(group_id), &groups, &hosts)
            .await
            .expect("claim send-ready work item")
            .expect("work item is claimable");
        sqlx::query(
            "UPDATE qintopia_agent_os.work_items SET claim_expires_at = now() - interval '1 second' WHERE id = $1",
        )
        .bind(group_id)
        .execute(&pool)
        .await
        .expect("expire test claim");
        let error = record_upload_acceptance(&pool, &claim, "stale-upload-request")
            .await
            .expect_err("stale claim must not record upload acceptance");
        assert!(error.to_string().contains("no longer current"));
        let stored_attempt: (i64, String, Option<String>) = sqlx::query_as(
            r#"
            SELECT count(*) OVER (), status, request_id_sha256
            FROM qintopia_agent_os.qiwe_image_send_attempts
            WHERE work_item_id = $1
            "#,
        )
        .bind(group_id)
        .fetch_one(&pool)
        .await
        .expect("read stale pre-upload attempt");
        assert_eq!(stored_attempt.0, 1);
        assert_eq!(stored_attempt.1, "uploading");
        assert_eq!(stored_attempt.2, None);
        delete_fixture(&pool, image_id, group_id).await;
    }
}
