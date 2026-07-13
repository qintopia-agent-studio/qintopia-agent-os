use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{postgres::PgPool, Row};
use uuid::Uuid;

use crate::{config::Cli, db};

const WORKER_ID: &str = "group-message-send-worker";
const SUPPORTED_WORK_ITEM_TYPE: &str = "group_message_request";
const SUPPORTED_CAPABILITY: &str = "erhua.send_group_message";
const FIXTURE_ALLOWED_GROUP_ALIAS: &str = "community_activity_group";
const MAX_SEND_READY_ATTEMPTS: i32 = 3;

#[derive(Debug, Serialize)]
pub struct GroupMessageSendWorkerReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub fixture_mode: bool,
    pub worker: &'static str,
    pub action_status: String,
    pub work_item_id: Option<Uuid>,
    pub current_status: String,
    pub send_executed: bool,
    pub target_channel: String,
    pub target_group_alias: Option<String>,
    pub target_group_id: Option<String>,
    pub approved_artifact_id: Option<Uuid>,
    pub message_preview: String,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone)]
struct GroupMessageWorkItem {
    id: Uuid,
    work_item_type: String,
    requester_agent: String,
    target_agent: String,
    capability_key: String,
    status: String,
    review_policy: String,
    payload: Value,
}

#[derive(Debug, Clone)]
struct SendPlan {
    target_channel: String,
    target_group_alias: Option<String>,
    target_group_id: Option<String>,
    approved_artifact_id: Uuid,
    message_text: String,
}

#[derive(Debug, Clone)]
struct SendPolicy {
    allowed_group_aliases: Vec<String>,
    allowed_group_ids: Vec<String>,
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
    if !once {
        bail!("group message send worker currently supports --once only");
    }

    let apply_requested = apply && !dry_run;
    let report = if fixture_mode {
        run_fixture(apply_requested)?
    } else {
        let database_url = cli.database_url_required()?;
        let pool = db::connect(database_url, cli.db_max_connections).await?;
        let policy = SendPolicy::from_cli(cli);
        run_once(&pool, &policy, apply_requested, work_item_id).await?
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn run_fixture(apply_requested: bool) -> Result<GroupMessageSendWorkerReport> {
    if apply_requested {
        bail!("fixture-mode cannot be used with --apply");
    }
    let policy = SendPolicy::fixture();
    let work_item = GroupMessageWorkItem {
        id: Uuid::nil(),
        work_item_type: SUPPORTED_WORK_ITEM_TYPE.to_string(),
        requester_agent: "xiaoman".to_string(),
        target_agent: "erhua".to_string(),
        capability_key: SUPPORTED_CAPABILITY.to_string(),
        status: "queued".to_string(),
        review_policy: "human_final_confirmation".to_string(),
        payload: json!({
            "approved_artifact_id": "02dd5f47-81f8-4b8c-898d-b4c926fcf9b5",
            "target_channel": "qiwe",
            "target_group_alias": FIXTURE_ALLOWED_GROUP_ALIAS,
            "message_text": "周末共创晚餐报名开始啦"
        }),
    };
    let plan = validate_work_item(&work_item, &policy)?;
    Ok(report_from_plan(
        true,
        false,
        true,
        "fixture_dry_run_ok",
        Some(work_item.id),
        &work_item.status,
        &plan,
    ))
}

async fn run_once(
    pool: &PgPool,
    policy: &SendPolicy,
    apply_requested: bool,
    work_item_id: Option<Uuid>,
) -> Result<GroupMessageSendWorkerReport> {
    if !apply_requested {
        let Some(work_item) = peek_work_item(pool, work_item_id).await? else {
            return Ok(empty_report(
                false,
                false,
                "no_claimable_group_message_request",
            ));
        };
        let plan = validate_work_item(&work_item, policy)?;
        validate_approved_artifact(
            pool,
            plan.approved_artifact_id,
            required_artifact_type(&work_item.payload),
        )
        .await?;
        return Ok(report_from_plan(
            false,
            false,
            false,
            "dry_run_ok",
            Some(work_item.id),
            &work_item.status,
            &plan,
        ));
    }

    let mut tx = pool
        .begin()
        .await
        .context("begin group message send worker transaction")?;
    let Some(work_item) = lock_work_item(&mut tx, work_item_id).await? else {
        tx.commit()
            .await
            .context("commit no-op group message send worker transaction")?;
        return Ok(empty_report(
            false,
            true,
            "no_claimable_group_message_request",
        ));
    };

    let plan = match validate_work_item(&work_item, policy) {
        Ok(plan) => plan,
        Err(err) => {
            mark_work_item_failed(&mut tx, &work_item, &err.to_string()).await?;
            tx.commit()
                .await
                .context("commit denied group message send transaction")?;
            return Err(err);
        }
    };
    if let Err(err) = validate_approved_artifact_in_tx(
        &mut tx,
        plan.approved_artifact_id,
        required_artifact_type(&work_item.payload),
    )
    .await
    {
        mark_work_item_failed(&mut tx, &work_item, &err.to_string()).await?;
        tx.commit()
            .await
            .context("commit failed group message send transaction")?;
        return Err(err);
    }

    record_send_ready(&mut tx, &work_item, &plan).await?;
    tx.commit()
        .await
        .context("commit group message send-ready transaction")?;

    Ok(report_from_plan(
        false,
        true,
        false,
        "send_ready_recorded",
        Some(work_item.id),
        "queued",
        &plan,
    ))
}

async fn peek_work_item(
    pool: &PgPool,
    work_item_id: Option<Uuid>,
) -> Result<Option<GroupMessageWorkItem>> {
    if let Some(work_item_id) = work_item_id {
        return peek_work_item_by_id(pool, work_item_id).await;
    }
    peek_next_work_item(pool).await
}

async fn peek_next_work_item(pool: &PgPool) -> Result<Option<GroupMessageWorkItem>> {
    let row = sqlx::query(
        r#"
        SELECT id, work_item_type, requester_agent, target_agent, capability_key,
               status, review_policy, payload
        FROM qintopia_agent_os.work_items
        WHERE status = 'queued'
          AND available_at <= now()
          AND attempts < $3
          AND work_item_type = $1
          AND capability_key = $2
          AND NOT EXISTS (
              SELECT 1
              FROM qintopia_agent_os.work_item_events ready
              WHERE ready.work_item_id = qintopia_agent_os.work_items.id
                AND ready.event_type = 'group_message_send_ready_recorded'
                AND ready.data->>'send_executed' = 'false'
          )
        ORDER BY priority DESC, available_at ASC, created_at ASC
        LIMIT 1
        "#,
    )
    .bind(SUPPORTED_WORK_ITEM_TYPE)
    .bind(SUPPORTED_CAPABILITY)
    .bind(MAX_SEND_READY_ATTEMPTS)
    .fetch_optional(pool)
    .await
    .context("peek next group message work item")?;
    row.map(work_item_from_row).transpose()
}

async fn peek_work_item_by_id(
    pool: &PgPool,
    work_item_id: Uuid,
) -> Result<Option<GroupMessageWorkItem>> {
    let row = sqlx::query(
        r#"
        SELECT id, work_item_type, requester_agent, target_agent, capability_key,
               status, review_policy, payload
        FROM qintopia_agent_os.work_items
        WHERE id = $1
          AND status = 'queued'
          AND available_at <= now()
          AND attempts < $4
          AND work_item_type = $2
          AND capability_key = $3
          AND NOT EXISTS (
              SELECT 1
              FROM qintopia_agent_os.work_item_events ready
              WHERE ready.work_item_id = qintopia_agent_os.work_items.id
                AND ready.event_type = 'group_message_send_ready_recorded'
                AND ready.data->>'send_executed' = 'false'
          )
        LIMIT 1
        "#,
    )
    .bind(work_item_id)
    .bind(SUPPORTED_WORK_ITEM_TYPE)
    .bind(SUPPORTED_CAPABILITY)
    .bind(MAX_SEND_READY_ATTEMPTS)
    .fetch_optional(pool)
    .await
    .context("peek group message work item by id")?;
    row.map(work_item_from_row).transpose()
}

async fn lock_work_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Option<Uuid>,
) -> Result<Option<GroupMessageWorkItem>> {
    if let Some(work_item_id) = work_item_id {
        return lock_work_item_by_id(tx, work_item_id).await;
    }
    lock_next_work_item(tx).await
}

async fn lock_next_work_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<Option<GroupMessageWorkItem>> {
    let row = sqlx::query(
        r#"
        WITH claimed AS (
            SELECT id
            FROM qintopia_agent_os.work_items
            WHERE status = 'queued'
              AND available_at <= now()
              AND attempts < $3
              AND work_item_type = $1
              AND capability_key = $2
              AND NOT EXISTS (
                  SELECT 1
                  FROM qintopia_agent_os.work_item_events ready
                  WHERE ready.work_item_id = qintopia_agent_os.work_items.id
                    AND ready.event_type = 'group_message_send_ready_recorded'
                    AND ready.data->>'send_executed' = 'false'
              )
            ORDER BY priority DESC, available_at ASC, created_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE qintopia_agent_os.work_items items
        SET
            claimed_by = $4,
            locked_at = now(),
            claim_expires_at = now() + interval '10 minutes',
            attempts = attempts + 1,
            updated_at = now()
        FROM claimed
        WHERE items.id = claimed.id
        RETURNING items.id, items.work_item_type, items.requester_agent,
                  items.target_agent, items.capability_key, items.status,
                  items.review_policy, items.payload
        "#,
    )
    .bind(SUPPORTED_WORK_ITEM_TYPE)
    .bind(SUPPORTED_CAPABILITY)
    .bind(MAX_SEND_READY_ATTEMPTS)
    .bind(WORKER_ID)
    .fetch_optional(&mut **tx)
    .await
    .context("lock next group message work item")?;
    row.map(work_item_from_row).transpose()
}

async fn lock_work_item_by_id(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Uuid,
) -> Result<Option<GroupMessageWorkItem>> {
    let row = sqlx::query(
        r#"
        WITH claimed AS (
            SELECT id
            FROM qintopia_agent_os.work_items
            WHERE id = $1
              AND status = 'queued'
              AND available_at <= now()
              AND attempts < $4
              AND work_item_type = $2
              AND capability_key = $3
              AND NOT EXISTS (
                  SELECT 1
                  FROM qintopia_agent_os.work_item_events ready
                  WHERE ready.work_item_id = qintopia_agent_os.work_items.id
                    AND ready.event_type = 'group_message_send_ready_recorded'
                    AND ready.data->>'send_executed' = 'false'
              )
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE qintopia_agent_os.work_items items
        SET
            claimed_by = $5,
            locked_at = now(),
            claim_expires_at = now() + interval '10 minutes',
            attempts = attempts + 1,
            updated_at = now()
        FROM claimed
        WHERE items.id = claimed.id
        RETURNING items.id, items.work_item_type, items.requester_agent,
                  items.target_agent, items.capability_key, items.status,
                  items.review_policy, items.payload
        "#,
    )
    .bind(work_item_id)
    .bind(SUPPORTED_WORK_ITEM_TYPE)
    .bind(SUPPORTED_CAPABILITY)
    .bind(MAX_SEND_READY_ATTEMPTS)
    .bind(WORKER_ID)
    .fetch_optional(&mut **tx)
    .await
    .context("lock group message work item by id")?;
    row.map(work_item_from_row).transpose()
}

fn work_item_from_row(row: sqlx::postgres::PgRow) -> Result<GroupMessageWorkItem> {
    Ok(GroupMessageWorkItem {
        id: row.try_get("id")?,
        work_item_type: row.try_get("work_item_type")?,
        requester_agent: row.try_get("requester_agent")?,
        target_agent: row.try_get("target_agent")?,
        capability_key: row.try_get("capability_key")?,
        status: row.try_get("status")?,
        review_policy: row.try_get("review_policy")?,
        payload: row.try_get("payload")?,
    })
}

fn validate_work_item(work_item: &GroupMessageWorkItem, policy: &SendPolicy) -> Result<SendPlan> {
    if work_item.work_item_type != SUPPORTED_WORK_ITEM_TYPE {
        bail!("work item type is not supported by group message send worker");
    }
    if work_item.capability_key != SUPPORTED_CAPABILITY {
        bail!("capability is not supported by group message send worker");
    }
    if work_item.requester_agent != "xiaoman" {
        bail!("requester_agent is not allowed for group message send worker");
    }
    if work_item.target_agent != "erhua" {
        bail!("target_agent must be erhua");
    }
    if work_item.status != "queued" {
        bail!("group message request must be queued after final confirmation");
    }
    if work_item.review_policy != "human_final_confirmation" {
        bail!("group message request must require human_final_confirmation");
    }
    validate_payload(&work_item.payload, policy)
}

fn validate_payload(payload: &Value, policy: &SendPolicy) -> Result<SendPlan> {
    if contains_sensitive_value(payload) {
        bail!("group message payload contains disallowed sensitive or raw internal content");
    }
    let approved_artifact_id = required_uuid(payload, "approved_artifact_id")?;
    let target_channel = required_text(payload, "target_channel")?;
    let message_text = required_text(payload, "message_text")?;
    let target_group_alias = optional_text(payload, "target_group_alias");
    let target_group_id = optional_text(payload, "target_group_id");
    if target_group_alias.is_none() && target_group_id.is_none() {
        bail!("target_group_alias or target_group_id is required for group message requests");
    }
    if !policy.group_allowed(target_group_alias.as_deref(), target_group_id.as_deref()) {
        bail!("target group is not allowlisted for group message requests");
    }
    Ok(SendPlan {
        target_channel,
        target_group_alias,
        target_group_id,
        approved_artifact_id,
        message_text,
    })
}

fn required_artifact_type(payload: &Value) -> Option<&'static str> {
    (payload.get("workflow_type").and_then(Value::as_str) == Some("activity_promotion"))
        .then_some("generated_image")
}

async fn validate_approved_artifact(
    pool: &PgPool,
    artifact_id: Uuid,
    required_artifact_type: Option<&str>,
) -> Result<()> {
    let row = sqlx::query(
        r#"
        SELECT artifact_type, review_status
        FROM qintopia_agent_os.artifacts
        WHERE id = $1
        "#,
    )
    .bind(artifact_id)
    .fetch_optional(pool)
    .await
    .context("load approved artifact for group message send")?
    .ok_or_else(|| anyhow::anyhow!("approved_artifact_id does not exist"))?;
    let artifact_type: String = row.get("artifact_type");
    let review_status: String = row.get("review_status");
    if review_status != "approved" {
        bail!("approved_artifact_id must reference an approved artifact");
    }
    if let Some(required_artifact_type) = required_artifact_type {
        if artifact_type != required_artifact_type {
            bail!("approved_artifact_id must reference an approved {required_artifact_type}");
        }
    }
    Ok(())
}

async fn validate_approved_artifact_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    artifact_id: Uuid,
    required_artifact_type: Option<&str>,
) -> Result<()> {
    let row = sqlx::query(
        r#"
        SELECT artifact_type, review_status
        FROM qintopia_agent_os.artifacts
        WHERE id = $1
        "#,
    )
    .bind(artifact_id)
    .fetch_optional(&mut **tx)
    .await
    .context("load approved artifact for group message send")?
    .ok_or_else(|| anyhow::anyhow!("approved_artifact_id does not exist"))?;
    let artifact_type: String = row.get("artifact_type");
    let review_status: String = row.get("review_status");
    if review_status != "approved" {
        bail!("approved_artifact_id must reference an approved artifact");
    }
    if let Some(required_artifact_type) = required_artifact_type {
        if artifact_type != required_artifact_type {
            bail!("approved_artifact_id must reference an approved {required_artifact_type}");
        }
    }
    Ok(())
}

async fn record_send_ready(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item: &GroupMessageWorkItem,
    plan: &SendPlan,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET
            status = 'queued',
            locked_at = NULL,
            claim_expires_at = NULL,
            last_error = NULL,
            metadata = metadata || $2,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(work_item.id)
    .bind(json!({
        "group_message_send_worker": {
            "last_ready_checked_by": WORKER_ID,
            "send_executed": false
        }
    }))
    .execute(&mut **tx)
    .await
    .context("record group message send-ready metadata")?;

    append_event_in_tx(
        tx,
        WorkItemEvent {
            work_item_id: Some(work_item.id),
            artifact_id: None,
            event_type: "group_message_send_ready_recorded",
            actor_type: "worker",
            actor_id: WORKER_ID,
            message: "group message send worker validated request without sending",
            data: json!({
                "target_channel": plan.target_channel,
                "target_group_alias": plan.target_group_alias,
                "target_group_id": plan.target_group_id,
                "approved_artifact_id": plan.approved_artifact_id,
                "send_executed": false,
                "message_preview": message_preview(&plan.message_text),
            }),
        },
    )
    .await?;
    Ok(())
}

async fn mark_work_item_failed(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item: &GroupMessageWorkItem,
    error: &str,
) -> Result<()> {
    let message = trim_error(error);
    sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET
            status = 'failed',
            locked_at = NULL,
            claim_expires_at = NULL,
            last_error = $2,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(work_item.id)
    .bind(&message)
    .execute(&mut **tx)
    .await
    .context("mark group message work item failed")?;
    append_event_in_tx(
        tx,
        WorkItemEvent {
            work_item_id: Some(work_item.id),
            artifact_id: None,
            event_type: "group_message_send_denied_by_policy",
            actor_type: "worker",
            actor_id: WORKER_ID,
            message: "group message send worker rejected request before sending",
            data: json!({"error": message, "send_executed": false}),
        },
    )
    .await?;
    Ok(())
}

struct WorkItemEvent<'a> {
    work_item_id: Option<Uuid>,
    artifact_id: Option<Uuid>,
    event_type: &'a str,
    actor_type: &'a str,
    actor_id: &'a str,
    message: &'a str,
    data: Value,
}

async fn append_event_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event: WorkItemEvent<'_>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_item_events
            (work_item_id, artifact_id, event_type, actor_type, actor_id, message, data)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(event.work_item_id)
    .bind(event.artifact_id)
    .bind(event.event_type)
    .bind(event.actor_type)
    .bind(event.actor_id)
    .bind(event.message)
    .bind(event.data)
    .execute(&mut **tx)
    .await
    .context("append group message send event")?;
    Ok(())
}

impl SendPolicy {
    fn from_cli(cli: &Cli) -> Self {
        Self {
            allowed_group_aliases: split_csv_normalized(&cli.operations_allowed_group_aliases),
            allowed_group_ids: split_csv_raw(&cli.operations_allowed_group_ids),
        }
    }

    fn fixture() -> Self {
        Self {
            allowed_group_aliases: vec![FIXTURE_ALLOWED_GROUP_ALIAS.to_string()],
            allowed_group_ids: Vec::new(),
        }
    }

    fn group_allowed(&self, alias: Option<&str>, group_id: Option<&str>) -> bool {
        alias
            .map(normalize_key)
            .filter(|item| {
                self.allowed_group_aliases
                    .iter()
                    .any(|allowed| allowed == item)
            })
            .is_some()
            || group_id
                .map(str::trim)
                .filter(|item| {
                    self.allowed_group_ids
                        .iter()
                        .any(|allowed| allowed == *item)
                })
                .is_some()
    }
}

fn report_from_plan(
    dry_run: bool,
    apply_requested: bool,
    fixture_mode: bool,
    action_status: &str,
    work_item_id: Option<Uuid>,
    current_status: &str,
    plan: &SendPlan,
) -> GroupMessageSendWorkerReport {
    GroupMessageSendWorkerReport {
        success: true,
        dry_run,
        apply_requested,
        fixture_mode,
        worker: WORKER_ID,
        action_status: action_status.to_string(),
        work_item_id,
        current_status: current_status.to_string(),
        send_executed: false,
        target_channel: plan.target_channel.clone(),
        target_group_alias: plan.target_group_alias.clone(),
        target_group_id: plan.target_group_id.clone(),
        approved_artifact_id: Some(plan.approved_artifact_id),
        message_preview: message_preview(&plan.message_text),
        limitations: limitations(),
        guardrails: guardrails(),
    }
}

fn empty_report(
    fixture_mode: bool,
    apply_requested: bool,
    action_status: &str,
) -> GroupMessageSendWorkerReport {
    GroupMessageSendWorkerReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        fixture_mode,
        worker: WORKER_ID,
        action_status: action_status.to_string(),
        work_item_id: None,
        current_status: String::new(),
        send_executed: false,
        target_channel: String::new(),
        target_group_alias: None,
        target_group_id: None,
        approved_artifact_id: None,
        message_preview: String::new(),
        limitations: vec!["no claimable group_message_request was found".to_string()],
        guardrails: guardrails(),
    }
}

fn limitations() -> Vec<String> {
    vec![
        "this worker validates send readiness only; it does not call QiWe or Erhua send APIs"
            .to_string(),
        "apply mode records a send-ready audit event and keeps the work item queued".to_string(),
        "real external send requires a separate adapter and production allowlist acceptance"
            .to_string(),
    ]
}

fn guardrails() -> Vec<String> {
    vec![
        "only queued group_message_request work items can be processed".to_string(),
        "target groups must be allowlisted".to_string(),
        "approved_artifact_id must reference an approved artifact".to_string(),
        "send results must be recorded in work_item_events before any production adapter is enabled"
            .to_string(),
    ]
}

fn required_text(payload: &Value, field: &str) -> Result<String> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| anyhow::anyhow!("{field} is required for group message send"))
}

fn optional_text(payload: &Value, field: &str) -> Option<String> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
}

fn required_uuid(payload: &Value, field: &str) -> Result<Uuid> {
    let text = required_text(payload, field)?;
    Uuid::parse_str(&text).with_context(|| format!("{field} must be a uuid"))
}

fn message_preview(text: &str) -> String {
    const MAX: usize = 80;
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX {
        return trimmed.to_string();
    }
    trimmed.chars().take(MAX).collect()
}

fn trim_error(error: &str) -> String {
    const MAX: usize = 500;
    let trimmed = error.trim();
    if trimmed.chars().count() <= MAX {
        return trimmed.to_string();
    }
    trimmed.chars().take(MAX).collect()
}

fn split_csv_normalized(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(normalize_key)
        .collect()
}

fn split_csv_raw(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn normalize_key(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace(' ', "_")
}

fn contains_sensitive_value(value: &Value) -> bool {
    match value {
        Value::String(text) => contains_sensitive_text(text),
        Value::Array(items) => items.iter().any(contains_sensitive_value),
        Value::Object(map) => map
            .iter()
            .any(|(key, value)| contains_sensitive_key(key) || contains_sensitive_value(value)),
        _ => false,
    }
}

fn contains_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "token",
        "secret",
        "app_secret",
        "app_token",
        "table_id",
        "base_token",
        "system_prompt",
        "raw_chat_text",
        "member_dossier",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

fn contains_sensitive_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "app_token",
        "tenant_access_token",
        "authorization: bearer",
        "base table",
        "system prompt",
        "raw private chat",
        "member dossier",
        "lark-base",
        "hermes kanban",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "postgres-integration-tests")]
    fn postgres_integration_database_url() -> String {
        assert_eq!(
            std::env::var("QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE").as_deref(),
            Ok("1"),
            "PostgreSQL integration test requires the explicit apply-smoke guard"
        );
        let database_url = std::env::var("QINTOPIA_SIDECAR_DATABASE_URL")
            .expect("PostgreSQL integration test requires QINTOPIA_SIDECAR_DATABASE_URL");
        let parsed = url::Url::parse(&database_url).expect("integration database URL must parse");
        assert!(
            matches!(parsed.scheme(), "postgres" | "postgresql"),
            "PostgreSQL integration test requires a postgres URL"
        );
        assert!(
            matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "::1")),
            "PostgreSQL integration test may only use a loopback database"
        );
        assert_eq!(
            parsed.path().trim_start_matches('/'),
            "qintopia_test",
            "PostgreSQL integration test may only use qintopia_test"
        );
        database_url
    }

    #[cfg(feature = "postgres-integration-tests")]
    async fn insert_image_and_group_request(
        pool: &PgPool,
        artifact_review_status: &str,
    ) -> (Uuid, Uuid, Uuid) {
        let image_work_item_id = Uuid::new_v4();
        let artifact_id = Uuid::new_v4();
        let group_work_item_id = Uuid::new_v4();
        let suffix = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.work_items
                (id, work_item_type, status, requester_agent, target_agent,
                 capability_key, brief_summary, purpose, source_type, source_refs,
                 dedupe_key, idempotency_key, risk_level, information_class,
                 payload, review_policy)
            VALUES
                ($1, 'image_generation_request', 'completed', 'xiaoman', 'huabaosi',
                 'huabaosi.generate_image_asset', 'integration generated image',
                 'activity promotion image', 'integration_test', '{}'::jsonb,
                 $2, $3, 'medium', 'internal_ops', '{}'::jsonb, 'before_external_use')
            "#,
        )
        .bind(image_work_item_id)
        .bind(format!("group-send-integration-image:{suffix}"))
        .bind(format!("group-send-integration-image:{suffix}"))
        .execute(pool)
        .await
        .expect("insert image-generation work item");

        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.artifacts
                (id, work_item_id, artifact_type, review_status, created_by_agent,
                 title, summary, artifact_uri, content_hash, risk_labels,
                 information_class, metadata)
            VALUES
                ($1, $2, 'generated_image', $3, 'huabaosi', 'integration image',
                 'sanitized integration fixture',
                 'https://media.example.test/posters/integration.png',
                 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                 ARRAY['external_publish']::text[], 'internal_ops', '{}'::jsonb)
            "#,
        )
        .bind(artifact_id)
        .bind(image_work_item_id)
        .bind(artifact_review_status)
        .execute(pool)
        .await
        .expect("insert generated-image artifact");

        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.work_items
                (id, work_item_type, status, requester_agent, target_agent,
                 capability_key, brief_summary, purpose, source_type, source_refs,
                 dedupe_key, idempotency_key, risk_level, information_class,
                 payload, review_policy)
            VALUES
                ($1, 'group_message_request', 'queued', 'xiaoman', 'erhua',
                 'erhua.send_group_message', 'integration group message',
                 'activity promotion send readiness', 'integration_test', '{}'::jsonb,
                 $2, $3, 'high', 'internal_ops', $4, 'human_final_confirmation')
            "#,
        )
        .bind(group_work_item_id)
        .bind(format!("group-send-integration-request:{suffix}"))
        .bind(format!("group-send-integration-request:{suffix}"))
        .bind(json!({
            "approved_artifact_id": artifact_id,
            "approved_artifact_type": "generated_image",
            "workflow_type": "activity_promotion",
            "target_channel": "qiwe",
            "target_group_alias": "integration_test_group",
            "message_text": "活动海报已完成审核，请发送到测试群。",
            "send_executed": false
        }))
        .execute(pool)
        .await
        .expect("insert group-message work item");

        (image_work_item_id, artifact_id, group_work_item_id)
    }

    #[cfg(feature = "postgres-integration-tests")]
    async fn delete_integration_rows(pool: &PgPool, image_id: Uuid, group_id: Uuid) {
        sqlx::query("DELETE FROM qintopia_agent_os.work_items WHERE id = $1")
            .bind(group_id)
            .execute(pool)
            .await
            .expect("delete group-message integration work item");
        sqlx::query("DELETE FROM qintopia_agent_os.work_items WHERE id = $1")
            .bind(image_id)
            .execute(pool)
            .await
            .expect("delete image-generation integration work item");
    }

    #[test]
    fn fixture_worker_reports_send_ready_without_sending() {
        let report = run_fixture(false).expect("fixture should validate");

        assert_eq!(report.action_status, "fixture_dry_run_ok");
        assert_eq!(report.current_status, "queued");
        assert_eq!(
            report.target_group_alias.as_deref(),
            Some("community_activity_group")
        );
        assert!(!report.send_executed);
    }

    #[test]
    fn payload_rejects_non_allowlisted_group() {
        let policy = SendPolicy::fixture();
        let err = validate_payload(
            &json!({
                "approved_artifact_id": "02dd5f47-81f8-4b8c-898d-b4c926fcf9b5",
                "target_channel": "qiwe",
                "target_group_alias": "unknown_group",
                "message_text": "周末共创晚餐报名开始啦"
            }),
            &policy,
        )
        .expect_err("non-allowlisted group should be rejected");

        assert!(err.to_string().contains("target group is not allowlisted"));
    }

    #[test]
    fn payload_rejects_sensitive_message_text() {
        let policy = SendPolicy::fixture();
        let err = validate_payload(
            &json!({
                "approved_artifact_id": "02dd5f47-81f8-4b8c-898d-b4c926fcf9b5",
                "target_channel": "qiwe",
                "target_group_alias": "community_activity_group",
                "message_text": "contains app_token"
            }),
            &policy,
        )
        .expect_err("sensitive message should be rejected");

        assert!(err.to_string().contains("contains disallowed sensitive"));
    }

    #[test]
    fn activity_promotion_requires_a_generated_image_artifact() {
        assert_eq!(
            required_artifact_type(&json!({"workflow_type": "activity_promotion"})),
            Some("generated_image")
        );
        assert_eq!(required_artifact_type(&json!({})), None);
    }

    #[test]
    fn work_item_must_be_queued_after_confirmation() {
        let policy = SendPolicy::fixture();
        let work_item = GroupMessageWorkItem {
            id: Uuid::nil(),
            work_item_type: SUPPORTED_WORK_ITEM_TYPE.to_string(),
            requester_agent: "xiaoman".to_string(),
            target_agent: "erhua".to_string(),
            capability_key: SUPPORTED_CAPABILITY.to_string(),
            status: "awaiting_publish".to_string(),
            review_policy: "human_final_confirmation".to_string(),
            payload: json!({
                "approved_artifact_id": "02dd5f47-81f8-4b8c-898d-b4c926fcf9b5",
                "target_channel": "qiwe",
                "target_group_alias": "community_activity_group",
                "message_text": "周末共创晚餐报名开始啦"
            }),
        };
        let err = validate_work_item(&work_item, &policy)
            .expect_err("awaiting_publish should require final confirmation first");

        assert!(err.to_string().contains("must be queued"));
    }

    #[tokio::test]
    #[cfg(feature = "postgres-integration-tests")]
    #[ignore = "requires guarded disposable qintopia_test PostgreSQL"]
    async fn postgres_send_ready_is_idempotent_and_fails_closed() {
        let database_url = postgres_integration_database_url();
        let pool = db::connect(&database_url, 2)
            .await
            .expect("connect disposable PostgreSQL");
        db::run_migrations(&pool)
            .await
            .expect("migrate disposable PostgreSQL");
        let policy = SendPolicy {
            allowed_group_aliases: vec!["integration_test_group".to_string()],
            allowed_group_ids: Vec::new(),
        };

        let (image_id, _artifact_id, group_id) =
            insert_image_and_group_request(&pool, "approved").await;
        let report = run_once(&pool, &policy, true, Some(group_id))
            .await
            .expect("approved request should record send readiness");
        assert_eq!(report.action_status, "send_ready_recorded");
        assert!(report.apply_requested);
        assert!(!report.send_executed);

        let state: (String, i32, i64, i64) = sqlx::query_as(
            r#"
            SELECT
                status,
                attempts,
                count(*) FILTER (
                    WHERE event_type = 'group_message_send_ready_recorded'
                      AND data->>'send_executed' = 'false'
                ),
                count(*) FILTER (
                    WHERE event_type IN ('send_executed', 'external_published')
                )
            FROM qintopia_agent_os.work_items item
            LEFT JOIN qintopia_agent_os.work_item_events event
              ON event.work_item_id = item.id
            WHERE item.id = $1
            GROUP BY item.status, item.attempts
            "#,
        )
        .bind(group_id)
        .fetch_one(&pool)
        .await
        .expect("read send-ready integration state");
        assert_eq!(state, ("queued".to_string(), 1, 1, 0));

        let second = run_once(&pool, &policy, true, Some(group_id))
            .await
            .expect("duplicate apply should be a no-op");
        assert_eq!(second.action_status, "no_claimable_group_message_request");
        let ready_count: (i64,) = sqlx::query_as(
            r#"
            SELECT count(*)
            FROM qintopia_agent_os.work_item_events
            WHERE work_item_id = $1
              AND event_type = 'group_message_send_ready_recorded'
            "#,
        )
        .bind(group_id)
        .fetch_one(&pool)
        .await
        .expect("count idempotent send-ready events");
        assert_eq!(ready_count.0, 1);
        delete_integration_rows(&pool, image_id, group_id).await;

        let (pending_image_id, _pending_artifact_id, pending_group_id) =
            insert_image_and_group_request(&pool, "pending").await;
        let error = run_once(&pool, &policy, true, Some(pending_group_id))
            .await
            .expect_err("pending artifact must fail closed");
        assert!(error
            .to_string()
            .contains("must reference an approved artifact"));
        let denied_state: (String, i32, i64, i64) = sqlx::query_as(
            r#"
            SELECT
                status,
                attempts,
                count(*) FILTER (
                    WHERE event_type = 'group_message_send_denied_by_policy'
                      AND data->>'send_executed' = 'false'
                ),
                count(*) FILTER (
                    WHERE event_type IN ('group_message_send_ready_recorded',
                                         'send_executed', 'external_published')
                )
            FROM qintopia_agent_os.work_items item
            LEFT JOIN qintopia_agent_os.work_item_events event
              ON event.work_item_id = item.id
            WHERE item.id = $1
            GROUP BY item.status, item.attempts
            "#,
        )
        .bind(pending_group_id)
        .fetch_one(&pool)
        .await
        .expect("read denied send-ready integration state");
        assert_eq!(denied_state, ("failed".to_string(), 1, 1, 0));
        delete_integration_rows(&pool, pending_image_id, pending_group_id).await;
    }
}
