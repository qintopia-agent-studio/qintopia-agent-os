use std::{env, fs, os::unix::fs::PermissionsExt, path::Path};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Row};
use uuid::Uuid;

use crate::{config::Cli, db};

const OUTPUT_PREFIX: &str = "xiaoman_real_activity_production_evidence=";
const COMPLETION_LINE: &str = "Xiaoman real activity production evidence retained: signal intake, image generation, human approval, send-ready, QiWe group delivery, and sanitized evidence retention completed";
const RETAINED_REPORT_SCHEMA: &str = "xiaoman-real-activity-production-evidence-v1";
const TARGET_GROUP_ALIAS: &str = "community_activity_group";
const GENERATED_IMAGE_WORKER: &str = "huabaosi-image-generation-worker";
const EVIDENCE_WORKER: &str = "xiaoman-real-activity-production-evidence";
const PRODUCTION_RELEASE_CURRENT_DIR: &str = "/home/ubuntu/qintopia-agent-os-releases/current";
const EXPECTED_SIDECAR_SHA_ENV: &str = "QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_SIDECAR_SHA256";
const EXPECTED_DATABASE_URL_SHA_ENV: &str =
    "QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_DATABASE_URL_SHA256";
const GENERATED_IMAGE_EVIDENCE_SQL: &str = r#"
        SELECT
            image_request.id AS image_generation_work_item_id,
            artifact.id AS generated_image_artifact_id,
            artifact.content_hash,
            artifact.metadata,
            creation.data AS creation_data,
            review.data AS review_data
        FROM qintopia_agent_os.artifacts artifact
        JOIN qintopia_agent_os.work_items image_request
          ON image_request.id = artifact.work_item_id
         AND image_request.capability_key = 'huabaosi.generate_image_asset'
         AND image_request.work_item_type = 'image_generation_request'
         AND image_request.status = 'completed'
        JOIN qintopia_agent_os.work_items visual_request
          ON visual_request.id = image_request.parent_work_item_id
         AND visual_request.parent_work_item_id = $1
         AND visual_request.capability_key = 'huabaosi.create_visual_asset'
         AND visual_request.work_item_type = 'visual_asset_request'
         AND visual_request.status = 'completed'
        JOIN qintopia_agent_os.work_item_events creation
          ON creation.work_item_id = image_request.id
         AND creation.artifact_id = artifact.id
         AND creation.event_type = 'generated_image_created'
         AND creation.actor_type = 'worker'
         AND creation.actor_id = $4
        JOIN qintopia_agent_os.work_item_events review
          ON review.work_item_id = image_request.id
         AND review.artifact_id = artifact.id
         AND review.event_type = 'review_decision_recorded'
         AND review.actor_type = 'human'
         AND review.data->>'previous_review_status' = 'pending'
         AND review.data->>'review_status' = 'approved'
         AND review.data->>'authenticated_feishu_revalidation' = 'true'
         AND review.data->>'does_not_publish' = 'true'
        WHERE artifact.id = $2
          AND artifact.artifact_type = 'generated_image'
          AND artifact.review_status = 'approved'
          AND artifact.content_hash = $3
        ORDER BY review.created_at DESC, review.id DESC
        LIMIT 1
        "#;
const SEND_READY_EVIDENCE_SQL: &str = r#"
        SELECT
            request.id,
            (request.payload->>'approved_artifact_id')::uuid AS generated_image_artifact_id,
            attempt.artifact_content_hash
        FROM qintopia_agent_os.work_items request
        JOIN qintopia_agent_os.work_item_events ready
          ON ready.work_item_id = request.id
         AND ready.event_type = 'group_message_send_ready_recorded'
         AND ready.data->>'send_executed' = 'false'
         AND ready.data->>'target_channel' = request.payload->>'target_channel'
         AND ready.data->>'target_group_alias' = request.payload->>'target_group_alias'
         AND ready.data->>'approved_artifact_id' = request.payload->>'approved_artifact_id'
        JOIN qintopia_agent_os.qiwe_image_send_attempts attempt
          ON attempt.work_item_id = request.id
         AND attempt.generated_image_artifact_id = (request.payload->>'approved_artifact_id')::uuid
         AND attempt.status = 'sent'
         AND attempt.request_id_sha256 IS NOT NULL
         AND attempt.callback_payload_sha256 IS NOT NULL
         AND attempt.provider_message_id_sha256 IS NOT NULL
        WHERE request.parent_work_item_id = $1
          AND request.capability_key = 'erhua.send_group_message'
          AND request.work_item_type = 'group_message_request'
          AND request.requester_agent = 'xiaoman'
          AND request.target_agent = 'erhua'
          AND request.status = 'completed'
          AND request.review_policy = 'human_final_confirmation'
          AND request.payload->>'target_channel' = 'qiwe'
          AND request.payload->>'target_group_alias' = $2
          AND EXISTS (
              SELECT 1
              FROM qintopia_agent_os.work_item_events confirm
              WHERE confirm.work_item_id = request.id
                AND confirm.event_type = 'group_message_final_confirmation_recorded'
                AND confirm.data->>'decision' = 'confirmed'
                AND confirm.data->>'current_status' = 'queued'
                AND confirm.data->>'send_executed' = 'false'
          )
        ORDER BY attempt.completed_at DESC NULLS LAST, attempt.created_at DESC
        "#;

pub async fn run(
    cli: &Cli,
    workflow_root_id: Option<Uuid>,
    source_event_signal_id: Option<Uuid>,
) -> Result<()> {
    if workflow_root_id.is_some() == source_event_signal_id.is_some() {
        bail!("provide exactly one of --workflow-root-id or --source-event-signal-id");
    }

    let database_url = cli.database_url_required()?;
    let boundary = ProductionBoundary::load(database_url)?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let snapshot = load_snapshot(&pool, workflow_root_id, source_event_signal_id).await?;
    let records = snapshot.records(&boundary)?;

    for record in records {
        println!("{OUTPUT_PREFIX}{}", serde_json::to_string(&record)?);
    }
    println!("{COMPLETION_LINE}");
    Ok(())
}

struct ProductionBoundary {
    production_release_sha: String,
    sidecar_binary_sha256: String,
    database_url_sha256: String,
    release_binary_verified: bool,
    approved_sidecar_sha256_matched: bool,
    approved_database_url_sha256_matched: bool,
}

impl ProductionBoundary {
    fn load(database_url: &str) -> Result<Self> {
        let production_release_sha = env::var("QINTOPIA_DEPLOYED_COMMIT_SHA")
            .context("QINTOPIA_DEPLOYED_COMMIT_SHA is required")?;
        if !is_lower_hex(&production_release_sha, 40) {
            bail!("QINTOPIA_DEPLOYED_COMMIT_SHA must be a 40-character lowercase Git SHA");
        }
        let expected_sidecar_sha256 = env::var(EXPECTED_SIDECAR_SHA_ENV)
            .with_context(|| format!("{EXPECTED_SIDECAR_SHA_ENV} is required"))?;
        if !is_lower_hex(&expected_sidecar_sha256, 64) {
            bail!("{EXPECTED_SIDECAR_SHA_ENV} must be a canonical lowercase SHA-256");
        }
        let expected_database_url_sha256 = env::var(EXPECTED_DATABASE_URL_SHA_ENV)
            .with_context(|| format!("{EXPECTED_DATABASE_URL_SHA_ENV} is required"))?;
        if !is_lower_hex(&expected_database_url_sha256, 64) {
            bail!("{EXPECTED_DATABASE_URL_SHA_ENV} must be a canonical lowercase SHA-256");
        }
        let database_url_sha256 = sha256_hex(database_url.as_bytes());
        if database_url_sha256 != expected_database_url_sha256 {
            bail!("configured database URL does not match the owner-approved SHA-256");
        }
        let sidecar_path = env::current_exe().context("resolve current sidecar binary path")?;
        verify_release_binary_path(&sidecar_path, &production_release_sha)?;
        let sidecar_bytes = fs::read(&sidecar_path).context("read current sidecar binary")?;
        let sidecar_binary_sha256 = sha256_hex(&sidecar_bytes);
        if sidecar_binary_sha256 != expected_sidecar_sha256 {
            bail!("current sidecar binary does not match the owner-approved SHA-256");
        }
        Ok(Self {
            production_release_sha,
            sidecar_binary_sha256,
            database_url_sha256,
            release_binary_verified: true,
            approved_sidecar_sha256_matched: true,
            approved_database_url_sha256_matched: true,
        })
    }

    fn common(&self, phase: &str, worker: &str, action_status: &str) -> Value {
        json!({
            "phase": phase,
            "success": true,
            "worker": worker,
            "action_status": action_status,
            "production_release_sha": self.production_release_sha,
            "sidecar_binary_sha256": self.sidecar_binary_sha256,
            "database_url_sha256": self.database_url_sha256,
            "release_binary_verified": self.release_binary_verified,
            "approved_sidecar_sha256_matched": self.approved_sidecar_sha256_matched,
            "approved_database_url_sha256_matched": self.approved_database_url_sha256_matched,
            "safe_for_chat": false,
        })
    }
}

fn verify_release_binary_path(sidecar_path: &Path, production_release_sha: &str) -> Result<()> {
    let current_real = fs::canonicalize(PRODUCTION_RELEASE_CURRENT_DIR)
        .context("resolve production release/current directory")?;
    if current_real.file_name().and_then(|name| name.to_str()) != Some(production_release_sha) {
        bail!("production release/current does not match QINTOPIA_DEPLOYED_COMMIT_SHA");
    }
    let expected = current_real
        .join("sidecar")
        .join("qintopia-message-sidecar");
    let sidecar_real = fs::canonicalize(sidecar_path).context("resolve current sidecar binary")?;
    if sidecar_real != expected {
        bail!(
            "production evidence export must run from the immutable release/current sidecar binary"
        );
    }
    if symlink_metadata(&sidecar_real)?.file_type().is_symlink() {
        bail!("production sidecar binary must not be a symlink");
    }
    if !symlink_metadata(&sidecar_real)?.is_file() {
        bail!("production sidecar binary is missing");
    }
    for path in [
        current_real,
        expected
            .parent()
            .context("production sidecar binary parent is missing")?
            .to_path_buf(),
        sidecar_real,
    ] {
        reject_group_or_world_writable(&path)?;
    }
    Ok(())
}

fn symlink_metadata(path: &Path) -> Result<fs::Metadata> {
    fs::symlink_metadata(path).with_context(|| format!("inspect {}", path.display()))
}

fn reject_group_or_world_writable(path: &Path) -> Result<()> {
    let mode = symlink_metadata(path)?.permissions().mode();
    if mode & 0o022 != 0 {
        bail!("production release binary path must not be group/world writable");
    }
    Ok(())
}

struct ActivityRoot {
    id: Uuid,
    source_event_signal_id: Uuid,
    activity_phase: String,
    activity_route: String,
}

struct SendReadyEvidence {
    work_item_id: Uuid,
    generated_image_artifact_id: Uuid,
    artifact_content_hash: String,
}

struct GeneratedImageEvidence {
    work_item_id: Uuid,
    artifact_id: Uuid,
    content_hash: String,
    storage_backend: String,
    mime_type: String,
    width: i64,
    height: i64,
    byte_size: i64,
    approval_authenticated_feishu_revalidation: bool,
}

struct QiweSentEvidence {
    callback_credential_schema: String,
    callback_additional_field_count: i64,
}

struct ActivityEvidenceSnapshot {
    root: ActivityRoot,
    send_ready: SendReadyEvidence,
    image: GeneratedImageEvidence,
    qiwe: QiweSentEvidence,
}

impl ActivityEvidenceSnapshot {
    fn records(&self, boundary: &ProductionBoundary) -> Result<Vec<Value>> {
        if self.image.artifact_id != self.send_ready.generated_image_artifact_id
            || self.image.content_hash != self.send_ready.artifact_content_hash
        {
            bail!("generated image evidence does not match send-ready artifact");
        }
        if !self.image.approval_authenticated_feishu_revalidation {
            bail!("generated image approval did not record authenticated Feishu revalidation");
        }

        let mut signal = boundary.common(
            "signal_intake",
            "xiaoman-activity-signal-worker",
            "signal_ingest_submitted",
        );
        merge(
            &mut signal,
            json!({
                "apply_requested": true,
                "dry_run": false,
                "source_event_signal_id": self.root.source_event_signal_id,
                "workflow_root_id": self.root.id,
                "activity_phase": self.root.activity_phase,
                "activity_route": self.root.activity_route,
                "external_send_executed": false,
            }),
        );

        let mut generation = boundary.common(
            "image_generation",
            GENERATED_IMAGE_WORKER,
            "generated_image_created",
        );
        merge(
            &mut generation,
            json!({
                "apply_requested": true,
                "dry_run": false,
                "workflow_root_id": self.root.id,
                "image_generation_work_item_id": self.image.work_item_id,
                "generated_image_artifact_id": self.image.artifact_id,
                "artifact_content_hash": self.image.content_hash,
                "artifact_type": "generated_image",
                "review_status": "pending",
                "storage_backend": self.image.storage_backend,
                "mime_type": self.image.mime_type,
                "width": self.image.width,
                "height": self.image.height,
                "byte_size": self.image.byte_size,
                "external_send_executed": false,
            }),
        );

        let mut approval = boundary.common(
            "human_approval",
            "huabaosi-generated-image-review",
            "generated_image_approved",
        );
        merge(
            &mut approval,
            json!({
                "workflow_root_id": self.root.id,
                "generated_image_artifact_id": self.image.artifact_id,
                "artifact_content_hash": self.image.content_hash,
                "artifact_type": "generated_image",
                "previous_review_status": "pending",
                "review_status": "approved",
                "human_review_applied": true,
                "feishu_revalidation_executed": true,
                "external_send_executed": false,
            }),
        );

        let mut send_ready = boundary.common(
            "send_ready",
            "operations-group-send-ready",
            "send_ready_recorded",
        );
        merge(
            &mut send_ready,
            json!({
                "workflow_root_id": self.root.id,
                "send_ready_work_item_id": self.send_ready.work_item_id,
                "generated_image_artifact_id": self.image.artifact_id,
                "artifact_content_hash": self.image.content_hash,
                "target_channel": "qiwe",
                "target_group_alias": TARGET_GROUP_ALIAS,
                "review_policy": "human_final_confirmation",
                "final_confirmation_recorded": true,
                "external_send_executed": false,
            }),
        );

        let mut upload = boundary.common(
            "qiwe_upload",
            "qiwe-image-send-adapter",
            "image_upload_accepted",
        );
        merge(
            &mut upload,
            json!({
                "send_ready_work_item_id": self.send_ready.work_item_id,
                "generated_image_artifact_id": self.image.artifact_id,
                "artifact_content_hash": self.image.content_hash,
                "apply_requested": true,
                "dry_run": false,
                "external_upload_requested": true,
                "callback_received": false,
                "external_send_executed": false,
            }),
        );

        let mut callback = boundary.common(
            "qiwe_callback_send",
            "qiwe-image-send-adapter",
            "image_send_completed",
        );
        merge(
            &mut callback,
            json!({
                "send_ready_work_item_id": self.send_ready.work_item_id,
                "generated_image_artifact_id": self.image.artifact_id,
                "artifact_content_hash": self.image.content_hash,
                "apply_requested": true,
                "dry_run": false,
                "external_upload_requested": false,
                "callback_received": true,
                "callback_credential_schema": self.qiwe.callback_credential_schema,
                "callback_additional_field_count": self.qiwe.callback_additional_field_count,
                "external_send_executed": true,
            }),
        );

        let mut retention = boundary.common(
            "sanitized_evidence_retention",
            EVIDENCE_WORKER,
            "sanitized_evidence_retained",
        );
        merge(
            &mut retention,
            json!({
                "source_event_signal_id": self.root.source_event_signal_id,
                "workflow_root_id": self.root.id,
                "send_ready_work_item_id": self.send_ready.work_item_id,
                "generated_image_artifact_id": self.image.artifact_id,
                "artifact_content_hash": self.image.content_hash,
                "retained_report_schema": RETAINED_REPORT_SCHEMA,
                "raw_secret_fields_retained": false,
                "external_send_executed": true,
            }),
        );

        Ok(vec![
            signal, generation, approval, send_ready, upload, callback, retention,
        ])
    }
}

async fn load_snapshot(
    pool: &PgPool,
    workflow_root_id: Option<Uuid>,
    source_event_signal_id: Option<Uuid>,
) -> Result<ActivityEvidenceSnapshot> {
    let root = load_activity_root(pool, workflow_root_id, source_event_signal_id).await?;
    let send_ready = load_send_ready(pool, root.id).await?;
    let image = load_generated_image(
        pool,
        root.id,
        send_ready.generated_image_artifact_id,
        &send_ready.artifact_content_hash,
    )
    .await?;
    let qiwe = load_qiwe_sent(pool, send_ready.work_item_id).await?;
    Ok(ActivityEvidenceSnapshot {
        root,
        send_ready,
        image,
        qiwe,
    })
}

async fn load_activity_root(
    pool: &PgPool,
    workflow_root_id: Option<Uuid>,
    source_event_signal_id: Option<Uuid>,
) -> Result<ActivityRoot> {
    let rows = sqlx::query(
        r#"
        SELECT
            id,
            source_event_signal_id,
            work_item_type,
            COALESCE(NULLIF(payload->>'activity_phase', ''), '') AS activity_phase
        FROM qintopia_agent_os.work_items
        WHERE capability_key = 'xiaoman.create_activity_request'
          AND requester_agent = 'default'
          AND target_agent = 'xiaoman'
          AND source_type = 'event_signal'
          AND ($1::uuid IS NULL OR id = $1)
          AND ($2::uuid IS NULL OR source_event_signal_id = $2)
        ORDER BY created_at DESC
        "#,
    )
    .bind(workflow_root_id)
    .bind(source_event_signal_id)
    .fetch_all(pool)
    .await
    .context("load Xiaoman activity root")?;
    if rows.len() != 1 {
        bail!("expected exactly one Xiaoman event-signal activity root");
    }
    let row = &rows[0];
    let id: Uuid = row.try_get("id")?;
    let source_event_signal_id = row
        .try_get::<Option<Uuid>, _>("source_event_signal_id")?
        .context("Xiaoman activity root is missing source_event_signal_id")?;
    let work_item_type: String = row.try_get("work_item_type")?;
    let raw_phase: String = row.try_get("activity_phase")?;
    let activity_phase = match (raw_phase.as_str(), work_item_type.as_str()) {
        ("pre_event", "activity_promotion_request") | ("", "activity_promotion_request") => {
            "pre_event"
        }
        ("post_event", "activity_recap_request") => "post_event",
        _ => bail!("Xiaoman activity root phase is not eligible for production send evidence"),
    };
    let activity_route = match activity_phase {
        "pre_event" => "activity_promotion",
        "post_event" => "activity_recap",
        _ => unreachable!("phase matched above"),
    };
    Ok(ActivityRoot {
        id,
        source_event_signal_id,
        activity_phase: activity_phase.to_string(),
        activity_route: activity_route.to_string(),
    })
}

async fn load_send_ready(pool: &PgPool, root_id: Uuid) -> Result<SendReadyEvidence> {
    let rows = sqlx::query(SEND_READY_EVIDENCE_SQL)
        .bind(root_id)
        .bind(TARGET_GROUP_ALIAS)
        .fetch_all(pool)
        .await
        .context("load Xiaoman send-ready QiWe delivery")?;
    if rows.len() != 1 {
        bail!("expected exactly one sent QiWe group-message request under the activity root");
    }
    let row = &rows[0];
    Ok(SendReadyEvidence {
        work_item_id: row.try_get("id")?,
        generated_image_artifact_id: row.try_get("generated_image_artifact_id")?,
        artifact_content_hash: row.try_get("artifact_content_hash")?,
    })
}

async fn load_generated_image(
    pool: &PgPool,
    root_id: Uuid,
    artifact_id: Uuid,
    content_hash: &str,
) -> Result<GeneratedImageEvidence> {
    let row = sqlx::query(GENERATED_IMAGE_EVIDENCE_SQL)
        .bind(root_id)
        .bind(artifact_id)
        .bind(content_hash)
        .bind(GENERATED_IMAGE_WORKER)
        .fetch_optional(pool)
        .await
        .context("load approved generated image evidence")?
        .context("generated image evidence is missing or not bound to the activity root")?;

    let metadata: Value = row.try_get("metadata")?;
    let creation_data: Value = row.try_get("creation_data")?;
    let review_data: Value = row.try_get("review_data")?;
    let stored_content_hash: String = row.try_get("content_hash")?;
    validate_generated_image_metadata(&metadata, &creation_data, &stored_content_hash)?;
    if review_data
        .get("previous_review_status")
        .and_then(Value::as_str)
        != Some("pending")
        || review_data.get("review_status").and_then(Value::as_str) != Some("approved")
        || review_data
            .get("authenticated_feishu_revalidation")
            .and_then(Value::as_bool)
            != Some(true)
        || review_data.get("does_not_publish").and_then(Value::as_bool) != Some(true)
    {
        bail!("generated image approval event does not satisfy the production evidence boundary");
    }

    Ok(GeneratedImageEvidence {
        work_item_id: row.try_get("image_generation_work_item_id")?,
        artifact_id: row.try_get("generated_image_artifact_id")?,
        content_hash: stored_content_hash,
        storage_backend: text_field(&metadata, "storage_provider")?,
        mime_type: text_field(&metadata, "mime_type")?,
        width: i64_field(&metadata, "width")?,
        height: i64_field(&metadata, "height")?,
        byte_size: i64_field(&metadata, "byte_size")?,
        approval_authenticated_feishu_revalidation: true,
    })
}

async fn load_qiwe_sent(pool: &PgPool, send_work_item_id: Uuid) -> Result<QiweSentEvidence> {
    let row = sqlx::query(
        r#"
        SELECT audit_metadata
        FROM qintopia_agent_os.qiwe_image_send_attempts
        WHERE work_item_id = $1
          AND status = 'sent'
          AND request_id_sha256 IS NOT NULL
          AND callback_payload_sha256 IS NOT NULL
          AND provider_message_id_sha256 IS NOT NULL
        ORDER BY completed_at DESC NULLS LAST, created_at DESC
        LIMIT 1
        "#,
    )
    .bind(send_work_item_id)
    .fetch_optional(pool)
    .await
    .context("load sent QiWe image attempt")?
    .context("sent QiWe image attempt is missing")?;
    let audit_metadata: Value = row.try_get("audit_metadata")?;
    if audit_metadata
        .get("callback_credentials_persisted")
        .and_then(Value::as_bool)
        != Some(false)
        || audit_metadata
            .get("external_upload_outcome")
            .and_then(Value::as_str)
            != Some("accepted")
        || audit_metadata
            .get("provider_confirmed_success")
            .and_then(Value::as_bool)
            != Some(true)
        || audit_metadata
            .get("external_send_executed")
            .and_then(Value::as_bool)
            != Some(true)
    {
        bail!("sent QiWe image attempt does not satisfy the sanitized audit boundary");
    }
    Ok(QiweSentEvidence {
        callback_credential_schema: text_field(&audit_metadata, "callback_credential_schema")?,
        callback_additional_field_count: i64_field(
            &audit_metadata,
            "callback_additional_field_count",
        )?,
    })
}

fn validate_generated_image_metadata(
    metadata: &Value,
    creation_data: &Value,
    content_hash: &str,
) -> Result<()> {
    for key in [
        "mime_type",
        "width",
        "height",
        "byte_size",
        "storage_provider",
    ] {
        if metadata.get(key) != creation_data.get(key) {
            bail!("generated image metadata does not match the creation event");
        }
    }
    if creation_data.get("content_hash").and_then(Value::as_str) != Some(content_hash)
        || creation_data
            .get("external_publish_executed")
            .and_then(Value::as_bool)
            != Some(false)
    {
        bail!("generated image creation event does not satisfy the production evidence boundary");
    }
    if text_field(metadata, "mime_type")? != "image/jpeg"
        || text_field(metadata, "storage_provider")? != "feishu-base"
        || i64_field(metadata, "width")? != 1024
        || i64_field(metadata, "height")? != 1024
        || i64_field(metadata, "byte_size")? <= 0
    {
        bail!("generated image metadata is not the reviewed Feishu-backed 1024 JPEG");
    }
    Ok(())
}

fn text_field(value: &Value, key: &str) -> Result<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
        .with_context(|| format!("{key} is missing from sanitized evidence source"))
}

fn i64_field(value: &Value, key: &str) -> Result<i64> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .with_context(|| format!("{key} is missing from sanitized evidence source"))
}

fn merge(base: &mut Value, fields: Value) {
    let base = base.as_object_mut().expect("base evidence is an object");
    let fields = fields.as_object().expect("extra evidence is an object");
    for (key, value) in fields {
        base.insert(key.clone(), value.clone());
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_lower_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boundary() -> ProductionBoundary {
        ProductionBoundary {
            production_release_sha: "a".repeat(40),
            sidecar_binary_sha256: "b".repeat(64),
            database_url_sha256: "c".repeat(64),
            release_binary_verified: true,
            approved_sidecar_sha256_matched: true,
            approved_database_url_sha256_matched: true,
        }
    }

    fn snapshot() -> ActivityEvidenceSnapshot {
        ActivityEvidenceSnapshot {
            root: ActivityRoot {
                id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
                source_event_signal_id: Uuid::parse_str("22222222-2222-4222-8222-222222222222")
                    .unwrap(),
                activity_phase: "pre_event".to_string(),
                activity_route: "activity_promotion".to_string(),
            },
            send_ready: SendReadyEvidence {
                work_item_id: Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
                generated_image_artifact_id: Uuid::parse_str(
                    "44444444-4444-4444-8444-444444444444",
                )
                .unwrap(),
                artifact_content_hash: format!("sha256:{}", "d".repeat(64)),
            },
            image: GeneratedImageEvidence {
                work_item_id: Uuid::parse_str("55555555-5555-4555-8555-555555555555").unwrap(),
                artifact_id: Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap(),
                content_hash: format!("sha256:{}", "d".repeat(64)),
                storage_backend: "feishu-base".to_string(),
                mime_type: "image/jpeg".to_string(),
                width: 1024,
                height: 1024,
                byte_size: 4096,
                approval_authenticated_feishu_revalidation: true,
            },
            qiwe: QiweSentEvidence {
                callback_credential_schema: "fileAesKey+fileId+fileMd5+fileSize+filename"
                    .to_string(),
                callback_additional_field_count: 0,
            },
        }
    }

    #[test]
    fn evidence_records_match_the_public_checker_shape() {
        let records = snapshot().records(&boundary()).unwrap();
        assert_eq!(records.len(), 7);
        assert_eq!(records[0]["phase"], "signal_intake");
        assert_eq!(records[5]["external_send_executed"], true);
        assert_eq!(records[6]["raw_secret_fields_retained"], false);
        let serialized = records
            .iter()
            .map(|record| format!("{OUTPUT_PREFIX}{}", serde_json::to_string(record).unwrap()))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in [
            "https://",
            "postgres://",
            "request_id",
            "callback_event_id",
            "target_group_id",
            "provider_response",
        ] {
            assert!(!serialized.contains(forbidden));
        }
    }

    #[test]
    fn evidence_records_fail_when_chain_identity_drifts() {
        let mut snapshot = snapshot();
        snapshot.image.content_hash = format!("sha256:{}", "e".repeat(64));
        assert!(snapshot.records(&boundary()).is_err());
    }

    #[test]
    fn generated_image_metadata_requires_creation_event_parity() {
        let metadata = json!({
            "mime_type": "image/jpeg",
            "width": 1024,
            "height": 1024,
            "byte_size": 4096,
            "storage_provider": "feishu-base",
        });
        let creation = json!({
            "content_hash": format!("sha256:{}", "f".repeat(64)),
            "mime_type": "image/jpeg",
            "width": 1024,
            "height": 1024,
            "byte_size": 4096,
            "storage_provider": "feishu-base",
            "external_publish_executed": false,
        });
        validate_generated_image_metadata(
            &metadata,
            &creation,
            &format!("sha256:{}", "f".repeat(64)),
        )
        .unwrap();
        let mut drifted = creation;
        drifted["byte_size"] = json!(4097);
        assert!(validate_generated_image_metadata(
            &metadata,
            &drifted,
            &format!("sha256:{}", "f".repeat(64)),
        )
        .is_err());
    }

    #[test]
    fn generated_image_query_selects_the_final_human_approval_event() {
        for fragment in [
            "review.data->>'previous_review_status' = 'pending'",
            "review.data->>'review_status' = 'approved'",
            "review.data->>'authenticated_feishu_revalidation' = 'true'",
            "review.data->>'does_not_publish' = 'true'",
            "ORDER BY review.created_at DESC, review.id DESC",
            "LIMIT 1",
        ] {
            assert!(
                GENERATED_IMAGE_EVIDENCE_SQL.contains(fragment),
                "generated image evidence query is missing {fragment}"
            );
        }
    }

    #[test]
    fn send_ready_query_binds_final_confirmation_ready_event_and_sent_attempt() {
        for fragment in [
            "ready.data->>'target_channel' = request.payload->>'target_channel'",
            "ready.data->>'target_group_alias' = request.payload->>'target_group_alias'",
            "ready.data->>'approved_artifact_id' = request.payload->>'approved_artifact_id'",
            "attempt.request_id_sha256 IS NOT NULL",
            "attempt.callback_payload_sha256 IS NOT NULL",
            "attempt.provider_message_id_sha256 IS NOT NULL",
            "request.requester_agent = 'xiaoman'",
            "request.target_agent = 'erhua'",
            "request.status = 'completed'",
            "confirm.data->>'current_status' = 'queued'",
        ] {
            assert!(
                SEND_READY_EVIDENCE_SQL.contains(fragment),
                "send-ready evidence query is missing {fragment}"
            );
        }
    }
}
