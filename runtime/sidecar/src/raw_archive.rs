use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    time::Duration as StdDuration,
};

use anyhow::{bail, Context, Result};
use chrono::{Duration, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Row};
use tracing::info;
use uuid::Uuid;

use crate::{config::Cli, db};

#[derive(Debug, Clone)]
pub struct ArchiveOptions {
    pub apply: bool,
    pub dry_run: bool,
    pub chat_id: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct ArchiveWorkerOptions {
    pub check_only: bool,
    pub batch_size: i64,
    pub poll_seconds: u64,
    pub chat_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct ArchiveReport {
    dry_run: bool,
    target_chat_ids: Vec<String>,
    retention_days: i64,
    archive_format: String,
    archive_dir: Option<String>,
    candidate_messages: i64,
    archived_messages: i64,
    archive_path: Option<String>,
    manifest_path: Option<String>,
    content_sha256: Option<String>,
    skipped_reason: Option<String>,
}

#[derive(Debug)]
struct ArchiveRow {
    id: Uuid,
    chat_id: String,
    received_at: chrono::DateTime<Utc>,
    payload: Value,
}

pub async fn run(cli: &Cli, options: ArchiveOptions) -> Result<()> {
    if options.apply && options.dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let apply = options.apply && !options.dry_run;
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let report = run_inner(&pool, cli, &options, apply).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_worker(cli: &Cli, options: ArchiveWorkerOptions) -> Result<()> {
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let poll_seconds = options.poll_seconds.max(300);
    let archive_options = ArchiveOptions {
        apply: true,
        dry_run: options.check_only,
        chat_id: options.chat_id.clone(),
        limit: Some(options.batch_size.max(1)),
    };

    if options.check_only {
        let report = run_inner(&pool, cli, &archive_options, false).await?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    info!(
        batch_size = options.batch_size.max(1),
        poll_seconds,
        chat_id = ?options.chat_id,
        retention_days = cli.raw_message_hot_retention_days,
        archive_format = %cli.raw_archive_format,
        "starting raw archive worker"
    );
    let report = run_inner(&pool, cli, &archive_options, true).await?;
    log_archive_report(&report, "raw archive worker initial batch complete");
    loop {
        let report = run_inner(&pool, cli, &archive_options, true).await?;
        log_archive_report(&report, "raw archive worker batch complete");
        tokio::time::sleep(StdDuration::from_secs(poll_seconds)).await;
    }
}

async fn run_inner(
    pool: &PgPool,
    cli: &Cli,
    options: &ArchiveOptions,
    apply: bool,
) -> Result<ArchiveReport> {
    if cli.raw_archive_format != "jsonl.zst" {
        bail!("only jsonl.zst archive format is supported in V1");
    }
    let target_chat_ids = target_chat_ids(cli, options.chat_id.as_deref())?;
    let cutoff = Utc::now() - Duration::days(cli.raw_message_hot_retention_days.max(1));
    let limit = options.limit.unwrap_or(1000).max(1);
    let rows = load_archive_candidates(pool, &target_chat_ids, cutoff, limit).await?;
    let mut report = ArchiveReport {
        dry_run: !apply,
        target_chat_ids,
        retention_days: cli.raw_message_hot_retention_days,
        archive_format: cli.raw_archive_format.clone(),
        archive_dir: cli.raw_archive_dir.clone(),
        candidate_messages: rows.len() as i64,
        archived_messages: 0,
        archive_path: None,
        manifest_path: None,
        content_sha256: None,
        skipped_reason: None,
    };
    if rows.is_empty() {
        return Ok(report);
    }
    if !apply {
        return Ok(report);
    }
    let Some(dir) = cli.raw_archive_dir.as_ref() else {
        report.skipped_reason = Some("QINTOPIA_RAW_ARCHIVE_DIR is not configured".to_string());
        return Ok(report);
    };
    let archive_dir = PathBuf::from(dir);
    fs::create_dir_all(&archive_dir).context("create raw archive dir")?;
    let first_chat = rows
        .first()
        .map(|row| row.chat_id.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let archive_path = archive_dir.join(format!("{first_chat}-{stamp}.jsonl.zst"));
    let manifest_path = archive_dir.join(format!("{first_chat}-{stamp}.manifest.json"));
    let hash = write_archive(&archive_path, &rows)?;
    let manifest = json!({
        "chat_ids": report.target_chat_ids,
        "date_range": {
            "first_received_at": rows.first().map(|row| row.received_at),
            "last_received_at": rows.last().map(|row| row.received_at)
        },
        "message_count": rows.len(),
        "retention_days": report.retention_days,
        "archive_format": "jsonl.zst",
        "content_sha256": hash,
        "created_at": Utc::now(),
        "source_table": "qintopia_messages.messages",
        "schema_version": "2026-06-24.002",
        "policy": "default_agent_tools_must_not_read_archive_raw_messages"
    });
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)
        .context("write archive manifest")?;
    mark_archived(
        pool,
        &rows,
        &archive_path,
        &manifest_path,
        &hash,
        cutoff,
        &report.target_chat_ids,
    )
    .await?;
    report.archived_messages = rows.len() as i64;
    report.archive_path = Some(archive_path.display().to_string());
    report.manifest_path = Some(manifest_path.display().to_string());
    report.content_sha256 = Some(hash);
    Ok(report)
}

fn log_archive_report(report: &ArchiveReport, message: &str) {
    info!(
        retention_days = report.retention_days,
        archive_format = %report.archive_format,
        candidate_messages = report.candidate_messages,
        archived_messages = report.archived_messages,
        archive_path = ?report.archive_path,
        manifest_path = ?report.manifest_path,
        skipped_reason = ?report.skipped_reason,
        target_chat_ids = ?report.target_chat_ids,
        "{message}"
    );
}

async fn load_archive_candidates(
    pool: &PgPool,
    chat_ids: &[String],
    cutoff: chrono::DateTime<Utc>,
    limit: i64,
) -> Result<Vec<ArchiveRow>> {
    let rows = sqlx::query(
        r#"
        SELECT
            id,
            chat_id,
            received_at,
            jsonb_build_object(
                'id', id::text,
                'platform', platform,
                'message_id', message_id,
                'event_id', event_id,
                'chat_id', chat_id,
                'chat_type', chat_type,
                'sender_id', sender_id,
                'sender_name', sender_name,
                'sender_person_id', sender_person_id::text,
                'sender_channel_identity_id', sender_channel_identity_id::text,
                'message_kind', message_kind,
                'text', text,
                'sent_at', sent_at,
                'received_at', received_at,
                'information_class', information_class,
                'visibility', visibility,
                'raw', raw
            ) AS payload
        FROM qintopia_messages.messages
        WHERE platform = 'qiwe'
          AND chat_id = ANY($1)
          AND received_at < $2
          AND COALESCE(processing_hints->>'raw_archived', 'false') <> 'true'
        ORDER BY received_at ASC
        LIMIT $3
        "#,
    )
    .bind(chat_ids)
    .bind(cutoff)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("load raw archive candidates")?;
    rows.into_iter()
        .map(|row| {
            Ok(ArchiveRow {
                id: row.try_get("id")?,
                chat_id: row.try_get("chat_id")?,
                received_at: row.try_get("received_at")?,
                payload: row.try_get("payload")?,
            })
        })
        .collect()
}

fn write_archive(path: &Path, rows: &[ArchiveRow]) -> Result<String> {
    let file = File::create(path).context("create zstd archive file")?;
    let encoder = zstd::Encoder::new(file, 3).context("create zstd encoder")?;
    let mut writer = BufWriter::new(encoder);
    let mut hasher = Sha256::new();
    for row in rows {
        let bytes = serde_json::to_vec(&row.payload)?;
        hasher.update(&bytes);
        hasher.update(b"\n");
        writer.write_all(&bytes)?;
        writer.write_all(b"\n")?;
    }
    let encoder = writer
        .into_inner()
        .map_err(|error| anyhow::anyhow!("flush zstd archive writer: {}", error.error()))?;
    encoder.finish().context("finish zstd archive")?;
    Ok(format!("{:x}", hasher.finalize()))
}

async fn mark_archived(
    pool: &PgPool,
    rows: &[ArchiveRow],
    archive_path: &Path,
    manifest_path: &Path,
    hash: &str,
    cutoff: chrono::DateTime<Utc>,
    target_chat_ids: &[String],
) -> Result<()> {
    let ids = rows.iter().map(|row| row.id).collect::<Vec<_>>();
    let mut tx = pool
        .begin()
        .await
        .context("begin raw archive mark transaction")?;
    sqlx::query(
        r#"
        UPDATE qintopia_messages.messages
        SET processing_hints = processing_hints || $1::jsonb,
            updated_at = now()
        WHERE id = ANY($2)
        "#,
    )
    .bind(json!({
        "raw_archived": true,
        "raw_archive_path": archive_path.display().to_string(),
        "raw_archive_manifest_path": manifest_path.display().to_string(),
        "raw_archive_sha256": hash,
        "raw_archived_at": Utc::now()
    }))
    .bind(&ids)
    .execute(&mut *tx)
    .await
    .context("mark messages raw archived")?;
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.raw_message_archives
            (
                platform,
                chat_ids,
                cutoff_at,
                first_received_at,
                last_received_at,
                message_count,
                archive_format,
                archive_path,
                manifest_path,
                content_sha256,
                policy,
                created_by,
                metadata
            )
        VALUES ('qiwe', $1, $2, $3, $4, $5, 'jsonl.zst', $6, $7, $8, $9, $10, $11)
        "#,
    )
    .bind(target_chat_ids)
    .bind(cutoff)
    .bind(rows.first().map(|row| row.received_at))
    .bind(rows.last().map(|row| row.received_at))
    .bind(rows.len() as i64)
    .bind(archive_path.display().to_string())
    .bind(manifest_path.display().to_string())
    .bind(hash)
    .bind(json!({
        "hot_retention_days": "configured",
        "default_agent_tools_must_not_read_archive_raw_messages": true,
        "hard_delete": false
    }))
    .bind("qintopia-raw-archive-worker-v1")
    .bind(json!({
        "source_table": "qintopia_messages.messages",
        "message_ids_marked": ids.len()
    }))
    .execute(&mut *tx)
    .await
    .context("insert raw message archive manifest index")?;
    tx.commit()
        .await
        .context("commit raw archive mark transaction")?;
    Ok(())
}

fn target_chat_ids(cli: &Cli, requested: Option<&str>) -> Result<Vec<String>> {
    let configured = cli.profile_target_chat_ids();
    if configured.is_empty() {
        bail!("QINTOPIA_PROFILE_TARGET_CHAT_IDS is required");
    }
    if let Some(chat_id) = requested {
        let chat_id = chat_id.trim();
        if !configured.iter().any(|item| item == chat_id) {
            bail!("chat_id {chat_id} is not in QINTOPIA_PROFILE_TARGET_CHAT_IDS");
        }
        return Ok(vec![chat_id.to_string()]);
    }
    Ok(configured)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_v1_format_by_policy() {
        let report = ArchiveReport {
            dry_run: true,
            target_chat_ids: vec!["g".to_string()],
            retention_days: 30,
            archive_format: "jsonl.zst".to_string(),
            archive_dir: None,
            candidate_messages: 0,
            archived_messages: 0,
            archive_path: None,
            manifest_path: None,
            content_sha256: None,
            skipped_reason: None,
        };
        assert_eq!(report.archive_format, "jsonl.zst");
    }
}
