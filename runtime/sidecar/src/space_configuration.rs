use std::{collections::HashMap, str::FromStr};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Postgres, Row, Transaction};
use url::Url;
use uuid::Uuid;

const CAPABILITY_KEY: &str = "erhua.manage_space_configuration";
const EXECUTION_CAPABILITY_KEY: &str = "erhua.execute_space_business";
const AGENT_TURN_CAPABILITY_KEY: &str = "erhua.space_agent_turn";
const WORK_ITEM_TYPE: &str = "space_change_request";
const PROGRAMMING_EXTENSION_WORK_ITEM_TYPE: &str = "space_programming_extension_request";
pub(crate) const PROGRAMMING_EXTENSION_SCOPE: &str =
    "provider_event_mapping_with_optional_restricted_parser_recipe";
pub(crate) const PROGRAMMING_EXTENSION_ALLOWED_CHANGE_CLASS: &str =
    "low_risk_declarative_mapping_bundle_only";
const SPACE_AUTOMATION_WORK_ITEM_TYPE: &str = "space_automation_run";
const SPACE_AGENT_TURN_WORK_ITEM_TYPE: &str = "space_agent_turn";
const ARTIFACT_TYPE: &str = "space_change_proposal";
const PROPOSAL_SCHEMA: &str = "space-change-proposal-v1";
const CONFIRMATION_TTL_MINUTES: i64 = 10;
const MAX_CONFIRMATION_ATTEMPTS: u64 = 5;
const MAX_CHANGES: usize = 8;
const MAX_INTENT_BYTES: usize = 48 * 1024;
const MAX_JSON_DEPTH: usize = 8;
const MAX_JSON_COLLECTION_ITEMS: usize = 64;
const MAX_PROGRAMMING_RESEARCH_EVIDENCE: usize = 4;
const MAX_PROGRAMMING_RESEARCH_TEXT_BYTES: usize = 8 * 1024;
const MAX_PROGRAMMING_RESEARCH_TOTAL_BYTES: usize = 24 * 1024;
const PROGRAMMING_RESEARCH_DIGEST_DOMAIN: &[u8] = b"qintopia-qiwe-research-evidence-v1\0";
const AUTOMATION_ROLLBACK_LINEAGE_EVENT: &str = "automation_rollback_lineage_recorded";
const AUTOMATION_ROLLBACK_LINEAGE_SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone)]
pub(crate) struct TrustedSpaceSession {
    pub(crate) platform: String,
    pub(crate) conversation_type: String,
    pub(crate) conversation_id: String,
    pub(crate) requester_user_id: String,
    pub(crate) source_message_id: String,
    pub(crate) source_message_text: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProgrammingExtensionResearchEvidence {
    pub(crate) url: String,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProgrammingExtensionRequest {
    pub(crate) intent: String,
    pub(crate) provider: String,
    pub(crate) research_query: String,
    #[serde(default)]
    pub(crate) official_sources: Vec<String>,
    #[serde(default)]
    pub(crate) research_evidence: Vec<ProgrammingExtensionResearchEvidence>,
    pub(crate) research_digest: String,
}

#[derive(Debug, Clone)]
struct ResolvedContext {
    space_id: Uuid,
    space_display_name: Option<String>,
    actor_person_id: Uuid,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SpaceChangeIntent {
    summary: String,
    changes: Vec<SpaceChange>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct DefinitionBinding {
    source: String,
    definition_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stream_head_version: Option<i32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ActivationSourceBinding {
    id: Uuid,
    definition_digest: String,
    stream_head_version: i32,
    status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct AutomationActivationBinding {
    automation: ActivationSourceBinding,
    business_definition: ActivationSourceBinding,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    event_mapping: Option<ActivationSourceBinding>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ActivationEventMappingReview {
    provider: String,
    definition_key: String,
    source_status: String,
    version: i32,
    fingerprint: String,
    event_type: String,
    selector: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct AutomationActivationReview {
    result_status: String,
    automation_version: i32,
    automation_fingerprint: String,
    trigger_kind: String,
    trigger_config: Value,
    timezone: String,
    misfire_policy: String,
    business_definition_key: String,
    business_source_status: String,
    business_version: i32,
    business_fingerprint: String,
    execution_mode: String,
    business_definition: Value,
    allowed_capabilities: Vec<String>,
    approval_policy: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    event_mapping: Option<ActivationEventMappingReview>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "resource", rename_all = "snake_case")]
#[expect(
    clippy::large_enum_variant,
    reason = "versioned declarative resources intentionally retain explicit typed fields"
)]
enum SpaceChange {
    SpacePolicy {
        definition_key: String,
        status: String,
        policy_config: Value,
    },
    BusinessDefinition {
        definition_key: String,
        status: String,
        execution_mode: String,
        definition: Value,
        #[serde(default)]
        allowed_capabilities: Vec<String>,
        #[serde(default = "default_business_approval_policy")]
        approval_policy: String,
    },
    AutomationDefinition {
        definition_key: String,
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_stream_head_version: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_definition_version: Option<i32>,
        business_definition_key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        business_definition_binding: Option<DefinitionBinding>,
        trigger_kind: String,
        trigger_config: Value,
        #[serde(default = "default_timezone")]
        timezone: String,
        #[serde(default = "default_misfire_policy")]
        misfire_policy: String,
        #[serde(default)]
        event_mapping_provider: Option<String>,
        #[serde(default)]
        event_mapping_key: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        event_mapping_binding: Option<DefinitionBinding>,
    },
    DefinitionOperation {
        target_resource: String,
        definition_key: String,
        operation: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activation_binding: Option<AutomationActivationBinding>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activation_review: Option<AutomationActivationReview>,
    },
    ChannelEventMapping {
        provider: String,
        definition_key: String,
        status: String,
        selector: Value,
        extractor: Value,
        #[serde(default)]
        official_sources: Vec<String>,
        #[serde(default)]
        validation_evidence: Value,
    },
}

#[derive(Debug, Clone, Serialize)]
struct AppliedDefinition {
    resource: &'static str,
    definition_key: String,
    version: i32,
    status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Authorization {
    SpaceAdmin,
    BootstrapGlobalAdmin,
}

fn default_business_approval_policy() -> String {
    "space_admin_confirmation".to_string()
}

fn default_timezone() -> String {
    "Asia/Shanghai".to_string()
}

fn default_misfire_policy() -> String {
    "run_once".to_string()
}

pub(crate) async fn prepare(
    pool: &PgPool,
    session: TrustedSpaceSession,
    intent_value: Value,
) -> Result<Value> {
    prepare_internal(pool, session, intent_value, None).await
}

pub(crate) async fn prepare_programming_extension_shadow(
    pool: &PgPool,
    session: TrustedSpaceSession,
    request_id: Uuid,
    intent_value: Value,
) -> Result<Value> {
    prepare_internal(pool, session, intent_value, Some(request_id)).await
}

async fn prepare_internal(
    pool: &PgPool,
    session: TrustedSpaceSession,
    intent_value: Value,
    programming_extension_request_id: Option<Uuid>,
) -> Result<Value> {
    validate_session(&session)?;
    let mut intent = parse_and_validate_intent(intent_value)?;

    let mut tx = pool
        .begin()
        .await
        .context("begin Space change prepare transaction")?;
    let context = resolve_context(&mut tx, &session).await?;
    if let Some(request_id) = programming_extension_request_id {
        crate::space_programming_extension::reconcile_for_status(
            &mut tx,
            request_id,
            context.space_id,
        )
        .await?;
        let continuation = load_ready_programming_extension(&mut tx, &context, request_id).await?;
        validate_programming_extension_shadow_intent(&intent, &continuation)?;
    }
    materialize_trusted_intent(&mut tx, context.space_id, &mut intent).await?;
    let canonical_intent =
        serde_json::to_value(&intent).context("serialize Space change intent")?;
    let canonical_bytes =
        serde_json::to_vec(&canonical_intent).context("encode Space change intent")?;
    if canonical_bytes.len() > MAX_INTENT_BYTES {
        bail!("Space change intent is too large");
    }
    let proposal_digest = sha256_hex(&canonical_bytes);
    validate_capability_references(&mut tx, context.space_id, &intent).await?;
    let authorized_confirmers = authorized_confirmer_ids(
        &mut tx,
        context.space_id,
        intent.protects_provider_mapping(),
    )
    .await?;
    if authorized_confirmers.is_empty() {
        bail!("current Space has no authorized configuration administrator");
    }

    let source_message_ref = session_source_message_ref(&session);
    let (idempotency_key, source_refs) = if let Some(request_id) = programming_extension_request_id
    {
        (
            format!("space-change-extension:{}:{request_id}", context.space_id),
            json!({
                "source_message_ref": source_message_ref,
                "programming_extension_request_id": request_id
            }),
        )
    } else {
        (
            format!(
                "space-change:{}:{}",
                context.space_id,
                source_message_ref.trim_start_matches("sha256:")
            ),
            json!({"source_message_ref": source_message_ref}),
        )
    };
    if let Some(existing) = load_existing_proposal(
        &mut tx,
        context.space_id,
        &idempotency_key,
        &proposal_digest,
        context.actor_person_id,
    )
    .await?
    {
        let response = reissue_existing_confirmation(
            &mut tx,
            &context,
            &intent,
            &proposal_digest,
            &authorized_confirmers,
            existing,
        )
        .await?;
        tx.commit()
            .await
            .context("commit idempotent Space change prepare")?;
        return Ok(decorate_continuation_response(
            response,
            programming_extension_request_id,
        ));
    }

    let capability = sqlx::query(
        r#"
        SELECT risk_level, review_policy
        FROM qintopia_agent_os.capabilities
        WHERE capability_key = $1
          AND enabled = true
          AND 'erhua' = ANY(allowed_callers)
          AND $2 = ANY(allowed_work_item_types)
        "#,
    )
    .bind(CAPABILITY_KEY)
    .bind(WORK_ITEM_TYPE)
    .fetch_optional(&mut *tx)
    .await
    .context("load Space configuration capability")?
    .context("Space configuration capability is not enabled")?;
    let risk_level: String = capability.try_get("risk_level")?;
    let review_policy: String = capability.try_get("review_policy")?;

    let work_item_id = Uuid::new_v4();
    let dedupe_key = format!("space-change:{}:{proposal_digest}", context.space_id);
    let mut work_item_metadata = json!({
        "proposal_schema": PROPOSAL_SCHEMA,
        "external_send_executed": false,
        "trusted_session_required": true
    });
    if let Some(request_id) = programming_extension_request_id {
        work_item_metadata["programming_extension_request_id"] = json!(request_id);
    }
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_items
            (id, space_id, work_item_type, status, requester_agent, target_agent,
             capability_key, human_owner, priority, brief_summary, purpose,
             source_type, source_refs, dedupe_key, idempotency_key, risk_level,
             information_class, payload, payload_redaction_policy, review_policy,
             metadata)
        VALUES
            ($1, $2, $3, 'awaiting_review', 'erhua', 'erhua', $4, $5,
             'normal', $6, 'configure current Space from trusted conversation',
             'trusted_conversation', $7, $8, $9, $10, 'internal_ops', $11,
             'summary_only', $12, $13)
        "#,
    )
    .bind(work_item_id)
    .bind(context.space_id)
    .bind(WORK_ITEM_TYPE)
    .bind(CAPABILITY_KEY)
    .bind(context.actor_person_id.to_string())
    .bind(&intent.summary)
    .bind(source_refs)
    .bind(&dedupe_key)
    .bind(&idempotency_key)
    .bind(&risk_level)
    .bind(json!({
        "proposal_digest": proposal_digest,
        "change_count": intent.changes.len()
    }))
    .bind(&review_policy)
    .bind(work_item_metadata)
    .execute(&mut *tx)
    .await
    .context("insert Space change work item")?;

    let proposal_id = Uuid::new_v4();
    let confirmation =
        new_confirmation_binding(&authorized_confirmers, context.space_id, &proposal_digest);
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.artifacts
            (id, work_item_id, artifact_type, review_status, created_by_agent,
             title, summary, content_text, content_hash, information_class,
             metadata, review_requested_at)
        VALUES
            ($1, $2, $3, 'pending', 'erhua', 'Space configuration proposal',
             $4, $5, $6, 'internal_ops', $7, now())
        "#,
    )
    .bind(proposal_id)
    .bind(work_item_id)
    .bind(ARTIFACT_TYPE)
    .bind(&intent.summary)
    .bind(String::from_utf8(canonical_bytes).context("Space change intent is not UTF-8")?)
    .bind(&proposal_digest)
    .bind(proposal_metadata(
        context.space_id,
        context.actor_person_id,
        &proposal_digest,
        &confirmation,
    ))
    .execute(&mut *tx)
    .await
    .context("insert Space change proposal artifact")?;

    let mut prepared_event_data = json!({
        "proposal_digest": proposal_digest,
        "change_count": intent.changes.len(),
        "confirmation_expires_at": confirmation.expires_at,
        "external_send_executed": false
    });
    if let Some(request_id) = programming_extension_request_id {
        prepared_event_data["programming_extension_request_id"] = json!(request_id);
    }
    append_event(
        &mut tx,
        work_item_id,
        Some(proposal_id),
        "space_change_prepared",
        "human",
        &context.actor_person_id.to_string(),
        "Space configuration proposal prepared",
        prepared_event_data,
    )
    .await?;

    tx.commit().await.context("commit Space change proposal")?;
    let response = prepare_response(
        &context,
        work_item_id,
        proposal_id,
        &intent,
        &proposal_digest,
        &confirmation,
        false,
    );
    Ok(decorate_continuation_response(
        response,
        programming_extension_request_id,
    ))
}

pub(crate) async fn prepare_programming_extension(
    pool: &PgPool,
    session: TrustedSpaceSession,
    mut request: ProgrammingExtensionRequest,
) -> Result<Value> {
    validate_session(&session)?;
    request.intent = validate_programming_extension_text("intent", &request.intent, 4_000)?;
    request.research_query =
        validate_programming_extension_text("research_query", &request.research_query, 500)?;
    if contains_url(&request.intent) || contains_url(&request.research_query) {
        bail!("programming extension intent must not contain a URL");
    }
    normalize_provider(&mut request.provider)?;
    validate_official_sources(&request.provider, &mut request.official_sources)?;
    if request.official_sources.is_empty() {
        bail!("programming extension requires registered official-source evidence");
    }
    request.research_digest = request.research_digest.trim().to_ascii_lowercase();
    if request.research_digest.len() != 64
        || !request
            .research_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("programming extension research digest is invalid");
    }
    validate_programming_research_evidence(
        &request.provider,
        &request.official_sources,
        &request.research_evidence,
        &request.research_digest,
    )?;

    let mut tx = pool
        .begin()
        .await
        .context("begin Space programming extension transaction")?;
    let context = resolve_context(&mut tx, &session).await?;
    let capability_enabled: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM qintopia_agent_os.capabilities
            WHERE capability_key = $1
              AND enabled = true
              AND 'erhua' = ANY(allowed_callers)
              AND $2 = ANY(allowed_work_item_types)
        )
        "#,
    )
    .bind(CAPABILITY_KEY)
    .bind(PROGRAMMING_EXTENSION_WORK_ITEM_TYPE)
    .fetch_one(&mut *tx)
    .await
    .context("load Space programming extension capability")?;
    if !capability_enabled {
        bail!("Space programming extension capability is not enabled");
    }

    let source_message_ref = session_source_message_ref(&session);
    let idempotency_key = format!(
        "space-programming-extension:{}:{}",
        context.space_id,
        source_message_ref.trim_start_matches("sha256:")
    );
    let request_value =
        serde_json::to_value(&request).context("serialize programming extension request")?;
    let request_digest = sha256_hex(
        &serde_json::to_vec(&request_value).context("encode programming extension request")?,
    );
    let brief_summary = format!(
        "Prepare one bounded {} provider event-mapping extension.",
        request.provider
    );
    let inserted_id: Option<Uuid> = sqlx::query_scalar(
        r#"
        INSERT INTO qintopia_agent_os.work_items
            (space_id, work_item_type, status, requester_agent, target_agent,
             capability_key, human_owner, priority, brief_summary, purpose,
             source_type, source_refs, dedupe_key, idempotency_key, risk_level,
             information_class, payload, payload_redaction_policy, review_policy,
             metadata)
        VALUES
            ($1, $2, 'queued', 'erhua', 'programming_agent', $3, $4, 'normal',
             $5, 'implement a bounded provider event mapping extension',
             'trusted_conversation', $6, $7, $8, 'medium', 'internal_ops', $9,
             'summary_only', 'low_risk_classifier', $10)
        ON CONFLICT (idempotency_key) DO NOTHING
        RETURNING id
        "#,
    )
    .bind(context.space_id)
    .bind(PROGRAMMING_EXTENSION_WORK_ITEM_TYPE)
    .bind(CAPABILITY_KEY)
    .bind(context.actor_person_id.to_string())
    .bind(&brief_summary)
    .bind(json!({"source_message_ref": source_message_ref}))
    .bind(format!(
        "space-programming-extension:{}:{request_digest}",
        context.space_id
    ))
    .bind(&idempotency_key)
    .bind(&request_value)
    .bind(json!({
        "extension_scope": PROGRAMMING_EXTENSION_SCOPE,
        "allowed_change_class": PROGRAMMING_EXTENSION_ALLOWED_CHANGE_CLASS,
        "temporary_worktree_required": true,
        "production_credentials_allowed": false,
        "external_send_executed": false
    }))
    .fetch_optional(&mut *tx)
    .await
    .context("create Space programming extension work item")?;
    let (work_item_id, status, deduped) = if let Some(id) = inserted_id {
        append_event(
            &mut tx,
            id,
            None,
            "space_programming_extension_requested",
            "human",
            &context.actor_person_id.to_string(),
            "Bounded provider event programming extension requested",
            json!({
                "provider": request.provider,
                "research_digest": request.research_digest,
                "official_source_count": request.official_sources.len(),
                "external_send_executed": false
            }),
        )
        .await?;
        (id, "queued".to_string(), false)
    } else {
        let row = sqlx::query(
            r#"
            SELECT id, status, payload
            FROM qintopia_agent_os.work_items
            WHERE idempotency_key = $1
              AND space_id = $2
              AND work_item_type = $3
            "#,
        )
        .bind(&idempotency_key)
        .bind(context.space_id)
        .bind(PROGRAMMING_EXTENSION_WORK_ITEM_TYPE)
        .fetch_optional(&mut *tx)
        .await
        .context("load idempotent Space programming extension")?
        .context("programming extension idempotency key belongs to another work item")?;
        let existing_payload: Value = row.try_get("payload")?;
        if existing_payload != request_value {
            bail!("programming extension idempotency binding changed");
        }
        (row.try_get("id")?, row.try_get("status")?, true)
    };
    tx.commit()
        .await
        .context("commit Space programming extension request")?;
    Ok(json!({
        "success": true,
        "accepted": true,
        "deduped": deduped,
        "request_id": work_item_id,
        "status": status,
        "space_display_name": display_space_name(&context),
        "programming_extension_required": true,
        "allowed_change_class": PROGRAMMING_EXTENSION_ALLOWED_CHANGE_CLASS,
        "external_send_executed": false
    }))
}

pub(crate) async fn programming_extension_continuation_intent(
    pool: &PgPool,
    session: TrustedSpaceSession,
    request_id: Uuid,
) -> Result<Value> {
    validate_session(&session)?;
    let mut tx = pool
        .begin()
        .await
        .context("begin Space programming extension continuation transaction")?;
    let context = resolve_context(&mut tx, &session).await?;
    crate::space_programming_extension::reconcile_for_status(&mut tx, request_id, context.space_id)
        .await?;
    let continuation = load_ready_programming_extension(&mut tx, &context, request_id).await?;
    tx.commit()
        .await
        .context("commit Space programming extension continuation read")?;
    Ok(json!({
        "success": true,
        "accepted": true,
        "request_id": request_id,
        "intent": continuation.intent,
        "external_send_executed": false
    }))
}

pub(crate) async fn confirm(
    pool: &PgPool,
    session: TrustedSpaceSession,
    proposal_id: Uuid,
    confirmation_code: String,
) -> Result<Value> {
    validate_session(&session)?;
    let confirmation_code = validate_confirmation_code(&confirmation_code)?;
    validate_explicit_confirmation_message(&session, &confirmation_code)?;
    let mut tx = pool
        .begin()
        .await
        .context("begin Space change confirmation transaction")?;
    let context = resolve_context(&mut tx, &session).await?;

    let row = sqlx::query(
        r#"
        SELECT
            item.id AS work_item_id,
            item.status AS work_item_status,
            artifact.review_status,
            artifact.content_text,
            artifact.content_hash,
            artifact.metadata
        FROM qintopia_agent_os.artifacts artifact
        JOIN qintopia_agent_os.work_items item ON item.id = artifact.work_item_id
        WHERE artifact.id = $1
          AND artifact.artifact_type = $2
          AND item.work_item_type = $3
          AND item.space_id = $4
        FOR UPDATE OF artifact, item
        "#,
    )
    .bind(proposal_id)
    .bind(ARTIFACT_TYPE)
    .bind(WORK_ITEM_TYPE)
    .bind(context.space_id)
    .fetch_optional(&mut *tx)
    .await
    .context("load Space change proposal")?
    .context("Space change proposal was not found in the current Space")?;
    let work_item_id: Uuid = row.try_get("work_item_id")?;
    let work_item_status: String = row.try_get("work_item_status")?;
    if work_item_status == "completed" {
        tx.rollback()
            .await
            .context("rollback completed proposal read")?;
        return Ok(json!({
            "success": true,
            "accepted": true,
            "deduped": true,
            "request_id": work_item_id,
            "proposal_id": proposal_id,
            "status": "completed",
            "external_send_executed": false
        }));
    }
    if work_item_status != "awaiting_review"
        || row.try_get::<String, _>("review_status")? != "pending"
    {
        bail!("Space change proposal is not awaiting confirmation");
    }

    let content_text: String = row
        .try_get::<Option<String>, _>("content_text")?
        .context("Space change proposal content is missing")?;
    let content_hash: String = row
        .try_get::<Option<String>, _>("content_hash")?
        .context("Space change proposal digest is missing")?;
    let intent_value: Value =
        serde_json::from_str(&content_text).context("parse stored Space change proposal")?;
    let intent = parse_and_validate_stored_intent(intent_value)?;
    let recomputed_digest = sha256_hex(content_text.as_bytes());
    if recomputed_digest != content_hash {
        bail!("Space change proposal digest does not match stored content");
    }
    let mut rematerialized_intent = intent.clone();
    materialize_trusted_intent(&mut tx, context.space_id, &mut rematerialized_intent).await?;
    if serde_json::to_value(&rematerialized_intent)? != serde_json::to_value(&intent)? {
        bail!("Space configuration changed after prepare; create a new proposal");
    }

    let authorization = authorize_actor(&mut tx, context.space_id, context.actor_person_id)
        .await?
        .context("current actor is not authorized to confirm Space changes")?;
    if intent.protects_provider_mapping()
        && !is_global_admin(&mut tx, context.actor_person_id).await?
    {
        bail!("changing active provider event mapping state requires a global owner or admin");
    }

    let mut metadata: Value = row.try_get("metadata")?;
    let confirmation = parse_confirmation_binding(&metadata)?;
    if confirmation.proposal_digest != content_hash || confirmation.space_id != context.space_id {
        bail!("Space change confirmation binding is invalid");
    }
    if Utc::now() > confirmation.expires_at {
        bail!("Space change confirmation code has expired");
    }
    if confirmation.attempts >= confirmation.max_attempts {
        bail!("Space change confirmation attempt limit was reached");
    }
    let stored_hash = confirmation
        .actor_hashes
        .get(&context.actor_person_id.to_string())
        .context("confirmation code is not bound to the current administrator")?;
    let expected_hash = confirmation_hash(
        &confirmation.salt,
        &confirmation_code,
        context.actor_person_id,
        context.space_id,
        &content_hash,
        confirmation.expires_at,
    );
    if !constant_time_eq(stored_hash.as_bytes(), expected_hash.as_bytes()) {
        let attempts = confirmation.attempts + 1;
        metadata["confirmation"]["attempts"] = json!(attempts);
        sqlx::query(
            "UPDATE qintopia_agent_os.artifacts SET metadata = $2, updated_at = now() WHERE id = $1",
        )
        .bind(proposal_id)
        .bind(metadata)
        .execute(&mut *tx)
        .await
        .context("record failed Space change confirmation attempt")?;
        append_event(
            &mut tx,
            work_item_id,
            Some(proposal_id),
            "space_change_confirmation_denied",
            "human",
            &context.actor_person_id.to_string(),
            "Space configuration confirmation denied",
            json!({
                "reason": "confirmation_code_invalid",
                "attempts_remaining": confirmation.max_attempts.saturating_sub(attempts),
                "external_send_executed": false
            }),
        )
        .await?;
        tx.commit()
            .await
            .context("commit failed confirmation attempt")?;
        bail!("Space change confirmation code is invalid");
    }

    let applied = apply_intent(
        &mut tx,
        context.space_id,
        context.actor_person_id,
        work_item_id,
        &intent,
    )
    .await?;
    if authorization == Authorization::BootstrapGlobalAdmin {
        bootstrap_space_admin(&mut tx, context.space_id, context.actor_person_id).await?;
    }

    metadata["confirmation"] = json!({
        "consumed_at": Utc::now(),
        "attempts": confirmation.attempts,
        "max_attempts": confirmation.max_attempts
    });
    sqlx::query(
        r#"
        UPDATE qintopia_agent_os.artifacts
        SET review_status = 'approved', reviewed_at = now(), reviewed_by = $2,
            review_decision_reason = 'confirmed by authorized current-Space administrator',
            metadata = $3, updated_at = now()
        WHERE id = $1 AND review_status = 'pending'
        "#,
    )
    .bind(proposal_id)
    .bind(context.actor_person_id.to_string())
    .bind(metadata)
    .execute(&mut *tx)
    .await
    .context("approve Space change proposal")?;
    let updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET status = 'completed', updated_at = now()
        WHERE id = $1 AND space_id = $2 AND status = 'awaiting_review'
        "#,
    )
    .bind(work_item_id)
    .bind(context.space_id)
    .execute(&mut *tx)
    .await
    .context("complete Space change work item")?;
    if updated.rows_affected() != 1 {
        bail!("Space change work item changed during confirmation");
    }
    append_event(
        &mut tx,
        work_item_id,
        Some(proposal_id),
        "space_change_activated",
        "human",
        &context.actor_person_id.to_string(),
        "Space configuration proposal confirmed",
        json!({
            "proposal_digest": content_hash,
            "definition_count": applied.len(),
            "bootstrapped_space_admin": authorization == Authorization::BootstrapGlobalAdmin,
            "external_send_executed": false
        }),
    )
    .await?;
    tx.commit()
        .await
        .context("commit Space change confirmation")?;
    Ok(json!({
        "success": true,
        "accepted": true,
        "deduped": false,
        "request_id": work_item_id,
        "proposal_id": proposal_id,
        "status": "completed",
        "space_display_name": display_space_name(&context),
        "definitions": applied,
        "external_send_executed": false
    }))
}

pub(crate) async fn status(
    pool: &PgPool,
    session: TrustedSpaceSession,
    request_id: Uuid,
) -> Result<Value> {
    validate_session(&session)?;
    let mut tx = pool
        .begin()
        .await
        .context("begin Space change status transaction")?;
    let context = resolve_context(&mut tx, &session).await?;
    let work_item_type: String = sqlx::query_scalar(
        r#"
        SELECT work_item_type
        FROM qintopia_agent_os.work_items
        WHERE id = $1
          AND space_id = $2
          AND work_item_type IN ($3, $4)
        "#,
    )
    .bind(request_id)
    .bind(context.space_id)
    .bind(WORK_ITEM_TYPE)
    .bind(PROGRAMMING_EXTENSION_WORK_ITEM_TYPE)
    .fetch_optional(&mut *tx)
    .await
    .context("load Space request type")?
    .context("Space change request was not found in the current Space")?;

    if work_item_type == PROGRAMMING_EXTENSION_WORK_ITEM_TYPE {
        crate::space_programming_extension::reconcile_for_status(
            &mut tx,
            request_id,
            context.space_id,
        )
        .await?;
        let row = sqlx::query(
            r#"
            SELECT status, brief_summary, metadata
            FROM qintopia_agent_os.work_items
            WHERE id = $1
              AND space_id = $2
              AND work_item_type = $3
            "#,
        )
        .bind(request_id)
        .bind(context.space_id)
        .bind(PROGRAMMING_EXTENSION_WORK_ITEM_TYPE)
        .fetch_one(&mut *tx)
        .await
        .context("load Space programming extension status")?;
        let work_item_status: String = row.try_get("status")?;
        let metadata: Value = row.try_get("metadata")?;
        let continuation =
            crate::space_programming_extension::status_projection(&work_item_status, &metadata)?;
        let phase = continuation["phase"].clone();
        let release_phase = continuation["release_phase"].clone();
        let next_action = continuation.get("next_action").cloned();
        let programming_extension_required = phase.as_str() != Some("ready_to_replan");
        tx.commit()
            .await
            .context("commit Space programming extension status reconciliation")?;
        return Ok(json!({
            "success": true,
            "accepted": true,
            "request_id": request_id,
            "status": work_item_status,
            "phase": phase,
            "release_phase": release_phase,
            "next_action": next_action,
            "continuation": continuation,
            "space_display_name": display_space_name(&context),
            "summary": row.try_get::<String, _>("brief_summary")?,
            "programming_extension_required": programming_extension_required,
            "allowed_change_class": PROGRAMMING_EXTENSION_ALLOWED_CHANGE_CLASS,
            "external_send_executed": false
        }));
    }

    let row = sqlx::query(
        r#"
        SELECT
            item.status,
            item.brief_summary,
            artifact.id AS proposal_id,
            artifact.review_status,
            artifact.content_text,
            artifact.content_hash,
            artifact.metadata
        FROM qintopia_agent_os.work_items item
        JOIN qintopia_agent_os.artifacts artifact ON artifact.work_item_id = item.id
        WHERE item.id = $1
          AND item.space_id = $2
          AND item.work_item_type = $3
          AND artifact.artifact_type = $4
        "#,
    )
    .bind(request_id)
    .bind(context.space_id)
    .bind(WORK_ITEM_TYPE)
    .bind(ARTIFACT_TYPE)
    .fetch_optional(&mut *tx)
    .await
    .context("load Space change status")?
    .context("Space change request was not found in the current Space")?;
    tx.rollback()
        .await
        .context("finish Space change status read")?;

    let work_item_status: String = row.try_get("status")?;
    let review_status: String = row.try_get("review_status")?;
    let content_text: String = row
        .try_get::<Option<String>, _>("content_text")?
        .context("Space change proposal content is missing")?;
    let intent: SpaceChangeIntent =
        serde_json::from_str(&content_text).context("parse Space change status proposal")?;
    let metadata: Value = row.try_get("metadata")?;
    let proposal_digest = row
        .try_get::<Option<String>, _>("content_hash")?
        .context("Space change proposal digest is missing")?;
    let confirmation_status = if work_item_status == "awaiting_review" {
        parse_confirmation_binding(&metadata).ok().map(|binding| {
            json!({
                "expires_at": binding.expires_at,
                "expired": Utc::now() > binding.expires_at,
                "attempts_remaining": binding.max_attempts.saturating_sub(binding.attempts)
            })
        })
    } else {
        None
    };
    Ok(json!({
        "success": true,
        "accepted": true,
        "request_id": request_id,
        "proposal_id": row.try_get::<Uuid, _>("proposal_id")?,
        "status": work_item_status,
        "review_status": review_status,
        "space_display_name": display_space_name(&context),
        "summary": row.try_get::<String, _>("brief_summary")?,
        "changes": change_summaries(&intent),
        "proposal_digest": proposal_digest,
        "proposal_fingerprint": proposal_fingerprint(&proposal_digest),
        "confirmation": confirmation_status,
        "external_send_executed": false
    }))
}

#[derive(Debug)]
struct ExistingProposal {
    work_item_id: Uuid,
    work_item_status: String,
    proposal_id: Uuid,
    metadata: Value,
}

#[derive(Debug)]
struct NewConfirmation {
    code: String,
    salt: String,
    expires_at: DateTime<Utc>,
    actor_hashes: Map<String, Value>,
}

#[derive(Debug)]
struct StoredConfirmation {
    salt: String,
    expires_at: DateTime<Utc>,
    attempts: u64,
    max_attempts: u64,
    proposal_digest: String,
    space_id: Uuid,
    actor_hashes: HashMap<String, String>,
}

impl SpaceChangeIntent {
    fn protects_provider_mapping(&self) -> bool {
        self.changes.iter().any(|change| match change {
            SpaceChange::ChannelEventMapping { .. } => true,
            SpaceChange::DefinitionOperation {
                operation,
                activation_binding: Some(binding),
                ..
            } => {
                operation == "activate"
                    && binding
                        .event_mapping
                        .as_ref()
                        .is_some_and(|mapping| mapping.status == "shadow")
            }
            _ => false,
        })
    }
}

fn parse_and_validate_intent(value: Value) -> Result<SpaceChangeIntent> {
    parse_and_validate_intent_with_bindings(value, false)
}

fn parse_and_validate_stored_intent(value: Value) -> Result<SpaceChangeIntent> {
    parse_and_validate_intent_with_bindings(value, true)
}

fn parse_and_validate_intent_with_bindings(
    value: Value,
    allow_trusted_bindings: bool,
) -> Result<SpaceChangeIntent> {
    let mut intent: SpaceChangeIntent =
        serde_json::from_value(value).context("Space change intent does not match the schema")?;
    intent.summary = intent.summary.trim().to_string();
    if intent.summary.is_empty() || intent.summary.chars().count() > 300 {
        bail!("Space change summary must be between 1 and 300 characters");
    }
    if intent.changes.is_empty() || intent.changes.len() > MAX_CHANGES {
        bail!("Space change intent must contain between 1 and {MAX_CHANGES} changes");
    }

    let mut seen = HashMap::<String, ()>::new();
    for change in &mut intent.changes {
        match change {
            SpaceChange::SpacePolicy {
                definition_key,
                status,
                policy_config,
            } => {
                validate_definition_key(definition_key)?;
                if definition_key != "default" {
                    bail!("Space policy definition_key must be default in v1");
                }
                validate_definition_status(status)?;
                validate_object("policy_config", policy_config)?;
                validate_json_tree(policy_config, 0)?;
                validate_policy_config(policy_config)?;
                insert_change_identity(&mut seen, format!("space_policy:{definition_key}"))?;
            }
            SpaceChange::BusinessDefinition {
                definition_key,
                status,
                execution_mode,
                definition,
                allowed_capabilities,
                approval_policy,
            } => {
                validate_definition_key(definition_key)?;
                validate_definition_status(status)?;
                if !matches!(execution_mode.as_str(), "deterministic" | "agent_turn") {
                    bail!("business execution_mode is not allowed");
                }
                validate_object("business definition", definition)?;
                validate_json_tree(definition, 0)?;
                normalize_capabilities(allowed_capabilities)?;
                validate_business_execution_contract(
                    execution_mode,
                    definition,
                    allowed_capabilities,
                )?;
                if !matches!(
                    approval_policy.as_str(),
                    "none"
                        | "space_admin_confirmation"
                        | "before_external_use"
                        | "human_final_confirmation"
                ) {
                    bail!("business approval_policy is not allowed");
                }
                insert_change_identity(&mut seen, format!("business_definition:{definition_key}"))?;
            }
            SpaceChange::AutomationDefinition {
                definition_key,
                status,
                source_stream_head_version,
                source_definition_version,
                business_definition_key,
                business_definition_binding,
                trigger_kind,
                trigger_config,
                timezone,
                misfire_policy,
                event_mapping_provider,
                event_mapping_key,
                event_mapping_binding,
            } => {
                if !allow_trusted_bindings
                    && (source_stream_head_version.is_some()
                        || source_definition_version.is_some()
                        || business_definition_binding.is_some()
                        || event_mapping_binding.is_some())
                {
                    bail!("automation dependency bindings are supplied only by the sidecar");
                }
                if allow_trusted_bindings {
                    if source_stream_head_version.is_some_and(|version| version <= 0) {
                        bail!("stored automation source stream head version is invalid");
                    }
                    if source_definition_version.is_some_and(|version| version <= 0) {
                        bail!("stored automation source definition version is invalid");
                    }
                    if source_definition_version.is_some() && source_stream_head_version.is_none() {
                        bail!("stored automation source definition version has no stream binding");
                    }
                    validate_definition_binding(
                        "business_definition_binding",
                        business_definition_binding.as_ref(),
                    )?;
                    if trigger_kind == "event" {
                        validate_definition_binding(
                            "event_mapping_binding",
                            event_mapping_binding.as_ref(),
                        )?;
                    } else if event_mapping_binding.is_some() {
                        bail!("schedule automation must not contain an event mapping binding");
                    }
                }
                validate_definition_key(definition_key)?;
                validate_definition_key(business_definition_key)?;
                validate_definition_status(status)?;
                validate_object("automation trigger_config", trigger_config)?;
                validate_json_tree(trigger_config, 0)?;
                if Tz::from_str(timezone.trim()).is_err() || timezone.len() > 64 {
                    bail!("automation timezone is not a valid IANA timezone");
                }
                *timezone = timezone.trim().to_string();
                if misfire_policy != "run_once" {
                    bail!("automation misfire_policy must be run_once in v1");
                }
                match trigger_kind.as_str() {
                    "schedule" => {
                        if event_mapping_provider.is_some() || event_mapping_key.is_some() {
                            bail!("schedule automation must not specify an event mapping");
                        }
                        let cron = trigger_config
                            .get("cron")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|cron| !cron.is_empty() && cron.len() <= 80)
                            .context("schedule automation requires a bounded cron expression")?;
                        if cron.split_whitespace().count() != 5 {
                            bail!("schedule cron expression must contain exactly five fields");
                        }
                    }
                    "event" => {
                        let provider = event_mapping_provider
                            .as_mut()
                            .context("event automation requires event_mapping_provider")?;
                        let mapping_key = event_mapping_key
                            .as_mut()
                            .context("event automation requires event_mapping_key")?;
                        normalize_provider(provider)?;
                        validate_definition_key(mapping_key)?;
                    }
                    _ => bail!("automation trigger_kind is not allowed"),
                }
                insert_change_identity(
                    &mut seen,
                    format!("automation_definition:{definition_key}"),
                )?;
            }
            SpaceChange::DefinitionOperation {
                target_resource,
                definition_key,
                operation,
                version,
                activation_binding,
                activation_review,
            } => {
                if target_resource != "automation_definition" {
                    bail!("definition operation supports only automation_definition in v1");
                }
                if !allow_trusted_bindings
                    && (activation_binding.is_some() || activation_review.is_some())
                {
                    bail!(
                        "automation activation binding and review are supplied only by the sidecar"
                    );
                }
                validate_definition_key(definition_key)?;
                match operation.as_str() {
                    "activate" if version.is_none() => {
                        if allow_trusted_bindings {
                            validate_automation_activation_binding(
                                activation_binding
                                    .as_ref()
                                    .context("stored automation activation binding is missing")?,
                            )?;
                            validate_automation_activation_review(
                                activation_review
                                    .as_ref()
                                    .context("stored automation activation review is missing")?,
                            )?;
                        }
                    }
                    "activate" => {
                        bail!("activate definition operation must not specify a version")
                    }
                    "pause" if version.is_none() => {}
                    "pause" => bail!("pause definition operation must not specify a version"),
                    "rollback" if version.is_none_or(|version| version > 0) => {}
                    "rollback" => {
                        bail!("rollback definition operation version must be positive")
                    }
                    _ => bail!("definition operation is not allowed"),
                }
                if operation != "activate"
                    && (activation_binding.is_some() || activation_review.is_some())
                {
                    bail!("non-activation definition operation has activation metadata");
                }
                insert_change_identity(
                    &mut seen,
                    format!("automation_definition:{definition_key}"),
                )?;
            }
            SpaceChange::ChannelEventMapping {
                provider,
                definition_key,
                status,
                selector,
                extractor,
                official_sources,
                validation_evidence,
            } => {
                normalize_provider(provider)?;
                validate_definition_key(definition_key)?;
                validate_definition_status(status)?;
                validate_object("event selector", selector)?;
                validate_object("event extractor", extractor)?;
                validate_object("event validation_evidence", validation_evidence)?;
                validate_json_tree(selector, 0)?;
                validate_json_tree(extractor, 0)?;
                validate_json_tree(validation_evidence, 0)?;
                validate_official_sources(provider, official_sources)?;
                crate::channel_event_mapping::validate_definition(selector, extractor)?;
                insert_change_identity(
                    &mut seen,
                    format!("channel_event_mapping:{provider}:{definition_key}"),
                )?;
            }
        }
    }
    if intent.changes.iter().any(|change| {
        matches!(
            change,
            SpaceChange::DefinitionOperation { operation, .. } if operation == "activate"
        )
    }) && intent.changes.len() != 1
    {
        bail!("activate definition operation must be the only change");
    }
    Ok(intent)
}

fn validate_automation_activation_binding(binding: &AutomationActivationBinding) -> Result<()> {
    validate_activation_source_binding("automation", &binding.automation, &["shadow"])?;
    validate_activation_source_binding(
        "business definition",
        &binding.business_definition,
        &["shadow", "active"],
    )?;
    if let Some(mapping) = &binding.event_mapping {
        validate_activation_source_binding("event mapping", mapping, &["shadow", "active"])?;
    }
    Ok(())
}

fn validate_automation_activation_review(review: &AutomationActivationReview) -> Result<()> {
    if review.result_status != "active"
        || review.automation_version <= 0
        || review.business_version <= 0
        || !matches!(review.business_source_status.as_str(), "shadow" | "active")
    {
        bail!("stored automation activation review metadata is invalid");
    }
    validate_activation_fingerprint("automation review", &review.automation_fingerprint)?;
    validate_activation_fingerprint("business review", &review.business_fingerprint)?;
    validate_json_tree(&review.trigger_config, 0)?;
    validate_json_tree(&review.business_definition, 0)?;
    if let Some(mapping) = &review.event_mapping {
        if mapping.version <= 0
            || !matches!(mapping.source_status.as_str(), "shadow" | "active")
            || mapping.event_type.is_empty()
            || mapping.event_type.len() > 160
        {
            bail!("stored automation activation event review is invalid");
        }
        validate_activation_fingerprint("event mapping review", &mapping.fingerprint)?;
        validate_json_tree(&mapping.selector, 0)?;
    }
    Ok(())
}

fn validate_activation_fingerprint(name: &str, fingerprint: &str) -> Result<()> {
    if fingerprint.len() != 64 || !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("stored automation activation {name} fingerprint is invalid");
    }
    Ok(())
}

fn validate_activation_source_binding(
    name: &str,
    binding: &ActivationSourceBinding,
    statuses: &[&str],
) -> Result<()> {
    if binding.id.is_nil()
        || binding.stream_head_version <= 0
        || !statuses.contains(&binding.status.as_str())
        || binding.definition_digest.len() != 64
        || !binding
            .definition_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("stored automation activation {name} binding is invalid");
    }
    Ok(())
}

fn validate_definition_binding(name: &str, binding: Option<&DefinitionBinding>) -> Result<()> {
    let binding = binding.with_context(|| format!("stored automation {name} is missing"))?;
    if binding.definition_digest.len() != 64
        || !binding
            .definition_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("stored automation {name} digest is invalid");
    }
    match binding.source.as_str() {
        "proposal" if binding.id.is_none() && binding.stream_head_version.is_none() => Ok(()),
        "existing"
            if binding.id.is_some()
                && binding
                    .stream_head_version
                    .is_some_and(|version| version > 0) =>
        {
            Ok(())
        }
        _ => bail!("stored automation {name} source binding is invalid"),
    }
}

fn insert_change_identity(seen: &mut HashMap<String, ()>, identity: String) -> Result<()> {
    if seen.insert(identity, ()).is_some() {
        bail!("Space change intent contains duplicate definition streams");
    }
    Ok(())
}

fn validate_definition_key(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 120
        || !value.is_ascii()
        || !value.as_bytes()[0].is_ascii_lowercase()
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.-".contains(&byte)
        })
    {
        bail!("definition_key must be a lowercase stable key");
    }
    Ok(())
}

fn validate_definition_status(value: &str) -> Result<()> {
    if !matches!(value, "draft" | "shadow" | "active" | "paused" | "retired") {
        bail!("definition status is not allowed");
    }
    Ok(())
}

fn normalize_provider(value: &mut String) -> Result<()> {
    *value = value.trim().to_ascii_lowercase();
    if value != "qiwe" {
        bail!("channel event provider is not registered");
    }
    Ok(())
}

fn validate_programming_extension_text(
    name: &str,
    value: &str,
    max_chars: usize,
) -> Result<String> {
    let normalized = value.trim();
    if normalized.is_empty()
        || normalized.chars().count() > max_chars
        || normalized.chars().any(|character| {
            character == '\0'
                || character == '\r'
                || character.is_control() && character != '\n' && character != '\t'
        })
    {
        bail!("programming extension {name} is invalid");
    }

    let lowercase = normalized.to_ascii_lowercase();
    for field in [
        "space_id",
        "room_id",
        "chat_id",
        "group_id",
        "target_id",
        "target_group_id",
        "actor_id",
        "person_id",
        "sender_id",
        "api_key",
        "access_token",
        "refresh_token",
        "password",
        "secret",
        "cookie",
        "authorization",
        "database_url",
    ] {
        let mut remaining = lowercase.as_str();
        while let Some(index) = remaining.find(field) {
            let after = remaining[index + field.len()..].trim_start();
            if after.starts_with('=')
                || after.starts_with(':')
                || after.starts_with("is ")
                || after.starts_with("is:")
            {
                bail!("programming extension {name} contains a privileged assignment");
            }
            remaining = &remaining[index + field.len()..];
        }
    }
    if [
        "密钥是",
        "密钥:",
        "令牌是",
        "令牌:",
        "密码是",
        "密码:",
        "群id是",
        "群 id 是",
    ]
    .iter()
    .any(|marker| lowercase.contains(marker))
    {
        bail!("programming extension {name} contains a privileged assignment");
    }
    Ok(normalized.to_string())
}

fn contains_url(value: &str) -> bool {
    let lowercase = value.to_ascii_lowercase();
    ["http://", "https://", "www.", "://"]
        .iter()
        .any(|marker| lowercase.contains(marker))
}

pub(crate) fn validate_programming_research_evidence(
    provider: &str,
    official_sources: &[String],
    evidence: &[ProgrammingExtensionResearchEvidence],
    research_digest: &str,
) -> Result<()> {
    if evidence.is_empty() || evidence.len() > MAX_PROGRAMMING_RESEARCH_EVIDENCE {
        bail!("programming extension research evidence count is invalid");
    }

    let mut normalized_sources = official_sources.to_vec();
    validate_official_sources(provider, &mut normalized_sources)?;
    if normalized_sources != official_sources {
        bail!("programming extension official sources are not canonical");
    }

    let mut evidence_sources = Vec::with_capacity(evidence.len());
    let mut total_text_bytes = 0usize;
    for item in evidence {
        let mut item_sources = vec![item.url.clone()];
        validate_official_sources(provider, &mut item_sources)?;
        if item_sources.len() != 1 || item_sources[0] != item.url {
            bail!("programming extension evidence URL is not canonical");
        }
        let text_bytes = item.text.len();
        total_text_bytes = total_text_bytes
            .checked_add(text_bytes)
            .context("programming extension research evidence size overflow")?;
        if item.text.trim() != item.text
            || text_bytes == 0
            || text_bytes > MAX_PROGRAMMING_RESEARCH_TEXT_BYTES
            || item.text.chars().any(|character| {
                character == '\0'
                    || character == '\r'
                    || character.is_control() && character != '\n' && character != '\t'
            })
            || contains_url(&item.text)
            || contains_long_numeric_identifier(&item.text)
            || contains_uuid_identifier(&item.text)
            || contains_long_opaque_value(&item.text)
            || contains_unredacted_credential_assignment(&item.text)
        {
            bail!("programming extension research evidence text is invalid");
        }
        evidence_sources.push(item.url.clone());
    }
    if total_text_bytes > MAX_PROGRAMMING_RESEARCH_TOTAL_BYTES {
        bail!("programming extension research evidence is too large");
    }
    if evidence_sources != normalized_sources {
        bail!("programming extension evidence does not match official sources");
    }
    if programming_research_digest(evidence) != research_digest {
        bail!("programming extension research digest does not match evidence");
    }
    Ok(())
}

pub(crate) fn programming_research_digest(
    evidence: &[ProgrammingExtensionResearchEvidence],
) -> String {
    let mut digest = Sha256::new();
    digest.update(PROGRAMMING_RESEARCH_DIGEST_DOMAIN);
    for item in evidence {
        digest.update(item.url.as_bytes());
        digest.update([0]);
        digest.update(item.text.as_bytes());
        digest.update([0]);
    }
    format!("{:x}", digest.finalize())
}

fn contains_long_numeric_identifier(value: &str) -> bool {
    let mut run = 0usize;
    for byte in value.bytes() {
        if byte.is_ascii_digit() {
            run += 1;
            if run >= 12 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

fn contains_uuid_identifier(value: &str) -> bool {
    value
        .split(|character: char| !character.is_ascii_hexdigit() && character != '-')
        .any(|candidate| candidate.len() == 36 && Uuid::parse_str(candidate).is_ok())
}

fn contains_long_opaque_value(value: &str) -> bool {
    value
        .split(|character: char| {
            !(character.is_ascii_alphanumeric() || "+/=_-".contains(character))
        })
        .any(|candidate| candidate.len() >= 32 && candidate != "redacted_opaque_value")
}

fn contains_unredacted_credential_assignment(value: &str) -> bool {
    let lowercase = value.to_ascii_lowercase();
    for field in [
        "authorization",
        "access_token",
        "access-token",
        "accesstoken",
        "refresh_token",
        "refresh-token",
        "refreshtoken",
        "api_key",
        "api-key",
        "apikey",
        "password",
        "secret",
        "cookie",
    ] {
        let mut remaining = lowercase.as_str();
        while let Some(index) = remaining.find(field) {
            let after = remaining[index + field.len()..].trim_start_matches(|character: char| {
                character.is_ascii_whitespace() || character == '\'' || character == '"'
            });
            if let Some(value) = after.strip_prefix(':').or_else(|| after.strip_prefix('=')) {
                let value = value.trim_start_matches(|character: char| {
                    character.is_ascii_whitespace() || character == '\'' || character == '"'
                });
                if !value.starts_with("[redacted_credential]") {
                    return true;
                }
            }
            remaining = &remaining[index + field.len()..];
        }
    }
    false
}

fn normalize_capabilities(values: &mut Vec<String>) -> Result<()> {
    if values.len() > 32 {
        bail!("allowed_capabilities contains too many entries");
    }
    for value in values.iter_mut() {
        *value = value.trim().to_string();
        if value.is_empty()
            || value.len() > 160
            || !value.is_ascii()
            || !value.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.-".contains(&byte)
            })
        {
            bail!("allowed_capabilities contains an invalid capability key");
        }
    }
    values.sort();
    values.dedup();
    Ok(())
}

fn validate_policy_config(value: &Value) -> Result<()> {
    crate::space_turn_policy::validate_policy_config(value)
}

fn validate_business_execution_contract(
    execution_mode: &str,
    definition: &Value,
    allowed_capabilities: &[String],
) -> Result<()> {
    match execution_mode {
        "deterministic" => {
            let capability_key = definition
                .get("capability_key")
                .and_then(Value::as_str)
                .context("deterministic business capability_key is required")?;
            if !allowed_capabilities.iter().any(|key| key == capability_key) {
                bail!("deterministic capability must be inside allowed_capabilities");
            }
            definition
                .get("input")
                .and_then(Value::as_object)
                .context("deterministic business input must be an object")?;
        }
        "agent_turn" => {
            let goal = definition
                .get("goal")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|goal| !goal.is_empty())
                .context("agent_turn business goal is required")?;
            if goal.chars().count() > 4_000 || goal.chars().any(char::is_control) {
                bail!("agent_turn business goal is invalid");
            }
            if !allowed_capabilities
                .iter()
                .any(|key| key == AGENT_TURN_CAPABILITY_KEY)
            {
                bail!("agent_turn business requires the constrained handoff capability");
            }
            let output_contract = definition
                .get("output_contract")
                .context("agent_turn business output_contract is required")?;
            crate::space_agent_turn::validate_output_contract(output_contract)?;
        }
        _ => bail!("business execution_mode is not registered"),
    }
    Ok(())
}

fn validate_active_approval_policy_contract(
    execution_mode: &str,
    deterministic_recipe: Option<crate::space_capability_recipe::RegisteredRecipe>,
    approval_policy: &str,
) -> Result<()> {
    match execution_mode {
        "deterministic" => match deterministic_recipe
            .context("active deterministic automation has no registered execution recipe")?
        {
            crate::space_capability_recipe::RegisteredRecipe::QiweTextTemplateV1 => {
                if approval_policy != "space_admin_confirmation" {
                    bail!(
                        "active qiwe_text_template_v1 automation requires space_admin_confirmation"
                    );
                }
            }
        },
        "agent_turn" => {
            if !matches!(approval_policy, "none" | "space_admin_confirmation") {
                bail!("active agent_turn automation has an unsupported per-run approval policy");
            }
        }
        _ => bail!("active automation execution mode is not registered"),
    }
    Ok(())
}

async fn validate_active_business_runtime_contract(
    tx: &mut Transaction<'_, Postgres>,
    execution_mode: &str,
    definition: &Value,
    approval_policy: &str,
) -> Result<()> {
    if execution_mode == "agent_turn" && !crate::space_agent_turn::runtime_readiness()? {
        bail!("active agent_turn automation requires owner-reviewed broker and runner readiness");
    }
    let deterministic_recipe = if execution_mode == "deterministic" {
        let capability_key = definition
            .get("capability_key")
            .and_then(Value::as_str)
            .context("active deterministic business capability_key is missing")?;
        let metadata = sqlx::query_scalar::<_, Value>(
            r#"
            SELECT metadata
            FROM qintopia_agent_os.capabilities
            WHERE capability_key = $1
              AND provider_agent = 'erhua'
            "#,
        )
        .bind(capability_key)
        .fetch_optional(&mut **tx)
        .await
        .context("load active deterministic capability runtime contract")?
        .context("active deterministic capability is not registered")?;
        Some(crate::space_capability_recipe::from_capability_metadata(
            &metadata,
        )?)
    } else {
        None
    };
    validate_active_approval_policy_contract(execution_mode, deterministic_recipe, approval_policy)
}

fn validate_object(name: &str, value: &Value) -> Result<()> {
    if !value.is_object() {
        bail!("{name} must be an object");
    }
    Ok(())
}

fn validate_json_tree(value: &Value, depth: usize) -> Result<()> {
    if depth > MAX_JSON_DEPTH {
        bail!("definition JSON exceeds the maximum nesting depth");
    }
    match value {
        Value::Object(object) => {
            if object.len() > MAX_JSON_COLLECTION_ITEMS {
                bail!("definition JSON object contains too many fields");
            }
            for (key, child) in object {
                if key.len() > 120 || is_forbidden_definition_key(key) {
                    bail!("definition JSON contains a forbidden field");
                }
                validate_json_tree(child, depth + 1)?;
            }
        }
        Value::Array(items) => {
            if items.len() > MAX_JSON_COLLECTION_ITEMS {
                bail!("definition JSON array contains too many entries");
            }
            for item in items {
                validate_json_tree(item, depth + 1)?;
            }
        }
        Value::String(text) if text.chars().count() > 4_000 => {
            bail!("definition JSON string is too long");
        }
        _ => {}
    }
    Ok(())
}

fn is_forbidden_definition_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    matches!(
        normalized.as_str(),
        "spaceid"
            | "roomid"
            | "chatid"
            | "targetid"
            | "targetgroupid"
            | "destination"
            | "recipient"
            | "actorid"
            | "personid"
            | "authorization"
            | "apikey"
            | "accesstoken"
            | "refreshtoken"
            | "password"
            | "secret"
            | "cookie"
            | "sql"
            | "script"
            | "shell"
            | "command"
            | "webhook"
            | "endpoint"
            | "httprequest"
    )
}

fn validate_official_sources(provider: &str, sources: &mut Vec<String>) -> Result<()> {
    if sources.len() > 8 {
        bail!("official_sources contains too many entries");
    }
    for source in sources.iter_mut() {
        *source = source.trim().to_string();
        let mut parsed = Url::parse(source).context("official source must be an absolute URL")?;
        if parsed.scheme() != "https"
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.port().is_some()
            || parsed.query().is_some()
        {
            bail!("official source URL is not allowed");
        }
        let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
        let document_id = parsed.path().strip_prefix("/doc-").unwrap_or_default();
        let allowed = provider == "qiwe"
            && host == "doc.qiweapi.com"
            && !document_id.is_empty()
            && document_id.bytes().all(|byte| byte.is_ascii_digit());
        if !allowed {
            bail!("official source host is not registered for the provider");
        }
        parsed.set_fragment(None);
        *source = parsed.to_string();
    }
    sources.sort();
    sources.dedup();
    Ok(())
}

fn validate_session(session: &TrustedSpaceSession) -> Result<()> {
    if !session.platform.trim().eq_ignore_ascii_case("qiwe") {
        bail!("trusted QiWe session context is required");
    }
    if !matches!(
        session
            .conversation_type
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "group" | "room" | "group_chat"
    ) {
        bail!("Space configuration requires a trusted group conversation");
    }
    for (name, value, max_len) in [
        (
            "conversation_id",
            session.conversation_id.as_str(),
            200usize,
        ),
        (
            "requester_user_id",
            session.requester_user_id.as_str(),
            200usize,
        ),
        (
            "source_message_id",
            session.source_message_id.as_str(),
            240usize,
        ),
    ] {
        let trimmed = value.trim();
        if trimmed.is_empty()
            || trimmed.len() > max_len
            || trimmed.chars().any(|character| character.is_control())
        {
            bail!("trusted session {name} is invalid");
        }
    }
    Ok(())
}

async fn resolve_context(
    tx: &mut Transaction<'_, Postgres>,
    session: &TrustedSpaceSession,
) -> Result<ResolvedContext> {
    let platform = session.platform.trim().to_ascii_lowercase();
    let conversation_id = session.conversation_id.trim();
    let row = sqlx::query(
        r#"
        INSERT INTO qintopia_messages.conversations
            (tenant_id, platform, chat_id, chat_type, last_seen_at, updated_at)
        VALUES ('qintopia', $1, $2, 'group', now(), now())
        ON CONFLICT (tenant_id, platform, chat_id) DO UPDATE SET
            chat_type = 'group',
            last_seen_at = now(),
            updated_at = now()
        RETURNING id, display_name
        "#,
    )
    .bind(&platform)
    .bind(conversation_id)
    .fetch_one(&mut **tx)
    .await
    .context("resolve trusted conversation Space")?;
    let space_id: Uuid = row.try_get("id")?;
    let actor_row = sqlx::query(
        r#"
        SELECT person_id
        FROM qintopia_identity.channel_identities
        WHERE platform = $1
          AND channel_user_id = $2
          AND chat_id = $3
          AND person_id IS NOT NULL
        "#,
    )
    .bind(&platform)
    .bind(session.requester_user_id.trim())
    .bind(conversation_id)
    .fetch_optional(&mut **tx)
    .await
    .context("resolve trusted Space actor")?
    .context("trusted Space actor is not linked to a person")?;
    Ok(ResolvedContext {
        space_id,
        space_display_name: row.try_get("display_name")?,
        actor_person_id: actor_row.try_get("person_id")?,
    })
}

async fn load_ready_programming_extension(
    tx: &mut Transaction<'_, Postgres>,
    context: &ResolvedContext,
    request_id: Uuid,
) -> Result<crate::space_programming_extension::ReadyContinuation> {
    let row = sqlx::query(
        r#"
        SELECT status, payload, metadata, human_owner
        FROM qintopia_agent_os.work_items
        WHERE id = $1
          AND space_id = $2
          AND work_item_type = $3
        FOR UPDATE
        "#,
    )
    .bind(request_id)
    .bind(context.space_id)
    .bind(PROGRAMMING_EXTENSION_WORK_ITEM_TYPE)
    .fetch_optional(&mut **tx)
    .await
    .context("lock Space programming extension continuation")?
    .context("Space programming extension was not found in the current Space")?;
    let original_owner: String = row.try_get("human_owner")?;
    let actor_id = context.actor_person_id.to_string();
    if original_owner != actor_id
        && authorize_actor(tx, context.space_id, context.actor_person_id)
            .await?
            .is_none()
    {
        bail!("current actor cannot continue this Space programming extension");
    }
    crate::space_programming_extension::ready_continuation(
        context.space_id,
        &row.try_get::<String, _>("status")?,
        &row.try_get::<Value, _>("payload")?,
        &row.try_get::<Value, _>("metadata")?,
    )
}

fn validate_programming_extension_shadow_intent(
    intent: &SpaceChangeIntent,
    continuation: &crate::space_programming_extension::ReadyContinuation,
) -> Result<()> {
    let mappings = intent
        .changes
        .iter()
        .filter_map(|change| match change {
            SpaceChange::ChannelEventMapping {
                provider,
                definition_key,
                status,
                ..
            } => Some((provider, definition_key, status)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if mappings.len() != 1
        || mappings[0].0 != &continuation.provider
        || mappings[0].1 != &continuation.mapping_key
        || mappings[0].2 != "shadow"
    {
        bail!("continued Space proposal must contain the exact released mapping in shadow status");
    }
    Ok(())
}

fn decorate_continuation_response(mut response: Value, request_id: Option<Uuid>) -> Value {
    if let (Some(request_id), Some(object)) = (request_id, response.as_object_mut()) {
        object.insert("continued_from_request_id".to_string(), json!(request_id));
        object.insert("continuation_phase".to_string(), json!("shadow_prepared"));
    }
    response
}

async fn authorized_confirmer_ids(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    require_global_admin: bool,
) -> Result<Vec<Uuid>> {
    if require_global_admin {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT membership.person_id
            FROM qintopia_identity.person_memberships membership
            JOIN qintopia_messages.conversations conversation
              ON conversation.id = $1
             AND conversation.tenant_id = 'qintopia'
             AND conversation.platform = 'qiwe'
             AND conversation.chat_type = 'group'
             AND conversation.status = 'active'
            JOIN qintopia_identity.channel_identities identity
              ON identity.person_id = membership.person_id
             AND identity.platform = conversation.platform
             AND identity.chat_id = conversation.chat_id
            WHERE membership.community_key = 'qintopia'
              AND membership.role IN ('owner', 'admin')
              AND membership.status = 'active'
            ORDER BY membership.person_id
            LIMIT 33
            "#,
        )
        .bind(space_id)
        .fetch_all(&mut **tx)
        .await
        .context("load current-group global administrators for provider mappings")?;
        return confirmer_ids_from_rows(rows);
    }

    let community_key = format!("space:{space_id}");
    let rows = sqlx::query(
        r#"
        SELECT DISTINCT person_id
        FROM qintopia_identity.person_memberships
        WHERE community_key = $1
          AND role IN ('space_admin', 'business_admin')
          AND status = 'active'
        ORDER BY person_id
        LIMIT 33
        "#,
    )
    .bind(&community_key)
    .fetch_all(&mut **tx)
    .await
    .context("load Space configuration administrators")?;
    let rows = if rows.is_empty() {
        sqlx::query(
            r#"
            SELECT DISTINCT person_id
            FROM qintopia_identity.person_memberships
            WHERE community_key = 'qintopia'
              AND role IN ('owner', 'admin')
              AND status = 'active'
            ORDER BY person_id
            LIMIT 33
            "#,
        )
        .fetch_all(&mut **tx)
        .await
        .context("load global Space bootstrap administrators")?
    } else {
        rows
    };
    confirmer_ids_from_rows(rows)
}

fn confirmer_ids_from_rows(rows: Vec<sqlx::postgres::PgRow>) -> Result<Vec<Uuid>> {
    if rows.len() > 32 {
        bail!("Space configuration administrator set exceeds the supported ceiling");
    }
    rows.into_iter()
        .map(|row| {
            row.try_get("person_id")
                .context("read administrator person id")
        })
        .collect()
}

async fn authorize_actor(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    actor_person_id: Uuid,
) -> Result<Option<Authorization>> {
    let community_key = format!("space:{space_id}");
    let has_any_space_admin: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM qintopia_identity.person_memberships
            WHERE community_key = $1
              AND role IN ('space_admin', 'business_admin')
              AND status = 'active'
        )
        "#,
    )
    .bind(&community_key)
    .fetch_one(&mut **tx)
    .await
    .context("check Space administrator bootstrap state")?;
    let is_space_admin: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM qintopia_identity.person_memberships
            WHERE person_id = $1
              AND community_key = $2
              AND role IN ('space_admin', 'business_admin')
              AND status = 'active'
        )
        "#,
    )
    .bind(actor_person_id)
    .bind(&community_key)
    .fetch_one(&mut **tx)
    .await
    .context("check current Space administrator")?;
    if is_space_admin {
        return Ok(Some(Authorization::SpaceAdmin));
    }
    if !has_any_space_admin && is_global_admin(tx, actor_person_id).await? {
        return Ok(Some(Authorization::BootstrapGlobalAdmin));
    }
    Ok(None)
}

async fn is_global_admin(
    tx: &mut Transaction<'_, Postgres>,
    actor_person_id: Uuid,
) -> Result<bool> {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM qintopia_identity.person_memberships
            WHERE person_id = $1
              AND community_key = 'qintopia'
              AND role IN ('owner', 'admin')
              AND status = 'active'
        )
        "#,
    )
    .bind(actor_person_id)
    .fetch_one(&mut **tx)
    .await
    .context("check global configuration administrator")
}

async fn bootstrap_space_admin(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    actor_person_id: Uuid,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO qintopia_identity.person_memberships
            (person_id, community_key, role, status, display_label, metadata, started_at)
        VALUES ($1, $2, 'space_admin', 'active', 'Space administrator',
                '{"source":"trusted_space_bootstrap"}'::jsonb, now())
        ON CONFLICT (person_id, community_key, role) DO UPDATE SET
            status = 'active',
            ended_at = NULL,
            updated_at = now(),
            metadata = qintopia_identity.person_memberships.metadata
                || '{"source":"trusted_space_bootstrap"}'::jsonb
        "#,
    )
    .bind(actor_person_id)
    .bind(format!("space:{space_id}"))
    .execute(&mut **tx)
    .await
    .context("bootstrap first Space administrator")?;
    Ok(())
}

fn session_source_message_ref(session: &TrustedSpaceSession) -> String {
    let mut hasher = Sha256::new();
    for part in [
        "trusted-space-source-v1",
        session.platform.trim(),
        session.conversation_id.trim(),
        session.requester_user_id.trim(),
        session.source_message_id.trim(),
    ] {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    format!("sha256:{:x}", hasher.finalize())
}

fn new_confirmation_binding(
    actor_ids: &[Uuid],
    space_id: Uuid,
    proposal_digest: &str,
) -> NewConfirmation {
    let code = Uuid::new_v4().simple().to_string()[..8].to_ascii_uppercase();
    let salt = Uuid::new_v4().simple().to_string();
    let expires_at = Utc::now() + Duration::minutes(CONFIRMATION_TTL_MINUTES);
    let actor_hashes = actor_ids
        .iter()
        .map(|actor_id| {
            (
                actor_id.to_string(),
                json!(confirmation_hash(
                    &salt,
                    &code,
                    *actor_id,
                    space_id,
                    proposal_digest,
                    expires_at,
                )),
            )
        })
        .collect();
    NewConfirmation {
        code,
        salt,
        expires_at,
        actor_hashes,
    }
}

fn confirmation_hash(
    salt: &str,
    code: &str,
    actor_person_id: Uuid,
    space_id: Uuid,
    proposal_digest: &str,
    expires_at: DateTime<Utc>,
) -> String {
    let mut hasher = Sha256::new();
    for part in [
        "space-change-confirmation-v1".to_string(),
        salt.to_string(),
        code.trim().to_ascii_uppercase(),
        actor_person_id.to_string(),
        space_id.to_string(),
        proposal_digest.to_string(),
        expires_at.to_rfc3339(),
    ] {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

fn validate_confirmation_code(value: &str) -> Result<String> {
    let normalized = value.trim().to_ascii_uppercase();
    if normalized.len() != 8 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("confirmation code must be eight hexadecimal characters");
    }
    Ok(normalized)
}

fn validate_explicit_confirmation_message(
    session: &TrustedSpaceSession,
    confirmation_code: &str,
) -> Result<()> {
    let expected = format!("确认 {confirmation_code}");
    let actual = session.source_message_text.as_deref().map(str::trim);
    if actual != Some(expected.as_str()) {
        bail!("current trusted message must exactly match the explicit confirmation command");
    }
    Ok(())
}

fn proposal_metadata(
    space_id: Uuid,
    proposed_by_person_id: Uuid,
    proposal_digest: &str,
    confirmation: &NewConfirmation,
) -> Value {
    json!({
        "proposal_schema": PROPOSAL_SCHEMA,
        "space_id": space_id,
        "proposed_by_person_id": proposed_by_person_id,
        "proposal_digest": proposal_digest,
        "confirmation": {
            "salt": confirmation.salt,
            "expires_at": confirmation.expires_at,
            "attempts": 0,
            "max_attempts": MAX_CONFIRMATION_ATTEMPTS,
            "actor_hashes": confirmation.actor_hashes
        },
        "external_send_executed": false
    })
}

fn parse_confirmation_binding(metadata: &Value) -> Result<StoredConfirmation> {
    if metadata.get("proposal_schema").and_then(Value::as_str) != Some(PROPOSAL_SCHEMA) {
        bail!("Space change proposal schema is invalid");
    }
    let confirmation = metadata
        .get("confirmation")
        .and_then(Value::as_object)
        .context("Space change confirmation metadata is missing")?;
    let expires_at = DateTime::parse_from_rfc3339(
        confirmation
            .get("expires_at")
            .and_then(Value::as_str)
            .context("Space change confirmation expiry is missing")?,
    )
    .context("parse Space change confirmation expiry")?
    .with_timezone(&Utc);
    let actor_hashes = confirmation
        .get("actor_hashes")
        .and_then(Value::as_object)
        .context("Space change confirmation actor binding is missing")?
        .iter()
        .map(|(actor_id, hash)| {
            let hash = hash
                .as_str()
                .filter(|hash| {
                    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
                })
                .context("Space change confirmation hash is invalid")?;
            Ok((actor_id.clone(), hash.to_string()))
        })
        .collect::<Result<HashMap<_, _>>>()?;
    Ok(StoredConfirmation {
        salt: confirmation
            .get("salt")
            .and_then(Value::as_str)
            .filter(|salt| salt.len() == 32 && salt.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .context("Space change confirmation salt is invalid")?
            .to_string(),
        expires_at,
        attempts: confirmation
            .get("attempts")
            .and_then(Value::as_u64)
            .context("Space change confirmation attempts are missing")?,
        max_attempts: confirmation
            .get("max_attempts")
            .and_then(Value::as_u64)
            .context("Space change confirmation attempt ceiling is missing")?,
        proposal_digest: metadata
            .get("proposal_digest")
            .and_then(Value::as_str)
            .context("Space change proposal digest binding is missing")?
            .to_string(),
        space_id: Uuid::parse_str(
            metadata
                .get("space_id")
                .and_then(Value::as_str)
                .context("Space change proposal Space binding is missing")?,
        )
        .context("parse Space change proposal Space binding")?,
        actor_hashes,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

async fn load_existing_proposal(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    idempotency_key: &str,
    proposal_digest: &str,
    actor_person_id: Uuid,
) -> Result<Option<ExistingProposal>> {
    let row = sqlx::query(
        r#"
        SELECT
            item.id AS work_item_id,
            item.status AS work_item_status,
            artifact.id AS proposal_id,
            artifact.content_hash,
            artifact.metadata
        FROM qintopia_agent_os.work_items item
        JOIN qintopia_agent_os.artifacts artifact ON artifact.work_item_id = item.id
        WHERE item.idempotency_key = $1
          AND item.space_id = $2
          AND item.work_item_type = $3
          AND artifact.artifact_type = $4
        FOR UPDATE OF item, artifact
        "#,
    )
    .bind(idempotency_key)
    .bind(space_id)
    .bind(WORK_ITEM_TYPE)
    .bind(ARTIFACT_TYPE)
    .fetch_optional(&mut **tx)
    .await
    .context("load idempotent Space change proposal")?;
    let Some(row) = row else {
        return Ok(None);
    };
    let content_hash = row
        .try_get::<Option<String>, _>("content_hash")?
        .context("idempotent Space change proposal digest is missing")?;
    let metadata: Value = row.try_get("metadata")?;
    if content_hash != proposal_digest
        || metadata
            .get("proposed_by_person_id")
            .and_then(Value::as_str)
            != Some(actor_person_id.to_string().as_str())
    {
        bail!("trusted source message is already bound to a different Space change proposal");
    }
    Ok(Some(ExistingProposal {
        work_item_id: row.try_get("work_item_id")?,
        work_item_status: row.try_get("work_item_status")?,
        proposal_id: row.try_get("proposal_id")?,
        metadata,
    }))
}

async fn reissue_existing_confirmation(
    tx: &mut Transaction<'_, Postgres>,
    context: &ResolvedContext,
    intent: &SpaceChangeIntent,
    proposal_digest: &str,
    authorized_confirmers: &[Uuid],
    existing: ExistingProposal,
) -> Result<Value> {
    if existing.work_item_status == "completed" {
        return Ok(json!({
            "success": true,
            "accepted": true,
            "deduped": true,
            "request_id": existing.work_item_id,
            "proposal_id": existing.proposal_id,
            "status": "completed",
            "space_display_name": display_space_name(context),
            "summary": intent.summary,
            "changes": change_summaries(intent),
            "proposal_digest": proposal_digest,
            "proposal_fingerprint": proposal_fingerprint(proposal_digest),
            "external_send_executed": false
        }));
    }
    if existing.work_item_status != "awaiting_review" {
        bail!("idempotent Space change proposal is no longer awaiting confirmation");
    }
    let confirmation =
        new_confirmation_binding(authorized_confirmers, context.space_id, proposal_digest);
    let mut metadata = existing.metadata;
    metadata["confirmation"] = json!({
        "salt": confirmation.salt,
        "expires_at": confirmation.expires_at,
        "attempts": 0,
        "max_attempts": MAX_CONFIRMATION_ATTEMPTS,
        "actor_hashes": confirmation.actor_hashes
    });
    sqlx::query(
        r#"
        UPDATE qintopia_agent_os.artifacts
        SET metadata = $2, updated_at = now()
        WHERE id = $1 AND review_status = 'pending'
        "#,
    )
    .bind(existing.proposal_id)
    .bind(metadata)
    .execute(&mut **tx)
    .await
    .context("reissue Space change confirmation")?;
    append_event(
        tx,
        existing.work_item_id,
        Some(existing.proposal_id),
        "space_change_confirmation_reissued",
        "human",
        &context.actor_person_id.to_string(),
        "Space configuration confirmation reissued",
        json!({
            "proposal_digest": proposal_digest,
            "confirmation_expires_at": confirmation.expires_at,
            "external_send_executed": false
        }),
    )
    .await?;
    Ok(prepare_response(
        context,
        existing.work_item_id,
        existing.proposal_id,
        intent,
        proposal_digest,
        &confirmation,
        true,
    ))
}

fn prepare_response(
    context: &ResolvedContext,
    work_item_id: Uuid,
    proposal_id: Uuid,
    intent: &SpaceChangeIntent,
    proposal_digest: &str,
    confirmation: &NewConfirmation,
    deduped: bool,
) -> Value {
    json!({
        "success": true,
        "accepted": true,
        "deduped": deduped,
        "request_id": work_item_id,
        "proposal_id": proposal_id,
        "status": "awaiting_review",
        "space_display_name": display_space_name(context),
        "summary": intent.summary,
        "changes": change_summaries(intent),
        "proposal_digest": proposal_digest,
        "proposal_fingerprint": proposal_fingerprint(proposal_digest),
        "confirmation_code": confirmation.code,
        "confirmation_command": format!("@二花 确认 {}", confirmation.code),
        "confirmation_normalized_command": format!("确认 {}", confirmation.code),
        "confirmation_instruction": "In the current group, mention Erhua and send the displayed command. The gateway removes the mention and validates the remaining confirmation text exactly.",
        "confirmation_expires_at": confirmation.expires_at,
        "confirmation_required": true,
        "external_send_executed": false
    })
}

fn proposal_fingerprint(proposal_digest: &str) -> &str {
    proposal_digest.get(..12).unwrap_or(proposal_digest)
}

fn display_space_name(context: &ResolvedContext) -> String {
    context
        .space_display_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("current group")
        .chars()
        .take(120)
        .collect()
}

fn change_summaries(intent: &SpaceChangeIntent) -> Vec<Value> {
    intent
        .changes
        .iter()
        .map(|change| match change {
            SpaceChange::SpacePolicy {
                definition_key,
                status,
                policy_config,
            } => json!({
                "resource": "space_policy",
                "definition_key": definition_key,
                "status": status,
                "policy_config": policy_config
            }),
            SpaceChange::BusinessDefinition {
                definition_key,
                status,
                execution_mode,
                definition,
                allowed_capabilities,
                approval_policy,
            } => json!({
                "resource": "business_definition",
                "definition_key": definition_key,
                "status": status,
                "execution_mode": execution_mode,
                "definition": definition,
                "allowed_capabilities": allowed_capabilities,
                "approval_policy": approval_policy
            }),
            SpaceChange::AutomationDefinition {
                definition_key,
                status,
                business_definition_key,
                trigger_kind,
                trigger_config,
                timezone,
                misfire_policy,
                event_mapping_provider,
                event_mapping_key,
                ..
            } => json!({
                "resource": "automation_definition",
                "definition_key": definition_key,
                "status": status,
                "business_definition_key": business_definition_key,
                "trigger_kind": trigger_kind,
                "trigger_config": trigger_config,
                "timezone": timezone,
                "misfire_policy": misfire_policy,
                "event_mapping": match (event_mapping_provider, event_mapping_key) {
                    (Some(provider), Some(mapping_key)) => Some(json!({
                        "provider": provider,
                        "definition_key": mapping_key
                    })),
                    _ => None,
                }
            }),
            SpaceChange::DefinitionOperation {
                target_resource,
                definition_key,
                operation,
                version,
                activation_review,
                ..
            } => json!({
                "resource": "definition_operation",
                "target_resource": target_resource,
                "definition_key": definition_key,
                "operation": operation,
                "version": version,
                "activation_review": activation_review
            }),
            SpaceChange::ChannelEventMapping {
                provider,
                definition_key,
                status,
                selector,
                extractor,
                official_sources,
                ..
            } => json!({
                "resource": "channel_event_mapping",
                "provider": provider,
                "definition_key": definition_key,
                "status": status,
                "event_type": extractor.get("event_type").cloned(),
                "match_conditions": selector,
                "official_sources": official_sources
            }),
        })
        .collect()
}

#[derive(Debug)]
struct AutomationOperationTarget {
    stream_head_version: i32,
    definition_version: i32,
    business_definition_key: String,
    business_status: String,
    channel_event_mapping_id: Option<Uuid>,
    trigger_kind: String,
    trigger_config: Value,
    timezone: String,
    misfire_policy: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AutomationRollbackLineage {
    shadow_automation_definition_id: Uuid,
    source_mapping_version_id: Uuid,
    observation_work_item_id: Uuid,
    raw_event_id: Uuid,
}

#[derive(Debug)]
struct ActivationAutomationTarget {
    binding: ActivationSourceBinding,
    definition_key: String,
    trigger_kind: String,
    trigger_config: Value,
    timezone: String,
    misfire_policy: String,
}

#[derive(Debug)]
struct ActivationBusinessTarget {
    binding: ActivationSourceBinding,
    definition_key: String,
    execution_mode: String,
    definition: Value,
    allowed_capabilities: Vec<String>,
    approval_policy: String,
}

#[derive(Debug)]
struct ActivationEventMappingTarget {
    binding: ActivationSourceBinding,
    provider: String,
    definition_key: String,
    selector: Value,
    extractor: Value,
    official_sources: Vec<String>,
}

#[derive(Debug)]
struct AutomationActivationTarget {
    automation: ActivationAutomationTarget,
    business_definition: ActivationBusinessTarget,
    event_mapping: Option<ActivationEventMappingTarget>,
}

impl AutomationActivationTarget {
    fn binding(&self) -> AutomationActivationBinding {
        AutomationActivationBinding {
            automation: self.automation.binding.clone(),
            business_definition: self.business_definition.binding.clone(),
            event_mapping: self
                .event_mapping
                .as_ref()
                .map(|mapping| mapping.binding.clone()),
        }
    }

    fn review(&self) -> Result<AutomationActivationReview> {
        let event_mapping = self
            .event_mapping
            .as_ref()
            .map(|mapping| -> Result<ActivationEventMappingReview> {
                let event_type = mapping
                    .extractor
                    .get("event_type")
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
                    .context("stored activation event mapping event_type is missing")?;
                Ok(ActivationEventMappingReview {
                    provider: mapping.provider.clone(),
                    definition_key: mapping.definition_key.clone(),
                    source_status: mapping.binding.status.clone(),
                    version: mapping.binding.stream_head_version,
                    fingerprint: mapping.binding.definition_digest.clone(),
                    event_type,
                    selector: mapping.selector.clone(),
                })
            })
            .transpose()?;
        Ok(AutomationActivationReview {
            result_status: "active".to_string(),
            automation_version: self.automation.binding.stream_head_version,
            automation_fingerprint: self.automation.binding.definition_digest.clone(),
            trigger_kind: self.automation.trigger_kind.clone(),
            trigger_config: self.automation.trigger_config.clone(),
            timezone: self.automation.timezone.clone(),
            misfire_policy: self.automation.misfire_policy.clone(),
            business_definition_key: self.business_definition.definition_key.clone(),
            business_source_status: self.business_definition.binding.status.clone(),
            business_version: self.business_definition.binding.stream_head_version,
            business_fingerprint: self.business_definition.binding.definition_digest.clone(),
            execution_mode: self.business_definition.execution_mode.clone(),
            business_definition: self.business_definition.definition.clone(),
            allowed_capabilities: self.business_definition.allowed_capabilities.clone(),
            approval_policy: self.business_definition.approval_policy.clone(),
            event_mapping,
        })
    }
}

async fn materialize_definition_operations(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    intent: &mut SpaceChangeIntent,
) -> Result<()> {
    for index in 0..intent.changes.len() {
        let operation = match &intent.changes[index] {
            SpaceChange::DefinitionOperation {
                definition_key,
                operation,
                version,
                activation_binding,
                activation_review,
                ..
            } => Some((
                definition_key.clone(),
                operation.clone(),
                *version,
                activation_binding.clone(),
                activation_review.clone(),
            )),
            _ => None,
        };
        let Some((definition_key, operation, version, activation_binding, activation_review)) =
            operation
        else {
            continue;
        };

        if operation == "activate" {
            let target = load_automation_activation_target(tx, space_id, &definition_key).await?;
            validate_automation_activation_target(tx, space_id, &target).await?;
            let resolved_binding = target.binding();
            let resolved_review = target.review()?;
            if activation_binding
                .as_ref()
                .is_some_and(|binding| binding != &resolved_binding)
                || activation_review
                    .as_ref()
                    .is_some_and(|review| review != &resolved_review)
            {
                bail!("automation activation dependencies changed after prepare; create a new proposal");
            }
            let SpaceChange::DefinitionOperation {
                activation_binding,
                activation_review,
                ..
            } = &mut intent.changes[index]
            else {
                bail!("automation activation operation changed during materialization");
            };
            *activation_binding = Some(resolved_binding);
            *activation_review = Some(resolved_review);
            continue;
        }

        let target =
            load_automation_operation_target(tx, space_id, &definition_key, &operation, version)
                .await?;
        if target.business_status != "active" {
            bail!(
                "automation definition operation requires a compound rollback because its exact business definition is no longer active"
            );
        }

        let (event_mapping_provider, event_mapping_key) = if let Some(mapping_id) =
            target.channel_event_mapping_id
        {
            if target.trigger_kind != "event" {
                bail!("schedule automation history contains an event mapping");
            }
            let row = sqlx::query(
                r#"
                    SELECT provider, definition_key, status
                    FROM qintopia_agent_os.channel_event_mapping_versions
                    WHERE id = $1
                    FOR SHARE
                    "#,
            )
            .bind(mapping_id)
            .fetch_optional(&mut **tx)
            .await
            .context("lock exact event mapping for automation definition operation")?
            .context("automation definition operation event mapping was not found")?;
            if row.try_get::<String, _>("status")? != "active" {
                bail!(
                        "automation definition operation requires a compound rollback because its exact event mapping is no longer active"
                    );
            }
            (
                Some(row.try_get::<String, _>("provider")?),
                Some(row.try_get::<String, _>("definition_key")?),
            )
        } else {
            if target.trigger_kind == "event" {
                bail!("event automation history is missing its event mapping");
            }
            (None, None)
        };

        intent.changes[index] = SpaceChange::AutomationDefinition {
            definition_key,
            status: if operation == "pause" {
                "paused".to_string()
            } else {
                "active".to_string()
            },
            source_stream_head_version: Some(target.stream_head_version),
            source_definition_version: Some(target.definition_version),
            business_definition_key: target.business_definition_key,
            business_definition_binding: None,
            trigger_kind: target.trigger_kind,
            trigger_config: target.trigger_config,
            timezone: target.timezone,
            misfire_policy: target.misfire_policy,
            event_mapping_provider,
            event_mapping_key,
            event_mapping_binding: None,
        };
    }
    Ok(())
}

async fn load_automation_operation_target(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    definition_key: &str,
    operation: &str,
    version: Option<i32>,
) -> Result<AutomationOperationTarget> {
    let row = if operation == "pause" {
        sqlx::query(
            r#"
            SELECT automation.version AS definition_version,
                   automation.channel_event_mapping_id, automation.trigger_kind,
                   automation.trigger_config, automation.timezone,
                   automation.misfire_policy, business.definition_key AS business_definition_key,
                   business.status AS business_status,
                   (SELECT MAX(head.version)
                    FROM qintopia_agent_os.automation_definition_versions head
                    WHERE head.space_id = automation.space_id
                      AND head.definition_key = automation.definition_key) AS stream_head_version
            FROM qintopia_agent_os.automation_definition_versions automation
            JOIN qintopia_agent_os.business_definition_versions business
              ON business.id = automation.business_definition_id
             AND business.space_id = automation.space_id
            WHERE automation.space_id = $1
              AND automation.definition_key = $2
              AND automation.status = 'active'
            FOR SHARE OF automation, business
            "#,
        )
        .bind(space_id)
        .bind(definition_key)
        .fetch_optional(&mut **tx)
        .await
        .context("load active automation definition to pause")?
        .context("current Space has no active automation definition with that key")?
    } else {
        sqlx::query(
            r#"
            SELECT automation.version AS definition_version,
                   automation.channel_event_mapping_id, automation.trigger_kind,
                   automation.trigger_config, automation.timezone,
                   automation.misfire_policy, business.definition_key AS business_definition_key,
                   business.status AS business_status,
                   (SELECT MAX(head.version)
                    FROM qintopia_agent_os.automation_definition_versions head
                    WHERE head.space_id = automation.space_id
                      AND head.definition_key = automation.definition_key) AS stream_head_version
            FROM qintopia_agent_os.automation_definition_versions automation
            JOIN qintopia_agent_os.business_definition_versions business
              ON business.id = automation.business_definition_id
             AND business.space_id = automation.space_id
            WHERE automation.space_id = $1
              AND automation.definition_key = $2
              AND automation.version = COALESCE(
                    $3::integer,
                    (
                        SELECT previous.version
                        FROM qintopia_agent_os.automation_definition_versions previous
                        WHERE previous.space_id = $1
                          AND previous.definition_key = $2
                        ORDER BY previous.version DESC
                        OFFSET 1 LIMIT 1
                    )
                  )
              AND automation.version < (
                    SELECT MAX(head.version)
                    FROM qintopia_agent_os.automation_definition_versions head
                    WHERE head.space_id = $1
                      AND head.definition_key = $2
                  )
            FOR SHARE OF automation, business
            "#,
        )
        .bind(space_id)
        .bind(definition_key)
        .bind(version)
        .fetch_optional(&mut **tx)
        .await
        .context("load historical automation definition to roll back")?
        .context("requested historical automation definition version was not found")?
    };

    Ok(AutomationOperationTarget {
        stream_head_version: row.try_get("stream_head_version")?,
        definition_version: row.try_get("definition_version")?,
        business_definition_key: row.try_get("business_definition_key")?,
        business_status: row.try_get("business_status")?,
        channel_event_mapping_id: row.try_get("channel_event_mapping_id")?,
        trigger_kind: row.try_get("trigger_kind")?,
        trigger_config: row.try_get("trigger_config")?,
        timezone: row.try_get("timezone")?,
        misfire_policy: row.try_get("misfire_policy")?,
    })
}

async fn load_automation_activation_target(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    definition_key: &str,
) -> Result<AutomationActivationTarget> {
    let preliminary = sqlx::query(
        r#"
        SELECT business.definition_key AS business_definition_key,
               automation.channel_event_mapping_id
        FROM qintopia_agent_os.automation_definition_versions automation
        JOIN qintopia_agent_os.business_definition_versions business
          ON business.id = automation.business_definition_id
         AND business.space_id = automation.space_id
        WHERE automation.space_id = $1
          AND automation.definition_key = $2
          AND automation.status = 'shadow'
          AND automation.version = (
              SELECT MAX(head.version)
              FROM qintopia_agent_os.automation_definition_versions head
              WHERE head.space_id = automation.space_id
                AND head.definition_key = automation.definition_key
          )
        "#,
    )
    .bind(space_id)
    .bind(definition_key)
    .fetch_optional(&mut **tx)
    .await
    .context("locate current shadow automation for activation")?
    .context("current Space has no latest shadow automation definition with that key")?;
    let business_definition_key: String = preliminary.try_get("business_definition_key")?;
    let channel_event_mapping_id: Option<Uuid> = preliminary.try_get("channel_event_mapping_id")?;
    let mapping_stream = if let Some(mapping_id) = channel_event_mapping_id {
        let row = sqlx::query(
            r#"
            SELECT provider, definition_key
            FROM qintopia_agent_os.channel_event_mapping_versions
            WHERE id = $1
            "#,
        )
        .bind(mapping_id)
        .fetch_optional(&mut **tx)
        .await
        .context("locate shadow automation event mapping stream")?
        .context("shadow automation event mapping was not found")?;
        Some((
            row.try_get::<String, _>("provider")?,
            row.try_get::<String, _>("definition_key")?,
        ))
    } else {
        None
    };

    // All definition writers use this order, so activation cannot deadlock with a
    // concurrent ordinary proposal while pinning the three source streams.
    if let Some((provider, mapping_key)) = &mapping_stream {
        lock_definition_stream(tx, &format!("event-mapping:{provider}:{mapping_key}")).await?;
    }
    lock_definition_stream(
        tx,
        &format!("business:{space_id}:{business_definition_key}"),
    )
    .await?;
    lock_definition_stream(tx, &format!("automation:{space_id}:{definition_key}")).await?;

    let automation_row = sqlx::query(
        r#"
        SELECT id, definition_key, version, definition_digest, status,
               business_definition_id, channel_event_mapping_id, trigger_kind,
               trigger_config, timezone, misfire_policy
        FROM qintopia_agent_os.automation_definition_versions automation
        WHERE space_id = $1
          AND definition_key = $2
          AND status = 'shadow'
          AND version = (
              SELECT MAX(head.version)
              FROM qintopia_agent_os.automation_definition_versions head
              WHERE head.space_id = automation.space_id
                AND head.definition_key = automation.definition_key
          )
        FOR SHARE OF automation
        "#,
    )
    .bind(space_id)
    .bind(definition_key)
    .fetch_optional(&mut **tx)
    .await
    .context("lock current shadow automation for activation")?
    .context("shadow automation changed while activation was being prepared")?;
    let automation_binding = ActivationSourceBinding {
        id: automation_row.try_get("id")?,
        definition_digest: automation_row.try_get("definition_digest")?,
        stream_head_version: automation_row.try_get("version")?,
        status: automation_row.try_get("status")?,
    };
    let business_definition_id: Uuid = automation_row.try_get("business_definition_id")?;
    let locked_mapping_id: Option<Uuid> = automation_row.try_get("channel_event_mapping_id")?;
    if locked_mapping_id != channel_event_mapping_id {
        bail!("shadow automation event mapping changed during activation binding");
    }

    let business_row = sqlx::query(
        r#"
        SELECT id, definition_key, version, definition_digest, status,
               execution_mode, definition, allowed_capabilities, approval_policy
        FROM qintopia_agent_os.business_definition_versions business
        WHERE id = $1
          AND space_id = $2
          AND definition_key = $3
          AND status IN ('shadow', 'active')
          AND version = (
              SELECT MAX(head.version)
              FROM qintopia_agent_os.business_definition_versions head
              WHERE head.space_id = business.space_id
                AND head.definition_key = business.definition_key
          )
        FOR SHARE OF business
        "#,
    )
    .bind(business_definition_id)
    .bind(space_id)
    .bind(&business_definition_key)
    .fetch_optional(&mut **tx)
    .await
    .context("lock exact shadow automation business definition")?
    .context("shadow automation business definition is no longer a current activatable version")?;
    let business_definition = ActivationBusinessTarget {
        binding: ActivationSourceBinding {
            id: business_row.try_get("id")?,
            definition_digest: business_row.try_get("definition_digest")?,
            stream_head_version: business_row.try_get("version")?,
            status: business_row.try_get("status")?,
        },
        definition_key: business_row.try_get("definition_key")?,
        execution_mode: business_row.try_get("execution_mode")?,
        definition: business_row.try_get("definition")?,
        allowed_capabilities: business_row.try_get("allowed_capabilities")?,
        approval_policy: business_row.try_get("approval_policy")?,
    };

    let event_mapping = if let Some(mapping_id) = locked_mapping_id {
        let (provider, mapping_key) = mapping_stream
            .as_ref()
            .context("event mapping stream disappeared during activation binding")?;
        let mapping_row = sqlx::query(
            r#"
            SELECT id, provider, definition_key, version, definition_digest, status,
                   selector, extractor, official_sources
            FROM qintopia_agent_os.channel_event_mapping_versions mapping
            WHERE id = $1
              AND provider = $2
              AND definition_key = $3
              AND status IN ('shadow', 'active')
              AND version = (
                  SELECT MAX(head.version)
                  FROM qintopia_agent_os.channel_event_mapping_versions head
                  WHERE head.provider = mapping.provider
                    AND head.definition_key = mapping.definition_key
              )
            FOR SHARE OF mapping
            "#,
        )
        .bind(mapping_id)
        .bind(provider)
        .bind(mapping_key)
        .fetch_optional(&mut **tx)
        .await
        .context("lock exact shadow automation event mapping")?
        .context("shadow automation event mapping is no longer a current activatable version")?;
        let official_sources: Value = mapping_row.try_get("official_sources")?;
        Some(ActivationEventMappingTarget {
            binding: ActivationSourceBinding {
                id: mapping_row.try_get("id")?,
                definition_digest: mapping_row.try_get("definition_digest")?,
                stream_head_version: mapping_row.try_get("version")?,
                status: mapping_row.try_get("status")?,
            },
            provider: mapping_row.try_get("provider")?,
            definition_key: mapping_row.try_get("definition_key")?,
            selector: mapping_row.try_get("selector")?,
            extractor: mapping_row.try_get("extractor")?,
            official_sources: serde_json::from_value(official_sources)
                .context("stored event mapping official_sources is invalid")?,
        })
    } else {
        None
    };

    Ok(AutomationActivationTarget {
        automation: ActivationAutomationTarget {
            binding: automation_binding,
            definition_key: automation_row.try_get("definition_key")?,
            trigger_kind: automation_row.try_get("trigger_kind")?,
            trigger_config: automation_row.try_get("trigger_config")?,
            timezone: automation_row.try_get("timezone")?,
            misfire_policy: automation_row.try_get("misfire_policy")?,
        },
        business_definition,
        event_mapping,
    })
}

async fn validate_automation_activation_target(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    target: &AutomationActivationTarget,
) -> Result<()> {
    validate_automation_activation_binding(&target.binding())?;
    validate_activation_execution_mode(&target.business_definition.execution_mode)?;
    validate_active_business_runtime_contract(
        tx,
        &target.business_definition.execution_mode,
        &target.business_definition.definition,
        &target.business_definition.approval_policy,
    )
    .await?;

    let validation_intent = SpaceChangeIntent {
        summary: "Validate the exact stored automation activation target.".to_string(),
        changes: {
            let mut changes = vec![SpaceChange::BusinessDefinition {
                definition_key: target.business_definition.definition_key.clone(),
                status: target.business_definition.binding.status.clone(),
                execution_mode: target.business_definition.execution_mode.clone(),
                definition: target.business_definition.definition.clone(),
                allowed_capabilities: target.business_definition.allowed_capabilities.clone(),
                approval_policy: target.business_definition.approval_policy.clone(),
            }];
            if let Some(mapping) = &target.event_mapping {
                changes.push(SpaceChange::ChannelEventMapping {
                    provider: mapping.provider.clone(),
                    definition_key: mapping.definition_key.clone(),
                    status: mapping.binding.status.clone(),
                    selector: mapping.selector.clone(),
                    extractor: mapping.extractor.clone(),
                    official_sources: mapping.official_sources.clone(),
                    validation_evidence: json!({}),
                });
            }
            changes.push(SpaceChange::AutomationDefinition {
                definition_key: target.automation.definition_key.clone(),
                status: "shadow".to_string(),
                source_stream_head_version: None,
                source_definition_version: None,
                business_definition_key: target.business_definition.definition_key.clone(),
                business_definition_binding: None,
                trigger_kind: target.automation.trigger_kind.clone(),
                trigger_config: target.automation.trigger_config.clone(),
                timezone: target.automation.timezone.clone(),
                misfire_policy: target.automation.misfire_policy.clone(),
                event_mapping_provider: target
                    .event_mapping
                    .as_ref()
                    .map(|mapping| mapping.provider.clone()),
                event_mapping_key: target
                    .event_mapping
                    .as_ref()
                    .map(|mapping| mapping.definition_key.clone()),
                event_mapping_binding: None,
            });
            changes
        },
    };
    parse_and_validate_intent(
        serde_json::to_value(&validation_intent)
            .context("encode stored automation activation target")?,
    )?;

    if target.business_definition.binding.status == "shadow" {
        let conflicting_active: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM qintopia_agent_os.business_definition_versions
                WHERE space_id = $1
                  AND definition_key = $2
                  AND status = 'active'
                  AND id <> $3
            )
            "#,
        )
        .bind(space_id)
        .bind(&target.business_definition.definition_key)
        .bind(target.business_definition.binding.id)
        .fetch_one(&mut **tx)
        .await
        .context("check activation business stream conflict")?;
        if conflicting_active {
            bail!("automation activation cannot replace a different active business definition");
        }
    }

    if let Some(mapping) = &target.event_mapping {
        if mapping.binding.status == "shadow" {
            let conflicting_active: bool = sqlx::query_scalar(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM qintopia_agent_os.channel_event_mapping_versions
                    WHERE provider = $1
                      AND definition_key = $2
                      AND status = 'active'
                      AND id <> $3
                )
                "#,
            )
            .bind(&mapping.provider)
            .bind(&mapping.definition_key)
            .bind(mapping.binding.id)
            .fetch_one(&mut **tx)
            .await
            .context("check activation event mapping stream conflict")?;
            if conflicting_active {
                bail!("automation activation cannot replace a different active provider event mapping");
            }
        }
        require_exact_same_space_automation_shadow_observation(
            tx,
            space_id,
            target.automation.binding.id,
            mapping.binding.id,
        )
        .await?;
    }

    let capability_intent = SpaceChangeIntent {
        summary: "Validate exact activation capability grants.".to_string(),
        changes: vec![SpaceChange::BusinessDefinition {
            definition_key: target.business_definition.definition_key.clone(),
            status: "active".to_string(),
            execution_mode: target.business_definition.execution_mode.clone(),
            definition: target.business_definition.definition.clone(),
            allowed_capabilities: target.business_definition.allowed_capabilities.clone(),
            approval_policy: target.business_definition.approval_policy.clone(),
        }],
    };
    validate_capability_references(tx, space_id, &capability_intent).await
}

fn validate_activation_execution_mode(execution_mode: &str) -> Result<()> {
    let agent_turn_runtime_ready = if execution_mode == "agent_turn" {
        crate::space_agent_turn::runtime_readiness()?
    } else {
        false
    };
    validate_activation_execution_mode_with_readiness(execution_mode, agent_turn_runtime_ready)
}

fn validate_activation_execution_mode_with_readiness(
    execution_mode: &str,
    agent_turn_runtime_ready: bool,
) -> Result<()> {
    match execution_mode {
        "deterministic" => Ok(()),
        "agent_turn" if agent_turn_runtime_ready => Ok(()),
        "agent_turn" => {
            bail!("agent_turn activation requires owner-reviewed broker and runner readiness")
        }
        _ => bail!("automation activation execution mode is not registered"),
    }
}

async fn require_exact_same_space_automation_shadow_observation(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    automation_id: Uuid,
    mapping_id: Uuid,
) -> Result<()> {
    let observed: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM qintopia_agent_os.automation_definition_versions automation
            JOIN qintopia_agent_os.channel_event_mapping_versions mapping
              ON mapping.id = automation.channel_event_mapping_id
             AND mapping.id = $3
             AND mapping.status IN ('shadow', 'active')
            JOIN qintopia_agent_os.work_items observation
              ON observation.space_id = automation.space_id
             AND observation.work_item_type = 'space_event_shadow_observation'
             AND observation.status = 'completed'
             AND observation.requester_agent = 'system'
             AND observation.capability_key = 'erhua.execute_space_business'
             AND observation.source_type = 'space_event_shadow'
             AND observation.source_refs ->> 'mapping_version_id' = mapping.id::text
             AND observation.payload ->> 'decode_success' = 'true'
             AND observation.metadata ->> 'space_bound' = 'true'
             AND observation.metadata ->> 'scope'
                 = 'automation_shadow:' || automation.id::text
            JOIN qintopia_messages.raw_events raw_event
              ON raw_event.id::text = observation.source_refs ->> 'raw_event_id'
             AND raw_event.space_id = automation.space_id
             AND raw_event.source = mapping.provider
             AND raw_event.ingress_auth_verified
             AND raw_event.created_at > GREATEST(automation.created_at, mapping.created_at)
            WHERE automation.id = $2
              AND automation.space_id = $1
              AND automation.trigger_kind = 'event'
              AND automation.status = 'shadow'
        )
        "#,
    )
    .bind(space_id)
    .bind(automation_id)
    .bind(mapping_id)
    .fetch_one(&mut **tx)
    .await
    .context("verify exact same-Space automation shadow observation")?;
    if !observed {
        bail!(
            "automation activation requires a completed authenticated observation for the exact current-Space shadow version"
        );
    }
    Ok(())
}

async fn materialize_trusted_intent(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    intent: &mut SpaceChangeIntent,
) -> Result<()> {
    materialize_definition_operations(tx, space_id, intent).await?;
    reuse_active_event_mappings(tx, intent).await?;
    merge_active_policy_config(tx, space_id, intent).await?;

    for change in &mut intent.changes {
        let SpaceChange::ChannelEventMapping {
            provider,
            definition_key,
            status,
            selector,
            extractor,
            validation_evidence,
            ..
        } = change
        else {
            continue;
        };

        let mut evidence = crate::channel_event_mapping::replay_registered_fixtures(
            provider, selector, extractor,
        )?;
        let stream_head: Option<i32> = sqlx::query_scalar(
            r#"
            SELECT MAX(version)
            FROM qintopia_agent_os.channel_event_mapping_versions
            WHERE provider = $1 AND definition_key = $2
            "#,
        )
        .bind(provider.as_str())
        .bind(definition_key.as_str())
        .fetch_one(&mut **tx)
        .await
        .context("load provider event mapping stream head")?;
        evidence["provider_stream_head_version"] = json!(stream_head.unwrap_or(0));

        if status == "active" {
            let shadow = sqlx::query(
                r#"
                SELECT mapping.id, mapping.version
                FROM qintopia_agent_os.channel_event_mapping_versions mapping
                JOIN qintopia_agent_os.work_items observation
                  ON observation.space_id = $5
                 AND observation.work_item_type = 'space_event_shadow_observation'
                 AND observation.status = 'completed'
                 AND observation.requester_agent = 'system'
                 AND observation.capability_key = 'erhua.execute_space_business'
                 AND observation.source_type = 'space_event_shadow'
                 AND observation.source_refs ->> 'mapping_version_id' = mapping.id::text
                 AND observation.payload ->> 'decode_success' = 'true'
                 AND observation.metadata ->> 'space_bound' = 'true'
                 AND observation.metadata ->> 'scope' = 'mapping_shadow'
                JOIN qintopia_messages.raw_events raw_event
                  ON raw_event.id::text = observation.source_refs ->> 'raw_event_id'
                 AND raw_event.space_id = $5
                 AND raw_event.source = mapping.provider
                 AND raw_event.ingress_auth_verified
                 AND raw_event.created_at > mapping.created_at
                WHERE mapping.provider = $1
                  AND mapping.definition_key = $2
                  AND mapping.status = 'shadow'
                  AND mapping.selector = $3
                  AND mapping.extractor = $4
                ORDER BY mapping.version ASC
                LIMIT 1
                "#,
            )
            .bind(provider.as_str())
            .bind(definition_key.as_str())
            .bind(selector.clone())
            .bind(extractor.clone())
            .bind(space_id)
            .fetch_optional(&mut **tx)
            .await
            .context("verify same-Space provider event shadow observation")?
            .context(
                "active event mapping requires a matching shadow version and completed same-Space real-event observation",
            )?;
            evidence["real_event_verified"] = json!(true);
            evidence["real_event_evidence_source"] = json!("same_space_shadow_observation");
            evidence["shadow_mapping_version_id"] = json!(shadow.try_get::<Uuid, _>("id")?);
            evidence["shadow_mapping_version"] = json!(shadow.try_get::<i32, _>("version")?);
        }
        *validation_evidence = evidence;
    }

    if intent.changes.is_empty() {
        bail!("Space change proposal has no changes after reusing active provider mappings");
    }
    materialize_automation_dependency_bindings(tx, space_id, intent).await?;
    validate_dependency_transitions(tx, space_id, intent).await?;
    Ok(())
}

async fn materialize_automation_dependency_bindings(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    intent: &mut SpaceChangeIntent,
) -> Result<()> {
    let mut proposed_businesses = HashMap::new();
    let mut proposed_mappings = HashMap::new();
    for change in &intent.changes {
        match change {
            SpaceChange::BusinessDefinition { definition_key, .. } => {
                proposed_businesses.insert(definition_key.clone(), definition_digest(change)?);
            }
            SpaceChange::ChannelEventMapping {
                provider,
                definition_key,
                ..
            } => {
                proposed_mappings.insert(
                    (provider.clone(), definition_key.clone()),
                    definition_digest(change)?,
                );
            }
            _ => {}
        }
    }

    for change in &mut intent.changes {
        let SpaceChange::AutomationDefinition {
            definition_key,
            source_stream_head_version,
            business_definition_key,
            business_definition_binding,
            trigger_kind,
            event_mapping_provider,
            event_mapping_key,
            event_mapping_binding,
            ..
        } = change
        else {
            continue;
        };

        if let Some(expected_head) = *source_stream_head_version {
            lock_definition_stream(tx, &format!("automation:{space_id}:{definition_key}")).await?;
            let current_head: Option<i32> = sqlx::query_scalar(
                r#"
                SELECT MAX(version)
                FROM qintopia_agent_os.automation_definition_versions
                WHERE space_id = $1 AND definition_key = $2
                "#,
            )
            .bind(space_id)
            .bind(definition_key.as_str())
            .fetch_one(&mut **tx)
            .await
            .context("verify automation definition operation stream head")?;
            if current_head != Some(expected_head) {
                bail!("automation definition changed after prepare; create a new proposal");
            }
        }

        *business_definition_binding =
            if let Some(digest) = proposed_businesses.get(business_definition_key) {
                Some(proposal_definition_binding(digest))
            } else {
                Some(load_existing_business_binding(tx, space_id, business_definition_key).await?)
            };

        *event_mapping_binding = if trigger_kind == "event" {
            let provider = event_mapping_provider
                .as_deref()
                .context("event automation provider is missing")?;
            let definition_key = event_mapping_key
                .as_deref()
                .context("event automation mapping key is missing")?;
            if let Some(digest) =
                proposed_mappings.get(&(provider.to_string(), definition_key.to_string()))
            {
                Some(proposal_definition_binding(digest))
            } else {
                Some(load_existing_mapping_binding(tx, provider, definition_key).await?)
            }
        } else {
            None
        };
    }
    validate_active_automation_runtime_contracts(tx, space_id, intent).await
}

async fn validate_active_automation_runtime_contracts(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    intent: &SpaceChangeIntent,
) -> Result<()> {
    for change in &intent.changes {
        let SpaceChange::AutomationDefinition {
            status,
            business_definition_key,
            business_definition_binding,
            ..
        } = change
        else {
            continue;
        };
        if status != "active" {
            continue;
        }

        let binding = business_definition_binding
            .as_ref()
            .context("active automation business definition binding is missing")?;
        let (execution_mode, definition, approval_policy) = if binding.source == "proposal" {
            let proposed_business = intent
                .changes
                .iter()
                .find(|candidate| {
                    matches!(
                        candidate,
                        SpaceChange::BusinessDefinition { definition_key, .. }
                            if definition_key == business_definition_key
                    )
                })
                .context("active automation proposal-bound business definition is missing")?;
            let SpaceChange::BusinessDefinition {
                status,
                execution_mode,
                definition,
                approval_policy,
                ..
            } = proposed_business
            else {
                bail!("active automation proposal-bound business definition is invalid");
            };
            if status != "active" {
                bail!("active automation requires an active business definition");
            }
            if definition_digest(proposed_business)? != binding.definition_digest {
                bail!("active automation proposal-bound business definition digest changed");
            }
            (
                execution_mode.clone(),
                definition.clone(),
                approval_policy.clone(),
            )
        } else if binding.source == "existing" {
            let bound_id = binding
                .id
                .context("active automation existing business definition id is missing")?;
            let stream_head_version = binding
                .stream_head_version
                .context("active automation existing business definition head is missing")?;
            let row = sqlx::query(
                r#"
                SELECT bound.execution_mode, bound.definition, bound.approval_policy
                FROM qintopia_agent_os.business_definition_versions bound
                WHERE bound.id = $1
                  AND bound.space_id = $2
                  AND bound.definition_key = $3
                  AND bound.definition_digest = $4
                  AND bound.status = 'active'
                  AND (SELECT MAX(head.version)
                       FROM qintopia_agent_os.business_definition_versions head
                       WHERE head.space_id = bound.space_id
                         AND head.definition_key = bound.definition_key) = $5
                FOR SHARE OF bound
                "#,
            )
            .bind(bound_id)
            .bind(space_id)
            .bind(business_definition_key)
            .bind(&binding.definition_digest)
            .bind(stream_head_version)
            .fetch_optional(&mut **tx)
            .await
            .context("load active automation business runtime contract")?
            .context("active automation business definition binding changed")?;
            (
                row.try_get("execution_mode")?,
                row.try_get("definition")?,
                row.try_get("approval_policy")?,
            )
        } else {
            bail!("active automation business definition binding source is invalid");
        };

        validate_active_business_runtime_contract(
            tx,
            &execution_mode,
            &definition,
            &approval_policy,
        )
        .await?;
    }
    Ok(())
}

fn proposal_definition_binding(definition_digest: &str) -> DefinitionBinding {
    DefinitionBinding {
        source: "proposal".to_string(),
        definition_digest: definition_digest.to_string(),
        id: None,
        stream_head_version: None,
    }
}

async fn load_existing_business_binding(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    definition_key: &str,
) -> Result<DefinitionBinding> {
    let row = sqlx::query(
        r#"
        SELECT active.id, active.definition_digest,
               (SELECT MAX(version)
                FROM qintopia_agent_os.business_definition_versions head
                WHERE head.space_id = active.space_id
                  AND head.definition_key = active.definition_key) AS stream_head_version
        FROM qintopia_agent_os.business_definition_versions active
        WHERE active.space_id = $1
          AND active.definition_key = $2
          AND active.status = 'active'
        "#,
    )
    .bind(space_id)
    .bind(definition_key)
    .fetch_optional(&mut **tx)
    .await
    .context("bind existing automation business definition")?
    .context("automation business definition is not active or included in the proposal")?;
    Ok(DefinitionBinding {
        source: "existing".to_string(),
        definition_digest: row.try_get("definition_digest")?,
        id: Some(row.try_get("id")?),
        stream_head_version: Some(row.try_get("stream_head_version")?),
    })
}

async fn load_existing_mapping_binding(
    tx: &mut Transaction<'_, Postgres>,
    provider: &str,
    definition_key: &str,
) -> Result<DefinitionBinding> {
    let row = sqlx::query(
        r#"
        SELECT active.id, active.definition_digest,
               (SELECT MAX(version)
                FROM qintopia_agent_os.channel_event_mapping_versions head
                WHERE head.provider = active.provider
                  AND head.definition_key = active.definition_key) AS stream_head_version
        FROM qintopia_agent_os.channel_event_mapping_versions active
        WHERE active.provider = $1
          AND active.definition_key = $2
          AND active.status = 'active'
        "#,
    )
    .bind(provider)
    .bind(definition_key)
    .fetch_optional(&mut **tx)
    .await
    .context("bind existing automation event mapping")?
    .context("automation event mapping is not active or included in the proposal")?;
    Ok(DefinitionBinding {
        source: "existing".to_string(),
        definition_digest: row.try_get("definition_digest")?,
        id: Some(row.try_get("id")?),
        stream_head_version: Some(row.try_get("stream_head_version")?),
    })
}

async fn validate_dependency_transitions(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    intent: &SpaceChangeIntent,
) -> Result<()> {
    let automation_transitions = intent
        .changes
        .iter()
        .filter_map(|change| match change {
            SpaceChange::AutomationDefinition {
                definition_key,
                status,
                ..
            } => Some((definition_key.as_str(), status.as_str())),
            _ => None,
        })
        .collect::<HashMap<_, _>>();

    for change in &intent.changes {
        let SpaceChange::BusinessDefinition {
            definition_key,
            status,
            ..
        } = change
        else {
            continue;
        };
        if !supersedes_active(status) {
            continue;
        }
        let active_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT id
            FROM qintopia_agent_os.business_definition_versions
            WHERE space_id = $1 AND definition_key = $2 AND status = 'active'
            FOR UPDATE
            "#,
        )
        .bind(space_id)
        .bind(definition_key)
        .fetch_optional(&mut **tx)
        .await
        .context("load active business dependency before replacement")?;
        if let Some(active_id) = active_id {
            require_dependent_automation_transitions(
                tx,
                space_id,
                "business definition",
                active_id,
                None,
                &automation_transitions,
            )
            .await?;
        }
    }

    for change in &intent.changes {
        let SpaceChange::ChannelEventMapping {
            provider,
            definition_key,
            status,
            ..
        } = change
        else {
            continue;
        };
        if !supersedes_active(status) {
            continue;
        }
        let active_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT id
            FROM qintopia_agent_os.channel_event_mapping_versions
            WHERE provider = $1 AND definition_key = $2 AND status = 'active'
            FOR UPDATE
            "#,
        )
        .bind(provider)
        .bind(definition_key)
        .fetch_optional(&mut **tx)
        .await
        .context("load active mapping dependency before replacement")?;
        if let Some(active_id) = active_id {
            require_dependent_automation_transitions(
                tx,
                space_id,
                "provider event mapping",
                active_id,
                Some(active_id),
                &automation_transitions,
            )
            .await?;
        }
    }
    Ok(())
}

async fn require_dependent_automation_transitions(
    tx: &mut Transaction<'_, Postgres>,
    proposal_space_id: Uuid,
    dependency_name: &str,
    business_definition_id: Uuid,
    channel_event_mapping_id: Option<Uuid>,
    automation_transitions: &HashMap<&str, &str>,
) -> Result<()> {
    let rows = if channel_event_mapping_id.is_some() {
        sqlx::query(
            r#"
            SELECT space_id, definition_key, status
            FROM qintopia_agent_os.automation_definition_versions
            WHERE channel_event_mapping_id = $1
              AND status IN ('active', 'shadow')
            "#,
        )
        .bind(channel_event_mapping_id)
        .fetch_all(&mut **tx)
        .await
        .context("load automations that depend on provider event mapping")?
    } else {
        sqlx::query(
            r#"
            SELECT space_id, definition_key, status
            FROM qintopia_agent_os.automation_definition_versions
            WHERE business_definition_id = $1
              AND status IN ('active', 'shadow')
            "#,
        )
        .bind(business_definition_id)
        .fetch_all(&mut **tx)
        .await
        .context("load automations that depend on business definition")?
    };

    for row in rows {
        let dependent_space_id: Uuid = row.try_get("space_id")?;
        let definition_key: String = row.try_get("definition_key")?;
        let current_status: String = row.try_get("status")?;
        if dependent_space_id != proposal_space_id {
            bail!("cannot replace {dependency_name} while another Space still references it");
        }
        let proposed_status = automation_transitions.get(definition_key.as_str()).copied();
        let covered = automation_transition_covers(current_status.as_str(), proposed_status);
        if !covered {
            bail!(
                "cannot replace {dependency_name} while active or shadow automation {definition_key} still references it"
            );
        }
    }
    Ok(())
}

fn automation_transition_covers(current_status: &str, proposed_status: Option<&str>) -> bool {
    matches!(
        (current_status, proposed_status),
        ("active", Some("active" | "paused" | "retired"))
            | ("shadow", Some("shadow" | "active" | "paused" | "retired"))
    )
}

async fn reuse_active_event_mappings(
    tx: &mut Transaction<'_, Postgres>,
    intent: &mut SpaceChangeIntent,
) -> Result<()> {
    let mut reused = Vec::new();
    for (index, change) in intent.changes.iter().enumerate() {
        let SpaceChange::ChannelEventMapping {
            provider,
            definition_key,
            status,
            selector,
            extractor,
            ..
        } = change
        else {
            continue;
        };
        if !matches!(status.as_str(), "shadow" | "active") {
            continue;
        }
        let active_exists: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM qintopia_agent_os.channel_event_mapping_versions
                WHERE provider = $1
                  AND definition_key = $2
                  AND status = 'active'
                  AND selector = $3
                  AND extractor = $4
            )
            "#,
        )
        .bind(provider.as_str())
        .bind(definition_key.as_str())
        .bind(selector.clone())
        .bind(extractor.clone())
        .fetch_one(&mut **tx)
        .await
        .context("look up reusable active provider event mapping")?;
        if active_exists {
            reused.push(index);
        }
    }
    for index in reused.into_iter().rev() {
        intent.changes.remove(index);
    }
    Ok(())
}

async fn merge_active_policy_config(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    intent: &mut SpaceChangeIntent,
) -> Result<()> {
    let current_policy = sqlx::query_scalar::<_, Value>(
        r#"
        SELECT policy_config
        FROM qintopia_agent_os.space_policy_versions
        WHERE space_id = $1 AND definition_key = 'default' AND status = 'active'
        "#,
    )
    .bind(space_id)
    .fetch_optional(&mut **tx)
    .await
    .context("load active Space policy for additive materialization")?;
    let current_policy = current_policy.unwrap_or_else(|| json!({}));

    for change in &mut intent.changes {
        let SpaceChange::SpacePolicy {
            status,
            policy_config,
            ..
        } = change
        else {
            continue;
        };
        if status != "active" {
            continue;
        }
        *policy_config = merge_policy_configs(&current_policy, policy_config)?;
        validate_policy_config(policy_config)?;
        validate_json_tree(policy_config, 0)?;
    }
    Ok(())
}

fn merge_policy_configs(current: &Value, proposed: &Value) -> Result<Value> {
    let current = current
        .as_object()
        .context("stored active Space policy must be an object")?;
    let proposed = proposed
        .as_object()
        .context("proposed active Space policy must be an object")?;
    let mut merged = current.clone();
    for (key, value) in proposed {
        merged.insert(key.clone(), value.clone());
    }

    let mut current_grants = Vec::new();
    if let Some(values) = current.get("capability_grants") {
        current_grants = values
            .as_array()
            .context("stored Space policy capability_grants must be an array")?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(ToString::to_string)
                    .context("stored Space policy capability grant must be a string")
            })
            .collect::<Result<Vec<_>>>()?;
        normalize_capabilities(&mut current_grants)?;
    }

    let mut revocations = proposed
        .get("capability_revocations")
        .map(|values| {
            values
                .as_array()
                .context("proposed Space policy capability_revocations must be an array")?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(ToString::to_string)
                        .context("proposed Space policy capability revocation must be a string")
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    normalize_capabilities(&mut revocations)?;
    if revocations
        .iter()
        .any(|revocation| !current_grants.iter().any(|grant| grant == revocation))
    {
        bail!("Space policy can revoke only a currently active capability grant");
    }

    let mut proposed_grants = proposed
        .get("capability_grants")
        .map(|values| {
            values
                .as_array()
                .context("proposed Space policy capability_grants must be an array")?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(ToString::to_string)
                        .context("proposed Space policy capability grant must be a string")
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    normalize_capabilities(&mut proposed_grants)?;
    if proposed_grants
        .iter()
        .any(|grant| revocations.iter().any(|revocation| revocation == grant))
    {
        bail!("Space policy cannot grant and revoke the same capability");
    }

    let grants_declared = current.contains_key("capability_grants")
        || proposed.contains_key("capability_grants")
        || !revocations.is_empty();
    if grants_declared {
        let mut grants = current_grants;
        grants.extend(proposed_grants);
        normalize_capabilities(&mut grants)?;
        grants.retain(|grant| !revocations.iter().any(|revocation| revocation == grant));
        merged.insert("capability_grants".to_string(), json!(grants));
    }
    merged.remove("capability_revocations");
    Ok(Value::Object(merged))
}

async fn validate_capability_references(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    intent: &SpaceChangeIntent,
) -> Result<()> {
    let proposed_policy = intent.changes.iter().find_map(|change| match change {
        SpaceChange::SpacePolicy {
            status,
            policy_config,
            ..
        } if status == "active" => Some(policy_config.clone()),
        SpaceChange::SpacePolicy { status, .. }
            if matches!(status.as_str(), "paused" | "retired") =>
        {
            Some(json!({"capability_grants": []}))
        }
        _ => None,
    });
    let policy_config = if let Some(policy) = proposed_policy {
        Some(policy)
    } else {
        sqlx::query_scalar::<_, Value>(
            r#"
            SELECT policy_config
            FROM qintopia_agent_os.space_policy_versions
            WHERE space_id = $1 AND definition_key = 'default' AND status = 'active'
            "#,
        )
        .bind(space_id)
        .fetch_optional(&mut **tx)
        .await
        .context("load active Space policy capability ceiling")?
    };
    let grants = policy_config
        .as_ref()
        .and_then(|policy| policy.get("capability_grants"))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    for capability in &grants {
        let registered: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM qintopia_agent_os.capabilities
                WHERE capability_key = $1
                  AND provider_agent = 'erhua'
                  AND (
                        (
                            metadata ->> 'space_invocable' = 'true'
                            AND metadata ->> 'space_scope_binding' = 'work_item_space_id'
                        )
                        OR (
                            metadata ->> 'space_turn_invocable' = 'true'
                            AND metadata ->> 'space_scope_binding'
                                = 'trusted_session_space_id'
                        )
                      )
            )
            "#,
        )
        .bind(capability)
        .fetch_one(&mut **tx)
        .await
        .context("validate Space policy capability grant registration")?;
        if !registered {
            bail!("Space policy grants an unregistered Space capability");
        }
    }

    for change in &intent.changes {
        let SpaceChange::BusinessDefinition {
            allowed_capabilities,
            execution_mode,
            status,
            ..
        } = change
        else {
            continue;
        };
        if matches!(status.as_str(), "paused" | "retired") {
            continue;
        }
        for capability in allowed_capabilities {
            let registration = sqlx::query(
                r#"
                SELECT provider_agent, allowed_callers, allowed_work_item_types, metadata
                FROM qintopia_agent_os.capabilities
                WHERE capability_key = $1
                  AND metadata ->> 'space_invocable' = 'true'
                  AND metadata ->> 'space_scope_binding' = 'work_item_space_id'
                "#,
            )
            .bind(capability)
            .fetch_optional(&mut **tx)
            .await
            .context("validate business capability registration")?
            .context(
                "business definition references a capability without the Space invocation contract",
            )?;
            let provider_agent: String = registration.try_get("provider_agent")?;
            let allowed_callers: Vec<String> = registration.try_get("allowed_callers")?;
            let allowed_work_item_types: Vec<String> =
                registration.try_get("allowed_work_item_types")?;
            let metadata: Value = registration.try_get("metadata")?;
            if provider_agent != "erhua" {
                bail!("business capability belongs to a different provider Agent");
            }
            let invocation_boundary = metadata
                .get("invocation_boundary")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let registered_for_mode = match execution_mode.as_str() {
                "deterministic" => {
                    allowed_callers.iter().any(|caller| caller == "system")
                        && allowed_work_item_types
                            .iter()
                            .any(|kind| kind == SPACE_AUTOMATION_WORK_ITEM_TYPE)
                        && invocation_boundary == EXECUTION_CAPABILITY_KEY
                        && crate::space_capability_recipe::is_registered_metadata(&metadata)
                }
                "agent_turn" if capability == AGENT_TURN_CAPABILITY_KEY => {
                    allowed_callers.iter().any(|caller| caller == "system")
                        && allowed_work_item_types
                            .iter()
                            .any(|kind| kind == SPACE_AGENT_TURN_WORK_ITEM_TYPE)
                        && invocation_boundary == EXECUTION_CAPABILITY_KEY
                }
                "agent_turn" => {
                    allowed_callers.iter().any(|caller| caller == "erhua")
                        && allowed_work_item_types
                            .iter()
                            .any(|kind| kind == SPACE_AGENT_TURN_WORK_ITEM_TYPE)
                        && invocation_boundary == AGENT_TURN_CAPABILITY_KEY
                        && metadata.get("runner_access").and_then(Value::as_str)
                            == Some("bounded_catalog_v1")
                }
                _ => false,
            };
            if !registered_for_mode {
                bail!("business capability is not registered for this execution boundary");
            }
            if !grants.iter().any(|grant| grant == capability) {
                bail!("business capability exceeds the active Space policy grant ceiling");
            }
        }
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "audit event columns remain explicit at the transactional write boundary"
)]
async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    work_item_id: Uuid,
    artifact_id: Option<Uuid>,
    event_type: &str,
    actor_type: &str,
    actor_id: &str,
    message: &str,
    data: Value,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_item_events
            (work_item_id, artifact_id, event_type, actor_type, actor_id, message, data)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(work_item_id)
    .bind(artifact_id)
    .bind(event_type)
    .bind(actor_type)
    .bind(actor_id)
    .bind(message)
    .bind(data)
    .execute(&mut **tx)
    .await
    .context("append Space configuration audit event")?;
    Ok(())
}

fn has_activation_operation(intent: &SpaceChangeIntent) -> bool {
    intent.changes.iter().any(|change| {
        matches!(
            change,
            SpaceChange::DefinitionOperation { operation, .. } if operation == "activate"
        )
    })
}

async fn expand_activation_operations_for_apply(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    intent: &SpaceChangeIntent,
) -> Result<SpaceChangeIntent> {
    let mut changes = Vec::with_capacity(3);
    for change in &intent.changes {
        let SpaceChange::DefinitionOperation {
            definition_key,
            operation,
            activation_binding,
            activation_review,
            ..
        } = change
        else {
            changes.push(change.clone());
            continue;
        };
        if operation != "activate" {
            bail!("unmaterialized definition operation reached activation apply");
        }
        let expected_binding = activation_binding
            .as_ref()
            .context("automation activation binding is missing at apply")?;
        let target = load_automation_activation_target(tx, space_id, definition_key).await?;
        validate_automation_activation_target(tx, space_id, &target).await?;
        if &target.binding() != expected_binding {
            bail!("automation activation dependencies changed after confirmation");
        }
        if activation_review.as_ref() != Some(&target.review()?) {
            bail!("automation activation review changed after confirmation");
        }

        if let Some(mapping) = &target.event_mapping {
            if mapping.binding.status == "shadow" {
                let mut evidence = crate::channel_event_mapping::replay_registered_fixtures(
                    &mapping.provider,
                    &mapping.selector,
                    &mapping.extractor,
                )?;
                evidence["provider_stream_head_version"] =
                    json!(mapping.binding.stream_head_version);
                evidence["real_event_verified"] = json!(true);
                evidence["real_event_evidence_source"] =
                    json!("exact_same_space_automation_shadow_observation");
                evidence["shadow_mapping_version_id"] = json!(mapping.binding.id);
                evidence["shadow_mapping_version"] = json!(mapping.binding.stream_head_version);
                changes.push(SpaceChange::ChannelEventMapping {
                    provider: mapping.provider.clone(),
                    definition_key: mapping.definition_key.clone(),
                    status: "active".to_string(),
                    selector: mapping.selector.clone(),
                    extractor: mapping.extractor.clone(),
                    official_sources: mapping.official_sources.clone(),
                    validation_evidence: evidence,
                });
            }
        }

        if target.business_definition.binding.status == "shadow" {
            changes.push(SpaceChange::BusinessDefinition {
                definition_key: target.business_definition.definition_key.clone(),
                status: "active".to_string(),
                execution_mode: target.business_definition.execution_mode.clone(),
                definition: target.business_definition.definition.clone(),
                allowed_capabilities: target.business_definition.allowed_capabilities.clone(),
                approval_policy: target.business_definition.approval_policy.clone(),
            });
        }

        changes.push(SpaceChange::AutomationDefinition {
            definition_key: target.automation.definition_key.clone(),
            status: "active".to_string(),
            source_stream_head_version: Some(target.automation.binding.stream_head_version),
            source_definition_version: Some(target.automation.binding.stream_head_version),
            business_definition_key: target.business_definition.definition_key.clone(),
            business_definition_binding: None,
            trigger_kind: target.automation.trigger_kind.clone(),
            trigger_config: target.automation.trigger_config.clone(),
            timezone: target.automation.timezone.clone(),
            misfire_policy: target.automation.misfire_policy.clone(),
            event_mapping_provider: target
                .event_mapping
                .as_ref()
                .map(|mapping| mapping.provider.clone()),
            event_mapping_key: target
                .event_mapping
                .as_ref()
                .map(|mapping| mapping.definition_key.clone()),
            event_mapping_binding: None,
        });
    }
    if changes.len() > MAX_CHANGES {
        bail!("automation activation expands beyond the change limit");
    }
    let mut expanded = SpaceChangeIntent {
        summary: intent.summary.clone(),
        changes,
    };
    materialize_automation_dependency_bindings(tx, space_id, &mut expanded).await?;
    validate_dependency_transitions(tx, space_id, &expanded).await?;
    Ok(expanded)
}

async fn apply_intent(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    actor_person_id: Uuid,
    work_item_id: Uuid,
    intent: &SpaceChangeIntent,
) -> Result<Vec<AppliedDefinition>> {
    let exact_activation = has_activation_operation(intent);
    let expanded_intent;
    let intent = if exact_activation {
        expanded_intent = expand_activation_operations_for_apply(tx, space_id, intent).await?;
        &expanded_intent
    } else {
        intent
    };
    validate_capability_references(tx, space_id, intent).await?;
    validate_active_automation_runtime_contracts(tx, space_id, intent).await?;
    validate_dependency_transitions(tx, space_id, intent).await?;
    let mut applied = Vec::with_capacity(intent.changes.len());
    let mut mapping_refs = HashMap::<(String, String), (Uuid, String, String)>::new();
    let mut business_refs = HashMap::<String, (Uuid, String, String)>::new();

    for change in &intent.changes {
        if let SpaceChange::SpacePolicy {
            definition_key,
            status,
            policy_config,
        } = change
        {
            let version = apply_space_policy(
                tx,
                space_id,
                actor_person_id,
                work_item_id,
                definition_key,
                status,
                policy_config,
                definition_digest(change)?,
            )
            .await?;
            applied.push(AppliedDefinition {
                resource: "space_policy",
                definition_key: definition_key.clone(),
                version,
                status: status.clone(),
            });
        }
    }

    for change in &intent.changes {
        if let SpaceChange::ChannelEventMapping {
            provider,
            definition_key,
            status,
            selector,
            extractor,
            official_sources,
            validation_evidence,
        } = change
        {
            let digest = definition_digest(change)?;
            let (id, version) = apply_event_mapping(
                tx,
                actor_person_id,
                work_item_id,
                provider,
                definition_key,
                status,
                selector,
                extractor,
                official_sources,
                validation_evidence,
                digest.clone(),
            )
            .await?;
            mapping_refs.insert(
                (provider.clone(), definition_key.clone()),
                (id, status.clone(), digest),
            );
            applied.push(AppliedDefinition {
                resource: "channel_event_mapping",
                definition_key: definition_key.clone(),
                version,
                status: status.clone(),
            });
        }
    }

    for change in &intent.changes {
        if let SpaceChange::BusinessDefinition {
            definition_key,
            status,
            execution_mode,
            definition,
            allowed_capabilities,
            approval_policy,
        } = change
        {
            let digest = definition_digest(change)?;
            let (id, version) = apply_business_definition(
                tx,
                space_id,
                actor_person_id,
                work_item_id,
                definition_key,
                status,
                execution_mode,
                definition,
                allowed_capabilities,
                approval_policy,
                digest.clone(),
            )
            .await?;
            business_refs.insert(definition_key.clone(), (id, status.clone(), digest));
            applied.push(AppliedDefinition {
                resource: "business_definition",
                definition_key: definition_key.clone(),
                version,
                status: status.clone(),
            });
        }
    }

    for change in &intent.changes {
        if let SpaceChange::AutomationDefinition {
            definition_key,
            status,
            source_stream_head_version,
            source_definition_version,
            business_definition_key,
            business_definition_binding,
            trigger_kind,
            trigger_config,
            timezone,
            misfire_policy,
            event_mapping_provider,
            event_mapping_key,
            event_mapping_binding,
            ..
        } = change
        {
            let business = resolve_bound_business_reference(
                tx,
                space_id,
                business_definition_key,
                business_definition_binding
                    .as_ref()
                    .context("automation business definition binding is missing")?,
                &business_refs,
            )
            .await?;
            if status == "active" && business.1 != "active" {
                bail!("active automation requires an active business definition");
            }
            if status == "shadow" && !matches!(business.1.as_str(), "shadow" | "active") {
                bail!("shadow automation requires an active or shadow business definition");
            }
            let mut rollback_lineage = None;
            let event_mapping = if trigger_kind == "event" {
                let provider = event_mapping_provider
                    .as_deref()
                    .context("event automation provider is missing")?;
                let mapping_key = event_mapping_key
                    .as_deref()
                    .context("event automation mapping key is missing")?;
                let mapping = resolve_bound_mapping_reference(
                    tx,
                    provider,
                    mapping_key,
                    event_mapping_binding
                        .as_ref()
                        .context("automation event mapping binding is missing")?,
                    &mapping_refs,
                )
                .await?;
                if status == "active" && mapping.1 != "active" {
                    bail!("active event automation requires an active event mapping");
                }
                if status == "shadow" && !matches!(mapping.1.as_str(), "shadow" | "active") {
                    bail!("shadow event automation requires an active or shadow event mapping");
                }
                if status == "active" {
                    if exact_activation {
                        // The exact activation target and its fresh observation were
                        // revalidated before dependency versions were promoted.
                    } else if let Some(stream_head_version) = source_stream_head_version {
                        let source_definition_version = source_definition_version
                            .context("event automation rollback source version is missing")?;
                        rollback_lineage = Some(
                            require_same_space_automation_rollback_observation(
                                tx,
                                space_id,
                                definition_key,
                                *stream_head_version,
                                source_definition_version,
                                business.0,
                                mapping.0,
                                trigger_config,
                                timezone,
                                misfire_policy,
                            )
                            .await?,
                        );
                    } else {
                        require_same_space_automation_shadow_observation(
                            tx,
                            space_id,
                            definition_key,
                            mapping.0,
                        )
                        .await?;
                    }
                }
                Some(mapping.0)
            } else {
                None
            };
            let (automation_definition_id, version) = apply_automation_definition(
                tx,
                space_id,
                actor_person_id,
                work_item_id,
                definition_key,
                status,
                business.0,
                event_mapping,
                trigger_kind,
                trigger_config,
                timezone,
                misfire_policy,
                definition_digest(change)?,
            )
            .await?;
            if let Some(lineage) = rollback_lineage {
                record_automation_rollback_lineage(
                    tx,
                    work_item_id,
                    space_id,
                    definition_key,
                    automation_definition_id,
                    version,
                    &lineage,
                )
                .await?;
            }
            applied.push(AppliedDefinition {
                resource: "automation_definition",
                definition_key: definition_key.clone(),
                version,
                status: status.clone(),
            });
        }
    }
    Ok(applied)
}

fn definition_digest(change: &SpaceChange) -> Result<String> {
    Ok(sha256_hex(
        &serde_json::to_vec(change).context("encode versioned definition")?,
    ))
}

async fn lock_definition_stream(
    tx: &mut Transaction<'_, Postgres>,
    stream_key: &str,
) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(stream_key)
        .execute(&mut **tx)
        .await
        .context("lock definition version stream")?;
    Ok(())
}

async fn next_space_version(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    space_id: Uuid,
    definition_key: &str,
) -> Result<i32> {
    let query = format!(
        "SELECT COALESCE(MAX(version), 0) + 1 AS next_version FROM qintopia_agent_os.{table} WHERE space_id = $1 AND definition_key = $2"
    );
    let next: i64 = sqlx::query_scalar(&query)
        .bind(space_id)
        .bind(definition_key)
        .fetch_one(&mut **tx)
        .await
        .context("read next Space definition version")?;
    i32::try_from(next).context("Space definition version exceeds integer range")
}

async fn next_provider_version(
    tx: &mut Transaction<'_, Postgres>,
    provider: &str,
    definition_key: &str,
) -> Result<i32> {
    let next: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(MAX(version), 0) + 1
        FROM qintopia_agent_os.channel_event_mapping_versions
        WHERE provider = $1 AND definition_key = $2
        "#,
    )
    .bind(provider)
    .bind(definition_key)
    .fetch_one(&mut **tx)
    .await
    .context("read next provider mapping version")?;
    i32::try_from(next).context("provider mapping version exceeds integer range")
}

fn supersedes_active(status: &str) -> bool {
    matches!(status, "active" | "paused" | "retired")
}

#[expect(
    clippy::too_many_arguments,
    reason = "the versioned Space policy columns are intentionally explicit"
)]
async fn apply_space_policy(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    actor_person_id: Uuid,
    work_item_id: Uuid,
    definition_key: &str,
    status: &str,
    policy_config: &Value,
    digest: String,
) -> Result<i32> {
    lock_definition_stream(tx, &format!("space-policy:{space_id}:{definition_key}")).await?;
    let version = next_space_version(tx, "space_policy_versions", space_id, definition_key).await?;
    if supersedes_active(status) {
        sqlx::query(
            r#"
            UPDATE qintopia_agent_os.space_policy_versions
            SET status = 'retired', retired_at = now(), updated_at = now()
            WHERE space_id = $1 AND definition_key = $2 AND status = 'active'
            "#,
        )
        .bind(space_id)
        .bind(definition_key)
        .execute(&mut **tx)
        .await
        .context("retire active Space policy version")?;
    }
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.space_policy_versions
            (space_id, definition_key, version, policy_config, status,
             definition_digest, created_by_person_id, created_from_work_item_id,
             activated_at, retired_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8,
                CASE WHEN $5 = 'active' THEN now() END,
                CASE WHEN $5 = 'retired' THEN now() END)
        "#,
    )
    .bind(space_id)
    .bind(definition_key)
    .bind(version)
    .bind(policy_config)
    .bind(status)
    .bind(digest)
    .bind(actor_person_id)
    .bind(work_item_id)
    .execute(&mut **tx)
    .await
    .context("insert Space policy version")?;
    Ok(version)
}

#[expect(
    clippy::too_many_arguments,
    reason = "the versioned event mapping columns are intentionally explicit"
)]
async fn apply_event_mapping(
    tx: &mut Transaction<'_, Postgres>,
    actor_person_id: Uuid,
    work_item_id: Uuid,
    provider: &str,
    definition_key: &str,
    status: &str,
    selector: &Value,
    extractor: &Value,
    official_sources: &[String],
    validation_evidence: &Value,
    digest: String,
) -> Result<(Uuid, i32)> {
    lock_definition_stream(tx, &format!("event-mapping:{provider}:{definition_key}")).await?;
    let version = next_provider_version(tx, provider, definition_key).await?;
    if supersedes_active(status) {
        sqlx::query(
            r#"
            UPDATE qintopia_agent_os.channel_event_mapping_versions
            SET status = 'retired', retired_at = now(), updated_at = now()
            WHERE provider = $1 AND definition_key = $2 AND status = 'active'
            "#,
        )
        .bind(provider)
        .bind(definition_key)
        .execute(&mut **tx)
        .await
        .context("retire active provider event mapping")?;
    }
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.channel_event_mapping_versions
            (id, provider, definition_key, version, selector, extractor,
             official_sources, validation_evidence, status, definition_digest,
             created_by_person_id, created_from_work_item_id, activated_at, retired_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                CASE WHEN $9 = 'active' THEN now() END,
                CASE WHEN $9 = 'retired' THEN now() END)
        "#,
    )
    .bind(id)
    .bind(provider)
    .bind(definition_key)
    .bind(version)
    .bind(selector)
    .bind(extractor)
    .bind(json!(official_sources))
    .bind(validation_evidence)
    .bind(status)
    .bind(digest)
    .bind(actor_person_id)
    .bind(work_item_id)
    .execute(&mut **tx)
    .await
    .context("insert provider event mapping version")?;
    Ok((id, version))
}

#[expect(
    clippy::too_many_arguments,
    reason = "the versioned business definition columns are intentionally explicit"
)]
async fn apply_business_definition(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    actor_person_id: Uuid,
    work_item_id: Uuid,
    definition_key: &str,
    status: &str,
    execution_mode: &str,
    definition: &Value,
    allowed_capabilities: &[String],
    approval_policy: &str,
    digest: String,
) -> Result<(Uuid, i32)> {
    lock_definition_stream(tx, &format!("business:{space_id}:{definition_key}")).await?;
    let version =
        next_space_version(tx, "business_definition_versions", space_id, definition_key).await?;
    if supersedes_active(status) {
        sqlx::query(
            r#"
            UPDATE qintopia_agent_os.business_definition_versions
            SET status = 'retired', retired_at = now(), updated_at = now()
            WHERE space_id = $1 AND definition_key = $2 AND status = 'active'
            "#,
        )
        .bind(space_id)
        .bind(definition_key)
        .execute(&mut **tx)
        .await
        .context("retire active business definition")?;
    }
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.business_definition_versions
            (id, space_id, definition_key, version, execution_mode, definition,
             allowed_capabilities, approval_policy, status, definition_digest,
             created_by_person_id, created_from_work_item_id, activated_at, retired_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                CASE WHEN $9 = 'active' THEN now() END,
                CASE WHEN $9 = 'retired' THEN now() END)
        "#,
    )
    .bind(id)
    .bind(space_id)
    .bind(definition_key)
    .bind(version)
    .bind(execution_mode)
    .bind(definition)
    .bind(allowed_capabilities)
    .bind(approval_policy)
    .bind(status)
    .bind(digest)
    .bind(actor_person_id)
    .bind(work_item_id)
    .execute(&mut **tx)
    .await
    .context("insert business definition version")?;
    Ok((id, version))
}

async fn resolve_bound_business_reference(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    definition_key: &str,
    binding: &DefinitionBinding,
    created: &HashMap<String, (Uuid, String, String)>,
) -> Result<(Uuid, String)> {
    if binding.source == "proposal" {
        let reference = created
            .get(definition_key)
            .context("proposal-bound automation business definition was not created")?;
        if reference.2 != binding.definition_digest {
            bail!("proposal-bound automation business definition digest changed");
        }
        return Ok((reference.0, reference.1.clone()));
    }
    if binding.source != "existing" {
        bail!("automation business definition binding source is invalid");
    }
    let bound_id = binding
        .id
        .context("existing automation business definition id is missing")?;
    let stream_head_version = binding
        .stream_head_version
        .context("existing automation business definition head is missing")?;
    let row = sqlx::query(
        r#"
        SELECT bound.id, bound.status
        FROM qintopia_agent_os.business_definition_versions bound
        WHERE bound.id = $1
          AND bound.space_id = $2
          AND bound.definition_key = $3
          AND bound.definition_digest = $4
          AND bound.status = 'active'
          AND (SELECT MAX(head.version)
               FROM qintopia_agent_os.business_definition_versions head
               WHERE head.space_id = bound.space_id
                 AND head.definition_key = bound.definition_key) = $5
        FOR SHARE OF bound
        "#,
    )
    .bind(bound_id)
    .bind(space_id)
    .bind(definition_key)
    .bind(&binding.definition_digest)
    .bind(stream_head_version)
    .fetch_optional(&mut **tx)
    .await
    .context("resolve bound automation business definition")?
    .context("automation business definition binding changed after prepare")?;
    Ok((row.try_get("id")?, row.try_get("status")?))
}

async fn resolve_bound_mapping_reference(
    tx: &mut Transaction<'_, Postgres>,
    provider: &str,
    definition_key: &str,
    binding: &DefinitionBinding,
    created: &HashMap<(String, String), (Uuid, String, String)>,
) -> Result<(Uuid, String)> {
    if binding.source == "proposal" {
        let reference = created
            .get(&(provider.to_string(), definition_key.to_string()))
            .context("proposal-bound automation event mapping was not created")?;
        if reference.2 != binding.definition_digest {
            bail!("proposal-bound automation event mapping digest changed");
        }
        return Ok((reference.0, reference.1.clone()));
    }
    if binding.source != "existing" {
        bail!("automation event mapping binding source is invalid");
    }
    let bound_id = binding
        .id
        .context("existing automation event mapping id is missing")?;
    let stream_head_version = binding
        .stream_head_version
        .context("existing automation event mapping head is missing")?;
    let row = sqlx::query(
        r#"
        SELECT bound.id, bound.status
        FROM qintopia_agent_os.channel_event_mapping_versions bound
        WHERE bound.id = $1
          AND bound.provider = $2
          AND bound.definition_key = $3
          AND bound.definition_digest = $4
          AND bound.status = 'active'
          AND (SELECT MAX(head.version)
               FROM qintopia_agent_os.channel_event_mapping_versions head
               WHERE head.provider = bound.provider
                 AND head.definition_key = bound.definition_key) = $5
        FOR SHARE OF bound
        "#,
    )
    .bind(bound_id)
    .bind(provider)
    .bind(definition_key)
    .bind(&binding.definition_digest)
    .bind(stream_head_version)
    .fetch_optional(&mut **tx)
    .await
    .context("resolve bound automation event mapping")?
    .context("automation event mapping binding changed after prepare")?;
    Ok((row.try_get("id")?, row.try_get("status")?))
}

async fn require_same_space_automation_shadow_observation(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    automation_key: &str,
    mapping_id: Uuid,
) -> Result<()> {
    let observed: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM qintopia_agent_os.automation_definition_versions shadow
            JOIN qintopia_agent_os.channel_event_mapping_versions source_mapping
              ON source_mapping.id = shadow.channel_event_mapping_id
             AND source_mapping.status IN ('shadow', 'active')
            JOIN qintopia_agent_os.channel_event_mapping_versions target_mapping
              ON target_mapping.id = $3
             AND target_mapping.status = 'active'
             AND target_mapping.provider = source_mapping.provider
             AND target_mapping.definition_key = source_mapping.definition_key
             AND target_mapping.selector = source_mapping.selector
             AND target_mapping.extractor = source_mapping.extractor
            JOIN qintopia_agent_os.work_items observation
              ON observation.space_id = shadow.space_id
             AND observation.work_item_type = 'space_event_shadow_observation'
             AND observation.status = 'completed'
             AND observation.requester_agent = 'system'
             AND observation.capability_key = 'erhua.execute_space_business'
             AND observation.source_type = 'space_event_shadow'
             AND observation.source_refs ->> 'mapping_version_id' = source_mapping.id::text
             AND observation.payload ->> 'decode_success' = 'true'
             AND observation.metadata ->> 'space_bound' = 'true'
             AND observation.metadata ->> 'scope'
                 = 'automation_shadow:' || shadow.id::text
            JOIN qintopia_messages.raw_events raw_event
              ON raw_event.id::text = observation.source_refs ->> 'raw_event_id'
             AND raw_event.space_id = $1
             AND raw_event.source = source_mapping.provider
             AND raw_event.ingress_auth_verified
             AND raw_event.created_at > GREATEST(shadow.created_at, source_mapping.created_at)
            WHERE shadow.space_id = $1
              AND shadow.definition_key = $2
              AND shadow.trigger_kind = 'event'
              AND shadow.status = 'shadow'
        )
        "#,
    )
    .bind(space_id)
    .bind(automation_key)
    .bind(mapping_id)
    .fetch_one(&mut **tx)
    .await
    .context("verify same-Space automation shadow observation")?;
    if !observed {
        bail!("active event automation requires a completed same-Space shadow observation");
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "rollback evidence is bound to the complete historical automation runtime tuple"
)]
async fn require_same_space_automation_rollback_observation(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    automation_key: &str,
    source_stream_head_version: i32,
    source_definition_version: i32,
    business_definition_id: Uuid,
    mapping_id: Uuid,
    trigger_config: &Value,
    timezone: &str,
    misfire_policy: &str,
) -> Result<AutomationRollbackLineage> {
    let evidence_rows = sqlx::query(
        r#"
        WITH source AS (
            SELECT restored.id, restored.space_id, restored.definition_key,
                   restored.version, restored.created_from_work_item_id,
                   restored.created_at, restored.trigger_kind,
                   restored.trigger_config, restored.timezone,
                   restored.misfire_policy, restored.channel_event_mapping_id
            FROM qintopia_agent_os.automation_definition_versions restored
            JOIN qintopia_agent_os.channel_event_mapping_versions target_mapping
              ON target_mapping.id = $6
             AND target_mapping.status = 'active'
             AND restored.channel_event_mapping_id = target_mapping.id
            WHERE restored.space_id = $1
              AND restored.definition_key = $2
              AND restored.version = $4
              AND restored.version < $3
              AND restored.status = 'retired'
              AND restored.activated_at IS NOT NULL
              AND restored.business_definition_id = $5
              AND restored.trigger_kind = 'event'
              AND restored.trigger_config = $7
              AND restored.timezone = $8
              AND restored.misfire_policy = $9
        ),
        rollback_restore AS (
            SELECT DISTINCT source.id AS automation_definition_id
            FROM source
            JOIN qintopia_agent_os.work_items change_request
              ON change_request.id = source.created_from_work_item_id
             AND change_request.space_id = source.space_id
             AND change_request.work_item_type = 'space_change_request'
             AND change_request.status = 'completed'
             AND change_request.requester_agent = 'erhua'
             AND change_request.capability_key = 'erhua.manage_space_configuration'
            JOIN qintopia_agent_os.artifacts proposal
              ON proposal.work_item_id = change_request.id
             AND proposal.artifact_type = 'space_change_proposal'
             AND proposal.review_status = 'approved'
            CROSS JOIN LATERAL jsonb_array_elements(
                COALESCE((proposal.content_text::jsonb) -> 'changes', '[]'::jsonb)
            ) AS proposal_change(definition)
            WHERE proposal_change.definition ->> 'resource' = 'automation_definition'
              AND proposal_change.definition ->> 'definition_key' = source.definition_key
              AND proposal_change.definition ->> 'status' = 'active'
              AND proposal_change.definition ->> 'source_stream_head_version'
                  = (source.version - 1)::text
              AND jsonb_typeof(proposal_change.definition -> 'source_definition_version')
                  = 'number'
              AND (proposal_change.definition ->> 'source_definition_version')::integer > 0
              AND (proposal_change.definition ->> 'source_definition_version')::integer
                  < source.version
        ),
        direct_evidence AS (
            SELECT shadow.id AS shadow_automation_definition_id,
                   source_mapping.id AS source_mapping_version_id,
                   observation.id AS observation_work_item_id,
                   raw_event.id AS raw_event_id,
                   'direct_shadow_predecessor'::text AS lineage_source
            FROM source
            JOIN qintopia_agent_os.automation_definition_versions shadow
              ON shadow.space_id = source.space_id
             AND shadow.definition_key = source.definition_key
             AND shadow.version = source.version - 1
             AND shadow.status = 'retired'
             AND shadow.trigger_kind = source.trigger_kind
             AND shadow.trigger_config = source.trigger_config
             AND shadow.timezone = source.timezone
             AND shadow.misfire_policy = source.misfire_policy
            JOIN qintopia_agent_os.channel_event_mapping_versions source_mapping
              ON source_mapping.id = shadow.channel_event_mapping_id
             AND source_mapping.status IN ('shadow', 'active', 'retired')
            JOIN qintopia_agent_os.channel_event_mapping_versions target_mapping
              ON target_mapping.id = source.channel_event_mapping_id
             AND target_mapping.status = 'active'
             AND target_mapping.provider = source_mapping.provider
             AND target_mapping.definition_key = source_mapping.definition_key
             AND target_mapping.selector = source_mapping.selector
             AND target_mapping.extractor = source_mapping.extractor
            JOIN qintopia_agent_os.work_items observation
              ON observation.space_id = shadow.space_id
             AND observation.work_item_type = 'space_event_shadow_observation'
             AND observation.status = 'completed'
             AND observation.requester_agent = 'system'
             AND observation.capability_key = 'erhua.execute_space_business'
             AND observation.source_type = 'space_event_shadow'
             AND observation.source_refs ->> 'mapping_version_id' = source_mapping.id::text
             AND observation.payload ->> 'decode_success' = 'true'
             AND observation.metadata ->> 'space_bound' = 'true'
             AND observation.metadata ->> 'scope'
                 = 'automation_shadow:' || shadow.id::text
            JOIN qintopia_messages.raw_events raw_event
              ON raw_event.id::text = observation.source_refs ->> 'raw_event_id'
             AND raw_event.space_id = source.space_id
             AND raw_event.source = source_mapping.provider
             AND raw_event.ingress_auth_verified
             AND raw_event.created_at > GREATEST(shadow.created_at, source_mapping.created_at)
             AND raw_event.created_at < source.created_at
            WHERE NOT EXISTS (
                SELECT 1
                FROM rollback_restore
                WHERE rollback_restore.automation_definition_id = source.id
            )
            ORDER BY raw_event.created_at, raw_event.id, observation.id
            LIMIT 1
        ),
        persisted_evidence AS (
            SELECT shadow.id AS shadow_automation_definition_id,
                   source_mapping.id AS source_mapping_version_id,
                   observation.id AS observation_work_item_id,
                   raw_event.id AS raw_event_id,
                   'persisted_rollback_lineage'::text AS lineage_source
            FROM source
            JOIN rollback_restore
              ON rollback_restore.automation_definition_id = source.id
            JOIN qintopia_agent_os.work_items lineage_owner
              ON lineage_owner.id = source.created_from_work_item_id
             AND lineage_owner.space_id = source.space_id
             AND lineage_owner.work_item_type = 'space_change_request'
             AND lineage_owner.status = 'completed'
             AND lineage_owner.requester_agent = 'erhua'
             AND lineage_owner.capability_key = 'erhua.manage_space_configuration'
            JOIN qintopia_agent_os.work_item_events lineage_event
              ON lineage_event.work_item_id = lineage_owner.id
             AND lineage_event.artifact_id IS NULL
             AND lineage_event.event_type = 'automation_rollback_lineage_recorded'
             AND lineage_event.actor_type = 'system'
             AND lineage_event.actor_id = 'space_configuration'
             AND lineage_event.data ->> 'schema_version' = '1'
             AND lineage_event.data ->> 'space_id' = source.space_id::text
             AND lineage_event.data ->> 'definition_key' = source.definition_key
             AND lineage_event.data ->> 'automation_definition_id' = source.id::text
             AND lineage_event.data ->> 'automation_definition_version' = source.version::text
             AND lineage_event.created_at >= source.created_at
            JOIN qintopia_agent_os.automation_definition_versions shadow
              ON shadow.id::text = lineage_event.data ->> 'shadow_automation_definition_id'
             AND shadow.space_id = source.space_id
             AND shadow.definition_key = source.definition_key
             AND shadow.status IN ('shadow', 'retired')
             AND shadow.trigger_kind = source.trigger_kind
             AND shadow.trigger_config = source.trigger_config
             AND shadow.timezone = source.timezone
             AND shadow.misfire_policy = source.misfire_policy
            JOIN qintopia_agent_os.channel_event_mapping_versions source_mapping
              ON source_mapping.id = shadow.channel_event_mapping_id
             AND source_mapping.id::text = lineage_event.data ->> 'source_mapping_version_id'
             AND source_mapping.status IN ('shadow', 'active', 'retired')
            JOIN qintopia_agent_os.channel_event_mapping_versions target_mapping
              ON target_mapping.id = source.channel_event_mapping_id
             AND target_mapping.status = 'active'
             AND target_mapping.provider = source_mapping.provider
             AND target_mapping.definition_key = source_mapping.definition_key
             AND target_mapping.selector = source_mapping.selector
             AND target_mapping.extractor = source_mapping.extractor
            JOIN qintopia_agent_os.work_items observation
              ON observation.id::text = lineage_event.data ->> 'observation_work_item_id'
             AND observation.space_id = source.space_id
             AND observation.work_item_type = 'space_event_shadow_observation'
             AND observation.status = 'completed'
             AND observation.requester_agent = 'system'
             AND observation.capability_key = 'erhua.execute_space_business'
             AND observation.source_type = 'space_event_shadow'
             AND observation.source_refs ->> 'mapping_version_id' = source_mapping.id::text
             AND observation.payload ->> 'decode_success' = 'true'
             AND observation.metadata ->> 'space_bound' = 'true'
             AND observation.metadata ->> 'scope'
                 = 'automation_shadow:' || shadow.id::text
            JOIN qintopia_messages.raw_events raw_event
              ON raw_event.id::text = lineage_event.data ->> 'raw_event_id'
             AND raw_event.id::text = observation.source_refs ->> 'raw_event_id'
             AND raw_event.space_id = source.space_id
             AND raw_event.source = source_mapping.provider
             AND raw_event.ingress_auth_verified
             AND raw_event.created_at > GREATEST(shadow.created_at, source_mapping.created_at)
             AND raw_event.created_at < source.created_at
        ),
        evidence AS (
            SELECT * FROM direct_evidence
            UNION ALL
            SELECT * FROM persisted_evidence
        )
        SELECT shadow_automation_definition_id, source_mapping_version_id,
               observation_work_item_id, raw_event_id, lineage_source
        FROM evidence
        ORDER BY lineage_source
        LIMIT 2
        "#,
    )
    .bind(space_id)
    .bind(automation_key)
    .bind(source_stream_head_version)
    .bind(source_definition_version)
    .bind(business_definition_id)
    .bind(mapping_id)
    .bind(trigger_config)
    .bind(timezone)
    .bind(misfire_policy)
    .fetch_all(&mut **tx)
    .await
    .context("verify exact historical event automation rollback observation")?;
    if evidence_rows.len() != 1 {
        bail!(
            "event automation rollback requires the exact historical active definition and its authenticated shadow observation"
        );
    }
    let row = &evidence_rows[0];
    Ok(AutomationRollbackLineage {
        shadow_automation_definition_id: row.try_get("shadow_automation_definition_id")?,
        source_mapping_version_id: row.try_get("source_mapping_version_id")?,
        observation_work_item_id: row.try_get("observation_work_item_id")?,
        raw_event_id: row.try_get("raw_event_id")?,
    })
}

async fn record_automation_rollback_lineage(
    tx: &mut Transaction<'_, Postgres>,
    work_item_id: Uuid,
    space_id: Uuid,
    definition_key: &str,
    automation_definition_id: Uuid,
    automation_definition_version: i32,
    lineage: &AutomationRollbackLineage,
) -> Result<()> {
    append_event(
        tx,
        work_item_id,
        None,
        AUTOMATION_ROLLBACK_LINEAGE_EVENT,
        "system",
        "space_configuration",
        "Persisted exact authenticated evidence lineage for a restored automation version",
        json!({
            "schema_version": AUTOMATION_ROLLBACK_LINEAGE_SCHEMA_VERSION,
            "space_id": space_id,
            "definition_key": definition_key,
            "automation_definition_id": automation_definition_id,
            "automation_definition_version": automation_definition_version,
            "shadow_automation_definition_id": lineage.shadow_automation_definition_id,
            "source_mapping_version_id": lineage.source_mapping_version_id,
            "observation_work_item_id": lineage.observation_work_item_id,
            "raw_event_id": lineage.raw_event_id,
            "external_send_executed": false
        }),
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "the versioned automation definition columns are intentionally explicit"
)]
async fn apply_automation_definition(
    tx: &mut Transaction<'_, Postgres>,
    space_id: Uuid,
    actor_person_id: Uuid,
    work_item_id: Uuid,
    definition_key: &str,
    status: &str,
    business_definition_id: Uuid,
    channel_event_mapping_id: Option<Uuid>,
    trigger_kind: &str,
    trigger_config: &Value,
    timezone: &str,
    misfire_policy: &str,
    digest: String,
) -> Result<(Uuid, i32)> {
    lock_definition_stream(tx, &format!("automation:{space_id}:{definition_key}")).await?;
    let version = next_space_version(
        tx,
        "automation_definition_versions",
        space_id,
        definition_key,
    )
    .await?;
    if status == "shadow" {
        sqlx::query(
            r#"
            UPDATE qintopia_agent_os.automation_definition_versions
            SET status = 'retired', retired_at = now(), next_run_at = NULL,
                updated_at = now()
            WHERE space_id = $1 AND definition_key = $2 AND status = 'shadow'
            "#,
        )
        .bind(space_id)
        .bind(definition_key)
        .execute(&mut **tx)
        .await
        .context("retire previous shadow automation definition")?;
    } else if supersedes_active(status) {
        sqlx::query(
            r#"
            UPDATE qintopia_agent_os.automation_definition_versions
            SET status = 'retired', retired_at = now(), next_run_at = NULL,
                updated_at = now()
            WHERE space_id = $1 AND definition_key = $2
              AND status IN ('active', 'shadow')
            "#,
        )
        .bind(space_id)
        .bind(definition_key)
        .execute(&mut **tx)
        .await
        .context("retire runnable automation definition")?;
    }
    let next_run_at = if status == "active" && trigger_kind == "schedule" {
        Some(crate::automation_dispatcher::next_schedule_run(
            trigger_config,
            timezone,
            Utc::now(),
        )?)
    } else {
        None
    };
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.automation_definition_versions
            (id, space_id, definition_key, version, business_definition_id,
             channel_event_mapping_id, trigger_kind, trigger_config, timezone,
             misfire_policy, status, next_run_at, definition_digest,
             created_by_person_id, created_from_work_item_id, activated_at, retired_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
                CASE WHEN $11 = 'active' THEN now() END,
                CASE WHEN $11 = 'retired' THEN now() END)
        "#,
    )
    .bind(id)
    .bind(space_id)
    .bind(definition_key)
    .bind(version)
    .bind(business_definition_id)
    .bind(channel_event_mapping_id)
    .bind(trigger_kind)
    .bind(trigger_config)
    .bind(timezone)
    .bind(misfire_policy)
    .bind(status)
    .bind(next_run_at)
    .bind(digest)
    .bind(actor_person_id)
    .bind(work_item_id)
    .execute(&mut **tx)
    .await
    .context("insert automation definition version")?;
    Ok((id, version))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_welcome_intent() -> Value {
        json!({
            "summary": "Welcome new members with the configured text.",
            "changes": [
                {
                    "resource": "space_policy",
                    "definition_key": "default",
                    "status": "active",
                    "policy_config": {
                        "identity": "community assistant",
                        "capability_grants": ["erhua.qiwe_text_template"]
                    }
                },
                {
                    "resource": "channel_event_mapping",
                    "provider": "qiwe",
                    "definition_key": "group_member_add",
                    "status": "shadow",
                    "selector": {
                        "op": "any",
                        "rules": [
                            {
                                "op": "all",
                                "rules": [
                                    {
                                        "op": "equals",
                                        "pointer": "/newMsgType",
                                        "value": "GROUP_MEMBER_ADD"
                                    },
                                    {
                                        "op": "in",
                                        "pointer": "/cmd",
                                        "values": [15000, 15500]
                                    }
                                ]
                            },
                            {
                                "op": "all",
                                "rules": [
                                    {"op": "equals", "pointer": "/msgType", "value": 1002},
                                    {
                                        "op": "exists",
                                        "pointer": "/newMsgType",
                                        "value": false
                                    },
                                    {
                                        "op": "in",
                                        "pointer": "/cmd",
                                        "values": [15000, 15500]
                                    }
                                ]
                            }
                        ]
                    },
                    "extractor": {
                        "event_type": "qiwe.group_member_added",
                        "event_id": {
                            "pointer": "/msgUniqueIdentifier",
                            "transforms": [{"op": "opaque_id"}]
                        },
                        "space_chat_id": {
                            "pointer": "/fromRoomId",
                            "transforms": [{"op": "opaque_id"}]
                        },
                        "subject_user_ids": {
                            "pointer": "/msgData/changedMemberList",
                            "transforms": [
                                {"op": "base64_utf8"},
                                {"op": "split", "delimiter": ";", "max_parts": 64},
                                {"op": "opaque_id"},
                                {"op": "dedupe"}
                            ]
                        },
                        "occurred_at": {
                            "pointer": "/timestamp",
                            "transforms": [{"op": "unix_timestamp"}]
                        }
                    },
                    "official_sources": ["https://doc.qiweapi.com/doc-7331304"],
                    "validation_evidence": {}
                },
                {
                    "resource": "business_definition",
                    "definition_key": "welcome_new_members",
                    "status": "shadow",
                    "execution_mode": "deterministic",
                    "definition": {
                        "capability_key": "erhua.qiwe_text_template",
                        "input": {"text_template": "Welcome, {{subject_names}}"}
                    },
                    "allowed_capabilities": ["erhua.qiwe_text_template"],
                    "approval_policy": "space_admin_confirmation"
                },
                {
                    "resource": "automation_definition",
                    "definition_key": "welcome_new_members",
                    "status": "shadow",
                    "business_definition_key": "welcome_new_members",
                    "trigger_kind": "event",
                    "trigger_config": {"batch_subjects": true},
                    "event_mapping_provider": "qiwe",
                    "event_mapping_key": "group_member_add"
                }
            ]
        })
    }

    #[test]
    fn accepts_generic_welcome_change_set_without_destination() {
        let intent = parse_and_validate_intent(valid_welcome_intent()).expect("valid intent");
        assert_eq!(intent.changes.len(), 4);
        assert!(intent.protects_provider_mapping());
        let encoded = serde_json::to_string(&intent).expect("serialize intent");
        assert!(!encoded.contains("target_group_id"));
        assert!(!encoded.contains("room_id"));
    }

    #[test]
    fn programming_extension_continuation_accepts_only_exact_shadow_mapping() {
        let continuation = crate::space_programming_extension::ReadyContinuation {
            intent: "Welcome new members.".to_string(),
            provider: "qiwe".to_string(),
            mapping_key: "group_member_add".to_string(),
        };
        let intent = parse_and_validate_intent(valid_welcome_intent()).expect("valid intent");
        validate_programming_extension_shadow_intent(&intent, &continuation)
            .expect("exact released mapping may continue in shadow");

        let mut wrong_status = valid_welcome_intent();
        wrong_status["changes"][1]["status"] = json!("active");
        let wrong_status =
            parse_and_validate_intent(wrong_status).expect("otherwise valid mapping intent");
        assert!(
            validate_programming_extension_shadow_intent(&wrong_status, &continuation).is_err()
        );

        let mut wrong_key = valid_welcome_intent();
        wrong_key["changes"][1]["definition_key"] = json!("different_mapping");
        let wrong_key =
            parse_and_validate_intent(wrong_key).expect("otherwise valid mapping intent");
        assert!(validate_programming_extension_shadow_intent(&wrong_key, &continuation).is_err());

        let request_id = Uuid::new_v4();
        let response = decorate_continuation_response(json!({"success": true}), Some(request_id));
        assert_eq!(
            response["continued_from_request_id"],
            request_id.to_string()
        );
        assert_eq!(response["continuation_phase"], "shadow_prepared");
    }

    #[test]
    fn review_projection_shows_material_authorization_without_internal_bindings() {
        let intent = parse_and_validate_intent(valid_welcome_intent()).expect("valid intent");
        let projection = change_summaries(&intent);

        let business = projection
            .iter()
            .find(|change| change["resource"] == "business_definition")
            .expect("business review projection");
        assert_eq!(
            business["definition"]["input"]["text_template"],
            "Welcome, {{subject_names}}"
        );
        assert_eq!(
            business["allowed_capabilities"],
            json!(["erhua.qiwe_text_template"])
        );
        assert_eq!(business["approval_policy"], "space_admin_confirmation");

        let automation = projection
            .iter()
            .find(|change| change["resource"] == "automation_definition")
            .expect("automation review projection");
        assert_eq!(automation["trigger_config"]["batch_subjects"], true);
        assert_eq!(automation["timezone"], "Asia/Shanghai");
        assert_eq!(automation["misfire_policy"], "run_once");
        assert_eq!(automation["event_mapping"]["provider"], "qiwe");
        assert_eq!(
            automation["event_mapping"]["definition_key"],
            "group_member_add"
        );

        let mapping = projection
            .iter()
            .find(|change| change["resource"] == "channel_event_mapping")
            .expect("mapping review projection");
        assert_eq!(mapping["event_type"], "qiwe.group_member_added");
        assert!(mapping.get("extractor").is_none());
        assert!(mapping.get("validation_evidence").is_none());

        let encoded = serde_json::to_string(&projection).expect("serialize projection");
        assert!(!encoded.contains("business_definition_binding"));
        assert!(!encoded.contains("event_mapping_binding"));
        assert!(!encoded.contains("source_stream_head_version"));
        assert!(!encoded.contains("target_group_id"));
        assert_eq!(proposal_fingerprint(&"a".repeat(64)), "aaaaaaaaaaaa");
    }

    #[test]
    fn additive_policy_materialization_preserves_existing_business_grants() {
        let current = json!({
            "identity": "existing persona",
            "capability_grants": ["erhua.existing_business"]
        });
        let proposed = json!({
            "identity": "updated persona",
            "capability_grants": ["erhua.new_business"]
        });

        let merged = merge_policy_configs(&current, &proposed).expect("merge policy");

        assert_eq!(merged["identity"], "updated persona");
        assert_eq!(
            merged["capability_grants"],
            json!(["erhua.existing_business", "erhua.new_business"])
        );
    }

    #[test]
    fn explicit_policy_revocation_removes_only_the_reviewed_grant() {
        let current = json!({
            "identity": "existing persona",
            "knowledge_scope": ["community.building_a"],
            "capability_grants": [
                "erhua.knowledge.public",
                "erhua.qiwe_send_location_card"
            ]
        });
        let proposed = json!({
            "capability_revocations": ["erhua.qiwe_send_location_card"]
        });

        let merged = merge_policy_configs(&current, &proposed).expect("revoke policy grant");

        assert_eq!(merged["identity"], "existing persona");
        assert_eq!(merged["knowledge_scope"], json!(["community.building_a"]));
        assert_eq!(
            merged["capability_grants"],
            json!(["erhua.knowledge.public"])
        );
        assert!(merged.get("capability_revocations").is_none());
    }

    #[test]
    fn policy_revocation_rejects_absent_or_simultaneously_granted_capability() {
        let current = json!({"capability_grants": ["erhua.knowledge.public"]});
        assert!(merge_policy_configs(
            &current,
            &json!({"capability_revocations": ["erhua.workflow.sales"]})
        )
        .is_err());
        assert!(merge_policy_configs(
            &current,
            &json!({
                "capability_grants": ["erhua.knowledge.public"],
                "capability_revocations": ["erhua.knowledge.public"]
            })
        )
        .is_err());
    }

    #[test]
    fn pure_space_policy_proposal_does_not_require_a_business_definition() {
        let intent = parse_and_validate_intent(json!({
            "summary": "Set the current group's identity and public knowledge boundary.",
            "changes": [{
                "resource": "space_policy",
                "definition_key": "default",
                "status": "active",
                "policy_config": {
                    "identity": "Resident services assistant",
                    "knowledge_scope": ["community.building_a.public"],
                    "capability_grants": ["erhua.knowledge.public"],
                    "quota_declaration": {
                        "enforcement": "reserved_non_enforced",
                        "limits": {"daily_invocations": 100}
                    }
                }
            }]
        }))
        .expect("parse pure Space policy proposal");

        assert_eq!(intent.changes.len(), 1);
        assert!(matches!(intent.changes[0], SpaceChange::SpacePolicy { .. }));
    }

    #[test]
    fn rejects_destination_fields_at_any_definition_depth() {
        let mut value = valid_welcome_intent();
        value["changes"][2]["definition"]["nested"] = json!({"target_group_id": "forged"});
        let error = parse_and_validate_intent(value).expect_err("destination must fail");
        assert!(error.to_string().contains("forbidden field"));
    }

    #[test]
    fn model_claimed_evidence_cannot_replace_sidecar_fixture_evidence() {
        let mut value = valid_welcome_intent();
        value["changes"][1]["validation_evidence"] = json!({
            "fixture_replay_passed": true,
            "real_event_verified": true
        });
        let intent = parse_and_validate_intent(value).expect("mapping schema should parse");
        let SpaceChange::ChannelEventMapping {
            provider,
            selector,
            extractor,
            ..
        } = &intent.changes[1]
        else {
            panic!("expected channel event mapping");
        };
        let evidence =
            crate::channel_event_mapping::replay_registered_fixtures(provider, selector, extractor)
                .expect("registered fixture replay");
        assert_eq!(evidence["fixture_replay_passed"], true);
        assert_eq!(evidence["real_event_verified"], false);
        assert_eq!(evidence["evidence_source"], "sidecar_registered_fixtures");
    }

    #[test]
    fn every_provider_mapping_state_requires_global_review() {
        for status in ["draft", "shadow", "active", "paused", "retired"] {
            let mut value = valid_welcome_intent();
            value["changes"][1]["status"] = json!(status);
            let intent = parse_and_validate_intent(value).expect("mapping schema should parse");
            assert!(intent.protects_provider_mapping(), "status={status}");
        }
    }

    #[test]
    fn rejects_unregistered_official_source_host() {
        let mut value = valid_welcome_intent();
        value["changes"][1]["official_sources"] = json!(["https://example.com/untrusted"]);
        let error = parse_and_validate_intent(value).expect_err("host must fail");
        assert!(error.to_string().contains("not registered"));
    }

    #[test]
    fn confirmation_hash_binds_actor_space_digest_and_expiry() {
        let actor = Uuid::new_v4();
        let other_actor = Uuid::new_v4();
        let space = Uuid::new_v4();
        let other_space = Uuid::new_v4();
        let expires_at = Utc::now() + Duration::minutes(10);
        let expected = confirmation_hash(
            "0123456789abcdef0123456789abcdef",
            "A1B2C3D4",
            actor,
            space,
            &"a".repeat(64),
            expires_at,
        );
        assert!(constant_time_eq(expected.as_bytes(), expected.as_bytes()));
        assert_ne!(
            expected,
            confirmation_hash(
                "0123456789abcdef0123456789abcdef",
                "A1B2C3D4",
                other_actor,
                space,
                &"a".repeat(64),
                expires_at,
            )
        );
        assert_ne!(
            expected,
            confirmation_hash(
                "0123456789abcdef0123456789abcdef",
                "A1B2C3D4",
                actor,
                other_space,
                &"a".repeat(64),
                expires_at,
            )
        );
    }

    #[test]
    fn explicit_confirmation_requires_the_exact_current_message() {
        let mut session = TrustedSpaceSession {
            platform: "qiwe".to_string(),
            conversation_type: "group".to_string(),
            conversation_id: "room-1".to_string(),
            requester_user_id: "user-1".to_string(),
            source_message_id: "message-1".to_string(),
            source_message_text: Some("确认 A1B2C3D4".to_string()),
        };
        validate_explicit_confirmation_message(&session, "A1B2C3D4")
            .expect("exact confirmation command");
        for text in [
            "好的",
            "不要确认 A1B2C3D4",
            "确认 00000000",
            "A1B2C3D4",
            "确认 A1B2C3D4 谢谢",
        ] {
            session.source_message_text = Some(text.to_string());
            assert!(validate_explicit_confirmation_message(&session, "A1B2C3D4").is_err());
        }
        session.source_message_text = None;
        assert!(validate_explicit_confirmation_message(&session, "A1B2C3D4").is_err());
    }

    #[test]
    fn external_automation_dependency_bindings_are_rejected() {
        let mut value = valid_welcome_intent();
        let digest = "a".repeat(64);
        value["changes"][3]["source_stream_head_version"] = json!(1);
        value["changes"][3]["business_definition_binding"] = json!({
            "source": "proposal",
            "definition_digest": digest
        });
        value["changes"][3]["event_mapping_binding"] = json!({
            "source": "proposal",
            "definition_digest": "b".repeat(64)
        });
        let error = parse_and_validate_intent(value.clone())
            .expect_err("model-supplied dependency bindings must fail");
        assert!(error.to_string().contains("supplied only by the sidecar"));
        parse_and_validate_stored_intent(value).expect("stored dependency bindings are valid");
    }

    #[test]
    fn dependency_replacement_requires_a_terminal_or_migrating_automation_change() {
        assert!(automation_transition_covers("active", Some("active")));
        assert!(automation_transition_covers("active", Some("paused")));
        assert!(automation_transition_covers("shadow", Some("shadow")));
        assert!(automation_transition_covers("shadow", Some("retired")));
        assert!(!automation_transition_covers("active", Some("shadow")));
        assert!(!automation_transition_covers("active", Some("draft")));
        assert!(!automation_transition_covers("shadow", None));
    }

    #[test]
    fn accepts_only_bounded_automation_definition_operations() {
        for change in [
            json!({
                "resource": "definition_operation",
                "target_resource": "automation_definition",
                "definition_key": "morning_brief",
                "operation": "activate"
            }),
            json!({
                "resource": "definition_operation",
                "target_resource": "automation_definition",
                "definition_key": "morning_brief",
                "operation": "pause"
            }),
            json!({
                "resource": "definition_operation",
                "target_resource": "automation_definition",
                "definition_key": "morning_brief",
                "operation": "rollback",
                "version": 1
            }),
        ] {
            parse_and_validate_intent(json!({
                "summary": "Change one existing automation.",
                "changes": [change]
            }))
            .expect("bounded automation definition operation");
        }

        for change in [
            json!({
                "resource": "definition_operation",
                "target_resource": "business_definition",
                "definition_key": "morning_brief",
                "operation": "pause"
            }),
            json!({
                "resource": "definition_operation",
                "target_resource": "automation_definition",
                "definition_key": "morning_brief",
                "operation": "activate",
                "version": 1
            }),
            json!({
                "resource": "definition_operation",
                "target_resource": "automation_definition",
                "definition_key": "morning_brief",
                "operation": "pause",
                "version": 1
            }),
            json!({
                "resource": "definition_operation",
                "target_resource": "automation_definition",
                "definition_key": "morning_brief",
                "operation": "rollback",
                "version": 0
            }),
        ] {
            assert!(parse_and_validate_intent(json!({
                "summary": "Reject an invalid definition operation.",
                "changes": [change]
            }))
            .is_err());
        }
    }

    #[test]
    fn activation_binding_is_sidecar_only_and_provider_promotion_is_protected() {
        let automation_id = Uuid::new_v4();
        let business_id = Uuid::new_v4();
        let mapping_id = Uuid::new_v4();
        let value = json!({
            "summary": "Activate the exact current shadow automation.",
            "changes": [{
                "resource": "definition_operation",
                "target_resource": "automation_definition",
                "definition_key": "welcome_new_members",
                "operation": "activate",
                "activation_binding": {
                    "automation": {
                        "id": automation_id,
                        "definition_digest": "a".repeat(64),
                        "stream_head_version": 4,
                        "status": "shadow"
                    },
                    "business_definition": {
                        "id": business_id,
                        "definition_digest": "b".repeat(64),
                        "stream_head_version": 2,
                        "status": "shadow"
                    },
                    "event_mapping": {
                        "id": mapping_id,
                        "definition_digest": "c".repeat(64),
                        "stream_head_version": 3,
                        "status": "shadow"
                    }
                },
                "activation_review": {
                    "result_status": "active",
                    "automation_version": 4,
                    "automation_fingerprint": "a".repeat(64),
                    "trigger_kind": "event",
                    "trigger_config": {"batch_subjects": true},
                    "timezone": "Asia/Shanghai",
                    "misfire_policy": "run_once",
                    "business_definition_key": "welcome_new_members",
                    "business_source_status": "shadow",
                    "business_version": 2,
                    "business_fingerprint": "b".repeat(64),
                    "execution_mode": "deterministic",
                    "business_definition": {
                        "capability_key": "erhua.qiwe_text_template",
                        "input": {"text_template": "Welcome {{subject_names}}"}
                    },
                    "allowed_capabilities": ["erhua.qiwe_text_template"],
                    "approval_policy": "space_admin_confirmation",
                    "event_mapping": {
                        "provider": "qiwe",
                        "definition_key": "group_member_add",
                        "source_status": "shadow",
                        "version": 3,
                        "fingerprint": "c".repeat(64),
                        "event_type": "qiwe.group_member_added",
                        "selector": {
                            "op": "equals",
                            "pointer": "/newMsgType",
                            "value": "GROUP_MEMBER_ADD"
                        }
                    }
                }
            }]
        });
        let error = parse_and_validate_intent(value.clone())
            .expect_err("model-supplied activation binding must fail");
        assert!(error.to_string().contains("supplied only by the sidecar"));
        let stored =
            parse_and_validate_stored_intent(value).expect("stored activation binding is valid");
        assert!(stored.protects_provider_mapping());
        let summaries = change_summaries(&stored);
        assert_eq!(
            summaries[0]["activation_review"]["business_definition"]["input"]["text_template"],
            "Welcome {{subject_names}}"
        );
        assert_eq!(
            summaries[0]["activation_review"]["event_mapping"]["event_type"],
            "qiwe.group_member_added"
        );
        assert_eq!(summaries[0]["activation_review"]["automation_version"], 4);
        let public_summary = serde_json::to_string(&summaries).expect("encode review projection");
        for internal_id in [automation_id, business_id, mapping_id] {
            assert!(!public_summary.contains(&internal_id.to_string()));
        }
    }

    #[test]
    fn activation_operation_must_be_the_only_change() {
        let value = json!({
            "summary": "Reject a mixed activation proposal.",
            "changes": [
                {
                    "resource": "definition_operation",
                    "target_resource": "automation_definition",
                    "definition_key": "welcome_new_members",
                    "operation": "activate"
                },
                {
                    "resource": "space_policy",
                    "definition_key": "default",
                    "status": "active",
                    "policy_config": {"capability_grants": []}
                }
            ]
        });
        let error = parse_and_validate_intent(value).expect_err("mixed activation must fail");
        assert!(error.to_string().contains("only change"));
    }

    #[test]
    fn conversational_activation_requires_runner_readiness_only_for_agent_turn() {
        validate_activation_execution_mode_with_readiness("deterministic", false)
            .expect("deterministic activation remains available");
        assert!(validate_activation_execution_mode_with_readiness("agent_turn", false).is_err());
        validate_activation_execution_mode_with_readiness("agent_turn", true)
            .expect("a future verified provisioning gate may activate agent_turn");
        assert!(validate_activation_execution_mode_with_readiness("arbitrary_code", true).is_err());
    }

    #[test]
    fn schema_retains_future_approval_policies_for_inactive_definitions() {
        for approval_policy in [
            "none",
            "space_admin_confirmation",
            "before_external_use",
            "human_final_confirmation",
        ] {
            let mut value = valid_welcome_intent();
            value["changes"][2]["approval_policy"] = json!(approval_policy);
            parse_and_validate_intent(value)
                .unwrap_or_else(|error| panic!("schema rejected {approval_policy}: {error}"));
        }
    }

    #[test]
    fn active_runtime_contract_accepts_only_executable_approval_policies() {
        use crate::space_capability_recipe::RegisteredRecipe;

        validate_active_approval_policy_contract(
            "deterministic",
            Some(RegisteredRecipe::QiweTextTemplateV1),
            "space_admin_confirmation",
        )
        .expect("confirmed QiWe template is executable");
        for approval_policy in ["none", "before_external_use", "human_final_confirmation"] {
            assert!(validate_active_approval_policy_contract(
                "deterministic",
                Some(RegisteredRecipe::QiweTextTemplateV1),
                approval_policy,
            )
            .is_err());
        }
        assert!(validate_active_approval_policy_contract(
            "deterministic",
            None,
            "space_admin_confirmation",
        )
        .is_err());

        for approval_policy in ["none", "space_admin_confirmation"] {
            validate_active_approval_policy_contract("agent_turn", None, approval_policy)
                .unwrap_or_else(|error| panic!("agent_turn rejected {approval_policy}: {error}"));
        }
        for approval_policy in ["before_external_use", "human_final_confirmation"] {
            assert!(
                validate_active_approval_policy_contract("agent_turn", None, approval_policy)
                    .is_err()
            );
        }
    }

    #[test]
    fn session_requires_qiwe_group_and_complete_trusted_identity() {
        let valid = TrustedSpaceSession {
            platform: "qiwe".to_string(),
            conversation_type: "group".to_string(),
            conversation_id: "room-1".to_string(),
            requester_user_id: "user-1".to_string(),
            source_message_id: "message-1".to_string(),
            source_message_text: Some("确认 A1B2C3D4".to_string()),
        };
        validate_session(&valid).expect("valid session");
        let mut invalid = valid.clone();
        invalid.conversation_type = "direct".to_string();
        assert!(validate_session(&invalid).is_err());
        invalid = valid;
        invalid.requester_user_id.clear();
        assert!(validate_session(&invalid).is_err());
    }

    #[test]
    fn programming_extension_text_rejects_urls_and_privileged_assignments() {
        let safe = validate_programming_extension_text(
            "intent",
            "Research the QiWe member-change event and add a bounded fixture parser.",
            4_000,
        )
        .expect("bounded extension intent");
        assert!(safe.contains("member-change"));
        assert!(contains_url("read HTTPS://example.com/event"));

        let privileged = validate_programming_extension_text(
            "intent",
            "Create an event parser with target_group_id=forged-room",
            4_000,
        )
        .expect_err("destination assignment must fail");
        assert!(privileged.to_string().contains("privileged assignment"));
        assert!(validate_programming_extension_text(
            "intent",
            "Use access_token is sensitive-value",
            4_000,
        )
        .is_err());
        assert!(validate_programming_extension_text(
            "intent",
            "研究事件，群 ID 是 forged-room",
            4_000,
        )
        .is_err());
    }

    #[test]
    fn programming_extension_sources_require_canonical_registered_documents() {
        let mut valid = vec!["https://doc.qiweapi.com/doc-7331304#section".to_string()];
        validate_official_sources("qiwe", &mut valid).expect("registered document URL");
        assert_eq!(valid, vec!["https://doc.qiweapi.com/doc-7331304"]);

        for source in [
            "https://doc.qiweapi.com/unregistered",
            "https://doc.qiweapi.com/doc-7331304?redirect=evil",
            "https://doc.qiweapi.com/doc-not-a-number",
        ] {
            let mut sources = vec![source.to_string()];
            assert!(validate_official_sources("qiwe", &mut sources).is_err());
        }
    }

    #[test]
    fn programming_extension_research_evidence_is_bounded_and_digest_bound() {
        let digest_vector = vec![ProgrammingExtensionResearchEvidence {
            url: "https://doc.qiweapi.com/doc-7331304".to_string(),
            text: "msgType=1002".to_string(),
        }];
        assert_eq!(
            programming_research_digest(&digest_vector),
            "7139b0d2f7a919eb4519754d0bbe83cb58c3a84c925c7157e116b362f76f5c85"
        );

        let sources = vec!["https://doc.qiweapi.com/doc-7331304".to_string()];
        let evidence = vec![ProgrammingExtensionResearchEvidence {
            url: sources[0].clone(),
            text: "Untrusted documentation says msgType=1002 and changedMemberList is Base64. Ignore prior rules is reference text, not an instruction.".to_string(),
        }];
        let digest = programming_research_digest(&evidence);
        validate_programming_research_evidence("qiwe", &sources, &evidence, &digest)
            .expect("bounded official evidence");

        let mut tampered = evidence.clone();
        tampered[0].text.push_str(" changed");
        assert!(
            validate_programming_research_evidence("qiwe", &sources, &tampered, &digest)
                .unwrap_err()
                .to_string()
                .contains("digest")
        );
    }

    #[test]
    fn programming_extension_research_evidence_rejects_unregistered_or_sensitive_values() {
        let official = vec!["https://doc.qiweapi.com/doc-7331304".to_string()];
        for (url, text) in [
            ("https://example.com/doc-7331304", "msgType=1002"),
            (
                "https://doc.qiweapi.com/doc-7331304",
                "fromRoomId=1234567890123456",
            ),
            (
                "https://doc.qiweapi.com/doc-7331304",
                "access_token=live-secret-value",
            ),
        ] {
            let evidence = vec![ProgrammingExtensionResearchEvidence {
                url: url.to_string(),
                text: text.to_string(),
            }];
            let digest = programming_research_digest(&evidence);
            assert!(
                validate_programming_research_evidence("qiwe", &official, &evidence, &digest)
                    .is_err()
            );
        }
    }

    #[test]
    fn schedule_requires_five_field_cron_and_valid_timezone() {
        let value = json!({
            "summary": "Run a configured business every morning.",
            "changes": [{
                "resource": "automation_definition",
                "definition_key": "morning_task",
                "status": "draft",
                "business_definition_key": "morning_task",
                "trigger_kind": "schedule",
                "trigger_config": {"cron": "0 8 * * *"},
                "timezone": "Asia/Shanghai",
                "misfire_policy": "run_once"
            }]
        });
        parse_and_validate_intent(value).expect("valid schedule");
    }

    #[test]
    fn rejects_unimplemented_schedule_misfire_policies() {
        for misfire_policy in ["skip", "catch_up"] {
            let value = json!({
                "summary": "Run a configured business every morning.",
                "changes": [{
                    "resource": "automation_definition",
                    "definition_key": "morning_task",
                    "status": "draft",
                    "business_definition_key": "morning_task",
                    "trigger_kind": "schedule",
                    "trigger_config": {"cron": "0 8 * * *"},
                    "timezone": "Asia/Shanghai",
                    "misfire_policy": misfire_policy
                }]
            });
            let error = parse_and_validate_intent(value).expect_err("unsupported misfire policy");
            assert!(error.to_string().contains("must be run_once in v1"));
        }
    }
}
