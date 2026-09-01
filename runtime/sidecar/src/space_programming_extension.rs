use std::{env, ffi::OsStr};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Postgres, Row, Transaction};
use url::Url;
use uuid::Uuid;

use crate::channel_event_mapping;
use crate::space_configuration::{
    validate_programming_research_evidence, ProgrammingExtensionRequest,
    ProgrammingExtensionResearchEvidence, PROGRAMMING_EXTENSION_ALLOWED_CHANGE_CLASS,
    PROGRAMMING_EXTENSION_SCOPE,
};

const ENABLE_ENV: &str = "QINTOPIA_SPACE_PROGRAMMING_EXTENSION_DISPATCH_ENABLED";
const PROTOCOL_VERSION: u8 = 1;
const WORK_ITEM_TYPE: &str = "space_programming_extension_request";
const TARGET_AGENT: &str = "programming_agent";
const REQUESTER_AGENT: &str = "erhua";
const CAPABILITY_KEY: &str = "erhua.manage_space_configuration";
const CLAIMED_BY: &str = "space-programming-extension-dispatcher-v1";
const CLAIM_TTL_MINUTES: i64 = 45;
const MAX_INVALID_CANDIDATES_PER_CLAIM: usize = 16;
const DEPLOYED_COMMIT_SHA_ENV: &str = "QINTOPIA_DEPLOYED_COMMIT_SHA";
const CONTINUATION_SCHEMA_VERSION: u8 = 1;
const CONTINUATION_DIGEST_DOMAIN: &[u8] = b"qintopia-space-programming-continuation-v1\0";

#[derive(Debug, Clone, Copy)]
pub(crate) struct DispatchConfig {
    enabled: bool,
}

impl DispatchConfig {
    pub(crate) fn from_env() -> Result<Self> {
        Self::from_value(env::var_os(ENABLE_ENV).as_deref())
    }

    fn from_value(value: Option<&OsStr>) -> Result<Self> {
        match value.and_then(OsStr::to_str) {
            None | Some("") | Some("0") => Ok(Self { enabled: false }),
            Some("1") => Ok(Self { enabled: true }),
            _ => bail!("{ENABLE_ENV} must be unset, 0, or 1"),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum BrokerRequest {
    SpaceProgrammingExtensionClaim {
        schema_version: u8,
    },
    SpaceProgrammingExtensionFinish {
        schema_version: u8,
        work_item_id: Uuid,
        claim_token: String,
        result: RunnerResult,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum RunnerResult {
    Succeeded {
        pr_url: String,
        pr_number: u64,
        candidate_sha: String,
        mapping_key: String,
        mapping_sha256: String,
        validation_status: String,
    },
    Failed {
        failure_code: String,
        validation_status: String,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProgrammingExtensionContinuation {
    schema_version: u8,
    phase: String,
    release_phase: String,
    provider: String,
    mapping_key: String,
    mapping_sha256: String,
    candidate_sha: String,
    pr_number: u64,
    request_sha256: String,
    binding_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deployed_release_sha: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReadyContinuation {
    pub(crate) intent: String,
    pub(crate) provider: String,
    pub(crate) mapping_key: String,
}

#[derive(Debug, Serialize)]
struct ClaimEnvelope {
    schema_version: u8,
    claimed: bool,
    work_item_id: Uuid,
    claim_token: String,
    claim_expires_at: DateTime<Utc>,
    intent: String,
    provider: String,
    research_query: String,
    official_sources: Vec<String>,
    research_evidence: Vec<ProgrammingExtensionResearchEvidence>,
    research_digest: String,
}

pub(crate) async fn handle(
    pool: &PgPool,
    config: DispatchConfig,
    request: BrokerRequest,
) -> Result<Value> {
    if !config.enabled {
        bail!("Space programming extension dispatch is disabled");
    }
    match request {
        BrokerRequest::SpaceProgrammingExtensionClaim { schema_version } => {
            validate_protocol(schema_version)?;
            claim(pool).await
        }
        BrokerRequest::SpaceProgrammingExtensionFinish {
            schema_version,
            work_item_id,
            claim_token,
            result,
        } => {
            validate_protocol(schema_version)?;
            finish(pool, work_item_id, &claim_token, result).await
        }
    }
}

fn validate_protocol(schema_version: u8) -> Result<()> {
    if schema_version != PROTOCOL_VERSION {
        bail!("unsupported Space programming extension broker schema version");
    }
    Ok(())
}

async fn claim(pool: &PgPool) -> Result<Value> {
    let mut tx = pool
        .begin()
        .await
        .context("begin Space programming extension claim")?;
    terminalize_expired_claims(&mut tx).await?;

    for _ in 0..MAX_INVALID_CANDIDATES_PER_CLAIM {
        let candidate = sqlx::query(
            r#"
            SELECT item.id, item.payload
            FROM qintopia_agent_os.work_items item
            JOIN qintopia_agent_os.capabilities capability
              ON capability.capability_key = item.capability_key
             AND capability.enabled
             AND $1 = ANY(capability.allowed_callers)
             AND $2 = ANY(capability.allowed_work_item_types)
            WHERE item.work_item_type = $2
              AND item.target_agent = $3
              AND item.requester_agent = $1
              AND item.capability_key = $4
              AND item.status = 'queued'
              AND item.available_at <= now()
              AND item.attempts = 0
              AND item.space_id IS NOT NULL
              AND item.review_policy = 'low_risk_classifier'
              AND item.payload_redaction_policy = 'summary_only'
              AND item.information_class = 'internal_ops'
              AND item.metadata @> $5::jsonb
              AND jsonb_typeof(item.payload) = 'object'
            ORDER BY
              CASE item.priority
                WHEN 'urgent' THEN 0
                WHEN 'high' THEN 1
                WHEN 'normal' THEN 2
                ELSE 3
              END,
              item.created_at,
              item.id
            FOR UPDATE OF item SKIP LOCKED
            LIMIT 1
            "#,
        )
        .bind(REQUESTER_AGENT)
        .bind(WORK_ITEM_TYPE)
        .bind(TARGET_AGENT)
        .bind(CAPABILITY_KEY)
        .bind(required_metadata())
        .fetch_optional(&mut *tx)
        .await
        .context("select Space programming extension work item")?;

        let Some(candidate) = candidate else {
            tx.commit()
                .await
                .context("commit empty Space programming extension claim")?;
            return Ok(json!({
                "schema_version": PROTOCOL_VERSION,
                "claimed": false
            }));
        };
        let work_item_id: Uuid = candidate.try_get("id")?;
        let payload: Value = candidate.try_get("payload")?;
        let request = match validate_payload(payload) {
            Ok(request) => request,
            Err(_) => {
                terminalize_invalid_candidate(&mut tx, work_item_id).await?;
                continue;
            }
        };

        let claim_token = new_claim_token();
        let claim_token_sha256 = sha256_hex(claim_token.as_bytes());
        let claim_expires_at = Utc::now() + Duration::minutes(CLAIM_TTL_MINUTES);
        let updated = sqlx::query(
            r#"
            UPDATE qintopia_agent_os.work_items
            SET status = 'processing',
                claimed_by = $2,
                locked_at = now(),
                claim_expires_at = $3,
                attempts = attempts + 1,
                last_error = NULL,
                metadata = jsonb_set(
                    metadata,
                    '{programming_extension_claim}',
                    jsonb_build_object(
                        'schema_version', $4::integer,
                        'token_sha256', $5::text
                    ),
                    true
                ),
                updated_at = now()
            WHERE id = $1
              AND status = 'queued'
              AND attempts = 0
              AND claimed_by IS NULL
              AND locked_at IS NULL
              AND claim_expires_at IS NULL
            "#,
        )
        .bind(work_item_id)
        .bind(CLAIMED_BY)
        .bind(claim_expires_at)
        .bind(PROTOCOL_VERSION as i32)
        .bind(&claim_token_sha256)
        .execute(&mut *tx)
        .await
        .context("claim Space programming extension work item")?;
        if updated.rows_affected() != 1 {
            bail!("Space programming extension claim changed concurrently");
        }
        append_event(
            &mut tx,
            work_item_id,
            "space_programming_extension_claimed",
            "Programming Agent claimed one bounded extension",
            json!({
                "attempt": 1,
                "lease_minutes": CLAIM_TTL_MINUTES,
                "external_send_executed": false
            }),
        )
        .await?;

        tx.commit()
            .await
            .context("commit Space programming extension claim")?;
        return serde_json::to_value(ClaimEnvelope {
            schema_version: PROTOCOL_VERSION,
            claimed: true,
            work_item_id,
            claim_token,
            claim_expires_at,
            intent: request.intent,
            provider: request.provider,
            research_query: request.research_query,
            official_sources: request.official_sources,
            research_evidence: request.research_evidence,
            research_digest: request.research_digest,
        })
        .context("serialize Space programming extension claim");
    }

    tx.commit()
        .await
        .context("commit terminalized invalid Space programming extension candidates")?;
    Ok(json!({
        "schema_version": PROTOCOL_VERSION,
        "claimed": false,
        "invalid_candidates_terminalized": MAX_INVALID_CANDIDATES_PER_CLAIM
    }))
}

async fn finish(
    pool: &PgPool,
    work_item_id: Uuid,
    claim_token: &str,
    result: RunnerResult,
) -> Result<Value> {
    validate_claim_token(claim_token)?;
    validate_runner_result(&result)?;
    let mut tx = pool
        .begin()
        .await
        .context("begin Space programming extension completion")?;
    let row = sqlx::query(
        r#"
        SELECT space_id, payload,
               metadata #>> '{programming_extension_claim,token_sha256}' AS token_sha256
        FROM qintopia_agent_os.work_items
        WHERE id = $1
          AND work_item_type = $2
          AND target_agent = $3
          AND requester_agent = $4
          AND capability_key = $5
          AND status = 'processing'
          AND attempts = 1
          AND claimed_by = $6
          AND claim_expires_at > now()
          AND metadata @> $7::jsonb
        FOR UPDATE
        "#,
    )
    .bind(work_item_id)
    .bind(WORK_ITEM_TYPE)
    .bind(TARGET_AGENT)
    .bind(REQUESTER_AGENT)
    .bind(CAPABILITY_KEY)
    .bind(CLAIMED_BY)
    .bind(required_metadata())
    .fetch_optional(&mut *tx)
    .await
    .context("lock Space programming extension claim")?
    .context("Space programming extension claim is missing or expired")?;
    let stored_token_sha256: String = row.try_get("token_sha256")?;
    let supplied_token_sha256 = sha256_hex(claim_token.as_bytes());
    if !constant_time_eq(
        stored_token_sha256.as_bytes(),
        supplied_token_sha256.as_bytes(),
    ) {
        bail!("Space programming extension claim token does not match");
    }

    let capability = sqlx::query(
        r#"
        SELECT enabled, allowed_callers, allowed_work_item_types
        FROM qintopia_agent_os.capabilities
        WHERE capability_key = $1
        FOR UPDATE
        "#,
    )
    .bind(CAPABILITY_KEY)
    .fetch_optional(&mut *tx)
    .await
    .context("lock Space programming extension capability")?
    .context("Space programming extension capability is missing")?;
    let enabled: bool = capability.try_get("enabled")?;
    let allowed_callers: Vec<String> = capability.try_get("allowed_callers")?;
    let allowed_work_item_types: Vec<String> = capability.try_get("allowed_work_item_types")?;
    if !enabled
        || !allowed_callers
            .iter()
            .any(|caller| caller == REQUESTER_AGENT)
        || !allowed_work_item_types
            .iter()
            .any(|work_item_type| work_item_type == WORK_ITEM_TYPE)
    {
        bail!("Space programming extension capability authorization is no longer active");
    }

    let space_id: Uuid = row
        .try_get::<Option<Uuid>, _>("space_id")?
        .context("Space programming extension lost its Space binding")?;
    let payload: Value = row.try_get("payload")?;
    let request = validate_payload(payload.clone())?;
    let request_sha256 = sha256_hex(
        &serde_json::to_vec(&payload).context("encode programming extension request binding")?,
    );
    let continuation = match &result {
        RunnerResult::Succeeded {
            pr_number,
            candidate_sha,
            mapping_key,
            mapping_sha256,
            ..
        } => Some(new_continuation(
            space_id,
            &request,
            &request_sha256,
            *pr_number,
            candidate_sha,
            mapping_key,
            mapping_sha256,
        )),
        RunnerResult::Failed { .. } => None,
    };
    let (status, failure_code, event_type, event_message) = match &result {
        RunnerResult::Succeeded { .. } => (
            "awaiting_publish",
            None,
            "space_programming_extension_pr_created",
            "Programming Agent created one validated pull request",
        ),
        RunnerResult::Failed { failure_code, .. } => (
            "failed",
            Some(failure_code.as_str()),
            "space_programming_extension_failed",
            "Programming Agent extension attempt failed closed",
        ),
    };
    let result_value = serde_json::to_value(&result).context("serialize runner result")?;
    let metadata_additions = match &continuation {
        Some(continuation) => json!({
            "programming_extension_continuation": continuation
        }),
        None => json!({}),
    };
    let event_data = match &continuation {
        Some(continuation) => json!({
            "phase": continuation.phase,
            "release_phase": continuation.release_phase,
            "pr_number": continuation.pr_number,
            "candidate_fingerprint": short_fingerprint(&continuation.candidate_sha),
            "mapping_provider": continuation.provider,
            "mapping_key": continuation.mapping_key,
            "mapping_digest_fingerprint": short_fingerprint(&continuation.mapping_sha256),
            "external_send_executed": false
        }),
        None => json!({
            "failure_code": failure_code,
            "validation_status": "failed",
            "external_send_executed": false
        }),
    };
    let updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET status = $2,
            claimed_by = NULL,
            locked_at = NULL,
            claim_expires_at = NULL,
            last_error = $3,
            metadata = (metadata - 'programming_extension_claim')
                || jsonb_build_object('programming_extension_result', $4::jsonb)
                || $5::jsonb,
            updated_at = now()
        WHERE id = $1
          AND status = 'processing'
          AND attempts = 1
          AND claimed_by = $6
        "#,
    )
    .bind(work_item_id)
    .bind(status)
    .bind(failure_code)
    .bind(&result_value)
    .bind(&metadata_additions)
    .bind(CLAIMED_BY)
    .execute(&mut *tx)
    .await
    .context("finish Space programming extension work item")?;
    if updated.rows_affected() != 1 {
        bail!("Space programming extension completion changed concurrently");
    }
    append_event(&mut tx, work_item_id, event_type, event_message, event_data).await?;
    tx.commit()
        .await
        .context("commit Space programming extension completion")?;
    Ok(json!({
        "schema_version": PROTOCOL_VERSION,
        "accepted": true,
        "status": status
    }))
}

async fn terminalize_expired_claims(tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    let rows = sqlx::query(
        r#"
        WITH stale AS (
            SELECT id
            FROM qintopia_agent_os.work_items
            WHERE work_item_type = $1
              AND target_agent = $2
              AND requester_agent = $3
              AND capability_key = $4
              AND status = 'processing'
              AND attempts = 1
              AND claimed_by = $5
              AND claim_expires_at <= now()
              AND metadata @> $6::jsonb
            ORDER BY claim_expires_at, id
            FOR UPDATE SKIP LOCKED
            LIMIT 32
        )
        UPDATE qintopia_agent_os.work_items item
        SET status = 'failed',
            claimed_by = NULL,
            locked_at = NULL,
            claim_expires_at = NULL,
            last_error = 'claim_expired_unknown',
            metadata = (metadata - 'programming_extension_claim')
                || jsonb_build_object(
                    'programming_extension_result',
                    jsonb_build_object(
                        'outcome', 'failed',
                        'failure_code', 'claim_expired_unknown',
                        'validation_status', 'failed'
                    )
                ),
            updated_at = now()
        FROM stale
        WHERE item.id = stale.id
        RETURNING item.id
        "#,
    )
    .bind(WORK_ITEM_TYPE)
    .bind(TARGET_AGENT)
    .bind(REQUESTER_AGENT)
    .bind(CAPABILITY_KEY)
    .bind(CLAIMED_BY)
    .bind(required_metadata())
    .fetch_all(&mut **tx)
    .await
    .context("terminalize expired Space programming extension claims")?;
    for row in rows {
        let work_item_id: Uuid = row.try_get("id")?;
        append_event(
            tx,
            work_item_id,
            "space_programming_extension_claim_expired",
            "Programming Agent claim expired with an unknown external outcome",
            json!({
                "failure_code": "claim_expired_unknown",
                "automatic_retry": false,
                "external_send_executed": false
            }),
        )
        .await?;
    }
    Ok(())
}

async fn terminalize_invalid_candidate(
    tx: &mut Transaction<'_, Postgres>,
    work_item_id: Uuid,
) -> Result<()> {
    let updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET status = 'failed',
            attempts = 1,
            last_error = 'invalid_request_contract',
            claimed_by = NULL,
            locked_at = NULL,
            claim_expires_at = NULL,
            metadata = metadata || jsonb_build_object(
                'programming_extension_result',
                jsonb_build_object(
                    'outcome', 'failed',
                    'failure_code', 'invalid_request_contract',
                    'validation_status', 'failed'
                )
            ),
            updated_at = now()
        WHERE id = $1
          AND status = 'queued'
          AND attempts = 0
        "#,
    )
    .bind(work_item_id)
    .execute(&mut **tx)
    .await
    .context("terminalize invalid Space programming extension candidate")?;
    if updated.rows_affected() != 1 {
        bail!("invalid Space programming extension candidate changed concurrently");
    }
    append_event(
        tx,
        work_item_id,
        "space_programming_extension_rejected",
        "Programming extension request failed the broker contract",
        json!({
            "failure_code": "invalid_request_contract",
            "automatic_retry": false,
            "external_send_executed": false
        }),
    )
    .await
}

async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    work_item_id: Uuid,
    event_type: &str,
    message: &str,
    data: Value,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_item_events
            (work_item_id, event_type, actor_type, actor_id, message, data)
        VALUES ($1, $2, 'agent', $3, $4, $5)
        "#,
    )
    .bind(work_item_id)
    .bind(event_type)
    .bind(CLAIMED_BY)
    .bind(message)
    .bind(data)
    .execute(&mut **tx)
    .await
    .context("append Space programming extension broker event")?;
    Ok(())
}

pub(crate) async fn reconcile_for_status(
    tx: &mut Transaction<'_, Postgres>,
    work_item_id: Uuid,
    space_id: Uuid,
) -> Result<()> {
    let deployed_release_sha = validated_deployed_commit_sha();
    reconcile_for_status_with_release(tx, work_item_id, space_id, deployed_release_sha.as_deref())
        .await
}

async fn reconcile_for_status_with_release(
    tx: &mut Transaction<'_, Postgres>,
    work_item_id: Uuid,
    space_id: Uuid,
    deployed_release_sha: Option<&str>,
) -> Result<()> {
    let Some(deployed_release_sha) = deployed_release_sha else {
        return Ok(());
    };
    validate_candidate_sha(deployed_release_sha)?;
    let row = sqlx::query(
        r#"
        SELECT status, payload, metadata
        FROM qintopia_agent_os.work_items
        WHERE id = $1
          AND space_id = $2
          AND work_item_type = $3
        FOR UPDATE
        "#,
    )
    .bind(work_item_id)
    .bind(space_id)
    .bind(WORK_ITEM_TYPE)
    .fetch_optional(&mut **tx)
    .await
    .context("lock Space programming extension continuation")?
    .context("Space programming extension continuation was not found")?;
    let status: String = row.try_get("status")?;
    if status != "awaiting_publish" {
        return Ok(());
    }
    let payload: Value = row.try_get("payload")?;
    let metadata: Value = row.try_get("metadata")?;
    let continuation_value = metadata
        .get("programming_extension_continuation")
        .cloned()
        .context("awaiting-publish programming extension continuation is missing")?;
    let mut continuation: ProgrammingExtensionContinuation =
        serde_json::from_value(continuation_value)
            .context("parse Space programming extension continuation")?;
    validate_continuation_binding(space_id, &payload, &continuation)?;
    if continuation.phase != "pr_created"
        || continuation.release_phase != "pending"
        || continuation.deployed_release_sha.is_some()
    {
        bail!("awaiting-publish programming extension continuation is invalid");
    }
    let Some(registered_sha256) = channel_event_mapping::registered_mapping_source_sha256(
        &continuation.provider,
        &continuation.mapping_key,
    )?
    else {
        return Ok(());
    };
    if registered_sha256 != continuation.mapping_sha256 {
        return Ok(());
    }

    continuation.phase = "ready_to_replan".to_string();
    continuation.release_phase = "released".to_string();
    continuation.deployed_release_sha = Some(deployed_release_sha.to_string());
    let continuation_value =
        serde_json::to_value(&continuation).context("serialize released continuation")?;
    let updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET status = 'completed',
            metadata = jsonb_set(
                metadata,
                '{programming_extension_continuation}',
                $3::jsonb,
                false
            ),
            updated_at = now()
        WHERE id = $1
          AND space_id = $2
          AND work_item_type = $4
          AND status = 'awaiting_publish'
        "#,
    )
    .bind(work_item_id)
    .bind(space_id)
    .bind(&continuation_value)
    .bind(WORK_ITEM_TYPE)
    .execute(&mut **tx)
    .await
    .context("mark Space programming extension ready to replan")?;
    if updated.rows_affected() != 1 {
        bail!("Space programming extension continuation changed concurrently");
    }
    let audit_data = json!({
        "mapping_provider": continuation.provider,
        "mapping_key": continuation.mapping_key,
        "mapping_digest_fingerprint": short_fingerprint(&continuation.mapping_sha256),
        "candidate_fingerprint": short_fingerprint(&continuation.candidate_sha),
        "deployed_release_fingerprint": short_fingerprint(deployed_release_sha),
        "same_space_required": true,
        "external_send_executed": false
    });
    append_event(
        tx,
        work_item_id,
        "space_programming_extension_released",
        "The exact mapping bundle is present in the active runtime release",
        json!({
            "phase": "released",
            "next_action": "same_space_shadow_replan",
            "evidence": audit_data.clone()
        }),
    )
    .await?;
    append_event(
        tx,
        work_item_id,
        "space_programming_extension_ready_to_replan",
        "The trusted same-Space wrapper may automatically prepare the released mapping in shadow",
        json!({
            "phase": "ready_to_replan",
            "next_action": "same_space_shadow_replan",
            "evidence": audit_data
        }),
    )
    .await
}

pub(crate) fn status_projection(work_item_status: &str, metadata: &Value) -> Result<Value> {
    let Some(value) = metadata.get("programming_extension_continuation") else {
        let phase = match work_item_status {
            "queued" => "queued",
            "processing" => "implementing",
            "failed" => "failed",
            _ => "unknown",
        };
        return Ok(json!({
            "phase": phase,
            "release_phase": "not_started",
            "same_space_required": true
        }));
    };
    let continuation: ProgrammingExtensionContinuation = serde_json::from_value(value.clone())
        .context("parse Space programming extension status continuation")?;
    validate_continuation_shape(&continuation)?;
    let next_action = if continuation.phase == "ready_to_replan" {
        "The trusted same-Space status wrapper will replan the retained intent and prepare an idempotent shadow proposal; administrator confirmation remains required."
    } else {
        "Wait for the exact mapping bundle to appear in the active runtime release, then check status again."
    };
    Ok(json!({
        "phase": continuation.phase,
        "release_phase": continuation.release_phase,
        "pr": {
            "number": continuation.pr_number,
            "candidate_fingerprint": short_fingerprint(&continuation.candidate_sha)
        },
        "mapping": {
            "provider": continuation.provider,
            "definition_key": continuation.mapping_key,
            "digest_fingerprint": short_fingerprint(&continuation.mapping_sha256)
        },
        "active_release_fingerprint": continuation
            .deployed_release_sha
            .as_deref()
            .map(short_fingerprint),
        "same_space_required": true,
        "next_action": next_action
    }))
}

pub(crate) fn ready_continuation(
    space_id: Uuid,
    work_item_status: &str,
    payload: &Value,
    metadata: &Value,
) -> Result<ReadyContinuation> {
    if work_item_status != "completed" {
        bail!("Space programming extension is not ready for shadow planning");
    }
    let continuation_value = metadata
        .get("programming_extension_continuation")
        .cloned()
        .context("Space programming extension continuation is missing")?;
    let continuation: ProgrammingExtensionContinuation = serde_json::from_value(continuation_value)
        .context("parse Space programming extension continuation")?;
    validate_continuation_binding(space_id, payload, &continuation)?;
    if continuation.phase != "ready_to_replan" || continuation.release_phase != "released" {
        bail!("Space programming extension is not ready for shadow planning");
    }
    let registered_sha256 = channel_event_mapping::registered_mapping_source_sha256(
        &continuation.provider,
        &continuation.mapping_key,
    )?
    .context("released Space programming extension mapping is not registered")?;
    if registered_sha256 != continuation.mapping_sha256 {
        bail!("released Space programming extension mapping digest changed");
    }
    let request = validate_payload(payload.clone())?;
    Ok(ReadyContinuation {
        intent: request.intent,
        provider: continuation.provider,
        mapping_key: continuation.mapping_key,
    })
}

fn new_continuation(
    space_id: Uuid,
    request: &ProgrammingExtensionRequest,
    request_sha256: &str,
    pr_number: u64,
    candidate_sha: &str,
    mapping_key: &str,
    mapping_sha256: &str,
) -> ProgrammingExtensionContinuation {
    ProgrammingExtensionContinuation {
        schema_version: CONTINUATION_SCHEMA_VERSION,
        phase: "pr_created".to_string(),
        release_phase: "pending".to_string(),
        provider: request.provider.clone(),
        mapping_key: mapping_key.to_string(),
        mapping_sha256: mapping_sha256.to_string(),
        candidate_sha: candidate_sha.to_string(),
        pr_number,
        request_sha256: request_sha256.to_string(),
        binding_sha256: continuation_binding_sha256(
            space_id,
            request_sha256,
            &request.provider,
            mapping_key,
            mapping_sha256,
            candidate_sha,
        ),
        deployed_release_sha: None,
    }
}

fn validate_continuation_binding(
    space_id: Uuid,
    payload: &Value,
    continuation: &ProgrammingExtensionContinuation,
) -> Result<()> {
    let request = validate_payload(payload.clone())?;
    let request_sha256 =
        sha256_hex(&serde_json::to_vec(payload).context("encode continuation request binding")?);
    validate_continuation_shape(continuation)?;
    if continuation.provider != request.provider
        || continuation.request_sha256 != request_sha256
        || continuation.binding_sha256
            != continuation_binding_sha256(
                space_id,
                &request_sha256,
                &request.provider,
                &continuation.mapping_key,
                &continuation.mapping_sha256,
                &continuation.candidate_sha,
            )
    {
        bail!("Space programming extension continuation binding changed");
    }
    Ok(())
}

fn validate_continuation_shape(continuation: &ProgrammingExtensionContinuation) -> Result<()> {
    if continuation.schema_version != CONTINUATION_SCHEMA_VERSION
        || continuation.pr_number == 0
        || continuation.provider != "qiwe"
        || !valid_mapping_key(&continuation.mapping_key)
        || !valid_sha256(&continuation.mapping_sha256)
        || !valid_sha256(&continuation.request_sha256)
        || !valid_sha256(&continuation.binding_sha256)
        || validate_candidate_sha(&continuation.candidate_sha).is_err()
        || continuation
            .deployed_release_sha
            .as_deref()
            .is_some_and(|sha| validate_candidate_sha(sha).is_err())
        || !matches!(
            continuation.phase.as_str(),
            "pr_created" | "ready_to_replan"
        )
        || !matches!(continuation.release_phase.as_str(), "pending" | "released")
        || (continuation.phase == "pr_created"
            && (continuation.release_phase != "pending"
                || continuation.deployed_release_sha.is_some()))
        || (continuation.phase == "ready_to_replan"
            && (continuation.release_phase != "released"
                || continuation.deployed_release_sha.is_none()))
    {
        bail!("Space programming extension continuation is invalid");
    }
    Ok(())
}

fn continuation_binding_sha256(
    space_id: Uuid,
    request_sha256: &str,
    provider: &str,
    mapping_key: &str,
    mapping_sha256: &str,
    candidate_sha: &str,
) -> String {
    let mut digest = Sha256::new();
    digest.update(CONTINUATION_DIGEST_DOMAIN);
    let space_id = space_id.to_string();
    for value in [
        space_id.as_str(),
        request_sha256,
        provider,
        mapping_key,
        mapping_sha256,
        candidate_sha,
    ] {
        digest.update(value.as_bytes());
        digest.update([0]);
    }
    format!("{:x}", digest.finalize())
}

fn validated_deployed_commit_sha() -> Option<String> {
    let value = env::var(DEPLOYED_COMMIT_SHA_ENV).ok()?;
    validate_candidate_sha(&value).ok()?;
    Some(value)
}

fn valid_mapping_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._:-".contains(&byte)
        })
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn short_fingerprint(value: &str) -> &str {
    value.get(..12).unwrap_or("invalid")
}

fn validate_payload(value: Value) -> Result<ProgrammingExtensionRequest> {
    let request: ProgrammingExtensionRequest =
        serde_json::from_value(value).context("parse programming extension payload")?;
    validate_bounded_text("intent", &request.intent, 4_000, true)?;
    validate_bounded_text("research_query", &request.research_query, 500, true)?;
    if request.provider != "qiwe" {
        bail!("programming extension provider is not allowed");
    }
    if request.research_digest.len() != 64
        || !request
            .research_digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("programming extension research digest is invalid");
    }
    validate_programming_research_evidence(
        &request.provider,
        &request.official_sources,
        &request.research_evidence,
        &request.research_digest,
    )?;
    Ok(request)
}

fn validate_bounded_text(
    name: &str,
    value: &str,
    max_chars: usize,
    reject_urls: bool,
) -> Result<()> {
    if value.is_empty()
        || value.chars().count() > max_chars
        || value.chars().any(char::is_control)
        || (reject_urls
            && value
                .to_ascii_lowercase()
                .split_whitespace()
                .any(|part| part.starts_with("http://") || part.starts_with("https://")))
    {
        bail!("programming extension {name} is invalid");
    }
    Ok(())
}

fn validate_runner_result(result: &RunnerResult) -> Result<()> {
    match result {
        RunnerResult::Succeeded {
            pr_url,
            pr_number,
            candidate_sha,
            mapping_key,
            mapping_sha256,
            validation_status,
        } => {
            if *pr_number == 0
                || validation_status != "passed"
                || !valid_mapping_key(mapping_key)
                || !valid_sha256(mapping_sha256)
            {
                bail!("successful programming extension result is invalid");
            }
            validate_candidate_sha(candidate_sha)?;
            validate_pr_url(pr_url, *pr_number)?;
        }
        RunnerResult::Failed {
            failure_code,
            validation_status,
        } => {
            const ALLOWED_FAILURE_CODES: [&str; 9] = [
                "agent_failed",
                "configuration_error",
                "pr_create_ambiguous",
                "pr_create_failed",
                "repository_state_changed",
                "tool_unavailable",
                "unsafe_diff",
                "validation_failed",
                "worktree_failed",
            ];
            if validation_status != "failed"
                || !ALLOWED_FAILURE_CODES.contains(&failure_code.as_str())
            {
                bail!("failed programming extension result is invalid");
            }
        }
    }
    Ok(())
}

fn validate_candidate_sha(value: &str) -> Result<()> {
    if value.len() != 40
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("programming extension candidate SHA is invalid");
    }
    Ok(())
}

fn validate_pr_url(value: &str, pr_number: u64) -> Result<()> {
    let url = Url::parse(value).context("programming extension PR URL is invalid")?;
    let expected_path = format!("/qintopia-agent-studio/qintopia-agent-os/pull/{pr_number}");
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != expected_path
    {
        bail!("programming extension PR URL is outside the registered repository");
    }
    Ok(())
}

fn validate_claim_token(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("Space programming extension claim token is invalid");
    }
    Ok(())
}

fn new_claim_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn sha256_hex(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in left.iter().zip(right) {
        difference |= left ^ right;
    }
    difference == 0
}

fn required_metadata() -> Value {
    json!({
        "extension_scope": PROGRAMMING_EXTENSION_SCOPE,
        "allowed_change_class": PROGRAMMING_EXTENSION_ALLOWED_CHANGE_CLASS,
        "temporary_worktree_required": true,
        "production_credentials_allowed": false,
        "external_send_executed": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_payload() -> Value {
        let research_evidence = vec![ProgrammingExtensionResearchEvidence {
            url: "https://doc.qiweapi.com/doc-7331304".to_string(),
            text: "msgType=1002 identifies one bounded QiWe membership event.".to_string(),
        }];
        json!({
            "intent": "Add a bounded QiWe membership event mapping.",
            "provider": "qiwe",
            "research_query": "QiWe group membership callback",
            "official_sources": ["https://doc.qiweapi.com/doc-7331304"],
            "research_digest": crate::space_configuration::programming_research_digest(
                &research_evidence
            ),
            "research_evidence": research_evidence
        })
    }

    #[test]
    fn dispatch_enablement_is_strict_and_default_off() {
        assert!(!DispatchConfig::from_value(None).unwrap().enabled);
        assert!(
            !DispatchConfig::from_value(Some(OsStr::new("")))
                .unwrap()
                .enabled
        );
        assert!(
            !DispatchConfig::from_value(Some(OsStr::new("0")))
                .unwrap()
                .enabled
        );
        assert!(
            DispatchConfig::from_value(Some(OsStr::new("1")))
                .unwrap()
                .enabled
        );
        for invalid in ["true", "yes", "01", " 1"] {
            assert!(DispatchConfig::from_value(Some(OsStr::new(invalid))).is_err());
        }
    }

    #[test]
    fn broker_wire_contract_rejects_unknown_fields() {
        let request: BrokerRequest = serde_json::from_value(json!({
            "operation": "space_programming_extension_claim",
            "schema_version": 1
        }))
        .unwrap();
        assert!(matches!(
            request,
            BrokerRequest::SpaceProgrammingExtensionClaim { .. }
        ));
        assert!(serde_json::from_value::<BrokerRequest>(json!({
            "operation": "space_programming_extension_claim",
            "schema_version": 1,
            "space_id": Uuid::new_v4()
        }))
        .is_err());
    }

    #[test]
    fn claim_payload_contains_only_registered_official_sources() {
        assert!(validate_payload(valid_payload()).is_ok());
        let mut arbitrary_url = valid_payload();
        arbitrary_url["official_sources"] = json!(["https://example.com/docs"]);
        assert!(validate_payload(arbitrary_url).is_err());
        let mut extra_evidence_identity = valid_payload();
        extra_evidence_identity["research_evidence"][0]["space_id"] = json!(Uuid::new_v4());
        assert!(validate_payload(extra_evidence_identity).is_err());
        let mut tampered_evidence = valid_payload();
        tampered_evidence["research_evidence"][0]["text"] = json!("changed without rebinding");
        assert!(validate_payload(tampered_evidence).is_err());
        let mut leaked_room = valid_payload();
        leaked_room["space_id"] = json!(Uuid::new_v4());
        assert!(validate_payload(leaked_room).is_err());
    }

    #[test]
    fn completion_result_is_pr_only_and_repository_bound() {
        let success = RunnerResult::Succeeded {
            pr_url: "https://github.com/qintopia-agent-studio/qintopia-agent-os/pull/123"
                .to_string(),
            pr_number: 123,
            candidate_sha: "a".repeat(40),
            mapping_key: "runner_probe_v1".to_string(),
            mapping_sha256: "b".repeat(64),
            validation_status: "passed".to_string(),
        };
        assert!(validate_runner_result(&success).is_ok());
        let wrong_repo = RunnerResult::Succeeded {
            pr_url: "https://github.com/other/repository/pull/123".to_string(),
            pr_number: 123,
            candidate_sha: "a".repeat(40),
            mapping_key: "runner_probe_v1".to_string(),
            mapping_sha256: "b".repeat(64),
            validation_status: "passed".to_string(),
        };
        assert!(validate_runner_result(&wrong_repo).is_err());
        let forbidden_failure = RunnerResult::Failed {
            failure_code: "deploy_failed".to_string(),
            validation_status: "failed".to_string(),
        };
        assert!(validate_runner_result(&forbidden_failure).is_err());
    }

    #[test]
    fn continuation_projection_is_bound_and_exposes_only_fingerprints() {
        let payload = valid_payload();
        let request = validate_payload(payload.clone()).unwrap();
        let request_sha256 = sha256_hex(&serde_json::to_vec(&payload).unwrap());
        let space_id = Uuid::new_v4();
        let candidate_sha = "a".repeat(40);
        let mapping_sha256 = "b".repeat(64);
        let continuation = new_continuation(
            space_id,
            &request,
            &request_sha256,
            123,
            &candidate_sha,
            "runner_probe_v1",
            &mapping_sha256,
        );
        validate_continuation_binding(space_id, &payload, &continuation).unwrap();
        assert!(validate_continuation_binding(Uuid::new_v4(), &payload, &continuation).is_err());

        let metadata = json!({"programming_extension_continuation": continuation});
        let projection = status_projection("awaiting_publish", &metadata).unwrap();
        assert_eq!(projection["phase"], "pr_created");
        assert_eq!(projection["release_phase"], "pending");
        assert_eq!(projection["pr"]["number"], 123);
        assert_eq!(projection["pr"]["candidate_fingerprint"], "aaaaaaaaaaaa");
        assert_eq!(projection["mapping"]["digest_fingerprint"], "bbbbbbbbbbbb");
        let serialized = serde_json::to_string(&projection).unwrap();
        assert!(!serialized.contains(&candidate_sha));
        assert!(!serialized.contains(&mapping_sha256));
        assert!(!serialized.contains("github.com"));
        assert!(!serialized.contains("pull/123"));
    }

    #[test]
    fn released_projection_requires_a_valid_deployed_release_binding() {
        let payload = valid_payload();
        let request = validate_payload(payload.clone()).unwrap();
        let request_sha256 = sha256_hex(&serde_json::to_vec(&payload).unwrap());
        let mut continuation = new_continuation(
            Uuid::new_v4(),
            &request,
            &request_sha256,
            7,
            &"a".repeat(40),
            "runner_probe_v1",
            &"b".repeat(64),
        );
        continuation.phase = "ready_to_replan".to_string();
        continuation.release_phase = "released".to_string();
        assert!(validate_continuation_shape(&continuation).is_err());
        continuation.deployed_release_sha = Some("c".repeat(40));
        assert!(validate_continuation_shape(&continuation).is_ok());

        let projection = status_projection(
            "completed",
            &json!({"programming_extension_continuation": continuation}),
        )
        .unwrap();
        assert_eq!(projection["phase"], "ready_to_replan");
        assert_eq!(projection["release_phase"], "released");
        assert_eq!(projection["active_release_fingerprint"], "cccccccccccc");
        assert_eq!(projection["same_space_required"], true);
        assert!(projection["next_action"]
            .as_str()
            .is_some_and(|value| value.contains("idempotent shadow proposal")));
        assert!(!projection["next_action"]
            .as_str()
            .is_some_and(|value| value.contains("Call qintopia_space_change_prepare again")));
    }

    #[test]
    fn ready_continuation_revalidates_payload_space_and_registered_mapping() {
        let payload = valid_payload();
        let request = validate_payload(payload.clone()).unwrap();
        let request_sha256 = sha256_hex(&serde_json::to_vec(&payload).unwrap());
        let space_id = Uuid::new_v4();
        let mapping_sha256 =
            channel_event_mapping::registered_mapping_source_sha256("qiwe", "group_member_add")
                .unwrap()
                .expect("registered member-add mapping");
        let mut continuation = new_continuation(
            space_id,
            &request,
            &request_sha256,
            7,
            &"a".repeat(40),
            "group_member_add",
            &mapping_sha256,
        );
        continuation.phase = "ready_to_replan".to_string();
        continuation.release_phase = "released".to_string();
        continuation.deployed_release_sha = Some("c".repeat(40));
        let metadata = json!({"programming_extension_continuation": continuation});

        let ready = ready_continuation(space_id, "completed", &payload, &metadata).unwrap();
        assert_eq!(ready.intent, request.intent);
        assert_eq!(ready.provider, "qiwe");
        assert_eq!(ready.mapping_key, "group_member_add");
        assert!(ready_continuation(space_id, "awaiting_publish", &payload, &metadata).is_err());
        assert!(ready_continuation(Uuid::new_v4(), "completed", &payload, &metadata).is_err());
    }

    #[test]
    fn claim_tokens_are_opaque_and_compared_without_early_exit() {
        let token = new_claim_token();
        assert!(validate_claim_token(&token).is_ok());
        assert!(constant_time_eq(token.as_bytes(), token.as_bytes()));
        assert!(!constant_time_eq(
            token.as_bytes(),
            "a".repeat(64).as_bytes()
        ));
        assert!(!constant_time_eq(token.as_bytes(), b"short"));
    }

    #[tokio::test]
    #[cfg(feature = "postgres-integration-tests")]
    #[ignore = "requires guarded disposable PostgreSQL qintopia_test"]
    async fn postgres_claim_is_sanitized_single_attempt_and_lease_bound() {
        let database_url = env::var("QINTOPIA_SIDECAR_DATABASE_URL")
            .expect("QINTOPIA_SIDECAR_DATABASE_URL is required");
        let parsed = Url::parse(&database_url).expect("parse integration database URL");
        assert!(matches!(
            parsed.host_str(),
            Some("127.0.0.1") | Some("localhost")
        ));
        assert_eq!(parsed.path(), "/qintopia_test");
        let pool = crate::db::connect(&database_url, 2)
            .await
            .expect("connect programming extension integration database");
        crate::db::run_migrations(&pool)
            .await
            .expect("migrate programming extension integration database");

        let suffix = Uuid::new_v4().simple().to_string();
        let conversation_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO qintopia_messages.conversations
                (tenant_id, platform, chat_id, chat_type, status)
            VALUES ('qintopia', 'qiwe', $1, 'group', 'active')
            RETURNING id
            "#,
        )
        .bind(format!("programming-extension-{suffix}"))
        .fetch_one(&pool)
        .await
        .expect("seed programming extension Space");
        sqlx::query(
            "UPDATE qintopia_agent_os.capabilities SET enabled=true WHERE capability_key=$1",
        )
        .bind(CAPABILITY_KEY)
        .execute(&pool)
        .await
        .expect("enable test capability");
        let work_item_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO qintopia_agent_os.work_items
                (space_id, work_item_type, status, requester_agent, target_agent,
                 capability_key, brief_summary, dedupe_key, idempotency_key,
                 information_class, payload, payload_redaction_policy, review_policy,
                 metadata)
            VALUES
                ($1, $2, 'queued', $3, $4, $5, 'bounded extension', $6, $6,
                 'internal_ops', $7, 'summary_only', 'low_risk_classifier', $8)
            RETURNING id
            "#,
        )
        .bind(conversation_id)
        .bind(WORK_ITEM_TYPE)
        .bind(REQUESTER_AGENT)
        .bind(TARGET_AGENT)
        .bind(CAPABILITY_KEY)
        .bind(format!("programming-extension-integration-{suffix}"))
        .bind(valid_payload())
        .bind(required_metadata())
        .fetch_one(&pool)
        .await
        .expect("seed programming extension work item");
        let mut stale_metadata = required_metadata();
        stale_metadata["programming_extension_claim"] = json!({
            "schema_version": 1,
            "token_sha256": "e".repeat(64)
        });
        let stale_work_item_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO qintopia_agent_os.work_items
                (space_id, work_item_type, status, requester_agent, target_agent,
                 capability_key, brief_summary, dedupe_key, idempotency_key,
                 information_class, payload, payload_redaction_policy, review_policy,
                 metadata, attempts, claimed_by, locked_at, claim_expires_at)
            VALUES
                ($1, $2, 'processing', $3, $4, $5, 'stale extension', $6, $6,
                 'internal_ops', $7, 'summary_only', 'low_risk_classifier', $8,
                 1, $9, now() - interval '1 hour', now() - interval '1 minute')
            RETURNING id
            "#,
        )
        .bind(conversation_id)
        .bind(WORK_ITEM_TYPE)
        .bind(REQUESTER_AGENT)
        .bind(TARGET_AGENT)
        .bind(CAPABILITY_KEY)
        .bind(format!("programming-extension-stale-{suffix}"))
        .bind(valid_payload())
        .bind(stale_metadata)
        .bind(CLAIMED_BY)
        .fetch_one(&pool)
        .await
        .expect("seed stale programming extension work item");

        let claimed = handle(
            &pool,
            DispatchConfig { enabled: true },
            BrokerRequest::SpaceProgrammingExtensionClaim { schema_version: 1 },
        )
        .await
        .expect("claim programming extension");
        assert_eq!(claimed["claimed"], true);
        assert_eq!(claimed["work_item_id"], work_item_id.to_string());
        assert!(claimed.get("space_id").is_none());
        assert!(claimed.get("actor_id").is_none());
        let stale_state: (String, Option<String>, i32) = sqlx::query_as(
            "SELECT status, last_error, attempts FROM qintopia_agent_os.work_items WHERE id=$1",
        )
        .bind(stale_work_item_id)
        .fetch_one(&pool)
        .await
        .expect("load stale programming extension terminal state");
        assert_eq!(stale_state.0, "failed");
        assert_eq!(stale_state.1.as_deref(), Some("claim_expired_unknown"));
        assert_eq!(stale_state.2, 1);
        let claim_token = claimed["claim_token"].as_str().unwrap();
        sqlx::query(
            "UPDATE qintopia_agent_os.capabilities SET enabled = false, updated_at = now() WHERE capability_key = $1",
        )
        .bind(CAPABILITY_KEY)
        .execute(&pool)
        .await
        .expect("revoke programming extension capability before finish");
        assert!(finish(
            &pool,
            work_item_id,
            claim_token,
            RunnerResult::Failed {
                failure_code: "agent_failed".to_string(),
                validation_status: "failed".to_string(),
            },
        )
        .await
        .is_err());
        sqlx::query(
            "UPDATE qintopia_agent_os.capabilities SET enabled = true, updated_at = now() WHERE capability_key = $1",
        )
        .bind(CAPABILITY_KEY)
        .execute(&pool)
        .await
        .expect("restore programming extension capability before finish");
        assert!(finish(
            &pool,
            work_item_id,
            &"b".repeat(64),
            RunnerResult::Failed {
                failure_code: "agent_failed".to_string(),
                validation_status: "failed".to_string(),
            },
        )
        .await
        .is_err());
        let completed = finish(
            &pool,
            work_item_id,
            claim_token,
            RunnerResult::Succeeded {
                pr_url: "https://github.com/qintopia-agent-studio/qintopia-agent-os/pull/123"
                    .to_string(),
                pr_number: 123,
                candidate_sha: "a".repeat(40),
                mapping_key: "group_member_add".to_string(),
                mapping_sha256: channel_event_mapping::registered_mapping_source_sha256(
                    "qiwe",
                    "group_member_add",
                )
                .expect("load registered mapping digest")
                .expect("registered group member mapping"),
                validation_status: "passed".to_string(),
            },
        )
        .await
        .expect("finish programming extension");
        assert_eq!(completed["status"], "awaiting_publish");
        let mut continuation_tx = pool
            .begin()
            .await
            .expect("begin programming extension continuation");
        let deployed_release_sha = "c".repeat(40);
        reconcile_for_status_with_release(
            &mut continuation_tx,
            work_item_id,
            conversation_id,
            Some(&deployed_release_sha),
        )
        .await
        .expect("observe exact mapping in active release");
        continuation_tx
            .commit()
            .await
            .expect("commit programming extension continuation");
        let released: (String, Value) =
            sqlx::query_as("SELECT status, metadata FROM qintopia_agent_os.work_items WHERE id=$1")
                .bind(work_item_id)
                .fetch_one(&pool)
                .await
                .expect("load released programming extension");
        assert_eq!(released.0, "completed");
        assert_eq!(
            released.1["programming_extension_continuation"]["phase"],
            "ready_to_replan"
        );
        assert_eq!(
            released.1["programming_extension_continuation"]["release_phase"],
            "released"
        );
        assert!(finish(
            &pool,
            work_item_id,
            claim_token,
            RunnerResult::Failed {
                failure_code: "agent_failed".to_string(),
                validation_status: "failed".to_string(),
            },
        )
        .await
        .is_err());
    }
}
