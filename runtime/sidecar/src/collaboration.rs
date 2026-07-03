use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Row};
use uuid::Uuid;

use crate::{config::Cli, db};

const WORKER_ID: &str = "collaboration-worker";
const SUPPORTED_WORK_ITEM_TYPE: &str = "visual_asset_request";
const SUPPORTED_CAPABILITY: &str = "huabaosi.create_visual_asset";

#[derive(Debug, Serialize)]
pub struct CollaborationWorkerReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub fixture_mode: bool,
    pub worker: &'static str,
    pub action_status: String,
    pub work_item_id: Option<Uuid>,
    pub artifact_ids: Vec<Uuid>,
    pub artifact_previews: Vec<ArtifactPreview>,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactPreview {
    pub artifact_type: String,
    pub title: String,
    pub summary: String,
    pub review_status: String,
    pub content_hash: String,
}

#[derive(Debug, Clone)]
struct WorkItem {
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
struct ArtifactDraft {
    artifact_type: String,
    title: String,
    summary: String,
    content_text: String,
    content_hash: String,
    review_status: String,
}

pub async fn run(
    cli: &Cli,
    work_item_type: String,
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
        bail!("collaboration worker currently supports --once only");
    }
    if work_item_type != SUPPORTED_WORK_ITEM_TYPE {
        bail!("work_item_type is not supported by this worker");
    }

    let report = if fixture_mode {
        run_fixture(apply && !dry_run)?
    } else {
        let database_url = cli.database_url_required()?;
        let pool = db::connect(database_url, cli.db_max_connections).await?;
        run_once(&pool, apply && !dry_run, work_item_id).await?
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn run_fixture(apply_requested: bool) -> Result<CollaborationWorkerReport> {
    if apply_requested {
        bail!("fixture-mode cannot be used with --apply");
    }
    let work_item = WorkItem {
        id: Uuid::nil(),
        work_item_type: SUPPORTED_WORK_ITEM_TYPE.to_string(),
        requester_agent: "xiaoman".to_string(),
        target_agent: "huabaosi".to_string(),
        capability_key: SUPPORTED_CAPABILITY.to_string(),
        brief_summary: "周末共创晚餐活动运营海报".to_string(),
        source_refs: json!({"source_record_ref": "activity_occurrence:fixture"}),
        payload: json!({"handoff_type": "visual_asset_request"}),
        review_policy: "before_external_use".to_string(),
    };
    validate_work_item(&work_item)?;
    let drafts = build_artifact_drafts(&work_item)?;
    Ok(report_from_drafts(
        true,
        false,
        true,
        "fixture_dry_run_ok",
        None,
        Vec::new(),
        &drafts,
    ))
}

async fn run_once(
    pool: &PgPool,
    apply_requested: bool,
    work_item_id: Option<Uuid>,
) -> Result<CollaborationWorkerReport> {
    if !apply_requested {
        let Some(work_item) = peek_work_item(pool, work_item_id).await? else {
            return Ok(empty_report(false, false, "no_claimable_work_item"));
        };
        validate_work_item(&work_item)?;
        let drafts = build_artifact_drafts(&work_item)?;
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

    let mut tx = pool
        .begin()
        .await
        .context("begin collaboration transaction")?;
    let Some(work_item) = claim_work_item(&mut tx, work_item_id).await? else {
        tx.commit()
            .await
            .context("commit no-op collaboration transaction")?;
        return Ok(empty_report(false, true, "no_claimable_work_item"));
    };
    if let Err(err) = validate_work_item(&work_item) {
        mark_work_item_failed(&mut tx, &work_item, &err.to_string()).await?;
        tx.commit()
            .await
            .context("commit failed validation collaboration transaction")?;
        return Err(err);
    }
    let drafts = match build_artifact_drafts(&work_item) {
        Ok(drafts) => drafts,
        Err(err) => {
            mark_work_item_failed(&mut tx, &work_item, &err.to_string()).await?;
            tx.commit()
                .await
                .context("commit failed artifact collaboration transaction")?;
            return Err(err);
        }
    };
    let mut artifact_ids = Vec::new();
    for draft in &drafts {
        artifact_ids.push(upsert_artifact(&mut tx, &work_item, draft).await?);
    }
    update_work_item_after_artifacts(&mut tx, &work_item).await?;
    append_event_in_tx(
        &mut tx,
        Some(work_item.id),
        None,
        "artifact_created",
        "worker",
        WORKER_ID,
        "visual asset artifacts created by collaboration worker",
        json!({
            "artifact_count": drafts.len(),
            "review_policy": work_item.review_policy,
            "target_agent": work_item.target_agent,
            "capability_key": work_item.capability_key,
        }),
    )
    .await?;
    tx.commit()
        .await
        .context("commit collaboration transaction")?;

    Ok(report_from_drafts(
        false,
        true,
        false,
        "artifacts_created",
        Some(work_item.id),
        artifact_ids,
        &drafts,
    ))
}

async fn peek_work_item(pool: &PgPool, work_item_id: Option<Uuid>) -> Result<Option<WorkItem>> {
    if let Some(work_item_id) = work_item_id {
        return peek_work_item_by_id(pool, work_item_id).await;
    }
    peek_next_work_item(pool).await
}

async fn peek_next_work_item(pool: &PgPool) -> Result<Option<WorkItem>> {
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
    .context("peek next collaboration work item")?;
    row.map(work_item_from_row).transpose()
}

async fn peek_work_item_by_id(pool: &PgPool, work_item_id: Uuid) -> Result<Option<WorkItem>> {
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
    .context("peek collaboration work item by id")?;
    row.map(work_item_from_row).transpose()
}

async fn claim_work_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Option<Uuid>,
) -> Result<Option<WorkItem>> {
    if let Some(work_item_id) = work_item_id {
        return claim_work_item_by_id(tx, work_item_id).await;
    }
    claim_next_work_item(tx).await
}

async fn claim_next_work_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<Option<WorkItem>> {
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
    .context("claim collaboration work item")?;
    row.map(work_item_from_row).transpose()
}

async fn claim_work_item_by_id(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Uuid,
) -> Result<Option<WorkItem>> {
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
    .context("claim collaboration work item by id")?;
    row.map(work_item_from_row).transpose()
}

fn work_item_from_row(row: sqlx::postgres::PgRow) -> Result<WorkItem> {
    Ok(WorkItem {
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

fn validate_work_item(work_item: &WorkItem) -> Result<()> {
    if work_item.work_item_type != SUPPORTED_WORK_ITEM_TYPE {
        bail!("work item type is not supported");
    }
    if work_item.capability_key != SUPPORTED_CAPABILITY {
        bail!("capability is not supported by this worker");
    }
    if work_item.requester_agent != "xiaoman" {
        bail!("requester_agent is not allowed for visual collaboration");
    }
    if work_item.target_agent != "huabaosi" {
        bail!("target_agent must be huabaosi");
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

fn build_artifact_drafts(work_item: &WorkItem) -> Result<Vec<ArtifactDraft>> {
    let title = format!("{} - 视觉素材 brief", work_item.brief_summary);
    let summary = format!("面向活动运营的视觉素材草稿：{}", work_item.brief_summary);
    let content_text = format!(
        "海报目标：{}\n画面方向：温暖、清晰、适合社区活动报名提醒。\n文案重点：活动主题、时间地点、报名提醒、审核后再外部使用。\n来源引用：{}",
        work_item.brief_summary,
        safe_source_refs(&work_item.source_refs)
    );
    let content_hash = content_hash(&format!(
        "{}|{}|{}",
        work_item.id, "poster_brief", content_text
    ));
    Ok(vec![ArtifactDraft {
        artifact_type: "poster_brief".to_string(),
        title,
        summary,
        content_text,
        content_hash,
        review_status: "pending".to_string(),
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
    work_item: &WorkItem,
    draft: &ArtifactDraft,
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
                metadata,
                review_requested_at
            )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, ARRAY['external_use_review_required']::text[], 'internal_ops', $10, now())
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
    .bind(json!({
        "generated_by": WORKER_ID,
        "capability_key": work_item.capability_key,
        "review_policy": work_item.review_policy,
    }))
    .fetch_one(&mut **tx)
    .await
    .context("upsert collaboration artifact")?;
    Ok(row.get("id"))
}

async fn update_work_item_after_artifacts(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item: &WorkItem,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET
            status = 'awaiting_review',
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
    .context("mark work item awaiting review")?;
    Ok(())
}

async fn mark_work_item_failed(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item: &WorkItem,
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
    .context("mark collaboration work item failed")?;
    append_event_in_tx(
        tx,
        Some(work_item.id),
        None,
        "failed",
        "worker",
        WORKER_ID,
        "collaboration worker failed to create artifacts",
        json!({"error": message}),
    )
    .await?;
    Ok(())
}

async fn append_event_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Option<Uuid>,
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
    .context("append collaboration event")?;
    Ok(())
}

fn report_from_drafts(
    dry_run: bool,
    apply_requested: bool,
    fixture_mode: bool,
    action_status: &str,
    work_item_id: Option<Uuid>,
    artifact_ids: Vec<Uuid>,
    drafts: &[ArtifactDraft],
) -> CollaborationWorkerReport {
    CollaborationWorkerReport {
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
            .map(|draft| ArtifactPreview {
                artifact_type: draft.artifact_type.clone(),
                title: draft.title.clone(),
                summary: draft.summary.clone(),
                review_status: draft.review_status.clone(),
                content_hash: draft.content_hash.clone(),
            })
            .collect(),
        limitations: vec![
            "fixture-mode and dry-run do not call Huabaosi or write Feishu Tasks".to_string(),
            "this worker creates draft artifacts only; external publishing is a separate reviewed capability".to_string(),
        ],
        guardrails: vec![
            "only visual_asset_request with huabaosi.create_visual_asset is supported".to_string(),
            "Hermes Kanban is not read or written".to_string(),
            "artifacts default to pending review before external use".to_string(),
        ],
    }
}

fn empty_report(
    fixture_mode: bool,
    apply_requested: bool,
    action_status: &str,
) -> CollaborationWorkerReport {
    CollaborationWorkerReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        fixture_mode,
        worker: WORKER_ID,
        action_status: action_status.to_string(),
        work_item_id: None,
        artifact_ids: Vec::new(),
        artifact_previews: Vec::new(),
        limitations: vec!["no claimable work item was found".to_string()],
        guardrails: vec![
            "only visual_asset_request with huabaosi.create_visual_asset is supported".to_string(),
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
    fn fixture_mode_generates_pending_poster_brief() {
        let report = run_fixture(false).expect("fixture should validate");

        assert_eq!(report.action_status, "fixture_dry_run_ok");
        assert_eq!(report.artifact_previews.len(), 1);
        assert_eq!(report.artifact_previews[0].artifact_type, "poster_brief");
        assert_eq!(report.artifact_previews[0].review_status, "pending");
        assert!(report.artifact_previews[0]
            .content_hash
            .starts_with("sha256:"));
    }

    #[test]
    fn rejects_sensitive_work_item_content() {
        let work_item = WorkItem {
            id: Uuid::nil(),
            work_item_type: SUPPORTED_WORK_ITEM_TYPE.to_string(),
            requester_agent: "xiaoman".to_string(),
            target_agent: "huabaosi".to_string(),
            capability_key: SUPPORTED_CAPABILITY.to_string(),
            brief_summary: "contains app_token".to_string(),
            source_refs: json!({}),
            payload: json!({}),
            review_policy: "before_external_use".to_string(),
        };

        let err = validate_work_item(&work_item).expect_err("sensitive content should fail");
        assert!(err
            .to_string()
            .contains("disallowed sensitive or raw internal content"));
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
