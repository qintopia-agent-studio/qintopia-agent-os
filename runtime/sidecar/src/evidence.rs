use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Row};
use uuid::Uuid;

use crate::{config::Cli, db};

const WORKER_ID: &str = "evidence-worker";
const SUPPORTED_WORK_ITEM_TYPE: &str = "evidence_request";
const SUPPORTED_CAPABILITY: &str = "wenyuange.retrieve_evidence";

#[derive(Debug, Serialize)]
pub struct EvidenceWorkerReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub fixture_mode: bool,
    pub worker: &'static str,
    pub action_status: String,
    pub work_item_id: Option<Uuid>,
    pub artifact_ids: Vec<Uuid>,
    pub artifact_previews: Vec<EvidenceArtifactPreview>,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvidenceArtifactPreview {
    pub artifact_type: String,
    pub title: String,
    pub summary: String,
    pub review_status: String,
    pub information_class: String,
    pub content_hash: String,
}

#[derive(Debug, Clone)]
struct EvidenceWorkItem {
    id: Uuid,
    work_item_type: String,
    requester_agent: String,
    target_agent: String,
    capability_key: String,
    brief_summary: String,
    source_refs: Value,
    payload: Value,
    review_policy: String,
}

#[derive(Debug, Clone)]
struct EvidenceDraft {
    artifact_type: String,
    title: String,
    summary: String,
    content_text: String,
    content_hash: String,
    review_status: String,
    information_class: String,
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
        bail!("evidence worker currently supports --once only");
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

fn run_fixture(apply_requested: bool) -> Result<EvidenceWorkerReport> {
    if apply_requested {
        bail!("fixture-mode cannot be used with --apply");
    }
    let work_item = EvidenceWorkItem {
        id: Uuid::nil(),
        work_item_type: SUPPORTED_WORK_ITEM_TYPE.to_string(),
        requester_agent: "xiaoman".to_string(),
        target_agent: "wenyuange".to_string(),
        capability_key: SUPPORTED_CAPABILITY.to_string(),
        brief_summary: "周末共创晚餐活动宣发背景资料".to_string(),
        source_refs: json!({"source_record_ref": "activity_occurrence:fixture"}),
        payload: json!({"question": "请整理活动宣发所需的背景资料和可引用事实"}),
        review_policy: "not_required".to_string(),
    };
    validate_work_item(&work_item)?;
    let drafts = build_evidence_drafts(&work_item)?;
    Ok(report_from_drafts(
        true,
        false,
        true,
        "fixture_dry_run_ok",
        Some(work_item.id),
        Vec::new(),
        &drafts,
    ))
}

async fn run_once(
    pool: &PgPool,
    apply_requested: bool,
    work_item_id: Option<Uuid>,
) -> Result<EvidenceWorkerReport> {
    if !apply_requested {
        let Some(work_item) = peek_work_item(pool, work_item_id).await? else {
            return Ok(empty_report(false, false, "no_claimable_evidence_request"));
        };
        validate_work_item(&work_item)?;
        let drafts = build_evidence_drafts(&work_item)?;
        return Ok(report_from_drafts(
            false,
            false,
            false,
            "dry_run_ok",
            Some(work_item.id),
            Vec::new(),
            &drafts,
        ));
    }

    let mut tx = pool.begin().await.context("begin evidence transaction")?;
    let Some(work_item) = claim_work_item(&mut tx, work_item_id).await? else {
        tx.commit()
            .await
            .context("commit no-op evidence transaction")?;
        return Ok(empty_report(false, true, "no_claimable_evidence_request"));
    };
    if let Err(err) = validate_work_item(&work_item) {
        mark_work_item_failed(&mut tx, &work_item, &err.to_string()).await?;
        tx.commit()
            .await
            .context("commit failed validation evidence transaction")?;
        return Err(err);
    }
    let drafts = match build_evidence_drafts(&work_item) {
        Ok(drafts) => drafts,
        Err(err) => {
            mark_work_item_failed(&mut tx, &work_item, &err.to_string()).await?;
            tx.commit()
                .await
                .context("commit failed draft evidence transaction")?;
            return Err(err);
        }
    };
    let mut artifact_ids = Vec::new();
    for draft in &drafts {
        artifact_ids.push(upsert_artifact(&mut tx, &work_item, draft).await?);
    }
    update_work_item_completed(&mut tx, &work_item).await?;
    append_event_in_tx(
        &mut tx,
        WorkItemEvent {
            work_item_id: Some(work_item.id),
            artifact_id: None,
            event_type: "evidence_artifact_created",
            actor_type: "worker",
            actor_id: WORKER_ID,
            message: "evidence summary artifact created by evidence worker",
            data: json!({
                "artifact_count": drafts.len(),
                "review_policy": work_item.review_policy,
                "target_agent": work_item.target_agent,
                "capability_key": work_item.capability_key,
                "external_calls_executed": false,
            }),
        },
    )
    .await?;
    tx.commit().await.context("commit evidence transaction")?;

    Ok(report_from_drafts(
        false,
        true,
        false,
        "evidence_artifact_created",
        Some(work_item.id),
        artifact_ids,
        &drafts,
    ))
}

async fn peek_work_item(
    pool: &PgPool,
    work_item_id: Option<Uuid>,
) -> Result<Option<EvidenceWorkItem>> {
    if let Some(work_item_id) = work_item_id {
        return peek_work_item_by_id(pool, work_item_id).await;
    }
    peek_next_work_item(pool).await
}

async fn peek_next_work_item(pool: &PgPool) -> Result<Option<EvidenceWorkItem>> {
    let row = sqlx::query(
        r#"
        SELECT id, work_item_type, requester_agent, target_agent, capability_key,
               brief_summary, source_refs, payload, review_policy
        FROM qintopia_agent_os.work_items
        WHERE status = 'queued'
          AND available_at <= now()
          AND work_item_type = $1
          AND capability_key = $2
        ORDER BY priority DESC, available_at ASC, created_at ASC
        LIMIT 1
        "#,
    )
    .bind(SUPPORTED_WORK_ITEM_TYPE)
    .bind(SUPPORTED_CAPABILITY)
    .fetch_optional(pool)
    .await
    .context("peek next evidence work item")?;
    row.map(work_item_from_row).transpose()
}

async fn peek_work_item_by_id(
    pool: &PgPool,
    work_item_id: Uuid,
) -> Result<Option<EvidenceWorkItem>> {
    let row = sqlx::query(
        r#"
        SELECT id, work_item_type, requester_agent, target_agent, capability_key,
               brief_summary, source_refs, payload, review_policy
        FROM qintopia_agent_os.work_items
        WHERE id = $1
          AND status = 'queued'
          AND available_at <= now()
          AND work_item_type = $2
          AND capability_key = $3
        LIMIT 1
        "#,
    )
    .bind(work_item_id)
    .bind(SUPPORTED_WORK_ITEM_TYPE)
    .bind(SUPPORTED_CAPABILITY)
    .fetch_optional(pool)
    .await
    .context("peek evidence work item by id")?;
    row.map(work_item_from_row).transpose()
}

async fn claim_work_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Option<Uuid>,
) -> Result<Option<EvidenceWorkItem>> {
    if let Some(work_item_id) = work_item_id {
        return claim_work_item_by_id(tx, work_item_id).await;
    }
    claim_next_work_item(tx).await
}

async fn claim_next_work_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<Option<EvidenceWorkItem>> {
    let row = sqlx::query(
        r#"
        WITH claimed AS (
            SELECT id
            FROM qintopia_agent_os.work_items
            WHERE (
                  (status = 'queued' AND available_at <= now())
                  OR (status = 'processing' AND claim_expires_at <= now())
              )
              AND work_item_type = $1
              AND capability_key = $2
            ORDER BY priority DESC, available_at ASC, created_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE qintopia_agent_os.work_items items
        SET
            status = 'processing',
            claimed_by = $3,
            locked_at = now(),
            claim_expires_at = now() + interval '10 minutes',
            attempts = attempts + 1,
            updated_at = now()
        FROM claimed
        WHERE items.id = claimed.id
        RETURNING items.id, items.work_item_type, items.requester_agent,
                  items.target_agent, items.capability_key, items.brief_summary,
                  items.source_refs, items.payload, items.review_policy
        "#,
    )
    .bind(SUPPORTED_WORK_ITEM_TYPE)
    .bind(SUPPORTED_CAPABILITY)
    .bind(WORKER_ID)
    .fetch_optional(&mut **tx)
    .await
    .context("claim evidence work item")?;
    row.map(work_item_from_row).transpose()
}

async fn claim_work_item_by_id(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Uuid,
) -> Result<Option<EvidenceWorkItem>> {
    let row = sqlx::query(
        r#"
        WITH claimed AS (
            SELECT id
            FROM qintopia_agent_os.work_items
            WHERE id = $1
              AND (
                  (status = 'queued' AND available_at <= now())
                  OR (status = 'processing' AND claim_expires_at <= now())
              )
              AND work_item_type = $2
              AND capability_key = $3
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE qintopia_agent_os.work_items items
        SET
            status = 'processing',
            claimed_by = $4,
            locked_at = now(),
            claim_expires_at = now() + interval '10 minutes',
            attempts = attempts + 1,
            updated_at = now()
        FROM claimed
        WHERE items.id = claimed.id
        RETURNING items.id, items.work_item_type, items.requester_agent,
                  items.target_agent, items.capability_key, items.brief_summary,
                  items.source_refs, items.payload, items.review_policy
        "#,
    )
    .bind(work_item_id)
    .bind(SUPPORTED_WORK_ITEM_TYPE)
    .bind(SUPPORTED_CAPABILITY)
    .bind(WORKER_ID)
    .fetch_optional(&mut **tx)
    .await
    .context("claim evidence work item by id")?;
    row.map(work_item_from_row).transpose()
}

fn work_item_from_row(row: sqlx::postgres::PgRow) -> Result<EvidenceWorkItem> {
    Ok(EvidenceWorkItem {
        id: row.try_get("id")?,
        work_item_type: row.try_get("work_item_type")?,
        requester_agent: row.try_get("requester_agent")?,
        target_agent: row.try_get("target_agent")?,
        capability_key: row.try_get("capability_key")?,
        brief_summary: row.try_get("brief_summary")?,
        source_refs: row.try_get("source_refs")?,
        payload: row.try_get("payload")?,
        review_policy: row.try_get("review_policy")?,
    })
}

fn validate_work_item(work_item: &EvidenceWorkItem) -> Result<()> {
    if work_item.work_item_type != SUPPORTED_WORK_ITEM_TYPE {
        bail!("work item type is not supported by evidence worker");
    }
    if work_item.capability_key != SUPPORTED_CAPABILITY {
        bail!("capability is not supported by evidence worker");
    }
    if !["xiaoman", "huabaosi", "silaoshi", "default"].contains(&work_item.requester_agent.as_str())
    {
        bail!("requester_agent is not allowed for evidence retrieval");
    }
    if work_item.target_agent != "wenyuange" {
        bail!("target_agent must be wenyuange");
    }
    if work_item.review_policy != "not_required" {
        bail!("evidence retrieval must use review_policy=not_required");
    }
    let combined = format!(
        "{} {} {}",
        work_item.brief_summary, work_item.source_refs, work_item.payload
    );
    if contains_sensitive_text(&combined) {
        bail!("work item contains disallowed sensitive or raw internal content");
    }
    Ok(())
}

fn build_evidence_drafts(work_item: &EvidenceWorkItem) -> Result<Vec<EvidenceDraft>> {
    let question = work_item
        .payload
        .get("question")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&work_item.brief_summary);
    let title = format!("{} - 运营证据摘要", work_item.brief_summary);
    let summary = format!("围绕「{}」整理的运营背景证据摘要。", question);
    let content_text = format!(
        "检索问题：{}\n证据范围：只允许使用已授权的知识库、消息证据或公开资料摘要；当前 worker 不执行外部检索。\n输出用途：为后续视觉素材、运营文案或人工审核提供背景依据。\n来源引用：{}",
        question,
        safe_source_refs(&work_item.source_refs)
    );
    let content_hash = content_hash(&format!(
        "{}|{}|{}",
        work_item.id, "evidence_summary", content_text
    ));
    Ok(vec![EvidenceDraft {
        artifact_type: "evidence_summary".to_string(),
        title,
        summary,
        content_text,
        content_hash,
        review_status: "not_required".to_string(),
        information_class: "internal_ops".to_string(),
    }])
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

async fn upsert_artifact(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item: &EvidenceWorkItem,
    draft: &EvidenceDraft,
) -> Result<Uuid> {
    let row = sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.artifacts
            (
                work_item_id,
                artifact_type,
                review_status,
                created_by_agent,
                title,
                summary,
                content_text,
                content_hash,
                source_ids,
                risk_labels,
                information_class,
                metadata
            )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, ARRAY['read_only_evidence']::text[], $10, $11)
        ON CONFLICT (work_item_id, content_hash) WHERE content_hash IS NOT NULL AND content_hash <> ''
        DO UPDATE SET
            summary = EXCLUDED.summary,
            metadata = qintopia_agent_os.artifacts.metadata || EXCLUDED.metadata,
            updated_at = now()
        RETURNING id
        "#,
    )
    .bind(work_item.id)
    .bind(&draft.artifact_type)
    .bind(&draft.review_status)
    .bind(&work_item.target_agent)
    .bind(&draft.title)
    .bind(&draft.summary)
    .bind(&draft.content_text)
    .bind(&draft.content_hash)
    .bind(json!([{"source_refs": work_item.source_refs}]))
    .bind(&draft.information_class)
    .bind(json!({
        "generated_by": WORKER_ID,
        "capability_key": work_item.capability_key,
        "review_policy": work_item.review_policy,
        "external_calls_executed": false,
    }))
    .fetch_one(&mut **tx)
    .await
    .context("upsert evidence artifact")?;
    Ok(row.get("id"))
}

async fn update_work_item_completed(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item: &EvidenceWorkItem,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET
            status = 'completed',
            locked_at = NULL,
            claim_expires_at = NULL,
            last_error = NULL,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(work_item.id)
    .execute(&mut **tx)
    .await
    .context("mark evidence work item completed")?;
    Ok(())
}

async fn mark_work_item_failed(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item: &EvidenceWorkItem,
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
    .context("mark evidence work item failed")?;
    append_event_in_tx(
        tx,
        WorkItemEvent {
            work_item_id: Some(work_item.id),
            artifact_id: None,
            event_type: "failed",
            actor_type: "worker",
            actor_id: WORKER_ID,
            message: "evidence worker failed to create artifacts",
            data: json!({"error": message}),
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
    .context("append evidence event")?;
    Ok(())
}

fn report_from_drafts(
    dry_run: bool,
    apply_requested: bool,
    fixture_mode: bool,
    action_status: &str,
    work_item_id: Option<Uuid>,
    artifact_ids: Vec<Uuid>,
    drafts: &[EvidenceDraft],
) -> EvidenceWorkerReport {
    EvidenceWorkerReport {
        success: true,
        dry_run,
        apply_requested,
        fixture_mode,
        worker: WORKER_ID,
        action_status: action_status.to_string(),
        work_item_id,
        artifact_ids,
        artifact_previews: drafts
            .iter()
            .map(|draft| EvidenceArtifactPreview {
                artifact_type: draft.artifact_type.clone(),
                title: draft.title.clone(),
                summary: draft.summary.clone(),
                review_status: draft.review_status.clone(),
                information_class: draft.information_class.clone(),
                content_hash: draft.content_hash.clone(),
            })
            .collect(),
        limitations: vec![
            "fixture-mode and dry-run do not query the live message store or call Feishu"
                .to_string(),
            "this worker creates a source-grounding artifact only; it does not mutate business records"
                .to_string(),
        ],
        guardrails: vec![
            "only evidence_request with wenyuange.retrieve_evidence is supported".to_string(),
            "Hermes Kanban is not read or written".to_string(),
            "evidence artifacts are internal operations context and are not external-send artifacts"
                .to_string(),
        ],
    }
}

fn empty_report(
    fixture_mode: bool,
    apply_requested: bool,
    action_status: &str,
) -> EvidenceWorkerReport {
    EvidenceWorkerReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        fixture_mode,
        worker: WORKER_ID,
        action_status: action_status.to_string(),
        work_item_id: None,
        artifact_ids: Vec::new(),
        artifact_previews: Vec::new(),
        limitations: vec!["no claimable evidence work item was found".to_string()],
        guardrails: vec![
            "only evidence_request with wenyuange.retrieve_evidence is supported".to_string(),
            "Hermes Kanban is not read or written".to_string(),
        ],
    }
}

fn content_hash(text: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(text.as_bytes()))
}

fn trim_error(error: &str) -> String {
    const MAX: usize = 500;
    let trimmed = error.trim();
    if trimmed.chars().count() <= MAX {
        return trimmed.to_string();
    }
    trimmed.chars().take(MAX).collect()
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
        "table_id",
        "base_token",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_mode_generates_not_required_evidence_summary() {
        let report = run_fixture(false).expect("fixture should validate");

        assert_eq!(report.action_status, "fixture_dry_run_ok");
        assert_eq!(report.artifact_previews.len(), 1);
        assert_eq!(
            report.artifact_previews[0].artifact_type,
            "evidence_summary"
        );
        assert_eq!(report.artifact_previews[0].review_status, "not_required");
        assert_eq!(
            report.artifact_previews[0].information_class,
            "internal_ops"
        );
        assert!(report.artifact_previews[0]
            .content_hash
            .starts_with("sha256:"));
    }

    #[test]
    fn rejects_sensitive_evidence_content() {
        let work_item = EvidenceWorkItem {
            id: Uuid::nil(),
            work_item_type: SUPPORTED_WORK_ITEM_TYPE.to_string(),
            requester_agent: "xiaoman".to_string(),
            target_agent: "wenyuange".to_string(),
            capability_key: SUPPORTED_CAPABILITY.to_string(),
            brief_summary: "contains raw private chat".to_string(),
            source_refs: json!({}),
            payload: json!({"question": "请整理"}),
            review_policy: "not_required".to_string(),
        };

        let err = validate_work_item(&work_item).expect_err("sensitive content should fail");
        assert!(err
            .to_string()
            .contains("disallowed sensitive or raw internal content"));
    }

    #[test]
    fn requires_wenyuange_target() {
        let work_item = EvidenceWorkItem {
            id: Uuid::nil(),
            work_item_type: SUPPORTED_WORK_ITEM_TYPE.to_string(),
            requester_agent: "xiaoman".to_string(),
            target_agent: "huabaosi".to_string(),
            capability_key: SUPPORTED_CAPABILITY.to_string(),
            brief_summary: "活动背景".to_string(),
            source_refs: json!({}),
            payload: json!({"question": "请整理"}),
            review_policy: "not_required".to_string(),
        };

        let err = validate_work_item(&work_item).expect_err("wrong target should fail");
        assert!(err.to_string().contains("target_agent must be wenyuange"));
    }

    #[test]
    fn safe_source_refs_redacts_sensitive_keys() {
        let refs = safe_source_refs(&json!({
            "source_record_ref": "activity_occurrence:abc",
            "table_id": "tbl_secret"
        }));

        assert!(refs.contains("source_record_ref"));
        assert!(!refs.contains("tbl_secret"));
        assert!(!refs.contains("table_id"));
    }
}
