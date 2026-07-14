use std::{
    fs,
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
    sync::Arc,
    time::Duration as StdDuration,
};

use anyhow::{anyhow, bail, Context, Result};
use chrono::{FixedOffset, TimeZone};
use rustls::{pki_types::ServerName, ClientConfig, ClientConnection, RootCertStore, Stream};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Row};
use tokio::time::sleep;
use uuid::Uuid;

use crate::{
    config::Cli,
    operations::{self, WorkItemCreateReport, WorkItemCreateRequest},
};

const ACTOR_AGENT: &str = "xiaoman";
const EVENT_SIGNAL_STATUSES: &[&str] = &["待处理", "处理中", "已完成", "已关闭"];
const ELIGIBLE_SIGNAL_WORKER_STATUSES: &[&str] = &["待处理", "处理中"];
const READ_ONLY_OPERATIONS: &[&str] = &[
    "record-get",
    "list-by-date",
    "material-summary",
    "shadow-validate",
];
const WRITE_OPERATIONS: &[&str] = &[
    "status-update",
    "gap-update",
    "handoff-create",
    "signal-ingest",
];
const TABLE_ROLES: &[&str] = &["activity_plan", "activity_occurrence"];
const FEISHU_BASE_API: &str = "https://open.feishu.cn/open-apis/bitable/v1/apps";
const FEISHU_AUTH_API: &str =
    "https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal";
const HANDOFF_TYPES: &[&str] = &[
    "visual_asset_request",
    "ops_followup",
    "member_notice",
    "human_confirmation",
    "activity_recap",
];
const HANDOFF_TARGETS: &[&str] = &["huabaosi", "silaoshi", "erhua", "default"];
const FEISHU_READ_LIMITATION: &str = "Feishu Base read is allowlisted and read-only; write parity, audit, and webhook shadow validation are still required before removing the legacy raw Base read path";

#[derive(Debug, Clone)]
pub struct ActivityRuntimeConfig {
    pub fixture_path: Option<PathBuf>,
    pub feishu_base: Option<ActivityFeishuBaseConfig>,
}

#[derive(Debug, Clone)]
pub struct SignalWorkerOptions {
    pub check_only: bool,
    pub once: bool,
    pub apply: bool,
    pub batch_size: i64,
    pub poll_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct ActivityFeishuBaseConfig {
    base_token: String,
    plan_table_id: String,
    occurrence_table_id: String,
    profile_env_path: String,
}

#[derive(Debug, Deserialize)]
struct ActivityPayload {
    actor_agent: String,
    operation: String,
    #[serde(default)]
    record_id: String,
    #[serde(default)]
    source_record_id: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    table_role: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    gap_summary: String,
    #[serde(default)]
    handoff_type: String,
    #[serde(default)]
    target_agent: String,
    #[serde(default)]
    brief_summary: String,
    #[serde(default)]
    event_signal_id: String,
    #[serde(default)]
    mutation_id: String,
    #[serde(default)]
    signal_type: String,
    #[serde(default)]
    activity_title: String,
    #[serde(default)]
    signal_date: String,
    #[serde(default)]
    chat_id: String,
    #[serde(default)]
    source_message_ids: Vec<String>,
    #[serde(default)]
    owner_name: String,
    #[serde(default)]
    priority: String,
    #[serde(default)]
    location: String,
    #[serde(default)]
    related_member_names: Vec<String>,
}

#[derive(Debug, Clone)]
struct ActivityRecord {
    record_id: String,
    table_role: String,
    title: String,
    activity_date: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    location: Option<String>,
    status: Option<String>,
    promotion_status: Option<String>,
    owner_name: Option<String>,
    initiator_name: Option<String>,
    material_summary: Option<String>,
    gap_summary: Option<String>,
    notes: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ActivityRecordView {
    table_role: String,
    record_ref: String,
    title: String,
    activity_date: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    location: Option<String>,
    status: Option<String>,
    promotion_status: Option<String>,
    owner_name: Option<String>,
    initiator_name: Option<String>,
    material_summary: Option<String>,
    gap_summary: Option<String>,
    notes: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Clone)]
struct FeishuClient {
    tenant_token: String,
    tls_config: Arc<ClientConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct FeishuProfileEnv {
    app_id: String,
    app_secret: String,
}

#[derive(Debug, Serialize)]
struct ActivityWorkerReport {
    success: bool,
    worker: &'static str,
    operation: String,
    source: String,
    actor_agent: String,
    dry_run: bool,
    apply_requested: bool,
    validation_status: String,
    action_status: String,
    safe_for_chat: bool,
    record_count: usize,
    records: Vec<ActivityRecordView>,
    summaries: Vec<String>,
    operations_work_item: Option<WorkItemCreateReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mutation_applied: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    feishu_record_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_signal_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    matched_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    missing_in_agentos: Option<Vec<ShadowItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    missing_in_feishu: Option<Vec<ShadowItem>>,
    limitations: Vec<String>,
    guardrails: Vec<String>,
}

#[derive(Debug, Clone)]
struct ActivityShadowReport {
    action_status: String,
    feishu_record_count: usize,
    event_signal_count: usize,
    matched_count: usize,
    missing_in_agentos: Vec<ShadowItem>,
    missing_in_feishu: Vec<ShadowItem>,
}

#[derive(Debug)]
struct EventSignalMutationOutcome {
    action_status: &'static str,
    mutation_applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ShadowItem {
    title: String,
    normalized_title: String,
    signal_date: String,
}

#[derive(Debug, Clone)]
struct EventSignalActivity {
    title: String,
    signal_date: String,
}

#[derive(Debug, Clone)]
struct EventSignalIngestCandidate {
    id: Uuid,
    signal_type: String,
    title: String,
    summary: String,
    signal_date: String,
    chat_id: String,
    source_message_ids: Vec<Uuid>,
    owner_name: String,
    priority: String,
    related_member_names: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SignalWorkerReport {
    success: bool,
    worker: &'static str,
    source: &'static str,
    dry_run: bool,
    apply_requested: bool,
    check_only: bool,
    batch_size: i64,
    scanned_count: usize,
    submitted_count: usize,
    existing_count: usize,
    review_needed_count: usize,
    action_status: String,
    safe_for_chat: bool,
    work_items: Vec<WorkItemCreateReport>,
    limitations: Vec<String>,
    guardrails: Vec<String>,
}

pub async fn run(
    cli: &Cli,
    operation: String,
    payload_json: String,
    apply: bool,
    dry_run: bool,
    fixture_path: Option<PathBuf>,
    use_feishu_base: bool,
) -> Result<()> {
    if apply && dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let payload: ActivityPayload = serde_json::from_str(&payload_json)?;
    validate(&operation, &payload)?;
    let config = ActivityRuntimeConfig {
        fixture_path,
        feishu_base: if use_feishu_base {
            Some(activity_feishu_base_config(cli)?)
        } else {
            None
        },
    };
    let report = execute_with_config(cli, operation, payload, apply, dry_run, &config).await?;

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_signal_worker(cli: &Cli, options: SignalWorkerOptions) -> Result<()> {
    if options.check_only || options.once {
        let report = run_signal_worker_batch(cli, &options).await?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    let poll_seconds = options.poll_seconds.max(60);
    loop {
        match run_signal_worker_batch(cli, &options).await {
            Ok(report) => tracing::info!(
                scanned = report.scanned_count,
                submitted = report.submitted_count,
                existing = report.existing_count,
                "xiaoman activity signal worker batch complete"
            ),
            Err(error) => tracing::warn!(
                error = %error,
                "xiaoman activity signal worker batch failed"
            ),
        }
        sleep(StdDuration::from_secs(poll_seconds)).await;
    }
}

async fn execute_with_config(
    cli: &Cli,
    operation: String,
    payload: ActivityPayload,
    apply: bool,
    dry_run: bool,
    config: &ActivityRuntimeConfig,
) -> Result<ActivityWorkerReport> {
    let operation_is_read = READ_ONLY_OPERATIONS.contains(&operation.as_str());
    let apply_requested = apply && !dry_run;
    let mut report = base_report(operation.clone(), &payload, apply_requested);

    if operation == "shadow-validate" {
        execute_shadow_validate(cli, &mut report, &payload, apply_requested, config).await?;
    } else if operation_is_read && apply_requested {
        execute_read_operation(&mut report, &operation, &payload, config)?;
    } else if operation_is_read {
        report.action_status = "dry_run_ok".to_string();
        report.limitations.push(
            "read operation was validated only; use --apply with an approved source for replay"
                .to_string(),
        );
    } else if operation == "status-update" || operation == "gap-update" {
        execute_event_signal_mutation(cli, &mut report, &payload, apply_requested).await?;
    } else if operation == "handoff-create" {
        execute_handoff_create(cli, &mut report, &payload, apply_requested).await?;
    } else if operation == "signal-ingest" {
        execute_signal_ingest(cli, &mut report, &payload, apply_requested).await?;
    } else {
        report.action_status = "dry_run_ok".to_string();
    }

    Ok(report)
}

fn base_report(
    operation: String,
    payload: &ActivityPayload,
    apply_requested: bool,
) -> ActivityWorkerReport {
    ActivityWorkerReport {
        success: true,
        worker: "xiaoman-activity",
        operation,
        source: "validation_only".to_string(),
        actor_agent: payload.actor_agent.clone(),
        dry_run: !apply_requested,
        apply_requested,
        validation_status: "ok".to_string(),
        action_status: "validation_ok".to_string(),
        safe_for_chat: false,
        record_count: 0,
        records: Vec::new(),
        summaries: Vec::new(),
        operations_work_item: None,
        mutation_applied: None,
        feishu_record_count: None,
        event_signal_count: None,
        matched_count: None,
        missing_in_agentos: None,
        missing_in_feishu: None,
        limitations: Vec::new(),
        guardrails: vec![
            "validates wrapper payload before any data access".to_string(),
            "fixture replay is allowed for local acceptance only".to_string(),
            "status-update and gap-update write only allowlisted AgentOS event-signal fields with mutation audit".to_string(),
            "technical report is for logs, not WeCom users".to_string(),
            "record_ref is hashed; raw Base record ids are not included in records".to_string(),
        ],
    }
}

async fn execute_event_signal_mutation(
    cli: &Cli,
    report: &mut ActivityWorkerReport,
    payload: &ActivityPayload,
    apply_requested: bool,
) -> Result<()> {
    report.source = "agentos_event_signals".to_string();
    report.safe_for_chat = false;
    report.record_count = 1;
    report.mutation_applied = Some(false);
    report
        .guardrails
        .push("event signal mutations never use Feishu record ids or write Feishu".to_string());
    report.guardrails.push(
        "mutation_id provides replay safety and each applied change writes an audit row"
            .to_string(),
    );

    if !apply_requested {
        report.action_status = match payload.operation.as_str() {
            "status-update" => "event_signal_status_preview",
            "gap-update" => "event_signal_gap_preview",
            _ => unreachable!("mutation operation validated before execution"),
        }
        .to_string();
        report
            .summaries
            .push("AgentOS event-signal mutation validated without database writes".to_string());
        return Ok(());
    }

    let database_url = cli.database_url_required()?;
    let pool = crate::db::connect(database_url, cli.db_max_connections).await?;
    let outcome = apply_event_signal_mutation(&pool, payload).await?;
    report.action_status = outcome.action_status.to_string();
    report.mutation_applied = Some(outcome.mutation_applied);
    report.summaries.push(if outcome.mutation_applied {
        "AgentOS event-signal mutation and audit row were committed".to_string()
    } else {
        "Existing AgentOS event-signal mutation was returned idempotently".to_string()
    });
    Ok(())
}

async fn apply_event_signal_mutation(
    pool: &PgPool,
    payload: &ActivityPayload,
) -> Result<EventSignalMutationOutcome> {
    let event_signal_id = Uuid::parse_str(payload.event_signal_id.trim())
        .context("event_signal_id must be a UUID")?;
    let mutation_id =
        Uuid::parse_str(payload.mutation_id.trim()).context("mutation_id must be a UUID")?;
    let idempotency_key = event_signal_mutation_idempotency_key(event_signal_id, mutation_id);
    let new_value = event_signal_mutation_value(payload)?;

    let mut tx = pool
        .begin()
        .await
        .context("begin event signal mutation transaction")?;
    let row = sqlx::query(
        r#"
        SELECT status, gap_summary
        FROM qintopia_agent_os.event_signals
        WHERE id = $1
          AND owner_agent = 'xiaoman'
        FOR UPDATE
        "#,
    )
    .bind(event_signal_id)
    .fetch_optional(&mut *tx)
    .await
    .context("lock Xiaoman event signal for mutation")?
    .context("event_signal_id does not reference a Xiaoman event signal")?;

    let existing = sqlx::query(
        r#"
        SELECT event_signal_id, operation, new_value
        FROM qintopia_agent_os.event_signal_mutations
        WHERE mutation_id = $1
           OR idempotency_key = $2
        LIMIT 1
        "#,
    )
    .bind(mutation_id)
    .bind(&idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .context("load existing event signal mutation")?;
    if let Some(existing) = existing {
        let existing_event_signal_id: Uuid = existing.try_get("event_signal_id")?;
        let existing_operation: String = existing.try_get("operation")?;
        let existing_new_value: Value = existing.try_get("new_value")?;
        if existing_event_signal_id != event_signal_id
            || existing_operation != payload.operation
            || existing_new_value != new_value
        {
            bail!("mutation_id was already used for a different event-signal mutation");
        }
        tx.commit()
            .await
            .context("commit idempotent event signal mutation transaction")?;
        return Ok(EventSignalMutationOutcome {
            action_status: "event_signal_mutation_idempotent_existing",
            mutation_applied: false,
        });
    }

    let previous_value = match payload.operation.as_str() {
        "status-update" => {
            let previous_status: String = row.try_get("status")?;
            let next_status = payload.status.trim();
            validate_status_transition(&previous_status, next_status)?;
            let result = sqlx::query(
                r#"
                UPDATE qintopia_agent_os.event_signals
                SET status = $2,
                    updated_at = now()
                WHERE id = $1
                "#,
            )
            .bind(event_signal_id)
            .bind(next_status)
            .execute(&mut *tx)
            .await
            .context("update Xiaoman event signal status")?;
            if result.rows_affected() != 1 {
                bail!("event signal status update did not affect exactly one row");
            }
            json!({"status": previous_status})
        }
        "gap-update" => {
            let previous_gap: String = row.try_get("gap_summary")?;
            let next_gap = normalize_gap_summary(&payload.gap_summary)?;
            let result = sqlx::query(
                r#"
                UPDATE qintopia_agent_os.event_signals
                SET gap_summary = $2,
                    updated_at = now()
                WHERE id = $1
                "#,
            )
            .bind(event_signal_id)
            .bind(next_gap)
            .execute(&mut *tx)
            .await
            .context("update Xiaoman event signal gap summary")?;
            if result.rows_affected() != 1 {
                bail!("event signal gap update did not affect exactly one row");
            }
            json!({"gap_summary": previous_gap})
        }
        _ => unreachable!("mutation operation validated before execution"),
    };

    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.event_signal_mutations
            (event_signal_id, mutation_id, idempotency_key, operation, actor_agent,
             previous_value, new_value, metadata)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(event_signal_id)
    .bind(mutation_id)
    .bind(idempotency_key)
    .bind(&payload.operation)
    .bind(ACTOR_AGENT)
    .bind(previous_value)
    .bind(new_value)
    .bind(json!({
        "source": "xiaoman-activity",
        "feishu_write_executed": false,
        "external_send_executed": false,
    }))
    .execute(&mut *tx)
    .await
    .context("append event signal mutation audit")?;
    tx.commit()
        .await
        .context("commit event signal mutation transaction")?;

    Ok(EventSignalMutationOutcome {
        action_status: match payload.operation.as_str() {
            "status-update" => "event_signal_status_updated",
            "gap-update" => "event_signal_gap_updated",
            _ => unreachable!("mutation operation validated before execution"),
        },
        mutation_applied: true,
    })
}

fn event_signal_mutation_idempotency_key(event_signal_id: Uuid, mutation_id: Uuid) -> String {
    format!("xiaoman_event_signal:{event_signal_id}:{mutation_id}")
}

fn event_signal_mutation_value(payload: &ActivityPayload) -> Result<Value> {
    match payload.operation.as_str() {
        "status-update" => Ok(json!({"status": payload.status.trim()})),
        "gap-update" => Ok(json!({
            "gap_summary": normalize_gap_summary(&payload.gap_summary)?
        })),
        _ => bail!("operation is not an event-signal mutation"),
    }
}

fn validate_status_transition(previous: &str, next: &str) -> Result<()> {
    if previous == next {
        return Ok(());
    }
    let allowed = matches!(
        (previous, next),
        ("待处理", "处理中" | "已完成" | "已关闭") | ("处理中", "待处理" | "已完成" | "已关闭")
    );
    if !allowed {
        bail!("event signal status transition is not allowed");
    }
    Ok(())
}

fn normalize_gap_summary(value: &str) -> Result<String> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        bail!("gap_summary is required");
    }
    if normalized.chars().count() > 500 {
        bail!("gap_summary must be 500 characters or fewer");
    }
    if contains_disallowed_mutation_text(&normalized) {
        bail!("gap_summary contains disallowed sensitive or raw internal content");
    }
    Ok(normalized)
}

fn contains_disallowed_mutation_text(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "app_token",
        "tenant_access_token",
        "authorization: bearer",
        "base_token",
        "table_id",
        "message_id",
        "chat_id",
        "sender_id",
        "system prompt",
        "raw private chat",
        "member dossier",
        "http://",
        "https://",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
        || value.contains('@')
        || value
            .split(|character: char| !character.is_ascii_digit())
            .any(|digits| digits.len() >= 7)
        || value.split_whitespace().any(|token| {
            token.chars().count() >= 32
                && token
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || "-_:".contains(character))
        })
}

async fn execute_handoff_create(
    cli: &Cli,
    report: &mut ActivityWorkerReport,
    payload: &ActivityPayload,
    apply_requested: bool,
) -> Result<()> {
    let request = handoff_work_item_request(payload)?;
    let work_item_report = if apply_requested {
        let database_url = cli.database_url_required()?;
        let pool = crate::db::connect(database_url, cli.db_max_connections).await?;
        let policy = operations::OperationsPolicy::from_cli(cli, true);
        operations::create_work_item(&pool, request, true, &policy).await?
    } else {
        operations::create_work_item_dry_run(request)?
    };

    report.action_status = format!("operations_{}", work_item_report.action_status);
    report.source = "agentos_operations_control_plane".to_string();
    report.safe_for_chat = false;
    report.operations_work_item = Some(work_item_report);
    report.guardrails.push(
        "handoff-create creates a capability-governed work item instead of raw prompt handoff"
            .to_string(),
    );
    report
        .guardrails
        .push("Feishu Task board mirroring remains a separate sync worker boundary".to_string());
    Ok(())
}

async fn execute_signal_ingest(
    cli: &Cli,
    report: &mut ActivityWorkerReport,
    payload: &ActivityPayload,
    apply_requested: bool,
) -> Result<()> {
    let missing_fields = signal_missing_fields(payload);
    let request = signal_work_item_request(payload, &missing_fields);
    let work_item_report = if apply_requested {
        let database_url = cli.database_url_required()?;
        let pool = crate::db::connect(database_url, cli.db_max_connections).await?;
        let policy = operations::OperationsPolicy::from_cli(cli, true);
        operations::create_work_item(&pool, request, true, &policy).await?
    } else {
        operations::create_work_item_dry_run(request)?
    };

    report.source = "agentos_event_signal".to_string();
    report.safe_for_chat = false;
    report.operations_work_item = Some(work_item_report);
    if missing_fields.is_empty() {
        report.action_status = report
            .operations_work_item
            .as_ref()
            .map(|item| format!("operations_{}", item.action_status))
            .unwrap_or_else(|| "operations_unavailable".to_string());
    } else {
        report.validation_status = "review_needed".to_string();
        report.action_status = "review_needed".to_string();
        report.limitations.push(format!(
            "activity signal missing required fields: {}",
            missing_fields.join(", ")
        ));
    }
    report.guardrails.push(
        "signal-ingest writes only AgentOS control-plane work items; it does not create visual assets or external sends".to_string(),
    );
    report
        .guardrails
        .push("duplicate signals use a stable event_signal_id idempotency key".to_string());
    Ok(())
}

async fn run_signal_worker_batch(
    cli: &Cli,
    options: &SignalWorkerOptions,
) -> Result<SignalWorkerReport> {
    let apply_requested = options.apply && !options.check_only;
    let database_url = cli.database_url_required()?;
    let pool = crate::db::connect(database_url, cli.db_max_connections).await?;
    let candidates = load_signal_ingest_candidates(&pool, options.batch_size).await?;
    let policy = operations::OperationsPolicy::from_cli(cli, true);
    let mut work_items = Vec::new();
    let mut review_needed_count = 0;

    for candidate in &candidates {
        let payload = candidate.to_activity_payload();
        let missing_fields = signal_missing_fields(&payload);
        if !missing_fields.is_empty() {
            review_needed_count += 1;
        }
        let request = signal_work_item_request(&payload, &missing_fields);
        let report = if apply_requested {
            operations::create_work_item(&pool, request, true, &policy).await?
        } else {
            operations::create_work_item_dry_run(request)?
        };
        work_items.push(report);
    }

    let existing_count = work_items.iter().filter(|item| item.existing).count();
    let submitted_count = work_items.len().saturating_sub(existing_count);
    let action_status = if candidates.is_empty() {
        "no_eligible_signals"
    } else if apply_requested {
        "signal_ingest_submitted"
    } else {
        "signal_ingest_preview"
    }
    .to_string();

    Ok(SignalWorkerReport {
        success: true,
        worker: "xiaoman-activity-signal-worker",
        source: "agentos_event_signals",
        dry_run: !apply_requested,
        apply_requested,
        check_only: options.check_only,
        batch_size: options.batch_size.max(1),
        scanned_count: candidates.len(),
        submitted_count,
        existing_count,
        review_needed_count,
        action_status,
        safe_for_chat: false,
        work_items,
        limitations: vec![
            "worker submits only AgentOS signal-ingest work items; it does not write Feishu or send QiWe messages".to_string(),
            "long-running production scheduling remains disabled until reviewed runtime config is added".to_string(),
        ],
        guardrails: vec![
            "selects owner_agent=xiaoman activity event_signals only".to_string(),
            "skips event_signals that already have a xiaoman.create_activity_request work item".to_string(),
            "uses signal-ingest idempotency keys for replay safety".to_string(),
            "technical report is for logs, not WeCom users".to_string(),
        ],
    })
}

async fn load_signal_ingest_candidates(
    pool: &PgPool,
    batch_size: i64,
) -> Result<Vec<EventSignalIngestCandidate>> {
    let rows = sqlx::query(
        r#"
        SELECT
            signals.id,
            signals.signal_type,
            signals.title,
            signals.summary,
            signals.signal_date::text AS signal_date,
            signals.chat_id,
            signals.source_message_ids,
            signals.owner_name,
            signals.priority,
            signals.related_member_names
        FROM qintopia_agent_os.event_signals signals
        WHERE signals.owner_agent = 'xiaoman'
          AND signals.signal_type = '活动/聚会'
          AND signals.status = ANY($1::text[])
          AND NOT EXISTS (
              SELECT 1
              FROM qintopia_agent_os.work_items work_items
              WHERE work_items.source_event_signal_id = signals.id
                AND work_items.capability_key = 'xiaoman.create_activity_request'
          )
        ORDER BY signals.signal_date ASC, signals.created_at ASC
        LIMIT $2
        "#,
    )
    .bind(ELIGIBLE_SIGNAL_WORKER_STATUSES)
    .bind(batch_size.max(1))
    .fetch_all(pool)
    .await
    .context("load Xiaoman event_signals for signal-ingest worker")?;

    rows.into_iter()
        .map(EventSignalIngestCandidate::from_row)
        .collect()
}

async fn execute_shadow_validate(
    cli: &Cli,
    report: &mut ActivityWorkerReport,
    payload: &ActivityPayload,
    apply_requested: bool,
    config: &ActivityRuntimeConfig,
) -> Result<()> {
    report.source = "feishu_agentos_shadow".to_string();
    report.safe_for_chat = false;
    report.guardrails.push(
        "shadow-validate compares Feishu mirror coverage against AgentOS event_signals without treating Feishu as source of truth".to_string(),
    );
    report.guardrails.push(
        "shadow-validate is read-only and must not write Feishu, QiWe, or AgentOS rows".to_string(),
    );

    if !apply_requested {
        report.action_status = "dry_run_ok".to_string();
        report.limitations.push(
            "shadow-validate dry-run only validates payload shape; use --apply with explicit Feishu allowlist and database config for read-only comparison".to_string(),
        );
        return Ok(());
    }

    let Some(base_config) = config.feishu_base.as_ref() else {
        report.action_status = "shadow_source_not_configured".to_string();
        apply_shadow_report(report, ActivityShadowReport::not_configured());
        report.limitations.push(
            "set --use-feishu-base with the explicit Xiaoman Feishu allowlist before shadow validation".to_string(),
        );
        return Ok(());
    };

    let database_url = match cli.database_url.as_ref() {
        Some(url) if !url.trim().is_empty() => url,
        _ => {
            report.action_status = "shadow_source_not_configured".to_string();
            apply_shadow_report(report, ActivityShadowReport::not_configured());
            report.limitations.push(
                "QINTOPIA_SIDECAR_DATABASE_URL is required for AgentOS event_signals shadow validation".to_string(),
            );
            return Ok(());
        }
    };

    let feishu_records = load_feishu_records_for_shadow(base_config)?;
    let feishu_items = shadow_items_from_records(&feishu_records, &payload.date);
    let pool = crate::db::connect(database_url, cli.db_max_connections).await?;
    let event_signals = load_xiaoman_event_signals_for_shadow(&pool, &payload.date).await?;
    let event_items = shadow_items_from_event_signals(&event_signals);
    let shadow = compare_shadow_items(&feishu_items, &event_items);

    report.action_status = shadow.action_status.clone();
    report.record_count = shadow.feishu_record_count;
    apply_shadow_report(report, shadow);
    Ok(())
}

fn apply_shadow_report(report: &mut ActivityWorkerReport, shadow: ActivityShadowReport) {
    report.feishu_record_count = Some(shadow.feishu_record_count);
    report.event_signal_count = Some(shadow.event_signal_count);
    report.matched_count = Some(shadow.matched_count);
    report.missing_in_agentos = Some(shadow.missing_in_agentos);
    report.missing_in_feishu = Some(shadow.missing_in_feishu);
}

fn signal_work_item_request(
    payload: &ActivityPayload,
    missing_fields: &[String],
) -> WorkItemCreateRequest {
    let source_event_signal_id = signal_uuid(&payload.event_signal_id);
    let source_refs = signal_source_refs(payload, source_event_signal_id);
    let review_needed = !missing_fields.is_empty();
    let brief_summary = if payload.brief_summary.trim().is_empty() {
        signal_brief_summary(payload, review_needed)
    } else {
        payload.brief_summary.trim().to_string()
    };

    WorkItemCreateRequest {
        requester_agent: "default".to_string(),
        target_agent: "xiaoman".to_string(),
        capability_key: "xiaoman.create_activity_request".to_string(),
        work_item_type: "activity_promotion_request".to_string(),
        brief_summary,
        purpose: "activity_signal_intake".to_string(),
        human_owner: payload.owner_name.trim().to_string(),
        priority: signal_priority(&payload.priority),
        source_type: "event_signal".to_string(),
        source_refs,
        source_event_signal_id,
        payload: json!({
            "workflow": "workflows/xiaoman-activity-signal",
            "requested_by": payload.actor_agent,
            "signal_type": payload.signal_type,
            "activity_title": payload.activity_title,
            "signal_date": payload.signal_date,
            "location": payload.location,
            "gap_summary": payload.gap_summary,
            "review_needed": review_needed,
            "missing_required_fields": missing_fields,
        }),
        payload_redaction_policy: "summary_only".to_string(),
        idempotency_key: signal_idempotency_key(&payload.event_signal_id),
        dedupe_key: String::new(),
        metadata: json!({
            "created_by_wrapper": "xiaoman-activity",
            "workflow": "workflows/xiaoman-activity-signal",
            "review_needed": review_needed,
            "missing_required_fields": missing_fields,
            "related_member_names_count": payload.related_member_names.len(),
        }),
        parent_work_item_id: None,
        approved_artifact_id: None,
    }
}

fn signal_source_refs(payload: &ActivityPayload, source_event_signal_id: Option<Uuid>) -> Value {
    let event_ref = source_event_signal_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| payload.event_signal_id.trim().to_string());
    let message_refs: Vec<String> = payload
        .source_message_ids
        .iter()
        .map(|item| activity_record_ref("event_signal_message", item))
        .collect();
    json!({
        "event_signal_id": event_ref,
        "signal_type": payload.signal_type,
        "signal_date": payload.signal_date,
        "chat_ref": activity_record_ref("event_signal_chat", &payload.chat_id),
        "source_message_refs": message_refs,
    })
}

fn signal_missing_fields(payload: &ActivityPayload) -> Vec<String> {
    [
        ("event_signal_id", payload.event_signal_id.as_str()),
        ("signal_type", payload.signal_type.as_str()),
        ("activity_title", payload.activity_title.as_str()),
        ("signal_date", payload.signal_date.as_str()),
    ]
    .into_iter()
    .filter_map(|(name, value)| {
        if value.trim().is_empty() {
            Some(name.to_string())
        } else {
            None
        }
    })
    .collect()
}

fn signal_uuid(value: &str) -> Option<Uuid> {
    Uuid::parse_str(value.trim()).ok()
}

fn signal_idempotency_key(event_signal_id: &str) -> String {
    format!("xiaoman_activity_signal:{}", event_signal_id.trim())
}

fn signal_priority(value: &str) -> String {
    match value.trim() {
        "high" | "urgent" | "low" | "normal" => value.trim().to_string(),
        "高" => "high".to_string(),
        "低" => "low".to_string(),
        _ => "normal".to_string(),
    }
}

fn signal_brief_summary(payload: &ActivityPayload, review_needed: bool) -> String {
    let title = if payload.activity_title.trim().is_empty() {
        "未命名活动"
    } else {
        payload.activity_title.trim()
    };
    let date = if payload.signal_date.trim().is_empty() {
        "日期待确认"
    } else {
        payload.signal_date.trim()
    };
    if review_needed {
        format!("{title} 活动信号需要人工补齐字段")
    } else {
        format!("{title} 活动信号待小满跟进（{date}）")
    }
}

fn handoff_work_item_request(payload: &ActivityPayload) -> Result<WorkItemCreateRequest> {
    let (capability_key, work_item_type) = match (
        payload.handoff_type.as_str(),
        payload.target_agent.as_str(),
    ) {
        ("visual_asset_request", "huabaosi") => (
            "huabaosi.create_visual_asset".to_string(),
            "visual_asset_request".to_string(),
        ),
        _ => bail!(
            "handoff-create is not mapped to an operations capability for handoff_type={} target_agent={}",
            payload.handoff_type,
            payload.target_agent
        ),
    };

    let table_role = if payload.table_role.trim().is_empty() {
        "activity_occurrence"
    } else {
        payload.table_role.as_str()
    };
    if !TABLE_ROLES.contains(&table_role) {
        bail!("table_role is not allowed");
    }

    let source_record_ref = activity_record_ref(table_role, &payload.source_record_id);
    let source_refs = json!({
        "source_record_ref": source_record_ref.clone(),
        "table_role": table_role,
        "wrapper_operation": payload.operation,
    });
    let payload_value = json!({
        "handoff_type": payload.handoff_type,
        "source_record_ref": source_record_ref.clone(),
        "table_role": table_role,
    });

    Ok(WorkItemCreateRequest {
        requester_agent: payload.actor_agent.clone(),
        target_agent: payload.target_agent.clone(),
        capability_key,
        work_item_type,
        brief_summary: payload.brief_summary.clone(),
        purpose: "activity_operations_handoff".to_string(),
        human_owner: String::new(),
        priority: "normal".to_string(),
        source_type: "xiaoman_activity".to_string(),
        source_refs,
        source_event_signal_id: None,
        payload: payload_value,
        payload_redaction_policy: "summary_only".to_string(),
        idempotency_key: String::new(),
        dedupe_key: String::new(),
        metadata: json!({
            "created_by_wrapper": "xiaoman-activity",
            "source_record_ref": source_record_ref,
        }),
        parent_work_item_id: None,
        approved_artifact_id: None,
    })
}

fn activity_record_ref(table_role: &str, record_id: &str) -> String {
    let digest = Sha256::digest(format!("{table_role}:{record_id}"));
    let mut suffix = String::new();
    for byte in &digest[..6] {
        suffix.push_str(&format!("{byte:02x}"));
    }
    format!("{table_role}:{suffix}")
}

fn execute_read_operation(
    report: &mut ActivityWorkerReport,
    operation: &str,
    payload: &ActivityPayload,
    config: &ActivityRuntimeConfig,
) -> Result<()> {
    let (records, source, limitation) = if let Some(path) = config.fixture_path.as_deref() {
        (
            load_fixture_records(path)?,
            "fixture".to_string(),
            "fixture-backed read proves wrapper shape and filtering; it is not production Base parity by itself".to_string(),
        )
    } else if let Some(base_config) = config.feishu_base.as_ref() {
        (
            load_feishu_records(base_config, &payload.table_role)?,
            "feishu_base_read_only".to_string(),
            FEISHU_READ_LIMITATION.to_string(),
        )
    } else {
        report.action_status = "read_source_not_configured".to_string();
        report.limitations.push(
            "set QINTOPIA_XIAOMAN_ACTIVITY_FIXTURE_PATH/--fixture-path for replay or enable QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE with an allowlisted Base config".to_string(),
        );
        return Ok(());
    };
    report.source = source;
    report.limitations.push(limitation);

    let matched: Vec<ActivityRecord> = match operation {
        "record-get" | "material-summary" => records
            .into_iter()
            .filter(|record| {
                record.table_role == payload.table_role && record.record_id == payload.record_id
            })
            .collect(),
        "list-by-date" => records
            .into_iter()
            .filter(|record| {
                record.table_role == payload.table_role && record.matches_date(&payload.date)
            })
            .collect(),
        _ => unreachable!("read operation allowlist checked above"),
    };

    report.record_count = matched.len();
    report.records = matched.iter().map(ActivityRecord::view).collect();
    report.summaries = matched.iter().map(ActivityRecord::safe_summary).collect();
    report.action_status = if matched.is_empty() {
        "record_not_found"
    } else {
        "read_ok"
    }
    .to_string();
    Ok(())
}

fn load_fixture_records(path: &std::path::Path) -> Result<Vec<ActivityRecord>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let value: Value = serde_json::from_str(&text).context("parse Xiaoman activity fixture")?;
    let items = if let Some(items) = value.as_array() {
        items.clone()
    } else {
        value
            .get("records")
            .and_then(Value::as_array)
            .cloned()
            .context("fixture must be an array or an object with records")?
    };
    items
        .iter()
        .map(ActivityRecord::from_fixture_value)
        .collect()
}

fn load_feishu_records(
    config: &ActivityFeishuBaseConfig,
    table_role: &str,
) -> Result<Vec<ActivityRecord>> {
    let table_id = config.table_id_for_role(table_role)?;
    let client = FeishuClient::from_profile_env(&config.profile_env_path)?;
    client
        .list_records(&config.base_token, table_id, 200)?
        .iter()
        .map(|record| ActivityRecord::from_feishu_value(record, table_role))
        .collect()
}

fn load_feishu_records_for_shadow(
    config: &ActivityFeishuBaseConfig,
) -> Result<Vec<ActivityRecord>> {
    let mut records = Vec::new();
    for table_role in TABLE_ROLES {
        records.extend(load_feishu_records(config, table_role)?);
    }
    Ok(records)
}

async fn load_xiaoman_event_signals_for_shadow(
    pool: &PgPool,
    signal_date: &str,
) -> Result<Vec<EventSignalActivity>> {
    let rows = sqlx::query(
        r#"
        SELECT title, signal_date::text AS signal_date
        FROM qintopia_agent_os.event_signals
        WHERE owner_agent = 'xiaoman'
          AND signal_date = $1::date
          AND status <> '已关闭'
        ORDER BY title ASC, created_at ASC
        "#,
    )
    .bind(signal_date)
    .fetch_all(pool)
    .await
    .context("load Xiaoman event_signals for shadow validation")?;

    rows.into_iter()
        .map(|row| {
            Ok(EventSignalActivity {
                title: row.try_get("title")?,
                signal_date: row.try_get("signal_date")?,
            })
        })
        .collect()
}

fn shadow_items_from_records(records: &[ActivityRecord], signal_date: &str) -> Vec<ShadowItem> {
    let mut items = records
        .iter()
        .filter(|record| record.matches_date(signal_date))
        .map(|record| ShadowItem::new(record.title.clone(), signal_date.to_string()))
        .collect::<Vec<_>>();
    sort_and_dedupe_shadow_items(&mut items);
    items
}

fn shadow_items_from_event_signals(signals: &[EventSignalActivity]) -> Vec<ShadowItem> {
    let mut items = signals
        .iter()
        .map(|signal| ShadowItem::new(signal.title.clone(), signal.signal_date.clone()))
        .collect::<Vec<_>>();
    sort_and_dedupe_shadow_items(&mut items);
    items
}

fn compare_shadow_items(
    feishu_items: &[ShadowItem],
    event_items: &[ShadowItem],
) -> ActivityShadowReport {
    let missing_in_agentos = feishu_items
        .iter()
        .filter(|item| {
            !event_items
                .iter()
                .any(|candidate| shadow_key_matches(item, candidate))
        })
        .cloned()
        .collect::<Vec<_>>();
    let missing_in_feishu = event_items
        .iter()
        .filter(|item| {
            !feishu_items
                .iter()
                .any(|candidate| shadow_key_matches(item, candidate))
        })
        .cloned()
        .collect::<Vec<_>>();
    let matched_count = feishu_items
        .iter()
        .filter(|item| {
            event_items
                .iter()
                .any(|candidate| shadow_key_matches(item, candidate))
        })
        .count();
    let action_status = if missing_in_agentos.is_empty() && missing_in_feishu.is_empty() {
        "shadow_match"
    } else {
        "shadow_mismatch"
    }
    .to_string();

    ActivityShadowReport {
        action_status,
        feishu_record_count: feishu_items.len(),
        event_signal_count: event_items.len(),
        matched_count,
        missing_in_agentos,
        missing_in_feishu,
    }
}

fn shadow_key_matches(left: &ShadowItem, right: &ShadowItem) -> bool {
    left.signal_date == right.signal_date && left.normalized_title == right.normalized_title
}

fn sort_and_dedupe_shadow_items(items: &mut Vec<ShadowItem>) {
    items.sort_by(|left, right| {
        left.signal_date
            .cmp(&right.signal_date)
            .then_with(|| left.normalized_title.cmp(&right.normalized_title))
    });
    items.dedup_by(|left, right| shadow_key_matches(left, right));
}

fn normalize_shadow_title(value: &str) -> String {
    value
        .chars()
        .filter_map(|ch| {
            if ch.is_whitespace()
                || matches!(
                    ch,
                    '，' | ','
                        | '。'
                        | '.'
                        | '、'
                        | '：'
                        | ':'
                        | '；'
                        | ';'
                        | '！'
                        | '!'
                        | '？'
                        | '?'
                        | '（'
                        | ')'
                        | '）'
                        | '('
                        | '【'
                        | '】'
                        | '['
                        | ']'
                        | '「'
                        | '」'
                        | '《'
                        | '》'
                        | '-'
                        | '_'
                        | '—'
                )
            {
                None
            } else {
                Some(ch.to_lowercase().to_string())
            }
        })
        .collect::<String>()
}

impl ActivityShadowReport {
    fn not_configured() -> Self {
        Self {
            action_status: "shadow_source_not_configured".to_string(),
            feishu_record_count: 0,
            event_signal_count: 0,
            matched_count: 0,
            missing_in_agentos: Vec::new(),
            missing_in_feishu: Vec::new(),
        }
    }
}

impl ShadowItem {
    fn new(title: String, signal_date: String) -> Self {
        let normalized_title = normalize_shadow_title(&title);
        Self {
            title,
            normalized_title,
            signal_date,
        }
    }
}

impl EventSignalIngestCandidate {
    fn from_row(row: sqlx::postgres::PgRow) -> Result<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            signal_type: row.try_get("signal_type")?,
            title: row.try_get("title")?,
            summary: row.try_get("summary")?,
            signal_date: row.try_get("signal_date")?,
            chat_id: row.try_get("chat_id")?,
            source_message_ids: row.try_get("source_message_ids")?,
            owner_name: row.try_get("owner_name")?,
            priority: row.try_get("priority")?,
            related_member_names: row.try_get("related_member_names")?,
        })
    }

    fn to_activity_payload(&self) -> ActivityPayload {
        ActivityPayload {
            actor_agent: ACTOR_AGENT.to_string(),
            operation: "signal-ingest".to_string(),
            record_id: String::new(),
            source_record_id: String::new(),
            date: String::new(),
            table_role: String::new(),
            status: String::new(),
            gap_summary: String::new(),
            handoff_type: String::new(),
            target_agent: String::new(),
            brief_summary: self.summary.clone(),
            event_signal_id: self.id.to_string(),
            mutation_id: String::new(),
            signal_type: self.signal_type.clone(),
            activity_title: self.title.clone(),
            signal_date: self.signal_date.clone(),
            chat_id: self.chat_id.clone(),
            source_message_ids: self
                .source_message_ids
                .iter()
                .map(Uuid::to_string)
                .collect(),
            owner_name: self.owner_name.clone(),
            priority: self.priority.clone(),
            location: String::new(),
            related_member_names: self.related_member_names.clone(),
        }
    }
}

fn activity_feishu_base_config(cli: &Cli) -> Result<ActivityFeishuBaseConfig> {
    let base_token = cli
        .xiaoman_activity_feishu_base_token
        .clone()
        .context("QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN is required")?;
    let allowed_tokens = cli.xiaoman_activity_allowed_feishu_base_tokens();
    if allowed_tokens.is_empty() || !allowed_tokens.iter().any(|token| token == &base_token) {
        bail!("Xiaoman activity Feishu Base token must be explicitly allowlisted");
    }
    Ok(ActivityFeishuBaseConfig {
        base_token,
        plan_table_id: cli
            .xiaoman_activity_feishu_plan_table_id
            .clone()
            .context("QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_PLAN_TABLE_ID is required")?,
        occurrence_table_id: cli
            .xiaoman_activity_feishu_occurrence_table_id
            .clone()
            .context("QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_OCCURRENCE_TABLE_ID is required")?,
        profile_env_path: cli.xiaoman_activity_feishu_profile_env_path.clone(),
    })
}

impl ActivityFeishuBaseConfig {
    fn table_id_for_role(&self, table_role: &str) -> Result<&str> {
        match table_role {
            "activity_plan" => Ok(&self.plan_table_id),
            "activity_occurrence" => Ok(&self.occurrence_table_id),
            _ => bail!("table_role is not allowed"),
        }
    }
}

impl ActivityRecord {
    fn from_feishu_value(value: &Value, table_role: &str) -> Result<Self> {
        let mut enriched = value.clone();
        if let Some(map) = enriched.as_object_mut() {
            map.entry("table_role".to_string())
                .or_insert_with(|| Value::String(table_role.to_string()));
        }
        let mut record = Self::from_fixture_value(&enriched)?;
        record.table_role = table_role.to_string();
        Ok(record)
    }

    fn from_fixture_value(value: &Value) -> Result<Self> {
        let fields = value.get("fields").unwrap_or(value);
        let record_id = string_at(value, &["record_id", "id"])
            .context("activity fixture record missing record_id")?;
        let table_role = string_at(value, &["table_role"])
            .or_else(|| field_string(fields, &["table_role"]))
            .context("activity fixture record missing table_role")?;
        if !TABLE_ROLES.contains(&table_role.as_str()) {
            bail!("fixture table_role is not allowed");
        }
        let title = field_string(
            fields,
            &[
                "活动名称",
                "活动标题",
                "活动信息",
                "活动内容",
                "标题",
                "name",
                "title",
            ],
        )
        .unwrap_or_else(|| "未命名活动".to_string());

        Ok(Self {
            record_id,
            table_role,
            title,
            activity_date: field_string(
                fields,
                &[
                    "活动日期",
                    "日期",
                    "计划日期",
                    "发生日期",
                    "date",
                    "activity_date",
                ],
            ),
            start_time: field_string(
                fields,
                &[
                    "开始时间",
                    "活动时间",
                    "计划时间",
                    "活动计划时间",
                    "start_time",
                    "startTime",
                ],
            ),
            end_time: field_string(fields, &["结束时间", "end_time", "endTime"]),
            location: field_string(fields, &["地点", "活动地点", "location"]),
            status: field_string(fields, &["小满运营状态", "活动状态", "状态", "status"]),
            promotion_status: field_string(fields, &["宣发判断", "宣发状态", "promotion_status"]),
            owner_name: field_string(fields, &["负责人", "负责同学", "owner", "owner_name"]),
            initiator_name: field_string(fields, &["发起人", "组织者", "initiator"]),
            material_summary: field_string(
                fields,
                &[
                    "素材照片",
                    "活动照片",
                    "素材",
                    "素材情况",
                    "material_summary",
                ],
            ),
            gap_summary: field_string(fields, &["补录缺口", "缺口", "gap_summary"]),
            notes: field_string(fields, &["小满备注", "备注", "notes"]),
            updated_at: field_string(fields, &["更新时间", "updated_at"]),
        })
    }

    fn matches_date(&self, date: &str) -> bool {
        self.activity_date.as_deref() == Some(date)
            || self
                .start_time
                .as_deref()
                .map(|item| item.starts_with(date))
                .unwrap_or(false)
    }

    fn view(&self) -> ActivityRecordView {
        ActivityRecordView {
            table_role: self.table_role.clone(),
            record_ref: self.record_ref(),
            title: self.title.clone(),
            activity_date: self.activity_date.clone(),
            start_time: self.start_time.clone(),
            end_time: self.end_time.clone(),
            location: self.location.clone(),
            status: self.status.clone(),
            promotion_status: self.promotion_status.clone(),
            owner_name: self.owner_name.clone(),
            initiator_name: self.initiator_name.clone(),
            material_summary: self.material_summary.clone(),
            gap_summary: self.gap_summary.clone(),
            notes: self.notes.clone(),
            updated_at: self.updated_at.clone(),
        }
    }

    fn record_ref(&self) -> String {
        let digest = Sha256::digest(format!("{}:{}", self.table_role, self.record_id));
        let mut suffix = String::new();
        for byte in &digest[..6] {
            suffix.push_str(&format!("{byte:02x}"));
        }
        format!("{}:{suffix}", self.table_role)
    }

    fn safe_summary(&self) -> String {
        let date = self
            .activity_date
            .clone()
            .or_else(|| self.start_time.clone())
            .unwrap_or_else(|| "日期未定".to_string());
        let location = self
            .location
            .clone()
            .unwrap_or_else(|| "地点未定".to_string());
        let status = self
            .status
            .clone()
            .unwrap_or_else(|| "状态未定".to_string());
        format!("{}｜{}｜{}｜{}", self.title, date, location, status)
    }
}

fn field_string(fields: &Value, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        fields
            .get(*name)
            .and_then(|value| field_cell_as_string(name, value))
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
    })
}

fn string_at(value: &Value, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        value
            .get(*name)
            .and_then(feishu_cell_as_string)
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
    })
}

fn field_cell_as_string(name: &str, value: &Value) -> Option<String> {
    if is_datetime_field(name) {
        if let Some(text) = feishu_timestamp_as_shanghai_datetime(value) {
            return Some(text);
        }
    }
    feishu_cell_as_string(value)
}

fn is_datetime_field(name: &str) -> bool {
    matches!(
        name,
        "开始时间"
            | "活动时间"
            | "计划时间"
            | "活动计划时间"
            | "结束时间"
            | "更新时间"
            | "start_time"
            | "startTime"
            | "end_time"
            | "endTime"
            | "updated_at"
    )
}

fn feishu_timestamp_as_shanghai_datetime(value: &Value) -> Option<String> {
    let millis = match value {
        Value::Number(number) => number.as_i64()?,
        Value::String(text) => text.parse::<i64>().ok()?,
        _ => return None,
    };
    if millis.abs() < 100_000_000_000 {
        return None;
    }
    let timezone = FixedOffset::east_opt(8 * 3600)?;
    let timestamp = timezone.timestamp_millis_opt(millis).single()?;
    Some(timestamp.format("%Y-%m-%d %H:%M").to_string())
}

fn feishu_cell_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Array(items) => {
            let values: Vec<String> = items.iter().filter_map(feishu_cell_as_string).collect();
            if values.is_empty() {
                None
            } else {
                Some(values.join(", "))
            }
        }
        Value::Object(map) => {
            for key in ["text", "name", "value", "url", "link"] {
                if let Some(text) = map.get(key).and_then(feishu_cell_as_string) {
                    return Some(text);
                }
            }
            None
        }
        Value::Null => None,
    }
}

impl FeishuClient {
    fn from_profile_env(path: &str) -> Result<Self> {
        let profile = read_feishu_profile_env(path)?;
        let tls_config = Arc::new(tls_config());
        let response = post_json(
            FEISHU_AUTH_API,
            None,
            &json!({"app_id": profile.app_id, "app_secret": profile.app_secret}),
            tls_config.clone(),
        )
        .context("request Feishu tenant access token")?;
        let parsed: Value =
            serde_json::from_str(&response).context("parse Feishu token response")?;
        if parsed.get("code").and_then(Value::as_i64) != Some(0) {
            bail!("Feishu token response error: {parsed}");
        }
        let tenant_token = parsed
            .get("tenant_access_token")
            .and_then(Value::as_str)
            .context("Feishu response missing tenant_access_token")?
            .to_string();
        Ok(Self {
            tenant_token,
            tls_config,
        })
    }

    fn list_records(
        &self,
        base_token: &str,
        table_id: &str,
        page_size: usize,
    ) -> Result<Vec<Value>> {
        let mut page_token = String::new();
        let mut out = Vec::new();
        loop {
            let mut url = format!(
                "{FEISHU_BASE_API}/{base_token}/tables/{table_id}/records?page_size={}",
                page_size.min(500)
            );
            if !page_token.is_empty() {
                url.push_str("&page_token=");
                url.push_str(&page_token);
            }
            let parsed = self.request_json("GET", &url, None)?;
            let data = parsed.get("data").cloned().unwrap_or_else(|| json!({}));
            if let Some(items) = data.get("items").and_then(Value::as_array) {
                out.extend(items.iter().cloned());
            }
            if data.get("has_more").and_then(Value::as_bool) != Some(true) {
                break;
            }
            page_token = data
                .get("page_token")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if page_token.is_empty() {
                break;
            }
        }
        Ok(out)
    }

    fn request_json(&self, method: &str, url: &str, body: Option<&Value>) -> Result<Value> {
        let response = request_json(
            method,
            url,
            Some(&self.tenant_token),
            body,
            self.tls_config.clone(),
        )
        .with_context(|| format!("call Feishu API {method} {}", sanitize_feishu_url(url)))?;
        let parsed: Value = serde_json::from_str(&response).context("parse Feishu API response")?;
        if parsed.get("code").and_then(Value::as_i64) != Some(0) {
            bail!("Feishu API response error: {parsed}");
        }
        Ok(parsed)
    }
}

fn read_feishu_profile_env(path: &str) -> Result<FeishuProfileEnv> {
    let text = fs::read_to_string(path).with_context(|| format!("read {path}"))?;
    let mut app_id = None;
    let mut app_secret = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let value = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        match key.trim() {
            "FEISHU_APP_ID" | "LARK_APP_ID" | "APP_ID" => app_id = Some(value),
            "FEISHU_APP_SECRET" | "LARK_APP_SECRET" | "APP_SECRET" => app_secret = Some(value),
            _ => {}
        }
    }
    Ok(FeishuProfileEnv {
        app_id: app_id.context("missing FEISHU_APP_ID in profile env")?,
        app_secret: app_secret.context("missing FEISHU_APP_SECRET in profile env")?,
    })
}

fn request_json(
    method: &str,
    endpoint: &str,
    bearer_token: Option<&str>,
    body: Option<&Value>,
    tls_config: Arc<ClientConfig>,
) -> Result<String> {
    let request_body = body
        .map(serde_json::to_string)
        .transpose()
        .context("serialize Feishu request")?
        .unwrap_or_default();
    let mut headers = vec![
        "Content-Type: application/json".to_string(),
        "Accept: application/json".to_string(),
        "Accept-Encoding: identity".to_string(),
    ];
    if let Some(token) = bearer_token {
        headers.push(format!("Authorization: Bearer {token}"));
    }
    post_raw(method, endpoint, &headers, &request_body, tls_config)
}

fn post_json(
    endpoint: &str,
    bearer_token: Option<&str>,
    request: &Value,
    tls_config: Arc<ClientConfig>,
) -> Result<String> {
    request_json("POST", endpoint, bearer_token, Some(request), tls_config)
}

fn post_raw(
    method: &str,
    endpoint: &str,
    headers: &[String],
    body: &str,
    tls_config: Arc<ClientConfig>,
) -> Result<String> {
    let url = url::Url::parse(endpoint).context("parse Feishu API URL")?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("Feishu API URL is missing host"))?;
    let port = url
        .port_or_known_default()
        .unwrap_or(if url.scheme() == "https" { 443 } else { 80 });
    let mut path = url.path().to_string();
    if path.is_empty() {
        path = "/".to_string();
    }
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for header in headers {
        request.push_str(header);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    request.push_str(body);

    if url.scheme() != "https" {
        bail!("Feishu API URL must use https");
    }
    let server_name = ServerName::try_from(host.to_string()).context("validate Feishu API host")?;
    let mut connection =
        ClientConnection::new(tls_config, server_name).context("create TLS connection")?;
    let mut socket = TcpStream::connect((host, port)).context("connect Feishu API")?;
    socket
        .set_read_timeout(Some(StdDuration::from_secs(30)))
        .context("set Feishu read timeout")?;
    socket
        .set_write_timeout(Some(StdDuration::from_secs(30)))
        .context("set Feishu write timeout")?;
    let mut stream = Stream::new(&mut connection, &mut socket);
    stream
        .write_all(request.as_bytes())
        .context("write Feishu request")?;
    stream.flush().context("flush Feishu request")?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .context("read Feishu response")?;
    parse_http_response(&response)
}

fn sanitize_feishu_url(endpoint: &str) -> String {
    match url::Url::parse(endpoint) {
        Ok(parsed) => parsed.path_segments().map_or_else(
            || {
                format!(
                    "{}://{}",
                    parsed.scheme(),
                    parsed.host_str().unwrap_or("unknown")
                )
            },
            |segments| {
                let mut redact_next = false;
                let mut redacted_segments = Vec::new();
                for segment in segments {
                    if redact_next {
                        redacted_segments.push("redacted".to_string());
                        redact_next = false;
                        continue;
                    }
                    redacted_segments.push(segment.to_string());
                    if segment == "apps" || segment == "tables" {
                        redact_next = true;
                    }
                }
                format!(
                    "{}://{}/{}",
                    parsed.scheme(),
                    parsed.host_str().unwrap_or("unknown"),
                    redacted_segments.join("/")
                )
            },
        ),
        Err(_) => "invalid-feishu-url".to_string(),
    }
}

fn parse_http_response(response: &[u8]) -> Result<String> {
    let header_end = find_header_end(response).ok_or_else(|| anyhow!("invalid HTTP response"))?;
    let head = String::from_utf8_lossy(&response[..header_end]);
    let body = &response[header_end + 4..];
    let status_line = head.lines().next().unwrap_or_default();
    if !status_line.contains(" 200 ") {
        bail!(
            "HTTP request failed: {status_line}; body={}",
            String::from_utf8_lossy(body)
        );
    }
    let is_chunked = head
        .lines()
        .any(|line| line.eq_ignore_ascii_case("transfer-encoding: chunked"));
    if is_chunked {
        decode_chunked_body(body)
    } else {
        String::from_utf8(body.to_vec()).context("decode HTTP response utf8")
    }
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn decode_chunked_body(body: &[u8]) -> Result<String> {
    let mut index = 0usize;
    let mut decoded = Vec::new();
    while index < body.len() {
        while body.get(index..index + 2) == Some(b"\r\n") {
            index += 2;
        }
        if index >= body.len() {
            break;
        }
        let line_end = find_crlf(body, index).ok_or_else(|| anyhow!("invalid chunked body"))?;
        let size_text = std::str::from_utf8(&body[index..line_end])
            .context("decode chunk size")?
            .split(';')
            .next()
            .unwrap_or_default()
            .trim();
        if size_text.is_empty() {
            break;
        }
        let size = usize::from_str_radix(size_text, 16).context("parse chunk size")?;
        index = line_end + 2;
        if size == 0 {
            break;
        }
        if index + size > body.len() {
            bail!("chunk exceeds body length");
        }
        decoded.extend_from_slice(&body[index..index + size]);
        index += size;
        if body.get(index..index + 2) != Some(b"\r\n") {
            bail!("chunk missing trailing CRLF");
        }
        index += 2;
    }
    String::from_utf8(decoded).context("decode chunked response utf8")
}

fn find_crlf(bytes: &[u8], start: usize) -> Option<usize> {
    bytes
        .get(start..)?
        .windows(2)
        .position(|pair| pair == b"\r\n")
        .map(|offset| start + offset)
}

fn tls_config() -> ClientConfig {
    let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth()
}

fn validate(operation: &str, payload: &ActivityPayload) -> Result<()> {
    if operation != payload.operation {
        bail!("operation mismatch between CLI and payload");
    }
    if !READ_ONLY_OPERATIONS.contains(&operation) && !WRITE_OPERATIONS.contains(&operation) {
        bail!("operation is not allowed");
    }
    if payload.actor_agent != ACTOR_AGENT {
        bail!("actor_agent must be xiaoman");
    }
    if !payload.table_role.is_empty() && !TABLE_ROLES.contains(&payload.table_role.as_str()) {
        bail!("table_role is not allowed");
    }
    match operation {
        "record-get" => require_fields(&[
            ("record_id", &payload.record_id),
            ("table_role", &payload.table_role),
        ])?,
        "list-by-date" => {
            require_fields(&[("date", &payload.date), ("table_role", &payload.table_role)])?
        }
        "status-update" => {
            require_fields(&[
                ("event_signal_id", &payload.event_signal_id),
                ("mutation_id", &payload.mutation_id),
                ("status", &payload.status),
            ])?;
            validate_event_signal_mutation_payload(payload)?;
        }
        "gap-update" => {
            require_fields(&[
                ("event_signal_id", &payload.event_signal_id),
                ("mutation_id", &payload.mutation_id),
                ("gap_summary", &payload.gap_summary),
            ])?;
            validate_event_signal_mutation_payload(payload)?;
        }
        "handoff-create" => {
            require_fields(&[
                ("source_record_id", &payload.source_record_id),
                ("handoff_type", &payload.handoff_type),
                ("target_agent", &payload.target_agent),
                ("brief_summary", &payload.brief_summary),
            ])?;
            if !HANDOFF_TYPES.contains(&payload.handoff_type.as_str()) {
                bail!("handoff_type is not allowed");
            }
            if !HANDOFF_TARGETS.contains(&payload.target_agent.as_str()) {
                bail!("target_agent is not allowed");
            }
        }
        "signal-ingest" => {
            require_fields(&[("event_signal_id", &payload.event_signal_id)])?;
        }
        "material-summary" => require_fields(&[
            ("record_id", &payload.record_id),
            ("table_role", &payload.table_role),
        ])?,
        "shadow-validate" => require_fields(&[("date", &payload.date)])?,
        _ => unreachable!("operation allowlist checked above"),
    }
    Ok(())
}

fn require_fields(fields: &[(&str, &str)]) -> Result<()> {
    for (name, value) in fields {
        if value.trim().is_empty() {
            bail!("{name} is required");
        }
    }
    Ok(())
}

fn validate_event_signal_mutation_payload(payload: &ActivityPayload) -> Result<()> {
    Uuid::parse_str(payload.event_signal_id.trim()).context("event_signal_id must be a UUID")?;
    Uuid::parse_str(payload.mutation_id.trim()).context("mutation_id must be a UUID")?;
    if !payload.record_id.trim().is_empty() || !payload.table_role.trim().is_empty() {
        bail!("event-signal mutations must not use Feishu record_id or table_role");
    }
    match payload.operation.as_str() {
        "status-update" => {
            if !payload.gap_summary.trim().is_empty() {
                bail!("status-update must not include gap_summary");
            }
            if !EVENT_SIGNAL_STATUSES.contains(&payload.status.trim()) {
                bail!("status is not allowed for event-signal mutation");
            }
        }
        "gap-update" => {
            if !payload.status.trim().is_empty() {
                bail!("gap-update must not include status");
            }
            normalize_gap_summary(&payload.gap_summary)?;
        }
        _ => bail!("operation is not an event-signal mutation"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use serde_json::json;

    fn payload(value: serde_json::Value) -> ActivityPayload {
        serde_json::from_value(value).expect("test payload should deserialize")
    }

    fn feishu_config() -> ActivityFeishuBaseConfig {
        ActivityFeishuBaseConfig {
            base_token: "app_test".to_string(),
            plan_table_id: "tbl_plan".to_string(),
            occurrence_table_id: "tbl_occurrence".to_string(),
            profile_env_path: "/tmp/xiaoman.env".to_string(),
        }
    }

    fn runtime_with_fixture() -> ActivityRuntimeConfig {
        ActivityRuntimeConfig {
            fixture_path: Some(fixture_path()),
            feishu_base: None,
        }
    }

    fn runtime_without_source() -> ActivityRuntimeConfig {
        ActivityRuntimeConfig {
            fixture_path: None,
            feishu_base: None,
        }
    }

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/xiaoman_activity_records.json")
    }

    #[tokio::test]
    async fn record_get_reads_fixture_without_raw_record_id_in_view() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "record-get",
            "record_id": "rec_plan_20260628",
            "table_role": "activity_plan"
        }));

        validate("record-get", &payload).expect("record-get payload should be valid");
        let report = execute_with_config(
            &Cli::parse_from(["qintopia-message-sidecar", "check"]),
            "record-get".to_string(),
            payload,
            true,
            false,
            &runtime_with_fixture(),
        )
        .await
        .expect("fixture read should succeed");

        assert_eq!(report.action_status, "read_ok");
        assert_eq!(report.source, "fixture");
        assert_eq!(report.record_count, 1);
        assert_eq!(report.records[0].title, "周日共创晚餐");
        assert!(!serde_json::to_string(&report.records)
            .unwrap()
            .contains("rec_plan_20260628"));
    }

    #[tokio::test]
    async fn list_by_date_filters_fixture_records() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "list-by-date",
            "date": "2026-06-28",
            "table_role": "activity_plan"
        }));

        validate("list-by-date", &payload).expect("list payload should be valid");
        let report = execute_with_config(
            &Cli::parse_from(["qintopia-message-sidecar", "check"]),
            "list-by-date".to_string(),
            payload,
            true,
            false,
            &runtime_with_fixture(),
        )
        .await
        .expect("fixture list should succeed");

        assert_eq!(report.action_status, "read_ok");
        assert_eq!(report.record_count, 1);
        assert!(report.summaries[0].contains("周日共创晚餐"));
    }

    #[tokio::test]
    async fn material_summary_reads_fixture_material_fields() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "material-summary",
            "record_id": "rec_occurrence_20260628",
            "table_role": "activity_occurrence"
        }));

        validate("material-summary", &payload).expect("material-summary payload should be valid");
        let report = execute_with_config(
            &Cli::parse_from(["qintopia-message-sidecar", "check"]),
            "material-summary".to_string(),
            payload,
            true,
            false,
            &runtime_with_fixture(),
        )
        .await
        .expect("fixture material summary should succeed");

        assert_eq!(report.action_status, "read_ok");
        assert_eq!(
            report.records[0].material_summary.as_deref(),
            Some("现场照片 6 张，待筛选 2 张可用于复盘")
        );
    }

    #[tokio::test]
    async fn signal_ingest_creates_activity_request_preview() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "signal-ingest",
            "event_signal_id": "11111111-1111-4111-8111-111111111111",
            "signal_type": "活动/聚会",
            "activity_title": "周日共创晚餐",
            "signal_date": "2026-06-28",
            "chat_id": "fixture-community-group",
            "source_message_ids": ["22222222-2222-4222-8222-222222222222"],
            "owner_name": "小满",
            "priority": "高",
            "location": "秦托邦共享厨房",
            "gap_summary": "缺少海报主图和报名截止时间"
        }));

        validate("signal-ingest", &payload).expect("signal payload should be valid");
        let report = execute_with_config(
            &Cli::parse_from(["qintopia-message-sidecar", "check"]),
            "signal-ingest".to_string(),
            payload,
            false,
            true,
            &runtime_without_source(),
        )
        .await
        .expect("signal ingest dry-run should succeed");

        let work_item = report
            .operations_work_item
            .expect("signal ingest should create a work item preview");
        assert_eq!(report.action_status, "operations_dry_run_ok");
        assert_eq!(report.source, "agentos_event_signal");
        assert_eq!(work_item.capability_key, "xiaoman.create_activity_request");
        assert_eq!(work_item.work_item_type, "activity_promotion_request");
        assert_eq!(work_item.requester_agent, "default");
        assert_eq!(work_item.target_agent, "xiaoman");
        assert_eq!(
            work_item.idempotency_key,
            "xiaoman_activity_signal:11111111-1111-4111-8111-111111111111"
        );
        assert_eq!(work_item.current_status, "queued");
        assert!(!serde_json::to_string(&work_item)
            .unwrap()
            .contains("22222222-2222-4222-8222-222222222222"));
    }

    #[tokio::test]
    async fn signal_ingest_duplicate_uses_same_idempotency_key() {
        let first = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "signal-ingest",
            "event_signal_id": "11111111-1111-4111-8111-111111111111",
            "signal_type": "活动/聚会",
            "activity_title": "周日共创晚餐",
            "signal_date": "2026-06-28"
        }));
        let duplicate = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "signal-ingest",
            "event_signal_id": "11111111-1111-4111-8111-111111111111",
            "signal_type": "活动/聚会",
            "activity_title": "周日共创晚餐",
            "signal_date": "2026-06-28"
        }));

        let first_report = execute_with_config(
            &Cli::parse_from(["qintopia-message-sidecar", "check"]),
            "signal-ingest".to_string(),
            first,
            false,
            true,
            &runtime_without_source(),
        )
        .await
        .expect("first signal should preview");
        let duplicate_report = execute_with_config(
            &Cli::parse_from(["qintopia-message-sidecar", "check"]),
            "signal-ingest".to_string(),
            duplicate,
            false,
            true,
            &runtime_without_source(),
        )
        .await
        .expect("duplicate signal should preview");

        assert_eq!(
            first_report.operations_work_item.unwrap().idempotency_key,
            duplicate_report
                .operations_work_item
                .unwrap()
                .idempotency_key
        );
    }

    #[tokio::test]
    async fn signal_ingest_missing_fields_marks_review_needed() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "signal-ingest",
            "event_signal_id": "33333333-3333-4333-8333-333333333333",
            "signal_type": "活动/聚会",
            "activity_title": "社区共学",
            "signal_date": ""
        }));

        validate("signal-ingest", &payload)
            .expect("signal missing non-id fields should still validate");
        let report = execute_with_config(
            &Cli::parse_from(["qintopia-message-sidecar", "check"]),
            "signal-ingest".to_string(),
            payload,
            false,
            true,
            &runtime_without_source(),
        )
        .await
        .expect("missing fields should create review-needed preview");

        let work_item = report
            .operations_work_item
            .expect("review-needed signal should still produce a work item preview");
        assert_eq!(report.action_status, "review_needed");
        assert_eq!(report.validation_status, "review_needed");
        assert_eq!(work_item.capability_key, "xiaoman.create_activity_request");
        assert_eq!(work_item.current_status, "queued");
        assert!(report
            .limitations
            .iter()
            .any(|item| item.contains("signal_date")));
    }

    #[test]
    fn signal_worker_candidate_uses_signal_ingest_contract() {
        let candidate = EventSignalIngestCandidate {
            id: Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap(),
            signal_type: "活动/聚会".to_string(),
            title: "周末共创晚餐".to_string(),
            summary: "成员在群内讨论周末共创晚餐，需要小满跟进活动宣发。".to_string(),
            signal_date: "2026-07-05".to_string(),
            chat_id: "fixture-community-group".to_string(),
            source_message_ids: vec![
                Uuid::parse_str("55555555-5555-4555-8555-555555555555").unwrap()
            ],
            owner_name: "小满".to_string(),
            priority: "高".to_string(),
            related_member_names: vec!["阿城".to_string(), "小林".to_string()],
        };

        let payload = candidate.to_activity_payload();
        let missing_fields = signal_missing_fields(&payload);
        let request = signal_work_item_request(&payload, &missing_fields);
        let source_refs_json = serde_json::to_string(&request.source_refs).unwrap();

        assert!(missing_fields.is_empty());
        assert_eq!(payload.actor_agent, "xiaoman");
        assert_eq!(payload.operation, "signal-ingest");
        assert_eq!(request.capability_key, "xiaoman.create_activity_request");
        assert_eq!(request.source_event_signal_id, Some(candidate.id));
        assert_eq!(
            request.idempotency_key,
            "xiaoman_activity_signal:44444444-4444-4444-8444-444444444444"
        );
        assert!(source_refs_json.contains("event_signal_message:"));
        assert!(!source_refs_json.contains("55555555-5555-4555-8555-555555555555"));
    }

    #[test]
    fn feishu_record_mapping_uses_table_role_and_hides_raw_id_in_view() {
        let value = json!({
            "record_id": "rec_feishu_plan_1",
            "fields": {
                "活动名称": [{"text": "周一共学"}],
                "活动日期": "2026-06-29",
                "地点": "秦托邦一楼",
                "小满运营状态": "待补素材"
            }
        });

        let record = ActivityRecord::from_feishu_value(&value, "activity_plan")
            .expect("Feishu record should map");
        let view = record.view();

        assert_eq!(view.table_role, "activity_plan");
        assert_eq!(view.title, "周一共学");
        assert_ne!(view.record_ref, "rec_feishu_plan_1");
        assert!(!serde_json::to_string(&view)
            .unwrap()
            .contains("rec_feishu_plan_1"));
    }

    #[test]
    fn feishu_record_mapping_normalizes_timestamp_activity_time() {
        let value = json!({
            "record_id": "rec_feishu_occurrence_1",
            "fields": {
                "活动信息": "夏至晚餐",
                "活动计划时间": 1782630000000_i64,
                "活动照片": [{"name": "dinner.jpg"}],
                "发起人": "小满"
            }
        });

        let record = ActivityRecord::from_feishu_value(&value, "activity_occurrence")
            .expect("Feishu occurrence should map");

        assert_eq!(record.title, "夏至晚餐");
        assert_eq!(record.start_time.as_deref(), Some("2026-06-28 15:00"));
        assert!(record.matches_date("2026-06-28"));
        assert_eq!(record.material_summary.as_deref(), Some("dinner.jpg"));
    }

    #[test]
    fn feishu_timestamp_normalization_ignores_plain_numbers() {
        assert_eq!(feishu_timestamp_as_shanghai_datetime(&json!(42)), None);
        assert_eq!(
            feishu_timestamp_as_shanghai_datetime(&json!("1782630000000")),
            Some("2026-06-28 15:00".to_string())
        );
    }

    #[test]
    fn shadow_validation_matches_feishu_records_and_event_signals() {
        let feishu = vec![
            ActivityRecord {
                record_id: "rec_plan_shadow_1".to_string(),
                table_role: "activity_plan".to_string(),
                title: "周日共创晚餐".to_string(),
                activity_date: Some("2026-06-28".to_string()),
                start_time: None,
                end_time: None,
                location: None,
                status: None,
                promotion_status: None,
                owner_name: None,
                initiator_name: None,
                material_summary: None,
                gap_summary: None,
                notes: None,
                updated_at: None,
            },
            ActivityRecord {
                record_id: "rec_occurrence_shadow_1".to_string(),
                table_role: "activity_occurrence".to_string(),
                title: "社区共学".to_string(),
                activity_date: None,
                start_time: Some("2026-06-28 15:00".to_string()),
                end_time: None,
                location: None,
                status: None,
                promotion_status: None,
                owner_name: None,
                initiator_name: None,
                material_summary: None,
                gap_summary: None,
                notes: None,
                updated_at: None,
            },
        ];
        let signals = vec![
            EventSignalActivity {
                title: "周日共创晚餐".to_string(),
                signal_date: "2026-06-28".to_string(),
            },
            EventSignalActivity {
                title: "社区共学".to_string(),
                signal_date: "2026-06-28".to_string(),
            },
        ];

        let shadow = compare_shadow_items(
            &shadow_items_from_records(&feishu, "2026-06-28"),
            &shadow_items_from_event_signals(&signals),
        );

        assert_eq!(shadow.action_status, "shadow_match");
        assert_eq!(shadow.feishu_record_count, 2);
        assert_eq!(shadow.event_signal_count, 2);
        assert_eq!(shadow.matched_count, 2);
        assert!(shadow.missing_in_agentos.is_empty());
        assert!(shadow.missing_in_feishu.is_empty());
    }

    #[test]
    fn shadow_validation_reports_missing_sides() {
        let feishu = vec![ShadowItem::new(
            "Feishu 只有的活动".to_string(),
            "2026-06-28".to_string(),
        )];
        let agentos = vec![ShadowItem::new(
            "AgentOS 只有的活动".to_string(),
            "2026-06-28".to_string(),
        )];

        let shadow = compare_shadow_items(&feishu, &agentos);

        assert_eq!(shadow.action_status, "shadow_mismatch");
        assert_eq!(shadow.matched_count, 0);
        assert_eq!(shadow.missing_in_agentos[0].title, "Feishu 只有的活动");
        assert_eq!(shadow.missing_in_feishu[0].title, "AgentOS 只有的活动");
    }

    #[test]
    fn shadow_title_normalization_handles_case_space_and_punctuation() {
        let feishu = vec![ShadowItem::new(
            " AgentOS：周日-共创 晚餐！".to_string(),
            "2026-06-28".to_string(),
        )];
        let agentos = vec![ShadowItem::new(
            "agentos周日共创晚餐".to_string(),
            "2026-06-28".to_string(),
        )];

        let shadow = compare_shadow_items(&feishu, &agentos);

        assert_eq!(shadow.action_status, "shadow_match");
        assert_eq!(shadow.matched_count, 1);
    }

    #[test]
    fn shadow_report_does_not_expose_raw_feishu_or_message_ids() {
        let feishu = vec![ActivityRecord {
            record_id: "rec_sensitive_shadow".to_string(),
            table_role: "activity_plan".to_string(),
            title: "安全输出活动".to_string(),
            activity_date: Some("2026-06-28".to_string()),
            start_time: None,
            end_time: None,
            location: None,
            status: None,
            promotion_status: None,
            owner_name: None,
            initiator_name: None,
            material_summary: None,
            gap_summary: None,
            notes: None,
            updated_at: None,
        }];
        let signals = vec![EventSignalActivity {
            title: "安全输出活动".to_string(),
            signal_date: "2026-06-28".to_string(),
        }];
        let shadow = compare_shadow_items(
            &shadow_items_from_records(&feishu, "2026-06-28"),
            &shadow_items_from_event_signals(&signals),
        );
        let raw = serde_json::to_string(&shadow.missing_in_agentos).unwrap()
            + &serde_json::to_string(&shadow.missing_in_feishu).unwrap();

        assert!(!raw.contains("rec_sensitive_shadow"));
        assert!(!raw.contains("tbl_"));
        assert!(!raw.contains("msg_"));
        assert!(!raw.contains("app_token"));
    }

    #[test]
    fn feishu_config_maps_only_allowed_table_roles() {
        let config = feishu_config();

        assert_eq!(
            config.table_id_for_role("activity_plan").unwrap(),
            "tbl_plan"
        );
        assert_eq!(
            config.table_id_for_role("activity_occurrence").unwrap(),
            "tbl_occurrence"
        );
        assert!(config.table_id_for_role("arbitrary").is_err());
    }

    #[tokio::test]
    async fn read_apply_without_source_reports_configuration_gap() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "record-get",
            "record_id": "rec_activity_1",
            "table_role": "activity_occurrence"
        }));

        validate("record-get", &payload).expect("record-get payload should be valid");
        let report = execute_with_config(
            &Cli::parse_from(["qintopia-message-sidecar", "check"]),
            "record-get".to_string(),
            payload,
            true,
            false,
            &runtime_without_source(),
        )
        .await
        .expect("validation-only read should succeed");

        assert_eq!(report.action_status, "read_source_not_configured");
        assert_eq!(report.record_count, 0);
    }

    #[tokio::test]
    async fn status_update_dry_run_targets_agentos_event_signal() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "status-update",
            "event_signal_id": "66666666-6666-4666-8666-666666666666",
            "mutation_id": "77777777-7777-4777-8777-777777777777",
            "status": "处理中"
        }));

        validate("status-update", &payload).expect("status-update payload should be valid");
        let report = execute_with_config(
            &Cli::parse_from(["qintopia-message-sidecar", "check"]),
            "status-update".to_string(),
            payload,
            false,
            true,
            &runtime_without_source(),
        )
        .await
        .expect("status update should preview without database access");

        assert!(report.success);
        assert!(!report.apply_requested);
        assert!(report.dry_run);
        assert_eq!(report.source, "agentos_event_signals");
        assert_eq!(report.action_status, "event_signal_status_preview");
        assert_eq!(report.mutation_applied, Some(false));
        assert!(!report.safe_for_chat);
    }

    #[tokio::test]
    async fn gap_update_dry_run_normalizes_without_exposing_gap_text() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "gap-update",
            "event_signal_id": "88888888-8888-4888-8888-888888888888",
            "mutation_id": "99999999-9999-4999-8999-999999999999",
            "gap_summary": "缺少   报名截止时间"
        }));

        validate("gap-update", &payload).expect("gap-update payload should be valid");
        let report = execute_with_config(
            &Cli::parse_from(["qintopia-message-sidecar", "check"]),
            "gap-update".to_string(),
            payload,
            false,
            true,
            &runtime_without_source(),
        )
        .await
        .expect("gap update should preview without database access");

        assert_eq!(report.action_status, "event_signal_gap_preview");
        assert_eq!(report.mutation_applied, Some(false));
        assert!(!serde_json::to_string(&report)
            .expect("report should serialize")
            .contains("报名截止时间"));
    }

    #[test]
    fn event_signal_mutations_reject_feishu_record_identifiers() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "status-update",
            "event_signal_id": "66666666-6666-4666-8666-666666666666",
            "mutation_id": "77777777-7777-4777-8777-777777777777",
            "record_id": "rec_feishu",
            "table_role": "activity_plan",
            "status": "处理中"
        }));

        let error = validate("status-update", &payload)
            .expect_err("Feishu identifiers must not be accepted for AgentOS writes");
        assert!(error
            .to_string()
            .contains("must not use Feishu record_id or table_role"));
    }

    #[test]
    fn event_signal_mutations_require_uuid_ids_and_allowlisted_status() {
        let invalid_id = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "status-update",
            "event_signal_id": "not-a-uuid",
            "mutation_id": "77777777-7777-4777-8777-777777777777",
            "status": "处理中"
        }));
        assert!(validate("status-update", &invalid_id)
            .expect_err("invalid event id must fail")
            .to_string()
            .contains("event_signal_id must be a UUID"));

        let invalid_status = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "status-update",
            "event_signal_id": "66666666-6666-4666-8666-666666666666",
            "mutation_id": "77777777-7777-4777-8777-777777777777",
            "status": "待人工确认"
        }));
        assert!(validate("status-update", &invalid_status)
            .expect_err("non-AgentOS status must fail")
            .to_string()
            .contains("status is not allowed"));
    }

    #[test]
    fn event_signal_mutations_accept_exactly_one_mutable_field() {
        let status_with_gap = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "status-update",
            "event_signal_id": "66666666-6666-4666-8666-666666666666",
            "mutation_id": "77777777-7777-4777-8777-777777777777",
            "status": "处理中",
            "gap_summary": "缺少报名截止时间"
        }));
        assert!(validate("status-update", &status_with_gap)
            .expect_err("status update must reject a second mutable field")
            .to_string()
            .contains("must not include gap_summary"));

        let gap_with_status = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "gap-update",
            "event_signal_id": "66666666-6666-4666-8666-666666666666",
            "mutation_id": "77777777-7777-4777-8777-777777777777",
            "status": "处理中",
            "gap_summary": "缺少报名截止时间"
        }));
        assert!(validate("gap-update", &gap_with_status)
            .expect_err("gap update must reject a second mutable field")
            .to_string()
            .contains("must not include status"));
    }

    #[test]
    fn event_signal_status_transition_rules_keep_terminal_states_closed() {
        assert!(validate_status_transition("待处理", "处理中").is_ok());
        assert!(validate_status_transition("处理中", "已完成").is_ok());
        assert!(validate_status_transition("处理中", "待处理").is_ok());
        assert!(validate_status_transition("已完成", "处理中").is_err());
        assert!(validate_status_transition("已关闭", "待处理").is_err());
        assert!(validate_status_transition("已完成", "已完成").is_ok());
        assert!(ELIGIBLE_SIGNAL_WORKER_STATUSES.contains(&"待处理"));
        assert!(ELIGIBLE_SIGNAL_WORKER_STATUSES.contains(&"处理中"));
        assert!(!ELIGIBLE_SIGNAL_WORKER_STATUSES.contains(&"已完成"));
        assert!(!ELIGIBLE_SIGNAL_WORKER_STATUSES.contains(&"已关闭"));
    }

    #[test]
    fn gap_summary_rejects_sensitive_and_overlong_content() {
        assert_eq!(
            normalize_gap_summary("缺少   报名截止时间").expect("summary should normalize"),
            "缺少 报名截止时间"
        );
        assert!(normalize_gap_summary("请联系 13800138000").is_err());
        assert!(normalize_gap_summary("https://example.com/private").is_err());
        assert!(normalize_gap_summary(&"长".repeat(501)).is_err());
    }

    #[test]
    fn event_signal_mutation_idempotency_key_is_stable() {
        let event_signal_id = Uuid::parse_str("66666666-6666-4666-8666-666666666666").unwrap();
        let mutation_id = Uuid::parse_str("77777777-7777-4777-8777-777777777777").unwrap();

        assert_eq!(
            event_signal_mutation_idempotency_key(event_signal_id, mutation_id),
            "xiaoman_event_signal:66666666-6666-4666-8666-666666666666:77777777-7777-4777-8777-777777777777"
        );
    }

    #[tokio::test]
    async fn handoff_create_dry_run_maps_visual_request_to_operations_work_item() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "handoff-create",
            "source_record_id": "rec_activity_1",
            "table_role": "activity_occurrence",
            "handoff_type": "visual_asset_request",
            "target_agent": "huabaosi",
            "brief_summary": "做一组活动前宣海报 brief"
        }));

        validate("handoff-create", &payload).expect("handoff payload should be valid");
        let report = execute_with_config(
            &Cli::parse_from(["qintopia-message-sidecar", "check"]),
            "handoff-create".to_string(),
            payload,
            false,
            true,
            &runtime_without_source(),
        )
        .await
        .expect("handoff dry-run should create operations preview");

        let work_item = report
            .operations_work_item
            .expect("operations work item report should exist");
        assert_eq!(report.action_status, "operations_dry_run_ok");
        assert_eq!(work_item.capability_key, "huabaosi.create_visual_asset");
        assert_eq!(work_item.work_item_type, "visual_asset_request");
        assert_eq!(work_item.requester_agent, "xiaoman");
        assert_eq!(work_item.target_agent, "huabaosi");
        assert!(work_item.idempotency_key.starts_with("ops:"));
        assert!(serde_json::to_string(&work_item)
            .unwrap()
            .contains("feishu_task"));
        assert!(!serde_json::to_string(&work_item)
            .unwrap()
            .contains("rec_activity_1"));
    }

    #[tokio::test]
    async fn handoff_create_rejects_unmapped_capability_pair() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "handoff-create",
            "source_record_id": "rec_activity_1",
            "handoff_type": "member_notice",
            "target_agent": "erhua",
            "brief_summary": "提醒成员报名"
        }));

        validate("handoff-create", &payload).expect("payload shape should be valid");
        let err = execute_with_config(
            &Cli::parse_from(["qintopia-message-sidecar", "check"]),
            "handoff-create".to_string(),
            payload,
            false,
            true,
            &runtime_without_source(),
        )
        .await
        .expect_err("unmapped handoff should be rejected");

        assert!(err
            .to_string()
            .contains("handoff-create is not mapped to an operations capability"));
    }

    #[test]
    fn handoff_rejects_unapproved_target_agent() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "handoff-create",
            "source_record_id": "rec_activity_1",
            "handoff_type": "visual_asset_request",
            "target_agent": "unknown",
            "brief_summary": "做一组活动前宣海报 brief"
        }));

        let err = validate("handoff-create", &payload).expect_err("target should be rejected");
        assert!(err.to_string().contains("target_agent is not allowed"));
    }

    #[test]
    fn rejects_non_xiaoman_actor() {
        let payload = payload(json!({
            "actor_agent": "erhua",
            "operation": "record-get",
            "record_id": "rec_activity_1",
            "table_role": "activity_occurrence"
        }));

        let err = validate("record-get", &payload).expect_err("actor should be rejected");
        assert!(err.to_string().contains("actor_agent must be xiaoman"));
    }

    #[test]
    fn rejects_disallowed_table_role() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "list-by-date",
            "date": "2026-06-28",
            "table_role": "arbitrary_table"
        }));

        let err = validate("list-by-date", &payload).expect_err("table role should be rejected");
        assert!(err.to_string().contains("table_role is not allowed"));
    }

    #[test]
    fn rejects_operation_mismatch_between_cli_and_payload() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "record-get",
            "record_id": "rec_activity_1",
            "table_role": "activity_occurrence"
        }));

        let err = validate("list-by-date", &payload).expect_err("operation mismatch should fail");
        assert!(err
            .to_string()
            .contains("operation mismatch between CLI and payload"));
    }

    #[test]
    fn decodes_chunked_body() {
        let body = b"5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";
        assert_eq!(decode_chunked_body(body).unwrap(), "hello world");
    }

    #[test]
    fn decodes_chunked_body_with_extensions_and_trailer() {
        let body = b"\r\n5;foo=bar\r\nhello\r\n6\r\n world\r\n0\r\nx-trace: ok\r\n\r\n";
        assert_eq!(decode_chunked_body(body).unwrap(), "hello world");
    }

    #[test]
    fn parses_chunked_http_response_as_bytes() {
        let response = concat!(
            "HTTP/1.1 200 OK\r\n",
            "Transfer-Encoding: chunked\r\n",
            "\r\n",
            "10\r\n",
            "{\"msg\":\"你好\"}",
            "\r\n0\r\n\r\n"
        )
        .as_bytes();
        assert_eq!(parse_http_response(response).unwrap(), "{\"msg\":\"你好\"}");
    }

    #[test]
    fn sanitizes_feishu_url_in_error_context() {
        let sanitized = sanitize_feishu_url(
            "https://open.feishu.cn/open-apis/bitable/v1/apps/app_token_123/tables/tbl_secret_456/records?page_size=200&page_token=next",
        );

        assert!(sanitized
            .contains("open.feishu.cn/open-apis/bitable/v1/apps/redacted/tables/redacted/records"));
        assert!(!sanitized.contains("app_token_123"));
        assert!(!sanitized.contains("tbl_secret_456"));
        assert!(!sanitized.contains("page_token"));
    }

    #[test]
    fn feishu_read_limitation_does_not_expose_internal_tool_names() {
        for forbidden in [
            "lark-base",
            "execute_code",
            "terminal",
            "skill_view",
            "Dangerous command requires approval",
            "Working",
        ] {
            assert!(
                !FEISHU_READ_LIMITATION.contains(forbidden),
                "forbidden term leaked in Feishu read limitation: {forbidden}"
            );
        }
    }
}
