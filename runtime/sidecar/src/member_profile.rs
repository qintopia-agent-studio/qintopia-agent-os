use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs,
    io::ErrorKind,
    time::Duration as StdDuration,
};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, NaiveDate, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Row};
use tracing::info;
use uuid::Uuid;

use crate::{config::Cli, db, event_signal::EventSignalRow};

const GENERATED_BY: &str = "qintopia-member-profile-worker-v1";
const PROFILE_VERSION: &str = "v1";
const PROFILE_FACT_ACTIVITY_PARTICIPANT_LIMIT: usize = 12;
const DEFAULT_DAILY_DIGEST_DISPATCH_RULES_JSON: &str = r#"
[
  {
    "signal": "activity_events",
    "agent": "小满",
    "template": "补录和复盘今日活动事件，重点检查 {activity_count} 个活动/聚会信号的时间、参与人和素材回收。"
  },
  {
    "signal": "services",
    "agent": "小管家",
    "template": "确认 {service_count} 条服务/设施问题的处理状态，必要时同步群内进展。"
  },
  {
    "signal": "operations",
    "agent": "文渊阁",
    "template": "更新 {operation_count} 条 FAQ/SOP 候选，尤其是工具入口、活动模板和设施故障流程。"
  },
  {
    "signal": "content_or_activities",
    "agent": "画报司",
    "template": "收集活动照片、海报和现场素材线索。"
  },
  {
    "signal": "content_or_activities",
    "agent": "关二爷",
    "template": "判断活动和成员故事是否适合转为公众号/自媒体内容。"
  },
  {
    "signal": "questions",
    "agent": "四老师",
    "template": "协助梳理 {question_count} 条未回答问题和内部协作待办。"
  },
  {
    "signal": "empty_digest",
    "agent": "小满",
    "template": "今日暂无明显可分派事项，保持常规巡检。"
  },
  {
    "signal": "always",
    "agent": "大总管",
    "template": "关注高风险事项和跨 Agent 协作验收。"
  }
]
"#;

#[derive(Debug, Clone)]
pub struct ProfileOptions {
    pub apply: bool,
    pub dry_run: bool,
    pub chat_id: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct ProfileWorkerOptions {
    pub check_only: bool,
    pub quiet: bool,
    pub batch_size: i64,
    pub poll_seconds: u64,
    pub chat_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DigestOptions {
    pub apply: bool,
    pub dry_run: bool,
    pub quiet: bool,
    pub chat_id: Option<String>,
    pub date: Option<NaiveDate>,
}

#[derive(Debug, Clone)]
pub struct DigestWorkerOptions {
    pub dry_run: bool,
    pub once: bool,
    pub quiet: bool,
    pub chat_id: Option<String>,
    pub date: Option<NaiveDate>,
    pub poll_seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileReport {
    dry_run: bool,
    target_chat_ids: Vec<String>,
    messages_scanned: i64,
    messages_skipped_without_person: i64,
    messages_skipped_excluded_identity: i64,
    valuable_messages: i64,
    candidate_facts: Vec<CandidateFact>,
    filtered_labels: BTreeMap<String, i64>,
    facts_inserted: i64,
    summaries_inserted: i64,
    snapshots_inserted: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DigestReport {
    dry_run: bool,
    owner_agent: String,
    target_chat_id: String,
    target_chat_name: String,
    digest_date: NaiveDate,
    schedule_time: String,
    timezone: String,
    feishu_parent_node_configured: bool,
    document_title: String,
    markdown: String,
    publish_status: String,
    digest_outbox_id: Option<String>,
    feishu_parent_node: Option<String>,
    messages_scanned: i64,
    useful_signals: i64,
    event_signal_count: i64,
}

#[derive(Debug, Clone, Serialize)]
struct DigestSummaryReport {
    dry_run: bool,
    owner_agent: String,
    target_chat_id: String,
    target_chat_name: String,
    digest_date: NaiveDate,
    schedule_time: String,
    timezone: String,
    feishu_parent_node_configured: bool,
    document_title: String,
    publish_status: String,
    digest_outbox_id: Option<String>,
    feishu_parent_node: Option<String>,
    messages_scanned: i64,
    useful_signals: i64,
    event_signal_count: i64,
    markdown_bytes: usize,
}

impl From<&DigestReport> for DigestSummaryReport {
    fn from(report: &DigestReport) -> Self {
        Self {
            dry_run: report.dry_run,
            owner_agent: report.owner_agent.clone(),
            target_chat_id: report.target_chat_id.clone(),
            target_chat_name: report.target_chat_name.clone(),
            digest_date: report.digest_date,
            schedule_time: report.schedule_time.clone(),
            timezone: report.timezone.clone(),
            feishu_parent_node_configured: report.feishu_parent_node_configured,
            document_title: report.document_title.clone(),
            publish_status: report.publish_status.clone(),
            digest_outbox_id: report.digest_outbox_id.clone(),
            feishu_parent_node: report.feishu_parent_node.clone(),
            messages_scanned: report.messages_scanned,
            useful_signals: report.useful_signals,
            event_signal_count: report.event_signal_count,
            markdown_bytes: report.markdown.len(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct ProfileSummaryReport {
    dry_run: bool,
    target_chat_ids: Vec<String>,
    messages_scanned: i64,
    messages_skipped_without_person: i64,
    messages_skipped_excluded_identity: i64,
    valuable_messages: i64,
    candidate_fact_count: usize,
    filtered_labels: BTreeMap<String, i64>,
    facts_inserted: i64,
    summaries_inserted: i64,
    snapshots_inserted: i64,
}

impl From<&ProfileReport> for ProfileSummaryReport {
    fn from(report: &ProfileReport) -> Self {
        Self {
            dry_run: report.dry_run,
            target_chat_ids: report.target_chat_ids.clone(),
            messages_scanned: report.messages_scanned,
            messages_skipped_without_person: report.messages_skipped_without_person,
            messages_skipped_excluded_identity: report.messages_skipped_excluded_identity,
            valuable_messages: report.valuable_messages,
            candidate_fact_count: report.candidate_facts.len(),
            filtered_labels: report.filtered_labels.clone(),
            facts_inserted: report.facts_inserted,
            summaries_inserted: report.summaries_inserted,
            snapshots_inserted: report.snapshots_inserted,
        }
    }
}

#[derive(Debug, Clone)]
struct MessageRow {
    id: Uuid,
    sender_person_id: Option<Uuid>,
    sender_channel_identity_id: Option<Uuid>,
    sender_id: String,
    sender_name: Option<String>,
    sender_is_bot: bool,
    text: String,
    sent_at: Option<DateTime<Utc>>,
    received_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CandidateFact {
    #[serde(skip)]
    message_id: Uuid,
    #[serde(skip)]
    person_id: Uuid,
    #[serde(skip)]
    channel_identity_id: Option<Uuid>,
    fact_type: String,
    fact_key: String,
    fact_text: String,
    confidence: f64,
    observed_at: DateTime<Utc>,
    source_message_id: String,
    sender_name: Option<String>,
}

#[derive(Debug, Default)]
struct PersonAggregate {
    person_id: Uuid,
    channel_identity_id: Option<Uuid>,
    sender_name: Option<String>,
    message_ids: Vec<Uuid>,
    fact_ids: Vec<Uuid>,
    topics: BTreeSet<String>,
    facts: Vec<CandidateFact>,
}

pub async fn run_profile(cli: &Cli, options: ProfileOptions) -> Result<()> {
    if options.apply && options.dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let apply = options.apply && !options.dry_run;
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let report = run_profile_inner(&pool, cli, &options, apply).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_profile_worker(cli: &Cli, options: ProfileWorkerOptions) -> Result<()> {
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let poll_seconds = options.poll_seconds.max(30);
    let profile_options = ProfileOptions {
        apply: true,
        dry_run: options.check_only,
        chat_id: options.chat_id.clone(),
        limit: Some(options.batch_size.max(1)),
    };

    if options.check_only {
        let report = run_profile_inner(&pool, cli, &profile_options, false).await?;
        if options.quiet {
            println!(
                "{}",
                serde_json::to_string_pretty(&ProfileSummaryReport::from(&report))?
            );
        } else {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        return Ok(());
    }

    info!(
        batch_size = options.batch_size.max(1),
        poll_seconds,
        chat_id = ?options.chat_id,
        "starting member profile worker"
    );
    let report = run_profile_inner(&pool, cli, &profile_options, true).await?;
    log_profile_report(&report, "member profile worker initial batch complete");
    loop {
        let report = run_profile_inner(&pool, cli, &profile_options, true).await?;
        log_profile_report(&report, "member profile worker batch complete");
        tokio::time::sleep(StdDuration::from_secs(poll_seconds)).await;
    }
}

pub async fn run_digest(cli: &Cli, options: DigestOptions) -> Result<()> {
    if options.apply && options.dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let apply = options.apply && !options.dry_run;
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let report = run_digest_inner(&pool, cli, &options, apply).await?;
    print_digest_report(&report, options.quiet)?;
    Ok(())
}

pub async fn run_digest_worker(cli: &Cli, options: DigestWorkerOptions) -> Result<()> {
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let poll_seconds = options.poll_seconds.max(30);
    let apply = !options.dry_run;
    let digest_options = DigestOptions {
        apply,
        dry_run: options.dry_run,
        quiet: options.quiet,
        chat_id: options.chat_id.clone(),
        date: options.date,
    };

    if options.once || options.dry_run || options.date.is_some() {
        let report = run_digest_inner(&pool, cli, &digest_options, apply).await?;
        print_digest_report(&report, options.quiet)?;
        return Ok(());
    }

    info!(
        schedule_time = %cli.daily_digest_time,
        timezone = %cli.daily_digest_timezone,
        poll_seconds,
        chat_id = ?options.chat_id,
        "starting daily digest worker"
    );
    let mut generated_dates = BTreeSet::new();
    loop {
        match due_digest_date(cli) {
            Ok(Some(digest_date)) if !generated_dates.contains(&digest_date) => {
                let run_options = DigestOptions {
                    apply: true,
                    dry_run: false,
                    quiet: true,
                    chat_id: options.chat_id.clone(),
                    date: Some(digest_date),
                };
                let report = run_digest_inner(&pool, cli, &run_options, true).await?;
                info!(
                    digest_date = %report.digest_date,
                    chat_id = %report.target_chat_id,
                    messages_scanned = report.messages_scanned,
                    useful_signals = report.useful_signals,
                    publish_status = %report.publish_status,
                    "daily digest worker generated digest"
                );
                generated_dates.insert(digest_date);
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(error = %error, "daily digest schedule check failed");
            }
        }
        tokio::time::sleep(StdDuration::from_secs(poll_seconds)).await;
    }
}

async fn run_profile_inner(
    pool: &PgPool,
    cli: &Cli,
    options: &ProfileOptions,
    apply: bool,
) -> Result<ProfileReport> {
    let target_chat_ids = target_chat_ids(cli, options.chat_id.as_deref())?;
    let rows = load_messages(pool, &target_chat_ids, options.limit.unwrap_or(500).max(1)).await?;
    let mut report = ProfileReport {
        dry_run: !apply,
        target_chat_ids,
        messages_scanned: rows.len() as i64,
        messages_skipped_without_person: 0,
        messages_skipped_excluded_identity: 0,
        valuable_messages: 0,
        candidate_facts: Vec::new(),
        filtered_labels: BTreeMap::new(),
        facts_inserted: 0,
        summaries_inserted: 0,
        snapshots_inserted: 0,
    };
    let mut aggregates = BTreeMap::<Uuid, PersonAggregate>::new();
    for row in rows {
        let Some(person_id) = row.sender_person_id else {
            report.messages_skipped_without_person += 1;
            continue;
        };
        if is_profile_excluded_identity(cli, &row) {
            report.messages_skipped_excluded_identity += 1;
            continue;
        }
        let labels = classify_profile_fact_message(&row.text);
        if labels.is_empty() {
            *report
                .filtered_labels
                .entry("noise_or_low_value".to_string())
                .or_insert(0) += 1;
            continue;
        }
        report.valuable_messages += 1;
        let aggregate = aggregates
            .entry(person_id)
            .or_insert_with(|| PersonAggregate {
                person_id,
                channel_identity_id: row.sender_channel_identity_id,
                sender_name: row.sender_name.clone(),
                ..PersonAggregate::default()
            });
        aggregate.message_ids.push(row.id);
        if aggregate.sender_name.is_none() {
            aggregate.sender_name = row.sender_name.clone();
        }
        if aggregate.channel_identity_id.is_none() {
            aggregate.channel_identity_id = row.sender_channel_identity_id;
        }
        for label in labels {
            if let Some(topic) = topic_for_label(&label) {
                aggregate.topics.insert(topic.to_string());
            }
            if let Some(fact) = candidate_fact_for_label(&row, person_id, &label) {
                aggregate.facts.push(fact.clone());
                report.candidate_facts.push(fact);
            }
        }
    }
    if apply {
        apply_profile(pool, &mut aggregates, &mut report).await?;
    }
    Ok(report)
}

async fn run_digest_inner(
    pool: &PgPool,
    cli: &Cli,
    options: &DigestOptions,
    apply: bool,
) -> Result<DigestReport> {
    let target_chat_id = target_chat_ids(cli, options.chat_id.as_deref())?
        .into_iter()
        .next()
        .context("daily digest requires one target chat id")?;
    let digest_date = options
        .date
        .unwrap_or_else(|| default_digest_date(&cli.daily_digest_timezone));
    let (period_start, period_end) = digest_utc_window(digest_date, &cli.daily_digest_timezone)?;
    let message_count =
        count_messages_between(pool, &target_chat_id, period_start, period_end).await?;
    let event_signals =
        crate::event_signal::load_event_signals_for_digest(pool, &target_chat_id, digest_date)
            .await?;
    let renderer = DigestV2Renderer::new(daily_digest_dispatch_rules(cli)?, event_signals);
    let target_chat_name = chat_display_name(cli, &target_chat_id)?;
    let document_title = format!("群聊运营日报 - {} - {}", target_chat_name, digest_date);
    let markdown = renderer.render_markdown(&document_title);
    let mut report = DigestReport {
        dry_run: !apply,
        owner_agent: cli.daily_digest_owner_agent.clone(),
        target_chat_id,
        target_chat_name,
        digest_date,
        schedule_time: cli.daily_digest_time.clone(),
        timezone: cli.daily_digest_timezone.clone(),
        feishu_parent_node_configured: cli.daily_digest_feishu_parent_node.is_some(),
        document_title,
        markdown,
        publish_status: "dry_run".to_string(),
        digest_outbox_id: None,
        feishu_parent_node: cli.daily_digest_feishu_parent_node.clone(),
        messages_scanned: message_count,
        useful_signals: renderer.event_count() as i64,
        event_signal_count: renderer.event_count() as i64,
    };
    if apply {
        let (id, status) = upsert_daily_digest(pool, cli, &report).await?;
        report.digest_outbox_id = Some(id.to_string());
        report.publish_status = status;
    }
    Ok(report)
}

async fn upsert_daily_digest(
    pool: &PgPool,
    cli: &Cli,
    report: &DigestReport,
) -> Result<(Uuid, String)> {
    let publish_status = if cli.daily_digest_feishu_parent_node.is_some() {
        "pending_feishu_publish"
    } else {
        "pending_feishu_parent_node"
    };
    let row: (Uuid, String) = sqlx::query_as(
        r#"
        INSERT INTO qintopia_agent_os.daily_digests
            (
                owner_agent,
                platform,
                chat_id,
                digest_date,
                schedule_time,
                timezone,
                title,
                markdown,
                feishu_parent_node,
                publish_status,
                message_count,
                useful_signal_count,
                generated_by,
                metadata
            )
        VALUES ($1, 'qiwe', $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        ON CONFLICT (platform, chat_id, digest_date, owner_agent) DO UPDATE SET
            schedule_time = EXCLUDED.schedule_time,
            timezone = EXCLUDED.timezone,
            title = EXCLUDED.title,
            markdown = EXCLUDED.markdown,
            feishu_parent_node = EXCLUDED.feishu_parent_node,
            publish_status = CASE
                WHEN qintopia_agent_os.daily_digests.publish_status = 'published'
                    AND (
                        qintopia_agent_os.daily_digests.markdown IS DISTINCT FROM EXCLUDED.markdown
                        OR qintopia_agent_os.daily_digests.message_count IS DISTINCT FROM EXCLUDED.message_count
                        OR qintopia_agent_os.daily_digests.useful_signal_count IS DISTINCT FROM EXCLUDED.useful_signal_count
                    )
                    THEN EXCLUDED.publish_status
                WHEN qintopia_agent_os.daily_digests.publish_status = 'published'
                    THEN qintopia_agent_os.daily_digests.publish_status
                ELSE EXCLUDED.publish_status
            END,
            message_count = EXCLUDED.message_count,
            useful_signal_count = EXCLUDED.useful_signal_count,
            generated_by = EXCLUDED.generated_by,
            generated_at = now(),
            metadata = qintopia_agent_os.daily_digests.metadata || EXCLUDED.metadata,
            updated_at = now()
        RETURNING id, publish_status
        "#,
    )
    .bind(&report.owner_agent)
    .bind(&report.target_chat_id)
    .bind(report.digest_date)
    .bind(&report.schedule_time)
    .bind(&report.timezone)
    .bind(&report.document_title)
    .bind(&report.markdown)
    .bind(&cli.daily_digest_feishu_parent_node)
    .bind(publish_status)
    .bind(report.messages_scanned)
    .bind(report.useful_signals)
    .bind(GENERATED_BY)
    .bind(json!({
        "document_kind": "group_operations_daily_digest",
        "chat_display_name": report.target_chat_name,
        "role_boundary": "xiaoman_owner_erhua_read_disabled",
        "feishu_write_mode": "outbox"
    }))
    .fetch_one(pool)
    .await
    .context("upsert daily digest outbox")?;
    Ok(row)
}

async fn apply_profile(
    pool: &PgPool,
    aggregates: &mut BTreeMap<Uuid, PersonAggregate>,
    report: &mut ProfileReport,
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("begin member profile transaction")?;
    for aggregate in aggregates.values_mut() {
        revoke_stale_profile_facts(&mut tx, aggregate).await?;
        for fact in &aggregate.facts {
            let id: (Uuid,) = sqlx::query_as(
                r#"
                INSERT INTO qintopia_identity.member_facts
                    (
                        person_id,
                        channel_identity_id,
                        fact_type,
                        fact_key,
                        fact_text,
                        evidence_type,
                        source_message_id,
                        information_class,
                        visibility,
                        confidence,
                        observed_at,
                        metadata
                    )
                SELECT $1, $2, $3, $4, $5, 'message', $6, 'Internal', 'internal', $7, $8, $9
                WHERE NOT EXISTS (
                    SELECT 1
                    FROM qintopia_identity.member_facts existing
                    WHERE existing.source_message_id = $6
                      AND existing.fact_type = $3
                      AND existing.fact_key = $4
                      AND existing.revoked_at IS NULL
                )
                RETURNING id
                "#,
            )
            .bind(fact.person_id)
            .bind(fact.channel_identity_id)
            .bind(&fact.fact_type)
            .bind(&fact.fact_key)
            .bind(&fact.fact_text)
            .bind(fact.message_id)
            .bind(fact.confidence)
            .bind(fact.observed_at)
            .bind(json!({
                "generated_by": GENERATED_BY,
                "v1_policy": "sensitive_fact_types_disabled"
            }))
            .fetch_optional(&mut *tx)
            .await
            .context("insert member fact")?
            .unwrap_or((Uuid::nil(),));
            if id.0 != Uuid::nil() {
                aggregate.fact_ids.push(id.0);
                report.facts_inserted += 1;
            }
        }
        if aggregate.message_ids.is_empty() {
            continue;
        }
        let input_hash = input_hash(aggregate);
        let existing_snapshot =
            find_active_snapshot_by_hash(&mut tx, aggregate.person_id, &input_hash).await?;
        if !existing_snapshot {
            aggregate.fact_ids = load_existing_fact_ids(&mut tx, aggregate).await?;
            let summary_text = build_summary_text(aggregate);
            let topics = aggregate.topics.iter().cloned().collect::<Vec<_>>();
            let summary_id: (Uuid,) = sqlx::query_as(
                r#"
            INSERT INTO qintopia_identity.person_interaction_summaries
                (
                    person_id,
                    channel_identity_id,
                    platform,
                    chat_id,
                    summary,
                    topics,
                    source_message_ids,
                    information_class,
                    confidence,
                    generated_by,
                    metadata
            )
            VALUES ($1, $2, 'qiwe', $3, $4, $5, $6, 'Internal', 0.72, $7, $8)
            RETURNING id
            "#,
            )
            .bind(aggregate.person_id)
            .bind(aggregate.channel_identity_id)
            .bind(
                report
                    .target_chat_ids
                    .first()
                    .map(String::as_str)
                    .unwrap_or(""),
            )
            .bind(&summary_text)
            .bind(&topics)
            .bind(&aggregate.message_ids)
            .bind(GENERATED_BY)
            .bind(json!({
                "summary_kind": "rule_based_v1",
                "message_count": aggregate.message_ids.len(),
                "fact_count": aggregate.facts.len(),
                "input_hash": input_hash
            }))
            .fetch_one(&mut *tx)
            .await
            .context("insert interaction summary")?;
            let summary_ids = vec![summary_id.0];
            report.summaries_inserted += 1;

            sqlx::query(
                r#"
            UPDATE qintopia_identity.member_profile_snapshots
            SET status = 'superseded'
            WHERE person_id = $1
              AND profile_kind = 'reply_context'
              AND status = 'active'
            "#,
            )
            .bind(aggregate.person_id)
            .execute(&mut *tx)
            .await
            .context("supersede previous member profile snapshots")?;
            sqlx::query(
            r#"
            INSERT INTO qintopia_identity.member_profile_snapshots
                (
                    person_id,
                    profile_kind,
                    profile_version,
                    status,
                    summary,
                    communication_style,
                    safe_reply_hints,
                    do_not_disclose,
                    source_fact_ids,
                    source_summary_ids,
                    information_class,
                    confidence,
                    generated_by,
                    input_hash
                )
            VALUES ($1, 'reply_context', $2, 'active', $3, $4, $5, $6, $7, $8, 'Internal', 0.72, $9, $10)
            "#,
        )
        .bind(aggregate.person_id)
        .bind(PROFILE_VERSION)
        .bind(build_safe_profile_summary(aggregate))
        .bind(communication_style(aggregate))
        .bind(safe_reply_hints(aggregate))
            .bind(json!({
                "raw_messages": true,
                "hidden_profile_details": true,
            "sensitive_facts": true,
            "internal_labels": true,
            "daily_digest_full_text": true
            }))
            .bind(&aggregate.fact_ids)
            .bind(&summary_ids)
            .bind(GENERATED_BY)
            .bind(input_hash)
            .execute(&mut *tx)
            .await
            .context("insert member profile snapshot")?;
            report.snapshots_inserted += 1;
        }
    }
    tx.commit()
        .await
        .context("commit member profile transaction")?;
    Ok(())
}

fn log_profile_report(report: &ProfileReport, message: &str) {
    info!(
        messages_scanned = report.messages_scanned,
        messages_skipped_without_person = report.messages_skipped_without_person,
        messages_skipped_excluded_identity = report.messages_skipped_excluded_identity,
        valuable_messages = report.valuable_messages,
        candidate_fact_count = report.candidate_facts.len(),
        facts_inserted = report.facts_inserted,
        summaries_inserted = report.summaries_inserted,
        snapshots_inserted = report.snapshots_inserted,
        target_chat_ids = ?report.target_chat_ids,
        "{message}"
    );
}

fn print_digest_report(report: &DigestReport, quiet: bool) -> Result<()> {
    if quiet {
        println!(
            "{}",
            serde_json::to_string_pretty(&DigestSummaryReport::from(report))?
        );
    } else {
        println!("{}", serde_json::to_string_pretty(report)?);
    }
    Ok(())
}

async fn find_active_snapshot_by_hash(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    person_id: Uuid,
    input_hash: &str,
) -> Result<bool> {
    let row = sqlx::query(
        r#"
        SELECT 1
        FROM qintopia_identity.member_profile_snapshots
        WHERE person_id = $1
          AND profile_kind = 'reply_context'
          AND profile_version = $2
          AND status = 'active'
          AND input_hash = $3
        ORDER BY generated_at DESC
        LIMIT 1
        "#,
    )
    .bind(person_id)
    .bind(PROFILE_VERSION)
    .bind(input_hash)
    .fetch_optional(&mut **tx)
    .await
    .context("lookup active member profile snapshot by input hash")?;
    Ok(row.is_some())
}

async fn revoke_stale_profile_facts(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    aggregate: &PersonAggregate,
) -> Result<()> {
    if aggregate.message_ids.is_empty() {
        return Ok(());
    }
    let current_keys = aggregate
        .facts
        .iter()
        .map(|fact| {
            (
                fact.message_id,
                fact.fact_type.clone(),
                fact.fact_key.clone(),
            )
        })
        .collect::<HashSet<_>>();
    let existing = sqlx::query_as::<_, (Uuid, Uuid, String, String)>(
        r#"
        SELECT id, source_message_id, fact_type, fact_key
        FROM qintopia_identity.member_facts
        WHERE person_id = $1
          AND source_message_id = ANY($2)
          AND revoked_at IS NULL
          AND metadata->>'generated_by' = $3
        "#,
    )
    .bind(aggregate.person_id)
    .bind(&aggregate.message_ids)
    .bind(GENERATED_BY)
    .fetch_all(&mut **tx)
    .await
    .context("load stale member facts")?;
    let stale_ids = existing
        .into_iter()
        .filter_map(|(id, message_id, fact_type, fact_key)| {
            if current_keys.contains(&(message_id, fact_type, fact_key)) {
                None
            } else {
                Some(id)
            }
        })
        .collect::<Vec<_>>();
    if stale_ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        r#"
        UPDATE qintopia_identity.member_facts
        SET revoked_at = now(),
            metadata = metadata || '{"revoked_by":"qintopia-member-profile-worker-v1","revoked_reason":"profile_rule_no_longer_matches"}'::jsonb
        WHERE id = ANY($1)
        "#,
    )
    .bind(&stale_ids)
    .execute(&mut **tx)
    .await
    .context("revoke stale member facts")?;
    Ok(())
}

async fn load_existing_fact_ids(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    aggregate: &PersonAggregate,
) -> Result<Vec<Uuid>> {
    let rows = sqlx::query_as::<_, (Uuid,)>(
        r#"
        SELECT id
        FROM qintopia_identity.member_facts
        WHERE person_id = $1
          AND source_message_id = ANY($2)
          AND revoked_at IS NULL
        ORDER BY observed_at ASC, id ASC
        "#,
    )
    .bind(aggregate.person_id)
    .bind(&aggregate.message_ids)
    .fetch_all(&mut **tx)
    .await
    .context("load existing member fact ids for snapshot")?;
    Ok(rows.into_iter().map(|row| row.0).collect())
}

async fn load_messages(pool: &PgPool, chat_ids: &[String], limit: i64) -> Result<Vec<MessageRow>> {
    let rows = sqlx::query(
        r#"
        SELECT
            m.id,
            m.chat_id,
            m.sender_id,
            m.sender_person_id,
            m.sender_channel_identity_id,
            m.sender_name,
            COALESCE(ci.is_bot, false) AS sender_is_bot,
            COALESCE(text, '') AS text,
            m.sent_at,
            m.received_at
        FROM qintopia_messages.messages m
        LEFT JOIN qintopia_identity.channel_identities ci
          ON ci.id = m.sender_channel_identity_id
        WHERE m.platform = 'qiwe'
          AND m.chat_id = ANY($1)
          AND COALESCE(m.text, '') <> ''
        ORDER BY m.received_at DESC
        LIMIT $2
        "#,
    )
    .bind(chat_ids)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("load messages for member profile")?;
    rows.into_iter().map(row_to_message).collect()
}

async fn count_messages_between(
    pool: &PgPool,
    chat_id: &str,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
) -> Result<i64> {
    let row: (i64,) = sqlx::query_as(
        r#"
        SELECT count(*)::bigint
        FROM qintopia_messages.messages
        WHERE platform = 'qiwe'
          AND chat_id = $1
          AND received_at >= $2
          AND received_at < $3
          AND COALESCE(text, '') <> ''
        "#,
    )
    .bind(chat_id)
    .bind(period_start)
    .bind(period_end)
    .fetch_one(pool)
    .await
    .context("count messages for daily digest")?;
    Ok(row.0)
}

fn row_to_message(row: sqlx::postgres::PgRow) -> Result<MessageRow> {
    Ok(MessageRow {
        id: row.try_get("id")?,
        sender_person_id: row.try_get("sender_person_id")?,
        sender_channel_identity_id: row.try_get("sender_channel_identity_id")?,
        sender_id: row.try_get("sender_id")?,
        sender_name: row.try_get("sender_name")?,
        sender_is_bot: row.try_get("sender_is_bot")?,
        text: row.try_get("text")?,
        sent_at: row.try_get("sent_at")?,
        received_at: row.try_get("received_at")?,
    })
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

fn chat_display_name(cli: &Cli, chat_id: &str) -> Result<String> {
    let Some(raw) = cli.chat_metadata_json.as_deref() else {
        return Ok(chat_id.to_string());
    };
    chat_display_name_from_metadata(raw, chat_id)
}

fn chat_display_name_from_metadata(raw: &str, chat_id: &str) -> Result<String> {
    let metadata: HashMap<String, ChatMetadata> =
        serde_json::from_str(raw).context("parse QINTOPIA_CHAT_METADATA_JSON")?;
    Ok(metadata
        .get(chat_id)
        .and_then(|item| item.display_name.as_deref())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(chat_id)
        .to_string())
}

fn is_profile_excluded_identity(cli: &Cli, row: &MessageRow) -> bool {
    if row.sender_is_bot {
        return true;
    }
    if cli
        .profile_excluded_channel_user_ids()
        .iter()
        .any(|id| id == &row.sender_id)
    {
        return true;
    }
    let Some(sender_name) = row.sender_name.as_deref() else {
        return false;
    };
    cli.profile_excluded_display_names()
        .iter()
        .any(|name| name == sender_name)
}

fn default_digest_date(timezone: &str) -> NaiveDate {
    if timezone.trim().eq_ignore_ascii_case("Asia/Shanghai") {
        return (Utc::now() + Duration::hours(8)).date_naive() - Duration::days(1);
    }
    Utc::now().date_naive() - Duration::days(1)
}

fn due_digest_date(cli: &Cli) -> Result<Option<NaiveDate>> {
    let local_now = local_now_for_timezone(&cli.daily_digest_timezone)?;
    let schedule_minutes = parse_hhmm_minutes(&cli.daily_digest_time)?;
    let now_minutes =
        i64::from(local_now.time().hour()) * 60 + i64::from(local_now.time().minute());
    if now_minutes < schedule_minutes {
        return Ok(None);
    }
    Ok(Some(local_now.date_naive() - Duration::days(1)))
}

fn local_now_for_timezone(timezone: &str) -> Result<chrono::DateTime<chrono::FixedOffset>> {
    let offset_hours = match timezone.trim() {
        "Asia/Shanghai" => 8,
        "UTC" | "Etc/UTC" => 0,
        other => bail!("unsupported daily digest timezone for V1: {other}"),
    };
    let offset = chrono::FixedOffset::east_opt(offset_hours * 3600)
        .context("invalid daily digest timezone offset")?;
    Ok(Utc::now().with_timezone(&offset))
}

fn parse_hhmm_minutes(value: &str) -> Result<i64> {
    let (hour, minute) = value
        .trim()
        .split_once(':')
        .context("QINTOPIA_DAILY_DIGEST_TIME must use HH:MM")?;
    let hour: i64 = hour.parse().context("parse daily digest hour")?;
    let minute: i64 = minute.parse().context("parse daily digest minute")?;
    if !(0..=23).contains(&hour) || !(0..=59).contains(&minute) {
        bail!("QINTOPIA_DAILY_DIGEST_TIME must be between 00:00 and 23:59");
    }
    Ok(hour * 60 + minute)
}

fn digest_utc_window(
    digest_date: NaiveDate,
    timezone: &str,
) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let offset_hours = match timezone.trim() {
        "Asia/Shanghai" => 8,
        "UTC" | "Etc/UTC" => 0,
        other => bail!("unsupported daily digest timezone for V1: {other}"),
    };
    let start_naive = digest_date
        .and_hms_opt(0, 0, 0)
        .context("invalid digest date start")?
        - Duration::hours(offset_hours);
    let end_naive = (digest_date + Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .context("invalid digest date end")?
        - Duration::hours(offset_hours);
    Ok((
        DateTime::<Utc>::from_naive_utc_and_offset(start_naive, Utc),
        DateTime::<Utc>::from_naive_utc_and_offset(end_naive, Utc),
    ))
}

fn classify_profile_fact_message(text: &str) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() <= 2 || is_system_or_placeholder_message(text) {
        return Vec::new();
    }
    let mut labels = Vec::new();
    let lower = text.to_lowercase();
    let is_temporary_state = is_temporary_communication_state(text);
    if is_reply_style_preference(text) {
        labels.push("reply_style_preference".to_string());
    }
    if is_temporary_state {
        labels.push("temporary_communication_state".to_string());
    }
    if !is_temporary_state && is_first_person_interest_or_skill(text) {
        labels.push("interest".to_string());
    }
    if is_activity_organizer_signal(text) {
        labels.push("activity_organizer".to_string());
    } else if is_activity_participation_signal(text) {
        labels.push("activity_participation".to_string());
    }
    if is_service_need_signal(text) {
        labels.push("service_need".to_string());
    }
    if !is_temporary_state && is_substantial_question(text) {
        labels.push("unresolved_question".to_string());
    }
    if is_content_story_lead(text) {
        labels.push("content_story_lead".to_string());
    }
    if lower.contains("sop") || contains_any(text, &["规则", "流程", "常见问题", "须知"])
    {
        labels.push("operation_signal".to_string());
    }
    labels.sort();
    labels.dedup();
    labels
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn is_reply_style_preference(text: &str) -> bool {
    contains_any(
        text,
        &["简短", "重点", "详细", "别啰嗦", "直接说", "以后你"],
    ) && contains_any(text, &["我", "二花", "你", "以后"])
}

fn is_temporary_communication_state(text: &str) -> bool {
    let has_first_person = contains_any(text, &["我", "本人"]);
    let has_silence_signal = contains_any(
        text,
        &[
            "不语者",
            "止语",
            "不开口说话",
            "不说话",
            "无声互动",
            "用眼神",
            "肢体语言",
            "不要故意引导我开口",
            "只是我不说话",
        ],
    );
    has_first_person && has_silence_signal
}

fn is_first_person_interest_or_skill(text: &str) -> bool {
    contains_any(
        text,
        &["我喜欢", "我想学", "我擅长", "我会", "我可以帮", "我感兴趣"],
    )
}

fn is_activity_organizer_signal(text: &str) -> bool {
    !is_solitaire_message(text)
        && contains_any(
            text,
            &[
                "我来组织",
                "我组织",
                "我发起",
                "我想发起",
                "我想在社区里发起",
                "我来发起",
                "我来带",
            ],
        )
}

fn is_activity_participation_signal(text: &str) -> bool {
    if is_solitaire_message(text) {
        return solitaire_participant_count(text) <= PROFILE_FACT_ACTIVITY_PARTICIPANT_LIMIT;
    }
    contains_any(
        text,
        &["我报名", "我参加", "我想参加", "我也去", "算我一个"],
    )
}

fn is_service_need_signal(text: &str) -> bool {
    contains_any(
        text,
        &[
            "维修",
            "坏了",
            "处理",
            "投诉",
            "反馈",
            "不满意",
            "没人管",
            "客服",
            "帮我",
            "求助",
        ],
    )
}

fn is_substantial_question(text: &str) -> bool {
    if text.chars().count() < 12 {
        return false;
    }
    contains_any(
        text,
        &[
            "怎么",
            "有没有",
            "哪里",
            "什么时候",
            "能不能",
            "可以吗",
            "求推荐",
            "谁知道",
        ],
    )
}

fn is_content_story_lead(text: &str) -> bool {
    contains_any(
        text,
        &[
            "素材",
            "故事",
            "小红书",
            "公众号",
            "拍照",
            "海报",
            "视频",
            "内容",
        ],
    )
}

fn is_solitaire_message(text: &str) -> bool {
    text.contains("#接龙")
}

fn solitaire_participant_count(text: &str) -> usize {
    text.lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            let mut chars = trimmed.chars();
            let has_digit = chars.next().map(|ch| ch.is_ascii_digit()).unwrap_or(false);
            has_digit && chars.any(|ch| ch == '.' || ch == '、')
        })
        .count()
}

fn is_system_or_placeholder_message(text: &str) -> bool {
    let text = text.trim();
    text.starts_with("[我发起了一笔群收款")
        || text.starts_with("[收到一条")
        || text.contains("该版本暂不支持查看")
}

fn candidate_fact_for_label(
    row: &MessageRow,
    person_id: Uuid,
    label: &str,
) -> Option<CandidateFact> {
    let (fact_type, fact_key, fact_text, confidence) = match label {
        "reply_style_preference" => (
            "reply_style_preference",
            "reply_style",
            format!("表达了沟通/回复风格偏好：{}", clean_snippet(&row.text, 120)),
            0.78,
        ),
        "temporary_communication_state" => (
            "temporary_communication_state",
            "temporary_communication",
            format!(
                "公开表达了短期沟通状态或互动边界：{}",
                clean_snippet(&row.text, 160)
            ),
            0.82,
        ),
        "interest" => (
            "interest",
            "interest_or_skill",
            format!(
                "表达了兴趣、技能或可提供帮助：{}",
                clean_snippet(&row.text, 120)
            ),
            0.70,
        ),
        "activity_organizer" => (
            "activity_organizer",
            "activity",
            format!("疑似组织或发起活动：{}", clean_snippet(&row.text, 120)),
            0.74,
        ),
        "activity_participation" => (
            "activity_participation",
            "activity",
            format!("疑似参与或询问活动：{}", clean_snippet(&row.text, 120)),
            0.68,
        ),
        "service_need" => (
            "service_need",
            "service",
            format!(
                "表达了服务、设施或人工处理需求：{}",
                clean_snippet(&row.text, 120)
            ),
            0.70,
        ),
        "unresolved_question" => (
            "unresolved_question",
            "question",
            format!(
                "提出了可能需要回答或跟进的问题：{}",
                clean_snippet(&row.text, 120)
            ),
            0.60,
        ),
        "content_story_lead" => (
            "content_story_lead",
            "content",
            format!(
                "包含内容、素材或成员故事线索：{}",
                clean_snippet(&row.text, 120)
            ),
            0.68,
        ),
        "operation_signal" => (
            "operation_signal",
            "operations",
            format!(
                "包含 SOP、规则、流程或运营改进信号：{}",
                clean_snippet(&row.text, 120)
            ),
            0.66,
        ),
        _ => return None,
    };
    Some(CandidateFact {
        message_id: row.id,
        person_id,
        channel_identity_id: row.sender_channel_identity_id,
        fact_type: fact_type.to_string(),
        fact_key: fact_key.to_string(),
        fact_text,
        confidence,
        observed_at: row.sent_at.unwrap_or(row.received_at),
        source_message_id: row.id.to_string(),
        sender_name: row.sender_name.clone(),
    })
}

fn topic_for_label(label: &str) -> Option<&'static str> {
    match label {
        "reply_style_preference" => Some("沟通偏好"),
        "temporary_communication_state" => Some("短期沟通状态"),
        "interest" => Some("兴趣技能"),
        "activity_organizer" | "activity_participation" => Some("活动"),
        "service_need" => Some("服务需求"),
        "unresolved_question" => Some("待回答问题"),
        "content_story_lead" => Some("内容线索"),
        "operation_signal" => Some("运营信号"),
        _ => None,
    }
}

fn clean_snippet(text: &str, max_chars: usize) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max_chars)
        .collect()
}

fn build_summary_text(aggregate: &PersonAggregate) -> String {
    let name = aggregate
        .sender_name
        .clone()
        .unwrap_or_else(|| "该成员".to_string());
    let topics = aggregate
        .topics
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join("、");
    if topics.is_empty() {
        format!("{name} 有少量群内互动，暂无稳定画像信号。")
    } else {
        format!(
            "{name} 在本窗口内出现了这些可运营信号：{topics}。该摘要只供内部运营和安全个性化使用。"
        )
    }
}

fn build_safe_profile_summary(aggregate: &PersonAggregate) -> String {
    let name = aggregate
        .sender_name
        .clone()
        .unwrap_or_else(|| "这位成员".to_string());
    let topics = aggregate
        .topics
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join("、");
    if topics.is_empty() {
        format!("{name} 暂无足够稳定的安全画像。")
    } else {
        format!("{name} 最近的安全上下文主要与 {topics} 有关。回复时保持自然，不暴露画像来源。")
    }
}

fn communication_style(aggregate: &PersonAggregate) -> Value {
    let prefers_concise = aggregate
        .facts
        .iter()
        .any(|fact| fact.fact_type == "reply_style_preference" && fact.fact_text.contains("重点"));
    let temporary_communication_notes = aggregate
        .facts
        .iter()
        .filter(|fact| fact.fact_type == "temporary_communication_state")
        .map(|fact| fact.fact_text.clone())
        .collect::<Vec<_>>();
    json!({
        "prefer_concise": prefers_concise,
        "tone": "natural_frontdesk",
        "temporary_communication_notes": temporary_communication_notes,
        "avoid_source_disclosure": true
    })
}

fn safe_reply_hints(aggregate: &PersonAggregate) -> Value {
    let temporary_communication_notes = aggregate
        .facts
        .iter()
        .filter(|fact| fact.fact_type == "temporary_communication_state")
        .map(|fact| fact.fact_text.clone())
        .collect::<Vec<_>>();
    json!({
        "use_display_name_only_if_natural": true,
        "topics": aggregate.topics.iter().cloned().collect::<Vec<_>>(),
        "temporary_communication_notes": temporary_communication_notes,
        "do_not_quote_raw_history": true
    })
}

fn input_hash(aggregate: &PersonAggregate) -> String {
    let mut hasher = Sha256::new();
    hasher.update(aggregate.person_id.as_bytes());
    for message_id in &aggregate.message_ids {
        hasher.update(message_id.as_bytes());
    }
    for fact in &aggregate.facts {
        hasher.update(fact.fact_type.as_bytes());
        hasher.update(fact.fact_text.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, Clone, Deserialize)]
struct DispatchRule {
    signal: DispatchSignal,
    agent: String,
    template: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatMetadata {
    display_name: Option<String>,
    #[allow(dead_code)]
    source: Option<String>,
    #[allow(dead_code)]
    short_name: Option<String>,
    #[allow(dead_code)]
    owner_agent: Option<String>,
    #[allow(dead_code)]
    enabled: Option<bool>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum DispatchSignal {
    ActivityEvents,
    Services,
    Operations,
    ContentOrActivities,
    Questions,
    EmptyDigest,
    Always,
}

struct DigestV2Renderer {
    dispatch_rules: Vec<DispatchRule>,
    events: Vec<EventSignalRow>,
}

impl DigestV2Renderer {
    fn new(dispatch_rules: Vec<DispatchRule>, events: Vec<EventSignalRow>) -> Self {
        Self {
            dispatch_rules,
            events,
        }
    }

    fn event_count(&self) -> usize {
        self.events.len()
    }

    fn render_markdown(&self, title: &str) -> String {
        let mut out =
            format!("# {title}\n\n> 内部运营材料。不是 Public-safe 内容，不主动发送到企微群。\n\n");
        out.push_str("## 今日摘要\n\n");
        if self.events.is_empty() {
            out.push_str("今日无显著群聊运营信号。\n\n");
        } else {
            out.push_str(&format!(
                "今日识别到 {} 条可运营事件信号。\n\n",
                self.events.len()
            ));
        }
        push_section(&mut out, "主要话题", &self.render_key_topics());
        push_section(
            &mut out,
            "高频问题 / 未回答问题",
            &self.render_type("未回答问题"),
        );
        push_section(&mut out, "活动和聚会信号", &self.render_type("活动/聚会"));
        push_section(&mut out, "服务 / 设施问题", &self.render_type("服务/设施"));
        push_section(
            &mut out,
            "内容线索和成员故事线索",
            &self.render_type("内容线索"),
        );
        push_section(&mut out, "FAQ / SOP 更新候选", &self.render_type("FAQ/SOP"));
        push_section(&mut out, "群氛围和风险提示", &self.render_risks());
        push_section(&mut out, "建议分派", &self.render_assignments());
        out
    }

    fn render_key_topics(&self) -> Vec<String> {
        let mut items = Vec::new();
        for (signal_type, label) in [
            ("活动/聚会", "活动组织"),
            ("服务/设施", "服务设施"),
            ("未回答问题", "待回答问题"),
            ("FAQ/SOP", "知识沉淀"),
            ("内容线索", "内容素材"),
        ] {
            let count = self.count_type(signal_type);
            if count > 0 {
                items.push(format!("- {label}：识别到 {count} 条结构化事件。"));
            }
        }
        items
    }

    fn render_type(&self, signal_type: &str) -> Vec<String> {
        self.events
            .iter()
            .filter(|event| event.signal_type == signal_type)
            .map(|event| {
                let mut line = format!("- {}：{}", event.title, event.summary);
                if !event.related_member_names.is_empty() {
                    line.push_str(&format!(
                        "；相关成员：{}",
                        event.related_member_names.join("、")
                    ));
                }
                if let Some(window_start) = event.source_window_start {
                    let window_end = event.source_window_end.unwrap_or(window_start);
                    line.push_str(&format!(
                        "；时间窗口：{} - {}",
                        window_start.format("%H:%M"),
                        window_end.format("%H:%M")
                    ));
                }
                line.push_str(&format!(
                    "；负责人：{}；优先级：{}；置信度：{:.2}",
                    event.owner_name, event.priority, event.confidence
                ));
                line
            })
            .collect()
    }

    fn render_risks(&self) -> Vec<String> {
        let mut items = vec![
            "- V2 只汇总已接受的结构化事件信号；闲聊、玩笑和低置信度问题不进入事件表。".to_string(),
        ];
        if self
            .events
            .iter()
            .any(|event| event.risk_level == "中" || event.risk_level == "高")
        {
            items.push("- 存在中高风险事件，请大总管确认披露边界和跟进责任。".to_string());
        }
        items
    }

    fn render_assignments(&self) -> Vec<String> {
        let mut items = Vec::new();
        for rule in &self.dispatch_rules {
            if self.dispatch_signal_active(rule.signal, !items.is_empty()) {
                push_unique(&mut items, self.render_dispatch_rule(rule));
            }
        }
        items
    }

    fn dispatch_signal_active(&self, signal: DispatchSignal, has_assignments: bool) -> bool {
        match signal {
            DispatchSignal::ActivityEvents => self.count_type("活动/聚会") > 0,
            DispatchSignal::Services => self.count_type("服务/设施") > 0,
            DispatchSignal::Operations => self.count_type("FAQ/SOP") > 0,
            DispatchSignal::ContentOrActivities => {
                self.count_type("内容线索") > 0 || self.count_type("活动/聚会") > 0
            }
            DispatchSignal::Questions => self.count_type("未回答问题") > 0,
            DispatchSignal::EmptyDigest => !has_assignments,
            DispatchSignal::Always => true,
        }
    }

    fn render_dispatch_rule(&self, rule: &DispatchRule) -> String {
        format!(
            "- {}：{}",
            rule.agent,
            apply_dispatch_template_v2(&rule.template, self)
        )
    }

    fn count_type(&self, signal_type: &str) -> usize {
        self.events
            .iter()
            .filter(|event| event.signal_type == signal_type)
            .count()
    }
}

fn push_section(out: &mut String, title: &str, items: &[String]) {
    out.push_str(&format!("## {title}\n\n"));
    if items.is_empty() {
        out.push_str("- 暂无明显信号。\n\n");
    } else {
        for item in items.iter().take(20) {
            out.push_str(item);
            out.push('\n');
        }
        out.push('\n');
    }
}

fn push_unique(items: &mut Vec<String>, item: String) {
    if !items.iter().any(|existing| existing == &item) {
        items.push(item);
    }
}

fn daily_digest_dispatch_rules(cli: &Cli) -> Result<Vec<DispatchRule>> {
    let raw = match cli.daily_digest_dispatch_rules_json.as_deref() {
        Some(value) => value.to_string(),
        None => match fs::read_to_string(&cli.daily_digest_dispatch_rules_path) {
            Ok(value) => value,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                DEFAULT_DAILY_DIGEST_DISPATCH_RULES_JSON.to_string()
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("read {}", cli.daily_digest_dispatch_rules_path));
            }
        },
    };
    let rules: Vec<DispatchRule> =
        serde_json::from_str(&raw).context("parse daily digest dispatch rules")?;
    if rules.is_empty() {
        bail!("QINTOPIA_DAILY_DIGEST_DISPATCH_RULES_JSON must contain at least one rule");
    }
    Ok(rules)
}

fn apply_dispatch_template_v2(template: &str, renderer: &DigestV2Renderer) -> String {
    template
        .replace(
            "{activity_count}",
            &renderer.count_type("活动/聚会").to_string(),
        )
        .replace(
            "{service_count}",
            &renderer.count_type("服务/设施").to_string(),
        )
        .replace(
            "{operation_count}",
            &renderer.count_type("FAQ/SOP").to_string(),
        )
        .replace(
            "{question_count}",
            &renderer.count_type("未回答问题").to_string(),
        )
        .replace(
            "{content_count}",
            &renderer.count_type("内容线索").to_string(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_low_value_as_empty() {
        assert!(classify_profile_fact_message("[微笑]").is_empty());
    }

    #[test]
    fn classifies_first_person_activity_organizer() {
        let labels = classify_profile_fact_message("这周末我来组织桌游，大家一起报名");
        assert!(labels.contains(&"activity_organizer".to_string()));
    }

    #[test]
    fn classifies_self_declared_temporary_silence_state() {
        let labels = classify_profile_fact_message(
            "6月27日至29日，我会以“不语者”的身份生活，除非非常紧急或必要，我将不开口说话。",
        );

        assert!(labels.contains(&"temporary_communication_state".to_string()));
        assert!(!labels.contains(&"interest".to_string()));
        assert!(!labels.contains(&"unresolved_question".to_string()));
    }

    #[test]
    fn classifies_short_followup_about_not_speaking() {
        let labels = classify_profile_fact_message("是哒 也可以和我讲话 只是我不说话而已");

        assert!(labels.contains(&"temporary_communication_state".to_string()));
    }

    #[test]
    fn classifies_silence_game_as_activity_organizer_without_question_noise() {
        let labels = classify_profile_fact_message(
            "你有没有发现，当我们暂时关闭某一种感官通道，其他感知力会变得更加敏锐？因此，我想在社区里发起一场为期三天的身心探索游戏。6月27日至29日，我会以“不语者”的身份生活。",
        );

        assert!(labels.contains(&"activity_organizer".to_string()));
        assert!(labels.contains(&"temporary_communication_state".to_string()));
        assert!(!labels.contains(&"interest".to_string()));
        assert!(!labels.contains(&"unresolved_question".to_string()));
    }

    #[test]
    fn does_not_emit_disabled_relationship_fact() {
        let labels = classify_profile_fact_message("我和小王关系不好");
        assert!(!labels.contains(&"relationship_context".to_string()));
    }

    #[test]
    fn profile_ignores_light_question() {
        let labels = classify_profile_fact_message("粉色的吗");
        assert!(!labels.contains(&"unresolved_question".to_string()));
    }

    #[test]
    fn profile_ignores_group_payment_placeholder() {
        let labels = classify_profile_fact_message("[我发起了一笔群收款，该版本暂不支持查看]");
        assert!(labels.is_empty());
    }

    #[test]
    fn profile_interest_requires_first_person() {
        let labels =
            classify_profile_fact_message("社区有没有特别擅长整理的人，可以帮我整理吧台吗");
        assert!(!labels.contains(&"interest".to_string()));
        assert!(labels.contains(&"service_need".to_string()));
    }

    #[test]
    fn digest_v2_assignments_use_structured_events() {
        let renderer = DigestV2Renderer::new(
            vec![
                DispatchRule {
                    signal: DispatchSignal::Services,
                    agent: "服务台".to_string(),
                    template: "处理 {service_count} 条服务问题。".to_string(),
                },
                DispatchRule {
                    signal: DispatchSignal::Always,
                    agent: "值班负责人".to_string(),
                    template: "做最终验收。".to_string(),
                },
            ],
            vec![fake_event("服务/设施", "吧台整理求助")],
        );

        let assignments = renderer.render_assignments();
        assert_eq!(
            assignments,
            vec![
                "- 服务台：处理 1 条服务问题。".to_string(),
                "- 值班负责人：做最终验收。".to_string(),
            ]
        );
    }

    #[test]
    fn digest_v2_empty_events_do_not_render_chat_questions() {
        let renderer = DigestV2Renderer::new(Vec::new(), Vec::new());
        let markdown = renderer.render_markdown("日报");
        assert!(markdown.contains("今日无显著群聊运营信号。"));
        assert!(!markdown.contains("粉色的吗"));
    }

    #[test]
    fn versioned_dispatch_rules_config_is_valid() {
        let raw = include_str!("../config/agentos/daily-digest-dispatch-rules.json");
        let rules: Vec<DispatchRule> = serde_json::from_str(raw).unwrap();
        assert!(rules
            .iter()
            .any(|rule| rule.signal == DispatchSignal::ActivityEvents && rule.agent == "小满"));
        assert!(rules
            .iter()
            .any(|rule| rule.signal == DispatchSignal::Always && rule.agent == "大总管"));
    }

    #[test]
    fn chat_metadata_display_name_overrides_digest_title_chat_id() {
        let raw = r#"{"10859791146538059":{"display_name":"秦托邦的小伙伴（新）","source":"manual_config"}}"#;
        assert_eq!(
            chat_display_name_from_metadata(raw, "10859791146538059").unwrap(),
            "秦托邦的小伙伴（新）"
        );
    }

    #[test]
    fn chat_metadata_missing_mapping_falls_back_to_chat_id() {
        let raw = r#"{"10859791146538059":{"display_name":"秦托邦的小伙伴（新）"}}"#;
        assert_eq!(
            chat_display_name_from_metadata(raw, "another-chat").unwrap(),
            "another-chat"
        );
    }

    #[test]
    fn shanghai_digest_window_uses_local_day() {
        let date = NaiveDate::from_ymd_opt(2026, 6, 26).unwrap();
        let (start, end) = digest_utc_window(date, "Asia/Shanghai").unwrap();
        assert_eq!(start.to_rfc3339(), "2026-06-25T16:00:00+00:00");
        assert_eq!(end.to_rfc3339(), "2026-06-26T16:00:00+00:00");
    }

    fn fake_event(signal_type: &str, title: &str) -> EventSignalRow {
        EventSignalRow {
            id: Uuid::new_v4(),
            signal_type: signal_type.to_string(),
            title: title.to_string(),
            summary: "成员提出一个需要运营处理的事项。".to_string(),
            related_member_names: vec!["成员".to_string()],
            owner_name: "服务台".to_string(),
            owner_agent: "service".to_string(),
            priority: "中".to_string(),
            status: "待处理".to_string(),
            confidence: 0.82,
            source_message_ids: vec![Uuid::new_v4()],
            source_window_start: None,
            source_window_end: None,
            dedupe_key: "dedupe".to_string(),
            judge_model: "rule_v2".to_string(),
            judge_reason: "test".to_string(),
            risk_level: "无".to_string(),
            external_publish_status: "未评估".to_string(),
            feishu_record_id: None,
        }
    }
}
