use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{postgres::PgPool, Row};
use uuid::Uuid;

use crate::{config::Cli, db};

const WORKER_ID: &str = "workbench-mirror-worker";
const PROVIDER: &str = "feishu_task_dry_run";
const TASKLIST_NAME: &str = "AgentOS · 运营协作工作台";

#[derive(Debug, Serialize)]
pub struct WorkbenchMirrorReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub fixture_mode: bool,
    pub worker: &'static str,
    pub action_status: String,
    pub work_item_id: Option<Uuid>,
    pub provider: &'static str,
    pub intended_tasklist_name: &'static str,
    pub task_title: String,
    pub task_section: String,
    pub description: String,
    pub description_fields: Vec<String>,
    pub external_id: Option<String>,
    pub sensitive_fields_redacted: bool,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone)]
struct WorkbenchWorkItem {
    id: Uuid,
    work_item_type: String,
    status: String,
    requester_agent: String,
    target_agent: String,
    capability_key: String,
    human_owner: String,
    priority: String,
    brief_summary: String,
    source_type: String,
    source_refs: Value,
    risk_level: String,
    review_policy: String,
    payload: Value,
    child_status_refs: Vec<ChildStatusRef>,
    current_blocking_point: Option<String>,
    artifact_count: i64,
    pending_artifact_count: i64,
    approved_artifact_count: i64,
}

#[derive(Debug, Clone)]
struct ChildStatusRef {
    work_item_id: Uuid,
    work_item_type: String,
    status: String,
    capability_key: String,
    blocking_reason: Option<String>,
}

#[derive(Debug, Clone)]
struct TaskMirrorPlan {
    title: String,
    section: String,
    description: String,
    description_fields: Vec<String>,
    external_id: String,
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
        bail!("workbench mirror worker currently supports --once only");
    }

    let apply_requested = apply && !dry_run;
    let report = if fixture_mode {
        run_fixture(apply_requested)?
    } else {
        let database_url = cli.database_url_required()?;
        let pool = db::connect(database_url, cli.db_max_connections).await?;
        run_once(&pool, apply_requested, work_item_id).await?
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn run_fixture(apply_requested: bool) -> Result<WorkbenchMirrorReport> {
    if apply_requested {
        bail!("fixture-mode cannot be used with --apply");
    }
    let item = WorkbenchWorkItem {
        id: Uuid::nil(),
        work_item_type: "visual_asset_request".to_string(),
        status: "awaiting_review".to_string(),
        requester_agent: "xiaoman".to_string(),
        target_agent: "huabaosi".to_string(),
        capability_key: "huabaosi.create_visual_asset".to_string(),
        human_owner: "运营负责人".to_string(),
        priority: "normal".to_string(),
        brief_summary: "周末共创晚餐活动运营海报".to_string(),
        source_type: "xiaoman_activity".to_string(),
        source_refs: json!({"source_record_ref": "activity_occurrence:fixture"}),
        risk_level: "medium".to_string(),
        review_policy: "before_external_use".to_string(),
        payload: json!({"handoff_type": "visual_asset_request"}),
        child_status_refs: vec![ChildStatusRef {
            work_item_id: Uuid::nil(),
            work_item_type: "visual_asset_request".to_string(),
            status: "awaiting_review".to_string(),
            capability_key: "huabaosi.create_visual_asset".to_string(),
            blocking_reason: Some("waiting_for_artifact_review".to_string()),
        }],
        current_blocking_point: Some(
            "visual_asset_request:waiting_for_artifact_review".to_string(),
        ),
        artifact_count: 1,
        pending_artifact_count: 1,
        approved_artifact_count: 0,
    };
    let plan = build_task_plan(&item)?;
    Ok(report_from_plan(
        true,
        false,
        true,
        "fixture_dry_run_ok",
        Some(item.id),
        None,
        &plan,
    ))
}

async fn run_once(
    pool: &PgPool,
    apply_requested: bool,
    work_item_id: Option<Uuid>,
) -> Result<WorkbenchMirrorReport> {
    if !apply_requested {
        let Some(item) = peek_work_item(pool, work_item_id).await? else {
            return Ok(empty_report(false, false, "no_mirrorable_work_item"));
        };
        let plan = build_task_plan(&item)?;
        return Ok(report_from_plan(
            false,
            false,
            false,
            "dry_run_ok",
            Some(item.id),
            None,
            &plan,
        ));
    }

    let mut tx = pool
        .begin()
        .await
        .context("begin workbench mirror transaction")?;
    let Some(item) = lock_work_item(&mut tx, work_item_id).await? else {
        tx.commit()
            .await
            .context("commit no-op workbench mirror transaction")?;
        return Ok(empty_report(false, true, "no_mirrorable_work_item"));
    };
    let plan = build_task_plan(&item)?;
    let ref_id = upsert_workbench_ref(&mut tx, &item, &plan).await?;
    append_event_in_tx(
        &mut tx,
        WorkItemEvent {
            work_item_id: Some(item.id),
            artifact_id: None,
            event_type: "mirror_dry_run_recorded",
            actor_type: "worker",
            actor_id: WORKER_ID,
            message: "Feishu Task mirror payload recorded without calling Feishu",
            data: json!({
                "provider": PROVIDER,
                "external_id": plan.external_id,
                "task_title": plan.title,
                "task_section": plan.section,
                "description_fields": plan.description_fields,
                "sensitive_fields_redacted": true,
            }),
        },
    )
    .await?;
    tx.commit()
        .await
        .context("commit workbench mirror transaction")?;

    Ok(report_from_plan(
        false,
        true,
        false,
        "mirror_dry_run_recorded",
        Some(item.id),
        Some(ref_id.to_string()),
        &plan,
    ))
}

async fn peek_work_item(
    pool: &PgPool,
    work_item_id: Option<Uuid>,
) -> Result<Option<WorkbenchWorkItem>> {
    if let Some(work_item_id) = work_item_id {
        return peek_work_item_by_id(pool, work_item_id).await;
    }
    peek_next_work_item(pool).await
}

async fn peek_next_work_item(pool: &PgPool) -> Result<Option<WorkbenchWorkItem>> {
    let row = sqlx::query(
        r#"
        SELECT
            wi.id,
            wi.work_item_type,
            wi.status,
            wi.requester_agent,
            wi.target_agent,
            wi.capability_key,
            wi.human_owner,
            wi.priority,
            wi.brief_summary,
            wi.source_type,
            wi.source_refs,
            wi.risk_level,
            wi.review_policy,
            wi.payload,
            COUNT(a.id)::bigint AS artifact_count,
            COUNT(a.id) FILTER (WHERE a.review_status = 'pending')::bigint AS pending_artifact_count,
            COUNT(a.id) FILTER (WHERE a.review_status = 'approved')::bigint AS approved_artifact_count
        FROM qintopia_agent_os.work_items wi
        LEFT JOIN qintopia_agent_os.artifacts a ON a.work_item_id = wi.id
        LEFT JOIN qintopia_agent_os.human_workbench_refs refs
          ON refs.work_item_id = wi.id AND refs.provider = $1
        WHERE refs.id IS NULL
          AND wi.status IN ('queued', 'processing', 'awaiting_review', 'awaiting_publish', 'failed')
        GROUP BY wi.id
        ORDER BY wi.created_at ASC
        LIMIT 1
        "#,
    )
    .bind(PROVIDER)
    .fetch_optional(pool)
    .await
    .context("peek next work item for workbench mirror")?;
    match row {
        Some(row) => work_item_from_row(pool, row).await.map(Some),
        None => Ok(None),
    }
}

async fn peek_work_item_by_id(
    pool: &PgPool,
    work_item_id: Uuid,
) -> Result<Option<WorkbenchWorkItem>> {
    let row = sqlx::query(
        r#"
        SELECT
            wi.id,
            wi.work_item_type,
            wi.status,
            wi.requester_agent,
            wi.target_agent,
            wi.capability_key,
            wi.human_owner,
            wi.priority,
            wi.brief_summary,
            wi.source_type,
            wi.source_refs,
            wi.risk_level,
            wi.review_policy,
            wi.payload,
            COUNT(a.id)::bigint AS artifact_count,
            COUNT(a.id) FILTER (WHERE a.review_status = 'pending')::bigint AS pending_artifact_count,
            COUNT(a.id) FILTER (WHERE a.review_status = 'approved')::bigint AS approved_artifact_count
        FROM qintopia_agent_os.work_items wi
        LEFT JOIN qintopia_agent_os.artifacts a ON a.work_item_id = wi.id
        LEFT JOIN qintopia_agent_os.human_workbench_refs refs
          ON refs.work_item_id = wi.id AND refs.provider = $1
        WHERE refs.id IS NULL
          AND wi.id = $2
          AND wi.status IN ('queued', 'processing', 'awaiting_review', 'awaiting_publish', 'failed')
        GROUP BY wi.id
        LIMIT 1
        "#,
    )
    .bind(PROVIDER)
    .bind(work_item_id)
    .fetch_optional(pool)
    .await
    .context("peek work item by id for workbench mirror")?;
    match row {
        Some(row) => work_item_from_row(pool, row).await.map(Some),
        None => Ok(None),
    }
}

async fn lock_work_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Option<Uuid>,
) -> Result<Option<WorkbenchWorkItem>> {
    if let Some(work_item_id) = work_item_id {
        return lock_work_item_by_id(tx, work_item_id).await;
    }
    lock_next_work_item(tx).await
}

async fn lock_next_work_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<Option<WorkbenchWorkItem>> {
    let row = sqlx::query(
        r#"
        WITH candidate AS (
            SELECT wi.id
            FROM qintopia_agent_os.work_items wi
            LEFT JOIN qintopia_agent_os.human_workbench_refs refs
              ON refs.work_item_id = wi.id AND refs.provider = $1
            WHERE refs.id IS NULL
              AND wi.status IN ('queued', 'processing', 'awaiting_review', 'awaiting_publish', 'failed')
            ORDER BY wi.created_at ASC
            LIMIT 1
            FOR UPDATE OF wi SKIP LOCKED
        )
        SELECT
            wi.id,
            wi.work_item_type,
            wi.status,
            wi.requester_agent,
            wi.target_agent,
            wi.capability_key,
            wi.human_owner,
            wi.priority,
            wi.brief_summary,
            wi.source_type,
            wi.source_refs,
            wi.risk_level,
            wi.review_policy,
            wi.payload,
            COUNT(a.id)::bigint AS artifact_count,
            COUNT(a.id) FILTER (WHERE a.review_status = 'pending')::bigint AS pending_artifact_count,
            COUNT(a.id) FILTER (WHERE a.review_status = 'approved')::bigint AS approved_artifact_count
        FROM candidate
        JOIN qintopia_agent_os.work_items wi ON wi.id = candidate.id
        LEFT JOIN qintopia_agent_os.artifacts a ON a.work_item_id = wi.id
        GROUP BY wi.id
        "#,
    )
    .bind(PROVIDER)
    .fetch_optional(&mut **tx)
    .await
    .context("lock next work item for workbench mirror")?;
    match row {
        Some(row) => work_item_from_row_tx(tx, row).await.map(Some),
        None => Ok(None),
    }
}

async fn lock_work_item_by_id(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Uuid,
) -> Result<Option<WorkbenchWorkItem>> {
    let row = sqlx::query(
        r#"
        WITH candidate AS (
            SELECT wi.id
            FROM qintopia_agent_os.work_items wi
            LEFT JOIN qintopia_agent_os.human_workbench_refs refs
              ON refs.work_item_id = wi.id AND refs.provider = $1
            WHERE refs.id IS NULL
              AND wi.id = $2
              AND wi.status IN ('queued', 'processing', 'awaiting_review', 'awaiting_publish', 'failed')
            LIMIT 1
            FOR UPDATE OF wi SKIP LOCKED
        )
        SELECT
            wi.id,
            wi.work_item_type,
            wi.status,
            wi.requester_agent,
            wi.target_agent,
            wi.capability_key,
            wi.human_owner,
            wi.priority,
            wi.brief_summary,
            wi.source_type,
            wi.source_refs,
            wi.risk_level,
            wi.review_policy,
            wi.payload,
            COUNT(a.id)::bigint AS artifact_count,
            COUNT(a.id) FILTER (WHERE a.review_status = 'pending')::bigint AS pending_artifact_count,
            COUNT(a.id) FILTER (WHERE a.review_status = 'approved')::bigint AS approved_artifact_count
        FROM candidate
        JOIN qintopia_agent_os.work_items wi ON wi.id = candidate.id
        LEFT JOIN qintopia_agent_os.artifacts a ON a.work_item_id = wi.id
        GROUP BY wi.id
        "#,
    )
    .bind(PROVIDER)
    .bind(work_item_id)
    .fetch_optional(&mut **tx)
    .await
    .context("lock work item by id for workbench mirror")?;
    match row {
        Some(row) => work_item_from_row_tx(tx, row).await.map(Some),
        None => Ok(None),
    }
}

async fn work_item_from_row(
    pool: &PgPool,
    row: sqlx::postgres::PgRow,
) -> Result<WorkbenchWorkItem> {
    let mut item = base_work_item_from_row(row)?;
    enrich_parent_status(pool, &mut item).await?;
    Ok(item)
}

async fn work_item_from_row_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    row: sqlx::postgres::PgRow,
) -> Result<WorkbenchWorkItem> {
    let mut item = base_work_item_from_row(row)?;
    enrich_parent_status_tx(tx, &mut item).await?;
    Ok(item)
}

fn base_work_item_from_row(row: sqlx::postgres::PgRow) -> Result<WorkbenchWorkItem> {
    Ok(WorkbenchWorkItem {
        id: row.try_get("id")?,
        work_item_type: row.try_get("work_item_type")?,
        status: row.try_get("status")?,
        requester_agent: row.try_get("requester_agent")?,
        target_agent: row.try_get("target_agent")?,
        capability_key: row.try_get("capability_key")?,
        human_owner: row.try_get("human_owner")?,
        priority: row.try_get("priority")?,
        brief_summary: row.try_get("brief_summary")?,
        source_type: row.try_get("source_type")?,
        source_refs: row.try_get("source_refs")?,
        risk_level: row.try_get("risk_level")?,
        review_policy: row.try_get("review_policy")?,
        payload: row.try_get("payload")?,
        child_status_refs: Vec::new(),
        current_blocking_point: None,
        artifact_count: row.try_get("artifact_count")?,
        pending_artifact_count: row.try_get("pending_artifact_count")?,
        approved_artifact_count: row.try_get("approved_artifact_count")?,
    })
}

async fn enrich_parent_status(pool: &PgPool, item: &mut WorkbenchWorkItem) -> Result<()> {
    let rows = sqlx::query(child_status_sql())
        .bind(item.id)
        .fetch_all(pool)
        .await
        .context("load child status refs for workbench mirror")?;
    apply_child_status_refs(item, rows)
}

async fn enrich_parent_status_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    item: &mut WorkbenchWorkItem,
) -> Result<()> {
    let rows = sqlx::query(child_status_sql())
        .bind(item.id)
        .fetch_all(&mut **tx)
        .await
        .context("load child status refs for workbench mirror")?;
    apply_child_status_refs(item, rows)
}

fn child_status_sql() -> &'static str {
    r#"
    SELECT
        wi.id,
        wi.work_item_type,
        wi.status,
        wi.capability_key,
        COUNT(a.id) FILTER (WHERE a.review_status = 'pending')::bigint AS pending_artifact_count,
        (
            SELECT count(*)::bigint
            FROM qintopia_agent_os.work_item_events e
            WHERE e.work_item_id = wi.id
              AND e.event_type = 'group_message_send_ready_recorded'
              AND e.data->>'send_executed' = 'false'
        ) AS send_ready_event_count
    FROM qintopia_agent_os.work_items wi
    LEFT JOIN qintopia_agent_os.artifacts a ON a.work_item_id = wi.id
    WHERE wi.parent_work_item_id = $1
    GROUP BY wi.id
    ORDER BY wi.created_at ASC
    "#
}

fn apply_child_status_refs(
    item: &mut WorkbenchWorkItem,
    rows: Vec<sqlx::postgres::PgRow>,
) -> Result<()> {
    let mut refs = Vec::new();
    for row in rows {
        let status: String = row.try_get("status")?;
        let work_item_type: String = row.try_get("work_item_type")?;
        let pending_artifact_count: i64 = row.try_get("pending_artifact_count")?;
        let send_ready_event_count: i64 = row.try_get("send_ready_event_count")?;
        let blocking_reason = blocking_reason_for(
            &status,
            &work_item_type,
            pending_artifact_count,
            send_ready_event_count,
        );
        refs.push(ChildStatusRef {
            work_item_id: row.try_get("id")?,
            work_item_type,
            status,
            capability_key: row.try_get("capability_key")?,
            blocking_reason,
        });
    }
    item.current_blocking_point = refs.iter().find_map(|child| {
        child
            .blocking_reason
            .as_ref()
            .map(|reason| format!("{}:{}", child.work_item_type, reason))
    });
    item.child_status_refs = refs;
    Ok(())
}

fn build_task_plan(item: &WorkbenchWorkItem) -> Result<TaskMirrorPlan> {
    validate_safe_item(item)?;
    let section = section_for_status(&item.status).to_string();
    let title = format!("[{}] {}", item.work_item_type, item.brief_summary);
    let description_fields = vec![
        "work_item_id".to_string(),
        "work_item_type".to_string(),
        "capability_key".to_string(),
        "requester_agent".to_string(),
        "target_agent".to_string(),
        "human_owner".to_string(),
        "source_refs".to_string(),
        "risk_level".to_string(),
        "review_policy".to_string(),
        "artifact_refs".to_string(),
        "child_status_refs".to_string(),
        "current_blocking_point".to_string(),
        "current_status".to_string(),
    ];
    let description = vec![
        format!("work_item_id: {}", item.id),
        format!("work_item_type: {}", item.work_item_type),
        format!("capability_key: {}", item.capability_key),
        format!("requester_agent: {}", item.requester_agent),
        format!("target_agent: {}", item.target_agent),
        format!("human_owner: {}", display_or_dash(&item.human_owner)),
        format!("priority: {}", item.priority),
        format!("source_type: {}", display_or_dash(&item.source_type)),
        format!("source_refs: {}", safe_source_refs(&item.source_refs)),
        format!("risk_level: {}", item.risk_level),
        format!("review_policy: {}", item.review_policy),
        format!("artifact_refs: {}", artifact_summary(item)),
        format!("child_status_refs: {}", child_status_summary(item)),
        format!(
            "current_blocking_point: {}",
            item.current_blocking_point.as_deref().unwrap_or("none")
        ),
        format!("current_status: {}", item.status),
    ]
    .join("\n");
    Ok(TaskMirrorPlan {
        title,
        section,
        description,
        description_fields,
        external_id: format!("agentos-work-item-{}", item.id),
    })
}

async fn upsert_workbench_ref(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    item: &WorkbenchWorkItem,
    plan: &TaskMirrorPlan,
) -> Result<Uuid> {
    let row = sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.human_workbench_refs
            (
                work_item_id,
                artifact_id,
                provider,
                external_id,
                external_url,
                display_title,
                status,
                metadata,
                last_synced_at
            )
        VALUES ($1, NULL, $2, $3, '', $4, 'active', $5, now())
        ON CONFLICT (provider, external_id)
        DO UPDATE SET
            display_title = EXCLUDED.display_title,
            metadata = qintopia_agent_os.human_workbench_refs.metadata || EXCLUDED.metadata,
            last_synced_at = now(),
            updated_at = now()
        RETURNING id
        "#,
    )
    .bind(item.id)
    .bind(PROVIDER)
    .bind(&plan.external_id)
    .bind(&plan.title)
    .bind(json!({
    "tasklist_name": TASKLIST_NAME,
    "task_section": plan.section,
    "description": plan.description,
        "description_fields": plan.description_fields,
        "dry_run_only": true,
        "current_blocking_point": item.current_blocking_point,
        "child_status_count": item.child_status_refs.len(),
        "sensitive_fields_redacted": true,
    }))
    .fetch_one(&mut **tx)
    .await
    .context("upsert workbench ref")?;
    Ok(row.get("id"))
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
    .context("append workbench mirror event")?;
    Ok(())
}

fn validate_safe_item(item: &WorkbenchWorkItem) -> Result<()> {
    if contains_sensitive_text(&item.brief_summary)
        || contains_sensitive_value(&item.source_refs)
        || contains_sensitive_value(&item.payload)
    {
        bail!("work item contains content that cannot be mirrored to Feishu Task");
    }
    Ok(())
}

fn section_for_status(status: &str) -> &str {
    match status {
        "queued" => "待处理",
        "processing" => "执行中",
        "awaiting_review" => "待审核",
        "awaiting_publish" => "待发送/待发布",
        "failed" => "失败待处理",
        "cancelled" => "已取消",
        "completed" => "已完成",
        _ => "待处理",
    }
}

fn safe_source_refs(value: &Value) -> String {
    value
        .as_object()
        .map(|map| {
            map.iter()
                .filter(|(key, _)| !contains_sensitive_key(key))
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|item| !item.is_empty())
        .unwrap_or_else(|| "none".to_string())
}

fn artifact_summary(item: &WorkbenchWorkItem) -> String {
    format!(
        "total={}, pending={}, approved={}",
        item.artifact_count, item.pending_artifact_count, item.approved_artifact_count
    )
}

fn child_status_summary(item: &WorkbenchWorkItem) -> String {
    if item.child_status_refs.is_empty() {
        return "none".to_string();
    }
    item.child_status_refs
        .iter()
        .map(|child| {
            format!(
                "{}:{}:{}:{}:{}",
                child.work_item_id,
                child.work_item_type,
                child.status,
                child.capability_key,
                child.blocking_reason.as_deref().unwrap_or("none")
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn blocking_reason_for(
    status: &str,
    work_item_type: &str,
    pending_artifact_count: i64,
    send_ready_event_count: i64,
) -> Option<String> {
    if status == "failed" {
        return Some("failed_requires_human_or_worker_retry".to_string());
    }
    if status == "awaiting_publish" {
        return Some("waiting_for_human_final_confirmation".to_string());
    }
    if pending_artifact_count > 0 {
        return Some("waiting_for_artifact_review".to_string());
    }
    if status == "awaiting_review" {
        return Some("waiting_for_review_or_next_step".to_string());
    }
    if work_item_type == "group_message_request" && status == "queued" && send_ready_event_count > 0
    {
        return Some("send_ready_waiting_for_production_send_adapter".to_string());
    }
    if status == "queued" {
        return Some("waiting_for_worker".to_string());
    }
    if status == "processing" {
        return Some("worker_processing_or_claim_expiry".to_string());
    }
    None
}

fn display_or_dash(value: &str) -> String {
    if value.trim().is_empty() {
        "-".to_string()
    } else {
        value.trim().to_string()
    }
}

fn report_from_plan(
    dry_run: bool,
    apply_requested: bool,
    fixture_mode: bool,
    action_status: &str,
    work_item_id: Option<Uuid>,
    ref_id: Option<String>,
    plan: &TaskMirrorPlan,
) -> WorkbenchMirrorReport {
    WorkbenchMirrorReport {
        success: true,
        dry_run,
        apply_requested,
        fixture_mode,
        worker: WORKER_ID,
        action_status: action_status.to_string(),
        work_item_id,
        provider: PROVIDER,
        intended_tasklist_name: TASKLIST_NAME,
        task_title: plan.title.clone(),
        task_section: plan.section.clone(),
        description: plan.description.clone(),
        description_fields: plan.description_fields.clone(),
        external_id: ref_id.or_else(|| Some(plan.external_id.clone())),
        sensitive_fields_redacted: true,
        limitations: limitations(),
        guardrails: guardrails(),
    }
}

fn empty_report(
    fixture_mode: bool,
    apply_requested: bool,
    action_status: &str,
) -> WorkbenchMirrorReport {
    WorkbenchMirrorReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        fixture_mode,
        worker: WORKER_ID,
        action_status: action_status.to_string(),
        work_item_id: None,
        provider: PROVIDER,
        intended_tasklist_name: TASKLIST_NAME,
        task_title: String::new(),
        task_section: String::new(),
        description: String::new(),
        description_fields: Vec::new(),
        external_id: None,
        sensitive_fields_redacted: true,
        limitations: vec!["no mirrorable work item was found".to_string()],
        guardrails: guardrails(),
    }
}

fn limitations() -> Vec<String> {
    vec![
        "this worker does not call Feishu Task APIs".to_string(),
        "apply mode records a dry-run human_workbench_refs row only".to_string(),
        "Feishu Task comments, sections, and review sync require a separate worker".to_string(),
    ]
}

fn guardrails() -> Vec<String> {
    vec![
        "Postgres remains the system source of truth".to_string(),
        "Feishu Task descriptions include only allowlisted fields".to_string(),
        "tokens, Base table ids, raw private text, and internal prompts are rejected".to_string(),
        "human edits in Feishu must be validated by a sync worker before writing Postgres"
            .to_string(),
    ]
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

    #[test]
    fn fixture_mirror_uses_allowlisted_fields() {
        let report = run_fixture(false).expect("fixture should validate");

        assert_eq!(report.action_status, "fixture_dry_run_ok");
        assert_eq!(report.task_section, "待审核");
        assert!(report.description.contains("work_item_id"));
        assert!(report.description.contains("artifact_refs"));
        assert!(!report.description.contains("payload"));
        assert!(report.sensitive_fields_redacted);
    }

    #[test]
    fn section_maps_awaiting_publish_to_publish_bucket() {
        assert_eq!(section_for_status("awaiting_publish"), "待发送/待发布");
    }

    #[test]
    fn mirror_rejects_sensitive_payload() {
        let item = WorkbenchWorkItem {
            id: Uuid::nil(),
            work_item_type: "visual_asset_request".to_string(),
            status: "queued".to_string(),
            requester_agent: "xiaoman".to_string(),
            target_agent: "huabaosi".to_string(),
            capability_key: "huabaosi.create_visual_asset".to_string(),
            human_owner: String::new(),
            priority: "normal".to_string(),
            brief_summary: "周末活动".to_string(),
            source_type: "test".to_string(),
            source_refs: json!({}),
            risk_level: "medium".to_string(),
            review_policy: "before_external_use".to_string(),
            payload: json!({"app_token": "secret"}),
            child_status_refs: Vec::new(),
            current_blocking_point: None,
            artifact_count: 0,
            pending_artifact_count: 0,
            approved_artifact_count: 0,
        };

        let err = build_task_plan(&item).expect_err("sensitive payload should be rejected");
        assert!(err.to_string().contains("cannot be mirrored"));
    }
}
