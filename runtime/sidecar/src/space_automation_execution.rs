use std::{collections::BTreeSet, time::Duration};

use anyhow::{anyhow, bail, Context, Result};
#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
use serde::Deserialize;
use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
use url::Url;
#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
use zeroize::{Zeroize, Zeroizing};

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
use crate::bounded_http::HttpClient;
#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
use crate::bounded_http::HttpResponse;
use crate::{config::Cli, db};

const WORKER_ID: &str = "space-automation-execution-worker";
const EXECUTION_CAPABILITY_KEY: &str = "erhua.execute_space_business";
#[cfg(test)]
const QIWE_TEXT_TEMPLATE_CAPABILITY_KEY: &str = "erhua.qiwe_text_template";
const AGENT_TURN_CAPABILITY_KEY: &str = "erhua.space_agent_turn";
const WORK_ITEM_TYPE: &str = "space_automation_run";
const AGENT_TURN_WORK_ITEM_TYPE: &str = "space_agent_turn";
const AGENT_TURN_BRIEF_SUMMARY: &str = "Execute one bounded, version-bound Space Agent turn.";
const AGENT_TURN_PURPOSE: &str = "space_agent_turn";
const AGENT_TURN_SOURCE_TYPE: &str = "space_automation_execution";
const AGENT_TURN_RISK_LEVEL: &str = "medium";
const AGENT_TURN_INFORMATION_CLASS: &str = "internal_ops";
const AGENT_TURN_PAYLOAD_REDACTION_POLICY: &str = "summary_only";
const AGENT_TURN_REVIEW_POLICY: &str = "definition_policy";
const MAX_TEMPLATE_CHARS: usize = 4_000;
const MAX_SUBJECTS: usize = 64;
const MAX_SUBJECT_ID_BYTES: usize = 256;
#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
const MAX_NAME_CHARS: usize = 200;
#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
const MAX_JSON_RESPONSE_BYTES: usize = 256 * 1024;
const CLAIM_TTL_SECONDS: i64 = 300;
const SUBJECT_NAMES_PLACEHOLDER: &str = "{{subject_names}}";
#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
const PERSIST_ROOM_DISPLAY_NAME_SQL: &str = r#"
    UPDATE qintopia_messages.conversations
    SET display_name = $3, updated_at = now()
    WHERE id = $1 AND platform = 'qiwe' AND chat_type = 'group'
      AND status = 'active' AND chat_id = $2
"#;
const EXECUTION_ENABLE_ENV: &str = "QINTOPIA_SPACE_AUTOMATION_EXECUTION_ENABLED";
const EXECUTION_APPROVAL_ENV: &str = "QINTOPIA_SPACE_AUTOMATION_EXECUTION_APPROVAL";
const EXECUTION_APPROVAL_PHRASE: &str = "approved-production-space-automation-execution";
const DATABASE_URL_SHA256_ENV: &str = "QINTOPIA_SPACE_AUTOMATION_EXECUTION_DATABASE_URL_SHA256";
#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
const QIWE_ALLOWED_HOSTS_ENV: &str = "QINTOPIA_SPACE_AUTOMATION_QIWE_ALLOWED_HOSTS";
#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
const QIWE_ROOM_DETAIL_METHOD: &str = "/room/batchGetRoomDetail";
#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
const QIWE_TEXT_SEND_METHOD: &str = "/msg/sendHyperText";

#[derive(Debug, Clone)]
pub struct ExecutionWorkerOptions {
    pub once: bool,
    pub apply: bool,
    pub dry_run: bool,
    pub work_item_id: Option<Uuid>,
    pub poll_seconds: u64,
}

#[derive(Debug, Serialize)]
struct ExecutionWorkerReport {
    success: bool,
    dry_run: bool,
    apply_requested: bool,
    worker: &'static str,
    action_status: String,
    work_item_id: Option<Uuid>,
    space_id: Option<Uuid>,
    automation_definition_id: Option<Uuid>,
    business_definition_id: Option<Uuid>,
    execution_mode: Option<String>,
    selected_capability_key: Option<String>,
    child_work_item_id: Option<Uuid>,
    subject_count: usize,
    current_subject_count: usize,
    target_derived_from_space: bool,
    external_send_executed: Option<bool>,
    automatic_retry_allowed: bool,
    limitations: Vec<String>,
    guardrails: Vec<String>,
}

#[derive(Debug, Clone)]
struct ExecutionClaim {
    work_item_id: Uuid,
    space_id: Uuid,
    payload: Value,
    automation_id: Uuid,
    automation_key: String,
    automation_version: i32,
    automation_status: String,
    automation_digest: String,
    trigger_kind: String,
    channel_event_mapping_id: Option<Uuid>,
    channel_event_mapping_digest: Option<String>,
    business_id: Uuid,
    business_status: String,
    business_digest: String,
    execution_mode: String,
    business_definition: Value,
    business_allowed_capabilities: Vec<String>,
    approval_policy: String,
    policy_id: Uuid,
    policy_digest: String,
    policy_config: Value,
    selected_capability_key: String,
    selected_capability_metadata: Value,
    conversation_chat_id: String,
    attempt_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
struct QiweTextTemplatePlan {
    #[cfg(any(
        test,
        feature = "qiwe-staging-adapter",
        feature = "qiwe-production-adapter"
    ))]
    text_template: String,
    #[cfg(any(
        test,
        feature = "qiwe-staging-adapter",
        feature = "qiwe-production-adapter"
    ))]
    subject_name_separator: String,
    subject_user_ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct AgentTurnPlan {
    goal: String,
    allowed_capabilities: Vec<String>,
    output_contract: Value,
}

#[derive(Debug, Clone)]
struct AgentTurnChildSpec {
    id: Uuid,
    parent_work_item_id: Uuid,
    space_id: Uuid,
    source_refs: Value,
    idempotency_key: String,
    payload: Value,
    metadata: Value,
}

impl AgentTurnChildSpec {
    fn immutable_tuple(&self) -> Value {
        json!({
            "parent_work_item_id": self.parent_work_item_id,
            "space_id": self.space_id,
            "work_item_type": AGENT_TURN_WORK_ITEM_TYPE,
            "status": "queued",
            "requester_agent": "system",
            "target_agent": "erhua",
            "capability_key": AGENT_TURN_CAPABILITY_KEY,
            "human_owner": "",
            "priority": "normal",
            "brief_summary": AGENT_TURN_BRIEF_SUMMARY,
            "purpose": AGENT_TURN_PURPOSE,
            "source_type": AGENT_TURN_SOURCE_TYPE,
            "source_refs": self.source_refs,
            "dedupe_key": self.idempotency_key,
            "idempotency_key": self.idempotency_key,
            "risk_level": AGENT_TURN_RISK_LEVEL,
            "information_class": AGENT_TURN_INFORMATION_CLASS,
            "payload": self.payload,
            "payload_redaction_policy": AGENT_TURN_PAYLOAD_REDACTION_POLICY,
            "review_policy": AGENT_TURN_REVIEW_POLICY,
            "metadata": self.metadata,
            "attempts": 0,
            "claim_state_clean": true
        })
    }
}

#[derive(Debug, Clone)]
enum ExecutionPlan {
    Shadow,
    QiweTextTemplate(QiweTextTemplatePlan),
    AgentTurn(AgentTurnPlan),
}

#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
#[derive(Debug, Clone)]
struct RoomMember {
    display_name: Option<String>,
}

#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
#[derive(Debug, Clone, Default)]
struct RoomRoster {
    display_name: String,
    members: std::collections::BTreeMap<String, RoomMember>,
}

#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
#[derive(Debug)]
enum SendOutcome {
    Sent {
        response_summary: Value,
    },
    FailedBeforeSend {
        failure_code: &'static str,
    },
    Ambiguous {
        failure_code: &'static str,
        response_summary: Option<Value>,
    },
}

#[derive(Debug, Clone, Copy)]
enum LiveAuthorizationGate {
    AgentTurnHandoff,
    #[cfg(any(
        feature = "qiwe-staging-adapter",
        feature = "qiwe-production-adapter",
        all(test, feature = "postgres-integration-tests")
    ))]
    QiweRosterAccess,
    #[cfg(any(
        feature = "qiwe-staging-adapter",
        feature = "qiwe-production-adapter",
        all(test, feature = "postgres-integration-tests")
    ))]
    QiweSend,
}

impl LiveAuthorizationGate {
    fn authorization_state(self) -> &'static str {
        match self {
            Self::AgentTurnHandoff => "handoff_committed",
            #[cfg(any(
                feature = "qiwe-staging-adapter",
                feature = "qiwe-production-adapter",
                all(test, feature = "postgres-integration-tests")
            ))]
            Self::QiweRosterAccess => "roster_access_authorized",
            #[cfg(any(
                feature = "qiwe-staging-adapter",
                feature = "qiwe-production-adapter",
                all(test, feature = "postgres-integration-tests")
            ))]
            Self::QiweSend => "send_committed",
        }
    }

    fn external_send_outcome(self) -> &'static str {
        match self {
            #[cfg(any(
                feature = "qiwe-staging-adapter",
                feature = "qiwe-production-adapter",
                all(test, feature = "postgres-integration-tests")
            ))]
            Self::QiweSend => "sending",
            Self::AgentTurnHandoff => "not_sent",
            #[cfg(any(
                feature = "qiwe-staging-adapter",
                feature = "qiwe-production-adapter",
                all(test, feature = "postgres-integration-tests")
            ))]
            Self::QiweRosterAccess => "not_sent",
        }
    }
}

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
#[derive(Debug)]
struct QiweAdapterConfig {
    api_url: Url,
    token: String,
    guid: String,
}

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
impl Drop for QiweAdapterConfig {
    fn drop(&mut self) {
        self.token.zeroize();
        self.guid.zeroize();
    }
}

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
#[derive(Serialize)]
struct QiweApiRequest<T> {
    method: &'static str,
    params: T,
}

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RoomDetailParams<'a> {
    guid: &'a str,
    room_id_list: [&'a str; 1],
}

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SendTextParams<'a> {
    guid: &'a str,
    #[serde(rename = "toId")]
    to_id: &'a str,
    #[serde(rename = "isNoNeedRead")]
    is_no_need_read: bool,
    content: [HyperTextSegment<'a>; 1],
}

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
#[derive(Serialize)]
struct HyperTextSegment<'a> {
    #[serde(rename = "type")]
    segment_type: &'static str,
    text: &'a str,
}

#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
#[derive(Deserialize)]
struct QiweApiResponse {
    code: Option<i64>,
    data: Option<Value>,
    msg: Option<String>,
}

#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
#[derive(Deserialize)]
struct QiweTextSendResult {
    #[serde(rename = "isSendSuccess")]
    is_send_success: i64,
}

pub async fn run(cli: &Cli, options: ExecutionWorkerOptions) -> Result<()> {
    if options.apply == options.dry_run {
        bail!("choose exactly one of --apply or --dry-run");
    }
    if !options.once && options.work_item_id.is_some() {
        bail!("--work-item-id requires --once");
    }
    if !options.once && options.poll_seconds == 0 {
        bail!("Space automation execution poll_seconds must be positive");
    }

    let database_url = cli.database_url_required()?;
    if options.apply {
        validate_execution_compile_boundary()?;
        validate_execution_boundary(database_url)?;
        crate::space_agent_turn::runtime_readiness()?;
    }
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    db::run_migrations(&pool).await?;

    loop {
        if options.apply {
            reconcile_stale_attempts(&pool).await?;
        }
        let report = if options.apply {
            execute_one(&pool, cli, options.work_item_id).await?
        } else {
            preview_one(&pool, options.work_item_id).await?
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        if options.once {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(options.poll_seconds)).await;
    }
}

async fn preview_one(pool: &PgPool, work_item_id: Option<Uuid>) -> Result<ExecutionWorkerReport> {
    let Some(claim) = load_eligible_claim(pool, work_item_id).await? else {
        return Ok(empty_report(false, "no_claimable_space_automation_run"));
    };
    let plan = validate_claim_and_build_plan(&claim)?;
    Ok(report_for_plan(
        &claim,
        &plan,
        false,
        "dry_run_ok",
        None,
        0,
        Some(false),
    ))
}

async fn execute_one(
    pool: &PgPool,
    cli: &Cli,
    work_item_id: Option<Uuid>,
) -> Result<ExecutionWorkerReport> {
    let Some(claim) = claim_one(pool, work_item_id).await? else {
        return Ok(empty_report(true, "no_claimable_space_automation_run"));
    };
    let plan = match validate_claim_and_build_plan(&claim) {
        Ok(plan) => plan,
        Err(error) => {
            terminal_failure(
                pool,
                &claim,
                "definition_contract_invalid",
                false,
                Some(error.to_string()),
            )
            .await?;
            return Ok(report_for_claim(
                &claim,
                true,
                "definition_contract_invalid",
                None,
                0,
                Some(false),
            ));
        }
    };

    match plan {
        ExecutionPlan::Shadow => {
            terminal_success(
                pool,
                &claim,
                "space_automation_shadow_observed",
                None,
                0,
                false,
            )
            .await?;
            Ok(report_for_claim(
                &claim,
                true,
                "space_automation_shadow_observed",
                None,
                0,
                Some(false),
            ))
        }
        ExecutionPlan::AgentTurn(plan) => execute_agent_turn(pool, &claim, plan).await,
        ExecutionPlan::QiweTextTemplate(plan) => {
            execute_qiwe_text_template(pool, cli, &claim, plan).await
        }
    }
}

async fn execute_agent_turn(
    pool: &PgPool,
    claim: &ExecutionClaim,
    plan: AgentTurnPlan,
) -> Result<ExecutionWorkerReport> {
    let mut tx = pool
        .begin()
        .await
        .context("begin Space agent-turn handoff transaction")?;
    let live_policy_config =
        authorize_live_claim_in_tx(&mut tx, claim, LiveAuthorizationGate::AgentTurnHandoff).await?;
    let enabled_capabilities = load_enabled_capability_intersection(
        &mut tx,
        &plan.allowed_capabilities,
        &policy_capability_grants(&live_policy_config)?,
    )
    .await?;

    let child_idempotency_key = format!("space-agent-turn:{}", claim.work_item_id);
    let trigger = bounded_trigger_for_handoff(&claim.payload)?;
    let child = AgentTurnChildSpec {
        id: Uuid::new_v4(),
        parent_work_item_id: claim.work_item_id,
        space_id: claim.space_id,
        source_refs: json!({
            "automation_definition_id": claim.automation_id,
            "business_definition_id": claim.business_id,
            "parent_work_item_id": claim.work_item_id,
            "channel_event_mapping_id": claim.channel_event_mapping_id,
            "channel_event_mapping_digest": claim.channel_event_mapping_digest
        }),
        idempotency_key: child_idempotency_key,
        payload: json!({
            "schema_version": 1,
            "automation_definition_id": claim.automation_id,
            "automation_definition_digest": claim.automation_digest,
            "business_definition_id": claim.business_id,
            "business_definition_digest": claim.business_digest,
            "space_policy_version_id": claim.policy_id,
            "space_policy_digest": claim.policy_digest,
            "channel_event_mapping_id": claim.channel_event_mapping_id,
            "channel_event_mapping_digest": claim.channel_event_mapping_digest,
            "goal": plan.goal,
            "trigger": trigger,
            "allowed_capabilities": enabled_capabilities,
            "output_contract": plan.output_contract
        }),
        metadata: json!({
            "space_bound": true,
            "definition_bound": true,
            "target_derived_from_space": true,
            "external_send_executed": false,
            "unrestricted_model_invocation": false,
            "handoff_state": crate::space_agent_turn::HANDOFF_STATE,
            "executor_boundary": crate::space_agent_turn::EXECUTOR_BOUNDARY,
            "runner_identity": crate::space_agent_turn::RUNNER_IDENTITY,
            "runner_contract_version": crate::space_agent_turn::RUNNER_CONTRACT_VERSION,
            "execution_gate": "owner_review_required"
        }),
    };
    let expected_immutable_tuple = child.immutable_tuple();
    let inserted = sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_items
            (id, parent_work_item_id, space_id, work_item_type, status,
             requester_agent, target_agent, capability_key, human_owner, priority,
             available_at, brief_summary, purpose, source_type, source_refs,
             dedupe_key, idempotency_key, risk_level, information_class, payload,
             payload_redaction_policy, review_policy, metadata)
        VALUES
            ($1, $2, $3, 'space_agent_turn', 'queued',
             'system', 'erhua', 'erhua.space_agent_turn', '', 'normal', now(),
             $4,
             'space_agent_turn', 'space_automation_execution', $5,
             $6, $6, 'medium', 'internal_ops', $7,
             'summary_only', 'definition_policy', $8)
        ON CONFLICT (idempotency_key) DO NOTHING
        "#,
    )
    .bind(child.id)
    .bind(child.parent_work_item_id)
    .bind(child.space_id)
    .bind(AGENT_TURN_BRIEF_SUMMARY)
    .bind(&child.source_refs)
    .bind(&child.idempotency_key)
    .bind(&child.payload)
    .bind(&child.metadata)
    .execute(&mut *tx)
    .await
    .context("create constrained Space agent-turn work item")?;

    let resolved_child_id = if inserted.rows_affected() == 1 {
        child.id
    } else {
        let row = sqlx::query(
            r#"
            SELECT id,
                   jsonb_build_object(
                       'parent_work_item_id', parent_work_item_id::text,
                       'space_id', space_id::text,
                       'work_item_type', work_item_type,
                       'status', status,
                       'requester_agent', requester_agent,
                       'target_agent', target_agent,
                       'capability_key', capability_key,
                       'human_owner', human_owner,
                       'priority', priority,
                       'brief_summary', brief_summary,
                       'purpose', purpose,
                       'source_type', source_type,
                       'source_refs', source_refs,
                       'dedupe_key', dedupe_key,
                       'idempotency_key', idempotency_key,
                       'risk_level', risk_level,
                       'information_class', information_class,
                       'payload', payload,
                       'payload_redaction_policy', payload_redaction_policy,
                       'review_policy', review_policy,
                       'metadata', metadata,
                       'attempts', attempts,
                       'claim_state_clean', claimed_by IS NULL
                           AND locked_at IS NULL
                           AND claim_expires_at IS NULL
                   ) AS immutable_tuple
            FROM qintopia_agent_os.work_items
            WHERE idempotency_key = $1
            FOR UPDATE
            "#,
        )
        .bind(&child.idempotency_key)
        .fetch_one(&mut *tx)
        .await
        .context("load existing constrained Space agent-turn work item")?;
        let existing_tuple: Value = row.try_get("immutable_tuple")?;
        validate_agent_turn_child_immutable_tuple(&existing_tuple, &expected_immutable_tuple)?;
        row.try_get("id")?
    };
    complete_claim_in_tx(
        &mut tx,
        claim,
        "space_agent_turn_queued_for_runner",
        json!({
            "child_work_item_id": resolved_child_id,
            "handoff_state": crate::space_agent_turn::HANDOFF_STATE,
            "runner_identity": crate::space_agent_turn::RUNNER_IDENTITY,
            "runner_contract_version": crate::space_agent_turn::RUNNER_CONTRACT_VERSION,
            "output_contract_sha256": crate::space_agent_turn::output_contract_digest(
                child.payload.get("output_contract").context("agent-turn child output contract is missing")?
            )?,
            "external_send_executed": false,
            "automatic_retry_allowed": false
        }),
    )
    .await?;
    tx.commit()
        .await
        .context("commit constrained Space agent-turn handoff")?;

    Ok(report_for_claim(
        claim,
        true,
        "space_agent_turn_queued_for_runner",
        Some(resolved_child_id),
        0,
        Some(false),
    ))
}

fn validate_agent_turn_child_immutable_tuple(existing: &Value, expected: &Value) -> Result<()> {
    if existing != expected {
        bail!("Space agent-turn idempotency key belongs to a different immutable child");
    }
    Ok(())
}

async fn execute_qiwe_text_template(
    pool: &PgPool,
    cli: &Cli,
    claim: &ExecutionClaim,
    plan: QiweTextTemplatePlan,
) -> Result<ExecutionWorkerReport> {
    #[cfg(not(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter")))]
    {
        let _ = (cli, plan);
        terminal_failure(pool, claim, "qiwe_adapter_not_compiled", false, None).await?;
        Ok(report_for_claim(
            claim,
            true,
            "qiwe_adapter_not_compiled",
            None,
            0,
            Some(false),
        ))
    }

    #[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
    {
        let adapter = match QiweAdapterConfig::from_cli(cli) {
            Ok(adapter) => adapter,
            Err(error) => {
                terminal_failure(
                    pool,
                    claim,
                    "qiwe_adapter_configuration_invalid",
                    false,
                    Some(error.to_string()),
                )
                .await?;
                return Ok(report_for_claim(
                    claim,
                    true,
                    "qiwe_adapter_configuration_invalid",
                    None,
                    0,
                    Some(false),
                ));
            }
        };
        let client = HttpClient::production();
        let (message_text, current_subject_count) = if plan.subject_user_ids.is_empty() {
            match render_qiwe_text_template(&plan, &[]) {
                Ok(text) => (text, 0),
                Err(error) => {
                    terminal_failure(
                        pool,
                        claim,
                        "text_template_render_failed",
                        false,
                        Some(error.to_string()),
                    )
                    .await?;
                    return Ok(report_for_claim(
                        claim,
                        true,
                        "text_template_render_failed",
                        None,
                        0,
                        Some(false),
                    ));
                }
            }
        } else {
            if let Err(error) = commit_qiwe_roster_access_gate(pool, claim).await {
                terminal_failure(
                    pool,
                    claim,
                    "space_automation_authorization_revoked_before_roster",
                    false,
                    Some(error.to_string()),
                )
                .await?;
                return Ok(report_for_claim(
                    claim,
                    true,
                    "space_automation_authorization_revoked_before_roster",
                    None,
                    0,
                    Some(false),
                ));
            }
            let roster =
                match load_exact_room_roster(&adapter, &claim.conversation_chat_id, &client) {
                    Ok(roster) => roster,
                    Err(error) => {
                        terminal_failure(
                            pool,
                            claim,
                            "room_roster_verification_failed",
                            false,
                            Some(error.to_string()),
                        )
                        .await?;
                        return Ok(report_for_claim(
                            claim,
                            true,
                            "room_roster_verification_failed",
                            None,
                            0,
                            Some(false),
                        ));
                    }
                };
            if let Err(error) = persist_room_display_name(pool, claim, &roster.display_name).await {
                terminal_failure(
                    pool,
                    claim,
                    "room_display_name_persistence_failed",
                    false,
                    Some(error.to_string()),
                )
                .await?;
                return Ok(report_for_claim(
                    claim,
                    true,
                    "room_display_name_persistence_failed",
                    None,
                    0,
                    Some(false),
                ));
            }
            let names = match current_subject_names(&plan.subject_user_ids, &roster) {
                Ok(names) => names,
                Err(error) => {
                    terminal_failure(
                        pool,
                        claim,
                        "room_subject_resolution_failed",
                        false,
                        Some(error.to_string()),
                    )
                    .await?;
                    return Ok(report_for_claim(
                        claim,
                        true,
                        "room_subject_resolution_failed",
                        None,
                        0,
                        Some(false),
                    ));
                }
            };
            if names.is_empty() {
                terminal_success(
                    pool,
                    claim,
                    "space_automation_no_current_subjects",
                    None,
                    0,
                    false,
                )
                .await?;
                return Ok(report_for_claim(
                    claim,
                    true,
                    "space_automation_no_current_subjects",
                    None,
                    0,
                    Some(false),
                ));
            }
            let text = match render_qiwe_text_template(&plan, &names) {
                Ok(text) => text,
                Err(error) => {
                    terminal_failure(
                        pool,
                        claim,
                        "text_template_render_failed",
                        false,
                        Some(error.to_string()),
                    )
                    .await?;
                    return Ok(report_for_claim(
                        claim,
                        true,
                        "text_template_render_failed",
                        None,
                        names.len(),
                        Some(false),
                    ));
                }
            };
            (text, names.len())
        };

        if let Err(error) = commit_qiwe_send_gate(pool, claim).await {
            terminal_failure(
                pool,
                claim,
                "space_automation_authorization_revoked_before_send",
                false,
                Some(error.to_string()),
            )
            .await?;
            return Ok(report_for_claim(
                claim,
                true,
                "space_automation_authorization_revoked_before_send",
                None,
                current_subject_count,
                Some(false),
            ));
        }

        match send_qiwe_text(
            &adapter,
            &claim.conversation_chat_id,
            &message_text,
            &client,
        ) {
            SendOutcome::Sent { response_summary } => {
                terminal_success(
                    pool,
                    claim,
                    "space_automation_qiwe_text_sent",
                    Some(json!({
                        "message_sha256": sha256_hex(message_text.as_bytes()),
                        "subject_count": plan.subject_user_ids.len(),
                        "current_subject_count": current_subject_count,
                        "qiwe_response": response_summary
                    })),
                    current_subject_count,
                    true,
                )
                .await?;
                Ok(report_for_claim(
                    claim,
                    true,
                    "space_automation_qiwe_text_sent",
                    None,
                    current_subject_count,
                    Some(true),
                ))
            }
            SendOutcome::FailedBeforeSend { failure_code } => {
                terminal_failure(pool, claim, failure_code, false, None).await?;
                Ok(report_for_claim(
                    claim,
                    true,
                    failure_code,
                    None,
                    current_subject_count,
                    Some(false),
                ))
            }
            SendOutcome::Ambiguous {
                failure_code,
                response_summary,
            } => {
                terminal_ambiguous(pool, claim, failure_code, response_summary).await?;
                Ok(report_for_claim(
                    claim,
                    true,
                    "space_automation_qiwe_send_ambiguous",
                    None,
                    current_subject_count,
                    None,
                ))
            }
        }
    }
}

fn validate_claim_and_build_plan(claim: &ExecutionClaim) -> Result<ExecutionPlan> {
    let agent_turn_runtime_ready =
        if claim.execution_mode == "agent_turn" && claim.automation_status != "shadow" {
            crate::space_agent_turn::runtime_readiness()?
        } else {
            false
        };
    validate_claim_and_build_plan_with_readiness(claim, agent_turn_runtime_ready)
}

fn validate_claim_and_build_plan_with_readiness(
    claim: &ExecutionClaim,
    agent_turn_runtime_ready: bool,
) -> Result<ExecutionPlan> {
    validate_claim_binding(claim)?;
    if claim.automation_status == "shadow" {
        return Ok(ExecutionPlan::Shadow);
    }
    match claim.execution_mode.as_str() {
        "deterministic" => {
            match crate::space_capability_recipe::from_capability_metadata(
                &claim.selected_capability_metadata,
            )? {
                crate::space_capability_recipe::RegisteredRecipe::QiweTextTemplateV1 => {
                    if claim.approval_policy != "space_admin_confirmation" {
                        bail!("QiWe text-template execution requires space_admin_confirmation");
                    }
                    Ok(ExecutionPlan::QiweTextTemplate(qiwe_text_template_plan(
                        claim,
                    )?))
                }
            }
        }
        "agent_turn" => {
            if !agent_turn_runtime_ready {
                bail!("agent_turn execution requires owner-reviewed broker and runner readiness");
            }
            if claim.selected_capability_key != AGENT_TURN_CAPABILITY_KEY {
                bail!("agent_turn is not bound to the constrained handoff capability");
            }
            if !matches!(
                claim.approval_policy.as_str(),
                "none" | "space_admin_confirmation"
            ) {
                bail!("agent_turn approval policy requires an unsupported per-run review");
            }
            Ok(ExecutionPlan::AgentTurn(agent_turn_plan(claim)?))
        }
        _ => bail!("business execution_mode is not registered"),
    }
}

fn validate_claim_binding(claim: &ExecutionClaim) -> Result<()> {
    if claim.automation_status == "active" && claim.business_status != "active" {
        bail!("active automation is bound to a stale business definition");
    }
    if claim.automation_status == "shadow"
        && !matches!(claim.business_status.as_str(), "active" | "shadow")
    {
        bail!("shadow automation is bound to a stale business definition");
    }
    if !matches!(claim.automation_status.as_str(), "active" | "shadow") {
        bail!("automation definition is stale");
    }
    if claim.automation_id.to_string() != required_text(&claim.payload, "automation_definition_id")?
    {
        bail!("work item automation definition binding is stale");
    }
    if claim.business_id.to_string() != required_text(&claim.payload, "business_definition_id")? {
        bail!("work item business definition binding is stale");
    }
    if claim.automation_digest != required_text(&claim.payload, "automation_definition_digest")? {
        bail!("work item automation definition digest binding is stale");
    }
    if claim.business_digest != required_text(&claim.payload, "business_definition_digest")? {
        bail!("work item business definition digest binding is stale");
    }
    if claim.policy_id.to_string() != required_text(&claim.payload, "space_policy_version_id")? {
        bail!("work item Space policy version binding is stale");
    }
    if claim.policy_digest != required_text(&claim.payload, "space_policy_digest")? {
        bail!("work item Space policy digest binding is stale");
    }
    if claim.automation_key != required_text(&claim.payload, "automation_key")? {
        bail!("work item automation key binding is stale");
    }
    let payload_version = claim
        .payload
        .get("automation_version")
        .and_then(Value::as_i64)
        .context("work item automation_version is missing")?;
    if payload_version != i64::from(claim.automation_version) {
        bail!("work item automation version binding is stale");
    }
    let trigger = claim
        .payload
        .get("trigger")
        .and_then(Value::as_object)
        .context("work item trigger is missing")?;
    if trigger.get("kind").and_then(Value::as_str) != Some(claim.trigger_kind.as_str()) {
        bail!("work item trigger kind does not match automation definition");
    }
    if claim.trigger_kind == "event"
        && (claim.channel_event_mapping_id.is_none()
            || claim
                .channel_event_mapping_digest
                .as_deref()
                .is_none_or(|digest| !valid_definition_digest(digest)))
    {
        bail!("event automation is missing an exact event-mapping binding");
    }
    if claim.trigger_kind == "event" {
        let payload_mapping_id = required_text(&claim.payload, "channel_event_mapping_id")?;
        if claim
            .channel_event_mapping_id
            .map(|id| id.to_string())
            .as_deref()
            != Some(payload_mapping_id.as_str())
        {
            bail!("work item event-mapping id binding is stale");
        }
        let payload_mapping_digest = required_text(&claim.payload, "channel_event_mapping_digest")?;
        if claim.channel_event_mapping_digest.as_deref() != Some(payload_mapping_digest.as_str()) {
            bail!("work item event-mapping digest binding is stale");
        }
    }
    if claim.trigger_kind == "schedule"
        && (claim.channel_event_mapping_id.is_some()
            || claim.channel_event_mapping_digest.is_some())
    {
        bail!("schedule automation unexpectedly carries an event-mapping binding");
    }
    if claim.conversation_chat_id.trim().is_empty() {
        bail!("Space conversation has no QiWe chat target");
    }
    if !claim
        .business_allowed_capabilities
        .iter()
        .any(|key| key == &claim.selected_capability_key)
    {
        bail!("selected capability is outside the business definition ceiling");
    }
    let grants = policy_capability_grants(&claim.policy_config)?;
    if !grants.contains(&claim.selected_capability_key) {
        bail!("selected capability is outside the active Space policy ceiling");
    }
    Ok(())
}

fn valid_definition_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn qiwe_text_template_plan(claim: &ExecutionClaim) -> Result<QiweTextTemplatePlan> {
    let definition = claim
        .business_definition
        .as_object()
        .context("business definition must be an object")?;
    if definition.get("capability_key").and_then(Value::as_str)
        != Some(claim.selected_capability_key.as_str())
    {
        bail!("business definition capability_key does not match the registered recipe");
    }
    let input = definition
        .get("input")
        .and_then(Value::as_object)
        .context("QiWe text-template definition.input must be an object")?;
    let text_template = required_object_text(input, "text_template")?;
    validate_text_template(&text_template)?;
    let subject_name_separator = input
        .get("subject_name_separator")
        .and_then(Value::as_str)
        .unwrap_or("、")
        .to_string();
    if subject_name_separator.chars().count() > 16
        || subject_name_separator.chars().any(char::is_control)
    {
        bail!("subject_name_separator is invalid");
    }
    let subject_user_ids = canonical_subject_user_ids(&claim.payload, &claim.trigger_kind)?;
    if text_template.contains(SUBJECT_NAMES_PLACEHOLDER) && subject_user_ids.is_empty() {
        bail!("subject_names placeholder requires canonical event subject_user_ids");
    }
    Ok(QiweTextTemplatePlan {
        #[cfg(any(
            test,
            feature = "qiwe-staging-adapter",
            feature = "qiwe-production-adapter"
        ))]
        text_template,
        #[cfg(any(
            test,
            feature = "qiwe-staging-adapter",
            feature = "qiwe-production-adapter"
        ))]
        subject_name_separator,
        subject_user_ids,
    })
}

fn agent_turn_plan(claim: &ExecutionClaim) -> Result<AgentTurnPlan> {
    let definition = claim
        .business_definition
        .as_object()
        .context("agent_turn definition must be an object")?;
    let goal = required_object_text(definition, "goal")?;
    if goal.chars().count() > MAX_TEMPLATE_CHARS || goal.chars().any(char::is_control) {
        bail!("agent_turn goal is invalid");
    }
    let output_contract = definition
        .get("output_contract")
        .cloned()
        .context("agent_turn output_contract is required")?;
    crate::space_agent_turn::validate_output_contract(&output_contract)?;
    Ok(AgentTurnPlan {
        goal,
        allowed_capabilities: claim.business_allowed_capabilities.clone(),
        output_contract,
    })
}

fn validate_text_template(template: &str) -> Result<()> {
    let trimmed = template.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_TEMPLATE_CHARS {
        bail!("text_template must contain between 1 and {MAX_TEMPLATE_CHARS} characters");
    }
    if trimmed.chars().any(char::is_control) {
        bail!("text_template contains control characters");
    }
    let without_subjects = trimmed.replace(SUBJECT_NAMES_PLACEHOLDER, "");
    if without_subjects.contains("{{") || without_subjects.contains("}}") {
        bail!("text_template contains an unsupported placeholder");
    }
    Ok(())
}

fn canonical_subject_user_ids(payload: &Value, trigger_kind: &str) -> Result<Vec<String>> {
    let trigger = payload
        .get("trigger")
        .and_then(Value::as_object)
        .context("work item trigger is missing")?;
    let subject_values = trigger.get("subject_user_ids");
    if trigger_kind != "event" {
        if subject_values
            .and_then(Value::as_array)
            .is_some_and(|values| !values.is_empty())
        {
            bail!("schedule trigger cannot carry subject_user_ids");
        }
        return Ok(Vec::new());
    }
    let values = subject_values
        .and_then(Value::as_array)
        .context("event trigger subject_user_ids must be an array")?;
    if values.len() > MAX_SUBJECTS {
        bail!("event trigger contains too many subject_user_ids");
    }
    let mut seen = BTreeSet::new();
    let mut subjects = Vec::new();
    for value in values {
        let subject = value
            .as_str()
            .context("canonical subject_user_ids entries must be strings")?
            .to_string();
        validate_opaque_user_id(&subject)?;
        if seen.insert(subject.clone()) {
            subjects.push(subject);
        }
    }
    Ok(subjects)
}

fn validate_opaque_user_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > MAX_SUBJECT_ID_BYTES
        || value.trim() != value
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        bail!("canonical subject_user_id is invalid");
    }
    Ok(())
}

#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
fn current_subject_names(subjects: &[String], roster: &RoomRoster) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for subject in subjects {
        let Some(member) = roster.members.get(subject) else {
            continue;
        };
        let name = member
            .display_name
            .as_deref()
            .context("current room subject is missing a display name")?;
        validate_display_name(name)?;
        names.push(name.to_string());
    }
    Ok(names)
}

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
async fn persist_room_display_name(
    pool: &PgPool,
    claim: &ExecutionClaim,
    display_name: &str,
) -> Result<()> {
    validate_display_name(display_name)?;
    let updated = sqlx::query(PERSIST_ROOM_DISPLAY_NAME_SQL)
        .bind(claim.space_id)
        .bind(&claim.conversation_chat_id)
        .bind(display_name)
        .execute(pool)
        .await
        .context("persist exact QiWe room display name")?;
    if updated.rows_affected() != 1 {
        bail!("exact Space conversation changed before room-name persistence");
    }
    Ok(())
}

#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
fn validate_display_name(value: &str) -> Result<()> {
    if value.trim().is_empty()
        || value.trim() != value
        || value.chars().count() > MAX_NAME_CHARS
        || value.chars().any(char::is_control)
    {
        bail!("current room member display name is invalid");
    }
    Ok(())
}

#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
fn render_qiwe_text_template(plan: &QiweTextTemplatePlan, names: &[String]) -> Result<String> {
    let joined_names = names.join(&plan.subject_name_separator);
    let rendered = plan
        .text_template
        .replace(SUBJECT_NAMES_PLACEHOLDER, &joined_names);
    if rendered.trim().is_empty()
        || rendered.chars().count() > MAX_TEMPLATE_CHARS
        || rendered.chars().any(char::is_control)
    {
        bail!("rendered QiWe text is invalid");
    }
    Ok(rendered)
}

fn policy_capability_grants(policy_config: &Value) -> Result<BTreeSet<String>> {
    let grants = policy_config
        .get("capability_grants")
        .and_then(Value::as_array)
        .context("active Space policy capability_grants must be an array")?;
    grants
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(ToString::to_string)
                .context("Space policy capability grant must be a string")
        })
        .collect()
}

fn bounded_trigger_for_handoff(payload: &Value) -> Result<Value> {
    let trigger = payload
        .get("trigger")
        .and_then(Value::as_object)
        .context("work item trigger is missing")?;
    let kind = trigger
        .get("kind")
        .and_then(Value::as_str)
        .context("work item trigger kind is missing")?;
    match kind {
        "event" => Ok(json!({
            "kind": "event",
            "event_type": trigger.get("event_type").cloned().unwrap_or(Value::Null),
            "provider_event_ref": trigger.get("provider_event_ref").cloned().unwrap_or(Value::Null),
            "subject_user_ids": trigger.get("subject_user_ids").cloned().unwrap_or_else(|| json!([])),
            "occurred_at": trigger.get("occurred_at").cloned().unwrap_or(Value::Null)
        })),
        "schedule" => Ok(json!({
            "kind": "schedule",
            "scheduled_for_utc": trigger.get("scheduled_for_utc").cloned().unwrap_or(Value::Null)
        })),
        _ => bail!("work item trigger kind is not supported"),
    }
}

async fn claim_one(pool: &PgPool, work_item_id: Option<Uuid>) -> Result<Option<ExecutionClaim>> {
    let mut tx = pool
        .begin()
        .await
        .context("begin Space automation execution claim")?;
    let query = eligible_claim_query(true);
    let Some(row) = sqlx::query(&query)
        .bind(work_item_id)
        .fetch_optional(&mut *tx)
        .await
        .context("select claimable Space automation run")?
    else {
        tx.commit()
            .await
            .context("commit empty Space automation claim")?;
        return Ok(None);
    };
    let mut claim = execution_claim_from_row(&row)?;
    let attempt_id = Uuid::new_v4();
    let updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET status = 'processing', claimed_by = $2, locked_at = now(),
            claim_expires_at = now() + make_interval(secs => $3),
            attempts = attempts + 1,
            metadata = metadata || $4,
            updated_at = now()
        WHERE id = $1 AND status = 'queued' AND attempts = 0
        "#,
    )
    .bind(claim.work_item_id)
    .bind(WORKER_ID)
    .bind(CLAIM_TTL_SECONDS as f64)
    .bind(json!({
        "space_automation_execution": {
            "attempt_id": attempt_id,
            "worker_id": WORKER_ID,
            "external_send_outcome": "pending",
            "automatic_retry_allowed": false
        }
    }))
    .execute(&mut *tx)
    .await
    .context("claim Space automation run")?;
    if updated.rows_affected() != 1 {
        bail!("Space automation claim changed concurrently");
    }
    append_event_in_tx(
        &mut tx,
        claim.work_item_id,
        "space_automation_execution_started",
        "Space automation execution attempt started before external access",
        json!({
            "attempt_id": attempt_id,
            "space_id": claim.space_id,
            "automation_definition_id": claim.automation_id,
            "automation_definition_digest": claim.automation_digest,
            "business_definition_id": claim.business_id,
            "business_definition_digest": claim.business_digest,
            "space_policy_version_id": claim.policy_id,
            "space_policy_digest": claim.policy_digest,
            "selected_capability_key": claim.selected_capability_key,
            "target_derived_from_space": true,
            "external_send_executed": false,
            "external_send_outcome": "pending",
            "automatic_retry_allowed": false
        }),
    )
    .await?;
    tx.commit()
        .await
        .context("commit Space automation attempt before external access")?;
    claim.attempt_id = Some(attempt_id);
    Ok(Some(claim))
}

async fn load_eligible_claim(
    pool: &PgPool,
    work_item_id: Option<Uuid>,
) -> Result<Option<ExecutionClaim>> {
    let query = eligible_claim_query(false);
    let row = sqlx::query(&query)
        .bind(work_item_id)
        .fetch_optional(pool)
        .await
        .context("preview claimable Space automation run")?;
    row.as_ref().map(execution_claim_from_row).transpose()
}

fn eligible_claim_query(lock: bool) -> String {
    let lock_clause = if lock {
        "FOR UPDATE OF work_item SKIP LOCKED"
    } else {
        ""
    };
    format!(
        r#"
        SELECT
            work_item.id AS work_item_id,
            work_item.space_id,
            work_item.payload,
            automation.id AS automation_id,
            automation.definition_key AS automation_key,
            automation.version AS automation_version,
            automation.status AS automation_status,
            automation.definition_digest AS automation_digest,
            automation.trigger_kind,
            automation.channel_event_mapping_id,
            mapping.definition_digest AS channel_event_mapping_digest,
            business.id AS business_id,
            business.status AS business_status,
            business.definition_digest AS business_digest,
            business.execution_mode,
            business.definition AS business_definition,
            business.allowed_capabilities AS business_allowed_capabilities,
            business.approval_policy,
            policy.id AS policy_id,
            policy.definition_digest AS policy_digest,
            policy.policy_config,
            selected.capability_key AS selected_capability_key,
            selected.metadata AS selected_capability_metadata,
            conversation.chat_id AS conversation_chat_id
        FROM qintopia_agent_os.work_items work_item
        JOIN qintopia_agent_os.automation_definition_versions automation
          ON automation.id::text = work_item.payload->>'automation_definition_id'
         AND automation.space_id = work_item.space_id
        JOIN qintopia_agent_os.business_definition_versions business
          ON business.id = automation.business_definition_id
         AND business.id::text = work_item.payload->>'business_definition_id'
         AND business.space_id = work_item.space_id
        JOIN qintopia_agent_os.space_policy_versions policy
          ON policy.space_id = work_item.space_id
         AND policy.definition_key = 'default'
         AND policy.status = 'active'
        JOIN qintopia_messages.conversations conversation
          ON conversation.id = work_item.space_id
         AND conversation.platform = 'qiwe'
         AND conversation.chat_type = 'group'
         AND conversation.status = 'active'
        JOIN qintopia_agent_os.capabilities execution_capability
          ON execution_capability.capability_key = '{EXECUTION_CAPABILITY_KEY}'
         AND execution_capability.enabled
        JOIN qintopia_agent_os.capabilities selected
          ON selected.capability_key = CASE business.execution_mode
              WHEN 'deterministic' THEN business.definition->>'capability_key'
              WHEN 'agent_turn' THEN '{AGENT_TURN_CAPABILITY_KEY}'
              ELSE NULL
         END
         AND selected.enabled
         AND selected.provider_agent = 'erhua'
         AND selected.metadata ->> 'space_invocable' = 'true'
         AND selected.metadata ->> 'space_scope_binding' = 'work_item_space_id'
         AND selected.metadata ->> 'invocation_boundary' = '{EXECUTION_CAPABILITY_KEY}'
        LEFT JOIN qintopia_agent_os.channel_event_mapping_versions mapping
          ON mapping.id = automation.channel_event_mapping_id
        WHERE ($1::uuid IS NULL OR work_item.id = $1)
          AND work_item.work_item_type = '{WORK_ITEM_TYPE}'
          AND work_item.capability_key = '{EXECUTION_CAPABILITY_KEY}'
          AND work_item.requester_agent = 'system'
          AND work_item.target_agent = 'erhua'
          AND work_item.status = 'queued'
          AND work_item.available_at <= now()
          AND work_item.attempts = 0
          AND work_item.space_id IS NOT NULL
          AND automation.status IN ('active', 'shadow')
          AND (
              (automation.status = 'active' AND business.status = 'active') OR
              (automation.status = 'shadow' AND business.status IN ('active', 'shadow'))
          )
          AND work_item.payload->>'automation_key' = automation.definition_key
          AND work_item.payload->>'automation_version' = automation.version::text
          AND work_item.payload->>'automation_definition_digest' = automation.definition_digest
          AND work_item.payload->>'business_definition_digest' = business.definition_digest
          AND work_item.payload->>'space_policy_version_id' = policy.id::text
          AND work_item.payload->>'space_policy_digest' = policy.definition_digest
          AND work_item.payload#>>'{{trigger,kind}}' = automation.trigger_kind
          AND (
              (automation.trigger_kind = 'schedule'
               AND automation.channel_event_mapping_id IS NULL
               AND NOT (work_item.payload ? 'channel_event_mapping_id')
               AND NOT (work_item.payload ? 'channel_event_mapping_digest')) OR
              (automation.trigger_kind = 'event'
               AND automation.channel_event_mapping_id IS NOT NULL
               AND mapping.id IS NOT NULL
               AND (
                   (automation.status = 'active' AND mapping.status = 'active') OR
                   (automation.status = 'shadow' AND mapping.status IN ('active', 'shadow'))
               )
               AND work_item.source_refs->>'mapping_version_id' = mapping.id::text
               AND work_item.payload->>'channel_event_mapping_id' = mapping.id::text
               AND work_item.payload->>'channel_event_mapping_digest' = mapping.definition_digest)
          )
          AND 'system' = ANY(execution_capability.allowed_callers)
          AND '{WORK_ITEM_TYPE}' = ANY(execution_capability.allowed_work_item_types)
          AND 'system' = ANY(selected.allowed_callers)
          AND (
              (business.execution_mode = 'deterministic'
               AND selected.metadata ? '{recipe_metadata_key}'
               AND '{WORK_ITEM_TYPE}' = ANY(selected.allowed_work_item_types)) OR
              (business.execution_mode = 'agent_turn'
               AND selected.capability_key = '{AGENT_TURN_CAPABILITY_KEY}'
               AND '{AGENT_TURN_WORK_ITEM_TYPE}' = ANY(selected.allowed_work_item_types))
          )
          AND selected.capability_key = ANY(business.allowed_capabilities)
          AND COALESCE(policy.policy_config->'capability_grants', '[]'::jsonb)
              ? selected.capability_key
          AND NOT EXISTS (
              SELECT 1 FROM qintopia_agent_os.work_item_events prior
              WHERE prior.work_item_id = work_item.id
                AND prior.event_type IN (
                    'space_automation_execution_started',
                    'space_automation_execution_completed',
                    'space_automation_execution_failed',
                    'space_automation_execution_ambiguous'
                )
          )
        ORDER BY work_item.priority DESC, work_item.available_at, work_item.created_at
        LIMIT 1
        {lock_clause}
        "#,
        recipe_metadata_key = crate::space_capability_recipe::RECIPE_METADATA_KEY,
    )
}

fn execution_claim_from_row(row: &sqlx::postgres::PgRow) -> Result<ExecutionClaim> {
    Ok(ExecutionClaim {
        work_item_id: row.try_get("work_item_id")?,
        space_id: row.try_get("space_id")?,
        payload: row.try_get("payload")?,
        automation_id: row.try_get("automation_id")?,
        automation_key: row.try_get("automation_key")?,
        automation_version: row.try_get("automation_version")?,
        automation_status: row.try_get("automation_status")?,
        automation_digest: row.try_get("automation_digest")?,
        trigger_kind: row.try_get("trigger_kind")?,
        channel_event_mapping_id: row.try_get("channel_event_mapping_id")?,
        channel_event_mapping_digest: row.try_get("channel_event_mapping_digest")?,
        business_id: row.try_get("business_id")?,
        business_status: row.try_get("business_status")?,
        business_digest: row.try_get("business_digest")?,
        execution_mode: row.try_get("execution_mode")?,
        business_definition: row.try_get("business_definition")?,
        business_allowed_capabilities: row.try_get("business_allowed_capabilities")?,
        approval_policy: row.try_get("approval_policy")?,
        policy_id: row.try_get("policy_id")?,
        policy_digest: row.try_get("policy_digest")?,
        policy_config: row.try_get("policy_config")?,
        selected_capability_key: row.try_get("selected_capability_key")?,
        selected_capability_metadata: row.try_get("selected_capability_metadata")?,
        conversation_chat_id: row.try_get("conversation_chat_id")?,
        attempt_id: None,
    })
}

async fn load_enabled_capability_intersection(
    tx: &mut Transaction<'_, Postgres>,
    business_capabilities: &[String],
    policy_grants: &BTreeSet<String>,
) -> Result<Vec<String>> {
    let candidates = business_capabilities
        .iter()
        .filter(|key| policy_grants.contains(*key))
        .cloned()
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT capability_key
        FROM qintopia_agent_os.capabilities
        WHERE enabled
          AND capability_key = ANY($1)
          AND capability_key <> 'erhua.space_agent_turn'
          AND provider_agent = 'erhua'
          AND 'erhua' = ANY(allowed_callers)
          AND 'space_agent_turn' = ANY(allowed_work_item_types)
          AND metadata ->> 'space_invocable' = 'true'
          AND metadata ->> 'space_scope_binding' = 'work_item_space_id'
          AND metadata ->> 'invocation_boundary' = 'erhua.space_agent_turn'
          AND metadata ->> 'runner_access' = 'bounded_catalog_v1'
        ORDER BY capability_key
        "#,
    )
    .bind(&candidates)
    .fetch_all(&mut **tx)
    .await
    .context("load enabled Space agent-turn capability ceiling")
}

async fn authorize_live_claim_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    claim: &ExecutionClaim,
    gate: LiveAuthorizationGate,
) -> Result<Value> {
    let row = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items AS work_item
        SET metadata = work_item.metadata || $12,
            updated_at = now()
        FROM qintopia_agent_os.automation_definition_versions automation,
             qintopia_agent_os.business_definition_versions business,
             qintopia_agent_os.space_policy_versions policy,
             qintopia_messages.conversations conversation,
             qintopia_agent_os.capabilities execution_capability,
             qintopia_agent_os.capabilities selected
        WHERE work_item.id = $1
          AND work_item.work_item_type = 'space_automation_run'
          AND work_item.status = 'processing'
          AND work_item.claimed_by = $2
          AND work_item.metadata#>>'{space_automation_execution,attempt_id}' = $3
          AND work_item.claim_expires_at > now()
          AND work_item.space_id = $4
          AND automation.id = $5
          AND automation.space_id = work_item.space_id
          AND automation.status = 'active'
          AND automation.definition_digest = $6
          AND business.id = $7
          AND business.id = automation.business_definition_id
          AND business.space_id = work_item.space_id
          AND business.status = 'active'
          AND business.definition_digest = $8
          AND business.execution_mode = $11
          AND policy.id = $9
          AND policy.space_id = work_item.space_id
          AND policy.definition_key = 'default'
          AND policy.status = 'active'
          AND policy.definition_digest = $10
          AND selected.capability_key = $13
          AND selected.enabled
          AND selected.provider_agent = 'erhua'
          AND selected.metadata ->> 'space_invocable' = 'true'
          AND selected.metadata ->> 'space_scope_binding' = 'work_item_space_id'
          AND selected.metadata ->> 'invocation_boundary' = 'erhua.execute_space_business'
          AND 'system' = ANY(selected.allowed_callers)
          AND (
              (business.execution_mode = 'deterministic'
               AND 'space_automation_run' = ANY(selected.allowed_work_item_types)
               AND selected.metadata ->> 'space_execution_recipe' = $16)
              OR
              (business.execution_mode = 'agent_turn'
               AND 'space_agent_turn' = ANY(selected.allowed_work_item_types))
          )
          AND selected.capability_key = ANY(business.allowed_capabilities)
          AND COALESCE(policy.policy_config->'capability_grants', '[]'::jsonb)
              ? selected.capability_key
          AND execution_capability.capability_key = 'erhua.execute_space_business'
          AND execution_capability.enabled
          AND 'system' = ANY(execution_capability.allowed_callers)
          AND 'space_automation_run' = ANY(execution_capability.allowed_work_item_types)
          AND conversation.id = work_item.space_id
          AND conversation.platform = 'qiwe'
          AND conversation.chat_type = 'group'
          AND conversation.status = 'active'
          AND conversation.chat_id = $14
          AND (
              (automation.trigger_kind = 'schedule'
               AND automation.channel_event_mapping_id IS NULL)
              OR
              (automation.trigger_kind = 'event'
               AND automation.channel_event_mapping_id = $15
               AND EXISTS (
                   SELECT 1
                   FROM qintopia_agent_os.channel_event_mapping_versions mapping
                   WHERE mapping.id = automation.channel_event_mapping_id
                     AND mapping.status = 'active'
               ))
          )
        RETURNING policy.policy_config
        "#,
    )
    .bind(claim.work_item_id)
    .bind(WORKER_ID)
    .bind(required_attempt_id(claim)?.to_string())
    .bind(claim.space_id)
    .bind(claim.automation_id)
    .bind(&claim.automation_digest)
    .bind(claim.business_id)
    .bind(&claim.business_digest)
    .bind(claim.policy_id)
    .bind(&claim.policy_digest)
    .bind(&claim.execution_mode)
    .bind(json!({
        "space_automation_execution": {
            "attempt_id": required_attempt_id(claim)?,
            "worker_id": WORKER_ID,
            "authorization_state": gate.authorization_state(),
            "external_send_outcome": gate.external_send_outcome(),
            "automatic_retry_allowed": false
        }
    }))
    .bind(&claim.selected_capability_key)
    .bind(&claim.conversation_chat_id)
    .bind(claim.channel_event_mapping_id)
    .bind(
        crate::space_capability_recipe::from_capability_metadata(
            &claim.selected_capability_metadata,
        )
        .map(|recipe| recipe.key())
        .unwrap_or(""),
    )
    .fetch_optional(&mut **tx)
    .await
    .context("atomically revalidate Space automation execution authorization")?
    .context("Space automation execution authorization is no longer active")?;
    row.try_get("policy_config")
        .context("read live Space policy from execution gate")
}

#[cfg(any(
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter",
    all(test, feature = "postgres-integration-tests")
))]
async fn commit_qiwe_roster_access_gate(pool: &PgPool, claim: &ExecutionClaim) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("begin Space automation roster-access gate transaction")?;
    authorize_live_claim_in_tx(&mut tx, claim, LiveAuthorizationGate::QiweRosterAccess).await?;
    append_event_in_tx(
        &mut tx,
        claim.work_item_id,
        "space_automation_roster_access_authorized",
        "Space automation authorization revalidated before QiWe room-detail access",
        json!({
            "attempt_id": required_attempt_id(claim)?,
            "automation_definition_id": claim.automation_id,
            "business_definition_id": claim.business_id,
            "space_policy_version_id": claim.policy_id,
            "selected_capability_key": claim.selected_capability_key,
            "target_derived_from_space": true,
            "external_send_executed": false,
            "external_send_outcome": "not_sent",
            "automatic_retry_allowed": false
        }),
    )
    .await?;
    tx.commit()
        .await
        .context("commit Space automation roster-access gate")
}

#[cfg(any(
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter",
    all(test, feature = "postgres-integration-tests")
))]
async fn commit_qiwe_send_gate(pool: &PgPool, claim: &ExecutionClaim) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("begin Space automation send gate transaction")?;
    authorize_live_claim_in_tx(&mut tx, claim, LiveAuthorizationGate::QiweSend).await?;
    append_event_in_tx(
        &mut tx,
        claim.work_item_id,
        "space_automation_send_committed",
        "Space automation authorization revalidated immediately before send",
        json!({
            "attempt_id": required_attempt_id(claim)?,
            "automation_definition_id": claim.automation_id,
            "business_definition_id": claim.business_id,
            "space_policy_version_id": claim.policy_id,
            "selected_capability_key": claim.selected_capability_key,
            "target_derived_from_space": true,
            "external_send_executed": null,
            "external_send_outcome": "sending",
            "automatic_retry_allowed": false
        }),
    )
    .await?;
    tx.commit()
        .await
        .context("commit Space automation send gate")
}

#[cfg(all(test, feature = "postgres-integration-tests"))]
pub(crate) async fn assert_capability_and_policy_revocation_gates_for_integration_test(
    pool: &PgPool,
    work_item_id: Uuid,
) -> Result<()> {
    let registration = sqlx::query(
        "SELECT provider_agent, metadata FROM qintopia_agent_os.capabilities WHERE capability_key = $1",
    )
    .bind(QIWE_TEXT_TEMPLATE_CAPABILITY_KEY)
    .fetch_one(pool)
    .await
    .context("load integration-test selected capability registration")?;
    let original_provider_agent: String = registration.try_get("provider_agent")?;
    let original_metadata: Value = registration.try_get("metadata")?;

    sqlx::query(
        "UPDATE qintopia_agent_os.capabilities SET provider_agent = 'revoked-test-provider', updated_at = now() WHERE capability_key = $1",
    )
    .bind(QIWE_TEXT_TEMPLATE_CAPABILITY_KEY)
    .execute(pool)
    .await
    .context("revoke integration-test selected capability provider after enqueue")?;
    let claim_with_revoked_provider = claim_one(pool, Some(work_item_id)).await?;
    sqlx::query(
        "UPDATE qintopia_agent_os.capabilities SET provider_agent = $2, updated_at = now() WHERE capability_key = $1",
    )
    .bind(QIWE_TEXT_TEMPLATE_CAPABILITY_KEY)
    .bind(&original_provider_agent)
    .execute(pool)
    .await
    .context("restore integration-test selected capability provider")?;
    if claim_with_revoked_provider.is_some() {
        bail!(
            "Space automation claim accepted a selected capability with revoked provider ownership"
        );
    }

    let claim = claim_one(pool, Some(work_item_id))
        .await?
        .context("integration-test Space automation was not claimable")?;

    let mut live_lease_tx = pool
        .begin()
        .await
        .context("begin integration-test live-lease authorization")?;
    authorize_live_claim_in_tx(&mut live_lease_tx, &claim, LiveAuthorizationGate::QiweSend)
        .await
        .context("unexpired Space automation claim must pass final authorization")?;
    live_lease_tx
        .rollback()
        .await
        .context("rollback integration-test live-lease authorization")?;

    let expired = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET claim_expires_at = now() - interval '1 second', updated_at = now()
        WHERE id = $1 AND status = 'processing' AND claimed_by = $2
          AND metadata#>>'{space_automation_execution,attempt_id}' = $3
        "#,
    )
    .bind(claim.work_item_id)
    .bind(WORKER_ID)
    .bind(required_attempt_id(&claim)?.to_string())
    .execute(pool)
    .await
    .context("expire integration-test Space automation claim")?;
    if expired.rows_affected() != 1 {
        bail!("integration-test Space automation claim was not expired");
    }

    let mut expired_lease_tx = pool
        .begin()
        .await
        .context("begin integration-test expired-lease authorization")?;
    let expired_lease_accepted = authorize_live_claim_in_tx(
        &mut expired_lease_tx,
        &claim,
        LiveAuthorizationGate::QiweSend,
    )
    .await
    .is_ok();
    expired_lease_tx
        .rollback()
        .await
        .context("rollback integration-test expired-lease authorization")?;
    if expired_lease_accepted {
        bail!("Space automation final authorization accepted an expired claim");
    }

    let restored = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET claim_expires_at = now() + interval '1 hour', updated_at = now()
        WHERE id = $1 AND status = 'processing' AND claimed_by = $2
          AND metadata#>>'{space_automation_execution,attempt_id}' = $3
        "#,
    )
    .bind(claim.work_item_id)
    .bind(WORKER_ID)
    .bind(required_attempt_id(&claim)?.to_string())
    .execute(pool)
    .await
    .context("restore integration-test Space automation claim lease")?;
    if restored.rows_affected() != 1 {
        bail!("integration-test Space automation claim lease was not restored");
    }

    sqlx::query(
        "UPDATE qintopia_agent_os.capabilities SET metadata = metadata - 'invocation_boundary', updated_at = now() WHERE capability_key = $1",
    )
    .bind(QIWE_TEXT_TEMPLATE_CAPABILITY_KEY)
    .execute(pool)
    .await
    .context("revoke integration-test selected capability invocation boundary after claim")?;
    let roster_gate_accepted = commit_qiwe_roster_access_gate(pool, &claim).await.is_ok();
    let send_gate_accepted = commit_qiwe_send_gate(pool, &claim).await.is_ok();
    sqlx::query(
        "UPDATE qintopia_agent_os.capabilities SET metadata = $2, updated_at = now() WHERE capability_key = $1",
    )
    .bind(QIWE_TEXT_TEMPLATE_CAPABILITY_KEY)
    .bind(&original_metadata)
    .execute(pool)
    .await
    .context("restore integration-test selected capability invocation boundary")?;
    if roster_gate_accepted {
        bail!("Space automation roster-access gate accepted a revoked capability boundary");
    }
    if send_gate_accepted {
        bail!("Space automation send gate accepted a revoked capability boundary");
    }

    let updated = sqlx::query(
        "UPDATE qintopia_agent_os.space_policy_versions SET status = 'paused', updated_at = now() WHERE id = $1 AND status = 'active'",
    )
    .bind(claim.policy_id)
    .execute(pool)
    .await
    .context("revoke integration-test Space policy after claim")?;
    if updated.rows_affected() != 1 {
        bail!("integration-test Space policy was not revoked");
    }
    if commit_qiwe_send_gate(pool, &claim).await.is_ok() {
        bail!("Space automation send gate accepted a revoked policy");
    }
    Ok(())
}

async fn terminal_success(
    pool: &PgPool,
    claim: &ExecutionClaim,
    action_status: &str,
    extra: Option<Value>,
    current_subject_count: usize,
    external_send_executed: bool,
) -> Result<()> {
    let mut data = json!({
        "action_status": action_status,
        "selected_capability_key": claim.selected_capability_key,
        "automation_definition_id": claim.automation_id,
        "business_definition_id": claim.business_id,
        "space_policy_version_id": claim.policy_id,
        "current_subject_count": current_subject_count,
        "target_derived_from_space": true,
        "external_send_executed": external_send_executed,
        "external_send_outcome": if external_send_executed { "sent" } else { "not_sent" },
        "automatic_retry_allowed": false
    });
    if let (Some(target), Some(Value::Object(extra))) = (data.as_object_mut(), extra) {
        target.extend(extra);
    }
    let mut tx = pool
        .begin()
        .await
        .context("begin Space automation success transaction")?;
    complete_claim_in_tx(&mut tx, claim, "space_automation_execution_completed", data).await?;
    tx.commit().await.context("commit Space automation success")
}

async fn complete_claim_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    claim: &ExecutionClaim,
    event_type: &str,
    data: Value,
) -> Result<()> {
    let updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET status = 'completed', claimed_by = NULL, locked_at = NULL,
            claim_expires_at = NULL, last_error = NULL,
            metadata = metadata || $4, updated_at = now()
        WHERE id = $1 AND status = 'processing' AND claimed_by = $2
          AND metadata#>>'{space_automation_execution,attempt_id}' = $3
        "#,
    )
    .bind(claim.work_item_id)
    .bind(WORKER_ID)
    .bind(required_attempt_id(claim)?.to_string())
    .bind(json!({
        "space_automation_execution": {
            "attempt_id": required_attempt_id(claim)?,
            "worker_id": WORKER_ID,
            "external_send_outcome": data.get("external_send_outcome").cloned().unwrap_or(Value::Null),
            "automatic_retry_allowed": false,
            "completed": true
        }
    }))
    .execute(&mut **tx)
    .await
    .context("complete Space automation work item")?;
    if updated.rows_affected() != 1 {
        bail!("Space automation success lost its exact claim");
    }
    append_event_in_tx(
        tx,
        claim.work_item_id,
        event_type,
        "Space automation execution completed",
        data,
    )
    .await
}

async fn terminal_failure(
    pool: &PgPool,
    claim: &ExecutionClaim,
    failure_code: &str,
    external_send_executed: bool,
    detail: Option<String>,
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("begin Space automation failure transaction")?;
    let data = json!({
        "failure_code": failure_code,
        "failure_detail": detail.map(|value| trim_error(&value)),
        "selected_capability_key": claim.selected_capability_key,
        "target_derived_from_space": true,
        "external_send_executed": external_send_executed,
        "external_send_outcome": "not_sent",
        "automatic_retry_allowed": false
    });
    fail_claim_in_tx(&mut tx, claim, failure_code, data).await?;
    tx.commit().await.context("commit Space automation failure")
}

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
async fn terminal_ambiguous(
    pool: &PgPool,
    claim: &ExecutionClaim,
    failure_code: &str,
    response_summary: Option<Value>,
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("begin Space automation ambiguous transaction")?;
    let updated = update_failed_claim(
        &mut tx,
        claim,
        failure_code,
        json!({
            "space_automation_execution": {
                "attempt_id": required_attempt_id(claim)?,
                "worker_id": WORKER_ID,
                "external_send_outcome": "unknown",
                "automatic_retry_allowed": false,
                "completed": true
            }
        }),
    )
    .await?;
    if updated != 1 {
        bail!("ambiguous Space automation outcome lost its exact claim");
    }
    append_event_in_tx(
        &mut tx,
        claim.work_item_id,
        "space_automation_execution_ambiguous",
        "Space automation external outcome is ambiguous",
        json!({
            "failure_code": failure_code,
            "selected_capability_key": claim.selected_capability_key,
            "qiwe_response": response_summary,
            "target_derived_from_space": true,
            "external_send_executed": Value::Null,
            "external_send_outcome": "unknown",
            "automatic_retry_allowed": false
        }),
    )
    .await?;
    tx.commit()
        .await
        .context("commit ambiguous Space automation outcome")
}

async fn fail_claim_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    claim: &ExecutionClaim,
    failure_code: &str,
    data: Value,
) -> Result<()> {
    let updated = update_failed_claim(
        tx,
        claim,
        failure_code,
        json!({
            "space_automation_execution": {
                "attempt_id": required_attempt_id(claim)?,
                "worker_id": WORKER_ID,
                "external_send_outcome": "not_sent",
                "automatic_retry_allowed": false,
                "completed": true
            }
        }),
    )
    .await?;
    if updated != 1 {
        bail!("failed Space automation outcome lost its exact claim");
    }
    append_event_in_tx(
        tx,
        claim.work_item_id,
        "space_automation_execution_failed",
        "Space automation execution failed closed",
        data,
    )
    .await
}

async fn update_failed_claim(
    tx: &mut Transaction<'_, Postgres>,
    claim: &ExecutionClaim,
    failure_code: &str,
    metadata: Value,
) -> Result<u64> {
    Ok(sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET status = 'failed', claimed_by = NULL, locked_at = NULL,
            claim_expires_at = NULL, last_error = $4,
            metadata = metadata || $5, updated_at = now()
        WHERE id = $1 AND status = 'processing' AND claimed_by = $2
          AND metadata#>>'{space_automation_execution,attempt_id}' = $3
        "#,
    )
    .bind(claim.work_item_id)
    .bind(WORKER_ID)
    .bind(required_attempt_id(claim)?.to_string())
    .bind(trim_error(failure_code))
    .bind(metadata)
    .execute(&mut **tx)
    .await
    .context("fail Space automation work item")?
    .rows_affected())
}

async fn reconcile_stale_attempts(pool: &PgPool) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("begin stale Space automation reconciliation")?;
    let rows = sqlx::query(
        r#"
        SELECT work_item.id,
               work_item.metadata#>>'{space_automation_execution,attempt_id}' AS attempt_id,
               started.data->>'selected_capability_key' AS selected_capability_key
        FROM qintopia_agent_os.work_items work_item
        JOIN LATERAL (
            SELECT data FROM qintopia_agent_os.work_item_events
            WHERE work_item_id = work_item.id
              AND event_type = 'space_automation_execution_started'
            ORDER BY created_at DESC LIMIT 1
        ) started ON true
        WHERE work_item.work_item_type = 'space_automation_run'
          AND work_item.status = 'processing'
          AND work_item.claimed_by = $1
          AND work_item.claim_expires_at <= now()
          AND NOT EXISTS (
              SELECT 1 FROM qintopia_agent_os.work_item_events terminal
              WHERE terminal.work_item_id = work_item.id
                AND terminal.event_type IN (
                    'space_automation_execution_completed',
                    'space_automation_execution_failed',
                    'space_automation_execution_ambiguous'
                )
          )
        FOR UPDATE OF work_item SKIP LOCKED
        "#,
    )
    .bind(WORKER_ID)
    .fetch_all(&mut *tx)
    .await
    .context("load stale Space automation attempts")?;

    for row in rows {
        let work_item_id: Uuid = row.try_get("id")?;
        let attempt_id: String = row.try_get("attempt_id")?;
        let selected_capability_key: String = row.try_get("selected_capability_key")?;
        let updated = sqlx::query(
            r#"
            UPDATE qintopia_agent_os.work_items
            SET status = 'failed', claimed_by = NULL, locked_at = NULL,
                claim_expires_at = NULL,
                last_error = 'space_automation_attempt_expired_outcome_ambiguous',
                metadata = metadata || $3, updated_at = now()
            WHERE id = $1 AND status = 'processing' AND claimed_by = $2
            "#,
        )
        .bind(work_item_id)
        .bind(WORKER_ID)
        .bind(json!({
            "space_automation_execution": {
                "attempt_id": attempt_id,
                "worker_id": WORKER_ID,
                "external_send_outcome": "unknown",
                "automatic_retry_allowed": false,
                "completed": true
            }
        }))
        .execute(&mut *tx)
        .await
        .context("terminalize stale Space automation attempt")?;
        if updated.rows_affected() != 1 {
            bail!("stale Space automation attempt changed concurrently");
        }
        append_event_in_tx(
            &mut tx,
            work_item_id,
            "space_automation_execution_ambiguous",
            "Expired Space automation attempt requires manual reconciliation",
            json!({
                "failure_code": "space_automation_attempt_expired_outcome_ambiguous",
                "selected_capability_key": selected_capability_key,
                "external_send_executed": Value::Null,
                "external_send_outcome": "unknown",
                "automatic_retry_allowed": false,
                "reconciled_after_claim_expiry": true
            }),
        )
        .await?;
    }
    tx.commit()
        .await
        .context("commit stale Space automation reconciliation")?;
    Ok(())
}

async fn append_event_in_tx(
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
    .context("append Space automation execution event")?;
    Ok(())
}

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
impl QiweAdapterConfig {
    fn from_cli(cli: &Cli) -> Result<Self> {
        let api_url = strict_qiwe_api_url(&required_env("QIWE_API_URL")?)?;
        let allowed_hosts = parse_csv_set(&required_env(QIWE_ALLOWED_HOSTS_ENV)?);
        let host = api_url.host_str().context("QiWe API URL has no host")?;
        if !allowed_hosts.contains(host) {
            bail!("QiWe API host is outside the Space automation allowlist");
        }
        let token = required_env("QIWE_TOKEN")?;
        let guid = required_env("QIWE_GUID")?;
        crate::bounded_http::validate_http_header("x-qiwei-token", &token)?;
        crate::bounded_http::validate_http_header("x-qiwei-guid", &guid)?;
        let _ = cli;
        Ok(Self {
            api_url,
            token,
            guid,
        })
    }
}

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
fn load_exact_room_roster(
    config: &QiweAdapterConfig,
    chat_id: &str,
    client: &HttpClient,
) -> Result<RoomRoster> {
    let response = qiwe_request(
        config,
        &QiweApiRequest {
            method: QIWE_ROOM_DETAIL_METHOD,
            params: RoomDetailParams {
                guid: &config.guid,
                room_id_list: [chat_id],
            },
        },
        client,
    )?;
    let envelope = parse_success_envelope(&response)?;
    parse_exact_room_roster(envelope.data.as_ref(), chat_id)
}

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
fn send_qiwe_text(
    config: &QiweAdapterConfig,
    chat_id: &str,
    message_text: &str,
    client: &HttpClient,
) -> SendOutcome {
    let request = QiweApiRequest {
        method: QIWE_TEXT_SEND_METHOD,
        params: SendTextParams {
            guid: &config.guid,
            to_id: chat_id,
            is_no_need_read: false,
            content: [HyperTextSegment {
                segment_type: "text",
                text: message_text,
            }],
        },
    };
    let body = match serde_json::to_vec(&request) {
        Ok(body) => Zeroizing::new(body),
        Err(_) => {
            return SendOutcome::FailedBeforeSend {
                failure_code: "qiwe_text_request_build_failed",
            };
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
            return if error.request_may_have_been_sent() {
                SendOutcome::Ambiguous {
                    failure_code: "qiwe_text_transport_ambiguous",
                    response_summary: None,
                }
            } else {
                SendOutcome::FailedBeforeSend {
                    failure_code: "qiwe_text_transport_not_sent",
                }
            };
        }
    };
    classify_send_response(&response)
}

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
fn qiwe_request<T: Serialize>(
    config: &QiweAdapterConfig,
    request: &QiweApiRequest<T>,
    client: &HttpClient,
) -> Result<HttpResponse> {
    let body = Zeroizing::new(serde_json::to_vec(request).context("encode QiWe request")?);
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
        .map_err(|_| anyhow!("QiWe room-detail request failed"))?;
    if !(200..300).contains(&response.status) {
        bail!("QiWe room-detail HTTP response was not successful");
    }
    Ok(response)
}

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
fn parse_success_envelope(response: &HttpResponse) -> Result<QiweApiResponse> {
    let envelope: QiweApiResponse =
        serde_json::from_slice(&response.body).context("parse QiWe API response")?;
    if !matches!(envelope.code, Some(0 | 200)) {
        bail!("QiWe API returned a non-success business code");
    }
    Ok(envelope)
}

#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
fn classify_send_response(response: &HttpResponse) -> SendOutcome {
    if !(200..300).contains(&response.status) {
        return SendOutcome::Ambiguous {
            failure_code: "qiwe_text_http_status_ambiguous",
            response_summary: Some(json!({"status": response.status})),
        };
    }
    let Ok(envelope) = serde_json::from_slice::<QiweApiResponse>(&response.body) else {
        return SendOutcome::Ambiguous {
            failure_code: "qiwe_text_response_parse_ambiguous",
            response_summary: None,
        };
    };
    let code = envelope.code.unwrap_or(-1);
    let summary = safe_qiwe_response_summary(code, envelope.msg.as_deref(), envelope.data.as_ref());
    let result = single_qiwe_text_send_result(envelope.data.as_ref());
    if code == 0 && result.is_some_and(|result| result.is_send_success == 1) {
        SendOutcome::Sent {
            response_summary: summary,
        }
    } else {
        SendOutcome::Ambiguous {
            failure_code: "qiwe_text_business_response_ambiguous",
            response_summary: Some(summary),
        }
    }
}

#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
fn single_qiwe_text_send_result(data: Option<&Value>) -> Option<QiweTextSendResult> {
    let value = match data? {
        Value::Object(_) => data?,
        Value::Array(results) if results.len() == 1 => &results[0],
        _ => return None,
    };
    serde_json::from_value(value.clone()).ok()
}

#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
fn parse_exact_room_roster(data: Option<&Value>, expected_chat_id: &str) -> Result<RoomRoster> {
    let data = data.context("QiWe room-detail response is missing data")?;
    let data = first_object(data).context("QiWe room-detail data has an unsupported shape")?;
    let room_list = data
        .get("roomList")
        .and_then(Value::as_array)
        .context("QiWe room-detail response is missing roomList")?;
    let matching_rooms = room_list
        .iter()
        .filter(|room| room_identifier(room).as_deref() == Some(expected_chat_id))
        .collect::<Vec<_>>();
    if matching_rooms.len() != 1 {
        bail!("QiWe room-detail response did not contain exactly the requested room");
    }
    let room = matching_rooms[0];
    let display_name = value_text(room.get("roomName"))
        .or_else(|| value_text(room.get("name")))
        .context("QiWe room-detail response is missing roomName")?;
    validate_display_name(&display_name)?;
    let members = room
        .get("memberList")
        .and_then(Value::as_array)
        .context("QiWe room-detail response is missing memberList")?;
    let mut roster = RoomRoster {
        display_name,
        ..RoomRoster::default()
    };
    for member in members {
        let Some(user_id) = value_text(member.get("userId")) else {
            continue;
        };
        validate_opaque_user_id(&user_id)?;
        let display_name =
            value_text(member.get("name")).or_else(|| value_text(member.get("roomRemarkName")));
        if let Some(name) = display_name.as_deref() {
            validate_display_name(name)?;
        }
        match roster.members.get(&user_id) {
            Some(existing) if existing.display_name != display_name => {
                bail!("QiWe room-detail response contains a conflicting member id");
            }
            Some(_) => {}
            None => {
                roster.members.insert(user_id, RoomMember { display_name });
            }
        }
    }
    Ok(roster)
}

#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
fn first_object(value: &Value) -> Option<&Map<String, Value>> {
    value
        .as_object()
        .or_else(|| value.as_array()?.first()?.as_object())
}

#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
fn room_identifier(room: &Value) -> Option<String> {
    value_text(room.get("roomId"))
        .or_else(|| value_text(room.get("room_id")))
        .or_else(|| value_text(room.get("chatId")))
        .or_else(|| value_text(room.get("id")))
}

#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
fn value_text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

#[cfg(any(
    test,
    feature = "qiwe-staging-adapter",
    feature = "qiwe-production-adapter"
))]
fn safe_qiwe_response_summary(code: i64, message: Option<&str>, data: Option<&Value>) -> Value {
    json!({
        "code": code,
        "message_present": message.is_some_and(|value| !value.trim().is_empty()),
        "data_present": data.is_some_and(|value| !value.is_null())
    })
}

fn validate_execution_boundary(database_url: &str) -> Result<()> {
    match std::env::var(EXECUTION_ENABLE_ENV).as_deref() {
        Ok("1") => {}
        Ok("0") | Err(_) => bail!("Space automation execution is disabled"),
        _ => bail!("Space automation execution enable flag must be 0 or 1"),
    }
    if std::env::var(EXECUTION_APPROVAL_ENV).as_deref() != Ok(EXECUTION_APPROVAL_PHRASE) {
        bail!("Space automation execution owner approval is required");
    }
    let expected = required_env(DATABASE_URL_SHA256_ENV)?;
    if expected.len() != 64
        || !expected.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !sha256_hex(database_url.as_bytes()).eq_ignore_ascii_case(&expected)
    {
        bail!("Space automation execution database URL hash is not owner-approved");
    }
    Ok(())
}

fn validate_execution_compile_boundary() -> Result<()> {
    #[cfg(not(all(
        feature = "qiwe-production-adapter",
        not(feature = "qiwe-staging-adapter")
    )))]
    {
        bail!("Space automation apply requires the production-only QiWe adapter");
    }

    #[cfg(all(
        feature = "qiwe-production-adapter",
        not(feature = "qiwe-staging-adapter")
    ))]
    {
        Ok(())
    }
}

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
fn strict_qiwe_api_url(value: &str) -> Result<Url> {
    let url = Url::parse(value).context("parse QiWe API URL")?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
        || url.fragment().is_some()
    {
        bail!("QiWe API URL must be a credential-free HTTPS URL");
    }
    Ok(url)
}

#[cfg(any(feature = "qiwe-staging-adapter", feature = "qiwe-production-adapter"))]
fn parse_csv_set(value: &str) -> BTreeSet<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| entry.to_ascii_lowercase())
        .collect()
}

fn required_env(name: &str) -> Result<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && !value.starts_with('<'))
        .ok_or_else(|| anyhow!("{name} is required"))
}

fn required_attempt_id(claim: &ExecutionClaim) -> Result<Uuid> {
    claim
        .attempt_id
        .context("Space automation execution claim has no attempt id")
}

fn required_text(value: &Value, key: &str) -> Result<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
        .with_context(|| format!("{key} is required"))
}

fn required_object_text(object: &Map<String, Value>, key: &str) -> Result<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
        .with_context(|| format!("{key} is required"))
}

fn sha256_hex(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn trim_error(value: &str) -> String {
    value.trim().chars().take(300).collect()
}

fn empty_report(apply_requested: bool, action_status: &str) -> ExecutionWorkerReport {
    ExecutionWorkerReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        worker: WORKER_ID,
        action_status: action_status.to_string(),
        work_item_id: None,
        space_id: None,
        automation_definition_id: None,
        business_definition_id: None,
        execution_mode: None,
        selected_capability_key: None,
        child_work_item_id: None,
        subject_count: 0,
        current_subject_count: 0,
        target_derived_from_space: true,
        external_send_executed: Some(false),
        automatic_retry_allowed: false,
        limitations: report_limitations(),
        guardrails: report_guardrails(),
    }
}

fn report_for_plan(
    claim: &ExecutionClaim,
    plan: &ExecutionPlan,
    apply_requested: bool,
    action_status: &str,
    child_work_item_id: Option<Uuid>,
    current_subject_count: usize,
    external_send_executed: Option<bool>,
) -> ExecutionWorkerReport {
    let subject_count = match plan {
        ExecutionPlan::QiweTextTemplate(plan) => plan.subject_user_ids.len(),
        ExecutionPlan::Shadow | ExecutionPlan::AgentTurn(_) => 0,
    };
    report_for_claim_with_subject_count(
        claim,
        apply_requested,
        action_status,
        child_work_item_id,
        subject_count,
        current_subject_count,
        external_send_executed,
    )
}

fn report_for_claim(
    claim: &ExecutionClaim,
    apply_requested: bool,
    action_status: &str,
    child_work_item_id: Option<Uuid>,
    current_subject_count: usize,
    external_send_executed: Option<bool>,
) -> ExecutionWorkerReport {
    let subject_count = canonical_subject_user_ids(&claim.payload, &claim.trigger_kind)
        .map(|subjects| subjects.len())
        .unwrap_or(0);
    report_for_claim_with_subject_count(
        claim,
        apply_requested,
        action_status,
        child_work_item_id,
        subject_count,
        current_subject_count,
        external_send_executed,
    )
}

fn report_for_claim_with_subject_count(
    claim: &ExecutionClaim,
    apply_requested: bool,
    action_status: &str,
    child_work_item_id: Option<Uuid>,
    subject_count: usize,
    current_subject_count: usize,
    external_send_executed: Option<bool>,
) -> ExecutionWorkerReport {
    ExecutionWorkerReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        worker: WORKER_ID,
        action_status: action_status.to_string(),
        work_item_id: Some(claim.work_item_id),
        space_id: Some(claim.space_id),
        automation_definition_id: Some(claim.automation_id),
        business_definition_id: Some(claim.business_id),
        execution_mode: Some(claim.execution_mode.clone()),
        selected_capability_key: Some(claim.selected_capability_key.clone()),
        child_work_item_id,
        subject_count,
        current_subject_count,
        target_derived_from_space: true,
        external_send_executed,
        automatic_retry_allowed: false,
        limitations: report_limitations(),
        guardrails: report_guardrails(),
    }
}

fn report_limitations() -> Vec<String> {
    vec![
        "the worker consumes only space_automation_run work items".to_string(),
        "agent_turn execution is delegated only to the separately authenticated bounded runner broker"
            .to_string(),
        "ambiguous external outcomes require manual reconciliation".to_string(),
    ]
}

fn report_guardrails() -> Vec<String> {
    vec![
        "QiWe targets are derived only from work_items.space_id and conversations.chat_id"
            .to_string(),
        "Space policy, automation, business, and capability versions are checked before claim"
            .to_string(),
        "an at-most-once attempt is committed before any QiWe room or send request".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_claim() -> ExecutionClaim {
        let automation_id = Uuid::new_v4();
        let business_id = Uuid::new_v4();
        ExecutionClaim {
            work_item_id: Uuid::new_v4(),
            space_id: Uuid::new_v4(),
            payload: json!({
                "automation_definition_id": automation_id,
                "automation_definition_digest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "automation_key": "resident_message",
                "automation_version": 3,
                "business_definition_id": business_id,
                "business_definition_digest": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "space_policy_version_id": "00000000-0000-0000-0000-000000000000",
                "space_policy_digest": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                "channel_event_mapping_id": "00000000-0000-0000-0000-000000000001",
                "channel_event_mapping_digest": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
                "trigger": {
                    "kind": "event",
                    "event_type": "group_member_added",
                    "provider_event_ref": "qiwe:event-1",
                    "subject_user_ids": ["9007199254740993", "9007199254740994"],
                    "occurred_at": "2026-08-14T00:00:00Z"
                },
                "target_group_id": "must-never-be-trusted"
            }),
            automation_id,
            automation_key: "resident_message".to_string(),
            automation_version: 3,
            automation_status: "active".to_string(),
            automation_digest: "a".repeat(64),
            trigger_kind: "event".to_string(),
            channel_event_mapping_id: Some(
                Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            ),
            channel_event_mapping_digest: Some("d".repeat(64)),
            business_id,
            business_status: "active".to_string(),
            business_digest: "b".repeat(64),
            execution_mode: "deterministic".to_string(),
            business_definition: json!({
                "capability_key": QIWE_TEXT_TEMPLATE_CAPABILITY_KEY,
                "input": {"text_template": "欢迎 {{subject_names}} 加入群聊"}
            }),
            business_allowed_capabilities: vec![QIWE_TEXT_TEMPLATE_CAPABILITY_KEY.to_string()],
            approval_policy: "space_admin_confirmation".to_string(),
            policy_id: Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap(),
            policy_digest: "c".repeat(64),
            policy_config: json!({
                "capability_grants": [QIWE_TEXT_TEMPLATE_CAPABILITY_KEY]
            }),
            selected_capability_key: QIWE_TEXT_TEMPLATE_CAPABILITY_KEY.to_string(),
            selected_capability_metadata: json!({
                crate::space_capability_recipe::RECIPE_METADATA_KEY:
                    crate::space_capability_recipe::QIWE_TEXT_TEMPLATE_V1
            }),
            conversation_chat_id: "trusted-room-from-space".to_string(),
            attempt_id: Some(Uuid::new_v4()),
        }
    }

    #[test]
    fn target_is_derived_from_space_even_when_payload_contains_an_evil_target() {
        let claim = fixture_claim();
        let plan = validate_claim_and_build_plan(&claim).expect("valid version-bound plan");
        assert!(matches!(plan, ExecutionPlan::QiweTextTemplate(_)));
        assert_eq!(claim.conversation_chat_id, "trusted-room-from-space");
        assert_ne!(
            claim.payload["target_group_id"].as_str(),
            Some(claim.conversation_chat_id.as_str())
        );
    }

    #[test]
    fn stale_business_definition_is_rejected() {
        let mut claim = fixture_claim();
        claim.business_status = "retired".to_string();
        let error = validate_claim_and_build_plan(&claim).expect_err("stale binding fails closed");
        assert!(error.to_string().contains("stale business definition"));
    }

    #[test]
    fn claim_rejects_drifted_version_digest_bindings() {
        for field in [
            "automation_definition_digest",
            "business_definition_digest",
            "space_policy_digest",
            "channel_event_mapping_digest",
        ] {
            let mut claim = fixture_claim();
            claim.payload[field] = json!("e".repeat(64));
            assert!(
                validate_claim_binding(&claim).is_err(),
                "drifted {field} must fail closed"
            );
        }
    }

    #[test]
    fn capability_must_be_in_both_business_and_space_policy_ceilings() {
        let mut claim = fixture_claim();
        claim.business_allowed_capabilities.clear();
        let error = validate_claim_and_build_plan(&claim)
            .expect_err("business capability ceiling fails closed");
        assert!(error.to_string().contains("business definition ceiling"));

        let mut claim = fixture_claim();
        claim.policy_config = json!({"capability_grants": []});
        let error =
            validate_claim_and_build_plan(&claim).expect_err("Space policy ceiling fails closed");
        assert!(error.to_string().contains("Space policy ceiling"));
    }

    #[test]
    fn wrong_space_binding_is_rejected_by_exact_ids() {
        let mut claim = fixture_claim();
        claim.payload["automation_definition_id"] = Value::String(Uuid::new_v4().to_string());
        let error = validate_claim_and_build_plan(&claim).expect_err("wrong binding fails closed");
        assert!(error.to_string().contains("automation definition binding"));
    }

    #[test]
    fn bad_member_ids_are_rejected_without_numeric_coercion() {
        let mut claim = fixture_claim();
        claim.payload["trigger"]["subject_user_ids"] = json!([9007199254740993_u64]);
        let error = validate_claim_and_build_plan(&claim).expect_err("numeric id fails closed");
        assert!(error.to_string().contains("must be strings"));

        claim.payload["trigger"]["subject_user_ids"] = json!(["member id with spaces"]);
        let error = validate_claim_and_build_plan(&claim).expect_err("invalid opaque id fails");
        assert!(error.to_string().contains("subject_user_id is invalid"));
    }

    #[test]
    fn multi_member_event_renders_one_combined_message_in_event_order() {
        let claim = fixture_claim();
        let ExecutionPlan::QiweTextTemplate(plan) =
            validate_claim_and_build_plan(&claim).expect("valid plan")
        else {
            panic!("expected QiWe text-template plan");
        };
        let roster = parse_exact_room_roster(
            Some(&json!({
                "roomList": [{
                    "roomId": "trusted-room-from-space",
                    "roomName": "二栋居民群",
                    "memberList": [
                        {"userId": "9007199254740994", "name": "小乙"},
                        {"userId": "9007199254740993", "name": "小甲"}
                    ]
                }]
            })),
            "trusted-room-from-space",
        )
        .expect("exact room roster");
        assert_eq!(roster.display_name, "二栋居民群");
        let names = current_subject_names(&plan.subject_user_ids, &roster).expect("current names");
        let message = render_qiwe_text_template(&plan, &names).expect("render one message");
        assert_eq!(names, vec!["小甲", "小乙"]);
        assert_eq!(message, "欢迎 小甲、小乙 加入群聊");
    }

    #[test]
    fn subjects_no_longer_in_the_exact_room_are_filtered() {
        let claim = fixture_claim();
        let ExecutionPlan::QiweTextTemplate(plan) =
            validate_claim_and_build_plan(&claim).expect("valid plan")
        else {
            panic!("expected QiWe text-template plan");
        };
        let roster = parse_exact_room_roster(
            Some(&json!({
                "roomList": [{
                    "roomId": "trusted-room-from-space",
                    "name": "二栋居民群",
                    "memberList": [{"userId": "9007199254740994", "name": "小乙"}]
                }]
            })),
            "trusted-room-from-space",
        )
        .expect("exact room roster");
        let names = current_subject_names(&plan.subject_user_ids, &roster).expect("intersection");
        assert_eq!(names, vec!["小乙"]);
    }

    #[test]
    fn wrong_room_response_never_verifies_members() {
        let error = parse_exact_room_roster(
            Some(&json!({
                "roomList": [{
                    "roomId": "other-room",
                    "memberList": [{"userId": "9007199254740993", "name": "小甲"}]
                }]
            })),
            "trusted-room-from-space",
        )
        .expect_err("wrong room fails closed");
        assert!(error.to_string().contains("exactly the requested room"));
    }

    #[test]
    fn invalid_room_display_name_fails_before_it_can_be_persisted() {
        let error = parse_exact_room_roster(
            Some(&json!({
                "roomList": [{
                    "roomId": "trusted-room-from-space",
                    "roomName": "bad\nroom",
                    "memberList": []
                }]
            })),
            "trusted-room-from-space",
        )
        .expect_err("control characters in room name fail closed");
        assert!(error.to_string().contains("display name is invalid"));
    }

    #[test]
    fn room_display_name_update_is_bound_to_space_and_exact_qiwe_chat_id() {
        assert!(PERSIST_ROOM_DISPLAY_NAME_SQL.contains("WHERE id = $1"));
        assert!(PERSIST_ROOM_DISPLAY_NAME_SQL.contains("chat_id = $2"));
        assert!(PERSIST_ROOM_DISPLAY_NAME_SQL.contains("platform = 'qiwe'"));
        assert!(PERSIST_ROOM_DISPLAY_NAME_SQL.contains("chat_type = 'group'"));
        assert!(!PERSIST_ROOM_DISPLAY_NAME_SQL.contains("WHERE display_name"));
    }

    #[test]
    fn duplicate_claim_guard_allows_only_the_initial_queue_state() {
        let query = eligible_claim_query(true);
        assert!(query.contains("work_item.status = 'queued'"));
        assert!(query.contains("work_item.attempts = 0"));
        assert!(query.contains("space_automation_execution_started"));
        assert!(query.contains("selected.provider_agent = 'erhua'"));
        assert!(query.contains(
            "selected.metadata ->> 'invocation_boundary' = 'erhua.execute_space_business'"
        ));
        assert!(query.contains("FOR UPDATE OF work_item SKIP LOCKED"));
    }

    #[test]
    fn business_failure_after_send_is_ambiguous_and_non_retryable() {
        let response = HttpResponse {
            status: 200,
            headers: Default::default(),
            body: br#"{"code":500,"msg":"provider rejected"}"#.to_vec(),
        };
        let outcome = classify_send_response(&response);
        let SendOutcome::Ambiguous {
            failure_code,
            response_summary,
        } = outcome
        else {
            panic!("non-success business response must be ambiguous");
        };
        assert_eq!(failure_code, "qiwe_text_business_response_ambiguous");
        assert_eq!(response_summary.expect("safe summary")["code"], 500);

        let success = classify_send_response(&HttpResponse {
            status: 200,
            headers: Default::default(),
            body: br#"{"code":0,"data":{"isSendSuccess":1}}"#.to_vec(),
        });
        let SendOutcome::Sent { response_summary } = success else {
            panic!("explicit QiWe success must be sent");
        };
        assert_eq!(response_summary["code"], 0);

        assert!(matches!(
            classify_send_response(&HttpResponse {
                status: 204,
                headers: Default::default(),
                body: br#"{"code":0,"data":[{"isSendSuccess":1}]}"#.to_vec(),
            }),
            SendOutcome::Sent { .. }
        ));

        for body in [
            br#"{"code":200}"#.as_slice(),
            br#"{"code":200,"data":{"isSendSuccess":1}}"#.as_slice(),
            br#"{"code":500,"data":{"isSendSuccess":1}}"#.as_slice(),
            br#"{"data":{"isSendSuccess":1}}"#.as_slice(),
            br#"{"code":0}"#.as_slice(),
            br#"{"code":0,"data":null}"#.as_slice(),
            br#"{"code":0,"data":"not-a-result"}"#.as_slice(),
            br#"{"code":0,"data":[]}"#.as_slice(),
            br#"{"code":0,"data":[{"isSendSuccess":1},{"isSendSuccess":1}]}"#.as_slice(),
            br#"{"code":0,"data":[null]}"#.as_slice(),
            br#"{"code":0,"data":{}}"#.as_slice(),
            br#"{"code":0,"data":{"isSendSuccess":null}}"#.as_slice(),
            br#"{"code":0,"data":{"isSendSuccess":0}}"#.as_slice(),
            br#"{"code":0,"data":{"isSendSuccess":2}}"#.as_slice(),
            br#"{"code":0,"data":{"isSendSuccess":"1"}}"#.as_slice(),
            br#"{"code":0,"data":{"isSendSuccess":true}}"#.as_slice(),
            br#"{"code":0,"data":{"isSendSuccess":1.0}}"#.as_slice(),
        ] {
            let outcome = classify_send_response(&HttpResponse {
                status: 200,
                headers: Default::default(),
                body: body.to_vec(),
            });
            let SendOutcome::Ambiguous {
                failure_code,
                response_summary,
            } = outcome
            else {
                panic!("unconfirmed QiWe response must be ambiguous");
            };
            assert_eq!(failure_code, "qiwe_text_business_response_ambiguous");
            assert!(response_summary
                .expect("safe summary")
                .get("isSendSuccess")
                .is_none());
        }

        assert!(matches!(
            classify_send_response(&HttpResponse {
                status: 503,
                headers: Default::default(),
                body: br#"{"code":0,"data":{"isSendSuccess":1}}"#.to_vec(),
            }),
            SendOutcome::Ambiguous {
                failure_code: "qiwe_text_http_status_ambiguous",
                ..
            }
        ));
        assert!(matches!(
            classify_send_response(&HttpResponse {
                status: 200,
                headers: Default::default(),
                body: br#"{"code":0,"data":broken}"#.to_vec(),
            }),
            SendOutcome::Ambiguous {
                failure_code: "qiwe_text_response_parse_ambiguous",
                response_summary: None
            }
        ));

        let failed_before_send = SendOutcome::FailedBeforeSend {
            failure_code: "fixture_not_sent",
        };
        assert!(matches!(
            failed_before_send,
            SendOutcome::FailedBeforeSend {
                failure_code: "fixture_not_sent"
            }
        ));
    }

    #[test]
    fn agent_turn_honors_the_business_output_contract_and_has_no_target() {
        let mut claim = fixture_claim();
        claim.execution_mode = "agent_turn".to_string();
        claim.business_definition = json!({
            "goal": "整理本群当天待办",
            "output_contract": {
                "type": "object",
                "additionalProperties": false,
                "required": ["summary"],
                "properties": {
                    "summary": {"type": "string", "minLength": 1, "maxLength": 200}
                }
            }
        });
        claim.business_allowed_capabilities = vec![AGENT_TURN_CAPABILITY_KEY.to_string()];
        claim.policy_config = json!({"capability_grants": [AGENT_TURN_CAPABILITY_KEY]});
        claim.selected_capability_key = AGENT_TURN_CAPABILITY_KEY.to_string();
        claim.selected_capability_metadata = json!({});
        let ExecutionPlan::AgentTurn(plan) =
            validate_claim_and_build_plan_with_readiness(&claim, true)
                .expect("valid constrained agent turn")
        else {
            panic!("expected agent-turn plan");
        };
        assert_eq!(plan.goal, "整理本群当天待办");
        let contract = plan.output_contract;
        assert_eq!(contract["additionalProperties"], false);
        assert_eq!(contract["required"], json!(["summary"]));
        assert!(contract["properties"].get("capability_requests").is_none());
    }

    #[test]
    fn agent_turn_never_reaches_the_handoff_plan_without_runner_readiness() {
        let mut claim = fixture_claim();
        claim.execution_mode = "agent_turn".to_string();
        claim.business_definition = json!({
            "goal": "Produce one bounded result.",
            "output_contract": {
                "type": "object",
                "additionalProperties": false,
                "required": ["summary"],
                "properties": {"summary": {"type": "string", "maxLength": 200}}
            }
        });
        claim.business_allowed_capabilities = vec![AGENT_TURN_CAPABILITY_KEY.to_string()];
        claim.policy_config = json!({"capability_grants": [AGENT_TURN_CAPABILITY_KEY]});
        claim.selected_capability_key = AGENT_TURN_CAPABILITY_KEY.to_string();
        claim.selected_capability_metadata = json!({});

        let error = validate_claim_and_build_plan_with_readiness(&claim, false)
            .expect_err("unready agent_turn must fail before a child can be queued");
        assert!(error.to_string().contains("broker and runner readiness"));

        let deterministic = fixture_claim();
        assert!(matches!(
            validate_claim_and_build_plan_with_readiness(&deterministic, false)
                .expect("deterministic execution remains available"),
            ExecutionPlan::QiweTextTemplate(_)
        ));
    }

    #[test]
    fn agent_turn_child_contract_is_brokered_and_exactly_idempotent() {
        let spec = AgentTurnChildSpec {
            id: Uuid::new_v4(),
            parent_work_item_id: Uuid::new_v4(),
            space_id: Uuid::new_v4(),
            source_refs: json!({
                "automation_definition_id": Uuid::new_v4(),
                "business_definition_id": Uuid::new_v4(),
                "parent_work_item_id": Uuid::new_v4(),
                "channel_event_mapping_id": Uuid::new_v4(),
                "channel_event_mapping_digest": "d".repeat(64)
            }),
            idempotency_key: format!("space-agent-turn:{}", Uuid::new_v4()),
            payload: json!({
                "schema_version": 1,
                "channel_event_mapping_digest": "d".repeat(64),
                "output_contract": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["summary"],
                    "properties": {"summary": {"type": "string", "maxLength": 200}}
                }
            }),
            metadata: json!({
                "space_bound": true,
                "definition_bound": true,
                "target_derived_from_space": true,
                "external_send_executed": false,
                "unrestricted_model_invocation": false,
                "handoff_state": crate::space_agent_turn::HANDOFF_STATE,
                "executor_boundary": crate::space_agent_turn::EXECUTOR_BOUNDARY,
                "runner_identity": crate::space_agent_turn::RUNNER_IDENTITY,
                "runner_contract_version": crate::space_agent_turn::RUNNER_CONTRACT_VERSION,
                "execution_gate": "owner_review_required"
            }),
        };
        let expected = spec.immutable_tuple();
        validate_agent_turn_child_immutable_tuple(&expected, &expected)
            .expect("exact immutable child may be reused");
        assert_eq!(
            expected["metadata"]["handoff_state"],
            crate::space_agent_turn::HANDOFF_STATE
        );
        assert_eq!(
            expected["metadata"]["runner_identity"],
            crate::space_agent_turn::RUNNER_IDENTITY
        );
        assert!(!expected.to_string().contains("capability_requests"));

        for field in [
            "parent_work_item_id",
            "space_id",
            "payload",
            "metadata",
            "status",
        ] {
            let mut forged = expected.clone();
            forged[field] = json!("different");
            assert!(
                validate_agent_turn_child_immutable_tuple(&forged, &expected).is_err(),
                "immutable mismatch in {field} must fail closed"
            );
        }
    }

    #[test]
    fn event_mapping_digest_is_required_but_schedule_has_no_mapping_binding() {
        let mut event_claim = fixture_claim();
        event_claim.channel_event_mapping_digest = None;
        assert!(validate_claim_binding(&event_claim).is_err());

        let mut schedule_claim = fixture_claim();
        schedule_claim.trigger_kind = "schedule".to_string();
        schedule_claim.payload["trigger"] = json!({
            "kind": "schedule",
            "scheduled_for_utc": "2026-08-14T00:00:00Z"
        });
        schedule_claim
            .payload
            .as_object_mut()
            .unwrap()
            .remove("channel_event_mapping_id");
        schedule_claim
            .payload
            .as_object_mut()
            .unwrap()
            .remove("channel_event_mapping_digest");
        schedule_claim.channel_event_mapping_id = None;
        schedule_claim.channel_event_mapping_digest = None;
        validate_claim_binding(&schedule_claim)
            .expect("schedule handoff must carry no event-mapping binding");
    }

    #[test]
    #[cfg(not(all(
        feature = "qiwe-production-adapter",
        not(feature = "qiwe-staging-adapter")
    )))]
    fn apply_compile_boundary_rejects_non_production_only_qiwe_builds() {
        assert!(validate_execution_compile_boundary().is_err());
    }

    #[test]
    #[cfg(all(
        feature = "qiwe-production-adapter",
        not(feature = "qiwe-staging-adapter")
    ))]
    fn apply_compile_boundary_accepts_only_the_production_qiwe_adapter() {
        validate_execution_compile_boundary().expect("production-only QiWe adapter is compiled");
    }
}
